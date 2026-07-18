//! Deterministic names derived from validated product structure.

use std::collections::{BTreeMap, BTreeSet};

use chem_domain::{FormulaComposition, ReactionDeclaration, ReactionTerm};
use chem_kernel::{SimulationFrame, SimulationFrames};

use crate::settings::ChemicalLabels;

/// Chooses the configured user-facing label without changing the stable
/// species identity. Names are used only when the chemistry pipeline supplied
/// one; otherwise name mode falls back to the exact formula.
pub fn display_species(labels: ChemicalLabels, name: Option<&str>, formula: &str) -> String {
    match labels {
        ChemicalLabels::Formulae => display_formula(formula),
        ChemicalLabels::Names => name
            .filter(|name| !name.trim().is_empty())
            .map_or_else(|| display_formula(formula), str::to_owned),
    }
}

/// Formats a checked reaction declaration using either exact formulae or its
/// checked display names. Coefficients come from the declaration and are never
/// inferred from display text.
pub fn display_declaration(declaration: &ReactionDeclaration, labels: ChemicalLabels) -> String {
    let side = |terms: &[ReactionTerm]| {
        terms
            .iter()
            .map(|term| {
                let species =
                    display_species(labels, Some(term.display_name()), term.formula_text());
                if term.coefficient() == 1 {
                    species
                } else {
                    format!("{} {species}", term.coefficient())
                }
            })
            .collect::<Vec<_>>()
            .join(" + ")
    };
    format!(
        "{} → {}",
        side(declaration.reactants()),
        side(declaration.products())
    )
}

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

    let (formula, suffix) = token
        .rsplit_once('[')
        .map_or((token, ""), |(formula, suffix)| {
            let suffix = suffix.strip_suffix(']').unwrap_or_default();
            if matches!(suffix, "molecular" | "ion" | "ionic" | "metallic") {
                (formula, &token[formula.len()..])
            } else {
                (token, "")
            }
        });
    let mut displayed = display_formula(formula);
    displayed.push_str(suffix);
    displayed
}

/// Renders one valid formula using conventional chemical typography.
///
/// Internal formulae stay canonical ASCII. Invalid text is returned unchanged
/// so versions, coefficients, diagnostics, and arbitrary prose are never
/// mistaken for chemistry.
pub fn display_formula(formula: &str) -> String {
    let canonical = formula
        .chars()
        .map(ascii_formula_character)
        .collect::<String>();
    if FormulaComposition::parse(&canonical).is_err() {
        return formula.to_owned();
    }

    let mut formatted = String::with_capacity(canonical.len());
    let mut adduct_multiplier = false;
    for character in canonical.chars() {
        match character {
            '.' => {
                formatted.push('·');
                adduct_multiplier = true;
            }
            digit if digit.is_ascii_digit() => {
                formatted.push(if adduct_multiplier {
                    digit
                } else {
                    subscript_digit(digit)
                });
            }
            other => {
                adduct_multiplier = false;
                formatted.push(other);
            }
        }
    }
    formatted
}

const fn ascii_formula_character(character: char) -> char {
    match character {
        '·' => '.',
        '₀' => '0',
        '₁' => '1',
        '₂' => '2',
        '₃' => '3',
        '₄' => '4',
        '₅' => '5',
        '₆' => '6',
        '₇' => '7',
        '₈' => '8',
        '₉' => '9',
        other => other,
    }
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
        .or_else(|| organic_graph_name(frame, product_atoms))
        .or_else(|| agent::compound_name(&counts, None))
        .unwrap_or_else(|| formula_text(&counts))
}

/// Names recognised organic molecules from the product's exact bond graph;
/// composition-based naming cannot tell isomers apart, the graph can.
fn organic_graph_name(
    frame: &SimulationFrame,
    product_atoms: &BTreeSet<chem_domain::AtomId>,
) -> Option<String> {
    let ordered: Vec<&chem_domain::AtomId> = product_atoms.iter().collect();
    let index_of = |target: &chem_domain::AtomId| ordered.iter().position(|atom| *atom == target);
    let symbols = ordered
        .iter()
        .map(|atom| Some(frame.atoms().get(*atom)?.element.as_str()))
        .collect::<Option<Vec<_>>>()?;
    let bonds = frame
        .covalent_edges()
        .values()
        .filter(|edge| product_atoms.contains(&edge.left) || product_atoms.contains(&edge.right))
        .map(|edge| {
            let order = match edge.order {
                chem_domain::BondOrder::Single => 1,
                chem_domain::BondOrder::Double => 2,
                chem_domain::BondOrder::Triple => 3,
            };
            Some((index_of(&edge.left)?, index_of(&edge.right)?, order))
        })
        .collect::<Option<Vec<_>>>()?;
    agent::molecular_graph_name(&symbols, &bonds)
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
    use crate::settings::ChemicalLabels;

    use super::{display_equation, display_formula, display_species};

    #[test]
    fn display_equations_preserve_coefficients_and_format_formulae() {
        assert_eq!(display_equation("3 H2 + N2 -> 2 NH3"), "3 H₂ + N₂ → 2 NH₃");
        assert_eq!(display_equation("I2 + 7 F2 -> 2 IF7"), "I₂ + 7 F₂ → 2 IF₇");
        assert_eq!(
            display_equation("2 Fe + 3 O2 → Fe2O3"),
            "2 Fe + 3 O₂ → Fe₂O₃"
        );
        assert_eq!(
            display_equation("CuSO4.5H2O[ionic] -> CuSO₄·5H₂O[ionic]"),
            "CuSO₄·5H₂O[ionic] → CuSO₄·5H₂O[ionic]"
        );
    }

    #[test]
    fn valid_formulae_use_chemical_typography_and_invalid_text_is_unchanged() {
        assert_eq!(display_formula("CH4"), "CH₄");
        assert_eq!(display_formula("C10H22"), "C₁₀H₂₂");
        assert_eq!(display_formula("Ca(OH)2"), "Ca(OH)₂");
        assert_eq!(display_formula("CuSO4.12H2O"), "CuSO₄·12H₂O");
        assert_eq!(display_formula("CH₄"), "CH₄");
        assert_eq!(display_formula("version1.2"), "version1.2");
        assert_eq!(display_formula("2"), "2");
    }

    #[test]
    fn chemical_labels_use_names_with_an_exact_formula_fallback() {
        assert_eq!(
            display_species(ChemicalLabels::Names, Some("water"), "H2O"),
            "water"
        );
        assert_eq!(
            display_species(ChemicalLabels::Formulae, Some("water"), "H2O"),
            "H₂O"
        );
        assert_eq!(display_species(ChemicalLabels::Names, None, "IF7"), "IF₇");
    }
}
