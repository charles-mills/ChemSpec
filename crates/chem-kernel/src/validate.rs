use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    ops::Deref,
    str::FromStr,
};

use chem_catalogue::{
    EventModel, SequenceModel, TrustedCatalogue, ValencePremiseRecord, ValidatedCatalogueBundle,
};
use chem_domain::{
    Atom, AtomGroup, AtomGroupId, AtomId, AtomMapping, ContentDigest, CovalentBond, CovalentBondId,
    CovalentElectronOrigin, IonicAssociation, IonicAssociationId, MetallicDomain, MetallicDomainId,
    MetallicReleaseAllocation, ReactionSide, StructuralGraph, StructuralOperationId,
    StructuralOperationView, StructureInstanceId, canonical_json,
};
use serde::Serialize;

use crate::{
    CatalogueTrust, ExpandedOperation, ExpandedStructuralReaction,
    TrustedExpandedStructuralReaction,
};

/// Stable classification for Slice 5 validation failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KernelFailureClass {
    InvalidExpansion,
    UnsupportedState,
    StaleInput,
    CorruptTrustedData,
}

/// One deterministic structural-kernel failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KernelError {
    class: KernelFailureClass,
    code: &'static str,
    message: String,
    operation: Option<u32>,
}

impl KernelError {
    fn invalid(code: &'static str, message: impl Into<String>, operation: Option<u32>) -> Self {
        Self {
            class: KernelFailureClass::InvalidExpansion,
            code,
            message: message.into(),
            operation,
        }
    }

    fn unsupported(code: &'static str, message: impl Into<String>, operation: Option<u32>) -> Self {
        Self {
            class: KernelFailureClass::UnsupportedState,
            code,
            message: message.into(),
            operation,
        }
    }

    fn stale(message: impl Into<String>) -> Self {
        Self {
            class: KernelFailureClass::StaleInput,
            code: "CHEMS-K080",
            message: message.into(),
            operation: None,
        }
    }

    fn corrupt(code: &'static str, message: impl Into<String>, operation: Option<u32>) -> Self {
        Self {
            class: KernelFailureClass::CorruptTrustedData,
            code,
            message: message.into(),
            operation,
        }
    }

    fn at_trusted_boundary(mut self) -> Self {
        if matches!(
            self.class,
            KernelFailureClass::InvalidExpansion | KernelFailureClass::UnsupportedState
        ) {
            self.class = KernelFailureClass::CorruptTrustedData;
        }
        self
    }

    #[must_use]
    pub const fn class(&self) -> KernelFailureClass {
        self.class
    }

    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.code
    }

    #[must_use]
    pub const fn operation(&self) -> Option<u32> {
        self.operation
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for KernelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(operation) = self.operation {
            write!(
                formatter,
                "{} at operation {operation}: {}",
                self.code, self.message
            )
        } else {
            write!(formatter, "{}: {}", self.code, self.message)
        }
    }
}

impl std::error::Error for KernelError {}

/// Exact conserved quantities recorded for every immutable graph state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct StructuralLedger {
    pub atom_count: u64,
    pub atom_local_non_bonding: u64,
    pub covalent_bond_electrons: u64,
    pub metallic_domain_electrons: u64,
    pub total_explicit_valence_electrons: u64,
    pub atom_formal_charge_sum: i64,
    pub system_net_charge: i128,
}

impl StructuralLedger {
    fn from_graph(graph: &StructuralGraph) -> Self {
        let atom_local_non_bonding = graph
            .atoms()
            .values()
            .map(|atom| u64::from(atom.electrons().non_bonding_electrons()))
            .sum();
        let covalent_bond_electrons = graph
            .covalent_bonds()
            .values()
            .map(|bond| u64::from(bond.order().electrons()))
            .sum();
        let metallic_domain_electrons = graph.delocalized_domain_electron_count();
        Self {
            atom_count: u64::try_from(graph.atoms().len()).unwrap_or(u64::MAX),
            atom_local_non_bonding,
            covalent_bond_electrons,
            metallic_domain_electrons,
            total_explicit_valence_electrons: graph.explicit_valence_electron_count(),
            atom_formal_charge_sum: graph.atom_formal_charge_sum(),
            system_net_charge: graph.system_net_charge(),
        }
    }
}

/// One immutable pre/post operation state in a structural derivation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StructuralState {
    ordinal: u32,
    operation: Option<StructuralOperationId>,
    graph: StructuralGraph,
    product_assignments: BTreeMap<StructureInstanceId, BTreeSet<AtomId>>,
    ledger: StructuralLedger,
    digest: ContentDigest,
}

impl StructuralState {
    #[must_use]
    pub const fn ordinal(&self) -> u32 {
        self.ordinal
    }

    #[must_use]
    pub const fn operation(&self) -> Option<&StructuralOperationId> {
        self.operation.as_ref()
    }

    #[must_use]
    pub const fn graph(&self) -> &StructuralGraph {
        &self.graph
    }

    #[must_use]
    pub const fn product_assignments(&self) -> &BTreeMap<StructureInstanceId, BTreeSet<AtomId>> {
        &self.product_assignments
    }

    #[must_use]
    pub const fn ledger(&self) -> StructuralLedger {
        self.ledger
    }

    #[must_use]
    pub const fn digest(&self) -> ContentDigest {
        self.digest
    }
}

/// Successful deterministic execution and proof record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StructuralDerivation {
    schema_version: u32,
    source_bytes_digest: ContentDigest,
    source_semantic_digest: ContentDigest,
    expansion_semantic_digest: ContentDigest,
    catalogue_digest: ContentDigest,
    rule: String,
    event_model: EventModel,
    sequence_model: SequenceModel,
    trust: DerivationTrust,
    expanded: ExpandedStructuralReaction,
    mapping: AtomMapping,
    states: Vec<StructuralState>,
    result: ValidationResult,
}

/// Initial successful public result domain. The mandatory model disclosures
/// mean the canonical rule always validates with assumptions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationResult {
    ValidatedWithAssumptions,
}

/// Trust provenance of a successful derivation. It is serialized into the
/// derivation itself so review-candidate JSON cannot masquerade as trusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DerivationTrust {
    ReviewCandidate,
    Trusted,
}

impl StructuralDerivation {
    #[must_use]
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    #[must_use]
    pub fn states(&self) -> &[StructuralState] {
        &self.states
    }

    #[must_use]
    pub fn rule(&self) -> &str {
        &self.rule
    }

    #[must_use]
    pub const fn event_model(&self) -> EventModel {
        self.event_model
    }

    #[must_use]
    pub const fn sequence_model(&self) -> SequenceModel {
        self.sequence_model
    }

    #[must_use]
    pub const fn mapping(&self) -> &AtomMapping {
        &self.mapping
    }

    #[must_use]
    pub const fn source_semantic_digest(&self) -> ContentDigest {
        self.source_semantic_digest
    }

    #[must_use]
    pub const fn source_bytes_digest(&self) -> ContentDigest {
        self.source_bytes_digest
    }

    #[must_use]
    pub const fn expansion_semantic_digest(&self) -> ContentDigest {
        self.expansion_semantic_digest
    }

    #[must_use]
    pub const fn catalogue_digest(&self) -> ContentDigest {
        self.catalogue_digest
    }

    #[must_use]
    pub const fn result(&self) -> ValidationResult {
        self.result
    }

    #[must_use]
    pub const fn trust(&self) -> DerivationTrust {
        self.trust
    }

    /// Returns the complete immutable expansion that was checked to produce
    /// this derivation, including operations, observations, and proof premises.
    #[must_use]
    pub const fn expanded(&self) -> &ExpandedStructuralReaction {
        &self.expanded
    }

    #[must_use]
    pub fn is_current(
        &self,
        source_bytes_digest: ContentDigest,
        source_semantic_digest: ContentDigest,
        catalogue_digest: ContentDigest,
    ) -> bool {
        self.source_bytes_digest == source_bytes_digest
            && self.source_semantic_digest == source_semantic_digest
            && self.catalogue_digest == catalogue_digest
    }

    /// Rejects reuse after source semantics or catalogue identity changes.
    ///
    /// # Errors
    ///
    /// Returns `CHEMS-K080` when either identity differs from this derivation.
    pub fn ensure_current(
        &self,
        source_bytes_digest: ContentDigest,
        source_semantic_digest: ContentDigest,
        catalogue_digest: ContentDigest,
    ) -> Result<(), KernelError> {
        if self.is_current(
            source_bytes_digest,
            source_semantic_digest,
            catalogue_digest,
        ) {
            Ok(())
        } else {
            Err(KernelError::stale(
                "source semantics or catalogue identity changed",
            ))
        }
    }

    /// Rejects reuse when any semantic expansion input, including evidence,
    /// differs from the expansion that produced this derivation.
    ///
    /// # Errors
    ///
    /// Returns `CHEMS-K080` when the expansion digest differs.
    pub fn ensure_expansion_current(
        &self,
        expansion_semantic_digest: ContentDigest,
    ) -> Result<(), KernelError> {
        if self.expansion_semantic_digest == expansion_semantic_digest {
            Ok(())
        } else {
            Err(KernelError::stale("expanded chemistry inputs changed"))
        }
    }

    /// Serializes the immutable semantic derivation.
    ///
    /// # Errors
    ///
    /// Returns a kernel system error if canonicalization fails.
    pub fn canonical_json(&self) -> Result<Vec<u8>, KernelError> {
        let value = serde_json::to_value(self)
            .map_err(|error| KernelError::corrupt("CHEMS-K090", error.to_string(), None))?;
        canonical_json(&value)
            .map_err(|error| KernelError::corrupt("CHEMS-K090", error.to_string(), None))
    }

