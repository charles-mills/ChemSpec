use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::Serialize;

use crate::identity::IdKind;
use crate::{
    AtomGroupId, AtomId, AtomMappingId, CanonicalJsonError, ContentDigest, CovalentBondId,
    CovalentDelocalizationId, DeclaredId, ElementSymbol, IonicAssociationId, MetallicDomainId,
    StructuralOperationId, StructureId, StructureInstanceId, canonical_json,
};

/// Exact atom-local electron state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct ElectronState {
    formal_charge: i16,
    non_bonding_electrons: u8,
    unpaired_electrons: u8,
}

impl ElectronState {
    /// Constructs an atom-local electron state.
    ///
    /// # Errors
    ///
    /// Returns [`StructuralError::InvalidUnpairedElectrons`] when the unpaired
    /// count exceeds the local count or leaves an odd number of electrons to
    /// be paired.
    pub fn new(
        formal_charge: i16,
        non_bonding_electrons: u8,
        unpaired_electrons: u8,
    ) -> Result<Self, StructuralError> {
        if unpaired_electrons > non_bonding_electrons
            || !(non_bonding_electrons - unpaired_electrons).is_multiple_of(2)
        {
            return Err(StructuralError::InvalidUnpairedElectrons {
                non_bonding_electrons,
                unpaired_electrons,
            });
        }
        Ok(Self {
            formal_charge,
            non_bonding_electrons,
            unpaired_electrons,
        })
    }

    #[must_use]
    pub const fn formal_charge(self) -> i16 {
        self.formal_charge
    }

    #[must_use]
    pub const fn non_bonding_electrons(self) -> u8 {
        self.non_bonding_electrons
    }

    #[must_use]
    pub const fn unpaired_electrons(self) -> u8 {
        self.unpaired_electrons
    }

    #[must_use]
    pub fn expected_formal_charge(
        self,
        neutral_valence_electrons: u8,
        covalent_bond_order_sum: u64,
    ) -> i128 {
        i128::from(neutral_valence_electrons)
            - i128::from(self.non_bonding_electrons)
            - i128::from(covalent_bond_order_sum)
    }

    #[must_use]
    pub fn formal_charge_matches(
        self,
        neutral_valence_electrons: u8,
        covalent_bond_order_sum: u64,
    ) -> bool {
        i128::from(self.formal_charge)
            == self.expected_formal_charge(neutral_valence_electrons, covalent_bond_order_sum)
    }
}

/// Stable atom identity plus its element and local electron state.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct Atom {
    id: AtomId,
    element: ElementSymbol,
    electrons: ElectronState,
    #[serde(skip_serializing_if = "Option::is_none")]
    chirality: Option<TetrahedralChirality>,
}

/// Tetrahedral handedness viewed from the first listed neighbour toward
/// the centre: the remaining three run counterclockwise (SMILES `@`) or
/// clockwise (`@@`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TetrahedralHandedness {
    Counterclockwise,
    Clockwise,
}

/// A tetrahedral stereocentre: the four neighbours (explicit hydrogens
/// included — the graph model always materializes them) in the order the
/// handedness is defined against. Absent for the overwhelmingly common
/// achiral case, so existing serializations and digests are untouched.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct TetrahedralChirality {
    neighbours: [AtomId; 4],
    handedness: TetrahedralHandedness,
}

impl TetrahedralChirality {
    /// # Errors
    ///
    /// Returns [`StructuralError::InvalidChirality`] for duplicate
    /// neighbours.
    pub fn new(
        neighbours: [AtomId; 4],
        handedness: TetrahedralHandedness,
    ) -> Result<Self, StructuralError> {
        for (index, neighbour) in neighbours.iter().enumerate() {
            if neighbours[..index].contains(neighbour) {
                return Err(StructuralError::InvalidChirality(neighbour.clone()));
            }
        }
        Ok(Self {
            neighbours,
            handedness,
        })
    }

    #[must_use]
    pub const fn neighbours(&self) -> &[AtomId; 4] {
        &self.neighbours
    }

    #[must_use]
    pub const fn handedness(&self) -> TetrahedralHandedness {
        self.handedness
    }
}

impl Atom {
    #[must_use]
    pub const fn new(id: AtomId, element: ElementSymbol, electrons: ElectronState) -> Self {
        Self {
            id,
            element,
            electrons,
            chirality: None,
        }
    }

    /// The same atom carrying a tetrahedral stereocentre descriptor. The
    /// graph constructor validates the neighbours against real bonds.
    #[must_use]
    pub fn with_chirality(mut self, chirality: TetrahedralChirality) -> Self {
        self.chirality = Some(chirality);
        self
    }

    #[must_use]
    pub const fn id(&self) -> &AtomId {
        &self.id
    }

    #[must_use]
    pub const fn element(&self) -> &ElementSymbol {
        &self.element
    }

    #[must_use]
    pub const fn electrons(&self) -> ElectronState {
        self.electrons
    }

    #[must_use]
    pub const fn chirality(&self) -> Option<&TetrahedralChirality> {
        self.chirality.as_ref()
    }
}

/// Closed localized covalent bond-order domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BondOrder {
    Single,
    Double,
    Triple,
}

/// Reduced rational effective bond order for a delocalised covalent edge.
///
/// Localized electron accounting continues to use [`BondOrder`]. This value
/// records the experimentally meaningful average over equivalent resonance
/// contributors, such as `3/2` for the oxygen-oxygen edge in superoxide.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct EffectiveBondOrder {
    numerator: u8,
    denominator: u8,
}

impl EffectiveBondOrder {
    /// Constructs a reduced positive rational order no greater than three.
    ///
    /// # Errors
    ///
    /// Rejects zero, unreduced, integral, or greater-than-triple values.
    pub const fn new(numerator: u8, denominator: u8) -> Result<Self, StructuralError> {
        if numerator == 0
            || denominator == 0
            || numerator.is_multiple_of(denominator)
            || numerator > denominator.saturating_mul(3)
            || gcd(numerator, denominator) != 1
        {
            return Err(StructuralError::InvalidEffectiveBondOrder {
                numerator,
                denominator,
            });
        }
        Ok(Self {
            numerator,
            denominator,
        })
    }

    #[must_use]
    pub const fn numerator(self) -> u8 {
        self.numerator
    }

    #[must_use]
    pub const fn denominator(self) -> u8 {
        self.denominator
    }
}

const fn gcd(mut left: u8, mut right: u8) -> u8 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

/// Typed resonance/delocalisation annotation attached to a localized Lewis
/// contributor. It changes displayed effective order without inventing
/// fractional electrons or weakening integral conservation proofs.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct CovalentDelocalization {
    domain: CovalentDelocalizationId,
    effective_order: EffectiveBondOrder,
}

impl CovalentDelocalization {
    #[must_use]
    pub const fn new(
        domain: CovalentDelocalizationId,
        effective_order: EffectiveBondOrder,
    ) -> Self {
        Self {
            domain,
            effective_order,
        }
    }

    #[must_use]
    pub const fn domain(&self) -> &CovalentDelocalizationId {
        &self.domain
    }

    #[must_use]
    pub const fn effective_order(&self) -> EffectiveBondOrder {
        self.effective_order
    }
}

impl BondOrder {
    #[must_use]
    pub const fn order(self) -> u8 {
        match self {
            Self::Single => 1,
            Self::Double => 2,
            Self::Triple => 3,
        }
    }

    #[must_use]
    pub const fn electrons(self) -> u8 {
        self.order() * 2
    }
}

/// Electron-origin annotation for a localized covalent edge.
///
/// A dative edge remains a single covalent bond. Its directed annotation
/// records that both forming electrons originated at the donor; it is
/// explanatory provenance, not a fourth bond order.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(tag = "electron_origin", rename_all = "snake_case")]
pub enum CovalentElectronOrigin {
    Shared,
    Dative { donor: AtomId, acceptor: AtomId },
}

impl CovalentElectronOrigin {
    #[must_use]
    pub const fn is_shared(&self) -> bool {
        matches!(self, Self::Shared)
    }
}

/// Canonical normalized element inventory used as a structural formula
/// summary. It records composition only and never substitutes for graph
/// identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ElementInventory {
    elements: BTreeMap<ElementSymbol, u64>,
}

impl ElementInventory {
    /// Constructs a nonempty normalized inventory.
    ///
    /// # Errors
    ///
    /// Rejects zero counts and duplicate element entries.
    pub fn new(
        elements: impl IntoIterator<Item = (ElementSymbol, u64)>,
    ) -> Result<Self, StructuralError> {
        let mut element_map = BTreeMap::new();
        for (element, count) in elements {
            if count == 0 {
                return Err(StructuralError::ZeroElementCount(element));
            }
            if element_map.insert(element.clone(), count).is_some() {
                return Err(StructuralError::DuplicateElement(element));
            }
        }
        if element_map.is_empty() {
            return Err(StructuralError::EmptyElementInventory);
        }
        Ok(Self {
            elements: element_map,
        })
    }

    #[must_use]
    pub const fn elements(&self) -> &BTreeMap<ElementSymbol, u64> {
        &self.elements
    }
}

/// A localized covalent edge between two distinct atoms.
/// Relative geometry across a double bond: one named substituent on each
/// end sits on the same side (cis) or opposite sides (trans). Absent for
/// bonds whose geometry is unspecified — the overwhelmingly common case —
/// so existing canonical serializations and digests are untouched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StereoArrangement {
    Cis,
    Trans,
}

/// The reference substituents anchoring a double bond's arrangement.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct DoubleBondStereo {
    left_reference: AtomId,
    right_reference: AtomId,
    arrangement: StereoArrangement,
}

impl DoubleBondStereo {
    #[must_use]
    pub const fn left_reference(&self) -> &AtomId {
        &self.left_reference
    }

    #[must_use]
    pub const fn right_reference(&self) -> &AtomId {
        &self.right_reference
    }

