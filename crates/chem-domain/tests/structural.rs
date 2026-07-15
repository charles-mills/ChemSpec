use std::{collections::BTreeSet, fs, path::PathBuf};

use chem_domain::{
    Atom, AtomGroup, AtomGroupId, AtomId, AtomMapping, AtomMappingId, BondOrder, CovalentBond,
    CovalentBondId, CovalentDelocalization, CovalentDelocalizationId, CovalentElectronOrigin,
    EffectiveBondOrder, ElectronAllocation, ElectronState, ElectronTransition, ElementInventory,
    ElementSymbol, IonicAssociation, IonicAssociationId, MetallicDomain, MetallicDomainId,
    MetallicReleaseAllocation, ReactionSide, RepresentationKind, StructuralError, StructuralGraph,
    StructuralOperation, StructuralOperationId, StructuralOperationInput, StructuralOperationView,
    StructureDefinition, StructureId, StructureInstance, StructureInstanceId,
};
use proptest::prelude::*;
use serde_json::Value;

fn atom_id(value: &str) -> AtomId {
    AtomId::new(value).expect("test atom ID should be valid")
}

fn group_id(value: &str) -> AtomGroupId {
    AtomGroupId::new(value).expect("test group ID should be valid")
}

fn bond_id(value: &str) -> CovalentBondId {
    CovalentBondId::new(value).expect("test bond ID should be valid")
}

fn atom(value: &str, element: &str, charge: i16, local: u8, unpaired: u8) -> Atom {
    Atom::new(
        atom_id(value),
        ElementSymbol::new(element).expect("test element should be valid"),
        ElectronState::new(charge, local, unpaired).expect("test electron state should be valid"),
    )
}

fn inventory(elements: &[(&str, u64)]) -> ElementInventory {
    ElementInventory::new(elements.iter().map(|(symbol, count)| {
        (
            ElementSymbol::new(*symbol).expect("test element should be valid"),
            *count,
        )
    }))
    .expect("test inventory should be valid")
}

fn bond(value: &str, left: &str, right: &str, order: BondOrder) -> CovalentBond {
    CovalentBond::new(bond_id(value), atom_id(left), atom_id(right), order)
        .expect("test bond should be valid")
}

fn dative_bond(value: &str, donor: &str, acceptor: &str) -> CovalentBond {
    CovalentBond::new_dative(bond_id(value), atom_id(donor), atom_id(acceptor))
        .expect("test dative bond should be valid")
}

#[test]
fn superoxide_uses_integral_electrons_with_a_three_halves_effective_bond() {
    let effective = EffectiveBondOrder::new(3, 2).expect("3/2 is a valid effective order");
    let delocalization = CovalentDelocalization::new(
        CovalentDelocalizationId::new("superoxide.resonance").expect("valid domain ID"),
        effective,
    );
    let bond = CovalentBond::new_delocalized(
        bond_id("superoxide.oo"),
        atom_id("superoxide.o1"),
        atom_id("superoxide.o2"),
        BondOrder::Single,
        delocalization,
    )
    .expect("superoxide edge is valid");
    let graph = graph(
        vec![
            atom("superoxide.o1", "O", -1, 6, 0),
            atom("superoxide.o2", "O", 0, 5, 1),
        ],
        vec![bond],
        vec![],
        vec![],
        vec![],
    )
    .expect("resonance contributor is structurally valid");
    let edge = graph.covalent_bonds().values().next().expect("edge");
    assert_eq!(edge.order(), BondOrder::Single);
    assert_eq!(
        edge.delocalization()
            .expect("delocalized")
            .effective_order(),
        effective
    );
    assert_eq!(graph.explicit_valence_electron_count(), 13);
}

#[test]
fn delocalisation_is_a_reversible_structural_operation() {
    let state = CovalentDelocalization::new(
        CovalentDelocalizationId::new("superoxide.resonance").unwrap(),
        EffectiveBondOrder::new(3, 2).unwrap(),
    );
    let operation = StructuralOperation::new(
        StructuralOperationId::new("operation.delocalise").unwrap(),
        StructuralOperationInput::ChangeCovalentDelocalization {
            left: atom_id("superoxide.o1"),
            right: atom_id("superoxide.o2"),
            expected: None,
            replacement: Some(state.clone()),
        },
    )
    .expect("localised-to-delocalised operation should be valid");
    assert!(matches!(
        operation.view(),
        StructuralOperationView::ChangeCovalentDelocalization {
            expected: None,
            replacement: Some(actual),
            ..
        } if actual == &state
    ));

    assert_eq!(
        StructuralOperation::new(
            StructuralOperationId::new("operation.unchanged-delocalisation").unwrap(),
            StructuralOperationInput::ChangeCovalentDelocalization {
                left: atom_id("superoxide.o1"),
                right: atom_id("superoxide.o2"),
                expected: Some(state.clone()),
                replacement: Some(state),
            },
        ),
        Err(StructuralError::UnchangedCovalentDelocalization)
    );
}

#[test]
fn metallic_release_can_localise_multiple_valence_electrons() {
    let site = atom_id("magnesium.metal");
    let operation = StructuralOperation::new(
        StructuralOperationId::new("operation.release-magnesium").unwrap(),
        StructuralOperationInput::ReleaseMetallic {
            site: site.clone(),
            domain: MetallicDomainId::new("magnesium.domain").unwrap(),
            allocation: MetallicReleaseAllocation::RetainElectron,
            transition: ElectronTransition::new(
                site,
                ElectronState::new(2, 0, 0).unwrap(),
                ElectronState::new(0, 2, 2).unwrap(),
            ),
            domain_electrons_before: 2,
            domain_electrons_after: 0,
        },
    );
    assert!(operation.is_ok());
}

fn electron_state_fixture(value: &Value) -> ElectronState {
    let values = value
        .as_array()
        .expect("electron-state fixture should be an array");
    ElectronState::new(
        i16::try_from(values[0].as_i64().unwrap()).unwrap(),
        u8::try_from(values[1].as_u64().unwrap()).unwrap(),
        u8::try_from(values[2].as_u64().unwrap()).unwrap(),
    )
    .expect("electron-state fixture should be valid")
}

