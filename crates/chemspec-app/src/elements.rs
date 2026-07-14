//! Curated element metadata for the reaction builder.
//!
//! This is presentation/catalogue input only. Being listed here means an
//! element can be selected in the UI; it does not make any reaction involving
//! the element valid or supported by the chemistry engine.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    AlkaliMetal,
    AlkalineEarth,
    TransitionMetal,
    PostTransitionMetal,
    Metalloid,
    ReactiveNonmetal,
    Halogen,
    NobleGas,
}

impl Category {
    pub const fn label(self) -> &'static str {
        match self {
            Self::AlkaliMetal => "Alkali metal",
            Self::AlkalineEarth => "Alkaline earth",
            Self::TransitionMetal => "Transition metal",
            Self::PostTransitionMetal => "Post-transition metal",
            Self::Metalloid => "Metalloid",
            Self::ReactiveNonmetal => "Reactive nonmetal",
            Self::Halogen => "Halogen",
            Self::NobleGas => "Noble gas",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ElementSpec {
    pub atomic_number: u8,
    pub symbol: &'static str,
    pub name: &'static str,
    pub atomic_mass: &'static str,
    pub period: u8,
    pub group: u8,
    pub valence_electrons: u8,
    pub category: Category,
}

pub const SUPPORTED: &[ElementSpec] = &[
    ElementSpec {
        atomic_number: 1,
        symbol: "H",
        name: "Hydrogen",
        atomic_mass: "1.008",
        period: 1,
        group: 1,
        valence_electrons: 1,
        category: Category::ReactiveNonmetal,
    },
    ElementSpec {
        atomic_number: 2,
        symbol: "He",
        name: "Helium",
        atomic_mass: "4.0026",
        period: 1,
        group: 18,
        valence_electrons: 2,
        category: Category::NobleGas,
    },
    ElementSpec {
        atomic_number: 3,
        symbol: "Li",
        name: "Lithium",
        atomic_mass: "6.94",
        period: 2,
        group: 1,
        valence_electrons: 1,
        category: Category::AlkaliMetal,
    },
    ElementSpec {
        atomic_number: 4,
        symbol: "Be",
        name: "Beryllium",
        atomic_mass: "9.0122",
        period: 2,
        group: 2,
        valence_electrons: 2,
        category: Category::AlkalineEarth,
    },
    ElementSpec {
        atomic_number: 5,
        symbol: "B",
        name: "Boron",
        atomic_mass: "10.81",
        period: 2,
        group: 13,
        valence_electrons: 3,
        category: Category::Metalloid,
    },
    ElementSpec {
        atomic_number: 6,
        symbol: "C",
        name: "Carbon",
        atomic_mass: "12.011",
        period: 2,
        group: 14,
        valence_electrons: 4,
        category: Category::ReactiveNonmetal,
    },
    ElementSpec {
        atomic_number: 7,
        symbol: "N",
        name: "Nitrogen",
        atomic_mass: "14.007",
        period: 2,
        group: 15,
        valence_electrons: 5,
        category: Category::ReactiveNonmetal,
    },
    ElementSpec {
        atomic_number: 8,
        symbol: "O",
        name: "Oxygen",
        atomic_mass: "15.999",
        period: 2,
        group: 16,
        valence_electrons: 6,
        category: Category::ReactiveNonmetal,
    },
    ElementSpec {
        atomic_number: 9,
        symbol: "F",
        name: "Fluorine",
        atomic_mass: "18.998",
        period: 2,
        group: 17,
        valence_electrons: 7,
        category: Category::Halogen,
    },
    ElementSpec {
        atomic_number: 10,
        symbol: "Ne",
        name: "Neon",
        atomic_mass: "20.180",
        period: 2,
        group: 18,
        valence_electrons: 8,
        category: Category::NobleGas,
    },
    ElementSpec {
        atomic_number: 11,
        symbol: "Na",
        name: "Sodium",
        atomic_mass: "22.990",
        period: 3,
        group: 1,
        valence_electrons: 1,
        category: Category::AlkaliMetal,
    },
    ElementSpec {
        atomic_number: 12,
        symbol: "Mg",
        name: "Magnesium",
        atomic_mass: "24.305",
        period: 3,
        group: 2,
        valence_electrons: 2,
        category: Category::AlkalineEarth,
    },
    ElementSpec {
        atomic_number: 13,
        symbol: "Al",
        name: "Aluminium",
        atomic_mass: "26.982",
        period: 3,
        group: 13,
        valence_electrons: 3,
        category: Category::PostTransitionMetal,
    },
    ElementSpec {
        atomic_number: 14,
        symbol: "Si",
        name: "Silicon",
        atomic_mass: "28.085",
        period: 3,
        group: 14,
        valence_electrons: 4,
        category: Category::Metalloid,
    },
    ElementSpec {
        atomic_number: 15,
        symbol: "P",
        name: "Phosphorus",
        atomic_mass: "30.974",
        period: 3,
        group: 15,
        valence_electrons: 5,
        category: Category::ReactiveNonmetal,
    },
    ElementSpec {
        atomic_number: 16,
        symbol: "S",
        name: "Sulfur",
        atomic_mass: "32.06",
        period: 3,
        group: 16,
        valence_electrons: 6,
        category: Category::ReactiveNonmetal,
    },
    ElementSpec {
        atomic_number: 17,
        symbol: "Cl",
        name: "Chlorine",
        atomic_mass: "35.45",
        period: 3,
        group: 17,
        valence_electrons: 7,
        category: Category::Halogen,
    },
    ElementSpec {
        atomic_number: 18,
        symbol: "Ar",
        name: "Argon",
        atomic_mass: "39.948",
        period: 3,
        group: 18,
        valence_electrons: 8,
        category: Category::NobleGas,
    },
    ElementSpec {
        atomic_number: 19,
        symbol: "K",
        name: "Potassium",
        atomic_mass: "39.098",
        period: 4,
        group: 1,
        valence_electrons: 1,
        category: Category::AlkaliMetal,
    },
    ElementSpec {
        atomic_number: 20,
        symbol: "Ca",
        name: "Calcium",
        atomic_mass: "40.078",
        period: 4,
        group: 2,
        valence_electrons: 2,
        category: Category::AlkalineEarth,
    },
    ElementSpec {
        atomic_number: 26,
        symbol: "Fe",
        name: "Iron",
        atomic_mass: "55.845",
        period: 4,
        group: 8,
        valence_electrons: 2,
        category: Category::TransitionMetal,
    },
    ElementSpec {
        atomic_number: 29,
        symbol: "Cu",
        name: "Copper",
        atomic_mass: "63.546",
        period: 4,
        group: 11,
        valence_electrons: 1,
        category: Category::TransitionMetal,
    },
    ElementSpec {
        atomic_number: 30,
        symbol: "Zn",
        name: "Zinc",
        atomic_mass: "65.38",
        period: 4,
        group: 12,
        valence_electrons: 2,
        category: Category::TransitionMetal,
    },
    ElementSpec {
        atomic_number: 35,
        symbol: "Br",
        name: "Bromine",
        atomic_mass: "79.904",
        period: 4,
        group: 17,
        valence_electrons: 7,
        category: Category::Halogen,
    },
    ElementSpec {
        atomic_number: 47,
        symbol: "Ag",
        name: "Silver",
        atomic_mass: "107.8682",
        period: 5,
        group: 11,
        valence_electrons: 1,
        category: Category::TransitionMetal,
    },
    ElementSpec {
        atomic_number: 53,
        symbol: "I",
        name: "Iodine",
        atomic_mass: "126.90447",
        period: 5,
        group: 17,
        valence_electrons: 7,
        category: Category::Halogen,
    },
];

pub fn by_atomic_number(atomic_number: u8) -> Option<&'static ElementSpec> {
    SUPPORTED
        .iter()
        .find(|element| element.atomic_number == atomic_number)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn element_positions_and_atomic_numbers_are_unique() {
        for (index, element) in SUPPORTED.iter().enumerate() {
            assert!((1..=5).contains(&element.period));
            assert!((1..=18).contains(&element.group));
            assert!(SUPPORTED[..index].iter().all(|other| {
                other.atomic_number != element.atomic_number
                    && (other.period, other.group) != (element.period, element.group)
            }));
            assert!((1..=8).contains(&element.valence_electrons));
        }
    }
}
