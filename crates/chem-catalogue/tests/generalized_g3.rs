use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
    str::FromStr,
};

use chem_catalogue::{
    CatalogueEnvelope, CatalogueErrorCode, GeneralizedCaseSelection,
    GeneralizedElaborationFailureClass, GeneralizedRoleInput, RepresentationRecord, RuleSideRecord,
    ValidatedCatalogueBundle,
};
use chem_domain::{ReactionRuleId, StructureId};
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
            "id":id,"statement":statement,
            "evidence":["evidence.openstax.chemistry-2e"],
            "review":{"status":"provisional"},"rule_version":"1"
        }));
}

fn replace_string(value: &mut Value, from: &str, to: &str) {
    match value {
        Value::String(text) if text == from => to.clone_into(text),
        Value::Array(values) => values
            .iter_mut()
            .for_each(|value| replace_string(value, from, to)),
        Value::Object(values) => values
            .values_mut()
            .for_each(|value| replace_string(value, from, to)),
        _ => {}
    }
}

#[allow(clippy::too_many_lines)]
fn with_g3(mut value: Value) -> Value {
    replace_string(
        &mut value,
        "Rules.AlkaliMetalWithWater",
        "Rules.LegacyLithiumWithWater",
    );
    replace_string(&mut value, "LithiumMetal", "LegacyLithiumMetal");
    replace_string(&mut value, "LithiumHydroxide", "LegacyLithiumHydroxide");
    for (id, statement) in [
        ("premise.element.h", "Reviewed hydrogen identity."),
        ("premise.element.li", "Reviewed lithium identity."),
        ("premise.element.na", "Reviewed sodium identity."),
        ("premise.element.k", "Reviewed potassium identity."),
        ("premise.element.o", "Reviewed oxygen identity."),
        ("premise.element.ca", "Reviewed calcium identity."),
        (
            "premise.category.alkali",
            "Group-one metals excluding hydrogen form the reviewed migration category.",
        ),
        (
            "premise.template.metal",
            "Reviewed elemental alkali-metal template.",
        ),
        (
            "premise.template.hydroxide",
            "Reviewed alkali-hydroxide template.",
        ),
        (
            "premise.application.li-metal",
            "Reviewed lithium metal application.",
        ),
        (
            "premise.application.na-metal",
            "Reviewed sodium metal application.",
        ),
        (
            "premise.application.k-metal",
            "Reviewed potassium metal application.",
        ),
        (
            "premise.application.li-oh",
            "Reviewed lithium hydroxide application.",
        ),
        (
            "premise.application.na-oh",
            "Reviewed sodium hydroxide application.",
        ),
        (
            "premise.application.k-oh",
            "Reviewed potassium hydroxide application.",
        ),
        (
            "premise.pattern.metal",
            "Reviewed complete metallic-site pattern.",
        ),
        (
            "premise.pattern.water",
            "Reviewed complete water pattern with symmetric proton sites.",
        ),
        (
            "premise.pattern.dative-pair",
            "Reviewed separated donor/acceptor pattern.",
        ),
        (
            "premise.valence.g3",
            "Reviewed sodium, potassium, and separated oxygen donor states.",
        ),
        (
            "premise.rule.generalized-water",
            "Reviewed generalized alkali-metal with water family.",
        ),
        (
            "premise.rule.alkali-water.standard-outcome",
            "Reviewed representative outcome for the Li, Na, and K water-reaction family.",
        ),
        (
            "premise.observation.alkali-metal-disappears",
            "The reacting alkali metal is consumed in the representative family outcome.",
        ),
        (
            "premise.case.generalized-water",
            "Reviewed common Li, Na, and K water-reaction case.",
        ),
        (
            "premise.rule.oxygen-design",
            "Reviewed oxygen-family design boundary.",
        ),
        (
            "premise.case.superoxide-unsupported",
            "Heavy superoxide bonding is outside the current domain.",
        ),
        (
            "premise.rule.dative-fixture",
            "Reviewed generalized directed dative rewrite fixture.",
        ),
        (
            "premise.case.dative-fixture",
            "Reviewed supported directed dative fixture case.",
        ),
        (
            "premise.structure.oxygen",
            "Reviewed dioxygen structural fixture.",
        ),
        (
            "premise.structure.dative-donor",
            "Reviewed monatomic donor fixture.",
        ),
        (
            "premise.structure.dative-acceptor",
            "Reviewed monatomic acceptor fixture.",
        ),
        (
            "premise.structure.dative-adduct",
            "Reviewed directed dative adduct fixture.",
        ),
        (
            "premise.trait.donor.definition",
            "A donor lone-pair trait exposes its exact donor atom.",
        ),
        (
            "premise.trait.donor.assertion",
            "The reviewed donor fixture satisfies the donor lone-pair trait.",
        ),
        (
            "premise.trait.acceptor.definition",
            "An empty acceptor-site trait exposes its exact acceptor atom.",
        ),
        (
            "premise.trait.acceptor.assertion",
            "The reviewed acceptor fixture satisfies the empty acceptor-site trait.",
        ),
        (
            "premise.pattern.dative-donor",
            "Reviewed donor trait-site pattern.",
        ),
        (
            "premise.pattern.dative-acceptor",
            "Reviewed acceptor trait-site pattern.",
        ),
    ] {
        add_premise(&mut value, id, statement);
    }

    value["bundle"]["elements"] = json!([
        {"symbol":"H","name":"Hydrogen","atomic_number":1,"period":1,"group":1,"block":"s","premise_ids":["premise.element.h"]},
        {"symbol":"Li","name":"Lithium","atomic_number":3,"period":2,"group":1,"block":"s","premise_ids":["premise.element.li"]},
        {"symbol":"Na","name":"Sodium","atomic_number":11,"period":3,"group":1,"block":"s","premise_ids":["premise.element.na"]},
        {"symbol":"K","name":"Potassium","atomic_number":19,"period":4,"group":1,"block":"s","premise_ids":["premise.element.k"]},
        {"symbol":"O","name":"Oxygen","atomic_number":8,"period":2,"group":16,"block":"p","premise_ids":["premise.element.o"]},
        {"symbol":"Ca","name":"Calcium","atomic_number":20,"period":4,"group":2,"block":"s","premise_ids":["premise.element.ca"]}
    ]);
    value["bundle"]["element_categories"] = json!([{
        "id":"Categories.AlkaliMetal","subject":"element",
        "membership":{"kind":"predicate","predicate":{"kind":"all","predicates":[
            {"kind":"equals","field":"group","value":1},
            {"kind":"not","predicate":{"kind":"equals","field":"symbol","value":"H"}}
        ]}},"premise_ids":["premise.category.alkali"]
    }]);
    value["bundle"]["valence_premises"].as_array_mut().unwrap().push(json!({
        "premise_id":"premise.valence.g3",
        "neutral_valence":[
            {"element":"Na","neutral_valence_electrons":1},
            {"element":"K","neutral_valence_electrons":1},
            {"element":"O","neutral_valence_electrons":6}
        ],
        "supported_states":[
            {"element":"Na","formal_charge":1,"non_bonding_electrons":0,"unpaired_electrons":0,"covalent_bond_order_sum":0},
            {"element":"K","formal_charge":1,"non_bonding_electrons":0,"unpaired_electrons":0,"covalent_bond_order_sum":0},
            {"element":"O","formal_charge":-2,"non_bonding_electrons":8,"unpaired_electrons":0,"covalent_bond_order_sum":0}
        ],
        "metallic_domain_states":[
            {"element":"Na","site_formal_charge":1,"site_local_electrons":0,"delocalized_electrons_per_site":1},
            {"element":"K","site_formal_charge":1,"site_local_electrons":0,"delocalized_electrons_per_site":1}
        ]
    }));
    value["bundle"]["structure_templates"] = json!([
        {
            "id":"Templates.ElementalAlkaliMetal","parameters":{"M":{"kind":"element","category":"Categories.AlkaliMetal"}},
            "representation":"metallic",
            "sites":[{"label":"metal","element":{"parameter":"M"},"formal_charge":1,"non_bonding_electrons":0,"unpaired_electrons":0}],
            "domains":[{"label":"domain","sites":["metal"],"delocalized_electrons":1}],
            "premise_ids":["premise.template.metal"]
        },
        {
            "id":"Templates.AlkaliHydroxide","parameters":{"M":{"kind":"element","category":"Categories.AlkaliMetal"}},
            "representation":"ionic",
            "components":[
                {"label":"cation","atoms":[{"label":"metal","element":{"parameter":"M"},"formal_charge":1,"non_bonding_electrons":0,"unpaired_electrons":0}]},
                {"label":"hydroxide","atoms":[
                    {"label":"oxygen","element":"O","formal_charge":-1,"non_bonding_electrons":6,"unpaired_electrons":0},
                    {"label":"hydrogen","element":"H","formal_charge":0,"non_bonding_electrons":0,"unpaired_electrons":0}
                ],"bonds":[{"left":"oxygen","right":"hydrogen","order":"single"}]}
            ],
            "associations":[{"label":"salt","components":["cation","hydroxide"]}],
            "premise_ids":["premise.template.hydroxide"]
        }
    ]);
    value["bundle"]["structure_applications"] = json!([
        {"id":"LithiumMetal","template":"Templates.ElementalAlkaliMetal","arguments":{"M":"Li"},"formula":"Li","premise_ids":["premise.application.li-metal"]},
        {"id":"SodiumMetal","template":"Templates.ElementalAlkaliMetal","arguments":{"M":"Na"},"formula":"Na","premise_ids":["premise.application.na-metal"]},
        {"id":"PotassiumMetal","template":"Templates.ElementalAlkaliMetal","arguments":{"M":"K"},"formula":"K","premise_ids":["premise.application.k-metal"]},
        {"id":"LithiumHydroxide","template":"Templates.AlkaliHydroxide","arguments":{"M":"Li"},"formula":"LiOH","premise_ids":["premise.application.li-oh"]},
        {"id":"SodiumHydroxide","template":"Templates.AlkaliHydroxide","arguments":{"M":"Na"},"formula":"NaOH","premise_ids":["premise.application.na-oh"]},
        {"id":"PotassiumHydroxide","template":"Templates.AlkaliHydroxide","arguments":{"M":"K"},"formula":"KOH","premise_ids":["premise.application.k-oh"]}
    ]);
    value["bundle"]["structural_traits"] = json!([
        {
            "id":"Traits.DonorLonePair",
            "sites":{"donor":"atom"},
            "values":{"paired_electrons":{"kind":"atom_non_bonding_electrons","site":"donor"}},
            "premise_ids":["premise.trait.donor.definition"]
        },
        {
            "id":"Traits.EmptyAcceptorSite",
            "sites":{"acceptor":"atom"},
            "values":{"formal_charge":{"kind":"atom_formal_charge","site":"acceptor"}},
            "premise_ids":["premise.trait.acceptor.definition"]
        }
    ]);
    value["bundle"]["structures"].as_array_mut().unwrap().extend([
        json!({
            "id":"Oxygen","premise_id":"premise.structure.oxygen","formula":"O2","representation":"molecular",
            "atoms":[
                {"label":"o1","element":"O","formal_charge":0,"non_bonding_electrons":4,"unpaired_electrons":0},
                {"label":"o2","element":"O","formal_charge":0,"non_bonding_electrons":4,"unpaired_electrons":0}
            ],"bonds":[{"left":"o1","right":"o2","order":"double"}]
        }),
        json!({
            "id":"DativeDonor","premise_id":"premise.structure.dative-donor","formula":"O","representation":"ion",
            "atoms":[{"label":"donor","element":"O","formal_charge":-2,"non_bonding_electrons":8,"unpaired_electrons":0}],
            "traits":[{
                "trait":"Traits.DonorLonePair","sites":{"donor":"donor"},
                "values":{"paired_electrons":8},
                "premise_ids":["premise.trait.donor.assertion"]
            }]
        }),
        json!({
            "id":"DativeAcceptor","premise_id":"premise.structure.dative-acceptor","formula":"H","representation":"ion",
            "atoms":[{"label":"acceptor","element":"H","formal_charge":1,"non_bonding_electrons":0,"unpaired_electrons":0}],
            "traits":[{
                "trait":"Traits.EmptyAcceptorSite","sites":{"acceptor":"acceptor"},
                "values":{"formal_charge":1},
                "premise_ids":["premise.trait.acceptor.assertion"]
            }]
        }),
        json!({
            "id":"DativeAdduct","premise_id":"premise.structure.dative-adduct","formula":"HO","representation":"ion",
            "atoms":[
                {"label":"donor","element":"O","formal_charge":-1,"non_bonding_electrons":6,"unpaired_electrons":0},
                {"label":"acceptor","element":"H","formal_charge":0,"non_bonding_electrons":0,"unpaired_electrons":0}
            ],"bonds":[{"left":"donor","right":"acceptor","order":"single","electron_origin":{"kind":"dative","donor":"donor","acceptor":"acceptor"}}]
        })
    ]);
    value["bundle"]["graph_patterns"] = json!([
        {
            "id":"Patterns.Metal","variables":{"metal":{"atom":{}}},
            "relationships":[{"kind":"metallic_domain","domain":"domain","sites":["metal"],"delocalized_electrons":1}],
            "premise_ids":["premise.pattern.metal"]
        },
        {
            "id":"Patterns.Water","variables":{"oxygen":{"atom":{"element":"O"}},"proton":{"atom":{"element":"H"}},"retained":{"atom":{"element":"H"}}},
            "relationships":[
                {"kind":"covalent","bond":"broken","left":"oxygen","right":"proton","order":"single"},
                {"kind":"covalent","bond":"retained_bond","left":"oxygen","right":"retained","order":"single"}
            ],"premise_ids":["premise.pattern.water"]
        },
        {
            "id":"Patterns.DativeDonor","variables":{
                "donor":{"atom":{"element":"O","formal_charge":-2,"non_bonding_electrons":8}}
            },
            "traits":[{"trait":"Traits.DonorLonePair","sites":{"donor":"donor"}}],
            "premise_ids":["premise.pattern.dative-donor"]
        },
        {
            "id":"Patterns.DativeAcceptor","variables":{
                "acceptor":{"atom":{"element":"H","formal_charge":1}}
            },
            "traits":[{"trait":"Traits.EmptyAcceptorSite","sites":{"acceptor":"acceptor"}}],
            "premise_ids":["premise.pattern.dative-acceptor"]
        }
    ]);
    value["bundle"]["generalized_rules"] =
        json!([alkali_water_rule(), oxygen_design_rule(), dative_rule()]);
    value
}

