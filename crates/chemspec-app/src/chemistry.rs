//! Application boundary for host-pinned trusted chemistry experiences.
//!
//! The UI may identify an exact supported draft, but every product, bond,
//! observation, and frame below is produced by the language and kernel crates.

use std::{collections::BTreeMap, str::FromStr, sync::LazyLock};

use chem_catalogue::{
    GeneralizedCaseSelection, GeneralizedReactionCaseRecord, ObservationPredicate, OxygenOutcome,
    TrustedCatalogue, ValidatedOxygenScreening,
};
use chem_domain::ReactionRuleId;
use chem_kernel::{
    CurrentArtifactIdentity, ObservationStatus, SimulationFrames, expand_trusted, generate_frames,
    validate_trusted,
};
use chem_presentation::{
    AppearanceProfile, AssetProfile, CameraBehaviour, CameraCue, EffectIntensity, EffectProfile,
    FlamePalette, MacroscopicMaterial, MacroscopicMaterialRole, MacroscopicReaction,
    ObjectObservationBinding, PresentationColourTransition, PresentationEffect, PresentationObject,
    PresentationProfile, PresentationTransform, SceneRole, VIRTUAL_ONLY_DISCLOSURE,
    compile_phase_driven_profile, visual_colour,
};

use crate::composition_catalogue::{self, CompositionId};

const CATALOGUE: &[u8] = include_bytes!("../../../catalogue/trusted/core-chemistry/catalogue.json");

const ALKALI_WATER_EVIDENCE: &[u8] =
    include_bytes!("../../../catalogue/candidates/periodic-table-and-alkali-water/evidence.json");
const PRECIPITATION_EVIDENCE: &[u8] =
    include_bytes!("../../../catalogue/candidates/precipitation-silver-halide/evidence.json");
const NEUTRALIZATION_EVIDENCE: &[u8] =
    include_bytes!("../../../catalogue/candidates/acid-base-neutralization/evidence.json");
const GAS_EVOLUTION_EVIDENCE: &[u8] =
    include_bytes!("../../../catalogue/candidates/acid-carbonate-gas-evolution/evidence.json");
const HALOGEN_DISPLACEMENT_EVIDENCE: &[u8] =
    include_bytes!("../../../catalogue/candidates/single-displacement-halogen/evidence.json");
const OXYGEN_SCREENING: &[u8] = include_bytes!("../../../catalogue/oxygen-screening/oxygen.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlkaliMetal {
    Lithium,
    Sodium,
    Potassium,
}