    #[must_use]
    pub const fn arrangement(&self) -> StereoArrangement {
        self.arrangement
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct CovalentBond {
    id: CovalentBondId,
    left: AtomId,
    right: AtomId,
    order: BondOrder,
    #[serde(flatten, skip_serializing_if = "CovalentElectronOrigin::is_shared")]
    electron_origin: CovalentElectronOrigin,
    #[serde(skip_serializing_if = "Option::is_none")]
    delocalization: Option<CovalentDelocalization>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stereo: Option<DoubleBondStereo>,
}

impl CovalentBond {
    /// Constructs a canonical undirected bond.
    ///
    /// # Errors
    ///
    /// Returns [`StructuralError::SelfBond`] when both endpoints are equal.
    pub fn new(
        id: CovalentBondId,
        left: AtomId,
        right: AtomId,
        order: BondOrder,
    ) -> Result<Self, StructuralError> {
        if left == right {
            return Err(StructuralError::SelfBond(left));
        }
        let (left, right) = if left < right {
            (left, right)
        } else {
            (right, left)
        };
        Ok(Self {
            id,
            left,
            right,
            order,
            electron_origin: CovalentElectronOrigin::Shared,
            delocalization: None,
            stereo: None,
        })
    }

    /// Constructs a directed dative single bond from donor to acceptor.
    ///
    /// # Errors
    ///
    /// Returns [`StructuralError::SelfBond`] when both endpoints are equal.
    pub fn new_dative(
        id: CovalentBondId,
        donor: AtomId,
        acceptor: AtomId,
    ) -> Result<Self, StructuralError> {
        if donor == acceptor {
            return Err(StructuralError::SelfBond(donor));
        }
        let (left, right) = if donor < acceptor {
            (donor.clone(), acceptor.clone())
        } else {
            (acceptor.clone(), donor.clone())
        };
        Ok(Self {
            id,
            left,
            right,
            order: BondOrder::Single,
            electron_origin: CovalentElectronOrigin::Dative { donor, acceptor },
            delocalization: None,
            stereo: None,
        })
    }

    /// Constructs a shared localized Lewis edge with a typed effective order
    /// representing its delocalised resonance average.
    ///
    /// # Errors
    ///
    /// Returns [`StructuralError::SelfBond`] for equal endpoints or
    /// [`StructuralError::RedundantEffectiveBondOrder`] when the effective
    /// order equals the localized integral order.
    pub fn new_delocalized(
        id: CovalentBondId,
        left: AtomId,
        right: AtomId,
        localized_order: BondOrder,
        delocalization: CovalentDelocalization,
    ) -> Result<Self, StructuralError> {
        if u16::from(delocalization.effective_order().numerator())
            == u16::from(localized_order.order())
                * u16::from(delocalization.effective_order().denominator())
        {
            return Err(StructuralError::RedundantEffectiveBondOrder);
        }
        let mut bond = Self::new(id, left, right, localized_order)?;
        bond.delocalization = Some(delocalization);
        Ok(bond)
    }

    /// Constructs a shared double bond carrying a cis/trans arrangement
    /// relative to one substituent on each end. The graph constructor
    /// validates that the references are genuine neighbours.
    ///
    /// # Errors
    ///
    /// Returns [`StructuralError::SelfBond`] for equal endpoints and
    /// [`StructuralError::InvalidStereo`] for a non-double order or a
    /// reference that duplicates an endpoint.
    pub fn new_stereo(
        id: CovalentBondId,
        left: AtomId,
        right: AtomId,
        order: BondOrder,
        left_reference: AtomId,
        right_reference: AtomId,
        arrangement: StereoArrangement,
    ) -> Result<Self, StructuralError> {
        // References follow the same canonical endpoint swap as the bond.
        let swapped = right < left;
        let mut bond = Self::new(id, left, right, order)?;
        if bond.order != BondOrder::Double {
            return Err(StructuralError::InvalidStereo(bond.id));
        }
        let (left_reference, right_reference) = if swapped {
            (right_reference, left_reference)
        } else {
            (left_reference, right_reference)
        };
        if left_reference == bond.left
            || left_reference == bond.right
            || right_reference == bond.left
            || right_reference == bond.right
        {
            return Err(StructuralError::InvalidStereo(bond.id));
        }
        bond.stereo = Some(DoubleBondStereo {
            left_reference,
            right_reference,
            arrangement,
        });
        Ok(bond)
    }

    #[must_use]
    pub const fn id(&self) -> &CovalentBondId {
        &self.id
    }

    #[must_use]
    pub const fn left(&self) -> &AtomId {
        &self.left
    }

    #[must_use]
    pub const fn right(&self) -> &AtomId {
        &self.right
    }

    #[must_use]
    pub const fn order(&self) -> BondOrder {
        self.order
    }

    #[must_use]
    pub const fn stereo(&self) -> Option<&DoubleBondStereo> {
        self.stereo.as_ref()
    }

    #[must_use]
    pub const fn electron_origin(&self) -> &CovalentElectronOrigin {
        &self.electron_origin
    }

    #[must_use]
    pub const fn delocalization(&self) -> Option<&CovalentDelocalization> {
        self.delocalization.as_ref()
    }
}

/// A named, nonempty set of atoms used for ionic components and rule roles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AtomGroup {
    id: AtomGroupId,
    atoms: BTreeSet<AtomId>,
}

impl AtomGroup {
    /// Constructs a group, canonicalizing atom order.
    ///
    /// # Errors
    ///
    /// Returns [`StructuralError::EmptyGroup`] for an empty atom set.
    pub fn new(
        id: AtomGroupId,
        atoms: impl IntoIterator<Item = AtomId>,
    ) -> Result<Self, StructuralError> {
        let mut atom_set = BTreeSet::new();
        for atom in atoms {
            if !atom_set.insert(atom.clone()) {
                return Err(StructuralError::DuplicateGroupAtom { group: id, atom });
            }
        }
        if atom_set.is_empty() {
            return Err(StructuralError::EmptyGroup(id));
        }
        Ok(Self {
            id,
            atoms: atom_set,
        })
    }

    #[must_use]
    pub const fn id(&self) -> &AtomGroupId {
        &self.id
    }

    #[must_use]
    pub const fn atoms(&self) -> &BTreeSet<AtomId> {
        &self.atoms
    }
}

/// A many-body ionic association between two or more charged components.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IonicAssociation {
    id: IonicAssociationId,
    components: BTreeSet<AtomGroupId>,
}

impl IonicAssociation {
    /// Constructs an association, canonicalizing component order.
    ///
    /// # Errors
    ///
    /// Returns [`StructuralError::TooFewIonicComponents`] unless at least two
    /// distinct groups participate.
    pub fn new(
        id: IonicAssociationId,
        components: impl IntoIterator<Item = AtomGroupId>,
    ) -> Result<Self, StructuralError> {
        let mut component_set = BTreeSet::new();
        for component in components {
            if !component_set.insert(component.clone()) {
                return Err(StructuralError::DuplicateIonicComponent {
                    association: id,
                    component,
                });
            }
        }
        if component_set.len() < 2 {
            return Err(StructuralError::TooFewIonicComponents(id));
        }
        Ok(Self {
            id,
            components: component_set,
        })
    }

    #[must_use]
    pub const fn id(&self) -> &IonicAssociationId {
        &self.id
    }

    #[must_use]
    pub const fn components(&self) -> &BTreeSet<AtomGroupId> {
        &self.components
    }
}

/// Explicit ownership of delocalized electrons by metallic sites.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MetallicDomain {
    id: MetallicDomainId,
    sites: BTreeSet<AtomId>,
    delocalized_electrons: u32,
}

impl MetallicDomain {
    /// Constructs a nonempty metallic electron domain.
    ///
    /// # Errors
    ///
    /// Returns an error when there are no sites or no domain-owned electrons.
    pub fn new(
        id: MetallicDomainId,
        sites: impl IntoIterator<Item = AtomId>,
        delocalized_electrons: u32,
    ) -> Result<Self, StructuralError> {
        let mut site_set = BTreeSet::new();
        for site in sites {
            if !site_set.insert(site.clone()) {
                return Err(StructuralError::DuplicateMetallicSite { domain: id, site });
            }
        }
        if site_set.is_empty() {
            return Err(StructuralError::EmptyMetallicDomain(id));
        }
        if delocalized_electrons == 0 {
            return Err(StructuralError::EmptyMetallicElectronPool(id));
        }
        Ok(Self {
            id,
            sites: site_set,
            delocalized_electrons,
        })
    }

    #[must_use]
    pub const fn id(&self) -> &MetallicDomainId {
        &self.id
    }

    #[must_use]
    pub const fn sites(&self) -> &BTreeSet<AtomId> {
        &self.sites
    }

    #[must_use]
    pub const fn delocalized_electrons(&self) -> u32 {
        self.delocalized_electrons
    }
}

/// Validated immutable structural graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StructuralGraph {
    atoms: BTreeMap<AtomId, Atom>,
    covalent_bonds: BTreeMap<CovalentBondId, CovalentBond>,
    groups: BTreeMap<AtomGroupId, AtomGroup>,
    ionic_associations: BTreeMap<IonicAssociationId, IonicAssociation>,
    metallic_domains: BTreeMap<MetallicDomainId, MetallicDomain>,
}

impl StructuralGraph {
    /// Validates and constructs a canonical structural graph.
    ///
    /// # Errors
    ///
    /// Rejects empty graphs, duplicate identities or edges, missing
    /// references, overlapping ionic components, non-neutral associations,
    /// and inconsistent metallic electron ownership.
    pub fn new(
        atoms: impl IntoIterator<Item = Atom>,
        covalent_bonds: impl IntoIterator<Item = CovalentBond>,
        groups: impl IntoIterator<Item = AtomGroup>,
        ionic_associations: impl IntoIterator<Item = IonicAssociation>,
        metallic_domains: impl IntoIterator<Item = MetallicDomain>,
    ) -> Result<Self, StructuralError> {
        let atom_map = build_atoms(atoms)?;
        let bond_map = build_bonds(covalent_bonds, &atom_map)?;
        validate_stereo_references(&bond_map)?;
        validate_chirality(&atom_map, &bond_map)?;
        let group_map = build_groups(groups, &atom_map)?;
        let association_map = build_associations(ionic_associations, &group_map, &atom_map)?;
        let domain_map = build_domains(metallic_domains, &atom_map, &bond_map)?;

        Ok(Self {
            atoms: atom_map,
            covalent_bonds: bond_map,
            groups: group_map,
            ionic_associations: association_map,
            metallic_domains: domain_map,
        })
    }

