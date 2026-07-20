//! Post-simulation product presentation compiled from validated final frames.
//!
//! This module performs deterministic layout and display formatting only. It
//! never parses source, selects a reaction, or invents a chemical property.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use chem_domain::AtomId;
use chem_kernel::{SimulationFrame, SimulationFrames};

use crate::settings::ChemicalLabels;

#[derive(Debug, Clone)]
pub struct SummaryData {
    pub products: Vec<ProductModel>,
}

#[derive(Debug, Clone)]
pub struct ProductModel {
    pub name: String,
    pub formula: String,
    pub coefficient: usize,
    pub classification: &'static str,
    pub composition: String,
    pub atom_count: usize,
    pub bond_count: usize,
    pub net_charge: i32,
    pub molar_mass: String,
    atoms: Vec<VisualAtom>,
    bonds: Vec<VisualBond>,
}

#[derive(Debug, Clone)]
struct VisualAtom {
    symbol: String,
}

#[derive(Debug, Clone)]
struct VisualBond {
    left: usize,
    right: usize,
    order: u8,
}

impl SummaryData {
    #[must_use]
    pub fn from_frames(frames: &SimulationFrames) -> Option<Self> {
        let frame = frames.frames().last()?;
        let mut grouped = BTreeMap::<String, ProductModel>::new();
        for atoms in frame.product_membership().values() {
            let model = ProductModel::from_membership(frame, atoms);
            let signature = model.signature();
            grouped
                .entry(signature)
                .and_modify(|existing| existing.coefficient += 1)
                .or_insert(model);
        }
        (!grouped.is_empty()).then(|| Self {
            products: grouped.into_values().collect(),
        })
    }
}

impl ProductModel {
    fn from_membership(frame: &SimulationFrame, membership: &BTreeSet<AtomId>) -> Self {
        let mut counts = BTreeMap::<String, usize>::new();
        let mut atom_indices = BTreeMap::<String, usize>::new();
        let mut atoms = Vec::new();
        let mut net_charge = 0_i32;
        for atom_id in membership {
            let Some(atom) = frame.atoms().get(atom_id) else {
                continue;
            };
            let index = atoms.len();
            atom_indices.insert(atom_id.as_str().to_owned(), index);
            *counts.entry(atom.element.as_str().to_owned()).or_default() += 1;
            net_charge += i32::from(atom.electrons.formal_charge());
            atoms.push(VisualAtom {
                symbol: atom.element.as_str().to_owned(),
            });
        }
        let bonds = frame
            .covalent_edges()
            .values()
            .filter_map(|bond| {
                let left = atom_indices.get(bond.left.as_str()).copied()?;
                let right = atom_indices.get(bond.right.as_str()).copied()?;
                Some(VisualBond {
                    left,
                    right,
                    order: bond.order.order(),
                })
            })
            .collect::<Vec<_>>();
        let ionic = frame.ionic_associations().values().any(|association| {
            association
                .components
                .values()
                .any(|component| component.iter().any(|atom| membership.contains(atom)))
        });
        let metallic = frame
            .metallic_domains()
            .values()
            .any(|domain| domain.sites.iter().any(|atom| membership.contains(atom)));
        let classification = if ionic {
            "Ionic assembly"
        } else if metallic {
            "Metallic structure"
        } else if !bonds.is_empty() {
            "Covalent molecule"
        } else if atoms.len() == 1 {
            "Atomic product"
        } else {
            "Molecular assembly"
        };
        let formula = product_formula(frame, membership, &counts, ionic);
        Self {
            name: crate::nomenclature::product_name(frame, membership),
            formula,
            coefficient: 1,
            classification,
            composition: composition(&counts),
            atom_count: atoms.len(),
            bond_count: bonds.len(),
            net_charge,
            molar_mass: molar_mass(&counts),
            atoms,
            bonds,
        }
    }

