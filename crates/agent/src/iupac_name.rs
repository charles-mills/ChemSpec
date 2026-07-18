//! Systematic names from organic graphs: the reverse of chem-domain's
//! name parser, over the same classroom subset. Acyclic C/H/O/halogen
//! molecules with one principal group (carboxylic acid, one to three
//! hydroxyls, an ester link, or one C-C unsaturation) and simple
//! alkyl/halo substituents name as `2-methylbutane`, `but-2-ene`,
//! `propane-1,2-diol`, `butanoic acid`, `propyl ethanoate`. Anything
//! richer returns None — a wrong name is worse than none.

use std::fmt::Write as _;

use crate::organic::{Editable, carboxyls, hydroxyls};

const ROOTS: [&str; 8] = ["meth", "eth", "prop", "but", "pent", "hex", "hept", "oct"];

const MULTIPLIERS: [&str; 4] = ["", "di", "tri", "tetra"];

/// Chain preference: principal-group locant set, substituent locant set,
/// then the assembled name for a stable tie-break.
type Score = (Vec<usize>, Vec<usize>, String);

/// Names a simple ester `<alkyl>yl <acyl-root>anoate`: a lone bridging
/// oxygen joining an unbranched alkyl chain to an unbranched acyl chain
/// whose first carbon carries the only other oxygen as a carbonyl.
fn ester_name(molecule: &Editable) -> Option<String> {
    let oxygens: Vec<usize> = (0..molecule.symbols.len())
        .filter(|index| molecule.symbols[*index] == "O")
        .collect();
    if oxygens.len() != 2 {
        return None;
    }
    // The bridge: an O with two single-bonded carbon neighbours and no H.
    let bridge = *oxygens.iter().find(|oxygen| {
        molecule.hydrogens[**oxygen] == 0
            && molecule.neighbours(**oxygen).count() == 2
            && molecule
                .neighbours(**oxygen)
                .all(|(atom, order)| order == 1 && molecule.symbols[atom] == "C")
    })?;
    let ends: Vec<usize> = molecule.neighbours(bridge).map(|(atom, _)| atom).collect();
    let [first, second] = ends.as_slice() else {
        return None;
    };
    let carbonyl_of = |carbon: usize| {
        molecule
            .neighbours(carbon)
            .any(|(atom, order)| order == 2 && molecule.symbols[atom] == "O")
    };
    let (acyl_carbon, alkyl_carbon) = match (carbonyl_of(*first), carbonyl_of(*second)) {
        (true, false) => (*first, *second),
        (false, true) => (*second, *first),
        _ => return None,
    };
    let alkyl = unbranched_chain_length(molecule, bridge, alkyl_carbon)?;
    let acyl = unbranched_chain_length(molecule, bridge, acyl_carbon)?;
    let alkyl_root = ROOTS.get(alkyl - 1)?;
    let acyl_root = ROOTS.get(acyl - 1)?;
    Some(format!("{alkyl_root}yl {acyl_root}anoate"))
}

/// The length of an unbranched all-carbon single-bond chain walked from
/// `start`, arriving from `previous`. None on branching, multiple C-C
/// bonds, or stray substituents.
fn unbranched_chain_length(molecule: &Editable, previous: usize, start: usize) -> Option<usize> {
    let mut length = 0;
    let mut from = previous;
    let mut current = start;
    loop {
        length += 1;
        let next: Vec<usize> = molecule
            .neighbours(current)
            .filter(|(atom, order)| {
                *atom != from && !(molecule.symbols[*atom] == "O" && *order == 2)
            })
            .map(|(atom, order)| (order == 1 && molecule.symbols[atom] == "C").then_some(atom))
            .collect::<Option<Vec<_>>>()?;
        match next.as_slice() {
            [] => return Some(length),
            [one] => {
                from = current;
                current = *one;
            }
            _ => return None,
        }
    }
}

