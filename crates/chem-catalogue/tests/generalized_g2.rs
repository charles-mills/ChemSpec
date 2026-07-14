use std::{collections::BTreeMap, fs, path::PathBuf, str::FromStr};

use chem_catalogue::{
    CatalogueEnvelope, CatalogueErrorCode, GraphPatternId, PatternRoleInput,
    ValidatedCatalogueBundle,
};
use chem_domain::{ElementSymbol, StructureId};
use serde_json::{Value, json};

fn fixture() -> Value {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    serde_json::from_slice(
        &fs::read(root.join("conformance/catalogue/lithium-rule-001.catalogue.json")).unwrap(),
    )
    .unwrap()
}

fn schema() -> Value {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    serde_json::from_slice(&fs::read(root.join("schemas/chem-catalogue-1.schema.json")).unwrap())
        .unwrap()
}

fn envelope(value: Value) -> CatalogueEnvelope {
    let mut envelope: CatalogueEnvelope = serde_json::from_value(value).unwrap();
    envelope.digest = envelope.computed_digest().unwrap();
    envelope
}

fn add_premise(value: &mut Value, id: &str, statement: &str) {
    value["bundle"]["premises"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "id": id,
            "statement": statement,
            "evidence": ["evidence.openstax.chemistry-2e"],
            "review": {"status": "provisional"},
            "rule_version": "1"
        }));
}

