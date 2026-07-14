use std::{fs, path::PathBuf};

use chem_catalogue::{CatalogueEnvelope, ValidatedCatalogueBundle};
use chem_kernel::{ExpansionFailureClass, expand_review_candidate};
use serde_json::{Value, json};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[allow(clippy::too_many_lines)]
fn generalized_catalogue_value() -> Value {
    let mut value: Value = serde_json::from_slice(
        &fs::read(root().join("conformance/catalogue/lithium-rule-001.catalogue.json")).unwrap(),
    )
    .unwrap();
    let concrete = value["bundle"]["rules"][0].clone();
    value["bundle"]["rules"][0]["id"] = json!("Rules.LegacyLithiumWithWater");
    value["bundle"]["structures"]
        .as_array_mut()
        .unwrap()
        .retain(|record| record["id"] != "LithiumMetal" && record["id"] != "LithiumHydroxide");
    let premise = concrete["applicability"]["premise_id"].clone();
    value["bundle"]["elements"] = json!([
        {"symbol":"H","name":"Hydrogen","atomic_number":1,"period":1,"group":1,"block":"s","premise_ids":[premise.clone()]},
        {"symbol":"Li","name":"Lithium","atomic_number":3,"period":2,"group":1,"block":"s","premise_ids":[premise.clone()]},
        {"symbol":"O","name":"Oxygen","atomic_number":8,"period":2,"group":16,"block":"p","premise_ids":[premise.clone()]},
        {"symbol":"Na","name":"Sodium","atomic_number":11,"period":3,"group":1,"block":"s","premise_ids":[premise.clone()]},
        {"symbol":"K","name":"Potassium","atomic_number":19,"period":4,"group":1,"block":"s","premise_ids":[premise.clone()]},
        {"symbol":"Ca","name":"Calcium","atomic_number":20,"period":4,"group":2,"block":"s","premise_ids":[premise.clone()]}
    ]);
    value["bundle"]["element_categories"] = json!([{
        "id":"Categories.G4Alkali","subject":"element",
        "membership":{"kind":"explicit","members":["Li","Na","K"]},
        "premise_ids":[premise.clone()]
    }]);
    value["bundle"]["valence_premises"][0]["neutral_valence"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"element":"Na","neutral_valence_electrons":1}),
            json!({"element":"K","neutral_valence_electrons":1}),
            json!({"element":"Ca","neutral_valence_electrons":2}),
        ]);
    value["bundle"]["valence_premises"][0]["supported_states"]
        .as_array_mut().unwrap().extend([
            json!({"element":"Na","formal_charge":1,"non_bonding_electrons":0,"unpaired_electrons":0,"covalent_bond_order_sum":0}),
            json!({"element":"Na","formal_charge":0,"non_bonding_electrons":1,"unpaired_electrons":1,"covalent_bond_order_sum":0}),
            json!({"element":"K","formal_charge":1,"non_bonding_electrons":0,"unpaired_electrons":0,"covalent_bond_order_sum":0}),
            json!({"element":"K","formal_charge":0,"non_bonding_electrons":1,"unpaired_electrons":1,"covalent_bond_order_sum":0}),
            json!({"element":"Ca","formal_charge":2,"non_bonding_electrons":0,"unpaired_electrons":0,"covalent_bond_order_sum":0})
        ]);
    value["bundle"]["valence_premises"][0]["metallic_domain_states"]
        .as_array_mut().unwrap().extend([
            json!({"element":"Na","site_formal_charge":1,"site_local_electrons":0,"delocalized_electrons_per_site":1}),
            json!({"element":"K","site_formal_charge":1,"site_local_electrons":0,"delocalized_electrons_per_site":1}),
            json!({"element":"Ca","site_formal_charge":2,"site_local_electrons":0,"delocalized_electrons_per_site":2})
        ]);
    value["bundle"]["structure_templates"] = json!([
        {
            "id":"Templates.G4Metal",
            "parameters":{"member":{"kind":"element","category":"Categories.G4Alkali"}},
            "representation":"metallic",
            "sites":[{"label":"li","element":{"parameter":"member"},"formal_charge":1,"non_bonding_electrons":0,"unpaired_electrons":0}],
            "domains":[{"label":"metallic","sites":["li"],"delocalized_electrons":1}],
            "premise_ids":[premise.clone()]
        },
        {
            "id":"Templates.G4Hydroxide",
            "parameters":{"member":{"kind":"element","category":"Categories.G4Alkali"}},
            "representation":"ionic",
            "components":[
                {"label":"lithium","atoms":[{"label":"li","element":{"parameter":"member"},"formal_charge":1,"non_bonding_electrons":0,"unpaired_electrons":0}]},
                {"label":"hydroxide","atoms":[
                    {"label":"o","element":"O","formal_charge":-1,"non_bonding_electrons":6,"unpaired_electrons":0},
                    {"label":"h","element":"H","formal_charge":0,"non_bonding_electrons":0,"unpaired_electrons":0}
                ],"bonds":[{"left":"o","right":"h","order":"single"}]}
            ],
            "associations":[{"label":"ionic","components":["lithium","hydroxide"]}],
            "premise_ids":[premise.clone()]
        }
    ]);
    value["bundle"]["structure_applications"] = json!([
        {"id":"LithiumMetal","template":"Templates.G4Metal","arguments":{"member":"Li"},"formula":"Li","premise_ids":[premise.clone()]},
        {"id":"SodiumMetal","template":"Templates.G4Metal","arguments":{"member":"Na"},"formula":"Na","premise_ids":[premise.clone()]},
        {"id":"PotassiumMetal","template":"Templates.G4Metal","arguments":{"member":"K"},"formula":"K","premise_ids":[premise.clone()]},
        {"id":"LithiumHydroxide","template":"Templates.G4Hydroxide","arguments":{"member":"Li"},"formula":"LiOH","premise_ids":[premise.clone()]},
        {"id":"SodiumHydroxide","template":"Templates.G4Hydroxide","arguments":{"member":"Na"},"formula":"NaOH","premise_ids":[premise.clone()]},
        {"id":"PotassiumHydroxide","template":"Templates.G4Hydroxide","arguments":{"member":"K"},"formula":"KOH","premise_ids":[premise.clone()]}
    ]);
    value["bundle"]["structures"].as_array_mut().unwrap().push(json!({
        "id":"CalciumMetal","premise_id":premise.clone(),"formula":"Ca","representation":"metallic",
        "sites":[{"label":"ca","element":"Ca","formal_charge":2,"non_bonding_electrons":0,"unpaired_electrons":0}],
        "domains":[{"label":"metallic","sites":["ca"],"delocalized_electrons":2}]
    }));
    value["bundle"]["graph_patterns"] = json!([
        {
            "id":"Patterns.G4LithiumMetal",
            "variables":{"li":{"atom":{"element":{"parameter":"member"}}}},
            "relationships":[{
                "kind":"metallic_domain","domain":"metallic","sites":["li"],
                "delocalized_electrons":1
            }],
            "premise_ids":[premise.clone()]
        },
        {
            "id":"Patterns.G4Water",
            "variables":{
                "o":{"atom":{"element":"O"}},
                "h1":{"atom":{"element":"H"}},
                "h2":{"atom":{"element":"H"}}
            },
            "relationships":[
                {"kind":"covalent","bond":"oh1","left":"o","right":"h1","order":"single"},
                {"kind":"covalent","bond":"oh2","left":"o","right":"h2","order":"single"}
            ],
            "premise_ids":[premise.clone()]
        }
    ]);
    value["bundle"]["generalized_rules"] = json!([{
        "id":"Rules.AlkaliMetalWithWater",
        "parameters":{"member":{"kind":"element","category":"Categories.G4Alkali"}},
        "roles":{
            "metal":{"side":"reactant","representation":"metallic","coefficient":2},
            "water":{"side":"reactant","representation":"molecular","coefficient":2},
            "hydroxide":{"side":"product","representation":"ionic","coefficient":2},
            "gasProduct":{"side":"product","representation":"molecular","coefficient":1}
        },
        "reactants":{
            "metal":{"kind":"template","template":"Templates.G4Metal","arguments":{"member":{"parameter":"member"}}},
            "water":{"kind":"exact","structure":"Water"}
        },
        "cases":[{
            "id":"lithium","status":"supported","when":{"kind":"always"},
            "products":{
                "hydroxide":{"kind":"template","template":"Templates.G4Hydroxide","arguments":{"member":{"parameter":"member"}}},
                "gasProduct":{"kind":"exact","structure":"Hydrogen"}
            },
            "patterns":{"metal":"Patterns.G4LithiumMetal","water":"Patterns.G4Water"},
            "correspondence":concrete["mapping_template"].clone(),
            "rewrite":concrete["operation_template"].clone(),
            "observation_compatibility":concrete["observation_compatibility"].clone(),
            "premise_ids":[premise.clone()]
        }],
        "applicability":{
            "premise_id":premise.clone(),"request_relation":"contact",
            "required_context":concrete["applicability"]["required_context"].clone()
        },
        "model_assumptions":concrete["model_assumptions"].clone(),
        "premise_ids":concrete["premise_ids"].clone()
    }]);
    value["bundle"]["generalized_rules"][0]["cases"][0]["observation_compatibility"][1]["evidence_subject"] =
        json!("alkali metal");
    value
}

