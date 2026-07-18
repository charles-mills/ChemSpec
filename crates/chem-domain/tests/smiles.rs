//! Subset-SMILES parsing and writing: classroom molecules round-trip and
//! everything outside the subset fails closed.

use chem_domain::{
    BondOrder, RepresentationKind, StereoArrangement, StructureId, smiles_from_structure,
    structure_from_smiles,
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
        [
            ("C".to_owned(), 2),
            ("H".to_owned(), 6),
            ("O".to_owned(), 1)
        ]
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
    assert_eq!(counts(&benzene), [("C".to_owned(), 6), ("H".to_owned(), 6)]);
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
    let salt = structure_from_smiles(id("t.ammonium-cyanate"), "[NH4+].[O-]C#N")
        .expect("ammonium cyanate");
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
        "C%12CC%12",     // multi-digit ring closures stay out of subset
        "CC(",           // unbalanced branch
        "C1CC",          // unclosed ring
        "C=",            // dangling bond
        "CC.O",          // neutral multi-component
        "[NH4+].[NH4+]", // non-zero net charge
        "Xx",            // junk element
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

#[test]
fn written_smiles_is_a_canonical_identity() {
    // Every spelling of one molecule writes the identical string.
    for spellings in [
        &["CCO", "OCC", "C(O)C"][..],
        &["CC(C)C", "C(C)(C)C"][..],
        &["CC(=O)O", "OC(=O)C", "C(C)(=O)O"][..],
        &["C1=CC=CC=C1", "C=1C=CC=CC1"][..],
        &["[NH4+].[O-]C#N", "[O-]C#N.[NH4+]"][..],
    ] {
        let written: Vec<String> = spellings
            .iter()
            .filter_map(|smiles| {
                let structure = structure_from_smiles(id("t.canon"), smiles)?;
                smiles_from_structure(&structure)
            })
            .collect();
        assert_eq!(written.len(), spellings.len(), "{spellings:?} all parse");
        assert!(
            written.windows(2).all(|pair| pair[0] == pair[1]),
            "{spellings:?} -> {written:?} must agree"
        );
    }
    // And distinct isomers stay distinct.
    let ethanol =
        smiles_from_structure(&structure_from_smiles(id("t.canon"), "CCO").unwrap()).unwrap();
    let ether =
        smiles_from_structure(&structure_from_smiles(id("t.canon"), "COC").unwrap()).unwrap();
    assert_ne!(ethanol, ether);
}

#[test]
fn double_bond_stereo_round_trips_and_distinguishes_isomers() {
    let trans = structure_from_smiles(id("t.stereo"), "C/C=C/C").expect("trans-but-2-ene");
    let cis = structure_from_smiles(id("t.stereo"), r"C/C=C\C").expect("cis-but-2-ene");
    let plain = structure_from_smiles(id("t.stereo"), "CC=CC").expect("but-2-ene");
    assert_eq!(trans.formula(), cis.formula());
    // Three distinct identities: cis, trans, unspecified.
    let digests: Vec<String> = [&trans, &cis, &plain]
        .iter()
        .map(|structure| structure.graph().digest().unwrap().to_hex())
        .collect();
    assert_ne!(digests[0], digests[1]);
    assert_ne!(digests[0], digests[2]);
    assert_ne!(digests[1], digests[2]);
    // Arrangements survive the writer round trip.
    let arrangement = |structure: &chem_domain::StructureDefinition| {
        structure
            .graph()
            .covalent_bonds()
            .values()
            .find_map(|bond| {
                bond.stereo()
                    .map(chem_domain::DoubleBondStereo::arrangement)
            })
    };
    for (structure, expected) in [
        (&trans, StereoArrangement::Trans),
        (&cis, StereoArrangement::Cis),
    ] {
        assert_eq!(arrangement(structure), Some(expected));
        let written = smiles_from_structure(structure).expect("writable");
        let reparsed = structure_from_smiles(id("t.stereo"), &written)
            .unwrap_or_else(|| panic!("{written:?} reparses"));
        assert_eq!(arrangement(&reparsed), Some(expected), "{written:?}");
    }
    assert_eq!(arrangement(&plain), None);
    // Conflicting directions fail closed.
    assert!(structure_from_smiles(id("t.stereo"), r"C/C=C").is_some());
    assert!(structure_from_smiles(id("t.stereo"), "C/1CC1").is_none());
}

#[test]
fn aromatic_lowercase_kekulizes() {
    // Benzene both ways lands on the same canonical identity.
    let aromatic = structure_from_smiles(id("t.arom"), "c1ccccc1").expect("aromatic benzene");
    let kekule = structure_from_smiles(id("t.arom"), "C1=CC=CC=C1").expect("kekule benzene");
    assert_eq!(
        smiles_from_structure(&aromatic),
        smiles_from_structure(&kekule)
    );
    assert!(
        aromatic
            .graph()
            .covalent_bonds()
            .values()
            .any(|bond| bond.delocalization().is_some()),
        "the resonance annotation still applies"
    );
    // Substituted and fused aromatics parse.
    let toluene = structure_from_smiles(id("t.arom"), "c1ccccc1C").expect("toluene");
    assert_eq!(
        toluene.formula().elements().values().sum::<u64>(),
        15,
        "C7H8"
    );
    assert!(
        structure_from_smiles(id("t.arom"), "c1ccc2ccccc2c1").is_some(),
        "naphthalene"
    );
    assert!(
        structure_from_smiles(id("t.arom"), "c1ccncc1").is_some(),
        "pyridine"
    );
    // Kekulization is structural, not thermodynamic: strained rings with
    // a valid matching (cyclobutadiene) parse; only unmatchable notation
    // fails closed.
    assert!(structure_from_smiles(id("t.arom"), "c1ccc1").is_some());
    for bad in ["cC", "c", "c1cc[nH]c1"] {
        assert!(
            structure_from_smiles(id("t.arom"), bad).is_none(),
            "{bad:?} must not kekulize"
        );
    }
}

#[test]
fn tetrahedral_chirality_round_trips_and_distinguishes_enantiomers() {
    let r_form = structure_from_smiles(id("t.chiral"), "C[C@H](O)CC").expect("@ butan-2-ol");
    let s_form = structure_from_smiles(id("t.chiral"), "C[C@@H](O)CC").expect("@@ butan-2-ol");
    let plain = structure_from_smiles(id("t.chiral"), "CC(O)CC").expect("plain butan-2-ol");
    assert_eq!(r_form.formula(), s_form.formula());
    let digests: Vec<String> = [&r_form, &s_form, &plain]
        .iter()
        .map(|structure| structure.graph().digest().unwrap().to_hex())
        .collect();
    assert_ne!(
        digests[0], digests[1],
        "enantiomers are distinct identities"
    );
    assert_ne!(digests[0], digests[2]);
    // The canonical form is a fixed point and distinguishes enantiomers.
    let written_r = smiles_from_structure(&r_form).expect("writable");
    let written_s = smiles_from_structure(&s_form).expect("writable");
    assert_ne!(written_r, written_s, "enantiomers write differently");
    for written in [&written_r, &written_s] {
        let reparsed =
            structure_from_smiles(id("t.chiral"), written).expect("written form reparses");
        assert!(
            reparsed
                .graph()
                .atoms()
                .values()
                .any(|atom| atom.chirality().is_some()),
            "descriptor survives"
        );
        assert_eq!(
            smiles_from_structure(&reparsed).as_ref(),
            Some(written),
            "canonical form is a fixed point"
        );
    }
    // A chiral centre with two hydrogens is rejected at parse.
    assert!(structure_from_smiles(id("t.chiral"), "C[C@H2]O").is_none());
}