    /// Computes the immutable derivation digest.
    ///
    /// # Errors
    ///
    /// Returns a kernel system error if canonicalization fails.
    pub fn digest(&self) -> Result<ContentDigest, KernelError> {
        Ok(ContentDigest::sha256(&self.canonical_json()?))
    }
}

/// Successful execution through the explicitly untrusted chemistry-review
/// path. This cannot be converted into [`ValidatedStructuralReaction`].
#[derive(Debug, Clone)]
pub struct ReviewCandidateStructuralDerivation {
    derivation: StructuralDerivation,
}

impl Deref for ReviewCandidateStructuralDerivation {
    type Target = StructuralDerivation;

    fn deref(&self) -> &Self::Target {
        &self.derivation
    }
}

/// Public trusted chemistry capability. Fields are private and construction is
/// possible only through [`validate_trusted`].
#[derive(Debug, Clone)]
pub struct ValidatedStructuralReaction {
    derivation: StructuralDerivation,
}

impl ValidatedStructuralReaction {
    /// Returns the complete immutable input accepted by the trusted kernel.
    #[must_use]
    pub const fn expanded(&self) -> &ExpandedStructuralReaction {
        self.derivation.expanded()
    }
}

impl Deref for ValidatedStructuralReaction {
    type Target = StructuralDerivation;

    fn deref(&self) -> &Self::Target {
        &self.derivation
    }
}

/// Executes an explicitly untrusted review-candidate expansion for conformance
/// and chemistry-review work. Success does not create trusted chemistry.
///
/// # Errors
///
/// Returns a typed kernel failure when any operation or mandatory invariant
/// fails.
pub fn validate_review_candidate(
    expanded: &ExpandedStructuralReaction,
    catalogue: &ValidatedCatalogueBundle,
) -> Result<ReviewCandidateStructuralDerivation, KernelError> {
    if expanded.claim.catalogue.trust != CatalogueTrust::ReviewCandidate {
        return Err(KernelError::invalid(
            "CHEMS-K001",
            "review-candidate validation requires review-candidate expansion",
            None,
        ));
    }
    let derivation = validate(expanded, catalogue)?;
    Ok(ReviewCandidateStructuralDerivation { derivation })
}

/// Executes a host-trusted expansion and privately constructs the public
/// validated-reaction capability only after every invariant succeeds.
///
/// # Errors
///
/// Returns a typed kernel failure when identity, operation, valence,
/// conservation, product, or staleness checks fail.
pub fn validate_trusted(
    expanded: &TrustedExpandedStructuralReaction,
    catalogue: &TrustedCatalogue,
) -> Result<ValidatedStructuralReaction, KernelError> {
    if expanded.claim.catalogue.trust != CatalogueTrust::Trusted {
        return Err(KernelError::corrupt(
            "CHEMS-K091",
            "trusted expansion lost its trusted catalogue marker",
            None,
        ));
    }
    let derivation = validate(expanded, catalogue).map_err(KernelError::at_trusted_boundary)?;
    Ok(ValidatedStructuralReaction { derivation })
}

fn validate(
    expanded: &ExpandedStructuralReaction,
    catalogue: &ValidatedCatalogueBundle,
) -> Result<StructuralDerivation, KernelError> {
    validate_identity(expanded, catalogue)?;
    validate_operation_sequence(expanded)?;
    revalidate_mapping(expanded)?;
    let valence = bound_valence_premises(expanded, catalogue)?;
    let mut working = WorkingState::from_expanded(expanded)?;
    let initial_graph = working.graph(None)?;
    validate_valence(&initial_graph, &valence, None)?;
    let baseline = StructuralLedger::from_graph(&initial_graph);
    let mut states = vec![make_state(
        0,
        None,
        initial_graph,
        &working.product_assignments,
    )?];

    for operation in &expanded.operations {
        working.apply(operation)?;
        let graph = working.graph(Some(operation.ordinal))?;
        validate_valence(&graph, &valence, Some(operation.ordinal))?;
        let ledger = StructuralLedger::from_graph(&graph);
        validate_conservation(baseline, ledger, operation.ordinal)?;
        states.push(make_state(
            operation.ordinal,
            Some(operation.operation.id().clone()),
            graph,
            &working.product_assignments,
        )?);
    }

    validate_products(expanded, &working)?;
    let expansion_semantic_digest = expanded.semantic_digest()?;
    Ok(StructuralDerivation {
        schema_version: 1,
        source_bytes_digest: expanded.claim.source.bytes_digest,
        source_semantic_digest: expanded.claim.source.semantic_digest,
        expansion_semantic_digest,
        catalogue_digest: expanded.claim.catalogue.digest,
        rule: expanded.claim.rule.rule.to_string(),
        event_model: expanded.claim.model.event,
        sequence_model: expanded.claim.model.sequence,
        trust: match expanded.claim.catalogue.trust {
            CatalogueTrust::ReviewCandidate => DerivationTrust::ReviewCandidate,
            CatalogueTrust::Trusted => DerivationTrust::Trusted,
        },
        expanded: expanded.clone(),
        mapping: expanded.mapping.clone(),
        states,
        result: ValidationResult::ValidatedWithAssumptions,
    })
}

fn validate_operation_sequence(expanded: &ExpandedStructuralReaction) -> Result<(), KernelError> {
    for (index, operation) in expanded.operations.iter().enumerate() {
        let ordinal = u32::try_from(index + 1)
            .map_err(|_| KernelError::corrupt("CHEMS-K090", "operation count exceeds u32", None))?;
        if operation.ordinal != ordinal
            || operation.operation.id().as_str() != format!("operation[{ordinal}]")
        {
            return Err(KernelError::invalid(
                "CHEMS-K012",
                "operation sequence is not contiguous and identity-stable",
                Some(operation.ordinal),
            ));
        }
    }
    Ok(())
}

impl From<crate::ExpansionError> for KernelError {
    fn from(error: crate::ExpansionError) -> Self {
        Self::corrupt("CHEMS-K090", error.to_string(), None)
    }
}

fn validate_identity(
    expanded: &ExpandedStructuralReaction,
    catalogue: &ValidatedCatalogueBundle,
) -> Result<(), KernelError> {
    if expanded.claim.catalogue.digest != catalogue.digest() {
        return Err(KernelError::stale(format!(
            "expansion catalogue {} differs from loaded catalogue {}",
            expanded.claim.catalogue.digest,
            catalogue.digest()
        )));
    }
    if expanded.schema_version != 1 {
        return Err(KernelError::invalid(
            "CHEMS-K001",
            "unsupported expansion schema",
            None,
        ));
    }
    if expanded.claim.model.event != EventModel::Representative
        || expanded.claim.model.sequence != SequenceModel::Explanatory
    {
        return Err(KernelError::invalid(
            "CHEMS-K002",
            "mandatory model assumptions are absent",
            None,
        ));
    }
    Ok(())
}

fn revalidate_mapping(expanded: &ExpandedStructuralReaction) -> Result<(), KernelError> {
    let reactants = ReactionSide::new(
        expanded
            .reactant_instances
            .values()
            .map(|instance| instance.instance.clone()),
    )
    .map_err(|error| KernelError::invalid("CHEMS-K010", error.to_string(), None))?;
    let products = ReactionSide::new(
        expanded
            .product_instances
            .values()
            .map(|instance| instance.instance.clone()),
    )
    .map_err(|error| KernelError::invalid("CHEMS-K010", error.to_string(), None))?;
    let mapping = AtomMapping::new(
        expanded.mapping.id().clone(),
        expanded.mapping.entries().clone(),
        &reactants,
        &products,
    )
    .map_err(|error| KernelError::invalid("CHEMS-K010", error.to_string(), None))?;
    if mapping != expanded.mapping {
        return Err(KernelError::invalid(
            "CHEMS-K010",
            "mapping changed during revalidation",
            None,
        ));
    }
    Ok(())
}

fn bound_valence_premises<'a>(
    expanded: &ExpandedStructuralReaction,
    catalogue: &'a ValidatedCatalogueBundle,
) -> Result<Vec<&'a ValencePremiseRecord>, KernelError> {
    let premises = catalogue
        .document()
        .valence_premises
        .iter()
        .filter(|premise| expanded.premises.contains(&premise.premise_id))
        .collect::<Vec<_>>();
    if premises.is_empty() {
        return Err(KernelError::invalid(
            "CHEMS-K092",
            "expansion has no bound valence premise",
            None,
        ));
    }
    Ok(premises)
}

fn make_state(
    ordinal: u32,
    operation: Option<StructuralOperationId>,
    graph: StructuralGraph,
    product_assignments: &BTreeMap<StructureInstanceId, BTreeSet<AtomId>>,
) -> Result<StructuralState, KernelError> {
    let ledger = StructuralLedger::from_graph(&graph);
    let semantic = serde_json::json!({
        "ordinal": ordinal,
        "operation": operation,
        "graph": graph,
        "product_assignments": product_assignments,
        "ledger": ledger,
    });
    let bytes = canonical_json(&semantic)
        .map_err(|error| KernelError::corrupt("CHEMS-K090", error.to_string(), Some(ordinal)))?;
    Ok(StructuralState {
        ordinal,
        operation,
        graph,
        product_assignments: product_assignments.clone(),
        ledger,
        digest: ContentDigest::sha256(&bytes),
    })
}

#[derive(Debug, Clone)]
struct WorkingState {
    atoms: BTreeMap<AtomId, Atom>,
    bonds: BTreeMap<CovalentBondId, CovalentBond>,
    groups: BTreeMap<AtomGroupId, AtomGroup>,
    associations: BTreeMap<IonicAssociationId, IonicAssociation>,
    domains: BTreeMap<MetallicDomainId, MetallicDomain>,
    product_assignments: BTreeMap<StructureInstanceId, BTreeSet<AtomId>>,
}

