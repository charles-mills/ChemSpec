use std::{fs, path::PathBuf, str::FromStr};

use chem_catalogue::{
    CatalogueEnvelope, CatalogueErrorCode, MacroscopicMaterialContextRecord,
    MacroscopicMaterialRecord, ObservationCompatibilityRecord, ObservationPredicate,
    PublicationKind, ReferenceCatalogue, ReferenceIntegrityPolicy, ReviewStatus, ReviewerRecord,
    ValidatedCatalogueBundle,
};
use chem_domain::{
    ContentDigest, Phase, PremiseId, ReactionRuleId, RepresentationKind, StructureId,
};
use serde_json::Value;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture_bytes() -> Vec<u8> {
    fs::read(workspace_root().join("conformance/catalogue/lithium-rule-001.catalogue.json"))
        .expect("lithium fixture should be readable")
}

fn reference_catalogue_bytes() -> Vec<u8> {
    fs::read(workspace_root().join("catalogue/reference/core-chemistry/catalogue.json"))
        .expect("reference catalogue should be readable")
}

fn reference_review_bytes() -> Vec<u8> {
    fs::read(workspace_root().join("catalogue/reviews/core-chemistry.review.json"))
        .expect("reference catalogue review should be readable")
}

fn integrity_policy() -> ReferenceIntegrityPolicy {
    let envelope: CatalogueEnvelope = serde_json::from_slice(&reference_catalogue_bytes()).unwrap();
    let review: Value = serde_json::from_slice(&reference_review_bytes()).unwrap();
    ReferenceIntegrityPolicy::new(envelope.digest, ContentDigest::of_json(&review).unwrap())
}

fn envelope() -> CatalogueEnvelope {
    serde_json::from_slice(&fixture_bytes()).expect("fixture should deserialize")
}

fn rebound(mut envelope: CatalogueEnvelope) -> CatalogueEnvelope {
    envelope.digest = envelope.computed_digest().expect("digest should compute");
    envelope
}

fn rebound_error(envelope: CatalogueEnvelope) -> CatalogueErrorCode {
    ValidatedCatalogueBundle::validate(rebound(envelope))
        .expect_err("corruption must fail")
        .code()
}

#[test]
fn canonical_fixture_matches_schema_digest_and_closed_domain() {
    let root = workspace_root();
    let schema: Value = serde_json::from_slice(
        &fs::read(root.join("schemas/chem-catalogue-1.schema.json")).unwrap(),
    )
    .unwrap();
    let fixture: Value = serde_json::from_slice(&fixture_bytes()).unwrap();
    let validator = jsonschema::draft202012::new(&schema).unwrap();
    assert!(
        validator.is_valid(&fixture),
        "fixture schema errors: {:?}",
        validator.iter_errors(&fixture).collect::<Vec<_>>()
    );
    let mut malformed_operation = fixture.clone();
    malformed_operation["bundle"]["rules"][0]["operation_template"][0]
        .as_object_mut()
        .unwrap()
        .remove("site");
    assert!(!validator.is_valid(&malformed_operation));

    let mut oversized_transfer = fixture.clone();
    oversized_transfer["bundle"]["rules"][0]["operation_template"][4]["count"] = Value::from(256);
    assert!(!validator.is_valid(&oversized_transfer));
    assert!(serde_json::from_value::<CatalogueEnvelope>(oversized_transfer).is_err());

    let mut oversized_charge = fixture.clone();
    oversized_charge["bundle"]["rules"][0]["operation_template"][0]["before"]["site"][0] =
        Value::from(32768);
    assert!(!validator.is_valid(&oversized_charge));
    assert!(serde_json::from_value::<CatalogueEnvelope>(oversized_charge).is_err());

    let mut indexed_static_label = fixture.clone();
    indexed_static_label["bundle"]["structures"][0]["sites"][0]["label"] = Value::from("li[1]");
    indexed_static_label["bundle"]["structures"][0]["domains"][0]["sites"][0] =
        Value::from("li[1]");
    assert!(!validator.is_valid(&indexed_static_label));
    let mut indexed_envelope: CatalogueEnvelope =
        serde_json::from_value(indexed_static_label).unwrap();
    indexed_envelope.digest = indexed_envelope.computed_digest().unwrap();
    assert_eq!(
        ValidatedCatalogueBundle::validate(indexed_envelope)
            .unwrap_err()
            .code(),
        CatalogueErrorCode::InvalidStructure
    );

    let envelope = envelope();
    assert_eq!(
        envelope.computed_digest().unwrap(),
        envelope.digest,
        "computed digest is {}",
        envelope.computed_digest().unwrap()
    );
    let catalogue = ValidatedCatalogueBundle::from_json(&fixture_bytes()).unwrap();
    assert_eq!(catalogue.structures().len(), 4);
    assert_eq!(catalogue.rules().len(), 1);
    assert_eq!(
        catalogue
            .structure(&StructureId::from_str("LithiumMetal").unwrap())
            .unwrap()
            .representation(),
        RepresentationKind::Metallic
    );
    assert_eq!(
        catalogue
            .structure(&StructureId::from_str("LithiumHydroxide").unwrap())
            .unwrap()
            .representation(),
        RepresentationKind::Ionic
    );
    assert_eq!(
        catalogue
            .rule(&ReactionRuleId::from_str("Rules.AlkaliMetalWithWater").unwrap())
            .unwrap()
            .reactant_atoms()
            .len(),
        8
    );
}

