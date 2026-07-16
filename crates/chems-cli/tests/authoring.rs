use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

use chem_catalogue::{CatalogueErrorCode, TrustedCatalogue};
use chem_domain::ContentDigest;
use serde_json::{Value, json};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn candidate() -> Value {
    serde_json::from_slice(
        &fs::read(
            root().join("catalogue/candidates/periodic-table-and-alkali-water/candidate.json"),
        )
        .unwrap(),
    )
    .unwrap()
}

fn temp_root(label: &str) -> PathBuf {
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "chems-authoring-{label}-{}-{sequence}",
        std::process::id()
    ))
}

fn write_package(path: &Path, candidate: &Value, source: &str) {
    fs::create_dir_all(path).unwrap();
    fs::write(
        path.join("candidate.json"),
        serde_json::to_vec_pretty(candidate).unwrap(),
    )
    .unwrap();
    fs::write(path.join("example.chems"), source).unwrap();
    fs::copy(
        root().join("catalogue/candidates/periodic-table-and-alkali-water/evidence.json"),
        path.join("evidence.json"),
    )
    .unwrap();
}

fn run(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_chems"))
        .args(arguments)
        .current_dir(root())
        .output()
        .unwrap()
}

fn source() -> String {
    fs::read_to_string(
        root().join("catalogue/candidates/periodic-table-and-alkali-water/example.chems"),
    )
    .unwrap()
}

fn precipitation_candidate() -> Value {
    serde_json::from_slice(
        &fs::read(root().join("catalogue/candidates/precipitation-silver-halide/candidate.json"))
            .unwrap(),
    )
    .unwrap()
}

fn precipitation_source() -> String {
    fs::read_to_string(
        root().join("catalogue/candidates/precipitation-silver-halide/example.chems"),
    )
    .unwrap()
}

fn write_precipitation_package(path: &Path, candidate: &Value, source: &str) {
    fs::create_dir_all(path).unwrap();
    fs::write(
        path.join("candidate.json"),
        serde_json::to_vec_pretty(candidate).unwrap(),
    )
    .unwrap();
    fs::write(path.join("example.chems"), source).unwrap();
    fs::copy(
        root().join("catalogue/candidates/precipitation-silver-halide/evidence.json"),
        path.join("evidence.json"),
    )
    .unwrap();
}

