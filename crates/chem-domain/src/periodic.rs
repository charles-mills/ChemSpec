//! The periodic table as code. Symbols and atomic numbers are physics, not
//! sourced data; no catalogue attestation is required to know them.

use std::sync::LazyLock;

use crate::{Element, ElementId, ElementSymbol, StaticElementRegistry};

pub const ELEMENT_SYMBOLS: [&str; 118] = [
    "H", "He", "Li", "Be", "B", "C", "N", "O", "F", "Ne", "Na", "Mg", "Al", "Si", "P", "S", "Cl",
    "Ar", "K", "Ca", "Sc", "Ti", "V", "Cr", "Mn", "Fe", "Co", "Ni", "Cu", "Zn", "Ga", "Ge", "As",
    "Se", "Br", "Kr", "Rb", "Sr", "Y", "Zr", "Nb", "Mo", "Tc", "Ru", "Rh", "Pd", "Ag", "Cd", "In",
    "Sn", "Sb", "Te", "I", "Xe", "Cs", "Ba", "La", "Ce", "Pr", "Nd", "Pm", "Sm", "Eu", "Gd", "Tb",
    "Dy", "Ho", "Er", "Tm", "Yb", "Lu", "Hf", "Ta", "W", "Re", "Os", "Ir", "Pt", "Au", "Hg", "Tl",
    "Pb", "Bi", "Po", "At", "Rn", "Fr", "Ra", "Ac", "Th", "Pa", "U", "Np", "Pu", "Am", "Cm", "Bk",
    "Cf", "Es", "Fm", "Md", "No", "Lr", "Rf", "Db", "Sg", "Bh", "Hs", "Mt", "Ds", "Rg", "Cn",
    "Nh", "Fl", "Mc", "Lv", "Ts", "Og",
];

static REGISTRY: LazyLock<StaticElementRegistry> = LazyLock::new(|| {
    StaticElementRegistry::new(ELEMENT_SYMBOLS.iter().enumerate().map(|(index, symbol)| {
        Element {
            id: ElementId::new(u16::try_from(index + 1).expect("small")).expect("nonzero"),
            symbol: ElementSymbol::new(*symbol).expect("valid symbol"),
        }
    }))
    .expect("unique periodic table")
});

/// The complete element registry, straight from the periodic table.
#[must_use]
pub fn element_registry() -> &'static StaticElementRegistry {
    &REGISTRY
}

/// Symbol for an atomic number, when one exists.
#[must_use]
pub fn symbol_of(atomic_number: u8) -> Option<&'static str> {
    (atomic_number >= 1).then(|| ELEMENT_SYMBOLS.get(usize::from(atomic_number) - 1).copied())?
}

/// English element names, indexed by atomic number - 1.
pub const ELEMENT_NAMES: [&str; 118] = [
    "hydrogen",
    "helium",
    "lithium",
    "beryllium",
    "boron",
    "carbon",
    "nitrogen",
    "oxygen",
    "fluorine",
    "neon",
    "sodium",
    "magnesium",
    "aluminium",
    "silicon",
    "phosphorus",
    "sulfur",
    "chlorine",
    "argon",
    "potassium",
    "calcium",
    "scandium",
    "titanium",
    "vanadium",
    "chromium",
    "manganese",
    "iron",
    "cobalt",
    "nickel",
    "copper",
    "zinc",
    "gallium",
    "germanium",
    "arsenic",
    "selenium",
    "bromine",
    "krypton",
    "rubidium",
    "strontium",
    "yttrium",
    "zirconium",
    "niobium",
    "molybdenum",
    "technetium",
    "ruthenium",
    "rhodium",
    "palladium",
    "silver",
    "cadmium",
    "indium",
    "tin",
    "antimony",
    "tellurium",
    "iodine",
    "xenon",
    "caesium",
    "barium",
    "lanthanum",
    "cerium",
    "praseodymium",
    "neodymium",
    "promethium",
    "samarium",
    "europium",
    "gadolinium",
    "terbium",
    "dysprosium",
    "holmium",
    "erbium",
    "thulium",
    "ytterbium",
    "lutetium",
    "hafnium",
    "tantalum",
    "tungsten",
    "rhenium",
    "osmium",
    "iridium",
    "platinum",
    "gold",
    "mercury",
    "thallium",
    "lead",
    "bismuth",
    "polonium",
    "astatine",
    "radon",
    "francium",
    "radium",
    "actinium",
    "thorium",
    "protactinium",
    "uranium",
    "neptunium",
    "plutonium",
    "americium",
    "curium",
    "berkelium",
    "californium",
    "einsteinium",
    "fermium",
    "mendelevium",
    "nobelium",
    "lawrencium",
    "rutherfordium",
    "dubnium",
    "seaborgium",
    "bohrium",
    "hassium",
    "meitnerium",
    "darmstadtium",
    "roentgenium",
    "copernicium",
    "nihonium",
    "flerovium",
    "moscovium",
    "livermorium",
    "tennessine",
    "oganesson",
];

/// English name for an element symbol.
#[must_use]
pub fn element_name(symbol: &str) -> Option<&'static str> {
    ELEMENT_SYMBOLS
        .iter()
        .position(|candidate| *candidate == symbol)
        .map(|index| ELEMENT_NAMES[index])
}

/// Neutral valence electron counts, indexed by atomic number - 1.
const VALENCE_ELECTRONS: [u8; 118] = [
    1, 2, // H He
    1, 2, 3, 4, 5, 6, 7, 8, // Li..Ne
    1, 2, 3, 4, 5, 6, 7, 8, // Na..Ar
    1, 2, 2, 2, 2, 1, 2, 2, 2, 2, 1, 2, 3, 4, 5, 6, 7, 8, // K..Kr
    1, 2, 2, 2, 1, 1, 2, 1, 1, 2, 1, 2, 3, 4, 5, 6, 7, 8, // Rb..Xe
    1, 2, // Cs Ba
    2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, // La..Lu
    2, 2, 2, 2, 2, 2, 1, 1, 2, 3, 4, 5, 6, 7, 8, // Hf..Rn
    1, 2, // Fr Ra
    2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, // Ac..Lr
    2, 2, 2, 2, 2, 2, 2, 1, 2, 3, 4, 5, 6, 7, 8, // Rf..Og
];

/// Neutral valence electrons for an element symbol.
#[must_use]
pub fn valence_electrons_of(symbol: &str) -> Option<u8> {
    ELEMENT_SYMBOLS
        .iter()
        .position(|candidate| *candidate == symbol)
        .map(|index| VALENCE_ELECTRONS[index])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ElementRegistry;

    #[test]
    fn registry_covers_all_118_elements() {
        assert_eq!(symbol_of(1), Some("H"));
        assert_eq!(symbol_of(16), Some("S"));
        assert_eq!(symbol_of(118), Some("Og"));
        assert_eq!(symbol_of(119), None);
        assert_eq!(symbol_of(0), None);
        let sulfur = ElementSymbol::new("S").expect("symbol");
        assert_eq!(
            element_registry()
                .resolve(&sulfur)
                .map(|element| element.id.atomic_number()),
            Some(16)
        );
    }
}
