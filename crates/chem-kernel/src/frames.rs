use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use chem_catalogue::{EventModel, ObservationPredicate, SequenceModel};
use chem_domain::{
    AtomGroupId, AtomId, BondOrder, ContentDigest, CovalentBondId, CovalentElectronOrigin,
    ElectronState, ElementSymbol, IonicAssociationId, MetallicDomainId, StructuralOperation,
    StructuralOperationView, StructureInstanceId, canonical_json,
};
use serde::Serialize;

use crate::{
    DerivationTrust, EvidenceTrust, ExpandedStructuralReaction, KernelError, Provenance,
    StructuralDerivation, StructuralState, ValidatedStructuralReaction, ValidationResult,
};

/// Stable identity of the source, expansion, and catalogue currently selected
/// by the host application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[allow(clippy::struct_field_names)]
pub struct CurrentArtifactIdentity {
    source_bytes_digest: ContentDigest,
    source_semantic_digest: ContentDigest,
    expansion_semantic_digest: ContentDigest,
    evidence_digest: ContentDigest,
    catalogue_digest: ContentDigest,
}

impl CurrentArtifactIdentity {
    /// Derives the complete current identity from a freshly expanded value.
    ///
    /// # Errors
    ///
    /// Returns an expansion error when semantic canonicalization fails.
    pub fn from_expanded(
        expanded: &ExpandedStructuralReaction,
    ) -> Result<Self, crate::ExpansionError> {
        Ok(Self {
            source_bytes_digest: expanded.claim.source.bytes_digest,
            source_semantic_digest: expanded.claim.source.semantic_digest,
            expansion_semantic_digest: expanded.semantic_digest()?,
            evidence_digest: expanded.claim.evidence.digest,
            catalogue_digest: expanded.claim.catalogue.digest,
        })
    }
}

/// Stable frame-boundary failure class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameFailureClass {
    StaleInput,
    CorruptValidatedArtifact,
}

/// Failure to project a current trusted validation into renderer data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FrameError {
    class: FrameFailureClass,
    code: &'static str,
    message: String,
}

impl FrameError {
    fn stale(error: &KernelError) -> Self {
        Self {
            class: FrameFailureClass::StaleInput,
            code: "CHEMS-F001",
            message: error.to_string(),
        }
    }

    fn corrupt(message: impl Into<String>) -> Self {
        Self {
            class: FrameFailureClass::CorruptValidatedArtifact,
            code: "CHEMS-F090",
            message: message.into(),
        }
    }

    #[must_use]
    pub const fn class(&self) -> FrameFailureClass {
        self.class
    }

    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.code
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for FrameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for FrameError {}

/// Mandatory user-facing interpretation of the representative event model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FrameModelDisclosure {
    pub event: EventModel,
    pub sequence: SequenceModel,
    pub representative_outcome: bool,
    pub explanatory_sequence_is_not_a_mechanism_claim: bool,
    pub provenance: Provenance,
}

/// Renderer-facing atom with exact validated local electron labels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FrameAtom {
    pub id: AtomId,
    pub element: ElementSymbol,
    pub electrons: ElectronState,
}

/// Renderer-facing covalent edge. Ionic and metallic relationships use their
/// own types and are never encoded as covalent bonds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FrameCovalentEdge {
    pub id: CovalentBondId,
    pub left: AtomId,
    pub right: AtomId,
    pub order: BondOrder,
    #[serde(flatten, skip_serializing_if = "CovalentElectronOrigin::is_shared")]
    pub electron_origin: CovalentElectronOrigin,
}

/// Exact named atom membership retained for ionic and educational grouping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FrameAtomGroup {
    pub id: AtomGroupId,
    pub atoms: BTreeSet<AtomId>,
}

/// Ionic association rendered as charged components, never fake bonds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FrameIonicAssociation {
    pub id: IonicAssociationId,
    pub components: BTreeMap<AtomGroupId, BTreeSet<AtomId>>,
    pub component_charges: BTreeMap<AtomGroupId, i64>,
}

/// Metallic membership and exact domain-owned electron count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FrameMetallicDomain {
    pub id: MetallicDomainId,
    pub sites: BTreeSet<AtomId>,
    pub delocalized_electrons: u32,
}

/// Complete typed operation active at one immutable state transition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FrameOperation {
    pub ordinal: u32,
    pub operation: StructuralOperation,
}

/// Relationship and electron deltas from the preceding immutable state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FrameChange {
    ElectronState {
        atom: AtomId,
        before: ElectronState,
        after: ElectronState,
    },
    Covalent {
        left: AtomId,
        right: AtomId,
        before: Option<BondOrder>,
        after: Option<BondOrder>,
        #[serde(skip_serializing_if = "Option::is_none")]
        before_electron_origin: Option<CovalentElectronOrigin>,
        #[serde(skip_serializing_if = "Option::is_none")]
        after_electron_origin: Option<CovalentElectronOrigin>,
    },
    Group {
        group: AtomGroupId,
        before: Option<BTreeSet<AtomId>>,
        after: Option<BTreeSet<AtomId>>,
    },
    Ionic {
        association: IonicAssociationId,
        before: Option<BTreeSet<AtomGroupId>>,
        after: Option<BTreeSet<AtomGroupId>>,
    },
    Metallic {
        domain: MetallicDomainId,
        sites_before: Option<BTreeSet<AtomId>>,
        sites_after: Option<BTreeSet<AtomId>>,
        electrons_before: Option<u32>,
        electrons_after: Option<u32>,
    },
    ProductAssignment {
        product: StructureInstanceId,
        before: Option<BTreeSet<AtomId>>,
        after: Option<BTreeSet<AtomId>>,
    },
}

/// Deterministic position of an observation relative to its validated trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationStatus {
    Pending,
    Active,
    Established,
}

/// One evidence-backed observation synchronized to a validated operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FrameObservation {
    pub claim: chem_domain::ClaimId,
    pub predicate: ObservationPredicate,
    pub subject_binding: String,
    pub value: Option<String>,
    pub evidence_digest: ContentDigest,
    /// Runtime evidence remains externally supplied and untrusted even when
    /// the catalogue-backed structural result crosses the trusted boundary.
    pub evidence_trust: EvidenceTrust,
    pub trigger_operation: u32,
    pub status: ObservationStatus,
    pub provenance: Provenance,
}

