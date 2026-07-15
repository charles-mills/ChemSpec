use std::{env, fs, path::PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest directory"));
    let root = manifest_dir.join("../..");
    let registry_path = root.join("catalogue/experience-registry.json");
    println!("cargo:rerun-if-changed={}", registry_path.display());
    let registry: serde_json::Value = serde_json::from_slice(
        &fs::read(&registry_path).expect("experience registry must be readable"),
    )
    .expect("experience registry must be valid JSON");
    assert_eq!(
        registry["schema_version"], 1,
        "unsupported experience registry schema"
    );
    let records = registry["experiences"]
        .as_array()
        .expect("experience registry requires experiences");
    assert!(!records.is_empty(), "experience registry cannot be empty");

    let mut generated =
        String::from("pub static EXPERIENCE_DEFINITIONS: &[ExperienceDefinition] = &[\n");
    for record in records {
        let string = |field: &str| {
            record[field]
                .as_str()
                .unwrap_or_else(|| panic!("experience field `{field}` must be a string"))
        };
        let source_path = string("source_path");
        let evidence_path = string("evidence_path");
        let status = string("status");
        assert!(
            matches!(status, "trusted" | "candidate"),
            "experience status must be `trusted` or `candidate`"
        );
        println!(
            "cargo:rerun-if-changed={}",
            root.join(source_path).display()
        );
        println!(
            "cargo:rerun-if-changed={}",
            root.join(evidence_path).display()
        );
        let source = fs::read_to_string(root.join(source_path)).expect("experience source");
        let evidence = fs::read_to_string(root.join(evidence_path)).expect("experience evidence");
        if status == "candidate" {
            continue;
        }
        let atoms = record["co_reactant_atoms"]
            .as_array()
            .expect("co_reactant_atoms must be an array")
            .iter()
            .map(|value| value.as_u64().expect("atomic number").to_string())
            .collect::<Vec<_>>()
            .join(",");
        generated.push_str(&format!(
            "ExperienceDefinition {{ id: {:?}, atomic_number: {}, co_reactant_atoms: &[{}], source_name: {:?}, source: {:?}, evidence: {:?}, request: {:?}, equation: {:?}, subject_name: {:?} }},\n",
            string("id"), record["atomic_number"], atoms, source_path, source, evidence,
            string("request"), string("equation"), string("subject_name")
        ));
    }
    generated.push_str("];\n");
    let output =
        PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR")).join("experience_registry.rs");
    fs::write(output, generated).expect("generated experience registry must be writable");
}
