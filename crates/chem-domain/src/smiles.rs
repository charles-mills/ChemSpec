//! A deliberately small SMILES subset: enough to write classroom molecules
//! and simple ions down as data instead of hand-built graphs.
//!
//! Supported: bare organic-subset atoms (B C N O P S F Cl Br I), bracket
//! atoms with an explicit hydrogen count and charge (`[NH4+]`, `[O-]`),
//! single/double/triple bonds (`-`, `=`, `#`), branches, single-digit ring
//! closures, and `.`-separated components. Kekulé input only — no aromatic
//! lowercase, no stereochemistry, no isotopes. Anything outside the subset
//! parses to None: a wrong molecule is worse than none.

use crate::generate::{SolvedUnit, build_molecular, ionic_structure};
use crate::identity::{AtomId, StructureId};
use crate::periodic::{ELEMENT_SYMBOLS, valence_electrons_of};
use crate::formula::ElementSymbol;
use crate::structural::{BondOrder, ElementInventory, RepresentationKind, StructureDefinition};

/// Elements writable without brackets, with their implicit-hydrogen valence.
const ORGANIC_SUBSET: [(&str, u8); 10] = [
    ("B", 3),
    ("C", 4),
    ("N", 3),
    ("O", 2),
    ("P", 3),
    ("S", 2),
    ("F", 1),
    ("Cl", 1),
    ("Br", 1),
    ("I", 1),
];

/// The implicit-hydrogen valence of a bare organic-subset atom.
#[must_use]
pub fn subset_valence(symbol: &str) -> Option<u8> {
    ORGANIC_SUBSET
        .iter()
        .find(|(candidate, _)| *candidate == symbol)
        .map(|(_, valence)| *valence)
}

#[derive(Debug, Clone)]
struct ParsedAtom {
    symbol: String,
    /// Bracket atoms carry their hydrogen count explicitly; bare atoms
    /// derive it from the subset valence.
    explicit_hydrogens: Option<u8>,
    charge: i16,
    /// Lowercase aromatic notation: the atom takes part in kekulization.
    aromatic: bool,
    /// Bracket `@` / `@@`: tetrahedral handedness against written order.
    chiral: Option<crate::structural::TetrahedralHandedness>,
}

/// One entry in an atom's written neighbour order, as SMILES chirality
/// defines it: the preceding atom first, a bracket hydrogen next, then
/// ring closures and children as written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NeighbourToken {
    Heavy(usize),
    ImplicitHydrogen,
    /// A ring digit seen at its opening atom; resolved at closure.
    Ring(u8),
}

#[derive(Debug, Default, Clone)]
struct Component {
    atoms: Vec<ParsedAtom>,
    /// (left, right, order 1..=3) over heavy-atom indices.
    bonds: Vec<(usize, usize, u8)>,
    /// Directional single bonds as written: (earlier atom, later atom, up).
    directions: Vec<(usize, usize, bool)>,
    /// Indices into `bonds` written between two aromatic atoms with no
    /// explicit order; kekulization resolves them to alternating orders.
    aromatic_bonds: Vec<usize>,
    /// Written neighbour order per atom, for chirality.
    orders: Vec<Vec<NeighbourToken>>,
}