#[test]
fn built_in_reference_data_verifies_its_packaged_review_identity() {
    let catalogue_bytes = reference_catalogue_bytes();
    let review_bytes = reference_review_bytes();
    let policy = integrity_policy();
    let reference =
        ReferenceCatalogue::from_canonical_json(&catalogue_bytes, &review_bytes, policy)
            .expect("the packaged catalogue and review should load");
    assert_eq!(reference.document().elements.len(), 118);
    assert_eq!(reference.document().generalized_rules.len(), 75);

    let wrong_catalogue_policy = ReferenceIntegrityPolicy::new(
        ContentDigest::sha256(b"different catalogue"),
        policy.review_digest(),
    );
    let wrong_catalogue = ReferenceCatalogue::from_canonical_json(
        &catalogue_bytes,
        &review_bytes,
        wrong_catalogue_policy,
    )
    .unwrap_err();
    assert_eq!(
        wrong_catalogue.code(),
        CatalogueErrorCode::IntegrityMismatch
    );
    assert_eq!(wrong_catalogue.diagnostic_code(), "CHEMS-C025");

    let wrong_review_policy = ReferenceIntegrityPolicy::new(
        policy.catalogue_digest(),
        ContentDigest::sha256(b"different review"),
    );
    let wrong_review = ReferenceCatalogue::from_canonical_json(
        &catalogue_bytes,
        &review_bytes,
        wrong_review_policy,
    )
    .unwrap_err();
    assert_eq!(wrong_review.code(), CatalogueErrorCode::IntegrityMismatch);
    assert_eq!(wrong_review.diagnostic_code(), "CHEMS-C025");

    let mut incomplete_review: Value = serde_json::from_slice(&review_bytes).unwrap();
    incomplete_review["premises"].as_array_mut().unwrap().pop();
    let incomplete_bytes = serde_json::to_vec(&incomplete_review).unwrap();
    let incomplete_policy = ReferenceIntegrityPolicy::new(
        policy.catalogue_digest(),
        ContentDigest::of_json(&incomplete_review).unwrap(),
    );
    let incomplete_review = ReferenceCatalogue::from_canonical_json(
        &catalogue_bytes,
        &incomplete_bytes,
        incomplete_policy,
    )
    .unwrap_err();
    assert_eq!(
        incomplete_review.code(),
        CatalogueErrorCode::InvalidReviewAttestation
    );
    assert_eq!(incomplete_review.diagnostic_code(), "CHEMS-C026");
}

#[test]
fn optional_macroscopic_materials_are_backward_compatible_and_role_aware() {
    let legacy = ValidatedCatalogueBundle::from_json(&fixture_bytes())
        .expect("legacy schema-1 catalogue remains valid");
    let lithium = StructureId::from_str("LithiumMetal").unwrap();
    assert!(legacy.macroscopic_material(&lithium, None).is_none());

    let mut enriched = envelope();
    let rule = ReactionRuleId::from_str("Rules.AlkaliMetalWithWater").unwrap();
    let premise = PremiseId::from_str("premise.structure.lithium-metal").unwrap();
    enriched.bundle.macroscopic_materials.extend([
        MacroscopicMaterialRecord {
            structure: lithium.clone(),
            context: MacroscopicMaterialContextRecord::Standard,
            phase: Phase::Solid,
            colour: None,
            premise_ids: [premise.clone()].into_iter().collect(),
        },
        MacroscopicMaterialRecord {
            structure: lithium.clone(),
            context: MacroscopicMaterialContextRecord::ReactionRole {
                rule: rule.clone(),
                role: "metal".to_owned(),
            },
            phase: Phase::Solid,
            colour: Some([186, 198, 204]),
            premise_ids: [premise].into_iter().collect(),
        },
    ]);
    let enriched = rebound(enriched);
    let schema: Value = serde_json::from_slice(
        &fs::read(workspace_root().join("schemas/chem-catalogue-1.schema.json")).unwrap(),
    )
    .unwrap();
    let validator = jsonschema::draft202012::new(&schema).unwrap();
    let serialized = serde_json::to_value(&enriched).unwrap();
    assert!(
        validator.is_valid(&serialized),
        "macroscopic catalogue schema errors: {:?}",
        validator.iter_errors(&serialized).collect::<Vec<_>>()
    );
    let enriched =
        ValidatedCatalogueBundle::validate(enriched).expect("reviewed phase records validate");
    assert_eq!(
        enriched.macroscopic_material(&lithium, None).unwrap().phase,
        Phase::Solid
    );
    let contextual = enriched
        .macroscopic_material(&lithium, Some((&rule, "metal")))
        .unwrap();
    assert_eq!(contextual.phase, Phase::Solid);
    assert_eq!(contextual.colour, Some([186, 198, 204]));
    assert!(matches!(
        contextual.context,
        MacroscopicMaterialContextRecord::ReactionRole { .. }
    ));
}