#[allow(clippy::too_many_lines)]
fn alkali_water_rule() -> Value {
    let premises = [
        "premise.rule.generalized-water",
        "premise.case.generalized-water",
        "premise.rule.alkali-water.standard-outcome",
        "premise.valence.li-h-o.initial-domain",
        "premise.valence.g3",
        "premise.observation.hydrogen-evolves",
        "premise.observation.alkali-metal-disappears",
        "premise.pattern.metal",
        "premise.pattern.water",
        "premise.category.alkali",
        "premise.element.li",
        "premise.element.na",
        "premise.element.k",
        "premise.template.metal",
        "premise.template.hydroxide",
        "premise.application.li-metal",
        "premise.application.na-metal",
        "premise.application.k-metal",
        "premise.application.li-oh",
        "premise.application.na-oh",
        "premise.application.k-oh",
        "premise.structure.water",
        "premise.structure.hydrogen",
    ];
    let correspondence = [
        ("metal[1].metal","hydroxide[1].cation.metal"),
        ("metal[2].metal","hydroxide[2].cation.metal"),
        ("water[1].oxygen","hydroxide[1].hydroxide.oxygen"),
        ("water[1].retained","hydroxide[1].hydroxide.hydrogen"),
        ("water[2].oxygen","hydroxide[2].hydroxide.oxygen"),
        ("water[2].retained","hydroxide[2].hydroxide.hydrogen"),
        ("water[1].proton","gasProduct[1].h1"),
        ("water[2].proton","gasProduct[1].h2")
    ].map(|(reactant,product)| json!({"reactant":reactant,"product":product,"premise_ids":["premise.case.generalized-water"]}));
    json!({
        "id":"Rules.AlkaliMetalWithWater",
        "parameters":{"M":{"kind":"element","category":"Categories.AlkaliMetal"}},
        "roles":{
            "metal":{"side":"reactant","representation":"metallic","coefficient":2},
            "water":{"side":"reactant","representation":"molecular","coefficient":2},
            "hydroxide":{"side":"product","representation":"ionic","coefficient":2},
            "gasProduct":{"side":"product","representation":"molecular","coefficient":1}
        },
        "reactants":{
            "metal":{"kind":"template","template":"Templates.ElementalAlkaliMetal","arguments":{"M":{"parameter":"M"}}},
            "water":{"kind":"exact","structure":"Water"}
        },
        "cases":[{
            "id":"common","status":"supported","when":{"kind":"always"},
            "products":{
                "hydroxide":{"kind":"template","template":"Templates.AlkaliHydroxide","arguments":{"M":{"parameter":"M"}}},
                "gasProduct":{"kind":"exact","structure":"Hydrogen"}
            },
            "patterns":{"metal":"Patterns.Metal","water":"Patterns.Water"},
            "correspondence":correspondence,
            "rewrite":alkali_rewrite(),
            "observation_compatibility":[
                {"subject_role":"gasProduct","predicate":"evolves","evidence_subject":"hydrogen","premise_id":"premise.observation.hydrogen-evolves"},
                {"subject_role":"metal","predicate":"disappears","evidence_subject":"alkali metal","premise_id":"premise.observation.alkali-metal-disappears"}
            ],
            "premise_ids":["premise.case.generalized-water"]
        }],
        "applicability":{"premise_id":"premise.rule.alkali-water.standard-outcome","request_relation":"contact","required_context":"reviewed representative educational outcome"},
        "model_assumptions":{"event":"representative","sequence":"explanatory","premise_ids":["premise.rule.alkali-water.standard-outcome"]},
        "premise_ids":premises
    })
}

