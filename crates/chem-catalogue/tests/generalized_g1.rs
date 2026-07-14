use std::{fs, path::PathBuf, str::FromStr};

use chem_catalogue::{
    CatalogueEnvelope, CatalogueErrorCode, StructuralTraitId, ValidatedCatalogueBundle,
};
use chem_domain::{
    AtomGroupId, AtomId, BondOrder, CovalentElectronOrigin, IonicAssociationId, MetallicDomainId,
    RepresentationKind, StructureId,
};
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

fn replace_string(value: &mut Value, from: &str, to: &str) {
    match value {
        Value::String(text) if text == from => to.clone_into(text),
        Value::Array(values) => {
            for value in values {
                replace_string(value, from, to);
            }
        }
        Value::Object(values) => {
            for value in values.values_mut() {
                replace_string(value, from, to);
            }
        }
        _ => {}
    }
}

#[allow(clippy::too_many_lines)]
fn with_g1(mut value: Value) -> Value {
    replace_string(&mut value, "LithiumMetal", "LegacyLithiumMetal");
    replace_string(&mut value, "LithiumHydroxide", "LegacyLithiumHydroxide");

    for (id, statement) in [
        (
            "premise.element.hydrogen.identity",
            "Reviewed intrinsic identity fields for hydrogen.",
        ),
        (
            "premise.element.lithium.identity",
            "Reviewed intrinsic identity fields for lithium.",
        ),
        (
            "premise.element.sodium.identity",
            "Reviewed intrinsic identity fields for sodium.",
        ),
        (
            "premise.element.potassium.identity",
            "Reviewed intrinsic identity fields for potassium.",
        ),
        (
            "premise.element.oxygen.identity",
            "Reviewed intrinsic identity fields for oxygen.",
        ),
        (
            "premise.element.calcium.identity",
            "Reviewed intrinsic identity fields for calcium.",
        ),
        (
            "premise.category.alkali-metal",
            "Alkali metals are group 1 elements excluding hydrogen in this model.",
        ),
        (
            "premise.trait.elemental-metal.definition",
            "The elemental monovalent metal trait exposes an exact positive site and one-electron metallic domain.",
        ),
        (
            "premise.trait.protic-oh.definition",
            "The protic hydroxyl trait exposes an exact oxygen-hydrogen shared single edge.",
        ),
        (
            "premise.template.elemental-alkali-metal",
            "The elemental alkali-metal template has one positive site and one domain-owned electron.",
        ),
        (
            "premise.template.alkali-hydroxide",
            "The alkali-hydroxide template has an M+ component and a covalently bonded OH- component.",
        ),
        (
            "premise.assertion.elemental-alkali-metal",
            "The elemental template graph satisfies its reviewed metallic trait.",
        ),
        (
            "premise.assertion.alkali-hydroxide",
            "The hydroxide template graph satisfies its reviewed protic O-H trait.",
        ),
        (
            "premise.structure.lithium-metal.application",
            "LithiumMetal is the M=Li application of the reviewed elemental template.",
        ),
        (
            "premise.structure.sodium-metal.application",
            "SodiumMetal is the M=Na application of the reviewed elemental template.",
        ),
        (
            "premise.structure.potassium-metal.application",
            "PotassiumMetal is the M=K application of the reviewed elemental template.",
        ),
        (
            "premise.structure.lithium-hydroxide.application",
            "LithiumHydroxide is the M=Li application of the reviewed hydroxide template.",
        ),
        (
            "premise.structure.sodium-hydroxide.application",
            "SodiumHydroxide is the M=Na application of the reviewed hydroxide template.",
        ),
        (
            "premise.structure.potassium-hydroxide.application",
            "PotassiumHydroxide is the M=K application of the reviewed hydroxide template.",
        ),
        (
            "premise.valence.sodium-potassium.g1",
            "The listed sodium and potassium states are the closed G1 valence support for template applications.",
        ),
    ] {
        add_premise(&mut value, id, statement);
    }

    value["bundle"]["elements"] = json!([
        {"symbol":"H","name":"Hydrogen","atomic_number":1,"period":1,"group":1,"block":"s","premise_ids":["premise.element.hydrogen.identity"]},
        {"symbol":"Li","name":"Lithium","atomic_number":3,"period":2,"group":1,"block":"s","premise_ids":["premise.element.lithium.identity"]},
        {"symbol":"Na","name":"Sodium","atomic_number":11,"period":3,"group":1,"block":"s","premise_ids":["premise.element.sodium.identity"]},
        {"symbol":"K","name":"Potassium","atomic_number":19,"period":4,"group":1,"block":"s","premise_ids":["premise.element.potassium.identity"]},
        {"symbol":"O","name":"Oxygen","atomic_number":8,"period":2,"group":16,"block":"p","premise_ids":["premise.element.oxygen.identity"]},
        {"symbol":"Ca","name":"Calcium","atomic_number":20,"period":4,"group":2,"block":"s","premise_ids":["premise.element.calcium.identity"]}
    ]);
    value["bundle"]["element_categories"] = json!([{
        "id":"Categories.AlkaliMetal",
        "subject":"element",
        "membership":{"kind":"predicate","predicate":{"kind":"all","predicates":[
            {"kind":"equals","field":"group","value":1},
            {"kind":"not","predicate":{"kind":"equals","field":"symbol","value":"H"}}
        ]}},
        "premise_ids":["premise.category.alkali-metal"]
    }]);
    value["bundle"]["valence_premises"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "premise_id":"premise.valence.sodium-potassium.g1",
            "neutral_valence":[
                {"element":"Na","neutral_valence_electrons":1},
                {"element":"K","neutral_valence_electrons":1}
            ],
            "supported_states":[
                {"element":"Na","formal_charge":1,"non_bonding_electrons":0,"unpaired_electrons":0,"covalent_bond_order_sum":0},
                {"element":"K","formal_charge":1,"non_bonding_electrons":0,"unpaired_electrons":0,"covalent_bond_order_sum":0}
            ],
            "metallic_domain_states":[
                {"element":"Na","site_formal_charge":1,"site_local_electrons":0,"delocalized_electrons_per_site":1},
                {"element":"K","site_formal_charge":1,"site_local_electrons":0,"delocalized_electrons_per_site":1}
            ]
        }));

    value["bundle"]["structural_traits"] = json!([
        {
            "id":"Traits.ElementalMonovalentMetalDomain",
            "sites":{"metal":"atom","domain":"metallic_domain"},
            "values":{
                "formal_charge":{"kind":"atom_formal_charge","site":"metal"},
                "local_electrons":{"kind":"atom_non_bonding_electrons","site":"metal"},
                "unpaired_electrons":{"kind":"atom_unpaired_electrons","site":"metal"},
                "domain_electrons":{"kind":"metallic_delocalized_electrons","site":"domain"},
                "domain_sites":{"kind":"metallic_site_count","site":"domain"}
            },
            "premise_ids":["premise.trait.elemental-metal.definition"]
        },
        {
            "id":"Traits.ProticOH",
            "sites":{
                "oxygen":"atom",
                "hydrogen":"atom",
                "bond":"covalent_bond",
                "component":"group",
                "association":"ionic_association"
            },
            "values":{
                "oxygen_element":{"kind":"atom_element","site":"oxygen"},
                "oxygen_bond_order_sum":{"kind":"atom_bond_order_sum","site":"oxygen"},
                "bond_order":{"kind":"covalent_bond_order","left_site":"oxygen","right_site":"hydrogen"},
                "electron_origin":{"kind":"covalent_electron_origin","left_site":"oxygen","right_site":"hydrogen"},
                "oxygen_charge":{"kind":"atom_formal_charge","site":"oxygen"},
                "component_atoms":{"kind":"group_atom_count","site":"component"},
                "association_components":{"kind":"ionic_component_count","site":"association"}
            },
            "premise_ids":["premise.trait.protic-oh.definition"]
        }
    ]);
    value["bundle"]["structure_templates"] = json!([
        {
            "id":"Templates.ElementalAlkaliMetal",
            "parameters":{"M":{"kind":"element","category":"Categories.AlkaliMetal"}},
            "representation":"metallic",
            "sites":[{"label":"li","element":{"parameter":"M"},"formal_charge":1,"non_bonding_electrons":0,"unpaired_electrons":0}],
            "domains":[{"label":"metallic","sites":["li"],"delocalized_electrons":1}],
            "traits":[{
                "trait":"Traits.ElementalMonovalentMetalDomain",
                "sites":{"metal":"li","domain":"metallic"},
                "values":{"formal_charge":1,"local_electrons":0,"unpaired_electrons":0,"domain_electrons":1,"domain_sites":1},
                "premise_ids":["premise.assertion.elemental-alkali-metal"]
            }],
            "premise_ids":["premise.template.elemental-alkali-metal"]
        },
        {
            "id":"Templates.AlkaliHydroxide",
            "parameters":{
                "M":{"kind":"element","category":"Categories.AlkaliMetal"},
                "bond_order":{"kind":"enum","values":["single"]}
            },
            "representation":"ionic",
            "components":[
                {"label":"cation","atoms":[{"label":"metal","element":{"parameter":"M"},"formal_charge":1,"non_bonding_electrons":0,"unpaired_electrons":0}]},
                {"label":"hydroxide","atoms":[
                    {"label":"oxygen","element":"O","formal_charge":-1,"non_bonding_electrons":6,"unpaired_electrons":0},
                    {"label":"hydrogen","element":"H","formal_charge":0,"non_bonding_electrons":0,"unpaired_electrons":0}
                ],"bonds":[{"left":"oxygen","right":"hydrogen","order":{"parameter":"bond_order"}}]}
            ],
            "associations":[{"label":"salt","components":["cation","hydroxide"]}],
            "traits":[{
                "trait":"Traits.ProticOH",
                "sites":{
                    "oxygen":"hydroxide.oxygen",
                    "hydrogen":"hydroxide.hydrogen",
                    "bond":"bond.0",
                    "component":"hydroxide",
                    "association":"salt"
                },
                "values":{
                    "oxygen_element":"O",
                    "oxygen_bond_order_sum":1,
                    "bond_order":"single",
                    "electron_origin":"shared",
                    "oxygen_charge":-1,
                    "component_atoms":2,
                    "association_components":2
                },
                "premise_ids":["premise.assertion.alkali-hydroxide"]
            }],
            "premise_ids":["premise.template.alkali-hydroxide"]
        }
    ]);
    value["bundle"]["structure_applications"] = json!([
        {"id":"LithiumMetal","template":"Templates.ElementalAlkaliMetal","arguments":{"M":"Li"},"formula":"Li","aliases":["lithium"],"premise_ids":["premise.structure.lithium-metal.application"]},
        {"id":"SodiumMetal","template":"Templates.ElementalAlkaliMetal","arguments":{"M":"Na"},"formula":"Na","aliases":["sodium"],"premise_ids":["premise.structure.sodium-metal.application"]},
        {"id":"PotassiumMetal","template":"Templates.ElementalAlkaliMetal","arguments":{"M":"K"},"formula":"K","aliases":["potassium"],"premise_ids":["premise.structure.potassium-metal.application"]},
        {"id":"LithiumHydroxide","template":"Templates.AlkaliHydroxide","arguments":{"M":"Li","bond_order":"single"},"formula":"LiOH","aliases":["lithium-hydroxide"],"premise_ids":["premise.structure.lithium-hydroxide.application"]},
        {"id":"SodiumHydroxide","template":"Templates.AlkaliHydroxide","arguments":{"M":"Na","bond_order":"single"},"formula":"NaOH","aliases":["sodium-hydroxide"],"premise_ids":["premise.structure.sodium-hydroxide.application"]},
        {"id":"PotassiumHydroxide","template":"Templates.AlkaliHydroxide","arguments":{"M":"K","bond_order":"single"},"formula":"KOH","aliases":["potassium-hydroxide"],"premise_ids":["premise.structure.potassium-hydroxide.application"]}
    ]);
    value
}