#[test]
fn invalid_macroscopic_role_is_rejected_with_a_typed_error() {
    let mut invalid = envelope();
    invalid
        .bundle
        .macroscopic_materials
        .push(MacroscopicMaterialRecord {
            structure: StructureId::from_str("LithiumMetal").unwrap(),
            context: MacroscopicMaterialContextRecord::ReactionRole {
                rule: ReactionRuleId::from_str("Rules.AlkaliMetalWithWater").unwrap(),
                role: "inventedRole".to_owned(),
            },
            phase: Phase::Solid,
            colour: None,
            premise_ids: [PremiseId::from_str("premise.structure.lithium-metal").unwrap()]
                .into_iter()
                .collect(),
        });
    assert_eq!(
        rebound_error(invalid),
        CatalogueErrorCode::InvalidMacroscopicMaterial
    );

    let mut unknown_rule = envelope();
    unknown_rule
        .bundle
        .macroscopic_materials
        .push(MacroscopicMaterialRecord {
            structure: StructureId::from_str("LithiumMetal").unwrap(),
            context: MacroscopicMaterialContextRecord::ReactionRole {
                rule: ReactionRuleId::from_str("Rules.Invented").unwrap(),
                role: "metal".to_owned(),
            },
            phase: Phase::Solid,
            colour: None,
            premise_ids: [PremiseId::from_str("premise.structure.lithium-metal").unwrap()]
                .into_iter()
                .collect(),
        });
    assert_eq!(
        rebound_error(unknown_rule),
        CatalogueErrorCode::InvalidMacroscopicMaterial
    );

    let mut unknown_structure = envelope();
    unknown_structure
        .bundle
        .macroscopic_materials
        .push(MacroscopicMaterialRecord {
            structure: StructureId::from_str("InventedMaterial").unwrap(),
            context: MacroscopicMaterialContextRecord::Standard,
            phase: Phase::Solid,
            colour: None,
            premise_ids: [PremiseId::from_str("premise.structure.lithium-metal").unwrap()]
                .into_iter()
                .collect(),
        });
    assert_eq!(
        rebound_error(unknown_structure),
        CatalogueErrorCode::InvalidMacroscopicMaterial
    );

    let valid_record = MacroscopicMaterialRecord {
        structure: StructureId::from_str("LithiumMetal").unwrap(),
        context: MacroscopicMaterialContextRecord::Standard,
        phase: Phase::Solid,
        colour: None,
        premise_ids: [PremiseId::from_str("premise.structure.lithium-metal").unwrap()]
            .into_iter()
            .collect(),
    };
    let mut unknown_premise = envelope();
    let mut missing_evidence = valid_record.clone();
    missing_evidence.premise_ids = [PremiseId::from_str("premise.invented").unwrap()]
        .into_iter()
        .collect();
    unknown_premise
        .bundle
        .macroscopic_materials
        .push(missing_evidence);
    assert_eq!(
        rebound_error(unknown_premise),
        CatalogueErrorCode::InvalidMacroscopicMaterial
    );

    let mut duplicate = envelope();
    duplicate.bundle.macroscopic_materials = vec![valid_record.clone(), valid_record];
    assert_eq!(
        rebound_error(duplicate),
        CatalogueErrorCode::InvalidMacroscopicMaterial
    );
}

#[test]
fn every_rule_premise_and_observation_premise_resolves() {
    let catalogue = ValidatedCatalogueBundle::from_json(&fixture_bytes()).unwrap();
    let rule = catalogue
        .rule(&ReactionRuleId::from_str("Rules.AlkaliMetalWithWater").unwrap())
        .unwrap();
    for premise in &rule.record().premise_ids {
        assert!(catalogue.premise(premise).is_some(), "missing `{premise}`");
    }
    for observation in &rule.record().observation_compatibility {
        assert!(catalogue.premise(&observation.premise_id).is_some());
    }
    assert!(
        catalogue
            .valence_premise(&PremiseId::from_str("premise.valence.li-h-o.initial-domain").unwrap())
            .is_some()
    );
}

