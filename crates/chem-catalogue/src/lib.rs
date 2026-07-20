//! Immutable, content-addressed structural chemistry catalogue bundles.
//!
//! A [`ValidatedCatalogueBundle`] has passed structural validation. A
//! [`ReferenceCatalogue`] identifies structurally valid data used as a local
//! reference. It records whether optional package review metadata verified;
//! neither type authorizes chemistry or acts as an allow-list.

mod generalized;
mod generalized_elaboration;
mod model;
mod oxygen;
mod pattern;
mod validation;

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    ops::Deref,
    str::FromStr,
};

use chem_domain::{
    Atom, AtomGroup, AtomId, BondOrder, ContentDigest, CovalentBond, CovalentDelocalization,
    CovalentElectronOrigin, EffectiveBondOrder, ElectronState, Element, ElementId,
    ElementInventory, ElementSymbol, EvidenceSourceId, IonicAssociation, MetallicDomain, PremiseId,
    ReactionRuleId, RepresentationKind, StaticElementRegistry, StructuralGraph,
    StructureDefinition, StructureId, canonical_json,
};
pub use generalized::*;
pub use generalized_elaboration::*;
pub use model::*;
pub use oxygen::*;
pub use pattern::*;

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
    InvalidElement,
    InvalidElementCategory,
    InvalidStructuralTrait,
    InvalidStructureTemplate,
    InvalidStructureApplication,
    InvalidGraphPattern,
    InvalidGeneralizedRule,
    InvalidGeneralizedCase,
    InvalidMacroscopicMaterial,
    IntegrityMismatch,
    InvalidReviewAttestation,
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
            Self::InvalidElement => "CHEMS-C016",
            Self::InvalidElementCategory => "CHEMS-C017",
            Self::InvalidStructuralTrait => "CHEMS-C018",
            Self::InvalidStructureTemplate => "CHEMS-C019",
            Self::InvalidStructureApplication => "CHEMS-C020",
            Self::InvalidGraphPattern => "CHEMS-C021",
            Self::InvalidGeneralizedRule => "CHEMS-C022",
            Self::InvalidGeneralizedCase => "CHEMS-C023",
            Self::InvalidMacroscopicMaterial => "CHEMS-C024",
            Self::IntegrityMismatch => "CHEMS-C025",
            Self::InvalidReviewAttestation => "CHEMS-C026",
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