fn valid_catalogue() -> ValidatedCatalogueBundle {
    ValidatedCatalogueBundle::validate(envelope(with_g1(fixture()))).unwrap()
}

fn with_structure_parameter_probe() -> Value {
    let mut value = with_g1(fixture());
    add_premise(
        &mut value,
        "premise.template.structure-constraint-probe",
        "The probe template validates a structure parameter against a checked trait.",
    );
    add_premise(
        &mut value,
        "premise.structure.structure-constraint-probe",
        "The probe application binds a trait-constrained structure argument.",
    );
    add_premise(
        &mut value,
        "premise.assertion.legacy-lithium-hydroxide",
        "The concrete legacy hydroxide graph satisfies the reviewed protic O-H trait.",
    );
    let concrete_hydroxide = &mut value["bundle"]["structures"][2];
    concrete_hydroxide["traits"] = json!([{
        "trait":"Traits.ProticOH",
        "sites":{
            "oxygen":"hydroxide.o",
            "hydrogen":"hydroxide.h",
            "bond":"bond.0",
            "component":"hydroxide",
            "association":"ionic"
        },
        "values":{
            "oxygen_element":"O",
            "oxygen_bond_order_sum":1,
            "bond_order":"single",
            "electron_origin":"shared",
            "oxygen_charge":-1,
            "component_atoms":2,
            "association_components":2
        },
        "premise_ids":["premise.assertion.legacy-lithium-hydroxide"]
    }]);
    value["bundle"]["structure_templates"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "id":"Templates.StructureConstraintProbe",
            "parameters":{
                "M":{"kind":"element","category":"Categories.AlkaliMetal"},
                "S":{"kind":"structure","traits":["Traits.ProticOH"]}
            },
            "representation":"metallic",
            "sites":[{"label":"site","element":{"parameter":"M"},"formal_charge":1,"non_bonding_electrons":0,"unpaired_electrons":0}],
            "domains":[{"label":"domain","sites":["site"],"delocalized_electrons":1}],
            "traits":[{
                "trait":"Traits.ElementalMonovalentMetalDomain",
                "sites":{"metal":"site","domain":"domain"},
                "values":{"formal_charge":1,"local_electrons":0,"unpaired_electrons":0,"domain_electrons":1,"domain_sites":1},
                "premise_ids":["premise.assertion.elemental-alkali-metal"]
            }],
            "premise_ids":["premise.template.structure-constraint-probe"]
        }));
    value["bundle"]["structure_applications"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "id":"ConstrainedLithiumMetal",
            "template":"Templates.StructureConstraintProbe",
            "arguments":{"M":"Li","S":"LegacyLithiumHydroxide"},
            "formula":"Li",
            "premise_ids":["premise.structure.structure-constraint-probe"]
        }));
    value
}