fn graph(
    atoms: Vec<Atom>,
    bonds: Vec<CovalentBond>,
    groups: Vec<AtomGroup>,
    associations: Vec<IonicAssociation>,
    domains: Vec<MetallicDomain>,
) -> Result<StructuralGraph, StructuralError> {
    StructuralGraph::new(atoms, bonds, groups, associations, domains)
}

fn fixture(path: &str) -> Value {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    serde_json::from_slice(
        &fs::read(root.join(path)).unwrap_or_else(|error| panic!("could not read {path}: {error}")),
    )
    .unwrap_or_else(|error| panic!("could not parse {path}: {error}"))
}

fn fixture_atoms(input: &Value) -> Vec<Atom> {
    input["atoms"]
        .as_array()
        .expect("fixture atoms should be an array")
        .iter()
        .map(|value| {
            atom(
                value["id"].as_str().expect("atom ID should be a string"),
                value["element"]
                    .as_str()
                    .expect("element should be a string"),
                i16::try_from(value["formal_charge"].as_i64().unwrap_or(0)).unwrap(),
                u8::try_from(value["non_bonding_electrons"].as_u64().unwrap_or(0)).unwrap(),
                u8::try_from(value["unpaired_electrons"].as_u64().unwrap_or(0)).unwrap(),
            )
        })
        .collect()
}

