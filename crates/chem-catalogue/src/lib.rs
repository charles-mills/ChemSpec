//! Immutable, content-addressed structural chemistry catalogue bundles.
//!
//! A [`ValidatedCatalogueBundle`] has passed structural validation but remains
//! untrusted. Only [`TrustedCatalogue::from_canonical_json`] can cross the
//! runtime trust boundary, and it requires both the host-pinned canonical
//! digest and an exact external review attestation.

mod model;

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    ops::Deref,
    str::FromStr,
};

use chem_domain::{
    Atom, AtomGroup, AtomId, BondOrder, ContentDigest, CovalentBond, ElectronState,
    ElementInventory, ElementSymbol, EvidenceSourceId, IonicAssociation, MetallicDomain, PremiseId,
    ReactionRuleId, RepresentationKind, StructuralGraph, StructureDefinition, StructureId,
    canonical_json,
};
pub use model::*;

/// Host-controlled trust root for the one closed production catalogue.
///
/// This value is intentionally compiled into the application. A runtime agent
/// can validate newly generated JSON, but cannot promote its digest into the
/// trusted catalogue type.
pub const PINNED_CANONICAL_CATALOGUE_DIGEST: &str =
    "cdf8afe54409acf1a4aa76ad772bd3e26207608f90cd0ee4c2f6f2ec0cf0bb4f";

/// Host-controlled digest of the exact external review attestation.
///
/// This remains `None` until the resident chemist supplies the signed-off
/// artifact. Runtime data cannot populate or override it.
pub const PINNED_CANONICAL_REVIEW_DIGEST: Option<&str> = None;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogueErrorCode {
    InvalidJson,
    UnsupportedSchema,
    DigestMismatch,
    InvalidMetadata,
    DuplicateId,
    UnknownReference,
    InvalidStructure,
    InvalidValencePremise,
    InvalidRule,
    InvalidMapping,
    InvalidOperationTemplate,
    InvalidApplicability,
    MissingEvidence,
    InvalidReview,
    IneligibleProductionRecord,
}

impl CatalogueErrorCode {
    #[must_use]
    pub const fn diagnostic_code(self) -> &'static str {
        match self {
            Self::InvalidJson => "CHEMS-C001",
            Self::UnsupportedSchema => "CHEMS-C002",
            Self::DigestMismatch => "CHEMS-C003",
            Self::InvalidMetadata => "CHEMS-C004",
            Self::DuplicateId => "CHEMS-C005",
            Self::UnknownReference => "CHEMS-C006",
            Self::InvalidStructure => "CHEMS-C007",
            Self::InvalidValencePremise => "CHEMS-C008",
            Self::InvalidRule => "CHEMS-C009",
            Self::InvalidMapping => "CHEMS-C010",
            Self::InvalidOperationTemplate => "CHEMS-C011",
            Self::InvalidApplicability => "CHEMS-C012",
            Self::MissingEvidence => "CHEMS-C013",
            Self::InvalidReview => "CHEMS-C014",
            Self::IneligibleProductionRecord => "CHEMS-C015",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogueError {
    code: CatalogueErrorCode,
    message: String,
}

impl CatalogueError {
    fn new(code: CatalogueErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    #[must_use]
    pub const fn code(&self) -> CatalogueErrorCode {
        self.code
    }

    #[must_use]
    pub const fn diagnostic_code(&self) -> &'static str {
        self.code.diagnostic_code()
    }

    #[must_use]
    pub const fn is_system_error(&self) -> bool {
        true
    }
}

impl fmt::Display for CatalogueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} catalogue system error: {}",
            self.diagnostic_code(),
            self.message
        )
    }
}

impl std::error::Error for CatalogueError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsupportedCatalogueItem<'a> {
    Structure(&'a StructureId),
    Rule(&'a ReactionRuleId),
}

impl fmt::Display for UnsupportedCatalogueItem<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Structure(id) => write!(formatter, "unsupported structure `{id}`"),
            Self::Rule(id) => write!(formatter, "unsupported reaction rule `{id}`"),
        }
    }
}

/// A rule whose role, pattern, mapping, operation, applicability, premise, and
/// observation references have all been validated against one catalogue.
#[derive(Debug, Clone)]
pub struct ValidatedReactionRule {
    record: ReactionRuleRecord,
    reactant_atoms: BTreeMap<String, (ElementSymbol, AtomId)>,
    product_atoms: BTreeMap<String, (ElementSymbol, AtomId)>,
}

impl ValidatedReactionRule {
    #[must_use]
    pub const fn record(&self) -> &ReactionRuleRecord {
        &self.record
    }

    #[must_use]
    pub const fn id(&self) -> &ReactionRuleId {
        &self.record.id
    }

    #[must_use]
    pub const fn reactant_atoms(&self) -> &BTreeMap<String, (ElementSymbol, AtomId)> {
        &self.reactant_atoms
    }

    #[must_use]
    pub const fn product_atoms(&self) -> &BTreeMap<String, (ElementSymbol, AtomId)> {
        &self.product_atoms
    }
}

/// Fully validated, immutable and lookup-indexed trusted catalogue.
#[derive(Debug, Clone)]
pub struct ValidatedCatalogueBundle {
    digest: ContentDigest,
    document: CatalogueDocument,
    structures: BTreeMap<StructureId, StructureDefinition>,
    structure_premises: BTreeMap<StructureId, PremiseId>,
    premises: BTreeMap<PremiseId, usize>,
    evidence: BTreeMap<EvidenceSourceId, usize>,
    valence_premises: BTreeMap<PremiseId, usize>,
    rules: BTreeMap<ReactionRuleId, ValidatedReactionRule>,
}

impl CatalogueEnvelope {
    /// Returns order-normalized canonical JSON bytes for the bundle semantics.
    ///
    /// # Errors
    ///
    /// Returns an error when conversion to canonical JSON fails.
    pub fn canonical_json(&self) -> Result<Vec<u8>, CatalogueError> {
        canonical_document(&self.bundle)
    }

    /// Computes the digest of order-normalized catalogue semantics.
    ///
    /// # Errors
    ///
    /// Returns an error when canonical JSON serialization fails.
    pub fn computed_digest(&self) -> Result<ContentDigest, CatalogueError> {
        digest_document(&self.bundle)
    }
}

impl CatalogueReviewAttestation {
    /// Returns canonical JSON bytes for the external review semantics.
    ///
    /// # Errors
    ///
    /// Returns an error when conversion to canonical JSON fails.
    pub fn canonical_json(&self) -> Result<Vec<u8>, CatalogueError> {
        let value = serde_json::to_value(self).map_err(|error| {
            CatalogueError::new(CatalogueErrorCode::InvalidReview, error.to_string())
        })?;
        canonical_json(&value).map_err(|error| {
            CatalogueError::new(CatalogueErrorCode::InvalidReview, error.to_string())
        })
    }

    /// Returns the canonical semantic digest of this review attestation.
    ///
    /// # Errors
    ///
    /// Returns an error when canonical serialization fails.
    pub fn canonical_digest(&self) -> Result<ContentDigest, CatalogueError> {
        Ok(ContentDigest::sha256(&self.canonical_json()?))
    }
}

impl ValidatedCatalogueBundle {
    /// Parses and validates an untrusted catalogue envelope.
    ///
    /// # Errors
    ///
    /// Every malformed or inconsistent trusted-data condition returns a typed
    /// catalogue system error. Unsupported runtime lookups are not errors here.
    pub fn from_json(bytes: &[u8]) -> Result<Self, CatalogueError> {
        let envelope: CatalogueEnvelope = serde_json::from_slice(bytes).map_err(|error| {
            CatalogueError::new(CatalogueErrorCode::InvalidJson, error.to_string())
        })?;
        Self::validate(envelope)
    }

    /// Validates an already decoded untrusted envelope.
    ///
    /// # Errors
    ///
    /// Returns a typed system error for invalid metadata, digest, structure,
    /// premise, evidence, review, mapping, applicability, or rule templates.
    pub fn validate(envelope: CatalogueEnvelope) -> Result<Self, CatalogueError> {
        validate_metadata(&envelope.bundle)?;
        let computed = envelope.computed_digest()?;
        if computed != envelope.digest {
            return Err(CatalogueError::new(
                CatalogueErrorCode::DigestMismatch,
                format!("declared {} but computed {computed}", envelope.digest),
            ));
        }

        let mut document = envelope.bundle;
        normalize_document(&mut document);
        let evidence = index_evidence(&document.evidence)?;
        let premises = index_premises(&document.premises, &evidence)?;
        if matches!(document.publication, PublicationKind::Production) {
            validate_production_reviews(&document.premises)?;
        }
        let valence_premises = validate_valence_premises(&document.valence_premises, &premises)?;
        let (structures, structure_premises) =
            validate_structures(&document.structures, &premises, &document.valence_premises)?;
        let rules = validate_rules(
            &document.rules,
            &structures,
            &structure_premises,
            &valence_premises,
            &document.valence_premises,
            &premises,
        )?;

        Ok(Self {
            digest: envelope.digest,
            document,
            structures,
            structure_premises,
            premises,
            evidence,
            valence_premises,
            rules,
        })
    }

    #[must_use]
    pub const fn digest(&self) -> ContentDigest {
        self.digest
    }