impl WorkingState {
    fn from_expanded(expanded: &ExpandedStructuralReaction) -> Result<Self, KernelError> {
        let mut state = Self {
            atoms: BTreeMap::new(),
            bonds: BTreeMap::new(),
            groups: BTreeMap::new(),
            associations: BTreeMap::new(),
            domains: BTreeMap::new(),
            product_assignments: BTreeMap::new(),
        };
        for instance in expanded.reactant_instances.values() {
            let graph = instance.instance.graph();
            merge_unique(&mut state.atoms, graph.atoms(), "atom")?;
            merge_unique(&mut state.bonds, graph.covalent_bonds(), "bond")?;
            merge_unique(&mut state.groups, graph.groups(), "group")?;
            merge_unique(
                &mut state.associations,
                graph.ionic_associations(),
                "ionic association",
            )?;
            merge_unique(
                &mut state.domains,
                graph.metallic_domains(),
                "metallic domain",
            )?;
        }
        Ok(state)
    }

    fn graph(&self, operation: Option<u32>) -> Result<StructuralGraph, KernelError> {
        StructuralGraph::new(
            self.atoms.values().cloned(),
            self.bonds.values().cloned(),
            self.groups.values().cloned(),
            self.associations.values().cloned(),
            self.domains.values().cloned(),
        )
        .map_err(|error| KernelError::invalid("CHEMS-K020", error.to_string(), operation))
    }

    #[allow(clippy::too_many_lines)]
    fn apply(&mut self, expanded: &ExpandedOperation) -> Result<(), KernelError> {
        let ordinal = expanded.ordinal;
        match expanded.operation.view() {
            StructuralOperationView::ReconfigureElectrons { transition } => {
                self.apply_transition(transition, ordinal)?;
            }
            StructuralOperationView::CleaveCovalent {
                left,
                right,
                expected_order,
                transitions,
                ..
            } => {
                let bond = self.require_bond(left, right, ordinal)?;
                if bond.order() != expected_order || !bond.electron_origin().is_shared() {
                    return Err(precondition(
                        ordinal,
                        "shared covalent bond identity or order mismatch",
                    ));
                }
                let bond_id = bond.id().clone();
                self.bonds.remove(&bond_id);
                self.apply_transitions(transitions, ordinal)?;
            }
            StructuralOperationView::FormCovalent {
                left,
                right,
                order,
                transitions,
            } => {
                if self.find_bond(left, right).is_some() {
                    return Err(precondition(ordinal, "covalent bond already exists"));
                }
                let contribution = expanded.electron_contribution.ok_or_else(|| {
                    KernelError::invalid(
                        "CHEMS-K021",
                        "bond formation has no endpoint contribution",
                        Some(ordinal),
                    )
                })?;
                if contribution.left != order.order() || contribution.right != order.order() {
                    return Err(precondition(
                        ordinal,
                        "bond formation contribution does not match order",
                    ));
                }
                for (atom, required) in [(left, contribution.left), (right, contribution.right)] {
                    let transition = &transitions[atom];
                    let before = self.require_atom(atom, ordinal)?.electrons();
                    if before.unpaired_electrons() < required {
                        return Err(precondition(
                            ordinal,
                            format!("atom `{atom}` lacks unpaired electrons"),
                        ));
                    }
                    if before.unpaired_electrons().checked_sub(required)
                        != Some(transition.after().unpaired_electrons())
                    {
                        return Err(precondition(
                            ordinal,
                            format!(
                                "atom `{atom}` bond contribution does not consume exact radicals"
                            ),
                        ));
                    }
                }
                self.apply_transitions(transitions, ordinal)?;
                let id = CovalentBondId::from_str(&format!("{}.bond", expanded.operation.id()))
                    .map_err(|error| {
                        KernelError::corrupt("CHEMS-K090", error.to_string(), Some(ordinal))
                    })?;
                let bond = CovalentBond::new(id.clone(), left.clone(), right.clone(), order)
                    .map_err(|error| {
                        KernelError::invalid("CHEMS-K021", error.to_string(), Some(ordinal))
                    })?;
                self.bonds.insert(id, bond);
            }
            StructuralOperationView::CleaveDative {
                donor,
                acceptor,
                transitions,
                ..
            } => {
                let bond = self.require_bond(donor, acceptor, ordinal)?;
                if bond.electron_origin()
                    != (&CovalentElectronOrigin::Dative {
                        donor: donor.clone(),
                        acceptor: acceptor.clone(),
                    })
                {
                    return Err(precondition(
                        ordinal,
                        "directed dative bond identity mismatch",
                    ));
                }
                let bond_id = bond.id().clone();
                self.bonds.remove(&bond_id);
                self.apply_transitions(transitions, ordinal)?;
            }
            StructuralOperationView::FormDative {
                donor,
                acceptor,
                transitions,
            } => {
                if self.find_bond(donor, acceptor).is_some() {
                    return Err(precondition(ordinal, "covalent bond already exists"));
                }
                self.apply_transitions(transitions, ordinal)?;
                let id = CovalentBondId::from_str(&format!("{}.bond", expanded.operation.id()))
                    .map_err(|error| {
                        KernelError::corrupt("CHEMS-K090", error.to_string(), Some(ordinal))
                    })?;
                let bond = CovalentBond::new_dative(id.clone(), donor.clone(), acceptor.clone())
                    .map_err(|error| {
                        KernelError::invalid("CHEMS-K021", error.to_string(), Some(ordinal))
                    })?;
                self.bonds.insert(id, bond);
            }
            StructuralOperationView::ChangeCovalent {
                left,
                right,
                old_order,
                new_order,
                transitions,
                ..
            } => {
                let old = self.require_bond(left, right, ordinal)?.clone();
                if old.order() != old_order || !old.electron_origin().is_shared() {
                    return Err(precondition(
                        ordinal,
                        "shared covalent bond identity or old order mismatch",
                    ));
                }
                self.apply_transitions(transitions, ordinal)?;
                let changed = if let Some(delocalization) = old.delocalization() {
                    CovalentBond::new_delocalized(
                        old.id().clone(),
                        left.clone(),
                        right.clone(),
                        new_order,
                        delocalization.clone(),
                    )
                } else {
                    CovalentBond::new(old.id().clone(), left.clone(), right.clone(), new_order)
                }
                .map_err(|error| {
                    KernelError::invalid("CHEMS-K021", error.to_string(), Some(ordinal))
                })?;
                self.bonds.insert(old.id().clone(), changed);
            }
            StructuralOperationView::ChangeCovalentDelocalization {
                left,
                right,
                expected,
                replacement,
            } => {
                let old = self.require_bond(left, right, ordinal)?.clone();
                if !old.electron_origin().is_shared() || old.delocalization() != expected {
                    return Err(precondition(
                        ordinal,
                        "shared covalent delocalisation precondition mismatch",
                    ));
                }
                let changed = if let Some(replacement) = replacement {
                    CovalentBond::new_delocalized(
                        old.id().clone(),
                        left.clone(),
                        right.clone(),
                        old.order(),
                        replacement.clone(),
                    )
                } else {
                    CovalentBond::new(old.id().clone(), left.clone(), right.clone(), old.order())
                }
                .map_err(|error| {
                    KernelError::invalid("CHEMS-K021", error.to_string(), Some(ordinal))
                })?;
                self.bonds.insert(old.id().clone(), changed);
            }
            StructuralOperationView::AssociateIonic { association } => {
                if self.associations.contains_key(association.id()) {
                    return Err(precondition(ordinal, "ionic association already exists"));
                }
                if expanded.ionic_components.len() != association.components().len() {
                    return Err(precondition(
                        ordinal,
                        "ionic component metadata is incomplete",
                    ));
                }
                let supplied_components = expanded
                    .ionic_components
                    .iter()
                    .map(|component| component.group.id().clone())
                    .collect::<BTreeSet<_>>();
                if &supplied_components != association.components() {
                    return Err(precondition(
                        ordinal,
                        "ionic component metadata names the wrong groups",
                    ));
                }
                for component in &expanded.ionic_components {
                    let actual_charge = component
                        .group
                        .atoms()
                        .iter()
                        .map(|atom| {
                            self.require_atom(atom, ordinal)
                                .map(|atom| atom.electrons().formal_charge())
                        })
                        .collect::<Result<Vec<_>, _>>()?
                        .into_iter()
                        .map(i64::from)
                        .sum::<i64>();
                    if actual_charge != i64::from(component.expected_charge) {
                        return Err(precondition(
                            ordinal,
                            format!("ionic component `{}` charge mismatch", component.group.id()),
                        ));
                    }
                    if self
                        .groups
                        .insert(component.group.id().clone(), component.group.clone())
                        .is_some()
                    {
                        return Err(precondition(ordinal, "ionic group identity already exists"));
                    }
                }
                self.associations
                    .insert(association.id().clone(), association.clone());
            }
            StructuralOperationView::DissociateIonic { association } => {
                let removed = self
                    .associations
                    .remove(association)
                    .ok_or_else(|| precondition(ordinal, "ionic association does not exist"))?;
                for component in removed.components() {
                    let still_used = self
                        .associations
                        .values()
                        .any(|other| other.components().contains(component));
                    if !still_used {
                        self.groups.remove(component);
                    }
                }
            }
            StructuralOperationView::ReleaseMetallic {
                site,
                domain,
                allocation,
                transition,
                domain_electrons_before,
                domain_electrons_after,
                ..
            } => {
                self.require_transition_before(transition, ordinal)?;
                if matches!(allocation, MetallicReleaseAllocation::RetainElectron) {
                    let released = domain_electrons_before
                        .checked_sub(domain_electrons_after)
                        .and_then(|value| u8::try_from(value).ok());
                    if released.is_none_or(|released| {
                        released == 0
                            || transition
                                .before()
                                .unpaired_electrons()
                                .checked_add(released)
                                != Some(transition.after().unpaired_electrons())
                    }) {
                        return Err(precondition(
                            ordinal,
                            "retained metallic electrons are not locally unpaired",
                        ));
                    }
                }
                let current = self.domains.get(domain).ok_or_else(|| {
                    precondition(
                        ordinal,
                        format!("metallic domain `{domain}` does not exist"),
                    )
                })?;
                if !current.sites().contains(site)
                    || current.delocalized_electrons() != domain_electrons_before
                {
                    return Err(precondition(ordinal, "metallic release pre-state mismatch"));
                }
                let mut sites = current.sites().clone();
                sites.remove(site);
                if sites.is_empty() {
                    if domain_electrons_after != 0 {
                        return Err(precondition(
                            ordinal,
                            "empty metallic domain retains electrons",
                        ));
                    }
                    self.domains.remove(domain);
                } else {
                    let replacement =
                        MetallicDomain::new(domain.clone(), sites, domain_electrons_after)
                            .map_err(|error| {
                                KernelError::invalid("CHEMS-K022", error.to_string(), Some(ordinal))
                            })?;
                    self.domains.insert(domain.clone(), replacement);
                }
                self.apply_transition(transition, ordinal)?;
            }
            StructuralOperationView::JoinMetallic {
                site,
                domain,
                transition,
                domain_electrons_before,
                domain_electrons_after,
                ..
            } => {
                self.require_transition_before(transition, ordinal)?;
                let joined = domain_electrons_after
                    .checked_sub(domain_electrons_before)
                    .and_then(|value| u8::try_from(value).ok());
                if joined.is_none_or(|joined| joined == 0) {
                    return Err(precondition(
                        ordinal,
                        "metallic join has no donated electrons",
                    ));
                }
                if transition
                    .before()
                    .unpaired_electrons()
                    .checked_sub(joined.expect("checked above"))
                    != Some(transition.after().unpaired_electrons())
                {
                    return Err(precondition(
                        ordinal,
                        "metallic donation does not consume its unpaired electrons",
                    ));
                }
                let mut sites = BTreeSet::new();
                if let Some(current) = self.domains.get(domain) {
                    if current.sites().contains(site)
                        || current.delocalized_electrons() != domain_electrons_before
                    {
                        return Err(precondition(ordinal, "metallic join pre-state mismatch"));
                    }
                    sites.extend(current.sites().iter().cloned());
                } else if domain_electrons_before != 0 {
                    return Err(precondition(ordinal, "missing nonempty metallic domain"));
                }
                sites.insert(site.clone());
                let replacement =
                    MetallicDomain::new(domain.clone(), sites, domain_electrons_after).map_err(
                        |error| {
                            KernelError::invalid("CHEMS-K022", error.to_string(), Some(ordinal))
                        },
                    )?;
                self.domains.insert(domain.clone(), replacement);
                self.apply_transition(transition, ordinal)?;
            }
            StructuralOperationView::TransferElectron {
                donor,
                count,
                transitions,
                ..
            } => {
                if self
                    .require_atom(donor, ordinal)?
                    .electrons()
                    .non_bonding_electrons()
                    < count
                {
                    return Err(precondition(
                        ordinal,
                        "electron donor lacks local electrons",
                    ));
                }
                self.apply_transitions(transitions, ordinal)?;
            }
            StructuralOperationView::AssignProduct { atoms, product } => {
                if !atoms.iter().all(|atom| self.atoms.contains_key(atom)) {
                    return Err(precondition(ordinal, "product assignment has unknown atom"));
                }
                if self.product_assignments.contains_key(product)
                    || self
                        .product_assignments
                        .values()
                        .any(|assigned| !assigned.is_disjoint(atoms))
                {
                    return Err(precondition(
                        ordinal,
                        "product assignments overlap or repeat a product",
                    ));
                }
                self.product_assignments
                    .insert(product.clone(), atoms.clone());
            }
        }
        Ok(())
    }

