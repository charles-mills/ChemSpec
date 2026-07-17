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
}

#[derive(Debug, Default, Clone)]
struct Component {
    atoms: Vec<ParsedAtom>,
    /// (left, right, order 1..=3) over heavy-atom indices.
    bonds: Vec<(usize, usize, u8)>,
}

/// Parses the subset into components of heavy atoms and bonds.
#[allow(clippy::too_many_lines)]
fn parse(smiles: &str) -> Option<Vec<Component>> {
    let mut components = Vec::new();
    let mut component = Component::default();
    let mut previous: Option<usize> = None;
    let mut branch_stack: Vec<Option<usize>> = Vec::new();
    let mut pending_order: Option<u8> = None;
    let mut rings: [Option<(usize, Option<u8>)>; 10] = [None; 10];
    let bytes = smiles.as_bytes();
    let mut position = 0;

    let bond = |component: &mut Component,
                    previous: &mut Option<usize>,
                    pending: &mut Option<u8>,
                    next: usize| {
        if let Some(left) = *previous {
            component
                .bonds
                .push((left, next, pending.take().unwrap_or(1)));
        }
        *previous = Some(next);
    };

    while position < bytes.len() {
        match bytes[position] {
            b'-' => {
                pending_order = Some(1);
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
                position += 1;
            }
            digit @ b'1'..=b'9' => {
                let index = usize::from(digit - b'0');
                let current = previous?;
                match rings[index].take() {
                    None => {
                        rings[index] = Some((current, pending_order.take()));
                    }
                    Some((open, open_order)) => {
                        if open == current {
                            return None;
                        }
                        let order = pending_order.take().or(open_order).unwrap_or(1);
                        component.bonds.push((open, current, order));
                    }
                }
                position += 1;
            }
            b'[' => {
                let close = position + bytes[position..].iter().position(|byte| *byte == b']')?;
                let atom = parse_bracket(&smiles[position + 1..close])?;
                component.atoms.push(atom);
                let next = component.atoms.len() - 1;
                bond(&mut component, &mut previous, &mut pending_order, next);
                position = close + 1;
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
                });
                let next = component.atoms.len() - 1;
                bond(&mut component, &mut previous, &mut pending_order, next);
                position += symbol.len();
            }
            _ => return None,
        }
    }
    if component.atoms.is_empty()
        || !branch_stack.is_empty()
        || pending_order.is_some()
        || rings.iter().any(Option::is_some)
    {
        return None;
    }
    components.push(component);
    Some(components)
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
    })
}