fn alkali_rewrite() -> Value {
    let mut operations = Vec::new();
    let operation_premises = json!([
        "premise.case.generalized-water",
        "premise.valence.li-h-o.initial-domain",
        "premise.valence.g3"
    ]);
    for instance in 1..=2 {
        operations.push(json!({
            "kind":"release_metallic","site":format!("metal[{instance}].metal"),"domain":format!("metal[{instance}].domain"),
            "allocation":"retain_electron","before":{"site":[1,0,0],"domain_electrons":1},"after":{"site":[0,1,1],"domain_electrons":0},
            "premise_ids":operation_premises.clone()
        }));
        operations.push(json!({
            "kind":"cleave_covalent","edge":[format!("water[{instance}].oxygen"),format!("water[{instance}].proton"),"single"],
            "allocation":{"heterolytic_to":format!("water[{instance}].oxygen")},"before":{"left":[0,4,0],"right":[0,0,0]},"after":{"left":[-1,6,0],"right":[1,0,0]},
            "premise_ids":operation_premises.clone()
        }));
        operations.push(json!({
            "kind":"transfer_electron","count":1,"donor":format!("metal[{instance}].metal"),"acceptor":format!("water[{instance}].proton"),
            "before":{"donor":[0,1,1],"acceptor":[1,0,0]},"after":{"donor":[1,0,0],"acceptor":[0,1,1]},
            "premise_ids":operation_premises.clone()
        }));
        operations.push(json!({
            "kind":"associate_ionic","label":format!("ionic.{instance}"),
            "components":[[format!("metal[{instance}].metal")],[format!("water[{instance}].oxygen"),format!("water[{instance}].retained")]],
            "component_charges":[1,-1],"premise_ids":operation_premises.clone()
        }));
        operations.push(json!({
            "kind":"assign_product","atoms":[format!("metal[{instance}].metal"),format!("water[{instance}].oxygen"),format!("water[{instance}].retained")],
            "product":format!("hydroxide[{instance}]"),"premise_ids":["premise.case.generalized-water"]
        }));
    }
    operations.push(json!({
        "kind":"form_covalent","edge":["water[1].proton","water[2].proton","single"],"electron_contribution":{"left":1,"right":1},
        "before":{"left":[0,1,1],"right":[0,1,1]},"after":{"left":[0,0,0],"right":[0,0,0]},"premise_ids":operation_premises
    }));
    operations.push(json!({
        "kind":"assign_product","atoms":["water[1].proton","water[2].proton"],"product":"gasProduct[1]","premise_ids":["premise.case.generalized-water"]
    }));
    Value::Array(operations)
}