fn fixture_graph(input: &Value) -> StructuralGraph {
    let bonds = input["covalent_edges"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|value| {
            let order = match value["order"].as_str() {
                Some("single") => BondOrder::Single,
                Some("double") => BondOrder::Double,
                Some("triple") => BondOrder::Triple,
                order => panic!("unexpected fixture bond order {order:?}"),
            };
            bond(
                value["id"].as_str().unwrap(),
                value["left"].as_str().unwrap(),
                value["right"].as_str().unwrap(),
                order,
            )
        })
        .collect::<Vec<_>>();
    let groups = input["groups"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|value| {
            AtomGroup::new(
                group_id(value["id"].as_str().unwrap()),
                value["atoms"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|atom| atom_id(atom.as_str().unwrap())),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    let associations = input["ionic_associations"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|value| {
            IonicAssociation::new(
                IonicAssociationId::new(value["id"].as_str().unwrap()).unwrap(),
                value["components"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|group| group_id(group.as_str().unwrap())),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    let domains = input["metallic_domains"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|value| {
            MetallicDomain::new(
                MetallicDomainId::new(value["id"].as_str().unwrap()).unwrap(),
                value["sites"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|site| atom_id(site.as_str().unwrap())),
                u32::try_from(value["delocalized_electrons"].as_u64().unwrap()).unwrap(),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    graph(fixture_atoms(input), bonds, groups, associations, domains)
        .expect("structural fixture should construct")
}

fn fixture_inventory(input: &Value) -> ElementInventory {
    ElementInventory::new(
        input["formula"]
            .as_object()
            .expect("fixture formula should be an object")
            .iter()
            .map(|(element, count)| {
                (
                    ElementSymbol::new(element).unwrap(),
                    count.as_u64().unwrap(),
                )
            }),
    )
    .unwrap()
}

fn fixture_isomer_graph(input: &Value, name: &str) -> StructuralGraph {
    graph(
        fixture_atoms(input),
        input["graphs"][name]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(index, endpoints)| {
                bond(
                    &format!("fixture.b{index}"),
                    endpoints[0].as_str().unwrap(),
                    endpoints[1].as_str().unwrap(),
                    BondOrder::Single,
                )
            })
            .collect(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )
    .unwrap()
}

fn water(atom_order: &[usize], bond_order: &[usize]) -> StructuralGraph {
    let atoms = [
        atom("water.o", "O", 0, 4, 0),
        atom("water.h1", "H", 0, 0, 0),
        atom("water.h2", "H", 0, 0, 0),
    ];
    let bonds = [
        bond("water.oh1", "water.o", "water.h1", BondOrder::Single),
        bond("water.oh2", "water.o", "water.h2", BondOrder::Single),
    ];
    graph(
        atom_order
            .iter()
            .map(|index| atoms[*index].clone())
            .collect(),
        bond_order
            .iter()
            .map(|index| bonds[*index].clone())
            .collect(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )
    .expect("water graph should validate")
}

#[test]
fn local_electron_pairing_is_constructed_only_when_consistent() {
    assert_eq!(BondOrder::Single.order(), 1);
    assert_eq!(BondOrder::Double.electrons(), 4);
    assert_eq!(BondOrder::Triple.electrons(), 6);
    assert!(ElectronState::new(0, 4, 0).is_ok());
    assert!(ElectronState::new(0, 3, 1).is_ok());
    assert_eq!(
        ElectronState::new(0, 1, 0),
        Err(StructuralError::InvalidUnpairedElectrons {
            non_bonding_electrons: 1,
            unpaired_electrons: 0,
        })
    );
    assert!(matches!(
        ElectronState::new(0, 2, 3),
        Err(StructuralError::InvalidUnpairedElectrons { .. })
    ));
}

#[test]
fn graph_order_is_semantically_irrelevant_and_serialization_is_stable() {
    let canonical = water(&[0, 1, 2], &[0, 1]);
    let atom_permutations = [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];
    for atom_order in atom_permutations {
        for bond_order in [[0, 1], [1, 0]] {
            let permuted = water(&atom_order, &bond_order);
            assert_eq!(permuted, canonical);
            assert_eq!(
                permuted.canonical_json().unwrap(),
                canonical.canonical_json().unwrap()
            );
            assert_eq!(permuted.digest().unwrap(), canonical.digest().unwrap());
        }
    }
    assert_eq!(canonical.explicit_valence_electron_count(), 8);
    assert_eq!(canonical.system_net_charge(), 0);
}

#[test]
fn invalid_and_duplicate_covalent_edges_never_form_a_graph() {
    assert_eq!(
        CovalentBond::new(
            bond_id("self"),
            atom_id("a"),
            atom_id("a"),
            BondOrder::Single,
        ),
        Err(StructuralError::SelfBond(atom_id("a")))
    );

    let atoms = vec![atom("a", "H", 0, 0, 0), atom("b", "H", 0, 0, 0)];
    let duplicate = graph(
        atoms.clone(),
        vec![
            bond("first", "a", "b", BondOrder::Single),
            bond("second", "b", "a", BondOrder::Double),
        ],
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );
    assert_eq!(
        duplicate,
        Err(StructuralError::DuplicateCovalentEdge(
            atom_id("a"),
            atom_id("b"),
        ))
    );

    let unknown = graph(
        atoms,
        vec![bond("unknown", "a", "missing", BondOrder::Single)],
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );
    assert_eq!(
        unknown,
        Err(StructuralError::UnknownAtom(atom_id("missing")))
    );
}

#[test]
fn groups_expand_deterministically_and_ionic_associations_are_not_bonds() {
    let lithium = atom("salt.li", "Li", 1, 0, 0);
    let oxygen = atom("salt.o", "O", -1, 6, 0);
    let hydrogen = atom("salt.h", "H", 0, 0, 0);
    let lithium_group = AtomGroup::new(group_id("salt.lithium"), [lithium.id().clone()]).unwrap();
    let hydroxide_group = AtomGroup::new(
        group_id("salt.hydroxide"),
        [hydrogen.id().clone(), oxygen.id().clone()],
    )
    .unwrap();
    let association = IonicAssociation::new(
        IonicAssociationId::new("salt.ionic").unwrap(),
        [lithium_group.id().clone(), hydroxide_group.id().clone()],
    )
    .unwrap();
    let salt = graph(
        vec![hydrogen, lithium, oxygen],
        vec![bond("salt.oh", "salt.o", "salt.h", BondOrder::Single)],
        vec![hydroxide_group.clone(), lithium_group.clone()],
        vec![association],
        Vec::new(),
    )
    .expect("lithium hydroxide should validate");

    assert_eq!(
        salt.groups()[hydroxide_group.id()].atoms(),
        &BTreeSet::from([atom_id("salt.h"), atom_id("salt.o")])
    );
    assert_eq!(salt.covalent_bonds().len(), 1);
    assert_eq!(salt.ionic_associations().len(), 1);
    assert_eq!(salt.system_net_charge(), 0);
    assert_eq!(salt.explicit_valence_electron_count(), 8);
}

#[test]
fn incompatible_ionic_components_are_rejected() {
    let lithium_a = atom("li.a", "Li", 1, 0, 0);
    let lithium_b = atom("li.b", "Li", 1, 0, 0);
    let group_a = AtomGroup::new(group_id("group.a"), [lithium_a.id().clone()]).unwrap();
    let group_b = AtomGroup::new(group_id("group.b"), [lithium_b.id().clone()]).unwrap();
    let association = IonicAssociation::new(
        IonicAssociationId::new("association").unwrap(),
        [group_a.id().clone(), group_b.id().clone()],
    )
    .unwrap();
    assert_eq!(
        graph(
            vec![lithium_a, lithium_b],
            Vec::new(),
            vec![group_a, group_b],
            vec![association],
            Vec::new(),
        ),
        Err(StructuralError::NonNeutralIonicAssociation {
            association: IonicAssociationId::new("association").unwrap(),
            charge: 2,
        })
    );
}

#[test]
fn empty_and_repeated_memberships_are_rejected_at_construction() {
    assert_eq!(
        AtomGroup::new(group_id("empty"), []),
        Err(StructuralError::EmptyGroup(group_id("empty")))
    );
    assert_eq!(
        AtomGroup::new(group_id("repeat"), [atom_id("a"), atom_id("a")]),
        Err(StructuralError::DuplicateGroupAtom {
            group: group_id("repeat"),
            atom: atom_id("a"),
        })
    );
    assert_eq!(
        IonicAssociation::new(
            IonicAssociationId::new("repeat").unwrap(),
            [
                group_id("positive"),
                group_id("negative"),
                group_id("positive")
            ],
        ),
        Err(StructuralError::DuplicateIonicComponent {
            association: IonicAssociationId::new("repeat").unwrap(),
            component: group_id("positive"),
        })
    );
    assert_eq!(
        MetallicDomain::new(
            MetallicDomainId::new("repeat").unwrap(),
            [atom_id("a"), atom_id("a")],
            2,
        ),
        Err(StructuralError::DuplicateMetallicSite {
            domain: MetallicDomainId::new("repeat").unwrap(),
            site: atom_id("a"),
        })
    );
}

#[test]
fn metallic_domains_own_electrons_exactly_once() {
    let first = atom("metal.li1", "Li", 1, 0, 0);
    let second = atom("metal.li2", "Li", 1, 0, 0);
    let domain = MetallicDomain::new(
        MetallicDomainId::new("metal.domain").unwrap(),
        [first.id().clone(), second.id().clone()],
        2,
    )
    .unwrap();
    let metal = graph(
        vec![second, first],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        vec![domain],
    )
    .expect("neutral lithium fragment should validate");
    assert_eq!(metal.atom_formal_charge_sum(), 2);
    assert_eq!(metal.delocalized_domain_electron_count(), 2);
    assert_eq!(metal.system_net_charge(), 0);
    assert_eq!(metal.explicit_valence_electron_count(), 2);

    let local = atom("metal.local", "Li", 0, 1, 1);
    let invalid_domain = MetallicDomain::new(
        MetallicDomainId::new("metal.invalid").unwrap(),
        [local.id().clone()],
        1,
    )
    .unwrap();
    assert_eq!(
        graph(
            vec![local],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![invalid_domain],
        ),
        Err(StructuralError::MetallicSiteHasLocalElectrons(atom_id(
            "metal.local"
        )))
    );

    let neutral_core = atom("metal.neutral", "Li", 0, 0, 0);
    let neutral_domain = MetallicDomain::new(
        MetallicDomainId::new("metal.neutral-domain").unwrap(),
        [neutral_core.id().clone()],
        1,
    )
    .unwrap();
    let neutral_graph = graph(
        vec![neutral_core],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        vec![neutral_domain],
    )
    .unwrap();
    assert!(matches!(
        StructureDefinition::new(
            StructureId::new("InvalidMetal").unwrap(),
            inventory(&[("Li", 1)]),
            RepresentationKind::Metallic,
            neutral_graph,
        ),
        Err(StructuralError::RepresentationMismatch { .. })
    ));
}

#[test]
fn equal_formula_inventories_do_not_equalize_structural_isomers() {
    let input = fixture("conformance/structural-domain/graph-identity-001.input.json");
    let formula = fixture_inventory(&input);
    let ethanol_graph = fixture_isomer_graph(&input, "alcohol");
    let ether_graph = fixture_isomer_graph(&input, "ether");
    let ethanol = StructureDefinition::new(
        StructureId::new("Ethanol").unwrap(),
        formula.clone(),
        RepresentationKind::Molecular,
        ethanol_graph,
    )
    .unwrap();
    let ether = StructureDefinition::new(
        StructureId::new("DimethylEther").unwrap(),
        formula,
        RepresentationKind::Molecular,
        ether_graph,
    )
    .unwrap();

    assert_eq!(ethanol.formula(), ether.formula());
    assert_ne!(ethanol.graph(), ether.graph());
    assert_ne!(ethanol, ether);
}

#[test]
fn formula_inventory_must_equal_the_complete_graph_inventory() {
    assert_eq!(
        StructureDefinition::new(
            StructureId::new("WrongWater").unwrap(),
            inventory(&[("H", 2), ("O", 2)]),
            RepresentationKind::Molecular,
            water(&[0, 1, 2], &[0, 1]),
        ),
        Err(StructuralError::FormulaGraphMismatch(
            StructureId::new("WrongWater").unwrap()
        ))
    );
}

#[test]
fn representation_kinds_require_complete_relationship_coverage() {
    let lithium = atom("mixed.li", "Li", 1, 0, 0);
    let chloride = atom("mixed.cl", "Cl", -1, 8, 0);
    let spectator = atom("mixed.h", "H", 0, 0, 0);
    let cation = AtomGroup::new(group_id("mixed.cation"), [lithium.id().clone()]).unwrap();
    let anion = AtomGroup::new(group_id("mixed.anion"), [chloride.id().clone()]).unwrap();
    let association = IonicAssociation::new(
        IonicAssociationId::new("mixed.ionic").unwrap(),
        [cation.id().clone(), anion.id().clone()],
    )
    .unwrap();
    let incomplete_ionic = graph(
        vec![lithium, chloride, spectator],
        Vec::new(),
        vec![cation, anion],
        vec![association],
        Vec::new(),
    )
    .unwrap();
    assert!(matches!(
        StructureDefinition::new(
            StructureId::new("IncompleteIonic").unwrap(),
            inventory(&[("Cl", 1), ("H", 1), ("Li", 1)]),
            RepresentationKind::Ionic,
            incomplete_ionic,
        ),
        Err(StructuralError::RepresentationMismatch { .. })
    ));

    let site = atom("mixed.metal", "Li", 1, 0, 0);
    let unowned = atom("mixed.unowned", "H", 0, 0, 0);
    let domain = MetallicDomain::new(
        MetallicDomainId::new("mixed.domain").unwrap(),
        [site.id().clone()],
        1,
    )
    .unwrap();
    let incomplete_metal = graph(
        vec![site, unowned],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        vec![domain],
    )
    .unwrap();
    assert!(matches!(
        StructureDefinition::new(
            StructureId::new("IncompleteMetal").unwrap(),
            inventory(&[("H", 1), ("Li", 1)]),
            RepresentationKind::Metallic,
            incomplete_metal,
        ),
        Err(StructuralError::RepresentationMismatch { .. })
    ));
}

#[test]
fn instances_are_definition_derived_with_total_canonical_relabeling() {
    let template = water(&[0, 1, 2], &[0, 1]);
    let definition = StructureDefinition::new(
        StructureId::new("Water").unwrap(),
        inventory(&[("H", 2), ("O", 1)]),
        RepresentationKind::Molecular,
        template,
    )
    .unwrap();
    let entries = vec![
        (atom_id("water.o"), atom_id("reactant.water1.o")),
        (atom_id("water.h1"), atom_id("reactant.water1.h1")),
        (atom_id("water.h2"), atom_id("reactant.water1.h2")),
    ];
    let forward = StructureInstance::instantiate(
        StructureInstanceId::new("reactant.water1").unwrap(),
        &definition,
        entries.clone(),
    )
    .unwrap();
    let reverse = StructureInstance::instantiate(
        StructureInstanceId::new("reactant.water1").unwrap(),
        &definition,
        entries.iter().cloned().rev(),
    )
    .unwrap();
    assert_eq!(forward, reverse);
    assert_eq!(forward.structure(), definition.id());
    assert!(
        forward
            .graph()
            .covalent_bonds()
            .keys()
            .all(|id| id.as_str().starts_with("reactant.water1."))
    );

    assert_eq!(
        StructureInstance::instantiate(
            StructureInstanceId::new("incomplete").unwrap(),
            &definition,
            entries[..2].iter().cloned(),
        ),
        Err(StructuralError::IncompleteInstanceRelabeling)
    );
    assert_eq!(
        StructureInstance::instantiate(
            StructureInstanceId::new("duplicate").unwrap(),
            &definition,
            [
                (atom_id("water.o"), atom_id("same")),
                (atom_id("water.h1"), atom_id("same")),
                (atom_id("water.h2"), atom_id("other")),
            ],
        ),
        Err(StructuralError::DuplicateInstanceAtom(atom_id("same")))
    );
}

fn one_atom_side(instance: &str, atom_name: &str, element: &str) -> ReactionSide {
    let graph = graph(
        vec![atom(atom_name, element, 0, 0, 0)],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    let definition = StructureDefinition::new(
        StructureId::new(format!("{element}Structure")).unwrap(),
        graph.element_inventory(),
        RepresentationKind::Molecular,
        graph,
    )
    .unwrap();
    ReactionSide::new([StructureInstance::instantiate(
        StructureInstanceId::new(instance).unwrap(),
        &definition,
        [(atom_id(atom_name), atom_id(atom_name))],
    )
    .unwrap()])
    .unwrap()
}

fn hydrogen_side(instance: &str, prefix: &str) -> ReactionSide {
    let template = graph(
        vec![
            atom("template.h1", "H", 0, 0, 0),
            atom("template.h2", "H", 0, 0, 0),
        ],
        vec![bond(
            "template.bond",
            "template.h1",
            "template.h2",
            BondOrder::Single,
        )],
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    let definition = StructureDefinition::new(
        StructureId::new("Hydrogen").unwrap(),
        template.element_inventory(),
        RepresentationKind::Molecular,
        template,
    )
    .unwrap();
    ReactionSide::new([StructureInstance::instantiate(
        StructureInstanceId::new(instance).unwrap(),
        &definition,
        [
            (atom_id("template.h1"), atom_id(&format!("{prefix}.h1"))),
            (atom_id("template.h2"), atom_id(&format!("{prefix}.h2"))),
        ],
    )
    .unwrap()])
    .unwrap()
}

#[test]
fn atom_mappings_are_total_bijective_and_element_preserving() {
    let reactants = one_atom_side("reactant.1", "reactant.h", "H");
    let products = one_atom_side("product.1", "product.h", "H");
    let mapping_id = AtomMappingId::new("mapping").unwrap();
    let mapping = AtomMapping::new(
        mapping_id.clone(),
        [(atom_id("reactant.h"), atom_id("product.h"))],
        &reactants,
        &products,
    )
    .unwrap();
    assert_eq!(mapping.id(), &mapping_id);

    assert_eq!(
        AtomMapping::new(
            AtomMappingId::new("empty").unwrap(),
            [],
            &reactants,
            &products,
        ),
        Err(StructuralError::IncompleteMappingSources)
    );
    let oxygen_products = one_atom_side("oxygen.1", "product.o", "O");
    assert_eq!(
        AtomMapping::new(
            AtomMappingId::new("element-change").unwrap(),
            [(atom_id("reactant.h"), atom_id("product.o"))],
            &reactants,
            &oxygen_products,
        ),
        Err(StructuralError::ElementChangingMapping {
            source: atom_id("reactant.h"),
            destination: atom_id("product.o"),
        })
    );
}

#[test]
fn mapping_entry_order_is_irrelevant_and_duplicate_destinations_fail() {
    let reactants = hydrogen_side("reactants.hydrogen", "reactants");
    let products = hydrogen_side("products.hydrogen", "products");
    let forward = AtomMapping::new(
        AtomMappingId::new("mapping.hydrogen").unwrap(),
        [
            (atom_id("reactants.h1"), atom_id("products.h1")),
            (atom_id("reactants.h2"), atom_id("products.h2")),
        ],
        &reactants,
        &products,
    )
    .unwrap();
    let reverse_input = AtomMapping::new(
        AtomMappingId::new("mapping.hydrogen").unwrap(),
        [
            (atom_id("reactants.h2"), atom_id("products.h2")),
            (atom_id("reactants.h1"), atom_id("products.h1")),
        ],
        &reactants,
        &products,
    )
    .unwrap();
    assert_eq!(forward, reverse_input);
    assert_eq!(
        chem_domain::canonical_structural_json(&forward).unwrap(),
        chem_domain::canonical_structural_json(&reverse_input).unwrap()
    );
    assert_eq!(
        chem_domain::structural_digest(&forward).unwrap(),
        chem_domain::structural_digest(&reverse_input).unwrap()
    );

    assert_eq!(
        AtomMapping::new(
            AtomMappingId::new("mapping.duplicate").unwrap(),
            [
                (atom_id("reactants.h1"), atom_id("products.h1")),
                (atom_id("reactants.h2"), atom_id("products.h1")),
            ],
            &reactants,
            &products,
        ),
        Err(StructuralError::DuplicateMappingDestination(atom_id(
            "products.h1"
        )))
    );
}

#[test]
fn structural_operation_values_retain_exact_endpoint_states() {
    let lithium_before = ElectronState::new(0, 1, 1).unwrap();
    let lithium_after = ElectronState::new(1, 0, 0).unwrap();
    let hydrogen_before = ElectronState::new(1, 0, 0).unwrap();
    let hydrogen_after = ElectronState::new(0, 1, 1).unwrap();
    let operation = StructuralOperation::new(
        StructuralOperationId::new("operation.5").unwrap(),
        StructuralOperationInput::TransferElectron {
            donor: atom_id("lithium.1"),
            acceptor: atom_id("water.1.h1"),
            count: 1,
            transitions: vec![
                ElectronTransition::new(atom_id("lithium.1"), lithium_before, lithium_after),
                ElectronTransition::new(atom_id("water.1.h1"), hydrogen_before, hydrogen_after),
            ],
        },
    )
    .unwrap();

    assert_eq!(operation.id().as_str(), "operation.5");
    let serialized = chem_domain::canonical_structural_json(&operation).unwrap();
    let value: Value = serde_json::from_slice(&serialized).unwrap();
    assert_eq!(value["count"], 1);
    assert_eq!(
        value["transitions"]["lithium.1"]["after"]["formal_charge"],
        1
    );
    assert_eq!(
        value["transitions"]["water.1.h1"]["after"]["unpaired_electrons"],
        1
    );
}

fn electron_transfer_transitions() -> Vec<ElectronTransition> {
    let donor_before = ElectronState::new(0, 1, 1).unwrap();
    let donor_after = ElectronState::new(1, 0, 0).unwrap();
    let acceptor_before = ElectronState::new(1, 0, 0).unwrap();
    let acceptor_after = ElectronState::new(0, 1, 1).unwrap();
    vec![
        ElectronTransition::new(atom_id("donor"), donor_before, donor_after),
        ElectronTransition::new(atom_id("acceptor"), acceptor_before, acceptor_after),
    ]
}

fn electron_transfer_input(transitions: Vec<ElectronTransition>) -> StructuralOperationInput {
    StructuralOperationInput::TransferElectron {
        donor: atom_id("donor"),
        acceptor: atom_id("acceptor"),
        count: 1,
        transitions,
    }
}

#[test]
fn structural_operation_transition_order_is_canonical() {
    let transitions = electron_transfer_transitions();
    let forward = StructuralOperation::new(
        StructuralOperationId::new("operation.transfer").unwrap(),
        electron_transfer_input(transitions.clone()),
    )
    .unwrap();
    let reverse = StructuralOperation::new(
        StructuralOperationId::new("operation.transfer").unwrap(),
        electron_transfer_input(transitions.iter().cloned().rev().collect()),
    )
    .unwrap();
    assert_eq!(forward, reverse);
    assert_eq!(
        chem_domain::canonical_structural_json(&forward).unwrap(),
        chem_domain::canonical_structural_json(&reverse).unwrap()
    );
}

#[test]
fn structural_operations_reject_invalid_shapes() {
    let transitions = electron_transfer_transitions();
    assert_eq!(
        StructuralOperation::new(
            StructuralOperationId::new("operation.zero").unwrap(),
            StructuralOperationInput::TransferElectron {
                donor: atom_id("donor"),
                acceptor: atom_id("acceptor"),
                count: 0,
                transitions: transitions.clone(),
            },
        ),
        Err(StructuralError::ZeroElectronTransfer)
    );
    assert_eq!(
        StructuralOperation::new(
            StructuralOperationId::new("operation.self").unwrap(),
            StructuralOperationInput::TransferElectron {
                donor: atom_id("donor"),
                acceptor: atom_id("donor"),
                count: 1,
                transitions: transitions.clone(),
            },
        ),
        Err(StructuralError::OperationSelfEndpoint(atom_id("donor")))
    );
    assert_eq!(
        StructuralOperation::new(
            StructuralOperationId::new("operation.duplicate").unwrap(),
            electron_transfer_input(vec![transitions[0].clone(), transitions[0].clone()]),
        ),
        Err(StructuralError::DuplicateOperationTransition(atom_id(
            "donor"
        )))
    );
    assert_eq!(
        StructuralOperation::new(
            StructuralOperationId::new("operation.order").unwrap(),
            StructuralOperationInput::ChangeCovalent {
                left: atom_id("donor"),
                right: atom_id("acceptor"),
                old_order: BondOrder::Single,
                new_order: BondOrder::Single,
                allocation: ElectronAllocation::Homolytic,
                transitions: transitions.clone(),
            },
        ),
        Err(StructuralError::UnchangedBondOrder)
    );
    assert_eq!(
        StructuralOperation::new(
            StructuralOperationId::new("operation.assignment").unwrap(),
            StructuralOperationInput::AssignProduct {
                atoms: Vec::new(),
                product: StructureInstanceId::new("product.1").unwrap(),
            },
        ),
        Err(StructuralError::EmptyProductAssignment)
    );
    assert_eq!(
        StructuralOperation::new(
            StructuralOperationId::new("operation.metal").unwrap(),
            StructuralOperationInput::ReleaseMetallic {
                site: atom_id("donor"),
                domain: MetallicDomainId::new("domain").unwrap(),
                allocation: MetallicReleaseAllocation::RetainElectron,
                transition: transitions[0].clone(),
                domain_electrons_before: 1,
                domain_electrons_after: 1,
            },
        ),
        Err(StructuralError::InvalidMetallicElectronLedger)
    );
}

#[test]
fn structural_operations_check_covalent_electron_ledgers() {
    let oxygen_before = ElectronState::new(0, 4, 0).unwrap();
    let oxygen_after = ElectronState::new(-1, 6, 0).unwrap();
    let proton_before = ElectronState::new(0, 0, 0).unwrap();
    let proton_after = ElectronState::new(1, 0, 0).unwrap();
    let cleavage = |oxygen_after| StructuralOperationInput::CleaveCovalent {
        left: atom_id("water.o"),
        right: atom_id("water.h"),
        expected_order: BondOrder::Single,
        allocation: ElectronAllocation::HeterolyticTo(atom_id("water.o")),
        transitions: vec![
            ElectronTransition::new(atom_id("water.o"), oxygen_before, oxygen_after),
            ElectronTransition::new(atom_id("water.h"), proton_before, proton_after),
        ],
    };
    assert!(
        StructuralOperation::new(
            StructuralOperationId::new("operation.cleave").unwrap(),
            cleavage(oxygen_after),
        )
        .is_ok()
    );
    assert_eq!(
        StructuralOperation::new(
            StructuralOperationId::new("operation.bad-cleave").unwrap(),
            cleavage(ElectronState::new(0, 6, 0).unwrap()),
        ),
        Err(StructuralError::InvalidCovalentElectronLedger)
    );
}

#[test]
fn dative_bonds_retain_direction_and_shared_serialization_is_compatible() {
    let forward = dative_bond("ammonium.n-h4", "ammonia.n", "proton.h");
    let reverse = dative_bond("ammonium.n-h4", "proton.h", "ammonia.n");
    assert_eq!(forward.order(), BondOrder::Single);
    assert_ne!(forward, reverse);
    assert_eq!(
        forward.electron_origin(),
        &CovalentElectronOrigin::Dative {
            donor: atom_id("ammonia.n"),
            acceptor: atom_id("proton.h"),
        }
    );
    let value: Value =
        serde_json::from_slice(&chem_domain::canonical_structural_json(&forward).unwrap()).unwrap();
    assert_eq!(value["electron_origin"], "dative");
    assert_eq!(value["donor"], "ammonia.n");
    assert_eq!(value["acceptor"], "proton.h");

    let shared = bond("water.o-h", "water.o", "water.h", BondOrder::Single);
    let shared_value: Value =
        serde_json::from_slice(&chem_domain::canonical_structural_json(&shared).unwrap()).unwrap();
    assert!(shared_value.get("electron_origin").is_none());
    assert_eq!(
        CovalentBond::new_dative(bond_id("bad"), atom_id("same"), atom_id("same")),
        Err(StructuralError::SelfBond(atom_id("same")))
    );
}

#[test]
fn dative_operations_require_a_donor_pair_and_explicit_cleavage_allocation() {
    let donor_before = ElectronState::new(0, 2, 0).unwrap();
    let donor_after = ElectronState::new(1, 0, 0).unwrap();
    let acceptor_before = ElectronState::new(1, 0, 0).unwrap();
    let acceptor_after = ElectronState::new(0, 0, 0).unwrap();
    let formation_transitions = vec![
        ElectronTransition::new(atom_id("ammonia.n"), donor_before, donor_after),
        ElectronTransition::new(atom_id("proton.h"), acceptor_before, acceptor_after),
    ];
    assert!(
        StructuralOperation::new(
            StructuralOperationId::new("operation.form-dative").unwrap(),
            StructuralOperationInput::FormDative {
                donor: atom_id("ammonia.n"),
                acceptor: atom_id("proton.h"),
                transitions: formation_transitions.clone(),
            },
        )
        .is_ok()
    );
    let no_pair = vec![
        ElectronTransition::new(
            atom_id("ammonia.n"),
            ElectronState::new(0, 2, 2).unwrap(),
            ElectronState::new(1, 0, 0).unwrap(),
        ),
        ElectronTransition::new(atom_id("proton.h"), acceptor_before, acceptor_after),
    ];
    assert_eq!(
        StructuralOperation::new(
            StructuralOperationId::new("operation.form-dative-without-pair").unwrap(),
            StructuralOperationInput::FormDative {
                donor: atom_id("ammonia.n"),
                acceptor: atom_id("proton.h"),
                transitions: no_pair,
            },
        ),
        Err(StructuralError::InvalidDativeElectronLedger)
    );

    let heterolytic_transitions = formation_transitions
        .into_iter()
        .map(|transition| {
            ElectronTransition::new(
                transition.atom().clone(),
                transition.after(),
                transition.before(),
            )
        })
        .collect::<Vec<_>>();
    assert!(
        StructuralOperation::new(
            StructuralOperationId::new("operation.cleave-dative-heterolytic").unwrap(),
            StructuralOperationInput::CleaveDative {
                donor: atom_id("ammonia.n"),
                acceptor: atom_id("proton.h"),
                allocation: ElectronAllocation::HeterolyticTo(atom_id("ammonia.n")),
                transitions: heterolytic_transitions,
            },
        )
        .is_ok()
    );
    assert!(
        StructuralOperation::new(
            StructuralOperationId::new("operation.cleave-dative-homolytic").unwrap(),
            StructuralOperationInput::CleaveDative {
                donor: atom_id("ammonia.n"),
                acceptor: atom_id("proton.h"),
                allocation: ElectronAllocation::Homolytic,
                transitions: vec![
                    ElectronTransition::new(
                        atom_id("ammonia.n"),
                        donor_after,
                        ElectronState::new(1, 1, 1).unwrap(),
                    ),
                    ElectronTransition::new(
                        atom_id("proton.h"),
                        acceptor_after,
                        ElectronState::new(0, 1, 1).unwrap(),
                    ),
                ],
            },
        )
        .is_ok()
    );
}

#[test]
fn dative_bonding_conformance_fixture_executes_exact_electron_arithmetic() {
    let input = fixture("conformance/structural-domain/dative-bonding-001.input.json");
    let expected = fixture("conformance/structural-domain/dative-bonding-001.domain.json");
    let donor = atom_id(input["bond"]["donor"].as_str().unwrap());
    let acceptor = atom_id(input["bond"]["acceptor"].as_str().unwrap());
    let dative = CovalentBond::new_dative(
        bond_id(input["bond"]["id"].as_str().unwrap()),
        donor.clone(),
        acceptor.clone(),
    )
    .unwrap();
    let donor_before = electron_state_fixture(&input["formation"]["donor_before"]);
    let donor_after = electron_state_fixture(&input["formation"]["donor_after"]);
    let acceptor_before = electron_state_fixture(&input["formation"]["acceptor_before"]);
    let acceptor_after = electron_state_fixture(&input["formation"]["acceptor_after"]);
    let transitions = vec![
        ElectronTransition::new(donor.clone(), donor_before, donor_after),
        ElectronTransition::new(acceptor.clone(), acceptor_before, acceptor_after),
    ];
    let formation_valid = StructuralOperation::new(
        StructuralOperationId::new("operation.fixture-form-dative").unwrap(),
        StructuralOperationInput::FormDative {
            donor: donor.clone(),
            acceptor: acceptor.clone(),
            transitions: transitions.clone(),
        },
    )
    .is_ok();
    let cleavage_valid = StructuralOperation::new(
        StructuralOperationId::new("operation.fixture-cleave-dative").unwrap(),
        StructuralOperationInput::CleaveDative {
            donor: donor.clone(),
            acceptor: acceptor.clone(),
            allocation: ElectronAllocation::HeterolyticTo(donor.clone()),
            transitions: transitions
                .into_iter()
                .map(|transition| {
                    ElectronTransition::new(
                        transition.atom().clone(),
                        transition.after(),
                        transition.before(),
                    )
                })
                .collect(),
        },
    )
    .is_ok();
    let before_electrons = u16::from(donor_before.non_bonding_electrons())
        + u16::from(acceptor_before.non_bonding_electrons());
    let after_electrons = u16::from(donor_after.non_bonding_electrons())
        + u16::from(acceptor_after.non_bonding_electrons())
        + 2 * u16::from(dative.order().order());
    assert_eq!(
        serde_json::json!({
            "bond_order": "single",
            "electron_origin": "dative",
            "donor": donor,
            "acceptor": acceptor,
            "formation_valid": formation_valid,
            "cleavage_valid": cleavage_valid,
            "explicit_electrons_conserved": before_electrons == after_electrons
        }),
        expected
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn property_graph_canonicalization_and_serialization_are_order_independent(
        atom_keys in any::<[u8; 3]>(),
        bond_keys in any::<[u8; 2]>(),
    ) {
        let mut atom_order = [0_usize, 1, 2];
        atom_order.sort_by_key(|index| (atom_keys[*index], *index));
        let mut bond_order = [0_usize, 1];
        bond_order.sort_by_key(|index| (bond_keys[*index], *index));
        let canonical = water(&[0, 1, 2], &[0, 1]);
        let generated = water(&atom_order, &bond_order);

        prop_assert_eq!(&generated, &canonical);
        prop_assert_eq!(generated.canonical_json().unwrap(), canonical.canonical_json().unwrap());
        prop_assert_eq!(generated.digest().unwrap(), canonical.digest().unwrap());
    }

    #[test]
    fn property_mapping_bijection_and_serialization_are_canonical(
        swap_destinations in any::<bool>(),
        reverse_input in any::<bool>(),
    ) {
        let reactants = hydrogen_side("reactants.property", "property.reactants");
        let products = hydrogen_side("products.property", "property.products");
        let mut entries = if swap_destinations {
            vec![
                (atom_id("property.reactants.h1"), atom_id("property.products.h2")),
                (atom_id("property.reactants.h2"), atom_id("property.products.h1")),
            ]
        } else {
            vec![
                (atom_id("property.reactants.h1"), atom_id("property.products.h1")),
                (atom_id("property.reactants.h2"), atom_id("property.products.h2")),
            ]
        };
        let canonical_entries = entries.clone();
        if reverse_input {
            entries.reverse();
        }
        let generated = AtomMapping::new(
            AtomMappingId::new("property.mapping").unwrap(),
            entries,
            &reactants,
            &products,
        ).unwrap();
        let canonical = AtomMapping::new(
            AtomMappingId::new("property.mapping").unwrap(),
            canonical_entries,
            &reactants,
            &products,
        ).unwrap();
        prop_assert_eq!(&generated, &canonical);
        prop_assert_eq!(
            chem_domain::canonical_structural_json(&generated).unwrap(),
            chem_domain::canonical_structural_json(&canonical).unwrap(),
        );
    }

    #[test]
    fn property_group_expansion_is_set_ordered(
        members in proptest::collection::btree_set(0_u8..32, 1..16),
        reverse_input in any::<bool>(),
    ) {
        let mut input = members
            .iter()
            .map(|member| atom_id(&format!("group.h{member}")))
            .collect::<Vec<_>>();
        if reverse_input {
            input.reverse();
        }
        let group = AtomGroup::new(group_id("property.group"), input).unwrap();
        let expected = members
            .iter()
            .map(|member| atom_id(&format!("group.h{member}")))
            .collect::<BTreeSet<_>>();
        prop_assert_eq!(group.atoms(), &expected);
    }

    #[test]
    fn property_metallic_charge_and_electron_accounting(site_count in 1_usize..64) {
        let atoms = (0..site_count)
            .map(|index| atom(&format!("metal.li{index}"), "Li", 1, 0, 0))
            .collect::<Vec<_>>();
        let sites = atoms.iter().map(|atom| atom.id().clone()).collect::<Vec<_>>();
        let electron_count = u32::try_from(site_count).unwrap();
        let domain = MetallicDomain::new(
            MetallicDomainId::new("property.metal").unwrap(),
            sites,
            electron_count,
        ).unwrap();
        let graph = graph(atoms, Vec::new(), Vec::new(), Vec::new(), vec![domain]).unwrap();

        prop_assert_eq!(graph.atom_formal_charge_sum(), i64::try_from(site_count).unwrap());
        prop_assert_eq!(graph.delocalized_domain_electron_count(), u64::try_from(site_count).unwrap());
        prop_assert_eq!(graph.system_net_charge(), 0);
        prop_assert_eq!(graph.explicit_valence_electron_count(), u64::try_from(site_count).unwrap());
    }
}

#[test]
fn electron_model_conformance_fixture_satisfies_exact_arithmetic() {
    let input = fixture("conformance/structural-domain/electron-model-001.input.json");
    for case in input["cases"].as_array().unwrap() {
        let state = ElectronState::new(
            i16::try_from(case["formal_charge"].as_i64().unwrap()).unwrap(),
            u8::try_from(case["non_bonding_electrons"].as_u64().unwrap()).unwrap(),
            u8::try_from(case["unpaired_electrons"].as_u64().unwrap()).unwrap(),
        )
        .unwrap();
        assert!(
            state.formal_charge_matches(
                u8::try_from(case["neutral_valence_electrons"].as_u64().unwrap()).unwrap(),
                case["covalent_bond_order_sum"].as_u64().unwrap(),
            ),
            "formal-charge mismatch in {}",
            case["id"]
        );
    }
    let expected = fixture("conformance/structural-domain/electron-model-001.domain.json");
    assert_eq!(expected["all_cases_consistent"], true);
    assert_eq!(
        expected["metallic_site_and_domain_ownership_disjoint"],
        true
    );
}

#[test]
fn graph_identity_conformance_fixture_distinguishes_isomers() {
    let input = fixture("conformance/structural-domain/graph-identity-001.input.json");
    let formula = fixture_inventory(&input);
    let alcohol = fixture_isomer_graph(&input, "alcohol");
    let ether = fixture_isomer_graph(&input, "ether");
    let expected = fixture("conformance/structural-domain/graph-identity-001.domain.json");
    assert_eq!(
        alcohol == ether,
        expected["graphs_equal"].as_bool().unwrap()
    );
    assert_eq!(formula, alcohol.element_inventory());
    assert_eq!(formula, ether.element_inventory());
    assert_eq!(expected["same_formula_inventory"], true);
    for graph in [&alcohol, &ether] {
        for atom in graph.atoms().values() {
            let neutral_valence = match atom.element().as_str() {
                "C" => 4,
                "H" => 1,
                "O" => 6,
                element => panic!("unexpected fixture element {element}"),
            };
            assert!(atom.electrons().formal_charge_matches(
                neutral_valence,
                graph.covalent_bond_order_sum(atom.id()).unwrap(),
            ));
        }
    }
}

#[test]
fn ionic_and_metallic_conformance_fixtures_preserve_distinct_ownership() {
    let ionic = fixture_graph(&fixture(
        "conformance/structural-domain/ionic-structure-001.input.json",
    ));
    let ionic_expected = fixture("conformance/structural-domain/ionic-structure-001.domain.json");
    assert_eq!(
        ionic.atom_formal_charge_sum(),
        ionic_expected["atom_formal_charge_sum"]
    );
    assert_eq!(
        ionic.system_net_charge(),
        i128::from(ionic_expected["system_net_charge"].as_i64().unwrap())
    );
    assert_eq!(
        ionic.explicit_valence_electron_count(),
        ionic_expected["explicit_valence_electrons"]
    );
    assert_eq!(
        ionic.covalent_bonds().len(),
        usize::try_from(ionic_expected["covalent_edge_count"].as_u64().unwrap()).unwrap()
    );
    assert_eq!(
        ionic.ionic_associations().len(),
        usize::try_from(ionic_expected["ionic_association_count"].as_u64().unwrap(),).unwrap()
    );

    let metallic = fixture_graph(&fixture(
        "conformance/structural-domain/metallic-structure-001.input.json",
    ));
    let metallic_expected =
        fixture("conformance/structural-domain/metallic-structure-001.domain.json");
    assert_eq!(
        metallic.atom_formal_charge_sum(),
        metallic_expected["atom_formal_charge_sum"]
    );
    assert_eq!(
        metallic.delocalized_domain_electron_count(),
        metallic_expected["delocalized_domain_electrons"]
    );
    assert_eq!(metallic.system_net_charge(), 0);
    assert_eq!(metallic.explicit_valence_electron_count(), 2);
}