#[allow(clippy::too_many_lines)]
fn with_g2(mut value: Value) -> Value {
    for (id, statement) in [
        (
            "premise.structure.dative-water",
            "A structural matcher probe with one shared and one directed dative O-H edge.",
        ),
        (
            "premise.structure.group-multiplicity-probe",
            "A structural matcher probe whose duplicate-membership groups break atom symmetry.",
        ),
        (
            "premise.trait.protic-oh.definition",
            "A checked protic O-H trait exposes exact atom and bond sites.",
        ),
        (
            "premise.trait.water.protic-oh",
            "The first shared O-H site in water satisfies the checked trait.",
        ),
        (
            "premise.pattern.lithium-site",
            "The metallic lithium-site pattern is reviewed.",
        ),
        (
            "premise.pattern.water-oh",
            "The shared O-H pattern is reviewed.",
        ),
        (
            "premise.pattern.dative-oh",
            "The directed donor-to-acceptor O-H pattern is reviewed.",
        ),
        (
            "premise.pattern.ionic-hydroxide",
            "The ionic component-membership pattern is reviewed.",
        ),
        (
            "premise.pattern.any-hydrogen",
            "The unconstrained hydrogen-site oracle pattern is reviewed.",
        ),
        (
            "premise.pattern.hydrogen-pair",
            "The injective pair oracle pattern is reviewed.",
        ),
        (
            "premise.pattern.checked-protic-oh",
            "The checked-trait O-H pattern is reviewed.",
        ),
        (
            "premise.pattern.parameter-element",
            "The element-parameter matcher probe is reviewed.",
        ),
        (
            "premise.pattern.relationship-alias",
            "Distinct local relationship names may bind one exact concrete relationship.",
        ),
    ] {
        add_premise(&mut value, id, statement);
    }

    value["bundle"]["structures"][1]["traits"] = json!([{
        "trait":"Traits.ProticOH",
        "sites":{"oxygen":"o","hydrogen":"h1","bond":"bond.0"},
        "values":{"bond_order":"single","electron_origin":"shared"},
        "premise_ids":["premise.trait.water.protic-oh"]
    }]);
    value["bundle"]["structures"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "id":"DativeWater",
            "premise_id":"premise.structure.dative-water",
            "formula":"H2O",
            "representation":"molecular",
            "atoms":[
                {"label":"o","element":"O","formal_charge":0,"non_bonding_electrons":4,"unpaired_electrons":0},
                {"label":"h1","element":"H","formal_charge":0,"non_bonding_electrons":0,"unpaired_electrons":0},
                {"label":"h2","element":"H","formal_charge":0,"non_bonding_electrons":0,"unpaired_electrons":0}
            ],
            "bonds":[
                {"left":"o","right":"h1","order":"single"},
                {"left":"o","right":"h2","order":"single","electron_origin":{"kind":"dative","donor":"o","acceptor":"h2"}}
            ]
        }));
    value["bundle"]["structures"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "id":"GroupMultiplicityProbe",
            "premise_id":"premise.structure.group-multiplicity-probe",
            "formula":"H2",
            "representation":"molecular",
            "atoms":[
                {"label":"h1","element":"H","formal_charge":0,"non_bonding_electrons":0,"unpaired_electrons":0},
                {"label":"h2","element":"H","formal_charge":0,"non_bonding_electrons":0,"unpaired_electrons":0}
            ],
            "bonds":[{"left":"h1","right":"h2","order":"single"}],
            "groups":[
                {"label":"first","atoms":["h1"]},
                {"label":"second","atoms":["h1"]},
                {"label":"third","atoms":["h2"]}
            ]
        }));
    value["bundle"]["structural_traits"] = json!([{
        "id":"Traits.ProticOH",
        "sites":{"oxygen":"atom","hydrogen":"atom","bond":"covalent_bond"},
        "values":{
            "bond_order":{"kind":"covalent_bond_order","left_site":"oxygen","right_site":"hydrogen"},
            "electron_origin":{"kind":"covalent_electron_origin","left_site":"oxygen","right_site":"hydrogen"}
        },
        "premise_ids":["premise.trait.protic-oh.definition"]
    }]);
    value["bundle"]["graph_patterns"] = json!([
        {
            "id":"Patterns.LithiumSite",
            "variables":{"metal":{"atom":{"element":"Li","formal_charge":1,"non_bonding_electrons":0,"unpaired_electrons":0,"bond_order_sum":0}}},
            "relationships":[{"kind":"metallic_domain","domain":"domain","sites":["metal"],"delocalized_electrons":1}],
            "premise_ids":["premise.pattern.lithium-site"]
        },
        {
            "id":"Patterns.WaterOH",
            "variables":{"oxygen":{"atom":{"element":"O","bond_order_sum":2}},"hydrogen":{"atom":{"element":"H"}}},
            "relationships":[{"kind":"covalent","bond":"oh","left":"oxygen","right":"hydrogen","order":"single"}],
            "premise_ids":["premise.pattern.water-oh"]
        },
        {
            "id":"Patterns.DativeOH",
            "variables":{"oxygen":{"atom":{"element":"O"}},"hydrogen":{"atom":{"element":"H"}}},
            "relationships":[{"kind":"covalent","bond":"oh","left":"oxygen","right":"hydrogen","order":"single","electron_origin":{"kind":"dative","donor":"oxygen","acceptor":"hydrogen"}}],
            "premise_ids":["premise.pattern.dative-oh"]
        },
        {
            "id":"Patterns.IonicHydroxide",
            "variables":{
                "metal":{"atom":{"element":"Li","formal_charge":1}},
                "oxygen":{"atom":{"element":"O","formal_charge":-1}},
                "hydrogen":{"atom":{"element":"H"}}
            },
            "relationships":[
                {"kind":"covalent","bond":"oh","left":"oxygen","right":"hydrogen","order":"single"},
                {"kind":"group_membership","group":"cation","atoms":["metal"]},
                {"kind":"group_membership","group":"anion","atoms":["oxygen","hydrogen"]},
                {"kind":"ionic_association","association":"salt","groups":["cation","anion"]}
            ],
            "premise_ids":["premise.pattern.ionic-hydroxide"]
        },
        {
            "id":"Patterns.AnyHydrogen",
            "variables":{"hydrogen":{"atom":{"element":"H"}}},
            "premise_ids":["premise.pattern.any-hydrogen"]
        },
        {
            "id":"Patterns.HydrogenPair",
            "variables":{"left":{"atom":{"element":"H"}},"right":{"atom":{"element":"H"}}},
            "premise_ids":["premise.pattern.hydrogen-pair"]
        },
        {
            "id":"Patterns.CheckedProticOH",
            "variables":{"oxygen":{"atom":{"element":"O"}},"hydrogen":{"atom":{"element":"H"}}},
            "relationships":[{"kind":"covalent","bond":"oh","left":"oxygen","right":"hydrogen","order":"single"}],
            "traits":[{"trait":"Traits.ProticOH","sites":{"oxygen":"oxygen","hydrogen":"hydrogen","bond":"oh"}}],
            "premise_ids":["premise.pattern.checked-protic-oh"]
        },
        {
            "id":"Patterns.ParameterElement",
            "variables":{"site":{"atom":{"element":{"parameter":"E"}}}},
            "premise_ids":["premise.pattern.parameter-element"]
        },
        {
            "id":"Patterns.RelationshipAlias",
            "variables":{"metal":{"atom":{"element":"Li"}}},
            "relationships":[
                {"kind":"group_membership","group":"first","atoms":["metal"]},
                {"kind":"group_membership","group":"second","atoms":["metal"]}
            ],
            "premise_ids":["premise.pattern.relationship-alias"]
        }
    ]);
    value
}