#[test]
fn derived_template_dependencies_must_be_nonempty_resolved_and_rule_bound() {
    let base: Value = serde_json::from_slice(&fixture_bytes()).unwrap();
    for mutation in [
        serde_json::json!([]),
        serde_json::json!(["premise.unknown"]),
        serde_json::json!(["premise.observation.hydrogen-evolves"]),
    ] {
        let mut value = base.clone();
        value["bundle"]["rules"][0]["mapping_template"][0]["premise_ids"] = mutation;
        assert!(ValidatedCatalogueBundle::from_json(&serde_json::to_vec(&value).unwrap()).is_err());
    }

    let document = ValidatedCatalogueBundle::from_json(&fixture_bytes()).unwrap();
    let rule_id = "Rules.AlkaliMetalWithWater".parse().unwrap();
    let rule = document.require_rule(&rule_id).unwrap().record();
    assert_eq!(
        rule.model_assumptions.premise_ids,
        ["premise.rule.lithium-water.standard-outcome"
            .parse()
            .unwrap()]
        .into_iter()
        .collect()
    );
}

#[test]
fn semantic_mutation_changes_digest_but_record_order_does_not() {
    let original = envelope();
    let digest = original.computed_digest().unwrap();

    let mut reordered = original.clone();
    reordered.bundle.evidence.reverse();
    reordered.bundle.premises.reverse();
    reordered.bundle.structures.reverse();
    reordered.bundle.rules.reverse();
    reordered.bundle.rules[0].mapping_template.reverse();
    reordered.bundle.structures[0..2].reverse();
    if let chem_catalogue::OperationTemplateRecord::AssociateIonic {
        components,
        component_charges,
        ..
    } = &mut reordered.bundle.rules[0].operation_template[7]
    {
        components.reverse();
        component_charges.reverse();
        components[0].reverse();
    }
    if let chem_catalogue::OperationTemplateRecord::AssignProduct { atoms, .. } =
        &mut reordered.bundle.rules[0].operation_template[9]
    {
        atoms.reverse();
    }
    assert_eq!(reordered.computed_digest().unwrap(), digest);
    assert_eq!(
        reordered.canonical_json().unwrap(),
        original.canonical_json().unwrap()
    );

    let mut changed = original;
    changed.bundle.rules[0]
        .applicability
        .required_context
        .push('!');
    assert_ne!(changed.computed_digest().unwrap(), digest);
}

#[test]
fn corrupt_structure_has_a_typed_catalogue_error() {
    let mut value: Value = serde_json::from_slice(&fixture_bytes()).unwrap();
    value["bundle"]["structures"][1]["atoms"][0]["formal_charge"] = Value::from(1);
    let mut envelope: CatalogueEnvelope = serde_json::from_value(value).unwrap();
    envelope = rebound(envelope);
    let error = ValidatedCatalogueBundle::validate(envelope).unwrap_err();
    assert!(matches!(
        error.code(),
        CatalogueErrorCode::InvalidStructure | CatalogueErrorCode::InvalidValencePremise
    ));
}

#[test]
fn corrupt_mapping_is_a_typed_system_error() {
    let mut envelope = envelope();
    envelope.bundle.rules[0].mapping_template.pop();
    assert_eq!(rebound_error(envelope), CatalogueErrorCode::InvalidMapping);
}

#[test]
fn omitted_structure_premise_and_overlapping_assignment_are_rejected() {
    let mut missing_premise = envelope();
    missing_premise.bundle.rules[0]
        .premise_ids
        .remove(&PremiseId::from_str("premise.structure.water").unwrap());
    assert_eq!(
        rebound_error(missing_premise),
        CatalogueErrorCode::InvalidRule
    );

    let mut value: Value = serde_json::from_slice(&fixture_bytes()).unwrap();
    value["bundle"]["rules"][0]["operation_template"][10]["atoms"][0] = Value::from("metal[1].li");
    let overlapping: CatalogueEnvelope = serde_json::from_value(value).unwrap();
    assert_eq!(
        rebound_error(overlapping),
        CatalogueErrorCode::InvalidOperationTemplate
    );
}

#[test]
fn corrupt_applicability_is_a_typed_system_error() {
    let mut envelope = envelope();
    envelope.bundle.rules[0]
        .applicability
        .reactant_structure_ids
        .pop_first();
    assert_eq!(
        rebound_error(envelope),
        CatalogueErrorCode::InvalidApplicability
    );
}

#[test]
fn corrupt_operation_template_is_a_typed_system_error() {
    let mut value: Value = serde_json::from_slice(&fixture_bytes()).unwrap();
    value["bundle"]["rules"][0]["operation_template"][4]["count"] = Value::from(0);
    let envelope: CatalogueEnvelope = serde_json::from_value(value).unwrap();
    assert_eq!(
        rebound_error(envelope),
        CatalogueErrorCode::InvalidOperationTemplate
    );
}