/// Fully validated, immutable and lookup-indexed reference catalogue data.
#[derive(Debug, Clone)]
pub struct ValidatedCatalogueBundle {
    digest: ContentDigest,
    document: CatalogueDocument,
    structures: BTreeMap<StructureId, StructureDefinition>,
    structure_premises: BTreeMap<StructureId, BTreeSet<PremiseId>>,
    premises: BTreeMap<PremiseId, usize>,
    evidence: BTreeMap<EvidenceSourceId, usize>,
    valence_premises: BTreeMap<PremiseId, usize>,
    rules: BTreeMap<ReactionRuleId, ValidatedReactionRule>,
    elements: BTreeMap<ElementSymbol, usize>,
    element_categories: BTreeMap<ElementCategoryId, usize>,
    element_category_members: BTreeMap<ElementCategoryId, BTreeSet<ElementSymbol>>,
    element_membership_provenance:
        BTreeMap<(ElementSymbol, ElementCategoryId), ElementMembershipProvenance>,
    structural_traits: BTreeMap<StructuralTraitId, usize>,
    structure_templates: BTreeMap<StructureTemplateId, usize>,
    structure_applications: BTreeMap<StructureId, usize>,
    structure_aliases: BTreeMap<String, StructureId>,
    structure_traits:
        BTreeMap<StructureId, BTreeMap<StructuralTraitId, StructuralTraitAssertionRecord>>,
    structure_application_provenance: BTreeMap<StructureId, StructureTemplateApplicationProvenance>,
    graph_patterns: BTreeMap<GraphPatternId, usize>,
    generalized_rules: BTreeMap<ReactionRuleId, ValidatedGeneralizedRule>,
    macroscopic_materials: BTreeMap<(StructureId, MacroscopicMaterialContextRecord), usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementMembershipProvenance {
    pub element_premise_ids: BTreeSet<PremiseId>,
    pub category_premise_ids: BTreeSet<PremiseId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructureTemplateApplicationProvenance {
    pub template_premise_ids: BTreeSet<PremiseId>,
    pub argument_element_premise_ids: BTreeSet<PremiseId>,
    pub argument_category_premise_ids: BTreeSet<PremiseId>,
    pub argument_structure_premise_ids: BTreeSet<PremiseId>,
    pub application_premise_ids: BTreeSet<PremiseId>,
    pub trait_definition_premise_ids: BTreeSet<PremiseId>,
    pub trait_assertion_premise_ids: BTreeSet<PremiseId>,
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

fn ensure_rule_namespaces_disjoint(
    rules: &BTreeMap<ReactionRuleId, ValidatedReactionRule>,
    generalized_rules: &BTreeMap<ReactionRuleId, ValidatedGeneralizedRule>,
) -> Result<(), CatalogueError> {
    if let Some(id) = generalized_rules.keys().find(|id| rules.contains_key(*id)) {
        return duplicate_id(id);
    }
    Ok(())
}

impl ValidatedCatalogueBundle {
    /// Parses and validates an external catalogue envelope.
    ///
    /// # Errors
    ///
    /// Every malformed or inconsistent reference-data condition returns a typed
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
        validation::validate(envelope)
    }

    #[must_use]
    pub const fn digest(&self) -> ContentDigest {
        self.digest
    }

    #[must_use]
    pub const fn document(&self) -> &CatalogueDocument {
        &self.document
    }

    /// Validates an external review against this exact catalogue and returns
    /// the review's canonical semantic digest. This records factual provenance;
    /// it does not grant runtime authority.
    ///
    /// # Errors
    ///
    /// Rejects malformed review JSON or a review that does not bind every
    /// exact catalogue premise and evidence source.
    pub fn validate_review_attestation(
        &self,
        review_json: &[u8],
    ) -> Result<ContentDigest, CatalogueError> {
        validate_review_attestation(self, review_json)
    }

    #[must_use]
    pub fn structure(&self, id: &StructureId) -> Option<&StructureDefinition> {
        self.structures.get(id)
    }

    #[must_use]
    pub fn element(&self, symbol: &ElementSymbol) -> Option<&ElementRecord> {
        self.elements
            .get(symbol)
            .map(|index| &self.document.elements[*index])
    }

    #[must_use]
    pub fn element_category(&self, id: &ElementCategoryId) -> Option<&ElementCategoryRecord> {
        self.element_categories
            .get(id)
            .map(|index| &self.document.element_categories[*index])
    }

    #[must_use]
    pub fn element_category_members(
        &self,
        id: &ElementCategoryId,
    ) -> Option<&BTreeSet<ElementSymbol>> {
        self.element_category_members.get(id)
    }

    #[must_use]
    pub fn element_is_member(
        &self,
        symbol: &ElementSymbol,
        category_id: &ElementCategoryId,
    ) -> Option<bool> {
        self.element(symbol)?;
        Some(self.element_category_members(category_id)?.contains(symbol))
    }

    #[must_use]
    pub fn element_membership_provenance(
        &self,
        symbol: &ElementSymbol,
        category_id: &ElementCategoryId,
    ) -> Option<&ElementMembershipProvenance> {
        self.element_membership_provenance
            .get(&(symbol.clone(), category_id.clone()))
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
        if let Some(record) = self
            .document
            .structures
            .iter()
            .find(|record| record.id() == id)
        {
            return self.premise(record.premise_id());
        }
        self.structure_application(id)
            .filter(|application| application.premise_ids.len() == 1)
            .and_then(|application| application.premise_ids.first())
            .and_then(|premise| self.premise(premise))
    }

    #[must_use]
    pub fn structure_premises(&self, id: &StructureId) -> Option<&BTreeSet<PremiseId>> {
        self.structure_premises.get(id)
    }

    #[must_use]
    pub fn structure_by_alias(&self, alias: &str) -> Option<&StructureDefinition> {
        self.structure_aliases
            .get(alias)
            .and_then(|id| self.structure(id))
    }

    #[must_use]
    pub fn structural_trait(
        &self,
        id: &StructuralTraitId,
    ) -> Option<&StructuralTraitDefinitionRecord> {
        self.structural_traits
            .get(id)
            .map(|index| &self.document.structural_traits[*index])
    }

    #[must_use]
    pub fn structure_template(&self, id: &StructureTemplateId) -> Option<&StructureTemplateRecord> {
        self.structure_templates
            .get(id)
            .map(|index| &self.document.structure_templates[*index])
    }

    #[must_use]
    pub fn structure_application(
        &self,
        id: &StructureId,
    ) -> Option<&StructureTemplateApplicationRecord> {
        self.structure_applications
            .get(id)
            .map(|index| &self.document.structure_applications[*index])
    }

    #[must_use]
    pub fn structure_trait_assertion(
        &self,
        structure: &StructureId,
        trait_id: &StructuralTraitId,
    ) -> Option<&StructuralTraitAssertionRecord> {
        self.structure_traits.get(structure)?.get(trait_id)
    }

    #[must_use]
    pub fn structure_application_provenance(
        &self,
        id: &StructureId,
    ) -> Option<&StructureTemplateApplicationProvenance> {
        self.structure_application_provenance.get(id)
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

    /// Resolves reviewed macroscopic state for a structure. A matching
    /// rule-role fact wins over the standard-context fallback.
    #[must_use]
    pub fn macroscopic_material(
        &self,
        structure: &StructureId,
        rule_role: Option<(&ReactionRuleId, &str)>,
    ) -> Option<&MacroscopicMaterialRecord> {
        let role_record = rule_role.and_then(|(rule, role)| {
            self.macroscopic_materials
                .get(&(
                    structure.clone(),
                    MacroscopicMaterialContextRecord::ReactionRole {
                        rule: rule.clone(),
                        role: role.to_owned(),
                    },
                ))
                .map(|index| &self.document.macroscopic_materials[*index])
        });
        role_record.or_else(|| {
            self.macroscopic_materials
                .get(&(
                    structure.clone(),
                    MacroscopicMaterialContextRecord::Standard,
                ))
                .map(|index| &self.document.macroscopic_materials[*index])
        })
    }
}

/// Structurally valid bundled reference data with optional review provenance.
#[derive(Debug, Clone)]
pub struct ReferenceCatalogue {
    validated: ValidatedCatalogueBundle,
    reviewed: bool,
}

/// The immutable digests expected for one packaged reference-data release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReferenceIntegrityPolicy {
    catalogue_digest: ContentDigest,
    review_digest: ContentDigest,
}

impl ReferenceIntegrityPolicy {
    #[must_use]
    pub const fn new(catalogue_digest: ContentDigest, review_digest: ContentDigest) -> Self {
        Self {
            catalogue_digest,
            review_digest,
        }
    }

    #[must_use]
    pub const fn catalogue_digest(self) -> ContentDigest {
        self.catalogue_digest
    }

    #[must_use]
    pub const fn review_digest(self) -> ContentDigest {
        self.review_digest
    }
}

impl ReferenceCatalogue {
    /// Loads structurally valid reference data without requiring review
    /// metadata. The resulting catalogue retains provisional provenance.
    ///
    /// # Errors
    ///
    /// Rejects invalid catalogue data.
    pub fn from_json(catalogue_json: &[u8]) -> Result<Self, CatalogueError> {
        Ok(Self {
            validated: ValidatedCatalogueBundle::from_json(catalogue_json)?,
            reviewed: false,
        })
    }

    #[must_use]
    pub const fn is_reviewed(&self) -> bool {
        self.reviewed
    }

    /// Loads catalogue and review JSON under an exact package-integrity policy.
    ///
    /// # Errors
    ///
    /// Rejects invalid catalogue data, either unpinned artifact, or a review
    /// that does not bind every exact premise and evidence source.
    pub fn from_canonical_json(
        catalogue_json: &[u8],
        review_json: &[u8],
        policy: ReferenceIntegrityPolicy,
    ) -> Result<Self, CatalogueError> {
        let validated = ValidatedCatalogueBundle::from_json(catalogue_json)?;
        if validated.digest() != policy.catalogue_digest {
            return Err(CatalogueError::new(
                CatalogueErrorCode::IntegrityMismatch,
                format!(
                    "catalogue digest {} does not match packaged identity {}",
                    validated.digest(),
                    policy.catalogue_digest
                ),
            ));
        }
        validate_review_integrity(&validated, review_json, policy.review_digest)?;
        Ok(Self {
            validated,
            reviewed: true,
        })
    }
}

fn validate_review_integrity(
    catalogue: &ValidatedCatalogueBundle,
    review_json: &[u8],
    expected_digest: ContentDigest,
) -> Result<(), CatalogueError> {
    let actual_digest = validate_review_attestation(catalogue, review_json)?;
    if actual_digest != expected_digest {
        return Err(CatalogueError::new(
            CatalogueErrorCode::IntegrityMismatch,
            format!(
                "review digest {actual_digest} does not match packaged identity {expected_digest}"
            ),
        ));
    }
    Ok(())
}

fn validate_review_attestation(
    catalogue: &ValidatedCatalogueBundle,
    review_json: &[u8],
) -> Result<ContentDigest, CatalogueError> {
    let value: serde_json::Value = serde_json::from_slice(review_json).map_err(|error| {
        CatalogueError::new(
            CatalogueErrorCode::InvalidReviewAttestation,
            error.to_string(),
        )
    })?;
    let actual_digest = ContentDigest::of_json(&value).map_err(|error| {
        CatalogueError::new(
            CatalogueErrorCode::InvalidReviewAttestation,
            error.to_string(),
        )
    })?;
    let review: CatalogueReviewAttestation = serde_json::from_value(value).map_err(|error| {
        CatalogueError::new(
            CatalogueErrorCode::InvalidReviewAttestation,
            error.to_string(),
        )
    })?;
    let expected_sources = catalogue
        .document()
        .evidence
        .iter()
        .map(|source| source.id.clone())
        .collect::<BTreeSet<_>>();
    let expected_premises = catalogue
        .document()
        .premises
        .iter()
        .map(|premise| premise.id.clone())
        .collect::<BTreeSet<_>>();
    let required_text = [
        review.id.as_str(),
        review.reviewer.as_str(),
        review.scope.as_str(),
        review.method.as_str(),
        review.coverage_conclusion.as_str(),
        review.limitation.as_str(),
    ];
    if review.schema_version != CATALOGUE_REVIEW_SCHEMA_VERSION
        || review.catalogue_digest != catalogue.digest()
        || !valid_date(&review.reviewed_on)
        || required_text.iter().any(|value| value.trim().is_empty())
        || review.sources != expected_sources
        || review.premises != expected_premises
    {
        return Err(CatalogueError::new(
            CatalogueErrorCode::InvalidReviewAttestation,
            "review must bind the exact catalogue digest, evidence sources, and premises",
        ));
    }
    Ok(actual_digest)
}

impl Deref for ReferenceCatalogue {
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
        || (document.rules.is_empty() && document.generalized_rules.is_empty())
    {
        return Err(CatalogueError::new(
            CatalogueErrorCode::InvalidMetadata,
            "catalogue metadata or required record collections are empty",
        ));
    }
    Ok(())
}

fn validate_macroscopic_materials(
    records: &[MacroscopicMaterialRecord],
    structures: &BTreeMap<StructureId, StructureDefinition>,
    rules: &BTreeMap<ReactionRuleId, ValidatedReactionRule>,
    generalized_rules: &BTreeMap<ReactionRuleId, ValidatedGeneralizedRule>,
    generalized_rule_records: &[GeneralizedReactionRuleRecord],
    premises: &BTreeMap<PremiseId, usize>,
) -> Result<BTreeMap<(StructureId, MacroscopicMaterialContextRecord), usize>, CatalogueError> {
    let mut index = BTreeMap::new();
    for (position, record) in records.iter().enumerate() {
        if !structures.contains_key(&record.structure) {
            return Err(CatalogueError::new(
                CatalogueErrorCode::InvalidMacroscopicMaterial,
                format!(
                    "macroscopic material references unknown structure `{}`",
                    record.structure
                ),
            ));
        }
        if record.premise_ids.is_empty()
            || record
                .premise_ids
                .iter()
                .any(|premise| !premises.contains_key(premise))
        {
            return Err(CatalogueError::new(
                CatalogueErrorCode::InvalidMacroscopicMaterial,
                format!(
                    "macroscopic material `{}` lacks resolvable premises",
                    record.structure
                ),
            ));
        }
        if let MacroscopicMaterialContextRecord::ReactionRole { rule, role } = &record.context {
            let legacy_role_exists = rules
                .get(rule)
                .is_some_and(|validated| validated.record().roles.contains_key(role));
            let generalized_role_exists = generalized_rules.contains_key(rule)
                && generalized_rule_records
                    .iter()
                    .find(|record| record.id == *rule)
                    .is_some_and(|record| record.roles.contains_key(role));
            if role.trim().is_empty() || (!legacy_role_exists && !generalized_role_exists) {
                return Err(CatalogueError::new(
                    CatalogueErrorCode::InvalidMacroscopicMaterial,
                    format!(
                        "macroscopic material `{}` references unknown role `{role}` on `{rule}`",
                        record.structure
                    ),
                ));
            }
        }
        let key = (record.structure.clone(), record.context.clone());
        if index.insert(key, position).is_some() {
            return Err(CatalogueError::new(
                CatalogueErrorCode::InvalidMacroscopicMaterial,
                format!(
                    "duplicate macroscopic material context for `{}`",
                    record.structure
                ),
            ));
        }
    }
    Ok(index)
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
    BTreeMap<StructureId, BTreeSet<PremiseId>>,
);

type ElementIndexes = (
    BTreeMap<ElementSymbol, usize>,
    BTreeMap<ElementCategoryId, usize>,
    BTreeMap<ElementCategoryId, BTreeSet<ElementSymbol>>,
    BTreeMap<(ElementSymbol, ElementCategoryId), ElementMembershipProvenance>,
);

#[allow(clippy::too_many_lines)]
fn validate_elements_and_categories(
    elements: &[ElementRecord],
    categories: &[ElementCategoryRecord],
    premises: &BTreeMap<PremiseId, usize>,
) -> Result<ElementIndexes, CatalogueError> {
    let mut element_index = BTreeMap::new();
    let mut atomic_numbers = BTreeSet::new();
    let mut names = BTreeSet::new();
    let mut domain_elements = Vec::with_capacity(elements.len());
    for (position, record) in elements.iter().enumerate() {
        if element_index
            .insert(record.symbol.clone(), position)
            .is_some()
            || !atomic_numbers.insert(record.atomic_number)
            || !names.insert(record.name.clone())
        {
            return Err(CatalogueError::new(
                CatalogueErrorCode::DuplicateId,
                format!("duplicate element identity `{}`", record.symbol),
            ));
        }
        if !(1..=118).contains(&record.atomic_number)
            || !(1..=7).contains(&record.period)
            || record.group.is_some_and(|group| !(1..=18).contains(&group))
            || record.name.trim().is_empty()
            || record.name != record.name.trim()
            || record.premise_ids.is_empty()
        {
            return Err(CatalogueError::new(
                CatalogueErrorCode::InvalidElement,
                format!("invalid element `{}`", record.symbol),
            ));
        }
        for premise in &record.premise_ids {
            require_premise(premise, premises)?;
        }
        let id = ElementId::new(record.atomic_number).map_err(|error| {
            CatalogueError::new(CatalogueErrorCode::InvalidElement, error.to_string())
        })?;
        domain_elements.push(Element {
            id,
            symbol: record.symbol.clone(),
        });
    }
    StaticElementRegistry::new(domain_elements).map_err(|error| {
        CatalogueError::new(CatalogueErrorCode::InvalidElement, error.to_string())
    })?;

    let mut category_index = BTreeMap::new();
    let mut members_index = BTreeMap::new();
    let mut provenance_index = BTreeMap::new();
    for (position, category) in categories.iter().enumerate() {
        if category_index
            .insert(category.id.clone(), position)
            .is_some()
        {
            return duplicate_id(&category.id);
        }
        if category.premise_ids.is_empty() {
            return invalid_category("category premise set is empty");
        }
        for premise in &category.premise_ids {
            require_premise(premise, premises)?;
        }
        if category.subject != ElementCategorySubjectRecord::Element {
            return invalid_category("unsupported category subject");
        }
        let (mut members, include, exclude) = match &category.membership {
            ElementCategoryMembershipRecord::Explicit { members } => {
                if members.is_empty() {
                    return invalid_category("explicit category is empty");
                }
                (members.clone(), BTreeSet::new(), BTreeSet::new())
            }
            ElementCategoryMembershipRecord::Predicate {
                predicate,
                include,
                exclude,
            } => {
                validate_predicate(predicate)?;
                if !include.is_disjoint(exclude) {
                    return invalid_category("category include and exclude overlap");
                }
                let mut derived = BTreeSet::new();
                for element in elements {
                    if evaluate_predicate(predicate, element) {
                        derived.insert(element.symbol.clone());
                    }
                }
                (derived, include.clone(), exclude.clone())
            }
        };
        for symbol in include.iter().chain(exclude.iter()).chain(members.iter()) {
            if !element_index.contains_key(symbol) {
                return Err(CatalogueError::new(
                    CatalogueErrorCode::UnknownReference,
                    format!(
                        "category `{}` references unknown element `{symbol}`",
                        category.id
                    ),
                ));
            }
        }
        members.extend(include);
        for symbol in exclude {
            members.remove(&symbol);
        }
        if members.is_empty() {
            return invalid_category("category derives no members");
        }
        for symbol in &members {
            let element = &elements[*element_index.get(symbol).expect("validated element")];
            provenance_index.insert(
                (symbol.clone(), category.id.clone()),
                ElementMembershipProvenance {
                    element_premise_ids: element.premise_ids.clone(),
                    category_premise_ids: category.premise_ids.clone(),
                },
            );
        }
        members_index.insert(category.id.clone(), members);
    }
    Ok((
        element_index,
        category_index,
        members_index,
        provenance_index,
    ))
}

fn invalid_category<T>(message: impl Into<String>) -> Result<T, CatalogueError> {
    Err(CatalogueError::new(
        CatalogueErrorCode::InvalidElementCategory,
        message,
    ))
}

fn validate_predicate(predicate: &ElementPredicateRecord) -> Result<(), CatalogueError> {
    match predicate {
        ElementPredicateRecord::All { predicates } | ElementPredicateRecord::Any { predicates } => {
            if predicates.is_empty() {
                return invalid_category("logical predicate is empty");
            }
            for child in predicates {
                validate_predicate(child)?;
            }
            let mut keys = predicates.iter().map(predicate_key).collect::<Vec<_>>();
            keys.sort();
            if keys.windows(2).any(|pair| pair[0] == pair[1]) {
                return invalid_category("duplicate logical predicate");
            }
        }
        ElementPredicateRecord::Not { predicate } => validate_predicate(predicate)?,
        ElementPredicateRecord::Equals { field, value } => validate_scalar(*field, value)?,
        ElementPredicateRecord::Range { field, min, max } => {
            if !matches!(
                field,
                ElementFieldRecord::AtomicNumber
                    | ElementFieldRecord::Period
                    | ElementFieldRecord::Group
            ) || min > max
            {
                return invalid_category("invalid predicate range");
            }
        }
        ElementPredicateRecord::InSet { field, values } => {
            if values.is_empty() {
                return invalid_category("in_set predicate is empty");
            }
            for value in values {
                validate_scalar(*field, value)?;
            }
            let mut keys = values.iter().map(scalar_key).collect::<Vec<_>>();
            keys.sort();
            if keys.windows(2).any(|pair| pair[0] == pair[1]) {
                return invalid_category("duplicate in_set value");
            }
        }
        ElementPredicateRecord::Present { .. } => {}
    }
    Ok(())
}

fn predicate_key(predicate: &ElementPredicateRecord) -> String {
    serde_json::to_string(predicate).expect("predicate is serializable")
}

fn scalar_key(scalar: &ElementScalarRecord) -> String {
    serde_json::to_string(scalar).expect("scalar is serializable")
}

fn validate_scalar(
    field: ElementFieldRecord,
    scalar: &ElementScalarRecord,
) -> Result<(), CatalogueError> {
    let valid = match field {
        ElementFieldRecord::Symbol => {
            matches!(scalar, ElementScalarRecord::String(value) if ElementSymbol::new(value.clone()).is_ok())
        }
        ElementFieldRecord::Name => matches!(scalar, ElementScalarRecord::String(_)),
        ElementFieldRecord::Block => {
            matches!(scalar, ElementScalarRecord::String(value) if matches!(value.as_str(), "s" | "p" | "d" | "f"))
        }
        ElementFieldRecord::AtomicNumber
        | ElementFieldRecord::Period
        | ElementFieldRecord::Group => matches!(scalar, ElementScalarRecord::Integer(_)),
    };
    if valid {
        Ok(())
    } else {
        invalid_category("predicate scalar type mismatch")
    }
}

fn evaluate_predicate(predicate: &ElementPredicateRecord, element: &ElementRecord) -> bool {
    match predicate {
        ElementPredicateRecord::All { predicates } => {
            predicates.iter().all(|p| evaluate_predicate(p, element))
        }
        ElementPredicateRecord::Any { predicates } => {
            predicates.iter().any(|p| evaluate_predicate(p, element))
        }
        ElementPredicateRecord::Not { predicate } => !evaluate_predicate(predicate, element),
        ElementPredicateRecord::Equals { field, value } => {
            scalar_for(*field, element).is_some_and(|actual| actual == *value)
        }
        ElementPredicateRecord::Range { field, min, max } => scalar_for(*field, element)
            .and_then(|value| match value {
                ElementScalarRecord::Integer(value) => Some(value >= *min && value <= *max),
                ElementScalarRecord::String(_) => None,
            })
            .unwrap_or(false),
        ElementPredicateRecord::InSet { field, values } => {
            scalar_for(*field, element).is_some_and(|actual| values.contains(&actual))
        }
        ElementPredicateRecord::Present { field } => {
            *field != ElementFieldRecord::Group || element.group.is_some()
        }
    }
}

fn scalar_for(field: ElementFieldRecord, element: &ElementRecord) -> Option<ElementScalarRecord> {
    match field {
        ElementFieldRecord::Symbol => Some(ElementScalarRecord::String(
            element.symbol.as_str().to_owned(),
        )),
        ElementFieldRecord::Name => Some(ElementScalarRecord::String(element.name.clone())),
        ElementFieldRecord::AtomicNumber => Some(ElementScalarRecord::Integer(i64::from(
            element.atomic_number,
        ))),
        ElementFieldRecord::Period => Some(ElementScalarRecord::Integer(i64::from(element.period))),
        ElementFieldRecord::Group => element
            .group
            .map(|value| ElementScalarRecord::Integer(i64::from(value))),
        ElementFieldRecord::Block => Some(ElementScalarRecord::String(
            match element.block {
                ElementBlockRecord::S => "s",
                ElementBlockRecord::P => "p",
                ElementBlockRecord::D => "d",
                ElementBlockRecord::F => "f",
            }
            .to_owned(),
        )),
    }
}

fn index_structural_traits(
    records: &[StructuralTraitDefinitionRecord],
    premises: &BTreeMap<PremiseId, usize>,
) -> Result<BTreeMap<StructuralTraitId, usize>, CatalogueError> {
    let mut index = BTreeMap::new();
    for (position, record) in records.iter().enumerate() {
        if index.insert(record.id.clone(), position).is_some() {
            return duplicate_id(&record.id);
        }
        if record.sites.is_empty() || record.premise_ids.is_empty() {
            return invalid_trait(&record.id, "site or premise set is empty");
        }
        for premise in &record.premise_ids {
            require_premise(premise, premises)?;
        }
        for site in record.sites.keys() {
            validate_label(site, CatalogueErrorCode::InvalidStructuralTrait)?;
        }
        for (value, projection) in &record.values {
            validate_label(value, CatalogueErrorCode::InvalidStructuralTrait)?;
            validate_trait_projection(record, projection)?;
        }
    }
    Ok(index)
}

fn validate_trait_projection(
    definition: &StructuralTraitDefinitionRecord,
    projection: &StructuralTraitValueProjectionRecord,
) -> Result<(), CatalogueError> {
    let require_site = |site: &str, expected: StructuralTraitSiteKindRecord| {
        if definition.sites.get(site) == Some(&expected) {
            Ok(())
        } else {
            invalid_trait(
                &definition.id,
                format!("projection references missing or wrongly typed site `{site}`"),
            )
        }
    };
    match projection {
        StructuralTraitValueProjectionRecord::AtomElement { site }
        | StructuralTraitValueProjectionRecord::AtomFormalCharge { site }
        | StructuralTraitValueProjectionRecord::AtomNonBondingElectrons { site }
        | StructuralTraitValueProjectionRecord::AtomUnpairedElectrons { site }
        | StructuralTraitValueProjectionRecord::AtomBondOrderSum { site } => {
            require_site(site, StructuralTraitSiteKindRecord::Atom)
        }
        StructuralTraitValueProjectionRecord::CovalentBondOrder {
            left_site,
            right_site,
        }
        | StructuralTraitValueProjectionRecord::CovalentElectronOrigin {
            left_site,
            right_site,
        } => {
            require_site(left_site, StructuralTraitSiteKindRecord::Atom)?;
            require_site(right_site, StructuralTraitSiteKindRecord::Atom)
        }
        StructuralTraitValueProjectionRecord::GroupAtomCount { site } => {
            require_site(site, StructuralTraitSiteKindRecord::Group)
        }
        StructuralTraitValueProjectionRecord::IonicComponentCount { site } => {
            require_site(site, StructuralTraitSiteKindRecord::IonicAssociation)
        }
        StructuralTraitValueProjectionRecord::MetallicSiteCount { site }
        | StructuralTraitValueProjectionRecord::MetallicDelocalizedElectrons { site } => {
            require_site(site, StructuralTraitSiteKindRecord::MetallicDomain)
        }
    }
}

fn validate_concrete_structure_traits(
    records: &[StructureRecord],
    structures: &BTreeMap<StructureId, StructureDefinition>,
    definitions: &[StructuralTraitDefinitionRecord],
    definition_index: &BTreeMap<StructuralTraitId, usize>,
    premises: &BTreeMap<PremiseId, usize>,
) -> Result<
    BTreeMap<StructureId, BTreeMap<StructuralTraitId, StructuralTraitAssertionRecord>>,
    CatalogueError,
> {
    let mut result = BTreeMap::new();
    for record in records {
        let assertions = validate_trait_assertions(
            record.traits(),
            &structures[record.id()],
            definitions,
            definition_index,
            premises,
        )?;
        if !assertions.is_empty() {
            result.insert(record.id().clone(), assertions);
        }
    }
    Ok(result)
}

fn validate_trait_assertion_shape(
    assertion: &StructuralTraitAssertionRecord,
    definition: &StructuralTraitDefinitionRecord,
    premises: &BTreeMap<PremiseId, usize>,
) -> Result<(), CatalogueError> {
    if assertion.premise_ids.is_empty()
        || assertion.sites.keys().collect::<BTreeSet<_>>()
            != definition.sites.keys().collect::<BTreeSet<_>>()
        || assertion.values.keys().collect::<BTreeSet<_>>()
            != definition.values.keys().collect::<BTreeSet<_>>()
    {
        return invalid_trait(
            &assertion.trait_id,
            "assertion sites, values, or premises do not match its definition",
        );
    }
    for premise in &assertion.premise_ids {
        require_premise(premise, premises)?;
    }
    Ok(())
}

fn validate_trait_assertions(
    assertions: &[StructuralTraitAssertionRecord],
    structure: &StructureDefinition,
    definitions: &[StructuralTraitDefinitionRecord],
    definition_index: &BTreeMap<StructuralTraitId, usize>,
    premises: &BTreeMap<PremiseId, usize>,
) -> Result<BTreeMap<StructuralTraitId, StructuralTraitAssertionRecord>, CatalogueError> {
    let mut result = BTreeMap::new();
    for assertion in assertions {
        let Some(position) = definition_index.get(&assertion.trait_id) else {
            return Err(CatalogueError::new(
                CatalogueErrorCode::UnknownReference,
                format!("trait `{}` does not resolve", assertion.trait_id),
            ));
        };
        let definition = &definitions[*position];
        validate_trait_assertion_shape(assertion, definition, premises)?;
        if result
            .insert(assertion.trait_id.clone(), assertion.clone())
            .is_some()
        {
            return duplicate_id(&assertion.trait_id);
        }
        for (name, kind) in &definition.sites {
            let site = &assertion.sites[name];
            if !trait_site_exists(*kind, site, structure.graph()) {
                return invalid_trait(
                    &assertion.trait_id,
                    format!("site `{name}` resolves to absent or wrongly typed `{site}`"),
                );
            }
        }
        for (name, projection) in &definition.values {
            let actual = project_trait_value(projection, assertion, structure.graph())?;
            if actual != assertion.values[name] {
                return invalid_trait(
                    &assertion.trait_id,
                    format!("asserted value `{name}` does not equal the graph projection"),
                );
            }
        }
    }
    Ok(result)
}

fn trait_site_exists(
    kind: StructuralTraitSiteKindRecord,
    value: &str,
    graph: &StructuralGraph,
) -> bool {
    match kind {
        StructuralTraitSiteKindRecord::Atom => {
            chem_domain::AtomId::from_str(value).is_ok_and(|id| graph.atoms().contains_key(&id))
        }
        StructuralTraitSiteKindRecord::CovalentBond => chem_domain::CovalentBondId::from_str(value)
            .is_ok_and(|id| graph.covalent_bonds().contains_key(&id)),
        StructuralTraitSiteKindRecord::Group => chem_domain::AtomGroupId::from_str(value)
            .is_ok_and(|id| graph.groups().contains_key(&id)),
        StructuralTraitSiteKindRecord::IonicAssociation => {
            chem_domain::IonicAssociationId::from_str(value)
                .is_ok_and(|id| graph.ionic_associations().contains_key(&id))
        }
        StructuralTraitSiteKindRecord::MetallicDomain => {
            chem_domain::MetallicDomainId::from_str(value)
                .is_ok_and(|id| graph.metallic_domains().contains_key(&id))
        }
    }
}

#[allow(clippy::too_many_lines)]
fn project_trait_value(
    projection: &StructuralTraitValueProjectionRecord,
    assertion: &StructuralTraitAssertionRecord,
    graph: &StructuralGraph,
) -> Result<StructuralTraitScalarRecord, CatalogueError> {
    let atom = |site: &str| {
        let id = chem_domain::AtomId::from_str(&assertion.sites[site]).map_err(|error| {
            CatalogueError::new(
                CatalogueErrorCode::InvalidStructuralTrait,
                error.to_string(),
            )
        })?;
        Ok::<_, CatalogueError>(&graph.atoms()[&id])
    };
    let integer = |value: u64| {
        i64::try_from(value)
            .map(StructuralTraitScalarRecord::Integer)
            .map_err(|_| {
                CatalogueError::new(
                    CatalogueErrorCode::InvalidStructuralTrait,
                    "trait projection exceeds the scalar range",
                )
            })
    };
    match projection {
        StructuralTraitValueProjectionRecord::AtomElement { site } => Ok(
            StructuralTraitScalarRecord::String(atom(site)?.element().as_str().to_owned()),
        ),
        StructuralTraitValueProjectionRecord::AtomFormalCharge { site } => {
            Ok(StructuralTraitScalarRecord::Integer(i64::from(
                atom(site)?.electrons().formal_charge(),
            )))
        }
        StructuralTraitValueProjectionRecord::AtomNonBondingElectrons { site } => {
            Ok(StructuralTraitScalarRecord::Integer(i64::from(
                atom(site)?.electrons().non_bonding_electrons(),
            )))
        }
        StructuralTraitValueProjectionRecord::AtomUnpairedElectrons { site } => {
            Ok(StructuralTraitScalarRecord::Integer(i64::from(
                atom(site)?.electrons().unpaired_electrons(),
            )))
        }
        StructuralTraitValueProjectionRecord::AtomBondOrderSum { site } => {
            let atom = atom(site)?;
            integer(
                graph
                    .covalent_bond_order_sum(atom.id())
                    .expect("validated trait atom belongs to graph"),
            )
        }
        StructuralTraitValueProjectionRecord::CovalentBondOrder {
            left_site,
            right_site,
        } => {
            let bond = trait_bond(left_site, right_site, assertion, graph)?;
            Ok(StructuralTraitScalarRecord::String(
                match bond.order() {
                    BondOrder::Single => "single",
                    BondOrder::Double => "double",
                    BondOrder::Triple => "triple",
                }
                .to_owned(),
            ))
        }
        StructuralTraitValueProjectionRecord::CovalentElectronOrigin {
            left_site,
            right_site,
        } => {
            let left = chem_domain::AtomId::from_str(&assertion.sites[left_site]).unwrap();
            let right = chem_domain::AtomId::from_str(&assertion.sites[right_site]).unwrap();
            let bond = trait_bond(left_site, right_site, assertion, graph)?;
            let value = match bond.electron_origin() {
                CovalentElectronOrigin::Shared => "shared",
                CovalentElectronOrigin::Dative { donor, acceptor }
                    if donor == &left && acceptor == &right =>
                {
                    "dative_left_to_right"
                }
                CovalentElectronOrigin::Dative { donor, acceptor }
                    if donor == &right && acceptor == &left =>
                {
                    "dative_right_to_left"
                }
                CovalentElectronOrigin::Dative { .. } => {
                    return Err(CatalogueError::new(
                        CatalogueErrorCode::InvalidStructuralTrait,
                        "dative trait edge direction is inconsistent",
                    ));
                }
            };
            Ok(StructuralTraitScalarRecord::String(value.to_owned()))
        }
        StructuralTraitValueProjectionRecord::GroupAtomCount { site } => {
            let id = chem_domain::AtomGroupId::from_str(&assertion.sites[site]).unwrap();
            integer(graph.groups()[&id].atoms().len() as u64)
        }
        StructuralTraitValueProjectionRecord::IonicComponentCount { site } => {
            let id = chem_domain::IonicAssociationId::from_str(&assertion.sites[site]).unwrap();
            integer(graph.ionic_associations()[&id].components().len() as u64)
        }
        StructuralTraitValueProjectionRecord::MetallicSiteCount { site } => {
            let id = chem_domain::MetallicDomainId::from_str(&assertion.sites[site]).unwrap();
            integer(graph.metallic_domains()[&id].sites().len() as u64)
        }
        StructuralTraitValueProjectionRecord::MetallicDelocalizedElectrons { site } => {
            let id = chem_domain::MetallicDomainId::from_str(&assertion.sites[site]).unwrap();
            integer(u64::from(
                graph.metallic_domains()[&id].delocalized_electrons(),
            ))
        }
    }
}

fn trait_bond<'a>(
    left_site: &str,
    right_site: &str,
    assertion: &StructuralTraitAssertionRecord,
    graph: &'a StructuralGraph,
) -> Result<&'a CovalentBond, CatalogueError> {
    let left = chem_domain::AtomId::from_str(&assertion.sites[left_site]).unwrap();
    let right = chem_domain::AtomId::from_str(&assertion.sites[right_site]).unwrap();
    graph
        .covalent_bonds()
        .values()
        .find(|bond| {
            (bond.left() == &left && bond.right() == &right)
                || (bond.left() == &right && bond.right() == &left)
        })
        .ok_or_else(|| {
            CatalogueError::new(
                CatalogueErrorCode::InvalidStructuralTrait,
                format!(
                    "trait `{}` references an absent covalent edge",
                    assertion.trait_id
                ),
            )
        })
}

