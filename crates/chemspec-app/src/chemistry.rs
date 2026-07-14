//! Application boundary for host-pinned trusted chemistry experiences.
//!
//! The UI may identify an exact supported draft, but every product, bond,
//! observation, and frame below is produced by the language and kernel crates.

use std::sync::LazyLock;

use chem_catalogue::{ObservationPredicate, TrustedCatalogue};
use chem_domain::ContentDigest;
use chem_kernel::{
    CurrentArtifactIdentity, SimulationFrames, expand_trusted, generate_frames, validate_trusted,
};
use chem_presentation::{
    AppearanceProfile, AssetProfile, CameraBehaviour, CameraCue, EffectIntensity, EffectProfile,
    PresentationEffect, PresentationObject, PresentationProfile, PresentationTransform, SceneRole,
    VIRTUAL_ONLY_DISCLOSURE,
};

pub const DISCLOSURE: &str = "Representative educational outcome. The structural sequence is explanatory, not a mechanism claim or laboratory procedure.";

const CATALOGUE: &[u8] =
    include_bytes!("../../../catalogue/trusted/periodic-table-and-alkali-water/catalogue.json");
const ATTESTATION: &[u8] =
    include_bytes!("../../../catalogue/trusted/periodic-table-and-alkali-water/review.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Experience {
    Lithium,
    Sodium,
    Potassium,
}

impl Experience {
    pub const DEFAULT: Self = Self::Lithium;
    pub const ALL: [Self; 3] = [Self::Lithium, Self::Sodium, Self::Potassium];

    #[must_use]
    pub const fn atomic_number(self) -> u8 {
        match self {
            Self::Lithium => 3,
            Self::Sodium => 11,
            Self::Potassium => 19,
        }
    }

    #[must_use]
    pub const fn source_name(self) -> &'static str {
        match self {
            Self::Lithium => "conformance/end-to-end/alkali-water-li-001.chems",
            Self::Sodium => "conformance/end-to-end/alkali-water-na-001.chems",
            Self::Potassium => "conformance/end-to-end/alkali-water-k-001.chems",
        }
    }

    #[must_use]
    pub const fn source(self) -> &'static str {
        match self {
            Self::Lithium => {
                include_str!("../../../conformance/end-to-end/alkali-water-li-001.chems")
            }
            Self::Sodium => {
                include_str!("../../../conformance/end-to-end/alkali-water-na-001.chems")
            }
            Self::Potassium => {
                include_str!("../../../conformance/end-to-end/alkali-water-k-001.chems")
            }
        }
    }

    const fn evidence(self) -> &'static [u8] {
        match self {
            Self::Lithium => {
                include_bytes!(
                    "../../../conformance/observations/alkali-water-li-001.evidence.json"
                )
            }
            Self::Sodium => {
                include_bytes!(
                    "../../../conformance/observations/alkali-water-na-001.evidence.json"
                )
            }
            Self::Potassium => {
                include_bytes!("../../../conformance/observations/alkali-water-k-001.evidence.json")
            }
        }
    }

    #[must_use]
    pub const fn request(self) -> &'static str {
        match self {
            Self::Lithium => "What happens when lithium metal comes into contact with water?",
            Self::Sodium => "What happens when sodium metal comes into contact with water?",
            Self::Potassium => "What happens when potassium metal comes into contact with water?",
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Lithium => "Lithium and water",
            Self::Sodium => "Sodium and water",
            Self::Potassium => "Potassium and water",
        }
    }

    #[must_use]
    pub const fn equation(self) -> &'static str {
        match self {
            Self::Lithium => "2Li + 2H₂O  →  2LiOH + H₂",
            Self::Sodium => "2Na + 2H₂O  →  2NaOH + H₂",
            Self::Potassium => "2K + 2H₂O  →  2KOH + H₂",
        }
    }

    const fn metal_name(self) -> &'static str {
        match self {
            Self::Lithium => "lithium",
            Self::Sodium => "sodium",
            Self::Potassium => "potassium",
        }
    }

    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Lithium => "alkali-water-lithium",
            Self::Sodium => "alkali-water-sodium",
            Self::Potassium => "alkali-water-potassium",
        }
    }
}

#[derive(Debug)]
pub struct TrustedRun {
    frames: SimulationFrames,
    frame_digest: ContentDigest,
}

impl TrustedRun {
    #[must_use]
    pub const fn frames(&self) -> &SimulationFrames {
        &self.frames
    }

    #[must_use]
    pub const fn frame_digest(&self) -> ContentDigest {
        self.frame_digest
    }
}

