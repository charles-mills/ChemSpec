//! Programmatic structure generation.
//!
//! Given an element multiset, derives a validated [`StructureDefinition`]
//! from periodic-table physics alone: duet/octet electron ledgers (with
//! expanded octets from period 3), degree-constrained multigraph search,
//! and electronegativity-guided scoring. No catalogue, no whitelist.

use crate::{
    Atom, AtomGroup, AtomGroupId, AtomId, BondOrder, CovalentBond, CovalentBondId, ElectronState,
    ElementInventory, ElementSymbol, IonicAssociation, IonicAssociationId, MetallicDomain,
    MetallicDomainId, RepresentationKind, StructuralGraph, StructureDefinition, StructureId,
};

// ponytail: DFS enumeration; benzene-sized covalent units bail to the
// caller's fallback (catalogue or LLM proposal). Ionic compounds are only
// bounded per ion unit, so large salts assemble fine.
const MAX_ATOMS: usize = 12;
const MAX_TOTAL_ATOMS: usize = 24;
const MAX_WORK: u32 = 500_000;
const HOMONUCLEAR_PENALTY: u32 = 1_000;
const CHARGE_SEPARATION_PENALTY: u32 = 40;
const TRIANGLE_PENALTY: u32 = 60;

#[derive(Clone, Copy)]
struct Facts {
    valence: u8,
    period: u8,
    /// Pauling electronegativity ×100; 0 when untabulated.
    electronegativity: u16,
    /// Common cation charges, preferred first; nonempty marks a metal.
    cation_charges: &'static [i16],
}

#[allow(clippy::match_same_arms)]
fn facts(symbol: &str) -> Option<Facts> {
    let f = |valence, period, electronegativity, cation_charges| {
        Some(Facts {
            valence,
            period,
            electronegativity,
            cation_charges,
        })
    };
    match symbol {
        "H" => f(1, 1, 220, &[]),
        "He" => f(2, 1, 0, &[]),
        "B" => f(3, 2, 204, &[]),
        "C" => f(4, 2, 255, &[]),
        "N" => f(5, 2, 304, &[]),
        "O" => f(6, 2, 344, &[]),
        "F" => f(7, 2, 398, &[]),
        "Ne" => f(8, 2, 0, &[]),
        "Si" => f(4, 3, 190, &[]),
        "P" => f(5, 3, 219, &[]),
        "S" => f(6, 3, 258, &[]),
        "Cl" => f(7, 3, 316, &[]),
        "Ar" => f(8, 3, 0, &[]),
        "As" => f(5, 4, 218, &[]),
        "Se" => f(6, 4, 255, &[]),
        "Br" => f(7, 4, 296, &[]),
        "Kr" => f(8, 4, 0, &[]),
        "Te" => f(6, 5, 210, &[]),
        "I" => f(7, 5, 266, &[]),
        "Xe" => f(8, 5, 260, &[]),
        "Rn" => f(8, 6, 0, &[]),
        "Li" => f(1, 2, 98, &[1]),
        "Na" => f(1, 3, 93, &[1]),
        "K" => f(1, 4, 82, &[1]),
        "Rb" => f(1, 5, 82, &[1]),
        "Cs" => f(1, 6, 79, &[1]),
        "Fr" => f(1, 7, 70, &[1]),
        "Be" => f(2, 2, 157, &[2]),
        "Mg" => f(2, 3, 131, &[2]),
        "Ca" => f(2, 4, 100, &[2]),
        "Sr" => f(2, 5, 95, &[2]),
        "Ba" => f(2, 6, 89, &[2]),
        "Ra" => f(2, 7, 90, &[2]),
        "Al" => f(3, 3, 161, &[3]),
        "Ga" => f(3, 4, 181, &[3]),
        "In" => f(3, 5, 178, &[3]),
        // Transition metals count s+d electrons (their group number), the
        // same convention the reviewed catalogue uses, so generated ions
        // match reviewed electron states (e.g. Zn2+ keeps 10 d electrons).
        "Zn" => f(12, 4, 165, &[2]),
        "Cd" => f(12, 5, 169, &[2]),
        "Ag" => f(11, 5, 193, &[1]),
        "Cu" => f(11, 4, 190, &[2, 1]),
        "Fe" => f(8, 4, 183, &[3, 2]),
        "Ni" => f(10, 4, 191, &[2]),
        "Co" => f(9, 4, 188, &[2]),
        "Mn" => f(7, 4, 155, &[2]),
        "Cr" => f(6, 4, 166, &[3, 2]),
        "Sn" => f(4, 5, 196, &[2, 4]),
        "Pb" => f(4, 6, 233, &[2, 4]),
        "Au" => f(11, 6, 254, &[3, 1]),
        _ => None,
    }
}

/// One atom slot in the covalent search: element facts plus assigned formal
/// charge.
#[derive(Clone)]
struct Slot {
    symbol: String,
    facts: Facts,
    formal_charge: i16,
}

impl Slot {
    /// Ledger-valid bond-order sums for this atom at its formal charge.
    fn bond_sum_options(&self) -> Vec<u8> {
        let valence = i16::from(self.facts.valence);
        let charge = self.formal_charge;
        if self.symbol == "H" {
            return if charge == 0 { vec![1] } else { Vec::new() };
        }
        if self.facts.valence == 8 || self.symbol == "He" {
            return if charge == 0 { vec![0] } else { Vec::new() };
        }
        if self.symbol == "B" && charge == 0 {
            return vec![3];
        }
        let base = 8 - valence + charge;
        let Ok(base) = u8::try_from(base) else {
            return Vec::new();
        };
        if base > 7 {
            return Vec::new();
        }
        if self.facts.period == 2 {
            return vec![base];
        }
        let Ok(max) = u8::try_from((valence + charge).clamp(0, 7)) else {
            return Vec::new();
        };
        (base..=max).step_by(2).collect()
    }

