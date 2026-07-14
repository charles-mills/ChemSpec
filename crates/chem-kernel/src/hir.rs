use std::collections::BTreeMap;

use chem_catalogue::{AssumptionStageScope, AssumptionTargetKind, SafetyClassification};
use chem_domain::{
    AssumptionKindId, AssumptionPremiseId, ContentDigest, ExperimentId, FactId, Material,
    MaterialId, MediumId, OperationId, Quantity, SpeciesId, StageId, SubstanceId, TemperaturePoint,
    VesselId,
};
use chems_lang::{ByteSpan, SourceExpectation, SourceNode};
use serde::Serialize;

/// Immutable identity of the selected reviewed catalogue bundle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogueBinding {
    pub name: String,
    pub version: String,
    pub digest: ContentDigest,
}

/// The exact initial environment used for catalogue applicability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Environment {
    pub temperature: TemperaturePoint,
    pub pressure: Quantity,
    pub medium: MediumId,
    pub solvent: SubstanceId,
    pub medium_identity_premise: FactId,
}

/// A fully resolved reference to an experiment-local assumption target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AssumptionTarget {
    Environment,
    Material {
        id: MaterialId,
    },
    Species {
        material: MaterialId,
        species: SpeciesId,
    },
    Vessel {
        id: VesselId,
    },
    Stage {
        id: StageId,
    },
}

/// A resolved built-in or labelled stage reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum StageReference {
    Initial,
    Final,
    Label { id: StageId },
}

/// Whether an assumption's reviewed condition domain can be decided at the
/// initial-material boundary or must be checked against a future stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AssumptionApplicability {
    Applicable,
    DeferredToProcedure,
}

/// The earliest phase at which use or non-use of an assumption is decidable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AssumptionUsage {
    UsedInMaterialElaboration,
    DeferredToProcedure,
    Unused,
}

/// One explicit catalogue-defined assumption request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TypedAssumption {
    pub id: AssumptionPremiseId,
    pub kind: AssumptionKindId,
    pub required_target: AssumptionTargetKind,
    pub stage_scope: AssumptionStageScope,
    pub safety: SafetyClassification,
    pub target: AssumptionTarget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<StageReference>,
    pub applicability: AssumptionApplicability,
    pub usage: AssumptionUsage,
}

/// One operation and the immutable stage it produces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TypedProcedureStep {
    pub resulting_stage: StageId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_label: Option<String>,
    pub operation: TypedOperation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum VesselClosure {
    Open,
    Closed,
}

/// An empty logical vessel declaration; state execution begins in Slice 5.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TypedVessel {
    pub id: VesselId,
    pub name: String,
    pub closure: VesselClosure,
    pub capacity: Quantity,
}

/// A procedure operand resolved to the declaration kind required by syntax.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TypedOperation {
    Place {
        id: OperationId,
        material: MaterialId,
        vessel: VesselId,
    },
    Add {
        id: OperationId,
        material: MaterialId,
        vessel: VesselId,
    },
    Combine {
        id: OperationId,
        left: MaterialId,
        right: MaterialId,
        vessel: VesselId,
    },
    Transfer {
        id: OperationId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        quantity: Option<Quantity>,
        source: VesselId,
        destination: VesselId,
    },
    Stir {
        id: OperationId,
        vessel: VesselId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration: Option<Quantity>,
    },
    Heat {
        id: OperationId,
        vessel: VesselId,
        target: TemperaturePoint,
    },
    Cool {
        id: OperationId,
        vessel: VesselId,
        target: TemperaturePoint,
    },
    Wait {
        id: OperationId,
        duration: Quantity,
    },
    Seal {
        id: OperationId,
        vessel: VesselId,
    },
    Open {
        id: OperationId,
        vessel: VesselId,
    },
    Filter {
        id: OperationId,
        source: VesselId,
        filtrate: VesselId,
        residue: VesselId,
    },
    Decant {
        id: OperationId,
        source: VesselId,
        destination: VesselId,
    },
}

/// Source sections intentionally deferred beyond Slice 4.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeferredSections {
    pub expectations: Vec<DeferredExpectation>,
    pub tactics: Vec<SourceNode>,
}

/// Structured claim syntax retained for Slice 6, with its stage reference
/// resolved while the experiment namespace is available.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeferredExpectation {
    pub source: SourceExpectation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<StageReference>,
}

/// Complete Slice 4 typed HIR. Every included operand and chemistry identity
/// is resolved; later semantic sections are represented only by source spans.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TypedExperiment {
    pub schema_version: u32,
    pub language_version: u32,
    pub source_digest: ContentDigest,
    pub catalogue: CatalogueBinding,
    pub id: ExperimentId,
    pub name: String,
    pub environment: Environment,
    pub assumptions: Vec<TypedAssumption>,
    pub materials: Vec<Material>,
    pub vessels: Vec<TypedVessel>,
    pub procedure: Vec<TypedProcedureStep>,
    pub deferred: DeferredSections,
    pub source_origins: BTreeMap<String, Vec<ByteSpan>>,
}