/// Parses the subset into components of heavy atoms and bonds.
#[allow(clippy::too_many_lines)]
fn parse(smiles: &str) -> Option<Vec<Component>> {
    let mut components = Vec::new();
    let mut component = Component::default();
    let mut previous: Option<usize> = None;
    let mut branch_stack: Vec<Option<usize>> = Vec::new();
    let mut pending_order: Option<u8> = None;
    let mut pending_direction: Option<bool> = None;
    let mut rings: [Option<(usize, Option<u8>)>; 10] = [None; 10];
    let bytes = smiles.as_bytes();
    let mut position = 0;

    let bond = |component: &mut Component,
                    previous: &mut Option<usize>,
                    pending: &mut Option<u8>,
                    direction: &mut Option<bool>,
                    next: usize| {
        if let Some(left) = *previous {
            let explicit = pending.take();
            let aromatic_default = explicit.is_none()
                && component.atoms[left].aromatic
                && component.atoms[next].aromatic;
            component.bonds.push((left, next, explicit.unwrap_or(1)));
            if aromatic_default {
                component.aromatic_bonds.push(component.bonds.len() - 1);
            }
            component.orders[left].push(NeighbourToken::Heavy(next));
            component.orders[next].push(NeighbourToken::Heavy(left));
            if let Some(up) = direction.take() {
                component.directions.push((left, next, up));
            }
        }
        *previous = Some(next);
    };

    while position < bytes.len() {
        match bytes[position] {
            b'-' => {
                pending_order = Some(1);
                position += 1;
            }
            b'/' => {
                pending_direction = Some(true);
                position += 1;
            }
            b'\\' => {
                pending_direction = Some(false);
                position += 1;
            }
            b'=' => {
                pending_order = Some(2);
                position += 1;
            }
            b'#' => {
                pending_order = Some(3);
                position += 1;
            }
            b'(' => {
                branch_stack.push(previous);
                position += 1;
            }
            b')' => {
                previous = branch_stack.pop()?;
                position += 1;
            }
            b'.' => {
                if component.atoms.is_empty() || previous.is_none() && !branch_stack.is_empty() {
                    return None;
                }
                if branch_stack.pop().is_some() || rings.iter().any(Option::is_some) {
                    return None;
                }
                components.push(std::mem::take(&mut component));
                previous = None;
                pending_order = None;
                pending_direction = None;
                position += 1;
            }
            digit @ b'1'..=b'9' => {
                if pending_direction.is_some() {
                    // Stereo on ring closures is out of the subset.
                    return None;
                }
                let index = usize::from(digit - b'0');
                let current = previous?;
                match rings[index].take() {
                    None => {
                        rings[index] = Some((current, pending_order.take()));
                        component.orders[current]
                            .push(NeighbourToken::Ring(u8::try_from(index).ok()?));
                    }
                    Some((open, open_order)) => {
                        if open == current {
                            return None;
                        }
                        let explicit = pending_order.take().or(open_order);
                        let aromatic_default = explicit.is_none()
                            && component.atoms[open].aromatic
                            && component.atoms[current].aromatic;
                        component.bonds.push((open, current, explicit.unwrap_or(1)));
                        if aromatic_default {
                            component.aromatic_bonds.push(component.bonds.len() - 1);
                        }
                        let digit_token = NeighbourToken::Ring(u8::try_from(index).ok()?);
                        for token in &mut component.orders[open] {
                            if *token == digit_token {
                                *token = NeighbourToken::Heavy(current);
                                break;
                            }
                        }
                        component.orders[current].push(NeighbourToken::Heavy(open));
                    }
                }
                position += 1;
            }
            b'[' => {
                let close = position + bytes[position..].iter().position(|byte| *byte == b']')?;
                let atom = parse_bracket(&smiles[position + 1..close])?;
                let hydrogen_marker =
                    atom.chiral.is_some() && atom.explicit_hydrogens == Some(1);
                if atom.chiral.is_some() && atom.explicit_hydrogens.unwrap_or(0) > 1 {
                    // A stereocentre cannot carry two hydrogens.
                    return None;
                }
                component.atoms.push(atom);
                component.orders.push(Vec::new());
                let next = component.atoms.len() - 1;
                bond(
                    &mut component,
                    &mut previous,
                    &mut pending_order,
                    &mut pending_direction,
                    next,
                );
                if hydrogen_marker {
                    component.orders[next].push(NeighbourToken::ImplicitHydrogen);
                }
                position = close + 1;
            }
            byte @ (b'c' | b'n' | b'o' | b's') if !byte.is_ascii_uppercase() => {
                component.atoms.push(ParsedAtom {
                    symbol: char::from(byte).to_ascii_uppercase().to_string(),
                    explicit_hydrogens: None,
                    charge: 0,
                    aromatic: true,
                    chiral: None,
                });
                component.orders.push(Vec::new());
                let next = component.atoms.len() - 1;
                bond(
                    &mut component,
                    &mut previous,
                    &mut pending_order,
                    &mut pending_direction,
                    next,
                );
                position += 1;
            }
            b'A'..=b'Z' => {
                // Two-letter subset symbols (Cl, Br) win over one-letter.
                let two = smiles.get(position..position + 2);
                let symbol = match two {
                    Some(pair) if subset_valence(pair).is_some() => pair,
                    _ => {
                        let one = &smiles[position..=position];
                        subset_valence(one)?;
                        one
                    }
                };
                component.atoms.push(ParsedAtom {
                    symbol: symbol.to_owned(),
                    explicit_hydrogens: None,
                    charge: 0,
                    aromatic: false,
                    chiral: None,
                });
                component.orders.push(Vec::new());
                let next = component.atoms.len() - 1;
                bond(
                    &mut component,
                    &mut previous,
                    &mut pending_order,
                    &mut pending_direction,
                    next,
                );
                position += symbol.len();
            }
            _ => return None,
        }
    }
    if component.atoms.is_empty()
        || !branch_stack.is_empty()
        || pending_order.is_some()
        || pending_direction.is_some()
        || rings.iter().any(Option::is_some)
    {
        return None;
    }
    components.push(component);
    for component in &mut components {
        kekulize(component)?;
    }
    Some(components)
}

/// Backtracking perfect matching for kekulization.
fn assign(
    remaining: &[usize],
    edges: &[(usize, usize)],
    matched: &mut std::collections::BTreeMap<usize, usize>,
) -> bool {
    let Some((atom, rest)) = remaining.split_first() else {
        return true;
    };
    if matched.contains_key(atom) {
        return assign(rest, edges, matched);
    }
    for (left, right) in edges {
        let partner = if left == atom {
            *right
        } else if right == atom {
            *left
        } else {
            continue;
        };
        if matched.contains_key(&partner) {
            continue;
        }
        matched.insert(*atom, partner);
        matched.insert(partner, *atom);
        if assign(rest, edges, matched) {
            return true;
        }
        matched.remove(atom);
        matched.remove(&partner);
    }
    false
}