/// Standalone traceability attached to every renderer frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct FrameTrace {
    pub source_bytes_digest: ContentDigest,
    pub source_semantic_digest: ContentDigest,
    pub expansion_semantic_digest: ContentDigest,
    pub evidence_digest: ContentDigest,
    pub catalogue_digest: ContentDigest,
    pub derivation_digest: ContentDigest,
    pub state_digest: ContentDigest,
}

/// One deterministic renderer-independent structural and observation frame.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SimulationFrame {
    schema_version: u32,
    ordinal: u32,
    trace: FrameTrace,
    active_operation: Option<FrameOperation>,
    model: FrameModelDisclosure,
    atoms: BTreeMap<AtomId, FrameAtom>,
    covalent_edges: BTreeMap<CovalentBondId, FrameCovalentEdge>,
    groups: BTreeMap<AtomGroupId, FrameAtomGroup>,
    ionic_associations: BTreeMap<IonicAssociationId, FrameIonicAssociation>,
    metallic_domains: BTreeMap<MetallicDomainId, FrameMetallicDomain>,
    product_membership: BTreeMap<StructureInstanceId, BTreeSet<AtomId>>,
    changes: Vec<FrameChange>,
    observations: Vec<FrameObservation>,
}

impl SimulationFrame {
    #[must_use]
    pub const fn ordinal(&self) -> u32 {
        self.ordinal
    }

    #[must_use]
    pub const fn trace(&self) -> FrameTrace {
        self.trace
    }

    #[must_use]
    pub const fn active_operation(&self) -> Option<&FrameOperation> {
        self.active_operation.as_ref()
    }

    #[must_use]
    pub const fn model(&self) -> &FrameModelDisclosure {
        &self.model
    }

    #[must_use]
    pub fn changes(&self) -> &[FrameChange] {
        &self.changes
    }

    #[must_use]
    pub fn observations(&self) -> &[FrameObservation] {
        &self.observations
    }

    #[must_use]
    pub const fn atoms(&self) -> &BTreeMap<AtomId, FrameAtom> {
        &self.atoms
    }

    #[must_use]
    pub const fn covalent_edges(&self) -> &BTreeMap<CovalentBondId, FrameCovalentEdge> {
        &self.covalent_edges
    }

    #[must_use]
    pub const fn groups(&self) -> &BTreeMap<AtomGroupId, FrameAtomGroup> {
        &self.groups
    }

    #[must_use]
    pub const fn ionic_associations(&self) -> &BTreeMap<IonicAssociationId, FrameIonicAssociation> {
        &self.ionic_associations
    }

    #[must_use]
    pub const fn metallic_domains(&self) -> &BTreeMap<MetallicDomainId, FrameMetallicDomain> {
        &self.metallic_domains
    }

    #[must_use]
    pub const fn product_membership(&self) -> &BTreeMap<StructureInstanceId, BTreeSet<AtomId>> {
        &self.product_membership
    }
}

/// Canonical paired frame artifact. Its fields are private so callers cannot
/// manufacture a renderer input that bypasses the trusted validation token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SimulationFrames {
    schema_version: u32,
    trust: DerivationTrust,
    result: ValidationResult,
    trace: FrameTrace,
    model: FrameModelDisclosure,
    frames: Vec<SimulationFrame>,
}

impl SimulationFrames {
    #[must_use]
    pub const fn trust(&self) -> DerivationTrust {
        self.trust
    }

    #[must_use]
    pub const fn result(&self) -> ValidationResult {
        self.result
    }

    #[must_use]
    pub fn frames(&self) -> &[SimulationFrame] {
        &self.frames
    }

    #[must_use]
    pub const fn trace(&self) -> FrameTrace {
        self.trace
    }

    #[must_use]
    pub const fn model(&self) -> &FrameModelDisclosure {
        &self.model
    }

    /// Canonically serializes the complete renderer-independent artifact.
    ///
    /// # Errors
    ///
    /// Returns `CHEMS-F090` when serialization cannot be canonicalized.
    pub fn canonical_json(&self) -> Result<Vec<u8>, FrameError> {
        let value =
            serde_json::to_value(self).map_err(|error| FrameError::corrupt(error.to_string()))?;
        canonical_json(&value).map_err(|error| FrameError::corrupt(error.to_string()))
    }

    /// Computes the stable semantic frame digest.
    ///
    /// # Errors
    ///
    /// Returns `CHEMS-F090` when serialization cannot be canonicalized.
    pub fn digest(&self) -> Result<ContentDigest, FrameError> {
        Ok(ContentDigest::sha256(&self.canonical_json()?))
    }
}

/// Projects only a current, privately constructed trusted validation token
/// into renderer-independent frames.
///
/// # Errors
///
/// Returns `CHEMS-F001` when any current identity differs from the validated
/// artifact, or `CHEMS-F090` when the supposedly validated artifact is
/// internally inconsistent.
///
/// Review-candidate derivations cannot cross this API boundary:
///
/// ```compile_fail
/// # use chem_kernel::{
/// #     CurrentArtifactIdentity, ReviewCandidateStructuralDerivation, generate_frames,
/// # };
/// # let candidate: ReviewCandidateStructuralDerivation = todo!();
/// # let identity: CurrentArtifactIdentity = todo!();
/// let _ = generate_frames(&candidate, identity);
/// ```
pub fn generate_frames(
    validated: &ValidatedStructuralReaction,
    current: CurrentArtifactIdentity,
) -> Result<SimulationFrames, FrameError> {
    ensure_current(validated, current)?;
    project_frames(validated)
}

fn ensure_current(
    derivation: &StructuralDerivation,
    current: CurrentArtifactIdentity,
) -> Result<(), FrameError> {
    derivation
        .ensure_current(
            current.source_bytes_digest,
            current.source_semantic_digest,
            current.catalogue_digest,
        )
        .map_err(|error| FrameError::stale(&error))?;
    derivation
        .ensure_expansion_current(current.expansion_semantic_digest)
        .map_err(|error| FrameError::stale(&error))?;
    if derivation.expanded().claim.evidence.digest != current.evidence_digest {
        return Err(FrameError {
            class: FrameFailureClass::StaleInput,
            code: "CHEMS-F001",
            message: "observation evidence identity changed".to_owned(),
        });
    }
    Ok(())
}

