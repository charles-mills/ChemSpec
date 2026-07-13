use std::collections::BTreeSet;

use chem_domain::{
    AssumptionKindId, Charge, ContentDigest, Count, CoverageId, Element, ElementSymbol,
    EvidenceSourceId, ExactScalar, FactId, MediumId, Phase, Quantity, SourceDecimal, SpeciesId,
    SubstanceId,
};
use serde::{Deserialize, Serialize};

/// Supported on-disk schema version for catalogue envelopes.
pub const CATALOGUE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogueEnvelope {
    pub digest: ContentDigest,
    pub bundle: CatalogueDocument,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogueDocument {
    pub schema_version: u32,
    pub name: String,
    pub version: String,
    pub publication: PublicationKind,
    pub created: CreationMetadata,
    pub elements: Vec<ElementRecord>,
    pub substances: Vec<SubstanceRecord>,
    pub species: Vec<SpeciesRecord>,
    pub media: Vec<MediumRecord>,
    pub facts: Vec<FactRecord>,
    pub assumption_kinds: Vec<AssumptionKindRecord>,
    pub coverage: Vec<CoverageDeclaration>,
    pub evidence: Vec<EvidenceSource>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PublicationKind {
    Working,
    Production,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreationMetadata {
    pub created_on: String,
    pub created_by: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ElementRecord {
    #[serde(flatten)]
    pub element: Element,
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub provenance: RecordProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubstanceRecord {
    pub id: SubstanceId,
    pub name: String,
    pub formula: MolecularFormula,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub provenance: RecordProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpeciesRecord {
    pub id: SpeciesId,
    pub substance: SubstanceId,
    pub formula: MolecularFormula,
    pub charge: Charge,
    pub phase: Phase,
    pub condition: ConditionDomain,
    pub provenance: RecordProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MolecularFormula {
    pub elements: Vec<FormulaElement>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormulaElement {
    pub symbol: ElementSymbol,
    pub count: Count,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediumRecord {
    pub id: MediumId,
    pub name: String,
    pub solvent: SubstanceId,
    pub supported_phases: BTreeSet<Phase>,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub provenance: RecordProvenance,
}

/// Stable evidence and review metadata for identity-bearing catalogue records.
/// Derivations reference `id` exactly as they reference an empirical
/// [`FactRecord`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordProvenance {
    pub id: FactId,
    pub evidence: BTreeSet<EvidenceSourceId>,
    pub review: ReviewMetadata,
    pub rule_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ConditionDomain {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature_kelvin: Option<ExactRange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pressure_pascal: Option<ExactRange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<BTreeSet<MediumId>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phases: Option<BTreeSet<Phase>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExactRange {
    pub minimum: ExactScalar,
    pub maximum: ExactScalar,
    pub minimum_bound: BoundaryKind,
    pub maximum_bound: BoundaryKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BoundaryKind {
    Inclusive,
    Exclusive,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConditionPoint {
    pub temperature_kelvin: ExactScalar,
    pub pressure_pascal: ExactScalar,
    pub medium: MediumId,
    pub phase: Option<Phase>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactRecord {
    pub id: FactId,
    pub proposition: FactProposition,
    pub condition: ConditionDomain,
    pub evidence: BTreeSet<EvidenceSourceId>,
    pub review: ReviewMetadata,
    pub rule_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FactProposition {
    HasAtomicMass {
        element: chem_domain::ElementId,
        relative_atomic_mass: SourceDecimal,
    },
    Dissociates {
        analytical_species: SpeciesId,
        products: Vec<SpeciesCoefficient>,
    },
    Soluble {
        species: SpeciesId,
    },
    Insoluble {
        species: SpeciesId,
    },
    HasDensity {
        substance: SubstanceId,
        density: Box<Quantity>,
    },
    SupportsGasModel {
        species: SpeciesId,
    },
    HasColour {
        species: SpeciesId,
        colour: String,
    },
    HasPhase {
        substance: SubstanceId,
        phase: Phase,
    },
    SupportsGasPattern {
        reactants: Vec<SpeciesCoefficient>,
        products: Vec<SpeciesCoefficient>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpeciesCoefficient {
    pub species: SpeciesId,
    pub coefficient: chem_domain::Count,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewMetadata {
    pub status: ReviewStatus,
    #[serde(default)]
    pub reviewers: Vec<ReviewerRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReviewStatus {
    Reviewed,
    Provisional,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewerRecord {
    pub reviewer: String,
    pub reviewed_on: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceSource {
    pub id: EvidenceSourceId,
    pub title: String,
    pub publisher: String,
    pub locator: String,
    pub reference: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publication_date: Option<String>,
    pub retrieved_on: String,
    pub usage: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssumptionKindRecord {
    pub id: AssumptionKindId,
    pub version: String,
    pub proposition: AssumptionPropositionKind,
    pub required_target: AssumptionTargetKind,
    pub stage_scope: AssumptionStageScope,
    pub condition: ConditionDomain,
    pub permitted_goals: BTreeSet<AssumptionGoalKind>,
    pub explanation: String,
    pub safety: SafetyClassification,
    pub evidence: BTreeSet<EvidenceSourceId>,
    pub review: ReviewMetadata,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AssumptionPropositionKind {
    IdealGasBehaviour,
    NegligibleVolumeChange,
    IdealFiltration,
    IdealDecanting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AssumptionTargetKind {
    Environment,
    Material,
    Species,
    Vessel,
    Stage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AssumptionStageScope {
    Initial,
    SingleStage,
    RemainingProcedure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AssumptionGoalKind {
    GasState,
    VolumeComposition,
    PhasePartition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SafetyClassification {
    EducationalModel,
    PhysicalApproximation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoverageDeclaration {
    pub id: CoverageId,
    pub species: BTreeSet<SpeciesId>,
    pub condition: ConditionDomain,
    pub families: BTreeSet<ReactionFamily>,
    #[serde(default)]
    pub exclusions: Vec<CoverageExclusion>,
    pub evidence: BTreeSet<EvidenceSourceId>,
    pub review: ReviewMetadata,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReactionFamily {
    Precipitation,
    StrongAcidBase,
    CuratedGasFormation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoverageExclusion {
    pub species: BTreeSet<SpeciesId>,
    pub families: BTreeSet<ReactionFamily>,
    pub reason: String,
}