/// The systematic name of an editable molecule within the subset.
#[allow(clippy::too_many_lines)]
pub(crate) fn systematic_name(molecule: &Editable) -> Option<String> {
    if molecule.symbols.is_empty()
        || molecule.bonds.len() + 1 != molecule.symbols.len()
        || molecule
            .symbols
            .iter()
            .any(|symbol| !matches!(symbol.as_str(), "C" | "O" | "F" | "Cl" | "Br" | "I"))
    {
        return None;
    }
    let carbons: Vec<usize> = (0..molecule.symbols.len())
        .filter(|index| molecule.symbols[*index] == "C")
        .collect();
    if carbons.is_empty() {
        return None;
    }
    // Principal group: one acid, else at most one hydroxyl, plus at most
    // one carbon-carbon multiple bond.
    let acids = carboxyls(molecule);
    let alcohols = hydroxyls(molecule);
    let multiples: Vec<(usize, usize, u8)> = molecule
        .bonds
        .iter()
        .filter(|(left, right, order)| {
            *order > 1 && molecule.symbols[*left] == "C" && molecule.symbols[*right] == "C"
        })
        .copied()
        .collect();
    let heteroatom_oxygens = molecule
        .symbols
        .iter()
        .filter(|symbol| *symbol == "O")
        .count();
    if let Some(name) = ester_name(molecule) {
        return Some(name);
    }
    // A lone C=O with no hydroxyl is an aldehyde or ketone carbonyl; a
    // carboxyl's own C=O belongs to the acid suffix, not here.
    let carboxyl_carbons: Vec<usize> = acids.iter().map(|group| group.carbon).collect();
    let carbonyls: Vec<(usize, usize)> = molecule
        .bonds
        .iter()
        .filter_map(|(left, right, order)| {
            if *order != 2 {
                return None;
            }
            match (
                molecule.symbols[*left].as_str(),
                molecule.symbols[*right].as_str(),
            ) {
                ("C", "O") => Some((*left, *right)),
                ("O", "C") => Some((*right, *left)),
                _ => None,
            }
        })
        .filter(|(carbon, _)| !carboxyl_carbons.contains(carbon))
        .collect();
    let (acid, hydroxyls, carbonyl) =
        match (acids.as_slice(), alcohols.as_slice(), carbonyls.as_slice()) {
            ([acid], [], []) if heteroatom_oxygens == 2 => (Some(*acid), Vec::new(), None),
            ([], polyols @ ([_] | [_, _] | [_, _, _]), [])
                if heteroatom_oxygens == polyols.len() =>
            {
                (None, polyols.to_vec(), None)
            }
            ([], [], [carbonyl]) if heteroatom_oxygens == 1 => (None, Vec::new(), Some(*carbonyl)),
            ([], [], []) if heteroatom_oxygens == 0 => (None, Vec::new(), None),
            _ => return None,
        };
    if multiples.len() > 1
        || (acid.is_some() || !hydroxyls.is_empty() || carbonyl.is_some()) && !multiples.is_empty()
    {
        // Mixed suffixes (enols, unsaturated acids) stay out of the subset.
        return None;
    }

    // Candidate chains: longest carbon paths that contain the principal
    // atoms. Graphs are tiny, so enumerate paths from every carbon.
    let required: Vec<usize> = if let Some(group) = acid {
        vec![group.carbon]
    } else if let Some((carbon, _)) = carbonyl {
        vec![carbon]
    } else if !hydroxyls.is_empty() {
        hydroxyls.iter().map(|(carbon, _)| *carbon).collect()
    } else {
        multiples
            .first()
            .map(|(left, right, _)| vec![*left, *right])
            .unwrap_or_default()
    };
    let chains = longest_carbon_paths(molecule, &carbons, &required);
    chains.first()?;

    // Score each chain and numbering direction by IUPAC preference:
    // principal-group locant first, then substituent locant set, then the
    // assembled name alphabetically for a stable choice.
    let mut best: Option<(Score, String)> = None;
    for chain in &chains {
        for direction in [chain.clone(), chain.iter().rev().copied().collect()] {
            if acid.is_some() && molecule.symbols[direction[0]] != "C" {
                continue;
            }
            let Some((score, name)) = name_for_chain(
                molecule,
                &direction,
                acid,
                &hydroxyls,
                carbonyl,
                multiples.first(),
            ) else {
                continue;
            };
            let candidate = (score, name);
            if best.as_ref().is_none_or(|current| candidate < *current) {
                best = Some(candidate);
            }
        }
    }
    best.map(|(_, name)| name)
}

/// All maximal-length simple carbon paths that include every required atom.
fn longest_carbon_paths(
    molecule: &Editable,
    carbons: &[usize],
    required: &[usize],
) -> Vec<Vec<usize>> {
    let mut best: Vec<Vec<usize>> = Vec::new();
    for start in carbons {
        let mut path = vec![*start];
        let mut visited = vec![false; molecule.symbols.len()];
        visited[*start] = true;
        extend_path(molecule, &mut path, &mut visited, &mut best, required);
    }
    best
}

