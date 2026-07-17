//! Systematic names for solver products, and the reverse: compositions
//! from classroom names typed by the user.
//!
//! Deterministic classroom nomenclature: ionic salts as cation + anion
//! (Roman numerals for variable-charge metals, -ide roots for monatomic
//! anions, the common polyatomic anion table), and prefix names for binary
//! molecular compounds. Falls back to the formula when a compound is
//! outside these rules — a wrong name is worse than none.

use std::collections::BTreeMap;

use chem_domain::{
    FormulaComposition, anion_valence_charge, common_cation_charge, element_name,
    has_variable_cation_charge,
};

/// (symbol, -ide root) for monatomic anions.
const ANION_ROOTS: [(&str, &str); 15] = [
    ("H", "hydr"),
    ("B", "bor"),
    ("C", "carb"),
    ("N", "nitr"),
    ("O", "ox"),
    ("F", "fluor"),
    ("Si", "silic"),
    ("P", "phosph"),
    ("S", "sulf"),
    ("Cl", "chlor"),
    ("As", "arsen"),
    ("Se", "selen"),
    ("Br", "brom"),
    ("Te", "tellur"),
    ("I", "iod"),
];

fn anion_root(symbol: &str) -> Option<&'static str> {
    ANION_ROOTS
        .iter()
        .find(|(candidate, _)| *candidate == symbol)
        .map(|(_, root)| *root)
}

/// Element counts of one ion unit.
type IonUnit = &'static [(&'static str, u64)];

/// (name, one anion unit, charge) for the common polyatomic anions.
const POLYATOMIC_ANIONS: [(&str, IonUnit, u64); 12] = [
    ("hydroxide", &[("H", 1), ("O", 1)], 1),
    ("cyanate", &[("C", 1), ("N", 1), ("O", 1)], 1),
    ("carbonate", &[("C", 1), ("O", 3)], 2),
    ("hydrogen carbonate", &[("C", 1), ("H", 1), ("O", 3)], 1),
    ("nitrate", &[("N", 1), ("O", 3)], 1),
    ("nitrite", &[("N", 1), ("O", 2)], 1),
    ("sulfate", &[("O", 4), ("S", 1)], 2),
    ("sulfite", &[("O", 3), ("S", 1)], 2),
    ("phosphate", &[("O", 4), ("P", 1)], 3),
    ("hypochlorite", &[("Cl", 1), ("O", 1)], 1),
    ("chlorate", &[("Cl", 1), ("O", 3)], 1),
    ("perchlorate", &[("Cl", 1), ("O", 4)], 1),
];

/// Common polyatomic anion names, matched by element composition.
fn polyatomic_anion(composition: &BTreeMap<&str, u64>) -> Option<&'static str> {
    POLYATOMIC_ANIONS
        .iter()
        .find(|(_, unit, _)| {
            unit.len() == composition.len()
                && unit
                    .iter()
                    .all(|(symbol, count)| composition.get(symbol) == Some(count))
        })
        .map(|(name, _, _)| *name)
}

const ROMAN: [&str; 7] = ["I", "II", "III", "IV", "V", "VI", "VII"];

/// The cation's name, with a Roman numeral for variable-charge metals.
fn cation_name(cation: &str, cation_charge: u64) -> Option<String> {
    let metal = element_name(cation)?;
    if has_variable_cation_charge(cation) {
        let numeral = ROMAN.get(usize::try_from(cation_charge).ok()?.checked_sub(1)?)?;
        Some(format!("{metal}({numeral})"))
    } else {
        Some(metal.to_owned())
    }
}

/// Name of one anion unit. With a known charge, dioxygen units resolve to
/// peroxide or superoxide; counts-only callers read O as plain oxide.
fn anion_unit_name(unit: &BTreeMap<&str, u64>, charge: Option<u64>) -> Option<String> {
    if unit.len() == 1 {
        let (symbol, count) = unit.iter().next()?;
        if *symbol == "O" && *count == 2 {
            return Some(
                match charge {
                    Some(1) => "superoxide",
                    Some(2) => "peroxide",
                    _ => "oxide",
                }
                .to_owned(),
            );
        }
        // Monatomic anions regardless of multiplicity: CaCl2 is a chloride.
        return Some(format!("{}ide", anion_root(symbol)?));
    }
    // Reduce repeated units so Mg(NO3)2's N2O6 still reads as nitrate.
    let shared = unit
        .values()
        .fold(0, |acc, count| crate::solve::gcd(acc, *count));
    let reduced = unit
        .iter()
        .map(|(symbol, count)| (*symbol, count / shared))
        .collect::<BTreeMap<_, _>>();
    polyatomic_anion(&reduced).map(str::to_owned)
}

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
    let anion_name = anion_unit_name(&anion, None)?;
    Some(format!(
        "{} {anion_name}",
        cation_name(cation, cation_charge)?
    ))
}