fn with_dative_trait_probe() -> Value {
    let mut value = with_g1(fixture());
    for (id, statement) in [
        (
            "premise.trait.dative-probe.definition",
            "The dative probe exposes its exact donor-to-acceptor edge.",
        ),
        (
            "premise.trait.dative-probe.assertion",
            "The probe graph satisfies the directed dative trait.",
        ),
        (
            "premise.structure.dative-probe",
            "The dative probe is a reviewed structural test fixture.",
        ),
    ] {
        add_premise(&mut value, id, statement);
    }
    value["bundle"]["structural_traits"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "id":"Traits.DativeProbe",
            "sites":{"donor":"atom","acceptor":"atom","bond":"covalent_bond"},
            "values":{
                "donor_element":{"kind":"atom_element","site":"donor"},
                "donor_bond_order_sum":{"kind":"atom_bond_order_sum","site":"donor"},
                "bond_order":{"kind":"covalent_bond_order","left_site":"donor","right_site":"acceptor"},
                "electron_origin":{"kind":"covalent_electron_origin","left_site":"donor","right_site":"acceptor"}
            },
            "premise_ids":["premise.trait.dative-probe.definition"]
        }));
    value["bundle"]["structures"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "id":"DativeProbe",
            "premise_id":"premise.structure.dative-probe",
            "formula":"HO",
            "representation":"ion",
            "atoms":[
                {"label":"o","element":"O","formal_charge":-1,"non_bonding_electrons":6,"unpaired_electrons":0},
                {"label":"h","element":"H","formal_charge":0,"non_bonding_electrons":0,"unpaired_electrons":0}
            ],
            "bonds":[{
                "left":"o",
                "right":"h",
                "order":"single",
                "electron_origin":{"kind":"dative","donor":"o","acceptor":"h"}
            }],
            "traits":[{
                "trait":"Traits.DativeProbe",
                "sites":{"donor":"o","acceptor":"h","bond":"bond.0"},
                "values":{
                    "donor_element":"O",
                    "donor_bond_order_sum":1,
                    "bond_order":"single",
                    "electron_origin":"dative_left_to_right"
                },
                "premise_ids":["premise.trait.dative-probe.assertion"]
            }]
        }));
    value
}