/// Resolves lowercase-aromatic notation to a Kekulé structure: aromatic
/// carbons and pyridine-type nitrogens each take exactly one double bond
/// (a perfect matching over the aromatic edges), while aromatic oxygen and
/// sulfur contribute lone pairs and keep single bonds. No matching — a
/// lone aromatic atom, pyrrole-style rings needing `[nH]` — fails closed.
fn kekulize(component: &mut Component) -> Option<()> {
    if component.aromatic_bonds.is_empty() {
        // Aromatic atoms without aromatic bonds are invalid notation.
        return component
            .atoms
            .iter()
            .all(|atom| !atom.aromatic)
            .then_some(());
    }
    let needs_double: Vec<usize> = component
        .atoms
        .iter()
        .enumerate()
        .filter(|(_, atom)| atom.aromatic && matches!(atom.symbol.as_str(), "C" | "N"))
        .map(|(index, _)| index)
        .collect();
    // Backtracking perfect matching over the aromatic edges.
    let edges: Vec<(usize, usize)> = component
        .aromatic_bonds
        .iter()
        .map(|bond| (component.bonds[*bond].0, component.bonds[*bond].1))
        .collect();
    let mut matched: std::collections::BTreeMap<usize, usize> = std::collections::BTreeMap::new();
    // Only carbon/nitrogen participate in the matching; a partner outside
    // needs_double (aromatic O/S) is not a valid double-bond end.
    let member = |atom: usize| needs_double.contains(&atom);
    let matching_edges: Vec<(usize, usize)> = edges
        .iter()
        .filter(|(left, right)| member(*left) && member(*right))
        .copied()
        .collect();
    if !assign(&needs_double, &matching_edges, &mut matched) {
        return None;
    }
    for bond_index in &component.aromatic_bonds {
        let (left, right, _) = component.bonds[*bond_index];
        if matched.get(&left) == Some(&right) {
            component.bonds[*bond_index].2 = 2;
        }
    }
    Some(())
}

/// Parses the inside of a bracket atom: `Symbol`, optional `H`/`Hn`,
/// optional charge (`+`, `-`, `+n`, `-n`).
fn parse_bracket(inner: &str) -> Option<ParsedAtom> {
    let bytes = inner.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let symbol_length = if bytes.len() >= 2
        && bytes[1].is_ascii_lowercase()
        && ELEMENT_SYMBOLS.contains(&&inner[..2])
    {
        2
    } else if ELEMENT_SYMBOLS.contains(&&inner[..1]) {
        1
    } else {
        return None;
    };
    let symbol = inner[..symbol_length].to_owned();
    let mut rest = &inner[symbol_length..];
    let chiral = if let Some(after) = rest.strip_prefix("@@") {
        rest = after;
        Some(crate::structural::TetrahedralHandedness::Clockwise)
    } else if let Some(after) = rest.strip_prefix('@') {
        rest = after;
        Some(crate::structural::TetrahedralHandedness::Counterclockwise)
    } else {
        None
    };
    let mut hydrogens = 0_u8;
    if let Some(after) = rest.strip_prefix('H') {
        let digits = after
            .bytes()
            .take_while(u8::is_ascii_digit)
            .count();
        hydrogens = if digits == 0 {
            1
        } else {
            after[..digits].parse().ok()?
        };
        rest = &after[digits..];
    }
    let charge = match rest {
        "" => 0,
        "+" => 1,
        "-" => -1,
        _ => {
            let (sign, digits) = rest.split_at(1);
            let magnitude: i16 = digits.parse().ok()?;
            match sign {
                "+" => magnitude,
                "-" => -magnitude,
                _ => return None,
            }
        }
    };
    Some(ParsedAtom {
        symbol,
        explicit_hydrogens: Some(hydrogens),
        charge,
        aromatic: false,
        chiral,
    })
}

/// Expands one parsed component into a covalent unit: implicit hydrogens
/// become explicit atoms, and each atom's non-bonding electrons follow the
/// formal-charge identity `lone = V - bonds - q`.
fn attached_hydrogens(component: &Component) -> Option<Vec<u8>> {
    let heavy = component.atoms.len();
    let mut order_sums = vec![0_u8; heavy];
    for (left, right, order) in &component.bonds {
        if *left >= heavy || *right >= heavy || !(1..=3).contains(order) {
            return None;
        }
        order_sums[*left] = order_sums[*left].checked_add(*order)?;
        order_sums[*right] = order_sums[*right].checked_add(*order)?;
    }
    component
        .atoms
        .iter()
        .enumerate()
        .map(|(index, atom)| match atom.explicit_hydrogens {
            Some(count) => Some(count),
            None => Some(subset_valence(&atom.symbol)?.saturating_sub(order_sums[index])),
        })
        .collect()
}

fn component_unit(component: &Component) -> Option<SolvedUnit> {
    let heavy = component.atoms.len();
    let hydrogens = attached_hydrogens(component)?;
    let mut order_sums = vec![0_u8; heavy];
    for (left, right, order) in &component.bonds {
        order_sums[*left] = order_sums[*left].checked_add(*order)?;
        order_sums[*right] = order_sums[*right].checked_add(*order)?;
    }
    let mut unit = SolvedUnit {
        symbols: Vec::new(),
        states: Vec::new(),
        bonds: component.bonds.clone(),
    };
    for (index, atom) in component.atoms.iter().enumerate() {
        let bonds = i16::from(order_sums[index]) + i16::from(hydrogens[index]);
        let valence = i16::from(valence_electrons_of(&atom.symbol)?);
        let lone = valence - bonds - atom.charge;
        if !(0..=8).contains(&lone) {
            return None;
        }
        unit.symbols.push(atom.symbol.clone());
        unit.states.push((atom.charge, u8::try_from(lone).ok()?));
    }
    for (index, count) in hydrogens.iter().enumerate() {
        for _ in 0..*count {
            unit.symbols.push("H".to_owned());
            unit.states.push((0, 0));
            unit.bonds.push((index, unit.symbols.len() - 1, 1));
        }
    }
    Some(unit)
}