    #[must_use]
    pub const fn document(&self) -> &CatalogueDocument {
        &self.document
    }

    #[must_use]
    pub fn structure(&self, id: &StructureId) -> Option<&StructureDefinition> {
        self.structures.get(id)
    }

    /// Requires a supported structure identity.
    ///
    /// # Errors
    ///
    /// Returns an explicit unsupported lookup result when the identity is not
    /// present. This never reports trusted-data corruption.
    pub fn require_structure<'a>(
        &'a self,
        id: &'a StructureId,
    ) -> Result<&'a StructureDefinition, UnsupportedCatalogueItem<'a>> {
        self.structure(id)
            .ok_or(UnsupportedCatalogueItem::Structure(id))
    }

    #[must_use]
    pub fn structure_premise(&self, id: &StructureId) -> Option<&PremiseRecord> {
        self.structure_premises
            .get(id)
            .and_then(|premise| self.premise(premise))
    }

    #[must_use]
    pub fn premise(&self, id: &PremiseId) -> Option<&PremiseRecord> {
        self.premises
            .get(id)
            .map(|index| &self.document.premises[*index])
    }

    #[must_use]
    pub fn evidence(&self, id: &EvidenceSourceId) -> Option<&EvidenceSource> {
        self.evidence
            .get(id)
            .map(|index| &self.document.evidence[*index])
    }

    #[must_use]
    pub fn valence_premise(&self, id: &PremiseId) -> Option<&ValencePremiseRecord> {
        self.valence_premises
            .get(id)
            .map(|index| &self.document.valence_premises[*index])
    }

    #[must_use]
    pub fn rule(&self, id: &ReactionRuleId) -> Option<&ValidatedReactionRule> {
        self.rules.get(id)
    }

    /// Requires a supported reaction rule identity.
    ///
    /// # Errors
    ///
    /// Returns an explicit unsupported lookup result when the identity is not
    /// present. This never reports trusted-data corruption.
    pub fn require_rule<'a>(
        &'a self,
        id: &'a ReactionRuleId,
    ) -> Result<&'a ValidatedReactionRule, UnsupportedCatalogueItem<'a>> {
        self.rule(id).ok_or(UnsupportedCatalogueItem::Rule(id))
    }

    #[must_use]
    pub const fn structures(&self) -> &BTreeMap<StructureId, StructureDefinition> {
        &self.structures
    }

    #[must_use]
    pub const fn rules(&self) -> &BTreeMap<ReactionRuleId, ValidatedReactionRule> {
        &self.rules
    }

    /// Validates a digest-bound external chemistry review attestation.
    ///
    /// # Errors
    ///
    /// Rejects the wrong schema or digest, empty fields, malformed dates,
    /// unresolved evidence, or a reviewer absent from bound premise reviews.
    pub fn validate_attestation(
        &self,
        attestation: &CatalogueReviewAttestation,
    ) -> Result<(), CatalogueError> {
        if attestation.schema_version != CATALOGUE_SCHEMA_VERSION
            || attestation.catalogue_digest != self.digest
            || !valid_declared_text_id(&attestation.id)
            || !valid_date(&attestation.reviewed_on)
            || [
                attestation.reviewer.as_str(),
                attestation.scope.as_str(),
                attestation.method.as_str(),
                attestation.coverage_conclusion.as_str(),
                attestation.limitation.as_str(),
            ]
            .iter()
            .any(|value| value.trim().is_empty())
        {
            return Err(CatalogueError::new(
                CatalogueErrorCode::InvalidReview,
                "review attestation metadata or digest is invalid",
            ));
        }
        if attestation.sources.is_empty()
            || attestation
                .sources
                .iter()
                .any(|source| !self.evidence.contains_key(source))
        {
            return Err(CatalogueError::new(
                CatalogueErrorCode::InvalidReview,
                "review attestation evidence does not resolve",
            ));
        }
        let expected_premises = self
            .document
            .premises
            .iter()
            .map(|premise| premise.id.clone())
            .collect::<BTreeSet<_>>();
        if attestation.premises != expected_premises {
            return Err(CatalogueError::new(
                CatalogueErrorCode::InvalidReview,
                "attestation is not bound to every exact catalogue premise",
            ));
        }
        Ok(())
    }
}

/// Host-trusted immutable catalogue. Construction is possible only for the
/// compiled canonical digest with an exact, independently supplied review.
#[derive(Debug, Clone)]
pub struct TrustedCatalogue {
    validated: ValidatedCatalogueBundle,
}

impl TrustedCatalogue {
    /// Loads the one host-pinned production catalogue and its external review.
    ///
    /// # Errors
    ///
    /// Rejects invalid catalogue data, a digest other than the compiled trust
    /// root, or an attestation whose canonical semantic digest is not
    /// host-pinned and bound to every premise in the exact bundle.
    pub fn from_canonical_json(
        catalogue_json: &[u8],
        attestation_json: &[u8],
    ) -> Result<Self, CatalogueError> {
        let validated = ValidatedCatalogueBundle::from_json(catalogue_json)?;
        let pinned =
            ContentDigest::from_str(PINNED_CANONICAL_CATALOGUE_DIGEST).map_err(|error| {
                CatalogueError::new(CatalogueErrorCode::InvalidMetadata, error.to_string())
            })?;
        if validated.digest != pinned {
            return Err(CatalogueError::new(
                CatalogueErrorCode::InvalidReview,
                "catalogue digest is not in the host-controlled trust root",
            ));
        }
        let attestation: CatalogueReviewAttestation = serde_json::from_slice(attestation_json)
            .map_err(|error| {
                CatalogueError::new(CatalogueErrorCode::InvalidReview, error.to_string())
            })?;
        let Some(review_digest) = PINNED_CANONICAL_REVIEW_DIGEST else {
            return Err(CatalogueError::new(
                CatalogueErrorCode::InvalidReview,
                "no external review attestation is pinned by the host",
            ));
        };
        let actual_review_digest = attestation.canonical_digest()?;
        let expected_review_digest = ContentDigest::from_str(review_digest).map_err(|error| {
            CatalogueError::new(CatalogueErrorCode::InvalidMetadata, error.to_string())
        })?;
        if actual_review_digest != expected_review_digest {
            return Err(CatalogueError::new(
                CatalogueErrorCode::InvalidReview,
                "external review does not match the host-controlled review digest",
            ));
        }
        validated.validate_attestation(&attestation)?;
        Ok(Self { validated })
    }
}

impl Deref for TrustedCatalogue {
    type Target = ValidatedCatalogueBundle;

    fn deref(&self) -> &Self::Target {
        &self.validated
    }
}

fn validate_metadata(document: &CatalogueDocument) -> Result<(), CatalogueError> {
    if document.schema_version != CATALOGUE_SCHEMA_VERSION {
        return Err(CatalogueError::new(
            CatalogueErrorCode::UnsupportedSchema,
            format!("unsupported schema version {}", document.schema_version),
        ));
    }
    if document.name.trim().is_empty()
        || document.version.trim().is_empty()
        || document.created.created_by.trim().is_empty()
        || !valid_date(&document.created.created_on)
        || document.evidence.is_empty()
        || document.premises.is_empty()
        || document.valence_premises.is_empty()
        || document.structures.is_empty()
        || document.rules.is_empty()
    {
        return Err(CatalogueError::new(
            CatalogueErrorCode::InvalidMetadata,
            "catalogue metadata or required record collections are empty",
        ));
    }
    Ok(())
}

fn index_evidence(
    records: &[EvidenceSource],
) -> Result<BTreeMap<EvidenceSourceId, usize>, CatalogueError> {
    let mut index = BTreeMap::new();
    for (position, record) in records.iter().enumerate() {
        if index.insert(record.id.clone(), position).is_some() {
            return duplicate_id(&record.id);
        }
        if [
            record.title.as_str(),
            record.publisher.as_str(),
            record.locator.as_str(),
            record.reference.as_str(),
            record.usage.as_str(),
        ]
        .iter()
        .any(|value| value.trim().is_empty())
            || !valid_date(&record.retrieved_on)
            || record
                .publication_date
                .as_deref()
                .is_some_and(|date| !valid_date(date))
        {
            return Err(CatalogueError::new(
                CatalogueErrorCode::MissingEvidence,
                format!("evidence `{}` has invalid metadata", record.id),
            ));
        }
    }
    Ok(index)
}

fn index_premises(
    records: &[PremiseRecord],
    evidence: &BTreeMap<EvidenceSourceId, usize>,
) -> Result<BTreeMap<PremiseId, usize>, CatalogueError> {
    let mut index = BTreeMap::new();
    for (position, record) in records.iter().enumerate() {
        if index.insert(record.id.clone(), position).is_some() {
            return duplicate_id(&record.id);
        }
        if record.statement.trim().is_empty()
            || record.rule_version.trim().is_empty()
            || record.evidence.is_empty()
            || record.evidence.iter().any(|id| !evidence.contains_key(id))
        {
            return Err(CatalogueError::new(
                CatalogueErrorCode::MissingEvidence,
                format!("premise `{}` lacks resolvable evidence", record.id),
            ));
        }
        validate_review(&record.id, &record.review)?;
    }
    Ok(index)
}

