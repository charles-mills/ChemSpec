use std::collections::BTreeMap;

use serde::Serialize;

use crate::{
    AnalyticalComponent, ExactScalar, InventoryPortionId, MaterialId, MediumId, OperationId,
    Phase, Quantity, ReactionOpportunityId, StageId, TemperaturePoint, VesselId,
};

/// Closure state of one logical vessel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ClosureState {
    Open,
    Closed,
}

/// Contact state asserted by explicit procedure operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MixingState {
    Unmixed,
    HomogeneousContact,
}

/// One active, linearly identified inventory portion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InventoryPortion {
    pub id: InventoryPortionId,
    pub root_material: MaterialId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<InventoryPortionId>,
    pub components: Vec<AnalyticalComponent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub known_volume: Option<ExactScalar>,
}

/// Current location of an active inventory portion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum InventoryLocation {
    Unplaced { material: MaterialId },
    InVessel { vessel: VesselId },
}

/// Append-only inventory movement and lineage ledger.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LedgerEntry {
    Initial {
        portion: InventoryPortionId,
        material: MaterialId,
    },
    Move {
        portion: InventoryPortionId,
        operation: OperationId,
        from: InventoryLocation,
        to: InventoryLocation,
    },
    Split {
        parent: InventoryPortionId,
        retained: InventoryPortionId,
        moved: InventoryPortionId,
        operation: OperationId,
        moved_fraction: ExactScalar,
    },
    Separate {
        parent: InventoryPortionId,
        products: Vec<InventoryPortionId>,
        operation: OperationId,
    },
}

/// Exact state of one vessel at one immutable stage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VesselState {
    pub id: VesselId,
    pub capacity: Quantity,
    pub closure: ClosureState,
    pub temperature: TemperaturePoint,
    pub pressure: Quantity,
    pub contents: Vec<InventoryPortion>,
    pub phase_partitions: BTreeMap<Phase, Vec<InventoryPortionId>>,
    pub mixing: MixingState,
}

/// Closed kernel rule families which may later inspect an opportunity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ReactionRuleFamily {
    Dissociation,
    Solubility,
    GasPattern,
    PhasePartition,
}

/// Trigger for an open reaction opportunity; this is not a reaction result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OpportunityTrigger {
    Placement,
    CoLocation,
    HomogeneousContact,
    ThermalChange,
    ClosureChange,
    Separation,
    Transfer,
}

/// Kernel-internal question created by a state transition without inferring
/// products or reaction extent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReactionOpportunity {
    pub id: ReactionOpportunityId,
    pub operation: OperationId,
    pub stage: StageId,
    pub vessel: VesselId,
    pub trigger: OpportunityTrigger,
    pub candidates: Vec<AnalyticalComponent>,
    pub temperature: TemperaturePoint,
    pub pressure: Quantity,
    pub families: Vec<ReactionRuleFamily>,
}

/// Exact environment retained by every stage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StageEnvironment {
    pub temperature: TemperaturePoint,
    pub pressure: Quantity,
    pub medium: MediumId,
}

/// Dependency-free byte range used by state artifacts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceRange {
    pub start: usize,
    pub end: usize,
}

/// One immutable experiment state. `final` aliases the last element of a
/// timeline and is never constructed separately.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Stage {
    pub id: StageId,
    pub ordinal: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_label: Option<String>,
    pub elapsed_seconds: ExactScalar,
    pub environment: StageEnvironment,
    pub vessels: BTreeMap<VesselId, VesselState>,
    pub unplaced: BTreeMap<MaterialId, InventoryPortion>,
    pub ledger: Vec<LedgerEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition: Option<OperationId>,
    pub reaction_opportunities: Vec<ReactionOpportunity>,
    pub source_origins: BTreeMap<String, Vec<SourceRange>>,
}

/// Complete immutable result of procedure execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StageTimeline {
    pub stages: Vec<Stage>,
}

impl StageTimeline {
    #[must_use]
    pub fn initial(&self) -> Option<&Stage> {
        self.stages.first()
    }

    #[must_use]
    pub fn final_stage(&self) -> Option<&Stage> {
        self.stages.last()
    }
}
