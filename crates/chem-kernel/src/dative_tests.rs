use std::{fs, path::PathBuf};

use chem_catalogue::{CatalogueEnvelope, ValidatedCatalogueBundle};
use chem_domain::{
    CovalentElectronOrigin, ElectronAllocation, ElectronState, ElectronTransition,
    StructuralOperation, StructuralOperationId, StructuralOperationInput, StructuralOperationView,
};
use serde_json::{Value, json};

use crate::{
    DerivationTrust, ObservationStatus, ValidationResult, expand_review_candidate,
    frames::project_frames, validate_review_candidate,
};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture(path: &str) -> Vec<u8> {
    fs::read(root().join(path)).unwrap()
}

fn add_premise(bundle: &mut Value, id: &str, statement: &str) {
    bundle["premises"].as_array_mut().unwrap().push(json!({
        "id": id,
        "statement": statement,
        "evidence": ["evidence.openstax.chemistry-2e"],
        "review": {"status": "provisional"},
        "rule_version": "1"
    }));
}

#[allow(clippy::too_many_lines)]
fn dative_catalogue_value(reverse_product_origin: bool) -> Value {
    let mut value: Value = serde_json::from_slice(&fixture(
        "conformance/catalogue/lithium-rule-001.catalogue.json",
    ))
    .unwrap();
    let bundle = &mut value["bundle"];
    for (id, statement) in [
        (
            "premise.structure.ammonia",
            "Ammonia is represented as neutral molecular NH3 with one nitrogen lone pair.",
        ),
        (
            "premise.structure.proton",
            "The proton reactant is represented as a monatomic H+ ion.",
        ),
        (
            "premise.structure.ammonium",
            "Ammonium retains donor-pair origin on the fourth localized N-H single bond.",
        ),
        (
            "premise.rule.ammonia-proton.ammonium",
            "Contact between ammonia and a proton has the representative structural outcome NH4+.",
        ),
        (
            "premise.valence.n-h.dative-domain",
            "The listed N and H states are the closed valence domain for ammonium formation.",
        ),
        (
            "premise.observation.ammonium-forms",
            "Ammonium is compatible with the authored observation predicate forms.",
        ),
    ] {
        add_premise(bundle, id, statement);
    }
    bundle["valence_premises"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "premise_id": "premise.valence.n-h.dative-domain",
            "neutral_valence": [
                {"element": "N", "neutral_valence_electrons": 5},
                {"element": "H", "neutral_valence_electrons": 1}
            ],
            "supported_states": [
                {"element": "N", "formal_charge": 0, "non_bonding_electrons": 2, "unpaired_electrons": 0, "covalent_bond_order_sum": 3},
                {"element": "N", "formal_charge": 1, "non_bonding_electrons": 0, "unpaired_electrons": 0, "covalent_bond_order_sum": 4},
                {"element": "H", "formal_charge": 1, "non_bonding_electrons": 0, "unpaired_electrons": 0, "covalent_bond_order_sum": 0},
                {"element": "H", "formal_charge": 0, "non_bonding_electrons": 0, "unpaired_electrons": 0, "covalent_bond_order_sum": 1}
            ],
            "metallic_domain_states": []
        }));
    bundle["structures"].as_array_mut().unwrap().extend([
        json!({
            "id": "Ammonia",
            "premise_id": "premise.structure.ammonia",
            "formula": "H3N",
            "representation": "molecular",
            "atoms": [
                {"label": "n", "element": "N", "formal_charge": 0, "non_bonding_electrons": 2, "unpaired_electrons": 0},
                {"label": "h1", "element": "H", "formal_charge": 0, "non_bonding_electrons": 0, "unpaired_electrons": 0},
                {"label": "h2", "element": "H", "formal_charge": 0, "non_bonding_electrons": 0, "unpaired_electrons": 0},
                {"label": "h3", "element": "H", "formal_charge": 0, "non_bonding_electrons": 0, "unpaired_electrons": 0}
            ],
            "bonds": [
                {"left": "n", "right": "h1", "order": "single"},
                {"left": "n", "right": "h2", "order": "single"},
                {"left": "n", "right": "h3", "order": "single"}
            ]
        }),
        json!({
            "id": "Proton",
            "premise_id": "premise.structure.proton",
            "formula": "H",
            "representation": "ion",
            "atoms": [
                {"label": "h", "element": "H", "formal_charge": 1, "non_bonding_electrons": 0, "unpaired_electrons": 0}
            ]
        }),
        json!({
            "id": "Ammonium",
            "premise_id": "premise.structure.ammonium",
            "formula": "H4N",
            "representation": "ion",
            "atoms": [
                {"label": "n", "element": "N", "formal_charge": 1, "non_bonding_electrons": 0, "unpaired_electrons": 0},
                {"label": "h1", "element": "H", "formal_charge": 0, "non_bonding_electrons": 0, "unpaired_electrons": 0},
                {"label": "h2", "element": "H", "formal_charge": 0, "non_bonding_electrons": 0, "unpaired_electrons": 0},
                {"label": "h3", "element": "H", "formal_charge": 0, "non_bonding_electrons": 0, "unpaired_electrons": 0},
                {"label": "h4", "element": "H", "formal_charge": 0, "non_bonding_electrons": 0, "unpaired_electrons": 0}
            ],
            "bonds": [
                {"left": "n", "right": "h1", "order": "single"},
                {"left": "n", "right": "h2", "order": "single"},
                {"left": "n", "right": "h3", "order": "single"},
                {
                    "left": "n", "right": "h4", "order": "single",
                    "electron_origin": {
                        "kind": "dative",
                        "donor": if reverse_product_origin { "h4" } else { "n" },
                        "acceptor": if reverse_product_origin { "n" } else { "h4" }
                    }
                }
            ]
        }),
    ]);
    bundle["rules"].as_array_mut().unwrap().push(json!({
        "id": "Rules.AmmoniaWithProton",
        "premise_ids": [
            "premise.structure.ammonia",
            "premise.structure.proton",
            "premise.structure.ammonium",
            "premise.rule.ammonia-proton.ammonium",
            "premise.valence.n-h.dative-domain",
            "premise.observation.ammonium-forms"
        ],
        "roles": {
            "ammonia": {"side": "reactant", "representation": "molecular"},
            "proton": {"side": "reactant", "representation": "ion"},
            "ammonium": {"side": "product", "representation": "ion"}
        },
        "reactant_pattern": [
            {"role": "ammonia", "structure_id": "Ammonia", "coefficient": 1},
            {"role": "proton", "structure_id": "Proton", "coefficient": 1}
        ],
        "product_pattern": [
            {"role": "ammonium", "structure_id": "Ammonium", "coefficient": 1}
        ],
        "applicability": {
            "premise_id": "premise.rule.ammonia-proton.ammonium",
            "request_relation": "contact",
            "reactant_structure_ids": ["Ammonia", "Proton"],
            "required_context": "representative educational ammonium formation"
        },
        "mapping_template": [
            {"reactant": "ammonia[1].n", "product": "ammonium[1].n", "premise_ids": ["premise.rule.ammonia-proton.ammonium", "premise.structure.ammonia", "premise.structure.ammonium"]},
            {"reactant": "ammonia[1].h1", "product": "ammonium[1].h1", "premise_ids": ["premise.rule.ammonia-proton.ammonium", "premise.structure.ammonia", "premise.structure.ammonium"]},
            {"reactant": "ammonia[1].h2", "product": "ammonium[1].h2", "premise_ids": ["premise.rule.ammonia-proton.ammonium", "premise.structure.ammonia", "premise.structure.ammonium"]},
            {"reactant": "ammonia[1].h3", "product": "ammonium[1].h3", "premise_ids": ["premise.rule.ammonia-proton.ammonium", "premise.structure.ammonia", "premise.structure.ammonium"]},
            {"reactant": "proton[1].h", "product": "ammonium[1].h4", "premise_ids": ["premise.rule.ammonia-proton.ammonium", "premise.structure.proton", "premise.structure.ammonium"]}
        ],
        "operation_template": [
            {
                "kind": "form_dative",
                "premise_ids": ["premise.rule.ammonia-proton.ammonium", "premise.structure.ammonium", "premise.valence.n-h.dative-domain"],
                "donor": "ammonia[1].n",
                "acceptor": "proton[1].h",
                "before": {"left": [0,2,0], "right": [1,0,0]},
                "after": {"left": [1,0,0], "right": [0,0,0]}
            },
            {
                "kind": "assign_product",
                "premise_ids": ["premise.rule.ammonia-proton.ammonium", "premise.structure.ammonium"],
                "atoms": ["ammonia[1].n", "ammonia[1].h1", "ammonia[1].h2", "ammonia[1].h3", "proton[1].h"],
                "product": "ammonium[1]"
            }
        ],
        "model_assumptions": {
            "event": "representative",
            "sequence": "explanatory",
            "premise_ids": ["premise.rule.ammonia-proton.ammonium"]
        },
        "observation_compatibility": [{
            "subject_role": "ammonium",
            "predicate": "forms",
            "evidence_subject": "ammonium",
            "premise_id": "premise.observation.ammonium-forms"
        }]
    }));

    value
}