    #[must_use]
    pub const fn atoms(&self) -> &BTreeMap<AtomId, Atom> {
        &self.atoms
    }

    #[must_use]
    pub const fn covalent_bonds(&self) -> &BTreeMap<CovalentBondId, CovalentBond> {
        &self.covalent_bonds
    }

    #[must_use]
    pub const fn groups(&self) -> &BTreeMap<AtomGroupId, AtomGroup> {
        &self.groups
    }

    #[must_use]
    pub const fn ionic_associations(&self) -> &BTreeMap<IonicAssociationId, IonicAssociation> {
        &self.ionic_associations
    }

    #[must_use]
    pub const fn metallic_domains(&self) -> &BTreeMap<MetallicDomainId, MetallicDomain> {
        &self.metallic_domains
    }

    #[must_use]
    pub fn atom_formal_charge_sum(&self) -> i64 {
        self.atoms
            .values()
            .map(|atom| i64::from(atom.electrons().formal_charge()))
            .sum()
    }

    #[must_use]
    pub fn delocalized_domain_electron_count(&self) -> u64 {
        self.metallic_domains
            .values()
            .map(|domain| u64::from(domain.delocalized_electrons()))
            .sum()
    }

    #[must_use]
    pub fn system_net_charge(&self) -> i128 {
        i128::from(self.atom_formal_charge_sum())
            - i128::from(self.delocalized_domain_electron_count())
    }

    #[must_use]
    pub fn explicit_valence_electron_count(&self) -> u64 {
        let local = self
            .atoms
            .values()
            .map(|atom| u64::from(atom.electrons().non_bonding_electrons()))
            .sum::<u64>();
        let covalent = self
            .covalent_bonds
            .values()
            .map(|bond| u64::from(bond.order().electrons()))
            .sum::<u64>();
        local + covalent + self.delocalized_domain_electron_count()
    }

    #[must_use]
    pub fn element_inventory(&self) -> ElementInventory {
        let mut elements = BTreeMap::new();
        for atom in self.atoms.values() {
            *elements.entry(atom.element().clone()).or_insert(0) += 1;
        }
        ElementInventory { elements }
    }

    #[must_use]
    pub fn covalent_bond_order_sum(&self, atom: &AtomId) -> Option<u64> {
        self.atoms.contains_key(atom).then(|| {
            self.covalent_bonds
                .values()
                .filter(|bond| bond.left() == atom || bond.right() == atom)
                .map(|bond| u64::from(bond.order().order()))
                .sum()
        })
    }

    /// Serializes the graph using canonical chemistry JSON.
    ///
    /// # Errors
    ///
    /// Returns a serialization error if conversion to canonical JSON fails.
    pub fn canonical_json(&self) -> Result<Vec<u8>, StructuralError> {
        canonical_structural_json(self)
    }

    /// Computes a digest over canonical graph semantics.
    ///
    /// # Errors
    ///
    /// Returns a serialization error if canonicalization fails.
    pub fn digest(&self) -> Result<ContentDigest, StructuralError> {
        structural_digest(self)
    }
}

/// The closed initial structural representation kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RepresentationKind {
    Molecular,
    Ion,
    Ionic,
    Metallic,
}

/// A catalogue-level structural identity and validated graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StructureDefinition {
    id: StructureId,
    formula: ElementInventory,
    representation: RepresentationKind,
    graph: StructuralGraph,
}

impl StructureDefinition {
    /// Constructs a structure whose relationship model matches its declared
    /// representation.
    ///
    /// # Errors
    ///
    /// Rejects a formula inventory that differs from the graph or a graph
    /// inconsistent with the representation kind.
    pub fn new(
        id: StructureId,
        formula: ElementInventory,
        representation: RepresentationKind,
        graph: StructuralGraph,
    ) -> Result<Self, StructuralError> {
        if formula != graph.element_inventory() {
            return Err(StructuralError::FormulaGraphMismatch(id));
        }
        let has_ionic = !graph.ionic_associations().is_empty();
        let has_metallic = !graph.metallic_domains().is_empty();
        let all_atoms = graph.atoms().keys().collect::<BTreeSet<_>>();
        let associated_atoms = graph
            .ionic_associations()
            .values()
            .flat_map(IonicAssociation::components)
            .flat_map(|group| graph.groups()[group].atoms())
            .collect::<BTreeSet<_>>();
        let metallic_sites = graph
            .metallic_domains()
            .values()
            .flat_map(MetallicDomain::sites)
            .collect::<BTreeSet<_>>();
        let metallic_sites_are_positive = metallic_sites
            .iter()
            .all(|site| graph.atoms()[*site].electrons().formal_charge() > 0);
        let valid = match representation {
            RepresentationKind::Molecular => {
                !has_ionic && !has_metallic && graph.system_net_charge() == 0
            }
            RepresentationKind::Ion => {
                !has_ionic && !has_metallic && graph.system_net_charge() != 0
            }
            RepresentationKind::Ionic => {
                has_ionic && !has_metallic && associated_atoms == all_atoms
            }
            RepresentationKind::Metallic => {
                has_metallic
                    && !has_ionic
                    && metallic_sites == all_atoms
                    && metallic_sites_are_positive
                    && graph.system_net_charge() == 0
            }
        };
        if !valid {
            return Err(StructuralError::RepresentationMismatch {
                structure: id,
                representation,
            });
        }
        Ok(Self {
            id,
            formula,
            representation,
            graph,
        })
    }

    #[must_use]
    pub const fn id(&self) -> &StructureId {
        &self.id
    }

    #[must_use]
    pub const fn formula(&self) -> &ElementInventory {
        &self.formula
    }

    #[must_use]
    pub const fn representation(&self) -> RepresentationKind {
        self.representation
    }

    #[must_use]
    pub const fn graph(&self) -> &StructuralGraph {
        &self.graph
    }
}

/// One coefficient-expanded structure instance with globally stable atom IDs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StructureInstance {
    id: StructureInstanceId,
    structure: StructureId,
    atom_relabeling: BTreeMap<AtomId, AtomId>,
    graph: StructuralGraph,
}

impl StructureInstance {
    /// Instantiates a definition with a total bijective atom relabeling.
    /// Relationship IDs are deterministically qualified by the instance ID.
    ///
    /// # Errors
    ///
    /// Rejects unknown, duplicate, or incomplete atom relabeling and any
    /// derived graph that fails structural validation.
    pub fn instantiate(
        id: StructureInstanceId,
        definition: &StructureDefinition,
        atom_relabeling: impl IntoIterator<Item = (AtomId, AtomId)>,
    ) -> Result<Self, StructuralError> {
        let mut relabeling = BTreeMap::new();
        let mut destinations = BTreeSet::new();
        for (template, instance) in atom_relabeling {
            if !definition.graph().atoms().contains_key(&template) {
                return Err(StructuralError::UnknownInstanceTemplateAtom(template));
            }
            if relabeling
                .insert(template.clone(), instance.clone())
                .is_some()
            {
                return Err(StructuralError::DuplicateInstanceTemplateAtom(template));
            }
            if !destinations.insert(instance.clone()) {
                return Err(StructuralError::DuplicateInstanceAtom(instance));
            }
        }
        if relabeling.keys().collect::<BTreeSet<_>>()
            != definition.graph().atoms().keys().collect::<BTreeSet<_>>()
        {
            return Err(StructuralError::IncompleteInstanceRelabeling);
        }
        let graph = relabel_graph(&id, definition.graph(), &relabeling)?;
        Ok(Self {
            id,
            structure: definition.id().clone(),
            atom_relabeling: relabeling,
            graph,
        })
    }

    #[must_use]
    pub const fn id(&self) -> &StructureInstanceId {
        &self.id
    }

    #[must_use]
    pub const fn structure(&self) -> &StructureId {
        &self.structure
    }

    #[must_use]
    pub const fn atom_relabeling(&self) -> &BTreeMap<AtomId, AtomId> {
        &self.atom_relabeling
    }

    #[must_use]
    pub const fn graph(&self) -> &StructuralGraph {
        &self.graph
    }
}

/// Canonically ordered collection of expanded instances on one reaction side.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReactionSide {
    instances: BTreeMap<StructureInstanceId, StructureInstance>,
}

impl ReactionSide {
    /// Constructs a side and verifies global atom identity uniqueness.
    ///
    /// # Errors
    ///
    /// Rejects an empty side, duplicate instance IDs, or atom IDs reused by
    /// two instances.
    pub fn new(
        instances: impl IntoIterator<Item = StructureInstance>,
    ) -> Result<Self, StructuralError> {
        let mut instance_map = BTreeMap::new();
        let mut atom_ids = BTreeSet::new();
        for instance in instances {
            for atom in instance.graph().atoms().keys() {
                if !atom_ids.insert(atom.clone()) {
                    return Err(StructuralError::DuplicateReactionAtom(atom.clone()));
                }
            }
            let id = instance.id().clone();
            if instance_map.insert(id.clone(), instance).is_some() {
                return Err(StructuralError::DuplicateIdentity(
                    "structure instance",
                    id.to_string(),
                ));
            }
        }
        if instance_map.is_empty() {
            return Err(StructuralError::EmptyReactionSide);
        }
        Ok(Self {
            instances: instance_map,
        })
    }

    #[must_use]
    pub const fn instances(&self) -> &BTreeMap<StructureInstanceId, StructureInstance> {
        &self.instances
    }