fn oxygen_design_rule() -> Value {
    json!({
        "id":"Rules.AlkaliMetalWithOxygenDesign",
        "parameters":{"M":{"kind":"element","category":"Categories.AlkaliMetal"}},
        "roles":{
            "metal":{"side":"reactant","representation":"metallic","coefficient":1},
            "oxygen":{"side":"reactant","representation":"molecular","coefficient":1},
            "oxide":{"side":"product","representation":"ionic","coefficient":1}
        },
        "reactants":{
            "metal":{"kind":"template","template":"Templates.ElementalAlkaliMetal","arguments":{"M":{"parameter":"M"}}},
            "oxygen":{"kind":"exact","structure":"Oxygen"}
        },
        "cases":[{
            "id":"heavy-superoxide","status":"unsupported",
            "when":{"kind":"parameter_equals","parameter":"M","value":"K"},
            "required_feature":"Features.SuperoxideBonding",
            "explanation":"Superoxide is outside the current structural domain.",
            "premise_ids":["premise.case.superoxide-unsupported"]
        }],
        "applicability":{"premise_id":"premise.rule.oxygen-design","request_relation":"contact","required_context":"oxygen-family design boundary"},
        "model_assumptions":{"event":"representative","sequence":"explanatory","premise_ids":["premise.rule.oxygen-design"]},
        "premise_ids":[
            "premise.rule.oxygen-design","premise.case.superoxide-unsupported",
            "premise.category.alkali","premise.element.li","premise.element.na","premise.element.k",
            "premise.template.metal","premise.application.li-metal","premise.application.na-metal","premise.application.k-metal",
            "premise.structure.oxygen"
        ]
    })
}

fn dative_rule() -> Value {
    json!({
        "id":"Rules.DativeDonorAcceptorFixture",
        "parameters":{
            "D":{"kind":"structure","trait":"Traits.DonorLonePair"},
            "A":{"kind":"structure","trait":"Traits.EmptyAcceptorSite"}
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
            "products":{"adduct":{"kind":"exact","structure":"DativeAdduct"}},
            "patterns":{"donor":"Patterns.DativeDonor","acceptor":"Patterns.DativeAcceptor"},
            "correspondence":[
                {"reactant":"donor[1].donor","product":"adduct[1].donor","premise_ids":["premise.case.dative-fixture"]},
                {"reactant":"acceptor[1].acceptor","product":"adduct[1].acceptor","premise_ids":["premise.case.dative-fixture"]}
            ],
            "rewrite":[
                {"kind":"form_dative","donor":"donor[1].donor","acceptor":"acceptor[1].acceptor","before":{"left":[-2,8,0],"right":[1,0,0]},"after":{"left":[-1,6,0],"right":[0,0,0]},"premise_ids":["premise.case.dative-fixture"]},
                {"kind":"assign_product","atoms":["donor[1].donor","acceptor[1].acceptor"],"product":"adduct[1]","premise_ids":["premise.case.dative-fixture"]}
            ],
            "premise_ids":["premise.case.dative-fixture"]
        }],
        "applicability":{"premise_id":"premise.rule.dative-fixture","request_relation":"contact","required_context":"directed dative structural fixture"},
        "model_assumptions":{"event":"representative","sequence":"explanatory","premise_ids":["premise.rule.dative-fixture"]},
        "premise_ids":[
            "premise.rule.dative-fixture","premise.case.dative-fixture",
            "premise.structure.dative-donor","premise.structure.dative-acceptor","premise.structure.dative-adduct",
            "premise.trait.donor.definition","premise.trait.donor.assertion",
            "premise.trait.acceptor.definition","premise.trait.acceptor.assertion",
            "premise.pattern.dative-donor","premise.pattern.dative-acceptor"
        ]
    })
}

fn catalogue() -> ValidatedCatalogueBundle {
    ValidatedCatalogueBundle::validate(envelope(with_g3(fixture()))).unwrap()
}

fn role_input(
    role: &str,
    structure: &str,
    coefficient: u32,
    side: RuleSideRecord,
    representation: RepresentationRecord,
) -> GeneralizedRoleInput {
    GeneralizedRoleInput {
        role: role.to_owned(),
        structure: StructureId::from_str(structure).unwrap(),
        coefficient,
        side,
        representation,
    }
}

fn water_inputs(metal: &str, hydroxide: &str) -> Vec<GeneralizedRoleInput> {
    vec![
        role_input(
            "metal",
            metal,
            2,
            RuleSideRecord::Reactant,
            RepresentationRecord::Metallic,
        ),
        role_input(
            "water",
            "Water",
            2,
            RuleSideRecord::Reactant,
            RepresentationRecord::Molecular,
        ),
        role_input(
            "hydroxide",
            hydroxide,
            2,
            RuleSideRecord::Product,
            RepresentationRecord::Ionic,
        ),
        role_input(
            "gasProduct",
            "Hydrogen",
            1,
            RuleSideRecord::Product,
            RepresentationRecord::Molecular,
        ),
    ]
}