fn validated_catalogue(value: Value) -> ValidatedCatalogueBundle {
    let mut envelope: CatalogueEnvelope = serde_json::from_value(value).unwrap();
    envelope.digest = envelope.computed_digest().unwrap();
    ValidatedCatalogueBundle::validate(envelope).unwrap()
}

fn dative_catalogue(reverse_product_origin: bool) -> ValidatedCatalogueBundle {
    validated_catalogue(dative_catalogue_value(reverse_product_origin))
}

fn cleavage_catalogue() -> ValidatedCatalogueBundle {
    let mut value = dative_catalogue_value(false);
    let bundle = &mut value["bundle"];
    add_premise(
        bundle,
        "premise.observation.dissociation-products-form",
        "Ammonia is compatible with the authored observation predicate forms.",
    );
    *bundle["rules"].as_array_mut().unwrap().last_mut().unwrap() = json!({
        "id": "Rules.AmmoniumDissociation",
        "premise_ids": [
            "premise.structure.ammonia",
            "premise.structure.proton",
            "premise.structure.ammonium",
            "premise.rule.ammonia-proton.ammonium",
            "premise.valence.n-h.dative-domain",
            "premise.observation.dissociation-products-form"
        ],
        "roles": {
            "ammonium": {"side": "reactant", "representation": "ion"},
            "ammonia": {"side": "product", "representation": "molecular"},
            "proton": {"side": "product", "representation": "ion"}
        },
        "reactant_pattern": [
            {"role": "ammonium", "structure_id": "Ammonium", "coefficient": 1}
        ],
        "product_pattern": [
            {"role": "ammonia", "structure_id": "Ammonia", "coefficient": 1},
            {"role": "proton", "structure_id": "Proton", "coefficient": 1}
        ],
        "applicability": {
            "premise_id": "premise.rule.ammonia-proton.ammonium",
            "request_relation": "contact",
            "reactant_structure_ids": ["Ammonium"],
            "required_context": "representative educational reversal used to verify dative cleavage"
        },
        "mapping_template": [
            {"reactant": "ammonium[1].n", "product": "ammonia[1].n", "premise_ids": ["premise.rule.ammonia-proton.ammonium", "premise.structure.ammonia", "premise.structure.ammonium"]},
            {"reactant": "ammonium[1].h1", "product": "ammonia[1].h1", "premise_ids": ["premise.rule.ammonia-proton.ammonium", "premise.structure.ammonia", "premise.structure.ammonium"]},
            {"reactant": "ammonium[1].h2", "product": "ammonia[1].h2", "premise_ids": ["premise.rule.ammonia-proton.ammonium", "premise.structure.ammonia", "premise.structure.ammonium"]},
            {"reactant": "ammonium[1].h3", "product": "ammonia[1].h3", "premise_ids": ["premise.rule.ammonia-proton.ammonium", "premise.structure.ammonia", "premise.structure.ammonium"]},
            {"reactant": "ammonium[1].h4", "product": "proton[1].h", "premise_ids": ["premise.rule.ammonia-proton.ammonium", "premise.structure.proton", "premise.structure.ammonium"]}
        ],
        "operation_template": [
            {
                "kind": "cleave_dative",
                "premise_ids": ["premise.rule.ammonia-proton.ammonium", "premise.structure.ammonium", "premise.valence.n-h.dative-domain"],
                "donor": "ammonium[1].n",
                "acceptor": "ammonium[1].h4",
                "allocation": {"heterolytic_to": "ammonium[1].n"},
                "before": {"left": [1,0,0], "right": [0,0,0]},
                "after": {"left": [0,2,0], "right": [1,0,0]}
            },
            {
                "kind": "assign_product",
                "premise_ids": ["premise.rule.ammonia-proton.ammonium", "premise.structure.ammonia"],
                "atoms": ["ammonium[1].n", "ammonium[1].h1", "ammonium[1].h2", "ammonium[1].h3"],
                "product": "ammonia[1]"
            },
            {
                "kind": "assign_product",
                "premise_ids": ["premise.rule.ammonia-proton.ammonium", "premise.structure.proton"],
                "atoms": ["ammonium[1].h4"],
                "product": "proton[1]"
            }
        ],
        "model_assumptions": {
            "event": "representative",
            "sequence": "explanatory",
            "premise_ids": ["premise.rule.ammonia-proton.ammonium"]
        },
        "observation_compatibility": [{
            "subject_role": "ammonia",
            "predicate": "forms",
            "evidence_subject": "ammonia",
            "premise_id": "premise.observation.dissociation-products-form"
        }]
    });
    validated_catalogue(value)
}