/// Name for one exact cation-anion pairing where the anion unit and its
/// charge are known (frames and structures know them; peroxide and
/// superoxide are distinguishable only by charge).
#[must_use]
pub fn ion_pair_name(
    cation: &str,
    cation_charge: u64,
    anion_unit: &BTreeMap<String, u64>,
    anion_charge: u64,
) -> Option<String> {
    let unit = anion_unit
        .iter()
        .map(|(symbol, count)| (symbol.as_str(), *count))
        .collect::<BTreeMap<_, _>>();
    let anion_name = anion_unit_name(&unit, Some(anion_charge))?;
    Some(format!(
        "{} {anion_name}",
        cation_name(cation, cation_charge)?
    ))
}

/// English name for a compound given by element counts plus an optional
/// cation: trivial names first, then elements, salts, and molecular
/// binaries. None outside the rules — a wrong name is worse than none.
#[must_use]
pub fn compound_name(
    counts: &BTreeMap<String, u64>,
    cation: Option<(&str, u64)>,
) -> Option<String> {
    if let Some(name) = trivial_name(counts, true) {
        return Some(name.to_owned());
    }
    if counts.len() == 1 {
        return element_name(counts.keys().next()?).map(str::to_owned);
    }
    let systematic = match cation {
        Some((symbol, charge)) => salt_name(symbol, charge, counts),
        None => binary_molecular_name(counts),
    };
    systematic.or_else(|| trivial_name(counts, false).map(str::to_owned))
}

const PREFIXES: [&str; 10] = [
    "mono", "di", "tri", "tetra", "penta", "hexa", "hepta", "octa", "nona", "deca",
];

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
    if (first, second) == (("H", 3), ("N", 1)) {
        return Some("ammonia".to_owned());
    }
    // The less electronegative element leads; the other takes the -ide.
    let (left, right) =
        if chem_domain::electronegativity(first.0)? <= chem_domain::electronegativity(second.0)? {
            (first, second)
        } else {
            (second, first)
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

/// Well-known names that no systematic rule covers. The `bool` marks
/// preferred display names; `false` entries are accepted as typed input
/// and used as a last-resort display fallback, but lose to a systematic
/// name (`HCl` gas is "hydrogen chloride"; the acid name belongs to the
/// solution).
const TRIVIAL_NAMES: [(&str, IonUnit, bool); 12] = [
    ("water", &[("H", 2), ("O", 1)], true),
    ("ammonia", &[("N", 1), ("H", 3)], true),
    ("methane", &[("C", 1), ("H", 4)], true),
    ("benzene", &[("C", 6), ("H", 6)], true),
    ("urea", &[("C", 1), ("H", 4), ("N", 2), ("O", 1)], true),
    ("hydrochloric acid", &[("H", 1), ("Cl", 1)], false),
    ("sulfuric acid", &[("H", 2), ("S", 1), ("O", 4)], false),
    ("sulfurous acid", &[("H", 2), ("S", 1), ("O", 3)], false),
    ("nitric acid", &[("H", 1), ("N", 1), ("O", 3)], false),
    ("nitrous acid", &[("H", 1), ("N", 1), ("O", 2)], false),
    ("carbonic acid", &[("H", 2), ("C", 1), ("O", 3)], false),
    ("phosphoric acid", &[("H", 3), ("P", 1), ("O", 4)], false),
];

/// Trivial-name lookup by exact composition, optionally restricted to
/// preferred display names.
fn trivial_name(counts: &BTreeMap<String, u64>, display_only: bool) -> Option<&'static str> {
    TRIVIAL_NAMES
        .iter()
        .find(|(_, unit, display)| {
            (!display_only || *display) && scaled(unit, 1, BTreeMap::new()) == *counts
        })
        .map(|(name, _, _)| *name)
}