#[test]
fn finite_domains_select_disjoint_supported_and_unsupported_cases() {
    let catalogue = catalogue();
    let water = ReactionRuleId::from_str("Rules.AlkaliMetalWithWater").unwrap();
    assert_eq!(
        catalogue.generalized_parameter_domains(&water).unwrap()["M"]
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["K", "Li", "Na"]
    );
    for metal in ["Li", "Na", "K"] {
        assert!(matches!(
            catalogue
                .select_generalized_case(
                    &water,
                    &BTreeMap::from([("M".to_owned(), metal.to_owned())])
                )
                .unwrap(),
            Some(GeneralizedCaseSelection::Supported(_))
        ));
    }
    let invalid = catalogue
        .select_generalized_case(&water, &BTreeMap::from([("M".to_owned(), "Ca".to_owned())]))
        .unwrap_err();
    assert_eq!(invalid.code(), CatalogueErrorCode::InvalidGeneralizedRule);

    let oxygen = ReactionRuleId::from_str("Rules.AlkaliMetalWithOxygenDesign").unwrap();
    assert!(matches!(
        catalogue
            .select_generalized_case(&oxygen, &BTreeMap::from([("M".to_owned(), "K".to_owned())]))
            .unwrap(),
        Some(GeneralizedCaseSelection::Unsupported(_))
    ));
    assert!(
        catalogue
            .select_generalized_case(
                &oxygen,
                &BTreeMap::from([("M".to_owned(), "Li".to_owned())])
            )
            .unwrap()
            .is_none()
    );
}

