//! Wöhler-pair generation: the formula CH4N2O canonically generates urea,
//! while ammonium cyanate — the same inventory — is reachable only by name.

use chem_domain::{
    BondOrder, ElementInventory, ElementSymbol, RepresentationKind, StructureId,
    generate_named_structure, generate_structure,
};

fn ch4n2o() -> ElementInventory {
    ElementInventory::new([
        (ElementSymbol::new("C").unwrap(), 1),
        (ElementSymbol::new("H").unwrap(), 4),
        (ElementSymbol::new("N").unwrap(), 2),
        (ElementSymbol::new("O").unwrap(), 1),
    ])
    .unwrap()
}

#[test]
fn ch4n2o_generates_urea() {
    let structure = generate_structure(StructureId::new("test.urea").unwrap(), &ch4n2o())
        .expect("CH4N2O has a canonical molecular structure");
    assert_eq!(structure.representation(), RepresentationKind::Molecular);
    let graph = structure.graph();
    // Urea, not the isourea tautomer: the one double bond joins C and O,
    // and every hydrogen sits on a nitrogen.
    let doubles = graph
        .covalent_bonds()
        .values()
        .filter(|bond| bond.order() == BondOrder::Double)
        .collect::<Vec<_>>();
    let element_of =
        |atom_id: &chem_domain::AtomId| graph.atoms()[atom_id].element().as_str().to_owned();
    assert_eq!(doubles.len(), 1, "urea has exactly one double bond");
    let mut ends = [
        element_of(doubles[0].left()),
        element_of(doubles[0].right()),
    ];
    ends.sort();
    assert_eq!(ends, ["C".to_owned(), "O".to_owned()]);
    for bond in graph.covalent_bonds().values() {
        let pair = [element_of(bond.left()), element_of(bond.right())];
        if pair.contains(&"H".to_owned()) {
            assert!(
                pair.contains(&"N".to_owned()),
                "every urea hydrogen bonds to nitrogen, found {pair:?}"
            );
        }
    }
}

#[test]
fn ammonium_cyanate_is_reachable_only_by_name() {
    let named = generate_named_structure(
        StructureId::new("test.ammonium-cyanate").unwrap(),
        "ammonium cyanate",
        &ch4n2o(),
    )
    .expect("named ammonium cyanate structure");
    assert_eq!(named.representation(), RepresentationKind::Ionic);
    let graph = named.graph();
    assert_eq!(graph.system_net_charge(), 0);
    assert_eq!(graph.ionic_associations().len(), 1);
    // The cyanate anion keeps its N≡C triple bond.
    assert!(
        graph
            .covalent_bonds()
            .values()
            .any(|bond| bond.order() == BondOrder::Triple),
        "cyanate carries a triple bond"
    );
    // The same inventory generates urea, and the two graphs are distinct.
    let urea = generate_structure(StructureId::new("test.urea").unwrap(), &ch4n2o()).unwrap();
    assert_ne!(urea.representation(), named.representation());
}

#[test]
fn named_structures_accept_formula_spellings_and_reject_mismatched_inventories() {
    for spelling in ["Ammonium  Cyanate", "NH4OCN", "nh4cno"] {
        assert!(
            generate_named_structure(
                StructureId::new("test.spelling").unwrap(),
                spelling,
                &ch4n2o(),
            )
            .is_some(),
            "spelling {spelling:?} resolves"
        );
    }
    let water = ElementInventory::new([
        (ElementSymbol::new("H").unwrap(), 2),
        (ElementSymbol::new("O").unwrap(), 1),
    ])
    .unwrap();
    assert!(
        generate_named_structure(
            StructureId::new("test.mismatch").unwrap(),
            "ammonium cyanate",
            &water,
        )
        .is_none(),
        "a named structure never overrides a mismatched inventory"
    );
}
