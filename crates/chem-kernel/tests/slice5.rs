use std::{collections::BTreeSet, fs, path::PathBuf};

use chem_catalogue::ValidatedCatalogueBundle;
use chem_domain::{
    AtomGroup, AtomGroupId, AtomId, AtomMapping, BondOrder, ElectronAllocation, ElectronState,
    ElectronTransition, IonicAssociationId, MetallicDomainId, MetallicJoinAllocation, ReactionSide,
    RepresentationKind, StructuralGraph, StructuralOperation, StructuralOperationId,
    StructuralOperationInput, StructuralOperationView, StructureDefinition, StructureInstance,
    StructureInstanceId,
};
use chem_kernel::{
    DerivationTrust, ExpandedOperation, KernelFailureClass, ValidationResult,
    expand_review_candidate, validate_review_candidate,
};
use serde_json::{Value, json};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture(path: &str) -> Vec<u8> {
    fs::read(workspace_root().join(path)).expect("fixture should be readable")
}

fn catalogue() -> ValidatedCatalogueBundle {
    ValidatedCatalogueBundle::from_json(&fixture(
        "conformance/catalogue/lithium-rule-001.catalogue.json",
    ))
    .unwrap()
}

fn expansion() -> chem_kernel::ExpandedStructuralReaction {
    let source = fixture("conformance/expansion/canonical-expansion-001.chems");
    expand_review_candidate(
        "conformance/expansion/canonical-expansion-001.chems",
        std::str::from_utf8(&source).unwrap(),
        &catalogue(),
        &fixture("conformance/observations/lithium-observations-001.input.json"),
    )
    .unwrap()
}

#[test]
fn canonical_review_candidate_executes_every_operation_immutably() {
    let expanded = expansion();
    let derivation = validate_review_candidate(&expanded, &catalogue()).unwrap();
    assert_eq!(
        derivation.result(),
        ValidationResult::ValidatedWithAssumptions
    );
    assert_eq!(derivation.trust(), DerivationTrust::ReviewCandidate);
    assert_eq!(derivation.expanded(), &expanded);
    assert_eq!(derivation.states().len(), expanded.operations.len() + 1);
    assert_eq!(derivation.states()[0].ordinal(), 0);
    assert!(derivation.states()[0].operation().is_none());
    assert_eq!(
        derivation.states().last().unwrap().ordinal(),
        u32::try_from(expanded.operations.len()).unwrap()
    );
    let digests = derivation
        .states()
        .iter()
        .map(chem_kernel::StructuralState::digest)
        .collect::<BTreeSet<_>>();
    assert_eq!(digests.len(), derivation.states().len());
    assert!(!derivation.canonical_json().unwrap().is_empty());
    let repeated = validate_review_candidate(&expanded, &catalogue()).unwrap();
    assert_eq!(
        derivation.canonical_json().unwrap(),
        repeated.canonical_json().unwrap()
    );
    assert!(
        String::from_utf8(derivation.canonical_json().unwrap())
            .unwrap()
            .contains("\"trust\":\"review_candidate\"")
    );
}

#[test]
fn complete_canonical_derivation_is_byte_exact() {
    let derivation = validate_review_candidate(&expansion(), &catalogue()).unwrap();
    let expected = fixture("conformance/validation-kernel/canonical-kernel-001.derivation.json");
    let expected = expected.strip_suffix(b"\n").unwrap_or(&expected);
    assert_eq!(derivation.canonical_json().unwrap(), expected);
}

