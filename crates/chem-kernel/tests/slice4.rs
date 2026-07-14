use std::{collections::BTreeSet, fs, path::PathBuf};

use chem_catalogue::ValidatedCatalogueBundle;
use chem_domain::{StructuralOperationView, canonical_json};
use chem_kernel::{
    CatalogueTrust, EvidenceTrust, ExpansionFailureClass, ValidatedEvidencePacket,
    expand_review_candidate,
};
use serde_json::Value;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture(path: &str) -> Vec<u8> {
    fs::read(workspace_root().join(path)).expect("fixture should be readable")
}

fn canonical_expansion() -> chem_kernel::ExpandedStructuralReaction {
    let source = fixture("conformance/expansion/canonical-expansion-001.chems");
    let source = std::str::from_utf8(&source).unwrap();
    let catalogue = ValidatedCatalogueBundle::from_json(&fixture(
        "conformance/catalogue/lithium-rule-001.catalogue.json",
    ))
    .unwrap();
    expand_review_candidate(
        "conformance/expansion/canonical-expansion-001.chems",
        source,
        &catalogue,
        &fixture("conformance/observations/lithium-observations-001.input.json"),
    )
    .unwrap()
}

fn catalogue() -> ValidatedCatalogueBundle {
    ValidatedCatalogueBundle::from_json(&fixture(
        "conformance/catalogue/lithium-rule-001.catalogue.json",
    ))
    .unwrap()
}

fn canonical_source() -> String {
    String::from_utf8(fixture(
        "conformance/expansion/canonical-expansion-001.chems",
    ))
    .unwrap()
}

fn evidence() -> Vec<u8> {
    fixture("conformance/observations/lithium-observations-001.input.json")
}

#[test]
fn canonical_source_expands_without_executing_operations() {
    let expanded = canonical_expansion();
    assert_eq!(
        expanded.claim.catalogue.trust,
        CatalogueTrust::ReviewCandidate
    );
    assert_eq!(expanded.reactant_instances.len(), 4);
    assert_eq!(expanded.product_instances.len(), 3);
    assert_eq!(expanded.mapping.entries().len(), 8);
    assert_eq!(expanded.operations.len(), 12);
    assert_eq!(expanded.premises.len(), 8);
    assert!(expanded.render_certificate().contains("status: unexecuted"));
    assert_eq!(
        expanded.render_certificate().as_bytes(),
        fixture("conformance/expansion/canonical-expansion-001.certificate.txt")
    );
    assert_eq!(
        expanded.render_provenance_report().as_bytes(),
        fixture("conformance/expansion/canonical-expansion-001.provenance.txt")
    );
    let expected: Value = serde_json::from_slice(&fixture(
        "conformance/expansion/canonical-expansion-001.expanded.json",
    ))
    .unwrap();
    assert_eq!(
        expanded.semantic_json().unwrap(),
        canonical_json(&expected).unwrap()
    );
    assert!(!expanded.canonical_json().unwrap().is_empty());
}

