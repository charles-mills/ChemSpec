use std::{fs, path::PathBuf};

use chem_catalogue::{CatalogueErrorCode, ValidatedCatalogueBundle};
use chem_domain::{CovalentElectronOrigin, StructureId};
use chem_kernel::{
    ExpansionFailureClass, KernelFailureClass, ValidationResult, expand_review_candidate,
    validate_review_candidate,
};
use serde_json::{Value, json};

#[path = "generalized_g4.rs"]
mod generalized_g4;

use generalized_g4::{
    generalized_catalogue_value, generalized_dative_catalogue, generalized_evidence, member_source,
    validate_catalogue_value,
};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn migrated_catalogue_value() -> Value {
    let mut value = generalized_catalogue_value();
    value["bundle"]["rules"] = json!([]);
    rewrite_catalogue_strings(
        &mut value,
        &[
            ("Categories.G4Alkali", "Categories.AlkaliMetal"),
            ("Templates.G4Metal", "Templates.AlkaliMetal"),
            ("Templates.G4Hydroxide", "Templates.AlkaliMetalHydroxide"),
            ("Patterns.G4LithiumMetal", "Patterns.AlkaliMetal"),
            ("Patterns.G4Water", "Patterns.Water"),
            (
                "premise.structure.lithium-metal",
                "premise.structure.alkali-metal",
            ),
            (
                "premise.structure.lithium-hydroxide",
                "premise.structure.alkali-metal-hydroxide",
            ),
            (
                "premise.rule.lithium-water.standard-outcome",
                "premise.rule.alkali-metal-water.standard-outcome",
            ),
            (
                "premise.valence.li-h-o.initial-domain",
                "premise.valence.alkali-h-o.initial-domain",
            ),
            (
                "premise.observation.lithium-disappears",
                "premise.observation.alkali-metal-disappears",
            ),
            (".lithium.li", ".metal.metal"),
            (".li", ".metal"),
        ],
    );
    rewrite_exact_catalogue_string(&mut value, "li", "metal");
    rewrite_exact_catalogue_string(&mut value, "lithium", "metal");
    let metal_variable = value["bundle"]["graph_patterns"][0]["variables"]
        .as_object_mut()
        .unwrap()
        .remove("li")
        .unwrap();
    value["bundle"]["graph_patterns"][0]["variables"]
        .as_object_mut()
        .unwrap()
        .insert("metal".to_owned(), metal_variable);
    value["bundle"]["created"]["notes"] =
        json!("Closed alkali-metal-water educational outcome catalogue");
    value["bundle"]["generalized_rules"][0]["cases"][0]["id"] = json!("standard");
    for premise in value["bundle"]["premises"].as_array_mut().unwrap() {
        match premise["id"].as_str() {
            Some("premise.structure.alkali-metal") => {
                premise["statement"] = json!(
                    "The representative alkali-metal fragment is an M+ site core with one domain-owned delocalized valence electron."
                );
            }
            Some("premise.structure.alkali-metal-hydroxide") => {
                premise["statement"] = json!(
                    "Each supported alkali-metal hydroxide is an ionic association of M+ and covalently bonded OH- components."
                );
            }
            Some("premise.rule.alkali-metal-water.standard-outcome") => {
                premise["statement"] = json!(
                    "Contact between Li, Na, or K metal and water has the reviewed representative outcome 2 M + 2 H2O -> 2 MOH + H2."
                );
            }
            Some("premise.valence.alkali-h-o.initial-domain") => {
                premise["statement"] = json!(
                    "The listed Li, Na, K, Ca, H, and O tuples are the closed migration and unsupported-probe valence domain."
                );
            }
            Some("premise.observation.alkali-metal-disappears") => {
                premise["statement"] = json!(
                    "The selected alkali metal is compatible with the authored observation predicate disappears for this family outcome."
                );
            }
            _ => {}
        }
    }
    value
}