impl AlkaliMetal {
    const fn name(self) -> &'static str {
        match self {
            Self::Lithium => "Lithium",
            Self::Sodium => "Sodium",
            Self::Potassium => "Potassium",
        }
    }

    const fn lower_name(self) -> &'static str {
        match self {
            Self::Lithium => "lithium",
            Self::Sodium => "sodium",
            Self::Potassium => "potassium",
        }
    }

    const fn symbol(self) -> &'static str {
        match self {
            Self::Lithium => "Li",
            Self::Sodium => "Na",
            Self::Potassium => "K",
        }
    }

    const fn atomic_number(self) -> u8 {
        match self {
            Self::Lithium => 3,
            Self::Sodium => 11,
            Self::Potassium => 19,
        }
    }

    const fn hydroxide(self) -> CompositionId {
        match self {
            Self::Lithium => CompositionId::LithiumHydroxide,
            Self::Sodium => CompositionId::SodiumHydroxide,
            Self::Potassium => CompositionId::PotassiumHydroxide,
        }
    }

    const fn carbonate(self) -> CompositionId {
        match self {
            Self::Lithium => CompositionId::LithiumCarbonate,
            Self::Sodium => CompositionId::SodiumCarbonate,
            Self::Potassium => CompositionId::PotassiumCarbonate,
        }
    }

    const fn bicarbonate(self) -> CompositionId {
        match self {
            Self::Lithium => CompositionId::LithiumBicarbonate,
            Self::Sodium => CompositionId::SodiumBicarbonate,
            Self::Potassium => CompositionId::PotassiumBicarbonate,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AlkaliWaterVisualEvidence {
    flame: Option<(FlamePalette, EffectIntensity)>,
}

/// Reviewed qualitative presentation metadata for the alkali-water family.
///
/// The Royal Society of Chemistry's classroom observation table describes
/// lithium as fizzing and potassium as self-igniting with a lilac flame. This
/// metadata remains upstream of the generic renderer and does not alter the
/// trusted reaction frames.
/// <https://edu.rsc.org/download?ac=512063>
const fn alkali_water_visual_evidence(metal: AlkaliMetal) -> AlkaliWaterVisualEvidence {
    match metal {
        AlkaliMetal::Potassium => AlkaliWaterVisualEvidence {
            flame: Some((FlamePalette::Lilac, EffectIntensity::Strong)),
        },
        AlkaliMetal::Lithium | AlkaliMetal::Sodium => AlkaliWaterVisualEvidence { flame: None },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Halogen {
    Chlorine,
    Bromine,
    Iodine,
}

impl Halogen {
    const fn name(self) -> &'static str {
        match self {
            Self::Chlorine => "Chlorine",
            Self::Bromine => "Bromine",
            Self::Iodine => "Iodine",
        }
    }

    const fn halide_name(self) -> &'static str {
        match self {
            Self::Chlorine => "Chloride",
            Self::Bromine => "Bromide",
            Self::Iodine => "Iodide",
        }
    }

    const fn symbol(self) -> &'static str {
        match self {
            Self::Chlorine => "Cl",
            Self::Bromine => "Br",
            Self::Iodine => "I",
        }
    }

    const fn precipitation_observation(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::Chlorine => ("R1", "White", "R2"),
            Self::Bromine => ("R3", "Cream", "R4"),
            Self::Iodine => ("R5", "Yellow", "R6"),
        }
    }

    const fn hydrogen_halide(self) -> CompositionId {
        match self {
            Self::Chlorine => CompositionId::HydrogenChloride,
            Self::Bromine => CompositionId::HydrogenBromide,
            Self::Iodine => CompositionId::HydrogenIodide,
        }
    }

    const fn sodium_halide(self) -> CompositionId {
        match self {
            Self::Chlorine => CompositionId::SodiumChloride,
            Self::Bromine => CompositionId::SodiumBromide,
            Self::Iodine => CompositionId::SodiumIodide,
        }
    }

    const fn molecule(self) -> CompositionId {
        match self {
            Self::Chlorine => CompositionId::Chlorine,
            Self::Bromine => CompositionId::Bromine,
            Self::Iodine => CompositionId::Iodine,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HalideElement {
    Fluorine,
    Chlorine,
    Bromine,
    Iodine,
}

impl HalideElement {
    const fn symbol(self) -> &'static str {
        match self {
            Self::Fluorine => "F",
            Self::Chlorine => "Cl",
            Self::Bromine => "Br",
            Self::Iodine => "I",
        }
    }

    const fn from_molecule(id: CompositionId) -> Option<Self> {
        match id {
            CompositionId::Fluorine => Some(Self::Fluorine),
            CompositionId::Chlorine => Some(Self::Chlorine),
            CompositionId::Bromine => Some(Self::Bromine),
            CompositionId::Iodine => Some(Self::Iodine),
            _ => None,
        }
    }

    const fn from_sodium_halide(id: CompositionId) -> Option<Self> {
        match id {
            CompositionId::SodiumFluoride => Some(Self::Fluorine),
            CompositionId::SodiumChloride => Some(Self::Chlorine),
            CompositionId::SodiumBromide => Some(Self::Bromine),
            CompositionId::SodiumIodide => Some(Self::Iodine),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnsupportedAcidFamily {
    Hydroxide,
    Bicarbonate,
    Carbonate,
}

impl UnsupportedAcidFamily {
    const fn rule_id(self) -> &'static str {
        match self {
            Self::Hydroxide => "Rules.MonoproticAcidHydroxideNeutralization",
            Self::Bicarbonate => "Rules.MonoproticAcidBicarbonateGasEvolution",
            Self::Carbonate => "Rules.DiproticAcidCarbonateGasEvolution",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnsupportedRequest {
    SilverFluoride,
    HydrofluoricAcid {
        family: UnsupportedAcidFamily,
        metal: AlkaliMetal,
    },
    HalogenDisplacement {
        displacing: HalideElement,
        displaced: HalideElement,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsupportedCase {
    pub id: String,
    pub required_feature: String,
    pub explanation: String,
}

impl UnsupportedRequest {
    fn from_participants(participants: [DraftParticipant; 2]) -> Option<Self> {
        let [
            DraftParticipant::Composition(first),
            DraftParticipant::Composition(second),
        ] = participants
        else {
            return None;
        };
        let pair = [first, second];
        let has = |id| pair.contains(&id);

        if has(CompositionId::SilverNitrate) && has(CompositionId::SodiumFluoride) {
            return Some(Self::SilverFluoride);
        }

        if has(CompositionId::HydrogenFluoride) {
            let other = if first == CompositionId::HydrogenFluoride {
                second
            } else {
                first
            };
            let (family, metal) = match other {
                CompositionId::LithiumHydroxide => {
                    (UnsupportedAcidFamily::Hydroxide, AlkaliMetal::Lithium)
                }
                CompositionId::SodiumHydroxide => {
                    (UnsupportedAcidFamily::Hydroxide, AlkaliMetal::Sodium)
                }
                CompositionId::PotassiumHydroxide => {
                    (UnsupportedAcidFamily::Hydroxide, AlkaliMetal::Potassium)
                }
                CompositionId::LithiumBicarbonate => {
                    (UnsupportedAcidFamily::Bicarbonate, AlkaliMetal::Lithium)
                }
                CompositionId::SodiumBicarbonate => {
                    (UnsupportedAcidFamily::Bicarbonate, AlkaliMetal::Sodium)
                }
                CompositionId::PotassiumBicarbonate => {
                    (UnsupportedAcidFamily::Bicarbonate, AlkaliMetal::Potassium)
                }
                CompositionId::LithiumCarbonate => {
                    (UnsupportedAcidFamily::Carbonate, AlkaliMetal::Lithium)
                }
                CompositionId::SodiumCarbonate => {
                    (UnsupportedAcidFamily::Carbonate, AlkaliMetal::Sodium)
                }
                CompositionId::PotassiumCarbonate => {
                    (UnsupportedAcidFamily::Carbonate, AlkaliMetal::Potassium)
                }
                _ => return None,
            };
            return Some(Self::HydrofluoricAcid { family, metal });
        }

        let displacing = pair
            .iter()
            .copied()
            .find_map(HalideElement::from_molecule)?;
        let displaced = pair
            .iter()
            .copied()
            .find_map(HalideElement::from_sodium_halide)?;
        Some(Self::HalogenDisplacement {
            displacing,
            displaced,
        })
    }

    fn catalogue_case(self) -> Result<UnsupportedCase, String> {
        let (rule, binding) = match self {
            Self::SilverFluoride => (
                "Rules.SilverHalidePrecipitation",
                BTreeMap::from([("halide".to_owned(), "F".to_owned())]),
            ),
            Self::HydrofluoricAcid { family, metal } => (
                family.rule_id(),
                BTreeMap::from([
                    ("halide".to_owned(), "F".to_owned()),
                    ("member".to_owned(), metal.symbol().to_owned()),
                ]),
            ),
            Self::HalogenDisplacement {
                displacing,
                displaced,
            } => (
                "Rules.HalogenDisplacement",
                BTreeMap::from([
                    ("displacing".to_owned(), displacing.symbol().to_owned()),
                    ("displaced".to_owned(), displaced.symbol().to_owned()),
                ]),
            ),
        };
        let rule_id = ReactionRuleId::from_str(rule).map_err(|error| error.to_string())?;
        let catalogue = TRUSTED_CATALOGUE.as_ref().map_err(String::as_str)?;
        let selection = catalogue
            .select_generalized_case(&rule_id, &binding)
            .map_err(|error| error.to_string())?;
        let Some(GeneralizedCaseSelection::Unsupported(
            GeneralizedReactionCaseRecord::Unsupported {
                id,
                required_feature,
                explanation,
                ..
            },
        )) = selection
        else {
            return Err(format!(
                "trusted catalogue did not select an unsupported case for `{rule_id}`"
            ));
        };
        Ok(UnsupportedCase {
            id: id.clone(),
            required_feature: required_feature.clone(),
            explanation: explanation.clone(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReactionFamily {
    AlkaliWater,
    SilverHalidePrecipitation,
    AcidBaseNeutralization,
    AcidBicarbonateGasEvolution,
    AcidCarbonateGasEvolution,
    HalogenDisplacement,
    Oxygen,
    FixedChargeIonPair,
    CovalentCombination,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum ExperienceParticipantDefinition {
    Element(u8),
    Composition(&'static str),
}

struct ExperienceDefinition {
    id: &'static str,
    family: ReactionFamily,
    participants: [ExperienceParticipantDefinition; 2],
    source_name: &'static str,
    source: &'static str,
    evidence: &'static str,
    equation: &'static str,
    subject_name: &'static str,
    product_name: Option<&'static str>,
    product_structure: Option<&'static str>,
}

include!(concat!(env!("OUT_DIR"), "/experience_registry.rs"));

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReactionKind {
    AlkaliWater {
        metal: AlkaliMetal,
    },
    SilverHalidePrecipitation {
        halogen: Halogen,
    },
    AcidBaseNeutralization {
        metal: AlkaliMetal,
        halogen: Halogen,
    },
    AcidBicarbonateGasEvolution {
        metal: AlkaliMetal,
        halogen: Halogen,
    },
    AcidCarbonateGasEvolution {
        metal: AlkaliMetal,
        halogen: Halogen,
    },
    HalogenDisplacement {
        displacing: Halogen,
        displaced: Halogen,
    },
    Registry {
        index: usize,
    },
}

/// A supported finite request. Private construction prevents unsupported
/// category bindings from masquerading as runnable chemistry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReactionRequest {
    kind: ReactionKind,
}

impl ReactionRequest {
    pub const DEFAULT: Self = Self::alkali_water(AlkaliMetal::Lithium);
    pub const ALL: [Self; 36] = [
        Self::alkali_water(AlkaliMetal::Lithium),
        Self::alkali_water(AlkaliMetal::Sodium),
        Self::alkali_water(AlkaliMetal::Potassium),
        Self::silver_halide_precipitation(Halogen::Chlorine),
        Self::silver_halide_precipitation(Halogen::Bromine),
        Self::silver_halide_precipitation(Halogen::Iodine),
        Self::acid_base_neutralization(AlkaliMetal::Lithium, Halogen::Chlorine),
        Self::acid_base_neutralization(AlkaliMetal::Lithium, Halogen::Bromine),
        Self::acid_base_neutralization(AlkaliMetal::Lithium, Halogen::Iodine),
        Self::acid_base_neutralization(AlkaliMetal::Sodium, Halogen::Chlorine),
        Self::acid_base_neutralization(AlkaliMetal::Sodium, Halogen::Bromine),
        Self::acid_base_neutralization(AlkaliMetal::Sodium, Halogen::Iodine),
        Self::acid_base_neutralization(AlkaliMetal::Potassium, Halogen::Chlorine),
        Self::acid_base_neutralization(AlkaliMetal::Potassium, Halogen::Bromine),
        Self::acid_base_neutralization(AlkaliMetal::Potassium, Halogen::Iodine),
        Self::acid_bicarbonate_gas_evolution(AlkaliMetal::Lithium, Halogen::Chlorine),
        Self::acid_bicarbonate_gas_evolution(AlkaliMetal::Lithium, Halogen::Bromine),
        Self::acid_bicarbonate_gas_evolution(AlkaliMetal::Lithium, Halogen::Iodine),
        Self::acid_bicarbonate_gas_evolution(AlkaliMetal::Sodium, Halogen::Chlorine),
        Self::acid_bicarbonate_gas_evolution(AlkaliMetal::Sodium, Halogen::Bromine),
        Self::acid_bicarbonate_gas_evolution(AlkaliMetal::Sodium, Halogen::Iodine),
        Self::acid_bicarbonate_gas_evolution(AlkaliMetal::Potassium, Halogen::Chlorine),
        Self::acid_bicarbonate_gas_evolution(AlkaliMetal::Potassium, Halogen::Bromine),
        Self::acid_bicarbonate_gas_evolution(AlkaliMetal::Potassium, Halogen::Iodine),
        Self::acid_carbonate_gas_evolution(AlkaliMetal::Lithium, Halogen::Chlorine),
        Self::acid_carbonate_gas_evolution(AlkaliMetal::Lithium, Halogen::Bromine),
        Self::acid_carbonate_gas_evolution(AlkaliMetal::Lithium, Halogen::Iodine),
        Self::acid_carbonate_gas_evolution(AlkaliMetal::Sodium, Halogen::Chlorine),
        Self::acid_carbonate_gas_evolution(AlkaliMetal::Sodium, Halogen::Bromine),
        Self::acid_carbonate_gas_evolution(AlkaliMetal::Sodium, Halogen::Iodine),
        Self::acid_carbonate_gas_evolution(AlkaliMetal::Potassium, Halogen::Chlorine),
        Self::acid_carbonate_gas_evolution(AlkaliMetal::Potassium, Halogen::Bromine),
        Self::acid_carbonate_gas_evolution(AlkaliMetal::Potassium, Halogen::Iodine),
        Self::halogen_displacement_unchecked(Halogen::Chlorine, Halogen::Bromine),
        Self::halogen_displacement_unchecked(Halogen::Chlorine, Halogen::Iodine),
        Self::halogen_displacement_unchecked(Halogen::Bromine, Halogen::Iodine),
    ];

    #[must_use]
    pub const fn alkali_water(metal: AlkaliMetal) -> Self {
        Self {
            kind: ReactionKind::AlkaliWater { metal },
        }
    }

    #[must_use]
    pub const fn silver_halide_precipitation(halogen: Halogen) -> Self {
        Self {
            kind: ReactionKind::SilverHalidePrecipitation { halogen },
        }
    }

    #[must_use]
    pub const fn acid_base_neutralization(metal: AlkaliMetal, halogen: Halogen) -> Self {
        Self {
            kind: ReactionKind::AcidBaseNeutralization { metal, halogen },
        }
    }

    #[must_use]
    pub const fn acid_bicarbonate_gas_evolution(metal: AlkaliMetal, halogen: Halogen) -> Self {
        Self {
            kind: ReactionKind::AcidBicarbonateGasEvolution { metal, halogen },
        }
    }

    #[must_use]
    pub const fn acid_carbonate_gas_evolution(metal: AlkaliMetal, halogen: Halogen) -> Self {
        Self {
            kind: ReactionKind::AcidCarbonateGasEvolution { metal, halogen },
        }
    }

    const fn halogen_displacement_unchecked(displacing: Halogen, displaced: Halogen) -> Self {
        Self {
            kind: ReactionKind::HalogenDisplacement {
                displacing,
                displaced,
            },
        }
    }

    const fn registry(index: usize) -> Self {
        Self {
            kind: ReactionKind::Registry { index },
        }
    }

    fn definition(self) -> Option<&'static ExperienceDefinition> {
        let ReactionKind::Registry { index } = self.kind else {
            return None;
        };
        EXPERIENCE_DEFINITIONS.get(index)
    }

    #[must_use]
    pub const fn family(self) -> ReactionFamily {
        match self.kind {
            ReactionKind::AlkaliWater { .. } => ReactionFamily::AlkaliWater,
            ReactionKind::SilverHalidePrecipitation { .. } => {
                ReactionFamily::SilverHalidePrecipitation
            }
            ReactionKind::AcidBaseNeutralization { .. } => ReactionFamily::AcidBaseNeutralization,
            ReactionKind::AcidBicarbonateGasEvolution { .. } => {
                ReactionFamily::AcidBicarbonateGasEvolution
            }
            ReactionKind::AcidCarbonateGasEvolution { .. } => {
                ReactionFamily::AcidCarbonateGasEvolution
            }
            ReactionKind::HalogenDisplacement { .. } => ReactionFamily::HalogenDisplacement,
            ReactionKind::Registry { index } => EXPERIENCE_DEFINITIONS[index].family,
        }
    }

    #[must_use]
    pub fn id(self) -> String {
        match self.kind {
            ReactionKind::AlkaliWater { metal } => {
                format!("alkali-water-{}", metal.lower_name())
            }
            ReactionKind::SilverHalidePrecipitation { halogen } => format!(
                "silver-halide-precipitation-{}",
                halogen.halide_name().to_lowercase()
            ),
            ReactionKind::AcidBaseNeutralization { metal, halogen } => format!(
                "acid-base-{}-{}",
                metal.lower_name(),
                halogen.halide_name().to_lowercase()
            ),
            ReactionKind::AcidBicarbonateGasEvolution { metal, halogen } => format!(
                "acid-bicarbonate-{}-{}",
                metal.lower_name(),
                halogen.halide_name().to_lowercase()
            ),
            ReactionKind::AcidCarbonateGasEvolution { metal, halogen } => format!(
                "acid-carbonate-{}-{}",
                metal.lower_name(),
                halogen.halide_name().to_lowercase()
            ),
            ReactionKind::HalogenDisplacement {
                displacing,
                displaced,
            } => format!(
                "halogen-displacement-{}-{}",
                displacing.name().to_lowercase(),
                displaced.halide_name().to_lowercase()
            ),
            ReactionKind::Registry { index } => EXPERIENCE_DEFINITIONS[index].id.to_owned(),
        }
    }

    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        requests().find(|request| request.id() == id)
    }

    #[must_use]
    pub fn source_name(self) -> String {
        self.definition().map_or_else(
            || format!("generated/{}.chems", self.id()),
            |definition| definition.source_name.to_owned(),
        )
    }

    #[must_use]
    pub fn equation(self) -> String {
        match self.kind {
            ReactionKind::AlkaliWater { metal } => {
                format!("2{} + 2H₂O  →  2{}OH + H₂", metal.symbol(), metal.symbol())
            }
            ReactionKind::SilverHalidePrecipitation { halogen } => format!(
                "AgNO₃ + Na{}  →  Ag{} + NaNO₃",
                halogen.symbol(),
                halogen.symbol()
            ),
            ReactionKind::AcidBaseNeutralization { metal, halogen } => format!(
                "H{} + {}OH  →  {}{} + H₂O",
                halogen.symbol(),
                metal.symbol(),
                metal.symbol(),
                halogen.symbol()
            ),
            ReactionKind::AcidBicarbonateGasEvolution { metal, halogen } => format!(
                "H{} + {}HCO₃  →  {}{} + H₂O + CO₂",
                halogen.symbol(),
                metal.symbol(),
                metal.symbol(),
                halogen.symbol()
            ),
            ReactionKind::AcidCarbonateGasEvolution { metal, halogen } => format!(
                "2H{} + {}₂CO₃  →  2{}{} + H₂O + CO₂",
                halogen.symbol(),
                metal.symbol(),
                metal.symbol(),
                halogen.symbol()
            ),
            ReactionKind::HalogenDisplacement {
                displacing,
                displaced,
            } => format!(
                "{}₂ + 2Na{}  →  2Na{} + {}₂",
                displacing.symbol(),
                displaced.symbol(),
                displacing.symbol(),
                displaced.symbol()
            ),
            ReactionKind::Registry { index } => EXPERIENCE_DEFINITIONS[index].equation.to_owned(),
        }
    }

    #[must_use]
    pub fn name(self) -> String {
        if let Some(name) = self
            .definition()
            .and_then(|definition| definition.product_name)
        {
            return name.to_owned();
        }
        run(self).map_or_else(
            |_| {
                self.definition().map_or_else(
                    || "reaction products".to_owned(),
                    |definition| format!("{} reaction product", definition.subject_name),
                )
            },
            |run| crate::nomenclature::product_names(run.frames()),
        )
    }

    fn evidence(self) -> &'static [u8] {
        match self.family() {
            ReactionFamily::AlkaliWater => ALKALI_WATER_EVIDENCE,
            ReactionFamily::SilverHalidePrecipitation => PRECIPITATION_EVIDENCE,
            ReactionFamily::AcidBaseNeutralization => NEUTRALIZATION_EVIDENCE,
            ReactionFamily::AcidBicarbonateGasEvolution
            | ReactionFamily::AcidCarbonateGasEvolution => GAS_EVOLUTION_EVIDENCE,
            ReactionFamily::HalogenDisplacement => HALOGEN_DISPLACEMENT_EVIDENCE,
            ReactionFamily::Oxygen
            | ReactionFamily::FixedChargeIonPair
            | ReactionFamily::CovalentCombination => self
                .definition()
                .expect("registry family has a generated definition")
                .evidence
                .as_bytes(),
        }
    }

    fn legacy_participants(self) -> Option<[DraftParticipant; 2]> {
        match self.kind {
            ReactionKind::AlkaliWater { metal } => Some([
                DraftParticipant::Atom(metal.atomic_number()),
                DraftParticipant::Composition(CompositionId::Water),
            ]),
            ReactionKind::SilverHalidePrecipitation { halogen } => Some([
                DraftParticipant::Composition(CompositionId::SilverNitrate),
                DraftParticipant::Composition(halogen.sodium_halide()),
            ]),
            ReactionKind::AcidBaseNeutralization { metal, halogen } => Some([
                DraftParticipant::Composition(halogen.hydrogen_halide()),
                DraftParticipant::Composition(metal.hydroxide()),
            ]),
            ReactionKind::AcidBicarbonateGasEvolution { metal, halogen } => Some([
                DraftParticipant::Composition(halogen.hydrogen_halide()),
                DraftParticipant::Composition(metal.bicarbonate()),
            ]),
            ReactionKind::AcidCarbonateGasEvolution { metal, halogen } => Some([
                DraftParticipant::Composition(halogen.hydrogen_halide()),
                DraftParticipant::Composition(metal.carbonate()),
            ]),
            ReactionKind::HalogenDisplacement {
                displacing,
                displaced,
            } => Some([
                DraftParticipant::Composition(displacing.molecule()),
                DraftParticipant::Composition(displaced.sodium_halide()),
            ]),
            ReactionKind::Registry { .. } => None,
        }
    }

    #[must_use]
    pub fn source(self) -> String {
        match self.kind {
            ReactionKind::AlkaliWater { metal } => alkali_water_source(metal),
            ReactionKind::SilverHalidePrecipitation { halogen } => precipitation_source(halogen),
            ReactionKind::AcidBaseNeutralization { metal, halogen } => {
                neutralization_source(metal, halogen)
            }
            ReactionKind::AcidBicarbonateGasEvolution { metal, halogen } => {
                gas_evolution_source(metal, halogen, false)
            }
            ReactionKind::AcidCarbonateGasEvolution { metal, halogen } => {
                gas_evolution_source(metal, halogen, true)
            }
            ReactionKind::HalogenDisplacement {
                displacing,
                displaced,
            } => halogen_displacement_source(displacing, displaced),
            ReactionKind::Registry { index } => EXPERIENCE_DEFINITIONS[index].source.to_owned(),
        }
    }

    /// Returns the exact reviewed product graph named by a registry experience.
    /// Formula-only lookup is intentionally not used here: several formulae can
    /// represent more than one structure.
    #[must_use]
    pub fn product_preview(self) -> Option<composition_catalogue::TrustedCompositionPreview> {
        let structure = self.definition()?.product_structure?;
        composition_catalogue::trusted_preview_by_structure_id(structure)
    }

    /// Exact reviewed reactant graphs used only to identify particles in the
    /// 3D presentation. The models are projected from the same host-pinned
    /// catalogue as the product preview and never participate in validation.
    #[must_use]
    pub fn reactant_previews(self) -> Vec<composition_catalogue::TrustedCompositionPreview> {
        if let Some(definition) = self.definition() {
            return definition
                .participants
                .into_iter()
                .filter_map(trusted_participant_preview)
                .collect();
        }
        self.legacy_participants()
            .into_iter()
            .flatten()
            .filter_map(trusted_draft_participant_preview)
            .collect()
    }
}

fn trusted_participant_preview(
    participant: ExperienceParticipantDefinition,
) -> Option<composition_catalogue::TrustedCompositionPreview> {
    match participant {
        ExperienceParticipantDefinition::Element(atomic_number) => {
            composition_catalogue::trusted_preview(standardize_elemental_draft(&[atomic_number]))
        }
        ExperienceParticipantDefinition::Composition(formula) => composition_catalogue::SUPPORTED
            .iter()
            .find(|preview| preview.formula == formula)
            .and_then(|preview| trusted_composition_preview(*preview)),
    }
}

fn trusted_draft_participant_preview(
    participant: DraftParticipant,
) -> Option<composition_catalogue::TrustedCompositionPreview> {
    match participant {
        DraftParticipant::Atom(atomic_number) => {
            composition_catalogue::trusted_preview(standardize_elemental_draft(&[atomic_number]))
        }
        DraftParticipant::Composition(id) => composition_catalogue::SUPPORTED
            .iter()
            .find(|preview| preview.id == id)
            .and_then(|preview| trusted_composition_preview(*preview)),
    }
}

fn trusted_composition_preview(
    preview: composition_catalogue::CompositionPreview,
) -> Option<composition_catalogue::TrustedCompositionPreview> {
    composition_catalogue::trusted_preview(preview.atoms.iter().flat_map(
        |(atomic_number, count)| std::iter::repeat_n(*atomic_number, usize::from(*count)),
    ))
}

pub fn requests() -> impl Iterator<Item = ReactionRequest> {
    ReactionRequest::ALL
        .into_iter()
        .chain((0..EXPERIENCE_DEFINITIONS.len()).map(ReactionRequest::registry))
}

fn alkali_water_source(metal: AlkaliMetal) -> String {
    format!(
        "chems 1\nuse catalog ChemSpec.Theoretical@1\nreaction {name}AndWater where\n  reactants\n    metal := 2 of {name}Metal\n    water := 2 of Water\n  products\n    hydroxide := 2 of {name}Hydroxide\n    hydrogen := 1 of Hydrogen\n  equation\n    2 {symbol}[metallic] + 2 H2O[molecular]\n    -> 2 {symbol}OH[ionic] + H2[molecular]\n  model\n    event := representative\n    sequence := explanatory\n  observe from Evidence.AlkaliWater@1\n    gas hydrogen evolves claim R1\n    reactant metal disappears claim R2\n  by\n    apply Rules.AlkaliMetalWithWater\n      metal := metal\n      water := water\n      hydroxide := hydroxide\n      gasProduct := hydrogen\n",
        name = metal.name(),
        symbol = metal.symbol()
    )
}

fn precipitation_source(halogen: Halogen) -> String {
    let (forms_claim, colour, colour_claim) = halogen.precipitation_observation();
    format!(
        "chems 1\nuse catalog ChemSpec.Theoretical@1\nreaction SilverNitrateAndSodium{halide} where\n  reactants\n    silverNitrate := 1 of SilverNitrate\n    sodiumHalide := 1 of Sodium{halide}\n  products\n    silverHalide := 1 of Silver{halide}\n    sodiumNitrate := 1 of SodiumNitrate\n  equation\n    AgNO3[ionic] + Na{symbol}[ionic]\n    -> Ag{symbol}[ionic] + NaNO3[ionic]\n  model\n    event := representative\n    sequence := explanatory\n  observe from Evidence.SilverHalidePrecipitation@1\n    product silverHalide forms claim {forms_claim}\n    product silverHalide has colour {colour} claim {colour_claim}\n  by\n    apply Rules.SilverHalidePrecipitation\n      silverSource := silverNitrate\n      halideSource := sodiumHalide\n      precipitate := silverHalide\n      spectatorSalt := sodiumNitrate\n",
        halide = halogen.halide_name(),
        symbol = halogen.symbol()
    )
}

fn neutralization_source(metal: AlkaliMetal, halogen: Halogen) -> String {
    format!(
        "chems 1\nuse catalog ChemSpec.Theoretical@1\nreaction AcidBase{name}{halide} where\n  reactants\n    acid := 1 of Hydrogen{halide}\n    base := 1 of {name}Hydroxide\n  products\n    salt := 1 of {name}{halide}\n    water := 1 of Water\n  equation\n    H{symbol}[molecular] + {metal_symbol}OH[ionic]\n    -> {metal_symbol}{symbol}[ionic] + H2O[molecular]\n  model\n    event := representative\n    sequence := explanatory\n  observe from Evidence.AcidBaseNeutralization@1\n    reactant acid disappears claim R1\n    product water forms claim R2\n  by\n    apply Rules.MonoproticAcidHydroxideNeutralization\n      acid := acid\n      base := base\n      saltProduct := salt\n      waterProduct := water\n",
        name = metal.name(),
        halide = halogen.halide_name(),
        symbol = halogen.symbol(),
        metal_symbol = metal.symbol()
    )
}

fn gas_evolution_source(metal: AlkaliMetal, halogen: Halogen, carbonate: bool) -> String {
    let (acid_coefficient, source_name, source_formula, salt_coefficient, rule, source_role) =
        if carbonate {
            (
                2,
                format!("{}Carbonate", metal.name()),
                format!("{}2CO3", metal.symbol()),
                2,
                "Rules.DiproticAcidCarbonateGasEvolution",
                "carbonateSource",
            )
        } else {
            (
                1,
                format!("{}Bicarbonate", metal.name()),
                format!("{}HCO3", metal.symbol()),
                1,
                "Rules.MonoproticAcidBicarbonateGasEvolution",
                "bicarbonateSource",
            )
        };
    format!(
        "chems 1\nuse catalog ChemSpec.Theoretical@1\nreaction GasEvolution{name}{halide}{source_role} where\n  reactants\n    acid := {acid_coefficient} of Hydrogen{halide}\n    carbonateSalt := 1 of {source_name}\n  products\n    carbonDioxide := 1 of CarbonDioxide\n    water := 1 of Water\n    salt := {salt_coefficient} of {name}{halide}\n  equation\n    {acid_coefficient} H{symbol}[molecular] + {source_formula}[ionic]\n    -> {salt_coefficient} {metal_symbol}{symbol}[ionic] + H2O[molecular] + CO2[molecular]\n  model\n    event := representative\n    sequence := explanatory\n  observe from Evidence.AcidCarbonateGasEvolution@1\n    gas carbonDioxide evolves claim R1\n    reactant acid disappears claim R2\n  by\n    apply {rule}\n      acid := acid\n      {source_role} := carbonateSalt\n      gasProduct := carbonDioxide\n      waterProduct := water\n      saltProduct := salt\n",
        name = metal.name(),
        halide = halogen.halide_name(),
        symbol = halogen.symbol(),
        metal_symbol = metal.symbol()
    )
}

fn halogen_displacement_source(displacing: Halogen, displaced: Halogen) -> String {
    format!(
        "chems 1\nuse catalog ChemSpec.Theoretical@1\nreaction {displacing_name}Displaces{displaced_name} where\n  reactants\n    displacingHalogen := 1 of {displacing_name}\n    saltSource := 2 of Sodium{displaced_halide}\n  products\n    newSalt := 2 of Sodium{displacing_halide}\n    displacedHalogen := 1 of {displaced_name}\n  equation\n    {displacing_symbol}2[molecular] + 2 Na{displaced_symbol}[ionic]\n    -> 2 Na{displacing_symbol}[ionic] + {displaced_symbol}2[molecular]\n  model\n    event := representative\n    sequence := explanatory\n  observe from Evidence.HalogenDisplacement@1\n    product displacedHalogen forms claim R1\n  by\n    apply Rules.HalogenDisplacement\n      displacingHalogen := displacingHalogen\n      saltSource := saltSource\n      newSalt := newSalt\n      displacedHalogen := displacedHalogen\n",
        displacing_name = displacing.name(),
        displaced_name = displaced.name(),
        displaced_halide = displaced.halide_name(),
        displacing_halide = displacing.halide_name(),
        displacing_symbol = displacing.symbol(),
        displaced_symbol = displaced.symbol()
    )
}

#[derive(Debug, Clone)]
pub struct TrustedRun {
    frames: SimulationFrames,
    macroscopic: Option<MacroscopicReaction>,
    declaration: chem_domain::ReactionDeclaration,
}

impl TrustedRun {
    #[must_use]
    pub const fn frames(&self) -> &SimulationFrames {
        &self.frames
    }

    /// Catalogue-resolved material phases for the generic presentation
    /// compiler. `None` means the trusted catalogue predates those optional
    /// records; it never means that a phase was guessed.
    #[must_use]
    pub const fn macroscopic(&self) -> Option<&MacroscopicReaction> {
        self.macroscopic.as_ref()
    }

    #[must_use]
    pub const fn declaration(&self) -> &chem_domain::ReactionDeclaration {
        &self.declaration
    }
}

#[derive(Debug)]
struct ValidatedRequestArtifacts {
    frames: SimulationFrames,
    macroscopic: Option<MacroscopicReaction>,
    declaration: chem_domain::ReactionDeclaration,
}

static TRUSTED_CATALOGUE: LazyLock<Result<TrustedCatalogue, String>> = LazyLock::new(|| {
    TrustedCatalogue::from_canonical_json(CATALOGUE).map_err(|error| error.to_string())
});

pub(crate) fn trusted_catalogue() -> Result<&'static TrustedCatalogue, &'static str> {
    TRUSTED_CATALOGUE.as_ref().map_err(String::as_str)
}

static VALIDATED_OXYGEN_SCREENING: LazyLock<Result<ValidatedOxygenScreening, String>> =
    LazyLock::new(|| {
        let catalogue = TRUSTED_CATALOGUE.as_ref().map_err(Clone::clone)?;
        ValidatedOxygenScreening::from_json(OXYGEN_SCREENING, catalogue)
            .map_err(|error| error.to_string())
    });
/// Returns a host-pinned, AI-reviewed experience result.
///
/// The returned frame type cannot be constructed by the application. Failure
/// is retained and shown honestly instead of falling back to UI-authored chemistry.
pub fn run(request: ReactionRequest) -> Result<TrustedRun, String> {
    build_run(request)
}

fn build_run(request: ReactionRequest) -> Result<TrustedRun, String> {
    let validated = validate_request_source(request, &request.source())?;
    Ok(TrustedRun {
        frames: validated.frames,
        macroscopic: validated.macroscopic,
        declaration: validated.declaration,
    })
}

/// Parses, expands, validates, and projects source against the exact host-pinned
/// catalogue and the evidence packet for the selected experience.
fn validate_request_source(
    request: ReactionRequest,
    source: &str,
) -> Result<ValidatedRequestArtifacts, String> {
    if source != request.source() {
        return Err(format!(
            "request/source identity mismatch for `{}`",
            request.id()
        ));
    }
    let catalogue = TRUSTED_CATALOGUE.as_ref().map_err(String::as_str)?;
    let expanded = expand_trusted(
        &request.source_name(),
        source,
        catalogue,
        request.evidence(),
    )
    .map_err(|error| error.to_string())?;
    let macroscopic = catalogue_macroscopic_reaction(request, &expanded, catalogue);
    let declaration = expanded.claim.declaration.clone();
    let current =
        CurrentArtifactIdentity::from_expanded(&expanded).map_err(|error| error.to_string())?;
    let validated = validate_trusted(&expanded, catalogue).map_err(|error| error.to_string())?;
    let frames = generate_frames(&validated, current).map_err(|error| error.to_string())?;
    Ok(ValidatedRequestArtifacts {
        frames,
        macroscopic,
        declaration,
    })
}

fn catalogue_macroscopic_reaction(
    request: ReactionRequest,
    expanded: &chem_kernel::ExpandedStructuralReaction,
    catalogue: &TrustedCatalogue,
) -> Option<MacroscopicReaction> {
    let rule = &expanded.claim.rule.rule;
    let resolve = |binding: &str,
                   resolved: &chem_kernel::ResolvedStructureBinding,
                   role: MacroscopicMaterialRole| {
        let rule_role = expanded
            .claim
            .rule
            .bindings
            .values()
            .find(|candidate| candidate.binding == binding)
            .map(|candidate| (rule, candidate.role.as_str()));
        catalogue
            .macroscopic_material(&resolved.structure, rule_role)
            .map(|record| MacroscopicMaterial {
                binding: binding.to_owned(),
                semantic_identity: resolved.name.clone(),
                role,
                phase: record.phase,
                representation: resolved.representation,
            })
    };
    let mut materials =
        Vec::with_capacity(expanded.claim.reactants.len() + expanded.claim.products.len());
    for (binding, material) in &expanded.claim.reactants {
        materials.push(resolve(
            binding,
            material,
            MacroscopicMaterialRole::Reactant,
        )?);
    }
    for (binding, material) in &expanded.claim.products {
        materials.push(resolve(
            binding,
            material,
            MacroscopicMaterialRole::Product,
        )?);
    }
    Some(MacroscopicReaction {
        profile_id: format!("presentation.catalogue.{}", request.id()),
        equation: request.equation(),
        materials,
        intensity: EffectIntensity::Moderate,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DraftParticipant {
    Atom(u8),
    Composition(CompositionId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DraftResolution {
    Supported(ReactionRequest),
    Multiple(Vec<ReactionRequest>),
    Screened(OxygenAssessment),
    ExplicitlyUnsupported(UnsupportedCase),
    Uncatalogued,
    Unrecognized,
    SystemError(String),
}

impl DraftResolution {
    #[must_use]
    pub fn inline_message(&self) -> Option<&str> {
        match self {
            Self::Multiple(_) => Some("Choose one reviewed product outcome."),
            Self::Screened(assessment) => Some(match &assessment.outcome {
                OxygenOutcome::Representative { .. } => {
                    "A representative outcome exists, but no reviewed structural simulation is available."
                }
                OxygenOutcome::NoDirectReaction { reason }
                | OxygenOutcome::Ambiguous { reason }
                | OxygenOutcome::Unsupported { reason } => reason,
            }),
            // Dynamic workflow copy belongs exclusively to the builder prompt
            // before launch and the modal after launch. Returning no inline
            // message makes a competing background status unrepresentable.
            Self::Supported(_)
            | Self::ExplicitlyUnsupported(_)
            | Self::Uncatalogued
            | Self::Unrecognized => None,
            Self::SystemError(_) => Some("The trusted chemistry catalogue is unavailable."),
        }
    }

    #[must_use]
    pub const fn is_system_error(&self) -> bool {
        matches!(self, Self::SystemError(_))
    }
}

/// Recognizes a supported input identity. This selects a request source; it
/// does not select products or construct chemistry.
pub fn request_for_participants(
    participants: impl IntoIterator<Item = DraftParticipant>,
) -> Option<ReactionRequest> {
    let mut actual = participants.into_iter().collect::<Vec<_>>();
    actual.sort_unstable();
    ReactionRequest::ALL.into_iter().find(|request| {
        let mut expected = request
            .legacy_participants()
            .expect("legacy requests have typed participants");
        expected.sort_unstable();
        actual == expected
    })
}

fn standard_state_count(atomic_number: u8) -> usize {
    match atomic_number {
        1 | 7 | 8 | 9 | 17 | 35 | 53 => 2,
        // Tetrahedral P4 and its arsenic analogue.
        15 | 33 => 4,
        16 => 8,
        _ => 1,
    }
}

/// Atomic numbers for a user-typed compound name or formula (`copper(II)
/// sulfate`, `CuSO4`, `oxygen`). None outside the nomenclature rules.
#[must_use]
pub fn atoms_from_name(input: &str) -> Option<Vec<u8>> {
    let counts = agent::composition_from_name(input)?;
    let mut atoms = Vec::new();
    for (symbol, count) in &counts {
        let index = chem_domain::ELEMENT_SYMBOLS
            .iter()
            .position(|candidate| candidate == symbol)?;
        let atomic_number = u8::try_from(index + 1).ok()?;
        atoms.extend(std::iter::repeat_n(
            atomic_number,
            usize::try_from(*count).ok()?,
        ));
    }
    Some(atoms)
}

/// A single periodic-table selection denotes the element in its catalogue
/// standard state. Explicit multi-atom compounds are otherwise preserved.
#[must_use]
pub fn standardize_elemental_draft(atoms: &[u8]) -> Vec<u8> {
    let [atomic_number] = atoms else {
        return atoms.to_vec();
    };
    vec![*atomic_number; standard_state_count(*atomic_number)]
}

fn elemental_identity(atoms: &[u8]) -> Option<u8> {
    let (&atomic_number, rest) = atoms.split_first()?;
    (rest.iter().all(|candidate| *candidate == atomic_number)
        && (atoms.len() == 1 || atoms.len() == standard_state_count(atomic_number)))
    .then_some(atomic_number)
}

fn registry_participant_matches(expected: ExperienceParticipantDefinition, atoms: &[u8]) -> bool {
    match expected {
        ExperienceParticipantDefinition::Element(atomic_number) => {
            elemental_identity(atoms) == Some(atomic_number)
        }
        ExperienceParticipantDefinition::Composition(formula) => {
            composition_catalogue::recognize(atoms.iter().copied())
                .is_some_and(|preview| preview.formula == formula)
        }
    }
}

#[must_use]
pub fn requests_for_drafts(first: &[u8], second: &[u8]) -> Vec<ReactionRequest> {
    let mut matches = Vec::new();
    let participant = |atoms: &[u8]| {
        if let [atomic_number] = atoms {
            return Some(DraftParticipant::Atom(*atomic_number));
        }
        composition_catalogue::recognize(atoms.iter().copied())
            .map(|preview| DraftParticipant::Composition(preview.id))
    };
    if let (Some(first), Some(second)) = (participant(first), participant(second))
        && let Some(request) = request_for_participants([first, second])
    {
        matches.push(request);
    }
    matches.extend(
        (0..EXPERIENCE_DEFINITIONS.len())
            .filter(|index| {
                let [expected_first, expected_second] = EXPERIENCE_DEFINITIONS[*index].participants;
                (registry_participant_matches(expected_first, first)
                    && registry_participant_matches(expected_second, second))
                    || (registry_participant_matches(expected_first, second)
                        && registry_participant_matches(expected_second, first))
            })
            .map(ReactionRequest::registry),
    );
    matches
}

#[cfg(test)]
#[must_use]
pub fn request_for_drafts(first: &[u8], second: &[u8]) -> Option<ReactionRequest> {
    match resolve_drafts(first, second) {
        DraftResolution::Supported(request) => Some(request),
        DraftResolution::Multiple(_)
        | DraftResolution::Screened(_)
        | DraftResolution::ExplicitlyUnsupported(_)
        | DraftResolution::Uncatalogued
        | DraftResolution::Unrecognized
        | DraftResolution::SystemError(_) => None,
    }
}

#[must_use]
pub fn resolve_drafts(first: &[u8], second: &[u8]) -> DraftResolution {
    fn participant(atoms: &[u8]) -> Option<DraftParticipant> {
        if let [atomic_number] = atoms {
            return Some(DraftParticipant::Atom(*atomic_number));
        }
        composition_catalogue::recognize(atoms.iter().copied())
            .map(|preview| DraftParticipant::Composition(preview.id))
    }

    let requests = requests_for_drafts(first, second);
    if let [request] = requests.as_slice() {
        return DraftResolution::Supported(*request);
    }
    if !requests.is_empty() {
        return DraftResolution::Multiple(requests);
    }

    if let (Some(first_participant), Some(second_participant)) =
        (participant(first), participant(second))
    {
        if let Some(request) =
            UnsupportedRequest::from_participants([first_participant, second_participant])
        {
            return match request.catalogue_case() {
                Ok(case) => DraftResolution::ExplicitlyUnsupported(case),
                Err(error) => DraftResolution::SystemError(error),
            };
        }
        if let Some(assessment) = oxygen_assessment_for_drafts(first, second) {
            return DraftResolution::Screened(assessment);
        }
        return DraftResolution::Uncatalogued;
    }

    // A draft the structure generator can build is understood chemistry that
    // simply has no reviewed catalogue experience: route it to derivation as
    // Uncatalogued instead of claiming the compounds are unrecognized.
    if draft_is_understood(first) && draft_is_understood(second) {
        return DraftResolution::Uncatalogued;
    }

    DraftResolution::Unrecognized
}

/// Whether one draft names chemistry the app understands: a bare element or
/// any composition the catalogue or structure generator can realize.
fn draft_is_understood(atoms: &[u8]) -> bool {
    matches!(atoms, [_]) || composition_catalogue::trusted_preview(atoms.iter().copied()).is_some()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OxygenAssessment {
    pub subject: String,
    pub outcome: OxygenOutcome,
}

/// Screens a reviewed element or known catalogue composition against oxygen.
/// Screening can report an outcome, but it cannot authorize simulation frames.
#[must_use]
pub fn oxygen_assessment_for_drafts(first: &[u8], second: &[u8]) -> Option<OxygenAssessment> {
    let screening = VALIDATED_OXYGEN_SCREENING.as_ref().ok()?;
    let first_element = elemental_identity(first);
    let second_element = elemental_identity(second);
    let (subject, subject_element) = match (first_element, second_element) {
        (Some(8), Some(other)) if other != 8 => (second, Some(other)),
        (Some(other), Some(8)) if other != 8 => (first, Some(other)),
        (Some(8), None) => (second, None),
        (None, Some(8)) => (first, None),
        _ => return None,
    };
    if let Some(atomic_number) = subject_element {
        let element = crate::elements::by_atomic_number(atomic_number)?;
        return Some(OxygenAssessment {
            subject: element.name.to_owned(),
            outcome: screening.element(atomic_number)?.clone(),
        });
    }
    let composition = composition_catalogue::recognize(subject.iter().copied())?;
    let name = composition_catalogue::trusted_preview(subject.iter().copied())
        .and_then(|preview| preview.name)
        .unwrap_or_else(|| composition.formula.to_owned());
    Some(OxygenAssessment {
        subject: name,
        outcome: screening.compound(composition.formula)?.clone(),
    })
}

/// Host-selected macroscopic styling for an exact trusted experience. This
/// profile can select meshes and effects, but cannot alter chemistry.
#[allow(clippy::too_many_lines)]
pub fn presentation_profile(
    request: ReactionRequest,
    frames: &SimulationFrames,
) -> Result<PresentationProfile, String> {
    let last_ordinal = frames
        .frames()
        .last()
        .and_then(|frame| u16::try_from(frame.ordinal()).ok())
        .ok_or_else(|| "trusted frames exceed the presentation range".to_owned())?;
    let transform = |translation, scale| PresentationTransform {
        translation,
        rotation: [0, 0, 0],
        scale,
    };
    let vessel = |asset| PresentationObject {
        id: "vessel".to_owned(),
        asset,
        semantic_identity: "open reaction vessel".to_owned(),
        appearance: AppearanceProfile::ClearGlass,
        role: SceneRole::Vessel,
        transform: transform([0, 0, 0], [1_100, 1_100, 1_100]),
        visible_from_ordinal: 0,
        observation: None,
        colour_transition: None,
    };
    let contents = |id: &str, identity: &str, appearance| PresentationObject {
        id: id.to_owned(),
        asset: AssetProfile::LiquidVolume,
        semantic_identity: identity.to_owned(),
        appearance,
        role: SceneRole::Contents,
        transform: transform([0, -150, 0], [1_000, 850, 1_000]),
        visible_from_ordinal: 0,
        observation: None,
        colour_transition: None,
    };
    let effect = |effect, trigger, start_ordinal, intensity| PresentationEffect {
        effect,
        trigger,
        intensity,
        start_ordinal,
        end_ordinal: last_ordinal,
    };
    // Kept as one full-range cue for presentation-plan compatibility. The 3D
    // renderer uses a fixed orthographic angle and only derives framing from
    // vessel scale; no cue changes its pose during playback.
    let camera = vec![CameraCue {
        behaviour: CameraBehaviour::WideEstablishingShot,
        start_ordinal: 0,
        end_ordinal: last_ordinal,
    }];

    let (objects, effects) = match request.kind {
        ReactionKind::AlkaliWater { metal } => {
            let (gas_ordinal, _) = active_observation(frames, ObservationPredicate::Evolves)?;
            let (disappears_ordinal, _) =
                active_observation(frames, ObservationPredicate::Disappears)?;
            let visual_evidence = alkali_water_visual_evidence(metal);
            let mut effects = vec![
                effect(
                    EffectProfile::BubbleEmitter,
                    ObservationPredicate::Evolves,
                    gas_ordinal,
                    EffectIntensity::Moderate,
                ),
                effect(
                    EffectProfile::GasRelease,
                    ObservationPredicate::Evolves,
                    gas_ordinal,
                    EffectIntensity::Moderate,
                ),
                effect(
                    EffectProfile::SurfaceDisturbance,
                    ObservationPredicate::Disappears,
                    disappears_ordinal,
                    EffectIntensity::Subtle,
                ),
                effect(
                    EffectProfile::ObjectShrinkage,
                    ObservationPredicate::Disappears,
                    disappears_ordinal,
                    EffectIntensity::Moderate,
                ),
            ];
            if let Some((palette, intensity)) = visual_evidence.flame {
                effects.push(effect(
                    EffectProfile::FlameEmitter(palette),
                    ObservationPredicate::Evolves,
                    gas_ordinal,
                    intensity,
                ));
            }
            (
                vec![
                    vessel(AssetProfile::Beaker),
                    contents("water", "water", AppearanceProfile::Water),
                    PresentationObject {
                        id: metal.lower_name().to_owned(),
                        asset: AssetProfile::MetalChunk,
                        semantic_identity: format!("{} metal", metal.lower_name()),
                        appearance: AppearanceProfile::AlkaliMetal,
                        role: SceneRole::Reactant,
                        transform: transform([0, 610, 0], [650, 650, 650]),
                        visible_from_ordinal: 0,
                        observation: None,
                        colour_transition: None,
                    },
                    PresentationObject {
                        id: "hydrogen".to_owned(),
                        asset: AssetProfile::GasCloud,
                        semantic_identity: "hydrogen gas".to_owned(),
                        appearance: AppearanceProfile::AqueousColourless,
                        role: SceneRole::Product,
                        transform: transform([180, 930, 0], [600, 600, 600]),
                        visible_from_ordinal: gas_ordinal,
                        observation: Some(ObjectObservationBinding {
                            predicate: ObservationPredicate::Evolves,
                            value: None,
                        }),
                        colour_transition: None,
                    },
                ],
                effects,
            )
        }
        ReactionKind::SilverHalidePrecipitation { halogen } => {
            let (forms_ordinal, _) = active_observation(frames, ObservationPredicate::Forms)?;
            let (colour_ordinal, colour) =
                active_observation(frames, ObservationPredicate::Colour)?;
            let colour = colour
                .ok_or_else(|| "trusted precipitate colour observation has no value".to_owned())?;
            let target = visual_colour(&colour)
                .ok_or_else(|| format!("unsupported trusted visual colour `{colour}`"))?;
            (
                vec![
                    vessel(AssetProfile::TestTube),
                    contents(
                        "aqueous-reactants",
                        "aqueous silver nitrate and sodium halide",
                        AppearanceProfile::AqueousColourless,
                    ),
                    PresentationObject {
                        id: format!("silver-{}", halogen.halide_name().to_lowercase()),
                        asset: AssetProfile::PrecipitateCloud,
                        semantic_identity: format!(
                            "silver {} precipitate",
                            halogen.halide_name().to_lowercase()
                        ),
                        appearance: AppearanceProfile::WhitePrecipitate,
                        role: SceneRole::Product,
                        transform: transform([0, -520, 0], [760, 360, 760]),
                        visible_from_ordinal: forms_ordinal,
                        observation: Some(ObjectObservationBinding {
                            predicate: ObservationPredicate::Forms,
                            value: None,
                        }),
                        colour_transition: Some(PresentationColourTransition {
                            subject_binding: "silverHalide".to_owned(),
                            value: colour,
                            target,
                            start_ordinal: colour_ordinal,
                        }),
                    },
                ],
                vec![
                    effect(
                        EffectProfile::PrecipitateFormation,
                        ObservationPredicate::Forms,
                        forms_ordinal,
                        EffectIntensity::Moderate,
                    ),
                    effect(
                        EffectProfile::Clouding,
                        ObservationPredicate::Forms,
                        forms_ordinal,
                        EffectIntensity::Subtle,
                    ),
                    effect(
                        EffectProfile::ColourTransition,
                        ObservationPredicate::Colour,
                        colour_ordinal,
                        EffectIntensity::Moderate,
                    ),
                ],
            )
        }
        ReactionKind::AcidBaseNeutralization { .. } => {
            let (disappears_ordinal, _) =
                active_observation(frames, ObservationPredicate::Disappears)?;
            let (forms_ordinal, _) = active_observation(frames, ObservationPredicate::Forms)?;
            (
                vec![
                    vessel(AssetProfile::Beaker),
                    contents(
                        "neutralization-mixture",
                        "aqueous acid and alkali hydroxide",
                        AppearanceProfile::AqueousColourless,
                    ),
                ],
                vec![
                    effect(
                        EffectProfile::LiquidMixing,
                        ObservationPredicate::Disappears,
                        disappears_ordinal,
                        EffectIntensity::Moderate,
                    ),
                    effect(
                        EffectProfile::SurfaceDisturbance,
                        ObservationPredicate::Forms,
                        forms_ordinal,
                        EffectIntensity::Subtle,
                    ),
                ],
            )
        }
        ReactionKind::AcidBicarbonateGasEvolution { .. }
        | ReactionKind::AcidCarbonateGasEvolution { .. } => {
            let (gas_ordinal, _) = active_observation(frames, ObservationPredicate::Evolves)?;
            let (disappears_ordinal, _) =
                active_observation(frames, ObservationPredicate::Disappears)?;
            (
                vec![
                    vessel(AssetProfile::ConicalFlask),
                    contents(
                        "aqueous-reactants",
                        "aqueous acid and carbonate reactants",
                        AppearanceProfile::AqueousColourless,
                    ),
                    PresentationObject {
                        id: "carbon-dioxide".to_owned(),
                        asset: AssetProfile::GasCloud,
                        semantic_identity: "carbon dioxide gas".to_owned(),
                        appearance: AppearanceProfile::AqueousColourless,
                        role: SceneRole::Product,
                        transform: transform([160, 930, 0], [620, 620, 620]),
                        visible_from_ordinal: gas_ordinal,
                        observation: Some(ObjectObservationBinding {
                            predicate: ObservationPredicate::Evolves,
                            value: None,
                        }),
                        colour_transition: None,
                    },
                ],
                vec![
                    effect(
                        EffectProfile::BubbleEmitter,
                        ObservationPredicate::Evolves,
                        gas_ordinal,
                        EffectIntensity::Moderate,
                    ),
                    effect(
                        EffectProfile::GasRelease,
                        ObservationPredicate::Evolves,
                        gas_ordinal,
                        EffectIntensity::Moderate,
                    ),
                    effect(
                        EffectProfile::SurfaceDisturbance,
                        ObservationPredicate::Disappears,
                        disappears_ordinal,
                        EffectIntensity::Subtle,
                    ),
                ],
            )
        }
        ReactionKind::HalogenDisplacement { .. } => {
            (vec![vessel(AssetProfile::TestTube)], Vec::new())
        }
        ReactionKind::Registry { index } => {
            let definition = &EXPERIENCE_DEFINITIONS[index];
            let (forms_ordinal, _) = active_observation(frames, ObservationPredicate::Forms)?;
            let co_reactant = match definition.participants[1] {
                ExperienceParticipantDefinition::Element(atomic_number) => {
                    crate::elements::by_atomic_number(atomic_number)
                        .map_or("co-reactant", |element| element.name)
                }
                ExperienceParticipantDefinition::Composition(formula) => formula,
            };
            (
                vec![
                    vessel(AssetProfile::Beaker),
                    PresentationObject {
                        id: "subject".to_owned(),
                        asset: AssetProfile::PowderPile,
                        semantic_identity: definition.subject_name.to_owned(),
                        appearance: AppearanceProfile::LaboratoryNeutral,
                        role: SceneRole::Reactant,
                        transform: transform([-300, 250, 0], [650, 650, 650]),
                        visible_from_ordinal: 0,
                        observation: None,
                        colour_transition: None,
                    },
                    PresentationObject {
                        id: "co-reactant".to_owned(),
                        asset: AssetProfile::GasCloud,
                        semantic_identity: co_reactant.to_owned(),
                        appearance: AppearanceProfile::LaboratoryNeutral,
                        role: SceneRole::Reactant,
                        transform: transform([300, 250, 0], [650, 650, 650]),
                        visible_from_ordinal: 0,
                        observation: None,
                        colour_transition: None,
                    },
                    PresentationObject {
                        id: "product".to_owned(),
                        asset: if definition.family == ReactionFamily::CovalentCombination {
                            AssetProfile::GasCloud
                        } else {
                            AssetProfile::CrystalCluster
                        },
                        semantic_identity: definition.product_name.map_or_else(
                            || crate::nomenclature::product_names(frames),
                            str::to_owned,
                        ),
                        appearance: AppearanceProfile::LaboratoryNeutral,
                        role: SceneRole::Product,
                        transform: transform([0, 250, 0], [750, 750, 750]),
                        visible_from_ordinal: forms_ordinal,
                        observation: Some(ObjectObservationBinding {
                            predicate: ObservationPredicate::Forms,
                            value: None,
                        }),
                        colour_transition: None,
                    },
                ],
                Vec::new(),
            )
        }
    };

    Ok(PresentationProfile {
        id: format!("presentation.ai.{}", request.id()),
        environment: AssetProfile::LaboratoryBench,
        objects,
        effects,
        camera,
        equation: request.equation(),
        disclosure: VIRTUAL_ONLY_DISCLOSURE.to_owned(),
    })
}

/// Selects the generic catalogue-driven compiler when every material phase is
/// reviewed, otherwise retaining the existing reviewed profile for backwards
/// compatibility with catalogues that predate macroscopic material records.
pub fn presentation_profile_with_catalogue(
    request: ReactionRequest,
    frames: &SimulationFrames,
    macroscopic: Option<&MacroscopicReaction>,
) -> Result<PresentationProfile, String> {
    if let Some(reaction) = macroscopic {
        return compile_phase_driven_profile(frames, reaction).map_err(|error| error.to_string());
    }
    presentation_profile(request, frames)
}

fn active_observation(
    frames: &SimulationFrames,
    predicate: ObservationPredicate,
) -> Result<(u16, Option<String>), String> {
    frames
        .frames()
        .iter()
        .find_map(|frame| {
            frame
                .observations()
                .iter()
                .find(|observation| {
                    observation.predicate == predicate
                        && observation.status == ObservationStatus::Active
                })
                .and_then(|observation| {
                    u16::try_from(frame.ordinal())
                        .ok()
                        .map(|ordinal| (ordinal, observation.value.clone()))
                })
        })
        .ok_or_else(|| format!("trusted frames have no active {predicate:?} observation"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chem_domain::{Phase, RepresentationKind};

    fn participant_atoms(participant: DraftParticipant) -> Vec<u8> {
        match participant {
            DraftParticipant::Atom(atomic_number) => vec![atomic_number],
            DraftParticipant::Composition(id) => composition_atoms(id),
        }
    }

    fn composition_atoms(id: CompositionId) -> Vec<u8> {
        composition_catalogue::SUPPORTED
            .iter()
            .find(|preview| preview.id == id)
            .expect("request composition is recognized by the composer")
            .atoms
            .iter()
            .flat_map(|(atomic_number, count)| {
                std::iter::repeat_n(*atomic_number, usize::from(*count))
            })
            .collect()
    }

    fn registry_participant_atoms(participant: ExperienceParticipantDefinition) -> Vec<u8> {
        match participant {
            ExperienceParticipantDefinition::Element(atomic_number) => vec![atomic_number],
            ExperienceParticipantDefinition::Composition(formula) => {
                composition_catalogue::SUPPORTED
                    .iter()
                    .find(|preview| preview.formula == formula)
                    .unwrap_or_else(|| panic!("registry composition `{formula}` is not recognized"))
                    .atoms
                    .iter()
                    .flat_map(|(atomic_number, count)| {
                        std::iter::repeat_n(*atomic_number, usize::from(*count))
                    })
                    .collect()
            }
        }
    }

    fn material(
        binding: &str,
        role: MacroscopicMaterialRole,
        phase: Phase,
        representation: RepresentationKind,
    ) -> MacroscopicMaterial {
        MacroscopicMaterial {
            binding: binding.to_owned(),
            semantic_identity: binding.to_owned(),
            role,
            phase,
            representation,
        }
    }

    fn future_profile(
        request: ReactionRequest,
        materials: Vec<MacroscopicMaterial>,
    ) -> PresentationProfile {
        let run = run(request).expect("future profile fixture validates");
        let profile = compile_phase_driven_profile(
            run.frames(),
            &MacroscopicReaction {
                profile_id: "presentation.catalogue.future-fixture".to_owned(),
                equation: request.equation(),
                materials,
                intensity: EffectIntensity::Moderate,
            },
        )
        .expect("catalogue phases compile");
        chem_presentation::compile_real_world_plan(run.frames(), &profile)
            .expect("generic profile remains observation-gated");
        profile
    }

    #[test]
    fn future_solid_plus_gas_to_gas_uses_cloud_without_liquid_bubbles() {
        let request = ReactionRequest::from_id("oxygen-carbon-oxygen")
            .expect("carbon oxygen experience exists");
        let profile = future_profile(
            request,
            vec![
                material(
                    "subject",
                    MacroscopicMaterialRole::Reactant,
                    Phase::Solid,
                    RepresentationKind::Molecular,
                ),
                material(
                    "oxygen",
                    MacroscopicMaterialRole::Reactant,
                    Phase::Gas,
                    RepresentationKind::Molecular,
                ),
                material(
                    "oxide",
                    MacroscopicMaterialRole::Product,
                    Phase::Gas,
                    RepresentationKind::Molecular,
                ),
            ],
        );

        assert!(profile.objects.iter().any(|object| {
            object.id == "oxide"
                && object.asset == AssetProfile::GasCloud
                && object.role == SceneRole::Product
        }));
        assert!(
            profile
                .effects
                .iter()
                .any(|effect| effect.effect == EffectProfile::GasRelease)
        );
        assert!(
            profile
                .effects
                .iter()
                .all(|effect| effect.effect != EffectProfile::BubbleEmitter)
        );
    }

    #[test]
    fn future_gas_plus_gas_to_liquid_forms_a_liquid_without_precipitate() {
        let request = ReactionRequest::from_id("oxygen-hydrogen-oxygen")
            .expect("hydrogen oxygen experience exists");
        let profile = future_profile(
            request,
            vec![
                material(
                    "subject",
                    MacroscopicMaterialRole::Reactant,
                    Phase::Gas,
                    RepresentationKind::Molecular,
                ),
                material(
                    "oxygen",
                    MacroscopicMaterialRole::Reactant,
                    Phase::Gas,
                    RepresentationKind::Molecular,
                ),
                material(
                    "oxide",
                    MacroscopicMaterialRole::Product,
                    Phase::Liquid,
                    RepresentationKind::Molecular,
                ),
            ],
        );

        assert!(
            profile.objects.iter().any(|object| {
                object.id == "oxide" && object.asset == AssetProfile::LiquidVolume
            })
        );
        assert!(
            profile
                .effects
                .iter()
                .any(|effect| effect.effect == EffectProfile::LiquidMixing)
        );
        assert!(profile.effects.iter().all(|effect| !matches!(
            effect.effect,
            EffectProfile::PrecipitateFormation | EffectProfile::BubbleEmitter
        )));
    }

    #[test]
    fn future_aqueous_product_solid_uses_generic_precipitation_physics() {
        let request = ReactionRequest::silver_halide_precipitation(Halogen::Bromine);
        let profile = future_profile(
            request,
            vec![
                material(
                    "silverNitrate",
                    MacroscopicMaterialRole::Reactant,
                    Phase::Aqueous,
                    RepresentationKind::Ionic,
                ),
                material(
                    "sodiumHalide",
                    MacroscopicMaterialRole::Reactant,
                    Phase::Aqueous,
                    RepresentationKind::Ionic,
                ),
                material(
                    "silverHalide",
                    MacroscopicMaterialRole::Product,
                    Phase::Solid,
                    RepresentationKind::Ionic,
                ),
                material(
                    "sodiumNitrate",
                    MacroscopicMaterialRole::Product,
                    Phase::Aqueous,
                    RepresentationKind::Ionic,
                ),
            ],
        );

        assert!(profile.objects.iter().any(|object| {
            object.id == "silverHalide" && object.asset == AssetProfile::PrecipitateCloud
        }));
        assert!(profile.effects.iter().any(|effect| {
            effect.effect == EffectProfile::PrecipitateFormation
                && effect.trigger == ObservationPredicate::Forms
        }));
        assert!(
            profile
                .effects
                .iter()
                .all(|effect| effect.effect != EffectProfile::GasRelease)
        );
    }

    #[test]
    fn every_supported_request_crosses_the_trusted_frame_boundary() {
        let mut ids = std::collections::BTreeSet::new();
        let mut families = std::collections::BTreeMap::new();
        let mut local_hit_latencies = Vec::new();
        for request in requests() {
            let id = request.id();
            assert!(ids.insert(id.clone()), "request IDs must be unique");
            assert_eq!(ReactionRequest::from_id(&id), Some(request));
            *families.entry(request.family()).or_insert(0) += 1;
            let source = request.source();
            assert_eq!(
                source,
                request.source(),
                "source authoring must be deterministic"
            );
            let started = std::time::Instant::now();
            let run = run(request).unwrap_or_else(|error| {
                panic!("registered request `{id}` should be trusted: {error}")
            });
            local_hit_latencies.push(started.elapsed());
            assert!(!run.frames().frames().is_empty());
            assert_eq!(run.frames().trust(), chem_kernel::DerivationTrust::Trusted);
            assert_eq!(
                run.frames().result(),
                chem_kernel::ValidationResult::ValidatedWithAssumptions
            );
        }
        assert_eq!(ids.len(), 205);
        assert_eq!(families[&ReactionFamily::AlkaliWater], 3);
        assert_eq!(families[&ReactionFamily::SilverHalidePrecipitation], 3);
        assert_eq!(families[&ReactionFamily::AcidBaseNeutralization], 9);
        assert_eq!(families[&ReactionFamily::AcidBicarbonateGasEvolution], 9);
        assert_eq!(families[&ReactionFamily::AcidCarbonateGasEvolution], 9);
        assert_eq!(families[&ReactionFamily::HalogenDisplacement], 3);
        assert_eq!(families[&ReactionFamily::Oxygen], 68);
        assert_eq!(families[&ReactionFamily::FixedChargeIonPair], 81);
        assert_eq!(families[&ReactionFamily::CovalentCombination], 20);
        local_hit_latencies.sort_unstable();
        let p95 = local_hit_latencies[(local_hit_latencies.len() * 95)
            .div_ceil(100)
            .saturating_sub(1)];
        assert!(
            p95 < std::time::Duration::from_millis(250),
            "catalogue local-hit p95 {p95:?} exceeded 250 ms"
        );
    }

    #[test]
    fn every_supported_request_is_reachable_from_drafts_in_either_order() {
        for expected in ReactionRequest::ALL {
            let [first, second] = expected
                .legacy_participants()
                .expect("legacy request participants")
                .map(participant_atoms);
            assert_eq!(request_for_drafts(&first, &second), Some(expected));
            assert_eq!(request_for_drafts(&second, &first), Some(expected));
        }
    }

    #[test]
    fn every_registry_experience_is_reachable_from_typed_participants() {
        for (index, definition) in EXPERIENCE_DEFINITIONS.iter().enumerate() {
            let expected = ReactionRequest::registry(index);
            let [first, second] = definition.participants.map(registry_participant_atoms);
            let forward = requests_for_drafts(&first, &second);
            let reverse = requests_for_drafts(&second, &first);
            assert!(
                forward.contains(&expected),
                "{} is unreachable in authored order",
                definition.id
            );
            assert!(
                reverse.contains(&expected),
                "{} is unreachable in reverse order",
                definition.id
            );
        }
    }

    #[test]
    fn all_23_unsupported_bindings_are_selected_from_the_trusted_catalogue() {
        let DraftResolution::ExplicitlyUnsupported(silver_fluoride) =
            resolve_drafts(&[47, 7, 8, 8, 8], &[11, 9])
        else {
            panic!("silver fluoride must reach its catalogue case");
        };
        assert_eq!(silver_fluoride.id, "silver-fluoride-soluble");
        assert_eq!(
            silver_fluoride.required_feature,
            "Features.SolubleHalideException"
        );
        assert!(
            silver_fluoride
                .explanation
                .starts_with("Silver fluoride is soluble")
        );

        let acid_sources = [
            CompositionId::LithiumHydroxide,
            CompositionId::SodiumHydroxide,
            CompositionId::PotassiumHydroxide,
            CompositionId::LithiumBicarbonate,
            CompositionId::SodiumBicarbonate,
            CompositionId::PotassiumBicarbonate,
            CompositionId::LithiumCarbonate,
            CompositionId::SodiumCarbonate,
            CompositionId::PotassiumCarbonate,
        ];
        for source in acid_sources {
            let resolution = resolve_drafts(
                &composition_atoms(CompositionId::HydrogenFluoride),
                &composition_atoms(source),
            );
            let DraftResolution::ExplicitlyUnsupported(case) = resolution else {
                panic!("HF family member must reach its catalogue case: {source:?}");
            };
            assert!(case.id.starts_with("hydrofluoric-acid-"));
            assert_eq!(case.required_feature, "Features.WeakAcidEquilibrium");
        }

        let halogens = [
            CompositionId::Fluorine,
            CompositionId::Chlorine,
            CompositionId::Bromine,
            CompositionId::Iodine,
        ];
        let halides = [
            CompositionId::SodiumFluoride,
            CompositionId::SodiumChloride,
            CompositionId::SodiumBromide,
            CompositionId::SodiumIodide,
        ];
        let mut supported = 0;
        let mut unsupported = 0;
        for halogen in halogens {
            for halide in halides {
                match resolve_drafts(&composition_atoms(halogen), &composition_atoms(halide)) {
                    DraftResolution::Supported(_) => supported += 1,
                    DraftResolution::ExplicitlyUnsupported(case) => {
                        unsupported += 1;
                        assert!(!case.id.is_empty());
                        assert!(!case.required_feature.is_empty());
                        assert!(!case.explanation.is_empty());
                    }
                    other => panic!("halogen binding was not catalogue-classified: {other:?}"),
                }
            }
        }
        assert_eq!(supported, 3);
        assert_eq!(unsupported, 13);
    }

    #[test]
    fn uncatalogued_and_unrecognized_pairs_are_distinct() {
        let DraftResolution::Supported(hydrogen_oxygen) = resolve_drafts(&[1, 1], &[8, 8]) else {
            panic!("the reviewed hydrogen/oxygen experience must be reachable");
        };
        assert_eq!(hydrogen_oxygen.family(), ReactionFamily::Oxygen);
        assert_eq!(
            resolve_drafts(&[20], &[1, 1, 8]),
            DraftResolution::Uncatalogued
        );
        assert_eq!(
            resolve_drafts(&[6, 6], &[8, 8]),
            DraftResolution::Unrecognized
        );
        assert_eq!(request_for_drafts(&[20], &[1, 1, 8]), None);
        assert_eq!(request_for_drafts(&[1, 1], &[8, 8]), Some(hydrogen_oxygen));
    }

    #[test]
    fn edited_invalid_source_never_retains_trusted_frames() {
        let error = validate_request_source(ReactionRequest::DEFAULT, "chems 1\n")
            .expect_err("incomplete source must fail");
        assert!(error.contains("request/source identity mismatch"));
    }

    #[test]
    fn trusted_request_identity_cannot_be_paired_with_another_member_source() {
        let chloride = ReactionRequest::silver_halide_precipitation(Halogen::Chlorine);
        let bromide = ReactionRequest::silver_halide_precipitation(Halogen::Bromine);
        let error = validate_request_source(chloride, &bromide.source())
            .expect_err("member substitution must fail before expansion");
        assert!(error.contains("request/source identity mismatch"));
    }

    #[test]
    fn every_supported_request_compiles_a_macroscopic_plan() {
        let mut profile_ids = std::collections::BTreeSet::new();
        for request in requests() {
            let run = run(request).expect("supported request validates");
            let profile =
                presentation_profile_with_catalogue(request, run.frames(), run.macroscopic())
                    .expect("trusted observations select a presentation profile");
            assert!(profile_ids.insert(profile.id.clone()));
            assert_eq!(profile.equation, request.equation());
            assert_eq!(profile.camera.len(), 1);
            assert_eq!(profile.camera[0].start_ordinal, 0);
            assert_eq!(
                profile.camera[0].end_ordinal,
                u16::try_from(run.frames().frames().len().saturating_sub(1))
                    .expect("frame count fits presentation ordinal")
            );
            assert!(
                profile
                    .objects
                    .iter()
                    .any(|object| object.role == SceneRole::Vessel)
            );
            if !matches!(
                request.family(),
                ReactionFamily::HalogenDisplacement
                    | ReactionFamily::Oxygen
                    | ReactionFamily::FixedChargeIonPair
                    | ReactionFamily::CovalentCombination
            ) {
                assert!(
                    profile
                        .objects
                        .iter()
                        .any(|object| object.role == SceneRole::Contents)
                );
            }
            let plan = chem_presentation::compile_real_world_plan(run.frames(), &profile)
                .expect("profile effects are bound to validated observations");
            assert_eq!(plan.profile_id, profile.id);
            assert!(!plan.timeline.beats.is_empty());
            for effect in &profile.effects {
                let (ordinal, _) = active_observation(run.frames(), effect.trigger)
                    .expect("effect trigger activates in trusted frames");
                assert_eq!(effect.start_ordinal, ordinal);
            }
            for object in &profile.objects {
                let Some(binding) = &object.observation else {
                    continue;
                };
                let (ordinal, value) = active_observation(run.frames(), binding.predicate)
                    .expect("object observation activates in trusted frames");
                assert_eq!(object.visible_from_ordinal, ordinal);
                assert_eq!(binding.value, value);
            }
        }
        assert_eq!(profile_ids.len(), 205);
    }

    #[test]
    fn registry_outcomes_are_selected_without_guessing_between_products() {
        let magnesium_fluorine = requests_for_drafts(&[12], &[9]);
        assert_eq!(magnesium_fluorine.len(), 1);
        assert_eq!(
            magnesium_fluorine[0].family(),
            ReactionFamily::FixedChargeIonPair
        );

        let iron_oxygen = requests_for_drafts(&[26], &[8]);
        assert_eq!(iron_oxygen.len(), 3);
        assert!(
            iron_oxygen
                .iter()
                .all(|request| request.family() == ReactionFamily::Oxygen)
        );
        assert!(matches!(
            resolve_drafts(&[26], &[8]),
            DraftResolution::Multiple(outcomes) if outcomes.len() == 3
        ));

        let chlorine_fluorine = requests_for_drafts(&[17], &[9]);
        assert_eq!(chlorine_fluorine.len(), 3);
        assert!(
            chlorine_fluorine
                .iter()
                .all(|request| request.family() == ReactionFamily::CovalentCombination)
        );
        assert!(
            chlorine_fluorine
                .iter()
                .all(|request| request.product_preview().is_some())
        );
    }

    #[test]
    fn precipitation_profiles_preserve_each_validated_precipitate_colour() {
        for (halogen, expected) in [
            (Halogen::Chlorine, "White"),
            (Halogen::Bromine, "Cream"),
            (Halogen::Iodine, "Yellow"),
        ] {
            let request = ReactionRequest::silver_halide_precipitation(halogen);
            let run = run(request).expect("precipitation request validates");
            let profile = presentation_profile(request, run.frames())
                .expect("trusted colour selects a supported appearance");
            let product = profile
                .objects
                .iter()
                .find(|object| object.role == SceneRole::Product)
                .expect("precipitate product is presented");
            let formation = product
                .observation
                .as_ref()
                .expect("precipitate is bound to its formation observation");
            let transition = product
                .colour_transition
                .as_ref()
                .expect("precipitate is bound to its colour observation");
            let (forms_ordinal, _) = active_observation(run.frames(), ObservationPredicate::Forms)
                .expect("trusted formation observation activates");
            let (colour_ordinal, value) =
                active_observation(run.frames(), ObservationPredicate::Colour)
                    .expect("trusted colour observation activates");
            assert_eq!(product.appearance, AppearanceProfile::WhitePrecipitate);
            assert_eq!(product.visible_from_ordinal, forms_ordinal);
            assert_eq!(formation.predicate, ObservationPredicate::Forms);
            assert_eq!(transition.start_ordinal, colour_ordinal);
            assert_eq!(transition.value, expected);
            assert_eq!(
                transition.target,
                visual_colour(expected).expect("colour resolves")
            );
            assert_eq!(value.as_deref(), Some(expected));
        }
    }

    #[test]
    fn reviewed_alkali_water_metadata_distinguishes_ignition_from_flame_test_colour() {
        for metal in [AlkaliMetal::Lithium, AlkaliMetal::Sodium] {
            let request = ReactionRequest::alkali_water(metal);
            let run = run(request).expect("alkali-water request validates");
            let profile =
                presentation_profile(request, run.frames()).expect("alkali-water profile compiles");
            assert!(
                !profile
                    .effects
                    .iter()
                    .any(|effect| matches!(effect.effect, EffectProfile::FlameEmitter(_))),
                "fizzing without reviewed ignition metadata must not invent a flame"
            );
        }

        let request = ReactionRequest::alkali_water(AlkaliMetal::Potassium);
        let run = run(request).expect("potassium-water request validates");
        let profile =
            presentation_profile(request, run.frames()).expect("potassium-water profile compiles");
        let flame = profile
            .effects
            .iter()
            .find(|effect| matches!(effect.effect, EffectProfile::FlameEmitter(_)))
            .expect("reviewed potassium-water metadata authorizes ignition");
        assert_eq!(
            flame.effect,
            EffectProfile::FlameEmitter(FlamePalette::Lilac)
        );
        assert_eq!(flame.intensity, EffectIntensity::Strong);
        assert_eq!(flame.trigger, ObservationPredicate::Evolves);
        let (gas_ordinal, _) = active_observation(run.frames(), ObservationPredicate::Evolves)
            .expect("trusted gas observation activates");
        assert_eq!(flame.start_ordinal, gas_ordinal);
        chem_presentation::compile_real_world_plan(run.frames(), &profile)
            .expect("flame remains gated by a trusted observation");
    }

    #[test]
    fn macroscopic_compiler_rejects_early_or_value_mismatched_observation_bindings() {
        let alkali = run(ReactionRequest::DEFAULT).expect("alkali request validates");
        let mut early = presentation_profile(ReactionRequest::DEFAULT, alkali.frames())
            .expect("alkali profile compiles");
        early.effects[0].start_ordinal = 0;
        assert_eq!(
            chem_presentation::compile_real_world_plan(alkali.frames(), &early),
            Err(chem_presentation::PlanError::UnsupportedEffectTrigger)
        );

        let mut incompatible_effect =
            presentation_profile(ReactionRequest::DEFAULT, alkali.frames())
                .expect("alkali profile compiles");
        let gas = incompatible_effect
            .effects
            .iter_mut()
            .find(|effect| effect.effect == EffectProfile::GasRelease)
            .expect("gas release exists");
        gas.trigger = ObservationPredicate::Disappears;
        assert_eq!(
            chem_presentation::compile_real_world_plan(alkali.frames(), &incompatible_effect),
            Err(chem_presentation::PlanError::IncompatibleEffectObservation)
        );

        let mut incompatible_object =
            presentation_profile(ReactionRequest::DEFAULT, alkali.frames())
                .expect("alkali profile compiles");
        incompatible_object
            .objects
            .iter_mut()
            .find(|object| object.asset == AssetProfile::GasCloud)
            .expect("gas product exists")
            .observation
            .as_mut()
            .expect("gas product is observation bound")
            .predicate = ObservationPredicate::Forms;
        assert_eq!(
            chem_presentation::compile_real_world_plan(alkali.frames(), &incompatible_object),
            Err(chem_presentation::PlanError::UnsupportedObjectObservation)
        );

        let request = ReactionRequest::silver_halide_precipitation(Halogen::Bromine);
        let precipitation = run(request).expect("precipitation request validates");
        let mut mismatched = presentation_profile(request, precipitation.frames())
            .expect("precipitation profile compiles");
        let product = mismatched
            .objects
            .iter_mut()
            .find(|object| object.role == SceneRole::Product)
            .expect("precipitate product exists");
        product
            .colour_transition
            .as_mut()
            .expect("product has a colour binding")
            .value = "Magenta".to_owned();
        assert_eq!(
            chem_presentation::compile_real_world_plan(precipitation.frames(), &mismatched),
            Err(chem_presentation::PlanError::InvalidVisualColour)
        );

        let mut premature_colour = presentation_profile(request, precipitation.frames())
            .expect("precipitation profile compiles");
        let product = premature_colour
            .objects
            .iter_mut()
            .find(|object| object.role == SceneRole::Product)
            .expect("precipitate product exists");
        product
            .colour_transition
            .as_mut()
            .expect("product has a colour binding")
            .start_ordinal = 0;
        assert_eq!(
            chem_presentation::compile_real_world_plan(precipitation.frames(), &premature_colour),
            Err(chem_presentation::PlanError::UnsupportedColourObservation)
        );

        let mut premature = presentation_profile(request, precipitation.frames())
            .expect("precipitation profile compiles");
        let product = premature
            .objects
            .iter_mut()
            .find(|object| object.role == SceneRole::Product)
            .expect("precipitate product exists");
        product.visible_from_ordinal = product.visible_from_ordinal.saturating_sub(1);
        assert_eq!(
            chem_presentation::compile_real_world_plan(precipitation.frames(), &premature),
            Err(chem_presentation::PlanError::UnsupportedObjectObservation)
        );

        let mut unbound = presentation_profile(request, precipitation.frames())
            .expect("precipitation profile compiles");
        unbound
            .objects
            .iter_mut()
            .find(|object| object.role == SceneRole::Product)
            .expect("precipitate product exists")
            .observation = None;
        assert_eq!(
            chem_presentation::compile_real_world_plan(precipitation.frames(), &unbound),
            Err(chem_presentation::PlanError::UnsupportedObjectObservation)
        );
    }
}