    fn require_atom(&self, atom: &AtomId, ordinal: u32) -> Result<&Atom, KernelError> {
        self.atoms.get(atom).ok_or_else(|| {
            precondition(
                ordinal,
                format!("operation references unknown atom `{atom}`"),
            )
        })
    }

    fn require_transition_before(
        &self,
        transition: &chem_domain::ElectronTransition,
        ordinal: u32,
    ) -> Result<(), KernelError> {
        let actual = self.require_atom(transition.atom(), ordinal)?.electrons();
        if actual != transition.before() {
            return Err(precondition(
                ordinal,
                format!("atom `{}` endpoint pre-state mismatch", transition.atom()),
            ));
        }
        Ok(())
    }

    fn apply_transition(
        &mut self,
        transition: &chem_domain::ElectronTransition,
        ordinal: u32,
    ) -> Result<(), KernelError> {
        self.require_transition_before(transition, ordinal)?;
        let existing = self.require_atom(transition.atom(), ordinal)?.clone();
        self.atoms.insert(
            transition.atom().clone(),
            Atom::new(
                transition.atom().clone(),
                existing.element().clone(),
                transition.after(),
            ),
        );
        Ok(())
    }

    fn apply_transitions(
        &mut self,
        transitions: &BTreeMap<AtomId, chem_domain::ElectronTransition>,
        ordinal: u32,
    ) -> Result<(), KernelError> {
        for transition in transitions.values() {
            self.require_transition_before(transition, ordinal)?;
        }
        for transition in transitions.values() {
            self.apply_transition(transition, ordinal)?;
        }
        Ok(())
    }

    fn find_bond(&self, left: &AtomId, right: &AtomId) -> Option<&CovalentBond> {
        self.bonds.values().find(|bond| {
            (bond.left() == left && bond.right() == right)
                || (bond.left() == right && bond.right() == left)
        })
    }

    fn require_bond(
        &self,
        left: &AtomId,
        right: &AtomId,
        ordinal: u32,
    ) -> Result<&CovalentBond, KernelError> {
        self.find_bond(left, right).ok_or_else(|| {
            precondition(
                ordinal,
                format!("covalent bond `{left}`-`{right}` does not exist"),
            )
        })
    }
}

fn merge_unique<K: Ord + Clone + fmt::Display, V: Clone>(
    target: &mut BTreeMap<K, V>,
    source: &BTreeMap<K, V>,
    kind: &str,
) -> Result<(), KernelError> {
    for (id, value) in source {
        if target.insert(id.clone(), value.clone()).is_some() {
            return Err(KernelError::invalid(
                "CHEMS-K011",
                format!("duplicate reactant {kind} `{id}`"),
                None,
            ));
        }
    }
    Ok(())
}

fn precondition(ordinal: u32, message: impl Into<String>) -> KernelError {
    KernelError::invalid("CHEMS-K020", message, Some(ordinal))
}

fn validate_valence(
    graph: &StructuralGraph,
    premises: &[&ValencePremiseRecord],
    operation: Option<u32>,
) -> Result<(), KernelError> {
    for atom in graph.atoms().values() {
        let bond_sum = graph
            .covalent_bond_order_sum(atom.id())
            .ok_or_else(|| KernelError::corrupt("CHEMS-K092", "atom vanished", operation))?;
        // Any declared neutral valence for the element may satisfy the
        // identity: reviewed transition-metal conventions and plain
        // periodic valences legitimately coexist.
        let formal = premises
            .iter()
            .flat_map(|premise| premise.neutral_valence.iter())
            .filter(|entry| entry.element == atom.element().as_str())
            .any(|entry| {
                atom.electrons()
                    .formal_charge_matches(entry.neutral_valence_electrons, bond_sum)
            });
        if !formal {
            return Err(KernelError::invalid(
                "CHEMS-K031",
                format!("atom `{}` violates the formal-charge equation", atom.id()),
                operation,
            ));
        }
        let supported = premises.iter().any(|premise| {
            premise.supported_states.iter().any(|state| {
                state.element == atom.element().as_str()
                    && state.formal_charge == atom.electrons().formal_charge()
                    && state.non_bonding_electrons == atom.electrons().non_bonding_electrons()
                    && state.unpaired_electrons == atom.electrons().unpaired_electrons()
                    && u64::from(state.covalent_bond_order_sum) == bond_sum
            })
        });
        if !supported {
            return Err(KernelError::unsupported(
                "CHEMS-K030",
                format!("atom `{}` has no reviewed valence state", atom.id()),
                operation,
            ));
        }
    }
    for domain in graph.metallic_domains().values() {
        let site_count = u32::try_from(domain.sites().len()).map_err(|_| {
            KernelError::corrupt("CHEMS-K092", "metallic site count overflow", operation)
        })?;
        if domain.delocalized_electrons() % site_count != 0 {
            return Err(KernelError::unsupported(
                "CHEMS-K032",
                format!(
                    "metallic domain `{}` has non-integral electrons per site",
                    domain.id()
                ),
                operation,
            ));
        }
        let per_site = domain.delocalized_electrons() / site_count;
        for site in domain.sites() {
            let atom = &graph.atoms()[site];
            let supported = premises.iter().any(|premise| {
                premise.metallic_domain_states.iter().any(|state| {
                    state.element == atom.element().as_str()
                        && state.site_formal_charge == atom.electrons().formal_charge()
                        && state.site_local_electrons == atom.electrons().non_bonding_electrons()
                        && state.delocalized_electrons_per_site == per_site
                })
            });
            if !supported {
                return Err(KernelError::unsupported(
                    "CHEMS-K032",
                    format!("metallic site `{site}` has no reviewed domain state"),
                    operation,
                ));
            }
        }
    }
    Ok(())
}

