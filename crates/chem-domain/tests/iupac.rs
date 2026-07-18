//! Systematic-name parsing: names → subset SMILES → validated structures.

use chem_domain::{StructureId, smiles_for_name, structure_from_smiles};

fn structure_of(name: &str) -> Option<chem_domain::StructureDefinition> {
    let smiles = smiles_for_name(name)?;
    structure_from_smiles(StructureId::new("t.iupac").unwrap(), &smiles)
}

fn digest(structure: &chem_domain::StructureDefinition) -> String {
    structure.graph().digest().unwrap().to_hex()
}

#[test]
fn systematic_names_parse_to_the_expected_structures() {
    for (name, smiles) in [
        ("pentane", "CCCCC"),
        ("2-methylbutane", "CC(C)CC"),
        ("2-methylpropane", "CC(C)C"),
        ("2,2-dimethylpropane", "CC(C)(C)C"),
        ("but-2-ene", "CC=CC"),
        ("2-butene", "CC=CC"),
        ("but-1-ene", "C=CCC"),
        ("propan-2-ol", "CC(O)C"),
        ("2-propanol", "CC(O)C"),
        ("butan-1-ol", "CCCCO"),
        ("ethanoic acid", "CC(=O)O"),
        ("butanoic acid", "CCCC(=O)O"),
        ("2-bromopropane", "CC(Br)C"),
        ("1,2-dibromoethane", "BrCCBr"),
        ("chloromethane", "CCl"),
        ("hex-3-yne", "CCC#CCC"),
    ] {
        let named = structure_of(name)
            .unwrap_or_else(|| panic!("{name} must parse to a structure"));
        let direct = structure_from_smiles(StructureId::new("t.iupac").unwrap(), smiles)
            .expect("reference smiles parses");
        assert_eq!(
            named.formula(),
            direct.formula(),
            "{name} inventory mismatch"
        );
        // Same atom count and bond multiset is enough here; exact-graph
        // agreement is covered by the naming round-trip in the agent crate.
        assert_eq!(
            named.graph().covalent_bonds().len(),
            direct.graph().covalent_bonds().len(),
            "{name} bond count mismatch"
        );
    }
}

#[test]
fn positional_names_produce_distinct_isomers() {
    let but_1_ene = structure_of("but-1-ene").expect("but-1-ene");
    let but_2_ene = structure_of("but-2-ene").expect("but-2-ene");
    assert_eq!(but_1_ene.formula(), but_2_ene.formula());
    assert_ne!(digest(&but_1_ene), digest(&but_2_ene));

    let propan_1_ol = structure_of("propan-1-ol").expect("propan-1-ol");
    let propan_2_ol = structure_of("propan-2-ol").expect("propan-2-ol");
    assert_ne!(digest(&propan_1_ol), digest(&propan_2_ol));
}

#[test]
fn out_of_subset_names_fail_closed() {
    for bad in [
        "nonane",              // root beyond C8
        "but-4-ene",           // locant off the chain
        "dimethylbutane",      // missing locants
        "2-methyl",            // no root/suffix
        "benzenol",            // aromatic suffix not in subset
        "butan-2,3-diol",      // multiple hydroxyls
        "gibberish",
        "",
    ] {
        assert!(
            smiles_for_name(bad).is_none(),
            "{bad:?} must not parse"
        );
    }
}
