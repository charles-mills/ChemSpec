//! Application boundary for the canonical trusted chemistry journey.
//!
//! The UI may identify an exact supported draft, but every product, bond,
//! observation, and frame below is produced by the language and kernel crates.

use std::sync::LazyLock;

use chem_catalogue::TrustedCatalogue;
use chem_domain::ContentDigest;
use chem_kernel::{
    CurrentArtifactIdentity, SimulationFrames, expand_trusted, generate_frames, validate_trusted,
};

pub const SOURCE_NAME: &str = "fixtures/lithium-water.chems";
pub const SOURCE: &str = include_str!("../../../conformance/end-to-end/lithium-outcome-001.chems");
pub const REQUEST: &str = "What happens when lithium metal comes into contact with water?";
pub const NAME: &str = "Lithium and water";
pub const EQUATION: &str = "2Li + 2H₂O  →  2LiOH + H₂";
pub const DISCLOSURE: &str = "Representative educational outcome. The structural sequence is explanatory, not a mechanism claim or laboratory procedure.";

const CATALOGUE: &[u8] =
    include_bytes!("../../../conformance/catalogue/lithium-rule-001.catalogue.json");
const ATTESTATION: &[u8] =
    include_bytes!("../../../conformance/catalogue/lithium-rule-001.review.json");
const EVIDENCE: &[u8] =
    include_bytes!("../../../conformance/observations/lithium-observations-001.input.json");

#[derive(Debug)]
pub struct CanonicalRun {
    frames: SimulationFrames,
    frame_digest: ContentDigest,
}

impl CanonicalRun {
    #[must_use]
    pub const fn frames(&self) -> &SimulationFrames {
        &self.frames
    }

    #[must_use]
    pub const fn frame_digest(&self) -> ContentDigest {
        self.frame_digest
    }
}

static CANONICAL_RUN: LazyLock<Result<CanonicalRun, String>> = LazyLock::new(build_canonical_run);

/// Returns the one host-pinned, AI-reviewed canonical result.
///
/// The returned frame type cannot be constructed by the application. Failure
/// is retained and shown honestly instead of falling back to a UI-authored
/// reaction.
pub fn canonical_run() -> Result<&'static CanonicalRun, &'static str> {
    CANONICAL_RUN.as_ref().map_err(String::as_str)
}

fn build_canonical_run() -> Result<CanonicalRun, String> {
    let frames = validate_source(SOURCE)?;
    let frame_digest = frames.digest().map_err(|error| error.to_string())?;
    Ok(CanonicalRun {
        frames,
        frame_digest,
    })
}

/// Parses, expands, validates, and projects the supplied source against the
/// exact host-pinned catalogue and evidence packet.
pub fn validate_source(source: &str) -> Result<SimulationFrames, String> {
    let catalogue = TrustedCatalogue::from_canonical_json(CATALOGUE, ATTESTATION)
        .map_err(|error| error.to_string())?;
    let expanded = expand_trusted(SOURCE_NAME, source, &catalogue, EVIDENCE)
        .map_err(|error| error.to_string())?;
    let current =
        CurrentArtifactIdentity::from_expanded(&expanded).map_err(|error| error.to_string())?;
    let validated = validate_trusted(&expanded, &catalogue).map_err(|error| error.to_string())?;
    generate_frames(&validated, current).map_err(|error| error.to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DraftParticipant {
    Atom(u8),
    Composition(&'static str),
}

/// Recognizes only the canonical input identity. This enables a request; it
/// does not select products or construct chemistry.
pub fn supports_participants(participants: impl IntoIterator<Item = DraftParticipant>) -> bool {
    let mut actual = participants.into_iter().collect::<Vec<_>>();
    actual.sort_unstable();
    actual
        == [
            DraftParticipant::Atom(3),
            DraftParticipant::Composition("H₂O"),
        ]
}

#[must_use]
pub fn supports_drafts(first: &[u8], second: &[u8]) -> bool {
    fn participant(atoms: &[u8]) -> Option<DraftParticipant> {
        let mut atoms = atoms.to_vec();
        atoms.sort_unstable();
        match atoms.as_slice() {
            [3] => Some(DraftParticipant::Atom(3)),
            [1, 1, 8] => Some(DraftParticipant::Composition("H₂O")),
            _ => None,
        }
    }

    supports_participants(
        [participant(first), participant(second)]
            .into_iter()
            .flatten(),
    ) && participant(first).is_some()
        && participant(second).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_journey_crosses_the_trusted_frame_boundary() {
        let run = canonical_run().expect("canonical run should be trusted");
        assert!(!run.frames().frames().is_empty());
        assert_eq!(run.frames().trust(), chem_kernel::DerivationTrust::Trusted);
        assert_eq!(
            run.frames().result(),
            chem_kernel::ValidationResult::ValidatedWithAssumptions
        );
    }

    #[test]
    fn draft_recognition_enables_only_lithium_and_water() {
        assert!(supports_drafts(&[3], &[1, 8, 1]));
        assert!(supports_drafts(&[8, 1, 1], &[3]));
        assert!(!supports_drafts(&[1, 1], &[8, 8]));
        assert!(!supports_drafts(&[6], &[8, 8]));
    }

    #[test]
    fn edited_invalid_source_never_retains_trusted_frames() {
        let error = validate_source("chems 1\n").expect_err("incomplete source must fail");
        assert!(error.contains("CHEMS-X001"));
    }
}