fn validate_catalogue_value(value: Value) -> ValidatedCatalogueBundle {
    let mut envelope: CatalogueEnvelope = serde_json::from_value(value).unwrap();
    envelope.digest = envelope.computed_digest().unwrap();
    ValidatedCatalogueBundle::validate(envelope).unwrap()
}

fn generalized_lithium_catalogue() -> ValidatedCatalogueBundle {
    validate_catalogue_value(generalized_catalogue_value())
}

fn generalized_dative_catalogue() -> ValidatedCatalogueBundle {
    let mut value = generalized_catalogue_value();
    let premise = value["bundle"]["rules"][0]["applicability"]["premise_id"].clone();
    value["bundle"]["valence_premises"][0]["supported_states"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "element":"O","formal_charge":-2,"non_bonding_electrons":8,
            "unpaired_electrons":0,"covalent_bond_order_sum":0
        }));
    value["bundle"]["structural_traits"] = json!([
        {
            "id":"Traits.G4Donor","sites":{"donor":"atom"},
            "values":{"paired_electrons":{"kind":"atom_non_bonding_electrons","site":"donor"}},
            "premise_ids":[premise.clone()]
        },
        {
            "id":"Traits.G4Acceptor","sites":{"acceptor":"atom"},
            "values":{"formal_charge":{"kind":"atom_formal_charge","site":"acceptor"}},
            "premise_ids":[premise.clone()]
        }
    ]);
    value["bundle"]["structures"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({
                "id":"G4DativeDonor","premise_id":premise.clone(),"formula":"O","representation":"ion",
                "atoms":[{"label":"donor","element":"O","formal_charge":-2,"non_bonding_electrons":8,"unpaired_electrons":0}],
                "traits":[{"trait":"Traits.G4Donor","sites":{"donor":"donor"},"values":{"paired_electrons":8},"premise_ids":[premise.clone()]}]
            }),
            json!({
                "id":"G4DativeAcceptor","premise_id":premise.clone(),"formula":"H","representation":"ion",
                "atoms":[{"label":"acceptor","element":"H","formal_charge":1,"non_bonding_electrons":0,"unpaired_electrons":0}],
                "traits":[{"trait":"Traits.G4Acceptor","sites":{"acceptor":"acceptor"},"values":{"formal_charge":1},"premise_ids":[premise.clone()]}]
            }),
            json!({
                "id":"G4DativeAdduct","premise_id":premise.clone(),"formula":"HO","representation":"ion",
                "atoms":[
                    {"label":"donor","element":"O","formal_charge":-1,"non_bonding_electrons":6,"unpaired_electrons":0},
                    {"label":"acceptor","element":"H","formal_charge":0,"non_bonding_electrons":0,"unpaired_electrons":0}
                ],
                "bonds":[{"left":"donor","right":"acceptor","order":"single","electron_origin":{"kind":"dative","donor":"donor","acceptor":"acceptor"}}]
            }),
        ]);
    value["bundle"]["graph_patterns"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({
                "id":"Patterns.G4DativeDonor","variables":{"donor":{"atom":{"element":"O","formal_charge":-2,"non_bonding_electrons":8}}},
                "traits":[{"trait":"Traits.G4Donor","sites":{"donor":"donor"}}],"premise_ids":[premise.clone()]
            }),
            json!({
                "id":"Patterns.G4DativeAcceptor","variables":{"acceptor":{"atom":{"element":"H","formal_charge":1}}},
                "traits":[{"trait":"Traits.G4Acceptor","sites":{"acceptor":"acceptor"}}],"premise_ids":[premise.clone()]
            }),
        ]);
    value["bundle"]["generalized_rules"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "id":"Rules.G4Dative",
            "parameters":{
                "D":{"kind":"structure","trait":"Traits.G4Donor"},
                "A":{"kind":"structure","trait":"Traits.G4Acceptor"}
            },
            "roles":{
                "donor":{"side":"reactant","representation":"ion","coefficient":1},
                "acceptor":{"side":"reactant","representation":"ion","coefficient":1},
                "adduct":{"side":"product","representation":"ion","coefficient":1}
            },
            "reactants":{
                "donor":{"kind":"structure_parameter","parameter":"D"},
                "acceptor":{"kind":"structure_parameter","parameter":"A"}
            },
            "cases":[{
                "id":"directed","status":"supported","when":{"kind":"always"},
                "products":{"adduct":{"kind":"exact","structure":"G4DativeAdduct"}},
                "patterns":{"donor":"Patterns.G4DativeDonor","acceptor":"Patterns.G4DativeAcceptor"},
                "correspondence":[
                    {"reactant":"donor[1].donor","product":"adduct[1].donor","premise_ids":[premise.clone()]},
                    {"reactant":"acceptor[1].acceptor","product":"adduct[1].acceptor","premise_ids":[premise.clone()]}
                ],
                "rewrite":[
                    {"kind":"form_dative","donor":"donor[1].donor","acceptor":"acceptor[1].acceptor","before":{"left":[-2,8,0],"right":[1,0,0]},"after":{"left":[-1,6,0],"right":[0,0,0]},"premise_ids":[premise.clone()]},
                    {"kind":"assign_product","atoms":["donor[1].donor","acceptor[1].acceptor"],"product":"adduct[1]","premise_ids":[premise.clone()]}
                ],
                "observation_compatibility":[{"subject_role":"adduct","predicate":"forms","evidence_subject":"dative adduct","premise_id":premise.clone()}],
                "premise_ids":[premise.clone()]
            }],
            "applicability":{"premise_id":premise.clone(),"request_relation":"contact","required_context":"directed dative fixture"},
            "model_assumptions":{"event":"representative","sequence":"explanatory","premise_ids":[premise.clone()]},
            "premise_ids":[premise]
        }));
    validate_catalogue_value(value)
}