pub(crate) fn project_frames(
    derivation: &StructuralDerivation,
) -> Result<SimulationFrames, FrameError> {
    let expanded = derivation.expanded();
    let derivation_digest = derivation
        .digest()
        .map_err(|error| FrameError::corrupt(error.to_string()))?;
    let model = FrameModelDisclosure {
        event: derivation.event_model(),
        sequence: derivation.sequence_model(),
        representative_outcome: true,
        explanatory_sequence_is_not_a_mechanism_claim: true,
        provenance: expanded.claim.model.provenance.clone(),
    };
    let observation_triggers = observation_triggers(expanded)?;
    let mut frames = Vec::with_capacity(derivation.states().len());
    let mut previous = None;
    for state in derivation.states() {
        let active_operation = active_operation(expanded, state)?;
        let trace = FrameTrace {
            source_bytes_digest: derivation.source_bytes_digest(),
            source_semantic_digest: derivation.source_semantic_digest(),
            expansion_semantic_digest: derivation.expansion_semantic_digest(),
            evidence_digest: expanded.claim.evidence.digest,
            catalogue_digest: derivation.catalogue_digest(),
            derivation_digest,
            state_digest: state.digest(),
        };
        frames.push(SimulationFrame {
            schema_version: 1,
            ordinal: state.ordinal(),
            trace,
            active_operation,
            model: model.clone(),
            atoms: frame_atoms(state),
            covalent_edges: frame_bonds(state),
            groups: frame_groups(state),
            ionic_associations: frame_ionic(state),
            metallic_domains: frame_metallic(state),
            product_membership: state.product_assignments().clone(),
            changes: previous.map_or_else(Vec::new, |prior| frame_changes(prior, state)),
            observations: frame_observations(expanded, &observation_triggers, state.ordinal()),
        });
        previous = Some(state);
    }
    let trace = frames
        .last()
        .map(SimulationFrame::trace)
        .ok_or_else(|| FrameError::corrupt("validated derivation has no states"))?;
    Ok(SimulationFrames {
        schema_version: 1,
        trust: derivation.trust(),
        result: derivation.result(),
        trace,
        model,
        frames,
    })
}

fn active_operation(
    expanded: &ExpandedStructuralReaction,
    state: &StructuralState,
) -> Result<Option<FrameOperation>, FrameError> {
    let Some(id) = state.operation() else {
        return Ok(None);
    };
    let index = usize::try_from(state.ordinal().saturating_sub(1))
        .map_err(|_| FrameError::corrupt("operation ordinal exceeds usize"))?;
    let operation = expanded
        .operations
        .get(index)
        .ok_or_else(|| FrameError::corrupt("state operation has no expanded operation"))?;
    if operation.ordinal != state.ordinal() || operation.operation.id() != id {
        return Err(FrameError::corrupt(
            "state operation identity differs from expanded operation",
        ));
    }
    Ok(Some(FrameOperation {
        ordinal: operation.ordinal,
        operation: operation.operation.clone(),
    }))
}

fn frame_atoms(state: &StructuralState) -> BTreeMap<AtomId, FrameAtom> {
    state
        .graph()
        .atoms()
        .values()
        .map(|atom| {
            (
                atom.id().clone(),
                FrameAtom {
                    id: atom.id().clone(),
                    element: atom.element().clone(),
                    electrons: atom.electrons(),
                },
            )
        })
        .collect()
}

fn frame_bonds(state: &StructuralState) -> BTreeMap<CovalentBondId, FrameCovalentEdge> {
    state
        .graph()
        .covalent_bonds()
        .values()
        .map(|bond| {
            (
                bond.id().clone(),
                FrameCovalentEdge {
                    id: bond.id().clone(),
                    left: bond.left().clone(),
                    right: bond.right().clone(),
                    order: bond.order(),
                    electron_origin: bond.electron_origin().clone(),
                },
            )
        })
        .collect()
}

fn frame_groups(state: &StructuralState) -> BTreeMap<AtomGroupId, FrameAtomGroup> {
    state
        .graph()
        .groups()
        .values()
        .map(|group| {
            (
                group.id().clone(),
                FrameAtomGroup {
                    id: group.id().clone(),
                    atoms: group.atoms().clone(),
                },
            )
        })
        .collect()
}

fn frame_ionic(state: &StructuralState) -> BTreeMap<IonicAssociationId, FrameIonicAssociation> {
    state
        .graph()
        .ionic_associations()
        .values()
        .map(|association| {
            let components = association
                .components()
                .iter()
                .map(|group| (group.clone(), state.graph().groups()[group].atoms().clone()))
                .collect();
            let component_charges = association
                .components()
                .iter()
                .map(|group| {
                    let charge = state.graph().groups()[group]
                        .atoms()
                        .iter()
                        .map(|atom| {
                            i64::from(state.graph().atoms()[atom].electrons().formal_charge())
                        })
                        .sum();
                    (group.clone(), charge)
                })
                .collect();
            (
                association.id().clone(),
                FrameIonicAssociation {
                    id: association.id().clone(),
                    components,
                    component_charges,
                },
            )
        })
        .collect()
}

fn frame_metallic(state: &StructuralState) -> BTreeMap<MetallicDomainId, FrameMetallicDomain> {
    state
        .graph()
        .metallic_domains()
        .values()
        .map(|domain| {
            (
                domain.id().clone(),
                FrameMetallicDomain {
                    id: domain.id().clone(),
                    sites: domain.sites().clone(),
                    delocalized_electrons: domain.delocalized_electrons(),
                },
            )
        })
        .collect()
}

type EdgeKey = (AtomId, AtomId);

type EdgeSemantics = (BondOrder, Option<CovalentElectronOrigin>);

fn edge_semantics(state: &StructuralState) -> BTreeMap<EdgeKey, EdgeSemantics> {
    state
        .graph()
        .covalent_bonds()
        .values()
        .map(|bond| {
            let origin = match bond.electron_origin() {
                CovalentElectronOrigin::Shared => None,
                value @ CovalentElectronOrigin::Dative { .. } => Some(value.clone()),
            };
            (
                (bond.left().clone(), bond.right().clone()),
                (bond.order(), origin),
            )
        })
        .collect()
}