    fn signature(&self) -> String {
        let mut bonds = self
            .bonds
            .iter()
            .map(|bond| {
                let mut endpoints = [
                    self.atoms[bond.left].symbol.as_str(),
                    self.atoms[bond.right].symbol.as_str(),
                ];
                endpoints.sort_unstable();
                format!("{}-{}:{}", endpoints[0], endpoints[1], bond.order)
            })
            .collect::<Vec<_>>();
        bonds.sort();
        format!(
            "{}|{}|{}|{}|{}",
            self.formula,
            self.classification,
            self.net_charge,
            self.atom_count,
            bonds.join(",")
        )
    }

    #[must_use]
    pub fn primary_label(&self, labels: ChemicalLabels) -> String {
        let mut label = match labels {
            ChemicalLabels::Formulae => self.formula.clone(),
            ChemicalLabels::Names => title_case(&self.name),
        };
        if self.coefficient > 1 {
            let _ = write!(label, "  ×{}", self.coefficient);
        }
        label
    }

    #[must_use]
    pub fn property_rows(&self) -> Vec<(&'static str, String)> {
        vec![
            ("Formula", self.formula.clone()),
            ("Structure", self.classification.to_owned()),
            ("Composition", self.composition.clone()),
            ("Validated atoms", self.atom_count.to_string()),
            ("Covalent bonds", self.bond_count.to_string()),
            ("Net formal charge", format_charge(self.net_charge)),
            ("Reference molar mass", self.molar_mass.clone()),
        ]
    }
}

fn formula(counts: &BTreeMap<String, usize>) -> String {
    format_ordered_formula(counts, &ordered_elements(counts))
}

fn product_formula(
    frame: &SimulationFrame,
    membership: &BTreeSet<AtomId>,
    counts: &BTreeMap<String, usize>,
    ionic: bool,
) -> String {
    if !ionic {
        return formula(counts);
    }
    let Some(association) = frame.ionic_associations().values().find(|association| {
        association
            .components
            .values()
            .flatten()
            .all(|atom| membership.contains(atom))
    }) else {
        return formula(counts);
    };
    let mut components = association
        .components
        .iter()
        .map(|(group, atoms)| {
            let mut component_counts = BTreeMap::<String, usize>::new();
            for atom in atoms {
                if let Some(atom) = frame.atoms().get(atom) {
                    *component_counts
                        .entry(atom.element.as_str().to_owned())
                        .or_default() += 1;
                }
            }
            (
                association
                    .component_charges
                    .get(group)
                    .copied()
                    .unwrap_or(0),
                component_counts,
            )
        })
        .collect::<Vec<_>>();
    components.sort_by_key(|component| std::cmp::Reverse(component.0));
    components
        .into_iter()
        .map(|(_, component)| conventional_component_formula(&component))
        .collect()
}

fn conventional_component_formula(counts: &BTreeMap<String, usize>) -> String {
    let preferred = if counts.contains_key("C") && counts.contains_key("H") {
        ["H", "C", "N", "O"]
    } else if counts.contains_key("C") {
        ["C", "H", "N", "O"]
    } else if counts.contains_key("N") && counts.contains_key("O") {
        ["N", "H", "C", "O"]
    } else if counts.contains_key("O") && counts.contains_key("H") {
        ["O", "H", "C", "N"]
    } else {
        return formula(counts);
    };
    let mut order = preferred
        .into_iter()
        .filter(|symbol| counts.contains_key(*symbol))
        .collect::<Vec<_>>();
    let remaining = counts
        .keys()
        .map(String::as_str)
        .filter(|symbol| !order.contains(symbol))
        .collect::<Vec<_>>();
    order.extend(remaining);
    format_ordered_formula(counts, &order)
}

fn format_ordered_formula(counts: &BTreeMap<String, usize>, order: &[&str]) -> String {
    order
        .iter()
        .copied()
        .map(|symbol| {
            let count = counts.get(symbol).copied().unwrap_or(1);
            if count == 1 {
                symbol.to_owned()
            } else {
                format!("{symbol}{}", subscript(count))
            }
        })
        .collect()
}