    #[must_use]
    pub fn atom(&self, id: &AtomId) -> Option<&Atom> {
        self.instances
            .values()
            .find_map(|instance| instance.graph().atoms().get(id))
    }

    #[must_use]
    pub fn atom_ids(&self) -> BTreeSet<AtomId> {
        self.instances
            .values()
            .flat_map(|instance| instance.graph().atoms().keys().cloned())
            .collect()
    }
}

/// A total, bijective, element-preserving reactant-to-product atom map.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AtomMapping {
    id: AtomMappingId,
    entries: BTreeMap<AtomId, AtomId>,
}

impl AtomMapping {
    /// Constructs and validates a mapping against complete reaction sides.
    ///
    /// # Errors
    ///
    /// Rejects missing, duplicate, unknown, or element-changing entries.
    pub fn new(
        id: AtomMappingId,
        entries: impl IntoIterator<Item = (AtomId, AtomId)>,
        reactants: &ReactionSide,
        products: &ReactionSide,
    ) -> Result<Self, StructuralError> {
        let mut entry_map = BTreeMap::new();
        let mut destinations = BTreeSet::new();
        for (source, destination) in entries {
            let source_atom = reactants
                .atom(&source)
                .ok_or_else(|| StructuralError::UnknownMappingSource(source.clone()))?;
            let destination_atom = products
                .atom(&destination)
                .ok_or_else(|| StructuralError::UnknownMappingDestination(destination.clone()))?;
            if source_atom.element() != destination_atom.element() {
                return Err(StructuralError::ElementChangingMapping {
                    source,
                    destination,
                });
            }
            if entry_map
                .insert(source.clone(), destination.clone())
                .is_some()
            {
                return Err(StructuralError::DuplicateMappingSource(source));
            }
            if !destinations.insert(destination.clone()) {
                return Err(StructuralError::DuplicateMappingDestination(destination));
            }
        }
        let expected_sources = reactants.atom_ids();
        let expected_destinations = products.atom_ids();
        let actual_sources = entry_map.keys().cloned().collect::<BTreeSet<_>>();
        if actual_sources != expected_sources {
            return Err(StructuralError::IncompleteMappingSources);
        }
        if destinations != expected_destinations {
            return Err(StructuralError::IncompleteMappingDestinations);
        }
        Ok(Self {
            id,
            entries: entry_map,
        })
    }

    #[must_use]
    pub const fn id(&self) -> &AtomMappingId {
        &self.id
    }

    #[must_use]
    pub const fn entries(&self) -> &BTreeMap<AtomId, AtomId> {
        &self.entries
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ElectronAllocation {
    Homolytic,
    HeterolyticTo(AtomId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MetallicReleaseAllocation {
    RetainElectron,
    LeaveElectron,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MetallicJoinAllocation {
    DonateElectron,
}

/// Exact reviewed local state transition for one operation endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ElectronTransition {
    atom: AtomId,
    before: ElectronState,
    after: ElectronState,
}

impl ElectronTransition {
    #[must_use]
    pub const fn new(atom: AtomId, before: ElectronState, after: ElectronState) -> Self {
        Self {
            atom,
            before,
            after,
        }
    }

    #[must_use]
    pub const fn atom(&self) -> &AtomId {
        &self.atom
    }

    #[must_use]
    pub const fn before(&self) -> ElectronState {
        self.before
    }

    #[must_use]
    pub const fn after(&self) -> ElectronState {
        self.after
    }
}

/// Unvalidated construction input for a structural operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StructuralOperationInput {
    ReconfigureElectrons {
        transition: ElectronTransition,
    },
    CleaveCovalent {
        left: AtomId,
        right: AtomId,
        expected_order: BondOrder,
        allocation: ElectronAllocation,
        transitions: Vec<ElectronTransition>,
    },
    FormCovalent {
        left: AtomId,
        right: AtomId,
        order: BondOrder,
        transitions: Vec<ElectronTransition>,
    },
    CleaveDative {
        donor: AtomId,
        acceptor: AtomId,
        allocation: ElectronAllocation,
        transitions: Vec<ElectronTransition>,
    },
    FormDative {
        donor: AtomId,
        acceptor: AtomId,
        transitions: Vec<ElectronTransition>,
    },
    ChangeCovalent {
        left: AtomId,
        right: AtomId,
        old_order: BondOrder,
        new_order: BondOrder,
        allocation: ElectronAllocation,
        transitions: Vec<ElectronTransition>,
    },
    ChangeCovalentDelocalization {
        left: AtomId,
        right: AtomId,
        expected: Option<CovalentDelocalization>,
        replacement: Option<CovalentDelocalization>,
    },
    AssociateIonic {
        association: IonicAssociation,
    },
    DissociateIonic {
        association: IonicAssociationId,
    },
    ReleaseMetallic {
        site: AtomId,
        domain: MetallicDomainId,
        allocation: MetallicReleaseAllocation,
        transition: ElectronTransition,
        domain_electrons_before: u32,
        domain_electrons_after: u32,
    },
    JoinMetallic {
        site: AtomId,
        domain: MetallicDomainId,
        allocation: MetallicJoinAllocation,
        transition: ElectronTransition,
        domain_electrons_before: u32,
        domain_electrons_after: u32,
    },
    TransferElectron {
        donor: AtomId,
        acceptor: AtomId,
        count: u8,
        transitions: Vec<ElectronTransition>,
    },
    AssignProduct {
        atoms: Vec<AtomId>,
        product: StructureInstanceId,
    },
}

/// Canonical validated operation semantics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum StructuralOperationKind {
    ReconfigureElectrons {
        transition: ElectronTransition,
    },
    CleaveCovalent {
        left: AtomId,
        right: AtomId,
        expected_order: BondOrder,
        allocation: ElectronAllocation,
        transitions: BTreeMap<AtomId, ElectronTransition>,
    },
    ChangeCovalentDelocalization {
        left: AtomId,
        right: AtomId,
        expected: Option<CovalentDelocalization>,
        replacement: Option<CovalentDelocalization>,
    },
    FormCovalent {
        left: AtomId,
        right: AtomId,
        order: BondOrder,
        transitions: BTreeMap<AtomId, ElectronTransition>,
    },
    CleaveDative {
        donor: AtomId,
        acceptor: AtomId,
        allocation: ElectronAllocation,
        transitions: BTreeMap<AtomId, ElectronTransition>,
    },
    FormDative {
        donor: AtomId,
        acceptor: AtomId,
        transitions: BTreeMap<AtomId, ElectronTransition>,
    },
    ChangeCovalent {
        left: AtomId,
        right: AtomId,
        old_order: BondOrder,
        new_order: BondOrder,
        allocation: ElectronAllocation,
        transitions: BTreeMap<AtomId, ElectronTransition>,
    },
    AssociateIonic {
        association: IonicAssociation,
    },
    DissociateIonic {
        association: IonicAssociationId,
    },
    ReleaseMetallic {
        site: AtomId,
        domain: MetallicDomainId,
        allocation: MetallicReleaseAllocation,
        transition: ElectronTransition,
        domain_electrons_before: u32,
        domain_electrons_after: u32,
    },
    JoinMetallic {
        site: AtomId,
        domain: MetallicDomainId,
        allocation: MetallicJoinAllocation,
        transition: ElectronTransition,
        domain_electrons_before: u32,
        domain_electrons_after: u32,
    },
    TransferElectron {
        donor: AtomId,
        acceptor: AtomId,
        count: u8,
        transitions: BTreeMap<AtomId, ElectronTransition>,
    },
    AssignProduct {
        atoms: BTreeSet<AtomId>,
        product: StructureInstanceId,
    },
}

/// Read-only view of canonical, validated structural operation semantics.
///
/// Values of this type borrow a [`StructuralOperation`]. Constructing a view
/// cannot forge a validated operation or bypass [`StructuralOperation::new`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructuralOperationView<'a> {
    ReconfigureElectrons {
        transition: &'a ElectronTransition,
    },
    CleaveCovalent {
        left: &'a AtomId,
        right: &'a AtomId,
        expected_order: BondOrder,
        allocation: &'a ElectronAllocation,
        transitions: &'a BTreeMap<AtomId, ElectronTransition>,
    },
    ChangeCovalentDelocalization {
        left: &'a AtomId,
        right: &'a AtomId,
        expected: Option<&'a CovalentDelocalization>,
        replacement: Option<&'a CovalentDelocalization>,
    },
    FormCovalent {
        left: &'a AtomId,
        right: &'a AtomId,
        order: BondOrder,
        transitions: &'a BTreeMap<AtomId, ElectronTransition>,
    },
    CleaveDative {
        donor: &'a AtomId,
        acceptor: &'a AtomId,
        allocation: &'a ElectronAllocation,
        transitions: &'a BTreeMap<AtomId, ElectronTransition>,
    },
    FormDative {
        donor: &'a AtomId,
        acceptor: &'a AtomId,
        transitions: &'a BTreeMap<AtomId, ElectronTransition>,
    },
    ChangeCovalent {
        left: &'a AtomId,
        right: &'a AtomId,
        old_order: BondOrder,
        new_order: BondOrder,
        allocation: &'a ElectronAllocation,
        transitions: &'a BTreeMap<AtomId, ElectronTransition>,
    },
    AssociateIonic {
        association: &'a IonicAssociation,
    },
    DissociateIonic {
        association: &'a IonicAssociationId,
    },
    ReleaseMetallic {
        site: &'a AtomId,
        domain: &'a MetallicDomainId,
        allocation: MetallicReleaseAllocation,
        transition: &'a ElectronTransition,
        domain_electrons_before: u32,
        domain_electrons_after: u32,
    },
    JoinMetallic {
        site: &'a AtomId,
        domain: &'a MetallicDomainId,
        allocation: MetallicJoinAllocation,
        transition: &'a ElectronTransition,
        domain_electrons_before: u32,
        domain_electrons_after: u32,
    },
    TransferElectron {
        donor: &'a AtomId,
        acceptor: &'a AtomId,
        count: u8,
        transitions: &'a BTreeMap<AtomId, ElectronTransition>,
    },
    AssignProduct {
        atoms: &'a BTreeSet<AtomId>,
        product: &'a StructureInstanceId,
    },
}

/// Closed, canonical typed operation value. Slice 1 validates operation shape
/// and electron ownership deltas; Slice 5 owns graph execution semantics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StructuralOperation {
    id: StructuralOperationId,
    #[serde(flatten)]
    kind: StructuralOperationKind,
}