fn invalid_trait<T>(
    id: &StructuralTraitId,
    message: impl Into<String>,
) -> Result<T, CatalogueError> {
    Err(CatalogueError::new(
        CatalogueErrorCode::InvalidStructuralTrait,
        format!("trait `{id}`: {}", message.into()),
    ))
}

struct G1Indexes {
    templates: BTreeMap<StructureTemplateId, usize>,
    applications: BTreeMap<StructureId, usize>,
    aliases: BTreeMap<String, StructureId>,
    provenance: BTreeMap<StructureId, StructureTemplateApplicationProvenance>,
}

struct G1ValidationContext<'a> {
    templates: &'a [StructureTemplateRecord],
    applications: &'a [StructureTemplateApplicationRecord],
    elements: &'a [ElementRecord],
    element_index: &'a BTreeMap<ElementSymbol, usize>,
    category_members: &'a BTreeMap<ElementCategoryId, BTreeSet<ElementSymbol>>,
    membership_provenance:
        &'a BTreeMap<(ElementSymbol, ElementCategoryId), ElementMembershipProvenance>,
    trait_definitions: &'a [StructuralTraitDefinitionRecord],
    trait_index: &'a BTreeMap<StructuralTraitId, usize>,
    premises: &'a BTreeMap<PremiseId, usize>,
    valence: &'a [ValencePremiseRecord],
    structures: &'a mut BTreeMap<StructureId, StructureDefinition>,
    structure_premises: &'a mut BTreeMap<StructureId, BTreeSet<PremiseId>>,
    structure_traits:
        &'a mut BTreeMap<StructureId, BTreeMap<StructuralTraitId, StructuralTraitAssertionRecord>>,
}

