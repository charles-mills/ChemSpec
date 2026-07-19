use std::{collections::BTreeSet, fs, path::PathBuf};

use chem_catalogue::ValidatedCatalogueBundle;
use chem_kernel::{
    DerivationTrust, ValidationResult, expand_review_candidate, validate_review_candidate,
};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture(path: &str) -> Vec<u8> {
    fs::read(workspace_root().join(path)).expect("fixture should be readable")
}

fn catalogue() -> ValidatedCatalogueBundle {
    ValidatedCatalogueBundle::from_json(&fixture(
        "conformance/catalogue/lithium-rule-001.catalogue.json",
    ))
    .unwrap()
}

fn expansion() -> chem_kernel::ExpandedStructuralReaction {
    let source = fixture("conformance/expansion/canonical-expansion-001.chems");
    expand_review_candidate(
        "conformance/expansion/canonical-expansion-001.chems",
        std::str::from_utf8(&source).unwrap(),
        &catalogue(),
        &fixture("conformance/observations/lithium-observations-001.input.json"),
    )
    .unwrap()
}

#[test]
fn canonical_review_candidate_executes_every_operation_immutably() {
    let expanded = expansion();
    let derivation = validate_review_candidate(&expanded, &catalogue()).unwrap();
    assert_eq!(
        derivation.result(),
        ValidationResult::ValidatedWithAssumptions
    );
    assert_eq!(derivation.trust(), DerivationTrust::ReviewCandidate);
    assert_eq!(derivation.expanded(), &expanded);
    assert_eq!(derivation.states().len(), expanded.operations().len() + 1);
    assert_eq!(derivation.states()[0].ordinal(), 0);
    assert!(derivation.states()[0].operation().is_none());
    assert_eq!(
        derivation.states().last().unwrap().ordinal(),
        u32::try_from(expanded.operations().len()).unwrap()
    );
    let digests = derivation
        .states()
        .iter()
        .map(chem_kernel::StructuralState::digest)
        .collect::<BTreeSet<_>>();
    assert_eq!(digests.len(), derivation.states().len());
    assert!(!derivation.canonical_json().unwrap().is_empty());
    let repeated = validate_review_candidate(&expanded, &catalogue()).unwrap();
    assert_eq!(
        derivation.canonical_json().unwrap(),
        repeated.canonical_json().unwrap()
    );
    assert!(
        String::from_utf8(derivation.canonical_json().unwrap())
            .unwrap()
            .contains("\"trust\":\"review_candidate\"")
    );
}

#[test]
fn complete_canonical_derivation_is_byte_exact() {
    let derivation = validate_review_candidate(&expansion(), &catalogue()).unwrap();
    let expected = fixture("conformance/validation-kernel/canonical-kernel-001.derivation.json");
    let expected = expected.strip_suffix(b"\n").unwrap_or(&expected);
    assert_eq!(derivation.canonical_json().unwrap(), expected);
}