impl StructuralOperation {
    /// Validates and canonicalizes a structural operation.
    ///
    /// # Errors
    ///
    /// Rejects self endpoints, incomplete or duplicate transition sets,
    /// zero-count transfers, unchanged bond orders, inconsistent transfer or
    /// metallic electron ledgers, and empty product assignments.
    pub fn new(
        id: StructuralOperationId,
        input: StructuralOperationInput,
    ) -> Result<Self, StructuralError> {
        let kind = validate_operation(input)?;
        Ok(Self { id, kind })
    }

    #[must_use]
    pub const fn id(&self) -> &StructuralOperationId {
        &self.id
    }

    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn view(&self) -> StructuralOperationView<'_> {
        match &self.kind {
            StructuralOperationKind::ReconfigureElectrons { transition } => {
                StructuralOperationView::ReconfigureElectrons { transition }
            }
            StructuralOperationKind::CleaveCovalent {
                left,
                right,
                expected_order,
                allocation,
                transitions,
            } => StructuralOperationView::CleaveCovalent {
                left,
                right,
                expected_order: *expected_order,
                allocation,
                transitions,
            },
            StructuralOperationKind::FormCovalent {
                left,
                right,
                order,
                transitions,
            } => StructuralOperationView::FormCovalent {
                left,
                right,
                order: *order,
                transitions,
            },
            StructuralOperationKind::CleaveDative {
                donor,
                acceptor,
                allocation,
                transitions,
            } => StructuralOperationView::CleaveDative {
                donor,
                acceptor,
                allocation,
                transitions,
            },
            StructuralOperationKind::FormDative {
                donor,
                acceptor,
                transitions,
            } => StructuralOperationView::FormDative {
                donor,
                acceptor,
                transitions,
            },
            StructuralOperationKind::ChangeCovalent {
                left,
                right,
                old_order,
                new_order,
                allocation,
                transitions,
            } => StructuralOperationView::ChangeCovalent {
                left,
                right,
                old_order: *old_order,
                new_order: *new_order,
                allocation,
                transitions,
            },
            StructuralOperationKind::ChangeCovalentDelocalization {
                left,
                right,
                expected,
                replacement,
            } => StructuralOperationView::ChangeCovalentDelocalization {
                left,
                right,
                expected: expected.as_ref(),
                replacement: replacement.as_ref(),
            },
            StructuralOperationKind::AssociateIonic { association } => {
                StructuralOperationView::AssociateIonic { association }
            }
            StructuralOperationKind::DissociateIonic { association } => {
                StructuralOperationView::DissociateIonic { association }
            }
            StructuralOperationKind::ReleaseMetallic {
                site,
                domain,
                allocation,
                transition,
                domain_electrons_before,
                domain_electrons_after,
            } => StructuralOperationView::ReleaseMetallic {
                site,
                domain,
                allocation: *allocation,
                transition,
                domain_electrons_before: *domain_electrons_before,
                domain_electrons_after: *domain_electrons_after,
            },
            StructuralOperationKind::JoinMetallic {
                site,
                domain,
                allocation,
                transition,
                domain_electrons_before,
                domain_electrons_after,
            } => StructuralOperationView::JoinMetallic {
                site,
                domain,
                allocation: *allocation,
                transition,
                domain_electrons_before: *domain_electrons_before,
                domain_electrons_after: *domain_electrons_after,
            },
            StructuralOperationKind::TransferElectron {
                donor,
                acceptor,
                count,
                transitions,
            } => StructuralOperationView::TransferElectron {
                donor,
                acceptor,
                count: *count,
                transitions,
            },
            StructuralOperationKind::AssignProduct { atoms, product } => {
                StructuralOperationView::AssignProduct { atoms, product }
            }
        }
    }
}

#[allow(clippy::too_many_lines)]
fn validate_operation(
    input: StructuralOperationInput,
) -> Result<StructuralOperationKind, StructuralError> {
    match input {
        StructuralOperationInput::ReconfigureElectrons { transition } => {
            if transition.before().formal_charge() != transition.after().formal_charge()
                || transition.before().non_bonding_electrons()
                    != transition.after().non_bonding_electrons()
            {
                return Err(StructuralError::InvalidElectronReconfiguration);
            }
            Ok(StructuralOperationKind::ReconfigureElectrons { transition })
        }
        StructuralOperationInput::CleaveCovalent {
            left,
            right,
            expected_order,
            allocation,
            transitions,
        } => {
            validate_endpoints(&left, &right)?;
            validate_allocation(&allocation, &left, &right)?;
            let transitions = canonical_transitions(transitions, [&left, &right])?;
            if !valid_bond_electron_ledger(
                &transitions,
                &left,
                &right,
                -i16::from(expected_order.order()),
                Some(&allocation),
            ) {
                return Err(StructuralError::InvalidCovalentElectronLedger);
            }
            Ok(StructuralOperationKind::CleaveCovalent {
                left,
                right,
                expected_order,
                allocation,
                transitions,
            })
        }
        StructuralOperationInput::FormCovalent {
            left,
            right,
            order,
            transitions,
        } => {
            validate_endpoints(&left, &right)?;
            let transitions = canonical_transitions(transitions, [&left, &right])?;
            if !valid_bond_electron_ledger(
                &transitions,
                &left,
                &right,
                i16::from(order.order()),
                None,
            ) {
                return Err(StructuralError::InvalidCovalentElectronLedger);
            }
            Ok(StructuralOperationKind::FormCovalent {
                left,
                right,
                order,
                transitions,
            })
        }
        StructuralOperationInput::CleaveDative {
            donor,
            acceptor,
            allocation,
            transitions,
        } => {
            validate_endpoints(&donor, &acceptor)?;
            validate_allocation(&allocation, &donor, &acceptor)?;
            let transitions = canonical_transitions(transitions, [&donor, &acceptor])?;
            if !valid_bond_electron_ledger(&transitions, &donor, &acceptor, -1, Some(&allocation)) {
                return Err(StructuralError::InvalidDativeElectronLedger);
            }
            Ok(StructuralOperationKind::CleaveDative {
                donor,
                acceptor,
                allocation,
                transitions,
            })
        }
        StructuralOperationInput::FormDative {
            donor,
            acceptor,
            transitions,
        } => {
            validate_endpoints(&donor, &acceptor)?;
            let transitions = canonical_transitions(transitions, [&donor, &acceptor])?;
            if !valid_dative_formation(&transitions, &donor, &acceptor) {
                return Err(StructuralError::InvalidDativeElectronLedger);
            }
            Ok(StructuralOperationKind::FormDative {
                donor,
                acceptor,
                transitions,
            })
        }
        StructuralOperationInput::ChangeCovalent {
            left,
            right,
            old_order,
            new_order,
            allocation,
            transitions,
        } => {
            validate_endpoints(&left, &right)?;
            if old_order == new_order {
                return Err(StructuralError::UnchangedBondOrder);
            }
            validate_allocation(&allocation, &left, &right)?;
            let transitions = canonical_transitions(transitions, [&left, &right])?;
            let order_delta = i16::from(new_order.order()) - i16::from(old_order.order());
            if !valid_bond_electron_ledger(
                &transitions,
                &left,
                &right,
                order_delta,
                Some(&allocation),
            ) {
                return Err(StructuralError::InvalidCovalentElectronLedger);
            }
            Ok(StructuralOperationKind::ChangeCovalent {
                left,
                right,
                old_order,
                new_order,
                allocation,
                transitions,
            })
        }
        StructuralOperationInput::ChangeCovalentDelocalization {
            left,
            right,
            expected,
            replacement,
        } => {
            validate_endpoints(&left, &right)?;
            if expected == replacement {
                return Err(StructuralError::UnchangedCovalentDelocalization);
            }
            Ok(StructuralOperationKind::ChangeCovalentDelocalization {
                left,
                right,
                expected,
                replacement,
            })
        }
        StructuralOperationInput::AssociateIonic { association } => {
            Ok(StructuralOperationKind::AssociateIonic { association })
        }
        StructuralOperationInput::DissociateIonic { association } => {
            Ok(StructuralOperationKind::DissociateIonic { association })
        }
        StructuralOperationInput::ReleaseMetallic {
            site,
            domain,
            allocation,
            transition,
            domain_electrons_before,
            domain_electrons_after,
        } => {
            if transition.atom() != &site
                || !valid_release(
                    allocation,
                    &transition,
                    domain_electrons_before,
                    domain_electrons_after,
                )
            {
                return Err(StructuralError::InvalidMetallicElectronLedger);
            }
            Ok(StructuralOperationKind::ReleaseMetallic {
                site,
                domain,
                allocation,
                transition,
                domain_electrons_before,
                domain_electrons_after,
            })
        }
        StructuralOperationInput::JoinMetallic {
            site,
            domain,
            allocation,
            transition,
            domain_electrons_before,
            domain_electrons_after,
        } => {
            if transition.atom() != &site
                || !valid_join(&transition, domain_electrons_before, domain_electrons_after)
            {
                return Err(StructuralError::InvalidMetallicElectronLedger);
            }
            Ok(StructuralOperationKind::JoinMetallic {
                site,
                domain,
                allocation,
                transition,
                domain_electrons_before,
                domain_electrons_after,
            })
        }
        StructuralOperationInput::TransferElectron {
            donor,
            acceptor,
            count,
            transitions,
        } => {
            validate_endpoints(&donor, &acceptor)?;
            if count == 0 {
                return Err(StructuralError::ZeroElectronTransfer);
            }
            let transitions = canonical_transitions(transitions, [&donor, &acceptor])?;
            if !valid_transfer(&transitions, &donor, &acceptor, count) {
                return Err(StructuralError::InvalidElectronTransfer);
            }
            Ok(StructuralOperationKind::TransferElectron {
                donor,
                acceptor,
                count,
                transitions,
            })
        }
        StructuralOperationInput::AssignProduct { atoms, product } => {
            let mut atom_set = BTreeSet::new();
            for atom in atoms {
                if !atom_set.insert(atom.clone()) {
                    return Err(StructuralError::DuplicateProductAssignmentAtom(atom));
                }
            }
            if atom_set.is_empty() {
                return Err(StructuralError::EmptyProductAssignment);
            }
            Ok(StructuralOperationKind::AssignProduct {
                atoms: atom_set,
                product,
            })
        }
    }
}