/// A parsed stereocentre over unit slots: (atom slot, its four neighbour
/// slots in written order, handedness).
type DerivedChirality = (usize, [usize; 4], crate::structural::TetrahedralHandedness);

/// Resolves written chirality into slot form. The implicit bracket
/// hydrogen maps to the atom's first expanded hydrogen slot.
fn derived_chirality(component: &Component) -> Option<Vec<DerivedChirality>> {
    let heavy = component.atoms.len();
    let hydrogens = attached_hydrogens(component)?;
    let hydrogen_start: Vec<usize> = hydrogens
        .iter()
        .scan(heavy, |next, count| {
            let start = *next;
            *next += usize::from(*count);
            Some(start)
        })
        .collect();
    let mut result = Vec::new();
    for (index, atom) in component.atoms.iter().enumerate() {
        let Some(handedness) = atom.chiral else {
            continue;
        };
        let tokens = component.orders.get(index)?;
        let mut slots = Vec::new();
        for token in tokens {
            match token {
                NeighbourToken::Heavy(neighbour) => slots.push(*neighbour),
                NeighbourToken::ImplicitHydrogen => {
                    if hydrogens[index] != 1 {
                        return None;
                    }
                    slots.push(hydrogen_start[index]);
                }
                NeighbourToken::Ring(_) => return None,
            }
        }
        let slots: [usize; 4] = slots.try_into().ok()?;
        result.push((index, slots, handedness));
    }
    Some(result)
}

fn unit_charge(unit: &SolvedUnit) -> i16 {
    unit.states.iter().map(|(charge, _)| *charge).sum()
}

fn units_inventory(units: &[SolvedUnit]) -> Option<ElementInventory> {
    let mut counts = std::collections::BTreeMap::new();
    for unit in units {
        for symbol in &unit.symbols {
            let symbol = ElementSymbol::new(symbol).ok()?;
            *counts.entry(symbol).or_insert(0_u64) += 1;
        }
    }
    ElementInventory::new(counts).ok()
}

/// Builds a validated structure from subset SMILES: one neutral component
/// becomes a molecular structure, two or more charged components with a
/// zero net charge become an ionic one. Anything else is None.
#[must_use]
pub fn structure_from_smiles(id: StructureId, smiles: &str) -> Option<StructureDefinition> {
    let components = parse(smiles.trim())?;
    let units = components
        .iter()
        .map(component_unit)
        .collect::<Option<Vec<_>>>()?;
    let inventory = units_inventory(&units)?;
    match units.as_slice() {
        [unit] if unit_charge(unit) == 0 => {
            let stereo = derived_stereo(&components[0])?;
            let chirality = derived_chirality(&components[0])?;
            let structure = build_molecular(id, &inventory, unit)?;
            decorate_molecular(structure, &stereo, &chirality)
        }
        _ => {
            if components.iter().any(|component| {
                !component.directions.is_empty()
                    || component.atoms.iter().any(|atom| atom.chiral.is_some())
            }) {
                // Stereo and chiral ions are out of the subset.
                return None;
            }
            let net: i16 = units.iter().map(unit_charge).sum();
            if net != 0 || units.iter().any(|unit| unit_charge(unit) == 0) {
                return None;
            }
            ionic_structure(id, &inventory, &units)
        }
    }
}

/// One double bond's parsed geometry over heavy-atom indices.
struct DerivedStereo {
    left: usize,
    right: usize,
    left_reference: usize,
    right_reference: usize,
    arrangement: crate::structural::StereoArrangement,
}

/// Resolves written bond directions into per-double-bond arrangements.
/// Conflicting or dangling directions fail the whole parse.
fn derived_stereo(component: &Component) -> Option<Vec<DerivedStereo>> {
    // The side of the double-bond axis a neighbour sits on: '/' written
    // neighbour-first puts it below, neighbour-second above; '\' inverts.
    let sides = |atom: usize, other: usize| -> Option<Vec<(usize, bool)>> {
        let mut found: Vec<(usize, bool)> = Vec::new();
        for (from, to, up) in &component.directions {
            let entry = if *from == atom && *to != other {
                (*to, *up)
            } else if *to == atom && *from != other {
                (*from, !*up)
            } else {
                continue;
            };
            found.push(entry);
        }
        // Two marked substituents on one end must sit on opposite sides.
        if let [(_, first), (_, second)] = found.as_slice()
            && first == second
        {
            return None;
        }
        (found.len() <= 2).then_some(found)
    };
    let mut result = Vec::new();
    for (left, right, order) in &component.bonds {
        if *order != 2 {
            continue;
        }
        let left_sides = sides(*left, *right)?;
        let right_sides = sides(*right, *left)?;
        let (Some((left_reference, left_position)), Some((right_reference, right_position))) =
            (left_sides.first(), right_sides.first())
        else {
            continue;
        };
        result.push(DerivedStereo {
            left: *left,
            right: *right,
            left_reference: *left_reference,
            right_reference: *right_reference,
            arrangement: if left_position == right_position {
                crate::structural::StereoArrangement::Cis
            } else {
                crate::structural::StereoArrangement::Trans
            },
        });
    }
    Some(result)
}

