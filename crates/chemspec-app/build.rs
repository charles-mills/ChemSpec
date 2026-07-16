use std::{collections::BTreeSet, env, fmt::Write as _, fs, path::PathBuf};

fn optional_string(record: &serde_json::Value, field: &str) -> String {
    record[field]
        .as_str()
        .map_or_else(|| "None".to_owned(), |value| format!("Some({value:?})"))
}

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
        registry["schema_version"], 2,
        "unsupported experience registry schema"
    );
    let records = registry["experiences"]
        .as_array()
        .expect("experience registry requires experiences");
    assert!(!records.is_empty(), "experience registry cannot be empty");

    let mut ids = BTreeSet::new();
    let mut generated =
        String::from("static EXPERIENCE_DEFINITIONS: &[ExperienceDefinition] = &[\n");
    for record in records {
        let string = |field: &str| {
            record[field]
                .as_str()
                .unwrap_or_else(|| panic!("experience field `{field}` must be a string"))
        };
        let id = string("id");
        assert!(ids.insert(id), "duplicate experience id `{id}`");
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

        let participants = record["participants"]
            .as_array()
            .expect("experience participants must be an array");
        assert_eq!(
            participants.len(),
            2,
            "experiences require two participants"
        );
        let participants = participants
            .iter()
            .map(|participant| match participant["kind"].as_str() {
                Some("element") => {
                    let atomic_number = participant["atomic_number"]
                        .as_u64()
                        .expect("element participant requires atomic_number");
                    assert!(
                        (1..=118).contains(&atomic_number),
                        "participant atomic number must be 1 through 118"
                    );
                    format!("ExperienceParticipantDefinition::Element({atomic_number})")
                }
                Some("composition") => format!(
                    "ExperienceParticipantDefinition::Composition({:?})",
                    participant["formula"]
                        .as_str()
                        .expect("composition participant requires formula")
                ),
                _ => panic!("unsupported experience participant kind"),
            })
            .collect::<Vec<_>>();
        let family = match string("family") {
            "oxygen" => "ReactionFamily::Oxygen",
            "fixed_charge_ion_pair" => "ReactionFamily::FixedChargeIonPair",
            "covalent_combination" => "ReactionFamily::CovalentCombination",
            family => panic!("unsupported experience family `{family}`"),
        };
        writeln!(
            generated,
            "ExperienceDefinition {{ id: {id:?}, family: {family}, participants: [{}, {}], source_name: {source_path:?}, source: {source:?}, evidence: {evidence:?}, equation: {:?}, subject_name: {:?}, product_name: {}, product_structure: {} }},",
            participants[0],
            participants[1],
            string("equation"),
            string("subject_name"),
            optional_string(record, "product_name"),
            optional_string(record, "product_structure"),
        )
        .expect("experience definition is writable");
    }
    generated.push_str("];\n");
    let output =
        PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR")).join("experience_registry.rs");
    fs::write(output, generated).expect("generated experience registry must be writable");
}