fn catalogue() -> ValidatedCatalogueBundle {
    ValidatedCatalogueBundle::validate(envelope(with_g2(fixture()))).unwrap()
}

fn request(role: &str, pattern: &str, structure: &str) -> PatternRoleInput {
    PatternRoleInput {
        role: role.to_owned(),
        pattern: GraphPatternId::from_str(pattern).unwrap(),
        structure: StructureId::from_str(structure).unwrap(),
    }
}

#[test]
fn canonical_raw_matching_covers_unique_shared_dative_ionic_and_metallic_sites() {
    let catalogue = catalogue();
    let no_parameters = BTreeMap::new();

    let metal = catalogue
        .raw_pattern_matches(
            &[request("metal", "Patterns.LithiumSite", "LithiumMetal")],
            &no_parameters,
        )
        .unwrap();
    assert_eq!(metal.len(), 1);
    let metal = &metal[0].roles()["metal"];
    assert_eq!(metal.atoms()["metal"].to_string(), "li");
    assert_eq!(metal.metallic_domains()["domain"].to_string(), "metallic");

    let shared = catalogue
        .raw_pattern_matches(
            &[request("water", "Patterns.WaterOH", "Water")],
            &no_parameters,
        )
        .unwrap();
    assert_eq!(shared.len(), 2);
    assert_eq!(
        shared
            .iter()
            .map(|binding| binding.roles()["water"].atoms()["hydrogen"].to_string())
            .collect::<Vec<_>>(),
        ["h1", "h2"]
    );
    assert!(
        catalogue
            .pattern_matches_are_automorphism_related(&shared[0], &shared[1])
            .unwrap()
    );

    let dative = catalogue
        .raw_pattern_matches(
            &[request("probe", "Patterns.DativeOH", "DativeWater")],
            &no_parameters,
        )
        .unwrap();
    assert_eq!(dative.len(), 1);
    assert_eq!(
        dative[0].roles()["probe"].atoms()["hydrogen"].to_string(),
        "h2"
    );

    let ionic = catalogue
        .raw_pattern_matches(
            &[request(
                "salt",
                "Patterns.IonicHydroxide",
                "LithiumHydroxide",
            )],
            &no_parameters,
        )
        .unwrap();
    assert_eq!(ionic.len(), 1);
    let ionic = &ionic[0].roles()["salt"];
    assert_eq!(ionic.groups()["cation"].to_string(), "lithium");
    assert_eq!(ionic.groups()["anion"].to_string(), "hydroxide");
    assert_eq!(ionic.ionic_associations()["salt"].to_string(), "ionic");

    let checked = catalogue
        .raw_pattern_matches(
            &[request("water", "Patterns.CheckedProticOH", "Water")],
            &no_parameters,
        )
        .unwrap();
    assert_eq!(checked.len(), 1);
    assert_eq!(
        checked[0].roles()["water"].atoms()["hydrogen"].to_string(),
        "h1"
    );
}

#[test]
fn atom_matching_is_injective_and_multi_role_enumeration_is_a_canonical_product() {
    let catalogue = catalogue();
    let no_parameters = BTreeMap::new();
    let pair = catalogue
        .raw_pattern_matches(
            &[request("gas", "Patterns.HydrogenPair", "Hydrogen")],
            &no_parameters,
        )
        .unwrap();
    assert_eq!(pair.len(), 2);
    for binding in &pair {
        let atoms = binding.roles()["gas"].atoms();
        assert_ne!(atoms["left"], atoms["right"]);
    }

    let combined = catalogue
        .raw_pattern_matches(
            &[
                request("metal", "Patterns.LithiumSite", "LithiumMetal"),
                request("water", "Patterns.WaterOH", "Water"),
            ],
            &no_parameters,
        )
        .unwrap();
    assert_eq!(combined.len(), 2);
    assert_eq!(
        combined[0].roles()["water"].atoms()["hydrogen"].to_string(),
        "h1"
    );
    assert_eq!(
        combined[1].roles()["water"].atoms()["hydrogen"].to_string(),
        "h2"
    );

    let aliases = catalogue
        .raw_pattern_matches(
            &[request(
                "salt",
                "Patterns.RelationshipAlias",
                "LithiumHydroxide",
            )],
            &no_parameters,
        )
        .unwrap();
    assert_eq!(aliases.len(), 1);
    assert_eq!(
        aliases[0].roles()["salt"].groups()["first"],
        aliases[0].roles()["salt"].groups()["second"]
    );
}

