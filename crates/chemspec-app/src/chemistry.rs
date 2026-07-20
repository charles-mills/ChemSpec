//! Application boundary for kernel-validated chemistry experiences.
//!
//! The UI may identify an exact supported draft, but every product, bond,
//! observation, and frame below is produced by the language and kernel crates.

use std::{collections::BTreeMap, str::FromStr, sync::LazyLock};

use chem_catalogue::{
    ExplosiveWaterContactVariantRecord, GeneralizedCaseSelection, GeneralizedReactionCaseRecord,
    ObservationPredicate, OxygenOutcome, ReferenceCatalogue, ReferenceIntegrityPolicy,
    ValidatedCatalogueBundle, ValidatedOxygenScreening, WaterContactBehaviourRecord,
};
use chem_domain::{ContentDigest, ReactionRuleId, RepresentationKind};
use chem_kernel::{
    CurrentArtifactIdentity, ObservationStatus, SimulationFrames, expand_provisional,
    expand_reference, generate_frames, validate_provisional, validate_reference,
};
use chem_presentation::{
    AppearanceProfile, AssetProfile, CameraBehaviour, CameraCue, EffectIntensity, EffectProfile,
    ExplosiveMetalWaterVariant, FlamePalette, MacroscopicMaterial, MacroscopicMaterialRole,
    MacroscopicProcess, MacroscopicReaction, ObjectObservationBinding,
    PresentationColourTransition, PresentationEffect, PresentationObject, PresentationProfile,
    PresentationTransform, SceneRole, VIRTUAL_ONLY_DISCLOSURE, VisualColour,
    compile_phase_driven_profile, complete_generic_visual_profile, visual_colour,
};

use crate::composition_catalogue::{self, CompositionId};

const CATALOGUE: &[u8] =
    include_bytes!("../../../catalogue/reference/core-chemistry/catalogue.json");
const CATALOGUE_REVIEW: &[u8] =
    include_bytes!("../../../catalogue/reviews/core-chemistry.review.json");
const CATALOGUE_DIGEST: &str = "cb51dacb986d35773a487879352a98ca478f9ba55f2755be0401c8d1b5e2d607";
const CATALOGUE_REVIEW_DIGEST: &str =
    "3dfac8a80f8567bda87d5d082ad39a7aabc6c6e8e4ced51da86dcaf02d2cab83";

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

/// The heavy alkali metals whose exact catalogue material facts authorise the
/// reusable high-energy water-contact assembly. This deliberately remains
/// separate from [`AlkaliMetal`]: the independently reviewed salt families
/// retain their original Li/Na/K finite domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeavyAlkaliMetal {
    Rubidium,
    Caesium,
    Francium,
}

