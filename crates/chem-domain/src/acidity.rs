//! Structure-derived acid capabilities.
//!
//! Acidity is not a formula-name whitelist. This module identifies exact
//! Brønsted-Lowry proton-donor *sites* on an already validated structure by
//! proving that heterolytic cleavage of an X-H bond has a valid local electron
//! ledger. Acid strength, solvent behaviour, equilibria, and whether a
//! particular reaction family applies remain separate, premise-bound facts.

use serde::Serialize;

use crate::{
    AtomId, BondOrder, CovalentBondId, CovalentElectronOrigin, ElectronAllocation, ElectronState,
    ElectronTransition, ElementSymbol, StructuralOperation, StructuralOperationId,
    StructuralOperationInput, StructureDefinition,
};

/// One structurally valid candidate for Brønsted-Lowry proton donation.
///
/// Construction is private: every value has crossed the normal covalent
/// cleavage electron-ledger validator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProtonDonorSite {
    proton: AtomId,
    donor: AtomId,
    donor_element: ElementSymbol,
    bond: CovalentBondId,
    conjugate_base_donor_state: ElectronState,
}

impl ProtonDonorSite {
    #[must_use]
    pub const fn proton(&self) -> &AtomId {
        &self.proton
    }

    #[must_use]
    pub const fn donor(&self) -> &AtomId {
        &self.donor
    }

    #[must_use]
    pub const fn donor_element(&self) -> &ElementSymbol {
        &self.donor_element
    }

    #[must_use]
    pub const fn bond(&self) -> &CovalentBondId {
        &self.bond
    }

    #[must_use]
    pub const fn conjugate_base_donor_state(&self) -> ElectronState {
        self.conjugate_base_donor_state
    }
}

/// Deterministic structural acidity profile for one validated compound.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BronstedAcidProfile {
    proton_donor_sites: Vec<ProtonDonorSite>,
}

impl BronstedAcidProfile {
    #[must_use]
    pub fn proton_donor_sites(&self) -> &[ProtonDonorSite] {
        &self.proton_donor_sites
    }

    /// Whether the structure contains at least one ledger-valid proton donor.
    ///
    /// This does not assert strength or favourable transfer in any particular
    /// solvent or reaction context.
    #[must_use]
    pub const fn is_protic_candidate(&self) -> bool {
        !self.proton_donor_sites.is_empty()
    }
}