const SOURCE: &str = "chems 1\
\nuse catalog ChemSpec.Theoretical@1\
\n\nreaction AmmoniumFormation where\
\n  reactants\
\n    ammonia := 1 of Ammonia\
\n    proton := 1 of Proton\
\n\n  products\
\n    ammonium := 1 of Ammonium\
\n\n  equation\
\n    H3N[molecular] + H[ion] -> H4N[ion]\
\n\n  model\
\n    event := representative\
\n    sequence := explanatory\
\n\n  observe from Evidence.Ammonium@1\
\n    product ammonium forms claim R1\
\n\n  by\
\n    apply Rules.AmmoniaWithProton\
\n      ammonia := ammonia\
\n      proton := proton\
\n      ammonium := ammonium\n";

const CLEAVAGE_SOURCE: &str = "chems 1\
\nuse catalog ChemSpec.Theoretical@1\
\n\nreaction AmmoniumDissociation where\
\n  reactants\
\n    ammonium := 1 of Ammonium\
\n\n  products\
\n    ammonia := 1 of Ammonia\
\n    proton := 1 of Proton\
\n\n  equation\
\n    H4N[ion] -> H3N[molecular] + H[ion]\
\n\n  model\
\n    event := representative\
\n    sequence := explanatory\
\n\n  observe from Evidence.Dissociation@1\
\n    product ammonia forms claim R1\
\n\n  by\
\n    apply Rules.AmmoniumDissociation\
\n      ammonium := ammonium\
\n      ammonia := ammonia\
\n      proton := proton\n";