#[test]
fn dative_fixture_and_total_correspondence_are_inert_validated_data() {
    let catalogue = catalogue();
    let id = ReactionRuleId::from_str("Rules.DativeDonorAcceptorFixture").unwrap();
    let rule = catalogue.generalized_rule(&id).unwrap();
    let domains = catalogue.generalized_parameter_domains(&id).unwrap();
    assert_eq!(domains["D"].iter().collect::<Vec<_>>(), ["DativeDonor"]);
    assert_eq!(domains["A"].iter().collect::<Vec<_>>(), ["DativeAcceptor"]);
    let serialized = serde_json::to_value(rule).unwrap();
    assert_eq!(serialized["cases"][0]["rewrite"][0]["kind"], "form_dative");
    assert_eq!(
        serialized["cases"][0]["rewrite"][0]["donor"],
        "donor[1].donor"
    );
    assert_eq!(
        serialized["cases"][0]["rewrite"][0]["acceptor"],
        "acceptor[1].acceptor"
    );
    assert_eq!(
        serialized["cases"][0]["correspondence"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn family_and_trait_provenance_is_bound_for_every_finite_member() {
    let catalogue = catalogue();
    let water = ReactionRuleId::from_str("Rules.AlkaliMetalWithWater").unwrap();
    let serialized = serde_json::to_value(catalogue.generalized_rule(&water).unwrap()).unwrap();
    let premises = serialized["premise_ids"].as_array().unwrap();
    for (metal, element_premise, application_premise) in [
        ("Li", "premise.element.li", "premise.application.li-metal"),
        ("Na", "premise.element.na", "premise.application.na-metal"),
        ("K", "premise.element.k", "premise.application.k-metal"),
    ] {
        assert!(matches!(
            catalogue
                .select_generalized_case(
                    &water,
                    &BTreeMap::from([("M".to_owned(), metal.to_owned())])
                )
                .unwrap(),
            Some(GeneralizedCaseSelection::Supported(_))
        ));
        for required in [
            element_premise,
            application_premise,
            "premise.rule.alkali-water.standard-outcome",
            "premise.valence.li-h-o.initial-domain",
            "premise.valence.g3",
            "premise.observation.alkali-metal-disappears",
        ] {
            assert!(premises.iter().any(|premise| premise == required));
        }
    }

    let dative = ReactionRuleId::from_str("Rules.DativeDonorAcceptorFixture").unwrap();
    let dative = serde_json::to_value(catalogue.generalized_rule(&dative).unwrap()).unwrap();
    for required in [
        "premise.trait.donor.definition",
        "premise.trait.donor.assertion",
        "premise.trait.acceptor.definition",
        "premise.trait.acceptor.assertion",
    ] {
        assert!(
            dative["premise_ids"]
                .as_array()
                .unwrap()
                .iter()
                .any(|premise| premise == required)
        );
    }
}

#[test]
fn generalized_family_elaborates_member_specific_concrete_certificates() {
    let catalogue = catalogue();
    let rule = ReactionRuleId::from_str("Rules.AlkaliMetalWithWater").unwrap();
    for (symbol, metal, hydroxide) in [
        ("Li", "LithiumMetal", "LithiumHydroxide"),
        ("Na", "SodiumMetal", "SodiumHydroxide"),
        ("K", "PotassiumMetal", "PotassiumHydroxide"),
    ] {
        let elaborated = catalogue
            .elaborate_generalized_rule(&rule, &water_inputs(metal, hydroxide))
            .unwrap()
            .unwrap();
        assert_eq!(elaborated.parameter_binding["M"], symbol);
        assert_eq!(elaborated.case_id, "common");
        assert_eq!(elaborated.equivalent_match_count, 4);
        let parameter_premises = elaborated.parameter_premise_ids["M"]
            .iter()
            .map(ToString::to_string)
            .collect::<BTreeSet<_>>();
        assert!(parameter_premises.contains("premise.category.alkali"));
        assert!(parameter_premises.contains(&format!("premise.element.{}", symbol.to_lowercase())));
        let metal_premises = elaborated.role_premise_ids["metal"]
            .iter()
            .map(ToString::to_string)
            .collect::<BTreeSet<_>>();
        assert!(metal_premises.contains("premise.template.metal"));
        assert!(metal_premises.contains("premise.pattern.metal"));
        assert!(metal_premises.contains(&format!(
            "premise.application.{}-metal",
            symbol.to_lowercase()
        )));
        assert_eq!(elaborated.matched_sites.len(), 4);
        assert!(elaborated.matched_sites["water[1]"].contains_key("broken"));
        assert!(elaborated.matched_sites["metal[1]"].contains_key("domain"));
        assert_eq!(
            elaborated.rule.reactant_pattern[0].structure_id.to_string(),
            metal
        );
        assert_eq!(
            elaborated.rule.product_pattern[1].structure_id.to_string(),
            hydroxide
        );
        assert!(
            elaborated
                .rule
                .mapping_template
                .iter()
                .any(|pair| pair.reactant == "water[1].h1" || pair.reactant == "water[1].h2")
        );
    }
}

#[test]
fn certificate_symmetry_never_rewrites_reference_shaped_free_text() {
    let mut value = with_g3(fixture());
    value["bundle"]["generalized_rules"][0]["applicability"]["required_context"] =
        json!("water[1]");
    value["bundle"]["generalized_rules"][0]["cases"][0]["observation_compatibility"][0]["evidence_subject"] =
        json!("water[2]");
    let catalogue = ValidatedCatalogueBundle::validate(envelope(value)).unwrap();
    let rule = ReactionRuleId::from_str("Rules.AlkaliMetalWithWater").unwrap();
    let elaborated = catalogue
        .elaborate_generalized_rule(&rule, &water_inputs("LithiumMetal", "LithiumHydroxide"))
        .unwrap()
        .unwrap();
    assert_eq!(elaborated.rule.applicability.required_context, "water[1]");
    assert_eq!(
        elaborated.rule.observation_compatibility[0].evidence_subject,
        "water[2]"
    );
}

#[test]
fn generalized_dative_elaboration_uses_checked_directed_trait_sites() {
    let catalogue = catalogue();
    let rule = ReactionRuleId::from_str("Rules.DativeDonorAcceptorFixture").unwrap();
    let inputs = vec![
        role_input(
            "donor",
            "DativeDonor",
            1,
            RuleSideRecord::Reactant,
            RepresentationRecord::Ion,
        ),
        role_input(
            "acceptor",
            "DativeAcceptor",
            1,
            RuleSideRecord::Reactant,
            RepresentationRecord::Ion,
        ),
        role_input(
            "adduct",
            "DativeAdduct",
            1,
            RuleSideRecord::Product,
            RepresentationRecord::Ion,
        ),
    ];
    let elaborated = catalogue
        .elaborate_generalized_rule(&rule, &inputs)
        .unwrap()
        .unwrap();
    assert_eq!(elaborated.parameter_binding["D"], "DativeDonor");
    assert_eq!(elaborated.parameter_binding["A"], "DativeAcceptor");
    let serialized = serde_json::to_value(&elaborated.rule.operation_template[0]).unwrap();
    assert_eq!(serialized["kind"], "form_dative");
    assert_eq!(serialized["donor"], "donor[1].donor");
    assert_eq!(serialized["acceptor"], "acceptor[1].acceptor");
    assert_eq!(elaborated.matched_sites["donor[1]"]["donor"], "donor");
    assert_eq!(
        elaborated.matched_sites["acceptor[1]"]["acceptor"],
        "acceptor"
    );
    for required in [
        "premise.trait.donor.definition",
        "premise.trait.donor.assertion",
        "premise.pattern.dative-donor",
    ] {
        assert!(
            elaborated.role_premise_ids["donor"]
                .iter()
                .any(|premise| premise.to_string() == required)
        );
    }

    let mut wrong = inputs;
    wrong[0].structure = StructureId::from_str("DativeAcceptor").unwrap();
    let failure = catalogue
        .elaborate_generalized_rule(&rule, &wrong)
        .unwrap()
        .unwrap_err();
    assert_eq!(
        failure.class,
        GeneralizedElaborationFailureClass::Unsupported
    );
}

#[test]
fn unsupported_bindings_and_cases_stop_before_graph_matching() {
    let catalogue = catalogue();
    let water = ReactionRuleId::from_str("Rules.AlkaliMetalWithWater").unwrap();
    let outside = catalogue
        .elaborate_generalized_rule(
            &water,
            &water_inputs("LegacyLithiumMetal", "LithiumHydroxide"),
        )
        .unwrap()
        .unwrap_err();
    assert_eq!(
        outside.class,
        GeneralizedElaborationFailureClass::Unsupported
    );

    let oxygen = ReactionRuleId::from_str("Rules.AlkaliMetalWithOxygenDesign").unwrap();
    let selected_gap = catalogue
        .elaborate_generalized_rule(
            &oxygen,
            &[
                role_input(
                    "metal",
                    "PotassiumMetal",
                    1,
                    RuleSideRecord::Reactant,
                    RepresentationRecord::Metallic,
                ),
                role_input(
                    "oxygen",
                    "Oxygen",
                    1,
                    RuleSideRecord::Reactant,
                    RepresentationRecord::Molecular,
                ),
                role_input(
                    "oxide",
                    "LithiumHydroxide",
                    1,
                    RuleSideRecord::Product,
                    RepresentationRecord::Ionic,
                ),
            ],
        )
        .unwrap()
        .unwrap_err();
    assert_eq!(
        selected_gap.required_feature.as_deref(),
        Some("Features.SuperoxideBonding")
    );
}

#[test]
fn non_automorphic_graph_matches_are_ambiguous_not_first_match() {
    let mut value = with_g3(fixture());
    let premise = "premise.case.generalized-water";
    value["bundle"]["graph_patterns"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "id":"Patterns.AsymmetricUnconstrained",
            "variables":{"a":{"atom":{}},"b":{"atom":{}},"c":{"atom":{}}},
            "premise_ids":[premise]
        }));
    let proof_premises = value["bundle"]["generalized_rules"][0]["premise_ids"].clone();
    value["bundle"]["generalized_rules"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "id":"Rules.AsymmetricAmbiguityProbe",
            "parameters":{"mode":{"kind":"enum","values":["probe"]}},
            "roles":{
                "substrate":{"side":"reactant","representation":"ionic","coefficient":1},
                "product":{"side":"product","representation":"ionic","coefficient":1}
            },
            "reactants":{"substrate":{"kind":"exact","structure":"LithiumHydroxide"}},
            "cases":[{
                "id":"probe","status":"supported","when":{"kind":"always"},
                "products":{"product":{"kind":"exact","structure":"LithiumHydroxide"}},
                "patterns":{"substrate":"Patterns.AsymmetricUnconstrained"},
                "correspondence":[
                    {"reactant":"substrate[1].a","product":"product[1].hydroxide.hydrogen","premise_ids":[premise]},
                    {"reactant":"substrate[1].b","product":"product[1].hydroxide.oxygen","premise_ids":[premise]},
                    {"reactant":"substrate[1].c","product":"product[1].cation.metal","premise_ids":[premise]}
                ],
                "rewrite":[{"kind":"assign_product","atoms":["substrate[1].a","substrate[1].b","substrate[1].c"],"product":"product[1]","premise_ids":[premise]}],
                "premise_ids":[premise]
            }],
            "applicability":{"premise_id":premise,"request_relation":"contact","required_context":"ambiguity probe"},
            "model_assumptions":{"event":"representative","sequence":"explanatory","premise_ids":[premise]},
            "premise_ids":proof_premises
        }));
    let catalogue = ValidatedCatalogueBundle::validate(envelope(value)).unwrap();
    let failure = catalogue
        .elaborate_generalized_rule(
            &ReactionRuleId::from_str("Rules.AsymmetricAmbiguityProbe").unwrap(),
            &[
                role_input(
                    "substrate",
                    "LithiumHydroxide",
                    1,
                    RuleSideRecord::Reactant,
                    RepresentationRecord::Ionic,
                ),
                role_input(
                    "product",
                    "LithiumHydroxide",
                    1,
                    RuleSideRecord::Product,
                    RepresentationRecord::Ionic,
                ),
            ],
        )
        .unwrap()
        .unwrap_err();
    assert_eq!(failure.class, GeneralizedElaborationFailureClass::Ambiguous);
}