#[test]
fn matching_is_invariant_under_graph_and_catalogue_declaration_order() {
    let original = catalogue();
    let mut reordered = with_g2(fixture());
    reordered["bundle"]["structures"]
        .as_array_mut()
        .unwrap()
        .reverse();
    for structure in reordered["bundle"]["structures"].as_array_mut().unwrap() {
        if let Some(atoms) = structure.get_mut("atoms").and_then(Value::as_array_mut) {
            atoms.reverse();
        }
        if let Some(bonds) = structure.get_mut("bonds").and_then(Value::as_array_mut) {
            bonds.reverse();
        }
        if let Some(components) = structure
            .get_mut("components")
            .and_then(Value::as_array_mut)
        {
            components.reverse();
            for component in components {
                if let Some(atoms) = component.get_mut("atoms").and_then(Value::as_array_mut) {
                    atoms.reverse();
                }
                if let Some(bonds) = component.get_mut("bonds").and_then(Value::as_array_mut) {
                    bonds.reverse();
                }
            }
        }
        if let Some(associations) = structure
            .get_mut("associations")
            .and_then(Value::as_array_mut)
        {
            associations.reverse();
        }
        if let Some(domains) = structure.get_mut("domains").and_then(Value::as_array_mut) {
            domains.reverse();
        }
    }
    reordered["bundle"]["graph_patterns"]
        .as_array_mut()
        .unwrap()
        .reverse();
    let reordered = ValidatedCatalogueBundle::validate(envelope(reordered)).unwrap();
    let inputs = [
        request("water", "Patterns.WaterOH", "Water"),
        request("salt", "Patterns.IonicHydroxide", "LithiumHydroxide"),
    ];
    assert_eq!(
        original
            .raw_pattern_matches(&inputs, &BTreeMap::new())
            .unwrap(),
        reordered
            .raw_pattern_matches(&inputs, &BTreeMap::new())
            .unwrap()
    );
}

#[test]
fn genuinely_non_equivalent_sites_are_not_collapsed_by_automorphism() {
    let catalogue = catalogue();
    let matches = catalogue
        .raw_pattern_matches(
            &[request("probe", "Patterns.AnyHydrogen", "DativeWater")],
            &BTreeMap::new(),
        )
        .unwrap();
    assert_eq!(matches.len(), 2);
    assert!(
        !catalogue
            .pattern_matches_are_automorphism_related(&matches[0], &matches[1])
            .unwrap()
    );

    let group_multiplicity = catalogue
        .raw_pattern_matches(
            &[request(
                "probe",
                "Patterns.AnyHydrogen",
                "GroupMultiplicityProbe",
            )],
            &BTreeMap::new(),
        )
        .unwrap();
    assert_eq!(group_multiplicity.len(), 2);
    assert!(
        !catalogue
            .pattern_matches_are_automorphism_related(
                &group_multiplicity[0],
                &group_multiplicity[1]
            )
            .unwrap()
    );
}