#[allow(clippy::too_many_lines)]
fn validate_structure_templates_and_applications(
    context: G1ValidationContext<'_>,
) -> Result<G1Indexes, CatalogueError> {
    let G1ValidationContext {
        templates,
        applications,
        elements,
        element_index,
        category_members,
        membership_provenance,
        trait_definitions,
        trait_index,
        premises,
        valence,
        structures,
        structure_premises,
        structure_traits,
    } = context;
    let template_index = index_structure_templates(
        templates,
        element_index,
        category_members,
        trait_definitions,
        trait_index,
        premises,
    )?;
    let concrete_structure_ids = structures.keys().cloned().collect::<BTreeSet<_>>();
    let mut application_index = BTreeMap::new();
    let mut provenance = BTreeMap::new();

    for (position, application) in applications.iter().enumerate() {
        if application_index
            .insert(application.id.clone(), position)
            .is_some()
            || structures.contains_key(&application.id)
        {
            return duplicate_id(&application.id);
        }
        let Some(template_position) = template_index.get(&application.template) else {
            return Err(CatalogueError::new(
                CatalogueErrorCode::UnknownReference,
                format!(
                    "application `{}` references template `{}`",
                    application.id, application.template
                ),
            ));
        };
        let template = &templates[*template_position];
        validate_application_arguments(
            application,
            template,
            elements,
            element_index,
            category_members,
            membership_provenance,
        )?;
        if application.premise_ids.is_empty() || application.formula.trim().is_empty() {
            return invalid_application(&application.id, "formula or premise set is empty");
        }
        for premise in &application.premise_ids {
            require_premise(premise, premises)?;
        }
        for alias in &application.aliases {
            validate_label(alias, CatalogueErrorCode::InvalidStructureApplication)?;
            if alias != alias.trim() {
                return invalid_application(&application.id, "alias is not trimmed");
            }
        }

        let record = instantiate_template_record(template, application)?;
        let definition = build_structure(&record).map_err(|error| {
            CatalogueError::new(
                CatalogueErrorCode::InvalidStructureApplication,
                format!("application `{}`: {error}", application.id),
            )
        })?;
        validate_graph_valence(&definition, valence).map_err(|error| {
            CatalogueError::new(
                CatalogueErrorCode::InvalidStructureApplication,
                format!("application `{}`: {error}", application.id),
            )
        })?;
        let assertions = validate_trait_assertions(
            template.traits(),
            &definition,
            trait_definitions,
            trait_index,
            premises,
        )?;

        let mut application_provenance = StructureTemplateApplicationProvenance {
            template_premise_ids: template.premise_ids().clone(),
            argument_element_premise_ids: BTreeSet::new(),
            argument_category_premise_ids: BTreeSet::new(),
            argument_structure_premise_ids: BTreeSet::new(),
            application_premise_ids: application.premise_ids.clone(),
            trait_definition_premise_ids: BTreeSet::new(),
            trait_assertion_premise_ids: BTreeSet::new(),
        };
        for (parameter_name, parameter) in template.parameters() {
            if let StructureTemplateParameterRecord::Element { category } = parameter {
                let symbol = ElementSymbol::new(&application.arguments[parameter_name]).unwrap();
                application_provenance
                    .argument_element_premise_ids
                    .extend(elements[element_index[&symbol]].premise_ids.iter().cloned());
                let member_provenance = &membership_provenance[&(symbol, category.clone())];
                application_provenance
                    .argument_category_premise_ids
                    .extend(member_provenance.category_premise_ids.iter().cloned());
            }
        }
        for assertion in template.traits() {
            application_provenance
                .trait_assertion_premise_ids
                .extend(assertion.premise_ids.iter().cloned());
            application_provenance.trait_definition_premise_ids.extend(
                trait_definitions[trait_index[&assertion.trait_id]]
                    .premise_ids
                    .iter()
                    .cloned(),
            );
        }
        let effective = effective_application_premises(&application_provenance);
        structure_premises.insert(application.id.clone(), effective);
        if !assertions.is_empty() {
            structure_traits.insert(application.id.clone(), assertions);
        }
        structures.insert(application.id.clone(), definition);
        provenance.insert(application.id.clone(), application_provenance);
    }

    let mut structure_arguments = BTreeMap::<StructureId, Vec<StructureId>>::new();
    let mut structure_argument_trait_premises = BTreeMap::<StructureId, BTreeSet<PremiseId>>::new();
    for application in applications {
        let template = &templates[template_index[&application.template]];
        for (parameter_name, parameter) in template.parameters() {
            let StructureTemplateParameterRecord::Structure { traits } = parameter else {
                continue;
            };
            let structure_id = StructureId::from_str(&application.arguments[parameter_name])
                .map_err(|error| {
                    CatalogueError::new(
                        CatalogueErrorCode::InvalidStructureApplication,
                        error.to_string(),
                    )
                })?;
            if !structures.contains_key(&structure_id) {
                return Err(CatalogueError::new(
                    CatalogueErrorCode::UnknownReference,
                    format!(
                        "application `{}` references structure `{structure_id}`",
                        application.id
                    ),
                ));
            }
            if traits.iter().any(|required| {
                !structure_traits
                    .get(&structure_id)
                    .is_some_and(|actual| actual.contains_key(required))
            }) {
                return invalid_application(
                    &application.id,
                    format!("structure argument `{parameter_name}` lacks a required trait"),
                );
            }
            for required in traits {
                let assertion = &structure_traits[&structure_id][required];
                structure_argument_trait_premises
                    .entry(application.id.clone())
                    .or_default()
                    .extend(assertion.premise_ids.iter().cloned());
                structure_argument_trait_premises
                    .entry(application.id.clone())
                    .or_default()
                    .extend(
                        trait_definitions[trait_index[required]]
                            .premise_ids
                            .iter()
                            .cloned(),
                    );
            }
            structure_arguments
                .entry(application.id.clone())
                .or_default()
                .push(structure_id);
        }
    }
    let mut base_premises = structure_premises.clone();
    for (application_id, trait_premises) in &structure_argument_trait_premises {
        base_premises
            .get_mut(application_id)
            .expect("validated application has structure premises")
            .extend(trait_premises.iter().cloned());
    }
    let application_ids = applications
        .iter()
        .map(|application| application.id.clone())
        .collect::<BTreeSet<_>>();
    let mut resolved_premises = BTreeMap::new();
    for application in applications {
        resolve_application_premises(
            &application.id,
            &application_ids,
            &structure_arguments,
            &base_premises,
            &mut BTreeSet::new(),
            &mut resolved_premises,
        )?;
    }
    for application in applications {
        let mut argument_premises = structure_argument_trait_premises
            .get(&application.id)
            .cloned()
            .unwrap_or_default();
        for argument in structure_arguments
            .get(&application.id)
            .into_iter()
            .flatten()
        {
            let contributing = resolved_premises
                .get(argument)
                .unwrap_or(&base_premises[argument]);
            argument_premises.extend(contributing.iter().cloned());
        }
        provenance
            .get_mut(&application.id)
            .unwrap()
            .argument_structure_premise_ids = argument_premises;
        structure_premises.insert(
            application.id.clone(),
            resolved_premises[&application.id].clone(),
        );
    }

    let mut aliases = BTreeMap::new();
    let all_ids = structures
        .keys()
        .map(ToString::to_string)
        .collect::<BTreeSet<_>>();
    for application in applications {
        for alias in &application.aliases {
            if all_ids.contains(alias)
                || aliases
                    .insert(alias.clone(), application.id.clone())
                    .is_some()
            {
                return invalid_application(&application.id, format!("alias `{alias}` collides"));
            }
        }
    }

    debug_assert!(concrete_structure_ids.is_subset(&structures.keys().cloned().collect()));
    Ok(G1Indexes {
        templates: template_index,
        applications: application_index,
        aliases,
        provenance,
    })
}

