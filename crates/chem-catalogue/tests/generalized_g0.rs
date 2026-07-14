use std::{fs, path::PathBuf, str::FromStr};

use chem_catalogue::{
    CatalogueEnvelope, CatalogueErrorCode, ElementCategoryId, ElementCategoryMembershipRecord,
    ElementFieldRecord, ElementPredicateRecord, ElementScalarRecord, ValidatedCatalogueBundle,
};
use chem_domain::ElementSymbol;
use serde_json::{Value, json};

fn fixture() -> Value {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    serde_json::from_slice(
        &fs::read(root.join("conformance/catalogue/lithium-rule-001.catalogue.json")).unwrap(),
    )
    .unwrap()
}

fn envelope(value: Value) -> CatalogueEnvelope {
    let mut envelope: CatalogueEnvelope = serde_json::from_value(value).unwrap();
    envelope.digest = envelope.computed_digest().unwrap();
    envelope
}

fn schema() -> Value {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    serde_json::from_slice(&fs::read(root.join("schemas/chem-catalogue-1.schema.json")).unwrap())
        .unwrap()
}

fn registry() -> Value {
    json!([
        {"symbol":"H","name":"Hydrogen","atomic_number":1,"period":1,"group":1,"block":"s","premise_ids":["premise.element.hydrogen.identity"]},
        {"symbol":"Li","name":"Lithium","atomic_number":3,"period":2,"group":1,"block":"s","premise_ids":["premise.element.lithium.identity"]},
        {"symbol":"Na","name":"Sodium","atomic_number":11,"period":3,"group":1,"block":"s","premise_ids":["premise.element.sodium.identity"]},
        {"symbol":"K","name":"Potassium","atomic_number":19,"period":4,"group":1,"block":"s","premise_ids":["premise.element.potassium.identity"]},
        {"symbol":"Ca","name":"Calcium","atomic_number":20,"period":4,"group":2,"block":"s","premise_ids":["premise.element.calcium.identity"]},
        {"symbol":"B","name":"Boron","atomic_number":5,"period":2,"group":13,"block":"p","premise_ids":["premise.element.boron.identity"]},
        {"symbol":"Si","name":"Silicon","atomic_number":14,"period":3,"group":14,"block":"p","premise_ids":["premise.element.silicon.identity"]},
        {"symbol":"Fe","name":"Iron","atomic_number":26,"period":4,"group":8,"block":"d","premise_ids":["premise.element.iron.identity"]}
    ])
}