fn composition(counts: &BTreeMap<String, usize>) -> String {
    ordered_elements(counts)
        .into_iter()
        .map(|symbol| format!("{} {symbol}", counts.get(symbol).copied().unwrap_or(0)))
        .collect::<Vec<_>>()
        .join("  ·  ")
}

fn ordered_elements(counts: &BTreeMap<String, usize>) -> Vec<&str> {
    let mut symbols = counts.keys().map(String::as_str).collect::<Vec<_>>();
    if counts.contains_key("C") {
        symbols.sort_by_key(|symbol| match *symbol {
            "C" => (0, ""),
            "H" => (1, ""),
            other => (2, other),
        });
    }
    symbols
}

fn subscript(value: usize) -> String {
    value
        .to_string()
        .chars()
        .map(|digit| match digit {
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
        })
        .collect()
}

fn molar_mass(counts: &BTreeMap<String, usize>) -> String {
    let mut total = 0_u64;
    let mut approximate = false;
    for (symbol, count) in counts {
        let Some(element) = crate::elements::SUPPORTED
            .iter()
            .find(|element| element.symbol == symbol)
        else {
            return "Not available".to_owned();
        };
        approximate |= element.atomic_mass.starts_with('[');
        let Some(mass) = decimal_millionths(element.atomic_mass.trim_matches(['[', ']'])) else {
            return "Not available".to_owned();
        };
        total =
            total.saturating_add(mass.saturating_mul(u64::try_from(*count).unwrap_or(u64::MAX)));
    }
    let rounded_thousandths = total.saturating_add(500) / 1_000;
    let mut value = format!(
        "{}.{:03}",
        rounded_thousandths / 1_000,
        rounded_thousandths % 1_000
    );
    while value.ends_with('0') {
        value.pop();
    }
    if value.ends_with('.') {
        value.pop();
    }
    format!("{}{value} g mol⁻¹", if approximate { "≈ " } else { "" })
}

fn decimal_millionths(value: &str) -> Option<u64> {
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    let whole = whole.parse::<u64>().ok()?;
    if fraction.len() > 6 || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let fraction = format!("{fraction:0<6}").parse::<u64>().ok()?;
    Some(whole.saturating_mul(1_000_000).saturating_add(fraction))
}

fn format_charge(charge: i32) -> String {
    match charge {
        0 => "0 (neutral)".to_owned(),
        1 => "+1".to_owned(),
        -1 => "−1".to_owned(),
        value if value > 0 => format!("+{value}"),
        value => value.to_string().replace('-', "−"),
    }
}

fn title_case(value: &str) -> String {
    let mut characters = value.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(characters).collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formula_uses_hill_order_and_unicode_subscripts() {
        let counts = BTreeMap::from([
            ("O".to_owned(), 1),
            ("H".to_owned(), 4),
            ("C".to_owned(), 2),
        ]);
        assert_eq!(formula(&counts), "C₂H₄O");
    }

    #[test]
    fn molar_mass_is_composed_from_exact_decimal_element_metadata() {
        let water = BTreeMap::from([("H".to_owned(), 2), ("O".to_owned(), 1)]);
        assert_eq!(molar_mass(&water), "18.015 g mol⁻¹");
    }
    #[test]
    fn summary_products_are_compiled_from_the_validated_final_frame() {
        let run = crate::chemistry::run(crate::chemistry::ReactionRequest::DEFAULT)
            .expect("default .chems request validates");
        let summary = SummaryData::from_frames(run.frames()).expect("products are assigned");
        let formulae = summary
            .products
            .iter()
            .map(|product| product.formula.as_str())
            .collect::<BTreeSet<_>>();

        assert!(formulae.contains("H₂"));
        assert!(formulae.contains("LiOH"));
        assert!(
            summary
                .products
                .iter()
                .all(|product| product.atom_count > 0)
        );
    }
}
