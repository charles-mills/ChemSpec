//! Curated composition previews for the Stage 2 reaction workspace.
//!
//! These patterns improve composition feedback, but they are not validation
//! results. The chemistry engine remains the only authority that may turn a
//! request into trusted chemical meaning.

use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompositionPreview {
    pub formula: &'static str,
    pub name: &'static str,
    atoms: &'static [(u8, u8)],
}

impl CompositionPreview {
    pub fn matches(self, atomic_numbers: impl IntoIterator<Item = u8>) -> bool {
        let actual =
            atomic_numbers
                .into_iter()
                .fold(BTreeMap::new(), |mut counts, atomic_number| {
                    *counts.entry(atomic_number).or_insert(0_usize) += 1;
                    counts
                });
        let expected = self
            .atoms
            .iter()
            .map(|(atomic_number, count)| (*atomic_number, usize::from(*count)))
            .collect::<BTreeMap<_, _>>();

        actual == expected
    }
}

pub const SUPPORTED: &[CompositionPreview] = &[
    CompositionPreview {
        formula: "H₂",
        name: "Hydrogen",
        atoms: &[(1, 2)],
    },
    CompositionPreview {
        formula: "O₂",
        name: "Oxygen",
        atoms: &[(8, 2)],
    },
    CompositionPreview {
        formula: "H₂O",
        name: "Water",
        atoms: &[(1, 2), (8, 1)],
    },
    CompositionPreview {
        formula: "LiOH",
        name: "Lithium hydroxide",
        atoms: &[(1, 1), (3, 1), (8, 1)],
    },
    CompositionPreview {
        formula: "NaCl",
        name: "Sodium chloride",
        atoms: &[(11, 1), (17, 1)],
    },
    CompositionPreview {
        formula: "CO₂",
        name: "Carbon dioxide",
        atoms: &[(6, 1), (8, 2)],
    },
];

pub fn recognize(atomic_numbers: impl IntoIterator<Item = u8>) -> Option<CompositionPreview> {
    let atomic_numbers = atomic_numbers.into_iter().collect::<Vec<_>>();

    SUPPORTED
        .iter()
        .copied()
        .find(|preview| preview.matches(atomic_numbers.iter().copied()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognition_is_order_independent_and_closed_world() {
        assert_eq!(recognize([8, 1, 1]).map(|item| item.formula), Some("H₂O"));
        assert_eq!(recognize([17, 11]).map(|item| item.formula), Some("NaCl"));
        assert!(recognize([6, 6]).is_none());
        assert!(recognize([1, 8]).is_none());
    }
}
