//! Cahn-Ingold-Prelog-lite stereodescriptors: R/S for tetrahedral
//! stereocentres whose four branches are acyclic and saturated, ranked by
//! sphere-wise atomic-number comparison. Anything richer — rings through
//! the centre, multiple bonds in a branch, ties that survive the walk —
//! returns None: a wrong descriptor is worse than none.

use std::collections::BTreeSet;

use crate::identity::AtomId;
use crate::periodic::ELEMENT_SYMBOLS;
use crate::structural::{BondOrder, StructureDefinition, TetrahedralHandedness};

/// The classic descriptor pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StereoDescriptor {
    R,
    S,
}

fn atomic_number(symbol: &str) -> Option<u8> {
    ELEMENT_SYMBOLS
        .iter()
        .position(|candidate| *candidate == symbol)
        .and_then(|index| u8::try_from(index + 1).ok())
}

/// The R/S descriptor of one chiral atom, when CIP-lite can rank its
/// branches decisively.
#[must_use]
#[allow(clippy::missing_panics_doc)]
pub fn stereocentre_descriptor(
    structure: &StructureDefinition,
    atom: &AtomId,
) -> Option<StereoDescriptor> {
    let graph = structure.graph();
    let chirality = graph.atoms().get(atom)?.chirality()?;
    let neighbours = chirality.neighbours();

    // Branch signatures: breadth-first spheres of descending atomic
    // numbers, walking away from the stereocentre. Multiple bonds and
    // ring re-entries are out of the subset.
    let mut signatures = Vec::new();
    for neighbour in neighbours {
        signatures.push(branch_signature(structure, atom, neighbour)?);
    }
    // Rank descending; any tie is indecisive.
    let mut order: Vec<usize> = (0..4).collect();
    order.sort_by(|a, b| signatures[*b].cmp(&signatures[*a]));
    for pair in order.windows(2) {
        if signatures[pair[0]] == signatures[pair[1]] {
            return None;
        }
    }
    // Arrange the stored tuple as [lowest, first, second, third]: viewed
    // from the lowest-priority branch, a counterclockwise run of the
    // remaining three (in priority order) appears clockwise from the far
    // side — the R convention.
    let target = [order[3], order[0], order[1], order[2]];
    let mut positions: Vec<usize> = (0..4)
        .map(|slot| {
            target
                .iter()
                .position(|entry| *entry == slot)
                .expect("permutation")
        })
        .collect();
    let mut swaps = 0;
    for index in 0..4 {
        while positions[index] != index {
            let destination = positions[index];
            positions.swap(index, destination);
            swaps += 1;
        }
    }
    let mut handedness = chirality.handedness();
    if swaps % 2 == 1 {
        handedness = match handedness {
            TetrahedralHandedness::Counterclockwise => TetrahedralHandedness::Clockwise,
            TetrahedralHandedness::Clockwise => TetrahedralHandedness::Counterclockwise,
        };
    }
    Some(match handedness {
        TetrahedralHandedness::Counterclockwise => StereoDescriptor::R,
        TetrahedralHandedness::Clockwise => StereoDescriptor::S,
    })
}

/// Sphere-wise branch signature walking away from the centre: sphere 0 is
/// the branch atom itself, each later sphere the sorted-descending atomic
/// numbers one bond further out. None on multiple bonds or ring re-entry.
fn branch_signature(
    structure: &StructureDefinition,
    centre: &AtomId,
    branch: &AtomId,
) -> Option<Vec<Vec<u8>>> {
    let graph = structure.graph();
    let mut visited: BTreeSet<&AtomId> = BTreeSet::new();
    visited.insert(centre);
    visited.insert(branch);
    let mut sphere = vec![branch.clone()];
    let mut signature = vec![vec![
        atomic_number(graph.atoms().get(branch)?.element().as_str())?,
    ]];
    while !sphere.is_empty() {
        let mut next = Vec::new();
        let mut numbers = Vec::new();
        for current in &sphere {
            for bond in graph.covalent_bonds().values() {
                let other = if bond.left() == current {
                    bond.right()
                } else if bond.right() == current {
                    bond.left()
                } else {
                    continue;
                };
                if bond.order() != BondOrder::Single {
                    return None;
                }
                if other == centre && *current != *branch {
                    // The walk found its way back around: a ring through
                    // the centre.
                    return None;
                }
                if !visited.insert(other) {
                    continue;
                }
                numbers.push(atomic_number(graph.atoms().get(other)?.element().as_str())?);
                next.push(other.clone());
            }
        }
        if !numbers.is_empty() {
            numbers.sort_unstable_by(|a, b| b.cmp(a));
            signature.push(numbers);
        }
        sphere = next;
    }
    Some(signature)
}