#[allow(clippy::too_many_lines)]
fn frame_changes(previous: &StructuralState, current: &StructuralState) -> Vec<FrameChange> {
    let mut changes = Vec::new();
    for (id, atom) in current.graph().atoms() {
        let before = previous.graph().atoms()[id].electrons();
        let after = atom.electrons();
        if before != after {
            changes.push(FrameChange::ElectronState {
                atom: id.clone(),
                before,
                after,
            });
        }
    }
    let before_edges = edge_semantics(previous);
    let after_edges = edge_semantics(current);
    for edge in before_edges
        .keys()
        .chain(after_edges.keys())
        .cloned()
        .collect::<BTreeSet<_>>()
    {
        let before = before_edges.get(&edge).cloned();
        let after = after_edges.get(&edge).cloned();
        if before != after {
            changes.push(FrameChange::Covalent {
                left: edge.0,
                right: edge.1,
                before: before.as_ref().map(|value| value.0),
                after: after.as_ref().map(|value| value.0),
                before_electron_origin: before.and_then(|value| value.1),
                after_electron_origin: after.and_then(|value| value.1),
            });
        }
    }
    for group in previous
        .graph()
        .groups()
        .keys()
        .chain(current.graph().groups().keys())
        .cloned()
        .collect::<BTreeSet<_>>()
    {
        let before = previous
            .graph()
            .groups()
            .get(&group)
            .map(|value| value.atoms().clone());
        let after = current
            .graph()
            .groups()
            .get(&group)
            .map(|value| value.atoms().clone());
        if before != after {
            changes.push(FrameChange::Group {
                group,
                before,
                after,
            });
        }
    }
    for association in previous
        .graph()
        .ionic_associations()
        .keys()
        .chain(current.graph().ionic_associations().keys())
        .cloned()
        .collect::<BTreeSet<_>>()
    {
        let before = previous
            .graph()
            .ionic_associations()
            .get(&association)
            .map(|value| value.components().clone());
        let after = current
            .graph()
            .ionic_associations()
            .get(&association)
            .map(|value| value.components().clone());
        if before != after {
            changes.push(FrameChange::Ionic {
                association,
                before,
                after,
            });
        }
    }
    for domain in previous
        .graph()
        .metallic_domains()
        .keys()
        .chain(current.graph().metallic_domains().keys())
        .cloned()
        .collect::<BTreeSet<_>>()
    {
        let before = previous.graph().metallic_domains().get(&domain);
        let after = current.graph().metallic_domains().get(&domain);
        if before != after {
            changes.push(FrameChange::Metallic {
                domain,
                sites_before: before.map(|value| value.sites().clone()),
                sites_after: after.map(|value| value.sites().clone()),
                electrons_before: before.map(chem_domain::MetallicDomain::delocalized_electrons),
                electrons_after: after.map(chem_domain::MetallicDomain::delocalized_electrons),
            });
        }
    }
    for product in previous
        .product_assignments()
        .keys()
        .chain(current.product_assignments().keys())
        .cloned()
        .collect::<BTreeSet<_>>()
    {
        let before = previous.product_assignments().get(&product).cloned();
        let after = current.product_assignments().get(&product).cloned();
        if before != after {
            changes.push(FrameChange::ProductAssignment {
                product,
                before,
                after,
            });
        }
    }
    changes
}