    /// Non-bonding electrons left after `bond_sum`, or None when invalid.
    fn lone_electrons_for(&self, bond_sum: u8) -> Option<u8> {
        let lone = i16::from(self.facts.valence) - i16::from(bond_sum) - self.formal_charge;
        ((0..=8).contains(&lone) && lone % 2 == 0).then(|| u8::try_from(lone).expect("range"))
    }
}

/// A solved covalent unit: per-slot electron states and bonds by slot index.
#[derive(Clone)]
struct SolvedUnit {
    symbols: Vec<String>,
    /// (formal charge, non-bonding electrons) per slot.
    states: Vec<(i16, u8)>,
    /// (left slot, right slot, order 1..=3).
    bonds: Vec<(usize, usize, u8)>,
}

/// Enumerates ledger-valid connected multigraphs across every candidate
/// slot assignment and returns the unique best-scoring one.
fn best_solution(slot_sets: &[Vec<Slot>]) -> Option<SolvedUnit> {
    let mut work = 0_u32;
    let mut best_score = u32::MAX;
    let mut best: Vec<(SolvedUnit, String)> = Vec::new();
    for slots in slot_sets {
        if slots.is_empty() || slots.len() > MAX_ATOMS {
            continue;
        }
        if slots.len() == 1 {
            if slots[0].bond_sum_options().contains(&0) && slots[0].lone_electrons_for(0).is_some()
            {
                consider(slots, &[0], &[], &mut best_score, &mut best);
            }
            continue;
        }
        let option_sets = slots.iter().map(Slot::bond_sum_options).collect::<Vec<_>>();
        if option_sets.iter().any(Vec::is_empty) {
            continue;
        }
        let mut combo = vec![0_usize; slots.len()];
        'combos: loop {
            let targets = combo
                .iter()
                .zip(&option_sets)
                .map(|(index, set)| set[*index])
                .collect::<Vec<_>>();
            let total: u32 = targets.iter().copied().map(u32::from).sum();
            let minimum_edges = u32::try_from(slots.len() - 1).ok()?;
            if total.is_multiple_of(2) && total / 2 >= minimum_edges {
                enumerate_graphs(slots, &targets, &mut work, &mut |bonds| {
                    consider(slots, &targets, bonds, &mut best_score, &mut best);
                });
            }
            if work > MAX_WORK {
                return None;
            }
            let mut position = 0;
            loop {
                if position == combo.len() {
                    break 'combos;
                }
                combo[position] += 1;
                if combo[position] < option_sets[position].len() {
                    break;
                }
                combo[position] = 0;
                position += 1;
            }
        }
    }
    (best.len() == 1).then(|| best.remove(0).0)
}

/// Scores one valid graph and folds it into the best-candidate pool.
fn consider(
    slots: &[Slot],
    targets: &[u8],
    bonds: &[(usize, usize, u8)],
    best_score: &mut u32,
    best: &mut Vec<(SolvedUnit, String)>,
) {
    // Octet expansion is only physical toward more electronegative
    // partners (S→O, I→F); anything else is a ledger-valid abomination.
    for (index, (slot, target)) in slots.iter().zip(targets).enumerate() {
        let minimum = slot.bond_sum_options().first().copied().unwrap_or(0);
        if *target <= minimum {
            continue;
        }
        let expansion_is_physical = bonds
            .iter()
            .filter_map(|(left, right, _)| {
                if *left == index {
                    Some(*right)
                } else if *right == index {
                    Some(*left)
                } else {
                    None
                }
            })
            .all(|partner| slots[partner].facts.electronegativity > slot.facts.electronegativity);
        if !expansion_is_physical {
            return;
        }
    }
    let score = score_graph(slots, targets, bonds);
    if score > *best_score {
        return;
    }
    let key = isomorphism_key(slots, bonds);
    if score < *best_score {
        *best_score = score;
        best.clear();
    }
    if best.iter().any(|(_, existing)| *existing == key) {
        return;
    }
    let states = targets
        .iter()
        .zip(slots)
        .map(|(bond_sum, slot)| {
            (
                slot.formal_charge,
                slot.lone_electrons_for(*bond_sum).expect("validated"),
            )
        })
        .collect();
    best.push((
        SolvedUnit {
            symbols: slots.iter().map(|slot| slot.symbol.clone()).collect(),
            states,
            bonds: bonds.to_vec(),
        },
        key,
    ));
}

/// Distributes each atom's remaining bond order over later atoms, depth-first.
fn enumerate_graphs(
    slots: &[Slot],
    targets: &[u8],
    work: &mut u32,
    emit: &mut impl FnMut(&[(usize, usize, u8)]),
) {
    #[allow(clippy::too_many_arguments)]
    fn recurse(
        slots: &[Slot],
        remaining: &mut [u8],
        atom: usize,
        partner: usize,
        bonds: &mut Vec<(usize, usize, u8)>,
        work: &mut u32,
        emit: &mut impl FnMut(&[(usize, usize, u8)]),
    ) {
        *work += 1;
        if *work > MAX_WORK {
            return;
        }
        if atom == slots.len() {
            if is_connected(slots.len(), bonds) {
                emit(bonds);
            }
            return;
        }
        if remaining[atom] == 0 {
            recurse(slots, remaining, atom + 1, atom + 2, bonds, work, emit);
            return;
        }
        if partner >= slots.len() {
            return;
        }
        // Capacity pruning: the rest of the row must be able to absorb what
        // this atom still owes.
        let available: u32 = (partner..slots.len())
            .map(|later| u32::from(remaining[later].min(edge_cap(slots, atom, later))))
            .sum();
        if u32::from(remaining[atom]) > available {
            return;
        }
        let cap = remaining[atom]
            .min(remaining[partner])
            .min(edge_cap(slots, atom, partner));
        for order in (0..=cap).rev() {
            // Identical-partner symmetry: once an interchangeable earlier
            // partner received nothing, this one receives nothing too.
            if order > 0
                && partner > atom + 1
                && interchangeable(slots, remaining, partner - 1, partner)
            {
                continue;
            }
            remaining[atom] -= order;
            remaining[partner] -= order;
            if order > 0 {
                bonds.push((atom, partner, order));
            }
            recurse(slots, remaining, atom, partner + 1, bonds, work, emit);
            if order > 0 {
                bonds.pop();
            }
            remaining[atom] += order;
            remaining[partner] += order;
        }
    }

    let mut remaining = targets.to_vec();
    let mut bonds = Vec::new();
    recurse(slots, &mut remaining, 0, 1, &mut bonds, work, emit);
}