static TRUSTED_CATALOGUE: LazyLock<Result<TrustedCatalogue, String>> = LazyLock::new(|| {
    TrustedCatalogue::from_canonical_json(CATALOGUE, ATTESTATION).map_err(|error| error.to_string())
});
static LITHIUM_RUN: LazyLock<Result<TrustedRun, String>> =
    LazyLock::new(|| build_run(Experience::Lithium));
static SODIUM_RUN: LazyLock<Result<TrustedRun, String>> =
    LazyLock::new(|| build_run(Experience::Sodium));
static POTASSIUM_RUN: LazyLock<Result<TrustedRun, String>> =
    LazyLock::new(|| build_run(Experience::Potassium));

/// Returns a host-pinned, AI-reviewed experience result.
///
/// The returned frame type cannot be constructed by the application. Failure
/// is retained and shown honestly instead of falling back to UI-authored chemistry.
pub fn run(experience: Experience) -> Result<&'static TrustedRun, &'static str> {
    match experience {
        Experience::Lithium => &LITHIUM_RUN,
        Experience::Sodium => &SODIUM_RUN,
        Experience::Potassium => &POTASSIUM_RUN,
    }
    .as_ref()
    .map_err(String::as_str)
}

fn build_run(experience: Experience) -> Result<TrustedRun, String> {
    let frames = validate_experience_source(experience, experience.source())?;
    let frame_digest = frames.digest().map_err(|error| error.to_string())?;
    Ok(TrustedRun {
        frames,
        frame_digest,
    })
}

/// Parses, expands, validates, and projects source against the exact host-pinned
/// catalogue and the evidence packet for the selected experience.
pub fn validate_experience_source(
    experience: Experience,
    source: &str,
) -> Result<SimulationFrames, String> {
    let catalogue = TRUSTED_CATALOGUE.as_ref().map_err(String::as_str)?;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DraftParticipant {
    Atom(u8),
    Composition(&'static str),
}

/// Recognizes a supported input identity. This selects a request source; it
/// does not select products or construct chemistry.
pub fn experience_for_participants(
    participants: impl IntoIterator<Item = DraftParticipant>,
) -> Option<Experience> {
    let mut actual = participants.into_iter().collect::<Vec<_>>();
    actual.sort_unstable();
    Experience::ALL.into_iter().find(|experience| {
        actual
            == [
                DraftParticipant::Atom(experience.atomic_number()),
                DraftParticipant::Composition("H₂O"),
            ]
    })
}

#[must_use]
pub fn experience_for_drafts(first: &[u8], second: &[u8]) -> Option<Experience> {
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
    experience_for_participants([first, second])
}

#[must_use]
pub fn supports_drafts(first: &[u8], second: &[u8]) -> bool {
    experience_for_drafts(first, second).is_some()
}

/// Host-selected macroscopic styling for an exact trusted experience. This
/// profile can select meshes and effects, but cannot alter chemistry.
#[must_use]
pub fn presentation_profile(experience: Experience, last_ordinal: u16) -> PresentationProfile {
    let transform = |translation, scale| PresentationTransform {
        translation,
        rotation: [0, 0, 0],
        scale,
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
                id: "water".to_owned(),
                asset: AssetProfile::LiquidVolume,
                semantic_identity: "water".to_owned(),
                appearance: AppearanceProfile::Water,
                role: SceneRole::Contents,
                transform: transform([0, -150, 0], [1_000, 850, 1_000]),
                visible_from_ordinal: 0,
            },
            PresentationObject {
                id: experience.metal_name().to_owned(),
                asset: AssetProfile::MetalChunk,
                semantic_identity: format!("{} metal", experience.metal_name()),
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
        equation: experience.equation().to_owned(),
        disclosure: VIRTUAL_ONLY_DISCLOSURE.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_registered_experience_crosses_the_trusted_frame_boundary() {
        for experience in Experience::ALL {
            let run = run(experience).expect("registered run should be trusted");
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
        for (atomic_number, expected) in [
            (3, Experience::Lithium),
            (11, Experience::Sodium),
            (19, Experience::Potassium),
        ] {
            assert_eq!(
                experience_for_drafts(&[atomic_number], &[1, 8, 1]),
                Some(expected)
            );
            assert_eq!(
                experience_for_drafts(&[8, 1, 1], &[atomic_number]),
                Some(expected)
            );
        }
        assert_eq!(experience_for_drafts(&[20], &[1, 1, 8]), None);
        assert_eq!(experience_for_drafts(&[1, 1], &[8, 8]), None);
    }

    #[test]
    fn edited_invalid_source_never_retains_trusted_frames() {
        let error = validate_experience_source(Experience::DEFAULT, "chems 1\n")
            .expect_err("incomplete source must fail");
        assert!(error.contains("CHEMS-X001"));
    }
}