fn extend_path(
    molecule: &Editable,
    path: &mut Vec<usize>,
    visited: &mut Vec<bool>,
    best: &mut Vec<Vec<usize>>,
    required: &[usize],
) {
    let mut extended = false;
    let current = *path.last().expect("paths start non-empty");
    let neighbours: Vec<usize> = molecule
        .bonds
        .iter()
        .filter_map(|(left, right, _)| {
            if *left == current {
                Some(*right)
            } else if *right == current {
                Some(*left)
            } else {
                None
            }
        })
        .collect();
    for neighbour in neighbours {
        if molecule.symbols[neighbour] == "C" && !visited[neighbour] {
            visited[neighbour] = true;
            path.push(neighbour);
            extend_path(molecule, path, visited, best, required);
            path.pop();
            visited[neighbour] = false;
            extended = true;
        }
    }
    if !extended && required.iter().all(|atom| path.contains(atom)) {
        match best.first().map(Vec::len) {
            Some(length) if path.len() < length => {}
            Some(length) if path.len() == length => best.push(path.clone()),
            _ => *best = vec![path.clone()],
        }
    }
}

/// The name for one directed chain, with its preference score, or None
/// when a substituent is not a simple alkyl or halo group.
#[allow(clippy::too_many_lines)]
fn name_for_chain(
    molecule: &Editable,
    chain: &[usize],
    acid: Option<crate::organic::Carboxyl>,
    hydroxyls: &[(usize, usize)],
    carbonyl: Option<(usize, usize)>,
    unsaturation: Option<&(usize, usize, u8)>,
) -> Option<(Score, String)> {
    let position_of = |atom: usize| chain.iter().position(|entry| *entry == atom).map(|p| p + 1);
    let in_chain = |atom: usize| chain.contains(&atom);

    // Principal locants.
    let acid_locant = match acid {
        Some(group) => {
            if position_of(group.carbon) != Some(1) {
                return None;
            }
            Some(1)
        }
        None => None,
    };
    let mut hydroxyl_locants: Vec<usize> = hydroxyls
        .iter()
        .map(|(carbon, _)| position_of(*carbon))
        .collect::<Option<_>>()?;
    hydroxyl_locants.sort_unstable();
    let carbonyl_locant = match carbonyl {
        Some((carbon, _)) => {
            let locant = position_of(carbon)?;
            // Aldehydes carry the carbonyl at the chain head; ketones
            // strictly inside it.
            if locant != 1 && locant >= chain.len() {
                return None;
            }
            Some(locant)
        }
        None => None,
    };
    let unsaturation_locant = match unsaturation {
        Some((left, right, _)) => {
            let (a, b) = (position_of(*left)?, position_of(*right)?);
            if a.abs_diff(b) != 1 {
                return None;
            }
            Some(a.min(b))
        }
        None => None,
    };

    // Substituents: everything hanging off the chain must be a halo atom
    // or an unbranched alkyl of 1-3 carbons.
    let skip: Vec<usize> = acid
        .map(|group| {
            let mut atoms = vec![group.hydroxyl_oxygen];
            atoms.extend(
                molecule
                    .neighbours(group.carbon)
                    .filter(|(atom, order)| molecule.symbols[*atom] == "O" && *order == 2)
                    .map(|(atom, _)| atom),
            );
            atoms
        })
        .unwrap_or_default();
    let mut hydroxyl_oxygens: Vec<usize> = hydroxyls.iter().map(|(_, oxygen)| *oxygen).collect();
    if let Some((_, oxygen)) = carbonyl {
        hydroxyl_oxygens.push(oxygen);
    }
    let mut substituents: Vec<(usize, String)> = Vec::new();
    for (position, atom) in chain.iter().enumerate() {
        for (neighbour, order) in molecule.neighbours(*atom) {
            if in_chain(neighbour)
                || skip.contains(&neighbour)
                || hydroxyl_oxygens.contains(&neighbour)
            {
                continue;
            }
            if order != 1 {
                return None;
            }
            let name = match molecule.symbols[neighbour].as_str() {
                "F" => "fluoro".to_owned(),
                "Cl" => "chloro".to_owned(),
                "Br" => "bromo".to_owned(),
                "I" => "iodo".to_owned(),
                "C" => alkyl_name(molecule, *atom, neighbour)?,
                _ => return None,
            };
            substituents.push((position + 1, name));
        }
    }

    // Assemble.
    let root = ROOTS.get(chain.len() - 1)?;
    let suffix = if let Some(locant) = acid_locant {
        let _ = locant;
        format!("{root}anoic acid")
    } else if let Some(locant) = carbonyl_locant {
        if locant == 1 {
            format!("{root}anal")
        } else if chain.len() == 3 {
            format!("{root}anone")
        } else {
            format!("{root}an-{locant}-one")
        }
    } else if hydroxyl_locants.len() > 1 {
        let glue = if hydroxyl_locants.len() == 2 {
            "diol"
        } else {
            "triol"
        };
        let locant_text = hydroxyl_locants
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",");
        format!("{root}ane-{locant_text}-{glue}")
    } else if let Some(locant) = hydroxyl_locants.first() {
        if chain.len() <= 2 {
            format!("{root}anol")
        } else {
            format!("{root}an-{locant}-ol")
        }
    } else if let Some((_, _, order)) = unsaturation {
        let glue = if *order == 2 { "ene" } else { "yne" };
        let locant = unsaturation_locant?;
        if chain.len() <= 3 && locant == 1 {
            format!("{root}{glue}")
        } else {
            format!("{root}-{locant}-{glue}")
        }
    } else {
        format!("{root}ane")
    };

    // Group substituents by name, alphabetically, with sorted locants.
    let mut grouped: std::collections::BTreeMap<String, Vec<usize>> =
        std::collections::BTreeMap::new();
    for (locant, name) in &substituents {
        grouped.entry(name.clone()).or_default().push(*locant);
    }
    let mut prefix = String::new();
    for (name, mut locants) in grouped {
        locants.sort_unstable();
        let multiplier = MULTIPLIERS.get(locants.len() - 1)?;
        if !prefix.is_empty() {
            prefix.push('-');
        }
        let locant_text = locants
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",");
        // Locants may be omitted only where position is forced.
        if chain.len() <= 2 && locants.len() == 1 {
            prefix.push_str(&name);
        } else {
            write!(&mut prefix, "{locant_text}-{multiplier}{name}")
                .expect("writing to a String cannot fail");
        }
    }
    let name = format!("{prefix}{suffix}");

    let principal = if let Some(locant) = acid_locant {
        vec![locant]
    } else if let Some(locant) = carbonyl_locant {
        vec![locant]
    } else if !hydroxyl_locants.is_empty() {
        hydroxyl_locants
    } else {
        unsaturation_locant
            .map(|locant| vec![locant])
            .unwrap_or_default()
    };
    let mut substituent_locants: Vec<usize> =
        substituents.iter().map(|(locant, _)| *locant).collect();
    substituent_locants.sort_unstable();
    Some(((principal, substituent_locants, name.clone()), name))
}