#[test]
fn complete_structure_automorphisms_are_bijective_over_duplicate_relationships() {
    let catalogue = catalogue();
    let water = catalogue
        .structure_automorphisms(&StructureId::from_str("Water").unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(water.len(), 2);

    let duplicates = catalogue
        .structure_automorphisms(&StructureId::from_str("GroupMultiplicityProbe").unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(duplicates.len(), 2);
    for automorphism in duplicates {
        let group_targets = ["first", "second", "third"]
            .into_iter()
            .map(|group| automorphism.sites()[group].clone())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(group_targets.len(), 3);
    }
}

#[test]
fn provisional_matches_are_bound_to_their_catalogue_digest() {
    let catalogue = catalogue();
    let local = catalogue
        .raw_pattern_matches(
            &[request("probe", "Patterns.AnyHydrogen", "DativeWater")],
            &BTreeMap::new(),
        )
        .unwrap();

    let mut foreign_value = with_g2(fixture());
    let foreign_structure = foreign_value["bundle"]["structures"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|structure| structure["id"] == "DativeWater")
        .unwrap();
    foreign_structure["atoms"][1]["label"] = json!("x");
    foreign_structure["bonds"][0]["right"] = json!("x");
    let foreign = ValidatedCatalogueBundle::validate(envelope(foreign_value)).unwrap();
    let foreign_matches = foreign
        .raw_pattern_matches(
            &[request("probe", "Patterns.AnyHydrogen", "DativeWater")],
            &BTreeMap::new(),
        )
        .unwrap();
    let foreign_x = foreign_matches
        .iter()
        .find(|binding| binding.roles()["probe"].atoms()["hydrogen"].to_string() == "x")
        .unwrap();

    let error = catalogue
        .pattern_matches_are_automorphism_related(&local[0], foreign_x)
        .unwrap_err();
    assert_eq!(error.code(), CatalogueErrorCode::InvalidGraphPattern);
}

#[test]
fn element_parameters_are_typed_match_inputs_not_runtime_inference() {
    let catalogue = catalogue();
    let input = [request("site", "Patterns.ParameterElement", "Water")];
    let error = catalogue
        .raw_pattern_matches(&input, &BTreeMap::new())
        .unwrap_err();
    assert_eq!(error.code(), CatalogueErrorCode::InvalidGraphPattern);

    let parameters = BTreeMap::from([("E".to_owned(), ElementSymbol::from_str("O").unwrap())]);
    let matches = catalogue.raw_pattern_matches(&input, &parameters).unwrap();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].roles()["site"].atoms()["site"].to_string(), "o");

    let mut unsatisfied_first = with_g2(fixture());
    unsatisfied_first["bundle"]["graph_patterns"][7]["variables"] = json!({
        "absent":{"atom":{"element":"Ca"}},
        "parameterized":{"atom":{"element":{"parameter":"E"}}}
    });
    let catalogue = ValidatedCatalogueBundle::validate(envelope(unsatisfied_first)).unwrap();
    let error = catalogue
        .raw_pattern_matches(&input, &BTreeMap::new())
        .unwrap_err();
    assert_eq!(error.code(), CatalogueErrorCode::InvalidGraphPattern);
}

#[test]
fn raw_matching_rejects_factorial_work_before_materializing_candidates() {
    let mut value = with_g2(fixture());
    value["bundle"]["structures"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "id":"SevenHydrogenProbe","premise_id":"premise.structure.dative-water",
            "formula":"H7","representation":"molecular",
            "atoms":(1..=7).map(|index| json!({
                "label":format!("h{index}"),"element":"H","formal_charge":0,
                "non_bonding_electrons":1,"unpaired_electrons":1
            })).collect::<Vec<_>>()
        }));
    value["bundle"]["graph_patterns"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "id":"Patterns.SevenHydrogenProbe",
            "variables":(1..=7).map(|index| (format!("h{index}"), json!({"atom":{"element":"H"}}))).collect::<serde_json::Map<_,_>>(),
            "premise_ids":["premise.pattern.hydrogen-pair"]
        }));
    let catalogue = ValidatedCatalogueBundle::validate(envelope(value)).unwrap();
    let error = catalogue
        .raw_pattern_matches(
            &[request(
                "probe",
                "Patterns.SevenHydrogenProbe",
                "SevenHydrogenProbe",
            )],
            &BTreeMap::new(),
        )
        .unwrap_err();
    assert_eq!(error.code(), CatalogueErrorCode::InvalidGraphPattern);
    assert!(error.to_string().contains("work limit"));
}