#[test]
fn unsupported_states_relationships_and_component_charges_are_rejected() {
    let mut unsupported: Value = serde_json::from_slice(&fixture_bytes()).unwrap();
    unsupported["bundle"]["rules"][0]["operation_template"][0]["after"]["site"] =
        serde_json::json!([0, 3, 1]);
    assert_eq!(
        rebound_error(serde_json::from_value(unsupported).unwrap()),
        CatalogueErrorCode::InvalidOperationTemplate
    );

    let mut absent_edge: Value = serde_json::from_slice(&fixture_bytes()).unwrap();
    absent_edge["bundle"]["rules"][0]["operation_template"][2]["edge"] =
        serde_json::json!(["water[1].h1", "water[1].h2", "single"]);
    assert_eq!(
        rebound_error(serde_json::from_value(absent_edge).unwrap()),
        CatalogueErrorCode::InvalidOperationTemplate
    );

    let mut wrong_charge: Value = serde_json::from_slice(&fixture_bytes()).unwrap();
    wrong_charge["bundle"]["rules"][0]["operation_template"][7]["component_charges"] =
        serde_json::json!([2, -2]);
    assert_eq!(
        rebound_error(serde_json::from_value(wrong_charge).unwrap()),
        CatalogueErrorCode::InvalidOperationTemplate
    );

    let mut incomplete_component: Value = serde_json::from_slice(&fixture_bytes()).unwrap();
    incomplete_component["bundle"]["rules"][0]["operation_template"][7]["components"][1]
        .as_array_mut()
        .unwrap()
        .pop();
    assert_eq!(
        rebound_error(serde_json::from_value(incomplete_component).unwrap()),
        CatalogueErrorCode::InvalidOperationTemplate
    );

    let mut unbound_valence: Value = serde_json::from_slice(&fixture_bytes()).unwrap();
    unbound_valence["bundle"]["premises"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "id": "premise.valence.unbound",
            "statement": "A deliberately unbound test premise.",
            "evidence": ["evidence.iupac.goldbook"],
            "review": {"status": "provisional"},
            "rule_version": "1"
        }));
    unbound_valence["bundle"]["valence_premises"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "premise_id": "premise.valence.unbound",
            "neutral_valence": [{"element": "O", "neutral_valence_electrons": 6}],
            "supported_states": [{
                "element": "O", "formal_charge": -1,
                "non_bonding_electrons": 6, "unpaired_electrons": 2,
                "covalent_bond_order_sum": 1
            }]
        }));
    unbound_valence["bundle"]["rules"][0]["operation_template"][2]["after"]["left"] =
        serde_json::json!([-1, 6, 2]);
    assert_eq!(
        rebound_error(serde_json::from_value(unbound_valence).unwrap()),
        CatalogueErrorCode::InvalidOperationTemplate
    );
}

