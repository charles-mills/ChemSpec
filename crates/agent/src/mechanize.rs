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
    MetallicElectronStateRecord, MetallicReleaseAllocationRecord, TransferElectronStateRecord,
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
pub(crate) fn derive_algorithmic_mechanism(
    request: &MechanismEscalationRequest,
) -> Option<MechanismEscalationResponse> {
    let reactants = expand_side(&request.reactants)?;
    let products = expand_side(&request.products)?;
    // ponytail: metallic products (displacement onto a metal) are left to
    // the model until join_metallic conventions are exercised by a test.
    if !products.domains.is_empty() {
        return None;
    }
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

    // 1. Every reactant ionic association dissociates.
    for association in &reactants.associations {
        operations.push(MechanismOperation::DissociateIonic {
            association: association.path.clone(),
        });
    }

    // 2. Metallic sites release their share of the domain electrons.
    for domain in &reactants.domains {
        let share = u8::try_from(domain.electrons / u32::try_from(domain.sites.len()).ok()?).ok()?;
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
            edge: (bond.left.clone(), bond.right.clone(), bond_record(bond.order)?),
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
        match delta {
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
            let donor_after = State {
                charge: donor_before.charge.checked_add(i16::from(count))?,
                lone: donor_before.lone.checked_sub(count)?,
                unpaired: donor_before.unpaired.abs_diff(count),
            };
            let acceptor_after = State {
                charge: acceptor_before.charge.checked_sub(i16::from(count))?,
                lone: acceptor_before.lone.checked_add(count)?,
                unpaired: acceptor_before.unpaired.abs_diff(count),
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
        let left_before = *ledger.get(left)?;
        let right_before = *ledger.get(right)?;
        let give_left = left_before.lone.checked_sub(target(left)?.lone)?;
        let give_right = right_before.lone.checked_sub(target(right)?.lone)?;
        let total = bond.order.checked_mul(2)?;
        let left_contribution = bond.order.min(give_left).max(total.saturating_sub(give_right));
        let right_contribution = total.checked_sub(left_contribution)?;
        if left_contribution > give_left || right_contribution > give_right {
            return None;
        }
        let contribute = |state: State, contribution: u8| -> Option<State> {
            Some(State {
                charge: state
                    .charge
                    .checked_add(i16::from(contribution))?
                    .checked_sub(i16::from(bond.order))?,
                lone: state.lone.checked_sub(contribution)?,
                unpaired: state.unpaired.saturating_sub(contribution),
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

    // 6. Product ionic associations reassemble from the mapped atoms.
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

    // 7. Assign every product instance its mapped atoms.
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
fn expand_side(species: &[MechanismSpecies]) -> Option<Side> {
    let mut side = Side::default();
    for entry in species {
        for instance in 1..=entry.coefficient {
            let prefix = format!("{}[{instance}]", entry.role);
            let path = |label: &str| format!("{prefix}.{label}");
            let mut instance_atoms = Vec::new();
            let mut push_atoms = |records: &[chem_catalogue::AtomRecord],
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

    let mut mapping = BTreeMap::<String, String>::new();
    let mut used = vec![false; reactants.atoms.len()];
    let mut assigned = vec![false; products.atoms.len()];

    // Pass 1: exact local-environment matches.
    for (product_index, product) in products.atoms.iter().enumerate() {
        if let Some(reactant_index) = reactants.atoms.iter().enumerate().position(|(index, _)| {
            !used[index] && reactant_signatures[index] == product_signatures[product_index]
        }) {
            used[reactant_index] = true;
            assigned[product_index] = true;
            mapping.insert(
                reactants.atoms[reactant_index].path.clone(),
                product.path.clone(),
            );
        }
    }
    // Pass 2: prefer candidates that preserve bonds to already-mapped atoms.
    let inverse_of = |mapping: &BTreeMap<String, String>, product_path: &str| {
        mapping
            .iter()
            .find(|(_, mapped)| mapped.as_str() == product_path)
            .map(|(reactant, _)| reactant.clone())
    };
    for (product_index, product) in products.atoms.iter().enumerate() {
        if assigned[product_index] {
            continue;
        }
        let bonded_reactant_partners = products
            .bonds
            .iter()
            .filter_map(|bond| {
                let other = if bond.left == product.path {
                    &bond.right
                } else if bond.right == product.path {
                    &bond.left
                } else {
                    return None;
                };
                inverse_of(&mapping, other)
            })
            .collect::<Vec<_>>();
        let score = |candidate: &SideAtom| {
            reactants
                .bonds
                .iter()
                .filter(|bond| {
                    (bond.left == candidate.path
                        && bonded_reactant_partners.contains(&bond.right))
                        || (bond.right == candidate.path
                            && bonded_reactant_partners.contains(&bond.left))
                })
                .count()
        };
        let best = reactants
            .atoms
            .iter()
            .enumerate()
            .filter(|(index, candidate)| !used[*index] && candidate.element == product.element)
            .max_by_key(|(_, candidate)| score(candidate))?;
        used[best.0] = true;
        assigned[product_index] = true;
        mapping.insert(best.1.path.clone(), product.path.clone());
    }
    (mapping.len() == reactants.atoms.len() && used.iter().all(|flag| *flag)).then_some(mapping)
}