fn resolve_application_premises(
    id: &StructureId,
    application_ids: &BTreeSet<StructureId>,
    structure_arguments: &BTreeMap<StructureId, Vec<StructureId>>,
    base_premises: &BTreeMap<StructureId, BTreeSet<PremiseId>>,
    visiting: &mut BTreeSet<StructureId>,
    resolved: &mut BTreeMap<StructureId, BTreeSet<PremiseId>>,
) -> Result<BTreeSet<PremiseId>, CatalogueError> {
    if let Some(premises) = resolved.get(id) {
        return Ok(premises.clone());
    }
    if !application_ids.contains(id) {
        return Ok(base_premises[id].clone());
    }
    if !visiting.insert(id.clone()) {
        return invalid_application(id, "structure-parameter dependency cycle");
    }
    let mut premises = base_premises[id].clone();
    for argument in structure_arguments.get(id).into_iter().flatten() {
        premises.extend(resolve_application_premises(
            argument,
            application_ids,
            structure_arguments,
            base_premises,
            visiting,
            resolved,
        )?);
    }
    visiting.remove(id);
    resolved.insert(id.clone(), premises.clone());
    Ok(premises)
}

fn effective_application_premises(
    provenance: &StructureTemplateApplicationProvenance,
) -> BTreeSet<PremiseId> {
    provenance
        .template_premise_ids
        .iter()
        .chain(&provenance.argument_element_premise_ids)
        .chain(&provenance.argument_category_premise_ids)
        .chain(&provenance.argument_structure_premise_ids)
        .chain(&provenance.application_premise_ids)
        .chain(&provenance.trait_definition_premise_ids)
        .chain(&provenance.trait_assertion_premise_ids)
        .cloned()
        .collect()
}

