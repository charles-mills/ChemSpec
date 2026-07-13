//! Immutable, content-addressed chemistry catalogue bundles.

mod model;

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use chem_domain::{
    AssumptionKindId, ContentDigest, CoverageId, Dimension, Element, ElementId, ElementRegistry,
    ElementSymbol, EvidenceSourceId, ExactScalar, FactId, FormulaPart, FormulaSegment,
    FormulaSyntax, MediumId, NormalizedFormula, Phase, SpeciesId, StaticElementRegistry,
    SubstanceId,
};
pub use model::*;
use num_bigint::{BigInt, BigUint};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogueErrorCode {
    InvalidJson,
    UnsupportedSchema,
    DigestMismatch,
    InvalidMetadata,
    DuplicateId,
    DuplicateAlias,
    UnknownReference,
    InvalidFormula,
    InconsistentSpecies,
    InvalidCondition,
    InvalidDimension,
    MissingEvidence,
    InvalidReview,
    ContradictoryFacts,
    InvalidCoverage,
    UnconservedReaction,
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
            Self::DuplicateAlias => "CHEMS-C006",
            Self::UnknownReference => "CHEMS-C007",
            Self::InvalidFormula => "CHEMS-C008",
            Self::InconsistentSpecies => "CHEMS-C009",
            Self::InvalidCondition => "CHEMS-C010",
            Self::InvalidDimension => "CHEMS-C011",
            Self::MissingEvidence => "CHEMS-C012",
            Self::InvalidReview => "CHEMS-C013",
            Self::ContradictoryFacts => "CHEMS-C014",
            Self::InvalidCoverage => "CHEMS-C015",
            Self::UnconservedReaction => "CHEMS-C016",
            Self::IneligibleProductionRecord => "CHEMS-C017",
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

#[derive(Debug, Clone)]
struct CatalogueIndexes {
    elements_by_symbol: BTreeMap<ElementSymbol, usize>,
    substances_by_id: BTreeMap<SubstanceId, usize>,
    substances_by_alias: BTreeMap<String, usize>,
    species_by_id: BTreeMap<SpeciesId, usize>,
    media_by_id: BTreeMap<MediumId, usize>,
    media_by_alias: BTreeMap<String, usize>,
    facts_by_id: BTreeMap<FactId, usize>,
    evidence_by_id: BTreeMap<EvidenceSourceId, usize>,
    assumptions_by_id: BTreeMap<AssumptionKindId, usize>,
    coverage_by_id: BTreeMap<CoverageId, usize>,
    provenance_by_id: BTreeMap<FactId, ProvenanceLocation>,
    facts_by_species: BTreeMap<SpeciesId, Vec<usize>>,
}

#[derive(Debug, Clone, Copy)]
enum ProvenanceLocation {
    Element(usize),
    Substance(usize),
    Species(usize),
    Medium(usize),
    Fact(usize),
}

/// A catalogue premise carrying stable fact identity, evidence, and review
/// metadata. Kernel derivations may bind to any variant through its `FactId`.
#[derive(Debug, Clone, Copy)]
pub enum CataloguePremiseRef<'a> {
    Element(&'a ElementRecord),
    Substance(&'a SubstanceRecord),
    Species(&'a SpeciesRecord),
    Medium(&'a MediumRecord),
    Fact(&'a FactRecord),
}

impl CataloguePremiseRef<'_> {
    #[must_use]
    pub fn id(&self) -> &FactId {
        match self {
            Self::Element(record) => &record.provenance.id,
            Self::Substance(record) => &record.provenance.id,
            Self::Species(record) => &record.provenance.id,
            Self::Medium(record) => &record.provenance.id,
            Self::Fact(record) => &record.id,
        }
    }

    #[must_use]
    pub fn evidence(&self) -> &BTreeSet<EvidenceSourceId> {
        match self {
            Self::Element(record) => &record.provenance.evidence,
            Self::Substance(record) => &record.provenance.evidence,
            Self::Species(record) => &record.provenance.evidence,
            Self::Medium(record) => &record.provenance.evidence,
            Self::Fact(record) => &record.evidence,
        }
    }

    #[must_use]
    pub const fn review(&self) -> &ReviewMetadata {
        match self {
            Self::Element(record) => &record.provenance.review,
            Self::Substance(record) => &record.provenance.review,
            Self::Species(record) => &record.provenance.review,
            Self::Medium(record) => &record.provenance.review,
            Self::Fact(record) => &record.review,
        }
    }

    #[must_use]
    pub fn rule_version(&self) -> &str {
        match self {
            Self::Element(record) => &record.provenance.rule_version,
            Self::Substance(record) => &record.provenance.rule_version,
            Self::Species(record) => &record.provenance.rule_version,
            Self::Medium(record) => &record.provenance.rule_version,
            Self::Fact(record) => &record.rule_version,
        }
    }
}

/// A fully checked catalogue. Construction is private to validation.
#[derive(Debug, Clone)]
pub struct ValidatedCatalogue {
    document: CatalogueDocument,
    digest: ContentDigest,
    indexes: CatalogueIndexes,
}

impl CatalogueEnvelope {
    /// Computes the content digest over the canonical JSON representation of
    /// the nested bundle document.
    ///
    /// # Errors
    ///
    /// Returns a system error if the document cannot be represented as
    /// canonical chemistry JSON.
    pub fn computed_digest(&self) -> Result<ContentDigest, CatalogueError> {
        digest_document(&self.bundle)
    }

    /// Validates this envelope and builds deterministic lookup indexes.
    ///
    /// # Errors
    ///
    /// Returns a catalogue system error for any corrupt or inconsistent
    /// record, including a digest mismatch.
    pub fn validate(self) -> Result<ValidatedCatalogue, CatalogueError> {
        validate_envelope(self)
    }
}

impl CatalogueDocument {
    /// Serializes the semantic bundle in canonical record order with sorted
    /// object keys and no insignificant whitespace.
    ///
    /// # Errors
    ///
    /// Returns a system error if serialization or canonicalization fails.
    pub fn canonical_json(&self) -> Result<Vec<u8>, CatalogueError> {
        let canonical = canonical_document(self);
        let value = serde_json::to_value(canonical).map_err(|error| {
            CatalogueError::new(
                CatalogueErrorCode::InvalidJson,
                format!("could not serialize catalogue document: {error}"),
            )
        })?;
        chem_domain::canonical_json(&value).map_err(|error| {
            CatalogueError::new(
                CatalogueErrorCode::InvalidJson,
                format!("could not canonicalize catalogue document: {error}"),
            )
        })
    }
}

impl ValidatedCatalogue {
    /// Loads a digest-bearing catalogue envelope from JSON bytes.
    ///
    /// # Errors
    ///
    /// Returns a catalogue system error for invalid JSON or failed catalogue
    /// validation.
    pub fn from_json(bytes: &[u8]) -> Result<Self, CatalogueError> {
        let envelope: CatalogueEnvelope = serde_json::from_slice(bytes).map_err(|error| {
            CatalogueError::new(
                CatalogueErrorCode::InvalidJson,
                format!("invalid catalogue JSON: {error}"),
            )
        })?;
        envelope.validate()
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
    pub fn element(&self, symbol: &ElementSymbol) -> Option<&ElementRecord> {
        self.indexes
            .elements_by_symbol
            .get(symbol)
            .map(|index| &self.document.elements[*index])
    }

    #[must_use]
    pub fn substance(&self, id: &SubstanceId) -> Option<&SubstanceRecord> {
        self.indexes
            .substances_by_id
            .get(id)
            .map(|index| &self.document.substances[*index])
    }

    #[must_use]
    pub fn substance_by_alias(&self, alias: &str) -> Option<&SubstanceRecord> {
        self.indexes
            .substances_by_alias
            .get(&alias_key(alias))
            .map(|index| &self.document.substances[*index])
    }

    #[must_use]
    pub fn species(&self, id: &SpeciesId) -> Option<&SpeciesRecord> {
        self.indexes
            .species_by_id
            .get(id)
            .map(|index| &self.document.species[*index])
    }

    #[must_use]
    pub fn medium(&self, id: &MediumId) -> Option<&MediumRecord> {
        self.indexes
            .media_by_id
            .get(id)
            .map(|index| &self.document.media[*index])
    }

    #[must_use]
    pub fn medium_by_alias(&self, alias: &str) -> Option<&MediumRecord> {
        self.indexes
            .media_by_alias
            .get(&alias_key(alias))
            .map(|index| &self.document.media[*index])
    }

    #[must_use]
    pub fn fact(&self, id: &FactId) -> Option<&FactRecord> {
        self.indexes
            .facts_by_id
            .get(id)
            .map(|index| &self.document.facts[*index])
    }

    /// Resolves any evidence-bearing identity or empirical premise by the
    /// stable `FactId` used in derivations.
    #[must_use]
    pub fn premise(&self, id: &FactId) -> Option<CataloguePremiseRef<'_>> {
        match self.indexes.provenance_by_id.get(id)? {
            ProvenanceLocation::Element(index) => Some(CataloguePremiseRef::Element(
                &self.document.elements[*index],
            )),
            ProvenanceLocation::Substance(index) => Some(CataloguePremiseRef::Substance(
                &self.document.substances[*index],
            )),
            ProvenanceLocation::Species(index) => {
                Some(CataloguePremiseRef::Species(&self.document.species[*index]))
            }
            ProvenanceLocation::Medium(index) => {
                Some(CataloguePremiseRef::Medium(&self.document.media[*index]))
            }
            ProvenanceLocation::Fact(index) => {
                Some(CataloguePremiseRef::Fact(&self.document.facts[*index]))
            }
        }
    }

    #[must_use]
    pub fn evidence(&self, id: &EvidenceSourceId) -> Option<&EvidenceSource> {
        self.indexes
            .evidence_by_id
            .get(id)
            .map(|index| &self.document.evidence[*index])
    }

    #[must_use]
    pub fn assumption_kind(&self, id: &AssumptionKindId) -> Option<&AssumptionKindRecord> {
        self.indexes
            .assumptions_by_id
            .get(id)
            .map(|index| &self.document.assumption_kinds[*index])
    }

    #[must_use]
    pub fn coverage(&self, id: &CoverageId) -> Option<&CoverageDeclaration> {
        self.indexes
            .coverage_by_id
            .get(id)
            .map(|index| &self.document.coverage[*index])
    }

    #[must_use]
    pub fn facts_for_species(&self, id: &SpeciesId) -> Vec<&FactRecord> {
        self.indexes
            .facts_by_species
            .get(id)
            .into_iter()
            .flatten()
            .map(|index| &self.document.facts[*index])
            .collect()
    }

    #[must_use]
    pub fn applicable_facts_for_species(
        &self,
        id: &SpeciesId,
        condition: &ConditionPoint,
    ) -> Vec<&FactRecord> {
        let Some(species) = self.species(id) else {
            return Vec::new();
        };
        if condition.phase.is_some_and(|phase| phase != species.phase) {
            return Vec::new();
        }
        let mut effective_condition = condition.clone();
        effective_condition.phase = Some(species.phase);
        self.facts_for_species(id)
            .into_iter()
            .filter(|fact| {
                fact.review.status == ReviewStatus::Reviewed
                    && fact.condition.contains(&effective_condition)
            })
            .collect()
    }
}

impl ElementRegistry for ValidatedCatalogue {
    fn resolve(&self, symbol: &ElementSymbol) -> Option<&Element> {
        self.element(symbol).map(|record| &record.element)
    }
}

impl ConditionDomain {
    #[must_use]
    pub fn contains(&self, point: &ConditionPoint) -> bool {
        self.temperature_kelvin
            .as_ref()
            .is_none_or(|range| range.contains(&point.temperature_kelvin))
            && self
                .pressure_pascal
                .as_ref()
                .is_none_or(|range| range.contains(&point.pressure_pascal))
            && self
                .media
                .as_ref()
                .is_none_or(|media| media.contains(&point.medium))
            && self
                .phases
                .as_ref()
                .is_none_or(|phases| point.phase.is_some_and(|phase| phases.contains(&phase)))
    }

    #[must_use]
    pub fn overlaps(&self, other: &Self) -> bool {
        optional_ranges_overlap(
            self.temperature_kelvin.as_ref(),
            other.temperature_kelvin.as_ref(),
        ) && optional_ranges_overlap(
            self.pressure_pascal.as_ref(),
            other.pressure_pascal.as_ref(),
        ) && optional_sets_overlap(self.media.as_ref(), other.media.as_ref())
            && optional_sets_overlap(self.phases.as_ref(), other.phases.as_ref())
    }

    #[must_use]
    pub fn is_subset_of(&self, other: &Self) -> bool {
        optional_range_is_subset(
            self.temperature_kelvin.as_ref(),
            other.temperature_kelvin.as_ref(),
        ) && optional_range_is_subset(
            self.pressure_pascal.as_ref(),
            other.pressure_pascal.as_ref(),
        ) && optional_set_is_subset(self.media.as_ref(), other.media.as_ref())
            && optional_set_is_subset(self.phases.as_ref(), other.phases.as_ref())
    }
}

fn optional_range_is_subset(child: Option<&ExactRange>, parent: Option<&ExactRange>) -> bool {
    match (child, parent) {
        (_, None) => true,
        (None, Some(_)) => false,
        (Some(child), Some(parent)) => {
            let minimum_inside = child.minimum > parent.minimum
                || child.minimum == parent.minimum
                    && (parent.minimum_bound == BoundaryKind::Inclusive
                        || child.minimum_bound == BoundaryKind::Exclusive);
            let maximum_inside = child.maximum < parent.maximum
                || child.maximum == parent.maximum
                    && (parent.maximum_bound == BoundaryKind::Inclusive
                        || child.maximum_bound == BoundaryKind::Exclusive);
            minimum_inside && maximum_inside
        }
    }
}

fn optional_set_is_subset<T: Ord>(
    child: Option<&BTreeSet<T>>,
    parent: Option<&BTreeSet<T>>,
) -> bool {
    match (child, parent) {
        (_, None) => true,
        (None, Some(_)) => false,
        (Some(child), Some(parent)) => child.is_subset(parent),
    }
}

impl ExactRange {
    #[must_use]
    pub fn contains(&self, value: &ExactScalar) -> bool {
        let above_minimum = match self.minimum_bound {
            BoundaryKind::Inclusive => value >= &self.minimum,
            BoundaryKind::Exclusive => value > &self.minimum,
        };
        let below_maximum = match self.maximum_bound {
            BoundaryKind::Inclusive => value <= &self.maximum,
            BoundaryKind::Exclusive => value < &self.maximum,
        };
        above_minimum && below_maximum
    }

    #[must_use]
    pub fn overlaps(&self, other: &Self) -> bool {
        if self.maximum < other.minimum || other.maximum < self.minimum {
            return false;
        }
        if self.maximum == other.minimum {
            return self.maximum_bound == BoundaryKind::Inclusive
                && other.minimum_bound == BoundaryKind::Inclusive;
        }
        if other.maximum == self.minimum {
            return other.maximum_bound == BoundaryKind::Inclusive
                && self.minimum_bound == BoundaryKind::Inclusive;
        }
        true
    }
}

fn optional_ranges_overlap(left: Option<&ExactRange>, right: Option<&ExactRange>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => left.overlaps(right),
        _ => true,
    }
}

fn optional_sets_overlap<T: Ord>(left: Option<&BTreeSet<T>>, right: Option<&BTreeSet<T>>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => left.iter().any(|value| right.contains(value)),
        _ => true,
    }
}

fn digest_document(document: &CatalogueDocument) -> Result<ContentDigest, CatalogueError> {
    Ok(ContentDigest::sha256(&document.canonical_json()?))
}

fn canonical_document(document: &CatalogueDocument) -> CatalogueDocument {
    let mut canonical = document.clone();
    let element_ids = canonical
        .elements
        .iter()
        .map(|record| (record.element.symbol.clone(), record.element.id))
        .collect::<BTreeMap<_, _>>();
    canonical.elements.sort_by_key(|record| record.element.id);
    for element in &mut canonical.elements {
        sort_aliases(&mut element.aliases);
        sort_reviewers(&mut element.provenance.review.reviewers);
    }
    canonical
        .substances
        .sort_by(|left, right| left.id.cmp(&right.id));
    for substance in &mut canonical.substances {
        sort_aliases(&mut substance.aliases);
        sort_formula(&mut substance.formula, &element_ids);
        sort_reviewers(&mut substance.provenance.review.reviewers);
    }
    canonical
        .species
        .sort_by(|left, right| left.id.cmp(&right.id));
    for species in &mut canonical.species {
        sort_formula(&mut species.formula, &element_ids);
        sort_reviewers(&mut species.provenance.review.reviewers);
    }
    canonical
        .media
        .sort_by(|left, right| left.id.cmp(&right.id));
    for medium in &mut canonical.media {
        sort_aliases(&mut medium.aliases);
        sort_reviewers(&mut medium.provenance.review.reviewers);
    }
    canonical
        .facts
        .sort_by(|left, right| left.id.cmp(&right.id));
    for fact in &mut canonical.facts {
        sort_reviewers(&mut fact.review.reviewers);
        match &mut fact.proposition {
            FactProposition::Dissociates { products, .. } => {
                products.sort_by(|left, right| left.species.cmp(&right.species));
            }
            FactProposition::SupportsGasPattern {
                reactants,
                products,
            } => {
                reactants.sort_by(|left, right| left.species.cmp(&right.species));
                products.sort_by(|left, right| left.species.cmp(&right.species));
            }
            FactProposition::HasAtomicMass { .. }
            | FactProposition::Soluble { .. }
            | FactProposition::Insoluble { .. }
            | FactProposition::HasDensity { .. }
            | FactProposition::SupportsGasModel { .. }
            | FactProposition::HasColour { .. }
            | FactProposition::HasPhase { .. } => {}
        }
    }
    canonical
        .assumption_kinds
        .sort_by(|left, right| left.id.cmp(&right.id));
    for assumption in &mut canonical.assumption_kinds {
        sort_reviewers(&mut assumption.review.reviewers);
    }
    canonical
        .coverage
        .sort_by(|left, right| left.id.cmp(&right.id));
    for coverage in &mut canonical.coverage {
        coverage.exclusions.sort_by(|left, right| {
            left.species
                .cmp(&right.species)
                .then_with(|| left.families.cmp(&right.families))
                .then_with(|| left.reason.cmp(&right.reason))
        });
        sort_reviewers(&mut coverage.review.reviewers);
    }
    canonical
        .evidence
        .sort_by(|left, right| left.id.cmp(&right.id));
    canonical
}

fn sort_aliases(aliases: &mut [String]) {
    aliases.sort_by_key(|alias| alias_key(alias));
}

fn sort_formula(formula: &mut MolecularFormula, ids: &BTreeMap<ElementSymbol, ElementId>) {
    formula.elements.sort_by(|left, right| {
        ids.get(&left.symbol)
            .cmp(&ids.get(&right.symbol))
            .then_with(|| left.symbol.cmp(&right.symbol))
    });
}

fn sort_reviewers(reviewers: &mut [ReviewerRecord]) {
    reviewers.sort_by(|left, right| {
        left.reviewer
            .cmp(&right.reviewer)
            .then_with(|| left.reviewed_on.cmp(&right.reviewed_on))
            .then_with(|| left.reference.cmp(&right.reference))
            .then_with(|| left.notes.cmp(&right.notes))
    });
}

fn validate_envelope(envelope: CatalogueEnvelope) -> Result<ValidatedCatalogue, CatalogueError> {
    if envelope.bundle.schema_version != CATALOGUE_SCHEMA_VERSION {
        return Err(CatalogueError::new(
            CatalogueErrorCode::UnsupportedSchema,
            format!(
                "unsupported catalogue schema version {}",
                envelope.bundle.schema_version
            ),
        ));
    }
    let computed = digest_document(&envelope.bundle)?;
    if envelope.digest != computed {
        return Err(CatalogueError::new(
            CatalogueErrorCode::DigestMismatch,
            format!(
                "declared digest {} does not match canonical bundle digest {computed}",
                envelope.digest
            ),
        ));
    }
    validate_metadata(&envelope.bundle)?;
    let mut indexes = build_and_validate_indexes(&envelope.bundle)?;
    validate_records(&envelope.bundle, &indexes)?;
    validate_conflicts(&envelope.bundle, &indexes)?;
    for (index, fact) in envelope.bundle.facts.iter().enumerate() {
        for species in fact_species(&fact.proposition) {
            indexes
                .facts_by_species
                .entry(species)
                .or_default()
                .push(index);
        }
    }
    Ok(ValidatedCatalogue {
        document: envelope.bundle,
        digest: computed,
        indexes,
    })
}

fn validate_metadata(document: &CatalogueDocument) -> Result<(), CatalogueError> {
    if !valid_catalogue_name(&document.name) {
        return Err(CatalogueError::new(
            CatalogueErrorCode::InvalidMetadata,
            format!("invalid catalogue public name `{}`", document.name),
        ));
    }
    if !valid_semantic_version(&document.version) {
        return Err(CatalogueError::new(
            CatalogueErrorCode::InvalidMetadata,
            format!("invalid catalogue semantic version `{}`", document.version),
        ));
    }
    require_nonempty("catalogue creator", &document.created.created_by)?;
    validate_date("catalogue creation date", &document.created.created_on)?;
    Ok(())
}

fn build_and_validate_indexes(
    document: &CatalogueDocument,
) -> Result<CatalogueIndexes, CatalogueError> {
    let mut elements_by_symbol = BTreeMap::new();
    let mut element_ids = BTreeSet::new();
    for (index, record) in document.elements.iter().enumerate() {
        if elements_by_symbol
            .insert(record.element.symbol.clone(), index)
            .is_some()
        {
            return duplicate_id("element symbol", &record.element.symbol.to_string());
        }
        if !element_ids.insert(record.element.id) {
            return duplicate_id(
                "atomic number",
                &record.element.id.atomic_number().to_string(),
            );
        }
        require_nonempty("element name", &record.name)?;
    }

    let substances_by_id = unique_index(&document.substances, "substance", |record| {
        record.id.clone()
    })?;
    let species_by_id = unique_index(&document.species, "species", |record| record.id.clone())?;
    let media_by_id = unique_index(&document.media, "medium", |record| record.id.clone())?;
    let facts_by_id = unique_index(&document.facts, "fact", |record| record.id.clone())?;
    let evidence_by_id = unique_index(&document.evidence, "evidence", |record| record.id.clone())?;
    let assumptions_by_id =
        unique_index(&document.assumption_kinds, "assumption kind", |record| {
            record.id.clone()
        })?;
    let coverage_by_id = unique_index(&document.coverage, "coverage", |record| record.id.clone())?;
    let provenance_by_id = build_provenance_index(document)?;

    let substances_by_alias = alias_index(
        document
            .substances
            .iter()
            .enumerate()
            .map(|(index, record)| (index, &record.name, &record.aliases)),
        "substance",
    )?;
    let media_by_alias = alias_index(
        document
            .media
            .iter()
            .enumerate()
            .map(|(index, record)| (index, &record.name, &record.aliases)),
        "medium",
    )?;
    let _element_aliases = alias_index(
        document
            .elements
            .iter()
            .enumerate()
            .map(|(index, record)| (index, &record.name, &record.aliases)),
        "element",
    )?;

    Ok(CatalogueIndexes {
        elements_by_symbol,
        substances_by_id,
        substances_by_alias,
        species_by_id,
        media_by_id,
        media_by_alias,
        facts_by_id,
        evidence_by_id,
        assumptions_by_id,
        coverage_by_id,
        provenance_by_id,
        facts_by_species: BTreeMap::new(),
    })
}

fn build_provenance_index(
    document: &CatalogueDocument,
) -> Result<BTreeMap<FactId, ProvenanceLocation>, CatalogueError> {
    let mut provenance_by_id = BTreeMap::new();
    for (index, record) in document.elements.iter().enumerate() {
        insert_provenance(
            &mut provenance_by_id,
            &record.provenance.id,
            ProvenanceLocation::Element(index),
        )?;
    }
    for (index, record) in document.substances.iter().enumerate() {
        insert_provenance(
            &mut provenance_by_id,
            &record.provenance.id,
            ProvenanceLocation::Substance(index),
        )?;
    }
    for (index, record) in document.species.iter().enumerate() {
        insert_provenance(
            &mut provenance_by_id,
            &record.provenance.id,
            ProvenanceLocation::Species(index),
        )?;
    }
    for (index, record) in document.media.iter().enumerate() {
        insert_provenance(
            &mut provenance_by_id,
            &record.provenance.id,
            ProvenanceLocation::Medium(index),
        )?;
    }
    for (index, record) in document.facts.iter().enumerate() {
        insert_provenance(
            &mut provenance_by_id,
            &record.id,
            ProvenanceLocation::Fact(index),
        )?;
    }

    Ok(provenance_by_id)
}

fn insert_provenance(
    index: &mut BTreeMap<FactId, ProvenanceLocation>,
    id: &FactId,
    location: ProvenanceLocation,
) -> Result<(), CatalogueError> {
    if index.insert(id.clone(), location).is_some() {
        duplicate_id("catalogue premise", &id.to_string())
    } else {
        Ok(())
    }
}

fn unique_index<T, K: Ord + fmt::Display>(
    values: &[T],
    kind: &str,
    key: impl Fn(&T) -> K,
) -> Result<BTreeMap<K, usize>, CatalogueError> {
    let mut index = BTreeMap::new();
    for (position, value) in values.iter().enumerate() {
        let id = key(value);
        if index.insert(id, position).is_some() {
            let duplicate = key(value);
            return duplicate_id(kind, &duplicate.to_string());
        }
    }
    Ok(index)
}

fn alias_index<'a>(
    records: impl IntoIterator<Item = (usize, &'a String, &'a Vec<String>)>,
    namespace: &str,
) -> Result<BTreeMap<String, usize>, CatalogueError> {
    let mut result = BTreeMap::new();
    for (index, name, aliases) in records {
        for alias in std::iter::once(name).chain(aliases) {
            require_nonempty(&format!("{namespace} alias"), alias)?;
            let key = alias_key(alias);
            if result.insert(key, index).is_some() {
                return Err(CatalogueError::new(
                    CatalogueErrorCode::DuplicateAlias,
                    format!("duplicate {namespace} alias `{alias}`"),
                ));
            }
        }
    }
    Ok(result)
}

fn validate_records(
    document: &CatalogueDocument,
    indexes: &CatalogueIndexes,
) -> Result<(), CatalogueError> {
    let registry = StaticElementRegistry::new(
        document
            .elements
            .iter()
            .map(|record| record.element.clone()),
    )
    .map_err(|error| CatalogueError::new(CatalogueErrorCode::InvalidFormula, error.to_string()))?;

    for source in &document.evidence {
        validate_evidence(source)?;
    }
    for record in &document.elements {
        validate_record_provenance(&record.provenance, document, indexes)?;
    }
    for record in &document.substances {
        require_nonempty("substance name", &record.name)?;
        normalize_formula(&record.formula, &registry, &record.id.to_string())?;
        validate_record_provenance(&record.provenance, document, indexes)?;
    }
    for record in &document.media {
        require_nonempty("medium name", &record.name)?;
        if !indexes.substances_by_id.contains_key(&record.solvent) {
            return unknown_reference("medium solvent substance", &record.solvent);
        }
        if record.supported_phases.is_empty() {
            return Err(CatalogueError::new(
                CatalogueErrorCode::InvalidCondition,
                format!("medium `{}` supports no phases", record.id),
            ));
        }
        validate_record_provenance(&record.provenance, document, indexes)?;
    }
    for record in &document.species {
        validate_species(record, document, indexes, &registry)?;
        validate_record_provenance(&record.provenance, document, indexes)?;
    }
    validate_species_ambiguity(document, &registry)?;

    for fact in &document.facts {
        validate_fact(fact, document, indexes, &registry)?;
    }
    for assumption in &document.assumption_kinds {
        validate_assumption(assumption, document, indexes)?;
    }
    for coverage in &document.coverage {
        validate_coverage(coverage, document, indexes)?;
    }

    Ok(())
}

fn validate_record_provenance(
    provenance: &RecordProvenance,
    document: &CatalogueDocument,
    indexes: &CatalogueIndexes,
) -> Result<(), CatalogueError> {
    require_nonempty("identity rule version", &provenance.rule_version)?;
    validate_reviewed_record(
        &provenance.id.to_string(),
        &provenance.review,
        &provenance.evidence,
        document.publication,
        indexes,
    )
}

fn validate_species(
    record: &SpeciesRecord,
    document: &CatalogueDocument,
    indexes: &CatalogueIndexes,
    registry: &StaticElementRegistry,
) -> Result<(), CatalogueError> {
    let Some(substance_index) = indexes.substances_by_id.get(&record.substance) else {
        return unknown_reference("species substance", &record.substance);
    };
    validate_condition(&record.condition, indexes)?;
    let species_formula = normalize_formula(&record.formula, registry, &record.id.to_string())?;
    let substance = &document.substances[*substance_index];
    let substance_formula =
        normalize_formula(&substance.formula, registry, &substance.id.to_string())?;
    if species_formula != substance_formula {
        return Err(CatalogueError::new(
            CatalogueErrorCode::InconsistentSpecies,
            format!(
                "species `{}` formula does not match substance `{}`",
                record.id, record.substance
            ),
        ));
    }
    if record.phase == Phase::Aqueous && record.condition.media.is_none() {
        return Err(CatalogueError::new(
            CatalogueErrorCode::InvalidCondition,
            format!(
                "aqueous species `{}` must select at least one medium",
                record.id
            ),
        ));
    }
    if record
        .condition
        .phases
        .as_ref()
        .is_some_and(|phases| !phases.contains(&record.phase))
    {
        return Err(CatalogueError::new(
            CatalogueErrorCode::InvalidCondition,
            format!(
                "species `{}` phase {:?} is excluded by its own condition domain",
                record.id, record.phase
            ),
        ));
    }
    if let Some(media) = &record.condition.media {
        for medium in media {
            let medium = &document.media[indexes.media_by_id[medium]];
            if !medium.supported_phases.contains(&record.phase) {
                return Err(CatalogueError::new(
                    CatalogueErrorCode::InconsistentSpecies,
                    format!(
                        "species `{}` phase {:?} is unsupported by medium `{}`",
                        record.id, record.phase, medium.id
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn validate_species_ambiguity(
    document: &CatalogueDocument,
    registry: &StaticElementRegistry,
) -> Result<(), CatalogueError> {
    let normalized = document
        .species
        .iter()
        .map(|record| normalize_formula(&record.formula, registry, &record.id.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    for left_index in 0..document.species.len() {
        for right_index in left_index + 1..document.species.len() {
            let left = &document.species[left_index];
            let right = &document.species[right_index];
            if normalized[left_index] == normalized[right_index]
                && left.charge == right.charge
                && left.phase == right.phase
                && left.condition.overlaps(&right.condition)
            {
                return Err(CatalogueError::new(
                    CatalogueErrorCode::InconsistentSpecies,
                    format!(
                        "species `{}` and `{}` ambiguously overlap for one formula, charge, phase, and condition tuple",
                        left.id, right.id
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn validate_evidence(source: &EvidenceSource) -> Result<(), CatalogueError> {
    require_nonempty("evidence title", &source.title)?;
    require_nonempty("evidence publisher", &source.publisher)?;
    require_nonempty("evidence locator", &source.locator)?;
    require_nonempty("evidence reference", &source.reference)?;
    require_nonempty("evidence usage metadata", &source.usage)?;
    validate_date("evidence retrieval date", &source.retrieved_on)?;
    if let Some(date) = &source.publication_date {
        validate_date("evidence publication date", date)?;
    }
    Ok(())
}

fn validate_fact(
    fact: &FactRecord,
    document: &CatalogueDocument,
    indexes: &CatalogueIndexes,
    registry: &StaticElementRegistry,
) -> Result<(), CatalogueError> {
    require_nonempty("fact rule version", &fact.rule_version)?;
    validate_condition(&fact.condition, indexes)?;
    validate_reviewed_record(
        &fact.id.to_string(),
        &fact.review,
        &fact.evidence,
        document.publication,
        indexes,
    )?;
    validate_fact_proposition(fact, document, indexes, registry)?;
    validate_fact_applicability(fact, document, indexes)?;
    Ok(())
}

fn validate_fact_proposition(
    fact: &FactRecord,
    document: &CatalogueDocument,
    indexes: &CatalogueIndexes,
    registry: &StaticElementRegistry,
) -> Result<(), CatalogueError> {
    match &fact.proposition {
        FactProposition::HasAtomicMass {
            element,
            relative_atomic_mass,
        } => validate_atomic_mass(fact, *element, relative_atomic_mass, document),
        FactProposition::Dissociates {
            analytical_species,
            products,
        } => validate_dissociation(
            fact,
            analytical_species,
            products,
            document,
            indexes,
            registry,
        ),
        FactProposition::Soluble { species } => {
            require_species(indexes, species, "fact species")?;
            require_species_phase(document, indexes, species, Phase::Aqueous, "soluble")
        }
        FactProposition::Insoluble { species } => {
            require_species(indexes, species, "fact species")?;
            require_species_phase(document, indexes, species, Phase::Solid, "insoluble")
        }
        FactProposition::SupportsGasModel { species } => {
            require_species(indexes, species, "fact species")?;
            require_species_phase(document, indexes, species, Phase::Gas, "gas-model")
        }
        FactProposition::HasColour { species, colour } => {
            require_species(indexes, species, "fact species")?;
            require_nonempty("colour", colour)
        }
        FactProposition::HasDensity { substance, density } => {
            validate_density(fact, substance, density, indexes)
        }
        FactProposition::HasPhase { substance, phase } => {
            validate_phase_fact(fact, substance, *phase, document, indexes)
        }
        FactProposition::SupportsGasPattern {
            reactants,
            products,
        } => validate_gas_pattern(fact, reactants, products, document, indexes, registry),
    }
}

fn require_species_phase(
    document: &CatalogueDocument,
    indexes: &CatalogueIndexes,
    id: &SpeciesId,
    phase: Phase,
    proposition: &str,
) -> Result<(), CatalogueError> {
    let species = &document.species[indexes.species_by_id[id]];
    if species.phase != phase {
        return Err(CatalogueError::new(
            CatalogueErrorCode::InconsistentSpecies,
            format!(
                "{proposition} proposition targets species `{id}` in phase {:?}, expected {phase:?}",
                species.phase
            ),
        ));
    }
    Ok(())
}

fn validate_phase_fact(
    fact: &FactRecord,
    substance: &SubstanceId,
    phase: Phase,
    document: &CatalogueDocument,
    indexes: &CatalogueIndexes,
) -> Result<(), CatalogueError> {
    require_substance(indexes, substance, "phase substance")?;
    let supported = document.species.iter().any(|species| {
        species.substance == *substance
            && species.phase == phase
            && fact.condition.is_subset_of(&species.condition)
    });
    if !supported {
        return Err(CatalogueError::new(
            CatalogueErrorCode::InconsistentSpecies,
            format!(
                "phase fact `{}` has no matching supported species for substance `{substance}`",
                fact.id
            ),
        ));
    }
    Ok(())
}

fn validate_atomic_mass(
    fact: &FactRecord,
    element: ElementId,
    relative_atomic_mass: &chem_domain::SourceDecimal,
    document: &CatalogueDocument,
) -> Result<(), CatalogueError> {
    if !document
        .elements
        .iter()
        .any(|record| record.element.id == element)
    {
        return Err(CatalogueError::new(
            CatalogueErrorCode::UnknownReference,
            format!(
                "unknown atomic-mass element reference `{}`",
                element.atomic_number()
            ),
        ));
    }
    if relative_atomic_mass.exact_value().is_zero()
        || relative_atomic_mass.exact_value().is_negative()
    {
        return Err(CatalogueError::new(
            CatalogueErrorCode::InvalidDimension,
            format!("fact `{}` has a non-positive atomic mass", fact.id),
        ));
    }
    Ok(())
}

fn validate_dissociation(
    fact: &FactRecord,
    analytical_species: &SpeciesId,
    products: &[SpeciesCoefficient],
    document: &CatalogueDocument,
    indexes: &CatalogueIndexes,
    registry: &StaticElementRegistry,
) -> Result<(), CatalogueError> {
    require_species(
        indexes,
        analytical_species,
        "dissociation analytical species",
    )?;
    validate_coefficients(products, indexes)?;
    if products.len() < 2 {
        return Err(CatalogueError::new(
            CatalogueErrorCode::InvalidFormula,
            format!(
                "fact `{}` must have at least two dissociation products",
                fact.id
            ),
        ));
    }
    require_species_phase(
        document,
        indexes,
        analytical_species,
        Phase::Aqueous,
        "dissociation",
    )?;
    let mut distinct_products = BTreeSet::new();
    for product in products {
        require_species_phase(
            document,
            indexes,
            &product.species,
            Phase::Aqueous,
            "dissociation product",
        )?;
        let species = &document.species[indexes.species_by_id[&product.species]];
        if product.species == *analytical_species
            || species.charge.value() == &BigInt::from(0_u8)
            || !distinct_products.insert(&product.species)
        {
            return Err(CatalogueError::new(
                CatalogueErrorCode::InconsistentSpecies,
                format!(
                    "fact `{}` dissociation products must be distinct charged aqueous species",
                    fact.id
                ),
            ));
        }
    }
    let reactants = [SpeciesCoefficient {
        species: analytical_species.clone(),
        coefficient: chem_domain::Count::one(),
    }];
    validate_conservation(
        &reactants,
        products,
        document,
        indexes,
        registry,
        &fact.id.to_string(),
    )
}

fn validate_density(
    fact: &FactRecord,
    substance: &SubstanceId,
    density: &chem_domain::Quantity,
    indexes: &CatalogueIndexes,
) -> Result<(), CatalogueError> {
    require_substance(indexes, substance, "density substance")?;
    if density.dimension() != Dimension::DENSITY
        || density.canonical_value().is_zero()
        || density.canonical_value().is_negative()
    {
        return Err(CatalogueError::new(
            CatalogueErrorCode::InvalidDimension,
            format!(
                "fact `{}` density must be positive and have Density dimension",
                fact.id
            ),
        ));
    }
    Ok(())
}

fn validate_gas_pattern(
    fact: &FactRecord,
    reactants: &[SpeciesCoefficient],
    products: &[SpeciesCoefficient],
    document: &CatalogueDocument,
    indexes: &CatalogueIndexes,
    registry: &StaticElementRegistry,
) -> Result<(), CatalogueError> {
    if reactants.is_empty() || products.is_empty() {
        return Err(CatalogueError::new(
            CatalogueErrorCode::InvalidFormula,
            format!(
                "fact `{}` gas pattern must have reactants and products",
                fact.id
            ),
        ));
    }
    validate_coefficients(reactants, indexes)?;
    validate_coefficients(products, indexes)?;
    if coefficient_map(reactants) == coefficient_map(products)
        || !products
            .iter()
            .any(|term| document.species[indexes.species_by_id[&term.species]].phase == Phase::Gas)
    {
        return Err(CatalogueError::new(
            CatalogueErrorCode::InconsistentSpecies,
            format!(
                "fact `{}` gas pattern must change species and produce a gas-phase species",
                fact.id
            ),
        ));
    }
    validate_conservation(
        reactants,
        products,
        document,
        indexes,
        registry,
        &fact.id.to_string(),
    )
}

fn validate_fact_applicability(
    fact: &FactRecord,
    document: &CatalogueDocument,
    indexes: &CatalogueIndexes,
) -> Result<(), CatalogueError> {
    for species_id in fact_species(&fact.proposition) {
        let species = &document.species[indexes.species_by_id[&species_id]];
        let mut fact_environment = fact.condition.clone();
        let mut species_environment = species.condition.clone();
        fact_environment.phases = None;
        species_environment.phases = None;
        if !fact_environment.is_subset_of(&species_environment)
            || fact
                .condition
                .phases
                .as_ref()
                .is_some_and(|phases| !phases.contains(&species.phase))
        {
            return Err(CatalogueError::new(
                CatalogueErrorCode::InvalidCondition,
                format!(
                    "fact `{}` extends beyond species `{species_id}` applicability",
                    fact.id
                ),
            ));
        }
    }
    Ok(())
}

fn validate_assumption(
    assumption: &AssumptionKindRecord,
    document: &CatalogueDocument,
    indexes: &CatalogueIndexes,
) -> Result<(), CatalogueError> {
    require_nonempty("assumption version", &assumption.version)?;
    require_nonempty("assumption explanation", &assumption.explanation)?;
    validate_condition(&assumption.condition, indexes)?;
    if assumption.permitted_goals.is_empty() {
        return Err(CatalogueError::new(
            CatalogueErrorCode::InvalidReview,
            format!("assumption `{}` cannot discharge any goal", assumption.id),
        ));
    }
    let expected_goal = match assumption.proposition {
        AssumptionPropositionKind::IdealGasBehaviour => AssumptionGoalKind::GasState,
        AssumptionPropositionKind::NegligibleVolumeChange => AssumptionGoalKind::VolumeComposition,
        AssumptionPropositionKind::IdealFiltration | AssumptionPropositionKind::IdealDecanting => {
            AssumptionGoalKind::PhasePartition
        }
    };
    if assumption.permitted_goals != BTreeSet::from([expected_goal])
        || !assumption_target_is_valid(assumption)
    {
        return Err(CatalogueError::new(
            CatalogueErrorCode::InvalidReview,
            format!(
                "assumption `{}` has goals or a target incompatible with its closed proposition schema",
                assumption.id
            ),
        ));
    }
    validate_reviewed_record(
        &assumption.id.to_string(),
        &assumption.review,
        &assumption.evidence,
        document.publication,
        indexes,
    )
}

fn assumption_target_is_valid(assumption: &AssumptionKindRecord) -> bool {
    match assumption.proposition {
        AssumptionPropositionKind::IdealGasBehaviour => matches!(
            assumption.required_target,
            AssumptionTargetKind::Environment | AssumptionTargetKind::Species
        ),
        AssumptionPropositionKind::NegligibleVolumeChange => matches!(
            assumption.required_target,
            AssumptionTargetKind::Material | AssumptionTargetKind::Stage
        ),
        AssumptionPropositionKind::IdealFiltration | AssumptionPropositionKind::IdealDecanting => {
            matches!(
                assumption.required_target,
                AssumptionTargetKind::Vessel | AssumptionTargetKind::Stage
            )
        }
    }
}

fn validate_coverage(
    coverage: &CoverageDeclaration,
    document: &CatalogueDocument,
    indexes: &CatalogueIndexes,
) -> Result<(), CatalogueError> {
    validate_condition(&coverage.condition, indexes)?;
    if coverage.species.is_empty() || coverage.families.is_empty() {
        return Err(CatalogueError::new(
            CatalogueErrorCode::InvalidCoverage,
            format!(
                "coverage `{}` must declare species and reaction families",
                coverage.id
            ),
        ));
    }
    for species in &coverage.species {
        require_species(indexes, species, "coverage species")?;
        let species_record = &document.species[indexes.species_by_id[species]];
        let mut coverage_environment = coverage.condition.clone();
        let mut species_environment = species_record.condition.clone();
        coverage_environment.phases = None;
        species_environment.phases = None;
        if !coverage_environment.is_subset_of(&species_environment) {
            return Err(CatalogueError::new(
                CatalogueErrorCode::InvalidCoverage,
                format!(
                    "coverage `{}` extends beyond species `{species}` applicability",
                    coverage.id
                ),
            ));
        }
        if coverage
            .condition
            .phases
            .as_ref()
            .is_some_and(|phases| !phases.contains(&species_record.phase))
        {
            return Err(CatalogueError::new(
                CatalogueErrorCode::InvalidCoverage,
                format!(
                    "coverage `{}` phase domain excludes species `{species}`",
                    coverage.id
                ),
            ));
        }
    }
    let mut seen_exclusions = BTreeSet::new();
    for exclusion in &coverage.exclusions {
        require_nonempty("coverage exclusion reason", &exclusion.reason)?;
        if exclusion.species.is_empty()
            || exclusion.families.is_empty()
            || !exclusion.species.is_subset(&coverage.species)
            || !exclusion.families.is_subset(&coverage.families)
        {
            return Err(CatalogueError::new(
                CatalogueErrorCode::InvalidCoverage,
                format!("coverage `{}` has an invalid exclusion", coverage.id),
            ));
        }
        if !seen_exclusions.insert((exclusion.species.clone(), exclusion.families.clone())) {
            return Err(CatalogueError::new(
                CatalogueErrorCode::InvalidCoverage,
                format!("coverage `{}` repeats an exclusion", coverage.id),
            ));
        }
    }
    validate_reviewed_record(
        &coverage.id.to_string(),
        &coverage.review,
        &coverage.evidence,
        document.publication,
        indexes,
    )
}

fn validate_reviewed_record(
    id: &str,
    review: &ReviewMetadata,
    evidence: &BTreeSet<EvidenceSourceId>,
    publication: PublicationKind,
    indexes: &CatalogueIndexes,
) -> Result<(), CatalogueError> {
    if publication == PublicationKind::Production && review.status != ReviewStatus::Reviewed {
        return Err(CatalogueError::new(
            CatalogueErrorCode::IneligibleProductionRecord,
            format!("production record `{id}` is not reviewed"),
        ));
    }
    if review.status == ReviewStatus::Reviewed {
        if evidence.is_empty() {
            return Err(CatalogueError::new(
                CatalogueErrorCode::MissingEvidence,
                format!("reviewed record `{id}` has no evidence"),
            ));
        }
        if review.reviewers.is_empty() {
            return Err(CatalogueError::new(
                CatalogueErrorCode::InvalidReview,
                format!("reviewed record `{id}` has no reviewer metadata"),
            ));
        }
    }
    for evidence_id in evidence {
        if !indexes.evidence_by_id.contains_key(evidence_id) {
            return unknown_reference("record evidence", evidence_id);
        }
    }
    for reviewer in &review.reviewers {
        require_nonempty("reviewer", &reviewer.reviewer)?;
        validate_date("review date", &reviewer.reviewed_on)?;
        if let Some(reference) = &reviewer.reference {
            require_nonempty("review reference", reference)?;
        }
    }
    Ok(())
}

fn validate_condition(
    condition: &ConditionDomain,
    indexes: &CatalogueIndexes,
) -> Result<(), CatalogueError> {
    if let Some(range) = &condition.temperature_kelvin {
        validate_range(range, "temperature", true)?;
    }
    if let Some(range) = &condition.pressure_pascal {
        validate_range(range, "pressure", true)?;
    }
    if condition.media.as_ref().is_some_and(BTreeSet::is_empty)
        || condition.phases.as_ref().is_some_and(BTreeSet::is_empty)
    {
        return Err(CatalogueError::new(
            CatalogueErrorCode::InvalidCondition,
            "condition-domain finite sets cannot be empty",
        ));
    }
    if let Some(media) = &condition.media {
        for medium in media {
            if !indexes.media_by_id.contains_key(medium) {
                return unknown_reference("condition medium", medium);
            }
        }
    }
    Ok(())
}

fn validate_range(range: &ExactRange, name: &str, nonnegative: bool) -> Result<(), CatalogueError> {
    let empty = range.minimum > range.maximum
        || range.minimum == range.maximum
            && (range.minimum_bound == BoundaryKind::Exclusive
                || range.maximum_bound == BoundaryKind::Exclusive);
    if empty || nonnegative && range.minimum.is_negative() {
        return Err(CatalogueError::new(
            CatalogueErrorCode::InvalidCondition,
            format!("invalid {name} condition range"),
        ));
    }
    Ok(())
}

fn validate_conflicts(
    document: &CatalogueDocument,
    indexes: &CatalogueIndexes,
) -> Result<(), CatalogueError> {
    for left_index in 0..document.facts.len() {
        for right_index in left_index + 1..document.facts.len() {
            let left = &document.facts[left_index];
            let right = &document.facts[right_index];
            if left.condition.overlaps(&right.condition)
                && propositions_contradict(&left.proposition, &right.proposition)
            {
                return Err(CatalogueError::new(
                    CatalogueErrorCode::ContradictoryFacts,
                    format!(
                        "facts `{}` and `{}` contradict over overlapping conditions",
                        left.id, right.id
                    ),
                ));
            }
        }
    }
    let _ = indexes;
    Ok(())
}

fn propositions_contradict(left: &FactProposition, right: &FactProposition) -> bool {
    match (left, right) {
        (
            FactProposition::Soluble { species: left },
            FactProposition::Insoluble { species: right },
        )
        | (
            FactProposition::Insoluble { species: left },
            FactProposition::Soluble { species: right },
        ) => left == right,
        (
            FactProposition::HasAtomicMass {
                element: left_element,
                relative_atomic_mass: left_value,
            },
            FactProposition::HasAtomicMass {
                element: right_element,
                relative_atomic_mass: right_value,
            },
        ) => left_element == right_element && left_value.exact_value() != right_value.exact_value(),
        (
            FactProposition::HasColour {
                species: left_species,
                colour: left_colour,
            },
            FactProposition::HasColour {
                species: right_species,
                colour: right_colour,
            },
        ) => left_species == right_species && alias_key(left_colour) != alias_key(right_colour),
        (
            FactProposition::HasPhase {
                substance: left_substance,
                phase: left_phase,
            },
            FactProposition::HasPhase {
                substance: right_substance,
                phase: right_phase,
            },
        ) => left_substance == right_substance && left_phase != right_phase,
        (
            FactProposition::Dissociates {
                analytical_species: left_species,
                products: left_products,
            },
            FactProposition::Dissociates {
                analytical_species: right_species,
                products: right_products,
            },
        ) => {
            left_species == right_species
                && coefficient_map(left_products) != coefficient_map(right_products)
        }
        (
            FactProposition::HasDensity {
                substance: left_substance,
                density: left_density,
            },
            FactProposition::HasDensity {
                substance: right_substance,
                density: right_density,
            },
        ) => left_substance == right_substance && left_density != right_density,
        (
            FactProposition::SupportsGasPattern {
                reactants: left_reactants,
                products: left_products,
            },
            FactProposition::SupportsGasPattern {
                reactants: right_reactants,
                products: right_products,
            },
        ) => {
            coefficient_map(left_reactants) == coefficient_map(right_reactants)
                && coefficient_map(left_products) != coefficient_map(right_products)
        }
        _ => false,
    }
}

fn coefficient_map(values: &[SpeciesCoefficient]) -> BTreeMap<SpeciesId, String> {
    values
        .iter()
        .map(|term| (term.species.clone(), term.coefficient.value().to_string()))
        .collect()
}

fn validate_coefficients(
    coefficients: &[SpeciesCoefficient],
    indexes: &CatalogueIndexes,
) -> Result<(), CatalogueError> {
    let mut seen = BTreeSet::new();
    for coefficient in coefficients {
        require_species(indexes, &coefficient.species, "reaction species")?;
        if !seen.insert(coefficient.species.clone()) {
            return Err(CatalogueError::new(
                CatalogueErrorCode::InvalidFormula,
                format!(
                    "duplicate species `{}` in reaction side",
                    coefficient.species
                ),
            ));
        }
    }
    Ok(())
}

fn validate_conservation(
    reactants: &[SpeciesCoefficient],
    products: &[SpeciesCoefficient],
    document: &CatalogueDocument,
    indexes: &CatalogueIndexes,
    registry: &StaticElementRegistry,
    id: &str,
) -> Result<(), CatalogueError> {
    let (reactant_atoms, reactant_charge) =
        reaction_totals(reactants, document, indexes, registry)?;
    let (product_atoms, product_charge) = reaction_totals(products, document, indexes, registry)?;
    if reactant_atoms != product_atoms || reactant_charge != product_charge {
        return Err(CatalogueError::new(
            CatalogueErrorCode::UnconservedReaction,
            format!("reaction record `{id}` does not conserve atoms and charge"),
        ));
    }
    Ok(())
}

fn reaction_totals(
    side: &[SpeciesCoefficient],
    document: &CatalogueDocument,
    indexes: &CatalogueIndexes,
    registry: &StaticElementRegistry,
) -> Result<(BTreeMap<ElementId, BigUint>, BigInt), CatalogueError> {
    let mut atoms = BTreeMap::<ElementId, BigUint>::new();
    let mut charge = BigInt::from(0_u8);
    for term in side {
        let species = &document.species[indexes.species_by_id[&term.species]];
        let formula = normalize_formula(&species.formula, registry, &species.id.to_string())?;
        for (element, count) in formula.composition() {
            *atoms.entry(*element).or_default() += count * term.coefficient.value();
        }
        charge += species.charge.value() * BigInt::from(term.coefficient.value().clone());
    }
    Ok((atoms, charge))
}

fn normalize_formula(
    formula: &MolecularFormula,
    registry: &StaticElementRegistry,
    owner: &str,
) -> Result<NormalizedFormula, CatalogueError> {
    if formula.elements.is_empty() {
        return Err(CatalogueError::new(
            CatalogueErrorCode::InvalidFormula,
            format!("invalid formula for `{owner}`: formula is empty"),
        ));
    }
    let mut seen = BTreeSet::new();
    if let Some(duplicate) = formula
        .elements
        .iter()
        .find(|element| !seen.insert(element.symbol.clone()))
    {
        return Err(CatalogueError::new(
            CatalogueErrorCode::InvalidFormula,
            format!(
                "invalid formula for `{owner}`: duplicate element `{}`",
                duplicate.symbol
            ),
        ));
    }
    let resolved_ids = formula
        .elements
        .iter()
        .map(|element| {
            registry
                .resolve(&element.symbol)
                .map(|resolved| resolved.id)
                .ok_or_else(|| {
                    CatalogueError::new(
                        CatalogueErrorCode::InvalidFormula,
                        format!(
                            "invalid formula for `{owner}`: unknown element `{}`",
                            element.symbol
                        ),
                    )
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if resolved_ids.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(CatalogueError::new(
            CatalogueErrorCode::InvalidFormula,
            format!(
                "invalid formula for `{owner}`: elements are not in ascending atomic-number order"
            ),
        ));
    }
    let syntax = FormulaSyntax {
        segments: vec![FormulaSegment {
            coefficient: chem_domain::Count::one(),
            parts: formula
                .elements
                .iter()
                .map(|element| FormulaPart::Element {
                    symbol: element.symbol.clone(),
                    count: element.count.clone(),
                })
                .collect(),
        }],
    };
    syntax.normalize(registry).map_err(|error| {
        CatalogueError::new(
            CatalogueErrorCode::InvalidFormula,
            format!("invalid formula for `{owner}`: {error}"),
        )
    })
}

fn fact_species(proposition: &FactProposition) -> Vec<SpeciesId> {
    match proposition {
        FactProposition::Dissociates {
            analytical_species,
            products,
        } => std::iter::once(analytical_species.clone())
            .chain(products.iter().map(|term| term.species.clone()))
            .collect(),
        FactProposition::Soluble { species }
        | FactProposition::Insoluble { species }
        | FactProposition::SupportsGasModel { species }
        | FactProposition::HasColour { species, .. } => vec![species.clone()],
        FactProposition::SupportsGasPattern {
            reactants,
            products,
        } => reactants
            .iter()
            .chain(products)
            .map(|term| term.species.clone())
            .collect(),
        FactProposition::HasAtomicMass { .. }
        | FactProposition::HasDensity { .. }
        | FactProposition::HasPhase { .. } => Vec::new(),
    }
}

fn require_species(
    indexes: &CatalogueIndexes,
    id: &SpeciesId,
    kind: &str,
) -> Result<(), CatalogueError> {
    if indexes.species_by_id.contains_key(id) {
        Ok(())
    } else {
        unknown_reference(kind, id)
    }
}

fn require_substance(
    indexes: &CatalogueIndexes,
    id: &SubstanceId,
    kind: &str,
) -> Result<(), CatalogueError> {
    if indexes.substances_by_id.contains_key(id) {
        Ok(())
    } else {
        unknown_reference(kind, id)
    }
}

fn require_nonempty(kind: &str, value: &str) -> Result<(), CatalogueError> {
    if value.trim().is_empty() {
        Err(CatalogueError::new(
            CatalogueErrorCode::InvalidMetadata,
            format!("{kind} must not be empty"),
        ))
    } else {
        Ok(())
    }
}

fn duplicate_id<T>(kind: &str, id: &str) -> Result<T, CatalogueError> {
    Err(CatalogueError::new(
        CatalogueErrorCode::DuplicateId,
        format!("duplicate {kind} identifier `{id}`"),
    ))
}

fn unknown_reference<T>(kind: &str, id: &impl fmt::Display) -> Result<T, CatalogueError> {
    Err(CatalogueError::new(
        CatalogueErrorCode::UnknownReference,
        format!("unknown {kind} reference `{id}`"),
    ))
}

fn alias_key(alias: &str) -> String {
    alias.trim().to_ascii_lowercase()
}

fn valid_catalogue_name(name: &str) -> bool {
    let mut segments = name.split('.');
    let valid = segments.all(|segment| {
        !segment.is_empty()
            && segment.as_bytes()[0].is_ascii_alphabetic()
            && segment
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    });
    valid && name.contains('.')
}

fn valid_semantic_version(version: &str) -> bool {
    let components = version.split('.').collect::<Vec<_>>();
    components.len() == 3
        && components.iter().all(|component| {
            !component.is_empty()
                && component.bytes().all(|byte| byte.is_ascii_digit())
                && (component == &"0" || !component.starts_with('0'))
        })
}

fn validate_date(kind: &str, date: &str) -> Result<(), CatalogueError> {
    let bytes = date.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes
            .iter()
            .enumerate()
            .any(|(index, byte)| index != 4 && index != 7 && !byte.is_ascii_digit())
    {
        return Err(CatalogueError::new(
            CatalogueErrorCode::InvalidMetadata,
            format!("invalid {kind} `{date}`"),
        ));
    }
    let year = parse_date_part(&date[0..4]);
    let month = parse_date_part(&date[5..7]);
    let day = parse_date_part(&date[8..10]);
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 0,
    };
    if year == 0 || day == 0 || day > days {
        return Err(CatalogueError::new(
            CatalogueErrorCode::InvalidMetadata,
            format!("invalid {kind} `{date}`"),
        ));
    }
    Ok(())
}

fn parse_date_part(part: &str) -> u32 {
    part.bytes()
        .fold(0_u32, |value, byte| value * 10 + u32::from(byte - b'0'))
}

#[cfg(test)]
mod tests {
    use chem_domain::ExactScalar;

    use super::{BoundaryKind, ExactRange};

    fn scalar(value: i64) -> ExactScalar {
        ExactScalar::from_integer(value)
    }

    #[test]
    fn range_boundaries_are_explicit() {
        let closed = ExactRange {
            minimum: scalar(0),
            maximum: scalar(10),
            minimum_bound: BoundaryKind::Inclusive,
            maximum_bound: BoundaryKind::Inclusive,
        };
        let right_open = ExactRange {
            minimum: scalar(10),
            maximum: scalar(20),
            minimum_bound: BoundaryKind::Exclusive,
            maximum_bound: BoundaryKind::Inclusive,
        };
        assert!(closed.contains(&scalar(0)));
        assert!(closed.contains(&scalar(10)));
        assert!(!right_open.contains(&scalar(10)));
        assert!(!closed.overlaps(&right_open));
    }
}
