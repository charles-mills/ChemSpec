//! Application boundary for host-pinned trusted chemistry experiences.
//!
//! The UI may identify an exact supported draft, but every product, bond,
//! observation, and frame below is produced by the language and kernel crates.

use std::sync::LazyLock;

use chem_catalogue::{ObservationPredicate, TrustedCatalogue};
use chem_kernel::{
    CurrentArtifactIdentity, SimulationFrames, expand_trusted, generate_frames, validate_trusted,
};
use chem_presentation::{
    AppearanceProfile, AssetProfile, CameraBehaviour, CameraCue, EffectIntensity, EffectProfile,
    PresentationEffect, PresentationObject, PresentationProfile, PresentationTransform, SceneRole,
    VIRTUAL_ONLY_DISCLOSURE,
};

const CATALOGUE: &[u8] = include_bytes!("../../../catalogue/trusted/core-chemistry/catalogue.json");
const ATTESTATION: &[u8] = include_bytes!("../../../catalogue/trusted/core-chemistry/review.json");

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReactionFamily {
    AlkaliWater,
    SilverHalidePrecipitation,
    AcidBaseNeutralization,
    AcidBicarbonateGasEvolution,
    AcidCarbonateGasEvolution,
    HalogenDisplacement,
}

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
        }
    }

    #[must_use]
    pub fn source_name(self) -> String {
        format!("generated/{}.chems", self.id())
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
        }
    }

    fn evidence(self) -> &'static [u8] {
        match self.family() {
            ReactionFamily::AlkaliWater => ALKALI_WATER_EVIDENCE,
            ReactionFamily::SilverHalidePrecipitation => PRECIPITATION_EVIDENCE,
            ReactionFamily::AcidBaseNeutralization => NEUTRALIZATION_EVIDENCE,
            ReactionFamily::AcidBicarbonateGasEvolution
            | ReactionFamily::AcidCarbonateGasEvolution => GAS_EVOLUTION_EVIDENCE,
            ReactionFamily::HalogenDisplacement => HALOGEN_DISPLACEMENT_EVIDENCE,
        }
    }

    fn alkali_water_metal(self) -> Option<AlkaliMetal> {
        match self.kind {
            ReactionKind::AlkaliWater { metal } => Some(metal),
            _ => None,
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
        }
    }
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

#[derive(Debug)]
pub struct TrustedRun {
    frames: SimulationFrames,
}

impl TrustedRun {
    #[must_use]
    pub const fn frames(&self) -> &SimulationFrames {
        &self.frames
    }
}

static TRUSTED_CATALOGUE: LazyLock<Result<TrustedCatalogue, String>> = LazyLock::new(|| {
    TrustedCatalogue::from_canonical_json(CATALOGUE, ATTESTATION).map_err(|error| error.to_string())
});
/// Returns a host-pinned, AI-reviewed experience result.
///
/// The returned frame type cannot be constructed by the application. Failure
/// is retained and shown honestly instead of falling back to UI-authored chemistry.
pub fn run(request: ReactionRequest) -> Result<TrustedRun, String> {
    build_run(request)
}

fn build_run(request: ReactionRequest) -> Result<TrustedRun, String> {
    let frames = validate_request_source(request, &request.source())?;
    Ok(TrustedRun { frames })
}