fn validate_review(id: &PremiseId, review: &ReviewMetadata) -> Result<(), CatalogueError> {
    match review.status {
        ReviewStatus::Reviewed if review.reviewers.is_empty() => {
            return Err(CatalogueError::new(
                CatalogueErrorCode::InvalidReview,
                format!("reviewed premise `{id}` has no reviewer"),
            ));
        }
        ReviewStatus::Provisional | ReviewStatus::Rejected if !review.reviewers.is_empty() => {
            return Err(CatalogueError::new(
                CatalogueErrorCode::InvalidReview,
                format!("non-reviewed premise `{id}` carries reviewer attestations"),
            ));
        }
        _ => {}
    }
    let mut identities = BTreeSet::new();
    for reviewer in &review.reviewers {
        if reviewer.reviewer.trim().is_empty()
            || reviewer.reference.trim().is_empty()
            || !valid_date(&reviewer.reviewed_on)
            || !identities.insert((reviewer.reviewer.as_str(), reviewer.reference.as_str()))
        {
            return Err(CatalogueError::new(
                CatalogueErrorCode::InvalidReview,
                format!("premise `{id}` has invalid reviewer metadata"),
            ));
        }
    }
    Ok(())
}

fn validate_production_reviews(records: &[PremiseRecord]) -> Result<(), CatalogueError> {
    if let Some(record) = records
        .iter()
        .find(|record| record.review.status != ReviewStatus::Reviewed)
    {
        return Err(CatalogueError::new(
            CatalogueErrorCode::IneligibleProductionRecord,
            format!("production premise `{}` is not reviewed", record.id),
        ));
    }
    Ok(())
}

fn validate_valence_premises(
    records: &[ValencePremiseRecord],
    premises: &BTreeMap<PremiseId, usize>,
) -> Result<BTreeMap<PremiseId, usize>, CatalogueError> {
    let mut index = BTreeMap::new();
    for (position, record) in records.iter().enumerate() {
        require_premise(&record.premise_id, premises)?;
        if index.insert(record.premise_id.clone(), position).is_some() {
            return duplicate_id(&record.premise_id);
        }
        let mut neutral = BTreeMap::new();
        for value in &record.neutral_valence {
            let element = ElementSymbol::new(&value.element).map_err(|error| {
                CatalogueError::new(CatalogueErrorCode::InvalidValencePremise, error.to_string())
            })?;
            if value.neutral_valence_electrons == 0
                || neutral
                    .insert(element, value.neutral_valence_electrons)
                    .is_some()
            {
                return Err(CatalogueError::new(
                    CatalogueErrorCode::InvalidValencePremise,
                    format!("invalid neutral valence entry in `{}`", record.premise_id),
                ));
            }
        }
        if neutral.is_empty() || record.supported_states.is_empty() {
            return Err(CatalogueError::new(
                CatalogueErrorCode::InvalidValencePremise,
                format!("valence premise `{}` is empty", record.premise_id),
            ));
        }
        let mut states = BTreeSet::new();
        for state in &record.supported_states {
            let element = ElementSymbol::new(&state.element).map_err(|error| {
                CatalogueError::new(CatalogueErrorCode::InvalidValencePremise, error.to_string())
            })?;
            let electrons = ElectronState::new(
                state.formal_charge,
                state.non_bonding_electrons,
                state.unpaired_electrons,
            )
            .map_err(|error| {
                CatalogueError::new(CatalogueErrorCode::InvalidValencePremise, error.to_string())
            })?;
            let Some(valence) = neutral.get(&element) else {
                return Err(CatalogueError::new(
                    CatalogueErrorCode::InvalidValencePremise,
                    format!("state element `{element}` has no neutral valence premise"),
                ));
            };
            if !electrons.formal_charge_matches(*valence, u64::from(state.covalent_bond_order_sum))
                || !states.insert(state)
            {
                return Err(CatalogueError::new(
                    CatalogueErrorCode::InvalidValencePremise,
                    format!(
                        "unsupported or duplicate arithmetic state in `{}`",
                        record.premise_id
                    ),
                ));
            }
        }
        let mut metallic = BTreeSet::new();
        for state in &record.metallic_domain_states {
            let element = ElementSymbol::new(&state.element).map_err(|error| {
                CatalogueError::new(CatalogueErrorCode::InvalidValencePremise, error.to_string())
            })?;
            if !neutral.contains_key(&element)
                || state.site_local_electrons != 0
                || state.delocalized_electrons_per_site == 0
                || !metallic.insert(state)
            {
                return Err(CatalogueError::new(
                    CatalogueErrorCode::InvalidValencePremise,
                    format!("invalid metallic state in `{}`", record.premise_id),
                ));
            }
        }
    }
    Ok(index)
}

type StructureIndexes = (
    BTreeMap<StructureId, StructureDefinition>,
    BTreeMap<StructureId, PremiseId>,
);

fn validate_structures(
    records: &[StructureRecord],
    premises: &BTreeMap<PremiseId, usize>,
    valence: &[ValencePremiseRecord],
) -> Result<StructureIndexes, CatalogueError> {
    let mut structures = BTreeMap::new();
    let mut structure_premises = BTreeMap::new();
    for record in records {
        require_premise(record.premise_id(), premises)?;
        let definition = build_structure(record)?;
        validate_graph_valence(&definition, valence)?;
        if structures.insert(record.id().clone(), definition).is_some() {
            return duplicate_id(record.id());
        }
        structure_premises.insert(record.id().clone(), record.premise_id().clone());
    }
    Ok((structures, structure_premises))
}

#[allow(clippy::too_many_lines)]
fn build_structure(record: &StructureRecord) -> Result<StructureDefinition, CatalogueError> {
    let (id, formula, representation, graph) = match record {
        StructureRecord::Molecular {
            id,
            formula,
            atoms,
            bonds,
            groups,
            ..
        } => (
            id,
            formula,
            RepresentationKind::Molecular,
            build_graph(atoms, bonds, groups, &[], &[])?,
        ),
        StructureRecord::Ion {
            id,
            formula,
            atoms,
            bonds,
            groups,
            ..
        } => (
            id,
            formula,
            RepresentationKind::Ion,
            build_graph(atoms, bonds, groups, &[], &[])?,
        ),
        StructureRecord::Ionic {
            id,
            formula,
            components,
            associations,
            ..
        } => {
            let mut atoms = Vec::new();
            let mut bonds = Vec::new();
            let mut groups = Vec::new();
            for component in components {
                validate_label(&component.label, CatalogueErrorCode::InvalidStructure)?;
                for atom in &component.atoms {
                    let mut atom = atom.clone();
                    atom.label = format!("{}.{}", component.label, atom.label);
                    atoms.push(atom);
                }
                for bond in &component.bonds {
                    bonds.push(BondRecord {
                        left: format!("{}.{}", component.label, bond.left),
                        right: format!("{}.{}", component.label, bond.right),
                        order: bond.order,
                    });
                }
                groups.push(GroupRecord {
                    label: component.label.clone(),
                    atoms: component
                        .atoms
                        .iter()
                        .map(|atom| format!("{}.{}", component.label, atom.label))
                        .collect(),
                });
                groups.extend(component.groups.iter().map(|group| {
                    GroupRecord {
                        label: format!("{}.{}", component.label, group.label),
                        atoms: group
                            .atoms
                            .iter()
                            .map(|atom| format!("{}.{}", component.label, atom))
                            .collect(),
                    }
                }));
            }
            let associations = associations
                .iter()
                .map(|association| (association.label.clone(), association.components.clone()))
                .collect::<Vec<_>>();
            (
                id,
                formula,
                RepresentationKind::Ionic,
                build_graph(&atoms, &bonds, &groups, &associations, &[])?,
            )
        }
        StructureRecord::Metallic {
            id,
            formula,
            sites,
            domains,
            ..
        } => {
            let domains = domains
                .iter()
                .map(|domain| {
                    (
                        domain.label.clone(),
                        domain.sites.clone(),
                        domain.delocalized_electrons,
                    )
                })
                .collect::<Vec<_>>();
            (
                id,
                formula,
                RepresentationKind::Metallic,
                build_graph(sites, &[], &[], &[], &domains)?,
            )
        }
    };
    let formula = parse_formula_inventory(formula)?;
    StructureDefinition::new(id.clone(), formula, representation, graph).map_err(|error| {
        CatalogueError::new(
            CatalogueErrorCode::InvalidStructure,
            format!("structure `{id}`: {error}"),
        )
    })
}

type IonicInput = (String, Vec<String>);
type MetallicInput = (String, Vec<String>, u32);