#[test]
fn every_closed_operation_variant_has_a_negative_validation_case() {
    let base: Value = serde_json::from_slice(&fixture_bytes()).unwrap();
    let mut cases = Vec::new();

    let mut release = base.clone();
    release["bundle"]["rules"][0]["operation_template"][0]["domain"] =
        Value::from("metal[1].missing");
    cases.push(release);

    let mut cleave = base.clone();
    cleave["bundle"]["rules"][0]["operation_template"][2]["edge"] =
        serde_json::json!(["water[1].h1", "water[1].h2", "single"]);
    cases.push(cleave);

    let mut form = base.clone();
    form["bundle"]["rules"][0]["operation_template"][6]["electron_contribution"] =
        serde_json::json!({"left": 0, "right": 0});
    cases.push(form);

    let mut cleave_dative = base.clone();
    cleave_dative["bundle"]["rules"][0]["operation_template"][2] = serde_json::json!({
        "kind": "cleave_dative",
        "premise_ids": ["premise.rule.lithium-water.standard-outcome", "premise.structure.water", "premise.valence.li-h-o.initial-domain"],
        "donor": "water[1].o",
        "acceptor": "water[1].h1",
        "allocation": {"heterolytic_to": "water[1].o"},
        "before": {"left": [0,4,0], "right": [0,0,0]},
        "after": {"left": [-1,6,0], "right": [1,0,0]}
    });
    cases.push(cleave_dative);

    let mut form_dative = base.clone();
    form_dative["bundle"]["rules"][0]["operation_template"][6] = serde_json::json!({
        "kind": "form_dative",
        "premise_ids": ["premise.rule.lithium-water.standard-outcome", "premise.structure.water", "premise.valence.li-h-o.initial-domain"],
        "donor": "water[1].o",
        "acceptor": "water[1].h1",
        "before": {"left": [-1,6,6], "right": [1,0,0]},
        "after": {"left": [0,4,6], "right": [0,0,0]}
    });
    cases.push(form_dative);

    let mut change = base.clone();
    change["bundle"]["rules"][0]["operation_template"]
        .as_array_mut()
        .unwrap()
        .insert(
            7,
            serde_json::json!({
                "kind": "change_covalent",
                "premise_ids": ["premise.rule.lithium-water.standard-outcome", "premise.structure.water", "premise.valence.li-h-o.initial-domain"],
                "edge": ["water[1].o", "water[1].h1"],
                "old_order": "single",
                "new_order": "single",
                "allocation": "homolytic",
                "before": {"left": [0,4,0], "right": [0,0,0]},
                "after": {"left": [0,4,0], "right": [0,0,0]}
            }),
        );
    cases.push(change);

    let mut associate = base.clone();
    associate["bundle"]["rules"][0]["operation_template"][7]["component_charges"] =
        serde_json::json!([2, -2]);
    cases.push(associate);

    let mut dissociate = base.clone();
    dissociate["bundle"]["rules"][0]["operation_template"]
        .as_array_mut()
        .unwrap()
        .insert(
            7,
            serde_json::json!({
                "kind": "dissociate_ionic",
                "premise_ids": ["premise.rule.lithium-water.standard-outcome", "premise.structure.lithium-hydroxide"],
                "association": "water[1].missing"
            }),
        );
    cases.push(dissociate);

    let mut join = base.clone();
    join["bundle"]["rules"][0]["operation_template"]
        .as_array_mut()
        .unwrap()
        .insert(
            7,
            serde_json::json!({
                "kind": "join_metallic",
                "premise_ids": ["premise.rule.lithium-water.standard-outcome", "premise.structure.lithium-metal", "premise.valence.li-h-o.initial-domain"],
                "site": "metal[1].li",
                "domain": "metal[1].missing",
                "allocation": "donate_electron",
                "before": {"site": [0,1,1], "domain_electrons": 0},
                "after": {"site": [1,0,0], "domain_electrons": 1}
            }),
        );
    cases.push(join);

    let mut transfer = base.clone();
    transfer["bundle"]["rules"][0]["operation_template"][4]["count"] = Value::from(0);
    cases.push(transfer);

    let mut assign = base;
    assign["bundle"]["rules"][0]["operation_template"][9]["product"] = Value::from("missing[1]");
    cases.push(assign);

    for case in cases {
        let envelope: CatalogueEnvelope = serde_json::from_value(case).unwrap();
        assert_eq!(
            rebound_error(envelope),
            CatalogueErrorCode::InvalidOperationTemplate
        );
    }
}

#[test]
fn ordinary_covalent_formation_rejects_dative_electron_allocation() {
    let mut dative: Value = serde_json::from_slice(&fixture_bytes()).unwrap();
    dative["bundle"]["rules"][0]["operation_template"][6] = serde_json::json!({
        "kind": "form_covalent",
        "premise_ids": ["premise.rule.lithium-water.standard-outcome", "premise.structure.hydrogen", "premise.structure.water", "premise.valence.li-h-o.initial-domain"],
        "edge": ["water[1].o", "water[1].h1", "single"],
        "electron_contribution": {"left": 2, "right": 0},
        "before": {"left": [-1,6,0], "right": [1,0,0]},
        "after": {"left": [0,4,0], "right": [0,0,0]}
    });
    assert_eq!(
        rebound_error(serde_json::from_value(dative).unwrap()),
        CatalogueErrorCode::InvalidOperationTemplate
    );
}

#[test]
fn reviewed_dative_operation_templates_require_premises_and_a_donor_pair() {
    let mut dative: Value = serde_json::from_slice(&fixture_bytes()).unwrap();
    dative["bundle"]["rules"][0]["operation_template"][6] = serde_json::json!({
        "kind": "form_dative",
        "premise_ids": ["premise.rule.lithium-water.standard-outcome", "premise.structure.water", "premise.valence.li-h-o.initial-domain"],
        "donor": "water[1].o",
        "acceptor": "water[1].h1",
        "before": {"left": [-1,6,0], "right": [1,0,0]},
        "after": {"left": [0,4,0], "right": [0,0,0]}
    });
    let envelope: CatalogueEnvelope = serde_json::from_value(dative.clone()).unwrap();
    assert!(ValidatedCatalogueBundle::validate(rebound(envelope)).is_ok());

    let mut missing_premise = dative.clone();
    missing_premise["bundle"]["rules"][0]["operation_template"][6]["premise_ids"] =
        serde_json::json!([]);
    assert_eq!(
        rebound_error(serde_json::from_value(missing_premise).unwrap()),
        CatalogueErrorCode::InvalidRule
    );

    dative["bundle"]["rules"][0]["operation_template"][6]["before"]["left"] =
        serde_json::json!([-1, 6, 6]);
    assert_eq!(
        rebound_error(serde_json::from_value(dative).unwrap()),
        CatalogueErrorCode::InvalidOperationTemplate
    );
}