#[test]
fn independent_oracle_agrees_on_instances_atoms_mapping_and_operation_order() {
    let expanded = canonical_expansion();
    let oracle: Value = serde_json::from_slice(&fixture(
        "conformance/expansion/canonical-expansion-001.hir.json",
    ))
    .unwrap();
    assert_eq!(oracle["catalogue"], "ChemSpec.Theoretical@1");
    assert_eq!(oracle["rule"], expanded.claim.rule.rule.as_str());

    let actual_reactants = expanded
        .reactant_instances
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    let expected_reactants = oracle["instances"]["reactants"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().to_owned())
        .collect::<BTreeSet<_>>();
    assert_eq!(actual_reactants, expected_reactants);
    let actual_products = expanded
        .product_instances
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    let expected_products = oracle["instances"]["products"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().to_owned())
        .collect::<BTreeSet<_>>();
    assert_eq!(actual_products, expected_products);

    let actual_atoms = expanded
        .reactant_instances
        .values()
        .flat_map(|instance| instance.instance.graph().atoms().values())
        .map(|atom| (atom.id().to_string(), atom.element().to_string()))
        .collect::<std::collections::BTreeMap<_, _>>();
    let expected_atoms = oracle["atom_elements"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(atom, element)| (atom.clone(), element.as_str().unwrap().to_owned()))
        .collect();
    assert_eq!(actual_atoms, expected_atoms);

    let expected_mapping = oracle["mapping"]
        .as_array()
        .unwrap()
        .iter()
        .map(|pair| {
            (
                pair[0].as_str().unwrap().to_owned(),
                pair[1].as_str().unwrap().to_owned(),
            )
        })
        .collect::<BTreeSet<_>>();
    let actual_mapping = expanded
        .mapping
        .entries()
        .iter()
        .map(|(source, product)| (source.to_string(), product.to_string()))
        .collect::<BTreeSet<_>>();
    assert_eq!(actual_mapping, expected_mapping);

    let expected_kinds = oracle["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|operation| operation["kind"].as_str().unwrap())
        .collect::<Vec<_>>();
    let actual_kinds = expanded
        .operations
        .iter()
        .map(|operation| match operation.operation.view() {
            StructuralOperationView::CleaveCovalent { .. } => "cleave_covalent",
            StructuralOperationView::FormCovalent { .. } => "form_covalent",
            StructuralOperationView::CleaveDative { .. } => "cleave_dative",
            StructuralOperationView::FormDative { .. } => "form_dative",
            StructuralOperationView::ChangeCovalent { .. } => "change_covalent",
            StructuralOperationView::AssociateIonic { .. } => "associate_ionic",
            StructuralOperationView::DissociateIonic { .. } => "dissociate_ionic",
            StructuralOperationView::ReleaseMetallic { .. } => "release_metallic",
            StructuralOperationView::JoinMetallic { .. } => "join_metallic",
            StructuralOperationView::TransferElectron { .. } => "transfer_electron",
            StructuralOperationView::AssignProduct { .. } => "assign_product",
        })
        .collect::<Vec<_>>();
    assert_eq!(actual_kinds, expected_kinds);
}

#[test]
fn operation_templates_expand_to_exact_bound_endpoints_and_ionic_components() {
    let expanded = canonical_expansion();
    match expanded.operations[0].operation.view() {
        StructuralOperationView::ReleaseMetallic {
            site,
            domain,
            transition,
            domain_electrons_before,
            domain_electrons_after,
            ..
        } => {
            assert_eq!(site.as_str(), "lithium[1].li");
            assert_eq!(domain.as_str(), "lithium[1].metallic");
            assert_eq!(transition.atom(), site);
            assert_eq!((domain_electrons_before, domain_electrons_after), (1, 0));
        }
        operation => panic!("unexpected operation: {operation:?}"),
    }
    match expanded.operations[4].operation.view() {
        StructuralOperationView::TransferElectron {
            donor,
            acceptor,
            count,
            ..
        } => {
            assert_eq!(donor.as_str(), "lithium[1].li");
            assert_eq!(acceptor.as_str(), "water[1].h1");
            assert_eq!(count, 1);
        }
        operation => panic!("unexpected operation: {operation:?}"),
    }
    match expanded.operations[6].operation.view() {
        StructuralOperationView::FormCovalent { left, right, .. } => {
            assert_eq!(left.as_str(), "water[1].h1");
            assert_eq!(right.as_str(), "water[2].h1");
        }
        operation => panic!("unexpected operation: {operation:?}"),
    }
    let ionic = &expanded.operations[7];
    assert_eq!(ionic.ionic_components.len(), 2);
    assert_eq!(ionic.ionic_components[0].expected_charge, 1);
    assert_eq!(ionic.ionic_components[1].expected_charge, -1);
    assert_eq!(
        ionic.ionic_components[0]
            .group
            .atoms()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        ["lithium[1].li"]
    );
    assert_eq!(
        ionic.ionic_components[1]
            .group
            .atoms()
            .iter()
            .map(ToString::to_string)
            .collect::<BTreeSet<_>>(),
        ["water[1].h2".to_owned(), "water[1].o".to_owned()]
            .into_iter()
            .collect()
    );
    match expanded.operations[9].operation.view() {
        StructuralOperationView::AssignProduct { atoms, product } => {
            assert_eq!(product.as_str(), "lithiumHydroxide[1]");
            assert_eq!(atoms.len(), 3);
        }
        operation => panic!("unexpected operation: {operation:?}"),
    }
}

#[test]
fn equivalent_declaration_order_has_identical_semantic_hir() {
    let original = canonical_source();
    let reordered = original
        .replace(
            "    lithium := 2 of LithiumMetal\n    water := 2 of Water",
            "    water := 2 of Water\n    lithium := 2 of LithiumMetal",
        )
        .replace(
            "    lithiumHydroxide := 2 of LithiumHydroxide\n    hydrogen := 1 of Hydrogen",
            "    hydrogen := 1 of Hydrogen\n    lithiumHydroxide := 2 of LithiumHydroxide",
        )
        .replace(
            "    2 Li[metallic] + 2 H2O[molecular]",
            "    2 H2O[molecular] + 2 Li[metallic]",
        )
        .replace(
            "    -> 2 LiOH[ionic] + H2[molecular]",
            "    -> H2[molecular] + 2 LiOH[ionic]",
        )
        .replace(
            "    gas hydrogen evolves claim R1\n    reactant lithium disappears claim R2",
            "    reactant lithium disappears claim R2\n    gas hydrogen evolves claim R1",
        )
        .replace(
            "      metal := lithium\n      water := water\n      hydroxide := lithiumHydroxide\n      gasProduct := hydrogen",
            "      gasProduct := hydrogen\n      hydroxide := lithiumHydroxide\n      water := water\n      metal := lithium",
        );
    let catalogue = catalogue();
    let first = expand_review_candidate("first.chems", &original, &catalogue, &evidence()).unwrap();
    let second =
        expand_review_candidate("second.chems", &reordered, &catalogue, &evidence()).unwrap();
    assert_ne!(
        first.claim.source.bytes_digest,
        second.claim.source.bytes_digest
    );
    assert_eq!(
        first.claim.source.semantic_digest,
        second.claim.source.semantic_digest
    );
    assert_eq!(
        first.semantic_digest().unwrap(),
        second.semantic_digest().unwrap()
    );
    assert_eq!(
        first.semantic_json().unwrap(),
        second.semantic_json().unwrap()
    );
    assert_eq!(first.render_certificate(), second.render_certificate());
    assert_ne!(
        first.render_provenance_report(),
        second.render_provenance_report()
    );
    assert_eq!(first.mapping, second.mapping);
    assert_eq!(first.operations.len(), second.operations.len());
    for (left, right) in first.operations.iter().zip(&second.operations) {
        assert_eq!(left.operation, right.operation);
    }
}

#[test]
fn every_derived_value_retains_source_catalogue_and_evidence_provenance() {
    let expanded = canonical_expansion();
    assert_eq!(
        expanded.claim.evidence.trust,
        EvidenceTrust::ExternalUntrusted
    );
    for binding in expanded
        .claim
        .reactants
        .values()
        .chain(expanded.claim.products.values())
    {
        assert!(!binding.provenance.source.is_empty());
        assert!(!binding.provenance.catalogue.is_empty());
    }
    for instance in expanded
        .reactant_instances
        .values()
        .chain(expanded.product_instances.values())
    {
        assert!(!instance.provenance.source.is_empty());
        assert!(!instance.provenance.catalogue.is_empty());
    }
    for operation in &expanded.operations {
        assert!(!operation.provenance.source.is_empty());
        assert!(!operation.provenance.catalogue.is_empty());
    }
    let premise_set = |operation: usize| {
        expanded.operations[operation]
            .provenance
            .catalogue
            .iter()
            .flat_map(|origin| origin.premises.iter().map(ToString::to_string))
            .collect::<BTreeSet<_>>()
    };
    assert_eq!(
        premise_set(0),
        [
            "premise.rule.lithium-water.standard-outcome".to_owned(),
            "premise.structure.lithium-metal".to_owned(),
            "premise.valence.li-h-o.initial-domain".to_owned(),
        ]
        .into_iter()
        .collect()
    );
    assert_eq!(
        premise_set(6),
        [
            "premise.rule.lithium-water.standard-outcome".to_owned(),
            "premise.structure.hydrogen".to_owned(),
            "premise.structure.water".to_owned(),
            "premise.valence.li-h-o.initial-domain".to_owned(),
        ]
        .into_iter()
        .collect()
    );
    assert_eq!(
        expanded
            .claim
            .model
            .provenance
            .catalogue
            .iter()
            .flat_map(|origin| origin.premises.iter().map(ToString::to_string))
            .collect::<BTreeSet<_>>(),
        ["premise.rule.lithium-water.standard-outcome".to_owned()]
            .into_iter()
            .collect()
    );
    assert_eq!(expanded.atom_provenance.len(), 16);
    assert!(
        expanded.atom_provenance.values().all(|provenance| {
            !provenance.source.is_empty() && !provenance.catalogue.is_empty()
        })
    );
    assert!(!expanded.mapping_provenance.source.is_empty());
    assert!(!expanded.mapping_provenance.catalogue.is_empty());
    assert_eq!(expanded.mapping_entry_provenance.len(), 8);
    assert_eq!(
        expanded.mapping_entry_provenance[&"water[1].h1".parse().unwrap()]
            .premises
            .iter()
            .map(ToString::to_string)
            .collect::<BTreeSet<_>>(),
        [
            "premise.rule.lithium-water.standard-outcome".to_owned(),
            "premise.structure.hydrogen".to_owned(),
            "premise.structure.water".to_owned(),
        ]
        .into_iter()
        .collect()
    );
    assert_eq!(expanded.premise_provenance.len(), expanded.premises.len());
    for (premise, origin) in &expanded.premise_provenance {
        assert!(origin.premises.contains(premise));
    }
    for observation in &expanded.claim.evidence.observations {
        assert!(!observation.provenance.source.is_empty());
        assert!(!observation.provenance.catalogue.is_empty());
        assert!(!observation.provenance.evidence.is_empty());
    }
}

#[test]
fn evidence_packet_is_order_canonical_duplicate_strict_and_digest_sensitive() {
    let bytes = evidence();
    let schema: Value =
        serde_json::from_slice(&fixture("schemas/chem-evidence-packet-1.schema.json")).unwrap();
    let evidence_value: Value = serde_json::from_slice(&bytes).unwrap();
    let validator = jsonschema::draft202012::new(&schema).unwrap();
    assert!(validator.is_valid(&evidence_value));
    let original = ValidatedEvidencePacket::from_json(&bytes).unwrap();
    let mut reordered: Value = serde_json::from_slice(&bytes).unwrap();
    reordered["claims"].as_array_mut().unwrap().reverse();
    reordered["sources"].as_array_mut().unwrap().reverse();
    for claim in reordered["claims"].as_array_mut().unwrap() {
        claim["sources"].as_array_mut().unwrap().reverse();
    }
    for source in reordered["sources"].as_array_mut().unwrap() {
        source["supports"].as_array_mut().unwrap().reverse();
    }
    let reordered =
        ValidatedEvidencePacket::from_json(&serde_json::to_vec(&reordered).unwrap()).unwrap();
    assert_eq!(original.digest(), reordered.digest());

    let mut changed: Value = serde_json::from_slice(&bytes).unwrap();
    changed["sources"][0]["url"] = Value::String("https://example.invalid/changed".to_owned());
    let changed =
        ValidatedEvidencePacket::from_json(&serde_json::to_vec(&changed).unwrap()).unwrap();
    assert_ne!(original.digest(), changed.digest());

    for field in ["claims", "sources"] {
        let mut duplicate: Value = serde_json::from_slice(&bytes).unwrap();
        let entry = duplicate[field][0].clone();
        duplicate[field].as_array_mut().unwrap().push(entry);
        assert!(
            ValidatedEvidencePacket::from_json(&serde_json::to_vec(&duplicate).unwrap()).is_err()
        );
    }

    for (pointer, invalid) in [
        ("/claims/0/predicate", Value::String("boils".to_owned())),
        ("/claims/0/id", Value::String("R[1]".to_owned())),
        ("/claims/0/subject_role", Value::String(" \t".to_owned())),
        ("/claims/0/subject", Value::String(" \n".to_owned())),
        ("/sources/0/title", Value::String("   ".to_owned())),
        ("/sources/0/publisher", Value::String("\t".to_owned())),
        ("/sources/0/url", Value::String("\r\n".to_owned())),
    ] {
        let mut malformed: Value = serde_json::from_slice(&bytes).unwrap();
        *malformed.pointer_mut(pointer).unwrap() = invalid;
        assert!(!validator.is_valid(&malformed));
        assert!(
            ValidatedEvidencePacket::from_json(&serde_json::to_vec(&malformed).unwrap()).is_err()
        );
    }
}

#[test]
fn invalid_unsupported_and_corrupt_boundaries_remain_distinct() {
    let catalogue = catalogue();
    let invalid_equation = canonical_source().replacen("2 H2O[molecular]", "3 H2O[molecular]", 1);
    let invalid =
        expand_review_candidate("invalid.chems", &invalid_equation, &catalogue, &evidence())
            .unwrap_err();
    assert_eq!(invalid.class(), ExpansionFailureClass::InvalidSource);
    assert_eq!(invalid.code(), "CHEMS-X005");

    let unsupported_source = canonical_source().replacen("of Water", "of UnknownWater", 1);
    let unsupported = expand_review_candidate(
        "unsupported.chems",
        &unsupported_source,
        &catalogue,
        &evidence(),
    )
    .unwrap_err();
    assert_eq!(
        unsupported.class(),
        ExpansionFailureClass::UnsupportedChemistry
    );
    assert_eq!(unsupported.code(), "CHEMS-X011");

    let mut corrupt_catalogue: Value = serde_json::from_slice(&fixture(
        "conformance/catalogue/lithium-rule-001.catalogue.json",
    ))
    .unwrap();
    corrupt_catalogue["bundle"]["rules"][0]["mapping_template"][0]["product"] =
        Value::String("gasProduct[1].h1".to_owned());
    assert!(
        ValidatedCatalogueBundle::from_json(&serde_json::to_vec(&corrupt_catalogue).unwrap())
            .is_err(),
        "corrupt trusted data must fail before the elaborator can receive it"
    );
}

#[test]
fn rule_binding_and_evidence_claim_mismatches_are_invalid() {
    let catalogue = catalogue();
    let wrong_binding = canonical_source().replacen("metal := lithium", "metal := water", 1);
    let error = expand_review_candidate(
        "wrong-binding.chems",
        &wrong_binding,
        &catalogue,
        &evidence(),
    )
    .unwrap_err();
    assert_eq!(error.class(), ExpansionFailureClass::InvalidSource);
    assert_eq!(error.code(), "CHEMS-X012");

    let mut wrong_evidence: Value = serde_json::from_slice(&evidence()).unwrap();
    wrong_evidence["claims"][0]["subject"] = Value::String("oxygen".to_owned());
    let error = expand_review_candidate(
        "wrong-evidence.chems",
        &canonical_source(),
        &catalogue,
        &serde_json::to_vec(&wrong_evidence).unwrap(),
    )
    .unwrap_err();
    assert_eq!(error.class(), ExpansionFailureClass::InvalidSource);
    assert_eq!(error.code(), "CHEMS-X023");
}