fn rewrite_catalogue_strings(value: &mut Value, replacements: &[(&str, &str)]) {
    match value {
        Value::String(text) => {
            for (from, to) in replacements {
                *text = text.replace(from, to);
            }
        }
        Value::Array(values) => {
            for value in values {
                rewrite_catalogue_strings(value, replacements);
            }
        }
        Value::Object(values) => {
            for value in values.values_mut() {
                rewrite_catalogue_strings(value, replacements);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

fn rewrite_exact_catalogue_string(value: &mut Value, from: &str, to: &str) {
    match value {
        Value::String(text) if text == from => to.clone_into(text),
        Value::Array(values) => {
            for value in values {
                rewrite_exact_catalogue_string(value, from, to);
            }
        }
        Value::Object(values) => {
            for value in values.values_mut() {
                rewrite_exact_catalogue_string(value, from, to);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn migrated_catalogue() -> ValidatedCatalogueBundle {
    ValidatedCatalogueBundle::from_json(
        &fs::read(root().join("conformance/catalogue/alkali-metal-water-001.catalogue.json"))
            .unwrap(),
    )
    .unwrap()
}

fn conformance_value(path: &str) -> Value {
    serde_json::from_slice(&fs::read(root().join(path)).unwrap()).unwrap()
}

fn oracle_usize(value: &Value, key: &str) -> usize {
    usize::try_from(value[key].as_u64().unwrap()).unwrap()
}

#[test]
fn registered_generalized_conformance_contract_matches_the_executable_surface() {
    let catalogue = conformance_value("conformance/catalogue/alkali-metal-water-001.input.json");
    assert_eq!(catalogue["family_rule"], "Rules.AlkaliMetalWithWater");
    assert_eq!(
        catalogue["element_category"]["members"],
        json!(["Li", "Na", "K"])
    );
    assert_eq!(catalogue["element_category"]["excluded_probe"], "Ca");
    assert_eq!(catalogue["legacy_concrete_rule_allowed"], false);
    let physical = conformance_value("conformance/catalogue/alkali-metal-water-001.catalogue.json");
    let schema = conformance_value("schemas/chem-catalogue-1.schema.json");
    let schema = jsonschema::draft202012::new(&schema).unwrap();
    schema.validate(&physical).unwrap();
    let mut ruleless = physical.clone();
    ruleless["bundle"]["generalized_rules"] = json!([]);
    assert!(schema.validate(&ruleless).is_err());
    let mut ruleless: chem_catalogue::CatalogueEnvelope = serde_json::from_value(ruleless).unwrap();
    ruleless.digest = ruleless.computed_digest().unwrap();
    assert_eq!(
        ValidatedCatalogueBundle::validate(ruleless)
            .unwrap_err()
            .code(),
        CatalogueErrorCode::InvalidMetadata
    );
    assert!(physical["bundle"]["rules"].as_array().unwrap().is_empty());
    assert_eq!(
        physical["bundle"]["generalized_rules"][0]["id"],
        "Rules.AlkaliMetalWithWater"
    );
    assert_eq!(
        physical["bundle"]["generalized_rules"][0]["cases"][0]["id"],
        "standard"
    );
    let mut generated: chem_catalogue::CatalogueEnvelope =
        serde_json::from_value(migrated_catalogue_value()).unwrap();
    generated.digest = generated.computed_digest().unwrap();
    assert_eq!(serde_json::to_value(generated).unwrap(), physical);
    assert_eq!(
        fs::read_to_string(
            root().join("conformance/catalogue/alkali-metal-water-001.catalogue.digest")
        )
        .unwrap()
        .trim(),
        physical["digest"].as_str().unwrap()
    );

    let end_to_end = conformance_value("conformance/end-to-end/alkali-water-family-001.input.json");
    assert_eq!(end_to_end["supported_members"], json!(["Li", "Na", "K"]));
    assert_eq!(end_to_end["unsupported_member"], "Ca");
}

#[test]
#[allow(clippy::too_many_lines)]
fn migrated_family_executes_li_na_and_k_to_exact_concrete_final_states() {
    let catalogue = migrated_catalogue();
    assert!(catalogue.rules().is_empty());
    let expansion_input =
        conformance_value("conformance/expansion/alkali-water-members-001.input.json");
    let expansion_oracle =
        conformance_value("conformance/expansion/alkali-water-members-001.expected.json");
    let kernel_oracle =
        conformance_value("conformance/validation-kernel/generalized-kernel-001.expected.json");
    for member in expansion_input["members"].as_array().unwrap() {
        let name = member["name"].as_str().unwrap();
        let symbol = member["symbol"].as_str().unwrap();
        let metal = member["metal"].as_str().unwrap();
        let hydroxide = member["hydroxide"].as_str().unwrap();
        let source_path = member["source"].as_str().unwrap();
        let evidence_path = member["evidence"].as_str().unwrap();
        let source = fs::read_to_string(root().join(source_path)).unwrap();
        let evidence = fs::read(root().join(evidence_path)).unwrap();
        let expanded =
            expand_review_candidate(source_path, &source, &catalogue, &evidence).unwrap();
        let selected = expanded.claim.rule.generalized.as_ref().unwrap();
        assert_eq!(selected.parameters["member"], symbol);
        assert_eq!(selected.case_id, expansion_oracle["selected_case"]);
        assert_eq!(
            selected.equivalent_match_count,
            oracle_usize(&expansion_oracle, "equivalent_match_count")
        );
        assert_eq!(
            expanded.operations.len(),
            oracle_usize(&expansion_oracle, "concrete_operations")
        );
        assert_eq!(
            expanded.mapping.entries().len(),
            oracle_usize(&expansion_oracle, "mapping_entries")
        );
        assert_eq!(
            expansion_oracle["member_applications"][symbol],
            json!([metal, hydroxide])
        );
        assert!(
            expanded.reactant_instances.values().any(|instance| instance
                .instance
                .structure()
                .as_str()
                == metal)
        );
        assert!(
            expanded.product_instances.values().any(|instance| instance
                .instance
                .structure()
                .as_str()
                == hydroxide)
        );

        let derivation = validate_review_candidate(&expanded, &catalogue).unwrap();
        assert_eq!(
            derivation.result(),
            ValidationResult::ValidatedWithAssumptions
        );
        assert_eq!(
            derivation.states().len(),
            oracle_usize(&kernel_oracle, "derivation_states")
        );
        let final_state = derivation.states().last().unwrap();
        assert_eq!(
            final_state.graph().atoms().len(),
            oracle_usize(&kernel_oracle, "final_atoms")
        );
        assert_eq!(
            final_state.graph().covalent_bonds().len(),
            oracle_usize(&kernel_oracle, "final_covalent_bonds")
        );
        assert_eq!(
            final_state.graph().ionic_associations().len(),
            oracle_usize(&kernel_oracle, "final_ionic_associations")
        );
        assert_eq!(
            final_state.graph().metallic_domains().len(),
            oracle_usize(&kernel_oracle, "final_metallic_domains")
        );
        assert_eq!(
            final_state.product_assignments().len(),
            oracle_usize(&kernel_oracle, "final_product_assignments")
        );
        assert_eq!(
            final_state.ledger().system_net_charge,
            kernel_oracle["system_net_charge"]
                .as_i64()
                .map(i128::from)
                .unwrap()
        );
        assert_eq!(
            final_state
                .graph()
                .atoms()
                .values()
                .filter(|atom| atom.element().to_string() == symbol)
                .count(),
            2
        );
        assert!(
            final_state
                .product_assignments()
                .keys()
                .any(|id| id.to_string() == format!("{name}Hydroxide[1]"))
        );

        let certificate = expanded.render_certificate();
        assert!(certificate.contains(&format!("\"member\": \"{symbol}\"")));
        assert!(certificate.contains("matched_sites:"));
        assert!(certificate.contains("parameter_premises:"));
        assert!(certificate.contains("role_premises:"));
        let serialized = serde_json::to_string(&expanded).unwrap();
        assert!(!serialized.contains("\"kind\":\"template\""));
        assert!(!serialized.contains("Templates.G4"));
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn migrated_family_mutations_fail_at_the_earliest_stable_boundary() {
    let oracle =
        conformance_value("conformance/validation-kernel/generalized-kernel-001.expected.json");
    let mut membership = migrated_catalogue_value();
    membership["bundle"]["element_categories"][0]["membership"]["members"] = json!(["Li", "K"]);
    let mut envelope: chem_catalogue::CatalogueEnvelope =
        serde_json::from_value(membership).unwrap();
    envelope.digest = envelope.computed_digest().unwrap();
    let error = ValidatedCatalogueBundle::validate(envelope).unwrap_err();
    assert_eq!(
        error.code(),
        CatalogueErrorCode::InvalidStructureApplication
    );

    let mut wrong_product = migrated_catalogue_value();
    wrong_product["bundle"]["structure_templates"][1]["components"][1]["bonds"][0]["electron_origin"] =
        json!({"kind":"dative","donor":"o","acceptor":"h"});
    let catalogue = validate_catalogue_value(wrong_product);
    let expanded = expand_review_candidate(
        "sodium-wrong-product.chems",
        &member_source("sodium", "Na", "SodiumMetal", "SodiumHydroxide"),
        &catalogue,
        &generalized_evidence(),
    )
    .unwrap();
    let error = validate_review_candidate(&expanded, &catalogue).unwrap_err();
    assert_eq!(error.class(), KernelFailureClass::InvalidExpansion);
    assert_eq!(
        error.code(),
        oracle["negative_boundaries"]["right_formula_wrong_graph"]
            .as_str()
            .unwrap()
    );
    assert!(error.message().contains("declared products"));

    let mut unsupported_case = migrated_catalogue_value();
    let premise_ids =
        unsupported_case["bundle"]["generalized_rules"][0]["cases"][0]["premise_ids"].clone();
    unsupported_case["bundle"]["generalized_rules"][0]["cases"][0] = json!({
        "id":"unsupported-domain","status":"unsupported","when":{"kind":"always"},
        "required_feature":"Features.AlkaliWaterOutcome",
        "explanation":"The reviewed family outcome is intentionally unavailable.",
        "premise_ids":premise_ids
    });
    let catalogue = validate_catalogue_value(unsupported_case);
    let error = expand_review_candidate(
        "sodium-unsupported-case.chems",
        &member_source("sodium", "Na", "SodiumMetal", "SodiumHydroxide"),
        &catalogue,
        &generalized_evidence(),
    )
    .unwrap_err();
    assert_eq!(error.class(), ExpansionFailureClass::UnsupportedChemistry);
    assert_eq!(
        error.code(),
        oracle["negative_boundaries"]["unsupported_case"]
            .as_str()
            .unwrap()
    );

    let mut no_match = migrated_catalogue_value();
    no_match["bundle"]["graph_patterns"][1]["variables"]["o"]["atom"]["element"] = json!("Ca");
    let catalogue = validate_catalogue_value(no_match);
    let error = expand_review_candidate(
        "sodium-no-match.chems",
        &member_source("sodium", "Na", "SodiumMetal", "SodiumHydroxide"),
        &catalogue,
        &generalized_evidence(),
    )
    .unwrap_err();
    assert_eq!(error.class(), ExpansionFailureClass::UnsupportedChemistry);
    assert_eq!(
        error.code(),
        oracle["negative_boundaries"]["no_graph_match"]
            .as_str()
            .unwrap()
    );

    let mut wrong_rewrite = migrated_catalogue_value();
    wrong_rewrite["bundle"]["generalized_rules"][0]["cases"][0]["rewrite"][0]["before"]["site"] =
        json!([2, 0, 0]);
    let catalogue = validate_catalogue_value(wrong_rewrite);
    let error = expand_review_candidate(
        "sodium-wrong-rewrite.chems",
        &member_source("sodium", "Na", "SodiumMetal", "SodiumHydroxide"),
        &catalogue,
        &generalized_evidence(),
    )
    .unwrap_err();
    assert_eq!(error.class(), ExpansionFailureClass::CorruptTrustedData);
    assert_eq!(
        error.code(),
        oracle["negative_boundaries"]["invalid_rewrite"]
            .as_str()
            .unwrap()
    );

    let calcium = member_source("calcium", "Ca", "CalciumMetal", "LithiumHydroxide")
        .replace("CaOH[ionic]", "LiOH[ionic]");
    let error = expand_review_candidate(
        "calcium-water.chems",
        &calcium,
        &migrated_catalogue(),
        &generalized_evidence(),
    )
    .unwrap_err();
    assert_eq!(error.class(), ExpansionFailureClass::UnsupportedChemistry);
    assert_eq!(error.code(), "CHEMS-X015");
}

#[test]
fn generalized_dative_direction_survives_concrete_kernel_execution() {
    let source = r"chems 1
use catalog ChemSpec.Theoretical@1

reaction DativeFixture where
  reactants
    donor := 1 of G4DativeDonor
    acceptor := 1 of G4DativeAcceptor

  products
    adduct := 1 of G4DativeAdduct

  equation
    O[ion] + H[ion] -> HO[ion]

  model
    event := representative
    sequence := explanatory

  observe from Evidence.DativeFixture@1
    product adduct forms claim R1

  by
    apply Rules.G4Dative
      donor := donor
      acceptor := acceptor
      adduct := adduct
";
    let evidence = serde_json::to_vec(&json!({
        "schema_version":1,"id":"Evidence.DativeFixture@1",
        "claims":[{"id":"R1","subject_role":"product","subject":"dative adduct","predicate":"forms","sources":["S1"]}],
        "sources":[{"id":"S1","title":"Reviewed dative fixture","publisher":"ChemSpec","url":"https://example.invalid/dative","supports":["R1"]}]
    }))
    .unwrap();
    let catalogue = generalized_dative_catalogue();
    let expanded =
        expand_review_candidate("dative-g5.chems", source, &catalogue, &evidence).unwrap();
    let derivation = validate_review_candidate(&expanded, &catalogue).unwrap();
    let final_graph = derivation.states().last().unwrap().graph();
    let bond = final_graph.covalent_bonds().values().next().unwrap();
    assert!(matches!(
        bond.electron_origin(),
        CovalentElectronOrigin::Dative { donor, acceptor }
            if donor.to_string() == "donor[1].donor"
                && acceptor.to_string() == "acceptor[1].acceptor"
    ));
    assert_eq!(
        expanded.product_instances["adduct[1]"].instance.structure(),
        &StructureId::new("G4DativeAdduct").unwrap()
    );
}

#[test]
fn legacy_non_generalized_catalogue_still_executes_as_the_exception_path() {
    let catalogue = ValidatedCatalogueBundle::from_json(
        &fs::read(root().join("conformance/catalogue/lithium-rule-001.catalogue.json")).unwrap(),
    )
    .unwrap();
    let source =
        fs::read_to_string(root().join("conformance/expansion/canonical-expansion-001.chems"))
            .unwrap();
    let evidence =
        fs::read(root().join("conformance/observations/lithium-observations-001.input.json"))
            .unwrap();
    let expanded =
        expand_review_candidate("legacy-concrete.chems", &source, &catalogue, &evidence).unwrap();
    assert!(expanded.claim.rule.generalized.is_none());
    validate_review_candidate(&expanded, &catalogue).unwrap();
}