/// Parses, expands, validates, and projects source against the exact host-pinned
/// catalogue and the evidence packet for the selected experience.
fn validate_request_source(
    request: ReactionRequest,
    source: &str,
) -> Result<SimulationFrames, String> {
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
    let current =
        CurrentArtifactIdentity::from_expanded(&expanded).map_err(|error| error.to_string())?;
    let validated = validate_trusted(&expanded, catalogue).map_err(|error| error.to_string())?;
    generate_frames(&validated, current).map_err(|error| error.to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DraftParticipant {
    Atom(u8),
    Composition(&'static str),
}

/// Recognizes a supported input identity. This selects a request source; it
/// does not select products or construct chemistry.
pub fn request_for_participants(
    participants: impl IntoIterator<Item = DraftParticipant>,
) -> Option<ReactionRequest> {
    let mut actual = participants.into_iter().collect::<Vec<_>>();
    actual.sort_unstable();
    ReactionRequest::ALL.into_iter().find(|request| {
        let ReactionKind::AlkaliWater { metal } = request.kind else {
            return false;
        };
        actual
            == [
                DraftParticipant::Atom(metal.atomic_number()),
                DraftParticipant::Composition("H₂O"),
            ]
    })
}

#[must_use]
pub fn request_for_drafts(first: &[u8], second: &[u8]) -> Option<ReactionRequest> {
    fn participant(atoms: &[u8]) -> Option<DraftParticipant> {
        let mut atoms = atoms.to_vec();
        atoms.sort_unstable();
        match atoms.as_slice() {
            [3 | 11 | 19] => Some(DraftParticipant::Atom(atoms[0])),
            [1, 1, 8] => Some(DraftParticipant::Composition("H₂O")),
            _ => None,
        }
    }

    let first = participant(first)?;
    let second = participant(second)?;
    request_for_participants([first, second])
}

#[must_use]
pub fn supports_drafts(first: &[u8], second: &[u8]) -> bool {
    request_for_drafts(first, second).is_some()
}

/// Host-selected macroscopic styling for an exact trusted experience. This
/// profile can select meshes and effects, but cannot alter chemistry.
pub fn presentation_profile(
    request: ReactionRequest,
    last_ordinal: u16,
) -> Result<PresentationProfile, String> {
    let metal = request
        .alkali_water_metal()
        .ok_or_else(|| format!("no macroscopic profile is registered for {}", request.id()))?;
    let transform = |translation, scale| PresentationTransform {
        translation,
        rotation: [0, 0, 0],
        scale,
    };
    Ok(PresentationProfile {
        id: format!("presentation.ai.{}", request.id()),
        environment: AssetProfile::LaboratoryBench,
        objects: vec![
            PresentationObject {
                id: "vessel".to_owned(),
                asset: AssetProfile::Beaker,
                semantic_identity: "open reaction vessel".to_owned(),
                appearance: AppearanceProfile::ClearGlass,
                role: SceneRole::Vessel,
                transform: transform([0, 0, 0], [1_100, 1_100, 1_100]),
                visible_from_ordinal: 0,
            },
            PresentationObject {
                id: "water".to_owned(),
                asset: AssetProfile::LiquidVolume,
                semantic_identity: "water".to_owned(),
                appearance: AppearanceProfile::Water,
                role: SceneRole::Contents,
                transform: transform([0, -150, 0], [1_000, 850, 1_000]),
                visible_from_ordinal: 0,
            },
            PresentationObject {
                id: metal.lower_name().to_owned(),
                asset: AssetProfile::MetalChunk,
                semantic_identity: format!("{} metal", metal.lower_name()),
                appearance: AppearanceProfile::AlkaliMetal,
                role: SceneRole::Reactant,
                transform: transform([0, 610, 0], [650, 650, 650]),
                visible_from_ordinal: 0,
            },
            PresentationObject {
                id: "hydrogen".to_owned(),
                asset: AssetProfile::GasCloud,
                semantic_identity: "hydrogen gas".to_owned(),
                appearance: AppearanceProfile::AqueousColourless,
                role: SceneRole::Product,
                transform: transform([180, 930, 0], [600, 600, 600]),
                visible_from_ordinal: last_ordinal.saturating_sub(2),
            },
        ],
        effects: vec![
            PresentationEffect {
                effect: EffectProfile::BubbleEmitter,
                trigger: ObservationPredicate::Evolves,
                intensity: EffectIntensity::Moderate,
                start_ordinal: 1,
                end_ordinal: last_ordinal,
            },
            PresentationEffect {
                effect: EffectProfile::GasRelease,
                trigger: ObservationPredicate::Evolves,
                intensity: EffectIntensity::Moderate,
                start_ordinal: 1,
                end_ordinal: last_ordinal,
            },
            PresentationEffect {
                effect: EffectProfile::SurfaceDisturbance,
                trigger: ObservationPredicate::Disappears,
                intensity: EffectIntensity::Subtle,
                start_ordinal: 1,
                end_ordinal: last_ordinal,
            },
            PresentationEffect {
                effect: EffectProfile::ObjectShrinkage,
                trigger: ObservationPredicate::Disappears,
                intensity: EffectIntensity::Moderate,
                start_ordinal: 1,
                end_ordinal: last_ordinal,
            },
        ],
        camera: vec![
            CameraCue {
                behaviour: CameraBehaviour::WideEstablishingShot,
                start_ordinal: 0,
                end_ordinal: 1,
            },
            CameraCue {
                behaviour: CameraBehaviour::ReactionFocus,
                start_ordinal: 2,
                end_ordinal: last_ordinal.saturating_sub(1),
            },
            CameraCue {
                behaviour: CameraBehaviour::FinalHeroShot,
                start_ordinal: last_ordinal,
                end_ordinal: last_ordinal,
            },
        ],
        equation: request.equation(),
        disclosure: VIRTUAL_ONLY_DISCLOSURE.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_supported_request_crosses_the_trusted_frame_boundary() {
        let mut ids = std::collections::BTreeSet::new();
        let mut families = std::collections::BTreeMap::new();
        for request in ReactionRequest::ALL {
            assert!(ids.insert(request.id()), "request IDs must be unique");
            *families.entry(request.family()).or_insert(0) += 1;
            let source = request.source();
            assert_eq!(
                source,
                request.source(),
                "source authoring must be deterministic"
            );
            let run = run(request).expect("registered request should be trusted");
            assert!(!run.frames().frames().is_empty());
            assert_eq!(run.frames().trust(), chem_kernel::DerivationTrust::Trusted);
            assert_eq!(
                run.frames().result(),
                chem_kernel::ValidationResult::ValidatedWithAssumptions
            );
        }
        assert_eq!(ids.len(), 36);
        assert_eq!(families[&ReactionFamily::AlkaliWater], 3);
        assert_eq!(families[&ReactionFamily::SilverHalidePrecipitation], 3);
        assert_eq!(families[&ReactionFamily::AcidBaseNeutralization], 9);
        assert_eq!(families[&ReactionFamily::AcidBicarbonateGasEvolution], 9);
        assert_eq!(families[&ReactionFamily::AcidCarbonateGasEvolution], 9);
        assert_eq!(families[&ReactionFamily::HalogenDisplacement], 3);
    }

    #[test]
    fn draft_recognition_selects_li_na_or_k_with_water() {
        for (atomic_number, expected) in [
            (3, ReactionRequest::alkali_water(AlkaliMetal::Lithium)),
            (11, ReactionRequest::alkali_water(AlkaliMetal::Sodium)),
            (19, ReactionRequest::alkali_water(AlkaliMetal::Potassium)),
        ] {
            assert_eq!(
                request_for_drafts(&[atomic_number], &[1, 8, 1]),
                Some(expected)
            );
            assert_eq!(
                request_for_drafts(&[8, 1, 1], &[atomic_number]),
                Some(expected)
            );
        }
        assert_eq!(request_for_drafts(&[20], &[1, 1, 8]), None);
        assert_eq!(request_for_drafts(&[1, 1], &[8, 8]), None);
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
    fn macroscopic_profiles_remain_explicit_until_family_profiles_land() {
        assert!(presentation_profile(ReactionRequest::DEFAULT, 4).is_ok());
        assert!(
            presentation_profile(
                ReactionRequest::silver_halide_precipitation(Halogen::Chlorine),
                4
            )
            .is_err()
        );
    }
}
