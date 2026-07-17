//! Algorithmic mechanism derivation.
//!
//! A mechanism is a computable path between two known endpoints: the
//! validated reactant graphs and the validated product graphs. This module
//! diffs them under an element-preserving atom mapping and emits the same
//! operation sequence a model would propose — metallic releases, homolytic
//! cleaves, electron transfers solved exactly from the formal-charge deltas,
//! bond formations with contributions solved from the lone-pair deltas, and
//! ionic reassociation. The kernel validates the result identically either
//! way; the model is only consulted when this derivation fails.

use std::collections::BTreeMap;

use chem_catalogue::{
    BinaryElectronStateRecord, BondOrderRecord, ElectronContributionRecord, ElectronStateRecord,
    MetallicElectronStateRecord, MetallicJoinAllocationRecord, MetallicReleaseAllocationRecord,
    TransferElectronStateRecord,
};

use crate::claim::{
    LabelledStructure, MECHANISM_ESCALATION_SCHEMA_VERSION, MechanismCleavageAllocation,
    MechanismEscalationRequest, MechanismEscalationResponse, MechanismHomolytic, MechanismMapping,
    MechanismOperation, MechanismSpecies,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct State {
    charge: i16,
    lone: u8,
    unpaired: u8,
}

impl State {
    const fn record(self) -> ElectronStateRecord {
        ElectronStateRecord(self.charge, self.lone, self.unpaired)
    }
}

#[derive(Debug, Clone)]
struct SideAtom {
    path: String,
    element: String,
    state: State,
}

#[derive(Debug, Clone)]
struct SideBond {
    left: String,
    right: String,
    order: u8,
}

#[derive(Debug, Clone)]
struct SideDomain {
    path: String,
    sites: Vec<String>,
    electrons: u32,
}

#[derive(Debug, Clone)]
struct SideAssociation {
    path: String,
    /// Atom paths per component group.
    components: Vec<Vec<String>>,
}

#[derive(Debug, Default)]
struct Side {
    atoms: Vec<SideAtom>,
    bonds: Vec<SideBond>,
    domains: Vec<SideDomain>,
    associations: Vec<SideAssociation>,
    /// Instance prefix -> atom paths, in declaration order.
    instances: Vec<(String, Vec<String>)>,
}

/// Derives a complete mechanism response for the request, or None when the
/// diff cannot be closed exactly (the caller then escalates to the model).
#[must_use]
#[allow(clippy::too_many_lines)]
pub(crate) fn derive_algorithmic_mechanism(
    request: &MechanismEscalationRequest,
) -> Option<MechanismEscalationResponse> {
    let reactants = expand_side(&request.reactants)?;
    let products = expand_side(&request.products)?;
    let mapping = map_atoms(&reactants, &products)?;
    let inverse = mapping
        .iter()
        .map(|(reactant, product)| (product.clone(), reactant.clone()))
        .collect::<BTreeMap<_, _>>();
    let target = |reactant_path: &str| -> Option<State> {
        let product_path = mapping.get(reactant_path)?;
        products
            .atoms
            .iter()
            .find(|atom| &atom.path == product_path)
            .map(|atom| atom.state)
    };

    let mut ledger: BTreeMap<String, State> = reactants
        .atoms
        .iter()
        .map(|atom| (atom.path.clone(), atom.state))
        .collect();
    let mut operations = Vec::new();

    // Product metallic domains: each joining site must first acquire its
    // electron share (through the transfers below) and then donate it.
    let mut join_share: BTreeMap<String, u8> = BTreeMap::new();
    let mut joins = Vec::new();
    for domain in &products.domains {
        let share =
            u8::try_from(domain.electrons / u32::try_from(domain.sites.len()).ok()?).ok()?;
        let sites = domain
            .sites
            .iter()
            .map(|site| inverse.get(site).cloned())
            .collect::<Option<Vec<_>>>()?;
        for site in &sites {
            join_share.insert(site.clone(), share);
        }
        joins.push((domain.path.clone(), sites, share));
    }

    // 1. Every reactant ionic association dissociates.
    for association in &reactants.associations {
        operations.push(MechanismOperation::DissociateIonic {
            association: association.path.clone(),
        });
    }

    // 2. Metallic sites release their share of the domain electrons.
    for domain in &reactants.domains {
        let share =
            u8::try_from(domain.electrons / u32::try_from(domain.sites.len()).ok()?).ok()?;
        let mut remaining = domain.electrons;
        for site in &domain.sites {
            let before = *ledger.get(site)?;
            let after = State {
                charge: before.charge.checked_sub(i16::from(share))?,
                lone: before.lone.checked_add(share)?,
                unpaired: before.unpaired.checked_add(share)?,
            };
            operations.push(MechanismOperation::ReleaseMetallic {
                site: site.clone(),
                domain: domain.path.clone(),
                allocation: MetallicReleaseAllocationRecord::RetainElectron,
                before: MetallicElectronStateRecord {
                    site: before.record(),
                    domain_electrons: remaining,
                },
                after: MetallicElectronStateRecord {
                    site: after.record(),
                    domain_electrons: remaining - u32::from(share),
                },
            });
            remaining -= u32::from(share);
            ledger.insert(site.clone(), after);
        }
    }

    // 3. Homolytic cleavage of every bond that does not survive the mapping.
    let preserved = |bond: &SideBond| {
        let (Some(left), Some(right)) = (mapping.get(&bond.left), mapping.get(&bond.right)) else {
            return false;
        };
        products.bonds.iter().any(|candidate| {
            candidate.order == bond.order
                && ((&candidate.left == left && &candidate.right == right)
                    || (&candidate.left == right && &candidate.right == left))
        })
    };
    for bond in &reactants.bonds {
        if preserved(bond) {
            continue;
        }
        let before_left = *ledger.get(&bond.left)?;
        let before_right = *ledger.get(&bond.right)?;
        let split = |state: State| -> Option<State> {
            Some(State {
                charge: state.charge,
                lone: state.lone.checked_add(bond.order)?,
                unpaired: state.unpaired.checked_add(bond.order)?,
            })
        };
        let after_left = split(before_left)?;
        let after_right = split(before_right)?;
        operations.push(MechanismOperation::CleaveCovalent {
            edge: (
                bond.left.clone(),
                bond.right.clone(),
                bond_record(bond.order)?,
            ),
            allocation: MechanismCleavageAllocation::Homolytic(MechanismHomolytic::Homolytic),
            before: BinaryElectronStateRecord {
                left: before_left.record(),
                right: before_right.record(),
            },
            after: BinaryElectronStateRecord {
                left: after_left.record(),
                right: after_right.record(),
            },
        });
        ledger.insert(bond.left.clone(), after_left);
        ledger.insert(bond.right.clone(), after_right);
    }

    // 4. Electron transfers reconcile every formal-charge delta exactly.
    let mut donors = Vec::new();
    let mut acceptors = Vec::new();
    for atom in &reactants.atoms {
        let current = *ledger.get(&atom.path)?;
        let delta = i32::from(target(&atom.path)?.charge) - i32::from(current.charge);
        // A joining site's charge rises by its donated share at the join,
        // so the transfers must deliver it that many extra electrons.
        let give = delta - i32::from(join_share.get(&atom.path).copied().unwrap_or(0));
        match give {
            0 => {}
            gain if gain > 0 => donors.push((atom.path.clone(), u8::try_from(gain).ok()?)),
            loss => acceptors.push((atom.path.clone(), u8::try_from(-loss).ok()?)),
        }
    }
    let mut donor_queue = donors.into_iter().collect::<Vec<_>>();
    for (acceptor, mut needed) in acceptors {
        while needed > 0 {
            let (donor, available) = donor_queue.iter_mut().find(|(_, left)| *left > 0)?;
            let count = needed.min(*available);
            let donor_path = donor.clone();
            *available -= count;
            needed -= count;
            let donor_before = *ledger.get(&donor_path)?;
            let acceptor_before = *ledger.get(&acceptor)?;
            let donor_lone = donor_before.lone.checked_sub(count)?;
            let donor_after = State {
                charge: donor_before.charge.checked_add(i16::from(count))?,
                lone: donor_lone,
                unpaired: settled_unpaired(donor_before.unpaired, count, donor_lone),
            };
            let acceptor_lone = acceptor_before.lone.checked_add(count)?;
            let acceptor_after = State {
                charge: acceptor_before.charge.checked_sub(i16::from(count))?,
                lone: acceptor_lone,
                unpaired: settled_unpaired(acceptor_before.unpaired, count, acceptor_lone),
            };
            operations.push(MechanismOperation::TransferElectron {
                count,
                donor: donor_path.clone(),
                acceptor: acceptor.clone(),
                before: TransferElectronStateRecord {
                    donor: donor_before.record(),
                    acceptor: acceptor_before.record(),
                },
                after: TransferElectronStateRecord {
                    donor: donor_after.record(),
                    acceptor: acceptor_after.record(),
                },
            });
            ledger.insert(donor_path, donor_after);
            ledger.insert(acceptor.clone(), acceptor_after);
        }
    }

    // 5. Form every product bond that was not preserved, contributions
    // solved from each side's remaining lone-electron surplus.
    for bond in &products.bonds {
        let left = inverse.get(&bond.left)?;
        let right = inverse.get(&bond.right)?;
        let survives = reactants.bonds.iter().any(|candidate| {
            candidate.order == bond.order
                && ((&candidate.left == left && &candidate.right == right)
                    || (&candidate.left == right && &candidate.right == left))
        });
        if survives {
            continue;
        }
        let give_left = ledger.get(left)?.lone.checked_sub(target(left)?.lone)?;
        let give_right = ledger.get(right)?.lone.checked_sub(target(right)?.lone)?;
        let total = bond.order.checked_mul(2)?;
        let left_contribution = bond
            .order
            .min(give_left)
            .max(total.saturating_sub(give_right));
        let right_contribution = total.checked_sub(left_contribution)?;
        if left_contribution > give_left || right_contribution > give_right {
            return None;
        }
        // The kernel forms bonds from unpaired electrons: break lone pairs
        // open first where a contributor has too few singles.
        for (path, contribution) in [(left, left_contribution), (right, right_contribution)] {
            let state = *ledger.get(path)?;
            if state.unpaired < contribution {
                let unpaired = if (state.lone - contribution) % 2 == 0 {
                    contribution
                } else {
                    contribution.checked_add(1)?
                };
                let opened = State { unpaired, ..state };
                operations.push(MechanismOperation::ReconfigureElectrons {
                    atom: path.clone(),
                    before: state.record(),
                    after: opened.record(),
                });
                ledger.insert(path.clone(), opened);
            }
        }
        let left_before = *ledger.get(left)?;
        let right_before = *ledger.get(right)?;
        let contribute = |state: State, contribution: u8| -> Option<State> {
            let lone = state.lone.checked_sub(contribution)?;
            Some(State {
                charge: state
                    .charge
                    .checked_add(i16::from(contribution))?
                    .checked_sub(i16::from(bond.order))?,
                lone,
                unpaired: settled_unpaired(state.unpaired, contribution, lone),
            })
        };
        let left_after = contribute(left_before, left_contribution)?;
        let right_after = contribute(right_before, right_contribution)?;
        operations.push(MechanismOperation::FormCovalent {
            edge: (left.clone(), right.clone(), bond_record(bond.order)?),
            electron_contribution: ElectronContributionRecord {
                left: left_contribution,
                right: right_contribution,
            },
            before: BinaryElectronStateRecord {
                left: left_before.record(),
                right: right_before.record(),
            },
            after: BinaryElectronStateRecord {
                left: left_after.record(),
                right: right_after.record(),
            },
        });
        ledger.insert(left.clone(), left_after);
        ledger.insert(right.clone(), right_after);
    }

    // 5b. Sites join product metallic domains, donating their acquired
    // share as unpaired electrons (pairs are broken open first).
    for (domain_path, sites, share) in &joins {
        let mut pooled = 0_u32;
        for site in sites {
            let state = *ledger.get(site)?;
            if state.unpaired < *share {
                let unpaired = if (state.lone.checked_sub(*share)?) % 2 == 0 {
                    *share
                } else {
                    share.checked_add(1)?
                };
                let opened = State { unpaired, ..state };
                operations.push(MechanismOperation::ReconfigureElectrons {
                    atom: site.clone(),
                    before: state.record(),
                    after: opened.record(),
                });
                ledger.insert(site.clone(), opened);
            }
            let before = *ledger.get(site)?;
            let after = State {
                charge: before.charge.checked_add(i16::from(*share))?,
                lone: before.lone.checked_sub(*share)?,
                unpaired: before.unpaired.checked_sub(*share)?,
            };
            operations.push(MechanismOperation::JoinMetallic {
                site: site.clone(),
                domain: domain_path.clone(),
                allocation: MetallicJoinAllocationRecord::DonateElectron,
                before: MetallicElectronStateRecord {
                    site: before.record(),
                    domain_electrons: pooled,
                },
                after: MetallicElectronStateRecord {
                    site: after.record(),
                    domain_electrons: pooled + u32::from(*share),
                },
            });
            pooled += u32::from(*share);
            ledger.insert(site.clone(), after);
        }
    }

    // 6. Close pure spin-pairing gaps: a donor that gave from its d-shell
    // may be left with unpaired electrons the product state has paired.
    for atom in &reactants.atoms {
        let current = *ledger.get(&atom.path)?;
        let goal = target(&atom.path)?;
        if current == goal {
            continue;
        }
        if current.charge == goal.charge && current.lone == goal.lone {
            operations.push(MechanismOperation::ReconfigureElectrons {
                atom: atom.path.clone(),
                before: current.record(),
                after: goal.record(),
            });
            ledger.insert(atom.path.clone(), goal);
        } else {
            // Anything beyond spin pairing means the diff did not close.
            return None;
        }
    }

    // 7. Product ionic associations reassemble from the mapped atoms.
    for (index, association) in products.associations.iter().enumerate() {
        let components = association
            .components
            .iter()
            .map(|component| {
                component
                    .iter()
                    .map(|atom| inverse.get(atom).cloned())
                    .collect::<Option<Vec<_>>>()
            })
            .collect::<Option<Vec<_>>>()?;
        let component_charges = association
            .components
            .iter()
            .map(|component| {
                component
                    .iter()
                    .map(|atom| {
                        products
                            .atoms
                            .iter()
                            .find(|candidate| &candidate.path == atom)
                            .map(|candidate| candidate.state.charge)
                    })
                    .sum::<Option<i16>>()
            })
            .collect::<Option<Vec<_>>>()?;
        operations.push(MechanismOperation::AssociateIonic {
            label: format!("ionic.derived{}", index + 1),
            components,
            component_charges,
        });
    }

    // 8. Assign every product instance its mapped atoms.
    for (prefix, atom_paths) in &products.instances {
        let atoms = atom_paths
            .iter()
            .map(|atom| inverse.get(atom).cloned())
            .collect::<Option<Vec<_>>>()?;
        operations.push(MechanismOperation::AssignProduct {
            atoms,
            product: prefix.clone(),
        });
    }

    Some(MechanismEscalationResponse {
        schema_version: MECHANISM_ESCALATION_SCHEMA_VERSION,
        mapping: mapping
            .into_iter()
            .map(|(reactant, product)| MechanismMapping { reactant, product })
            .collect(),
        operations,
    })
}

/// Unpaired electrons after moving `delta` electrons in or out of a shell:
/// existing unpaired electrons participate first, and any pair that had to
/// break leaves a single behind (parity of the remaining shell decides).
fn settled_unpaired(unpaired: u8, delta: u8, lone_after: u8) -> u8 {
    let base = unpaired.saturating_sub(delta).min(lone_after);
    if (lone_after - base) % 2 == 1 {
        base + 1
    } else {
        base
    }
}

fn bond_record(order: u8) -> Option<BondOrderRecord> {
    match order {
        1 => Some(BondOrderRecord::Single),
        2 => Some(BondOrderRecord::Double),
        3 => Some(BondOrderRecord::Triple),
        _ => None,
    }
}

const fn order_of(record: BondOrderRecord) -> u8 {
    match record {
        BondOrderRecord::Single => 1,
        BondOrderRecord::Double => 2,
        BondOrderRecord::Triple => 3,
    }
}

/// Expands every species instance (one per coefficient) into flat,
/// path-labelled atoms, bonds, domains, and associations.
#[allow(clippy::too_many_lines)]
fn expand_side(species: &[MechanismSpecies]) -> Option<Side> {
    let mut side = Side::default();
    for entry in species {
        for instance in 1..=entry.coefficient {
            let prefix = format!("{}[{instance}]", entry.role);
            let path = |label: &str| format!("{prefix}.{label}");
            let mut instance_atoms = Vec::new();
            let push_atoms = |records: &[chem_catalogue::AtomRecord],
                              side: &mut Side,
                              instance_atoms: &mut Vec<String>| {
                for atom in records {
                    let atom_path = path(&atom.label);
                    instance_atoms.push(atom_path.clone());
                    side.atoms.push(SideAtom {
                        path: atom_path,
                        element: atom.element.clone(),
                        state: State {
                            charge: atom.formal_charge,
                            lone: atom.non_bonding_electrons,
                            unpaired: atom.unpaired_electrons,
                        },
                    });
                }
            };
            let push_bonds = |records: &[chem_catalogue::BondRecord], side: &mut Side| {
                for bond in records {
                    side.bonds.push(SideBond {
                        left: path(&bond.left),
                        right: path(&bond.right),
                        order: order_of(bond.order),
                    });
                }
            };
            match &entry.structure {
                LabelledStructure::Molecular { atoms, bonds, .. }
                | LabelledStructure::Ion { atoms, bonds, .. } => {
                    push_atoms(atoms, &mut side, &mut instance_atoms);
                    push_bonds(bonds, &mut side);
                }
                LabelledStructure::Ionic {
                    components,
                    associations,
                    ..
                } => {
                    let mut component_atoms = BTreeMap::new();
                    for component in components {
                        push_atoms(&component.atoms, &mut side, &mut instance_atoms);
                        push_bonds(&component.bonds, &mut side);
                        component_atoms.insert(
                            component.label.clone(),
                            component
                                .atoms
                                .iter()
                                .map(|atom| path(&atom.label))
                                .collect::<Vec<_>>(),
                        );
                    }
                    for association in associations {
                        side.associations.push(SideAssociation {
                            path: path(&association.label),
                            components: association
                                .components
                                .iter()
                                .map(|label| component_atoms.get(label).cloned())
                                .collect::<Option<Vec<_>>>()?,
                        });
                    }
                }
                LabelledStructure::Metallic { sites, domains, .. } => {
                    push_atoms(sites, &mut side, &mut instance_atoms);
                    for domain in domains {
                        side.domains.push(SideDomain {
                            path: path(&domain.label),
                            sites: domain.sites.iter().map(|site| path(site)).collect(),
                            electrons: domain.delocalized_electrons,
                        });
                    }
                }
            }
            side.instances.push((prefix, instance_atoms));
        }
    }
    Some(side)
}

/// Element-preserving bijection from reactant atoms to product atoms.
/// First pass pairs identical local signatures (preserved fragments), the
/// second prefers candidates that keep already-mapped bonds intact, the
/// remainder match by element alone.
/// Instance ordinal from an expanded path ("reactant1[2].a3" is copy 2).
fn instance_ordinal(path: &str) -> u32 {
    path.split('[')
        .nth(1)
        .and_then(|rest| rest.split(']').next())
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

/// Adjacency as (neighbour atom index, bond order) per atom.
fn adjacency(side: &Side) -> Vec<Vec<(usize, u8)>> {
    let index_of = side
        .atoms
        .iter()
        .enumerate()
        .map(|(index, atom)| (atom.path.as_str(), index))
        .collect::<BTreeMap<_, _>>();
    let mut result = vec![Vec::new(); side.atoms.len()];
    for bond in &side.bonds {
        if let (Some(&left), Some(&right)) = (
            index_of.get(bond.left.as_str()),
            index_of.get(bond.right.as_str()),
        ) {
            result[left].push((right, bond.order));
            result[right].push((left, bond.order));
        }
    }
    result
}

/// Least-action score for one candidate assignment, to maximize: preserved
/// bonds first (every unpreserved bond is a cleave plus a form), then the
/// fewest electron transfers, then instances staying with their own copies.
fn mapping_action(
    reactants: &Side,
    products: &Side,
    reactant_adjacency: &[Vec<(usize, u8)>],
    assignment: &[Option<usize>],
) -> (i64, i64, i64) {
    let mut preserved = 0_i64;
    for bond in &products.bonds {
        let index_of = |path: &str| products.atoms.iter().position(|atom| atom.path == path);
        let (Some(left), Some(right)) = (index_of(&bond.left), index_of(&bond.right)) else {
            continue;
        };
        if let (Some(mapped_left), Some(mapped_right)) = (assignment[left], assignment[right])
            && reactant_adjacency[mapped_left]
                .iter()
                .any(|(neighbour, order)| *neighbour == mapped_right && *order == bond.order)
        {
            preserved += 1;
        }
    }
    let mut charge_mismatch = 0_i64;
    let mut instance_distance = 0_i64;
    for (product_index, reactant_index) in assignment.iter().enumerate() {
        let Some(reactant_index) = reactant_index else {
            continue;
        };
        let reactant = &reactants.atoms[*reactant_index];
        let product = &products.atoms[product_index];
        charge_mismatch +=
            i64::from((i32::from(reactant.state.charge) - i32::from(product.state.charge)).abs());
        instance_distance +=
            i64::from(instance_ordinal(&reactant.path).abs_diff(instance_ordinal(&product.path)));
    }
    (preserved, -charge_mismatch, -instance_distance)
}

/// Element-preserving bijection from reactant atoms to product atoms,
/// chosen for least chemical action: preserved fragments pair exactly
/// (nearest copy first), the remainder grows outward from already-mapped
/// neighbours so bond-preservation scoring has context, and a bounded
/// 2-opt pass removes gratuitous swaps. Fewer unpreserved bonds and
/// transfers mean fewer operations and calmer animation; the kernel
/// validates the result identically either way.
#[allow(clippy::too_many_lines)]
fn map_atoms(reactants: &Side, products: &Side) -> Option<BTreeMap<String, String>> {
    let signature = |side: &Side, atom: &SideAtom| {
        let mut neighbours = side
            .bonds
            .iter()
            .filter_map(|bond| {
                let other = if bond.left == atom.path {
                    &bond.right
                } else if bond.right == atom.path {
                    &bond.left
                } else {
                    return None;
                };
                side.atoms
                    .iter()
                    .find(|candidate| &candidate.path == other)
                    .map(|candidate| (candidate.element.clone(), bond.order))
            })
            .collect::<Vec<_>>();
        neighbours.sort_unstable();
        (
            atom.element.clone(),
            atom.state.charge,
            atom.state.lone,
            atom.state.unpaired,
            neighbours,
        )
    };
    let reactant_signatures = reactants
        .atoms
        .iter()
        .map(|atom| signature(reactants, atom))
        .collect::<Vec<_>>();
    let product_signatures = products
        .atoms
        .iter()
        .map(|atom| signature(products, atom))
        .collect::<Vec<_>>();
    let reactant_adjacency = adjacency(reactants);
    let product_adjacency = adjacency(products);

    let mut assignment: Vec<Option<usize>> = vec![None; products.atoms.len()];
    let mut used = vec![false; reactants.atoms.len()];

    // Pass 1: exact local-environment matches, nearest copy first.
    for (product_index, product) in products.atoms.iter().enumerate() {
        let candidate = (0..reactants.atoms.len())
            .filter(|index| {
                !used[*index] && reactant_signatures[*index] == product_signatures[product_index]
            })
            .min_by_key(|index| {
                (
                    instance_ordinal(&reactants.atoms[*index].path)
                        .abs_diff(instance_ordinal(&product.path)),
                    *index,
                )
            });
        if let Some(index) = candidate {
            used[index] = true;
            assignment[product_index] = Some(index);
        }
    }

    // Pass 2: grow outward from mapped atoms; the frontier atom with the
    // most already-mapped neighbours goes first so its score has context.
    loop {
        let mapped_neighbours = |product_index: usize| {
            product_adjacency[product_index]
                .iter()
                .filter(|(neighbour, _)| assignment[*neighbour].is_some())
                .count()
        };
        let Some(product_index) = (0..products.atoms.len())
            .filter(|index| assignment[*index].is_none())
            .max_by_key(|index| (mapped_neighbours(*index), std::cmp::Reverse(*index)))
        else {
            break;
        };
        let product = &products.atoms[product_index];
        let preserved_with = |candidate: usize| {
            product_adjacency[product_index]
                .iter()
                .filter(|(neighbour, order)| {
                    assignment[*neighbour].is_some_and(|mapped| {
                        reactant_adjacency[candidate]
                            .iter()
                            .any(|(other, other_order)| *other == mapped && other_order == order)
                    })
                })
                .count()
        };
        let best = (0..reactants.atoms.len())
            .filter(|index| !used[*index] && reactants.atoms[*index].element == product.element)
            .max_by_key(|index| {
                (
                    preserved_with(*index),
                    std::cmp::Reverse(
                        instance_ordinal(&reactants.atoms[*index].path)
                            .abs_diff(instance_ordinal(&product.path)),
                    ),
                    std::cmp::Reverse(*index),
                )
            })?;
        used[best] = true;
        assignment[product_index] = Some(best);
    }

    // 2-opt: swap same-element destinations while the action improves.
    let mut action = mapping_action(reactants, products, &reactant_adjacency, &assignment);
    for _ in 0..6 {
        let mut improved = false;
        for first in 0..assignment.len() {
            for second in (first + 1)..assignment.len() {
                if products.atoms[first].element != products.atoms[second].element {
                    continue;
                }
                assignment.swap(first, second);
                let swapped = mapping_action(reactants, products, &reactant_adjacency, &assignment);
                if swapped > action {
                    action = swapped;
                    improved = true;
                } else {
                    assignment.swap(first, second);
                }
            }
        }
        if !improved {
            break;
        }
    }

    let mapping = assignment
        .iter()
        .enumerate()
        .map(|(product_index, reactant_index)| {
            reactant_index.map(|index| {
                (
                    reactants.atoms[index].path.clone(),
                    products.atoms[product_index].path.clone(),
                )
            })
        })
        .collect::<Option<BTreeMap<_, _>>>()?;
    (mapping.len() == reactants.atoms.len() && used.iter().all(|flag| *flag)).then_some(mapping)
}