fn with_two_hop_structure_parameter_probe() -> Value {
    let mut value = with_structure_parameter_probe();
    add_premise(
        &mut value,
        "premise.template.outer-constraint-probe",
        "The outer probe constrains an already constrained template application.",
    );
    add_premise(
        &mut value,
        "premise.structure.outer-constraint-probe",
        "The outer probe retains the complete transitive structure dependency chain.",
    );
    value["bundle"]["structure_templates"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "id":"Templates.OuterConstraintProbe",
            "parameters":{
                "M":{"kind":"element","category":"Categories.AlkaliMetal"},
                "S":{"kind":"structure","traits":["Traits.ElementalMonovalentMetalDomain"]}
            },
            "representation":"metallic",
            "sites":[{"label":"site","element":{"parameter":"M"},"formal_charge":1,"non_bonding_electrons":0,"unpaired_electrons":0}],
            "domains":[{"label":"domain","sites":["site"],"delocalized_electrons":1}],
            "premise_ids":["premise.template.outer-constraint-probe"]
        }));
    value["bundle"]["structure_applications"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "id":"OuterConstrainedLithiumMetal",
            "template":"Templates.OuterConstraintProbe",
            "arguments":{"M":"Li","S":"ConstrainedLithiumMetal"},
            "formula":"Li",
            "premise_ids":["premise.structure.outer-constraint-probe"]
        }));
    value
}