fn index_structure_templates(
    records: &[StructureTemplateRecord],
    elements: &BTreeMap<ElementSymbol, usize>,
    categories: &BTreeMap<ElementCategoryId, BTreeSet<ElementSymbol>>,
    trait_definitions: &[StructuralTraitDefinitionRecord],
    trait_index: &BTreeMap<StructuralTraitId, usize>,
    premises: &BTreeMap<PremiseId, usize>,
) -> Result<BTreeMap<StructureTemplateId, usize>, CatalogueError> {
    let mut index = BTreeMap::new();
    for (position, record) in records.iter().enumerate() {
        if index.insert(record.id().clone(), position).is_some() {
            return duplicate_id(record.id());
        }
        if record.parameters().is_empty() || record.premise_ids().is_empty() {
            return invalid_template(record.id(), "parameter or premise set is empty");
        }
        for premise in record.premise_ids() {
            require_premise(premise, premises)?;
        }
        for (name, parameter) in record.parameters() {
            validate_label(name, CatalogueErrorCode::InvalidStructureTemplate)?;
            match parameter {
                StructureTemplateParameterRecord::Element { category } => {
                    if !categories.contains_key(category) {
                        return Err(CatalogueError::new(
                            CatalogueErrorCode::UnknownReference,
                            format!(
                                "template `{}` category `{category}` does not resolve",
                                record.id()
                            ),
                        ));
                    }
                }
                StructureTemplateParameterRecord::Structure { traits } => {
                    if traits.is_empty() {
                        return invalid_template(record.id(), "structure parameter has no traits");
                    }
                    if let Some(missing) = traits.iter().find(|id| !trait_index.contains_key(*id)) {
                        return Err(CatalogueError::new(
                            CatalogueErrorCode::UnknownReference,
                            format!(
                                "template `{}` trait `{missing}` does not resolve",
                                record.id()
                            ),
                        ));
                    }
                }
                StructureTemplateParameterRecord::Enum { values } => {
                    if values.is_empty()
                        || values
                            .iter()
                            .any(|value| value != value.trim() || !valid_declared_text_id(value))
                    {
                        return invalid_template(record.id(), "enum parameter values are invalid");
                    }
                }
            }
        }
        validate_template_shape(record, elements)?;
        let mut asserted_traits = BTreeSet::new();
        for assertion in record.traits() {
            if !asserted_traits.insert(assertion.trait_id.clone()) {
                return duplicate_id(&assertion.trait_id);
            }
            let Some(position) = trait_index.get(&assertion.trait_id) else {
                return Err(CatalogueError::new(
                    CatalogueErrorCode::UnknownReference,
                    format!(
                        "template `{}` trait `{}` does not resolve",
                        record.id(),
                        assertion.trait_id
                    ),
                ));
            };
            validate_trait_assertion_shape(assertion, &trait_definitions[*position], premises)?;
        }
    }
    Ok(index)
}

