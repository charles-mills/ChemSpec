//! Curated composition previews for the Stage 2 reaction workspace.
//!
//! These patterns improve composition feedback, but they are not validation
//! results. The chemistry engine remains the only authority that may turn a
//! request into trusted chemical meaning.

use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CompositionId {
    Hydrogen,
    Oxygen,
    Water,
    LithiumHydroxide,
    SodiumHydroxide,
    PotassiumHydroxide,
    SodiumFluoride,
    SodiumChloride,
    SodiumBromide,
    SodiumIodide,
    SilverNitrate,
    CarbonDioxide,
    HydrogenFluoride,
    HydrogenChloride,
    HydrogenBromide,
    HydrogenIodide,
    LithiumCarbonate,
    SodiumCarbonate,
    PotassiumCarbonate,
    LithiumBicarbonate,
    SodiumBicarbonate,
    PotassiumBicarbonate,
    Fluorine,
    Chlorine,
    Bromine,
    Iodine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompositionKind {
    ElementalSubstance,
    Compound,
}

impl CompositionKind {
    pub const fn recognition_label(self) -> &'static str {
        match self {
            Self::ElementalSubstance => "Recognised substance",
            Self::Compound => "Recognised compound",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompositionPreview {
    pub id: CompositionId,
    pub formula: &'static str,
    pub name: &'static str,
    pub(crate) atoms: &'static [(u8, u8)],
}

impl CompositionPreview {
    pub const fn kind(self) -> CompositionKind {
        match self.id {
            CompositionId::Hydrogen
            | CompositionId::Oxygen
            | CompositionId::Fluorine
            | CompositionId::Chlorine
            | CompositionId::Bromine
            | CompositionId::Iodine => CompositionKind::ElementalSubstance,
            _ => CompositionKind::Compound,
        }
    }

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
        id: CompositionId::Hydrogen,
        formula: "H₂",
        name: "Hydrogen",
        atoms: &[(1, 2)],
    },
    CompositionPreview {
        id: CompositionId::Oxygen,
        formula: "O₂",
        name: "Oxygen",
        atoms: &[(8, 2)],
    },
    CompositionPreview {
        id: CompositionId::Water,
        formula: "H₂O",
        name: "Water",
        atoms: &[(1, 2), (8, 1)],
    },
    CompositionPreview {
        id: CompositionId::LithiumHydroxide,
        formula: "LiOH",
        name: "Lithium hydroxide",
        atoms: &[(1, 1), (3, 1), (8, 1)],
    },
    CompositionPreview {
        id: CompositionId::SodiumHydroxide,
        formula: "NaOH",
        name: "Sodium hydroxide",
        atoms: &[(1, 1), (8, 1), (11, 1)],
    },
    CompositionPreview {
        id: CompositionId::PotassiumHydroxide,
        formula: "KOH",
        name: "Potassium hydroxide",
        atoms: &[(1, 1), (8, 1), (19, 1)],
    },
    CompositionPreview {
        id: CompositionId::SodiumFluoride,
        formula: "NaF",
        name: "Sodium fluoride",
        atoms: &[(9, 1), (11, 1)],
    },
    CompositionPreview {
        id: CompositionId::SodiumChloride,
        formula: "NaCl",
        name: "Sodium chloride",
        atoms: &[(11, 1), (17, 1)],
    },
    CompositionPreview {
        id: CompositionId::SodiumBromide,
        formula: "NaBr",
        name: "Sodium bromide",
        atoms: &[(11, 1), (35, 1)],
    },
    CompositionPreview {
        id: CompositionId::SodiumIodide,
        formula: "NaI",
        name: "Sodium iodide",
        atoms: &[(11, 1), (53, 1)],
    },
    CompositionPreview {
        id: CompositionId::SilverNitrate,
        formula: "AgNO₃",
        name: "Silver nitrate",
        atoms: &[(7, 1), (8, 3), (47, 1)],
    },
    CompositionPreview {
        id: CompositionId::CarbonDioxide,
        formula: "CO₂",
        name: "Carbon dioxide",
        atoms: &[(6, 1), (8, 2)],
    },
    CompositionPreview {
        id: CompositionId::HydrogenFluoride,
        formula: "HF",
        name: "Hydrogen fluoride",
        atoms: &[(1, 1), (9, 1)],
    },
    CompositionPreview {
        id: CompositionId::HydrogenChloride,
        formula: "HCl",
        name: "Hydrogen chloride",
        atoms: &[(1, 1), (17, 1)],
    },
    CompositionPreview {
        id: CompositionId::HydrogenBromide,
        formula: "HBr",
        name: "Hydrogen bromide",
        atoms: &[(1, 1), (35, 1)],
    },
    CompositionPreview {
        id: CompositionId::HydrogenIodide,
        formula: "HI",
        name: "Hydrogen iodide",
        atoms: &[(1, 1), (53, 1)],
    },
    CompositionPreview {
        id: CompositionId::LithiumCarbonate,
        formula: "Li₂CO₃",
        name: "Lithium carbonate",
        atoms: &[(3, 2), (6, 1), (8, 3)],
    },
    CompositionPreview {
        id: CompositionId::SodiumCarbonate,
        formula: "Na₂CO₃",
        name: "Sodium carbonate",
        atoms: &[(6, 1), (8, 3), (11, 2)],
    },
    CompositionPreview {
        id: CompositionId::PotassiumCarbonate,
        formula: "K₂CO₃",
        name: "Potassium carbonate",
        atoms: &[(6, 1), (8, 3), (19, 2)],
    },
    CompositionPreview {
        id: CompositionId::LithiumBicarbonate,
        formula: "LiHCO₃",
        name: "Lithium bicarbonate",
        atoms: &[(1, 1), (3, 1), (6, 1), (8, 3)],
    },
    CompositionPreview {
        id: CompositionId::SodiumBicarbonate,
        formula: "NaHCO₃",
        name: "Sodium bicarbonate",
        atoms: &[(1, 1), (6, 1), (8, 3), (11, 1)],
    },
    CompositionPreview {
        id: CompositionId::PotassiumBicarbonate,
        formula: "KHCO₃",
        name: "Potassium bicarbonate",
        atoms: &[(1, 1), (6, 1), (8, 3), (19, 1)],
    },
    CompositionPreview {
        id: CompositionId::Fluorine,
        formula: "F₂",
        name: "Fluorine",
        atoms: &[(9, 2)],
    },
    CompositionPreview {
        id: CompositionId::Chlorine,
        formula: "Cl₂",
        name: "Chlorine",
        atoms: &[(17, 2)],
    },
    CompositionPreview {
        id: CompositionId::Bromine,
        formula: "Br₂",
        name: "Bromine",
        atoms: &[(35, 2)],
    },
    CompositionPreview {
        id: CompositionId::Iodine,
        formula: "I₂",
        name: "Iodine",
        atoms: &[(53, 2)],
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
    fn supported_compositions_have_unique_identities_and_atom_inventories() {
        let mut ids = std::collections::BTreeSet::new();
        let mut inventories = std::collections::BTreeSet::new();
        for preview in SUPPORTED {
            assert!(ids.insert(preview.id), "duplicate composition ID");
            assert!(
                inventories.insert(preview.atoms),
                "duplicate atom inventory for {}",
                preview.formula
            );
        }
    }

    #[test]
    fn elemental_halogen_previews_are_not_labelled_as_compounds() {
        let bromine = recognize([35, 35]).expect("bromine is recognized");
        assert_eq!(bromine.kind(), CompositionKind::ElementalSubstance);
        assert_eq!(bromine.kind().recognition_label(), "Recognised substance");

        let sodium_bromide = recognize([11, 35]).expect("sodium bromide is recognized");
        assert_eq!(sodium_bromide.kind(), CompositionKind::Compound);
        assert_eq!(
            sodium_bromide.kind().recognition_label(),
            "Recognised compound"
        );
    }

    #[test]
    fn recognition_is_order_independent_and_closed_world() {
        assert_eq!(recognize([8, 1, 1]).map(|item| item.formula), Some("H₂O"));
        assert_eq!(recognize([17, 11]).map(|item| item.formula), Some("NaCl"));
        assert_eq!(
            recognize([47, 7, 8, 8, 8]).map(|item| item.formula),
            Some("AgNO₃")
        );
        assert_eq!(
            recognize([8, 19, 1]).map(|item| item.id),
            Some(CompositionId::PotassiumHydroxide)
        );
        assert_eq!(
            recognize([8, 6, 3, 8, 3, 8]).map(|item| item.id),
            Some(CompositionId::LithiumCarbonate)
        );
        assert_eq!(
            recognize([35, 35]).map(|item| item.id),
            Some(CompositionId::Bromine)
        );
        assert!(recognize([6, 6]).is_none());
        assert!(recognize([1, 8]).is_none());
    }
}
