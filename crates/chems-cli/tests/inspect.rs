use std::{path::PathBuf, process::Command};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn source_and_expansion_inspection_are_visible_and_non_promoting() {
    let root = root();
    let source = root.join("conformance/expansion/canonical-expansion-001.chems");
    let source_output = Command::new(env!("CARGO_BIN_EXE_chems"))
        .args(["inspect", "source"])
        .arg(&source)
        .output()
        .unwrap();
    assert!(source_output.status.success());
    let source_text = String::from_utf8(source_output.stdout).unwrap();
    assert!(source_text.contains("\"inspection\": \"authored_source\""));
    assert!(source_text.contains("\"source_bytes_digest\""));

    let expanded_output = Command::new(env!("CARGO_BIN_EXE_chems"))
        .args(["inspect", "expanded"])
        .arg(&source)
        .arg("--catalogue")
        .arg(root.join("conformance/catalogue/lithium-rule-001.catalogue.json"))
        .arg("--evidence")
        .arg(root.join("conformance/observations/lithium-observations-001.input.json"))
        .output()
        .unwrap();
    assert!(expanded_output.status.success());
    let expanded_text = String::from_utf8(expanded_output.stdout).unwrap();
    assert!(expanded_text.contains("status: unexecuted"));
    assert!(expanded_text.contains("trust=ReviewCandidate"));
}

#[test]
fn generalized_expansion_inspection_exposes_selection_and_provenance() {
    let root = root();
    let output = Command::new(env!("CARGO_BIN_EXE_chems"))
        .args(["inspect", "expanded"])
        .arg(root.join("conformance/end-to-end/alkali-water-li-001.chems"))
        .arg("--catalogue")
        .arg(root.join("conformance/catalogue/alkali-metal-water-001.catalogue.json"))
        .arg("--evidence")
        .arg(root.join("conformance/observations/alkali-water-li-001.evidence.json"))
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    for expected in [
        "rule: Rules.AlkaliMetalWithWater",
        "generalized: parameters={\"member\": \"Li\"} case=standard equivalent_matches=4",
        "matched_sites:",
        "parameter_premises:",
        "role_premises:",
        "structure=LithiumMetal",
        "structure=LithiumHydroxide",
        "premise.rule.alkali-metal-water.standard-outcome",
    ] {
        assert!(
            text.contains(expected),
            "missing `{expected}` from:\n{text}"
        );
    }
}
