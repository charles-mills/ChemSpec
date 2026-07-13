use std::{fs, path::PathBuf, process::Command};

use chems_conformance::validate_repository;
use serde_json::{Value, json};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn repository_contract_is_internally_valid() {
    let summary =
        validate_repository(&workspace_root()).expect("repository contract should validate");
    assert_eq!(summary.grammar_productions, 95);
    assert_eq!(summary.components, 14);
    assert_eq!(summary.cases, 12);
    assert!(!summary.is_complete());
}

#[test]
fn partial_suite_reports_incomplete_coverage() {
    let output = Command::new(env!("CARGO_BIN_EXE_chems-conformance"))
        .arg("report")
        .output()
        .expect("conformance runner should start");
    assert_eq!(output.status.code(), Some(3));
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("cases   0; requirements"),
        "coverage table was missing: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("coverage is incomplete"),
        "incomplete result was not explicit: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn manifest_schema_checks_case_evidence_and_identifier_shape() {
    let root = workspace_root();
    let schema: Value = serde_json::from_str(
        &fs::read_to_string(root.join("conformance/manifest.schema.json"))
            .expect("manifest schema should be readable"),
    )
    .expect("manifest schema should be JSON");
    let validator = jsonschema::draft202012::new(&schema).expect("manifest schema should compile");
    let mut manifest: Value = serde_json::from_str(
        &fs::read_to_string(root.join("conformance/manifest.json"))
            .expect("manifest should be readable"),
    )
    .expect("manifest should be JSON");
    manifest["cases"] = json!([{
        "id": "tab-indentation-001",
        "component": "parsing",
        "requirements": ["LEX-009"],
        "source": "conformance/parsing/tab-indentation-001.chems",
        "expected": {
            "state": "malformed",
            "diagnostics": [{
                "code": "CHEMS-L001",
                "severity": "Error",
                "primary_span": {
                    "fixture": "conformance/parsing/tab-indentation-001.chems",
                    "start": 0,
                    "end": 1
                }
            }],
            "formatted_source": "conformance/parsing/tab-indentation-001.formatted.chems"
        }
    }]);
    assert!(validator.is_valid(&manifest));

    let mut string_diagnostic = manifest.clone();
    string_diagnostic["cases"][0]["expected"]["diagnostics"] = json!(["CHEMS-L001"]);
    assert!(!validator.is_valid(&string_diagnostic));

    let mut repeated_hyphen = manifest;
    repeated_hyphen["components"][0]["id"] = json!("bad--component");
    assert!(!validator.is_valid(&repeated_hyphen));
}