fn edge_cap(slots: &[Slot], left: usize, right: usize) -> u8 {
    if slots[left].symbol == "H" || slots[right].symbol == "H" {
        1
    } else {
        3
    }
}

fn interchangeable(slots: &[Slot], remaining: &[u8], left: usize, right: usize) -> bool {
    slots[left].symbol == slots[right].symbol
        && slots[left].formal_charge == slots[right].formal_charge
        && remaining[left] == remaining[right]
}

fn is_connected(count: usize, bonds: &[(usize, usize, u8)]) -> bool {
    if count == 0 {
        return false;
    }
    let mut reached = vec![false; count];
    let mut queue = vec![0_usize];
    reached[0] = true;
    while let Some(index) = queue.pop() {
        for (left, right, _) in bonds {
            let next = if *left == index {
                *right
            } else if *right == index {
                *left
            } else {
                continue;
            };
            if !reached[next] {
                reached[next] = true;
                queue.push(next);
            }
        }
    }
    reached.into_iter().all(|flag| flag)
}

/// Lower is better: penalize homonuclear heteroatom bonds, hydrogens and
/// negative charges away from the most electronegative atoms, and
/// unnecessary octet expansion.
fn score_graph(slots: &[Slot], targets: &[u8], bonds: &[(usize, usize, u8)]) -> u32 {
    let elemental = slots.iter().all(|slot| slot.symbol == slots[0].symbol);
    let top_heavy = slots
        .iter()
        .filter(|slot| slot.symbol != "H")
        .map(|slot| slot.facts.electronegativity)
        .max()
        .unwrap_or(0);
    let mut score = 0_u32;
    for (left, right, _) in bonds {
        // O-O, N-N, and halogen-halogen bonds are last resorts; C-C and
        // S-S chains are ordinary chemistry and stay unpenalized.
        if !elemental
            && slots[*left].symbol == slots[*right].symbol
            && slots[*left].facts.electronegativity >= 300
        {
            score += HOMONUCLEAR_PENALTY;
        }
        for (first, second) in [(*left, *right), (*right, *left)] {
            if slots[first].symbol == "H" && slots[second].symbol != "H" {
                score += u32::from(top_heavy - slots[second].facts.electronegativity);
            }
        }
    }
    let bottom_heavy = slots
        .iter()
        .filter(|slot| slot.symbol != "H")
        .map(|slot| slot.facts.electronegativity)
        .min()
        .unwrap_or(0);
    for (slot, target) in slots.iter().zip(targets) {
        let minimum = slot.bond_sum_options().first().copied().unwrap_or(0);
        score += u32::from(target - minimum);
        if slot.formal_charge < 0 && slot.symbol != "H" {
            score += u32::from(top_heavy - slot.facts.electronegativity)
                * u32::from(slot.formal_charge.unsigned_abs());
        }
        if slot.formal_charge > 0 {
            // Charge separation is a last resort, placed on the least
            // electronegative heavy atom when unavoidable.
            score += CHARGE_SEPARATION_PENALTY
                + u32::from(slot.facts.electronegativity.saturating_sub(bottom_heavy));
        }
    }
    // Three-membered rings are strained; prefer open forms (real ozone
    // over cyclic ozone).
    let bonded = |a: usize, b: usize| {
        bonds
            .iter()
            .any(|(left, right, _)| (*left == a && *right == b) || (*left == b && *right == a))
    };
    for a in 0..slots.len() {
        for b in a + 1..slots.len() {
            for c in b + 1..slots.len() {
                if bonded(a, b) && bonded(b, c) && bonded(a, c) {
                    score += TRIANGLE_PENALTY;
                }
            }
        }
    }
    score
}