/// Rebuilds a freshly built molecular structure with stereo descriptors on
/// the named double bonds and chirality descriptors on stereocentres.
/// Slot `i` is atom `a{i}` by construction.
fn decorate_molecular(
    structure: StructureDefinition,
    stereo: &[DerivedStereo],
    chirality: &[DerivedChirality],
) -> Option<StructureDefinition> {
    use crate::structural::CovalentBond;
    if stereo.is_empty() && chirality.is_empty() {
        return Some(structure);
    }
    let atom_id = |index: usize| AtomId::new(format!("a{index}")).ok();
    let mut stereo_by_edge = std::collections::BTreeMap::new();
    for entry in stereo {
        let (left, right) = (atom_id(entry.left)?, atom_id(entry.right)?);
        let key = if left < right {
            (left.clone(), right.clone())
        } else {
            (right.clone(), left.clone())
        };
        stereo_by_edge.insert(key, entry);
    }
    let graph = structure.graph();
    let bonds = graph
        .covalent_bonds()
        .values()
        .map(|bond| {
            let key = (bond.left().clone(), bond.right().clone());
            match stereo_by_edge.get(&key) {
                Some(entry) => CovalentBond::new_stereo(
                    bond.id().clone(),
                    atom_id(entry.left)?,
                    atom_id(entry.right)?,
                    bond.order(),
                    atom_id(entry.left_reference)?,
                    atom_id(entry.right_reference)?,
                    entry.arrangement,
                )
                .ok(),
                None => Some(bond.clone()),
            }
        })
        .collect::<Option<Vec<_>>>()?;
    let atoms = graph
        .atoms()
        .values()
        .map(|atom| {
            let slot = chirality.iter().find_map(|(index, neighbours, handedness)| {
                (atom_id(*index)? == *atom.id()).then_some((neighbours, *handedness))
            });
            match slot {
                Some((neighbours, handedness)) => {
                    let listed = [
                        atom_id(neighbours[0])?,
                        atom_id(neighbours[1])?,
                        atom_id(neighbours[2])?,
                        atom_id(neighbours[3])?,
                    ];
                    let descriptor =
                        crate::structural::TetrahedralChirality::new(listed, handedness).ok()?;
                    Some(atom.clone().with_chirality(descriptor))
                }
                None => Some(atom.clone()),
            }
        })
        .collect::<Option<Vec<_>>>()?;
    let rebuilt = crate::structural::StructuralGraph::new(atoms, bonds, [], [], []).ok()?;
    StructureDefinition::new(
        structure.id().clone(),
        structure.formula().clone(),
        RepresentationKind::Molecular,
        rebuilt,
    )
    .ok()
}

/// Builds a molecular structure from a heavy-atom sketch graph: implicit
/// hydrogens follow the organic-subset valences, exactly as bare SMILES
/// atoms do. The sketcher's submit path.
#[must_use]
pub fn structure_from_heavy_graph(
    id: StructureId,
    symbols: &[&str],
    bonds: &[(usize, usize, u8)],
) -> Option<StructureDefinition> {
    let component = Component {
        atoms: symbols
            .iter()
            .map(|symbol| ParsedAtom {
                symbol: (*symbol).to_owned(),
                explicit_hydrogens: None,
                charge: 0,
                aromatic: false,
                chiral: None,
            })
            .collect(),
        bonds: bonds.to_vec(),
        directions: Vec::new(),
        aromatic_bonds: Vec::new(),
        orders: Vec::new(),
    };
    let unit = component_unit(&component)?;
    if unit_charge(&unit) != 0 {
        return None;
    }
    let inventory = units_inventory(std::slice::from_ref(&unit))?;
    build_molecular(id, &inventory, &unit)
}

/// Writes subset SMILES for a validated structure: molecular graphs as one
/// component, ionic graphs as one component per ion group. Hydrogens fold
/// into their heavy atom. None for metallic structures, bare ions, and
/// hydrogen-only components.
#[must_use]
pub fn smiles_from_structure(structure: &StructureDefinition) -> Option<String> {
    let graph = structure.graph();
    let atom_ids: Vec<_> = graph.atoms().keys().collect();
    match structure.representation() {
        RepresentationKind::Molecular => write_component(structure, &atom_ids),
        RepresentationKind::Ionic => {
            if graph
                .covalent_bonds()
                .values()
                .any(|bond| bond.stereo().is_some())
            {
                return None;
            }
            let mut components = graph
                .groups()
                .values()
                .map(|group| {
                    let members: Vec<_> = atom_ids
                        .iter()
                        .filter(|id| group.atoms().contains(**id))
                        .copied()
                        .collect();
                    write_component(structure, &members)
                })
                .collect::<Option<Vec<_>>>()?;
            // Canonical component order, independent of group labels.
            components.sort();
            Some(components.join("."))
        }
        RepresentationKind::Ion | RepresentationKind::Metallic => None,
    }
}