fn generalized_ambiguous_catalogue() -> ValidatedCatalogueBundle {
    let mut value = generalized_catalogue_value();
    let premise = value["bundle"]["rules"][0]["applicability"]["premise_id"].clone();
    value["bundle"]["graph_patterns"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "id":"Patterns.G4Ambiguous","variables":{
                "a":{"atom":{}},"b":{"atom":{}},"c":{"atom":{}}
            },"premise_ids":[premise.clone()]
        }));
    value["bundle"]["generalized_rules"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "id":"Rules.G4Ambiguous",
            "parameters":{"mode":{"kind":"enum","values":["probe"]}},
            "roles":{
                "substrate":{"side":"reactant","representation":"ionic","coefficient":1},
                "result":{"side":"product","representation":"ionic","coefficient":1}
            },
            "reactants":{"substrate":{"kind":"exact","structure":"LithiumHydroxide"}},
            "cases":[{
                "id":"probe","status":"supported","when":{"kind":"always"},
                "products":{"result":{"kind":"exact","structure":"LithiumHydroxide"}},
                "patterns":{"substrate":"Patterns.G4Ambiguous"},
                "correspondence":[
                    {"reactant":"substrate[1].a","product":"result[1].lithium.li","premise_ids":[premise.clone()]},
                    {"reactant":"substrate[1].b","product":"result[1].hydroxide.o","premise_ids":[premise.clone()]},
                    {"reactant":"substrate[1].c","product":"result[1].hydroxide.h","premise_ids":[premise.clone()]}
                ],
                "rewrite":[{"kind":"assign_product","atoms":["substrate[1].a","substrate[1].b","substrate[1].c"],"product":"result[1]","premise_ids":[premise.clone()]}],
                "observation_compatibility":[{"subject_role":"result","predicate":"forms","evidence_subject":"ambiguity probe","premise_id":premise.clone()}],
                "premise_ids":[premise.clone()]
            }],
            "applicability":{"premise_id":premise.clone(),"request_relation":"contact","required_context":"ambiguity probe"},
            "model_assumptions":{"event":"representative","sequence":"explanatory","premise_ids":[premise.clone()]},
            "premise_ids":[premise]
        }));
    let mut no_match_pattern = value["bundle"]["graph_patterns"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .clone();
    no_match_pattern["id"] = json!("Patterns.G4NoMatch");
    no_match_pattern["variables"] = json!({
        "a":{"atom":{"element":"Li","formal_charge":0}},
        "b":{"atom":{"element":"O"}},
        "c":{"atom":{"element":"H"}}
    });
    value["bundle"]["graph_patterns"]
        .as_array_mut()
        .unwrap()
        .push(no_match_pattern);
    let mut no_match_rule = value["bundle"]["generalized_rules"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .clone();
    no_match_rule["id"] = json!("Rules.G4NoMatch");
    no_match_rule["cases"][0]["patterns"]["substrate"] = json!("Patterns.G4NoMatch");
    value["bundle"]["generalized_rules"]
        .as_array_mut()
        .unwrap()
        .push(no_match_rule);
    let mut unsupported_rule = value["bundle"]["generalized_rules"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .clone();
    unsupported_rule["id"] = json!("Rules.G4SelectedUnsupported");
    unsupported_rule["cases"] = json!([{
        "id":"unsupported","status":"unsupported","when":{"kind":"always"},
        "required_feature":"Features.G4Probe","explanation":"Reviewed probe is outside the current domain.",
        "premise_ids":[value["bundle"]["rules"][0]["applicability"]["premise_id"].clone()]
    }]);
    value["bundle"]["generalized_rules"]
        .as_array_mut()
        .unwrap()
        .push(unsupported_rule);
    validate_catalogue_value(value)
}

fn generalized_evidence() -> Vec<u8> {
    let mut value: Value = serde_json::from_slice(
        &fs::read(root().join("conformance/observations/lithium-observations-001.input.json"))
            .unwrap(),
    )
    .unwrap();
    value["claims"][1]["subject"] = json!("alkali metal");
    serde_json::to_vec(&value).unwrap()
}

fn member_source(name: &str, symbol: &str, metal: &str, hydroxide: &str) -> String {
    fs::read_to_string(root().join("conformance/end-to-end/lithium-outcome-001.chems"))
        .unwrap()
        .replace("LithiumMetal", metal)
        .replace("LithiumHydroxide", hydroxide)
        .replace("lithiumHydroxide", &format!("{name}Hydroxide"))
        .replace("lithium :=", &format!("{name} :="))
        .replace(
            "reactant lithium disappears",
            &format!("reactant {name} disappears"),
        )
        .replace("metal := lithium", &format!("metal := {name}"))
        .replace(
            "hydroxide := lithiumHydroxide",
            &format!("hydroxide := {name}Hydroxide"),
        )
        .replace("Li[metallic]", &format!("{symbol}[metallic]"))
        .replace("LiOH[ionic]", &format!("{symbol}OH[ionic]"))
}

#[test]
fn unchanged_chems_source_expands_through_generalized_rule_to_concrete_hir() {
    let source =
        fs::read_to_string(root().join("conformance/end-to-end/lithium-outcome-001.chems"))
            .unwrap();
    let evidence = generalized_evidence();
    let expanded = expand_review_candidate(
        "lithium-outcome-001.chems",
        &source,
        &generalized_lithium_catalogue(),
        &evidence,
    )
    .unwrap();
    let selected = expanded.claim.rule.generalized.as_ref().unwrap();
    assert_eq!(selected.parameters["member"], "Li");
    assert_eq!(selected.case_id, "lithium");
    assert_eq!(selected.equivalent_match_count, 4);
    assert!(selected.matched_sites["water[1]"].contains_key("h1"));
    assert!(!selected.parameter_premises["member"].is_empty());
    assert!(!selected.role_premises["metal"].is_empty());
    assert_eq!(expanded.reactant_instances.len(), 4);
    assert_eq!(expanded.product_instances.len(), 3);
    assert_eq!(expanded.mapping.entries().len(), 8);
    assert_eq!(expanded.operations.len(), 12);
    assert!(
        selected
            .provenance
            .catalogue
            .iter()
            .any(|origin| origin.record.contains("case lithium"))
    );
}

#[test]
fn generalized_role_shape_errors_remain_invalid_source() {
    let source =
        fs::read_to_string(root().join("conformance/end-to-end/lithium-outcome-001.chems"))
            .unwrap()
            .replace(
                "lithium := 2 of LithiumMetal",
                "lithium := 1 of LithiumMetal",
            )
            .replace("2 Li[metallic]", "Li[metallic]");
    let evidence = generalized_evidence();
    let error = expand_review_candidate(
        "wrong-coefficient.chems",
        &source,
        &generalized_lithium_catalogue(),
        &evidence,
    )
    .unwrap_err();
    assert_eq!(error.class(), ExpansionFailureClass::InvalidSource);
    assert_eq!(error.code(), "CHEMS-X013");
}

#[test]
fn lithium_sodium_and_potassium_share_one_public_rule_surface() {
    let catalogue = generalized_lithium_catalogue();
    let evidence = generalized_evidence();
    for (name, symbol, metal, hydroxide) in [
        ("lithium", "Li", "LithiumMetal", "LithiumHydroxide"),
        ("sodium", "Na", "SodiumMetal", "SodiumHydroxide"),
        ("potassium", "K", "PotassiumMetal", "PotassiumHydroxide"),
    ] {
        let expanded = expand_review_candidate(
            &format!("{name}-water.chems"),
            &member_source(name, symbol, metal, hydroxide),
            &catalogue,
            &evidence,
        )
        .unwrap();
        assert_eq!(
            expanded.claim.rule.generalized.as_ref().unwrap().parameters["member"],
            symbol
        );
        assert!(
            expanded.reactant_instances[&format!("{name}[1]")]
                .instance
                .graph()
                .atoms()
                .values()
                .any(|atom| atom.element().to_string() == symbol)
        );
        assert!(
            expanded.product_instances[&format!("{name}Hydroxide[1]")]
                .instance
                .graph()
                .atoms()
                .values()
                .any(|atom| atom.element().to_string() == symbol)
        );
    }
}

#[test]
fn calcium_is_unsupported_before_generalized_operation_expansion() {
    let source = member_source("calcium", "Ca", "CalciumMetal", "LithiumHydroxide")
        .replace("CaOH[ionic]", "LiOH[ionic]");
    let error = expand_review_candidate(
        "calcium-water.chems",
        &source,
        &generalized_lithium_catalogue(),
        &generalized_evidence(),
    )
    .unwrap_err();
    assert_eq!(error.class(), ExpansionFailureClass::UnsupportedChemistry);
    assert_eq!(error.code(), "CHEMS-X015");
}

#[test]
fn concrete_legacy_rules_continue_to_elaborate_during_migration() {
    let source =
        fs::read_to_string(root().join("conformance/end-to-end/lithium-outcome-001.chems"))
            .unwrap()
            .replace("Rules.AlkaliMetalWithWater", "Rules.LegacyLithiumWithWater");
    let evidence =
        fs::read(root().join("conformance/observations/lithium-observations-001.input.json"))
            .unwrap();
    let expanded = expand_review_candidate(
        "legacy-lithium.chems",
        &source,
        &generalized_lithium_catalogue(),
        &evidence,
    )
    .unwrap();
    assert!(expanded.claim.rule.generalized.is_none());
    assert_eq!(expanded.operations.len(), 12);
}

#[test]
fn dative_trait_sites_reach_the_public_kernel_as_directed_form_dative() {
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
    let expanded = expand_review_candidate(
        "dative.chems",
        source,
        &generalized_dative_catalogue(),
        &evidence,
    )
    .unwrap();
    let operation = serde_json::to_value(&expanded.operations[0].operation).unwrap();
    assert_eq!(operation["kind"], "form_dative");
    assert_eq!(operation["donor"], "donor[1].donor");
    assert_eq!(operation["acceptor"], "acceptor[1].acceptor");
    let selected = expanded.claim.rule.generalized.as_ref().unwrap();
    assert_eq!(selected.matched_sites["donor[1]"]["donor"], "donor");
    assert_eq!(
        selected.matched_sites["acceptor[1]"]["acceptor"],
        "acceptor"
    );
}

#[test]
fn non_equivalent_complete_certificates_surface_as_public_ambiguity() {
    let source = r"chems 1
use catalog ChemSpec.Theoretical@1

reaction AmbiguityFixture where
  reactants
    substrate := 1 of LithiumHydroxide

  products
    output := 1 of LithiumHydroxide

  equation
    LiOH[ionic] -> LiOH[ionic]

  model
    event := representative
    sequence := explanatory

  observe from Evidence.AmbiguityFixture@1
    product output forms claim R1

  by
    apply Rules.G4Ambiguous
      substrate := substrate
      result := output
";
    let evidence = serde_json::to_vec(&json!({
        "schema_version":1,"id":"Evidence.AmbiguityFixture@1",
        "claims":[{"id":"R1","subject_role":"product","subject":"ambiguity probe","predicate":"forms","sources":["S1"]}],
        "sources":[{"id":"S1","title":"Ambiguity fixture","publisher":"ChemSpec","url":"https://example.invalid/ambiguity","supports":["R1"]}]
    }))
    .unwrap();
    let error = expand_review_candidate(
        "ambiguous.chems",
        source,
        &generalized_ambiguous_catalogue(),
        &evidence,
    )
    .unwrap_err();
    assert_eq!(
        error.class(),
        ExpansionFailureClass::AmbiguousChemistry,
        "{error:?}"
    );
    assert_eq!(error.code(), "CHEMS-X016");

    let no_match = source.replace("Rules.G4Ambiguous", "Rules.G4NoMatch");
    let error = expand_review_candidate(
        "no-match.chems",
        &no_match,
        &generalized_ambiguous_catalogue(),
        &evidence,
    )
    .unwrap_err();
    assert_eq!(error.class(), ExpansionFailureClass::UnsupportedChemistry);
    assert_eq!(error.code(), "CHEMS-X015");
    assert!(error.to_string().contains("no graph match"));

    let selected_unsupported = source.replace("Rules.G4Ambiguous", "Rules.G4SelectedUnsupported");
    let error = expand_review_candidate(
        "selected-unsupported.chems",
        &selected_unsupported,
        &generalized_ambiguous_catalogue(),
        &evidence,
    )
    .unwrap_err();
    assert_eq!(error.class(), ExpansionFailureClass::UnsupportedChemistry);
    assert_eq!(error.code(), "CHEMS-X015");
    assert!(error.to_string().contains("Features.G4Probe"));
}
