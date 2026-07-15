use std::{collections::BTreeSet, str::FromStr};

use chem_domain::StructureId;

use crate::{StructuralTraitId, ValidatedCatalogueBundle};

/// Reaction families suggested solely by reviewed catalogue traits.
///
/// A suggestion is not a product claim. The selected generalized rule must
/// still resolve a supported case and cross kernel validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReactionFamilyCandidate {
    MetalHalogen,
    GroupOneWater,
    MetalAcid,
    Combustion,
    AcidBase,
    CarbonateAcid,
    Displacement,
    Precipitation,
}

impl ValidatedCatalogueBundle {
    #[must_use]
    pub fn structure_has_reaction_trait(&self, structure: &StructureId, trait_id: &str) -> bool {
        StructuralTraitId::from_str(trait_id)
            .ok()
            .is_some_and(|trait_id| {
                self.structure_trait_assertion(structure, &trait_id)
                    .is_some()
            })
    }

    /// Classifies a pair by semantic graph traits, independent of compound IDs.
    #[must_use]
    pub fn reaction_family_candidates(
        &self,
        first: &StructureId,
        second: &StructureId,
    ) -> BTreeSet<ReactionFamilyCandidate> {
        let pair_has = |left: &str, right: &str| {
            (self.structure_has_reaction_trait(first, left)
                && self.structure_has_reaction_trait(second, right))
                || (self.structure_has_reaction_trait(second, left)
                    && self.structure_has_reaction_trait(first, right))
        };
        let mut result = BTreeSet::new();
        if pair_has("Traits.ElementalMetalReactant", "Traits.HalogenOxidant") {
            result.insert(ReactionFamilyCandidate::MetalHalogen);
        }
        if pair_has("Traits.ElementalMetalReactant", "Traits.WaterReactant") {
            result.insert(ReactionFamilyCandidate::GroupOneWater);
        }
        if pair_has(
            "Traits.ElementalMetalReactant",
            "Traits.BronstedAcidProtonDonor",
        ) {
            result.insert(ReactionFamilyCandidate::MetalAcid);
        }
        if pair_has("Traits.CombustibleFuel", "Traits.OxygenOxidant") {
            result.insert(ReactionFamilyCandidate::Combustion);
        }
        if pair_has("Traits.BronstedAcidProtonDonor", "Traits.HydroxideBase") {
            result.insert(ReactionFamilyCandidate::AcidBase);
        }
        if pair_has("Traits.BronstedAcidProtonDonor", "Traits.CarbonateBase") {
            result.insert(ReactionFamilyCandidate::CarbonateAcid);
        }
        if pair_has(
            "Traits.ElementalMetalReactant",
            "Traits.SolubleIonicReactant",
        ) || pair_has("Traits.HalogenOxidant", "Traits.SolubleIonicReactant")
        {
            result.insert(ReactionFamilyCandidate::Displacement);
        }
        if self.structure_has_reaction_trait(first, "Traits.SolubleIonicReactant")
            && self.structure_has_reaction_trait(second, "Traits.SolubleIonicReactant")
        {
            result.insert(ReactionFamilyCandidate::Precipitation);
        }
        result
    }
}
