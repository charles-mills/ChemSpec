//! Trusted structural resolution for reactant-composition previews.
//!
//! This module does not invent Lewis structures from a loose valence
//! heuristic. An atom multiset resolves only when the host-pinned catalogue
//! already contains one unambiguous validated structural graph for it. Those
//! graphs are the structures used by the currently installed bonding and
//! reaction rules.

use std::collections::BTreeMap;

use chem_catalogue::TrustedCatalogue;
use chem_domain::StructureDefinition;

use crate::{chemistry, elements};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositionPreview {
    pub structure_id: String,
    pub formula: String,
    pub atoms: Vec<PreviewAtom>,
    covalent_bonds: Vec<PreviewCovalentBond>,
    ionic_links: Vec<PreviewIonicLink>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewAtom {
    pub label: String,
    pub atomic_number: u8,
    pub formal_charge: i16,
    pub non_bonding_electrons: u8,
    pub unpaired_electrons: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreviewCovalentBond {
    pub start: usize,
    pub end: usize,
    pub order: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreviewIonicLink {
    pub start: usize,
    pub end: usize,
}

impl CompositionPreview {
    pub fn covalent_bonds(&self) -> &[PreviewCovalentBond] {
        &self.covalent_bonds
    }

    pub fn ionic_links(&self) -> &[PreviewIonicLink] {
        &self.ionic_links
    }
}

pub fn recognize(atomic_numbers: impl IntoIterator<Item = u8>) -> Option<CompositionPreview> {
    let catalogue = chemistry::trusted_catalogue().ok()?;
    resolve_with_catalogue(catalogue, atomic_numbers)
}

fn resolve_with_catalogue(
    catalogue: &TrustedCatalogue,
    atomic_numbers: impl IntoIterator<Item = u8>,
) -> Option<CompositionPreview> {
    let atomic_numbers = atomic_numbers.into_iter().collect::<Vec<_>>();
    let atomic_numbers = chemistry::standardize_elemental_draft(&atomic_numbers);
    let selected = atomic_numbers.into_iter().try_fold(
        BTreeMap::<String, u64>::new(),
        |mut counts, number| {
            let element = elements::by_atomic_number(number)?;
            *counts.entry(element.symbol.to_owned()).or_default() += 1;
            Some(counts)
        },
    )?;
    if selected.is_empty() {
        return None;
    }

    let mut matches = Vec::<(&StructureDefinition, &str)>::new();
    for record in &catalogue.document().structures {
        add_matching_structure(
            catalogue,
            record.id(),
            record.formula(),
            &selected,
            &mut matches,
        );
    }
    for application in &catalogue.document().structure_applications {
        add_matching_structure(
            catalogue,
            &application.id,
            &application.formula,
            &selected,
            &mut matches,
        );
    }

    let mut unique = Vec::<(&StructureDefinition, &str)>::new();
    for candidate in matches {
        if unique.iter().any(|(existing, _)| {
            existing.representation() == candidate.0.representation()
                && existing.graph() == candidate.0.graph()
        }) {
            continue;
        }
        unique.push(candidate);
    }
    let [(definition, formula)] = unique.as_slice() else {
        return None;
    };
    preview_from_definition(definition, formula)
}

fn add_matching_structure<'a>(
    catalogue: &'a TrustedCatalogue,
    id: &chem_domain::StructureId,
    formula: &'a str,
    selected: &BTreeMap<String, u64>,
    matches: &mut Vec<(&'a StructureDefinition, &'a str)>,
) {
    let Some(definition) = catalogue.structure(id) else {
        return;
    };
    let inventory = definition
        .formula()
        .elements()
        .iter()
        .map(|(symbol, count)| (symbol.as_str().to_owned(), *count))
        .collect::<BTreeMap<_, _>>();
    if &inventory == selected {
        matches.push((definition, formula));
    }
}

fn preview_from_definition(
    definition: &StructureDefinition,
    formula: &str,
) -> Option<CompositionPreview> {
    let graph = definition.graph();
    let mut atom_indices = BTreeMap::new();
    let atoms = graph
        .atoms()
        .values()
        .enumerate()
        .map(|(index, atom)| {
            atom_indices.insert(atom.id().as_str().to_owned(), index);
            let element = elements::SUPPORTED
                .iter()
                .find(|candidate| candidate.symbol == atom.element().as_str())?;
            let electrons = atom.electrons();
            Some(PreviewAtom {
                label: atom.id().as_str().to_owned(),
                atomic_number: element.atomic_number,
                formal_charge: electrons.formal_charge(),
                non_bonding_electrons: electrons.non_bonding_electrons(),
                unpaired_electrons: electrons.unpaired_electrons(),
            })
        })
        .collect::<Option<Vec<_>>>()?;
    let covalent_bonds = graph
        .covalent_bonds()
        .values()
        .map(|bond| {
            Some(PreviewCovalentBond {
                start: atom_indices[bond.left().as_str()],
                end: atom_indices[bond.right().as_str()],
                order: bond.order().order(),
            })
        })
        .collect::<Option<Vec<_>>>()?;

    let mut ionic_links = Vec::new();
    for association in graph.ionic_associations().values() {
        let mut positive = Vec::new();
        let mut negative = Vec::new();
        for group_id in association.components() {
            let group = graph.groups().get(group_id)?;
            let charge = group
                .atoms()
                .iter()
                .map(|id| i64::from(graph.atoms()[id].electrons().formal_charge()))
                .sum::<i64>();
            let anchor = group
                .atoms()
                .iter()
                .filter_map(|id| {
                    let atom = &graph.atoms()[id];
                    let atom_charge = i64::from(atom.electrons().formal_charge());
                    (atom_charge.signum() == charge.signum())
                        .then_some((atom_indices[id.as_str()], atom_charge.unsigned_abs()))
                })
                .max_by_key(|(_, magnitude)| *magnitude)
                .map(|(index, _)| index)?;
            if charge > 0 {
                positive.push(anchor);
            } else if charge < 0 {
                negative.push(anchor);
            }
        }
        ionic_links.extend(charge_topology(&positive, &negative));
    }

    Some(CompositionPreview {
        structure_id: definition.id().as_str().to_owned(),
        formula: display_formula(formula),
        atoms,
        covalent_bonds,
        ionic_links,
    })
}

fn display_formula(formula: &str) -> String {
    formula
        .chars()
        .map(|character| match character {
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
            _ => character,
        })
        .collect()
}

fn charge_topology(positive: &[usize], negative: &[usize]) -> Vec<PreviewIonicLink> {
    let shared = positive.len().min(negative.len());
    if shared == 0 {
        return Vec::new();
    }
    let mut links = Vec::with_capacity(positive.len() + negative.len() - 1);
    for index in 0..shared {
        links.push(PreviewIonicLink {
            start: positive[index],
            end: negative[index],
        });
        if index + 1 < shared {
            links.push(PreviewIonicLink {
                start: negative[index],
                end: positive[index + 1],
            });
        }
    }
    for (offset, start) in positive.iter().copied().skip(shared).enumerate() {
        links.push(PreviewIonicLink {
            start,
            end: negative[offset % negative.len()],
        });
    }
    for (offset, start) in negative.iter().copied().skip(shared).enumerate() {
        links.push(PreviewIonicLink {
            start,
            end: positive[offset % positive.len()],
        });
    }
    links
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trusted_rules_resolve_complete_structures_order_independently() {
        let lithium_hydroxide = recognize([1, 3, 8]).expect("LiOH structure");
        assert_eq!(lithium_hydroxide.formula, "LiOH");
        assert_eq!(lithium_hydroxide.atoms.len(), 3);
        assert_eq!(lithium_hydroxide.covalent_bonds.len(), 1);
        assert_eq!(lithium_hydroxide.ionic_links.len(), 1);

        let magnesium_fluoride = recognize([9, 12, 9]).expect("MgF2 structure");
        assert_eq!(magnesium_fluoride.formula, "MgF₂");
        assert_eq!(magnesium_fluoride.atoms.len(), 3);
        assert_eq!(magnesium_fluoride.ionic_links.len(), 2);

        let carbon_dioxide = recognize([8, 6, 8]).expect("CO2 structure");
        assert_eq!(carbon_dioxide.formula, "CO₂");
        assert_eq!(
            carbon_dioxide
                .covalent_bonds
                .iter()
                .map(|bond| bond.order)
                .collect::<Vec<_>>(),
            [2, 2]
        );

        let oxygen = recognize([8, 8]).expect("O2 structure");
        assert_eq!(oxygen.covalent_bonds[0].order, 2);
    }

    #[test]
    fn unresolved_multisets_are_not_given_invented_structures() {
        assert!(recognize([6, 6]).is_none());
        assert!(recognize([1, 8]).is_none());
        assert!(recognize([3, 6]).is_none());
    }
}