fn observation_triggers(
    expanded: &ExpandedStructuralReaction,
) -> Result<BTreeMap<chem_domain::ClaimId, u32>, FrameError> {
    let assignments = expanded
        .operations
        .iter()
        .filter_map(|operation| match operation.operation.view() {
            StructuralOperationView::AssignProduct { atoms, product } => {
                Some((operation.ordinal, product, atoms))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let mut triggers = BTreeMap::new();
    for observation in &expanded.claim.evidence.observations {
        let trigger = match observation.predicate {
            ObservationPredicate::Evolves
            | ObservationPredicate::Forms
            | ObservationPredicate::Colour => {
                let products = expanded
                    .product_instances
                    .values()
                    .filter(|instance| instance.binding == observation.subject_binding)
                    .map(|instance| instance.instance.id())
                    .collect::<BTreeSet<_>>();
                let matched = assignments
                    .iter()
                    .filter(|(_, product, _)| products.contains(product))
                    .map(|(ordinal, product, _)| (*ordinal, *product))
                    .collect::<Vec<_>>();
                if matched
                    .iter()
                    .map(|(_, product)| *product)
                    .collect::<BTreeSet<_>>()
                    != products
                {
                    return Err(FrameError::corrupt(format!(
                        "observation `{}` has incomplete product assignment trigger",
                        observation.claim
                    )));
                }
                matched
                    .iter()
                    .map(|(ordinal, _)| *ordinal)
                    .max()
                    .ok_or_else(|| FrameError::corrupt("product observation has no trigger"))?
            }
            ObservationPredicate::Disappears => {
                let subject_atoms = expanded
                    .reactant_instances
                    .values()
                    .filter(|instance| instance.binding == observation.subject_binding)
                    .flat_map(|instance| instance.instance.graph().atoms().keys().cloned())
                    .collect::<BTreeSet<_>>();
                let mut assigned = BTreeSet::new();
                let mut trigger = None;
                for (ordinal, _, atoms) in &assignments {
                    let relevant = atoms
                        .intersection(&subject_atoms)
                        .cloned()
                        .collect::<BTreeSet<_>>();
                    if !relevant.is_empty() {
                        assigned.extend(relevant);
                        trigger = Some(*ordinal);
                    }
                }
                if assigned != subject_atoms {
                    return Err(FrameError::corrupt(format!(
                        "observation `{}` has incomplete reactant disappearance trigger",
                        observation.claim
                    )));
                }
                trigger.ok_or_else(|| FrameError::corrupt("reactant observation has no trigger"))?
            }
        };
        if triggers
            .insert(observation.claim.clone(), trigger)
            .is_some()
        {
            return Err(FrameError::corrupt("duplicate observation claim"));
        }
    }
    Ok(triggers)
}

fn frame_observations(
    expanded: &ExpandedStructuralReaction,
    triggers: &BTreeMap<chem_domain::ClaimId, u32>,
    ordinal: u32,
) -> Vec<FrameObservation> {
    expanded
        .claim
        .evidence
        .observations
        .iter()
        .map(|observation| {
            let trigger_operation = triggers[&observation.claim];
            let status = match ordinal.cmp(&trigger_operation) {
                std::cmp::Ordering::Less => ObservationStatus::Pending,
                std::cmp::Ordering::Equal => ObservationStatus::Active,
                std::cmp::Ordering::Greater => ObservationStatus::Established,
            };
            FrameObservation {
                claim: observation.claim.clone(),
                predicate: observation.predicate,
                subject_binding: observation.subject_binding.clone(),
                value: observation.value.clone(),
                evidence_digest: expanded.claim.evidence.digest,
                evidence_trust: expanded.claim.evidence.trust,
                trigger_operation,
                status,
                provenance: observation.provenance.clone(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, BTreeSet},
        fs,
        path::PathBuf,
    };

    use chem_catalogue::ValidatedCatalogueBundle;

    use crate::{expand_review_candidate, validate_review_candidate};

    use super::{
        ContentDigest, CurrentArtifactIdentity, DerivationTrust, FrameFailureClass,
        ObservationStatus, ensure_current, project_frames,
    };

    fn root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn review_candidate_derivation() -> crate::ReviewCandidateStructuralDerivation {
        let root = root();
        let catalogue = ValidatedCatalogueBundle::from_json(
            &fs::read(root.join("conformance/catalogue/lithium-rule-001.catalogue.json")).unwrap(),
        )
        .unwrap();
        let source =
            fs::read(root.join("conformance/expansion/canonical-expansion-001.chems")).unwrap();
        let observations =
            fs::read(root.join("conformance/observations/lithium-observations-001.input.json"))
                .unwrap();
        let expanded = expand_review_candidate(
            "conformance/expansion/canonical-expansion-001.chems",
            std::str::from_utf8(&source).unwrap(),
            &catalogue,
            &observations,
        )
        .unwrap();
        validate_review_candidate(&expanded, &catalogue).unwrap()
    }

    fn review_candidate_frames() -> super::SimulationFrames {
        project_frames(&review_candidate_derivation()).unwrap()
    }

    fn electron_tuple(value: &serde_json::Value) -> serde_json::Value {
        serde_json::json!([
            value["formal_charge"],
            value["non_bonding_electrons"],
            value["unpaired_electrons"]
        ])
    }

    fn electron_object(tuple: &serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "formal_charge": tuple[0],
            "non_bonding_electrons": tuple[1],
            "unpaired_electrons": tuple[2]
        })
    }

    fn normalized_pair(left: &serde_json::Value, right: &serde_json::Value) -> Vec<String> {
        let mut pair = vec![
            left.as_str().unwrap().to_owned(),
            right.as_str().unwrap().to_owned(),
        ];
        pair.sort();
        pair
    }

    fn compact_frame_state(frame: &super::SimulationFrame) -> serde_json::Value {
        let atoms = frame
            .atoms()
            .iter()
            .map(|(id, atom)| {
                (
                    id.to_string(),
                    serde_json::json!([
                        atom.electrons.formal_charge(),
                        atom.electrons.non_bonding_electrons(),
                        atom.electrons.unpaired_electrons()
                    ]),
                )
            })
            .collect::<serde_json::Map<_, _>>();
        let mut covalent_edges = frame
            .covalent_edges()
            .values()
            .map(|edge| {
                let mut pair = [edge.left.to_string(), edge.right.to_string()];
                pair.sort();
                serde_json::json!([pair[0], pair[1], edge.order])
            })
            .collect::<Vec<_>>();
        covalent_edges.sort_by_key(serde_json::Value::to_string);
        let mut metallic_domains = frame
            .metallic_domains()
            .values()
            .map(|domain| {
                serde_json::json!([domain.id, domain.sites, domain.delocalized_electrons])
            })
            .collect::<Vec<_>>();
        metallic_domains.sort_by_key(serde_json::Value::to_string);
        let mut ionic_associations = frame
            .ionic_associations()
            .values()
            .map(|association| {
                let mut components = association
                    .components
                    .values()
                    .map(|atoms| serde_json::json!(atoms))
                    .collect::<Vec<_>>();
                components.sort_by_key(serde_json::Value::to_string);
                serde_json::json!([components[0], components[1], "ionic"])
            })
            .collect::<Vec<_>>();
        ionic_associations.sort_by_key(serde_json::Value::to_string);
        let product_assignments = frame
            .product_membership()
            .iter()
            .map(|(product, atoms)| serde_json::json!([product, atoms]))
            .collect::<Vec<_>>();
        serde_json::json!({
            "atoms": atoms,
            "covalent_edges": covalent_edges,
            "metallic_domains": metallic_domains,
            "ionic_associations": ionic_associations,
            "product_assignments": product_assignments
        })
    }

    fn compact_oracle_state(state: &serde_json::Value) -> serde_json::Value {
        let mut covalent_edges = state["covalent_edges"].as_array().unwrap().clone();
        for edge in &mut covalent_edges {
            let pair = normalized_pair(&edge[0], &edge[1]);
            *edge = serde_json::json!([pair[0], pair[1], edge[2]]);
        }
        covalent_edges.sort_by_key(serde_json::Value::to_string);
        let mut ionic_associations = state["ionic_associations"].as_array().unwrap().clone();
        for association in &mut ionic_associations {
            let mut components = vec![association[0].clone(), association[1].clone()];
            for component in &mut components {
                component
                    .as_array_mut()
                    .unwrap()
                    .sort_by_key(|atom| atom.as_str().unwrap().to_owned());
            }
            components.sort_by_key(serde_json::Value::to_string);
            *association = serde_json::json!([components[0], components[1], "ionic"]);
        }
        ionic_associations.sort_by_key(serde_json::Value::to_string);
        let mut product_assignments = state["product_assignments"].as_array().unwrap().clone();
        for assignment in &mut product_assignments {
            assignment[1]
                .as_array_mut()
                .unwrap()
                .sort_by_key(|atom| atom.as_str().unwrap().to_owned());
        }
        product_assignments.sort_by_key(serde_json::Value::to_string);
        serde_json::json!({
            "atoms": state["atoms"],
            "covalent_edges": covalent_edges,
            "metallic_domains": state["metallic_domains"],
            "ionic_associations": ionic_associations,
            "product_assignments": product_assignments
        })
    }

    fn oracle_edges(state: &serde_json::Value) -> BTreeMap<(String, String), serde_json::Value> {
        state["covalent_edges"]
            .as_array()
            .unwrap()
            .iter()
            .map(|edge| {
                let pair = normalized_pair(&edge[0], &edge[1]);
                ((pair[0].clone(), pair[1].clone()), edge[2].clone())
            })
            .collect()
    }

    fn oracle_domains(state: &serde_json::Value) -> BTreeMap<String, serde_json::Value> {
        state["metallic_domains"]
            .as_array()
            .unwrap()
            .iter()
            .map(|domain| (domain[0].as_str().unwrap().to_owned(), domain.clone()))
            .collect()
    }

    fn oracle_products(state: &serde_json::Value) -> BTreeMap<String, serde_json::Value> {
        state["product_assignments"]
            .as_array()
            .unwrap()
            .iter()
            .map(|assignment| {
                let mut atoms = assignment[1].as_array().unwrap().clone();
                atoms.sort_by_key(|atom| atom.as_str().unwrap().to_owned());
                (
                    assignment[0].as_str().unwrap().to_owned(),
                    serde_json::Value::Array(atoms),
                )
            })
            .collect()
    }

    fn expected_changes(
        before: &serde_json::Value,
        after: &serde_json::Value,
        ordinal: usize,
        ionic_disclosures: &serde_json::Value,
    ) -> Vec<serde_json::Value> {
        let mut changes = Vec::new();
        for (atom, after_tuple) in after["atoms"].as_object().unwrap() {
            let before_tuple = &before["atoms"][atom];
            if before_tuple != after_tuple {
                changes.push(serde_json::json!({
                    "kind": "electron_state",
                    "atom": atom,
                    "before": electron_object(before_tuple),
                    "after": electron_object(after_tuple)
                }));
            }
        }

        let before_edges = oracle_edges(before);
        let after_edges = oracle_edges(after);
        for (left, right) in before_edges
            .keys()
            .chain(after_edges.keys())
            .cloned()
            .collect::<BTreeSet<_>>()
        {
            let old = before_edges.get(&(left.clone(), right.clone()));
            let new = after_edges.get(&(left.clone(), right.clone()));
            if old != new {
                changes.push(serde_json::json!({
                    "kind": "covalent",
                    "left": left,
                    "right": right,
                    "before": old,
                    "after": new
                }));
            }
        }

        if matches!(ordinal, 8 | 9) {
            let association_id = if ordinal == 8 {
                "ionic[8].ionic.product1"
            } else {
                "ionic[9].ionic.product2"
            };
            let disclosure = &ionic_disclosures[association_id];
            for (group, atoms) in disclosure["components"].as_object().unwrap() {
                changes.push(serde_json::json!({
                    "kind": "group",
                    "group": group,
                    "before": null,
                    "after": atoms
                }));
            }
            changes.push(serde_json::json!({
                "kind": "ionic",
                "association": association_id,
                "before": null,
                "after": disclosure["components"].as_object().unwrap().keys().collect::<Vec<_>>()
            }));
        }

        let before_domains = oracle_domains(before);
        let after_domains = oracle_domains(after);
        for domain in before_domains
            .keys()
            .chain(after_domains.keys())
            .cloned()
            .collect::<BTreeSet<_>>()
        {
            let old = before_domains.get(&domain);
            let new = after_domains.get(&domain);
            if old != new {
                changes.push(serde_json::json!({
                    "kind": "metallic",
                    "domain": domain,
                    "sites_before": old.map(|value| &value[1]),
                    "sites_after": new.map(|value| &value[1]),
                    "electrons_before": old.map(|value| &value[2]),
                    "electrons_after": new.map(|value| &value[2])
                }));
            }
        }

        let before_products = oracle_products(before);
        let after_products = oracle_products(after);
        for product in before_products
            .keys()
            .chain(after_products.keys())
            .cloned()
            .collect::<BTreeSet<_>>()
        {
            let old = before_products.get(&product);
            let new = after_products.get(&product);
            if old != new {
                changes.push(serde_json::json!({
                    "kind": "product_assignment",
                    "product": product,
                    "before": old,
                    "after": new
                }));
            }
        }
        changes
    }

    #[allow(clippy::too_many_lines)]
    fn assert_operation_payload(
        actual: &serde_json::Value,
        oracle: &serde_json::Value,
        before: &serde_json::Value,
        after: &serde_json::Value,
        ordinal: usize,
    ) {
        assert_eq!(actual["ordinal"], ordinal);
        let operation = &actual["operation"];
        assert_eq!(operation["id"], oracle["id"]);
        assert_eq!(operation["kind"], oracle["kind"]);
        match oracle["kind"].as_str().unwrap() {
            "release_metallic" => {
                for field in ["site", "domain", "allocation"] {
                    assert_eq!(operation[field], oracle[field]);
                }
                let atom = oracle["site"].as_str().unwrap();
                assert_eq!(
                    electron_tuple(&operation["transition"]["before"]),
                    before["atoms"][atom]
                );
                assert_eq!(
                    electron_tuple(&operation["transition"]["after"]),
                    after["atoms"][atom]
                );
                assert_eq!(
                    electron_tuple(&operation["transition"]["after"]),
                    oracle["endpoint_after"]
                );
            }
            "cleave_covalent" => {
                assert_eq!(operation["left"], oracle["edge"][0]);
                assert_eq!(operation["right"], oracle["edge"][1]);
                assert_eq!(operation["expected_order"], oracle["edge"][2]);
                assert_eq!(operation["allocation"], oracle["allocation"]);
                for side in ["left", "right"] {
                    let atom = oracle["edge"][usize::from(side != "left")]
                        .as_str()
                        .unwrap();
                    assert_eq!(
                        electron_tuple(&operation["transitions"][atom]["before"]),
                        before["atoms"][atom]
                    );
                    assert_eq!(
                        electron_tuple(&operation["transitions"][atom]["after"]),
                        oracle["endpoints_after"][side]
                    );
                    assert_eq!(
                        electron_tuple(&operation["transitions"][atom]["after"]),
                        after["atoms"][atom]
                    );
                }
            }
            "transfer_electron" => {
                for field in ["donor", "acceptor", "count"] {
                    assert_eq!(operation[field], oracle[field]);
                }
                for role in ["donor", "acceptor"] {
                    let atom = oracle[role].as_str().unwrap();
                    assert_eq!(
                        electron_tuple(&operation["transitions"][atom]["before"]),
                        before["atoms"][atom]
                    );
                    assert_eq!(
                        electron_tuple(&operation["transitions"][atom]["after"]),
                        oracle["endpoints_after"][role]
                    );
                    assert_eq!(
                        electron_tuple(&operation["transitions"][atom]["after"]),
                        after["atoms"][atom]
                    );
                }
            }
            "form_covalent" => {
                assert_eq!(operation["left"], oracle["edge"][0]);
                assert_eq!(operation["right"], oracle["edge"][1]);
                assert_eq!(operation["order"], oracle["edge"][2]);
                for side in ["left", "right"] {
                    let atom = oracle["edge"][usize::from(side != "left")]
                        .as_str()
                        .unwrap();
                    assert_eq!(
                        electron_tuple(&operation["transitions"][atom]["before"]),
                        before["atoms"][atom]
                    );
                    assert_eq!(
                        electron_tuple(&operation["transitions"][atom]["after"]),
                        oracle["endpoints_after"][side]
                    );
                    assert_eq!(
                        electron_tuple(&operation["transitions"][atom]["after"]),
                        after["atoms"][atom]
                    );
                }
            }
            "associate_ionic" => {
                assert!(operation["association"]["id"].is_string());
                assert_eq!(
                    operation["association"]["components"]
                        .as_array()
                        .map(Vec::len),
                    Some(2)
                );
            }
            "assign_product" => {
                let mut actual_atoms = operation["atoms"].as_array().unwrap().clone();
                let mut oracle_atoms = oracle["atoms"].as_array().unwrap().clone();
                actual_atoms.sort_by_key(|atom| atom.as_str().unwrap().to_owned());
                oracle_atoms.sort_by_key(|atom| atom.as_str().unwrap().to_owned());
                assert_eq!(actual_atoms, oracle_atoms);
                assert_eq!(operation["product"], oracle["product"]);
            }
            kind => panic!("unsupported oracle operation kind: {kind}"),
        }
    }

    #[test]
    fn review_candidate_projection_is_deterministic_and_never_trusted() {
        let first = review_candidate_frames();
        let second = review_candidate_frames();
        assert_eq!(first.trust(), DerivationTrust::ReviewCandidate);
        assert_eq!(
            first.canonical_json().unwrap(),
            second.canonical_json().unwrap()
        );
        assert_eq!(first.digest().unwrap(), second.digest().unwrap());
        assert_eq!(first.frames().len(), 13);
    }

    #[test]
    fn presentation_timing_and_layout_are_absent_from_chemistry_frames() {
        fn assert_no_presentation_keys(value: &serde_json::Value) {
            match value {
                serde_json::Value::Object(object) => {
                    for (key, value) in object {
                        assert!(!matches!(
                            key.as_str(),
                            "layout" | "position" | "speed" | "duration" | "interpolation"
                        ));
                        assert_no_presentation_keys(value);
                    }
                }
                serde_json::Value::Array(values) => {
                    values.iter().for_each(assert_no_presentation_keys);
                }
                _ => {}
            }
        }

        let frames = review_candidate_frames();
        let value: serde_json::Value =
            serde_json::from_slice(&frames.canonical_json().unwrap()).unwrap();
        assert_no_presentation_keys(&value);
    }

    #[test]
    fn observations_are_synchronized_to_complete_validated_assignments() {
        let frames = review_candidate_frames();
        let lithium = frames
            .frames()
            .iter()
            .map(|frame| &frame.observations()[1])
            .collect::<Vec<_>>();
        assert_eq!(lithium[10].status, ObservationStatus::Pending);
        assert_eq!(lithium[11].status, ObservationStatus::Active);
        assert_eq!(lithium[12].status, ObservationStatus::Established);
        assert_eq!(lithium[11].trigger_operation, 11);

        let hydrogen = frames
            .frames()
            .iter()
            .map(|frame| &frame.observations()[0])
            .collect::<Vec<_>>();
        assert_eq!(hydrogen[11].status, ObservationStatus::Pending);
        assert_eq!(hydrogen[12].status, ObservationStatus::Active);
        assert_eq!(hydrogen[12].trigger_operation, 12);
    }

    #[test]
    fn every_frame_is_traceable_and_contains_only_validated_state_data() {
        let frames = review_candidate_frames();
        for (ordinal, frame) in frames.frames().iter().enumerate() {
            assert_eq!(frame.ordinal(), u32::try_from(ordinal).unwrap());
            assert!(!frame.atoms().is_empty());
            assert_ne!(frame.trace().state_digest, ContentDigest::sha256(b""));
            assert_eq!(frame.active_operation().is_some(), ordinal != 0);
        }
        assert_eq!(frames.frames()[0].changes().len(), 0);
        assert!(!frames.frames()[1].changes().is_empty());
        assert_eq!(frames.frames()[12].product_membership().len(), 3);
    }

    #[test]
    fn every_frame_value_exactly_projects_its_validated_state() {
        let derivation = review_candidate_derivation();
        let frames = project_frames(&derivation).unwrap();
        for (state, frame) in derivation.states().iter().zip(frames.frames()) {
            assert_eq!(state.digest(), frame.trace().state_digest);
            assert_eq!(state.graph().atoms().len(), frame.atoms.len());
            for (id, atom) in state.graph().atoms() {
                assert_eq!(frame.atoms[id].id, *id);
                assert_eq!(frame.atoms[id].element, *atom.element());
                assert_eq!(frame.atoms[id].electrons, atom.electrons());
            }
            assert_eq!(
                state.graph().covalent_bonds().len(),
                frame.covalent_edges.len()
            );
            for (id, bond) in state.graph().covalent_bonds() {
                let edge = &frame.covalent_edges[id];
                assert_eq!(edge.left, *bond.left());
                assert_eq!(edge.right, *bond.right());
                assert_eq!(edge.order, bond.order());
            }
            assert_eq!(state.graph().groups().len(), frame.groups.len());
            for (id, group) in state.graph().groups() {
                assert_eq!(frame.groups[id].atoms, *group.atoms());
            }
            assert_eq!(
                state.graph().ionic_associations().len(),
                frame.ionic_associations.len()
            );
            for (id, association) in state.graph().ionic_associations() {
                assert_eq!(
                    frame.ionic_associations[id]
                        .components
                        .keys()
                        .collect::<BTreeSet<_>>(),
                    association.components().iter().collect()
                );
                for group in association.components() {
                    let charge = state.graph().groups()[group]
                        .atoms()
                        .iter()
                        .map(|atom| {
                            i64::from(state.graph().atoms()[atom].electrons().formal_charge())
                        })
                        .sum::<i64>();
                    assert_eq!(
                        frame.ionic_associations[id].component_charges[group],
                        charge
                    );
                }
            }
            assert_eq!(
                state.graph().metallic_domains().len(),
                frame.metallic_domains.len()
            );
            for (id, domain) in state.graph().metallic_domains() {
                assert_eq!(frame.metallic_domains[id].sites, *domain.sites());
                assert_eq!(
                    frame.metallic_domains[id].delocalized_electrons,
                    domain.delocalized_electrons()
                );
            }
            assert_eq!(state.product_assignments(), &frame.product_membership);
        }
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn independent_frame_oracle_matches_operations_changes_and_observations() {
        let expected: serde_json::Value = serde_json::from_slice(
            &fs::read(root().join("conformance/frames/canonical-frames-001.expected.json"))
                .unwrap(),
        )
        .unwrap();
        let kernel_oracle: serde_json::Value = serde_json::from_slice(
            &fs::read(
                root().join("conformance/validation-kernel/canonical-kernel-001.expected.json"),
            )
            .unwrap(),
        )
        .unwrap();
        let frames = review_candidate_frames();
        assert_eq!(expected["frame_count"], frames.frames().len());
        assert_eq!(expected["trust"], "review_candidate");
        assert_eq!(expected["result"], serde_json::json!(frames.result()));
        assert_eq!(expected["trace"], serde_json::json!(frames.trace()));
        assert_eq!(expected["model"], serde_json::json!(frames.model()));
        assert_eq!(
            expected["state_digests"],
            serde_json::json!(
                frames
                    .frames()
                    .iter()
                    .map(|frame| frame.trace().state_digest)
                    .collect::<Vec<_>>()
            )
        );
        for (index, (frame, state)) in frames
            .frames()
            .iter()
            .zip(kernel_oracle["states"].as_array().unwrap())
            .enumerate()
        {
            assert_eq!(
                compact_frame_state(frame),
                compact_oracle_state(state),
                "state {index}"
            );
            if index > 0 {
                let operation = serde_json::to_value(frame.active_operation().unwrap()).unwrap();
                assert_operation_payload(
                    &operation,
                    &kernel_oracle["operations"][index - 1],
                    &kernel_oracle["states"][index - 1],
                    state,
                    index,
                );
                if matches!(index, 8 | 9) {
                    let association_id = if index == 8 {
                        "ionic[8].ionic.product1"
                    } else {
                        "ionic[9].ionic.product2"
                    };
                    let disclosure = &expected["ionic_disclosures"][association_id];
                    assert_eq!(
                        operation["operation"]["association"],
                        serde_json::json!({
                            "id": association_id,
                            "components": disclosure["components"]
                                .as_object()
                                .unwrap()
                                .keys()
                                .collect::<Vec<_>>()
                        })
                    );
                    assert_eq!(
                        serde_json::to_value(frame.ionic_associations()).unwrap()[association_id],
                        *disclosure
                    );
                }
                assert_eq!(
                    serde_json::to_value(frame.changes()).unwrap(),
                    serde_json::json!(expected_changes(
                        &kernel_oracle["states"][index - 1],
                        state,
                        index,
                        &expected["ionic_disclosures"]
                    )),
                    "change payload {index}"
                );
            }
        }
        assert_eq!(
            expected["ionic_disclosures"],
            serde_json::to_value(frames.frames().last().unwrap().ionic_associations()).unwrap()
        );
        let operations = frames
            .frames()
            .iter()
            .map(|frame| {
                frame
                    .active_operation()
                    .map_or(serde_json::Value::Null, |operation| {
                        serde_json::json!(operation.operation.id())
                    })
            })
            .collect::<Vec<_>>();
        assert_eq!(expected["active_operations"], serde_json::json!(operations));
        let change_kinds = frames
            .frames()
            .iter()
            .map(|frame| {
                serde_json::to_value(frame.changes())
                    .unwrap()
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|change| change["kind"].clone())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        assert_eq!(expected["change_kinds"], serde_json::json!(change_kinds));
        for (claim, index) in [("R1", 0), ("R2", 1)] {
            let observation = &expected["observations"][claim];
            let actual = frames
                .frames()
                .iter()
                .map(|frame| &frame.observations()[index])
                .collect::<Vec<_>>();
            assert_eq!(
                observation["trigger_operation"],
                actual[0].trigger_operation
            );
            assert_eq!(
                observation["predicate"],
                serde_json::json!(actual[0].predicate)
            );
            assert_eq!(observation["subject_binding"], actual[0].subject_binding);
            for field in [
                "claim",
                "value",
                "evidence_digest",
                "evidence_trust",
                "provenance",
            ] {
                assert_eq!(
                    observation[field],
                    serde_json::to_value(actual[0]).unwrap()[field]
                );
            }
            assert_eq!(
                observation["statuses"],
                serde_json::json!(actual.iter().map(|item| item.status).collect::<Vec<_>>())
            );
        }
    }

    #[test]
    fn every_stale_identity_is_rejected_before_projection() {
        let root = root();
        let catalogue = ValidatedCatalogueBundle::from_json(
            &fs::read(root.join("conformance/catalogue/lithium-rule-001.catalogue.json")).unwrap(),
        )
        .unwrap();
        let source =
            fs::read(root.join("conformance/expansion/canonical-expansion-001.chems")).unwrap();
        let observations =
            fs::read(root.join("conformance/observations/lithium-observations-001.input.json"))
                .unwrap();
        let expanded = expand_review_candidate(
            "conformance/expansion/canonical-expansion-001.chems",
            std::str::from_utf8(&source).unwrap(),
            &catalogue,
            &observations,
        )
        .unwrap();
        let derivation = validate_review_candidate(&expanded, &catalogue).unwrap();
        let current = CurrentArtifactIdentity::from_expanded(&expanded).unwrap();
        assert!(ensure_current(&derivation, current).is_ok());

        for stale in [
            CurrentArtifactIdentity {
                source_bytes_digest: ContentDigest::sha256(b"bytes"),
                ..current
            },
            CurrentArtifactIdentity {
                source_semantic_digest: ContentDigest::sha256(b"semantics"),
                ..current
            },
            CurrentArtifactIdentity {
                expansion_semantic_digest: ContentDigest::sha256(b"expansion"),
                ..current
            },
            CurrentArtifactIdentity {
                evidence_digest: ContentDigest::sha256(b"evidence"),
                ..current
            },
            CurrentArtifactIdentity {
                catalogue_digest: ContentDigest::sha256(b"catalogue"),
                ..current
            },
        ] {
            let error = ensure_current(&derivation, stale).unwrap_err();
            assert_eq!(error.class(), FrameFailureClass::StaleInput);
            assert_eq!(error.code(), "CHEMS-F001");
        }
    }
}
