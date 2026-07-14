use std::collections::BTreeSet;

use serde::Serialize;

use crate::{
    AssumptionPremiseId, Charge, Dimension, ExactScalar, FactId, MaterialId, MediumId,
    NormalizedFormula, Phase, Quantity, SpeciesId, SubstanceId,
};

/// Closed exact-arithmetic rule used to construct a derived quantity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DerivedQuantityRule {
    AuthoredQuantity,
    AnalyticalAmount,
    MolarMass,
    AmountFromMass,
    MassFromVolumeAndDensity,
    AmountFromLiquidVolume,
    IdealGasAmount,
    PreparedComponentSum,
    MixtureVolume,
    DissociationStoichiometry,
    ProportionalSplit,
}

/// One exact typed input to a derived-quantity rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DerivedInput {
    pub role: String,
    pub canonical_value: ExactScalar,
    pub dimension: Dimension,
}

/// Replayable provenance for one exact derived quantity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct QuantityDerivation {
    pub rule: DerivedQuantityRule,
    pub inputs: Vec<DerivedInput>,
    pub premises: BTreeSet<FactId>,
    pub assumptions: BTreeSet<AssumptionPremiseId>,
}

/// A quantity derived by exact arithmetic rather than authored directly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DerivedQuantity {
    pub canonical_value: ExactScalar,
    pub dimension: Dimension,
    pub derivation: Box<QuantityDerivation>,
}

impl DerivedQuantity {
    #[must_use]
    pub fn new(
        canonical_value: ExactScalar,
        dimension: Dimension,
        derivation: QuantityDerivation,
    ) -> Self {
        Self {
            canonical_value,
            dimension,
            derivation: Box::new(derivation),
        }
    }
}

/// One catalogue-resolved chemical species used by typed experiment input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedSpecies {
    pub id: SpeciesId,
    pub substance: SubstanceId,
    pub formula: NormalizedFormula,
    pub charge: Charge,
    pub phase: Phase,
    pub identity_premise: FactId,
}

/// The analytical preparation view of one material component.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AnalyticalComponent {
    pub species: ResolvedSpecies,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount: Option<DerivedQuantity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mass: Option<DerivedQuantity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<DerivedQuantity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub concentration: Option<DerivedQuantity>,
}

/// A normalized prepared-component inventory retaining every authored origin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedComponent {
    pub analytical: AnalyticalComponent,
    pub source_component_indices: Vec<u32>,
}

/// The dimension-directed constructor selected for an initial material.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum MaterialForm {
    SampleByAmount {
        species: ResolvedSpecies,
        amount: Quantity,
    },
    SampleByMass {
        species: ResolvedSpecies,
        mass: Quantity,
        molar_mass: DerivedQuantity,
        amount: DerivedQuantity,
    },
    LiquidSampleByVolume {
        species: ResolvedSpecies,
        volume: Quantity,
        density: Quantity,
        mass: DerivedQuantity,
        molar_mass: DerivedQuantity,
        amount: DerivedQuantity,
    },
    GasSampleByVolume {
        species: ResolvedSpecies,
        volume: Quantity,
        amount: DerivedQuantity,
    },
    Solution {
        analytical_species: ResolvedSpecies,
        total_volume: Quantity,
        analytical_concentration: Quantity,
        analytical_amount: DerivedQuantity,
        medium: MediumId,
        solvent: SubstanceId,
    },
    Prepared {
        components: Vec<PreparedComponent>,
    },
}

/// One finite, linearly identified initial material.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Material {
    pub id: MaterialId,
    pub name: String,
    pub form: MaterialForm,
    pub analytical_inventory: Vec<AnalyticalComponent>,
    pub required_premises: BTreeSet<FactId>,
    pub required_assumptions: BTreeSet<AssumptionPremiseId>,
}