fn validate_conservation(
    baseline: StructuralLedger,
    current: StructuralLedger,
    operation: u32,
) -> Result<(), KernelError> {
    if current.atom_count != baseline.atom_count {
        return Err(KernelError::invalid(
            "CHEMS-K040",
            "atom count is not conserved",
            Some(operation),
        ));
    }
    if current.total_explicit_valence_electrons != baseline.total_explicit_valence_electrons {
        return Err(KernelError::invalid(
            "CHEMS-K041",
            "explicit valence electrons are not conserved",
            Some(operation),
        ));
    }
    if current.system_net_charge != baseline.system_net_charge {
        return Err(KernelError::invalid(
            "CHEMS-K042",
            "closed-system charge is not conserved",
            Some(operation),
        ));
    }
    Ok(())
}

fn validate_products(
    expanded: &ExpandedStructuralReaction,
    state: &WorkingState,
) -> Result<(), KernelError> {
    let all_atoms = state.atoms.keys().cloned().collect::<BTreeSet<_>>();
    let assigned = state
        .product_assignments
        .values()
        .flat_map(|atoms| atoms.iter().cloned())
        .collect::<BTreeSet<_>>();
    if assigned != all_atoms || state.product_assignments.len() != expanded.product_instances.len()
    {
        return Err(KernelError::invalid(
            "CHEMS-K050",
            "product assignments do not partition every conserved atom",
            None,
        ));
    }
    for (instance_id, atoms) in &state.product_assignments {
        let expected = expanded
            .product_instances
            .get(instance_id.as_str())
            .ok_or_else(|| {
                KernelError::invalid(
                    "CHEMS-K050",
                    format!("unknown assigned product `{instance_id}`"),
                    None,
                )
            })?;
        let mapped = atoms
            .iter()
            .map(|atom| {
                expanded
                    .mapping
                    .entries()
                    .get(atom)
                    .cloned()
                    .ok_or_else(|| {
                        KernelError::invalid(
                            "CHEMS-K010",
                            format!("assigned atom `{atom}` is unmapped"),
                            None,
                        )
                    })
            })
            .collect::<Result<BTreeSet<_>, _>>()?;
        if mapped != expected.instance.graph().atoms().keys().cloned().collect() {
            return Err(KernelError::invalid(
                "CHEMS-K051",
                format!("assignment for `{instance_id}` disagrees with atom mapping"),
                None,
            ));
        }
    }
    compare_final_atoms(expanded, state)?;
    compare_final_bonds(expanded, state)?;
    compare_final_groups(expanded, state)?;
    compare_final_ionic(expanded, state)?;
    compare_final_metallic(expanded, state)?;
    Ok(())
}

fn compare_final_groups(
    expanded: &ExpandedStructuralReaction,
    state: &WorkingState,
) -> Result<(), KernelError> {
    let mut actual = state
        .groups
        .values()
        .map(|group| {
            group
                .atoms()
                .iter()
                .map(|atom| expanded.mapping.entries()[atom].clone())
                .collect::<BTreeSet<_>>()
        })
        .collect::<Vec<_>>();
    let mut expected = expanded
        .product_instances
        .values()
        .flat_map(|instance| instance.instance.graph().groups().values())
        .map(|group| group.atoms().clone())
        .collect::<Vec<_>>();
    actual.sort();
    expected.sort();
    if actual != expected {
        return Err(KernelError::invalid(
            "CHEMS-K056",
            "final atom groups disagree with declared products",
            None,
        ));
    }
    Ok(())
}

fn compare_final_atoms(
    expanded: &ExpandedStructuralReaction,
    state: &WorkingState,
) -> Result<(), KernelError> {
    let expected = expanded
        .product_instances
        .values()
        .flat_map(|instance| instance.instance.graph().atoms())
        .collect::<BTreeMap<_, _>>();
    for (source, destination) in expanded.mapping.entries() {
        let actual = &state.atoms[source];
        let product = expected[destination];
        if actual.element() != product.element() || actual.electrons() != product.electrons() {
            return Err(KernelError::invalid(
                "CHEMS-K052",
                format!("final atom `{source}` disagrees with mapped product `{destination}`"),
                None,
            ));
        }
    }
    Ok(())
}

fn compare_final_bonds(
    expanded: &ExpandedStructuralReaction,
    state: &WorkingState,
) -> Result<(), KernelError> {
    let actual = state
        .bonds
        .values()
        .map(|bond| final_bond_identity(bond, |atom| expanded.mapping.entries()[atom].clone()))
        .collect::<BTreeSet<_>>();
    let expected = expanded
        .product_instances
        .values()
        .flat_map(|instance| instance.instance.graph().covalent_bonds().values())
        .map(|bond| final_bond_identity(bond, Clone::clone))
        .collect::<BTreeSet<_>>();
    require_matching_final_bonds(&actual, &expected)
}

type FinalBondIdentity = (
    AtomId,
    AtomId,
    chem_domain::BondOrder,
    Option<(AtomId, AtomId)>,
    Option<chem_domain::CovalentDelocalization>,
);

fn final_bond_identity(
    bond: &CovalentBond,
    map_atom: impl Fn(&AtomId) -> AtomId,
) -> FinalBondIdentity {
    let mut ends = [map_atom(bond.left()), map_atom(bond.right())];
    ends.sort();
    let origin = match bond.electron_origin() {
        CovalentElectronOrigin::Shared => None,
        CovalentElectronOrigin::Dative { donor, acceptor } => {
            Some((map_atom(donor), map_atom(acceptor)))
        }
    };
    (
        ends[0].clone(),
        ends[1].clone(),
        bond.order(),
        origin,
        bond.delocalization().cloned(),
    )
}

fn require_matching_final_bonds(
    actual: &BTreeSet<FinalBondIdentity>,
    expected: &BTreeSet<FinalBondIdentity>,
) -> Result<(), KernelError> {
    if actual != expected {
        return Err(KernelError::invalid(
            "CHEMS-K053",
            "final covalent graph disagrees with declared products",
            None,
        ));
    }
    Ok(())
}

type IonicShape = BTreeSet<BTreeSet<AtomId>>;

fn compare_final_ionic(
    expanded: &ExpandedStructuralReaction,
    state: &WorkingState,
) -> Result<(), KernelError> {
    let actual = state
        .associations
        .values()
        .map(|association| {
            association
                .components()
                .iter()
                .map(|component| {
                    state.groups[component]
                        .atoms()
                        .iter()
                        .map(|atom| expanded.mapping.entries()[atom].clone())
                        .collect::<BTreeSet<_>>()
                })
                .collect::<IonicShape>()
        })
        .collect::<BTreeSet<_>>();
    let expected = expanded
        .product_instances
        .values()
        .flat_map(|instance| {
            let graph = instance.instance.graph();
            graph.ionic_associations().values().map(move |association| {
                association
                    .components()
                    .iter()
                    .map(|component| graph.groups()[component].atoms().clone())
                    .collect::<IonicShape>()
            })
        })
        .collect::<BTreeSet<_>>();
    if actual != expected {
        return Err(KernelError::invalid(
            "CHEMS-K054",
            "final ionic associations disagree with declared products",
            None,
        ));
    }
    Ok(())
}

type MetallicShape = (BTreeSet<AtomId>, u32);