#[test]
fn templates_instantiate_exact_alkali_metals_and_hydroxides() {
    let catalogue = valid_catalogue();
    for (metal, hydroxide, element) in [
        ("LithiumMetal", "LithiumHydroxide", "Li"),
        ("SodiumMetal", "SodiumHydroxide", "Na"),
        ("PotassiumMetal", "PotassiumHydroxide", "K"),
    ] {
        let metal = catalogue
            .structure(&StructureId::from_str(metal).unwrap())
            .unwrap();
        assert_eq!(metal.representation(), RepresentationKind::Metallic);
        assert_eq!(metal.graph().atoms().len(), 1);
        let metal_atom_id = AtomId::from_str("li").unwrap();
        let metal_atom = &metal.graph().atoms()[&metal_atom_id];
        assert_eq!(metal_atom.element().as_str(), element);
        assert_eq!(metal_atom.electrons().formal_charge(), 1);
        assert_eq!(metal_atom.electrons().non_bonding_electrons(), 0);
        assert_eq!(metal_atom.electrons().unpaired_electrons(), 0);
        assert_eq!(metal.graph().metallic_domains().len(), 1);
        let domain =
            &metal.graph().metallic_domains()[&MetallicDomainId::from_str("metallic").unwrap()];
        assert_eq!(domain.sites().iter().collect::<Vec<_>>(), [&metal_atom_id]);
        assert_eq!(domain.delocalized_electrons(), 1);
        assert!(metal.graph().covalent_bonds().is_empty());
        assert!(metal.graph().groups().is_empty());
        assert!(metal.graph().ionic_associations().is_empty());

        let hydroxide = catalogue
            .structure(&StructureId::from_str(hydroxide).unwrap())
            .unwrap();
        assert_eq!(hydroxide.representation(), RepresentationKind::Ionic);
        assert_eq!(hydroxide.graph().atoms().len(), 3);
        assert_eq!(hydroxide.graph().covalent_bonds().len(), 1);
        assert_eq!(hydroxide.graph().ionic_associations().len(), 1);
        assert_eq!(hydroxide.graph().groups().len(), 2);

        let cation_id = AtomId::from_str("cation.metal").unwrap();
        let oxygen_id = AtomId::from_str("hydroxide.oxygen").unwrap();
        let hydrogen_id = AtomId::from_str("hydroxide.hydrogen").unwrap();
        let cation = &hydroxide.graph().atoms()[&cation_id];
        let oxygen = &hydroxide.graph().atoms()[&oxygen_id];
        let hydrogen = &hydroxide.graph().atoms()[&hydrogen_id];
        assert_eq!(cation.element().as_str(), element);
        assert_eq!(cation.electrons().formal_charge(), 1);
        assert_eq!(cation.electrons().non_bonding_electrons(), 0);
        assert_eq!(cation.electrons().unpaired_electrons(), 0);
        assert_eq!(oxygen.element().as_str(), "O");
        assert_eq!(oxygen.electrons().formal_charge(), -1);
        assert_eq!(oxygen.electrons().non_bonding_electrons(), 6);
        assert_eq!(oxygen.electrons().unpaired_electrons(), 0);
        assert_eq!(hydrogen.element().as_str(), "H");
        assert_eq!(hydrogen.electrons().formal_charge(), 0);
        assert_eq!(hydrogen.electrons().non_bonding_electrons(), 0);
        assert_eq!(hydrogen.electrons().unpaired_electrons(), 0);

        let bond = hydroxide.graph().covalent_bonds().values().next().unwrap();
        assert_eq!(bond.order(), BondOrder::Single);
        assert_eq!(bond.electron_origin(), &CovalentElectronOrigin::Shared);
        assert_eq!(
            [bond.left(), bond.right()]
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>(),
            [&oxygen_id, &hydrogen_id].into_iter().collect()
        );
        let cation_group_id = AtomGroupId::from_str("cation").unwrap();
        let hydroxide_group_id = AtomGroupId::from_str("hydroxide").unwrap();
        let cation_group = &hydroxide.graph().groups()[&cation_group_id];
        assert_eq!(
            cation_group.atoms().iter().collect::<Vec<_>>(),
            [&cation_id]
        );
        let hydroxide_group = &hydroxide.graph().groups()[&hydroxide_group_id];
        assert_eq!(
            hydroxide_group.atoms().iter().collect::<Vec<_>>(),
            [&hydrogen_id, &oxygen_id]
        );
        let association =
            &hydroxide.graph().ionic_associations()[&IonicAssociationId::from_str("salt").unwrap()];
        assert_eq!(
            association.components().iter().collect::<Vec<_>>(),
            [&cation_group_id, &hydroxide_group_id]
        );
        assert!(hydroxide.graph().metallic_domains().is_empty());
    }

    let legacy = catalogue
        .structure(&StructureId::from_str("LegacyLithiumMetal").unwrap())
        .unwrap();
    let application = catalogue
        .structure(&StructureId::from_str("LithiumMetal").unwrap())
        .unwrap();
    assert_eq!(legacy.graph(), application.graph());
    assert_eq!(legacy.formula(), application.formula());
    assert_ne!(legacy.id(), application.id());
    assert_eq!(
        catalogue
            .structure_by_alias("sodium")
            .unwrap()
            .id()
            .to_string(),
        "SodiumMetal"
    );
}