#[test]
#[allow(clippy::too_many_lines)]
fn every_immutable_state_matches_the_independently_authored_derivation() {
    let expanded = expansion();
    let derivation = validate_review_candidate(&expanded, &catalogue()).unwrap();
    let oracle: Value = serde_json::from_slice(&fixture(
        "conformance/validation-kernel/canonical-kernel-001.expected.json",
    ))
    .unwrap();
    assert_eq!(oracle["schema_version"], 1);
    assert_eq!(oracle["source"], expanded.claim.source.name);
    assert_eq!(
        oracle["catalogue"],
        format!(
            "{}@{}",
            expanded.claim.catalogue.name, expanded.claim.catalogue.version
        )
    );
    assert_eq!(oracle["rule"], expanded.claim.rule.rule.to_string());
    assert_eq!(
        oracle["model"],
        json!({ "event": "representative", "sequence": "explanatory" })
    );
    assert_eq!(
        oracle["state_encoding"],
        json!({
            "atom_tuple": ["formal_charge", "non_bonding_electrons", "unpaired_electrons"],
            "covalent_edge_tuple": ["left_atom", "right_atom", "order"],
            "metallic_domain_tuple": ["domain_id", "site_atoms", "delocalized_electrons"],
            "ionic_association_tuple": ["left_atoms", "right_atoms", "kind"],
            "product_assignment_tuple": ["product_instance", "atoms"]
        })
    );
    let atom_elements = derivation.states()[0]
        .graph()
        .atoms()
        .values()
        .map(|atom| (atom.id().to_string(), json!(atom.element())))
        .collect::<serde_json::Map<_, _>>();
    assert_eq!(oracle["atom_elements"], Value::Object(atom_elements));
    assert_eq!(
        oracle_items(&oracle["instances"]["reactants"]),
        expanded
            .reactant_instances
            .values()
            .map(|item| canonical_item(&json!(item.instance.id())))
            .collect()
    );
    assert_eq!(
        oracle_items(&oracle["instances"]["products"]),
        expanded
            .product_instances
            .values()
            .map(|item| canonical_item(&json!(item.instance.id())))
            .collect()
    );
    let mapping = expanded
        .mapping
        .entries()
        .iter()
        .map(|(from, to)| canonical_item(&json!([from, to])))
        .collect::<BTreeSet<_>>();
    assert_eq!(oracle_items(&oracle["mapping"]), mapping);
    let expected_states = oracle["states"].as_array().unwrap();
    assert_eq!(derivation.states().len(), expected_states.len());

    for (state, expected) in derivation.states().iter().zip(expected_states) {
        assert_eq!(expected["id"], format!("state[{}]", state.ordinal()));

        let atoms = state
            .graph()
            .atoms()
            .values()
            .map(|atom| {
                (
                    atom.id().to_string(),
                    json!([
                        atom.electrons().formal_charge(),
                        atom.electrons().non_bonding_electrons(),
                        atom.electrons().unpaired_electrons()
                    ]),
                )
            })
            .collect::<serde_json::Map<_, _>>();
        assert_eq!(Value::Object(atoms), expected["atoms"]);

        let bonds = state
            .graph()
            .covalent_bonds()
            .values()
            .map(|bond| {
                let order = serde_json::to_value(bond.order()).unwrap();
                let mut endpoints = [bond.left().to_string(), bond.right().to_string()];
                endpoints.sort();
                canonical_item(&json!([endpoints[0], endpoints[1], order]))
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(bonds, oracle_edges(&expected["covalent_edges"]));

        let domains = state
            .graph()
            .metallic_domains()
            .values()
            .map(|domain| {
                canonical_item(&json!([
                    domain.id(),
                    domain.sites(),
                    domain.delocalized_electrons()
                ]))
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(domains, oracle_items(&expected["metallic_domains"]));

        let ionic = state
            .graph()
            .ionic_associations()
            .values()
            .map(|association| {
                let components = association
                    .components()
                    .iter()
                    .map(|component| {
                        state.graph().groups()[component]
                            .atoms()
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>();
                canonical_ionic(components)
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(ionic, oracle_ionic_items(&expected["ionic_associations"]));

        let assignments = state
            .product_assignments()
            .iter()
            .map(|(product, atoms)| {
                canonical_assignment(
                    product.as_str(),
                    atoms.iter().map(ToString::to_string).collect(),
                )
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(
            assignments,
            oracle_assignments(&expected["product_assignments"])
        );

        let ledger = state.ledger();
        assert_eq!(
            expected["ledger"],
            json!({
                "atom_local_non_bonding": ledger.atom_local_non_bonding,
                "covalent_bond_electrons": ledger.covalent_bond_electrons,
                "metallic_domain_electrons": ledger.metallic_domain_electrons,
                "total_explicit_valence_electrons": ledger.total_explicit_valence_electrons,
                "atom_formal_charge_sum": ledger.atom_formal_charge_sum,
                "system_net_charge": ledger.system_net_charge,
            })
        );
    }

    let expected_operations = oracle["operations"].as_array().unwrap();
    for (index, (operation, expanded_operation)) in expected_operations
        .iter()
        .zip(&expanded.operations)
        .enumerate()
    {
        assert_eq!(operation["before_state"], format!("state[{index}]"));
        assert_eq!(operation["after_state"], format!("state[{}]", index + 1));
        assert_eq!(
            derivation.states()[index + 1].operation().unwrap().as_str(),
            operation["id"].as_str().unwrap()
        );
        assert_eq!(operation, &operation_oracle(expanded_operation));
    }
    assert_eq!(
        oracle["review"],
        json!({
            "status": "technical-review-candidate",
            "external_chemist_review": "pending",
            "trusted_output": false
        })
    );
}

#[allow(clippy::too_many_lines)]
fn operation_oracle(operation: &ExpandedOperation) -> Value {
    let before = format!("state[{}]", operation.ordinal - 1);
    let after = format!("state[{}]", operation.ordinal);
    let id = operation.operation.id();
    match operation.operation.view() {
        StructuralOperationView::CleaveCovalent {
            left,
            right,
            expected_order,
            allocation,
            transitions,
        } => json!({
            "id": id, "kind": "cleave_covalent", "before_state": before,
            "after_state": after, "edge": [left, right, expected_order],
            "allocation": allocation,
            "endpoints_after": {
                "left": electron_tuple(transitions[left].after()),
                "right": electron_tuple(transitions[right].after())
            }
        }),
        StructuralOperationView::FormCovalent {
            left,
            right,
            order,
            transitions,
        } => json!({
            "id": id, "kind": "form_covalent", "before_state": before,
            "after_state": after, "edge": [left, right, order],
            "electron_contribution": operation.electron_contribution,
            "endpoints_after": {
                "left": electron_tuple(transitions[left].after()),
                "right": electron_tuple(transitions[right].after())
            }
        }),
        StructuralOperationView::CleaveDative {
            donor,
            acceptor,
            allocation,
            transitions,
        } => json!({
            "id": id, "kind": "cleave_dative", "before_state": before,
            "after_state": after, "donor": donor, "acceptor": acceptor,
            "allocation": allocation,
            "endpoints_after": {
                "donor": electron_tuple(transitions[donor].after()),
                "acceptor": electron_tuple(transitions[acceptor].after())
            }
        }),
        StructuralOperationView::FormDative {
            donor,
            acceptor,
            transitions,
        } => json!({
            "id": id, "kind": "form_dative", "before_state": before,
            "after_state": after, "donor": donor, "acceptor": acceptor,
            "endpoints_after": {
                "donor": electron_tuple(transitions[donor].after()),
                "acceptor": electron_tuple(transitions[acceptor].after())
            }
        }),
        StructuralOperationView::ChangeCovalent {
            left,
            right,
            old_order,
            new_order,
            allocation,
            transitions,
        } => json!({
            "id": id, "kind": "change_covalent", "before_state": before,
            "after_state": after, "edge": [left, right, old_order, new_order],
            "allocation": allocation,
            "endpoints_after": {
                "left": electron_tuple(transitions[left].after()),
                "right": electron_tuple(transitions[right].after())
            }
        }),
        StructuralOperationView::AssociateIonic { .. } => {
            let components = &operation.ionic_components;
            json!({
                "id": id, "kind": "associate_ionic", "before_state": before,
                "after_state": after,
                "left": components[0].group.atoms(),
                "right": components[1].group.atoms(),
                "component_charges": [
                    components[0].expected_charge,
                    components[1].expected_charge
                ]
            })
        }
        StructuralOperationView::DissociateIonic { association } => json!({
            "id": id, "kind": "dissociate_ionic", "before_state": before,
            "after_state": after, "association": association
        }),
        StructuralOperationView::ReleaseMetallic {
            site,
            domain,
            allocation,
            transition,
            ..
        } => json!({
            "id": id, "kind": "release_metallic", "before_state": before,
            "after_state": after, "site": site, "domain": domain,
            "allocation": allocation, "endpoint_after": electron_tuple(transition.after())
        }),
        StructuralOperationView::JoinMetallic {
            site,
            domain,
            allocation,
            transition,
            ..
        } => json!({
            "id": id, "kind": "join_metallic", "before_state": before,
            "after_state": after, "site": site, "domain": domain,
            "allocation": allocation, "endpoint_after": electron_tuple(transition.after())
        }),
        StructuralOperationView::TransferElectron {
            donor,
            acceptor,
            count,
            transitions,
        } => json!({
            "id": id, "kind": "transfer_electron", "before_state": before,
            "after_state": after, "count": count, "donor": donor, "acceptor": acceptor,
            "endpoints_after": {
                "donor": electron_tuple(transitions[donor].after()),
                "acceptor": electron_tuple(transitions[acceptor].after())
            }
        }),
        StructuralOperationView::AssignProduct { atoms, product } => json!({
            "id": id, "kind": "assign_product", "before_state": before,
            "after_state": after, "atoms": atoms, "product": product
        }),
    }
}

fn electron_tuple(state: ElectronState) -> Value {
    json!([
        state.formal_charge(),
        state.non_bonding_electrons(),
        state.unpaired_electrons()
    ])
}

#[test]
fn every_closed_operation_kind_enforces_its_immediate_precondition() {
    let canonical = expansion();
    let expected: Value = serde_json::from_slice(&fixture(
        "conformance/validation-kernel/kernel-negative-001.input.json",
    ))
    .unwrap();
    let mut cases = Vec::new();

    let mut cleave = canonical.clone();
    cleave.operations[0] = custom_cleave(1);
    cases.push(("cleave", cleave, 1));

    let mut form = canonical.clone();
    form.operations[0] = reidentify(&canonical.operations[6], 1);
    cases.push(("form", form, 1));

    let mut change = canonical.clone();
    change.operations[0] = custom_change(1);
    cases.push(("change", change, 1));

    let mut associate = canonical.clone();
    associate.operations[0] = reidentify(&canonical.operations[7], 1);
    cases.push(("associate", associate, 1));

    let mut dissociate = canonical.clone();
    dissociate.operations[0] = custom_dissociate(1);
    cases.push(("dissociate", dissociate, 1));

    let mut release = canonical.clone();
    release.operations[1] = reidentify(&canonical.operations[0], 2);
    cases.push(("release", release, 2));

    let mut join = canonical.clone();
    join.operations[0] = custom_join(1);
    cases.push(("join", join, 1));

    let mut transfer = canonical.clone();
    transfer.operations[0] = reidentify(&canonical.operations[4], 1);
    cases.push(("transfer", transfer, 1));

    let mut assign = canonical;
    assign.operations[0] = custom_assign(1);
    cases.push(("assign", assign, 1));

    let catalogue = catalogue();
    for (kind, expanded, ordinal) in cases {
        let expectation = expected["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["mutation"] == kind)
            .unwrap();
        let error = validate_review_candidate(&expanded, &catalogue)
            .expect_err(kind)
            .clone();
        assert_eq!(
            error.class(),
            KernelFailureClass::InvalidExpansion,
            "{kind}"
        );
        assert_eq!(
            error.code(),
            expectation["code"].as_str().unwrap(),
            "{kind}: {error}"
        );
        assert_eq!(
            error.operation(),
            expectation["operation"]
                .as_u64()
                .map(|value| u32::try_from(value).unwrap()),
            "{kind}"
        );
        assert_eq!(error.operation(), Some(ordinal), "{kind}");
    }
    assert_eq!(expected["cases"].as_array().unwrap().len(), 52);
}

#[test]
#[allow(clippy::too_many_lines)]
fn sequence_mapping_products_premises_and_staleness_are_mandatory() {
    let catalogue = catalogue();
    let expected: Value = serde_json::from_slice(&fixture(
        "conformance/validation-kernel/kernel-negative-001.input.json",
    ))
    .unwrap();

    let mut reordered = expansion();
    reordered.operations.swap(0, 1);
    let error = validate_review_candidate(&reordered, &catalogue).unwrap_err();
    assert_negative(&expected, "operation_sequence", &error);

    let mut missing_assignment = expansion();
    missing_assignment.operations.pop();
    let error = validate_review_candidate(&missing_assignment, &catalogue).unwrap_err();
    assert_negative(&expected, "product_partition", &error);

    let mut remapped = expansion();
    let mut entries = remapped.mapping.entries().clone();
    let first: AtomId = "lithium[1].li".parse().unwrap();
    let second: AtomId = "lithium[2].li".parse().unwrap();
    let first_destination = entries[&first].clone();
    let second_destination = entries[&second].clone();
    entries.insert(first, second_destination);
    entries.insert(second, first_destination);
    let reactants = ReactionSide::new(
        remapped
            .reactant_instances
            .values()
            .map(|instance| instance.instance.clone()),
    )
    .unwrap();
    let products = ReactionSide::new(
        remapped
            .product_instances
            .values()
            .map(|instance| instance.instance.clone()),
    )
    .unwrap();
    remapped.mapping = AtomMapping::new(
        remapped.mapping.id().clone(),
        entries,
        &reactants,
        &products,
    )
    .unwrap();
    let error = validate_review_candidate(&remapped, &catalogue).unwrap_err();
    assert_negative(&expected, "remapping", &error);

    let mut grouped_product = expansion();
    let hydrogen = grouped_product
        .product_instances
        .get_mut("hydrogen[1]")
        .unwrap();
    let graph = hydrogen.instance.graph();
    let group = AtomGroup::new(
        "educational_group".parse::<AtomGroupId>().unwrap(),
        graph.atoms().keys().cloned(),
    )
    .unwrap();
    let graph_with_group = StructuralGraph::new(
        graph.atoms().values().cloned(),
        graph.covalent_bonds().values().cloned(),
        graph.groups().values().cloned().chain([group]),
        graph.ionic_associations().values().cloned(),
        graph.metallic_domains().values().cloned(),
    )
    .unwrap();
    let definition = StructureDefinition::new(
        hydrogen.instance.structure().clone(),
        graph_with_group.element_inventory(),
        RepresentationKind::Molecular,
        graph_with_group,
    )
    .unwrap();
    hydrogen.instance = StructureInstance::instantiate(
        hydrogen.instance.id().clone(),
        &definition,
        graph
            .atoms()
            .keys()
            .cloned()
            .map(|atom| (atom.clone(), atom)),
    )
    .unwrap();
    let error = validate_review_candidate(&grouped_product, &catalogue).unwrap_err();
    assert_negative(&expected, "final_groups", &error);

    let mut no_valence = expansion();
    no_valence
        .premises
        .remove(&"premise.valence.li-h-o.initial-domain".parse().unwrap());
    let error = validate_review_candidate(&no_valence, &catalogue).unwrap_err();
    assert_eq!(error.class(), KernelFailureClass::InvalidExpansion);
    assert_negative(&expected, "missing_valence_premise", &error);

    let mut unsupported_valence = expansion();
    unsupported_valence.operations[2] = unsupported_cleavage(3);
    let error = validate_review_candidate(&unsupported_valence, &catalogue).unwrap_err();
    assert_eq!(error.class(), KernelFailureClass::UnsupportedState);
    assert_negative(&expected, "unsupported_valence", &error);

    let mut stale_expansion = expansion();
    stale_expansion.claim.catalogue.digest =
        chem_domain::ContentDigest::sha256(b"different catalogue");
    let error = validate_review_candidate(&stale_expansion, &catalogue).unwrap_err();
    assert_eq!(error.class(), KernelFailureClass::StaleInput);
    assert_negative(&expected, "stale_catalogue", &error);

    let expanded = expansion();
    let derivation = validate_review_candidate(&expanded, &catalogue).unwrap();
    assert!(
        derivation
            .ensure_current(
                expanded.claim.source.bytes_digest,
                expanded.claim.source.semantic_digest,
                catalogue.digest(),
            )
            .is_ok()
    );
    let error = derivation
        .ensure_current(
            expanded.claim.source.bytes_digest,
            chem_domain::ContentDigest::sha256(b"edited source semantics"),
            catalogue.digest(),
        )
        .unwrap_err();
    assert_eq!(error.class(), KernelFailureClass::StaleInput);
    assert_negative(&expected, "stale_source", &error);
    let mut edited_source = String::from_utf8(fixture(
        "conformance/expansion/canonical-expansion-001.chems",
    ))
    .unwrap();
    edited_source.push_str("\n-- comment-only edit\n");
    let edited = expand_review_candidate(
        "conformance/expansion/canonical-expansion-001.chems",
        &edited_source,
        &catalogue,
        &fixture("conformance/observations/lithium-observations-001.input.json"),
    )
    .unwrap();
    assert_eq!(
        edited.claim.source.semantic_digest,
        expanded.claim.source.semantic_digest
    );
    assert_ne!(
        edited.claim.source.bytes_digest,
        expanded.claim.source.bytes_digest
    );
    let error = derivation
        .ensure_current(
            edited.claim.source.bytes_digest,
            edited.claim.source.semantic_digest,
            catalogue.digest(),
        )
        .unwrap_err();
    assert_negative(&expected, "stale_source_bytes", &error);
    assert!(
        derivation
            .ensure_expansion_current(expanded.semantic_digest().unwrap())
            .is_ok()
    );
    let error = derivation
        .ensure_expansion_current(chem_domain::ContentDigest::sha256(b"changed evidence"))
        .unwrap_err();
    assert_negative(&expected, "stale_expansion", &error);
    let error = derivation
        .ensure_current(
            expanded.claim.source.bytes_digest,
            expanded.claim.source.semantic_digest,
            chem_domain::ContentDigest::sha256(b"new catalogue"),
        )
        .unwrap_err();
    assert_negative(&expected, "stale_catalogue", &error);
}

fn assert_negative(expected: &Value, mutation: &str, error: &chem_kernel::KernelError) {
    let case = expected["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["mutation"] == mutation)
        .unwrap();
    assert_eq!(error.code(), case["code"].as_str().unwrap(), "{mutation}");
    assert_eq!(
        error.operation(),
        case.get("operation")
            .and_then(Value::as_u64)
            .map(|value| u32::try_from(value).unwrap()),
        "{mutation}"
    );
}

fn operation_id(ordinal: u32) -> StructuralOperationId {
    format!("operation[{ordinal}]").parse().unwrap()
}

fn expanded_operation(ordinal: u32, input: StructuralOperationInput) -> ExpandedOperation {
    let template = expansion().operations[0].clone();
    ExpandedOperation {
        ordinal,
        operation: StructuralOperation::new(operation_id(ordinal), input).unwrap(),
        electron_contribution: None,
        ionic_components: Vec::new(),
        provenance: template.provenance,
    }
}

#[allow(clippy::too_many_lines)]
fn reidentify(template: &ExpandedOperation, ordinal: u32) -> ExpandedOperation {
    let input = match template.operation.view() {
        StructuralOperationView::CleaveCovalent {
            left,
            right,
            expected_order,
            allocation,
            transitions,
        } => StructuralOperationInput::CleaveCovalent {
            left: left.clone(),
            right: right.clone(),
            expected_order,
            allocation: allocation.clone(),
            transitions: transitions.values().cloned().collect(),
        },
        StructuralOperationView::FormCovalent {
            left,
            right,
            order,
            transitions,
        } => StructuralOperationInput::FormCovalent {
            left: left.clone(),
            right: right.clone(),
            order,
            transitions: transitions.values().cloned().collect(),
        },
        StructuralOperationView::CleaveDative {
            donor,
            acceptor,
            allocation,
            transitions,
        } => StructuralOperationInput::CleaveDative {
            donor: donor.clone(),
            acceptor: acceptor.clone(),
            allocation: allocation.clone(),
            transitions: transitions.values().cloned().collect(),
        },
        StructuralOperationView::FormDative {
            donor,
            acceptor,
            transitions,
        } => StructuralOperationInput::FormDative {
            donor: donor.clone(),
            acceptor: acceptor.clone(),
            transitions: transitions.values().cloned().collect(),
        },
        StructuralOperationView::ChangeCovalent {
            left,
            right,
            old_order,
            new_order,
            allocation,
            transitions,
        } => StructuralOperationInput::ChangeCovalent {
            left: left.clone(),
            right: right.clone(),
            old_order,
            new_order,
            allocation: allocation.clone(),
            transitions: transitions.values().cloned().collect(),
        },
        StructuralOperationView::AssociateIonic { association } => {
            StructuralOperationInput::AssociateIonic {
                association: association.clone(),
            }
        }
        StructuralOperationView::DissociateIonic { association } => {
            StructuralOperationInput::DissociateIonic {
                association: association.clone(),
            }
        }
        StructuralOperationView::ReleaseMetallic {
            site,
            domain,
            allocation,
            transition,
            domain_electrons_before,
            domain_electrons_after,
        } => StructuralOperationInput::ReleaseMetallic {
            site: site.clone(),
            domain: domain.clone(),
            allocation,
            transition: transition.clone(),
            domain_electrons_before,
            domain_electrons_after,
        },
        StructuralOperationView::JoinMetallic {
            site,
            domain,
            allocation,
            transition,
            domain_electrons_before,
            domain_electrons_after,
        } => StructuralOperationInput::JoinMetallic {
            site: site.clone(),
            domain: domain.clone(),
            allocation,
            transition: transition.clone(),
            domain_electrons_before,
            domain_electrons_after,
        },
        StructuralOperationView::TransferElectron {
            donor,
            acceptor,
            count,
            transitions,
        } => StructuralOperationInput::TransferElectron {
            donor: donor.clone(),
            acceptor: acceptor.clone(),
            count,
            transitions: transitions.values().cloned().collect(),
        },
        StructuralOperationView::AssignProduct { atoms, product } => {
            StructuralOperationInput::AssignProduct {
                atoms: atoms.iter().cloned().collect(),
                product: product.clone(),
            }
        }
    };
    let mut expanded = template.clone();
    expanded.ordinal = ordinal;
    expanded.operation = StructuralOperation::new(operation_id(ordinal), input).unwrap();
    expanded
}

fn custom_cleave(ordinal: u32) -> ExpandedOperation {
    let left: AtomId = "water[1].h1".parse().unwrap();
    let right: AtomId = "water[2].h1".parse().unwrap();
    let neutral = ElectronState::new(0, 0, 0).unwrap();
    expanded_operation(
        ordinal,
        StructuralOperationInput::CleaveCovalent {
            left: left.clone(),
            right: right.clone(),
            expected_order: BondOrder::Single,
            allocation: ElectronAllocation::HeterolyticTo(left.clone()),
            transitions: vec![
                ElectronTransition::new(left, neutral, ElectronState::new(-1, 2, 0).unwrap()),
                ElectronTransition::new(right, neutral, ElectronState::new(1, 0, 0).unwrap()),
            ],
        },
    )
}

fn custom_change(ordinal: u32) -> ExpandedOperation {
    let left: AtomId = "water[1].h1".parse().unwrap();
    let right: AtomId = "water[2].h1".parse().unwrap();
    let radical = ElectronState::new(0, 1, 1).unwrap();
    let neutral = ElectronState::new(0, 0, 0).unwrap();
    expanded_operation(
        ordinal,
        StructuralOperationInput::ChangeCovalent {
            left: left.clone(),
            right: right.clone(),
            old_order: BondOrder::Single,
            new_order: BondOrder::Double,
            allocation: ElectronAllocation::Homolytic,
            transitions: vec![
                ElectronTransition::new(left, radical, neutral),
                ElectronTransition::new(right, radical, neutral),
            ],
        },
    )
}

fn unsupported_cleavage(ordinal: u32) -> ExpandedOperation {
    let oxygen: AtomId = "water[1].o".parse().unwrap();
    let hydrogen: AtomId = "water[1].h1".parse().unwrap();
    expanded_operation(
        ordinal,
        StructuralOperationInput::CleaveCovalent {
            left: oxygen.clone(),
            right: hydrogen.clone(),
            expected_order: BondOrder::Single,
            allocation: ElectronAllocation::HeterolyticTo(oxygen.clone()),
            transitions: vec![
                ElectronTransition::new(
                    oxygen,
                    ElectronState::new(0, 4, 0).unwrap(),
                    ElectronState::new(-1, 6, 2).unwrap(),
                ),
                ElectronTransition::new(
                    hydrogen,
                    ElectronState::new(0, 0, 0).unwrap(),
                    ElectronState::new(1, 0, 0).unwrap(),
                ),
            ],
        },
    )
}

fn custom_dissociate(ordinal: u32) -> ExpandedOperation {
    expanded_operation(
        ordinal,
        StructuralOperationInput::DissociateIonic {
            association: IonicAssociationId::new("missing.ionic").unwrap(),
        },
    )
}

fn custom_join(ordinal: u32) -> ExpandedOperation {
    let site: AtomId = "lithium[1].li".parse().unwrap();
    expanded_operation(
        ordinal,
        StructuralOperationInput::JoinMetallic {
            site: site.clone(),
            domain: MetallicDomainId::new("lithium[1].metallic").unwrap(),
            allocation: MetallicJoinAllocation::DonateElectron,
            transition: ElectronTransition::new(
                site,
                ElectronState::new(0, 1, 1).unwrap(),
                ElectronState::new(1, 0, 0).unwrap(),
            ),
            domain_electrons_before: 0,
            domain_electrons_after: 1,
        },
    )
}

fn custom_assign(ordinal: u32) -> ExpandedOperation {
    expanded_operation(
        ordinal,
        StructuralOperationInput::AssignProduct {
            atoms: vec![AtomId::new("missing.atom").unwrap()],
            product: StructureInstanceId::new("lithiumHydroxide[1]").unwrap(),
        },
    )
}

fn canonical_item(value: &Value) -> Vec<u8> {
    chem_domain::canonical_json(value).unwrap()
}

fn oracle_items(value: &Value) -> BTreeSet<Vec<u8>> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(canonical_item)
        .collect()
}

fn oracle_edges(value: &Value) -> BTreeSet<Vec<u8>> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|edge| {
            let mut endpoints = [
                edge[0].as_str().unwrap().to_owned(),
                edge[1].as_str().unwrap().to_owned(),
            ];
            endpoints.sort();
            canonical_item(&json!([endpoints[0], endpoints[1], edge[2]]))
        })
        .collect()
}

fn canonical_ionic(mut components: Vec<Vec<String>>) -> Vec<u8> {
    for component in &mut components {
        component.sort();
    }
    components.sort();
    canonical_item(&json!([components[0], components[1], "ionic"]))
}

fn oracle_ionic_items(value: &Value) -> BTreeSet<Vec<u8>> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|association| {
            canonical_ionic(vec![
                association[0]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|atom| atom.as_str().unwrap().to_owned())
                    .collect(),
                association[1]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|atom| atom.as_str().unwrap().to_owned())
                    .collect(),
            ])
        })
        .collect()
}

fn canonical_assignment(product: &str, mut atoms: Vec<String>) -> Vec<u8> {
    atoms.sort();
    canonical_item(&json!([product, atoms]))
}

fn oracle_assignments(value: &Value) -> BTreeSet<Vec<u8>> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|assignment| {
            canonical_assignment(
                assignment[0].as_str().unwrap(),
                assignment[1]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|atom| atom.as_str().unwrap().to_owned())
                    .collect(),
            )
        })
        .collect()
}