fn with_registry(mut value: Value) -> Value {
    for (id, statement) in [
        (
            "premise.element.hydrogen.identity",
            "Hydrogen has the reviewed symbol H, atomic number 1, name, period, group, and block used by the G0 registry.",
        ),
        (
            "premise.element.lithium.identity",
            "Lithium has the reviewed symbol Li, atomic number 3, name, period, group, and block used by the G0 registry.",
        ),
        (
            "premise.element.sodium.identity",
            "Sodium has the reviewed symbol Na, atomic number 11, name, period, group, and block used by the G0 registry.",
        ),
        (
            "premise.element.potassium.identity",
            "Potassium has the reviewed symbol K, atomic number 19, name, period, group, and block used by the G0 registry.",
        ),
        (
            "premise.element.calcium.identity",
            "Calcium has the reviewed symbol Ca, atomic number 20, name, period, group, and block used by the G0 registry.",
        ),
        (
            "premise.element.boron.identity",
            "Boron has the reviewed symbol B, atomic number 5, name, period, group, and block used by the G0 registry.",
        ),
        (
            "premise.element.silicon.identity",
            "Silicon has the reviewed symbol Si, atomic number 14, name, period, group, and block used by the G0 registry.",
        ),
        (
            "premise.element.iron.identity",
            "Iron has the reviewed symbol Fe, atomic number 26, name, period, group, and block used by the G0 registry.",
        ),
        (
            "premise.category.alkali-metal",
            "The reviewed alkali-metal category is group 1 excluding hydrogen for this catalogue model.",
        ),
        (
            "premise.category.metalloid",
            "The reviewed conventional metalloid category contains boron and silicon in the G0 test registry.",
        ),
    ] {
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
    value["bundle"]["elements"] = registry();
    value
}

fn with_generalized(value: Value) -> Value {
    let mut value = with_registry(value);
    value["bundle"]["element_categories"] = json!([
        {"id":"Categories.AlkaliMetal","subject":"element","membership":{"kind":"predicate","predicate":{"kind":"all","predicates":[{"kind":"equals","field":"group","value":1},{"kind":"not","predicate":{"kind":"equals","field":"symbol","value":"H"}}]}},"premise_ids":["premise.category.alkali-metal"]},
        {"id":"Categories.Metalloid","subject":"element","membership":{"kind":"explicit","members":["B","Si"]},"premise_ids":["premise.category.metalloid"]}
    ]);
    value
}

fn valid_catalogue() -> ValidatedCatalogueBundle {
    ValidatedCatalogueBundle::validate(envelope(with_generalized(fixture()))).unwrap()
}

#[test]
fn legacy_fixture_and_registry_are_compatible() {
    let legacy: CatalogueEnvelope = serde_json::from_value(fixture()).unwrap();
    assert_eq!(legacy.computed_digest().unwrap(), legacy.digest);
    assert!(ValidatedCatalogueBundle::validate(legacy).is_ok());

    let catalogue = valid_catalogue();
    let alkali = ElementCategoryId::from_str("Categories.AlkaliMetal").unwrap();
    let metalloid = ElementCategoryId::from_str("Categories.Metalloid").unwrap();
    assert_eq!(
        catalogue
            .element_category_members(&alkali)
            .unwrap()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        ["K", "Li", "Na"]
    );
    assert_eq!(
        catalogue
            .element_category_members(&metalloid)
            .unwrap()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        ["B", "Si"]
    );
    assert_eq!(
        catalogue.element_is_member(&ElementSymbol::new("Ca").unwrap(), &alkali),
        Some(false)
    );
    assert_eq!(
        catalogue.element_is_member(&ElementSymbol::new("O").unwrap(), &alkali),
        None
    );
    assert_eq!(
        catalogue.element_is_member(
            &ElementSymbol::new("Li").unwrap(),
            &ElementCategoryId::from_str("Categories.Absent").unwrap()
        ),
        None
    );
}

#[test]
fn generalized_fixture_satisfies_the_public_schema() {
    let schema = schema();
    let validator = jsonschema::draft202012::new(&schema).unwrap();
    let value = with_generalized(fixture());
    let errors = validator.iter_errors(&value).collect::<Vec<_>>();
    assert!(errors.is_empty(), "schema errors: {errors:?}");
}

#[test]
fn membership_exposes_separate_premise_dependencies() {
    let catalogue = valid_catalogue();
    for (category, symbol, element_premise, category_premise) in [
        (
            "Categories.AlkaliMetal",
            "Li",
            "premise.element.lithium.identity",
            "premise.category.alkali-metal",
        ),
        (
            "Categories.AlkaliMetal",
            "Na",
            "premise.element.sodium.identity",
            "premise.category.alkali-metal",
        ),
        (
            "Categories.AlkaliMetal",
            "K",
            "premise.element.potassium.identity",
            "premise.category.alkali-metal",
        ),
        (
            "Categories.Metalloid",
            "B",
            "premise.element.boron.identity",
            "premise.category.metalloid",
        ),
        (
            "Categories.Metalloid",
            "Si",
            "premise.element.silicon.identity",
            "premise.category.metalloid",
        ),
    ] {
        let category = ElementCategoryId::from_str(category).unwrap();
        let provenance = catalogue
            .element_membership_provenance(&ElementSymbol::new(symbol).unwrap(), &category)
            .unwrap();
        assert_eq!(
            provenance
                .element_premise_ids
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            [element_premise]
        );
        assert_eq!(
            provenance
                .category_premise_ids
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            [category_premise]
        );
    }
}

#[test]
fn semantic_order_is_canonical_and_mutations_change_digest() {
    let mut original = with_generalized(fixture());
    original["bundle"]["element_categories"][0]["membership"]["include"] = json!(["Ca", "Fe"]);
    original["bundle"]["element_categories"][0]["membership"]["exclude"] = json!(["B", "Si"]);
    let digest = envelope(original.clone()).digest;
    let mut reordered = original.clone();
    reordered["bundle"]["elements"]
        .as_array_mut()
        .unwrap()
        .reverse();
    reordered["bundle"]["element_categories"][0]["membership"]["include"]
        .as_array_mut()
        .unwrap()
        .reverse();
    reordered["bundle"]["element_categories"][0]["membership"]["exclude"]
        .as_array_mut()
        .unwrap()
        .reverse();
    reordered["bundle"]["element_categories"][0]["membership"]["predicate"]["predicates"]
        .as_array_mut()
        .unwrap()
        .reverse();
    reordered["bundle"]["element_categories"][1]["membership"]["members"]
        .as_array_mut()
        .unwrap()
        .reverse();
    reordered["bundle"]["element_categories"]
        .as_array_mut()
        .unwrap()
        .reverse();
    assert_eq!(envelope(reordered).digest, digest);

    for (path, replacement) in [
        (vec!["bundle", "elements", "0", "symbol"], json!("He")),
        (vec!["bundle", "elements", "0", "name"], json!("Deuterium")),
        (vec!["bundle", "elements", "0", "atomic_number"], json!(2)),
        (vec!["bundle", "elements", "0", "period"], json!(2)),
        (vec!["bundle", "elements", "0", "group"], json!(2)),
        (vec!["bundle", "elements", "0", "block"], json!("p")),
        (
            vec![
                "bundle",
                "element_categories",
                "0",
                "membership",
                "predicate",
                "predicates",
                "0",
                "value",
            ],
            json!(2),
        ),
        (
            vec!["bundle", "element_categories", "0", "membership", "include"],
            json!(["Ca", "Fe", "K"]),
        ),
        (
            vec!["bundle", "element_categories", "0", "membership", "exclude"],
            json!(["B"]),
        ),
        (
            vec!["bundle", "element_categories", "1", "membership", "members"],
            json!(["B", "Si", "Ca"]),
        ),
        (
            vec!["bundle", "elements", "0", "premise_ids"],
            json!([
                "premise.element.hydrogen.identity",
                "premise.element.silicon.identity"
            ]),
        ),
        (
            vec!["bundle", "element_categories", "0", "premise_ids"],
            json!([
                "premise.category.alkali-metal",
                "premise.category.metalloid"
            ]),
        ),
    ] {
        let mut changed = original.clone();
        let mut cursor = &mut changed;
        for segment in &path[..path.len() - 1] {
            cursor = if let Ok(index) = segment.parse::<usize>() {
                &mut cursor[index]
            } else {
                &mut cursor[*segment]
            };
        }
        let key = path.last().unwrap();
        cursor[*key] = replacement;
        assert_ne!(envelope(changed).digest, digest);
    }
}

#[test]
fn omitted_and_empty_records_have_identical_semantics() {
    let mut omitted = fixture();
    let mut explicit = fixture();
    explicit["bundle"]["elements"] = json!([]);
    explicit["bundle"]["element_categories"] = json!([]);
    let omitted = envelope(omitted.take());
    let explicit = envelope(explicit);
    assert_eq!(omitted.digest, explicit.digest);
    assert_eq!(
        omitted.canonical_json().unwrap(),
        explicit.canonical_json().unwrap()
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
fn invalid_element_facts_are_typed() {
    for (field, replacement) in [
        ("atomic_number", json!(0)),
        ("atomic_number", json!(119)),
        ("period", json!(8)),
        ("group", json!(19)),
        ("name", json!(" padded ")),
    ] {
        let mut value = with_registry(fixture());
        value["bundle"]["elements"][0][field] = replacement;
        assert_code(value, CatalogueErrorCode::InvalidElement);
    }
    let mut invalid_block = with_registry(fixture());
    invalid_block["bundle"]["elements"][0]["block"] = json!("x");
    assert!(serde_json::from_value::<CatalogueEnvelope>(invalid_block).is_err());
    let mut duplicate = with_registry(fixture());
    duplicate["bundle"]["elements"][1]["atomic_number"] = json!(1);
    assert_code(duplicate, CatalogueErrorCode::DuplicateId);

    for (field, replacement) in [("symbol", json!("H")), ("name", json!("Hydrogen"))] {
        let mut duplicate = with_registry(fixture());
        duplicate["bundle"]["elements"][1][field] = replacement;
        assert_code(duplicate, CatalogueErrorCode::DuplicateId);
    }

    let mut blank_name = with_registry(fixture());
    blank_name["bundle"]["elements"][0]["name"] = json!("");
    assert_code(blank_name, CatalogueErrorCode::InvalidElement);

    let mut empty_premises = with_registry(fixture());
    empty_premises["bundle"]["elements"][0]["premise_ids"] = json!([]);
    assert_code(empty_premises, CatalogueErrorCode::InvalidElement);

    let mut unknown_premise = with_registry(fixture());
    unknown_premise["bundle"]["elements"][0]["premise_ids"] = json!(["premise.absent"]);
    assert_code(unknown_premise, CatalogueErrorCode::UnknownReference);

    for (field, replacement) in [("symbol", json!("hydrogen")), ("block", json!("x"))] {
        let mut invalid = with_registry(fixture());
        invalid["bundle"]["elements"][0][field] = replacement;
        assert!(serde_json::from_value::<CatalogueEnvelope>(invalid).is_err());
    }

    let mut missing_premises = with_registry(fixture());
    missing_premises["bundle"]["elements"][0]
        .as_object_mut()
        .unwrap()
        .remove("premise_ids");
    assert!(serde_json::from_value::<CatalogueEnvelope>(missing_premises).is_err());
}

#[test]
fn invalid_categories_are_typed() {
    let mut empty = with_registry(fixture());
    empty["bundle"]["element_categories"] = json!([{"id":"Categories.Empty","subject":"element","membership":{"kind":"predicate","predicate":{"kind":"all","predicates":[]}},"premise_ids":["premise.rule.lithium-water.standard-outcome"]}]);
    assert_code(empty, CatalogueErrorCode::InvalidElementCategory);

    let mut unknown = with_generalized(fixture());
    unknown["bundle"]["element_categories"][0]["membership"]["include"] = json!(["Xx"]);
    assert_code(unknown, CatalogueErrorCode::UnknownReference);

    let mut mismatch = with_generalized(fixture());
    mismatch["bundle"]["element_categories"][0]["membership"]["predicate"]["predicates"][0]["value"] =
        json!("one");
    assert_code(mismatch, CatalogueErrorCode::InvalidElementCategory);

    let mut range = with_generalized(fixture());
    range["bundle"]["element_categories"][0]["membership"]["predicate"] =
        json!({"kind":"range","field":"name","min":1,"max":2});
    assert_code(range, CatalogueErrorCode::InvalidElementCategory);

    let mut backwards_range = with_generalized(fixture());
    backwards_range["bundle"]["element_categories"][0]["membership"]["predicate"] =
        json!({"kind":"range","field":"group","min":2,"max":1});
    assert_code(backwards_range, CatalogueErrorCode::InvalidElementCategory);

    for membership in [
        json!({"kind":"explicit","members":["Xx"]}),
        json!({"kind":"predicate","predicate":{"kind":"equals","field":"group","value":1},"include":["Xx"]}),
        json!({"kind":"predicate","predicate":{"kind":"equals","field":"group","value":1},"exclude":["Xx"]}),
    ] {
        let mut unknown = with_generalized(fixture());
        unknown["bundle"]["element_categories"][0]["membership"] = membership;
        assert_code(unknown, CatalogueErrorCode::UnknownReference);
    }

    let mut overlap = with_generalized(fixture());
    overlap["bundle"]["element_categories"][0]["membership"]["include"] = json!(["Ca"]);
    overlap["bundle"]["element_categories"][0]["membership"]["exclude"] = json!(["Ca"]);
    assert_code(overlap, CatalogueErrorCode::InvalidElementCategory);

    let mut no_members = with_generalized(fixture());
    no_members["bundle"]["element_categories"][0]["membership"] = json!({
        "kind":"predicate",
        "predicate":{"kind":"equals","field":"group","value":18}
    });
    assert_code(no_members, CatalogueErrorCode::InvalidElementCategory);

    for predicate in [
        json!({"kind":"all","predicates":[]}),
        json!({"kind":"any","predicates":[]}),
        json!({"kind":"in_set","field":"group","values":[]}),
        json!({"kind":"all","predicates":[
            {"kind":"equals","field":"group","value":1},
            {"kind":"equals","field":"group","value":1}
        ]}),
        json!({"kind":"in_set","field":"group","values":[1,1]}),
        json!({"kind":"all","predicates":[
            {"kind":"all","predicates":[
                {"kind":"equals","field":"group","value":1},
                {"kind":"equals","field":"block","value":"s"}
            ]},
            {"kind":"all","predicates":[
                {"kind":"equals","field":"block","value":"s"},
                {"kind":"equals","field":"group","value":1}
            ]}
        ]}),
    ] {
        let mut invalid = with_generalized(fixture());
        invalid["bundle"]["element_categories"][0]["membership"] =
            json!({"kind":"predicate","predicate":predicate,"include":["Li"]});
        assert_code(invalid, CatalogueErrorCode::InvalidElementCategory);
    }

    let mut empty_category_premises = with_generalized(fixture());
    empty_category_premises["bundle"]["element_categories"][0]["premise_ids"] = json!([]);
    assert_code(
        empty_category_premises,
        CatalogueErrorCode::InvalidElementCategory,
    );

    let mut unknown_category_premise = with_generalized(fixture());
    unknown_category_premise["bundle"]["element_categories"][0]["premise_ids"] =
        json!(["premise.absent"]);
    assert_code(
        unknown_category_premise,
        CatalogueErrorCode::UnknownReference,
    );

    let mut duplicate_category = with_generalized(fixture());
    let category = duplicate_category["bundle"]["element_categories"][0].clone();
    duplicate_category["bundle"]["element_categories"]
        .as_array_mut()
        .unwrap()
        .push(category);
    assert_code(duplicate_category, CatalogueErrorCode::DuplicateId);

    let mut empty_explicit = with_generalized(fixture());
    empty_explicit["bundle"]["element_categories"][0]["membership"] =
        json!({"kind":"explicit","members":[]});
    assert_code(empty_explicit, CatalogueErrorCode::InvalidElementCategory);
}

#[test]
fn closed_wire_records_reject_unknown_variants_fields_and_missing_dependencies() {
    for membership in [
        json!({"kind":"unknown","members":["Li"]}),
        json!({"kind":"predicate","predicate":{"kind":"unknown"}}),
        json!({"kind":"predicate","predicate":{"kind":"equals","field":"unknown","value":1}}),
        json!({"kind":"predicate","predicate":{"kind":"equals","field":"group","value":1,"extra":true}}),
        json!({"kind":"predicate","predicate":{"kind":"equals","field":"group","value":1},"extra":true}),
    ] {
        let mut invalid = with_generalized(fixture());
        invalid["bundle"]["element_categories"][0]["membership"] = membership;
        assert!(serde_json::from_value::<CatalogueEnvelope>(invalid).is_err());
    }

    let mut missing = with_generalized(fixture());
    missing["bundle"]["element_categories"][0]
        .as_object_mut()
        .unwrap()
        .remove("premise_ids");
    assert!(serde_json::from_value::<CatalogueEnvelope>(missing).is_err());

    let mut invalid_subject = with_generalized(fixture());
    invalid_subject["bundle"]["element_categories"][0]["subject"] = json!("structure");
    assert!(serde_json::from_value::<CatalogueEnvelope>(invalid_subject).is_err());

    let mut extra_element_field = with_generalized(fixture());
    extra_element_field["bundle"]["elements"][0]["extra"] = json!(true);
    assert!(serde_json::from_value::<CatalogueEnvelope>(extra_element_field).is_err());

    let mut extra_category_field = with_generalized(fixture());
    extra_category_field["bundle"]["element_categories"][0]["extra"] = json!(true);
    assert!(serde_json::from_value::<CatalogueEnvelope>(extra_category_field).is_err());
}

#[test]
fn public_wire_types_remain_closed_and_typed() {
    let predicate = ElementPredicateRecord::Equals {
        field: ElementFieldRecord::Group,
        value: ElementScalarRecord::Integer(1),
    };
    let membership = ElementCategoryMembershipRecord::Explicit {
        members: [ElementSymbol::new("B").unwrap()].into_iter().collect(),
    };
    assert_eq!(
        predicate,
        serde_json::from_value(json!({"kind":"equals","field":"group","value":1})).unwrap()
    );
    assert_eq!(
        membership,
        serde_json::from_value(json!({"kind":"explicit","members":["B"]})).unwrap()
    );
}