fn compare_final_metallic(
    expanded: &ExpandedStructuralReaction,
    state: &WorkingState,
) -> Result<(), KernelError> {
    let actual = state
        .domains
        .values()
        .map(|domain| {
            (
                domain
                    .sites()
                    .iter()
                    .map(|atom| expanded.mapping.entries()[atom].clone())
                    .collect(),
                domain.delocalized_electrons(),
            )
        })
        .collect::<BTreeSet<MetallicShape>>();
    let expected = expanded
        .product_instances
        .values()
        .flat_map(|instance| instance.instance.graph().metallic_domains().values())
        .map(|domain| (domain.sites().clone(), domain.delocalized_electrons()))
        .collect::<BTreeSet<MetallicShape>>();
    if actual != expected {
        return Err(KernelError::invalid(
            "CHEMS-K055",
            "final metallic domains disagree with declared products",
            None,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, BTreeSet},
        fs,
        path::PathBuf,
        str::FromStr,
    };

    use chem_catalogue::{
        ElementValenceRecord, MetallicValenceStateRecord, ValencePremiseRecord, ValenceStateRecord,
        ValidatedCatalogueBundle,
    };
    use chem_domain::{
        Atom, AtomGroup, AtomGroupId, AtomId, BondOrder, CovalentBond, CovalentBondId,
        CovalentDelocalization, CovalentDelocalizationId, EffectiveBondOrder, ElectronAllocation,
        ElectronState, ElectronTransition, ElementSymbol, IonicAssociation, IonicAssociationId,
        MetallicDomain, MetallicDomainId, MetallicJoinAllocation, MetallicReleaseAllocation,
        StructuralGraph, StructuralOperation, StructuralOperationId, StructuralOperationInput,
        StructureInstanceId,
    };
    use serde_json::Value;

    use crate::{
        ExpandedElectronContribution, ExpandedIonicComponent, ExpandedOperation, Provenance,
    };

    use super::{
        KernelFailureClass, StructuralLedger, WorkingState, final_bond_identity,
        require_matching_final_bonds, validate_conservation, validate_valence,
    };

    fn atom(id: &str, charge: i16, local: u8, unpaired: u8) -> Atom {
        element_atom(id, "H", charge, local, unpaired)
    }

    fn element_atom(id: &str, element: &str, charge: i16, local: u8, unpaired: u8) -> Atom {
        Atom::new(
            AtomId::from_str(id).unwrap(),
            ElementSymbol::from_str(element).unwrap(),
            ElectronState::new(charge, local, unpaired).unwrap(),
        )
    }

    fn transition_premise() -> ValencePremiseRecord {
        ValencePremiseRecord {
            premise_id: "premise.test.complete-transition".parse().unwrap(),
            neutral_valence: vec![
                ElementValenceRecord {
                    element: "H".into(),
                    neutral_valence_electrons: 1,
                },
                ElementValenceRecord {
                    element: "C".into(),
                    neutral_valence_electrons: 4,
                },
                ElementValenceRecord {
                    element: "Li".into(),
                    neutral_valence_electrons: 1,
                },
                ElementValenceRecord {
                    element: "Na".into(),
                    neutral_valence_electrons: 1,
                },
                ElementValenceRecord {
                    element: "Cl".into(),
                    neutral_valence_electrons: 7,
                },
            ],
            supported_states: vec![
                ValenceStateRecord {
                    element: "H".into(),
                    formal_charge: 0,
                    non_bonding_electrons: 0,
                    unpaired_electrons: 0,
                    covalent_bond_order_sum: 1,
                },
                ValenceStateRecord {
                    element: "H".into(),
                    formal_charge: 0,
                    non_bonding_electrons: 1,
                    unpaired_electrons: 1,
                    covalent_bond_order_sum: 0,
                },
                ValenceStateRecord {
                    element: "C".into(),
                    formal_charge: 0,
                    non_bonding_electrons: 3,
                    unpaired_electrons: 1,
                    covalent_bond_order_sum: 1,
                },
                ValenceStateRecord {
                    element: "C".into(),
                    formal_charge: 0,
                    non_bonding_electrons: 2,
                    unpaired_electrons: 0,
                    covalent_bond_order_sum: 2,
                },
                ValenceStateRecord {
                    element: "Li".into(),
                    formal_charge: 1,
                    non_bonding_electrons: 0,
                    unpaired_electrons: 0,
                    covalent_bond_order_sum: 0,
                },
                ValenceStateRecord {
                    element: "Li".into(),
                    formal_charge: 0,
                    non_bonding_electrons: 1,
                    unpaired_electrons: 1,
                    covalent_bond_order_sum: 0,
                },
                ValenceStateRecord {
                    element: "Na".into(),
                    formal_charge: 1,
                    non_bonding_electrons: 0,
                    unpaired_electrons: 0,
                    covalent_bond_order_sum: 0,
                },
                ValenceStateRecord {
                    element: "Cl".into(),
                    formal_charge: -1,
                    non_bonding_electrons: 8,
                    unpaired_electrons: 0,
                    covalent_bond_order_sum: 0,
                },
            ],
            metallic_domain_states: vec![
                MetallicValenceStateRecord {
                    element: "Li".into(),
                    site_formal_charge: 1,
                    site_local_electrons: 0,
                    delocalized_electrons_per_site: 1,
                },
                MetallicValenceStateRecord {
                    element: "Li".into(),
                    site_formal_charge: 1,
                    site_local_electrons: 0,
                    delocalized_electrons_per_site: 2,
                },
            ],
        }
    }

    fn validate_complete_transition(state: &mut WorkingState, operation: &ExpandedOperation) {
        let premise = transition_premise();
        let premises = [&premise];
        let before = state.graph(None).unwrap();
        validate_valence(&before, &premises, None).unwrap();
        let baseline = StructuralLedger::from_graph(&before);
        state.apply(operation).unwrap();
        let after = state.graph(Some(operation.ordinal)).unwrap();
        validate_valence(&after, &premises, Some(operation.ordinal)).unwrap();
        validate_conservation(
            baseline,
            StructuralLedger::from_graph(&after),
            operation.ordinal,
        )
        .unwrap();
    }

    fn operation(input: StructuralOperationInput) -> ExpandedOperation {
        ExpandedOperation {
            ordinal: 1,
            operation: StructuralOperation::new(
                StructuralOperationId::from_str("operation[1]").unwrap(),
                input,
            )
            .unwrap(),
            electron_contribution: None,
            ionic_components: Vec::new(),
            provenance: Provenance::derived([], [], []),
        }
    }

    fn state(atoms: Vec<Atom>) -> WorkingState {
        WorkingState {
            atoms: atoms
                .into_iter()
                .map(|atom| (atom.id().clone(), atom))
                .collect(),
            bonds: BTreeMap::new(),
            groups: BTreeMap::new(),
            associations: BTreeMap::new(),
            domains: BTreeMap::new(),
            product_assignments: BTreeMap::new(),
        }
    }

    fn transition(atom: &str, before: (i16, u8, u8), after: (i16, u8, u8)) -> ElectronTransition {
        ElectronTransition::new(
            AtomId::from_str(atom).unwrap(),
            ElectronState::new(before.0, before.1, before.2).unwrap(),
            ElectronState::new(after.0, after.1, after.2).unwrap(),
        )
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn every_noncanonical_operation_path_has_a_successful_transition() {
        let left = AtomId::from_str("left").unwrap();
        let right = AtomId::from_str("right").unwrap();

        let mut homolytic = state(vec![atom("left", 0, 0, 0), atom("right", 0, 0, 0)]);
        let bond_id = CovalentBondId::from_str("bond").unwrap();
        homolytic.bonds.insert(
            bond_id.clone(),
            CovalentBond::new(bond_id, left.clone(), right.clone(), BondOrder::Single).unwrap(),
        );
        let homolytic_operation = operation(StructuralOperationInput::CleaveCovalent {
            left: left.clone(),
            right: right.clone(),
            expected_order: BondOrder::Single,
            allocation: ElectronAllocation::Homolytic,
            transitions: vec![
                transition("left", (0, 0, 0), (0, 1, 1)),
                transition("right", (0, 0, 0), (0, 1, 1)),
            ],
        });
        validate_complete_transition(&mut homolytic, &homolytic_operation);
        assert!(homolytic.bonds.is_empty());

        let mut change = state(vec![
            element_atom("left", "C", 0, 3, 1),
            element_atom("right", "C", 0, 3, 1),
        ]);
        let bond_id = CovalentBondId::from_str("bond").unwrap();
        change.bonds.insert(
            bond_id.clone(),
            CovalentBond::new(bond_id, left.clone(), right.clone(), BondOrder::Single).unwrap(),
        );
        let change_operation = operation(StructuralOperationInput::ChangeCovalent {
            left: left.clone(),
            right: right.clone(),
            old_order: BondOrder::Single,
            new_order: BondOrder::Double,
            allocation: ElectronAllocation::Homolytic,
            transitions: vec![
                transition("left", (0, 3, 1), (0, 2, 0)),
                transition("right", (0, 3, 1), (0, 2, 0)),
            ],
        });
        validate_complete_transition(&mut change, &change_operation);
        assert_eq!(
            change.find_bond(&left, &right).unwrap().order(),
            BondOrder::Double
        );

        let positive =
            AtomGroup::new(AtomGroupId::from_str("positive").unwrap(), [left.clone()]).unwrap();
        let negative =
            AtomGroup::new(AtomGroupId::from_str("negative").unwrap(), [right.clone()]).unwrap();
        let association = IonicAssociation::new(
            IonicAssociationId::from_str("salt").unwrap(),
            [positive.id().clone(), negative.id().clone()],
        )
        .unwrap();
        let mut dissociate = state(vec![
            element_atom("left", "Na", 1, 0, 0),
            element_atom("right", "Cl", -1, 8, 0),
        ]);
        dissociate.groups.insert(positive.id().clone(), positive);
        dissociate.groups.insert(negative.id().clone(), negative);
        dissociate
            .associations
            .insert(association.id().clone(), association.clone());
        let dissociate_operation = operation(StructuralOperationInput::DissociateIonic {
            association: association.id().clone(),
        });
        validate_complete_transition(&mut dissociate, &dissociate_operation);
        assert!(dissociate.associations.is_empty());
        assert!(dissociate.groups.is_empty());

        let domain_id = MetallicDomainId::from_str("metal").unwrap();
        let mut join = state(vec![element_atom("left", "Li", 0, 1, 1)]);
        let join_operation = operation(StructuralOperationInput::JoinMetallic {
            site: left.clone(),
            domain: domain_id.clone(),
            allocation: MetallicJoinAllocation::DonateElectron,
            transition: transition("left", (0, 1, 1), (1, 0, 0)),
            domain_electrons_before: 0,
            domain_electrons_after: 1,
        });
        validate_complete_transition(&mut join, &join_operation);
        assert_eq!(join.domains[&domain_id].delocalized_electrons(), 1);

        let mut leave = state(vec![
            element_atom("left", "Li", 1, 0, 0),
            element_atom("right", "Li", 1, 0, 0),
        ]);
        leave.domains.insert(
            domain_id.clone(),
            MetallicDomain::new(domain_id.clone(), [left.clone(), right.clone()], 2).unwrap(),
        );
        let leave_operation = operation(StructuralOperationInput::ReleaseMetallic {
            site: left,
            domain: domain_id.clone(),
            allocation: MetallicReleaseAllocation::LeaveElectron,
            transition: transition("left", (1, 0, 0), (1, 0, 0)),
            domain_electrons_before: 2,
            domain_electrons_after: 2,
        });
        validate_complete_transition(&mut leave, &leave_operation);
        assert_eq!(
            leave.domains[&domain_id].sites(),
            &[right].into_iter().collect()
        );
    }

    #[test]
    fn bond_order_changes_preserve_covalent_delocalization() {
        let left = AtomId::from_str("left").unwrap();
        let right = AtomId::from_str("right").unwrap();
        let delocalization = CovalentDelocalization::new(
            CovalentDelocalizationId::from_str("resonance").unwrap(),
            EffectiveBondOrder::new(3, 2).unwrap(),
        );
        let mut state = state(vec![
            element_atom("left", "C", 0, 3, 1),
            element_atom("right", "C", 0, 3, 1),
        ]);
        let bond_id = CovalentBondId::from_str("bond").unwrap();
        state.bonds.insert(
            bond_id.clone(),
            CovalentBond::new_delocalized(
                bond_id,
                left.clone(),
                right.clone(),
                BondOrder::Single,
                delocalization.clone(),
            )
            .unwrap(),
        );
        let change = operation(StructuralOperationInput::ChangeCovalent {
            left: left.clone(),
            right: right.clone(),
            old_order: BondOrder::Single,
            new_order: BondOrder::Double,
            allocation: ElectronAllocation::Homolytic,
            transitions: vec![
                transition("left", (0, 3, 1), (0, 2, 0)),
                transition("right", (0, 3, 1), (0, 2, 0)),
            ],
        });

        validate_complete_transition(&mut state, &change);

        assert_eq!(
            state.find_bond(&left, &right).unwrap().delocalization(),
            Some(&delocalization)
        );
    }

    #[test]
    fn final_bond_comparison_rejects_missing_or_wrong_delocalization() {
        let left = AtomId::from_str("product.left").unwrap();
        let right = AtomId::from_str("product.right").unwrap();
        let bond_id = CovalentBondId::from_str("product.bond").unwrap();
        let annotation = |domain: &str, numerator, denominator| {
            CovalentDelocalization::new(
                CovalentDelocalizationId::from_str(domain).unwrap(),
                EffectiveBondOrder::new(numerator, denominator).unwrap(),
            )
        };
        let expected_bond = CovalentBond::new_delocalized(
            bond_id.clone(),
            left.clone(),
            right.clone(),
            BondOrder::Single,
            annotation("resonance", 3, 2),
        )
        .unwrap();
        let expected = BTreeSet::from([final_bond_identity(&expected_bond, Clone::clone)]);

        for actual_bond in [
            CovalentBond::new(
                bond_id.clone(),
                left.clone(),
                right.clone(),
                BondOrder::Single,
            )
            .unwrap(),
            CovalentBond::new_delocalized(
                bond_id.clone(),
                left.clone(),
                right.clone(),
                BondOrder::Single,
                annotation("other-resonance", 3, 2),
            )
            .unwrap(),
            CovalentBond::new_delocalized(
                bond_id.clone(),
                left.clone(),
                right.clone(),
                BondOrder::Single,
                annotation("resonance", 4, 3),
            )
            .unwrap(),
        ] {
            let actual = BTreeSet::from([final_bond_identity(&actual_bond, Clone::clone)]);
            let error = require_matching_final_bonds(&actual, &expected).unwrap_err();
            assert_eq!(error.code(), "CHEMS-K053");
            assert_eq!(error.class(), KernelFailureClass::InvalidExpansion);
        }
    }

    fn expect_precondition(
        fixture: &Value,
        mutation: &str,
        result: Result<(), super::KernelError>,
    ) {
        let error = result.expect_err(mutation);
        let expected = fixture["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["mutation"] == mutation)
            .unwrap();
        assert_eq!(
            error.code(),
            expected["code"].as_str().unwrap(),
            "{mutation}"
        );
        assert_eq!(error.operation(), Some(1), "{mutation}");
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn distinct_runtime_preconditions_have_exact_negative_cases() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("conformance/validation-kernel/kernel-negative-001.input.json");
        let fixture: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        let left = AtomId::from_str("left").unwrap();
        let right = AtomId::from_str("right").unwrap();
        let bond_id = CovalentBondId::from_str("bond").unwrap();

        let cleavage = operation(StructuralOperationInput::CleaveCovalent {
            left: left.clone(),
            right: right.clone(),
            expected_order: BondOrder::Single,
            allocation: ElectronAllocation::Homolytic,
            transitions: vec![
                transition("left", (0, 0, 0), (0, 1, 1)),
                transition("right", (0, 0, 0), (0, 1, 1)),
            ],
        });
        let mut wrong_order = state(vec![atom("left", 0, 0, 0), atom("right", 0, 0, 0)]);
        wrong_order.bonds.insert(
            bond_id.clone(),
            CovalentBond::new(
                bond_id.clone(),
                left.clone(),
                right.clone(),
                BondOrder::Double,
            )
            .unwrap(),
        );
        expect_precondition(&fixture, "cleave_wrong_order", wrong_order.apply(&cleavage));

        let mut stale_endpoint = state(vec![atom("left", 1, 0, 0), atom("right", 0, 0, 0)]);
        stale_endpoint.bonds.insert(
            bond_id.clone(),
            CovalentBond::new(
                bond_id.clone(),
                left.clone(),
                right.clone(),
                BondOrder::Single,
            )
            .unwrap(),
        );
        expect_precondition(
            &fixture,
            "cleave_stale_endpoint",
            stale_endpoint.apply(&cleavage),
        );

        let form_input = || StructuralOperationInput::FormCovalent {
            left: left.clone(),
            right: right.clone(),
            order: BondOrder::Single,
            transitions: vec![
                transition("left", (0, 1, 1), (0, 0, 0)),
                transition("right", (0, 1, 1), (0, 0, 0)),
            ],
        };
        let mut missing_contribution = state(vec![atom("left", 0, 1, 1), atom("right", 0, 1, 1)]);
        expect_precondition(
            &fixture,
            "form_missing_contribution",
            missing_contribution.apply(&operation(form_input())),
        );
        let mut wrong_contribution = operation(form_input());
        wrong_contribution.electron_contribution =
            Some(ExpandedElectronContribution { left: 2, right: 2 });
        let mut radicals = state(vec![atom("left", 0, 1, 1), atom("right", 0, 1, 1)]);
        expect_precondition(
            &fixture,
            "form_wrong_contribution",
            radicals.apply(&wrong_contribution),
        );
        let mut form = operation(form_input());
        form.electron_contribution = Some(ExpandedElectronContribution { left: 1, right: 1 });
        let mut insufficient = state(vec![atom("left", 0, 0, 0), atom("right", 0, 1, 1)]);
        expect_precondition(
            &fixture,
            "form_insufficient_radical",
            insufficient.apply(&form),
        );
        let mut existing = state(vec![atom("left", 0, 1, 1), atom("right", 0, 1, 1)]);
        existing.bonds.insert(
            bond_id.clone(),
            CovalentBond::new(
                bond_id.clone(),
                left.clone(),
                right.clone(),
                BondOrder::Single,
            )
            .unwrap(),
        );
        expect_precondition(&fixture, "form_existing_bond", existing.apply(&form));
        let mut inexact = state(vec![atom("left", 0, 3, 1), atom("right", 0, 1, 1)]);
        let mut inexact_form = operation(StructuralOperationInput::FormCovalent {
            left: left.clone(),
            right: right.clone(),
            order: BondOrder::Single,
            transitions: vec![
                transition("left", (0, 3, 1), (0, 2, 2)),
                transition("right", (0, 1, 1), (0, 0, 0)),
            ],
        });
        inexact_form.electron_contribution =
            Some(ExpandedElectronContribution { left: 1, right: 1 });
        expect_precondition(
            &fixture,
            "form_inexact_radical",
            inexact.apply(&inexact_form),
        );
        let mut stale_form = state(vec![atom("left", 1, 1, 1), atom("right", 0, 1, 1)]);
        expect_precondition(&fixture, "form_stale_endpoint", stale_form.apply(&form));

        let change = operation(StructuralOperationInput::ChangeCovalent {
            left: left.clone(),
            right: right.clone(),
            old_order: BondOrder::Single,
            new_order: BondOrder::Double,
            allocation: ElectronAllocation::Homolytic,
            transitions: vec![
                transition("left", (0, 1, 1), (0, 0, 0)),
                transition("right", (0, 1, 1), (0, 0, 0)),
            ],
        });
        let mut wrong_change = state(vec![atom("left", 0, 1, 1), atom("right", 0, 1, 1)]);
        wrong_change.bonds.insert(
            bond_id.clone(),
            CovalentBond::new(
                bond_id.clone(),
                left.clone(),
                right.clone(),
                BondOrder::Double,
            )
            .unwrap(),
        );
        expect_precondition(&fixture, "change_wrong_order", wrong_change.apply(&change));
        let mut stale_change = state(vec![atom("left", 1, 1, 1), atom("right", 0, 1, 1)]);
        stale_change.bonds.insert(
            bond_id.clone(),
            CovalentBond::new(
                bond_id.clone(),
                left.clone(),
                right.clone(),
                BondOrder::Single,
            )
            .unwrap(),
        );
        expect_precondition(
            &fixture,
            "change_stale_endpoint",
            stale_change.apply(&change),
        );

        let positive =
            AtomGroup::new(AtomGroupId::from_str("positive").unwrap(), [left.clone()]).unwrap();
        let negative =
            AtomGroup::new(AtomGroupId::from_str("negative").unwrap(), [right.clone()]).unwrap();
        let association = IonicAssociation::new(
            IonicAssociationId::from_str("salt").unwrap(),
            [positive.id().clone(), negative.id().clone()],
        )
        .unwrap();
        let association_input = StructuralOperationInput::AssociateIonic {
            association: association.clone(),
        };
        let mut duplicate_association = state(vec![atom("left", 1, 0, 0), atom("right", -1, 0, 0)]);
        duplicate_association
            .associations
            .insert(association.id().clone(), association.clone());
        expect_precondition(
            &fixture,
            "associate_existing",
            duplicate_association.apply(&operation(association_input.clone())),
        );
        let mut incomplete = state(vec![atom("left", 1, 0, 0), atom("right", -1, 0, 0)]);
        expect_precondition(
            &fixture,
            "associate_incomplete_metadata",
            incomplete.apply(&operation(association_input.clone())),
        );
        let mut wrong_groups = operation(association_input.clone());
        wrong_groups.ionic_components = vec![
            ExpandedIonicComponent {
                group: AtomGroup::new(AtomGroupId::from_str("other1").unwrap(), [left.clone()])
                    .unwrap(),
                expected_charge: 1,
            },
            ExpandedIonicComponent {
                group: AtomGroup::new(AtomGroupId::from_str("other2").unwrap(), [right.clone()])
                    .unwrap(),
                expected_charge: -1,
            },
        ];
        let mut ions = state(vec![atom("left", 1, 0, 0), atom("right", -1, 0, 0)]);
        expect_precondition(
            &fixture,
            "associate_wrong_groups",
            ions.apply(&wrong_groups),
        );
        let mut charged = operation(association_input.clone());
        charged.ionic_components = vec![
            ExpandedIonicComponent {
                group: positive.clone(),
                expected_charge: 2,
            },
            ExpandedIonicComponent {
                group: negative.clone(),
                expected_charge: -1,
            },
        ];
        let mut ions = state(vec![atom("left", 1, 0, 0), atom("right", -1, 0, 0)]);
        expect_precondition(&fixture, "associate_charge_mismatch", ions.apply(&charged));
        let mut collision = operation(association_input);
        collision.ionic_components = vec![
            ExpandedIonicComponent {
                group: positive.clone(),
                expected_charge: 1,
            },
            ExpandedIonicComponent {
                group: negative,
                expected_charge: -1,
            },
        ];
        let mut ions = state(vec![atom("left", 1, 0, 0), atom("right", -1, 0, 0)]);
        ions.groups.insert(positive.id().clone(), positive);
        expect_precondition(
            &fixture,
            "associate_group_collision",
            ions.apply(&collision),
        );

        let domain = MetallicDomainId::from_str("metal").unwrap();
        let retain = operation(StructuralOperationInput::ReleaseMetallic {
            site: left.clone(),
            domain: domain.clone(),
            allocation: MetallicReleaseAllocation::RetainElectron,
            transition: transition("left", (1, 0, 0), (0, 1, 1)),
            domain_electrons_before: 1,
            domain_electrons_after: 0,
        });
        let mut no_domain = state(vec![atom("left", 1, 0, 0)]);
        expect_precondition(&fixture, "release_missing_domain", no_domain.apply(&retain));
        let mut stale_release = state(vec![atom("left", 0, 0, 0)]);
        stale_release.domains.insert(
            domain.clone(),
            MetallicDomain::new(domain.clone(), [left.clone()], 1).unwrap(),
        );
        expect_precondition(
            &fixture,
            "release_stale_endpoint",
            stale_release.apply(&retain),
        );
        let mut wrong_domain = state(vec![atom("left", 1, 0, 0)]);
        wrong_domain.domains.insert(
            domain.clone(),
            MetallicDomain::new(domain.clone(), [left.clone()], 2).unwrap(),
        );
        expect_precondition(
            &fixture,
            "release_domain_mismatch",
            wrong_domain.apply(&retain),
        );
        let bad_radical = operation(StructuralOperationInput::ReleaseMetallic {
            site: left.clone(),
            domain: domain.clone(),
            allocation: MetallicReleaseAllocation::RetainElectron,
            transition: transition("left", (1, 2, 0), (0, 3, 3)),
            domain_electrons_before: 1,
            domain_electrons_after: 0,
        });
        let mut metal = state(vec![atom("left", 1, 2, 0)]);
        metal.domains.insert(
            domain.clone(),
            MetallicDomain::new(domain.clone(), [left.clone()], 1).unwrap(),
        );
        expect_precondition(
            &fixture,
            "release_inexact_radical",
            metal.apply(&bad_radical),
        );
        let leave = operation(StructuralOperationInput::ReleaseMetallic {
            site: left.clone(),
            domain: domain.clone(),
            allocation: MetallicReleaseAllocation::LeaveElectron,
            transition: transition("left", (1, 0, 0), (1, 0, 0)),
            domain_electrons_before: 1,
            domain_electrons_after: 1,
        });
        let mut empty_retains = state(vec![atom("left", 1, 0, 0)]);
        empty_retains.domains.insert(
            domain.clone(),
            MetallicDomain::new(domain.clone(), [left.clone()], 1).unwrap(),
        );
        expect_precondition(
            &fixture,
            "release_empty_retains",
            empty_retains.apply(&leave),
        );

        let zero_radical = operation(StructuralOperationInput::JoinMetallic {
            site: left.clone(),
            domain: domain.clone(),
            allocation: MetallicJoinAllocation::DonateElectron,
            transition: transition("left", (0, 2, 0), (1, 1, 1)),
            domain_electrons_before: 0,
            domain_electrons_after: 1,
        });
        let mut local_pair = state(vec![atom("left", 0, 2, 0)]);
        expect_precondition(
            &fixture,
            "join_zero_radical",
            local_pair.apply(&zero_radical),
        );
        let inexact_join = operation(StructuralOperationInput::JoinMetallic {
            site: left.clone(),
            domain: domain.clone(),
            allocation: MetallicJoinAllocation::DonateElectron,
            transition: transition("left", (0, 3, 3), (1, 2, 0)),
            domain_electrons_before: 0,
            domain_electrons_after: 1,
        });
        let mut radicals = state(vec![atom("left", 0, 3, 3)]);
        expect_precondition(
            &fixture,
            "join_inexact_donation",
            radicals.apply(&inexact_join),
        );
        let missing_nonempty = operation(StructuralOperationInput::JoinMetallic {
            site: left.clone(),
            domain: domain.clone(),
            allocation: MetallicJoinAllocation::DonateElectron,
            transition: transition("left", (0, 1, 1), (1, 0, 0)),
            domain_electrons_before: 1,
            domain_electrons_after: 2,
        });
        let mut absent = state(vec![atom("left", 0, 1, 1)]);
        expect_precondition(
            &fixture,
            "join_missing_nonempty_domain",
            absent.apply(&missing_nonempty),
        );
        let join = operation(StructuralOperationInput::JoinMetallic {
            site: left.clone(),
            domain: domain.clone(),
            allocation: MetallicJoinAllocation::DonateElectron,
            transition: transition("left", (0, 1, 1), (1, 0, 0)),
            domain_electrons_before: 1,
            domain_electrons_after: 2,
        });
        let mut stale_join = state(vec![atom("left", 1, 1, 1)]);
        expect_precondition(&fixture, "join_stale_endpoint", stale_join.apply(&join));
        let mut existing_site = state(vec![atom("left", 0, 1, 1)]);
        existing_site.domains.insert(
            domain.clone(),
            MetallicDomain::new(domain.clone(), [left.clone()], 1).unwrap(),
        );
        expect_precondition(&fixture, "join_domain_mismatch", existing_site.apply(&join));

        let transfer = operation(StructuralOperationInput::TransferElectron {
            donor: left.clone(),
            acceptor: right.clone(),
            count: 1,
            transitions: vec![
                transition("left", (0, 1, 1), (1, 0, 0)),
                transition("right", (0, 0, 0), (-1, 1, 1)),
            ],
        });
        let mut empty_donor = state(vec![atom("left", 0, 0, 0), atom("right", 0, 0, 0)]);
        expect_precondition(
            &fixture,
            "transfer_empty_donor",
            empty_donor.apply(&transfer),
        );
        let mut stale_transfer = state(vec![atom("left", 1, 1, 1), atom("right", 0, 0, 0)]);
        expect_precondition(
            &fixture,
            "transfer_stale_endpoint",
            stale_transfer.apply(&transfer),
        );

        let product = StructureInstanceId::from_str("product[1]").unwrap();
        let assignment = operation(StructuralOperationInput::AssignProduct {
            atoms: vec![left.clone()],
            product: product.clone(),
        });
        let mut unknown = state(vec![atom("right", 0, 0, 0)]);
        expect_precondition(&fixture, "assign_unknown_atom", unknown.apply(&assignment));
        let mut repeated = state(vec![atom("left", 0, 0, 0)]);
        repeated.apply(&assignment).unwrap();
        expect_precondition(
            &fixture,
            "assign_repeat_product",
            repeated.apply(&assignment),
        );
        let overlapping = operation(StructuralOperationInput::AssignProduct {
            atoms: vec![left],
            product: StructureInstanceId::from_str("product[2]").unwrap(),
        });
        expect_precondition(&fixture, "assign_overlap", repeated.apply(&overlapping));
    }

    #[test]
    fn every_conservation_class_has_an_exact_negative_fixture() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("conformance/validation-kernel/kernel-negative-001.input.json");
        let fixture: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        let baseline = StructuralLedger {
            atom_count: 8,
            atom_local_non_bonding: 8,
            covalent_bond_electrons: 8,
            metallic_domain_electrons: 2,
            total_explicit_valence_electrons: 18,
            atom_formal_charge_sum: 2,
            system_net_charge: 0,
        };
        for (mutation, current) in [
            (
                "atom_conservation",
                StructuralLedger {
                    atom_count: 7,
                    ..baseline
                },
            ),
            (
                "electron_conservation",
                StructuralLedger {
                    total_explicit_valence_electrons: 17,
                    ..baseline
                },
            ),
            (
                "charge_conservation",
                StructuralLedger {
                    system_net_charge: 1,
                    ..baseline
                },
            ),
        ] {
            let expected = fixture["cases"]
                .as_array()
                .unwrap()
                .iter()
                .find(|case| case["mutation"] == mutation)
                .unwrap();
            let error = validate_conservation(baseline, current, 1).unwrap_err();
            assert_eq!(error.code(), expected["code"].as_str().unwrap());
            assert_eq!(error.operation(), Some(1));
        }
    }

    #[test]
    fn arithmetic_valence_failure_precedes_reviewed_tuple_support() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let catalogue = ValidatedCatalogueBundle::from_json(
            &fs::read(root.join("conformance/catalogue/lithium-rule-001.catalogue.json")).unwrap(),
        )
        .unwrap();
        let premises = catalogue
            .document()
            .valence_premises
            .iter()
            .collect::<Vec<_>>();
        let graph =
            StructuralGraph::new([atom("invalid_hydrogen", 0, 0, 0)], [], [], [], []).unwrap();
        let error = validate_valence(&graph, &premises, Some(1)).unwrap_err();
        assert_eq!(error.code(), "CHEMS-K031");
        assert_eq!(error.class(), KernelFailureClass::InvalidExpansion);
    }
}