#[test]
fn public_automorphism_comparison_reports_exhaustion_instead_of_false() {
    let mut value = with_g2(fixture());
    let groups = (1..=8)
        .flat_map(|atom| {
            (1..=atom).map(move |ordinal| {
                json!({"label":format!("g{atom}_{ordinal}"),"atoms":[format!("h{atom}")]})
            })
        })
        .collect::<Vec<_>>();
    value["bundle"]["structures"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "id":"AutomorphismWorkProbe","premise_id":"premise.structure.dative-water",
            "formula":"H8","representation":"molecular",
            "atoms":(1..=8).map(|index| json!({
                "label":format!("h{index}"),"element":"H","formal_charge":0,
                "non_bonding_electrons":1,"unpaired_electrons":1
            })).collect::<Vec<_>>(),
            "groups":groups
        }));
    let catalogue = ValidatedCatalogueBundle::validate(envelope(value)).unwrap();
    let matches = catalogue
        .raw_pattern_matches(
            &[request(
                "probe",
                "Patterns.AnyHydrogen",
                "AutomorphismWorkProbe",
            )],
            &BTreeMap::new(),
        )
        .unwrap();
    let h1 = matches
        .iter()
        .find(|matched| matched.roles()["probe"].atoms()["hydrogen"].to_string() == "h1")
        .unwrap();
    let h2 = matches
        .iter()
        .find(|matched| matched.roles()["probe"].atoms()["hydrogen"].to_string() == "h2")
        .unwrap();
    let error = catalogue
        .pattern_matches_are_automorphism_related(h1, h2)
        .unwrap_err();
    assert_eq!(error.code(), CatalogueErrorCode::InvalidGraphPattern);
    assert!(error.to_string().contains("work limit"));
}

#[test]
fn graph_pattern_schema_canonicalization_and_digest_are_exact() {
    let original = with_g2(fixture());
    let validator = jsonschema::draft202012::new(&schema()).unwrap();
    let errors = validator.iter_errors(&original).collect::<Vec<_>>();
    assert!(errors.is_empty(), "schema errors: {errors:?}");

    let original = envelope(original);
    let mut reordered: Value = serde_json::to_value(&original).unwrap();
    reordered["bundle"]["graph_patterns"][3]["relationships"]
        .as_array_mut()
        .unwrap()
        .reverse();
    reordered["bundle"]["graph_patterns"]
        .as_array_mut()
        .unwrap()
        .reverse();
    let reordered = envelope(reordered);
    assert_eq!(original.digest, reordered.digest);
    assert_eq!(
        original.canonical_json().unwrap(),
        reordered.canonical_json().unwrap()
    );

    let mut changed: Value = serde_json::to_value(&original).unwrap();
    changed["bundle"]["graph_patterns"][0]["premise_ids"] = json!(["premise.pattern.water-oh"]);
    assert_ne!(original.digest, envelope(changed).digest);
}

#[test]
fn invalid_pattern_references_types_and_unsupported_fields_are_rejected() {
    let assert_code = |value: Value, expected| {
        assert_eq!(
            ValidatedCatalogueBundle::validate(envelope(value))
                .unwrap_err()
                .code(),
            expected
        );
    };

    let mut unknown_atom = with_g2(fixture());
    unknown_atom["bundle"]["graph_patterns"][1]["relationships"][0]["right"] = json!("missing");
    assert_code(unknown_atom, CatalogueErrorCode::InvalidGraphPattern);

    let mut repeated_binding = with_g2(fixture());
    repeated_binding["bundle"]["graph_patterns"][1]["relationships"][0]["bond"] = json!("oxygen");
    assert_code(repeated_binding, CatalogueErrorCode::InvalidGraphPattern);

    let mut wrong_trait_kind = with_g2(fixture());
    wrong_trait_kind["bundle"]["graph_patterns"][6]["traits"][0]["sites"]["bond"] = json!("oxygen");
    assert_code(wrong_trait_kind, CatalogueErrorCode::InvalidGraphPattern);

    let mut missing_premise = with_g2(fixture());
    missing_premise["bundle"]["graph_patterns"][0]["premise_ids"] =
        json!(["premise.pattern.absent"]);
    assert_code(missing_premise, CatalogueErrorCode::UnknownReference);

    let mut invalid_dative = with_g2(fixture());
    invalid_dative["bundle"]["graph_patterns"][2]["relationships"][0]["order"] = json!("double");
    assert_code(invalid_dative, CatalogueErrorCode::InvalidGraphPattern);

    let mut unsupported = with_g2(fixture());
    unsupported["bundle"]["graph_patterns"][0]["recursive_path"] = json!(true);
    assert!(serde_json::from_value::<CatalogueEnvelope>(unsupported).is_err());
}

#[test]
fn omitted_and_empty_pattern_arrays_preserve_legacy_semantics() {
    let omitted = envelope(fixture());
    let mut empty = fixture();
    empty["bundle"]["graph_patterns"] = json!([]);
    let empty = envelope(empty);
    assert_eq!(omitted.digest, empty.digest);
    assert_eq!(
        omitted.canonical_json().unwrap(),
        empty.canonical_json().unwrap()
    );
}