fn evidence() -> Vec<u8> {
    serde_json::to_vec(&json!({
        "schema_version": 1,
        "id": "Evidence.Ammonium@1",
        "claims": [{
            "id": "R1",
            "subject_role": "product",
            "subject": "ammonium",
            "predicate": "forms",
            "sources": ["S1"]
        }],
        "sources": [{
            "id": "S1",
            "title": "Ammonium formation review fixture",
            "publisher": "ChemSpec",
            "url": "https://example.edu/ammonium",
            "supports": ["R1"]
        }]
    }))
    .unwrap()
}

fn cleavage_evidence() -> Vec<u8> {
    serde_json::to_vec(&json!({
        "schema_version": 1,
        "id": "Evidence.Dissociation@1",
        "claims": [{
            "id": "R1",
            "subject_role": "product",
            "subject": "ammonia",
            "predicate": "forms",
            "sources": ["S1"]
        }],
        "sources": [{
            "id": "S1",
            "title": "Dative cleavage review fixture",
            "publisher": "ChemSpec",
            "url": "https://example.edu/dative-cleavage",
            "supports": ["R1"]
        }]
    }))
    .unwrap()
}

fn expanded(reverse_product_origin: bool) -> crate::ExpandedStructuralReaction {
    expand_review_candidate(
        "conformance/dative/ammonium.chems",
        SOURCE,
        &dative_catalogue(reverse_product_origin),
        &evidence(),
    )
    .unwrap()
}