#[test]
fn traits_and_application_provenance_are_exact_and_separate() {
    let catalogue = valid_catalogue();
    let sodium = StructureId::from_str("SodiumMetal").unwrap();
    let metallic_trait =
        StructuralTraitId::from_str("Traits.ElementalMonovalentMetalDomain").unwrap();
    assert!(
        catalogue
            .structure_trait_assertion(&sodium, &metallic_trait)
            .is_some()
    );
    let provenance = catalogue.structure_application_provenance(&sodium).unwrap();
    assert_eq!(
        provenance
            .template_premise_ids
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        ["premise.template.elemental-alkali-metal"]
    );
    assert_eq!(
        provenance
            .argument_element_premise_ids
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        ["premise.element.sodium.identity"]
    );
    assert_eq!(
        provenance
            .argument_category_premise_ids
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        ["premise.category.alkali-metal"]
    );
    assert_eq!(
        provenance
            .trait_definition_premise_ids
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        ["premise.trait.elemental-metal.definition"]
    );
    assert_eq!(
        provenance
            .trait_assertion_premise_ids
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        ["premise.assertion.elemental-alkali-metal"]
    );
    assert_eq!(
        provenance
            .application_premise_ids
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        ["premise.structure.sodium-metal.application"]
    );
}

#[test]
fn every_trait_site_and_projection_kind_is_checked() {
    let catalogue =
        ValidatedCatalogueBundle::validate(envelope(with_dative_trait_probe())).unwrap();
    let probe = StructureId::from_str("DativeProbe").unwrap();
    let trait_id = StructuralTraitId::from_str("Traits.DativeProbe").unwrap();
    assert!(
        catalogue
            .structure_trait_assertion(&probe, &trait_id)
            .is_some()
    );

    let mut false_projection = with_dative_trait_probe();
    false_projection["bundle"]["structures"]
        .as_array_mut()
        .unwrap()
        .last_mut()
        .unwrap()["traits"][0]["values"]["electron_origin"] = json!("dative_right_to_left");
    assert_code(false_projection, CatalogueErrorCode::InvalidStructuralTrait);

    let mut wrong_scalar_type = with_g1(fixture());
    wrong_scalar_type["bundle"]["structure_templates"][1]["traits"][0]["values"]["oxygen_element"] =
        json!(8);
    assert_code(
        wrong_scalar_type,
        CatalogueErrorCode::InvalidStructuralTrait,
    );

    let mut wrong_site_kind = with_g1(fixture());
    wrong_site_kind["bundle"]["structure_templates"][1]["traits"][0]["sites"]["component"] =
        json!("hydroxide.oxygen");
    assert_code(wrong_site_kind, CatalogueErrorCode::InvalidStructuralTrait);
}

#[test]
fn structure_parameters_require_checked_traits_and_retain_dependencies() {
    let catalogue =
        ValidatedCatalogueBundle::validate(envelope(with_structure_parameter_probe())).unwrap();
    let id = StructureId::from_str("ConstrainedLithiumMetal").unwrap();
    let provenance = catalogue.structure_application_provenance(&id).unwrap();
    assert!(
        provenance
            .argument_structure_premise_ids
            .iter()
            .any(|premise| premise.to_string() == "premise.structure.lithium-hydroxide")
    );
    for expected in [
        "premise.trait.protic-oh.definition",
        "premise.assertion.legacy-lithium-hydroxide",
    ] {
        assert!(
            provenance
                .argument_structure_premise_ids
                .iter()
                .any(|premise| premise.to_string() == expected)
        );
        assert!(
            catalogue
                .structure_premises(&id)
                .unwrap()
                .iter()
                .any(|premise| premise.to_string() == expected)
        );
    }

    let mut missing_trait = with_structure_parameter_probe();
    let application = missing_trait["bundle"]["structure_applications"]
        .as_array_mut()
        .unwrap()
        .last_mut()
        .unwrap();
    application["arguments"]["S"] = json!("Water");
    assert_code(
        missing_trait,
        CatalogueErrorCode::InvalidStructureApplication,
    );
}

