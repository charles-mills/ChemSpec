//! Application boundary for host-pinned trusted chemistry experiences.
//!
//! The UI may identify an exact supported draft, but every product, bond,
//! observation, and frame below is produced by the language and kernel crates.

use std::sync::LazyLock;

use chem_catalogue::{OxygenOutcome, TrustedCatalogue, ValidatedOxygenScreening};
use chem_kernel::{
    CurrentArtifactIdentity, SimulationFrames, expand_trusted, generate_frames, validate_trusted,
};
use chem_presentation::{
    AppearanceProfile, AssetProfile, CameraBehaviour, CameraCue, PresentationObject,
    PresentationProfile, PresentationTransform, SceneRole, VIRTUAL_ONLY_DISCLOSURE,
};

pub const DISCLOSURE: &str = "Representative educational outcome. The structural sequence is explanatory, not a mechanism claim or laboratory procedure.";

const CATALOGUE: &[u8] =
    include_bytes!("../../../catalogue/trusted/periodic-table-and-alkali-water/catalogue.json");
const ATTESTATION: &[u8] =
    include_bytes!("../../../catalogue/trusted/periodic-table-and-alkali-water/review.json");
const OXYGEN_SCREENING: &[u8] = include_bytes!("../../../catalogue/oxygen-screening/oxygen.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Experience(usize);

pub struct ExperienceDefinition {
    id: &'static str,
    atomic_number: u8,
    co_reactant_atoms: &'static [u8],
    source_name: &'static str,
    source: &'static str,
    evidence: &'static str,
    request: &'static str,
    equation: &'static str,
    subject_name: &'static str,
}

include!(concat!(env!("OUT_DIR"), "/experience_registry.rs"));

impl Experience {
    pub const DEFAULT: Self = Self(0);

    fn definition(self) -> &'static ExperienceDefinition {
        &EXPERIENCE_DEFINITIONS[self.0]
    }

    #[must_use]
    pub fn atomic_number(self) -> u8 {
        self.definition().atomic_number
    }
    #[must_use]
    pub fn source_name(self) -> &'static str {
        self.definition().source_name
    }
    #[must_use]
    pub fn source(self) -> &'static str {
        self.definition().source
    }
    fn evidence(self) -> &'static [u8] {
        self.definition().evidence.as_bytes()
    }
    #[must_use]
    pub fn request(self) -> &'static str {
        self.definition().request
    }
    #[must_use]
    pub fn name(self) -> String {
        run(self).map_or_else(
            |_| "reaction products".to_owned(),
            |run| crate::nomenclature::product_names(run.frames()),
        )
    }
    #[must_use]
    pub fn equation(self) -> &'static str {
        self.definition().equation
    }
    #[must_use]
    pub fn id(self) -> &'static str {
        self.definition().id
    }
}