#[test]
fn dative_journey_expands_validates_and_projects_exact_direction() {
    let expanded = expanded(false);
    assert_eq!(expanded.operations.len(), 2);
    match expanded.operations[0].operation.view() {
        StructuralOperationView::FormDative {
            donor, acceptor, ..
        } => {
            assert_eq!(donor.as_str(), "ammonia[1].n");
            assert_eq!(acceptor.as_str(), "proton[1].h");
        }
        operation => panic!("unexpected first operation: {operation:?}"),
    }
    let derivation = validate_review_candidate(&expanded, &dative_catalogue(false)).unwrap();
    assert_eq!(derivation.trust(), DerivationTrust::ReviewCandidate);
    assert_eq!(
        derivation.result(),
        ValidationResult::ValidatedWithAssumptions
    );
    assert_eq!(derivation.states().len(), 3);
    let final_bond = derivation.states()[2]
        .graph()
        .covalent_bonds()
        .values()
        .find(|bond| !bond.electron_origin().is_shared())
        .unwrap();
    assert_eq!(
        final_bond.electron_origin(),
        &CovalentElectronOrigin::Dative {
            donor: "ammonia[1].n".parse().unwrap(),
            acceptor: "proton[1].h".parse().unwrap(),
        }
    );

    let frames = project_frames(&derivation).unwrap();
    assert_eq!(frames.frames().len(), 3);
    let dative_edge = frames.frames()[1]
        .covalent_edges()
        .values()
        .find(|edge| !edge.electron_origin.is_shared())
        .unwrap();
    assert_eq!(dative_edge.electron_origin, *final_bond.electron_origin());
    let change = serde_json::to_value(frames.frames()[1].changes()).unwrap();
    assert_eq!(change[0]["kind"], "electron_state");
    assert!(change.as_array().unwrap().iter().any(|item| {
        item["kind"] == "covalent"
            && item["after_electron_origin"]["electron_origin"] == "dative"
            && item["after_electron_origin"]["donor"] == "ammonia[1].n"
            && item["after_electron_origin"]["acceptor"] == "proton[1].h"
    }));
    assert_eq!(
        frames.frames()[1].observations()[0].status,
        ObservationStatus::Pending
    );
    assert_eq!(
        frames.frames()[2].observations()[0].status,
        ObservationStatus::Active
    );
}

#[test]
fn dative_cleavage_expands_validates_and_projects_exact_direction() {
    let catalogue = cleavage_catalogue();
    let expanded = expand_review_candidate(
        "conformance/dative/ammonium-dissociation.chems",
        CLEAVAGE_SOURCE,
        &catalogue,
        &cleavage_evidence(),
    )
    .unwrap();
    assert_eq!(expanded.operations.len(), 3);
    match expanded.operations[0].operation.view() {
        StructuralOperationView::CleaveDative {
            donor,
            acceptor,
            allocation,
            ..
        } => {
            assert_eq!(donor.as_str(), "ammonium[1].n");
            assert_eq!(acceptor.as_str(), "ammonium[1].h4");
            assert_eq!(
                allocation,
                &ElectronAllocation::HeterolyticTo("ammonium[1].n".parse().unwrap())
            );
        }
        operation => panic!("unexpected first operation: {operation:?}"),
    }
    let operation_premises = expanded.operations[0]
        .provenance
        .catalogue
        .iter()
        .flat_map(|origin| origin.premises.iter().map(chem_domain::DeclaredId::as_str))
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        operation_premises,
        [
            "premise.rule.ammonia-proton.ammonium",
            "premise.structure.ammonium",
            "premise.valence.n-h.dative-domain",
        ]
        .into_iter()
        .collect()
    );

    let derivation = validate_review_candidate(&expanded, &catalogue).unwrap();
    assert_eq!(derivation.states().len(), 4);
    let cleaved = &derivation.states()[1];
    assert_eq!(cleaved.graph().covalent_bonds().len(), 3);
    assert!(
        cleaved
            .graph()
            .covalent_bonds()
            .values()
            .all(|bond| bond.electron_origin().is_shared())
    );
    assert_eq!(
        cleaved.graph().atoms()[&"ammonium[1].n".parse().unwrap()].electrons(),
        ElectronState::new(0, 2, 0).unwrap()
    );
    assert_eq!(
        cleaved.graph().atoms()[&"ammonium[1].h4".parse().unwrap()].electrons(),
        ElectronState::new(1, 0, 0).unwrap()
    );

    let frames = project_frames(&derivation).unwrap();
    let cleavage_changes = serde_json::to_value(frames.frames()[1].changes()).unwrap();
    assert!(cleavage_changes.as_array().unwrap().iter().any(|item| {
        item["kind"] == "covalent"
            && item["before"] == "single"
            && item["after"].is_null()
            && item["before_electron_origin"]["electron_origin"] == "dative"
            && item["before_electron_origin"]["donor"] == "ammonium[1].n"
            && item["before_electron_origin"]["acceptor"] == "ammonium[1].h4"
            && item.get("after_electron_origin").is_none()
    }));
}