fn validate_endpoints(left: &AtomId, right: &AtomId) -> Result<(), StructuralError> {
    if left == right {
        Err(StructuralError::OperationSelfEndpoint(left.clone()))
    } else {
        Ok(())
    }
}

fn validate_allocation(
    allocation: &ElectronAllocation,
    left: &AtomId,
    right: &AtomId,
) -> Result<(), StructuralError> {
    if let ElectronAllocation::HeterolyticTo(recipient) = allocation
        && recipient != left
        && recipient != right
    {
        return Err(StructuralError::UnrelatedElectronAllocation(
            recipient.clone(),
        ));
    }
    Ok(())
}

fn canonical_transitions<const N: usize>(
    transitions: Vec<ElectronTransition>,
    required: [&AtomId; N],
) -> Result<BTreeMap<AtomId, ElectronTransition>, StructuralError> {
    let required = required.into_iter().cloned().collect::<BTreeSet<_>>();
    let mut result = BTreeMap::new();
    for transition in transitions {
        let atom = transition.atom().clone();
        if !required.contains(&atom) {
            return Err(StructuralError::UnrelatedOperationTransition(atom));
        }
        if transition.before() == transition.after() {
            return Err(StructuralError::UnchangedOperationTransition(atom));
        }
        if result.insert(atom.clone(), transition).is_some() {
            return Err(StructuralError::DuplicateOperationTransition(atom));
        }
    }
    if result.keys().cloned().collect::<BTreeSet<_>>() != required {
        return Err(StructuralError::IncompleteOperationTransitions);
    }
    Ok(result)
}

fn valid_transfer(
    transitions: &BTreeMap<AtomId, ElectronTransition>,
    donor: &AtomId,
    acceptor: &AtomId,
    count: u8,
) -> bool {
    let donor = &transitions[donor];
    let acceptor = &transitions[acceptor];
    donor.before().non_bonding_electrons().checked_sub(count)
        == Some(donor.after().non_bonding_electrons())
        && acceptor.before().non_bonding_electrons().checked_add(count)
            == Some(acceptor.after().non_bonding_electrons())
        && i32::from(donor.before().formal_charge()) + i32::from(count)
            == i32::from(donor.after().formal_charge())
        && i32::from(acceptor.before().formal_charge()) - i32::from(count)
            == i32::from(acceptor.after().formal_charge())
}

fn valid_bond_electron_ledger(
    transitions: &BTreeMap<AtomId, ElectronTransition>,
    left: &AtomId,
    right: &AtomId,
    bond_order_delta: i16,
    allocation: Option<&ElectronAllocation>,
) -> bool {
    let (left_local_delta, right_local_delta) = match allocation {
        None | Some(ElectronAllocation::Homolytic) => (-bond_order_delta, -bond_order_delta),
        Some(ElectronAllocation::HeterolyticTo(recipient)) if recipient == left => {
            (-2 * bond_order_delta, 0)
        }
        Some(ElectronAllocation::HeterolyticTo(_)) => (0, -2 * bond_order_delta),
    };
    valid_endpoint_delta(&transitions[left], left_local_delta, bond_order_delta)
        && valid_endpoint_delta(&transitions[right], right_local_delta, bond_order_delta)
}

fn valid_dative_formation(
    transitions: &BTreeMap<AtomId, ElectronTransition>,
    donor: &AtomId,
    acceptor: &AtomId,
) -> bool {
    let donor_transition = &transitions[donor];
    let acceptor_transition = &transitions[acceptor];
    let donor_has_pair = donor_transition
        .before()
        .non_bonding_electrons()
        .saturating_sub(donor_transition.before().unpaired_electrons())
        >= 2;
    donor_has_pair
        && donor_transition.before().unpaired_electrons()
            == donor_transition.after().unpaired_electrons()
        && acceptor_transition.before().unpaired_electrons()
            == acceptor_transition.after().unpaired_electrons()
        && valid_endpoint_delta(donor_transition, -2, 1)
        && valid_endpoint_delta(acceptor_transition, 0, 1)
}

fn valid_endpoint_delta(
    transition: &ElectronTransition,
    local_delta: i16,
    bond_order_delta: i16,
) -> bool {
    let expected_local = i16::from(transition.before().non_bonding_electrons()) + local_delta;
    let expected_formal = i32::from(transition.before().formal_charge())
        - i32::from(local_delta)
        - i32::from(bond_order_delta);
    expected_local == i16::from(transition.after().non_bonding_electrons())
        && expected_formal == i32::from(transition.after().formal_charge())
}

fn valid_release(
    allocation: MetallicReleaseAllocation,
    transition: &ElectronTransition,
    domain_before: u32,
    domain_after: u32,
) -> bool {
    match allocation {
        MetallicReleaseAllocation::RetainElectron => {
            let Some(released) = domain_before.checked_sub(domain_after) else {
                return false;
            };
            let Ok(released) = u8::try_from(released) else {
                return false;
            };
            released > 0
                && transition
                    .before()
                    .non_bonding_electrons()
                    .checked_add(released)
                    == Some(transition.after().non_bonding_electrons())
                && i32::from(transition.before().formal_charge()) - i32::from(released)
                    == i32::from(transition.after().formal_charge())
        }
        MetallicReleaseAllocation::LeaveElectron => {
            domain_before == domain_after && transition.before() == transition.after()
        }
    }
}

fn valid_join(transition: &ElectronTransition, domain_before: u32, domain_after: u32) -> bool {
    let Some(joined) = domain_after.checked_sub(domain_before) else {
        return false;
    };
    let Ok(joined) = u8::try_from(joined) else {
        return false;
    };
    joined > 0
        && transition
            .before()
            .non_bonding_electrons()
            .checked_sub(joined)
            == Some(transition.after().non_bonding_electrons())
        && i32::from(transition.before().formal_charge()) + i32::from(joined)
            == i32::from(transition.after().formal_charge())
}

/// Canonically serializes any structural value.
///
/// # Errors
///
/// Returns a typed error when serde conversion or chemistry JSON
/// canonicalization fails.
pub fn canonical_structural_json<T: Serialize>(value: &T) -> Result<Vec<u8>, StructuralError> {
    let value = serde_json::to_value(value)
        .map_err(|error| StructuralError::Serialization(error.to_string()))?;
    canonical_json(&value).map_err(StructuralError::Canonicalization)
}

/// Computes a semantic SHA-256 digest from canonical structural JSON.
///
/// # Errors
///
/// Returns a typed error when canonical serialization fails.
pub fn structural_digest<T: Serialize>(value: &T) -> Result<ContentDigest, StructuralError> {
    Ok(ContentDigest::sha256(&canonical_structural_json(value)?))
}

/// Every stereo reference must be a genuine neighbour of its own end of
/// the double bond, reached through some other bond.
fn validate_stereo_references(
    bonds: &BTreeMap<CovalentBondId, CovalentBond>,
) -> Result<(), StructuralError> {
    let bonded = |a: &AtomId, b: &AtomId| {
        bonds.values().any(|bond| {
            (bond.left() == a && bond.right() == b) || (bond.left() == b && bond.right() == a)
        })
    };
    for bond in bonds.values() {
        if let Some(stereo) = bond.stereo()
            && (!bonded(bond.left(), stereo.left_reference())
                || !bonded(bond.right(), stereo.right_reference()))
        {
            return Err(StructuralError::InvalidStereo(bond.id().clone()));
        }
    }
    Ok(())
}

/// A chiral atom's four listed neighbours must be exactly its bonded
/// partners.
fn validate_chirality(
    atoms: &BTreeMap<AtomId, Atom>,
    bonds: &BTreeMap<CovalentBondId, CovalentBond>,
) -> Result<(), StructuralError> {
    for atom in atoms.values() {
        let Some(chirality) = atom.chirality() else {
            continue;
        };
        let mut partners: Vec<&AtomId> = bonds
            .values()
            .filter_map(|bond| {
                if bond.left() == atom.id() {
                    Some(bond.right())
                } else if bond.right() == atom.id() {
                    Some(bond.left())
                } else {
                    None
                }
            })
            .collect();
        partners.sort();
        let mut listed: Vec<&AtomId> = chirality.neighbours().iter().collect();
        listed.sort();
        if partners != listed {
            return Err(StructuralError::InvalidChirality(atom.id().clone()));
        }
    }
    Ok(())
}