fn build_graph(
    atom_records: &[AtomRecord],
    bond_records: &[BondRecord],
    group_records: &[GroupRecord],
    association_records: &[IonicInput],
    domain_records: &[MetallicInput],
) -> Result<StructuralGraph, CatalogueError> {
    let atoms = atom_records
        .iter()
        .map(build_atom)
        .collect::<Result<Vec<_>, _>>()?;
    let atom_ids = atoms
        .iter()
        .map(|atom| atom.id().clone())
        .collect::<BTreeSet<_>>();
    if atom_ids.len() != atoms.len() {
        return Err(CatalogueError::new(
            CatalogueErrorCode::DuplicateId,
            "duplicate atom label",
        ));
    }
    let bonds = bond_records
        .iter()
        .enumerate()
        .map(|(index, record)| {
            let left = parse_id::<chem_domain::AtomKind>(&record.left)?;
            let right = parse_id::<chem_domain::AtomKind>(&record.right)?;
            CovalentBond::new(
                parse_id::<chem_domain::CovalentBondKind>(&format!("bond.{index}"))?,
                left,
                right,
                record.order.into(),
            )
            .map_err(structure_error)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let groups = group_records
        .iter()
        .map(|record| {
            AtomGroup::new(
                parse_id::<chem_domain::AtomGroupKind>(&record.label)?,
                record
                    .atoms
                    .iter()
                    .map(|atom| parse_id::<chem_domain::AtomKind>(atom))
                    .collect::<Result<Vec<_>, _>>()?,
            )
            .map_err(structure_error)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let associations = association_records
        .iter()
        .map(|(label, components)| {
            IonicAssociation::new(
                parse_id::<chem_domain::IonicAssociationKind>(label)?,
                components
                    .iter()
                    .map(|component| parse_id::<chem_domain::AtomGroupKind>(component))
                    .collect::<Result<Vec<_>, _>>()?,
            )
            .map_err(structure_error)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let domains = domain_records
        .iter()
        .map(|(label, sites, electrons)| {
            MetallicDomain::new(
                parse_id::<chem_domain::MetallicDomainKind>(label)?,
                sites
                    .iter()
                    .map(|site| parse_id::<chem_domain::AtomKind>(site))
                    .collect::<Result<Vec<_>, _>>()?,
                *electrons,
            )
            .map_err(structure_error)
        })
        .collect::<Result<Vec<_>, _>>()?;
    StructuralGraph::new(atoms, bonds, groups, associations, domains).map_err(structure_error)
}

fn build_atom(record: &AtomRecord) -> Result<Atom, CatalogueError> {
    let id = parse_id::<chem_domain::AtomKind>(&record.label)?;
    let element = ElementSymbol::new(&record.element).map_err(structure_error)?;
    let electrons = ElectronState::new(
        record.formal_charge,
        record.non_bonding_electrons,
        record.unpaired_electrons,
    )
    .map_err(structure_error)?;
    Ok(Atom::new(id, element, electrons))
}

impl From<BondOrderRecord> for BondOrder {
    fn from(value: BondOrderRecord) -> Self {
        match value {
            BondOrderRecord::Single => Self::Single,
            BondOrderRecord::Double => Self::Double,
            BondOrderRecord::Triple => Self::Triple,
        }
    }
}

fn validate_graph_valence(
    definition: &StructureDefinition,
    premises: &[ValencePremiseRecord],
) -> Result<(), CatalogueError> {
    for atom in definition.graph().atoms().values() {
        let bond_sum = definition
            .graph()
            .covalent_bond_order_sum(atom.id())
            .expect("atom belongs to graph");
        let supported = premises.iter().any(|premise| {
            premise.supported_states.iter().any(|state| {
                state.element == atom.element().as_str()
                    && state.formal_charge == atom.electrons().formal_charge()
                    && state.non_bonding_electrons == atom.electrons().non_bonding_electrons()
                    && state.unpaired_electrons == atom.electrons().unpaired_electrons()
                    && u64::from(state.covalent_bond_order_sum) == bond_sum
            })
        });
        let metallic_supported = definition.representation() == RepresentationKind::Metallic
            && premises.iter().any(|premise| {
                premise.metallic_domain_states.iter().any(|state| {
                    state.element == atom.element().as_str()
                        && state.site_formal_charge == atom.electrons().formal_charge()
                        && state.site_local_electrons == atom.electrons().non_bonding_electrons()
                })
            });
        if !supported && !metallic_supported {
            return Err(CatalogueError::new(
                CatalogueErrorCode::InvalidValencePremise,
                format!(
                    "structure `{}` atom `{}` has no supported valence state",
                    definition.id(),
                    atom.id()
                ),
            ));
        }
    }
    Ok(())
}

fn validate_rules(
    records: &[ReactionRuleRecord],
    structures: &BTreeMap<StructureId, StructureDefinition>,
    structure_premises: &BTreeMap<StructureId, PremiseId>,
    valence_premises: &BTreeMap<PremiseId, usize>,
    valence_records: &[ValencePremiseRecord],
    premises: &BTreeMap<PremiseId, usize>,
) -> Result<BTreeMap<ReactionRuleId, ValidatedReactionRule>, CatalogueError> {
    let mut rules = BTreeMap::new();
    for record in records {
        if rules.contains_key(&record.id) {
            return duplicate_id(&record.id);
        }
        if record.roles.is_empty()
            || record.reactant_pattern.is_empty()
            || record.product_pattern.is_empty()
            || record.operation_template.is_empty()
            || record.mapping_template.is_empty()
        {
            return rule_error(&record.id, "required rule sections are empty");
        }
        for premise in &record.premise_ids {
            require_premise(premise, premises)?;
        }
        validate_rule_dependency_sets(record, premises)?;
        if !record
            .premise_ids
            .iter()
            .any(|premise| valence_premises.contains_key(premise))
        {
            return rule_error(&record.id, "no valence premise is proof-bound");
        }
        for term in record
            .reactant_pattern
            .iter()
            .chain(&record.product_pattern)
        {
            if !structures.contains_key(&term.structure_id) {
                return Err(CatalogueError::new(
                    CatalogueErrorCode::UnknownReference,
                    format!(
                        "rule `{}` references structure `{}`",
                        record.id, term.structure_id
                    ),
                ));
            }
            let structure_premise = &structure_premises[&term.structure_id];
            if !record.premise_ids.contains(structure_premise) {
                return rule_error(
                    &record.id,
                    format!(
                        "structure `{}` premise `{structure_premise}` is not proof-bound",
                        term.structure_id
                    ),
                );
            }
        }
        require_premise(&record.applicability.premise_id, premises)?;
        if !record
            .premise_ids
            .contains(&record.applicability.premise_id)
        {
            return applicability_error(&record.id, "applicability premise is not proof-bound");
        }
        for role in record.roles.keys() {
            validate_label(role, CatalogueErrorCode::InvalidRule)?;
        }
        let reactant_atoms = expand_pattern(
            record,
            &record.reactant_pattern,
            RuleSideRecord::Reactant,
            structures,
        )?;
        let product_atoms = expand_pattern(
            record,
            &record.product_pattern,
            RuleSideRecord::Product,
            structures,
        )?;
        validate_applicability(record)?;
        validate_mapping(record, &reactant_atoms, &product_atoms)?;
        validate_operations(
            record,
            &reactant_atoms,
            &product_atoms,
            structures,
            valence_records,
        )?;
        validate_observations(record, premises)?;
        rules.insert(
            record.id.clone(),
            ValidatedReactionRule {
                record: record.clone(),
                reactant_atoms,
                product_atoms,
            },
        );
    }
    Ok(rules)
}

fn validate_rule_dependency_sets(
    rule: &ReactionRuleRecord,
    premises: &BTreeMap<PremiseId, usize>,
) -> Result<(), CatalogueError> {
    for (label, dependencies) in rule
        .mapping_template
        .iter()
        .enumerate()
        .map(|(index, mapping)| {
            (
                format!("mapping template {}", index + 1),
                &mapping.premise_ids,
            )
        })
        .chain(
            rule.operation_template
                .iter()
                .enumerate()
                .map(|(index, operation)| {
                    (
                        format!("operation template {}", index + 1),
                        operation.premise_ids(),
                    )
                }),
        )
        .chain([(
            "model assumptions".to_owned(),
            &rule.model_assumptions.premise_ids,
        )])
    {
        if dependencies.is_empty() {
            return rule_error(&rule.id, format!("{label} has no exact premise dependency"));
        }
        for premise in dependencies {
            require_premise(premise, premises)?;
            if !rule.premise_ids.contains(premise) {
                return rule_error(
                    &rule.id,
                    format!("{label} dependency `{premise}` is not rule-bound"),
                );
            }
        }
    }
    Ok(())
}

fn expand_pattern(
    rule: &ReactionRuleRecord,
    terms: &[PatternTermRecord],
    expected_side: RuleSideRecord,
    structures: &BTreeMap<StructureId, StructureDefinition>,
) -> Result<BTreeMap<String, (ElementSymbol, AtomId)>, CatalogueError> {
    let mut seen_roles = BTreeSet::new();
    let mut atoms = BTreeMap::new();
    for term in terms {
        if term.coefficient == 0 || !seen_roles.insert(&term.role) {
            return rule_error(
                &rule.id,
                "pattern roles must be unique with positive coefficients",
            );
        }
        let Some(role_schema) = rule.roles.get(&term.role) else {
            return rule_error(&rule.id, "pattern references an undeclared role");
        };
        if role_schema.side != expected_side {
            return rule_error(&rule.id, "pattern role is declared on the wrong side");
        }
        let Some(structure) = structures.get(&term.structure_id) else {
            return Err(CatalogueError::new(
                CatalogueErrorCode::UnknownReference,
                format!(
                    "rule `{}` references structure `{}`",
                    rule.id, term.structure_id
                ),
            ));
        };
        if representation_record(structure.representation()) != role_schema.representation {
            return rule_error(&rule.id, "role representation does not match its structure");
        }
        for instance in 1..=term.coefficient {
            for atom in structure.graph().atoms().values() {
                let reference = format!("{}[{instance}].{}", term.role, atom.id());
                atoms.insert(reference, (atom.element().clone(), atom.id().clone()));
            }
        }
    }
    let declared = rule
        .roles
        .iter()
        .filter(|(_, schema)| schema.side == expected_side)
        .map(|(role, _)| role)
        .collect::<BTreeSet<_>>();
    if seen_roles != declared {
        return rule_error(
            &rule.id,
            "every declared role must occur exactly once in its pattern",
        );
    }
    Ok(atoms)
}

fn representation_record(kind: RepresentationKind) -> RepresentationRecord {
    match kind {
        RepresentationKind::Molecular => RepresentationRecord::Molecular,
        RepresentationKind::Ion => RepresentationRecord::Ion,
        RepresentationKind::Ionic => RepresentationRecord::Ionic,
        RepresentationKind::Metallic => RepresentationRecord::Metallic,
    }
}

fn validate_applicability(rule: &ReactionRuleRecord) -> Result<(), CatalogueError> {
    if rule.applicability.required_context.trim().is_empty() {
        return applicability_error(&rule.id, "required context is empty");
    }
    let pattern = rule
        .reactant_pattern
        .iter()
        .map(|term| term.structure_id.clone())
        .collect::<BTreeSet<_>>();
    if pattern != rule.applicability.reactant_structure_ids {
        return applicability_error(
            &rule.id,
            "applicability identities do not exactly match the reactant pattern",
        );
    }
    Ok(())
}

fn validate_mapping(
    rule: &ReactionRuleRecord,
    reactants: &BTreeMap<String, (ElementSymbol, AtomId)>,
    products: &BTreeMap<String, (ElementSymbol, AtomId)>,
) -> Result<(), CatalogueError> {
    let mut sources = BTreeSet::new();
    let mut targets = BTreeSet::new();
    for pair in &rule.mapping_template {
        let Some((source_element, _)) = reactants.get(&pair.reactant) else {
            return mapping_error(&rule.id, "mapping source does not resolve");
        };
        let Some((target_element, _)) = products.get(&pair.product) else {
            return mapping_error(&rule.id, "mapping target does not resolve");
        };
        if source_element != target_element
            || !sources.insert(pair.reactant.as_str())
            || !targets.insert(pair.product.as_str())
        {
            return mapping_error(&rule.id, "mapping is duplicate or changes element identity");
        }
    }
    if sources != reactants.keys().map(String::as_str).collect()
        || targets != products.keys().map(String::as_str).collect()
    {
        return mapping_error(&rule.id, "mapping is not a total bijection");
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn validate_operations(
    rule: &ReactionRuleRecord,
    reactants: &BTreeMap<String, (ElementSymbol, AtomId)>,
    products: &BTreeMap<String, (ElementSymbol, AtomId)>,
    structures: &BTreeMap<StructureId, StructureDefinition>,
    valence_records: &[ValencePremiseRecord],
) -> Result<(), CatalogueError> {
    let valence_records = valence_records
        .iter()
        .filter(|premise| rule.premise_ids.contains(&premise.premise_id))
        .collect::<Vec<_>>();
    let valence_records = valence_records.as_slice();
    let product_instances = rule
        .product_pattern
        .iter()
        .flat_map(|term| (1..=term.coefficient).map(move |index| format!("{}[{index}]", term.role)))
        .collect::<BTreeSet<_>>();
    let mapping = rule
        .mapping_template
        .iter()
        .map(|pair| (pair.reactant.as_str(), pair.product.as_str()))
        .collect::<BTreeMap<_, _>>();
    let mut assigned_atoms = BTreeSet::new();
    let mut assigned_products = BTreeSet::new();
    for operation in &rule.operation_template {
        match operation {
            OperationTemplateRecord::CleaveCovalent {
                edge,
                allocation,
                before,
                after,
                ..
            } => {
                require_distinct_atoms(rule, reactants, &edge.0, &edge.1)?;
                validate_binary_states(before, after)?;
                require_initial_bond(rule, &edge.0, &edge.1, edge.2, structures)?;
                validate_supported_state(&edge.0, before.left, reactants, valence_records)?;
                validate_supported_state(&edge.1, before.right, reactants, valence_records)?;
                validate_supported_state(&edge.0, after.left, reactants, valence_records)?;
                validate_supported_state(&edge.1, after.right, reactants, valence_records)?;
                let order = i16::from(BondOrder::from(edge.2).order());
                let Some((left_local_delta, right_local_delta)) =
                    allocation_local_deltas(allocation, &edge.0, &edge.1, -order)
                else {
                    return operation_error(&rule.id, "invalid cleavage allocation");
                };
                if !valid_covalent_endpoint(before.left, after.left, left_local_delta, -order)
                    || !valid_covalent_endpoint(
                        before.right,
                        after.right,
                        right_local_delta,
                        -order,
                    )
                {
                    return operation_error(&rule.id, "invalid cleavage electron ledger");
                }
            }
            OperationTemplateRecord::FormCovalent {
                edge,
                electron_contribution,
                before,
                after,
                ..
            } => {
                require_distinct_atoms(rule, reactants, &edge.0, &edge.1)?;
                validate_binary_states(before, after)?;
                validate_supported_state(&edge.0, before.left, reactants, valence_records)?;
                validate_supported_state(&edge.1, before.right, reactants, valence_records)?;
                validate_supported_state(&edge.0, after.left, reactants, valence_records)?;
                validate_supported_state(&edge.1, after.right, reactants, valence_records)?;
                let order = BondOrder::from(edge.2).order();
                if electron_contribution.left != order
                    || electron_contribution.right != order
                    || before.left.2.checked_sub(order) != Some(after.left.2)
                    || before.right.2.checked_sub(order) != Some(after.right.2)
                {
                    return operation_error(
                        &rule.id,
                        "localized bond formation requires one unpaired electron per endpoint and order",
                    );
                }
                let order = i16::from(order);
                if !valid_covalent_endpoint(
                    before.left,
                    after.left,
                    -i16::from(electron_contribution.left),
                    order,
                ) || !valid_covalent_endpoint(
                    before.right,
                    after.right,
                    -i16::from(electron_contribution.right),
                    order,
                ) {
                    return operation_error(&rule.id, "invalid bond-formation electron ledger");
                }
            }
            OperationTemplateRecord::ChangeCovalent {
                edge,
                old_order,
                new_order,
                allocation,
                before,
                after,
                ..
            } => {
                require_distinct_atoms(rule, reactants, &edge.0, &edge.1)?;
                validate_binary_states(before, after)?;
                require_initial_bond(rule, &edge.0, &edge.1, *old_order, structures)?;
                validate_supported_state(&edge.0, before.left, reactants, valence_records)?;
                validate_supported_state(&edge.1, before.right, reactants, valence_records)?;
                validate_supported_state(&edge.0, after.left, reactants, valence_records)?;
                validate_supported_state(&edge.1, after.right, reactants, valence_records)?;
                if old_order == new_order {
                    return operation_error(&rule.id, "bond-order change is unchanged");
                }
                let order_delta = i16::from(BondOrder::from(*new_order).order())
                    - i16::from(BondOrder::from(*old_order).order());
                let Some((left_local_delta, right_local_delta)) =
                    allocation_local_deltas(allocation, &edge.0, &edge.1, order_delta)
                else {
                    return operation_error(&rule.id, "invalid bond-change allocation");
                };
                if !valid_covalent_endpoint(before.left, after.left, left_local_delta, order_delta)
                    || !valid_covalent_endpoint(
                        before.right,
                        after.right,
                        right_local_delta,
                        order_delta,
                    )
                {
                    return operation_error(&rule.id, "invalid bond-order electron ledger");
                }
            }
            OperationTemplateRecord::AssociateIonic {
                label,
                components,
                component_charges,
                ..
            } => {
                validate_label(label, CatalogueErrorCode::InvalidOperationTemplate)?;
                if components.len() < 2
                    || components.len() != component_charges.len()
                    || component_charges
                        .iter()
                        .map(|charge| i64::from(*charge))
                        .sum::<i64>()
                        != 0
                {
                    return operation_error(&rule.id, "invalid ionic association template");
                }
                let mut atoms = BTreeSet::new();
                let mut product_instance = None;
                for (component, declared_charge) in components.iter().zip(component_charges) {
                    if component.is_empty() {
                        return operation_error(&rule.id, "empty ionic component");
                    }
                    let mut component_group = None;
                    let mut mapped_targets = BTreeSet::new();
                    let mut actual_charge = 0_i64;
                    for atom in component {
                        require_atom(rule, reactants, atom)?;
                        if !atoms.insert(atom) {
                            return operation_error(&rule.id, "overlapping ionic components");
                        }
                        let target = mapping[atom.as_str()];
                        mapped_targets.insert(target);
                        let (instance, local) =
                            split_template_reference(target).ok_or_else(|| {
                                CatalogueError::new(
                                    CatalogueErrorCode::InvalidOperationTemplate,
                                    format!("rule `{}`: malformed mapped product atom", rule.id),
                                )
                            })?;
                        if product_instance
                            .replace(instance)
                            .is_some_and(|existing| existing != instance)
                        {
                            return operation_error(
                                &rule.id,
                                "ionic association spans product instances",
                            );
                        }
                        let Some((group, _)) = local.rsplit_once('.') else {
                            return operation_error(
                                &rule.id,
                                "ionic association maps outside a product component",
                            );
                        };
                        if component_group
                            .replace(group)
                            .is_some_and(|existing| existing != group)
                        {
                            return operation_error(
                                &rule.id,
                                "ionic component maps across product components",
                            );
                        }
                        actual_charge += i64::from(
                            resolve_template_atom(target, &rule.product_pattern, structures)?
                                .electrons()
                                .formal_charge(),
                        );
                    }
                    if actual_charge != i64::from(*declared_charge) {
                        return operation_error(
                            &rule.id,
                            "ionic component charge contradicts the reviewed product",
                        );
                    }
                    let instance = product_instance.expect("nonempty component set an instance");
                    let group = component_group.expect("mapped ionic atom has a component group");
                    let expected_prefix = format!("{instance}.{group}.");
                    let expected_targets = products
                        .keys()
                        .map(String::as_str)
                        .filter(|target| target.starts_with(&expected_prefix))
                        .collect::<BTreeSet<_>>();
                    if mapped_targets != expected_targets {
                        return operation_error(
                            &rule.id,
                            "ionic component is not the complete reviewed product component",
                        );
                    }
                }
            }
            OperationTemplateRecord::DissociateIonic { association, .. } => {
                require_relationship_ref(rule, association, "association", structures)?;
            }
            OperationTemplateRecord::ReleaseMetallic {
                site,
                domain,
                before,
                after,
                ..
            } => {
                require_atom(rule, reactants, site)?;
                require_metallic_membership(rule, site, domain, structures)?;
                validate_electron_state(before.site)?;
                validate_electron_state(after.site)?;
                validate_supported_state(site, before.site, reactants, valence_records)?;
                validate_supported_state(site, after.site, reactants, valence_records)?;
                let retain_is_valid = before.domain_electrons != 0
                    && before.domain_electrons == after.domain_electrons.saturating_add(1)
                    && before.site.1.checked_add(1) == Some(after.site.1)
                    && before.site.2.checked_add(1) == Some(after.site.2)
                    && before.site.0.checked_sub(1) == Some(after.site.0);
                let leave_is_valid = before == after;
                let valid = match operation {
                    OperationTemplateRecord::ReleaseMetallic {
                        allocation: MetallicReleaseAllocationRecord::RetainElectron,
                        ..
                    } => retain_is_valid,
                    OperationTemplateRecord::ReleaseMetallic {
                        allocation: MetallicReleaseAllocationRecord::LeaveElectron,
                        ..
                    } => leave_is_valid,
                    _ => unreachable!(),
                };
                if !valid {
                    return operation_error(&rule.id, "invalid metallic release ledger");
                }
            }
            OperationTemplateRecord::JoinMetallic {
                site,
                domain,
                before,
                after,
                ..
            } => {
                require_atom(rule, reactants, site)?;
                require_metallic_membership(rule, site, domain, structures)?;
                validate_electron_state(before.site)?;
                validate_electron_state(after.site)?;
                validate_supported_state(site, before.site, reactants, valence_records)?;
                validate_supported_state(site, after.site, reactants, valence_records)?;
                if after.domain_electrons != before.domain_electrons.saturating_add(1)
                    || before.site.1.checked_sub(1) != Some(after.site.1)
                    || before.site.2.checked_sub(1) != Some(after.site.2)
                    || before.site.0.checked_add(1) != Some(after.site.0)
                {
                    return operation_error(&rule.id, "invalid metallic join ledger");
                }
            }
            OperationTemplateRecord::TransferElectron {
                count,
                donor,
                acceptor,
                before,
                after,
                ..
            } => {
                require_distinct_atoms(rule, reactants, donor, acceptor)?;
                if *count == 0 {
                    return operation_error(&rule.id, "zero electron transfer");
                }
                for state in [before.donor, before.acceptor, after.donor, after.acceptor] {
                    validate_electron_state(state)?;
                }
                validate_supported_state(donor, before.donor, reactants, valence_records)?;
                validate_supported_state(acceptor, before.acceptor, reactants, valence_records)?;
                validate_supported_state(donor, after.donor, reactants, valence_records)?;
                validate_supported_state(acceptor, after.acceptor, reactants, valence_records)?;
                let donor_delta = i16::from(before.donor.1) - i16::from(after.donor.1);
                let acceptor_delta = i16::from(after.acceptor.1) - i16::from(before.acceptor.1);
                if donor_delta != i16::from(*count)
                    || acceptor_delta != i16::from(*count)
                    || before.donor.0.checked_add(i16::from(*count)) != Some(after.donor.0)
                    || before.acceptor.0.checked_sub(i16::from(*count)) != Some(after.acceptor.0)
                {
                    return operation_error(&rule.id, "electron transfer ledger is inconsistent");
                }
            }
            OperationTemplateRecord::AssignProduct { atoms, product, .. } => {
                if atoms.is_empty()
                    || !product_instances.contains(product)
                    || !assigned_products.insert(product.as_str())
                {
                    return operation_error(&rule.id, "invalid product assignment target");
                }
                let mut unique = BTreeSet::new();
                for atom in atoms {
                    require_atom(rule, reactants, atom)?;
                    if !unique.insert(atom) || !assigned_atoms.insert(atom.as_str()) {
                        return operation_error(&rule.id, "duplicate assigned atom");
                    }
                    if !mapping[atom.as_str()].starts_with(&format!("{product}.")) {
                        return operation_error(
                            &rule.id,
                            "assigned atom maps to a different product instance",
                        );
                    }
                }
                let expected = products
                    .keys()
                    .filter(|atom| atom.starts_with(&format!("{product}.")))
                    .count();
                if unique.len() != expected {
                    return operation_error(&rule.id, "product assignment atom count is wrong");
                }
            }
        }
    }
    if assigned_atoms != reactants.keys().map(String::as_str).collect()
        || assigned_products != product_instances.iter().map(String::as_str).collect()
    {
        return operation_error(
            &rule.id,
            "product assignments are not a total disjoint partition",
        );
    }
    Ok(())
}

fn validate_observations(
    rule: &ReactionRuleRecord,
    premises: &BTreeMap<PremiseId, usize>,
) -> Result<(), CatalogueError> {
    let mut seen = BTreeSet::new();
    for observation in &rule.observation_compatibility {
        require_premise(&observation.premise_id, premises)?;
        let Some(role_schema) = rule.roles.get(&observation.subject_role) else {
            return rule_error(&rule.id, "observation references an unknown role");
        };
        let predicate_matches_role = match observation.predicate {
            ObservationPredicate::Evolves => {
                role_schema.side == RuleSideRecord::Product
                    && role_schema.representation == RepresentationRecord::Molecular
                    && observation.value.is_none()
            }
            ObservationPredicate::Disappears => {
                role_schema.side == RuleSideRecord::Reactant && observation.value.is_none()
            }
            ObservationPredicate::Forms => {
                role_schema.side == RuleSideRecord::Product && observation.value.is_none()
            }
            ObservationPredicate::Colour => {
                role_schema.side == RuleSideRecord::Product
                    && observation
                        .value
                        .as_deref()
                        .is_some_and(|value| !value.trim().is_empty())
            }
        };
        if !rule.premise_ids.contains(&observation.premise_id)
            || !predicate_matches_role
            || observation.evidence_subject.trim().is_empty()
            || !seen.insert((
                observation.subject_role.as_str(),
                observation.predicate,
                observation.evidence_subject.as_str(),
                observation.value.as_deref(),
            ))
        {
            return rule_error(&rule.id, "invalid observation compatibility fact");
        }
    }
    Ok(())
}

fn validate_binary_states(
    before: &BinaryElectronStateRecord,
    after: &BinaryElectronStateRecord,
) -> Result<(), CatalogueError> {
    for state in [before.left, before.right, after.left, after.right] {
        validate_electron_state(state)?;
    }
    Ok(())
}

fn valid_covalent_endpoint(
    before: ElectronStateRecord,
    after: ElectronStateRecord,
    local_delta: i16,
    bond_order_delta: i16,
) -> bool {
    i16::from(before.1).checked_add(local_delta) == Some(i16::from(after.1))
        && before
            .0
            .checked_sub(local_delta)
            .and_then(|charge| charge.checked_sub(bond_order_delta))
            == Some(after.0)
}

fn allocation_local_deltas(
    allocation: &CleavageAllocationRecord,
    left: &str,
    right: &str,
    bond_order_delta: i16,
) -> Option<(i16, i16)> {
    match allocation {
        CleavageAllocationRecord::Homolytic(value) if value == "homolytic" => {
            Some((-bond_order_delta, -bond_order_delta))
        }
        CleavageAllocationRecord::Heterolytic { heterolytic_to } if heterolytic_to == left => {
            Some((-2 * bond_order_delta, 0))
        }
        CleavageAllocationRecord::Heterolytic { heterolytic_to } if heterolytic_to == right => {
            Some((0, -2 * bond_order_delta))
        }
        _ => None,
    }
}

fn validate_electron_state(state: ElectronStateRecord) -> Result<(), CatalogueError> {
    ElectronState::new(state.0, state.1, state.2)
        .map(|_| ())
        .map_err(|error| {
            CatalogueError::new(
                CatalogueErrorCode::InvalidOperationTemplate,
                error.to_string(),
            )
        })
}

fn require_atom(
    rule: &ReactionRuleRecord,
    atoms: &BTreeMap<String, (ElementSymbol, AtomId)>,
    reference: &str,
) -> Result<(), CatalogueError> {
    if atoms.contains_key(reference) {
        Ok(())
    } else {
        operation_error(
            &rule.id,
            format!("atom template reference `{reference}` does not resolve"),
        )
    }
}

fn require_distinct_atoms(
    rule: &ReactionRuleRecord,
    atoms: &BTreeMap<String, (ElementSymbol, AtomId)>,
    left: &str,
    right: &str,
) -> Result<(), CatalogueError> {
    require_atom(rule, atoms, left)?;
    require_atom(rule, atoms, right)?;
    if left == right {
        operation_error(&rule.id, "operation endpoints are identical")
    } else {
        Ok(())
    }
}

fn validate_supported_state(
    reference: &str,
    state: ElectronStateRecord,
    atoms: &BTreeMap<String, (ElementSymbol, AtomId)>,
    premises: &[&ValencePremiseRecord],
) -> Result<(), CatalogueError> {
    let element = &atoms[reference].0;
    let supported = premises.iter().any(|premise| {
        premise.supported_states.iter().any(|candidate| {
            candidate.element == element.as_str()
                && candidate.formal_charge == state.0
                && candidate.non_bonding_electrons == state.1
                && candidate.unpaired_electrons == state.2
        }) || premise.metallic_domain_states.iter().any(|candidate| {
            candidate.element == element.as_str()
                && candidate.site_formal_charge == state.0
                && candidate.site_local_electrons == state.1
                && state.2 == 0
        })
    });
    if supported {
        Ok(())
    } else {
        Err(CatalogueError::new(
            CatalogueErrorCode::InvalidOperationTemplate,
            format!("operation state for `{reference}` has no reviewed valence premise"),
        ))
    }
}

fn require_initial_bond(
    rule: &ReactionRuleRecord,
    left: &str,
    right: &str,
    order: BondOrderRecord,
    structures: &BTreeMap<StructureId, StructureDefinition>,
) -> Result<(), CatalogueError> {
    let (left_instance, _) = split_template_reference(left).ok_or_else(|| {
        CatalogueError::new(
            CatalogueErrorCode::InvalidOperationTemplate,
            "malformed bond endpoint",
        )
    })?;
    let (right_instance, _) = split_template_reference(right).ok_or_else(|| {
        CatalogueError::new(
            CatalogueErrorCode::InvalidOperationTemplate,
            "malformed bond endpoint",
        )
    })?;
    if left_instance != right_instance {
        return operation_error(&rule.id, "covalent edge spans structure instances");
    }
    let left_atom = resolve_template_atom(left, &rule.reactant_pattern, structures)?;
    let right_atom = resolve_template_atom(right, &rule.reactant_pattern, structures)?;
    let (role_name, _) = parse_instance(left_instance).ok_or_else(|| {
        CatalogueError::new(
            CatalogueErrorCode::InvalidOperationTemplate,
            "malformed bond instance",
        )
    })?;
    let structure = rule
        .reactant_pattern
        .iter()
        .find(|term| term.role == role_name)
        .map(|term| &structures[&term.structure_id])
        .expect("resolved atom role has a pattern term");
    let exists = structure.graph().covalent_bonds().values().any(|bond| {
        ((bond.left() == left_atom.id() && bond.right() == right_atom.id())
            || (bond.left() == right_atom.id() && bond.right() == left_atom.id()))
            && bond.order() == BondOrder::from(order)
    });
    if exists {
        Ok(())
    } else {
        operation_error(
            &rule.id,
            "referenced covalent edge/order is absent from the template",
        )
    }
}

fn require_metallic_membership(
    rule: &ReactionRuleRecord,
    site: &str,
    domain: &str,
    structures: &BTreeMap<StructureId, StructureDefinition>,
) -> Result<(), CatalogueError> {
    let (site_instance, _) = split_template_reference(site).ok_or_else(|| {
        CatalogueError::new(
            CatalogueErrorCode::InvalidOperationTemplate,
            "malformed metallic site",
        )
    })?;
    let (domain_instance, domain_local) = split_template_reference(domain).ok_or_else(|| {
        CatalogueError::new(
            CatalogueErrorCode::InvalidOperationTemplate,
            "malformed metallic domain",
        )
    })?;
    if site_instance != domain_instance {
        return operation_error(&rule.id, "metallic site and domain use different instances");
    }
    let site_atom = resolve_template_atom(site, &rule.reactant_pattern, structures)?;
    let (role_name, _) = parse_instance(site_instance).ok_or_else(|| {
        CatalogueError::new(
            CatalogueErrorCode::InvalidOperationTemplate,
            "malformed metallic instance",
        )
    })?;
    let structure = rule
        .reactant_pattern
        .iter()
        .find(|term| term.role == role_name)
        .map(|term| &structures[&term.structure_id])
        .expect("resolved site role has a pattern term");
    let member = structure
        .graph()
        .metallic_domains()
        .iter()
        .any(|(id, value)| id.as_str() == domain_local && value.sites().contains(site_atom.id()));
    if member {
        Ok(())
    } else {
        operation_error(
            &rule.id,
            "site is not owned by the referenced metallic domain",
        )
    }
}

fn resolve_template_atom<'a>(
    reference: &str,
    pattern: &[PatternTermRecord],
    structures: &'a BTreeMap<StructureId, StructureDefinition>,
) -> Result<&'a Atom, CatalogueError> {
    let Some((instance, local)) = split_template_reference(reference) else {
        return Err(CatalogueError::new(
            CatalogueErrorCode::InvalidOperationTemplate,
            format!("malformed atom template reference `{reference}`"),
        ));
    };
    let Some((role_name, index)) = parse_instance(instance) else {
        return Err(CatalogueError::new(
            CatalogueErrorCode::InvalidOperationTemplate,
            format!("malformed atom instance `{instance}`"),
        ));
    };
    let Some(term) = pattern.iter().find(|term| term.role == role_name) else {
        return Err(CatalogueError::new(
            CatalogueErrorCode::InvalidOperationTemplate,
            format!("unknown atom role `{role_name}`"),
        ));
    };
    if index == 0 || index > term.coefficient {
        return Err(CatalogueError::new(
            CatalogueErrorCode::InvalidOperationTemplate,
            format!("atom instance index is out of range in `{reference}`"),
        ));
    }
    let atom_id = AtomId::from_str(local).map_err(|error| {
        CatalogueError::new(
            CatalogueErrorCode::InvalidOperationTemplate,
            error.to_string(),
        )
    })?;
    structures[&term.structure_id]
        .graph()
        .atoms()
        .get(&atom_id)
        .ok_or_else(|| {
            CatalogueError::new(
                CatalogueErrorCode::InvalidOperationTemplate,
                format!("atom `{reference}` does not resolve"),
            )
        })
}

fn split_template_reference(value: &str) -> Option<(&str, &str)> {
    let separator = value.find("].")?;
    Some((&value[..=separator], &value[separator + 2..]))
}

fn require_relationship_ref(
    rule: &ReactionRuleRecord,
    reference: &str,
    relationship: &str,
    structures: &BTreeMap<StructureId, StructureDefinition>,
) -> Result<(), CatalogueError> {
    let Some((instance, local)) = split_template_reference(reference) else {
        return operation_error(&rule.id, "malformed relationship template reference");
    };
    let Some((role_name, index)) = parse_instance(instance) else {
        return operation_error(&rule.id, "malformed relationship instance reference");
    };
    let Some(term) = rule
        .reactant_pattern
        .iter()
        .find(|term| term.role == role_name)
    else {
        return operation_error(&rule.id, "unknown relationship role");
    };
    if index == 0 || index > term.coefficient {
        return operation_error(&rule.id, "relationship instance index is out of range");
    }
    let graph = structures[&term.structure_id].graph();
    let exists = match relationship {
        "domain" => graph
            .metallic_domains()
            .keys()
            .any(|id| id.as_str() == local),
        "association" => graph
            .ionic_associations()
            .keys()
            .any(|id| id.as_str() == local),
        _ => false,
    };
    if exists {
        Ok(())
    } else {
        operation_error(&rule.id, "relationship template reference does not resolve")
    }
}

fn parse_instance(value: &str) -> Option<(&str, u32)> {
    let open = value.rfind('[')?;
    let close = value.strip_suffix(']')?;
    let role = &value[..open];
    let index = close.get(open + 1..)?.parse().ok()?;
    Some((role, index))
}

fn parse_formula_inventory(source: &str) -> Result<ElementInventory, CatalogueError> {
    let bytes = source.as_bytes();
    let mut index = 0;
    let mut elements = BTreeMap::new();
    while index < bytes.len() {
        if !bytes[index].is_ascii_uppercase() {
            return Err(CatalogueError::new(
                CatalogueErrorCode::InvalidStructure,
                format!("invalid formula summary `{source}`"),
            ));
        }
        let start = index;
        index += 1;
        if index < bytes.len() && bytes[index].is_ascii_lowercase() {
            index += 1;
        }
        let element = ElementSymbol::new(&source[start..index]).map_err(structure_error)?;
        let count_start = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        let count = if count_start == index {
            1
        } else {
            source[count_start..index].parse::<u64>().map_err(|_| {
                CatalogueError::new(
                    CatalogueErrorCode::InvalidStructure,
                    format!("invalid formula count in `{source}`"),
                )
            })?
        };
        if count == 0 || elements.insert(element.clone(), count).is_some() {
            return Err(CatalogueError::new(
                CatalogueErrorCode::InvalidStructure,
                format!("non-normalized formula summary `{source}`"),
            ));
        }
    }
    ElementInventory::new(elements).map_err(structure_error)
}

fn canonical_document(document: &CatalogueDocument) -> Result<Vec<u8>, CatalogueError> {
    let mut normalized = document.clone();
    normalize_document(&mut normalized);
    let value = serde_json::to_value(&normalized)
        .map_err(|error| CatalogueError::new(CatalogueErrorCode::InvalidJson, error.to_string()))?;
    canonical_json(&value)
        .map_err(|error| CatalogueError::new(CatalogueErrorCode::InvalidJson, error.to_string()))
}

fn digest_document(document: &CatalogueDocument) -> Result<ContentDigest, CatalogueError> {
    Ok(ContentDigest::sha256(&canonical_document(document)?))
}

fn normalize_document(document: &mut CatalogueDocument) {
    document
        .evidence
        .sort_by(|left, right| left.id.cmp(&right.id));
    document
        .premises
        .sort_by(|left, right| left.id.cmp(&right.id));
    for premise in &mut document.premises {
        premise.review.reviewers.sort_by(|left, right| {
            (&left.reviewer, &left.reference).cmp(&(&right.reviewer, &right.reference))
        });
    }
    document
        .valence_premises
        .sort_by(|left, right| left.premise_id.cmp(&right.premise_id));
    for premise in &mut document.valence_premises {
        premise
            .neutral_valence
            .sort_by(|left, right| left.element.cmp(&right.element));
        premise.supported_states.sort();
        premise.metallic_domain_states.sort();
    }
    for structure in &mut document.structures {
        normalize_structure(structure);
    }
    document
        .structures
        .sort_by(|left, right| left.id().cmp(right.id()));
    for rule in &mut document.rules {
        rule.reactant_pattern
            .sort_by(|left, right| left.role.cmp(&right.role));
        rule.product_pattern
            .sort_by(|left, right| left.role.cmp(&right.role));
        rule.mapping_template.sort_by(|left, right| {
            (&left.reactant, &left.product).cmp(&(&right.reactant, &right.product))
        });
        for operation in &mut rule.operation_template {
            normalize_operation(operation);
        }
        rule.observation_compatibility.sort_by(|left, right| {
            (
                &left.subject_role,
                left.predicate,
                &left.evidence_subject,
                &left.value,
            )
                .cmp(&(
                    &right.subject_role,
                    right.predicate,
                    &right.evidence_subject,
                    &right.value,
                ))
        });
    }
    document.rules.sort_by(|left, right| left.id.cmp(&right.id));
}

fn normalize_operation(operation: &mut OperationTemplateRecord) {
    match operation {
        OperationTemplateRecord::AssociateIonic {
            components,
            component_charges,
            ..
        } => {
            for component in components.iter_mut() {
                component.sort();
            }
            let mut paired = std::mem::take(components)
                .into_iter()
                .zip(std::mem::take(component_charges))
                .collect::<Vec<_>>();
            paired.sort();
            for (component, charge) in paired {
                components.push(component);
                component_charges.push(charge);
            }
        }
        OperationTemplateRecord::AssignProduct { atoms, .. } => atoms.sort(),
        _ => {}
    }
}

fn normalize_structure(record: &mut StructureRecord) {
    match record {
        StructureRecord::Molecular {
            atoms,
            bonds,
            groups,
            ..
        }
        | StructureRecord::Ion {
            atoms,
            bonds,
            groups,
            ..
        } => normalize_graph_records(atoms, bonds, groups),
        StructureRecord::Ionic {
            components,
            associations,
            ..
        } => {
            for component in components.iter_mut() {
                normalize_graph_records(
                    &mut component.atoms,
                    &mut component.bonds,
                    &mut component.groups,
                );
            }
            components.sort_by(|left, right| left.label.cmp(&right.label));
            for association in associations.iter_mut() {
                association.components.sort();
            }
            associations.sort_by(|left, right| left.label.cmp(&right.label));
        }
        StructureRecord::Metallic { sites, domains, .. } => {
            sites.sort_by(|left, right| left.label.cmp(&right.label));
            for domain in domains.iter_mut() {
                domain.sites.sort();
            }
            domains.sort_by(|left, right| left.label.cmp(&right.label));
        }
    }
}

fn normalize_graph_records(
    atoms: &mut [AtomRecord],
    bonds: &mut [BondRecord],
    groups: &mut [GroupRecord],
) {
    atoms.sort_by(|left, right| left.label.cmp(&right.label));
    for bond in bonds.iter_mut() {
        if bond.right < bond.left {
            std::mem::swap(&mut bond.left, &mut bond.right);
        }
    }
    bonds.sort_by(|left, right| {
        (&left.left, &left.right, left.order).cmp(&(&right.left, &right.right, right.order))
    });
    for group in groups.iter_mut() {
        group.atoms.sort();
    }
    groups.sort_by(|left, right| left.label.cmp(&right.label));
}

fn valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
    {
        return false;
    }
    let Ok(year) = value[0..4].parse::<u16>() else {
        return false;
    };
    let Ok(month) = value[5..7].parse::<u8>() else {
        return false;
    };
    let Ok(day) = value[8..10].parse::<u8>() else {
        return false;
    };
    if year == 0 || !(1..=12).contains(&month) {
        return false;
    }
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    (1..=days).contains(&day)
}

fn valid_declared_text_id(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
}

fn validate_label(value: &str, code: CatalogueErrorCode) -> Result<(), CatalogueError> {
    if valid_declared_text_id(value) {
        Ok(())
    } else {
        Err(CatalogueError::new(
            code,
            format!("invalid label `{value}`"),
        ))
    }
}

fn parse_id<K: chem_domain::IdKind>(
    value: &str,
) -> Result<chem_domain::DeclaredId<K>, CatalogueError> {
    validate_label(value, CatalogueErrorCode::InvalidStructure)?;
    chem_domain::DeclaredId::<K>::from_str(value).map_err(|error| {
        CatalogueError::new(CatalogueErrorCode::InvalidStructure, error.to_string())
    })
}

fn require_premise(
    id: &PremiseId,
    premises: &BTreeMap<PremiseId, usize>,
) -> Result<(), CatalogueError> {
    if premises.contains_key(id) {
        Ok(())
    } else {
        Err(CatalogueError::new(
            CatalogueErrorCode::UnknownReference,
            format!("premise `{id}` does not resolve"),
        ))
    }
}

fn structure_error(error: impl fmt::Display) -> CatalogueError {
    CatalogueError::new(CatalogueErrorCode::InvalidStructure, error.to_string())
}

fn duplicate_id<I: fmt::Display, T>(id: &I) -> Result<T, CatalogueError> {
    Err(CatalogueError::new(
        CatalogueErrorCode::DuplicateId,
        format!("duplicate identifier `{id}`"),
    ))
}

fn rule_error<T>(id: &ReactionRuleId, message: impl Into<String>) -> Result<T, CatalogueError> {
    Err(CatalogueError::new(
        CatalogueErrorCode::InvalidRule,
        format!("rule `{id}`: {}", message.into()),
    ))
}

fn mapping_error<T>(id: &ReactionRuleId, message: impl Into<String>) -> Result<T, CatalogueError> {
    Err(CatalogueError::new(
        CatalogueErrorCode::InvalidMapping,
        format!("rule `{id}`: {}", message.into()),
    ))
}

fn operation_error<T>(
    id: &ReactionRuleId,
    message: impl Into<String>,
) -> Result<T, CatalogueError> {
    Err(CatalogueError::new(
        CatalogueErrorCode::InvalidOperationTemplate,
        format!("rule `{id}`: {}", message.into()),
    ))
}

fn applicability_error<T>(
    id: &ReactionRuleId,
    message: impl Into<String>,
) -> Result<T, CatalogueError> {
    Err(CatalogueError::new(
        CatalogueErrorCode::InvalidApplicability,
        format!("rule `{id}`: {}", message.into()),
    ))
}

#[cfg(test)]
mod tests {
    use super::{CleavageAllocationRecord, allocation_local_deltas};

    #[test]
    fn covalent_allocations_scale_for_every_bond_order_delta() {
        let homolytic = CleavageAllocationRecord::Homolytic("homolytic".to_owned());
        let to_left = CleavageAllocationRecord::Heterolytic {
            heterolytic_to: "left".to_owned(),
        };
        for order in 1_i16..=3 {
            assert_eq!(
                allocation_local_deltas(&homolytic, "left", "right", -order),
                Some((order, order))
            );
            assert_eq!(
                allocation_local_deltas(&to_left, "left", "right", -order),
                Some((2 * order, 0))
            );
        }
        assert_eq!(
            allocation_local_deltas(&homolytic, "left", "right", 2),
            Some((-2, -2))
        );
        assert_eq!(
            allocation_local_deltas(&to_left, "left", "right", 2),
            Some((-4, 0))
        );
    }
}