#[test]
fn final_product_direction_is_proof_relevant() {
    let expanded = expanded(true);
    let error = validate_review_candidate(&expanded, &dative_catalogue(true)).unwrap_err();
    assert_eq!(error.code(), "CHEMS-K053");
}

#[test]
fn shared_cleavage_cannot_consume_a_dative_edge() {
    let catalogue = dative_catalogue(false);
    let mut expanded = expanded(false);
    expanded.operations[1].operation = StructuralOperation::new(
        StructuralOperationId::new("operation[2]").unwrap(),
        StructuralOperationInput::CleaveCovalent {
            left: "ammonia[1].n".parse().unwrap(),
            right: "proton[1].h".parse().unwrap(),
            expected_order: chem_domain::BondOrder::Single,
            allocation: ElectronAllocation::HeterolyticTo("ammonia[1].n".parse().unwrap()),
            transitions: vec![
                ElectronTransition::new(
                    "ammonia[1].n".parse().unwrap(),
                    ElectronState::new(1, 0, 0).unwrap(),
                    ElectronState::new(0, 2, 0).unwrap(),
                ),
                ElectronTransition::new(
                    "proton[1].h".parse().unwrap(),
                    ElectronState::new(0, 0, 0).unwrap(),
                    ElectronState::new(1, 0, 0).unwrap(),
                ),
            ],
        },
    )
    .unwrap();
    let error = validate_review_candidate(&expanded, &catalogue).unwrap_err();
    assert_eq!(error.code(), "CHEMS-K020");
    assert!(error.to_string().contains("shared covalent bond identity"));
}

#[test]
fn dative_cleavage_requires_exact_direction_and_rejects_shared_edges() {
    let catalogue = dative_catalogue(false);
    let mut reversed = expanded(false);
    reversed.operations[1].operation = StructuralOperation::new(
        StructuralOperationId::new("operation[2]").unwrap(),
        StructuralOperationInput::CleaveDative {
            donor: "proton[1].h".parse().unwrap(),
            acceptor: "ammonia[1].n".parse().unwrap(),
            allocation: ElectronAllocation::HeterolyticTo("ammonia[1].n".parse().unwrap()),
            transitions: vec![
                ElectronTransition::new(
                    "proton[1].h".parse().unwrap(),
                    ElectronState::new(0, 0, 0).unwrap(),
                    ElectronState::new(1, 0, 0).unwrap(),
                ),
                ElectronTransition::new(
                    "ammonia[1].n".parse().unwrap(),
                    ElectronState::new(1, 0, 0).unwrap(),
                    ElectronState::new(0, 2, 0).unwrap(),
                ),
            ],
        },
    )
    .unwrap();
    let error = validate_review_candidate(&reversed, &catalogue).unwrap_err();
    assert_eq!(error.code(), "CHEMS-K020");
    assert!(error.to_string().contains("directed dative bond identity"));

    let source = fixture("conformance/expansion/canonical-expansion-001.chems");
    let lithium_catalogue = ValidatedCatalogueBundle::from_json(&fixture(
        "conformance/catalogue/lithium-rule-001.catalogue.json",
    ))
    .unwrap();
    let mut shared = expand_review_candidate(
        "conformance/expansion/canonical-expansion-001.chems",
        std::str::from_utf8(&source).unwrap(),
        &lithium_catalogue,
        &fixture("conformance/observations/lithium-observations-001.input.json"),
    )
    .unwrap();
    let original = match shared.operations[2].operation.view() {
        StructuralOperationView::CleaveCovalent {
            left,
            right,
            allocation,
            transitions,
            ..
        } => (
            left.clone(),
            right.clone(),
            allocation.clone(),
            transitions.clone(),
        ),
        _ => unreachable!(),
    };
    shared.operations[2].operation = StructuralOperation::new(
        StructuralOperationId::new("operation[3]").unwrap(),
        StructuralOperationInput::CleaveDative {
            donor: original.0,
            acceptor: original.1,
            allocation: original.2,
            transitions: original.3.into_values().collect(),
        },
    )
    .unwrap();
    let error = validate_review_candidate(&shared, &lithium_catalogue).unwrap_err();
    assert_eq!(error.code(), "CHEMS-K020");
    assert!(error.to_string().contains("directed dative bond identity"));
}