fn validate_application_arguments(
    application: &StructureTemplateApplicationRecord,
    template: &StructureTemplateRecord,
    elements: &[ElementRecord],
    element_index: &BTreeMap<ElementSymbol, usize>,
    category_members: &BTreeMap<ElementCategoryId, BTreeSet<ElementSymbol>>,
    membership_provenance: &BTreeMap<
        (ElementSymbol, ElementCategoryId),
        ElementMembershipProvenance,
    >,
) -> Result<(), CatalogueError> {
    if application.arguments.keys().collect::<BTreeSet<_>>()
        != template.parameters().keys().collect::<BTreeSet<_>>()
    {
        return invalid_application(&application.id, "arguments do not exactly match parameters");
    }
    for (name, parameter) in template.parameters() {
        let argument = &application.arguments[name];
        match parameter {
            StructureTemplateParameterRecord::Element { category } => {
                let symbol = ElementSymbol::new(argument).map_err(|error| {
                    CatalogueError::new(
                        CatalogueErrorCode::InvalidStructureApplication,
                        error.to_string(),
                    )
                })?;
                if !element_index.contains_key(&symbol) {
                    return Err(CatalogueError::new(
                        CatalogueErrorCode::UnknownReference,
                        format!(
                            "application `{}` element `{symbol}` does not resolve",
                            application.id
                        ),
                    ));
                }
                if !category_members[category].contains(&symbol)
                    || !membership_provenance.contains_key(&(symbol.clone(), category.clone()))
                {
                    return invalid_application(
                        &application.id,
                        format!("element argument `{name}` does not satisfy `{category}`"),
                    );
                }
                let _ = &elements[element_index[&symbol]];
            }
            StructureTemplateParameterRecord::Structure { .. } => {
                StructureId::from_str(argument).map_err(|error| {
                    CatalogueError::new(
                        CatalogueErrorCode::InvalidStructureApplication,
                        error.to_string(),
                    )
                })?;
            }
            StructureTemplateParameterRecord::Enum { values } => {
                if !values.contains(argument) {
                    return invalid_application(
                        &application.id,
                        format!("enum argument `{name}` is outside its closed values"),
                    );
                }
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn validate_template_shape(
    template: &StructureTemplateRecord,
    elements: &BTreeMap<ElementSymbol, usize>,
) -> Result<(), CatalogueError> {
    let mut used_parameters = BTreeSet::new();
    match template {
        StructureTemplateRecord::Molecular {
            atoms,
            bonds,
            groups,
            ..
        }
        | StructureTemplateRecord::Ion {
            atoms,
            bonds,
            groups,
            ..
        } => validate_template_graph_records(
            template,
            atoms,
            bonds,
            groups,
            elements,
            &mut used_parameters,
        )?,
        StructureTemplateRecord::Ionic {
            components,
            associations,
            ..
        } => {
            if components.len() < 2 || associations.is_empty() {
                return invalid_template(template.id(), "ionic graph is incomplete");
            }
            let mut component_labels = BTreeSet::new();
            for component in components {
                validate_label(
                    &component.label,
                    CatalogueErrorCode::InvalidStructureTemplate,
                )?;
                if !component_labels.insert(component.label.clone()) {
                    return duplicate_id(&component.label);
                }
                validate_template_graph_records(
                    template,
                    &component.atoms,
                    &component.bonds,
                    &component.groups,
                    elements,
                    &mut used_parameters,
                )?;
            }
            let mut association_labels = BTreeSet::new();
            for association in associations {
                validate_label(
                    &association.label,
                    CatalogueErrorCode::InvalidStructureTemplate,
                )?;
                if !association_labels.insert(association.label.clone())
                    || association.components.len() < 2
                    || association.components.iter().collect::<BTreeSet<_>>().len()
                        != association.components.len()
                    || association
                        .components
                        .iter()
                        .any(|component| !component_labels.contains(component))
                {
                    return invalid_template(
                        template.id(),
                        "ionic association references invalid components",
                    );
                }
            }
        }
        StructureTemplateRecord::Metallic { sites, domains, .. } => {
            if domains.is_empty() {
                return invalid_template(template.id(), "metallic graph has no domain");
            }
            validate_template_graph_records(
                template,
                sites,
                &[],
                &[],
                elements,
                &mut used_parameters,
            )?;
            let site_labels = sites
                .iter()
                .map(|site| site.label.as_str())
                .collect::<BTreeSet<_>>();
            let mut domain_labels = BTreeSet::new();
            for domain in domains {
                validate_label(&domain.label, CatalogueErrorCode::InvalidStructureTemplate)?;
                if !domain_labels.insert(domain.label.clone())
                    || domain.sites.is_empty()
                    || domain.sites.iter().collect::<BTreeSet<_>>().len() != domain.sites.len()
                    || domain.delocalized_electrons == 0
                    || domain
                        .sites
                        .iter()
                        .any(|site| !site_labels.contains(site.as_str()))
                {
                    return invalid_template(
                        template.id(),
                        "metallic domain references invalid sites",
                    );
                }
            }
        }
    }
    for (name, parameter) in template.parameters() {
        if matches!(
            parameter,
            StructureTemplateParameterRecord::Element { .. }
                | StructureTemplateParameterRecord::Enum { .. }
        ) && !used_parameters.contains(name)
        {
            return invalid_template(
                template.id(),
                format!("parameter `{name}` is never substituted"),
            );
        }
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn validate_template_graph_records(
    template: &StructureTemplateRecord,
    atoms: &[TemplateAtomRecord],
    bonds: &[TemplateBondRecord],
    groups: &[GroupRecord],
    elements: &BTreeMap<ElementSymbol, usize>,
    used_parameters: &mut BTreeSet<String>,
) -> Result<(), CatalogueError> {
    if atoms.is_empty() {
        return invalid_template(template.id(), "graph has no atoms");
    }
    let mut atom_labels = BTreeSet::new();
    for atom in atoms {
        validate_label(&atom.label, CatalogueErrorCode::InvalidStructureTemplate)?;
        if !atom_labels.insert(atom.label.clone()) {
            return duplicate_id(&atom.label);
        }
        ElectronState::new(
            atom.formal_charge,
            atom.non_bonding_electrons,
            atom.unpaired_electrons,
        )
        .map_err(|error| {
            CatalogueError::new(
                CatalogueErrorCode::InvalidStructureTemplate,
                format!(
                    "template `{}` atom `{}`: {error}",
                    template.id(),
                    atom.label
                ),
            )
        })?;
        match &atom.element {
            TemplateElementRecord::Literal(element) => {
                if !elements.contains_key(element) {
                    return Err(CatalogueError::new(
                        CatalogueErrorCode::UnknownReference,
                        format!(
                            "template `{}` literal element `{element}` does not resolve",
                            template.id()
                        ),
                    ));
                }
            }
            TemplateElementRecord::Parameter(reference) => {
                let Some(parameter) = template.parameters().get(&reference.parameter) else {
                    return invalid_template(
                        template.id(),
                        format!("unknown element parameter `{}`", reference.parameter),
                    );
                };
                if !matches!(parameter, StructureTemplateParameterRecord::Element { .. }) {
                    return invalid_template(
                        template.id(),
                        format!("parameter `{}` is not an element", reference.parameter),
                    );
                }
                used_parameters.insert(reference.parameter.clone());
            }
        }
    }
    let mut edges = BTreeSet::new();
    for bond in bonds {
        if bond.left == bond.right
            || !atom_labels.contains(&bond.left)
            || !atom_labels.contains(&bond.right)
        {
            return invalid_template(template.id(), "bond has invalid endpoints");
        }
        let edge = if bond.left < bond.right {
            (&bond.left, &bond.right)
        } else {
            (&bond.right, &bond.left)
        };
        if !edges.insert(edge) {
            return invalid_template(template.id(), "duplicate covalent edge");
        }
        let order_values = match &bond.order {
            TemplateBondOrderRecord::Literal(order) => vec![*order],
            TemplateBondOrderRecord::Parameter(reference) => {
                let Some(StructureTemplateParameterRecord::Enum { values }) =
                    template.parameters().get(&reference.parameter)
                else {
                    return invalid_template(
                        template.id(),
                        format!(
                            "bond order parameter `{}` is not an enum",
                            reference.parameter
                        ),
                    );
                };
                used_parameters.insert(reference.parameter.clone());
                values
                    .iter()
                    .map(|value| parse_bond_order(value, template.id()))
                    .collect::<Result<Vec<_>, _>>()?
            }
        };
        if let BondElectronOriginRecord::Dative { donor, acceptor } = &bond.electron_origin
            && (!((donor == &bond.left && acceptor == &bond.right)
                || (donor == &bond.right && acceptor == &bond.left))
                || order_values
                    .iter()
                    .any(|order| *order != BondOrderRecord::Single))
        {
            return invalid_template(
                template.id(),
                "dative bond must be single and name its endpoints",
            );
        }
    }
    let mut group_labels = BTreeSet::new();
    for group in groups {
        validate_label(&group.label, CatalogueErrorCode::InvalidStructureTemplate)?;
        if !group_labels.insert(group.label.clone())
            || group.atoms.is_empty()
            || group.atoms.iter().collect::<BTreeSet<_>>().len() != group.atoms.len()
            || group.atoms.iter().any(|atom| !atom_labels.contains(atom))
        {
            return invalid_template(template.id(), "group references invalid atoms");
        }
    }
    Ok(())
}

fn instantiate_template_record(
    template: &StructureTemplateRecord,
    application: &StructureTemplateApplicationRecord,
) -> Result<StructureRecord, CatalogueError> {
    let premise_id = application
        .premise_ids
        .first()
        .expect("application premise set validated")
        .clone();
    match template {
        StructureTemplateRecord::Molecular {
            atoms,
            bonds,
            groups,
            traits,
            ..
        } => Ok(StructureRecord::Molecular {
            id: application.id.clone(),
            premise_id,
            formula: application.formula.clone(),
            atoms: instantiate_template_atoms(atoms, application),
            bonds: instantiate_template_bonds(bonds, application, template)?,
            groups: groups.clone(),
            traits: traits.clone(),
        }),
        StructureTemplateRecord::Ion {
            atoms,
            bonds,
            groups,
            traits,
            ..
        } => Ok(StructureRecord::Ion {
            id: application.id.clone(),
            premise_id,
            formula: application.formula.clone(),
            atoms: instantiate_template_atoms(atoms, application),
            bonds: instantiate_template_bonds(bonds, application, template)?,
            groups: groups.clone(),
            traits: traits.clone(),
        }),
        StructureTemplateRecord::Ionic {
            components,
            associations,
            traits,
            ..
        } => Ok(StructureRecord::Ionic {
            id: application.id.clone(),
            premise_id,
            formula: application.formula.clone(),
            components: components
                .iter()
                .map(|component| {
                    Ok(ComponentRecord {
                        label: component.label.clone(),
                        atoms: instantiate_template_atoms(&component.atoms, application),
                        bonds: instantiate_template_bonds(&component.bonds, application, template)?,
                        groups: component.groups.clone(),
                    })
                })
                .collect::<Result<Vec<_>, CatalogueError>>()?,
            associations: associations.clone(),
            traits: traits.clone(),
        }),
        StructureTemplateRecord::Metallic {
            sites,
            domains,
            traits,
            ..
        } => Ok(StructureRecord::Metallic {
            id: application.id.clone(),
            premise_id,
            formula: application.formula.clone(),
            sites: instantiate_template_atoms(sites, application),
            domains: domains.clone(),
            traits: traits.clone(),
        }),
    }
}

fn instantiate_template_atoms(
    atoms: &[TemplateAtomRecord],
    application: &StructureTemplateApplicationRecord,
) -> Vec<AtomRecord> {
    atoms
        .iter()
        .map(|atom| AtomRecord {
            label: atom.label.clone(),
            element: match &atom.element {
                TemplateElementRecord::Literal(element) => element.to_string(),
                TemplateElementRecord::Parameter(reference) => {
                    application.arguments[&reference.parameter].clone()
                }
            },
            formal_charge: atom.formal_charge,
            non_bonding_electrons: atom.non_bonding_electrons,
            unpaired_electrons: atom.unpaired_electrons,
        })
        .collect()
}

fn instantiate_template_bonds(
    bonds: &[TemplateBondRecord],
    application: &StructureTemplateApplicationRecord,
    template: &StructureTemplateRecord,
) -> Result<Vec<BondRecord>, CatalogueError> {
    bonds
        .iter()
        .map(|bond| {
            let order = match &bond.order {
                TemplateBondOrderRecord::Literal(order) => *order,
                TemplateBondOrderRecord::Parameter(reference) => {
                    parse_bond_order(&application.arguments[&reference.parameter], template.id())?
                }
            };
            Ok(BondRecord {
                left: bond.left.clone(),
                right: bond.right.clone(),
                order,
                electron_origin: bond.electron_origin.clone(),
                delocalization: bond.delocalization.clone(),
            })
        })
        .collect()
}

fn parse_bond_order(
    value: &str,
    template: &StructureTemplateId,
) -> Result<BondOrderRecord, CatalogueError> {
    match value {
        "single" => Ok(BondOrderRecord::Single),
        "double" => Ok(BondOrderRecord::Double),
        "triple" => Ok(BondOrderRecord::Triple),
        _ => invalid_template(
            template,
            format!("`{value}` is not a bond-order enum value"),
        ),
    }
}

fn invalid_template<T>(
    id: &StructureTemplateId,
    message: impl Into<String>,
) -> Result<T, CatalogueError> {
    Err(CatalogueError::new(
        CatalogueErrorCode::InvalidStructureTemplate,
        format!("template `{id}`: {}", message.into()),
    ))
}

fn invalid_application<T>(
    id: &StructureId,
    message: impl Into<String>,
) -> Result<T, CatalogueError> {
    Err(CatalogueError::new(
        CatalogueErrorCode::InvalidStructureApplication,
        format!("application `{id}`: {}", message.into()),
    ))
}

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
        structure_premises.insert(
            record.id().clone(),
            [record.premise_id().clone()].into_iter().collect(),
        );
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
                    let electron_origin = match &bond.electron_origin {
                        BondElectronOriginRecord::Shared => BondElectronOriginRecord::Shared,
                        BondElectronOriginRecord::Dative { donor, acceptor } => {
                            BondElectronOriginRecord::Dative {
                                donor: format!("{}.{}", component.label, donor),
                                acceptor: format!("{}.{}", component.label, acceptor),
                            }
                        }
                    };
                    bonds.push(BondRecord {
                        left: format!("{}.{}", component.label, bond.left),
                        right: format!("{}.{}", component.label, bond.right),
                        order: bond.order,
                        electron_origin,
                        delocalization: bond.delocalization.clone(),
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

#[allow(clippy::too_many_lines)]
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
            let id = parse_id::<chem_domain::CovalentBondKind>(&format!("bond.{index}"))?;
            match &record.electron_origin {
                BondElectronOriginRecord::Shared => {
                    if let Some(delocalization) = &record.delocalization {
                        let domain = parse_id::<chem_domain::CovalentDelocalizationKind>(
                            &delocalization.domain,
                        )?;
                        let order = EffectiveBondOrder::new(
                            delocalization.effective_order.numerator,
                            delocalization.effective_order.denominator,
                        )
                        .map_err(structure_error)?;
                        CovalentBond::new_delocalized(
                            id,
                            left,
                            right,
                            record.order.into(),
                            CovalentDelocalization::new(domain, order),
                        )
                    } else {
                        CovalentBond::new(id, left, right, record.order.into())
                    }
                }
                BondElectronOriginRecord::Dative { donor, acceptor } => {
                    let donor = parse_id::<chem_domain::AtomKind>(donor)?;
                    let acceptor = parse_id::<chem_domain::AtomKind>(acceptor)?;
                    if record.delocalization.is_some()
                        || record.order != BondOrderRecord::Single
                        || !((donor == left && acceptor == right)
                            || (donor == right && acceptor == left))
                    {
                        return Err(CatalogueError::new(
                            CatalogueErrorCode::InvalidStructure,
                            "dative bond must be single and name its two edge endpoints",
                        ));
                    }
                    CovalentBond::new_dative(id, donor, acceptor)
                }
            }
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
    structure_premises: &BTreeMap<StructureId, BTreeSet<PremiseId>>,
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
            let missing_structure_premises = structure_premises[&term.structure_id]
                .difference(&record.premise_ids)
                .collect::<Vec<_>>();
            if !missing_structure_premises.is_empty() {
                return rule_error(
                    &record.id,
                    format!(
                        "structure `{}` premises {missing_structure_premises:?} are not proof-bound",
                        term.structure_id,
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
            OperationTemplateRecord::ReconfigureElectrons {
                atom,
                before,
                after,
                ..
            } => {
                validate_supported_state(atom, *before, reactants, valence_records)?;
                validate_supported_state(atom, *after, reactants, valence_records)?;
                if before == after || before.0 != after.0 || before.1 != after.1 {
                    return operation_error(
                        &rule.id,
                        "electron reconfiguration must only change unpaired-electron occupancy",
                    );
                }
            }
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
            OperationTemplateRecord::CleaveDative {
                donor,
                acceptor,
                allocation,
                before,
                after,
                ..
            } => {
                require_distinct_atoms(rule, reactants, donor, acceptor)?;
                validate_binary_states(before, after)?;
                require_initial_dative_bond(rule, donor, acceptor, structures)?;
                validate_supported_state(donor, before.left, reactants, valence_records)?;
                validate_supported_state(acceptor, before.right, reactants, valence_records)?;
                validate_supported_state(donor, after.left, reactants, valence_records)?;
                validate_supported_state(acceptor, after.right, reactants, valence_records)?;
                let Some((donor_local_delta, acceptor_local_delta)) =
                    allocation_local_deltas(allocation, donor, acceptor, -1)
                else {
                    return operation_error(&rule.id, "invalid dative cleavage allocation");
                };
                if !valid_covalent_endpoint(before.left, after.left, donor_local_delta, -1)
                    || !valid_covalent_endpoint(before.right, after.right, acceptor_local_delta, -1)
                {
                    return operation_error(&rule.id, "invalid dative-cleavage electron ledger");
                }
            }
            OperationTemplateRecord::FormDative {
                donor,
                acceptor,
                before,
                after,
                ..
            } => {
                require_distinct_atoms(rule, reactants, donor, acceptor)?;
                validate_binary_states(before, after)?;
                validate_supported_state(donor, before.left, reactants, valence_records)?;
                validate_supported_state(acceptor, before.right, reactants, valence_records)?;
                validate_supported_state(donor, after.left, reactants, valence_records)?;
                validate_supported_state(acceptor, after.right, reactants, valence_records)?;
                let donor_paired_electrons = before.left.1.saturating_sub(before.left.2);
                if donor_paired_electrons < 2
                    || before.left.2 != after.left.2
                    || before.right.2 != after.right.2
                    || !valid_covalent_endpoint(before.left, after.left, -2, 1)
                    || !valid_covalent_endpoint(before.right, after.right, 0, 1)
                {
                    return operation_error(
                        &rule.id,
                        "dative formation requires one donor lone pair and no acceptor contribution",
                    );
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
            OperationTemplateRecord::ChangeCovalentDelocalization {
                edge,
                expected,
                replacement,
                ..
            } => {
                require_distinct_atoms(rule, reactants, &edge.0, &edge.1)?;
                if expected == replacement {
                    return operation_error(&rule.id, "covalent delocalisation is unchanged");
                }
                for value in expected.iter().chain(replacement.iter()) {
                    chem_domain::CovalentDelocalizationId::from_str(&value.domain).map_err(
                        |error| {
                            CatalogueError::new(
                                CatalogueErrorCode::InvalidOperationTemplate,
                                error.to_string(),
                            )
                        },
                    )?;
                    EffectiveBondOrder::new(
                        value.effective_order.numerator,
                        value.effective_order.denominator,
                    )
                    .map_err(|error| {
                        CatalogueError::new(
                            CatalogueErrorCode::InvalidOperationTemplate,
                            error.to_string(),
                        )
                    })?;
                }
                let has_shared_edge = [
                    BondOrderRecord::Single,
                    BondOrderRecord::Double,
                    BondOrderRecord::Triple,
                ]
                .into_iter()
                .any(|order| {
                    require_initial_bond(rule, &edge.0, &edge.1, order, structures).is_ok()
                });
                if !has_shared_edge {
                    return operation_error(
                        &rule.id,
                        "delocalisation change requires an initial shared covalent edge",
                    );
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
            && bond.electron_origin().is_shared()
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

fn require_initial_dative_bond(
    rule: &ReactionRuleRecord,
    donor: &str,
    acceptor: &str,
    structures: &BTreeMap<StructureId, StructureDefinition>,
) -> Result<(), CatalogueError> {
    let (donor_instance, _) = split_template_reference(donor).ok_or_else(|| {
        CatalogueError::new(
            CatalogueErrorCode::InvalidOperationTemplate,
            "malformed dative donor",
        )
    })?;
    let (acceptor_instance, _) = split_template_reference(acceptor).ok_or_else(|| {
        CatalogueError::new(
            CatalogueErrorCode::InvalidOperationTemplate,
            "malformed dative acceptor",
        )
    })?;
    if donor_instance != acceptor_instance {
        return operation_error(&rule.id, "dative edge spans structure instances");
    }
    let donor_atom = resolve_template_atom(donor, &rule.reactant_pattern, structures)?;
    let acceptor_atom = resolve_template_atom(acceptor, &rule.reactant_pattern, structures)?;
    let (role_name, _) = parse_instance(donor_instance).ok_or_else(|| {
        CatalogueError::new(
            CatalogueErrorCode::InvalidOperationTemplate,
            "malformed dative bond instance",
        )
    })?;
    let structure = rule
        .reactant_pattern
        .iter()
        .find(|term| term.role == role_name)
        .map(|term| &structures[&term.structure_id])
        .expect("resolved atom role has a pattern term");
    let exists = structure.graph().covalent_bonds().values().any(|bond| {
        matches!(
            bond.electron_origin(),
            CovalentElectronOrigin::Dative {
                donor,
                acceptor
            } if donor == donor_atom.id() && acceptor == acceptor_atom.id()
        )
    });
    if exists {
        Ok(())
    } else {
        operation_error(
            &rule.id,
            "referenced directed dative edge is absent from the template",
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
    // The domain formula parser understands grouped units like Ca(OH)2.
    let composition = chem_domain::FormulaComposition::parse(source).map_err(|_| {
        CatalogueError::new(
            CatalogueErrorCode::InvalidStructure,
            format!("invalid formula summary `{source}`"),
        )
    })?;
    ElementInventory::new(
        composition
            .elements()
            .iter()
            .map(|(symbol, count)| (symbol.clone(), *count)),
    )
    .map_err(structure_error)
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
    document
        .elements
        .sort_by(|left, right| left.symbol.cmp(&right.symbol));
    for category in &mut document.element_categories {
        if let ElementCategoryMembershipRecord::Predicate { predicate, .. } =
            &mut category.membership
        {
            normalize_predicate(predicate);
        }
    }
    document
        .element_categories
        .sort_by(|left, right| left.id.cmp(&right.id));
    document
        .structural_traits
        .sort_by(|left, right| left.id.cmp(&right.id));
    for template in &mut document.structure_templates {
        normalize_structure_template(template);
    }
    document
        .structure_templates
        .sort_by(|left, right| left.id().cmp(right.id()));
    document
        .structure_applications
        .sort_by(|left, right| left.id.cmp(&right.id));
    for pattern in &mut document.graph_patterns {
        pattern.relationships.sort_by_key(|relationship| {
            serde_json::to_string(relationship).expect("graph-pattern relationship serializes")
        });
        pattern.traits.sort();
    }
    document
        .graph_patterns
        .sort_by(|left, right| left.id.cmp(&right.id));
    normalize_generalized_rules(&mut document.generalized_rules);
    document.macroscopic_materials.sort_by(|left, right| {
        (&left.structure, &left.context).cmp(&(&right.structure, &right.context))
    });
}

fn normalize_generalized_rules(rules: &mut [GeneralizedReactionRuleRecord]) {
    for rule in rules.iter_mut() {
        for case in &mut rule.cases {
            normalize_generalized_predicate(case.when_mut());
            if let GeneralizedReactionCaseRecord::Supported {
                correspondence,
                rewrite,
                observation_compatibility,
                ..
            } = case
            {
                correspondence.sort_by(|left, right| {
                    (&left.reactant, &left.product).cmp(&(&right.reactant, &right.product))
                });
                for operation in rewrite {
                    normalize_operation(operation);
                }
                observation_compatibility.sort_by(|left, right| {
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
        }
        rule.cases.sort_by(|left, right| left.id().cmp(right.id()));
    }
    rules.sort_by(|left, right| left.id.cmp(&right.id));
}

fn normalize_generalized_predicate(predicate: &mut GeneralizedCasePredicateRecord) {
    match predicate {
        GeneralizedCasePredicateRecord::All { predicates }
        | GeneralizedCasePredicateRecord::Any { predicates } => {
            for child in predicates.iter_mut() {
                normalize_generalized_predicate(child);
            }
            predicates.sort_by_key(|child| {
                serde_json::to_string(child).expect("generalized predicate serializes")
            });
        }
        GeneralizedCasePredicateRecord::Not { predicate } => {
            normalize_generalized_predicate(predicate);
        }
        GeneralizedCasePredicateRecord::Always
        | GeneralizedCasePredicateRecord::ParameterEquals { .. }
        | GeneralizedCasePredicateRecord::ParameterInSet { .. } => {}
    }
}

fn normalize_predicate(predicate: &mut ElementPredicateRecord) {
    match predicate {
        ElementPredicateRecord::All { predicates } | ElementPredicateRecord::Any { predicates } => {
            for child in predicates.iter_mut() {
                normalize_predicate(child);
            }
            predicates.sort_by_key(predicate_key);
        }
        ElementPredicateRecord::Not { predicate } => normalize_predicate(predicate),
        ElementPredicateRecord::InSet { values, .. } => values.sort_by_key(scalar_key),
        _ => {}
    }
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
    normalize_trait_assertions(record.traits_mut());
}

fn normalize_trait_assertions(assertions: &mut [StructuralTraitAssertionRecord]) {
    assertions.sort_by(|left, right| left.trait_id.cmp(&right.trait_id));
}

fn normalize_structure_template(template: &mut StructureTemplateRecord) {
    match template {
        StructureTemplateRecord::Molecular {
            atoms,
            bonds,
            groups,
            traits,
            ..
        }
        | StructureTemplateRecord::Ion {
            atoms,
            bonds,
            groups,
            traits,
            ..
        } => {
            normalize_template_graph_records(atoms, bonds, groups);
            normalize_trait_assertions(traits);
        }
        StructureTemplateRecord::Ionic {
            components,
            associations,
            traits,
            ..
        } => {
            for component in components.iter_mut() {
                normalize_template_graph_records(
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
            normalize_trait_assertions(traits);
        }
        StructureTemplateRecord::Metallic {
            sites,
            domains,
            traits,
            ..
        } => {
            sites.sort_by(|left, right| left.label.cmp(&right.label));
            for domain in domains.iter_mut() {
                domain.sites.sort();
            }
            domains.sort_by(|left, right| left.label.cmp(&right.label));
            normalize_trait_assertions(traits);
        }
    }
}

fn normalize_template_graph_records(
    atoms: &mut [TemplateAtomRecord],
    bonds: &mut [TemplateBondRecord],
    groups: &mut [GroupRecord],
) {
    atoms.sort_by(|left, right| left.label.cmp(&right.label));
    for bond in bonds.iter_mut() {
        if bond.right < bond.left {
            std::mem::swap(&mut bond.left, &mut bond.right);
        }
    }
    bonds.sort_by_key(|bond| serde_json::to_string(bond).expect("template bond serializes"));
    for group in groups.iter_mut() {
        group.atoms.sort();
    }
    groups.sort_by(|left, right| left.label.cmp(&right.label));
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