const fn order_symbol(order: BondOrder) -> &'static str {
    match order {
        BondOrder::Single => "",
        BondOrder::Double => "=",
        BondOrder::Triple => "#",
    }
}

#[allow(clippy::too_many_lines)]
fn write_component(
    structure: &StructureDefinition,
    members: &[&AtomId],
) -> Option<String> {
    use std::collections::{BTreeMap, BTreeSet};
    let graph = structure.graph();
    let member_set: BTreeSet<_> = members.iter().copied().collect();
    // Adjacency over heavy atoms; hydrogens fold into counts.
    let mut neighbours: BTreeMap<&AtomId, Vec<(&AtomId, BondOrder)>> =
        BTreeMap::new();
    let mut hydrogen_counts: BTreeMap<&AtomId, u8> = BTreeMap::new();
    let is_hydrogen =
        |id: &AtomId| graph.atoms()[id].element().as_str() == "H";
    for bond in graph.covalent_bonds().values() {
        let (left, right) = (bond.left(), bond.right());
        if !member_set.contains(left) || !member_set.contains(right) {
            continue;
        }
        match (is_hydrogen(left), is_hydrogen(right)) {
            (false, false) => {
                neighbours.entry(left).or_default().push((right, bond.order()));
                neighbours.entry(right).or_default().push((left, bond.order()));
            }
            (false, true) => *hydrogen_counts.entry(left).or_default() += 1,
            (true, false) => *hydrogen_counts.entry(right).or_default() += 1,
            // ponytail: H-H bonds (molecular hydrogen) stay unwritable;
            // nothing organic needs them as SMILES.
            (true, true) => return None,
        }
    }
    let heavy: Vec<_> = members
        .iter()
        .filter(|id| !is_hydrogen(id))
        .copied()
        .collect();
    // Canonical ranks make the emitted SMILES label-insensitive: any
    // relabelling of the same molecule writes the identical string.
    let ranks = canonical_ranks(structure, &heavy, &neighbours, &hydrogen_counts);
    for list in neighbours.values_mut() {
        list.sort_by_key(|(neighbour, order)| (ranks.get(*neighbour).copied(), *order));
    }
    let start = *heavy
        .iter()
        .min_by_key(|id| ranks.get(**id).copied())?;

    // Iterative DFS emitting atoms, branches, and ring-closure digits.
    let mut visited = BTreeSet::new();
    let mut used_edges = BTreeSet::new();
    let mut closures: BTreeMap<&AtomId, Vec<(u8, BondOrder, &AtomId)>> = BTreeMap::new();
    let mut next_digit = 1_u8;
    let edge_key = |a: &AtomId, b: &AtomId| {
        let (a, b) = if a <= b { (a, b) } else { (b, a) };
        (a.clone(), b.clone())
    };
    // First pass: find ring-closure edges via DFS.
    let mut stack = vec![start];
    let mut parents: BTreeMap<&AtomId, &AtomId> =
        BTreeMap::new();
    visited.insert(start);
    while let Some(current) = stack.pop() {
        for (neighbour, order) in neighbours.get(current).into_iter().flatten() {
            let key = edge_key(current, neighbour);
            if used_edges.contains(&key) {
                continue;
            }
            if visited.insert(*neighbour) {
                parents.insert(neighbour, current);
                used_edges.insert(key);
                stack.push(neighbour);
            } else if parents.get(current) != Some(neighbour) {
                // Back edge: assign a ring digit to both endpoints.
                used_edges.insert(key);
                if next_digit > 9 {
                    return None;
                }
                closures
                    .entry(current)
                    .or_default()
                    .push((next_digit, *order, *neighbour));
                closures
                    .entry(neighbour)
                    .or_default()
                    .push((next_digit, *order, current));
                next_digit += 1;
            }
        }
    }
    if visited.len() != heavy.len() {
        return None;
    }

    // Stereo double bonds mark their reference single bonds with a
    // direction each; the left reference is arbitrarily "up" and the right
    // follows the arrangement, which round-trips exactly.
    let mut stereo_marks: BTreeMap<(AtomId, AtomId), (AtomId, bool)> = BTreeMap::new();
    for bond in graph.covalent_bonds().values() {
        let Some(stereo) = bond.stereo() else {
            continue;
        };
        if !member_set.contains(bond.left()) {
            continue;
        }
        let right_position =
            stereo.arrangement() == crate::structural::StereoArrangement::Cis;
        for (endpoint, reference, position) in [
            (bond.left(), stereo.left_reference(), true),
            (bond.right(), stereo.right_reference(), right_position),
        ] {
            let key = edge_key(endpoint, reference);
            if stereo_marks
                .insert(key, (reference.clone(), position))
                .is_some()
            {
                // A reference bond claimed by two stereo bonds is out of
                // the subset.
                return None;
            }
        }
    }

    // Second pass: emit the spanning tree recursively.
    let mut output = String::new();
    let context = EmitContext {
        structure,
        neighbours: &neighbours,
        parents: &parents,
        closures: &closures,
        hydrogen_counts: &hydrogen_counts,
        stereo_marks: &stereo_marks,
    };
    let mut emitted = BTreeSet::new();
    emit(&context, start, None, &mut emitted, &mut output)?;
    Some(output)
}