/// Weisfeiler-Leman style refinement key for duplicate detection.
// ponytail: heuristic isomorphism, not exact; small molecules refine fully
// in practice. Swap in a canonical-form algorithm if ambiguity misfires.
fn isomorphism_key(slots: &[Slot], bonds: &[(usize, usize, u8)]) -> String {
    let mut colors = slots
        .iter()
        .map(|slot| format!("{}:{}", slot.symbol, slot.formal_charge))
        .collect::<Vec<_>>();
    for _ in 0..slots.len() {
        colors = (0..slots.len())
            .map(|index| {
                let mut neighbours = bonds
                    .iter()
                    .filter_map(|(left, right, order)| {
                        if *left == index {
                            Some(format!("{}x{order}", colors[*right]))
                        } else if *right == index {
                            Some(format!("{}x{order}", colors[*left]))
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>();
                neighbours.sort_unstable();
                format!("{}({})", colors[index], neighbours.join(","))
            })
            .collect();
    }
    colors.sort_unstable();
    colors.join(";")
}

/// Builds every way of handing out `charge` units of -1 across non-hydrogen
/// atoms, identical elements collapsed.
/// Every way of placing `negatives` × (-1) and `positives` × (+1) across
/// the unit's non-hydrogen atoms, identical elements collapsed. An atom
/// carries at most one unit of charge of one sign.
fn charged_slot_sets(
    elements: &[(String, Facts, u64)],
    negatives: u32,
    positives: u32,
) -> Vec<Vec<Slot>> {
    fn distribute(
        elements: &[(String, Facts, u64)],
        index: usize,
        left: u64,
        occupied: &[u64],
        allocation: &mut Vec<u64>,
        out: &mut Vec<Vec<u64>>,
    ) {
        if index == elements.len() {
            if left == 0 {
                out.push(allocation.clone());
            }
            return;
        }
        let cap = if elements[index].0 == "H" {
            0
        } else {
            left.min(
                elements[index]
                    .2
                    .saturating_sub(occupied.get(index).copied().unwrap_or(0)),
            )
        };
        for take in 0..=cap {
            allocation.push(take);
            distribute(elements, index + 1, left - take, occupied, allocation, out);
            allocation.pop();
        }
    }

    let mut positive_allocations = Vec::new();
    distribute(
        elements,
        0,
        u64::from(positives),
        &[],
        &mut Vec::new(),
        &mut positive_allocations,
    );
    let mut sets = Vec::new();
    for positive in positive_allocations {
        let mut negative_allocations = Vec::new();
        distribute(
            elements,
            0,
            u64::from(negatives),
            &positive,
            &mut Vec::new(),
            &mut negative_allocations,
        );
        for negative in negative_allocations {
            let mut slots = Vec::new();
            for (index, (symbol, element_facts, count)) in elements.iter().enumerate() {
                let plus = positive.get(index).copied().unwrap_or(0);
                let minus = negative.get(index).copied().unwrap_or(0);
                for position in 0..*count {
                    let formal_charge = if position < plus {
                        1
                    } else if position < plus + minus {
                        -1
                    } else {
                        0
                    };
                    slots.push(Slot {
                        symbol: symbol.clone(),
                        facts: *element_facts,
                        formal_charge,
                    });
                }
            }
            sets.push(slots);
        }
    }
    sets
}

/// The most common cation charge for a metal, or None for nonmetals.
#[must_use]
pub fn common_cation_charge(symbol: &str) -> Option<i16> {
    facts(symbol)?.cation_charges.first().copied()
}

/// The smallest common cation charge. Cations above it (Fe3+, Cu2+ vs Cu+)
/// can oxidise where the lowest state would not.
#[must_use]
pub fn lowest_cation_charge(symbol: &str) -> Option<i16> {
    facts(symbol)?.cation_charges.iter().copied().min()
}

/// Whether a metal commonly takes more than one cation charge (and so
/// needs a Roman numeral in salt names).
#[must_use]
pub fn has_variable_cation_charge(symbol: &str) -> bool {
    facts(symbol).is_some_and(|element_facts| element_facts.cation_charges.len() > 1)
}

/// The metal activity series, most reactive first, with hydrogen as the
/// pivot for acid reactivity. Periodic-trend knowledge, kept as code.
const ACTIVITY_SERIES: [&str; 24] = [
    "Cs", "Rb", "K", "Ba", "Sr", "Ca", "Na", "Mg", "Al", "Mn", "Zn", "Cr", "Fe", "Cd", "Co", "Ni",
    "Sn", "Pb", "H", "Cu", "Ag", "Hg", "Pt", "Au",
];

/// Position in the activity series (lower is more reactive), when listed.
#[must_use]
pub fn activity_rank(symbol: &str) -> Option<usize> {
    ACTIVITY_SERIES
        .iter()
        .position(|candidate| *candidate == symbol)
}

/// Whether this metal displaces hydrogen from non-oxidizing acids. None
/// when the element is not in the series.
#[must_use]
pub fn displaces_hydrogen_from_acids(symbol: &str) -> Option<bool> {
    let hydrogen = activity_rank("H").unwrap_or(usize::MAX);
    activity_rank(symbol).map(|rank| rank < hydrogen)
}

/// The standard anion charge a nonmetal takes in binary compounds
/// (8 - valence electrons; hydride is 1). None for metals and noble gases.
#[must_use]
pub fn anion_valence_charge(symbol: &str) -> Option<u8> {
    let element_facts = facts(symbol)?;
    if !element_facts.cation_charges.is_empty() || element_facts.valence == 8 {
        return None;
    }
    if symbol == "H" {
        return Some(1);
    }
    Some(8 - element_facts.valence)
}

/// Generates the single best structure for an element inventory, or None
/// when nothing valid exists or the result stays ambiguous.
#[must_use]
#[allow(clippy::missing_panics_doc, clippy::needless_pass_by_value)]
pub fn generate_structure(
    id: StructureId,
    inventory: &ElementInventory,
) -> Option<StructureDefinition> {
    let counts = inventory.elements();
    let total: u64 = counts.values().sum();
    if total == 0 || total > u64::try_from(MAX_TOTAL_ATOMS).expect("small constant") {
        return None;
    }
    let mut metals = Vec::new();
    let mut nonmetals = Vec::new();
    for (symbol, count) in counts {
        let element_facts = facts(symbol.as_str())?;
        if element_facts.cation_charges.is_empty() {
            nonmetals.push((symbol.as_str().to_owned(), element_facts, *count));
        } else {
            metals.push((symbol.as_str().to_owned(), element_facts, *count));
        }
    }
    if metals.is_empty() {
        if let Some(structure) = generate_allotrope(id.clone(), inventory, &nonmetals) {
            return Some(structure);
        }
        let mut slots = charged_slot_sets(&nonmetals, 0, 0);
        // Charge-separated neutrals (CO, HNO3, ozone) join the pool; the
        // separation penalty keeps plain octet solutions ahead when both
        // exist.
        slots.extend(charged_slot_sets(&nonmetals, 1, 1));
        if let Some(unit) = best_solution(&slots) {
            return build_molecular(id, inventory, &unit);
        }
        generate_ammonium_salt(id, inventory, &nonmetals)
    } else if nonmetals.is_empty() {
        generate_elemental_metal(id, inventory, &metals)
    } else {
        generate_ionic(id, inventory, &metals, &nonmetals)
    }
}

/// A single-site metallic structure: the standard model for one atom of an
/// elemental metal (site holds its cation charge, the metallic domain owns
/// the released electrons, net zero).
fn generate_elemental_metal(
    id: StructureId,
    inventory: &ElementInventory,
    metals: &[(String, Facts, u64)],
) -> Option<StructureDefinition> {
    let [(symbol, element_facts, 1)] = metals else {
        return None;
    };
    element_facts.cation_charges.first()?;
    // Every valence electron joins the domain, the same convention the
    // reviewed catalogue uses, so metallic and ionic electron books agree.
    let charge = i16::from(element_facts.valence);
    let electrons = u32::from(element_facts.valence);
    let atom = make_atom(0, symbol, charge, 0)?;
    let domain = MetallicDomain::new(
        MetallicDomainId::new("metallic").ok()?,
        [atom_id(0)?],
        electrons,
    )
    .ok()?;
    let graph = StructuralGraph::new([atom], [], [], [], [domain]).ok()?;
    StructureDefinition::new(id, inventory.clone(), RepresentationKind::Metallic, graph).ok()
}

/// Deterministic standard-state allotropes the bond search cannot settle on
/// its own (their graphs tie with rings of doubled bonds): the P4/As4
/// tetrahedron and the S8 crown.
fn generate_allotrope(
    id: StructureId,
    inventory: &ElementInventory,
    nonmetals: &[(String, Facts, u64)],
) -> Option<StructureDefinition> {
    let [(symbol, element_facts, count)] = nonmetals else {
        return None;
    };
    let bonds: Vec<(usize, usize)> = match (element_facts.valence, *count) {
        // Tetrahedron: every atom single-bonded to every other.
        (5, 4) => vec![(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)],
        // Crown ring.
        (6, 8) => (0..8).map(|index| (index, (index + 1) % 8)).collect(),
        _ => return None,
    };
    let count = usize::try_from(*count).ok()?;
    let lone = element_facts.valence - (8 - element_facts.valence);
    let atoms = (0..count)
        .map(|index| make_atom(index, symbol, 0, lone))
        .collect::<Option<Vec<_>>>()?;
    let bonds = bonds
        .iter()
        .map(|(left, right)| (*left, *right, 1))
        .collect::<Vec<_>>();
    let bonds = make_bonds(&bonds, 0)?;
    let graph = StructuralGraph::new(atoms, bonds, [], [], []).ok()?;
    StructureDefinition::new(id, inventory.clone(), RepresentationKind::Molecular, graph).ok()
}

fn build_molecular(
    id: StructureId,
    inventory: &ElementInventory,
    unit: &SolvedUnit,
) -> Option<StructureDefinition> {
    let atoms = unit
        .symbols
        .iter()
        .zip(&unit.states)
        .enumerate()
        .map(|(index, (symbol, (charge, lone)))| make_atom(index, symbol, *charge, *lone))
        .collect::<Option<Vec<_>>>()?;
    let bonds = make_bonds(&unit.bonds, 0)?;
    let graph = StructuralGraph::new(atoms, bonds, [], [], []).ok()?;
    StructureDefinition::new(id, inventory.clone(), RepresentationKind::Molecular, graph).ok()
}

#[allow(clippy::needless_pass_by_value)]
fn generate_ionic(
    id: StructureId,
    inventory: &ElementInventory,
    metals: &[(String, Facts, u64)],
    nonmetals: &[(String, Facts, u64)],
) -> Option<StructureDefinition> {
    // Iterate cation charge assignments, preferred charges first.
    let mut charge_combo = vec![0_usize; metals.len()];
    loop {
        let charges = metals
            .iter()
            .zip(&charge_combo)
            .map(|((_, element_facts, _), index)| element_facts.cation_charges[*index])
            .collect::<Vec<_>>();
        let positive = metals
            .iter()
            .zip(&charges)
            .map(|((_, _, count), charge)| {
                i64::try_from(*count)
                    .ok()
                    .map(|count| count * i64::from(*charge))
            })
            .sum::<Option<i64>>()?;
        if let Some(structure) =
            assemble_ionic(id.clone(), inventory, metals, &charges, nonmetals, positive)
        {
            return Some(structure);
        }
        let mut position = 0;
        loop {
            if position == charge_combo.len() {
                return None;
            }
            charge_combo[position] += 1;
            if charge_combo[position] < metals[position].1.cation_charges.len() {
                break;
            }
            charge_combo[position] = 0;
            position += 1;
        }
    }
}

fn assemble_ionic(
    id: StructureId,
    inventory: &ElementInventory,
    metals: &[(String, Facts, u64)],
    charges: &[i16],
    nonmetals: &[(String, Facts, u64)],
    positive: i64,
) -> Option<StructureDefinition> {
    let (copies, unit) = divide_anion_units(nonmetals, positive)?;
    let mut units = Vec::new();
    for ((symbol, element_facts, count), charge) in metals.iter().zip(charges) {
        for _ in 0..*count {
            let lone = u8::try_from((i16::from(element_facts.valence) - charge).max(0)).ok()?;
            units.push(SolvedUnit {
                symbols: vec![symbol.clone()],
                states: vec![(*charge, lone)],
                bonds: Vec::new(),
            });
        }
    }
    for _ in 0..copies {
        units.push(unit.clone());
    }
    ionic_structure(id, inventory, &units)
}

/// Splits the anion inventory into the most, smallest solvable units
/// (2 OH⁻ over HO-OH²⁻) that balance the given positive charge.
fn divide_anion_units(
    nonmetals: &[(String, Facts, u64)],
    positive: i64,
) -> Option<(u64, SolvedUnit)> {
    let count_gcd = nonmetals
        .iter()
        .map(|(_, _, count)| *count)
        .fold(u64::try_from(positive).ok()?, gcd);
    for divisor in (1..=count_gcd).rev() {
        if !count_gcd.is_multiple_of(divisor) {
            continue;
        }
        let unit_elements = nonmetals
            .iter()
            .map(|(symbol, element_facts, count)| (symbol.clone(), *element_facts, count / divisor))
            .collect::<Vec<_>>();
        let unit_charge = u32::try_from(positive / i64::try_from(divisor).ok()?).ok()?;
        if let Some(unit) = solve_anion_unit(&unit_elements, unit_charge) {
            return Some((divisor, unit));
        }
    }
    None
}

/// Builds the ionic graph for a list of ion units, one group per unit.
/// Ionic atom ids follow the catalogue's `<group>.<atom>` convention so the
/// graph round-trips through record validation unchanged.
fn ionic_structure(
    id: StructureId,
    inventory: &ElementInventory,
    units: &[SolvedUnit],
) -> Option<StructureDefinition> {
    let mut atoms = Vec::new();
    let mut bonds = Vec::new();
    let mut group_records = Vec::new();
    let ion_atom = |group: usize, offset: usize| AtomId::new(format!("g{group}.a{offset}")).ok();
    for (group_index, unit) in units.iter().enumerate() {
        let mut members = Vec::new();
        for (offset, symbol) in unit.symbols.iter().enumerate() {
            let (charge, lone) = unit.states[offset];
            let atom_id = ion_atom(group_index, offset)?;
            atoms.push(Atom::new(
                atom_id.clone(),
                ElementSymbol::new(symbol).ok()?,
                // Odd non-bonding counts (d9 cations like Cu2+) necessarily
                // leave one electron unpaired.
                ElectronState::new(charge, lone, lone % 2).ok()?,
            ));
            members.push(atom_id);
        }
        for (bond_index, (left, right, order)) in unit.bonds.iter().enumerate() {
            let order = match order {
                1 => BondOrder::Single,
                2 => BondOrder::Double,
                3 => BondOrder::Triple,
                _ => return None,
            };
            bonds.push(
                CovalentBond::new(
                    CovalentBondId::new(format!("b{group_index}.{bond_index}")).ok()?,
                    ion_atom(group_index, *left)?,
                    ion_atom(group_index, *right)?,
                    order,
                )
                .ok()?,
            );
        }
        group_records
            .push(AtomGroup::new(AtomGroupId::new(format!("g{group_index}")).ok()?, members).ok()?);
    }

    let association = IonicAssociation::new(
        IonicAssociationId::new("ia").ok()?,
        group_records.iter().map(|group| group.id().clone()),
    )
    .ok()?;
    let graph = StructuralGraph::new(atoms, bonds, group_records, [association], []).ok()?;
    StructureDefinition::new(id, inventory.clone(), RepresentationKind::Ionic, graph).ok()
}

/// Ammonium salts: the one common polyatomic cation. Tried only after the
/// molecular search fails, so amines keep their covalent structures.
#[allow(clippy::needless_pass_by_value)]
fn generate_ammonium_salt(
    id: StructureId,
    inventory: &ElementInventory,
    nonmetals: &[(String, Facts, u64)],
) -> Option<StructureDefinition> {
    let nitrogen_facts = facts("N")?;
    let hydrogen_facts = facts("H")?;
    let ammonium = best_solution(&charged_slot_sets(
        &[
            ("N".to_owned(), nitrogen_facts, 1),
            ("H".to_owned(), hydrogen_facts, 4),
        ],
        0,
        1,
    ))?;
    let count_of = |symbol: &str| {
        nonmetals
            .iter()
            .find(|(candidate, ..)| candidate == symbol)
            .map_or(0, |(_, _, count)| *count)
    };
    for cations in 1..=3_u64 {
        if count_of("N") < cations || count_of("H") < cations * 4 {
            break;
        }
        let remainder = nonmetals
            .iter()
            .map(|(symbol, element_facts, count)| {
                let used = match symbol.as_str() {
                    "N" => cations,
                    "H" => cations * 4,
                    _ => 0,
                };
                (symbol.clone(), *element_facts, count - used)
            })
            .filter(|(_, _, count)| *count > 0)
            .collect::<Vec<_>>();
        if remainder.is_empty() {
            continue;
        }
        let Some((copies, anion)) = divide_anion_units(&remainder, i64::try_from(cations).ok()?)
        else {
            continue;
        };
        let mut units = Vec::new();
        for _ in 0..cations {
            units.push(ammonium.clone());
        }
        for _ in 0..copies {
            units.push(anion.clone());
        }
        if let Some(structure) = ionic_structure(id.clone(), inventory, &units) {
            return Some(structure);
        }
    }
    None
}

fn solve_anion_unit(elements: &[(String, Facts, u64)], charge: u32) -> Option<SolvedUnit> {
    let total: u64 = elements.iter().map(|(_, _, count)| *count).sum();
    if total == 1 && charge > 0 {
        let (symbol, element_facts, _) = elements.iter().find(|(_, _, count)| *count == 1)?;
        let lone = i16::from(element_facts.valence) + i16::try_from(charge).ok()?;
        if (0..=8).contains(&lone) && lone % 2 == 0 {
            return Some(SolvedUnit {
                symbols: vec![symbol.clone()],
                states: vec![(
                    -i16::try_from(charge).ok()?,
                    u8::try_from(lone).expect("range"),
                )],
                bonds: Vec::new(),
            });
        }
        return None;
    }
    let mut slot_sets = charged_slot_sets(elements, charge, 0);
    slot_sets.extend(charged_slot_sets(elements, charge + 1, 1));
    best_solution(&slot_sets)
}

fn make_atom(index: usize, symbol: &str, charge: i16, lone: u8) -> Option<Atom> {
    Some(Atom::new(
        atom_id(index)?,
        ElementSymbol::new(symbol).ok()?,
        ElectronState::new(charge, lone, 0).ok()?,
    ))
}

fn atom_id(index: usize) -> Option<AtomId> {
    AtomId::new(format!("a{index}")).ok()
}

fn make_bonds(bonds: &[(usize, usize, u8)], base: usize) -> Option<Vec<CovalentBond>> {
    bonds
        .iter()
        .enumerate()
        .map(|(index, (left, right, order))| {
            let order = match order {
                1 => BondOrder::Single,
                2 => BondOrder::Double,
                3 => BondOrder::Triple,
                _ => return None,
            };
            CovalentBond::new(
                CovalentBondId::new(format!("b{base}.{index}")).ok()?,
                atom_id(base + left)?,
                atom_id(base + right)?,
                order,
            )
            .ok()
        })
        .collect()
}

const fn gcd(mut left: u64, mut right: u64) -> u64 {
    while right != 0 {
        let swap = left % right;
        left = right;
        right = swap;
    }
    left
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn structure(pairs: &[(&str, u64)]) -> Option<StructureDefinition> {
        let inventory = ElementInventory::new(
            pairs
                .iter()
                .map(|(symbol, count)| (ElementSymbol::new(*symbol).expect("symbol"), *count))
                .collect::<BTreeMap<_, _>>(),
        )
        .expect("inventory");
        generate_structure(StructureId::new("generated.test").expect("id"), &inventory)
    }

    #[test]
    fn water_is_generated_with_two_single_bonds() {
        let water = structure(&[("H", 2), ("O", 1)]).expect("water");
        assert_eq!(water.representation(), RepresentationKind::Molecular);
        assert_eq!(water.graph().covalent_bonds().len(), 2);
    }

    #[test]
    fn sulfuric_acid_is_generated_without_any_catalogue() {
        let acid = structure(&[("H", 2), ("S", 1), ("O", 4)]).expect("H2SO4");
        assert_eq!(acid.representation(), RepresentationKind::Molecular);
        let graph = acid.graph();
        let sulfur = graph
            .atoms()
            .values()
            .find(|atom| atom.element().as_str() == "S")
            .expect("sulfur");
        assert_eq!(graph.covalent_bond_order_sum(sulfur.id()), Some(6));
        for bond in graph.covalent_bonds().values() {
            let left = graph.atoms()[bond.left()].element().as_str();
            let right = graph.atoms()[bond.right()].element().as_str();
            assert_ne!((left, right), ("O", "O"), "no peroxide bonds");
            if left == "H" || right == "H" {
                assert!(left == "O" || right == "O", "H binds O, not S");
            }
        }
        assert!(crate::classify_bronsted_acid(&acid).is_protic_candidate());
    }

    #[test]
    fn carbon_dioxide_uses_double_bonds() {
        let carbon_dioxide = structure(&[("C", 1), ("O", 2)]).expect("CO2");
        assert!(
            carbon_dioxide
                .graph()
                .covalent_bonds()
                .values()
                .all(|bond| bond.order() == BondOrder::Double)
        );
    }

    #[test]
    fn nitrogen_is_triple_bonded() {
        let nitrogen = structure(&[("N", 2)]).expect("N2");
        assert_eq!(
            nitrogen
                .graph()
                .covalent_bonds()
                .values()
                .next()
                .expect("bond")
                .order(),
            BondOrder::Triple
        );
    }

    #[test]
    fn methane_and_ammonia_and_hydrogen_halides_generate() {
        assert!(structure(&[("C", 1), ("H", 4)]).is_some());
        assert!(structure(&[("N", 1), ("H", 3)]).is_some());
        assert!(structure(&[("H", 1), ("Cl", 1)]).is_some());
        assert!(structure(&[("H", 1), ("F", 1)]).is_some());
    }

    #[test]
    fn sodium_hydroxide_is_ionic_with_hydroxide_unit() {
        let sodium_hydroxide = structure(&[("Na", 1), ("O", 1), ("H", 1)]).expect("NaOH");
        assert_eq!(sodium_hydroxide.representation(), RepresentationKind::Ionic);
        assert_eq!(sodium_hydroxide.graph().covalent_bonds().len(), 1);
        assert_eq!(sodium_hydroxide.graph().ionic_associations().len(), 1);
        assert_eq!(sodium_hydroxide.graph().system_net_charge(), 0);
    }

    #[test]
    fn calcium_hydroxide_splits_into_two_hydroxides() {
        let calcium_hydroxide = structure(&[("Ca", 1), ("O", 2), ("H", 2)]).expect("Ca(OH)2");
        assert_eq!(calcium_hydroxide.graph().groups().len(), 3);
        assert_eq!(calcium_hydroxide.graph().covalent_bonds().len(), 2);
    }

    #[test]
    fn sodium_sulfate_generates_a_sulfate_anion() {
        let sodium_sulfate = structure(&[("Na", 2), ("S", 1), ("O", 4)]).expect("Na2SO4");
        assert_eq!(sodium_sulfate.representation(), RepresentationKind::Ionic);
        assert_eq!(sodium_sulfate.graph().system_net_charge(), 0);
        assert_eq!(sodium_sulfate.graph().groups().len(), 3);
    }

    #[test]
    fn carbonates_and_salts_generate() {
        assert!(structure(&[("Na", 2), ("C", 1), ("O", 3)]).is_some());
        assert!(structure(&[("Na", 1), ("Cl", 1)]).is_some());
        assert!(structure(&[("Ca", 1), ("O", 1)]).is_some());
        assert!(structure(&[("Mg", 1), ("F", 2)]).is_some());
    }

    #[test]
    fn elemental_metals_are_single_site_metallic_structures() {
        let sodium = structure(&[("Na", 1)]).expect("Na metal");
        assert_eq!(sodium.representation(), RepresentationKind::Metallic);
        assert_eq!(sodium.graph().system_net_charge(), 0);
        let calcium = structure(&[("Ca", 1)]).expect("Ca metal");
        assert_eq!(
            calcium
                .graph()
                .metallic_domains()
                .values()
                .next()
                .expect("domain")
                .delocalized_electrons(),
            2
        );
    }

    #[test]
    fn standard_state_allotropes_are_constructed_deterministically() {
        let phosphorus = structure(&[("P", 4)]).expect("P4");
        assert_eq!(phosphorus.graph().covalent_bonds().len(), 6);
        let arsenic = structure(&[("As", 4)]).expect("As4");
        assert_eq!(arsenic.graph().covalent_bonds().len(), 6);
        let sulfur = structure(&[("S", 8)]).expect("S8");
        assert_eq!(sulfur.graph().covalent_bonds().len(), 8);
        assert!(
            sulfur
                .graph()
                .atoms()
                .values()
                .all(|atom| atom.electrons().non_bonding_electrons() == 4)
        );
    }

    #[test]
    fn charge_separated_species_generate() {
        // Nitrate: N carries +1, two O carry -1.
        let sodium_nitrate = structure(&[("Na", 1), ("N", 1), ("O", 3)]).expect("NaNO3");
        assert_eq!(sodium_nitrate.representation(), RepresentationKind::Ionic);
        assert_eq!(sodium_nitrate.graph().system_net_charge(), 0);
        let nitrogen = sodium_nitrate
            .graph()
            .atoms()
            .values()
            .find(|atom| atom.element().as_str() == "N")
            .expect("nitrogen");
        assert_eq!(nitrogen.electrons().formal_charge(), 1);

        let silver_nitrate = structure(&[("Ag", 1), ("N", 1), ("O", 3)]).expect("AgNO3");
        assert_eq!(silver_nitrate.representation(), RepresentationKind::Ionic);

        // Nitric acid and carbon monoxide are charge-separated neutrals.
        let nitric_acid = structure(&[("H", 1), ("N", 1), ("O", 3)]).expect("HNO3");
        assert_eq!(nitric_acid.representation(), RepresentationKind::Molecular);
        assert!(crate::classify_bronsted_acid(&nitric_acid).is_protic_candidate());

        let carbon_monoxide = structure(&[("C", 1), ("O", 1)]).expect("CO");
        assert_eq!(
            carbon_monoxide
                .graph()
                .covalent_bonds()
                .values()
                .next()
                .expect("bond")
                .order(),
            BondOrder::Triple
        );
    }

    #[test]
    fn ammonium_salts_generate_with_a_polyatomic_cation() {
        let ammonium_chloride = structure(&[("N", 1), ("H", 4), ("Cl", 1)]).expect("NH4Cl");
        assert_eq!(
            ammonium_chloride.representation(),
            RepresentationKind::Ionic
        );
        assert_eq!(ammonium_chloride.graph().system_net_charge(), 0);
        let nitrogen = ammonium_chloride
            .graph()
            .atoms()
            .values()
            .find(|atom| atom.element().as_str() == "N")
            .expect("nitrogen");
        assert_eq!(nitrogen.electrons().formal_charge(), 1);

        let ammonium_nitrate = structure(&[("N", 2), ("H", 4), ("O", 3)]).expect("NH4NO3");
        assert_eq!(ammonium_nitrate.representation(), RepresentationKind::Ionic);
        assert_eq!(ammonium_nitrate.graph().groups().len(), 2);

        let ammonium_sulfate =
            structure(&[("N", 2), ("H", 8), ("S", 1), ("O", 4)]).expect("(NH4)2SO4");
        assert_eq!(ammonium_sulfate.graph().groups().len(), 3);

        // Methylamine stays a covalent molecule, not an ammonium salt.
        let methylamine = structure(&[("C", 1), ("N", 1), ("H", 5)]).expect("CH3NH2");
        assert_eq!(methylamine.representation(), RepresentationKind::Molecular);
    }

    #[test]
    fn plain_octet_solutions_still_beat_charge_separation() {
        // Water and CO2 must not regress into charge-separated variants.
        let water = structure(&[("H", 2), ("O", 1)]).expect("water");
        assert!(
            water
                .graph()
                .atoms()
                .values()
                .all(|atom| atom.electrons().formal_charge() == 0)
        );
        let carbon_dioxide = structure(&[("C", 1), ("O", 2)]).expect("CO2");
        assert!(
            carbon_dioxide
                .graph()
                .atoms()
                .values()
                .all(|atom| atom.electrons().formal_charge() == 0)
        );
    }

    #[test]
    fn unbuildable_inventories_are_declined() {
        assert!(structure(&[("He", 2)]).is_none());
        assert!(structure(&[("Na", 2)]).is_none());
    }
}
