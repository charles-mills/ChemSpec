//! Subset-SMILES parsing and writing: classroom molecules round-trip and
//! everything outside the subset fails closed.

use chem_domain::{
    BondOrder, RepresentationKind, StructureId, smiles_from_structure, structure_from_smiles,
};

fn id(text: &str) -> StructureId {
    StructureId::new(text).unwrap()
}

fn counts(structure: &chem_domain::StructureDefinition) -> Vec<(String, u64)> {
    structure
        .formula()
        .elements()
        .iter()
        .map(|(symbol, count)| (symbol.as_str().to_owned(), *count))
        .collect()
}

#[test]
fn organic_molecules_parse_with_implicit_hydrogens() {
    let ethanol = structure_from_smiles(id("t.ethanol"), "CCO").expect("ethanol");
    assert_eq!(ethanol.representation(), RepresentationKind::Molecular);
    assert_eq!(
        counts(&ethanol),
        [("C".to_owned(), 2), ("H".to_owned(), 6), ("O".to_owned(), 1)]
    );

    let ether = structure_from_smiles(id("t.ether"), "COC").expect("dimethyl ether");
    assert_eq!(counts(&ether), counts(&ethanol));
    assert_ne!(
        ethanol.graph().digest().unwrap().to_hex(),
        ether.graph().digest().unwrap().to_hex(),
        "constitutional isomers keep distinct graphs"
    );

    let acid = structure_from_smiles(id("t.acetic"), "CC(=O)O").expect("acetic acid");
    let doubles = acid
        .graph()
        .covalent_bonds()
        .values()
        .filter(|bond| bond.order() == BondOrder::Double)
        .count();
    assert_eq!(doubles, 1);
}

#[test]
fn rings_parse_and_benzene_keeps_its_resonance_annotation() {
    let benzene = structure_from_smiles(id("t.benzene"), "C1=CC=CC=C1").expect("benzene");
    assert_eq!(
        counts(&benzene),
        [("C".to_owned(), 6), ("H".to_owned(), 6)]
    );
    // 6 ring bonds + 6 C-H bonds; the Kekulé alternation is delocalized.
    assert_eq!(benzene.graph().covalent_bonds().len(), 12);
    assert!(
        benzene
            .graph()
            .covalent_bonds()
            .values()
            .any(|bond| bond.delocalization().is_some()),
        "the ring hybrid is annotated like the generated benzene"
    );
}

#[test]
fn ions_parse_into_ionic_structures() {
    let salt =
        structure_from_smiles(id("t.ammonium-cyanate"), "[NH4+].[O-]C#N").expect("ammonium cyanate");
    assert_eq!(salt.representation(), RepresentationKind::Ionic);
    assert_eq!(salt.graph().system_net_charge(), 0);
    assert!(
        salt.graph()
            .covalent_bonds()
            .values()
            .any(|bond| bond.order() == BondOrder::Triple)
    );
}

#[test]
fn out_of_subset_input_fails_closed() {
    for bad in [
        "c1ccccc1",     // aromatic lowercase
        "C/C=C/C",      // stereo bonds
        "[C@H](N)C",    // chirality
        "CC(",          // unbalanced branch
        "C1CC",         // unclosed ring
        "C=",           // dangling bond
        "CC.O",         // neutral multi-component
        "[NH4+].[NH4+]", // non-zero net charge
        "Xx",           // junk element
        "",
    ] {
        assert!(
            structure_from_smiles(id("t.bad"), bad).is_none(),
            "{bad:?} must not parse"
        );
    }
}

#[test]
fn writer_round_trips_through_the_parser() {
    for smiles in [
        "CCO",
        "COC",
        "CC(=O)O",
        "C1=CC=CC=C1",
        "CC(C)C",
        "C#C",
        "CCOC(=O)C",
        "[NH4+].[O-]C#N",
        "NC(=O)N",
    ] {
        let original = structure_from_smiles(id("t.round"), smiles).expect(smiles);
        let written = smiles_from_structure(&original).expect("writable");
        let reparsed = structure_from_smiles(id("t.round"), &written)
            .unwrap_or_else(|| panic!("written form {written:?} of {smiles:?} must reparse"));
        assert_eq!(
            counts(&original),
            counts(&reparsed),
            "{smiles:?} -> {written:?}"
        );
        assert_eq!(original.representation(), reparsed.representation());
        assert_eq!(
            original.graph().covalent_bonds().len(),
            reparsed.graph().covalent_bonds().len(),
            "{smiles:?} -> {written:?} keeps its bond count"
        );
    }
}
