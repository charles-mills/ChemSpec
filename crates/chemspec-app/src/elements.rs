//! Complete presentation metadata for the reaction-builder periodic table.
//!
//! Listing an element here only makes it available as user input. It does not
//! make a substance or reaction involving that element chemically supported.

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
    Lanthanide,
    Actinide,
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
            Self::Lanthanide => "Lanthanide",
            Self::Actinide => "Actinide",
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

macro_rules! e {
    ($z:literal, $symbol:literal, $name:literal, $mass:literal, $period:literal, $group:literal, $valence:literal, $category:ident) => {
        ElementSpec {
            atomic_number: $z,
            symbol: $symbol,
            name: $name,
            atomic_mass: $mass,
            period: $period,
            group: $group,
            valence_electrons: $valence,
            category: Category::$category,
        }
    };
}

pub const SUPPORTED: &[ElementSpec] = &[
    e!(1, "H", "Hydrogen", "1.008", 1, 1, 1, ReactiveNonmetal),
    e!(2, "He", "Helium", "4.0026", 1, 18, 2, NobleGas),
    e!(3, "Li", "Lithium", "6.94", 2, 1, 1, AlkaliMetal),
    e!(4, "Be", "Beryllium", "9.0122", 2, 2, 2, AlkalineEarth),
    e!(5, "B", "Boron", "10.81", 2, 13, 3, Metalloid),
    e!(6, "C", "Carbon", "12.011", 2, 14, 4, ReactiveNonmetal),
    e!(7, "N", "Nitrogen", "14.007", 2, 15, 5, ReactiveNonmetal),
    e!(8, "O", "Oxygen", "15.999", 2, 16, 6, ReactiveNonmetal),
    e!(9, "F", "Fluorine", "18.998", 2, 17, 7, Halogen),
    e!(10, "Ne", "Neon", "20.180", 2, 18, 8, NobleGas),
    e!(11, "Na", "Sodium", "22.990", 3, 1, 1, AlkaliMetal),
    e!(12, "Mg", "Magnesium", "24.305", 3, 2, 2, AlkalineEarth),
    e!(
        13,
        "Al",
        "Aluminium",
        "26.982",
        3,
        13,
        3,
        PostTransitionMetal
    ),
    e!(14, "Si", "Silicon", "28.085", 3, 14, 4, Metalloid),
    e!(15, "P", "Phosphorus", "30.974", 3, 15, 5, ReactiveNonmetal),
    e!(16, "S", "Sulfur", "32.06", 3, 16, 6, ReactiveNonmetal),
    e!(17, "Cl", "Chlorine", "35.45", 3, 17, 7, Halogen),
    e!(18, "Ar", "Argon", "39.948", 3, 18, 8, NobleGas),
    e!(19, "K", "Potassium", "39.098", 4, 1, 1, AlkaliMetal),
    e!(20, "Ca", "Calcium", "40.078", 4, 2, 2, AlkalineEarth),
    e!(21, "Sc", "Scandium", "44.956", 4, 3, 2, TransitionMetal),
    e!(22, "Ti", "Titanium", "47.867", 4, 4, 2, TransitionMetal),
    e!(23, "V", "Vanadium", "50.942", 4, 5, 2, TransitionMetal),
    e!(24, "Cr", "Chromium", "51.996", 4, 6, 1, TransitionMetal),
    e!(25, "Mn", "Manganese", "54.938", 4, 7, 2, TransitionMetal),
    e!(26, "Fe", "Iron", "55.845", 4, 8, 2, TransitionMetal),
    e!(27, "Co", "Cobalt", "58.933", 4, 9, 2, TransitionMetal),
    e!(28, "Ni", "Nickel", "58.693", 4, 10, 2, TransitionMetal),
    e!(29, "Cu", "Copper", "63.546", 4, 11, 1, TransitionMetal),
    e!(30, "Zn", "Zinc", "65.38", 4, 12, 2, TransitionMetal),
    e!(31, "Ga", "Gallium", "69.723", 4, 13, 3, PostTransitionMetal),
    e!(32, "Ge", "Germanium", "72.630", 4, 14, 4, Metalloid),
    e!(33, "As", "Arsenic", "74.922", 4, 15, 5, Metalloid),
    e!(34, "Se", "Selenium", "78.971", 4, 16, 6, ReactiveNonmetal),
    e!(35, "Br", "Bromine", "79.904", 4, 17, 7, Halogen),
    e!(36, "Kr", "Krypton", "83.798", 4, 18, 8, NobleGas),
    e!(37, "Rb", "Rubidium", "85.468", 5, 1, 1, AlkaliMetal),
    e!(38, "Sr", "Strontium", "87.62", 5, 2, 2, AlkalineEarth),
    e!(39, "Y", "Yttrium", "88.906", 5, 3, 2, TransitionMetal),
    e!(40, "Zr", "Zirconium", "91.224", 5, 4, 2, TransitionMetal),
    e!(41, "Nb", "Niobium", "92.906", 5, 5, 1, TransitionMetal),
    e!(42, "Mo", "Molybdenum", "95.95", 5, 6, 1, TransitionMetal),
    e!(43, "Tc", "Technetium", "[98]", 5, 7, 2, TransitionMetal),
    e!(44, "Ru", "Ruthenium", "101.07", 5, 8, 1, TransitionMetal),
    e!(45, "Rh", "Rhodium", "102.91", 5, 9, 1, TransitionMetal),
    e!(46, "Pd", "Palladium", "106.42", 5, 10, 2, TransitionMetal),
    e!(47, "Ag", "Silver", "107.87", 5, 11, 1, TransitionMetal),
    e!(48, "Cd", "Cadmium", "112.41", 5, 12, 2, TransitionMetal),
    e!(49, "In", "Indium", "114.82", 5, 13, 3, PostTransitionMetal),
    e!(50, "Sn", "Tin", "118.71", 5, 14, 4, PostTransitionMetal),
    e!(51, "Sb", "Antimony", "121.76", 5, 15, 5, Metalloid),
    e!(52, "Te", "Tellurium", "127.60", 5, 16, 6, Metalloid),
    e!(53, "I", "Iodine", "126.90", 5, 17, 7, Halogen),
    e!(54, "Xe", "Xenon", "131.29", 5, 18, 8, NobleGas),
    e!(55, "Cs", "Caesium", "132.91", 6, 1, 1, AlkaliMetal),
    e!(56, "Ba", "Barium", "137.33", 6, 2, 2, AlkalineEarth),
    e!(57, "La", "Lanthanum", "138.91", 6, 3, 2, Lanthanide),
    e!(58, "Ce", "Cerium", "140.12", 6, 3, 2, Lanthanide),
    e!(59, "Pr", "Praseodymium", "140.91", 6, 3, 2, Lanthanide),
    e!(60, "Nd", "Neodymium", "144.24", 6, 3, 2, Lanthanide),
    e!(61, "Pm", "Promethium", "[145]", 6, 3, 2, Lanthanide),
    e!(62, "Sm", "Samarium", "150.36", 6, 3, 2, Lanthanide),
    e!(63, "Eu", "Europium", "151.96", 6, 3, 2, Lanthanide),
    e!(64, "Gd", "Gadolinium", "157.25", 6, 3, 2, Lanthanide),
    e!(65, "Tb", "Terbium", "158.93", 6, 3, 2, Lanthanide),
    e!(66, "Dy", "Dysprosium", "162.50", 6, 3, 2, Lanthanide),
    e!(67, "Ho", "Holmium", "164.93", 6, 3, 2, Lanthanide),
    e!(68, "Er", "Erbium", "167.26", 6, 3, 2, Lanthanide),
    e!(69, "Tm", "Thulium", "168.93", 6, 3, 2, Lanthanide),
    e!(70, "Yb", "Ytterbium", "173.05", 6, 3, 2, Lanthanide),
    e!(71, "Lu", "Lutetium", "174.97", 6, 3, 2, Lanthanide),
    e!(72, "Hf", "Hafnium", "178.49", 6, 4, 2, TransitionMetal),
    e!(73, "Ta", "Tantalum", "180.95", 6, 5, 2, TransitionMetal),
    e!(74, "W", "Tungsten", "183.84", 6, 6, 2, TransitionMetal),
    e!(75, "Re", "Rhenium", "186.21", 6, 7, 2, TransitionMetal),
    e!(76, "Os", "Osmium", "190.23", 6, 8, 2, TransitionMetal),
    e!(77, "Ir", "Iridium", "192.22", 6, 9, 2, TransitionMetal),
    e!(78, "Pt", "Platinum", "195.08", 6, 10, 1, TransitionMetal),
    e!(79, "Au", "Gold", "196.97", 6, 11, 1, TransitionMetal),
    e!(80, "Hg", "Mercury", "200.59", 6, 12, 2, TransitionMetal),
    e!(
        81,
        "Tl",
        "Thallium",
        "204.38",
        6,
        13,
        3,
        PostTransitionMetal
    ),
    e!(82, "Pb", "Lead", "207.2", 6, 14, 4, PostTransitionMetal),
    e!(83, "Bi", "Bismuth", "208.98", 6, 15, 5, PostTransitionMetal),
    e!(84, "Po", "Polonium", "[209]", 6, 16, 6, PostTransitionMetal),
    e!(85, "At", "Astatine", "[210]", 6, 17, 7, Halogen),
    e!(86, "Rn", "Radon", "[222]", 6, 18, 8, NobleGas),
    e!(87, "Fr", "Francium", "[223]", 7, 1, 1, AlkaliMetal),
    e!(88, "Ra", "Radium", "[226]", 7, 2, 2, AlkalineEarth),
    e!(89, "Ac", "Actinium", "[227]", 7, 3, 2, Actinide),
    e!(90, "Th", "Thorium", "232.04", 7, 3, 2, Actinide),
    e!(91, "Pa", "Protactinium", "231.04", 7, 3, 2, Actinide),
    e!(92, "U", "Uranium", "238.03", 7, 3, 2, Actinide),
    e!(93, "Np", "Neptunium", "[237]", 7, 3, 2, Actinide),
    e!(94, "Pu", "Plutonium", "[244]", 7, 3, 2, Actinide),
    e!(95, "Am", "Americium", "[243]", 7, 3, 2, Actinide),
    e!(96, "Cm", "Curium", "[247]", 7, 3, 2, Actinide),
    e!(97, "Bk", "Berkelium", "[247]", 7, 3, 2, Actinide),
    e!(98, "Cf", "Californium", "[251]", 7, 3, 2, Actinide),
    e!(99, "Es", "Einsteinium", "[252]", 7, 3, 2, Actinide),
    e!(100, "Fm", "Fermium", "[257]", 7, 3, 2, Actinide),
    e!(101, "Md", "Mendelevium", "[258]", 7, 3, 2, Actinide),
    e!(102, "No", "Nobelium", "[259]", 7, 3, 2, Actinide),
    e!(103, "Lr", "Lawrencium", "[266]", 7, 3, 2, Actinide),
    e!(
        104,
        "Rf",
        "Rutherfordium",
        "[267]",
        7,
        4,
        2,
        TransitionMetal
    ),
    e!(105, "Db", "Dubnium", "[268]", 7, 5, 2, TransitionMetal),
    e!(106, "Sg", "Seaborgium", "[269]", 7, 6, 2, TransitionMetal),
    e!(107, "Bh", "Bohrium", "[270]", 7, 7, 2, TransitionMetal),
    e!(108, "Hs", "Hassium", "[277]", 7, 8, 2, TransitionMetal),
    e!(109, "Mt", "Meitnerium", "[278]", 7, 9, 2, TransitionMetal),
    e!(
        110,
        "Ds",
        "Darmstadtium",
        "[281]",
        7,
        10,
        2,
        TransitionMetal
    ),
    e!(111, "Rg", "Roentgenium", "[282]", 7, 11, 1, TransitionMetal),
    e!(112, "Cn", "Copernicium", "[285]", 7, 12, 2, TransitionMetal),
    e!(
        113,
        "Nh",
        "Nihonium",
        "[286]",
        7,
        13,
        3,
        PostTransitionMetal
    ),
    e!(
        114,
        "Fl",
        "Flerovium",
        "[289]",
        7,
        14,
        4,
        PostTransitionMetal
    ),
    e!(
        115,
        "Mc",
        "Moscovium",
        "[290]",
        7,
        15,
        5,
        PostTransitionMetal
    ),
    e!(
        116,
        "Lv",
        "Livermorium",
        "[293]",
        7,
        16,
        6,
        PostTransitionMetal
    ),
    e!(117, "Ts", "Tennessine", "[294]", 7, 17, 7, Halogen),
    e!(118, "Og", "Oganesson", "[294]", 7, 18, 8, NobleGas),
];

pub fn by_atomic_number(atomic_number: u8) -> Option<&'static ElementSpec> {
    SUPPORTED
        .iter()
        .find(|element| element.atomic_number == atomic_number)
}

/// Returns the row and column in the nine-row long-form presentation.
pub const fn display_position(element: ElementSpec) -> (u8, u8) {
    match element.atomic_number {
        57..=71 => (8, element.atomic_number - 53),
        89..=103 => (9, element.atomic_number - 85),
        _ => (element.period, element.group),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn complete_catalogue_has_unique_atomic_numbers_and_display_positions() {
        assert_eq!(SUPPORTED.len(), 118);
        let numbers = SUPPORTED
            .iter()
            .map(|element| element.atomic_number)
            .collect::<BTreeSet<_>>();
        let positions = SUPPORTED
            .iter()
            .map(|element| display_position(*element))
            .collect::<BTreeSet<_>>();

        assert_eq!(numbers.len(), 118);
        assert_eq!(positions.len(), 118);
        assert_eq!(by_atomic_number(37).map(|item| item.symbol), Some("Rb"));
    }
}