/// Display name for an explicit molecular graph (hydrogens as atoms), as
/// animation frames carry them: recognised named molecules by exact graph
/// match, None otherwise — a wrong name is worse than none.
#[must_use]
pub fn molecular_graph_name(symbols: &[&str], bonds: &[(usize, usize, u8)]) -> Option<String> {
    let editable = crate::organic::editable_from_explicit(symbols, bonds)?;
    crate::organic::recognised_name(&editable).map(str::to_owned)
}

/// English name for a whole validated structure: element names for
/// elements, trivial names, recognised organic molecules by graph, salt
/// nomenclature (using the structure's own cation charge), and prefix
/// names for molecular binaries. None when the compound is outside these
/// rules — a wrong name is worse than none.
#[must_use]
pub fn structure_name(structure: &chem_domain::StructureDefinition) -> Option<String> {
    if let Some(editable) = crate::organic::Editable::from_structure(structure)
        && let Some(name) = crate::organic::recognised_name(&editable)
    {
        return Some(name.to_owned());
    }
    let counts = structure
        .formula()
        .elements()
        .iter()
        .map(|(symbol, count)| (symbol.as_str().to_owned(), *count))
        .collect::<BTreeMap<String, u64>>();
    match structure.representation() {
        chem_domain::RepresentationKind::Ionic => {
            let salt = crate::solve::ionic_salt(structure)?;
            ion_pair_name(
                &salt.cation,
                salt.cation_charge,
                &salt.anion,
                salt.anion_charge,
            )
        }
        chem_domain::RepresentationKind::Molecular | chem_domain::RepresentationKind::Metallic => {
            compound_name(&counts, None)
        }
        chem_domain::RepresentationKind::Ion => None,
    }
}

/// Element counts for a user-typed compound: a formula (`CuSO4`,
/// `Mg(NO3)2`), an element or compound name ("oxygen", "copper(II)
/// sulfate", "carbon dioxide", "water"), in classroom nomenclature.
#[must_use]
pub fn composition_from_name(input: &str) -> Option<BTreeMap<String, u64>> {
    let trimmed = input.trim();
    // Formulas are case-sensitive; try the input verbatim first. The parser
    // only checks symbol shape, so require every symbol to be a real element
    // ("HCL" parses as H-C-L; the junk "L" must fall through to the
    // case-insensitive reading instead of poisoning the result).
    if let Ok(formula) = FormulaComposition::parse(trimmed) {
        let counts = formula
            .elements()
            .iter()
            .map(|(symbol, count)| (symbol.as_str().to_owned(), *count))
            .collect::<BTreeMap<String, u64>>();
        if counts.keys().all(|symbol| element_name(symbol).is_some()) {
            return Some(counts);
        }
    }
    // British spellings and spaced numerals normalize away.
    let name = trimmed
        .to_lowercase()
        .replace("sulph", "sulf")
        .replace("aluminum", "aluminium")
        .replace(" (", "(");
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if let Some((_, unit, _)) = TRIVIAL_NAMES.iter().find(|(known, _, _)| *known == name) {
        return Some(scaled(unit, 1, BTreeMap::new()));
    }
    // Named molecules (organics, mostly) resolve through their subset
    // SMILES: names identify isomers a composition cannot.
    if let Some(smiles) = chem_domain::named_molecule_smiles(&name)
        && let Some(structure) = chem_domain::structure_from_smiles(
            chem_domain::StructureId::new("named.composition").ok()?,
            smiles,
        )
    {
        return Some(
            structure
                .formula()
                .elements()
                .iter()
                .map(|(symbol, count)| (symbol.as_str().to_owned(), *count))
                .collect(),
        );
    }
    if let Some(symbol) = element_by_name(&name) {
        // Standard states: diatomic gases and the P4/S8 allotropes.
        let count = match symbol {
            "H" | "N" | "O" | "F" | "Cl" | "Br" | "I" => 2,
            "P" => 4,
            "S" => 8,
            _ => 1,
        };
        return Some([(symbol.to_owned(), count)].into());
    }
    let words = name.split(' ').collect::<Vec<_>>();
    salt_composition(&words)
        .or_else(|| molecular_composition(&words))
        .or_else(|| case_insensitive_formula(trimmed))
}