/// Canonical heavy-atom ranks by iterative neighbourhood refinement
/// (Morgan/Weisfeiler-Leman) with deterministic tie-splitting: atoms in
/// the same automorphism orbit may share a split order, which cannot
/// change the emitted string.
// ponytail: WL-1 refinement; pathological regular graphs could fool it,
// but nothing under the 24-atom classroom cap does.
fn canonical_ranks<'a>(
    structure: &StructureDefinition,
    heavy: &[&'a AtomId],
    neighbours: &std::collections::BTreeMap<&'a AtomId, Vec<(&'a AtomId, BondOrder)>>,
    hydrogen_counts: &std::collections::BTreeMap<&'a AtomId, u8>,
) -> std::collections::BTreeMap<&'a AtomId, usize> {
    use std::collections::BTreeMap;
    let graph = structure.graph();
    let mut labels: BTreeMap<&'a AtomId, String> = heavy
        .iter()
        .map(|id| {
            let atom = &graph.atoms()[*id];
            let degree = neighbours.get(*id).map_or(0, Vec::len);
            (
                *id,
                format!(
                    "{}|{}|{}|{degree}",
                    atom.element().as_str(),
                    atom.electrons().formal_charge(),
                    hydrogen_counts.get(*id).copied().unwrap_or(0),
                ),
            )
        })
        .collect();
    let rank_of = |labels: &BTreeMap<&'a AtomId, String>| -> BTreeMap<&'a AtomId, usize> {
        let mut distinct: Vec<&String> = labels.values().collect();
        distinct.sort();
        distinct.dedup();
        labels
            .iter()
            .map(|(id, label)| {
                (
                    *id,
                    distinct
                        .binary_search(&label)
                        .expect("label drawn from the set"),
                )
            })
            .collect()
    };
    let mut ranks = rank_of(&labels);
    loop {
        // Refine until stable.
        loop {
            let refined: BTreeMap<&'a AtomId, String> = heavy
                .iter()
                .map(|id| {
                    let mut around: Vec<String> = neighbours
                        .get(*id)
                        .into_iter()
                        .flatten()
                        .map(|(neighbour, order)| {
                            format!("{:?}:{}", order, ranks[*neighbour])
                        })
                        .collect();
                    around.sort();
                    (*id, format!("{}<{}>", ranks[*id], around.join(",")))
                })
                .collect();
            let next = rank_of(&refined);
            if next == ranks {
                break;
            }
            labels = refined;
            ranks = next;
        }
        let _ = &labels;
        // Split the first remaining tie deterministically and re-refine.
        let mut by_rank: BTreeMap<usize, Vec<&AtomId>> = BTreeMap::new();
        for (id, rank) in &ranks {
            by_rank.entry(*rank).or_default().push(*id);
        }
        let Some(tied) = by_rank.values().find(|group| group.len() > 1) else {
            return ranks;
        };
        let chosen = tied[0];
        let promoted: BTreeMap<&'a AtomId, String> = ranks
            .iter()
            .map(|(id, rank)| {
                let marker = usize::from(*id != chosen);
                (*id, format!("{rank}.{marker}"))
            })
            .collect();
        ranks = rank_of(&promoted);
    }
}