/// Classifies every structurally possible Brønsted proton-donor site.
///
/// The algorithm is intentionally element-family independent. It considers a
/// neutral, locally electron-free hydrogen connected by one shared single
/// covalent bond to a non-hydrogen atom. A site is returned only when
/// heterolytic cleavage to the donor atom constructs a valid exact electron
/// ledger through [`StructuralOperation::new`]. This admits heteroatom and
/// carbon acids without declaring their context-dependent strengths equal.
#[must_use]
pub fn classify_bronsted_acid(structure: &StructureDefinition) -> BronstedAcidProfile {
    let graph = structure.graph();
    let (Ok(proton_state), Ok(proton_after)) =
        (ElectronState::new(0, 0, 0), ElectronState::new(1, 0, 0))
    else {
        return BronstedAcidProfile {
            proton_donor_sites: Vec::new(),
        };
    };
    let mut sites = Vec::new();
    for (index, bond) in graph.covalent_bonds().values().enumerate() {
        if bond.order() != BondOrder::Single
            || !matches!(bond.electron_origin(), CovalentElectronOrigin::Shared)
        {
            continue;
        }
        let left = &graph.atoms()[bond.left()];
        let right = &graph.atoms()[bond.right()];
        let (proton, donor) = if left.element().as_str() == "H" {
            (left, right)
        } else if right.element().as_str() == "H" {
            (right, left)
        } else {
            continue;
        };
        if donor.element().as_str() == "H"
            || proton.electrons() != proton_state
            || graph.covalent_bond_order_sum(proton.id()) != Some(1)
        {
            continue;
        }
        let donor_before = donor.electrons();
        let Some(local_after) = donor_before.non_bonding_electrons().checked_add(2) else {
            continue;
        };
        let Some(charge_after) = donor_before.formal_charge().checked_sub(1) else {
            continue;
        };
        let Ok(donor_after) =
            ElectronState::new(charge_after, local_after, donor_before.unpaired_electrons())
        else {
            continue;
        };
        let Ok(operation_id) = StructuralOperationId::new(format!("acid.site{index}")) else {
            continue;
        };
        if StructuralOperation::new(
            operation_id,
            StructuralOperationInput::CleaveCovalent {
                left: proton.id().clone(),
                right: donor.id().clone(),
                expected_order: BondOrder::Single,
                allocation: ElectronAllocation::HeterolyticTo(donor.id().clone()),
                transitions: vec![
                    ElectronTransition::new(proton.id().clone(), proton.electrons(), proton_after),
                    ElectronTransition::new(donor.id().clone(), donor_before, donor_after),
                ],
            },
        )
        .is_err()
        {
            continue;
        }
        sites.push(ProtonDonorSite {
            proton: proton.id().clone(),
            donor: donor.id().clone(),
            donor_element: donor.element().clone(),
            bond: bond.id().clone(),
            conjugate_base_donor_state: donor_after,
        });
    }
    BronstedAcidProfile {
        proton_donor_sites: sites,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Atom, CovalentBond, ElementInventory, RepresentationKind, StructuralGraph, StructureId,
    };

    fn atom(id: &str, element: &str, charge: i16, local: u8, unpaired: u8) -> Atom {
        Atom::new(
            AtomId::new(id).expect("atom id"),
            ElementSymbol::new(element).expect("element"),
            ElectronState::new(charge, local, unpaired).expect("electron state"),
        )
    }

    fn molecule(id: &str, atoms: Vec<Atom>, bonds: Vec<(&str, &str, &str)>) -> StructureDefinition {
        let mut inventory = std::collections::BTreeMap::<ElementSymbol, u64>::new();
        for atom in &atoms {
            *inventory.entry(atom.element().clone()).or_default() += 1;
        }
        let bonds = bonds
            .into_iter()
            .map(|(id, left, right)| {
                CovalentBond::new(
                    CovalentBondId::new(id).expect("bond id"),
                    AtomId::new(left).expect("left atom"),
                    AtomId::new(right).expect("right atom"),
                    BondOrder::Single,
                )
                .expect("bond")
            })
            .collect::<Vec<_>>();
        StructureDefinition::new(
            StructureId::new(id).expect("structure id"),
            ElementInventory::new(inventory).expect("inventory"),
            RepresentationKind::Molecular,
            StructuralGraph::new(atoms, bonds, [], [], []).expect("graph"),
        )
        .expect("molecule")
    }

    #[test]
    fn hydrogen_halide_is_classified_without_a_formula_or_name_list() {
        let hcl = molecule(
            "test.hcl",
            vec![atom("h", "H", 0, 0, 0), atom("cl", "Cl", 0, 6, 0)],
            vec![("hcl", "h", "cl")],
        );
        let profile = classify_bronsted_acid(&hcl);
        assert!(profile.is_protic_candidate());
        assert_eq!(profile.proton_donor_sites().len(), 1);
        assert_eq!(
            profile.proton_donor_sites()[0].donor_element().as_str(),
            "Cl"
        );
        assert_eq!(
            profile.proton_donor_sites()[0]
                .conjugate_base_donor_state()
                .formal_charge(),
            -1
        );
    }

    #[test]
    fn carbon_acid_sites_remain_structurally_representable_and_context_bound() {
        let methane = molecule(
            "test.methane",
            vec![
                atom("c", "C", 0, 0, 0),
                atom("h1", "H", 0, 0, 0),
                atom("h2", "H", 0, 0, 0),
                atom("h3", "H", 0, 0, 0),
                atom("h4", "H", 0, 0, 0),
            ],
            vec![
                ("ch1", "c", "h1"),
                ("ch2", "c", "h2"),
                ("ch3", "c", "h3"),
                ("ch4", "c", "h4"),
            ],
        );
        let profile = classify_bronsted_acid(&methane);
        assert_eq!(profile.proton_donor_sites().len(), 4);
        assert!(
            profile
                .proton_donor_sites()
                .iter()
                .all(|site| site.donor_element().as_str() == "C")
        );
    }

    #[test]
    fn multiple_donor_sites_are_counted_from_one_structure() {
        let peroxide = molecule(
            "test.peroxide",
            vec![
                atom("h1", "H", 0, 0, 0),
                atom("o1", "O", 0, 4, 0),
                atom("o2", "O", 0, 4, 0),
                atom("h2", "H", 0, 0, 0),
            ],
            vec![
                ("h1o1", "h1", "o1"),
                ("o1o2", "o1", "o2"),
                ("o2h2", "o2", "h2"),
            ],
        );
        let profile = classify_bronsted_acid(&peroxide);
        assert_eq!(profile.proton_donor_sites().len(), 2);
    }

    #[test]
    fn hydrogen_molecule_is_not_misclassified_as_a_compound_acid_site() {
        let hydrogen = molecule(
            "test.hydrogen",
            vec![atom("h1", "H", 0, 0, 0), atom("h2", "H", 0, 0, 0)],
            vec![("hh", "h1", "h2")],
        );
        assert!(!classify_bronsted_acid(&hydrogen).is_protic_candidate());
    }
}