#[test]
fn structure_parameter_provenance_is_transitive_and_rule_proof_bound() {
    let catalogue =
        ValidatedCatalogueBundle::validate(envelope(with_two_hop_structure_parameter_probe()))
            .unwrap();
    let outer = StructureId::from_str("OuterConstrainedLithiumMetal").unwrap();
    let provenance = catalogue.structure_application_provenance(&outer).unwrap();
    for expected in [
        "premise.structure.lithium-hydroxide",
        "premise.trait.protic-oh.definition",
        "premise.assertion.legacy-lithium-hydroxide",
        "premise.structure.structure-constraint-probe",
    ] {
        assert!(
            provenance
                .argument_structure_premise_ids
                .iter()
                .any(|premise| premise.to_string() == expected),
            "missing transitive premise {expected}"
        );
    }

    let mut missing_trait_proof = with_structure_parameter_probe();
    missing_trait_proof["bundle"]["rules"][0]["reactant_pattern"][0]["structure_id"] =
        json!("ConstrainedLithiumMetal");
    for premise in [
        "premise.template.structure-constraint-probe",
        "premise.structure.structure-constraint-probe",
        "premise.element.lithium.identity",
        "premise.category.alkali-metal",
        "premise.trait.elemental-metal.definition",
        "premise.assertion.elemental-alkali-metal",
        "premise.trait.protic-oh.definition",
    ] {
        missing_trait_proof["bundle"]["rules"][0]["premise_ids"]
            .as_array_mut()
            .unwrap()
            .push(json!(premise));
    }
    assert_code(missing_trait_proof, CatalogueErrorCode::InvalidRule);
}

#[test]
fn generalized_g1_fixture_matches_the_public_schema() {
    let schema = schema();
    let validator = jsonschema::draft202012::new(&schema).unwrap();
    let value = with_g1(fixture());
    let errors = validator.iter_errors(&value).collect::<Vec<_>>();
    assert!(errors.is_empty(), "schema errors: {errors:?}");
}

#[test]
fn g1_semantics_are_order_canonical_and_digest_complete() {
    let original = with_g1(fixture());
    let original_envelope = envelope(original.clone());
    let mut reordered = original.clone();
    reordered["bundle"]["structural_traits"]
        .as_array_mut()
        .unwrap()
        .reverse();
    reordered["bundle"]["structure_templates"][1]["components"]
        .as_array_mut()
        .unwrap()
        .reverse();
    if let Some(records) =
        reordered["bundle"]["structure_templates"][1]["components"][0]["bonds"].as_array_mut()
    {
        records.reverse();
    }
    reordered["bundle"]["structure_templates"][1]["associations"][0]["components"]
        .as_array_mut()
        .unwrap()
        .reverse();
    reordered["bundle"]["structure_templates"]
        .as_array_mut()
        .unwrap()
        .reverse();
    reordered["bundle"]["structure_applications"]
        .as_array_mut()
        .unwrap()
        .reverse();
    let reordered = envelope(reordered);
    assert_eq!(reordered.digest, original_envelope.digest);
    assert_eq!(
        reordered.canonical_json().unwrap(),
        original_envelope.canonical_json().unwrap()
    );

    for changed in [
        {
            let mut value = original.clone();
            value["bundle"]["structural_traits"][0]["values"]["formal_charge"]["site"] =
                json!("domain");
            value
        },
        {
            let mut value = original.clone();
            value["bundle"]["structure_templates"][0]["sites"][0]["formal_charge"] = json!(2);
            value
        },
        {
            let mut value = original.clone();
            value["bundle"]["structure_templates"][0]["premise_ids"] = json!([
                "premise.template.elemental-alkali-metal",
                "premise.template.alkali-hydroxide"
            ]);
            value
        },
        {
            let mut value = original.clone();
            value["bundle"]["structure_applications"][1]["arguments"]["M"] = json!("K");
            value
        },
        {
            let mut value = original.clone();
            value["bundle"]["structure_applications"][1]["aliases"] = json!(["sodium", "natrium"]);
            value
        },
        {
            let mut value = original.clone();
            value["bundle"]["structure_applications"][1]["premise_ids"] = json!([
                "premise.structure.sodium-metal.application",
                "premise.structure.potassium-metal.application"
            ]);
            value
        },
    ] {
        assert_ne!(envelope(changed).digest, original_envelope.digest);
    }
}

#[test]
fn omitted_and_empty_g1_arrays_have_identical_legacy_semantics() {
    let omitted = envelope(fixture());
    let mut empty = fixture();
    empty["bundle"]["structural_traits"] = json!([]);
    empty["bundle"]["structure_templates"] = json!([]);
    empty["bundle"]["structure_applications"] = json!([]);
    let empty = envelope(empty);
    assert_eq!(omitted.digest, empty.digest);
    assert_eq!(
        omitted.canonical_json().unwrap(),
        empty.canonical_json().unwrap()
    );
}

fn assert_code(value: Value, code: CatalogueErrorCode) {
    assert_eq!(
        ValidatedCatalogueBundle::validate(envelope(value))
            .unwrap_err()
            .code(),
        code
    );
}

