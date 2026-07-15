//! Curated composition previews for the Stage 2 reaction workspace.
//!
//! These patterns improve composition feedback, but they are not validation
//! results. The chemistry engine remains the only authority that may turn a
//! request into trusted chemical meaning.

use std::collections::BTreeMap;

use chem_catalogue::TrustedCatalogue;
use chem_domain::StructureDefinition;

use crate::{chemistry, elements};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CompositionId {
    Hydrogen,
    Oxygen,
    Ozone,
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
pub struct CompositionPreview {
    pub id: CompositionId,
    pub formula: &'static str,
    pub name: &'static str,
    pub(crate) atoms: &'static [(u8, u8)],
}

impl CompositionPreview {
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
        id: CompositionId::Ozone,
        formula: "O3",
        name: "Ozone",
        atoms: &[(8, 3)],
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

/// A preview projected from one unambiguous graph in the host-pinned catalogue.
/// It is deliberately separate from the small curated naming table above.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustedCompositionPreview {
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

impl TrustedCompositionPreview {
    pub fn covalent_bonds(&self) -> &[PreviewCovalentBond] {
        &self.covalent_bonds
    }

    pub fn ionic_links(&self) -> &[PreviewIonicLink] {
        &self.ionic_links
    }
}

/// Resolves a draft only when the trusted catalogue contains one unambiguous
/// structural graph for its exact atom inventory.
pub fn trusted_preview(
    atomic_numbers: impl IntoIterator<Item = u8>,
) -> Option<TrustedCompositionPreview> {
    let catalogue = chemistry::trusted_catalogue().ok()?;
    resolve_with_catalogue(catalogue, atomic_numbers)
}

/// Resolves one exact structure identity from the host-pinned catalogue.
/// This is used when a reviewed experience already names its product graph.
#[must_use]
pub fn trusted_preview_by_structure_id(id: &str) -> Option<TrustedCompositionPreview> {
    let catalogue = chemistry::trusted_catalogue().ok()?;
    let structure = catalogue
        .document()
        .structures
        .iter()
        .find(|record| record.id().as_str() == id)
        .map(|record| (record.id(), record.formula()))
        .or_else(|| {
            catalogue
                .document()
                .structure_applications
                .iter()
                .find(|application| application.id.as_str() == id)
                .map(|application| (&application.id, application.formula.as_str()))
        })?;
    preview_from_definition(catalogue.structure(structure.0)?, structure.1)
}

/// Matches a registry formula through the trusted structural catalogue rather
/// than the legacy UI preview table. The atom inventory must resolve to one
/// unambiguous topology, and that topology must be isomorphic to a catalogue
/// structure carrying the requested formula.
pub fn trusted_formula_matches(
    formula: &str,
    atomic_numbers: impl IntoIterator<Item = u8>,
) -> bool {
    let Some(preview) = trusted_preview(atomic_numbers) else {
        return false;
    };
    let Ok(catalogue) = chemistry::trusted_catalogue() else {
        return false;
    };
    let definitions = catalogue
        .document()
        .structures
        .iter()
        .filter(|record| record.formula() == formula)
        .map(|record| record.id())
        .chain(
            catalogue
                .document()
                .structure_applications
                .iter()
                .filter(|application| application.formula == formula)
                .map(|application| &application.id),
        );
    definitions
        .filter_map(|id| catalogue.structure(id))
        .any(|definition| {
            preview_from_definition(definition, formula).is_some_and(|candidate| {
                previews_are_isomorphic(&preview, &candidate).is_some_and(|value| value)
            })
        })
}

fn resolve_with_catalogue(
    catalogue: &TrustedCatalogue,
    atomic_numbers: impl IntoIterator<Item = u8>,
) -> Option<TrustedCompositionPreview> {
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

    let mut matches = Vec::<TrustedCompositionPreview>::new();
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

    let mut unique = Vec::<TrustedCompositionPreview>::new();
    for candidate in matches {
        if unique.iter().any(|existing| {
            previews_are_isomorphic(existing, &candidate).is_some_and(|equivalent| equivalent)
        }) {
            continue;
        }
        unique.push(candidate);
    }
    let [preview] = unique.as_slice() else {
        return None;
    };
    Some(preview.clone())
}

fn add_matching_structure(
    catalogue: &TrustedCatalogue,
    id: &chem_domain::StructureId,
    formula: &str,
    selected: &BTreeMap<String, u64>,
    matches: &mut Vec<TrustedCompositionPreview>,
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
    if &inventory == selected
        && let Some(preview) = preview_from_definition(definition, formula)
    {
        matches.push(preview);
    }
}

fn preview_from_definition(
    definition: &StructureDefinition,
    formula: &str,
) -> Option<TrustedCompositionPreview> {
    let graph = definition.graph();
    let mut atom_indices = BTreeMap::new();
    let atoms = graph
        .atoms()
        .values()
        .enumerate()
        .map(|(index, atom)| {
            atom_indices.insert(atom.id().clone(), index);
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
                start: *atom_indices.get(bond.left())?,
                end: *atom_indices.get(bond.right())?,
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
                        .then_some((atom_indices[id], atom_charge.unsigned_abs()))
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

    Some(TrustedCompositionPreview {
        structure_id: definition.id().as_str().to_owned(),
        formula: display_formula(formula),
        atoms,
        covalent_bonds,
        ionic_links,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PreviewAtomSignature {
    atomic_number: u8,
    formal_charge: i16,
    non_bonding_electrons: u8,
    unpaired_electrons: u8,
    covalent_orders: Vec<u8>,
    ionic_degree: usize,
}

fn preview_atom_signature(
    preview: &TrustedCompositionPreview,
    index: usize,
) -> PreviewAtomSignature {
    let atom = &preview.atoms[index];
    let mut covalent_orders = preview
        .covalent_bonds
        .iter()
        .filter_map(|bond| (bond.start == index || bond.end == index).then_some(bond.order))
        .collect::<Vec<_>>();
    covalent_orders.sort_unstable();
    let ionic_degree = preview
        .ionic_links
        .iter()
        .filter(|link| link.start == index || link.end == index)
        .count();
    PreviewAtomSignature {
        atomic_number: atom.atomic_number,
        formal_charge: atom.formal_charge,
        non_bonding_electrons: atom.non_bonding_electrons,
        unpaired_electrons: atom.unpaired_electrons,
        covalent_orders,
        ionic_degree,
    }
}

fn preview_edge_signature(
    preview: &TrustedCompositionPreview,
    left: usize,
    right: usize,
) -> (Option<u8>, bool) {
    let covalent = preview.covalent_bonds.iter().find_map(|bond| {
        ((bond.start == left && bond.end == right) || (bond.start == right && bond.end == left))
            .then_some(bond.order)
    });
    let ionic = preview.ionic_links.iter().any(|link| {
        (link.start == left && link.end == right) || (link.start == right && link.end == left)
    });
    (covalent, ionic)
}

#[allow(clippy::items_after_statements)]
fn previews_are_isomorphic(
    left: &TrustedCompositionPreview,
    right: &TrustedCompositionPreview,
) -> Option<bool> {
    const MAX_PREVIEW_ISOMORPHISM_WORK: usize = 4_096;

    if left.atoms.len() != right.atoms.len()
        || left.covalent_bonds.len() != right.covalent_bonds.len()
        || left.ionic_links.len() != right.ionic_links.len()
    {
        return Some(false);
    }
    let left_signatures = (0..left.atoms.len())
        .map(|index| preview_atom_signature(left, index))
        .collect::<Vec<_>>();
    let right_signatures = (0..right.atoms.len())
        .map(|index| preview_atom_signature(right, index))
        .collect::<Vec<_>>();
    let mut sources = (0..left.atoms.len()).collect::<Vec<_>>();
    sources.sort_by_key(|source| {
        (
            right_signatures
                .iter()
                .filter(|candidate| *candidate == &left_signatures[*source])
                .count(),
            *source,
        )
    });

    #[allow(clippy::too_many_arguments)]
    fn search(
        depth: usize,
        sources: &[usize],
        left: &TrustedCompositionPreview,
        right: &TrustedCompositionPreview,
        left_signatures: &[PreviewAtomSignature],
        right_signatures: &[PreviewAtomSignature],
        mapping: &mut [Option<usize>],
        used: &mut [bool],
        work: &mut usize,
    ) -> Option<bool> {
        *work += 1;
        if *work > MAX_PREVIEW_ISOMORPHISM_WORK {
            return None;
        }
        let Some(source) = sources.get(depth).copied() else {
            return Some(true);
        };
        for target in 0..right.atoms.len() {
            if used[target] || left_signatures[source] != right_signatures[target] {
                continue;
            }
            let preserves_edges = mapping.iter().enumerate().all(|(other_source, mapped)| {
                mapped.is_none_or(|other_target| {
                    preview_edge_signature(left, source, other_source)
                        == preview_edge_signature(right, target, other_target)
                })
            });
            if !preserves_edges {
                continue;
            }
            mapping[source] = Some(target);
            used[target] = true;
            match search(
                depth + 1,
                sources,
                left,
                right,
                left_signatures,
                right_signatures,
                mapping,
                used,
                work,
            ) {
                Some(true) => return Some(true),
                Some(false) => {}
                None => return None,
            }
            mapping[source] = None;
            used[target] = false;
        }
        Some(false)
    }

    let mut work = 0;
    search(
        0,
        &sources,
        left,
        right,
        &left_signatures,
        &right_signatures,
        &mut vec![None; left.atoms.len()],
        &mut vec![false; right.atoms.len()],
        &mut work,
    )
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

    #[test]
    fn trusted_previews_project_structure_instead_of_formula_switches() {
        let oxygen = trusted_preview([8]).expect("single oxygen selection uses O2");
        assert_eq!(oxygen.formula, "O₂");
        assert_eq!(oxygen.atoms.len(), 2);
        assert_eq!(oxygen.covalent_bonds().len(), 1);
        assert_eq!(oxygen.covalent_bonds()[0].order, 2);

        let carbon_dioxide = trusted_preview([8, 6, 8]).expect("catalogued CO2 graph");
        assert_eq!(carbon_dioxide.formula, "CO₂");
        assert_eq!(carbon_dioxide.covalent_bonds().len(), 2);
        assert!(
            carbon_dioxide
                .covalent_bonds()
                .iter()
                .all(|bond| bond.order == 2)
        );

        let magnesium_fluoride = trusted_preview([9, 12, 9]).expect("catalogued MgF2 graph");
        assert_eq!(magnesium_fluoride.formula, "MgF₂");
        assert!(magnesium_fluoride.covalent_bonds().is_empty());
        assert_eq!(magnesium_fluoride.ionic_links().len(), 2);
    }

    #[test]
    fn exact_product_identity_resolves_reviewed_covalent_graphs() {
        let ammonia = trusted_preview_by_structure_id("Ammonia").expect("reviewed ammonia graph");
        assert_eq!(ammonia.formula, "NH₃");
        assert_eq!(ammonia.covalent_bonds().len(), 3);

        let iodine_heptafluoride = trusted_preview_by_structure_id("InterhalogenIF7")
            .expect("reviewed iodine heptafluoride graph");
        assert_eq!(iodine_heptafluoride.formula, "IF₇");
        assert_eq!(iodine_heptafluoride.covalent_bonds().len(), 7);
        assert!(iodine_heptafluoride.ionic_links().is_empty());
        assert!(trusted_preview_by_structure_id("NotAReviewedStructure").is_none());
    }
}