#[test]
fn generalized_fixture_matches_schema_and_is_order_canonical_digest_data() {
    let original = with_g3(fixture());
    let validator = jsonschema::draft202012::new(&schema()).unwrap();
    let errors = validator.iter_errors(&original).collect::<Vec<_>>();
    assert!(errors.is_empty(), "schema errors: {errors:?}");
    let original = envelope(original);
    let mut reordered: Value = serde_json::to_value(&original).unwrap();
    reordered["bundle"]["generalized_rules"]
        .as_array_mut()
        .unwrap()
        .reverse();
    reordered["bundle"]["generalized_rules"][2]["cases"]
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
    changed["bundle"]["generalized_rules"][0]["premise_ids"]
        .as_array_mut()
        .unwrap()
        .push(json!("premise.rule.dative-fixture"));
    assert_ne!(original.digest, envelope(changed).digest);

    let mut split = with_g3(fixture());
    let common = &mut split["bundle"]["generalized_rules"][0]["cases"][0];
    common["when"] = json!({"kind":"parameter_in_set","parameter":"M","values":["Li","Na"]});
    let mut potassium = common.clone();
    potassium["id"] = json!("potassium");
    potassium["when"] = json!({"kind":"parameter_equals","parameter":"M","value":"K"});
    split["bundle"]["generalized_rules"][0]["cases"]
        .as_array_mut()
        .unwrap()
        .push(potassium);
    let split = envelope(split);
    let mut reversed: Value = serde_json::to_value(&split).unwrap();
    reversed["bundle"]["generalized_rules"][0]["cases"]
        .as_array_mut()
        .unwrap()
        .reverse();
    let reversed = envelope(reversed);
    assert_eq!(split.digest, reversed.digest);
    assert_eq!(
        split.canonical_json().unwrap(),
        reversed.canonical_json().unwrap()
    );
}

fn assert_code(value: Value, expected: CatalogueErrorCode) {
    assert_eq!(
        ValidatedCatalogueBundle::validate(envelope(value))
            .unwrap_err()
            .code(),
        expected
    );
}