#[test]
fn application_arguments_formulae_and_aliases_are_closed() {
    let mut missing = with_g1(fixture());
    missing["bundle"]["structure_applications"][1]["arguments"] = json!({});
    assert_code(missing, CatalogueErrorCode::InvalidStructureApplication);

    let mut unknown = with_g1(fixture());
    unknown["bundle"]["structure_applications"][1]["arguments"]["extra"] = json!("Na");
    assert_code(unknown, CatalogueErrorCode::InvalidStructureApplication);

    let mut wrong_category = with_g1(fixture());
    wrong_category["bundle"]["structure_applications"][1]["arguments"]["M"] = json!("Ca");
    wrong_category["bundle"]["structure_applications"][1]["formula"] = json!("Ca");
    assert_code(
        wrong_category,
        CatalogueErrorCode::InvalidStructureApplication,
    );

    let mut wrong_formula = with_g1(fixture());
    wrong_formula["bundle"]["structure_applications"][1]["formula"] = json!("K");
    assert_code(
        wrong_formula,
        CatalogueErrorCode::InvalidStructureApplication,
    );

    let mut colliding_alias = with_g1(fixture());
    colliding_alias["bundle"]["structure_applications"][1]["aliases"] = json!(["Water"]);
    assert_code(
        colliding_alias,
        CatalogueErrorCode::InvalidStructureApplication,
    );

    let mut wrong_enum = with_g1(fixture());
    wrong_enum["bundle"]["structure_applications"][4]["arguments"]["bond_order"] = json!("double");
    assert_code(wrong_enum, CatalogueErrorCode::InvalidStructureApplication);
}

#[test]
fn malformed_template_graphs_and_traits_are_rejected() {
    let mut self_bond = with_g1(fixture());
    self_bond["bundle"]["structure_templates"][1]["components"][1]["bonds"][0]["right"] =
        json!("oxygen");
    assert_code(self_bond, CatalogueErrorCode::InvalidStructureTemplate);

    let mut invalid_dative = with_g1(fixture());
    invalid_dative["bundle"]["structure_templates"][1]["components"][1]["bonds"][0]["electron_origin"] =
        json!({"kind":"dative","donor":"oxygen","acceptor":"absent"});
    assert_code(invalid_dative, CatalogueErrorCode::InvalidStructureTemplate);

    let mut non_single_dative_enum = with_g1(fixture());
    non_single_dative_enum["bundle"]["structure_templates"][1]["parameters"]["bond_order"]["values"] =
        json!(["single", "double"]);
    non_single_dative_enum["bundle"]["structure_templates"][1]["components"][1]["bonds"][0]["electron_origin"] =
        json!({"kind":"dative","donor":"oxygen","acceptor":"hydrogen"});
    assert_code(
        non_single_dative_enum,
        CatalogueErrorCode::InvalidStructureTemplate,
    );

    let mut broken_trait_site = with_g1(fixture());
    broken_trait_site["bundle"]["structure_templates"][0]["traits"][0]["sites"]["metal"] =
        json!("absent");
    assert_code(
        broken_trait_site,
        CatalogueErrorCode::InvalidStructuralTrait,
    );

    let mut false_trait_value = with_g1(fixture());
    false_trait_value["bundle"]["structure_templates"][0]["traits"][0]["values"]["domain_electrons"] =
        json!(2);
    assert_code(
        false_trait_value,
        CatalogueErrorCode::InvalidStructuralTrait,
    );
}

#[test]
fn structure_parameter_cycles_are_rejected_deterministically() {
    let mut cycle = with_g1(fixture());
    cycle["bundle"]["structure_templates"][1]["parameters"]["S"] =
        json!({"kind":"structure","traits":["Traits.ProticOH"]});
    cycle["bundle"]["structure_applications"][3]["arguments"]["S"] = json!("SodiumHydroxide");
    cycle["bundle"]["structure_applications"][4]["arguments"]["S"] = json!("LithiumHydroxide");
    cycle["bundle"]["structure_applications"][5]["arguments"]["S"] = json!("LithiumHydroxide");
    assert_code(cycle, CatalogueErrorCode::InvalidStructureApplication);
}

#[test]
fn unsupported_conditional_and_unknown_wire_fields_are_decode_errors() {
    for (target, field) in [("template", "when"), ("application", "conditional_atoms")] {
        let mut invalid = with_g1(fixture());
        let record = if target == "template" {
            &mut invalid["bundle"]["structure_templates"][0]
        } else {
            &mut invalid["bundle"]["structure_applications"][0]
        };
        record[field] = json!(true);
        assert!(serde_json::from_value::<CatalogueEnvelope>(invalid).is_err());
    }
}