impl HeavyAlkaliMetal {
    const fn name(self) -> &'static str {
        match self {
            Self::Rubidium => "Rubidium",
            Self::Caesium => "Caesium",
            Self::Francium => "Francium",
        }
    }

    const fn lower_name(self) -> &'static str {
        match self {
            Self::Rubidium => "rubidium",
            Self::Caesium => "caesium",
            Self::Francium => "francium",
        }
    }

    const fn symbol(self) -> &'static str {
        match self {
            Self::Rubidium => "Rb",
            Self::Caesium => "Cs",
            Self::Francium => "Fr",
        }
    }

    const fn atomic_number(self) -> u8 {
        match self {
            Self::Rubidium => 37,
            Self::Caesium => 55,
            Self::Francium => 87,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AlkaliWaterVisualEvidence {
    activity: EffectIntensity,
    flame: Option<(FlamePalette, EffectIntensity)>,
}

/// Reviewed qualitative presentation metadata for the alkali-water family.
///
/// The Royal Society of Chemistry's classroom observations describe lithium
/// as fizzing, sodium as fizzing vigorously, and potassium as vigorous with
/// lilac self-ignition. This metadata remains upstream of the generic renderer
/// and does not alter the reference reaction frames.
/// <https://edu.rsc.org/download?ac=512063>
const fn alkali_water_visual_evidence(metal: AlkaliMetal) -> AlkaliWaterVisualEvidence {
    match metal {
        AlkaliMetal::Lithium => AlkaliWaterVisualEvidence {
            activity: EffectIntensity::Subtle,
            flame: None,
        },
        AlkaliMetal::Sodium => AlkaliWaterVisualEvidence {
            activity: EffectIntensity::Moderate,
            flame: None,
        },
        AlkaliMetal::Potassium => AlkaliWaterVisualEvidence {
            activity: EffectIntensity::Strong,
            flame: Some((FlamePalette::Lilac, EffectIntensity::Strong)),
        },
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

    /// Reviewed aqueous solution colour of this halogen when displaced, with
    /// its evidence claim. Chlorine is the strongest oxidiser in the supported
    /// set and is never displaced.
    fn displacement_observation(self) -> (&'static str, &'static str) {
        match self {
            Self::Chlorine => unreachable!("chlorine is never the displaced halogen"),
            Self::Bromine => ("Orange", "R2"),
            Self::Iodine => ("Brown", "R3"),
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
        let catalogue = VALIDATED_CATALOGUE.as_ref().map_err(String::as_str)?;
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
                "reference catalogue did not select an unsupported case for `{rule_id}`"
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
    HeavyAlkaliWater {
        metal: HeavyAlkaliMetal,
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
    pub const ALL: [Self; 39] = [
        Self::alkali_water(AlkaliMetal::Lithium),
        Self::alkali_water(AlkaliMetal::Sodium),
        Self::alkali_water(AlkaliMetal::Potassium),
        Self::heavy_alkali_water(HeavyAlkaliMetal::Rubidium),
        Self::heavy_alkali_water(HeavyAlkaliMetal::Caesium),
        Self::heavy_alkali_water(HeavyAlkaliMetal::Francium),
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
    pub const fn heavy_alkali_water(metal: HeavyAlkaliMetal) -> Self {
        Self {
            kind: ReactionKind::HeavyAlkaliWater { metal },
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
            ReactionKind::AlkaliWater { .. } | ReactionKind::HeavyAlkaliWater { .. } => {
                ReactionFamily::AlkaliWater
            }
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
            ReactionKind::HeavyAlkaliWater { metal } => {
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
            ReactionKind::HeavyAlkaliWater { metal } => {
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
            ReactionKind::HeavyAlkaliWater { metal } => Some([
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
            ReactionKind::AlkaliWater { metal } => {
                alkali_water_source(metal.name(), metal.symbol())
            }
            ReactionKind::HeavyAlkaliWater { metal } => {
                alkali_water_source(metal.name(), metal.symbol())
            }
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
    pub fn product_preview(self) -> Option<composition_catalogue::ReferenceCompositionPreview> {
        let structure = self.definition()?.product_structure?;
        composition_catalogue::reference_preview_by_structure_id(structure)
    }

    /// The composer draft inventories that resolve to this request, in the
    /// request's own participant order.
    fn draft_atoms(self) -> [Vec<u8>; 2] {
        if let Some(definition) = self.definition() {
            return definition.participants.map(registry_participant_atoms);
        }
        self.legacy_participants()
            .expect("non-registry requests define legacy participants")
            .map(participant_atoms)
    }
}

pub fn requests() -> impl Iterator<Item = ReactionRequest> {
    ReactionRequest::ALL
        .into_iter()
        .chain((0..EXPERIENCE_DEFINITIONS.len()).map(ReactionRequest::registry))
}

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
        .flat_map(|(atomic_number, count)| std::iter::repeat_n(*atomic_number, usize::from(*count)))
        .collect()
}

fn registry_participant_atoms(participant: ExperienceParticipantDefinition) -> Vec<u8> {
    match participant {
        ExperienceParticipantDefinition::Element(atomic_number) => vec![atomic_number],
        ExperienceParticipantDefinition::Composition(formula) => composition_catalogue::SUPPORTED
            .iter()
            .find(|preview| preview.formula == formula)
            .unwrap_or_else(|| panic!("registry composition `{formula}` is not recognized"))
            .atoms
            .iter()
            .flat_map(|(atomic_number, count)| {
                std::iter::repeat_n(*atomic_number, usize::from(*count))
            })
            .collect(),
    }
}

/// Reactant atom inventories for every supported request, grouped by
/// family. The dice roll samples a family first so one large family
/// (the registry's ion pairs alone outnumber every showcase reaction)
/// cannot drown out the visibly dramatic chemistry.
pub fn roll_candidates() -> Vec<(ReactionFamily, Vec<[Vec<u8>; 2]>)> {
    let mut by_family = BTreeMap::<ReactionFamily, Vec<[Vec<u8>; 2]>>::new();
    for request in requests() {
        by_family
            .entry(request.family())
            .or_default()
            .push(request.draft_atoms());
    }
    by_family.into_iter().collect()
}

fn alkali_water_source(name: &str, symbol: &str) -> String {
    format!(
        "chems 1\nuse catalog ChemSpec.Theoretical@1\nreaction {name}AndWater where\n  reactants\n    metal := 2 of {name}Metal\n    water := 2 of Water\n  products\n    hydroxide := 2 of {name}Hydroxide\n    hydrogen := 1 of Hydrogen\n  equation\n    2 {symbol}[metallic] + 2 H2O[molecular]\n    -> 2 {symbol}OH[ionic] + H2[molecular]\n  model\n    event := representative\n    sequence := explanatory\n  observe from Evidence.AlkaliWater@1\n    gas hydrogen evolves claim R1\n    reactant metal disappears claim R2\n  by\n    apply Rules.AlkaliMetalWithWater\n      metal := metal\n      water := water\n      hydroxide := hydroxide\n      gasProduct := hydrogen\n",
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
    let (colour, colour_claim) = displaced.displacement_observation();
    format!(
        "chems 1\nuse catalog ChemSpec.Theoretical@1\nreaction {displacing_name}Displaces{displaced_name} where\n  reactants\n    displacingHalogen := 1 of {displacing_name}\n    saltSource := 2 of Sodium{displaced_halide}\n  products\n    newSalt := 2 of Sodium{displacing_halide}\n    displacedHalogen := 1 of {displaced_name}\n  equation\n    {displacing_symbol}2[molecular] + 2 Na{displaced_symbol}[ionic]\n    -> 2 Na{displacing_symbol}[ionic] + {displaced_symbol}2[molecular]\n  model\n    event := representative\n    sequence := explanatory\n  observe from Evidence.HalogenDisplacement@1\n    product displacedHalogen forms claim R1\n    product displacedHalogen has colour {colour} claim {colour_claim}\n  by\n    apply Rules.HalogenDisplacement\n      displacingHalogen := displacingHalogen\n      saltSource := saltSource\n      newSalt := newSalt\n      displacedHalogen := displacedHalogen\n",
        displacing_name = displacing.name(),
        displaced_name = displaced.name(),
        displaced_halide = displaced.halide_name(),
        displacing_halide = displacing.halide_name(),
        displacing_symbol = displacing.symbol(),
        displaced_symbol = displaced.symbol()
    )
}

#[derive(Debug, Clone)]
pub struct ValidatedRun {
    frames: SimulationFrames,
    macroscopic: Option<MacroscopicReaction>,
    declaration: chem_domain::ReactionDeclaration,
}

impl ValidatedRun {
    #[must_use]
    pub const fn frames(&self) -> &SimulationFrames {
        &self.frames
    }

    /// Catalogue-resolved material phases for the generic presentation
    /// compiler. `None` means the reference catalogue predates those optional
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

static REFERENCE_CATALOGUE: LazyLock<Result<ReferenceCatalogue, String>> = LazyLock::new(|| {
    let reviewed = || -> Result<ReferenceCatalogue, String> {
        let catalogue_digest =
            ContentDigest::from_str(CATALOGUE_DIGEST).map_err(|error| error.to_string())?;
        let review_digest =
            ContentDigest::from_str(CATALOGUE_REVIEW_DIGEST).map_err(|error| error.to_string())?;
        ReferenceCatalogue::from_canonical_json(
            CATALOGUE,
            CATALOGUE_REVIEW,
            ReferenceIntegrityPolicy::new(catalogue_digest, review_digest),
        )
        .map_err(|error| error.to_string())
    };
    reviewed()
        .or_else(|_| ReferenceCatalogue::from_json(CATALOGUE).map_err(|error| error.to_string()))
});

static VALIDATED_CATALOGUE: LazyLock<Result<ValidatedCatalogueBundle, String>> =
    LazyLock::new(|| {
        ValidatedCatalogueBundle::from_json(CATALOGUE).map_err(|error| error.to_string())
    });

pub(crate) fn reference_catalogue() -> Result<&'static ReferenceCatalogue, &'static str> {
    REFERENCE_CATALOGUE.as_ref().map_err(String::as_str)
}

static VALIDATED_OXYGEN_SCREENING: LazyLock<Result<ValidatedOxygenScreening, String>> =
    LazyLock::new(|| {
        let catalogue = VALIDATED_CATALOGUE.as_ref().map_err(Clone::clone)?;
        ValidatedOxygenScreening::from_json(OXYGEN_SCREENING, catalogue)
            .map_err(|error| error.to_string())
    });
/// Returns a kernel-validated experience result.
///
/// The returned frame type cannot be constructed by the application. Failure
/// is retained and shown honestly instead of falling back to UI-authored chemistry.
pub fn run(request: ReactionRequest) -> Result<ValidatedRun, String> {
    build_run(request)
}

fn build_run(request: ReactionRequest) -> Result<ValidatedRun, String> {
    let validated = validate_request_source(request, &request.source())?;
    Ok(ValidatedRun {
        frames: validated.frames,
        macroscopic: validated.macroscopic,
        declaration: validated.declaration,
    })
}

/// Parses, expands, validates, and projects source against bundled reference
/// data and the evidence packet for the selected experience.
fn validate_request_source(
    request: ReactionRequest,
    source: &str,
) -> Result<ValidatedRequestArtifacts, String> {
    validate_request_source_with_reference(request, source, REFERENCE_CATALOGUE.as_ref().ok())
}

fn validate_request_source_with_reference(
    request: ReactionRequest,
    source: &str,
    reference: Option<&ReferenceCatalogue>,
) -> Result<ValidatedRequestArtifacts, String> {
    if source != request.source() {
        return Err(format!(
            "request/source identity mismatch for `{}`",
            request.id()
        ));
    }
    let catalogue = VALIDATED_CATALOGUE.as_ref().map_err(String::as_str)?;
    let (frames, macroscopic, declaration) = if let Some(reference) = reference {
        let expanded = expand_reference(
            &request.source_name(),
            source,
            reference,
            request.evidence(),
        )
        .map_err(|error| error.to_string())?;
        let macroscopic = catalogue_macroscopic_reaction(request, &expanded, catalogue);
        let declaration = expanded.claim().declaration().clone();
        let current =
            CurrentArtifactIdentity::from_expanded(&expanded).map_err(|error| error.to_string())?;
        let validated =
            validate_reference(&expanded, reference).map_err(|error| error.to_string())?;
        let frames = generate_frames(&validated, current).map_err(|error| error.to_string())?;
        (frames, macroscopic, declaration)
    } else {
        let expanded = expand_provisional(
            &request.source_name(),
            source,
            catalogue,
            request.evidence(),
        )
        .map_err(|error| error.to_string())?;
        let macroscopic = catalogue_macroscopic_reaction(request, &expanded, catalogue);
        let declaration = expanded.claim().declaration().clone();
        let current =
            CurrentArtifactIdentity::from_expanded(&expanded).map_err(|error| error.to_string())?;
        let validated =
            validate_provisional(&expanded, catalogue).map_err(|error| error.to_string())?;
        let frames = generate_frames(&validated, current).map_err(|error| error.to_string())?;
        (frames, macroscopic, declaration)
    };
    Ok(ValidatedRequestArtifacts {
        frames,
        macroscopic,
        declaration,
    })
}

fn catalogue_macroscopic_reaction(
    request: ReactionRequest,
    expanded: &chem_kernel::ExpandedStructuralReaction,
    catalogue: &ValidatedCatalogueBundle,
) -> Option<MacroscopicReaction> {
    let mut materials =
        Vec::with_capacity(expanded.claim().reactants().len() + expanded.claim().products().len());
    for (binding, material) in expanded.claim().reactants() {
        materials.push(catalogue_macroscopic_material(
            request,
            expanded,
            catalogue,
            binding,
            material,
            MacroscopicMaterialRole::Reactant,
        )?);
    }
    for (binding, material) in expanded.claim().products() {
        materials.push(catalogue_macroscopic_material(
            request,
            expanded,
            catalogue,
            binding,
            material,
            MacroscopicMaterialRole::Product,
        )?);
    }
    let process = classify_catalogue_macroscopic_process(expanded, &materials);
    let fuel_carbon_count = process
        .filter(|process| {
            matches!(
                process,
                MacroscopicProcess::CompleteCombustion | MacroscopicProcess::IncompleteCombustion
            )
        })
        .and_then(|_| catalogue_combustion_fuel_carbon_count(expanded));
    Some(MacroscopicReaction {
        profile_id: format!("presentation.catalogue.{}", request.id()),
        equation: request.equation(),
        materials,
        intensity: macroscopic_process_intensity(process),
        process,
        fuel_carbon_count,
        surface_oxide_colour: None,
    })
}

fn catalogue_macroscopic_material(
    request: ReactionRequest,
    expanded: &chem_kernel::ExpandedStructuralReaction,
    catalogue: &ValidatedCatalogueBundle,
    binding: &str,
    resolved: &chem_kernel::ResolvedStructureBinding,
    role: MacroscopicMaterialRole,
) -> Option<MacroscopicMaterial> {
    let rule_id = &expanded.claim().rule().rule;
    let rule_role = expanded
        .claim()
        .rule()
        .bindings
        .values()
        .find(|candidate| candidate.binding == binding)
        .map(|candidate| (rule_id, candidate.role.as_str()));
    let record = catalogue.macroscopic_material(&resolved.structure, rule_role);
    let phase = record.map(|material| material.phase).or_else(|| {
        (request.family() == ReactionFamily::Oxygen).then_some(chem_domain::Phase::Unknown)
    })?;
    Some(MacroscopicMaterial {
        binding: binding.to_owned(),
        semantic_identity: resolved.name.clone(),
        structure_id: resolved.structure.to_string(),
        formula: chem_domain::conventional_formula(
            resolved
                .formula
                .iter()
                .map(|(symbol, count)| (symbol.as_str(), *count)),
        ),
        role,
        phase,
        representation: resolved.representation,
        colour: record.and_then(|material| {
            material
                .colour
                .map(|[red, green, blue]| VisualColour { red, green, blue })
        }),
        explosive_water_contact: record
            .and_then(|material| material.water_contact)
            .map(explosive_metal_water_variant),
    })
}

pub(crate) const fn explosive_metal_water_variant(
    water_contact: WaterContactBehaviourRecord,
) -> ExplosiveMetalWaterVariant {
    match water_contact {
        WaterContactBehaviourRecord::Explosive { variant } => match variant {
            ExplosiveWaterContactVariantRecord::Rubidium => ExplosiveMetalWaterVariant::Rubidium,
            ExplosiveWaterContactVariantRecord::Caesium => ExplosiveMetalWaterVariant::Caesium,
            ExplosiveWaterContactVariantRecord::Francium => ExplosiveMetalWaterVariant::Francium,
        },
    }
}

pub(crate) const fn macroscopic_process_intensity(
    process: Option<MacroscopicProcess>,
) -> EffectIntensity {
    match process {
        Some(
            MacroscopicProcess::CompleteCombustion
            | MacroscopicProcess::IncompleteCombustion
            | MacroscopicProcess::ExplosiveMetalWater(_),
        ) => EffectIntensity::Strong,
        Some(
            MacroscopicProcess::AqueousPrecipitation
            | MacroscopicProcess::GasEvolutionLiquidLiquid
            | MacroscopicProcess::GasEvolutionSolidLiquid
            | MacroscopicProcess::MetalDisplacement
            | MacroscopicProcess::SolidSolidSynthesis
            | MacroscopicProcess::SolidGasSynthesis
            | MacroscopicProcess::GasGasSynthesis
            | MacroscopicProcess::SolventEvaporationCrystallization
            | MacroscopicProcess::SurfaceOxidation,
        )
        | None => EffectIntensity::Moderate,
    }
}

#[allow(clippy::too_many_lines)]
fn classify_catalogue_macroscopic_process(
    expanded: &chem_kernel::ExpandedStructuralReaction,
    materials: &[MacroscopicMaterial],
) -> Option<MacroscopicProcess> {
    if expanded.claim().reactants().len() != 2 {
        return None;
    }
    let material_has_phase =
        |binding: &str, role: MacroscopicMaterialRole, phase: chem_domain::Phase| {
            materials.iter().any(|material| {
                material.binding == binding && material.role == role && material.phase == phase
            })
        };
    let mut reactants = expanded.claim().reactants().iter();
    let first = reactants.next()?;
    let second = reactants.next()?;
    if classifies_catalogue_metal_displacement(expanded, materials) {
        return Some(MacroscopicProcess::MetalDisplacement);
    }
    if classifies_validated_structural_surface_oxidation(expanded) {
        return Some(MacroscopicProcess::SurfaceOxidation);
    }
    if let Some(variant) = classifies_explosive_metal_water(expanded, materials) {
        return Some(MacroscopicProcess::ExplosiveMetalWater(variant));
    }
    let liquid_water = expanded
        .claim()
        .products()
        .iter()
        .any(|(binding, product)| {
            material_has_phase(
                binding,
                MacroscopicMaterialRole::Product,
                chem_domain::Phase::Liquid,
            ) && has_formula_counts(&product.formula, &[("H", 2), ("O", 1)])
        });
    let dissolved_ionic_product = expanded
        .claim()
        .products()
        .iter()
        .any(|(binding, product)| {
            material_has_phase(
                binding,
                MacroscopicMaterialRole::Product,
                chem_domain::Phase::Aqueous,
            ) && product.representation == RepresentationKind::Ionic
        });
    let mobile_reactants = expanded.claim().reactants().iter().all(|(binding, _)| {
        materials.iter().any(|material| {
            material.binding == binding.as_str()
                && material.role == MacroscopicMaterialRole::Reactant
                && matches!(
                    material.phase,
                    chem_domain::Phase::Aqueous | chem_domain::Phase::Liquid
                )
        })
    });
    if liquid_water && dissolved_ionic_product && mobile_reactants {
        return Some(MacroscopicProcess::SolventEvaporationCrystallization);
    }
    let combustion = (|| {
        let (fuel, oxygen) = if has_formula_counts(&first.1.formula, &[("O", 2)]) {
            (second, first)
        } else if has_formula_counts(&second.1.formula, &[("O", 2)]) {
            (first, second)
        } else {
            return None;
        };
        if fuel.1.representation != RepresentationKind::Molecular
            || oxygen.1.representation != RepresentationKind::Molecular
            || !is_carbon_hydrogen_oxygen_fuel(&fuel.1.formula)
            || !material_has_phase(
                oxygen.0,
                MacroscopicMaterialRole::Reactant,
                chem_domain::Phase::Gas,
            )
        {
            return None;
        }
        let product_is_gas = |binding: &str| {
            material_has_phase(
                binding,
                MacroscopicMaterialRole::Product,
                chem_domain::Phase::Gas,
            )
        };
        let has_carbon_dioxide = expanded
            .claim()
            .products()
            .iter()
            .any(|(binding, product)| {
                product_is_gas(binding)
                    && has_formula_counts(&product.formula, &[("C", 1), ("O", 2)])
            });
        let has_carbon_monoxide = expanded
            .claim()
            .products()
            .iter()
            .any(|(binding, product)| {
                product_is_gas(binding)
                    && has_formula_counts(&product.formula, &[("C", 1), ("O", 1)])
            });
        let has_water_vapour = expanded
            .claim()
            .products()
            .iter()
            .any(|(binding, product)| {
                product_is_gas(binding)
                    && has_formula_counts(&product.formula, &[("H", 2), ("O", 1)])
            });
        if has_carbon_monoxide {
            Some(MacroscopicProcess::IncompleteCombustion)
        } else {
            (has_carbon_dioxide && has_water_vapour)
                .then_some(MacroscopicProcess::CompleteCombustion)
        }
    })();
    if combustion.is_some() {
        return combustion;
    }
    let solid_reactants = expanded.claim().reactants().iter().all(|(binding, _)| {
        material_has_phase(
            binding,
            MacroscopicMaterialRole::Reactant,
            chem_domain::Phase::Solid,
        )
    });
    if expanded.claim().products().len() != 1 {
        return None;
    }
    let (product_binding, _) = expanded.claim().products().iter().next()?;
    let reactant_phase = |binding: &str| {
        materials
            .iter()
            .find(|material| {
                material.binding == binding && material.role == MacroscopicMaterialRole::Reactant
            })
            .map(|material| material.phase)
    };
    let product_phase = materials
        .iter()
        .find(|material| {
            material.binding == product_binding.as_str()
                && material.role == MacroscopicMaterialRole::Product
        })?
        .phase;
    let first_formula: Vec<(&str, u64)> = first
        .1
        .formula
        .iter()
        .map(|(symbol, count)| (symbol.as_str(), *count))
        .collect();
    let second_formula: Vec<(&str, u64)> = second
        .1
        .formula
        .iter()
        .map(|(symbol, count)| (symbol.as_str(), *count))
        .collect();
    if let (Some(first_phase), Some(second_phase)) = (
        reactant_phase(first.0.as_str()),
        reactant_phase(second.0.as_str()),
    ) && let Some(route) = chem_domain::classify_phase_synthesis(
        (&first_formula, first_phase),
        (&second_formula, second_phase),
        product_phase,
    ) {
        return Some(match route {
            chem_domain::PhaseSynthesisRoute::SolidGas => MacroscopicProcess::SolidGasSynthesis,
            chem_domain::PhaseSynthesisRoute::GasGas => MacroscopicProcess::GasGasSynthesis,
        });
    }
    (solid_reactants
        && material_has_phase(
            product_binding,
            MacroscopicMaterialRole::Product,
            chem_domain::Phase::Solid,
        )
        && !materials.iter().any(|material| {
            material.role == MacroscopicMaterialRole::Product
                && material.phase == chem_domain::Phase::Gas
        }))
    .then_some(MacroscopicProcess::SolidSolidSynthesis)
}

fn classifies_explosive_metal_water(
    expanded: &chem_kernel::ExpandedStructuralReaction,
    materials: &[MacroscopicMaterial],
) -> Option<ExplosiveMetalWaterVariant> {
    let reactants = materials
        .iter()
        .filter(|material| material.role == MacroscopicMaterialRole::Reactant)
        .collect::<Vec<_>>();
    let [first, second] = reactants.as_slice() else {
        return None;
    };
    let variant = match (*first, *second) {
        (metal, water)
            if metal.phase == chem_domain::Phase::Solid
                && metal.representation == RepresentationKind::Metallic
                && water.phase == chem_domain::Phase::Liquid
                && water.representation == RepresentationKind::Molecular
                && expanded
                    .claim()
                    .reactants()
                    .get(&water.binding)
                    .is_some_and(|resolved| {
                        has_formula_counts(&resolved.formula, &[("H", 2), ("O", 1)])
                    }) =>
        {
            metal.explosive_water_contact
        }
        (water, metal)
            if metal.phase == chem_domain::Phase::Solid
                && metal.representation == RepresentationKind::Metallic
                && water.phase == chem_domain::Phase::Liquid
                && water.representation == RepresentationKind::Molecular
                && expanded
                    .claim()
                    .reactants()
                    .get(&water.binding)
                    .is_some_and(|resolved| {
                        has_formula_counts(&resolved.formula, &[("H", 2), ("O", 1)])
                    }) =>
        {
            metal.explosive_water_contact
        }
        _ => None,
    }?;
    let products = materials
        .iter()
        .filter(|material| material.role == MacroscopicMaterialRole::Product)
        .collect::<Vec<_>>();
    let [first, second] = products.as_slice() else {
        return None;
    };
    let product_layout = |hydroxide: &MacroscopicMaterial, hydrogen: &MacroscopicMaterial| {
        hydroxide.phase == chem_domain::Phase::Aqueous
            && hydroxide.representation == RepresentationKind::Ionic
            && hydrogen.phase == chem_domain::Phase::Gas
            && hydrogen.representation == RepresentationKind::Molecular
            && expanded
                .claim()
                .products()
                .get(&hydrogen.binding)
                .is_some_and(|resolved| has_formula_counts(&resolved.formula, &[("H", 2)]))
    };
    (product_layout(first, second) || product_layout(second, first)).then_some(variant)
}

fn classifies_catalogue_metal_displacement(
    expanded: &chem_kernel::ExpandedStructuralReaction,
    materials: &[MacroscopicMaterial],
) -> bool {
    if expanded.claim().reactants().len() != 2
        || expanded.claim().products().len() != 2
        || materials.iter().any(|material| {
            material.role == MacroscopicMaterialRole::Product
                && material.phase == chem_domain::Phase::Gas
        })
    {
        return false;
    }
    let phase = |binding: &str, role, expected| {
        materials.iter().any(|material| {
            material.binding == binding && material.role == role && material.phase == expected
        })
    };
    let reactants = expanded.claim().reactants().iter().collect::<Vec<_>>();
    let (original_metal, initial_solution) = match reactants.as_slice() {
        [metal, solution] | [solution, metal]
            if metal.1.representation == RepresentationKind::Metallic
                && phase(
                    metal.0,
                    MacroscopicMaterialRole::Reactant,
                    chem_domain::Phase::Solid,
                )
                && solution.1.representation == RepresentationKind::Ionic
                && phase(
                    solution.0,
                    MacroscopicMaterialRole::Reactant,
                    chem_domain::Phase::Aqueous,
                ) =>
        {
            (*metal, *solution)
        }
        _ => return false,
    };
    let products = expanded.claim().products().iter().collect::<Vec<_>>();
    let (final_solution, deposited_metal) = match products.as_slice() {
        [solution, metal] | [metal, solution]
            if solution.1.representation == RepresentationKind::Ionic
                && phase(
                    solution.0,
                    MacroscopicMaterialRole::Product,
                    chem_domain::Phase::Aqueous,
                )
                && metal.1.representation == RepresentationKind::Metallic
                && phase(
                    metal.0,
                    MacroscopicMaterialRole::Product,
                    chem_domain::Phase::Solid,
                ) =>
        {
            (*solution, *metal)
        }
        _ => return false,
    };
    let single_element = |formula: &BTreeMap<String, u64>| {
        if formula.len() != 1 {
            return None;
        }
        formula.keys().next().cloned()
    };
    let Some(original_symbol) = single_element(&original_metal.1.formula) else {
        return false;
    };
    let Some(deposited_symbol) = single_element(&deposited_metal.1.formula) else {
        return false;
    };
    original_symbol != deposited_symbol
        && final_solution.1.formula.contains_key(&original_symbol)
        && initial_solution.1.formula.contains_key(&deposited_symbol)
}

fn catalogue_combustion_fuel_carbon_count(
    expanded: &chem_kernel::ExpandedStructuralReaction,
) -> Option<u64> {
    let mut reactants = expanded.claim().reactants().iter();
    let first = reactants.next()?;
    let second = reactants.next()?;
    let fuel = if has_formula_counts(&first.1.formula, &[("O", 2)]) {
        second
    } else if has_formula_counts(&second.1.formula, &[("O", 2)]) {
        first
    } else {
        return None;
    };
    fuel.1.formula.get("C").copied()
}

fn classifies_validated_structural_surface_oxidation(
    expanded: &chem_kernel::ExpandedStructuralReaction,
) -> bool {
    let mut reactants = expanded.claim().reactants().iter();
    let Some(first) = reactants.next() else {
        return false;
    };
    let Some(second) = reactants.next() else {
        return false;
    };
    let Some((surface_metal, surface_oxygen)) =
        (if has_formula_counts(&first.1.formula, &[("O", 2)]) {
            Some((second, first))
        } else if has_formula_counts(&second.1.formula, &[("O", 2)]) {
            Some((first, second))
        } else {
            None
        })
    else {
        return false;
    };
    let surface_product = expanded
        .claim()
        .products()
        .iter()
        .next()
        .filter(|_| expanded.claim().products().len() == 1);
    surface_metal.1.representation == RepresentationKind::Metallic
        && surface_oxygen.1.representation == RepresentationKind::Molecular
        && surface_product.is_some_and(|(binding, product)| {
            product.representation == RepresentationKind::Ionic
                && product.formula.contains_key("O")
                && expanded
                    .claim()
                    .evidence()
                    .observations
                    .iter()
                    .any(|observation| {
                        observation.subject_binding == binding.as_str()
                            && observation.predicate == ObservationPredicate::Forms
                    })
        })
}

fn is_carbon_hydrogen_oxygen_fuel(formula: &BTreeMap<String, u64>) -> bool {
    formula
        .keys()
        .all(|element| matches!(element.as_str(), "C" | "H" | "O"))
        && formula.contains_key("C")
        && formula.contains_key("H")
}

fn has_formula_counts(formula: &BTreeMap<String, u64>, expected: &[(&str, u64)]) -> bool {
    formula.len() == expected.len()
        && expected
            .iter()
            .all(|(symbol, count)| formula.get(*symbol) == Some(count))
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
            Self::SystemError(_) => Some("The chemistry reference data is unavailable."),
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

fn representative_oxygen_request(requests: &[ReactionRequest]) -> Option<ReactionRequest> {
    let oxygen_requests = requests
        .iter()
        .copied()
        .filter(|request| request.family() == ReactionFamily::Oxygen)
        .collect::<Vec<_>>();
    let [request] = oxygen_requests.as_slice() else {
        return None;
    };
    requests
        .iter()
        .all(|candidate| {
            matches!(
                candidate.family(),
                ReactionFamily::Oxygen | ReactionFamily::FixedChargeIonPair
            )
        })
        .then_some(*request)
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
    if let Some(request) = representative_oxygen_request(&requests) {
        return DraftResolution::Supported(request);
    }
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
    matches!(atoms, [_])
        || composition_catalogue::reference_preview(atoms.iter().copied()).is_some()
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
    let name = composition_catalogue::reference_preview(subject.iter().copied())
        .and_then(|preview| preview.name)
        .unwrap_or_else(|| composition.formula.to_owned());
    Some(OxygenAssessment {
        subject: name,
        outcome: screening.compound(composition.formula)?.clone(),
    })
}

/// Host-selected macroscopic styling for an exact reference experience. This
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
        .ok_or_else(|| "validated frames exceed the presentation range".to_owned())?;
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
        authorization: chem_presentation::EffectAuthorization::Observation(trigger),
        intensity,
        start_ordinal,
        end_ordinal: last_ordinal,
        surface_oxide_colour: None,
    };
    // Kept as one full-range cue for presentation-plan compatibility. The 3D
    // renderer uses a fixed orthographic angle and only derives framing from
    // vessel scale; no cue changes its pose during playback.
    let camera = vec![CameraCue {
        behaviour: CameraBehaviour::WideEstablishingShot,
        start_ordinal: 0,
        end_ordinal: last_ordinal,
    }];

    let post_process = matches!(
        request.kind,
        ReactionKind::AcidBaseNeutralization { .. }
            | ReactionKind::AcidBicarbonateGasEvolution { .. }
            | ReactionKind::AcidCarbonateGasEvolution { .. }
    )
    .then_some(MacroscopicProcess::SolventEvaporationCrystallization);
    let (objects, effects) = match request.kind {
        ReactionKind::HeavyAlkaliWater { .. } => {
            return Err(
                "heavy-alkali water presentation requires current reviewed macroscopic material facts"
                    .to_owned(),
            );
        }
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
                    visual_evidence.activity,
                ),
                effect(
                    EffectProfile::GasRelease,
                    ObservationPredicate::Evolves,
                    gas_ordinal,
                    visual_evidence.activity,
                ),
                effect(
                    EffectProfile::SurfaceDisturbance,
                    ObservationPredicate::Disappears,
                    disappears_ordinal,
                    visual_evidence.activity,
                ),
                effect(
                    EffectProfile::ObjectShrinkage,
                    ObservationPredicate::Disappears,
                    disappears_ordinal,
                    visual_evidence.activity,
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
                    vessel(AssetProfile::ReactiveMetalWaterAssembly),
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
            let colour = colour.ok_or_else(|| {
                "reference precipitate colour observation has no value".to_owned()
            })?;
            let target = visual_colour(&colour)
                .ok_or_else(|| format!("unsupported reference visual colour `{colour}`"))?;
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
                    vessel(AssetProfile::NeutralisationEvaporationAssembly),
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
                    vessel(AssetProfile::NeutralisationEvaporationAssembly),
                    contents(
                        "acid",
                        "aqueous acid reactant",
                        AppearanceProfile::AqueousColourless,
                    ),
                    contents(
                        "carbonateSalt",
                        "aqueous carbonate reactant",
                        AppearanceProfile::AqueousColourless,
                    ),
                    PresentationObject {
                        id: "carbonDioxide".to_owned(),
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
            let (forms_ordinal, _) = active_observation(frames, ObservationPredicate::Forms)?;
            let (colour_ordinal, colour) =
                active_observation(frames, ObservationPredicate::Colour)?;
            let colour = colour.ok_or_else(|| {
                "reference displaced-halogen colour observation has no value".to_owned()
            })?;
            let target = visual_colour(&colour)
                .ok_or_else(|| format!("unsupported reference visual colour `{colour}`"))?;
            let mut solution = contents(
                "halide-solution",
                "aqueous sodium halide solution",
                AppearanceProfile::AqueousColourless,
            );
            // The displaced halogen dissolves where it forms: the solution
            // itself carries the reviewed colour change.
            solution.colour_transition = Some(PresentationColourTransition {
                subject_binding: "displacedHalogen".to_owned(),
                value: colour,
                target,
                start_ordinal: colour_ordinal,
            });
            (
                vec![vessel(AssetProfile::TestTube), solution],
                vec![
                    effect(
                        EffectProfile::ColourTransition,
                        ObservationPredicate::Colour,
                        colour_ordinal,
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
        ReactionKind::Registry { .. } => {
            // Every registry experience carries reviewed macroscopic material
            // records, so it always compiles through the phase-driven profile.
            return Err(
                "registry presentation requires current reviewed macroscopic material facts"
                    .to_owned(),
            );
        }
    };

    Ok(PresentationProfile {
        id: format!("presentation.ai.{}", request.id()),
        environment: AssetProfile::LaboratoryBench,
        objects,
        effects,
        camera,
        precipitation: None,
        gas_evolution: None,
        metal_displacement: None,
        solid_solid_synthesis: None,
        phase_synthesis: None,
        explosive_metal_water: None,
        post_process,
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
    let profile = if let Some(reaction) = macroscopic {
        compile_phase_driven_profile(frames, reaction).map_err(|error| error.to_string())?
    } else {
        presentation_profile(request, frames)?
    };
    complete_generic_visual_profile(frames, profile).map_err(|error| error.to_string())
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
        .ok_or_else(|| format!("validated frames have no active {predicate:?} observation"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chem_domain::{Phase, RepresentationKind};

    #[test]
    fn experience_registry_has_no_approval_gate() {
        let registry: serde_json::Value =
            serde_json::from_str(include_str!("../../../catalogue/experience-registry.json"))
                .expect("experience registry");
        for experience in registry["experiences"].as_array().expect("experiences") {
            assert!(experience.get("status").is_none());
            assert!(experience.get("trusted").is_none());
            assert!(experience.get("approved").is_none());
        }
    }

    #[test]
    fn missing_review_metadata_changes_provenance_not_simulation_capability() {
        let request = ReactionRequest::DEFAULT;
        let provisional_reference = ReferenceCatalogue::from_json(CATALOGUE).unwrap();
        let outcome = validate_request_source_with_reference(
            request,
            &request.source(),
            Some(&provisional_reference),
        )
        .unwrap();
        assert_eq!(
            outcome.frames.provenance(),
            chem_kernel::DerivationProvenance::Provisional
        );
        assert!(!outcome.frames.frames().is_empty());
    }

    #[test]
    fn every_roll_candidate_resolves_to_runnable_chemistry() {
        let pool = roll_candidates();
        assert_eq!(pool.len(), 9, "every reaction family is rollable");
        for (family, members) in pool {
            assert!(!members.is_empty(), "family {family:?} has no candidates");
            for [first, second] in members {
                let first = standardize_elemental_draft(&first);
                let second = standardize_elemental_draft(&second);
                assert!(
                    matches!(
                        resolve_drafts(&first, &second),
                        DraftResolution::Supported(_)
                            | DraftResolution::Multiple(_)
                            | DraftResolution::Screened(_)
                    ),
                    "family {family:?} candidate {first:?} + {second:?} does not resolve locally"
                );
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
            structure_id: format!("Structures.{binding}"),
            formula: binding.to_owned(),
            role,
            phase,
            representation,
            colour: None,
            explosive_water_contact: None,
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
                process: None,
                fuel_carbon_count: None,
                surface_oxide_colour: None,
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
    fn carbon_oxidation_uses_the_reviewed_standard_phases() {
        let request = ReactionRequest::from_id("oxygen-carbon-oxygen")
            .expect("carbon oxygen experience exists");
        let run = run(request).expect("carbon oxygen validates");
        let reaction = run
            .macroscopic()
            .expect("all participants have reviewed macroscopic states");
        let phase = |binding: &str, role| {
            reaction
                .materials
                .iter()
                .find(|material| material.binding == binding && material.role == role)
                .map(|material| material.phase)
        };

        assert_eq!(
            phase("subject", MacroscopicMaterialRole::Reactant),
            Some(Phase::Solid)
        );
        assert_eq!(
            phase("oxygen", MacroscopicMaterialRole::Reactant),
            Some(Phase::Gas)
        );
        assert_eq!(
            phase("oxide", MacroscopicMaterialRole::Product),
            Some(Phase::Gas)
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
    fn hydrogen_oxidation_uses_the_reviewed_standard_phases() {
        let request = ReactionRequest::from_id("oxygen-hydrogen-oxygen")
            .expect("hydrogen oxygen experience exists");
        let run = run(request).expect("hydrogen oxygen validates");
        let reaction = run
            .macroscopic()
            .expect("all participants have reviewed macroscopic states");
        let phase = |binding: &str, role| {
            reaction
                .materials
                .iter()
                .find(|material| material.binding == binding && material.role == role)
                .map(|material| material.phase)
        };

        assert_eq!(
            phase("subject", MacroscopicMaterialRole::Reactant),
            Some(Phase::Gas)
        );
        assert_eq!(
            phase("oxygen", MacroscopicMaterialRole::Reactant),
            Some(Phase::Gas)
        );
        assert_eq!(
            phase("oxide", MacroscopicMaterialRole::Product),
            Some(Phase::Liquid)
        );
    }

    #[test]
    fn registered_phase_synthesis_examples_select_the_authored_assemblies() {
        for (id, expected_variant, expected_asset) in [
            (
                "covalent-h-i-hi",
                chem_presentation::PhaseSynthesisVariant::SolidGas,
                AssetProfile::SolidGasSynthesisAssembly,
            ),
            (
                "covalent-h-s-h2s",
                chem_presentation::PhaseSynthesisVariant::SolidGas,
                AssetProfile::SolidGasSynthesisAssembly,
            ),
            (
                "covalent-h-cl-hcl",
                chem_presentation::PhaseSynthesisVariant::GasGas,
                AssetProfile::GasGasSynthesisAssembly,
            ),
            (
                "covalent-h-br-hbr",
                chem_presentation::PhaseSynthesisVariant::GasGas,
                AssetProfile::GasGasSynthesisAssembly,
            ),
            (
                "covalent-h-n-nh3",
                chem_presentation::PhaseSynthesisVariant::GasGas,
                AssetProfile::GasGasSynthesisAssembly,
            ),
            (
                "covalent-h-f-hf",
                chem_presentation::PhaseSynthesisVariant::GasGas,
                AssetProfile::GasGasSynthesisAssembly,
            ),
            (
                "covalent-cl-f-clf",
                chem_presentation::PhaseSynthesisVariant::GasGas,
                AssetProfile::GasGasSynthesisAssembly,
            ),
            (
                "covalent-br-cl-brcl",
                chem_presentation::PhaseSynthesisVariant::GasGas,
                AssetProfile::GasGasSynthesisAssembly,
            ),
            (
                "covalent-i-f-if7",
                chem_presentation::PhaseSynthesisVariant::SolidGas,
                AssetProfile::SolidGasSynthesisAssembly,
            ),
        ] {
            let request = ReactionRequest::from_id(id).expect("registered example exists");
            let run = run(request).expect("registered example validates");
            let reaction = run
                .macroscopic()
                .expect("phase-qualified materials resolve from the catalogue");
            let profile =
                presentation_profile_with_catalogue(request, run.frames(), run.macroscopic())
                    .expect("phase-synthesis profile compiles");

            assert_eq!(
                reaction.process,
                Some(match expected_variant {
                    chem_presentation::PhaseSynthesisVariant::SolidGas => {
                        MacroscopicProcess::SolidGasSynthesis
                    }
                    chem_presentation::PhaseSynthesisVariant::GasGas => {
                        MacroscopicProcess::GasGasSynthesis
                    }
                }),
                "{id} must be classified before rendering"
            );
            assert_eq!(
                profile
                    .phase_synthesis
                    .as_ref()
                    .map(|synthesis| synthesis.variant),
                Some(expected_variant),
                "{id} must bind the authored phase-synthesis profile"
            );
            assert!(
                profile.objects.iter().any(|object| {
                    object.role == SceneRole::Vessel && object.asset == expected_asset
                }),
                "{id} must replace the legacy vessel animation"
            );
        }
    }

    #[test]
    fn phase_synthesis_visible_catalogue_colours_reach_exact_material_slots() {
        for (id, expected) in [
            (
                "covalent-h-i-hi",
                VisualColour {
                    red: 62,
                    green: 53,
                    blue: 70,
                },
            ),
            (
                "covalent-h-s-h2s",
                VisualColour {
                    red: 232,
                    green: 196,
                    blue: 55,
                },
            ),
        ] {
            let request = ReactionRequest::from_id(id).expect("solid-gas example exists");
            let run = run(request).expect("solid-gas example validates");
            let profile =
                presentation_profile_with_catalogue(request, run.frames(), run.macroscopic())
                    .expect("solid-gas profile compiles");
            let synthesis = profile
                .phase_synthesis
                .expect("solid-gas material bindings exist");
            assert_eq!(
                synthesis.variant,
                chem_presentation::PhaseSynthesisVariant::SolidGas
            );
            assert_eq!(
                synthesis.reactant_a.colour, expected,
                "{id} solid slot must use its exact catalogue RGB"
            );
            assert_ne!(
                synthesis.reactant_a.colour, synthesis.reactant_b.colour,
                "{id} solid must remain visually distinct from its colourless gas"
            );
        }

        let request =
            ReactionRequest::from_id("covalent-h-cl-hcl").expect("gas-gas example exists");
        let run = run(request).expect("gas-gas example validates");
        let profile = presentation_profile_with_catalogue(request, run.frames(), run.macroscopic())
            .expect("gas-gas profile compiles");
        let synthesis = profile
            .phase_synthesis
            .expect("gas-gas material bindings exist");
        assert_eq!(
            synthesis.variant,
            chem_presentation::PhaseSynthesisVariant::GasGas
        );
        let chlorine = VisualColour {
            red: 202,
            green: 220,
            blue: 112,
        };
        assert!(
            [synthesis.reactant_a.colour, synthesis.reactant_b.colour].contains(&chlorine),
            "the exact chlorine binding must carry its visible catalogue RGB regardless of deterministic binding order"
        );
        assert_ne!(
            synthesis.reactant_a.colour, synthesis.reactant_b.colour,
            "colourless hydrogen and visible chlorine must not collapse to one material colour"
        );

        let request =
            ReactionRequest::from_id("covalent-h-br-hbr").expect("bromine example exists");
        let run = super::run(request).expect("bromine example validates");
        let profile = presentation_profile_with_catalogue(request, run.frames(), run.macroscopic())
            .expect("bromine gas-gas profile compiles");
        let synthesis = profile
            .phase_synthesis
            .expect("bromine gas-gas material bindings exist");
        let bromine_vapour = VisualColour {
            red: 142,
            green: 57,
            blue: 47,
        };
        assert!(
            [synthesis.reactant_a.colour, synthesis.reactant_b.colour].contains(&bromine_vapour),
            "the reacting bromine-vapour slot must carry its reviewed red-brown RGB"
        );
        assert_ne!(
            synthesis.product.colour, bromine_vapour,
            "colourless hydrogen bromide product must replace the bromine-vapour colour"
        );
    }

    #[test]
    fn structurally_validated_metal_oxidation_selects_surface_scene_without_phase_guessing() {
        for id in [
            "oxygen-ni-oxide-2-o1",
            "oxygen-fe-oxide-3-3-o3",
            "oxygen-lithium-oxygen",
            "oxygen-sodium-oxygen",
        ] {
            let request = ReactionRequest::from_id(id).expect("oxygen experience exists");
            let run = run(request).expect("oxygen experience validates");
            let reaction = run
                .macroscopic()
                .expect("structural oxygen fallback supplies renderer inputs");
            assert_eq!(
                reaction.process,
                Some(MacroscopicProcess::SurfaceOxidation),
                "{id} must retain the validated metal-oxidation process"
            );
            // Process classification must not invent phases: the metal and
            // its oxide have no reviewed records and stay Unknown. Molecular
            // oxygen carries its reviewed standard-state record.
            assert!(
                reaction
                    .materials
                    .iter()
                    .filter(|material| material.binding != "oxygen")
                    .all(|material| material.phase == Phase::Unknown),
                "{id} must not turn process classification into a phase claim"
            );
            assert!(
                reaction
                    .materials
                    .iter()
                    .any(|material| material.binding == "oxygen" && material.phase == Phase::Gas),
                "{id} presents molecular oxygen at its reviewed standard phase"
            );
            let profile =
                presentation_profile_with_catalogue(request, run.frames(), run.macroscopic())
                    .expect("surface oxidation profile compiles");

            assert!(profile.effects.iter().any(|effect| {
                effect.effect == EffectProfile::SurfaceOxidation
                    && effect.authorization
                        == chem_presentation::EffectAuthorization::Process(
                            MacroscopicProcess::SurfaceOxidation,
                        )
            }));
            assert!(
                profile
                    .objects
                    .iter()
                    .all(|object| object.role != SceneRole::Vessel)
            );
            assert!(profile.objects.iter().any(|object| {
                object.asset == AssetProfile::MetalChunk && object.role == SceneRole::Reactant
            }));
        }
    }

    #[test]
    fn surface_oxidation_uses_exact_product_bound_colour_and_reviewed_colour_wins() {
        let request = ReactionRequest::from_id("oxygen-sodium-oxygen")
            .expect("reviewed sodium oxidation exists");
        let run = run(request).expect("oxygen experience validates");
        let mut reaction = MacroscopicReaction {
            profile_id: "presentation.test.surface-oxidation".to_owned(),
            equation: request.equation(),
            materials: vec![
                material(
                    "subject",
                    MacroscopicMaterialRole::Reactant,
                    Phase::Solid,
                    RepresentationKind::Metallic,
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
                    Phase::Solid,
                    RepresentationKind::Ionic,
                ),
            ],
            intensity: EffectIntensity::Moderate,
            process: Some(MacroscopicProcess::SurfaceOxidation),
            fuel_carbon_count: None,
            surface_oxide_colour: None,
        };
        let product_binding = reaction
            .materials
            .iter()
            .find(|material| material.role == MacroscopicMaterialRole::Product)
            .expect("oxide product")
            .binding
            .clone();
        reaction.surface_oxide_colour = Some(chem_presentation::SurfaceOxideColour {
            product_binding: product_binding.clone(),
            target: VisualColour {
                red: 0xb9,
                green: 0x42,
                blue: 0x3b,
            },
            authority: chem_presentation::MacroscopicColourAuthority::ModelAsserted,
        });
        let profile = compile_phase_driven_profile(run.frames(), &reaction)
            .expect("model-asserted colour remains product bound");
        let effect = profile
            .effects
            .iter()
            .find(|effect| effect.effect == EffectProfile::SurfaceOxidation)
            .expect("surface effect");
        assert_eq!(
            effect
                .surface_oxide_colour
                .as_ref()
                .expect("enriched colour")
                .target,
            VisualColour {
                red: 0xb9,
                green: 0x42,
                blue: 0x3b,
            }
        );

        reaction
            .materials
            .iter_mut()
            .find(|material| material.binding == product_binding)
            .expect("oxide product")
            .colour = Some(VisualColour {
            red: 0xee,
            green: 0xf1,
            blue: 0xef,
        });
        let profile = compile_phase_driven_profile(run.frames(), &reaction)
            .expect("reviewed colour compiles");
        let colour = profile
            .effects
            .iter()
            .find(|effect| effect.effect == EffectProfile::SurfaceOxidation)
            .and_then(|effect| effect.surface_oxide_colour.as_ref())
            .expect("reviewed colour retained");
        assert_eq!(
            colour.authority,
            chem_presentation::MacroscopicColourAuthority::Reviewed
        );
        assert_eq!(
            colour.target,
            VisualColour {
                red: 0xee,
                green: 0xf1,
                blue: 0xef,
            }
        );
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
        assert!(profile.objects.iter().any(|object| {
            object.role == SceneRole::Vessel
                && object.asset == AssetProfile::AqueousPrecipitationAssembly
        }));
        let precipitation = profile
            .precipitation
            .as_ref()
            .expect("typed precipitation metadata selects the authored assembly");
        assert_eq!(precipitation.precipitate.binding, "silverHalide");
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
    fn forms_alone_does_not_select_the_precipitation_assembly() {
        let request = ReactionRequest::silver_halide_precipitation(Halogen::Bromine);
        let run = run(request).expect("precipitation request validates");
        let mut profile =
            presentation_profile(request, run.frames()).expect("legacy profile compiles");
        profile.effects.retain(|effect| {
            !matches!(
                effect.effect,
                EffectProfile::PrecipitateFormation | EffectProfile::Clouding
            )
        });
        profile.precipitation = None;
        let profile = chem_presentation::complete_generic_visual_profile(run.frames(), profile)
            .expect("generic completion remains conservative");
        assert!(profile.precipitation.is_none());
        assert!(
            profile
                .objects
                .iter()
                .all(|object| { object.asset != AssetProfile::AqueousPrecipitationAssembly })
        );
    }

    #[test]
    fn neutralization_adds_a_separate_evaporation_and_crystallization_process() {
        let request =
            ReactionRequest::acid_base_neutralization(AlkaliMetal::Sodium, Halogen::Chlorine);
        let neutralization_run = run(request).expect("neutralization validates");
        let profile = presentation_profile_with_catalogue(
            request,
            neutralization_run.frames(),
            neutralization_run.macroscopic(),
        )
        .expect("presentation profile");
        assert_eq!(
            profile.post_process,
            Some(MacroscopicProcess::SolventEvaporationCrystallization)
        );
        assert!(profile.objects.iter().any(|object| {
            object.role == SceneRole::Vessel
                && object.asset == AssetProfile::NeutralisationEvaporationAssembly
        }));
        let plan =
            chem_presentation::compile_real_world_plan(neutralization_run.frames(), &profile)
                .expect("separation plan");
        assert_eq!(
            plan.timeline.beats[plan.timeline.beats.len() - 3..]
                .iter()
                .map(|beat| beat.stage)
                .collect::<Vec<_>>(),
            [
                chem_presentation::MacroscopicStage::HeatingPreparation,
                chem_presentation::MacroscopicStage::SolventBoiling,
                chem_presentation::MacroscopicStage::CrystalGrowth,
            ]
        );

        let gas_evolution =
            ReactionRequest::acid_carbonate_gas_evolution(AlkaliMetal::Sodium, Halogen::Chlorine);
        let gas_run = run(gas_evolution).expect("gas evolution validates");
        let gas_profile = presentation_profile_with_catalogue(
            gas_evolution,
            gas_run.frames(),
            gas_run.macroscopic(),
        )
        .expect("gas presentation");
        assert_eq!(
            gas_profile.post_process,
            Some(MacroscopicProcess::SolventEvaporationCrystallization),
            "acid-carbonate neutralisation keeps its gas effect before shared solvent separation"
        );
        assert!(gas_profile.objects.iter().any(|object| {
            object.role == SceneRole::Vessel
                && object.asset == AssetProfile::NeutralisationEvaporationAssembly
        }));
    }

    #[test]
    fn every_reviewed_neutralization_uses_the_shared_authored_assembly() {
        for metal in [
            AlkaliMetal::Lithium,
            AlkaliMetal::Sodium,
            AlkaliMetal::Potassium,
        ] {
            for halogen in [Halogen::Chlorine, Halogen::Bromine, Halogen::Iodine] {
                for request in [
                    ReactionRequest::acid_base_neutralization(metal, halogen),
                    ReactionRequest::acid_bicarbonate_gas_evolution(metal, halogen),
                    ReactionRequest::acid_carbonate_gas_evolution(metal, halogen),
                ] {
                    let neutralization_run = run(request).expect("neutralization validates");
                    let profile = presentation_profile_with_catalogue(
                        request,
                        neutralization_run.frames(),
                        neutralization_run.macroscopic(),
                    )
                    .expect("presentation profile");
                    assert!(profile.objects.iter().any(|object| {
                        object.role == SceneRole::Vessel
                            && object.asset == AssetProfile::NeutralisationEvaporationAssembly
                    }));
                    assert_eq!(
                        profile.post_process,
                        Some(MacroscopicProcess::SolventEvaporationCrystallization)
                    );
                }
            }
        }
    }

    #[test]
    fn every_supported_request_crosses_the_validated_frame_boundary() {
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
                panic!("registered request `{id}` should validate: {error}")
            });
            local_hit_latencies.push(started.elapsed());
            assert!(!run.frames().frames().is_empty());
            assert_eq!(
                run.frames().provenance(),
                chem_kernel::DerivationProvenance::ReviewedReference
            );
            assert_eq!(
                run.frames().result(),
                chem_kernel::ValidationResult::ValidatedWithAssumptions
            );
        }
        assert_eq!(ids.len(), 208);
        assert_eq!(families[&ReactionFamily::AlkaliWater], 6);
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
    fn all_23_unsupported_bindings_are_selected_from_reference_data() {
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
    #[ignore = "diagnostic: prints structure ids for the objectless oxygen scenes"]
    fn print_objectless_oxygen_structures() {
        let targets = [
            "oxygen-hydrogen-oxygen",
            "oxygen-boron-oxygen",
            "oxygen-carbon-oxygen",
            "oxygen-silicon-oxygen",
            "oxygen-phosphorus-oxygen",
            "oxygen-sulfur-oxygen",
            "oxygen-cr-oxide-6-o3",
            "oxygen-mo-oxide-6-o3",
            "oxygen-w-oxide-6-o3",
            "oxygen-mn-oxide-7-7-o7",
            "oxygen-tc-oxide-7-7-o7",
            "oxygen-re-oxide-7-7-o7",
            "oxygen-ru-oxide-8-o4",
            "oxygen-os-oxide-8-o4",
        ];
        for request in requests() {
            if !targets.contains(&request.id().as_str()) {
                continue;
            }
            let run = run(request).expect("oxygen request validates");
            let reaction = run.macroscopic().expect("oxygen scenes carry materials");
            let materials: Vec<String> = reaction
                .materials
                .iter()
                .map(|material| {
                    format!(
                        "{}={} ({:?} {:?})",
                        material.binding,
                        material.structure_id,
                        material.representation,
                        material.phase,
                    )
                })
                .collect();
            println!("{}: {}", request.id(), materials.join(" | "));
        }
    }

    /// Every request's presentation pipeline, pinned as an explicit table.
    ///
    /// A missing catalogue record silently demotes a reaction from the
    /// phase-driven compiler to an authored legacy arm (or, historically, a
    /// generic fallback scene) — the failure mode behind H2+F2 rendering the
    /// wrong animation. This snapshot makes any routing change loud: adding a
    /// species or record now requires declaring which path it lands on.
    #[test]
    fn every_request_resolves_to_its_pinned_presentation_path() {
        let mut actual = String::new();
        for request in requests() {
            let id = request.id();
            let run = run(request).expect("every registered request validates");
            match run.macroscopic() {
                None => {
                    actual.push_str(&format!("{id} | legacy-authored\n"));
                }
                Some(reaction) => {
                    let profile =
                        presentation_profile_with_catalogue(request, run.frames(), Some(reaction))
                            .expect("phase-driven profile compiles");
                    let vessel = profile
                        .objects
                        .iter()
                        .find(|object| object.role == chem_presentation::SceneRole::Vessel)
                        .map_or_else(|| "no-vessel".to_owned(), |object| {
                            format!("{:?}", object.asset)
                        });
                    let process = reaction
                        .process
                        .as_ref()
                        .map_or_else(|| "generic".to_owned(), |process| format!("{process:?}"));
                    actual.push_str(&format!("{id} | phase-driven | {process} | {vessel}\n"));
                }
            }
        }
        let expected = include_str!("presentation_paths.snapshot");
        assert!(
            actual == expected,
            "presentation-path coverage changed. If this routing change is intentional, \
             update crates/chemspec-app/src/presentation_paths.snapshot to:\n{actual}"
        );
    }

    #[test]
    #[ignore = "diagnostic: prints the canonical digest of the packaged review attestation"]
    fn print_review_attestation_digest() {
        let value: serde_json::Value =
            serde_json::from_slice(CATALOGUE_REVIEW).expect("review parses");
        let digest = chem_domain::ContentDigest::of_json(&value).expect("digest computes");
        println!("review digest: {digest}");
    }

    #[test]
    #[ignore = "diagnostic: prints observations for the halogen displacement frames"]
    fn print_halogen_observations() {
        for request in
            requests().filter(|request| request.family() == ReactionFamily::HalogenDisplacement)
        {
            let run = run(request).expect("halogen request validates");
            println!("== {} | {}", request.id(), request.equation());
            for frame in run.frames().frames() {
                for observation in frame.observations() {
                    println!(
                        "  ord {} {:?} {:?} value={:?} subject={:?}",
                        frame.ordinal(),
                        observation.status,
                        observation.predicate,
                        observation.value,
                        observation.subject_binding,
                    );
                }
            }
        }
    }

    #[test]
    #[ignore = "diagnostic: prints scene inventory for every supported reaction"]
    fn print_scene_inventories() {
        for request in requests() {
            let Ok(run) = run(request) else {
                println!("{}: RUN FAILED", request.id());
                continue;
            };
            let Ok(profile) =
                presentation_profile_with_catalogue(request, run.frames(), run.macroscopic())
            else {
                println!("{}: PROFILE FAILED", request.id());
                continue;
            };
            let phases: Vec<String> = run.macroscopic().map_or_else(Vec::new, |reaction| {
                reaction
                    .materials
                    .iter()
                    .map(|material| {
                        format!(
                            "{}:{:?}:{:?}",
                            material.binding, material.role, material.phase
                        )
                    })
                    .collect()
            });
            let objects: Vec<String> = profile
                .objects
                .iter()
                .map(|object| {
                    format!(
                        "{:?}/{:?}@{:?}",
                        object.role, object.asset, object.appearance
                    )
                })
                .collect();
            let assemblies = format!(
                "precip={} gas={} displ={} synth={} process={:?}",
                profile.precipitation.is_some(),
                profile.gas_evolution.is_some(),
                profile.metal_displacement.is_some(),
                profile.solid_solid_synthesis.is_some(),
                profile.post_process,
            );
            let has_vessel = profile
                .objects
                .iter()
                .any(|object| object.role == SceneRole::Vessel);
            let has_contents = profile
                .objects
                .iter()
                .any(|object| object.role == SceneRole::Contents);
            let flag = if has_vessel && !has_contents {
                " <-- VESSEL WITHOUT CONTENTS"
            } else {
                ""
            };
            println!(
                "{}: {} | {} | {}{}",
                request.id(),
                assemblies,
                phases.join(" "),
                objects.join(" "),
                flag
            );
        }
    }

    #[test]
    fn every_supported_request_compiles_a_macroscopic_plan() {
        let mut profile_ids = std::collections::BTreeSet::new();
        for request in requests() {
            let run = run(request).expect("supported request validates");
            let profile =
                presentation_profile_with_catalogue(request, run.frames(), run.macroscopic())
                    .expect("validated observations select a presentation profile");
            assert!(profile_ids.insert(profile.id.clone()));
            assert_eq!(profile.equation, request.equation());
            assert_eq!(profile.camera.len(), 1);
            assert_eq!(profile.camera[0].start_ordinal, 0);
            assert_eq!(
                profile.camera[0].end_ordinal,
                u16::try_from(run.frames().frames().len().saturating_sub(1))
                    .expect("frame count fits presentation ordinal")
            );
            let surface_oxidation = profile
                .effects
                .iter()
                .any(|effect| effect.effect == EffectProfile::SurfaceOxidation);
            assert_eq!(
                profile
                    .objects
                    .iter()
                    .any(|object| object.role == SceneRole::Vessel),
                !surface_oxidation
            );
            assert!(
                !profile.effects.is_empty(),
                "supported reaction `{}` must not compile to an inert macroscopic plan",
                request.id()
            );
            // Dry syntheses (oxygen burns, ion pairs, covalent combinations)
            // legitimately hold no liquid; every other family stages liquid
            // contents.
            if !matches!(
                request.family(),
                ReactionFamily::Oxygen
                    | ReactionFamily::FixedChargeIonPair
                    | ReactionFamily::CovalentCombination
            ) {
                assert!(
                    profile
                        .objects
                        .iter()
                        .any(|object| object.role == SceneRole::Contents),
                    "`{}` must stage liquid contents",
                    request.id()
                );
            }
            // No scene may present a bare vessel: a vessel implies staged
            // matter (contents, reactants, or products).
            if !surface_oxidation {
                assert!(
                    profile
                        .objects
                        .iter()
                        .any(|object| object.role != SceneRole::Vessel),
                    "`{}` stages a vessel with nothing in the scene",
                    request.id()
                );
            }
            let plan = chem_presentation::compile_real_world_plan(run.frames(), &profile)
                .expect("profile effects are bound to validated observations");
            assert_eq!(plan.profile_id, profile.id);
            assert!(!plan.timeline.beats.is_empty());
            assert!(
                plan.annotations.iter().all(|annotation| {
                    annotation.title != "REACTANT CONSUMED"
                        && !annotation.text.contains("reactant is being used up")
                }),
                "`{}` must not emit a reactant-consumption annotation",
                request.id()
            );
            for effect in &profile.effects {
                if matches!(
                    effect.authorization,
                    chem_presentation::EffectAuthorization::Observation(_)
                ) {
                    let (ordinal, _) = active_observation(run.frames(), effect.trigger).expect(
                        "observation-authorized effect trigger activates in validated frames",
                    );
                    assert_eq!(effect.start_ordinal, ordinal);
                }
            }
            for object in &profile.objects {
                let Some(binding) = &object.observation else {
                    continue;
                };
                let (ordinal, value) = active_observation(run.frames(), binding.predicate)
                    .expect("object observation activates in validated frames");
                assert_eq!(object.visible_from_ordinal, ordinal);
                assert_eq!(binding.value, value);
            }
        }
        assert_eq!(profile_ids.len(), 208);
    }

    #[test]
    fn reviewed_heavy_alkali_water_requests_select_exact_typed_variants() {
        for (metal, variant, hydroxide_formula) in [
            (
                HeavyAlkaliMetal::Rubidium,
                ExplosiveMetalWaterVariant::Rubidium,
                "RbOH",
            ),
            (
                HeavyAlkaliMetal::Caesium,
                ExplosiveMetalWaterVariant::Caesium,
                "CsOH",
            ),
            (
                HeavyAlkaliMetal::Francium,
                ExplosiveMetalWaterVariant::Francium,
                "FrOH",
            ),
        ] {
            let request = ReactionRequest::heavy_alkali_water(metal);
            let run = run(request).expect("reviewed heavy-alkali request validates");
            let reaction = run.macroscopic().expect("reviewed material facts project");
            assert_eq!(
                reaction.process,
                Some(MacroscopicProcess::ExplosiveMetalWater(variant))
            );
            assert!(reaction.materials.iter().any(|material| {
                material.role == MacroscopicMaterialRole::Product
                    && material.representation == RepresentationKind::Ionic
                    && material.formula == hydroxide_formula
            }));
            let profile =
                presentation_profile_with_catalogue(request, run.frames(), Some(reaction))
                    .expect("typed macroscopic process compiles");
            assert_eq!(
                profile
                    .explosive_metal_water
                    .as_ref()
                    .map(|visual| visual.variant),
                Some(variant)
            );
            assert!(profile.objects.iter().any(|object| {
                object.role == SceneRole::Vessel
                    && object.asset == AssetProfile::ExplosiveMetalWaterAssembly
            }));
            assert!(
                profile.effects.iter().all(|effect| {
                    !matches!(
                        effect.authorization,
                        chem_presentation::EffectAuthorization::Process(
                            MacroscopicProcess::GasEvolutionLiquidLiquid
                                | MacroscopicProcess::GasEvolutionSolidLiquid
                        )
                    )
                }),
                "the specific validated layout must not silently degrade to generic gas evolution"
            );
        }
    }

    #[test]
    fn high_energy_assembly_rejects_missing_or_conflicting_typed_layouts() {
        let request = ReactionRequest::heavy_alkali_water(HeavyAlkaliMetal::Rubidium);
        let run = run(request).expect("reviewed heavy-alkali request validates");
        let base_reaction = run.macroscopic().expect("reviewed material facts project");
        let mut missing_water = base_reaction.clone();
        missing_water
            .materials
            .retain(|material| material.binding != "water");
        let profile = compile_phase_driven_profile(run.frames(), &missing_water)
            .expect("generic profile construction is conservative");
        assert!(profile.explosive_metal_water.is_none());
        assert!(matches!(
            chem_presentation::compile_real_world_plan(run.frames(), &profile),
            Err(chem_presentation::PlanError::InvalidExplosiveMetalWaterProfile)
        ));

        let mut mismatched_variant = base_reaction.clone();
        let metal = mismatched_variant
            .materials
            .iter_mut()
            .find(|material| {
                material.role == MacroscopicMaterialRole::Reactant
                    && material.representation == RepresentationKind::Metallic
            })
            .expect("metal material");
        metal.explosive_water_contact = Some(ExplosiveMetalWaterVariant::Caesium);
        let profile = compile_phase_driven_profile(run.frames(), &mismatched_variant)
            .expect("generic profile construction is conservative");
        assert!(profile.explosive_metal_water.is_none());
        assert!(matches!(
            chem_presentation::compile_real_world_plan(run.frames(), &profile),
            Err(chem_presentation::PlanError::InvalidExplosiveMetalWaterProfile)
        ));
    }

    #[test]
    fn registry_outcomes_are_selected_without_guessing_between_products() {
        let DraftResolution::Supported(sodium_oxygen) = resolve_drafts(&[11], &[8, 8]) else {
            panic!("elemental oxygen must select its representative oxidation outcome");
        };
        assert_eq!(sodium_oxygen.id(), "oxygen-sodium-oxygen");

        let unrelated_overlap = [sodium_oxygen, ReactionRequest::DEFAULT];
        assert_eq!(representative_oxygen_request(&unrelated_overlap), None);

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
    fn hydrogen_halogen_synthesis_is_not_presented_as_proton_transfer() {
        for (symbol, atomic_number) in [("F", 9), ("Cl", 17), ("Br", 35), ("I", 53)] {
            let requests = requests_for_drafts(&[1, 1], &[atomic_number, atomic_number]);
            let [request] = requests.as_slice() else {
                panic!("H2 + {symbol}2 must select one reviewed outcome");
            };
            let run = run(*request).expect("reviewed hydrogen halide synthesis validates");
            let plan = chem_presentation::compile_educational_plan(
                run.frames(),
                run.declaration().required_context(),
            )
            .expect("educational plan compiles");
            assert!(
                plan.scenes.iter().flat_map(|scene| &scene.cues).all(|cue| {
                    !matches!(
                        cue,
                        chem_presentation::EducationalCue::InterpretProtonTransfer { .. }
                    )
                }),
                "H2 + {symbol}2 must retain bond-cleavage and bond-formation narration"
            );
        }
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
                .expect("reference colour selects a supported appearance");
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
                .expect("reference formation observation activates");
            let (colour_ordinal, value) =
                active_observation(run.frames(), ObservationPredicate::Colour)
                    .expect("reference colour observation activates");
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
        for (metal, expected_activity) in [
            (AlkaliMetal::Lithium, EffectIntensity::Subtle),
            (AlkaliMetal::Sodium, EffectIntensity::Moderate),
        ] {
            let request = ReactionRequest::alkali_water(metal);
            let run = run(request).expect("alkali-water request validates");
            let profile =
                presentation_profile(request, run.frames()).expect("alkali-water profile compiles");
            assert!(profile.objects.iter().any(|object| {
                object.role == SceneRole::Vessel
                    && object.asset == AssetProfile::ReactiveMetalWaterAssembly
            }));
            assert_eq!(
                profile
                    .effects
                    .iter()
                    .find(|effect| effect.effect == EffectProfile::BubbleEmitter)
                    .expect("reviewed fizzing drives the reusable clip")
                    .intensity,
                expected_activity
            );
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
        assert!(profile.objects.iter().any(|object| {
            object.role == SceneRole::Vessel
                && object.asset == AssetProfile::ReactiveMetalWaterAssembly
        }));
        assert_eq!(
            profile
                .effects
                .iter()
                .find(|effect| effect.effect == EffectProfile::BubbleEmitter)
                .expect("reviewed fizzing drives the reusable clip")
                .intensity,
            EffectIntensity::Strong
        );
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
            .expect("reference gas observation activates");
        assert_eq!(flame.start_ordinal, gas_ordinal);
        chem_presentation::compile_real_world_plan(run.frames(), &profile)
            .expect("flame remains gated by a validated observation");
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