/// Formula typed without conventional capitalisation (`hcl`, `NAOH`,
/// `h2so4`). Case-folded formulas can be ambiguous (`hno3` reads as H-N-O₃
/// or H-No₃, `cuso4` as Cu-S-O₄ or C-U-S-O₄), so every segmentation is
/// enumerated and ambiguity is resolved by asking the structure generator
/// which candidates are actually buildable chemistry. Anything still
/// ambiguous after that is rejected: a wrong parse is worse than none.
// ponytail: no parentheses support — `mg(no3)2` still needs proper case.
fn case_insensitive_formula(input: &str) -> Option<BTreeMap<String, u64>> {
    let folded = input.to_lowercase();
    if folded.len() > 32 || !folded.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return None;
    }
    // A bare element symbol wins outright: `mg` is magnesium, `si` is
    // silicon, `co` is cobalt (carbon monoxide is reachable as `CO` or
    // "carbon monoxide").
    if folded.bytes().all(|byte| byte.is_ascii_lowercase())
        && let Some(symbol) = chem_domain::ELEMENT_SYMBOLS
            .iter()
            .find(|symbol| symbol.to_lowercase() == folded)
    {
        return Some([((*symbol).to_owned(), 1)].into());
    }

    let mut candidates = Vec::new();
    segment_formula(folded.as_bytes(), &mut BTreeMap::new(), &mut candidates);
    candidates.sort();
    candidates.dedup();
    if let [composition] = candidates.as_slice() {
        return Some(composition.clone());
    }
    // Several readings: keep the ones the structure generator can actually
    // build, then prefer the reading with more distinct elements (`co2` is
    // carbon dioxide, not a cobalt cluster).
    let mut buildable = candidates
        .into_iter()
        .filter(structure_exists)
        .collect::<Vec<_>>();
    buildable.sort_by_key(|composition| std::cmp::Reverse(composition.len()));
    match buildable.as_slice() {
        [composition] => Some(composition.clone()),
        [first, second, ..] if first.len() > second.len() => Some(first.clone()),
        _ => None,
    }
}

/// Recursively segments a case-folded formula into element symbols with
/// optional counts, collecting every complete reading.
fn segment_formula(
    rest: &[u8],
    prefix: &mut BTreeMap<String, u64>,
    out: &mut Vec<BTreeMap<String, u64>>,
) {
    if rest.is_empty() {
        if !prefix.is_empty() {
            out.push(prefix.clone());
        }
        return;
    }
    for length in [2_usize, 1] {
        if rest.len() < length || !rest[..length].iter().all(u8::is_ascii_lowercase) {
            continue;
        }
        let Some(symbol) = chem_domain::ELEMENT_SYMBOLS
            .iter()
            .find(|symbol| symbol.len() == length && symbol.to_lowercase().as_bytes() == &rest[..length])
        else {
            continue;
        };
        let digits = rest[length..]
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .count();
        let count = if digits == 0 {
            1
        } else {
            match std::str::from_utf8(&rest[length..length + digits])
                .ok()
                .and_then(|digits| digits.parse::<u64>().ok())
            {
                Some(count) if count > 0 => count,
                _ => continue,
            }
        };
        *prefix.entry((*symbol).to_owned()).or_insert(0) += count;
        segment_formula(&rest[length + digits..], prefix, out);
        let entry = prefix.get_mut(*symbol).expect("just inserted");
        if *entry > count {
            *entry -= count;
        } else {
            prefix.remove(*symbol);
        }
    }
}

