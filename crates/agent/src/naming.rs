//! Systematic names for solver products.
//!
//! Deterministic classroom nomenclature: ionic salts as cation + anion
//! (Roman numerals for variable-charge metals, -ide roots for monatomic
//! anions, the common polyatomic anion table), and prefix names for binary
//! molecular compounds. Falls back to the formula when a compound is
//! outside these rules — a wrong name is worse than none.

use std::collections::BTreeMap;

use chem_domain::{element_name, has_variable_cation_charge};

/// -ide roots for monatomic anions.
fn anion_root(symbol: &str) -> Option<&'static str> {
    Some(match symbol {
        "H" => "hydr",
        "B" => "bor",
        "C" => "carb",
        "N" => "nitr",
        "O" => "ox",
        "F" => "fluor",
        "Si" => "silic",
        "P" => "phosph",
        "S" => "sulf",
        "Cl" => "chlor",
        "As" => "arsen",
        "Se" => "selen",
        "Br" => "brom",
        "Te" => "tellur",
        "I" => "iod",
        _ => return None,
    })
}

/// Common polyatomic anion names, keyed by sorted element composition.
fn polyatomic_anion(composition: &BTreeMap<&str, u64>) -> Option<&'static str> {
    let key = composition
        .iter()
        .map(|(symbol, count)| format!("{symbol}{count}"))
        .collect::<Vec<_>>()
        .join(" ");
    Some(match key.as_str() {
        "H1 O1" => "hydroxide",
        "C1 O3" => "carbonate",
        "C1 H1 O3" => "hydrogen carbonate",
        "N1 O3" => "nitrate",
        "N1 O2" => "nitrite",
        "O4 S1" => "sulfate",
        "O3 S1" => "sulfite",
        "O4 P1" => "phosphate",
        "Cl1 O1" => "hypochlorite",
        "Cl1 O3" => "chlorate",
        "Cl1 O4" => "perchlorate",
        _ => return None,
    })
}

const ROMAN: [&str; 7] = ["I", "II", "III", "IV", "V", "VI", "VII"];

/// Name for an ionic salt of one metal cation and one anion.
#[must_use]
pub(crate) fn salt_name(
    cation: &str,
    cation_charge: u64,
    counts: &BTreeMap<String, u64>,
) -> Option<String> {
    let mut anion = BTreeMap::new();
    for (symbol, count) in counts {
        if symbol == cation {
            continue;
        }
        anion.insert(symbol.as_str(), *count);
    }
    let anion_name = if anion.len() == 1 {
        // Monatomic anions regardless of multiplicity: CaCl2 is a chloride.
        format!("{}ide", anion_root(anion.keys().next()?)?)
    } else {
        polyatomic_anion(&anion)?.to_owned()
    };
    let metal = element_name(cation)?;
    let cation_name = if has_variable_cation_charge(cation) {
        let numeral = ROMAN.get(usize::try_from(cation_charge).ok()?.checked_sub(1)?)?;
        format!("{metal}({numeral})")
    } else {
        metal.to_owned()
    };
    Some(format!("{cation_name} {anion_name}"))
}

const PREFIXES: [&str; 4] = ["mono", "di", "tri", "tetra"];

/// Prefix name for a binary molecular compound ("carbon dioxide",
/// "dinitrogen monoxide"), with hydride and ammonia conventions.
#[must_use]
pub(crate) fn binary_molecular_name(counts: &BTreeMap<String, u64>) -> Option<String> {
    if counts.len() != 2 {
        return None;
    }
    let mut entries = counts
        .iter()
        .map(|(symbol, count)| (symbol.as_str(), *count));
    let (first, second) = (entries.next()?, entries.next()?);
    // Less electronegative element first: hydrogen leads, oxygen trails,
    // alphabetical order is otherwise already conventional.
    if (first, second) == (("H", 3), ("N", 1)) {
        return Some("ammonia".to_owned());
    }
    let (left, right) = if first.0 == "O" || second.0 == "H" {
        (second, first)
    } else {
        (first, second)
    };
    if left.0 == "H" && right.1 == 1 {
        // Binary hydrides read as plain "hydrogen ...ide".
        return Some(format!("hydrogen {}ide", anion_root(right.0)?));
    }
    let left_name = element_name(left.0)?;
    let root = anion_root(right.0)?;
    let left_prefix = if left.1 == 1 {
        ""
    } else {
        PREFIXES.get(usize::try_from(left.1).ok()?.checked_sub(1)?)?
    };
    let mut right_prefix =
        (*PREFIXES.get(usize::try_from(right.1).ok()?.checked_sub(1)?)?).to_owned();
    // Elide the prefix vowel before a vowel root: monoxide, tetroxide.
    if root.starts_with(['a', 'e', 'i', 'o', 'u']) && right_prefix.ends_with(['a', 'o']) {
        right_prefix.pop();
    }
    Some(format!("{left_prefix}{left_name} {right_prefix}{root}ide"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counts(pairs: &[(&str, u64)]) -> BTreeMap<String, u64> {
        pairs
            .iter()
            .map(|(symbol, count)| ((*symbol).to_owned(), *count))
            .collect()
    }

    #[test]
    fn salts_name_systematically() {
        assert_eq!(
            salt_name("Na", 1, &counts(&[("Na", 1), ("Cl", 1)])).as_deref(),
            Some("sodium chloride")
        );
        assert_eq!(
            salt_name("Ca", 2, &counts(&[("Ca", 1), ("O", 1)])).as_deref(),
            Some("calcium oxide")
        );
        assert_eq!(
            salt_name("Zn", 2, &counts(&[("Zn", 1), ("Cl", 2)])).as_deref(),
            Some("zinc chloride")
        );
        assert_eq!(
            salt_name("Fe", 3, &counts(&[("Fe", 1), ("Cl", 3)])).as_deref(),
            Some("iron(III) chloride")
        );
        assert_eq!(
            salt_name("Na", 1, &counts(&[("Na", 2), ("S", 1), ("O", 4)])).as_deref(),
            Some("sodium sulfate")
        );
        assert_eq!(
            salt_name("Na", 1, &counts(&[("Na", 1), ("N", 1), ("O", 3)])).as_deref(),
            Some("sodium nitrate")
        );
        assert_eq!(
            salt_name("Ca", 2, &counts(&[("Ca", 1), ("C", 1), ("O", 3)])).as_deref(),
            Some("calcium carbonate")
        );
        assert_eq!(
            salt_name("Mg", 2, &counts(&[("Mg", 3), ("N", 2)])).as_deref(),
            Some("magnesium nitride")
        );
    }

    #[test]
    fn molecular_binaries_name_with_prefixes() {
        assert_eq!(
            binary_molecular_name(&counts(&[("H", 1), ("Cl", 1)])).as_deref(),
            Some("hydrogen chloride")
        );
        assert_eq!(
            binary_molecular_name(&counts(&[("H", 2), ("S", 1)])).as_deref(),
            Some("hydrogen sulfide")
        );
        assert_eq!(
            binary_molecular_name(&counts(&[("N", 1), ("H", 3)])).as_deref(),
            Some("ammonia")
        );
        assert_eq!(
            binary_molecular_name(&counts(&[("C", 1), ("O", 2)])).as_deref(),
            Some("carbon dioxide")
        );
    }
}
