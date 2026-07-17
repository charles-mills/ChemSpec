//! Deterministic names derived from validated product structure.

use std::collections::{BTreeMap, BTreeSet};

use chem_kernel::{SimulationFrame, SimulationFrames};

/// Formats an ASCII catalogue equation for display without changing its
/// trusted source representation. Stoichiometric coefficients remain normal
/// digits, formula counts become Unicode subscripts, and ASCII arrows are
/// normalized to the typographic reaction arrow used elsewhere in the app.
pub fn display_equation(equation: &str) -> String {
    equation
        .split_whitespace()
        .map(display_equation_token)
        .collect::<Vec<_>>()
        .join(" ")
        .replace("->", "→")
}

fn display_equation_token(token: &str) -> String {
    if token.chars().all(|character| character.is_ascii_digit()) {
        return token.to_owned();
    }

    let characters = token.chars().collect::<Vec<_>>();
    let mut formatted = String::with_capacity(token.len());
    for (index, character) in characters.iter().copied().enumerate() {
        let previous = index.checked_sub(1).and_then(|index| characters.get(index));
        let formula_count = character.is_ascii_digit()
            && previous.is_some_and(|previous| {
                previous.is_ascii_alphabetic()
                    || previous.is_ascii_digit()
                    || matches!(previous, ')' | ']')
            });
        formatted.push(if formula_count {
            subscript_digit(character)
        } else {
            character
        });
    }
    formatted
}

const fn subscript_digit(digit: char) -> char {
    match digit {
        '0' => '₀',
        '1' => '₁',
        '2' => '₂',
        '3' => '₃',
        '4' => '₄',
        '5' => '₅',
        '6' => '₆',
        '7' => '₇',
        '8' => '₈',
        '9' => '₉',
        _ => digit,
    }
}

pub fn product_names(frames: &SimulationFrames) -> String {
    let Some(frame) = frames.frames().last() else {
        return "reaction products".to_owned();
    };
    let names = frame
        .product_membership()
        .values()
        .map(|atoms| product_name(frame, atoms))
        .collect::<BTreeSet<_>>();
    if names.is_empty() {
        "reaction products".to_owned()
    } else {
        names.into_iter().collect::<Vec<_>>().join(" + ")
    }
}

/// One product's display name, delegated to the shared naming module so
/// animations, previews, and solver claims never disagree. Falls back to a
/// plain formula when the compound is outside the nomenclature rules.
pub(crate) fn product_name(
    frame: &SimulationFrame,
    product_atoms: &BTreeSet<chem_domain::AtomId>,
) -> String {
    let mut counts = BTreeMap::<String, u64>::new();
    for atom in product_atoms {
        if let Some(atom) = frame.atoms().get(atom) {
            *counts.entry(atom.element.as_str().to_owned()).or_default() += 1;
        }
    }
    ionic_pair_name(frame, product_atoms)
        .or_else(|| agent::compound_name(&counts, None))
        .unwrap_or_else(|| formula_text(&counts))
}

/// Names an ionic product from the exact cation and anion unit its
/// association records (charge-aware, so peroxide and superoxide resolve).
fn ionic_pair_name(
    frame: &SimulationFrame,
    product_atoms: &BTreeSet<chem_domain::AtomId>,
) -> Option<String> {
    let association = frame.ionic_associations().values().find(|association| {
        association
            .components
            .values()
            .flatten()
            .any(|atom| product_atoms.contains(atom))
    })?;
    let mut cation: Option<(String, u64)> = None;
    let mut anion: Option<(BTreeMap<String, u64>, u64)> = None;
    for (group, atoms) in &association.components {
        if !atoms.iter().all(|atom| product_atoms.contains(atom)) {
            continue;
        }
        let charge = association
            .component_charges
            .get(group)
            .copied()
            .unwrap_or(0);
        if charge > 0 && atoms.len() == 1 {
            let atom = frame.atoms().get(atoms.iter().next()?)?;
            let found = (
                atom.element.as_str().to_owned(),
                u64::try_from(charge).ok()?,
            );
            cation.get_or_insert(found);
        } else if charge < 0 {
            let unit = atoms.iter().filter_map(|id| frame.atoms().get(id)).fold(
                BTreeMap::<String, u64>::new(),
                |mut unit, atom| {
                    *unit.entry(atom.element.as_str().to_owned()).or_default() += 1;
                    unit
                },
            );
            anion.get_or_insert((unit, charge.unsigned_abs()));
        }
    }
    let (cation, cation_charge) = cation?;
    let (anion_unit, anion_charge) = anion?;
    agent::ion_pair_name(&cation, cation_charge, &anion_unit, anion_charge)
}

/// Plain formula fallback: carbon, hydrogen, then the rest alphabetically.
fn formula_text(counts: &BTreeMap<String, u64>) -> String {
    let mut formula = String::new();
    let mut append = |symbol: &str, count: u64| {
        formula.push_str(symbol);
        if count > 1 {
            formula.push_str(&count.to_string());
        }
    };
    for symbol in ["C", "H"] {
        if let Some(count) = counts.get(symbol) {
            append(symbol, *count);
        }
    }
    for (symbol, count) in counts {
        if symbol != "C" && symbol != "H" {
            append(symbol, *count);
        }
    }
    formula
}

#[cfg(test)]
mod tests {
    use super::display_equation;

    #[test]
    fn display_equations_preserve_coefficients_and_format_formulae() {
        assert_eq!(display_equation("3 H2 + N2 -> 2 NH3"), "3 H₂ + N₂ → 2 NH₃");
        assert_eq!(display_equation("I2 + 7 F2 -> 2 IF7"), "I₂ + 7 F₂ → 2 IF₇");
        assert_eq!(
            display_equation("2 Fe + 3 O2 → Fe2O3"),
            "2 Fe + 3 O₂ → Fe₂O₃"
        );
    }
}