#[test]
fn reviewed_dative_cleavage_requires_the_exact_directed_edge() {
    let mut dative: Value = serde_json::from_slice(&fixture_bytes()).unwrap();
    dative["bundle"]["structures"][1]["bonds"][0]["electron_origin"] = serde_json::json!({
        "kind": "dative",
        "donor": "o",
        "acceptor": "h1"
    });
    for (index, instance) in [(2, 1), (3, 2)] {
        dative["bundle"]["rules"][0]["operation_template"][index] = serde_json::json!({
            "kind": "cleave_dative",
            "premise_ids": ["premise.rule.lithium-water.standard-outcome", "premise.structure.water", "premise.valence.li-h-o.initial-domain"],
            "donor": format!("water[{instance}].o"),
            "acceptor": format!("water[{instance}].h1"),
            "allocation": {"heterolytic_to": format!("water[{instance}].o")},
            "before": {"left": [0,4,0], "right": [0,0,0]},
            "after": {"left": [-1,6,0], "right": [1,0,0]}
        });
    }
    let envelope: CatalogueEnvelope = serde_json::from_value(dative.clone()).unwrap();
    match &envelope.bundle.rules[0].operation_template[2] {
        chem_catalogue::OperationTemplateRecord::CleaveDative { premise_ids, .. } => {
            assert_eq!(premise_ids.len(), 3);
            assert!(premise_ids.contains(&PremiseId::from_str("premise.structure.water").unwrap()));
        }
        operation => panic!("unexpected operation: {operation:?}"),
    }
    assert!(ValidatedCatalogueBundle::validate(rebound(envelope)).is_ok());

    let mut reversed = dative.clone();
    reversed["bundle"]["rules"][0]["operation_template"][2]["donor"] = Value::from("water[1].h1");
    reversed["bundle"]["rules"][0]["operation_template"][2]["acceptor"] = Value::from("water[1].o");
    assert_eq!(
        rebound_error(serde_json::from_value(reversed).unwrap()),
        CatalogueErrorCode::InvalidOperationTemplate
    );

    dative["bundle"]["rules"][0]["operation_template"][2]["acceptor"] = Value::from("water[1].h2");
    assert_eq!(
        rebound_error(serde_json::from_value(dative).unwrap()),
        CatalogueErrorCode::InvalidOperationTemplate
    );
}

#[test]
fn dative_structure_records_reject_non_single_and_nonendpoint_origins() {
    let base: Value = serde_json::from_slice(&fixture_bytes()).unwrap();
    for origin in [
        serde_json::json!({"kind": "dative", "donor": "o", "acceptor": "h1"}),
        serde_json::json!({"kind": "dative", "donor": "o", "acceptor": "missing"}),
    ] {
        let mut value = base.clone();
        value["bundle"]["structures"][1]["bonds"][0]["electron_origin"] = origin;
        if value["bundle"]["structures"][1]["bonds"][0]["electron_origin"]["acceptor"] == "h1" {
            value["bundle"]["structures"][1]["bonds"][0]["order"] = serde_json::json!("double");
        }
        let envelope: CatalogueEnvelope = serde_json::from_value(value).unwrap();
        assert_eq!(
            rebound_error(envelope),
            CatalogueErrorCode::InvalidStructure
        );
    }
}

#[test]
fn inexact_metallic_radicals_are_rejected() {
    let add_states = |value: &mut Value| {
        let states = value["bundle"]["valence_premises"][0]["supported_states"]
            .as_array_mut()
            .unwrap();
        states.push(serde_json::json!({
            "element": "Li", "formal_charge": -1,
            "non_bonding_electrons": 2, "unpaired_electrons": 0,
            "covalent_bond_order_sum": 0
        }));
        states.push(serde_json::json!({
            "element": "Li", "formal_charge": -2,
            "non_bonding_electrons": 3, "unpaired_electrons": 3,
            "covalent_bond_order_sum": 0
        }));
    };

    let mut release: Value = serde_json::from_slice(&fixture_bytes()).unwrap();
    add_states(&mut release);
    release["bundle"]["rules"][0]["operation_template"][0]["before"]["site"] =
        serde_json::json!([-1, 2, 0]);
    release["bundle"]["rules"][0]["operation_template"][0]["after"]["site"] =
        serde_json::json!([-2, 3, 3]);
    assert_eq!(
        rebound_error(serde_json::from_value(release).unwrap()),
        CatalogueErrorCode::InvalidOperationTemplate
    );

    let mut join: Value = serde_json::from_slice(&fixture_bytes()).unwrap();
    add_states(&mut join);
    join["bundle"]["rules"][0]["operation_template"]
        .as_array_mut()
        .unwrap()
        .insert(
            7,
            serde_json::json!({
                "kind": "join_metallic",
                "premise_ids": ["premise.rule.lithium-water.standard-outcome", "premise.structure.lithium-metal", "premise.valence.li-h-o.initial-domain"],
                "site": "metal[1].li",
                "domain": "metal[1].metallic",
                "allocation": "donate_electron",
                "before": {"site": [-2,3,3], "domain_electrons": 0},
                "after": {"site": [-1,2,0], "domain_electrons": 1}
            }),
        );
    assert_eq!(
        rebound_error(serde_json::from_value(join).unwrap()),
        CatalogueErrorCode::InvalidOperationTemplate
    );
}