/// Expands one parsed component into a covalent unit: implicit hydrogens
/// become explicit atoms, and each atom's non-bonding electrons follow the
/// formal-charge identity `lone = V - bonds - q`.
fn component_unit(component: &Component) -> Option<SolvedUnit> {
    let heavy = component.atoms.len();
    let mut order_sums = vec![0_u8; heavy];
    for (left, right, order) in &component.bonds {
        if *left >= heavy || *right >= heavy || !(1..=3).contains(order) {
            return None;
        }
        order_sums[*left] = order_sums[*left].checked_add(*order)?;
        order_sums[*right] = order_sums[*right].checked_add(*order)?;
    }
    let mut unit = SolvedUnit {
        symbols: Vec::new(),
        states: Vec::new(),
        bonds: component.bonds.clone(),
    };
    let mut hydrogens = Vec::new();
    for (index, atom) in component.atoms.iter().enumerate() {
        let attached = match atom.explicit_hydrogens {
            Some(count) => count,
            None => subset_valence(&atom.symbol)?.saturating_sub(order_sums[index]),
        };
        let bonds = i16::from(order_sums[index]) + i16::from(attached);
        let valence = i16::from(valence_electrons_of(&atom.symbol)?);
        let lone = valence - bonds - atom.charge;
        if !(0..=8).contains(&lone) {
            return None;
        }
        unit.symbols.push(atom.symbol.clone());
        unit.states.push((atom.charge, u8::try_from(lone).ok()?));
        hydrogens.push(attached);
    }
    for (index, count) in hydrogens.into_iter().enumerate() {
        for _ in 0..count {
            unit.symbols.push("H".to_owned());
            unit.states.push((0, 0));
            unit.bonds.push((index, unit.symbols.len() - 1, 1));
        }
    }
    Some(unit)
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
        [unit] if unit_charge(unit) == 0 => build_molecular(id, &inventory, unit),
        _ => {
            let net: i16 = units.iter().map(unit_charge).sum();
            if net != 0 || units.iter().any(|unit| unit_charge(unit) == 0) {
                return None;
            }
            ionic_structure(id, &inventory, &units)
        }
    }
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
            let components = graph
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
    let start = *heavy.first()?;

    // Iterative DFS emitting atoms, branches, and ring-closure digits.
    let mut visited = BTreeSet::new();
    let mut used_edges = BTreeSet::new();
    let mut closures: BTreeMap<&AtomId, Vec<(u8, BondOrder)>> = BTreeMap::new();
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
                closures.entry(current).or_default().push((next_digit, *order));
                closures
                    .entry(neighbour)
                    .or_default()
                    .push((next_digit, *order));
                next_digit += 1;
            }
        }
    }
    if visited.len() != heavy.len() {
        return None;
    }

    // Second pass: emit the spanning tree recursively.
    let mut output = String::new();
    let context = EmitContext {
        structure,
        neighbours: &neighbours,
        parents: &parents,
        closures: &closures,
        hydrogen_counts: &hydrogen_counts,
    };
    let mut emitted = BTreeSet::new();
    emit(&context, start, None, &mut emitted, &mut output);
    Some(output)
}

/// Shared read-only state for the spanning-tree emitter.
struct EmitContext<'a> {
    structure: &'a StructureDefinition,
    neighbours: &'a std::collections::BTreeMap<&'a AtomId, Vec<(&'a AtomId, BondOrder)>>,
    parents: &'a std::collections::BTreeMap<&'a AtomId, &'a AtomId>,
    closures: &'a std::collections::BTreeMap<&'a AtomId, Vec<(u8, BondOrder)>>,
    hydrogen_counts: &'a std::collections::BTreeMap<&'a AtomId, u8>,
}

fn emit(
    context: &EmitContext<'_>,
    atom: &AtomId,
    parent: Option<&AtomId>,
    emitted_closures: &mut std::collections::BTreeSet<u8>,
    output: &mut String,
) {
    output.push_str(&atom_text(
        context.structure,
        atom,
        context.hydrogen_counts.get(atom).copied().unwrap_or(0),
    ));
    for (digit, order) in context.closures.get(atom).into_iter().flatten() {
        if !emitted_closures.insert(*digit) {
            // Second endpoint carries the bond symbol.
            output.push_str(order_symbol(*order));
        }
        output.push(char::from(b'0' + digit));
    }
    let children: Vec<_> = context
        .neighbours
        .get(atom)
        .into_iter()
        .flatten()
        .filter(|(neighbour, _)| {
            context.parents.get(*neighbour) == Some(&atom) && Some(*neighbour) != parent
        })
        .collect();
    for (index, (child, order)) in children.iter().enumerate() {
        let last = index + 1 == children.len();
        if !last {
            output.push('(');
        }
        output.push_str(order_symbol(*order));
        emit(context, child, Some(atom), emitted_closures, output);
        if !last {
            output.push(')');
        }
    }
}

/// One atom's text: bare when the subset's implicit-hydrogen rule
/// reproduces reality, a bracket atom otherwise.
fn atom_text(
    structure: &StructureDefinition,
    atom: &AtomId,
    hydrogens: u8,
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
        && subset_valence(symbol)
            .is_some_and(|valence| valence.saturating_sub(heavy_order_sum) == hydrogens)
    {
        return symbol.to_owned();
    }
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
    format!("[{symbol}{hydrogens_text}{charge_text}]")
}