fn relabel_graph(
    instance: &StructureInstanceId,
    graph: &StructuralGraph,
    atoms: &BTreeMap<AtomId, AtomId>,
) -> Result<StructuralGraph, StructuralError> {
    let relabeled_atoms = graph
        .atoms()
        .values()
        .map(|atom| {
            Atom::new(
                atoms[atom.id()].clone(),
                atom.element().clone(),
                atom.electrons(),
            )
        })
        .collect::<Vec<_>>();
    let relabeled_bonds = graph
        .covalent_bonds()
        .values()
        .map(|bond| {
            let id = qualified_id(instance, bond.id())?;
            match bond.electron_origin() {
                CovalentElectronOrigin::Shared => CovalentBond::new(
                    id,
                    atoms[bond.left()].clone(),
                    atoms[bond.right()].clone(),
                    bond.order(),
                ),
                CovalentElectronOrigin::Dative { donor, acceptor } => {
                    CovalentBond::new_dative(id, atoms[donor].clone(), atoms[acceptor].clone())
                }
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    let group_ids = graph
        .groups()
        .keys()
        .map(|id| Ok((id.clone(), qualified_id(instance, id)?)))
        .collect::<Result<BTreeMap<_, _>, StructuralError>>()?;
    let relabeled_groups = graph
        .groups()
        .values()
        .map(|group| {
            AtomGroup::new(
                group_ids[group.id()].clone(),
                group.atoms().iter().map(|atom| atoms[atom].clone()),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let relabeled_associations = graph
        .ionic_associations()
        .values()
        .map(|association| {
            IonicAssociation::new(
                qualified_id(instance, association.id())?,
                association
                    .components()
                    .iter()
                    .map(|group| group_ids[group].clone()),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let relabeled_domains = graph
        .metallic_domains()
        .values()
        .map(|domain| {
            MetallicDomain::new(
                qualified_id(instance, domain.id())?,
                domain.sites().iter().map(|site| atoms[site].clone()),
                domain.delocalized_electrons(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    StructuralGraph::new(
        relabeled_atoms,
        relabeled_bonds,
        relabeled_groups,
        relabeled_associations,
        relabeled_domains,
    )
}

fn qualified_id<K: IdKind>(
    instance: &StructureInstanceId,
    local: &DeclaredId<K>,
) -> Result<DeclaredId<K>, StructuralError> {
    DeclaredId::new(format!("{instance}.{local}"))
        .map_err(|_| StructuralError::InvalidDerivedIdentity(format!("{instance}.{local}")))
}

fn build_atoms(
    atoms: impl IntoIterator<Item = Atom>,
) -> Result<BTreeMap<AtomId, Atom>, StructuralError> {
    let mut result = BTreeMap::new();
    for atom in atoms {
        let id = atom.id().clone();
        if result.insert(id.clone(), atom).is_some() {
            return Err(StructuralError::DuplicateIdentity("atom", id.to_string()));
        }
    }
    if result.is_empty() {
        return Err(StructuralError::EmptyGraph);
    }
    Ok(result)
}

fn build_bonds(
    bonds: impl IntoIterator<Item = CovalentBond>,
    atoms: &BTreeMap<AtomId, Atom>,
) -> Result<BTreeMap<CovalentBondId, CovalentBond>, StructuralError> {
    let mut result = BTreeMap::new();
    let mut endpoint_pairs = BTreeSet::new();
    for bond in bonds {
        require_atom(atoms, bond.left())?;
        require_atom(atoms, bond.right())?;
        let pair = (bond.left().clone(), bond.right().clone());
        if !endpoint_pairs.insert(pair.clone()) {
            return Err(StructuralError::DuplicateCovalentEdge(pair.0, pair.1));
        }
        let id = bond.id().clone();
        if result.insert(id.clone(), bond).is_some() {
            return Err(StructuralError::DuplicateIdentity("bond", id.to_string()));
        }
    }
    Ok(result)
}

fn build_groups(
    groups: impl IntoIterator<Item = AtomGroup>,
    atoms: &BTreeMap<AtomId, Atom>,
) -> Result<BTreeMap<AtomGroupId, AtomGroup>, StructuralError> {
    let mut result = BTreeMap::new();
    for group in groups {
        for atom in group.atoms() {
            require_atom(atoms, atom)?;
        }
        let id = group.id().clone();
        if result.insert(id.clone(), group).is_some() {
            return Err(StructuralError::DuplicateIdentity("group", id.to_string()));
        }
    }
    Ok(result)
}

fn build_associations(
    associations: impl IntoIterator<Item = IonicAssociation>,
    groups: &BTreeMap<AtomGroupId, AtomGroup>,
    atoms: &BTreeMap<AtomId, Atom>,
) -> Result<BTreeMap<IonicAssociationId, IonicAssociation>, StructuralError> {
    let mut result = BTreeMap::new();
    let mut associated_groups = BTreeSet::new();
    let mut associated_atoms = BTreeSet::new();
    for association in associations {
        validate_association(
            &association,
            groups,
            atoms,
            &mut associated_groups,
            &mut associated_atoms,
        )?;
        let id = association.id().clone();
        if result.insert(id.clone(), association).is_some() {
            return Err(StructuralError::DuplicateIdentity(
                "ionic association",
                id.to_string(),
            ));
        }
    }
    Ok(result)
}

fn validate_association(
    association: &IonicAssociation,
    groups: &BTreeMap<AtomGroupId, AtomGroup>,
    atoms: &BTreeMap<AtomId, Atom>,
    associated_groups: &mut BTreeSet<AtomGroupId>,
    associated_atoms: &mut BTreeSet<AtomId>,
) -> Result<(), StructuralError> {
    let mut total_charge = 0_i64;
    for component_id in association.components() {
        let component = groups
            .get(component_id)
            .ok_or_else(|| StructuralError::UnknownGroup(component_id.clone()))?;
        if !associated_groups.insert(component_id.clone()) {
            return Err(StructuralError::MultiplyAssociatedGroup(
                component_id.clone(),
            ));
        }
        for atom in component.atoms() {
            if !associated_atoms.insert(atom.clone()) {
                return Err(StructuralError::OverlappingIonicComponents(
                    association.id().clone(),
                    atom.clone(),
                ));
            }
        }
        let charge = group_charge(component, atoms);
        if charge == 0 {
            return Err(StructuralError::NeutralIonicComponent(component_id.clone()));
        }
        total_charge += charge;
    }
    if total_charge != 0 {
        return Err(StructuralError::NonNeutralIonicAssociation {
            association: association.id().clone(),
            charge: total_charge,
        });
    }
    Ok(())
}

fn build_domains(
    domains: impl IntoIterator<Item = MetallicDomain>,
    atoms: &BTreeMap<AtomId, Atom>,
    bonds: &BTreeMap<CovalentBondId, CovalentBond>,
) -> Result<BTreeMap<MetallicDomainId, MetallicDomain>, StructuralError> {
    let bonded_atoms = bonds
        .values()
        .flat_map(|bond| [bond.left(), bond.right()])
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut result = BTreeMap::new();
    let mut owned_sites = BTreeSet::new();
    for domain in domains {
        validate_domain(&domain, atoms, &bonded_atoms, &mut owned_sites)?;
        let id = domain.id().clone();
        if result.insert(id.clone(), domain).is_some() {
            return Err(StructuralError::DuplicateIdentity(
                "metallic domain",
                id.to_string(),
            ));
        }
    }
    Ok(result)
}

fn validate_domain(
    domain: &MetallicDomain,
    atoms: &BTreeMap<AtomId, Atom>,
    bonded_atoms: &BTreeSet<AtomId>,
    owned_sites: &mut BTreeSet<AtomId>,
) -> Result<(), StructuralError> {
    for site in domain.sites() {
        let atom = require_atom(atoms, site)?;
        if !owned_sites.insert(site.clone()) {
            return Err(StructuralError::MultiplyOwnedMetallicSite(site.clone()));
        }
        if atom.electrons().non_bonding_electrons() != 0
            || atom.electrons().unpaired_electrons() != 0
        {
            return Err(StructuralError::MetallicSiteHasLocalElectrons(site.clone()));
        }
        if bonded_atoms.contains(site) {
            return Err(StructuralError::MetallicSiteHasCovalentBond(site.clone()));
        }
    }
    Ok(())
}

fn require_atom<'a>(
    atoms: &'a BTreeMap<AtomId, Atom>,
    id: &AtomId,
) -> Result<&'a Atom, StructuralError> {
    atoms
        .get(id)
        .ok_or_else(|| StructuralError::UnknownAtom(id.clone()))
}

fn group_charge(group: &AtomGroup, atoms: &BTreeMap<AtomId, Atom>) -> i64 {
    group
        .atoms()
        .iter()
        .map(|id| {
            i64::from(
                atoms
                    .get(id)
                    .expect("validated group atoms must exist")
                    .electrons()
                    .formal_charge(),
            )
        })
        .sum()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StructuralError {
    InvalidStereo(CovalentBondId),
    InvalidChirality(AtomId),
    InvalidUnpairedElectrons {
        non_bonding_electrons: u8,
        unpaired_electrons: u8,
    },
    EmptyElementInventory,
    ZeroElementCount(ElementSymbol),
    DuplicateElement(ElementSymbol),
    EmptyGraph,
    DuplicateIdentity(&'static str, String),
    SelfBond(AtomId),
    InvalidEffectiveBondOrder {
        numerator: u8,
        denominator: u8,
    },
    RedundantEffectiveBondOrder,
    DuplicateCovalentEdge(AtomId, AtomId),
    UnknownAtom(AtomId),
    EmptyGroup(AtomGroupId),
    DuplicateGroupAtom {
        group: AtomGroupId,
        atom: AtomId,
    },
    UnknownGroup(AtomGroupId),
    TooFewIonicComponents(IonicAssociationId),
    DuplicateIonicComponent {
        association: IonicAssociationId,
        component: AtomGroupId,
    },
    OverlappingIonicComponents(IonicAssociationId, AtomId),
    NeutralIonicComponent(AtomGroupId),
    NonNeutralIonicAssociation {
        association: IonicAssociationId,
        charge: i64,
    },
    MultiplyAssociatedGroup(AtomGroupId),
    EmptyMetallicDomain(MetallicDomainId),
    DuplicateMetallicSite {
        domain: MetallicDomainId,
        site: AtomId,
    },
    EmptyMetallicElectronPool(MetallicDomainId),
    MultiplyOwnedMetallicSite(AtomId),
    MetallicSiteHasLocalElectrons(AtomId),
    MetallicSiteHasCovalentBond(AtomId),
    FormulaGraphMismatch(StructureId),
    RepresentationMismatch {
        structure: StructureId,
        representation: RepresentationKind,
    },
    EmptyReactionSide,
    DuplicateReactionAtom(AtomId),
    UnknownInstanceTemplateAtom(AtomId),
    DuplicateInstanceTemplateAtom(AtomId),
    DuplicateInstanceAtom(AtomId),
    IncompleteInstanceRelabeling,
    InvalidDerivedIdentity(String),
    UnknownMappingSource(AtomId),
    UnknownMappingDestination(AtomId),
    DuplicateMappingSource(AtomId),
    DuplicateMappingDestination(AtomId),
    ElementChangingMapping {
        source: AtomId,
        destination: AtomId,
    },
    IncompleteMappingSources,
    IncompleteMappingDestinations,
    OperationSelfEndpoint(AtomId),
    UnrelatedElectronAllocation(AtomId),
    UnrelatedOperationTransition(AtomId),
    UnchangedOperationTransition(AtomId),
    DuplicateOperationTransition(AtomId),
    IncompleteOperationTransitions,
    UnchangedBondOrder,
    UnchangedCovalentDelocalization,
    ZeroElectronTransfer,
    InvalidElectronTransfer,
    InvalidElectronReconfiguration,
    InvalidCovalentElectronLedger,
    InvalidDativeElectronLedger,
    InvalidMetallicElectronLedger,
    EmptyProductAssignment,
    DuplicateProductAssignmentAtom(AtomId),
    Serialization(String),
    Canonicalization(CanonicalJsonError),
}

impl fmt::Display for StructuralError {
    #[allow(clippy::too_many_lines)]
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidStereo(bond) => write!(
                formatter,
                "bond `{bond}` carries a stereo descriptor that is not a double bond between valid neighbour references"
            ),
            Self::InvalidChirality(atom) => write!(
                formatter,
                "atom `{atom}` carries a chirality descriptor whose neighbours are not its four bonded partners"
            ),
            Self::InvalidUnpairedElectrons {
                non_bonding_electrons,
                unpaired_electrons,
            } => write!(
                formatter,
                "{unpaired_electrons} unpaired electrons are inconsistent with {non_bonding_electrons} local electrons"
            ),
            Self::EmptyElementInventory => {
                formatter.write_str("an element inventory must not be empty")
            }
            Self::ZeroElementCount(element) => {
                write!(formatter, "element inventory count for `{element}` is zero")
            }
            Self::DuplicateElement(element) => {
                write!(formatter, "element inventory repeats `{element}`")
            }
            Self::EmptyGraph => formatter.write_str("a structural graph must contain an atom"),
            Self::DuplicateIdentity(kind, id) => {
                write!(formatter, "duplicate {kind} identity `{id}`")
            }
            Self::InvalidEffectiveBondOrder {
                numerator,
                denominator,
            } => write!(
                formatter,
                "effective bond order `{numerator}/{denominator}` is not a reduced positive non-integral order up to three"
            ),
            Self::RedundantEffectiveBondOrder => formatter.write_str(
                "a delocalised effective bond order must differ from its localized Lewis edge",
            ),
            Self::UnchangedCovalentDelocalization => formatter.write_str(
                "a covalent delocalisation operation must change the resonance annotation",
            ),
            Self::SelfBond(atom) => write!(formatter, "atom `{atom}` cannot bond to itself"),
            Self::DuplicateCovalentEdge(left, right) => {
                write!(formatter, "duplicate covalent edge `{left}`-`{right}`")
            }
            Self::UnknownAtom(atom) => write!(formatter, "unknown atom `{atom}`"),
            Self::EmptyGroup(group) => write!(formatter, "atom group `{group}` is empty"),
            Self::DuplicateGroupAtom { group, atom } => {
                write!(formatter, "atom group `{group}` repeats atom `{atom}`")
            }
            Self::UnknownGroup(group) => write!(formatter, "unknown atom group `{group}`"),
            Self::TooFewIonicComponents(association) => write!(
                formatter,
                "ionic association `{association}` requires at least two components"
            ),
            Self::DuplicateIonicComponent {
                association,
                component,
            } => write!(
                formatter,
                "ionic association `{association}` repeats component `{component}`"
            ),
            Self::OverlappingIonicComponents(association, atom) => write!(
                formatter,
                "ionic association `{association}` contains atom `{atom}` in multiple components"
            ),
            Self::NeutralIonicComponent(group) => {
                write!(formatter, "ionic component `{group}` is neutral")
            }
            Self::NonNeutralIonicAssociation {
                association,
                charge,
            } => write!(
                formatter,
                "ionic association `{association}` has net component charge {charge}"
            ),
            Self::MultiplyAssociatedGroup(group) => write!(
                formatter,
                "ionic component `{group}` belongs to more than one association"
            ),
            Self::EmptyMetallicDomain(domain) => {
                write!(formatter, "metallic domain `{domain}` has no sites")
            }
            Self::DuplicateMetallicSite { domain, site } => {
                write!(
                    formatter,
                    "metallic domain `{domain}` repeats site `{site}`"
                )
            }
            Self::EmptyMetallicElectronPool(domain) => write!(
                formatter,
                "metallic domain `{domain}` has no delocalized electrons"
            ),
            Self::MultiplyOwnedMetallicSite(site) => write!(
                formatter,
                "metallic site `{site}` belongs to more than one electron domain"
            ),
            Self::MetallicSiteHasLocalElectrons(site) => write!(
                formatter,
                "metallic site `{site}` cannot simultaneously own local electrons"
            ),
            Self::MetallicSiteHasCovalentBond(site) => write!(
                formatter,
                "metallic site `{site}` cannot simultaneously have a localized covalent edge"
            ),
            Self::FormulaGraphMismatch(structure) => {
                write!(
                    formatter,
                    "structure `{structure}` formula inventory differs from its graph"
                )
            }
            Self::RepresentationMismatch {
                structure,
                representation,
            } => write!(
                formatter,
                "structure `{structure}` does not match representation {representation:?}"
            ),
            Self::EmptyReactionSide => {
                formatter.write_str("a reaction side must contain an instance")
            }
            Self::DuplicateReactionAtom(atom) => {
                write!(formatter, "reaction-side atom identity `{atom}` is reused")
            }
            Self::UnknownInstanceTemplateAtom(atom) => {
                write!(formatter, "unknown instance template atom `{atom}`")
            }
            Self::DuplicateInstanceTemplateAtom(atom) => {
                write!(
                    formatter,
                    "instance relabeling repeats template atom `{atom}`"
                )
            }
            Self::DuplicateInstanceAtom(atom) => {
                write!(
                    formatter,
                    "instance relabeling repeats destination atom `{atom}`"
                )
            }
            Self::IncompleteInstanceRelabeling => {
                formatter.write_str("instance relabeling does not cover every template atom")
            }
            Self::InvalidDerivedIdentity(identity) => {
                write!(formatter, "invalid derived instance identity `{identity}`")
            }
            Self::UnknownMappingSource(atom) => {
                write!(formatter, "unknown atom-mapping source `{atom}`")
            }
            Self::UnknownMappingDestination(atom) => {
                write!(formatter, "unknown atom-mapping destination `{atom}`")
            }
            Self::DuplicateMappingSource(atom) => {
                write!(
                    formatter,
                    "atom-mapping source `{atom}` occurs more than once"
                )
            }
            Self::DuplicateMappingDestination(atom) => write!(
                formatter,
                "atom-mapping destination `{atom}` occurs more than once"
            ),
            Self::ElementChangingMapping {
                source,
                destination,
            } => write!(
                formatter,
                "atom mapping changes element from `{source}` to `{destination}`"
            ),
            Self::IncompleteMappingSources => {
                formatter.write_str("atom mapping does not cover every reactant atom exactly once")
            }
            Self::IncompleteMappingDestinations => {
                formatter.write_str("atom mapping does not cover every product atom exactly once")
            }
            Self::OperationSelfEndpoint(atom) => {
                write!(formatter, "structural operation repeats endpoint `{atom}`")
            }
            Self::UnrelatedElectronAllocation(atom) => {
                write!(
                    formatter,
                    "electron allocation names unrelated atom `{atom}`"
                )
            }
            Self::UnrelatedOperationTransition(atom) => {
                write!(
                    formatter,
                    "operation contains transition for unrelated atom `{atom}`"
                )
            }
            Self::UnchangedOperationTransition(atom) => {
                write!(
                    formatter,
                    "operation transition for `{atom}` does not change state"
                )
            }
            Self::DuplicateOperationTransition(atom) => {
                write!(formatter, "operation repeats transition for `{atom}`")
            }
            Self::IncompleteOperationTransitions => {
                formatter.write_str("operation transition set does not cover every endpoint")
            }
            Self::UnchangedBondOrder => {
                formatter.write_str("bond-order change must change the order")
            }
            Self::ZeroElectronTransfer => {
                formatter.write_str("electron transfer count must be positive")
            }
            Self::InvalidElectronTransfer => {
                formatter.write_str("electron transfer endpoint states do not match its count")
            }
            Self::InvalidElectronReconfiguration => formatter.write_str(
                "electron reconfiguration must preserve formal charge and local electron count",
            ),
            Self::InvalidCovalentElectronLedger => formatter
                .write_str("covalent operation endpoint states do not match its bond allocation"),
            Self::InvalidDativeElectronLedger => formatter
                .write_str("dative operation endpoint states do not match donor-pair allocation"),
            Self::InvalidMetallicElectronLedger => formatter
                .write_str("metallic operation endpoint states or domain ledger are inconsistent"),
            Self::EmptyProductAssignment => {
                formatter.write_str("product assignment atom set must not be empty")
            }
            Self::DuplicateProductAssignmentAtom(atom) => {
                write!(formatter, "product assignment repeats atom `{atom}`")
            }
            Self::Serialization(error) => {
                write!(formatter, "structural serialization failed: {error}")
            }
            Self::Canonicalization(error) => {
                write!(formatter, "structural canonicalization failed: {error}")
            }
        }
    }
}

impl std::error::Error for StructuralError {}