#[test]
fn overlapping_unreachable_and_out_of_domain_cases_are_rejected() {
    let mut overlap = with_g3(fixture());
    let duplicate = overlap["bundle"]["generalized_rules"][0]["cases"][0].clone();
    overlap["bundle"]["generalized_rules"][0]["cases"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    overlap["bundle"]["generalized_rules"][0]["cases"][1]["id"] = json!("overlap");
    assert_code(overlap, CatalogueErrorCode::InvalidGeneralizedCase);

    let mut unreachable = with_g3(fixture());
    let mut case = unreachable["bundle"]["generalized_rules"][1]["cases"][0].clone();
    case["id"] = json!("never");
    case["when"] = json!({"kind":"all","predicates":[
        {"kind":"parameter_equals","parameter":"M","value":"Li"},
        {"kind":"parameter_equals","parameter":"M","value":"Na"}
    ]});
    unreachable["bundle"]["generalized_rules"][1]["cases"]
        .as_array_mut()
        .unwrap()
        .push(case);
    assert_code(unreachable, CatalogueErrorCode::InvalidGeneralizedCase);

    let mut outside = with_g3(fixture());
    outside["bundle"]["generalized_rules"][1]["cases"][0]["when"]["value"] = json!("Ca");
    assert_code(outside, CatalogueErrorCode::InvalidGeneralizedCase);
}

#[test]
fn every_supported_reference_and_total_shape_is_checked() {
    let mut missing_pattern = with_g3(fixture());
    missing_pattern["bundle"]["generalized_rules"][0]["cases"][0]["patterns"]["water"] =
        json!("Patterns.Absent");
    assert_code(missing_pattern, CatalogueErrorCode::UnknownReference);

    let mut partial_mapping = with_g3(fixture());
    partial_mapping["bundle"]["generalized_rules"][0]["cases"][0]["correspondence"]
        .as_array_mut()
        .unwrap()
        .pop();
    assert_code(partial_mapping, CatalogueErrorCode::InvalidGeneralizedCase);

    let mut broken_rewrite = with_g3(fixture());
    broken_rewrite["bundle"]["generalized_rules"][2]["cases"][0]["rewrite"][0]["acceptor"] =
        json!("pair[1].absent");
    assert_code(broken_rewrite, CatalogueErrorCode::InvalidGeneralizedCase);

    let mut wrong_product = with_g3(fixture());
    wrong_product["bundle"]["generalized_rules"][0]["cases"][0]["products"]["hydroxide"]["arguments"]
        ["M"] = json!("Ca");
    assert_code(wrong_product, CatalogueErrorCode::InvalidGeneralizedCase);

    let mut missing_derived_premise = with_g3(fixture());
    missing_derived_premise["bundle"]["generalized_rules"][0]["premise_ids"]
        .as_array_mut()
        .unwrap()
        .retain(|premise| premise != "premise.element.na");
    assert_code(
        missing_derived_premise,
        CatalogueErrorCode::InvalidGeneralizedRule,
    );
}

#[test]
fn pattern_parameters_and_finite_validation_work_are_bounded() {
    let mut unknown = with_g3(fixture());
    unknown["bundle"]["graph_patterns"][0]["variables"]["metal"]["atom"]["element"] =
        json!({"parameter":"Absent"});
    assert_code(unknown, CatalogueErrorCode::InvalidGeneralizedCase);

    let mut wrong_kind = with_g3(fixture());
    wrong_kind["bundle"]["generalized_rules"][0]["parameters"]["X"] =
        json!({"kind":"enum","values":["x"]});
    wrong_kind["bundle"]["graph_patterns"][0]["variables"]["metal"]["atom"]["element"] =
        json!({"parameter":"X"});
    assert_code(wrong_kind, CatalogueErrorCode::InvalidGeneralizedCase);

    let mut excessive = with_g3(fixture());
    excessive["bundle"]["generalized_rules"][1]["parameters"]["X"] = json!({
        "kind":"enum",
        "values":(0..4097).map(|index| format!("v{index}")).collect::<Vec<_>>()
    });
    assert_code(excessive, CatalogueErrorCode::InvalidGeneralizedRule);

    let mut too_many_singletons = with_g3(fixture());
    for index in 0..64 {
        too_many_singletons["bundle"]["generalized_rules"][1]["parameters"][format!("X{index}")] =
            json!({"kind":"enum","values":["only"]});
    }
    assert_code(
        too_many_singletons,
        CatalogueErrorCode::InvalidGeneralizedRule,
    );
}

#[test]
fn binary_rewrites_require_distinct_endpoints_and_local_cleavage_targets() {
    let mut cleave = with_g3(fixture());
    cleave["bundle"]["generalized_rules"][0]["cases"][0]["rewrite"][1]["edge"][1] =
        cleave["bundle"]["generalized_rules"][0]["cases"][0]["rewrite"][1]["edge"][0].clone();
    assert_code(cleave, CatalogueErrorCode::InvalidGeneralizedCase);

    let mut form = with_g3(fixture());
    form["bundle"]["generalized_rules"][0]["cases"][0]["rewrite"][10]["edge"][1] =
        json!("water[1].proton");
    assert_code(form, CatalogueErrorCode::InvalidGeneralizedCase);

    let mut transfer = with_g3(fixture());
    transfer["bundle"]["generalized_rules"][0]["cases"][0]["rewrite"][2]["acceptor"] =
        json!("metal[1].metal");
    assert_code(transfer, CatalogueErrorCode::InvalidGeneralizedCase);

    let mut form_dative = with_g3(fixture());
    form_dative["bundle"]["generalized_rules"][2]["cases"][0]["rewrite"][0]["acceptor"] =
        json!("donor[1].donor");
    assert_code(form_dative, CatalogueErrorCode::InvalidGeneralizedCase);

    let mut cleave_dative = with_g3(fixture());
    cleave_dative["bundle"]["generalized_rules"][2]["cases"][0]["rewrite"][0] = json!({
        "kind":"cleave_dative",
        "donor":"donor[1].donor","acceptor":"donor[1].donor",
        "allocation":"homolytic",
        "before":{"left":[-1,6,0],"right":[0,0,0]},
        "after":{"left":[-2,7,1],"right":[1,1,1]},
        "premise_ids":["premise.case.dative-fixture"]
    });
    assert_code(cleave_dative, CatalogueErrorCode::InvalidGeneralizedCase);

    let mut change = with_g3(fixture());
    change["bundle"]["generalized_rules"][0]["cases"][0]["rewrite"][1] = json!({
        "kind":"change_covalent",
        "edge":["water[1].oxygen","water[1].oxygen"],
        "old_order":"single","new_order":"double","allocation":"homolytic",
        "before":{"left":[0,4,0],"right":[0,0,0]},
        "after":{"left":[0,3,1],"right":[0,1,1]},
        "premise_ids":["premise.case.generalized-water"]
    });
    assert_code(change, CatalogueErrorCode::InvalidGeneralizedCase);

    let mut unrelated_target = with_g3(fixture());
    unrelated_target["bundle"]["generalized_rules"][0]["cases"][0]["rewrite"][1]["allocation"]["heterolytic_to"] =
        json!("metal[1].metal");
    assert_code(unrelated_target, CatalogueErrorCode::InvalidGeneralizedCase);
}

#[test]
fn generalized_observations_match_closed_concrete_semantics() {
    let mut wrong_side = with_g3(fixture());
    wrong_side["bundle"]["generalized_rules"][0]["cases"][0]["observation_compatibility"][0]["subject_role"] =
        json!("metal");
    assert_code(wrong_side, CatalogueErrorCode::InvalidGeneralizedCase);

    let mut wrong_representation = with_g3(fixture());
    wrong_representation["bundle"]["generalized_rules"][0]["cases"][0]["observation_compatibility"]
        [0]["subject_role"] = json!("hydroxide");
    assert_code(
        wrong_representation,
        CatalogueErrorCode::InvalidGeneralizedCase,
    );

    let mut missing_colour = with_g3(fixture());
    missing_colour["bundle"]["generalized_rules"][0]["cases"][0]["observation_compatibility"][0]
        ["predicate"] = json!("colour");
    assert_code(missing_colour, CatalogueErrorCode::InvalidGeneralizedCase);

    let mut duplicate = with_g3(fixture());
    let fact = duplicate["bundle"]["generalized_rules"][0]["cases"][0]["observation_compatibility"]
        [0]
    .clone();
    duplicate["bundle"]["generalized_rules"][0]["cases"][0]["observation_compatibility"]
        .as_array_mut()
        .unwrap()
        .push(fact);
    assert_code(duplicate, CatalogueErrorCode::InvalidGeneralizedCase);
}

#[test]
fn trait_domains_rule_ids_and_model_assumptions_are_integrity_checked() {
    let mut wrong_trait = with_g3(fixture());
    wrong_trait["bundle"]["generalized_rules"][2]["parameters"]["D"]["trait"] =
        json!("Traits.EmptyAcceptorSite");
    assert_code(wrong_trait, CatalogueErrorCode::InvalidGeneralizedCase);

    let mut missing_trait_premise = with_g3(fixture());
    missing_trait_premise["bundle"]["generalized_rules"][2]["premise_ids"]
        .as_array_mut()
        .unwrap()
        .retain(|premise| premise != "premise.trait.donor.assertion");
    assert_code(
        missing_trait_premise,
        CatalogueErrorCode::InvalidGeneralizedRule,
    );

    let mut collision = with_g3(fixture());
    collision["bundle"]["rules"][0]["id"] = json!("Rules.AlkaliMetalWithWater");
    assert_code(collision, CatalogueErrorCode::DuplicateId);

    let mut empty_assumptions = with_g3(fixture());
    empty_assumptions["bundle"]["generalized_rules"][0]["model_assumptions"]["premise_ids"] =
        json!([]);
    let bytes = serde_json::to_vec(&envelope(empty_assumptions)).unwrap();
    assert_eq!(
        ValidatedCatalogueBundle::from_json(&bytes)
            .unwrap_err()
            .code(),
        CatalogueErrorCode::InvalidGeneralizedRule
    );
}

#[test]
fn unsupported_cases_cannot_carry_rewrite_payloads_or_unknown_fields() {
    let mut unsupported = with_g3(fixture());
    unsupported["bundle"]["generalized_rules"][1]["cases"][0]["rewrite"] = json!([]);
    assert!(serde_json::from_value::<CatalogueEnvelope>(unsupported).is_err());

    let mut unknown = with_g3(fixture());
    unknown["bundle"]["generalized_rules"][0]["runtime_selector"] = json!("first");
    assert!(serde_json::from_value::<CatalogueEnvelope>(unknown).is_err());
}

#[test]
fn omitted_and_empty_generalized_rules_preserve_legacy_semantics() {
    let omitted = envelope(fixture());
    let mut empty = fixture();
    empty["bundle"]["generalized_rules"] = json!([]);
    let empty = envelope(empty);
    assert_eq!(omitted.digest, empty.digest);
    assert_eq!(
        omitted.canonical_json().unwrap(),
        empty.canonical_json().unwrap()
    );
}