#[test]
fn precipitation_candidate_checks_with_the_base_package_and_covers_the_halide_domain() {
    let temporary = temp_root("precipitation");
    fs::create_dir(&temporary).unwrap();
    let output = temporary.join("output");
    let base = root().join("catalogue/candidates/periodic-table-and-alkali-water");
    let precipitation = root().join("catalogue/candidates/precipitation-silver-halide");
    let result = run(&[
        "catalogue",
        "check",
        "--out",
        output.to_str().unwrap(),
        base.to_str().unwrap(),
        precipitation.to_str().unwrap(),
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );

    let candidate = precipitation_candidate();
    let rule = &candidate["generalized_rules"][0];
    assert_eq!(rule["id"], "Rules.SilverHalidePrecipitation");
    let cases = rule["cases"].as_array().unwrap();
    let supported = cases
        .iter()
        .filter(|case| case["status"] == "supported")
        .map(|case| {
            (
                case["when"]["value"].as_str().unwrap(),
                case["observation_compatibility"][1]["value"]
                    .as_str()
                    .unwrap(),
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(
        supported,
        std::collections::BTreeMap::from([("Br", "Cream"), ("Cl", "White"), ("I", "Yellow"),])
    );
    let unsupported = cases
        .iter()
        .find(|case| case["status"] == "unsupported")
        .unwrap();
    assert_eq!(unsupported["when"]["value"], "F");

    let request: Value =
        serde_json::from_slice(&fs::read(output.join("review-request.json")).unwrap()).unwrap();
    assert_eq!(request["status"], "pending-ai-review");
    assert!(
        fs::read(
            output
                .join("inspections/precipitation-silver-halide")
                .join("frames.json")
        )
        .is_ok(),
        "the precipitation example must produce candidate frames"
    );

    let reversed_output = temporary.join("reversed-output");
    let reversed = run(&[
        "catalogue",
        "check",
        "--out",
        reversed_output.to_str().unwrap(),
        precipitation.to_str().unwrap(),
        base.to_str().unwrap(),
    ]);
    assert!(
        reversed.status.success(),
        "{}",
        String::from_utf8_lossy(&reversed.stderr)
    );
    assert_eq!(
        fs::read(output.join("catalogue.digest")).unwrap(),
        fs::read(reversed_output.join("catalogue.digest")).unwrap()
    );
    fs::remove_dir_all(temporary).unwrap();
}

fn precipitation_source_for(halide: &str) -> String {
    let (name, symbol, colour, forms_claim, colour_claim) = match halide {
        "Cl" => ("Chloride", "Cl", "White", "R1", "R2"),
        "Br" => ("Bromide", "Br", "Cream", "R3", "R4"),
        "I" => ("Iodide", "I", "Yellow", "R5", "R6"),
        other => panic!("unsupported precipitation source member {other}"),
    };
    format!(
        "chems 1\nuse catalog ChemSpec.Theoretical@1\nreaction SilverNitrateAndSodium{name} where\n  reactants\n    silverNitrate := 1 of SilverNitrate\n    sodiumHalide := 1 of Sodium{name}\n  products\n    silverHalide := 1 of Silver{name}\n    sodiumNitrate := 1 of SodiumNitrate\n  equation\n    AgNO3[ionic] + Na{symbol}[ionic]\n    -> Ag{symbol}[ionic] + NaNO3[ionic]\n  model\n    event := representative\n    sequence := explanatory\n  observe from Evidence.SilverHalidePrecipitation@1\n    product silverHalide forms claim {forms_claim}\n    product silverHalide has colour {colour} claim {colour_claim}\n  by\n    apply Rules.SilverHalidePrecipitation\n      silverSource := silverNitrate\n      halideSource := sodiumHalide\n      precipitate := silverHalide\n      spectatorSalt := sodiumNitrate\n"
    )
}

#[test]
fn every_supported_silver_halide_executes_with_its_exact_colour() {
    let temporary = temp_root("precipitation-members");
    fs::create_dir(&temporary).unwrap();
    let base = root().join("catalogue/candidates/periodic-table-and-alkali-water");
    for halide in ["Cl", "Br", "I"] {
        let package = temporary.join(format!("member-{halide}"));
        write_precipitation_package(
            &package,
            &precipitation_candidate(),
            &precipitation_source_for(halide),
        );
        let output = temporary.join(format!("output-{halide}"));
        let result = run(&[
            "catalogue",
            "check",
            "--out",
            output.to_str().unwrap(),
            base.to_str().unwrap(),
            package.to_str().unwrap(),
        ]);
        assert!(
            result.status.success(),
            "{halide}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    fs::remove_dir_all(temporary).unwrap();
}

#[test]
fn silver_fluoride_remains_unsupported_rather_than_precipitating() {
    let temporary = temp_root("precipitation-fluoride");
    fs::create_dir(&temporary).unwrap();
    let base = root().join("catalogue/candidates/periodic-table-and-alkali-water");
    let unsupported_source = precipitation_source()
        .replace("SodiumChloride", "SodiumFluoride")
        .replace("NaCl[ionic]", "NaF[ionic]");
    // The precipitate side has no fluoride application (the case is unsupported and
    // therefore never resolves a product), so binding the fluoride salt as the
    // halide source is sufficient to force the unsupported case.
    let package = temporary.join("fluoride");
    write_precipitation_package(&package, &precipitation_candidate(), &unsupported_source);
    let output = temporary.join("output");
    let result = run(&[
        "catalogue",
        "check",
        "--out",
        output.to_str().unwrap(),
        base.to_str().unwrap(),
        package.to_str().unwrap(),
    ]);
    assert!(!result.status.success());
    let error = String::from_utf8_lossy(&result.stderr);
    assert!(error.contains("UnsupportedChemistry"), "{error}");
    assert!(!output.exists());
    fs::remove_dir_all(temporary).unwrap();
}

#[test]
fn physical_candidate_has_all_elements_and_generates_non_promoting_artifacts() {
    let temporary = temp_root("physical");
    fs::create_dir(&temporary).unwrap();
    let output = temporary.join("output");
    let package = root().join("catalogue/candidates/periodic-table-and-alkali-water");
    let result = run(&[
        "catalogue",
        "check",
        "--out",
        output.to_str().unwrap(),
        package.to_str().unwrap(),
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );

    let candidate = candidate();
    assert_eq!(candidate["elements"].as_array().unwrap().len(), 118);
    assert_eq!(candidate["elements"][0]["atomic_number"], 1);
    assert_eq!(candidate["elements"][117]["atomic_number"], 118);
    assert!(
        candidate["elements"]
            .as_array()
            .unwrap()
            .iter()
            .all(|record| {
                record["premise_ids"] == json!(["premise.elements.iupac-periodic-table"])
            })
    );
    let elements = candidate["elements"]
        .as_array()
        .unwrap()
        .iter()
        .map(|record| (record["symbol"].as_str().unwrap(), record))
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(elements["Sc"]["group"], 3);
    assert_eq!(elements["Y"]["group"], 3);
    for disputed in ["La", "Lu", "Ac", "Lr"] {
        assert!(elements[disputed].get("group").is_none(), "{disputed}");
    }

    let catalogue: Value =
        serde_json::from_slice(&fs::read(output.join("catalogue.json")).unwrap()).unwrap();
    let digest = fs::read_to_string(output.join("catalogue.digest")).unwrap();
    assert_eq!(digest.trim(), catalogue["digest"]);
    assert_eq!(
        catalogue["bundle"]["elements"].as_array().unwrap().len(),
        118
    );

    let request: Value =
        serde_json::from_slice(&fs::read(output.join("review-request.json")).unwrap()).unwrap();
    assert_eq!(request["status"], "pending-ai-review");
    assert_eq!(request["promotable"], false);
    assert_eq!(request["catalogue_digest"], catalogue["digest"]);
    assert!(request.get("reviewer").is_none());
    assert_eq!(
        TrustedCatalogue::from_canonical_json(
            &fs::read(output.join("catalogue.json")).unwrap(),
            &fs::read(output.join("review-request.json")).unwrap(),
        )
        .unwrap_err()
        .code(),
        CatalogueErrorCode::InvalidReview
    );

    for (artifact, digest_key) in [
        ("expanded-certificate.json", "expanded_certificate"),
        ("derivation.json", "derivation"),
        ("frames.json", "frames"),
    ] {
        let bytes = fs::read(
            output
                .join("inspections/periodic-table-and-alkali-water")
                .join(artifact),
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["status"], "candidate-inspection-only");
        assert_eq!(value["promotable"], false);
        assert_eq!(
            request["inspections"]["periodic-table-and-alkali-water"][digest_key],
            ContentDigest::sha256(&bytes).to_string()
        );
    }
    fs::remove_dir_all(temporary).unwrap();
}

#[test]
fn host_selected_ai_attestation_promotes_only_the_exact_generated_digest() {
    let temporary = temp_root("promotion");
    fs::create_dir(&temporary).unwrap();
    let output = temporary.join("trusted");
    let packages = displacement_packages();
    let oxygen = root().join("catalogue/candidates/oxygen-reactions");
    let covalent = root().join("catalogue/candidates/covalent-combinations");
    let attestation = root().join("catalogue/reviews/core-chemistry.review.json");
    let result = run(&[
        "catalogue",
        "promote",
        "--out",
        output.to_str().unwrap(),
        "--attestation",
        attestation.to_str().unwrap(),
        packages[0].to_str().unwrap(),
        packages[1].to_str().unwrap(),
        packages[2].to_str().unwrap(),
        packages[3].to_str().unwrap(),
        packages[4].to_str().unwrap(),
        oxygen.to_str().unwrap(),
        covalent.to_str().unwrap(),
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );

    let manifest: Value =
        serde_json::from_slice(&fs::read(output.join("promotion.json")).unwrap()).unwrap();
    assert_eq!(manifest["status"], "host-selected-ai-reviewed");
    assert_eq!(
        manifest["catalogue_digest"],
        fs::read_to_string(output.join("catalogue.digest"))
            .unwrap()
            .trim()
    );
    TrustedCatalogue::from_canonical_json(
        &fs::read(output.join("catalogue.json")).unwrap(),
        &fs::read(output.join("review.json")).unwrap(),
    )
    .expect("the promoted files must match the compiled host trust root");

    let mut wrong_review: Value = serde_json::from_slice(&fs::read(&attestation).unwrap()).unwrap();
    wrong_review["catalogue_digest"] =
        json!("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    let wrong_review_path = temporary.join("wrong-review.json");
    fs::write(
        &wrong_review_path,
        serde_json::to_vec_pretty(&wrong_review).unwrap(),
    )
    .unwrap();
    let rejected_output = temporary.join("rejected");
    let result = run(&[
        "catalogue",
        "promote",
        "--out",
        rejected_output.to_str().unwrap(),
        "--attestation",
        wrong_review_path.to_str().unwrap(),
        packages[0].to_str().unwrap(),
        packages[1].to_str().unwrap(),
        packages[2].to_str().unwrap(),
        packages[3].to_str().unwrap(),
        packages[4].to_str().unwrap(),
        oxygen.to_str().unwrap(),
        covalent.to_str().unwrap(),
    ]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("CHEMS-A041"));
    assert!(!rejected_output.exists());
    fs::remove_dir_all(temporary).unwrap();
}

#[test]
fn shard_order_is_irrelevant_and_duplicates_fail_before_output() {
    let temporary = temp_root("ordering");
    fs::create_dir(&temporary).unwrap();
    let mut left = candidate();
    let mut right = json!({
        "schema_version": 1,
        "id": "second-half",
        "elements": []
    });
    left["id"] = json!("first-half");
    let elements = left["elements"].as_array_mut().unwrap().split_off(59);
    right["elements"] = Value::Array(elements);
    let left_path = temporary.join("left");
    let right_path = temporary.join("right");
    write_package(&left_path, &left, &source());
    write_package(&right_path, &right, &source());

    let first = temporary.join("first-output");
    let second = temporary.join("second-output");
    for (output, packages) in [
        (&first, [&left_path, &right_path]),
        (&second, [&right_path, &left_path]),
    ] {
        let result = run(&[
            "catalogue",
            "check",
            "--out",
            output.to_str().unwrap(),
            packages[0].to_str().unwrap(),
            packages[1].to_str().unwrap(),
        ]);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    assert_eq!(
        fs::read(first.join("catalogue.json")).unwrap(),
        fs::read(second.join("catalogue.json")).unwrap()
    );
    assert_eq!(
        fs::read(first.join("catalogue.digest")).unwrap(),
        fs::read(second.join("catalogue.digest")).unwrap()
    );

    let duplicate_path = temporary.join("duplicate");
    let mut duplicate = right;
    duplicate["id"] = json!("duplicate-element");
    duplicate["elements"] = json!([left["elements"][0].clone()]);
    write_package(&duplicate_path, &duplicate, &source());
    let rejected_output = temporary.join("rejected-output");
    let result = run(&[
        "catalogue",
        "check",
        "--out",
        rejected_output.to_str().unwrap(),
        left_path.to_str().unwrap(),
        duplicate_path.to_str().unwrap(),
    ]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("CHEMS-A005 duplicate element"));
    assert!(!rejected_output.exists());
    fs::remove_dir_all(temporary).unwrap();
}

#[test]
fn package_surface_is_closed_and_unsupported_is_reported_honestly() {
    let temporary = temp_root("closed");
    fs::create_dir(&temporary).unwrap();
    let mut forbidden = candidate();
    forbidden["trust_root"] = json!("candidate-controlled");
    let forbidden_path = temporary.join("forbidden");
    write_package(&forbidden_path, &forbidden, &source());
    let result = run(&[
        "catalogue",
        "check",
        "--out",
        temporary.join("forbidden-output").to_str().unwrap(),
        forbidden_path.to_str().unwrap(),
    ]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("CHEMS-A004"));

    let mut self_reviewed = candidate();
    self_reviewed["id"] = json!("self-reviewed");
    self_reviewed["premises"][0]["review"] = json!({
        "status": "reviewed",
        "reviewers": [{
            "reviewer": "Luna",
            "reviewed_on": "2026-07-14",
            "reference": "self-asserted"
        }]
    });
    let self_reviewed_path = temporary.join("self-reviewed");
    write_package(&self_reviewed_path, &self_reviewed, &source());
    let result = run(&[
        "catalogue",
        "check",
        "--out",
        temporary.join("self-reviewed-output").to_str().unwrap(),
        self_reviewed_path.to_str().unwrap(),
    ]);
    assert!(!result.status.success());
    let error = String::from_utf8_lossy(&result.stderr);
    assert!(error.contains("only provisional premises with no reviewers"));

    let mut alias_collision = candidate();
    alias_collision["id"] = json!("alias-collision");
    alias_collision["structure_applications"][0]["aliases"] = json!(["Water"]);
    let alias_collision_path = temporary.join("alias-collision");
    write_package(&alias_collision_path, &alias_collision, &source());
    let alias_output = temporary.join("alias-output");
    let result = run(&[
        "catalogue",
        "check",
        "--out",
        alias_output.to_str().unwrap(),
        alias_collision_path.to_str().unwrap(),
    ]);
    assert!(!result.status.success());
    let error = String::from_utf8_lossy(&result.stderr);
    assert!(
        error.contains("CHEMS-A005 application alias `Water`"),
        "{error}"
    );
    assert!(!alias_output.exists());

    let unsupported_path = temporary.join("unsupported");
    let unsupported_source = source().replace(
        "apply Rules.AlkaliMetalWithWater",
        "apply Rules.NotInCandidate",
    );
    write_package(&unsupported_path, &candidate(), &unsupported_source);
    let unsupported_output = temporary.join("unsupported-output");
    let result = run(&[
        "catalogue",
        "check",
        "--out",
        unsupported_output.to_str().unwrap(),
        unsupported_path.to_str().unwrap(),
    ]);
    assert!(!result.status.success());
    let error = String::from_utf8_lossy(&result.stderr);
    assert!(error.contains("UnsupportedChemistry"), "{error}");
    assert!(!unsupported_output.exists());

    let extra_path = temporary.join("extra");
    write_package(&extra_path, &candidate(), &source());
    fs::write(extra_path.join("generated.json"), b"{}\n").unwrap();
    let result = run(&[
        "catalogue",
        "check",
        "--out",
        temporary.join("extra-output").to_str().unwrap(),
        extra_path.to_str().unwrap(),
    ]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("CHEMS-A002"));
    fs::remove_dir_all(temporary).unwrap();
}

fn acid_base_candidate() -> Value {
    serde_json::from_slice(
        &fs::read(root().join("catalogue/candidates/acid-base-neutralization/candidate.json"))
            .unwrap(),
    )
    .unwrap()
}

fn acid_base_source() -> String {
    fs::read_to_string(root().join("catalogue/candidates/acid-base-neutralization/example.chems"))
        .unwrap()
}

fn write_acid_base_package(path: &Path, candidate: &Value, source: &str) {
    fs::create_dir_all(path).unwrap();
    fs::write(
        path.join("candidate.json"),
        serde_json::to_vec_pretty(candidate).unwrap(),
    )
    .unwrap();
    fs::write(path.join("example.chems"), source).unwrap();
    fs::copy(
        root().join("catalogue/candidates/acid-base-neutralization/evidence.json"),
        path.join("evidence.json"),
    )
    .unwrap();
}

fn acid_base_packages() -> [PathBuf; 3] {
    [
        root().join("catalogue/candidates/periodic-table-and-alkali-water"),
        root().join("catalogue/candidates/precipitation-silver-halide"),
        root().join("catalogue/candidates/acid-base-neutralization"),
    ]
}

#[test]
fn acid_base_candidate_checks_with_prior_packages_and_reuses_the_salt_template() {
    let temporary = temp_root("acid-base");
    fs::create_dir(&temporary).unwrap();
    let output = temporary.join("output");
    let packages = acid_base_packages();
    let result = run(&[
        "catalogue",
        "check",
        "--out",
        output.to_str().unwrap(),
        packages[0].to_str().unwrap(),
        packages[1].to_str().unwrap(),
        packages[2].to_str().unwrap(),
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );

    let candidate = acid_base_candidate();
    let rule = &candidate["generalized_rules"][0];
    assert_eq!(rule["id"], "Rules.MonoproticAcidHydroxideNeutralization");
    let supported = rule["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["status"] == "supported")
        .unwrap();
    assert_eq!(supported["when"]["values"], json!(["Cl", "Br", "I"]));
    // No new salt template is declared: the rule reuses family 1's
    // Templates.AlkaliMetalHalide product template exactly.
    assert!(
        candidate["structure_templates"]
            .as_array()
            .unwrap()
            .iter()
            .all(|template| template["id"] != "Templates.AlkaliMetalHalide")
    );
    assert!(
        fs::read(
            output
                .join("inspections/acid-base-neutralization")
                .join("frames.json")
        )
        .is_ok()
    );

    let reversed_output = temporary.join("reversed-output");
    let reversed = run(&[
        "catalogue",
        "check",
        "--out",
        reversed_output.to_str().unwrap(),
        packages[2].to_str().unwrap(),
        packages[1].to_str().unwrap(),
        packages[0].to_str().unwrap(),
    ]);
    assert!(
        reversed.status.success(),
        "{}",
        String::from_utf8_lossy(&reversed.stderr)
    );
    assert_eq!(
        fs::read(output.join("catalogue.digest")).unwrap(),
        fs::read(reversed_output.join("catalogue.digest")).unwrap()
    );
    fs::remove_dir_all(temporary).unwrap();
}

fn acid_base_source_for(member: &str, halide: &str) -> String {
    let (member_name, member_symbol) = match member {
        "Li" => ("Lithium", "Li"),
        "Na" => ("Sodium", "Na"),
        "K" => ("Potassium", "K"),
        other => panic!("unsupported alkali member {other}"),
    };
    let halide_name = match halide {
        "F" => "Fluoride",
        "Cl" => "Chloride",
        "Br" => "Bromide",
        "I" => "Iodide",
        other => panic!("unsupported halide {other}"),
    };
    format!(
        "chems 1\nuse catalog ChemSpec.Theoretical@1\nreaction AcidBase{member_name}{halide_name} where\n  reactants\n    acid := 1 of Hydrogen{halide_name}\n    base := 1 of {member_name}Hydroxide\n  products\n    salt := 1 of {member_name}{halide_name}\n    water := 1 of Water\n  equation\n    H{halide}[molecular] + {member_symbol}OH[ionic]\n    -> {member_symbol}{halide}[ionic] + H2O[molecular]\n  model\n    event := representative\n    sequence := explanatory\n  observe from Evidence.AcidBaseNeutralization@1\n    reactant acid disappears claim R1\n    product water forms claim R2\n  by\n    apply Rules.MonoproticAcidHydroxideNeutralization\n      acid := acid\n      base := base\n      saltProduct := salt\n      waterProduct := water\n"
    )
}

#[test]
fn every_acid_base_domain_member_executes_or_reaches_the_explicit_boundary() {
    let temporary = temp_root("acid-base-members");
    fs::create_dir(&temporary).unwrap();
    let prior = acid_base_packages();
    for member in ["Li", "Na", "K"] {
        for halide in ["F", "Cl", "Br", "I"] {
            let package = temporary.join(format!("{member}-{halide}"));
            write_acid_base_package(
                &package,
                &acid_base_candidate(),
                &acid_base_source_for(member, halide),
            );
            let output = temporary.join(format!("output-{member}-{halide}"));
            let result = run(&[
                "catalogue",
                "check",
                "--out",
                output.to_str().unwrap(),
                prior[0].to_str().unwrap(),
                prior[1].to_str().unwrap(),
                package.to_str().unwrap(),
            ]);
            let expected_success = halide != "F";
            assert_eq!(
                result.status.success(),
                expected_success,
                "{member}/{halide}: {}",
                String::from_utf8_lossy(&result.stderr)
            );
            if !expected_success {
                assert!(String::from_utf8_lossy(&result.stderr).contains("UnsupportedChemistry"));
                assert!(!output.exists());
            }
        }
    }
    fs::remove_dir_all(temporary).unwrap();
}

#[test]
fn hydrofluoric_acid_remains_unsupported_as_a_weak_acid() {
    let temporary = temp_root("acid-base-fluoride");
    fs::create_dir(&temporary).unwrap();
    let unsupported_source = acid_base_source()
        .replace("HydrogenChloride", "HydrogenFluoride")
        .replace("HCl[molecular]", "HF[molecular]");
    let package = temporary.join("fluoride");
    write_acid_base_package(&package, &acid_base_candidate(), &unsupported_source);
    let packages = acid_base_packages();
    let output = temporary.join("output");
    let result = run(&[
        "catalogue",
        "check",
        "--out",
        output.to_str().unwrap(),
        packages[0].to_str().unwrap(),
        packages[1].to_str().unwrap(),
        package.to_str().unwrap(),
    ]);
    assert!(!result.status.success());
    let error = String::from_utf8_lossy(&result.stderr);
    assert!(error.contains("UnsupportedChemistry"), "{error}");
    assert!(!output.exists());
    fs::remove_dir_all(temporary).unwrap();
}

fn gas_evolution_candidate() -> Value {
    serde_json::from_slice(
        &fs::read(root().join("catalogue/candidates/acid-carbonate-gas-evolution/candidate.json"))
            .unwrap(),
    )
    .unwrap()
}

fn write_gas_evolution_package(path: &Path, candidate: &Value, source: &str) {
    fs::create_dir_all(path).unwrap();
    fs::write(
        path.join("candidate.json"),
        serde_json::to_vec_pretty(candidate).unwrap(),
    )
    .unwrap();
    fs::write(path.join("example.chems"), source).unwrap();
    fs::copy(
        root().join("catalogue/candidates/acid-carbonate-gas-evolution/evidence.json"),
        path.join("evidence.json"),
    )
    .unwrap();
}

fn gas_evolution_packages() -> [PathBuf; 4] {
    [
        root().join("catalogue/candidates/periodic-table-and-alkali-water"),
        root().join("catalogue/candidates/precipitation-silver-halide"),
        root().join("catalogue/candidates/acid-base-neutralization"),
        root().join("catalogue/candidates/acid-carbonate-gas-evolution"),
    ]
}

#[test]
fn carbonate_family_checks_with_prior_packages_and_executes_the_full_carbonate_example() {
    let temporary = temp_root("gas-evolution");
    fs::create_dir(&temporary).unwrap();
    let output = temporary.join("output");
    let packages = gas_evolution_packages();
    let result = run(&[
        "catalogue",
        "check",
        "--out",
        output.to_str().unwrap(),
        packages[0].to_str().unwrap(),
        packages[1].to_str().unwrap(),
        packages[2].to_str().unwrap(),
        packages[3].to_str().unwrap(),
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );

    let candidate = gas_evolution_candidate();
    let rule_ids = candidate["generalized_rules"]
        .as_array()
        .unwrap()
        .iter()
        .map(|rule| rule["id"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        rule_ids,
        std::collections::BTreeSet::from([
            "Rules.DiproticAcidCarbonateGasEvolution",
            "Rules.MonoproticAcidBicarbonateGasEvolution",
        ])
    );
    assert!(
        fs::read(
            output
                .join("inspections/acid-carbonate-gas-evolution")
                .join("frames.json")
        )
        .is_ok()
    );

    let reversed_output = temporary.join("reversed-output");
    let reversed = run(&[
        "catalogue",
        "check",
        "--out",
        reversed_output.to_str().unwrap(),
        packages[3].to_str().unwrap(),
        packages[2].to_str().unwrap(),
        packages[1].to_str().unwrap(),
        packages[0].to_str().unwrap(),
    ]);
    assert!(
        reversed.status.success(),
        "{}",
        String::from_utf8_lossy(&reversed.stderr)
    );
    assert_eq!(
        fs::read(output.join("catalogue.digest")).unwrap(),
        fs::read(reversed_output.join("catalogue.digest")).unwrap()
    );
    fs::remove_dir_all(temporary).unwrap();
}

fn gas_evolution_source_for(member: &str, halide: &str, carbonate: bool) -> String {
    let (member_name, member_symbol) = match member {
        "Li" => ("Lithium", "Li"),
        "Na" => ("Sodium", "Na"),
        "K" => ("Potassium", "K"),
        other => panic!("unsupported alkali member {other}"),
    };
    let halide_name = match halide {
        "F" => "Fluoride",
        "Cl" => "Chloride",
        "Br" => "Bromide",
        "I" => "Iodide",
        other => panic!("unsupported halide {other}"),
    };
    let (acid_coefficient, source_name, source_formula, salt_coefficient, rule, source_role) =
        if carbonate {
            (
                2,
                format!("{member_name}Carbonate"),
                format!("{member_symbol}2CO3"),
                2,
                "Rules.DiproticAcidCarbonateGasEvolution",
                "carbonateSource",
            )
        } else {
            (
                1,
                format!("{member_name}Bicarbonate"),
                format!("{member_symbol}HCO3"),
                1,
                "Rules.MonoproticAcidBicarbonateGasEvolution",
                "bicarbonateSource",
            )
        };
    format!(
        "chems 1\nuse catalog ChemSpec.Theoretical@1\nreaction GasEvolution{member_name}{halide_name}{source_role} where\n  reactants\n    acid := {acid_coefficient} of Hydrogen{halide_name}\n    carbonateSalt := 1 of {source_name}\n  products\n    carbonDioxide := 1 of CarbonDioxide\n    water := 1 of Water\n    salt := {salt_coefficient} of {member_name}{halide_name}\n  equation\n    {acid_coefficient} H{halide}[molecular] + {source_formula}[ionic]\n    -> {salt_coefficient} {member_symbol}{halide}[ionic] + H2O[molecular] + CO2[molecular]\n  model\n    event := representative\n    sequence := explanatory\n  observe from Evidence.AcidCarbonateGasEvolution@1\n    gas carbonDioxide evolves claim R1\n    reactant acid disappears claim R2\n  by\n    apply {rule}\n      acid := acid\n      {source_role} := carbonateSalt\n      gasProduct := carbonDioxide\n      waterProduct := water\n      saltProduct := salt\n"
    )
}

#[test]
fn every_carbonate_and_bicarbonate_member_executes_or_reaches_the_explicit_boundary() {
    let temporary = temp_root("gas-evolution-members");
    fs::create_dir(&temporary).unwrap();
    let prior = gas_evolution_packages();
    for carbonate in [false, true] {
        for member in ["Li", "Na", "K"] {
            for halide in ["F", "Cl", "Br", "I"] {
                let kind = if carbonate {
                    "carbonate"
                } else {
                    "bicarbonate"
                };
                let package = temporary.join(format!("{kind}-{member}-{halide}"));
                write_gas_evolution_package(
                    &package,
                    &gas_evolution_candidate(),
                    &gas_evolution_source_for(member, halide, carbonate),
                );
                let output = temporary.join(format!("output-{kind}-{member}-{halide}"));
                let result = run(&[
                    "catalogue",
                    "check",
                    "--out",
                    output.to_str().unwrap(),
                    prior[0].to_str().unwrap(),
                    prior[1].to_str().unwrap(),
                    prior[2].to_str().unwrap(),
                    package.to_str().unwrap(),
                ]);
                let expected_success = halide != "F";
                assert_eq!(
                    result.status.success(),
                    expected_success,
                    "{kind}/{member}/{halide}: {}",
                    String::from_utf8_lossy(&result.stderr)
                );
                if !expected_success {
                    assert!(
                        String::from_utf8_lossy(&result.stderr).contains("UnsupportedChemistry")
                    );
                    assert!(!output.exists());
                }
            }
        }
    }
    fs::remove_dir_all(temporary).unwrap();
}

#[test]
fn hydrofluoric_acid_remains_unsupported_for_both_gas_evolution_rules() {
    for carbonate in [false, true] {
        let temporary = temp_root(if carbonate {
            "carbonate-fluoride"
        } else {
            "bicarbonate-fluoride"
        });
        fs::create_dir(&temporary).unwrap();
        let package = temporary.join("fluoride");
        write_gas_evolution_package(
            &package,
            &gas_evolution_candidate(),
            &gas_evolution_source_for("Na", "F", carbonate),
        );
        let packages = gas_evolution_packages();
        let output = temporary.join("output");
        let result = run(&[
            "catalogue",
            "check",
            "--out",
            output.to_str().unwrap(),
            packages[0].to_str().unwrap(),
            packages[1].to_str().unwrap(),
            packages[2].to_str().unwrap(),
            package.to_str().unwrap(),
        ]);
        assert!(!result.status.success());
        let error = String::from_utf8_lossy(&result.stderr);
        assert!(error.contains("UnsupportedChemistry"), "{error}");
        assert!(!output.exists());
        fs::remove_dir_all(temporary).unwrap();
    }
}

fn displacement_candidate() -> Value {
    serde_json::from_slice(
        &fs::read(root().join("catalogue/candidates/single-displacement-halogen/candidate.json"))
            .unwrap(),
    )
    .unwrap()
}

fn write_displacement_package(path: &Path, candidate: &Value, source: &str) {
    fs::create_dir_all(path).unwrap();
    fs::write(
        path.join("candidate.json"),
        serde_json::to_vec_pretty(candidate).unwrap(),
    )
    .unwrap();
    fs::write(path.join("example.chems"), source).unwrap();
    fs::copy(
        root().join("catalogue/candidates/single-displacement-halogen/evidence.json"),
        path.join("evidence.json"),
    )
    .unwrap();
}

fn displacement_packages() -> [PathBuf; 5] {
    [
        root().join("catalogue/candidates/periodic-table-and-alkali-water"),
        root().join("catalogue/candidates/precipitation-silver-halide"),
        root().join("catalogue/candidates/acid-base-neutralization"),
        root().join("catalogue/candidates/acid-carbonate-gas-evolution"),
        root().join("catalogue/candidates/single-displacement-halogen"),
    ]
}

fn displacement_source_for(displacing: &str, displaced: &str) -> String {
    let details = |symbol| match symbol {
        "F" => ("Fluorine", "Fluoride"),
        "Cl" => ("Chlorine", "Chloride"),
        "Br" => ("Bromine", "Bromide"),
        "I" => ("Iodine", "Iodide"),
        other => panic!("unsupported halogen {other}"),
    };
    let (displacing_name, displacing_halide) = details(displacing);
    let (displaced_name, displaced_halide) = details(displaced);
    format!(
        "chems 1\nuse catalog ChemSpec.Theoretical@1\nreaction {displacing_name}Displaces{displaced_name} where\n  reactants\n    displacingHalogen := 1 of {displacing_name}\n    saltSource := 2 of Sodium{displaced_halide}\n  products\n    newSalt := 2 of Sodium{displacing_halide}\n    displacedHalogen := 1 of {displaced_name}\n  equation\n    {displacing}2[molecular] + 2 Na{displaced}[ionic]\n    -> 2 Na{displacing}[ionic] + {displaced}2[molecular]\n  model\n    event := representative\n    sequence := explanatory\n  observe from Evidence.HalogenDisplacement@1\n    product displacedHalogen forms claim R1\n  by\n    apply Rules.HalogenDisplacement\n      displacingHalogen := displacingHalogen\n      saltSource := saltSource\n      newSalt := newSalt\n      displacedHalogen := displacedHalogen\n"
    )
}

#[test]
fn halogen_displacement_candidate_checks_with_all_prior_packages() {
    let temporary = temp_root("displacement");
    fs::create_dir(&temporary).unwrap();
    let output = temporary.join("output");
    let packages = displacement_packages();
    let result = run(&[
        "catalogue",
        "check",
        "--out",
        output.to_str().unwrap(),
        packages[0].to_str().unwrap(),
        packages[1].to_str().unwrap(),
        packages[2].to_str().unwrap(),
        packages[3].to_str().unwrap(),
        packages[4].to_str().unwrap(),
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let candidate = displacement_candidate();
    assert_eq!(
        candidate["generalized_rules"][0]["id"],
        "Rules.HalogenDisplacement"
    );
    assert!(
        fs::read(
            output
                .join("inspections/single-displacement-halogen")
                .join("frames.json")
        )
        .is_ok()
    );

    let reversed_output = temporary.join("reversed-output");
    let reversed = run(&[
        "catalogue",
        "check",
        "--out",
        reversed_output.to_str().unwrap(),
        packages[4].to_str().unwrap(),
        packages[3].to_str().unwrap(),
        packages[2].to_str().unwrap(),
        packages[1].to_str().unwrap(),
        packages[0].to_str().unwrap(),
    ]);
    assert!(
        reversed.status.success(),
        "{}",
        String::from_utf8_lossy(&reversed.stderr)
    );
    assert_eq!(
        fs::read(output.join("catalogue.digest")).unwrap(),
        fs::read(reversed_output.join("catalogue.digest")).unwrap()
    );
    fs::remove_dir_all(temporary).unwrap();
}

#[test]
fn every_halogen_displacement_pair_executes_or_reaches_an_explicit_boundary() {
    let temporary = temp_root("displacement-members");
    fs::create_dir(&temporary).unwrap();
    let prior = displacement_packages();
    let supported = std::collections::BTreeSet::from([("Cl", "Br"), ("Cl", "I"), ("Br", "I")]);
    for displacing in ["F", "Cl", "Br", "I"] {
        for displaced in ["F", "Cl", "Br", "I"] {
            let package = temporary.join(format!("{displacing}-{displaced}"));
            write_displacement_package(
                &package,
                &displacement_candidate(),
                &displacement_source_for(displacing, displaced),
            );
            let output = temporary.join(format!("output-{displacing}-{displaced}"));
            let result = run(&[
                "catalogue",
                "check",
                "--out",
                output.to_str().unwrap(),
                prior[0].to_str().unwrap(),
                prior[1].to_str().unwrap(),
                prior[2].to_str().unwrap(),
                prior[3].to_str().unwrap(),
                package.to_str().unwrap(),
            ]);
            let expected_success = supported.contains(&(displacing, displaced));
            assert_eq!(
                result.status.success(),
                expected_success,
                "{displacing}/{displaced}: {}",
                String::from_utf8_lossy(&result.stderr)
            );
            if !expected_success {
                assert!(String::from_utf8_lossy(&result.stderr).contains("UnsupportedChemistry"));
                assert!(!output.exists());
            }
        }
    }
    fs::remove_dir_all(temporary).unwrap();
}

#[test]
fn less_reactive_halogen_cannot_displace_a_more_reactive_halide() {
    let temporary = temp_root("displacement-reversed");
    fs::create_dir(&temporary).unwrap();
    let package = temporary.join("reversed");
    write_displacement_package(
        &package,
        &displacement_candidate(),
        &displacement_source_for("Br", "Cl"),
    );
    let packages = displacement_packages();
    let output = temporary.join("output");
    let result = run(&[
        "catalogue",
        "check",
        "--out",
        output.to_str().unwrap(),
        packages[0].to_str().unwrap(),
        packages[1].to_str().unwrap(),
        packages[2].to_str().unwrap(),
        packages[3].to_str().unwrap(),
        package.to_str().unwrap(),
    ]);
    assert!(!result.status.success());
    let error = String::from_utf8_lossy(&result.stderr);
    assert!(error.contains("UnsupportedChemistry"), "{error}");
    assert!(!output.exists());
    fs::remove_dir_all(temporary).unwrap();
}