#[test]
fn corrupt_evidence_and_review_are_typed_system_errors() {
    let mut missing_evidence = envelope();
    missing_evidence.bundle.evidence.pop();
    assert_eq!(
        rebound_error(missing_evidence),
        CatalogueErrorCode::MissingEvidence
    );

    let mut provisional = envelope();
    provisional.bundle.publication = PublicationKind::Production;
    assert_eq!(
        rebound_error(provisional),
        CatalogueErrorCode::IneligibleProductionRecord
    );
}

#[test]
fn calendar_dates_are_validated_for_all_review_and_evidence_metadata() {
    let mut invalid_creation = envelope();
    invalid_creation.bundle.created.created_on = "2026-02-29".to_owned();
    assert_eq!(
        rebound_error(invalid_creation),
        CatalogueErrorCode::InvalidMetadata
    );

    let mut invalid_evidence = envelope();
    invalid_evidence.bundle.evidence[0].retrieved_on = "2026-99-99".to_owned();
    assert_eq!(
        rebound_error(invalid_evidence),
        CatalogueErrorCode::MissingEvidence
    );

    let mut invalid_reviewer = envelope();
    invalid_reviewer.bundle.premises[0].review.status = ReviewStatus::Reviewed;
    invalid_reviewer.bundle.premises[0].review.reviewers = vec![ReviewerRecord {
        reviewer: "Reviewer".to_owned(),
        reviewed_on: "0000-00-00".to_owned(),
        reference: "review.invalid-date".to_owned(),
        notes: None,
    }];
    assert_eq!(
        rebound_error(invalid_reviewer),
        CatalogueErrorCode::InvalidReview
    );
}

#[test]
fn all_four_language_observation_predicates_are_typed() {
    let mut extended = envelope();
    let rule = &mut extended.bundle.rules[0];
    rule.observation_compatibility
        .push(ObservationCompatibilityRecord {
            subject_role: "gasProduct".to_owned(),
            predicate: ObservationPredicate::Forms,
            evidence_subject: "hydrogen".to_owned(),
            value: None,
            premise_id: PremiseId::from_str("premise.observation.hydrogen-evolves").unwrap(),
        });
    rule.observation_compatibility
        .push(ObservationCompatibilityRecord {
            subject_role: "gasProduct".to_owned(),
            predicate: ObservationPredicate::Colour,
            evidence_subject: "hydrogen".to_owned(),
            value: Some("colourless".to_owned()),
            premise_id: PremiseId::from_str("premise.observation.hydrogen-evolves").unwrap(),
        });
    ValidatedCatalogueBundle::validate(rebound(extended)).unwrap();

    let mut missing_value = envelope();
    missing_value.bundle.rules[0]
        .observation_compatibility
        .push(ObservationCompatibilityRecord {
            subject_role: "gasProduct".to_owned(),
            predicate: ObservationPredicate::Colour,
            evidence_subject: "hydrogen".to_owned(),
            value: None,
            premise_id: PremiseId::from_str("premise.observation.hydrogen-evolves").unwrap(),
        });
    assert_eq!(
        rebound_error(missing_value),
        CatalogueErrorCode::InvalidRule
    );
}

#[test]
fn unsupported_lookup_is_distinct_from_invalid_bundle() {
    let catalogue = ValidatedCatalogueBundle::from_json(&fixture_bytes()).unwrap();
    let unknown_structure = StructureId::from_str("UnknownStructure").unwrap();
    let unknown_rule = ReactionRuleId::from_str("Rules.Unknown").unwrap();
    assert!(catalogue.structure(&unknown_structure).is_none());
    assert!(catalogue.rule(&unknown_rule).is_none());
    assert_eq!(
        catalogue
            .require_structure(&unknown_structure)
            .unwrap_err()
            .to_string(),
        "unsupported structure `UnknownStructure`"
    );
    assert_eq!(
        catalogue
            .require_rule(&unknown_rule)
            .unwrap_err()
            .to_string(),
        "unsupported reaction rule `Rules.Unknown`"
    );
}