/// Whether the structure generator can build one coherent structure from
/// this composition — the chemistry oracle used to reject nonsense readings.
fn structure_exists(composition: &BTreeMap<String, u64>) -> bool {
    let Ok(counts) = composition
        .iter()
        .map(|(symbol, count)| {
            chem_domain::ElementSymbol::new(symbol.as_str()).map(|symbol| (symbol, *count))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()
    else {
        return false;
    };
    let Ok(inventory) = chem_domain::ElementInventory::new(counts) else {
        return false;
    };
    let Ok(id) = chem_domain::StructureId::new("naming.case-fold-probe") else {
        return false;
    };
    chem_domain::generate_structure(id, &inventory).is_some()
}

fn element_by_name(name: &str) -> Option<&'static str> {
    chem_domain::ELEMENT_NAMES
        .iter()
        .position(|candidate| *candidate == name)
        .map(|index| chem_domain::ELEMENT_SYMBOLS[index])
}

fn scaled(
    unit: &[(&str, u64)],
    factor: u64,
    mut counts: BTreeMap<String, u64>,
) -> BTreeMap<String, u64> {
    for (symbol, count) in unit {
        *counts.entry((*symbol).to_owned()).or_insert(0) += count * factor;
    }
    counts
}

/// "{cation}[(numeral)] {anion}": charge-balanced ionic composition.
fn salt_composition(words: &[&str]) -> Option<BTreeMap<String, u64>> {
    let (first, anion_words) = words.split_first()?;
    let (cation_name, numeral) = match first.split_once('(') {
        Some((name, rest)) => (name, Some(rest.strip_suffix(')')?)),
        None => (*first, None),
    };
    let (cation_unit, natural_charge): (Vec<(&str, u64)>, Option<i16>) =
        if cation_name == "ammonium" {
            (vec![("N", 1), ("H", 4)], Some(1))
        } else {
            let symbol = element_by_name(cation_name)?;
            if symbol == "H" {
                return None; // hydrogen compounds are molecular
            }
            (vec![(symbol, 1)], common_cation_charge(symbol))
        };
    let cation_charge = match numeral {
        Some(numeral) => u64::try_from(
            ROMAN
                .iter()
                .position(|candidate| candidate.eq_ignore_ascii_case(numeral))?
                + 1,
        )
        .ok()?,
        None => u64::try_from(natural_charge?).ok()?,
    };
    let phrase = anion_words.join(" ");
    let phrase = if phrase == "bicarbonate" {
        "hydrogen carbonate".to_owned()
    } else {
        phrase
    };
    let (anion_unit, anion_charge): (Vec<(&str, u64)>, u64) = if let Some((_, unit, charge)) =
        POLYATOMIC_ANIONS
            .iter()
            .find(|(known, _, _)| *known == phrase)
    {
        (unit.to_vec(), *charge)
    } else {
        let root = phrase.strip_suffix("ide")?;
        let (symbol, _) = ANION_ROOTS.iter().find(|(_, known)| *known == root)?;
        (vec![(*symbol, 1)], u64::from(anion_valence_charge(symbol)?))
    };
    let shared = crate::solve::gcd(cation_charge, anion_charge);
    let counts = scaled(&cation_unit, anion_charge / shared, BTreeMap::new());
    Some(scaled(&anion_unit, cation_charge / shared, counts))
}

/// Numeric multiplier prefixes, with their elided forms (mon-, tetr-).
const NAME_PREFIXES: [(&str, u64); 6] = [
    ("mono", 1),
    ("mon", 1),
    ("di", 2),
    ("tri", 3),
    ("tetra", 4),
    ("tetr", 4),
];

/// "{prefix?}{element} {prefix?}{root}ide": binary molecular composition.
fn molecular_composition(words: &[&str]) -> Option<BTreeMap<String, u64>> {
    let [left, right] = words else {
        return None;
    };
    // Prefer the un-prefixed reading so "tin(...)" never parses as tri-n.
    let (left_symbol, left_count) = element_by_name(left).map_or_else(
        || {
            NAME_PREFIXES.iter().find_map(|(prefix, count)| {
                left.strip_prefix(prefix)
                    .and_then(element_by_name)
                    .map(|symbol| (symbol, Some(*count)))
            })
        },
        |symbol| Some((symbol, None)),
    )?;
    let root_symbol = |token: &str| {
        let root = token.strip_suffix("ide")?;
        ANION_ROOTS
            .iter()
            .find(|(_, known)| *known == root)
            .map(|(symbol, _)| *symbol)
    };
    let (right_symbol, right_count) = root_symbol(right).map_or_else(
        || {
            NAME_PREFIXES.iter().find_map(|(prefix, count)| {
                right
                    .strip_prefix(prefix)
                    .and_then(root_symbol)
                    .map(|symbol| (symbol, Some(*count)))
            })
        },
        |symbol| Some((symbol, None)),
    )?;
    // Bare hydrides take their count from the partner's valence:
    // "hydrogen sulfide" is H2S.
    let left_count = match (left_symbol, left_count, right_count) {
        ("H", None, None) => u64::from(anion_valence_charge(right_symbol)?),
        _ => left_count.unwrap_or(1),
    };
    Some(scaled(
        &[
            (left_symbol, left_count),
            (right_symbol, right_count.unwrap_or(1)),
        ],
        1,
        BTreeMap::new(),
    ))
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
    fn structures_name_themselves() {
        let named = |pairs: &[(&str, u64)]| {
            let inventory = chem_domain::ElementInventory::new(
                pairs
                    .iter()
                    .map(|(symbol, count)| {
                        (
                            chem_domain::ElementSymbol::new(*symbol).expect("symbol"),
                            *count,
                        )
                    })
                    .collect::<BTreeMap<_, _>>(),
            )
            .expect("inventory");
            let structure = chem_domain::generate_structure(
                chem_domain::StructureId::new("generated.naming").expect("id"),
                &inventory,
            )?;
            structure_name(&structure)
        };
        assert_eq!(named(&[("H", 2), ("O", 1)]).as_deref(), Some("water"));
        assert_eq!(named(&[("O", 2)]).as_deref(), Some("oxygen"));
        assert_eq!(named(&[("Cu", 1)]).as_deref(), Some("copper"));
        assert_eq!(
            named(&[("Na", 1), ("Cl", 1)]).as_deref(),
            Some("sodium chloride")
        );
        assert_eq!(
            named(&[("Cu", 1), ("S", 1), ("O", 4)]).as_deref(),
            Some("copper(II) sulfate")
        );
        assert_eq!(
            named(&[("C", 1), ("O", 2)]).as_deref(),
            Some("carbon dioxide")
        );
        assert_eq!(named(&[("C", 6), ("H", 6)]).as_deref(), Some("benzene"));
    }

    #[test]
    fn ion_pairs_name_charge_aware_anions() {
        let dioxygen = counts(&[("O", 2)]);
        assert_eq!(
            ion_pair_name("Na", 1, &dioxygen, 2).as_deref(),
            Some("sodium peroxide")
        );
        assert_eq!(
            ion_pair_name("K", 1, &dioxygen, 1).as_deref(),
            Some("potassium superoxide")
        );
        assert_eq!(
            ion_pair_name("Ca", 2, &counts(&[("O", 1)]), 2).as_deref(),
            Some("calcium oxide")
        );
        assert_eq!(
            ion_pair_name("Fe", 2, &counts(&[("O", 4), ("S", 1)]), 2).as_deref(),
            Some("iron(II) sulfate")
        );
    }


    #[test]
    fn case_insensitive_formulas_resolve_common_classroom_input() {
        let counts = |pairs: &[(&str, u64)]| {
            pairs
                .iter()
                .map(|(symbol, count)| ((*symbol).to_owned(), *count))
                .collect::<BTreeMap<String, u64>>()
        };
        // Casing mistakes users actually make.
        for input in ["hcl", "HCL", "hCl"] {
            assert_eq!(
                composition_from_name(input),
                Some(counts(&[("H", 1), ("Cl", 1)])),
                "{input}"
            );
        }
        for input in ["naoh", "NAOH"] {
            assert_eq!(
                composition_from_name(input),
                Some(counts(&[("Na", 1), ("O", 1), ("H", 1)])),
                "{input}"
            );
        }
        assert_eq!(
            composition_from_name("h2so4"),
            Some(counts(&[("H", 2), ("S", 1), ("O", 4)]))
        );
        // Ambiguous case-folded readings resolve to real chemistry: hno3
        // could read H-No3 (nobelium) and cuso4 could read C-U-S-O4
        // (uranium); the structure oracle keeps the sensible parse.
        assert_eq!(
            composition_from_name("hno3"),
            Some(counts(&[("H", 1), ("N", 1), ("O", 3)]))
        );
        assert_eq!(
            composition_from_name("cuso4"),
            Some(counts(&[("Cu", 1), ("S", 1), ("O", 4)]))
        );
        assert_eq!(
            composition_from_name("co2"),
            Some(counts(&[("C", 1), ("O", 2)]))
        );
        assert_eq!(
            composition_from_name("caco3"),
            Some(counts(&[("Ca", 1), ("C", 1), ("O", 3)]))
        );
        // A bare symbol is the element, whatever the case.
        assert_eq!(composition_from_name("mg"), Some(counts(&[("Mg", 1)])));
        assert_eq!(composition_from_name("si"), Some(counts(&[("Si", 1)])));
        // Junk still fails instead of guessing.
        assert_eq!(composition_from_name("xyz"), None);
        assert_eq!(composition_from_name("acid"), None);
    }

    #[test]
    fn compound_names_prefer_systematic_over_acid_aliases() {
        // HCl gas is hydrogen chloride; the acid name belongs to solution.
        assert_eq!(
            compound_name(&counts(&[("H", 1), ("Cl", 1)]), None).as_deref(),
            Some("hydrogen chloride")
        );
        // Sulfuric acid has no systematic binary name; the alias holds.
        assert_eq!(
            compound_name(&counts(&[("H", 2), ("S", 1), ("O", 4)]), None).as_deref(),
            Some("sulfuric acid")
        );
        assert_eq!(
            compound_name(&counts(&[("H", 2), ("O", 1)]), None).as_deref(),
            Some("water")
        );
        // Large prefixes and electronegativity ordering.
        assert_eq!(
            binary_molecular_name(&counts(&[("I", 1), ("F", 7)])).as_deref(),
            Some("iodine heptafluoride")
        );
        assert_eq!(
            binary_molecular_name(&counts(&[("C", 1), ("S", 2)])).as_deref(),
            Some("carbon disulfide")
        );
        assert_eq!(
            binary_molecular_name(&counts(&[("P", 2), ("O", 5)])).as_deref(),
            Some("diphosphorus pentoxide")
        );
    }

    #[test]
    fn compositions_resolve_from_names_and_formulas() {
        let composition = |input: &str| {
            composition_from_name(input).map(|counts| {
                counts
                    .into_iter()
                    .map(|(symbol, count)| format!("{symbol}{count}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
        };
        // Formulas, verbatim and parenthesized.
        assert_eq!(composition("CuSO4").as_deref(), Some("Cu1 O4 S1"));
        assert_eq!(composition("Mg(NO3)2").as_deref(), Some("Mg1 N2 O6"));
        // Elements in their standard states.
        assert_eq!(composition("oxygen").as_deref(), Some("O2"));
        assert_eq!(composition("copper").as_deref(), Some("Cu1"));
        assert_eq!(composition("sulfur").as_deref(), Some("S8"));
        // Trivial names.
        assert_eq!(composition("water").as_deref(), Some("H2 O1"));
        assert_eq!(composition("Sulfuric Acid").as_deref(), Some("H2 O4 S1"));
        // Salts, with and without numerals, including British spelling.
        assert_eq!(composition("sodium chloride").as_deref(), Some("Cl1 Na1"));
        assert_eq!(
            composition("copper(II) sulphate").as_deref(),
            Some("Cu1 O4 S1")
        );
        assert_eq!(
            composition("iron (III) chloride").as_deref(),
            Some("Cl3 Fe1")
        );
        assert_eq!(
            composition("magnesium nitrate").as_deref(),
            Some("Mg1 N2 O6")
        );
        assert_eq!(
            composition("sodium bicarbonate").as_deref(),
            Some("C1 H1 Na1 O3")
        );
        assert_eq!(
            composition("ammonium chloride").as_deref(),
            Some("Cl1 H4 N1")
        );
        // The Wöhler pair: distinct names, one shared composition.
        assert_eq!(
            composition("ammonium cyanate").as_deref(),
            Some("C1 H4 N2 O1")
        );
        assert_eq!(composition("urea").as_deref(), Some("C1 H4 N2 O1"));
        // Molecular binaries.
        assert_eq!(composition("carbon dioxide").as_deref(), Some("C1 O2"));
        assert_eq!(composition("carbon monoxide").as_deref(), Some("C1 O1"));
        assert_eq!(composition("dinitrogen monoxide").as_deref(), Some("N2 O1"));
        assert_eq!(composition("hydrogen sulfide").as_deref(), Some("H2 S1"));
        // Refusals: gibberish and bare hydrogen-cation salts.
        assert_eq!(composition("unobtainium nitrate"), None);
        assert_eq!(composition(""), None);
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
