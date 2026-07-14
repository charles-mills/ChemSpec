use std::{fs, path::PathBuf};

use chem_catalogue::ValidatedCatalogueBundle;
use chem_kernel::{
    CurrentArtifactIdentity, FrameError, SimulationFrames, ValidatedStructuralReaction,
    ValidationResult, expand_review_candidate, generate_frames, validate_review_candidate,
};
use serde_json::Value;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture(path: &str) -> Vec<u8> {
    fs::read(root().join(path)).unwrap()
}

#[test]
fn artifact_boundary_fixture_matches_the_public_kernel_contract() {
    let expected: Value = serde_json::from_slice(&fixture(
        "conformance/artifacts/artifact-boundary-001.input.json",
    ))
    .unwrap();
    let catalogue = ValidatedCatalogueBundle::from_json(&fixture(
        "conformance/catalogue/lithium-rule-001.catalogue.json",
    ))
    .unwrap();
    let source = fixture("conformance/expansion/canonical-expansion-001.chems");
    let expanded = expand_review_candidate(
        "conformance/expansion/canonical-expansion-001.chems",
        std::str::from_utf8(&source).unwrap(),
        &catalogue,
        &fixture("conformance/observations/lithium-observations-001.input.json"),
    )
    .unwrap();
    let derivation = validate_review_candidate(&expanded, &catalogue).unwrap();

    assert_eq!(
        derivation.result(),
        ValidationResult::ValidatedWithAssumptions
    );
    assert_eq!(expected["result_state"], "validated_with_assumptions");
    assert_eq!(
        serde_json::to_value(derivation.event_model()).unwrap(),
        expected["mandatory_model_disclosure"]["event"]
    );
    assert_eq!(
        serde_json::to_value(derivation.sequence_model()).unwrap(),
        expected["mandatory_model_disclosure"]["sequence"]
    );
    assert_eq!(
        expected["mandatory_model_disclosure"]["explanatory_sequence_is_not_a_mechanism_claim"],
        true
    );
    assert!(CurrentArtifactIdentity::from_expanded(&expanded).is_ok());
    let serialized = serde_json::to_value(&*derivation).unwrap();
    for field in expected["identity"].as_array().unwrap() {
        let field = field.as_str().unwrap();
        if field == "derivation_digest" {
            assert!(derivation.digest().is_ok());
        } else if field == "evidence_digest" {
            assert_eq!(
                derivation.expanded().claim.evidence.digest,
                expanded.claim.evidence.digest
            );
        } else {
            assert!(serialized.get(field).is_some(), "{field}");
        }
    }
    assert_eq!(expected["stale_when_any_identity_changes"], true);
    assert_eq!(
        expected["runtime_agents_cannot_construct_trusted_output"],
        true
    );

    let _: fn(
        &ValidatedStructuralReaction,
        CurrentArtifactIdentity,
    ) -> Result<SimulationFrames, FrameError> = generate_frames;
}