pub fn experiences() -> impl ExactSizeIterator<Item = Experience> {
    (0..EXPERIENCE_DEFINITIONS.len()).map(Experience)
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

pub(crate) fn trusted_catalogue() -> Result<&'static TrustedCatalogue, &'static str> {
    TRUSTED_CATALOGUE.as_ref().map_err(String::as_str)
}
static VALIDATED_OXYGEN_SCREENING: LazyLock<Result<ValidatedOxygenScreening, String>> =
    LazyLock::new(|| {
        let catalogue = TRUSTED_CATALOGUE.as_ref().map_err(Clone::clone)?;
        ValidatedOxygenScreening::from_json(OXYGEN_SCREENING, catalogue)
            .map_err(|error| error.to_string())
    });
static RUNS: LazyLock<Vec<Result<TrustedRun, String>>> =
    LazyLock::new(|| experiences().map(build_run).collect());

/// Returns a host-pinned, AI-reviewed experience result.
///
/// The returned frame type cannot be constructed by the application. Failure
/// is retained and shown honestly instead of falling back to UI-authored chemistry.
pub fn run(experience: Experience) -> Result<&'static TrustedRun, &'static str> {
    RUNS.get(experience.0)
        .ok_or("experience registry index is invalid")?
        .as_ref()
        .map_err(String::as_str)
}

fn build_run(experience: Experience) -> Result<TrustedRun, String> {
    let frames = validate_experience_source(experience, experience.source())?;
    Ok(TrustedRun { frames })
}

/// Parses, expands, validates, and projects source against the exact host-pinned
/// catalogue and the evidence packet for the selected experience.
pub fn validate_experience_source(
    experience: Experience,
    source: &str,
) -> Result<SimulationFrames, String> {
    let catalogue = trusted_catalogue()?;
    let expanded = expand_trusted(
        experience.source_name(),
        source,
        catalogue,
        experience.evidence(),
    )
    .map_err(|error| error.to_string())?;
    let current =
        CurrentArtifactIdentity::from_expanded(&expanded).map_err(|error| error.to_string())?;
    let validated = validate_trusted(&expanded, catalogue).map_err(|error| error.to_string())?;
    generate_frames(&validated, current).map_err(|error| error.to_string())
}

#[must_use]
pub fn experiences_for_drafts(first: &[u8], second: &[u8]) -> Vec<Experience> {
    fn sorted(atoms: &[u8]) -> Vec<u8> {
        let mut atoms = atoms.to_vec();
        atoms.sort_unstable();
        atoms
    }

    let first = sorted(first);
    let second = sorted(second);
    experiences()
        .filter(|experience| {
            let definition = experience.definition();
            (first.as_slice() == [experience.atomic_number()]
                && second == definition.co_reactant_atoms)
                || (second.as_slice() == [experience.atomic_number()]
                    && first == definition.co_reactant_atoms)
        })
        .collect()
}

#[must_use]
pub fn supports_drafts(first: &[u8], second: &[u8]) -> bool {
    !experiences_for_drafts(first, second).is_empty()
        || oxygen_assessment_for_drafts(first, second).is_some()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OxygenAssessment {
    pub subject: String,
    pub outcome: OxygenOutcome,
}

/// Screens an element, or a compound already present in the structural
/// catalogue, against elemental oxygen. A representative result is still not
/// a simulation authorization; that requires reviewed structural frames.
#[must_use]
pub fn oxygen_assessment_for_drafts(first: &[u8], second: &[u8]) -> Option<OxygenAssessment> {
    let screening = VALIDATED_OXYGEN_SCREENING.as_ref().ok()?;
    let (subject, oxygen) = if is_oxygen(first) {
        (second, first)
    } else {
        (first, second)
    };
    if !is_oxygen(oxygen) {
        return None;
    }
    if let [atomic_number] = subject {
        let element = crate::elements::by_atomic_number(*atomic_number)?;
        return Some(OxygenAssessment {
            subject: element.name.to_owned(),
            outcome: screening.element(*atomic_number)?.clone(),
        });
    }

    let formula = catalogue_compound_formula(subject)?;
    Some(OxygenAssessment {
        subject: formula.to_owned(),
        outcome: screening.compound(formula)?.clone(),
    })
}

fn is_oxygen(atoms: &[u8]) -> bool {
    atoms == [8, 8]
}

fn catalogue_compound_formula(atoms: &[u8]) -> Option<&'static str> {
    let mut atoms = atoms.to_vec();
    atoms.sort_unstable();
    match atoms.as_slice() {
        [1, 1] => Some("H2"),
        [1, 1, 8] => Some("H2O"),
        [1, 3, 8] => Some("LiOH"),
        [1, 8, 11] => Some("NaOH"),
        [1, 8, 19] => Some("KOH"),
        _ => None,
    }
}

/// Host-selected macroscopic styling for an exact trusted experience. This
/// profile can select meshes and effects, but cannot alter chemistry.
#[must_use]
pub fn presentation_profile(
    experience: Experience,
    last_ordinal: u16,
    _frames: &SimulationFrames,
) -> PresentationProfile {
    let transform = |translation, scale| PresentationTransform {
        translation,
        rotation: [0, 0, 0],
        scale,
    };
    let co_reactant = if experience.definition().co_reactant_atoms == [8, 8] {
        "oxygen"
    } else if experience.definition().co_reactant_atoms == [1, 8, 1] {
        "water"
    } else {
        "co-reactant"
    };
    PresentationProfile {
        id: format!("presentation.ai.{}", experience.id()),
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
                id: "subject".to_owned(),
                asset: AssetProfile::PowderPile,
                semantic_identity: experience.definition().subject_name.to_owned(),
                appearance: AppearanceProfile::LaboratoryNeutral,
                role: SceneRole::Reactant,
                transform: transform([-300, 250, 0], [650, 650, 650]),
                visible_from_ordinal: 0,
            },
            PresentationObject {
                id: "co-reactant".to_owned(),
                asset: if co_reactant == "water" {
                    AssetProfile::LiquidVolume
                } else {
                    AssetProfile::GasCloud
                },
                semantic_identity: co_reactant.to_owned(),
                appearance: if co_reactant == "water" {
                    AppearanceProfile::Water
                } else {
                    AppearanceProfile::LaboratoryNeutral
                },
                role: SceneRole::Reactant,
                transform: transform([300, 250, 0], [650, 650, 650]),
                visible_from_ordinal: 0,
            },
            PresentationObject {
                id: "product".to_owned(),
                asset: AssetProfile::CrystalCluster,
                semantic_identity: format!("product of {}", experience.equation()),
                appearance: AppearanceProfile::LaboratoryNeutral,
                role: SceneRole::Product,
                transform: transform([0, 250, 0], [750, 750, 750]),
                visible_from_ordinal: last_ordinal,
            },
        ],
        effects: Vec::new(),
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
        equation: experience.equation().to_owned(),
        disclosure: VIRTUAL_ONLY_DISCLOSURE.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_registered_experience_crosses_the_trusted_frame_boundary() {
        for experience in experiences() {
            let run = run(experience).unwrap_or_else(|error| {
                panic!(
                    "registered run `{}` should be trusted: {error}",
                    experience.id()
                )
            });
            assert!(!run.frames().frames().is_empty());
            assert_eq!(run.frames().trust(), chem_kernel::DerivationTrust::Trusted);
            assert_eq!(
                run.frames().result(),
                chem_kernel::ValidationResult::ValidatedWithAssumptions
            );
        }
    }

    #[test]
    fn draft_recognition_selects_li_na_or_k_with_water() {
        for atomic_number in [3, 11, 19] {
            let expected = experiences()
                .find(|experience| experience.atomic_number() == atomic_number)
                .expect("registered atomic number");
            assert_eq!(
                experiences_for_drafts(&[atomic_number], &[1, 8, 1]),
                vec![expected]
            );
            assert_eq!(
                experiences_for_drafts(&[8, 1, 1], &[atomic_number]),
                vec![expected]
            );
        }
        assert!(experiences_for_drafts(&[20], &[1, 1, 8]).is_empty());
        assert!(experiences_for_drafts(&[1, 1], &[8, 8]).is_empty());
    }

    #[test]
    fn iron_and_oxygen_exposes_all_three_reviewed_products() {
        let outcomes = experiences_for_drafts(&[26], &[8, 8]);
        assert_eq!(outcomes.len(), 3);
        assert_eq!(
            outcomes
                .iter()
                .map(|experience| experience.equation())
                .collect::<std::collections::BTreeSet<_>>(),
            std::collections::BTreeSet::from([
                "2 Fe + O2 -> 2 FeO",
                "3 Fe + 2 O2 -> Fe3O4",
                "4 Fe + 3 O2 -> 2 Fe2O3",
            ])
        );
    }

    #[test]
    fn product_names_are_derived_from_validated_structures() {
        let names = experiences()
            .map(|experience| (experience.id(), experience.name()))
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(names["oxygen-fe-oxide-2-o1"], "iron(II) oxide");
        assert_eq!(names["oxygen-fe-oxide-3-3-o3"], "iron(III) oxide");
        assert_eq!(names["oxygen-fe-oxide-2-3-3-o4"], "iron(II,III) oxide");
        assert_eq!(names["oxygen-sodium-oxygen"], "sodium peroxide");
        assert_eq!(names["oxygen-potassium-oxygen"], "potassium superoxide");
        assert_eq!(names["oxygen-barium-oxygen"], "barium oxide");
        assert_eq!(names["oxygen-carbon-oxygen"], "carbon dioxide");
        assert_eq!(
            names["oxygen-phosphorus-oxygen"],
            "tetraphosphorus decoxide"
        );
    }

    #[test]
    fn barium_and_oxygen_resolves_the_normal_oxide() {
        let outcomes = experiences_for_drafts(&[56], &[8, 8]);
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].equation(), "2 Ba + O2 -> 2 BaO");
        assert_eq!(outcomes[0].name(), "barium oxide");
        run(outcomes[0]).expect("reviewed BaO structure should produce trusted frames");
    }

    #[test]
    fn fixed_charge_pairs_balance_repeated_atoms_and_run_trusted_frames() {
        let magnesium_fluoride = experiences_for_drafts(&[12], &[9, 9]);
        assert_eq!(magnesium_fluoride.len(), 1);
        assert_eq!(magnesium_fluoride[0].equation(), "Mg + F2 -> MgF2");
        assert_eq!(magnesium_fluoride[0].name(), "magnesium fluoride");
        run(magnesium_fluoride[0]).expect("MgF2 should produce trusted frames");

        let aluminium_sulfide = experiences_for_drafts(&[13], &[16; 8]);
        assert_eq!(aluminium_sulfide.len(), 1);
        assert_eq!(
            aluminium_sulfide[0].equation(),
            "16 Al + 3 S8 -> 8 Al2S3"
        );
        assert_eq!(aluminium_sulfide[0].name(), "aluminium sulfide");
    }

    #[test]
    fn every_element_can_be_screened_with_oxygen_but_unknown_compounds_cannot() {
        for atomic_number in 1..=118 {
            assert!(oxygen_assessment_for_drafts(&[atomic_number], &[8, 8]).is_some());
            assert!(supports_drafts(&[8, 8], &[atomic_number]));
        }
        assert!(oxygen_assessment_for_drafts(&[6, 8], &[8, 8]).is_none());
    }

    #[test]
    fn edited_invalid_source_never_retains_trusted_frames() {
        let error = validate_experience_source(Experience::DEFAULT, "chems 1\n")
            .expect_err("incomplete source must fail");
        assert!(error.contains("CHEMS-X001"));
    }
}