/// Shared read-only state for the spanning-tree emitter.
struct EmitContext<'a> {
    structure: &'a StructureDefinition,
    neighbours: &'a std::collections::BTreeMap<&'a AtomId, Vec<(&'a AtomId, BondOrder)>>,
    parents: &'a std::collections::BTreeMap<&'a AtomId, &'a AtomId>,
    closures: &'a std::collections::BTreeMap<&'a AtomId, Vec<(u8, BondOrder, &'a AtomId)>>,
    hydrogen_counts: &'a std::collections::BTreeMap<&'a AtomId, u8>,
    /// Reference-bond directions for stereo double bonds, keyed by the
    /// ordered edge: (reference atom, its side of the axis).
    stereo_marks: &'a std::collections::BTreeMap<(AtomId, AtomId), (AtomId, bool)>,
}

fn emit(
    context: &EmitContext<'_>,
    atom: &AtomId,
    parent: Option<&AtomId>,
    emitted_closures: &mut std::collections::BTreeSet<u8>,
    output: &mut String,
) -> Option<()> {
    let children: Vec<_> = context
        .neighbours
        .get(atom)
        .into_iter()
        .flatten()
        .filter(|(neighbour, _)| {
            context.parents.get(*neighbour) == Some(&atom) && Some(*neighbour) != parent
        })
        .collect();
    let hydrogens = context.hydrogen_counts.get(atom).copied().unwrap_or(0);
    let glyph = chirality_glyph(context, atom, parent, &children, hydrogens)?;
    output.push_str(&atom_text(context.structure, atom, hydrogens, glyph));
    for (digit, order, _) in context.closures.get(atom).into_iter().flatten() {
        if !emitted_closures.insert(*digit) {
            // Second endpoint carries the bond symbol.
            output.push_str(order_symbol(*order));
        }
        output.push(char::from(b'0' + digit));
    }
    for (index, (child, order)) in children.iter().enumerate() {
        let last = index + 1 == children.len();
        if !last {
            output.push('(');
        }
        let key = if *atom < **child {
            (atom.clone(), (*child).clone())
        } else {
            ((*child).clone(), atom.clone())
        };
        match context.stereo_marks.get(&key) {
            Some((reference, position)) if *order == BondOrder::Single => {
                // '/' means the later-written atom sits up from the
                // earlier one; flip when the reference is written first.
                let up = if *child == reference { *position } else { !*position };
                output.push(if up { '/' } else { '\\' });
            }
            _ => output.push_str(order_symbol(*order)),
        }
        emit(context, child, Some(atom), emitted_closures, output)?;
        if !last {
            output.push(')');
        }
    }
    Some(())
}

/// The `@`/`@@` glyph for a chiral atom relative to the order this writer
/// emits its neighbours in: Some(None) for achiral atoms, and outer None
/// when a chiral descriptor cannot be expressed (two folded hydrogens),
/// failing the whole write.
#[allow(clippy::option_option)]
fn chirality_glyph(
    context: &EmitContext<'_>,
    atom: &AtomId,
    parent: Option<&AtomId>,
    children: &[&(&AtomId, BondOrder)],
    hydrogens: u8,
) -> Option<Option<&'static str>> {
    use crate::structural::TetrahedralHandedness;
    let graph = context.structure.graph();
    let Some(chirality) = graph.atoms()[atom].chirality() else {
        return Some(None);
    };
    if hydrogens > 1 {
        return None;
    }
    let hydrogen_id = graph.covalent_bonds().values().find_map(|bond| {
        let other = if bond.left() == atom {
            bond.right()
        } else if bond.right() == atom {
            bond.left()
        } else {
            return None;
        };
        (graph.atoms()[other].element().as_str() == "H").then(|| other.clone())
    });
    // Emitted neighbour order: parent, folded hydrogen, ring closures in
    // digit order, then children.
    let mut emitted: Vec<AtomId> = Vec::new();
    if let Some(parent) = parent {
        emitted.push(parent.clone());
    }
    if hydrogens == 1 {
        emitted.push(hydrogen_id?);
    }
    for (_, _, partner) in context.closures.get(atom).into_iter().flatten() {
        emitted.push((*partner).clone());
    }
    for (child, _) in children.iter().map(|entry| **entry) {
        emitted.push(child.clone());
    }
    let emitted: [AtomId; 4] = emitted.try_into().ok()?;
    let odd = permutation_is_odd(chirality.neighbours(), &emitted)?;
    let glyph = match (chirality.handedness(), odd) {
        (TetrahedralHandedness::Counterclockwise, false)
        | (TetrahedralHandedness::Clockwise, true) => "@",
        _ => "@@",
    };
    Some(Some(glyph))
}

/// Whether mapping one neighbour tuple onto the other is an odd
/// permutation. None when the tuples are not the same set.
fn permutation_is_odd(from: &[AtomId; 4], to: &[AtomId; 4]) -> Option<bool> {
    let mut positions: Vec<usize> = from
        .iter()
        .map(|entry| to.iter().position(|candidate| candidate == entry))
        .collect::<Option<Vec<_>>>()?;
    let mut swaps = 0;
    for index in 0..positions.len() {
        while positions[index] != index {
            let target = positions[index];
            positions.swap(index, target);
            swaps += 1;
        }
    }
    Some(swaps % 2 == 1)
}

/// One atom's text: bare when the subset's implicit-hydrogen rule
/// reproduces reality, a bracket atom otherwise.
fn atom_text(
    structure: &StructureDefinition,
    atom: &AtomId,
    hydrogens: u8,
    chirality: Option<&'static str>,
) -> String {
    let graph = structure.graph();
    let record = &graph.atoms()[atom];
    let symbol = record.element().as_str();
    let charge = record.electrons().formal_charge();
    let heavy_order_sum: u8 = graph
        .covalent_bonds()
        .values()
        .filter(|bond| bond.left() == atom || bond.right() == atom)
        .filter(|bond| {
            let other = if bond.left() == atom {
                bond.right()
            } else {
                bond.left()
            };
            graph.atoms()[other].element().as_str() != "H"
        })
        .map(|bond| match bond.order() {
            BondOrder::Single => 1,
            BondOrder::Double => 2,
            BondOrder::Triple => 3,
        })
        .sum();
    if charge == 0
        && chirality.is_none()
        && subset_valence(symbol)
            .is_some_and(|valence| valence.saturating_sub(heavy_order_sum) == hydrogens)
    {
        return symbol.to_owned();
    }
    let chirality_text = chirality.unwrap_or("");
    let hydrogens_text = match hydrogens {
        0 => String::new(),
        1 => "H".to_owned(),
        count => format!("H{count}"),
    };
    let charge_text = match charge {
        0 => String::new(),
        1 => "+".to_owned(),
        -1 => "-".to_owned(),
        positive if positive > 0 => format!("+{positive}"),
        negative => format!("-{}", -negative),
    };
    format!("[{symbol}{chirality_text}{hydrogens_text}{charge_text}]")
}