/// The unbranched alkyl substituent starting at `first` (walked away from
/// the chain atom), or None when branched or longer than propyl.
fn alkyl_name(molecule: &Editable, chain_atom: usize, first: usize) -> Option<String> {
    let mut length = 0;
    let mut previous = chain_atom;
    let mut current = first;
    loop {
        length += 1;
        if length > 3 {
            return None;
        }
        let next: Vec<usize> = molecule
            .neighbours(current)
            .filter(|(atom, _)| *atom != previous)
            .map(|(atom, _)| atom)
            .collect();
        match next.as_slice() {
            [] => break,
            [one] if molecule.symbols[*one] == "C" => {
                previous = current;
                current = *one;
            }
            _ => return None,
        }
    }
    Some(
        match length {
            1 => "methyl",
            2 => "ethyl",
            _ => "propyl",
        }
        .to_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editable(smiles: &str) -> Editable {
        let structure = chem_domain::structure_from_smiles(
            chem_domain::StructureId::new("t.name").unwrap(),
            smiles,
        )
        .expect("test smiles parses");
        Editable::from_structure(&structure).expect("molecular editable")
    }

    #[test]
    fn graphs_name_systematically() {
        for (smiles, expected) in [
            ("CCCCC", "pentane"),
            ("CC(C)CC", "2-methylbutane"),
            ("CC(C)(C)C", "2,2-dimethylpropane"),
            ("CC=CC", "but-2-ene"),
            ("C=CCC", "but-1-ene"),
            ("CC(O)CC", "butan-2-ol"),
            ("CCCC(=O)O", "butanoic acid"),
            ("CC(Br)C", "2-bromopropane"),
            ("CCC#CCC", "hex-3-yne"),
            ("CCCC(C)CC", "3-methylhexane"),
            ("CC(C)(C)CC(C)CC", "2,2,4-trimethylhexane"),
        ] {
            assert_eq!(
                systematic_name(&editable(smiles)).as_deref(),
                Some(expected),
                "{smiles}"
            );
        }
    }

    #[test]
    fn names_round_trip_through_the_parser() {
        for name in [
            "pentane",
            "2-methylbutane",
            "but-2-ene",
            "butan-2-ol",
            "butanoic acid",
            "2-bromopropane",
            "2,2-dimethylpropane",
            "ethane-1,2-diol",
            "propane-1,2,3-triol",
            "propyl ethanoate",
            "methyl butanoate",
            "ethanal",
            "propanal",
            "propanone",
            "butan-2-one",
            "pentan-3-one",
            "hexanal",
        ] {
            let smiles = chem_domain::smiles_for_name(name).expect(name);
            assert_eq!(
                systematic_name(&editable(&smiles)).as_deref(),
                Some(name),
                "{name} -> {smiles}"
            );
        }
    }

    #[test]
    fn out_of_subset_graphs_stay_unnamed() {
        // Esters now name via the two-word form.
        assert_eq!(
            systematic_name(&editable("CCOC(=O)C")).as_deref(),
            Some("ethyl ethanoate")
        );
        // Diols now name; a tetraol stays out of the subset.
        assert_eq!(
            systematic_name(&editable("OCCO")).as_deref(),
            Some("ethane-1,2-diol")
        );
        assert_eq!(systematic_name(&editable("OCC(O)C(O)CO")), None);
        // Branched substituent the main chain cannot absorb (isopropyl).
        assert_eq!(systematic_name(&editable("CCCC(C(C)C)CCC")), None);
    }
}

#[cfg(test)]
mod stereo_tests {
    #[test]
    fn stereo_names_round_trip_end_to_end() {
        for (name, arrangement) in [
            ("cis-but-2-ene", chem_domain::StereoArrangement::Cis),
            ("trans-but-2-ene", chem_domain::StereoArrangement::Trans),
            ("trans-pent-2-ene", chem_domain::StereoArrangement::Trans),
        ] {
            let smiles = chem_domain::resolved_name_smiles(name).expect(name);
            let structure = chem_domain::structure_from_smiles(
                chem_domain::StructureId::new("t.stereo-name").unwrap(),
                &smiles,
            )
            .expect("stereo smiles parses");
            let stereo = structure
                .graph()
                .covalent_bonds()
                .values()
                .find_map(chem_domain::CovalentBond::stereo)
                .expect("stereo descriptor present");
            assert_eq!(stereo.arrangement(), arrangement, "{name}");
            assert_eq!(
                crate::naming::structure_name(&structure).as_deref(),
                Some(name),
                "display name round-trips"
            );
        }
        // The plain name stays unprefixed, and impossible prefixes fail.
        assert!(chem_domain::resolved_name_smiles("but-2-ene").is_some());
        assert!(
            chem_domain::resolved_name_smiles("cis-but-1-ene").is_none(),
            "terminal alkenes have no cis/trans"
        );
        assert!(chem_domain::resolved_name_smiles("cis-butane").is_none());
    }
}

#[cfg(test)]
mod chirality_tests {
    #[test]
    fn rs_names_round_trip_end_to_end() {
        for (name, other) in [
            ("(R)-butan-2-ol", "(S)-butan-2-ol"),
            ("(S)-2-bromobutane", "(R)-2-bromobutane"),
        ] {
            let smiles = chem_domain::resolved_name_smiles(name).expect(name);
            let sibling = chem_domain::resolved_name_smiles(other).expect(other);
            assert_ne!(smiles, sibling, "enantiomer names build distinct SMILES");
            let structure = chem_domain::structure_from_smiles(
                chem_domain::StructureId::new("t.rs-name").unwrap(),
                &smiles,
            )
            .expect("chiral smiles parses");
            assert_eq!(
                crate::naming::structure_name(&structure).as_deref(),
                Some(name),
                "display name round-trips"
            );
        }
        // Not a stereocentre: both variants fail the descriptor check.
        assert!(chem_domain::resolved_name_smiles("(R)-propan-2-ol").is_none());
        assert!(chem_domain::resolved_name_smiles("(R)-butane").is_none());
    }
}
