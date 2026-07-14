use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Deref,
};

use chem_catalogue::{EventModel, ObservationPredicate, RequestRelation, SequenceModel};
use chem_domain::{
    AtomGroup, AtomId, AtomMapping, ClaimId, ContentDigest, EvidenceSourceId, PremiseId,
    ReactionRuleId, RepresentationKind, StructuralOperation, StructureId, StructureInstance,
};
use chems_lang::ByteSpan;
use serde::Serialize;
use serde_json::Value;

use crate::{EvidencePacketReference, ExpansionError};

/// Whether expansion used the production trust boundary or an explicit review candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogueTrust {
    ReviewCandidate,
    Trusted,
}

/// Evidence packets are runtime research inputs, never host-authenticated
/// chemistry authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceTrust {
    ExternalUntrusted,
}

/// Reaction side retained in typed HIR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReactionSideKind {
    Reactant,
    Product,
}

/// Exact authored-source location for one resolved or derived value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct SourceOrigin {
    pub source: String,
    pub construct: String,
    pub span: ByteSpan,
}

/// Exact catalogue record and premise dependency for one derived value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct CatalogueOrigin {
    pub catalogue_digest: ContentDigest,
    pub record: String,
    pub premises: BTreeSet<PremiseId>,
}

/// Evidence provenance attached to a typed observation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct EvidenceOrigin {
    pub packet: String,
    pub packet_digest: ContentDigest,
    pub claim: ClaimId,
    pub sources: BTreeSet<EvidenceSourceId>,
}

/// Combined source, catalogue, and evidence derivation provenance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Provenance {
    pub source: BTreeSet<SourceOrigin>,
    pub catalogue: BTreeSet<CatalogueOrigin>,
    pub evidence: BTreeSet<EvidenceOrigin>,
}

impl Provenance {
    #[must_use]
    pub fn source(origin: SourceOrigin) -> Self {
        Self {
            source: [origin].into_iter().collect(),
            catalogue: BTreeSet::new(),
            evidence: BTreeSet::new(),
        }
    }

    #[must_use]
    pub fn derived(
        source: impl IntoIterator<Item = SourceOrigin>,
        catalogue: impl IntoIterator<Item = CatalogueOrigin>,
        evidence: impl IntoIterator<Item = EvidenceOrigin>,
    ) -> Self {
        Self {
            source: source.into_iter().collect(),
            catalogue: catalogue.into_iter().collect(),
            evidence: evidence.into_iter().collect(),
        }
    }
}

/// Canonical source identity and both byte-level and semantic hashes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceReference {
    pub name: String,
    pub bytes_digest: ContentDigest,
    pub semantic_digest: ContentDigest,
}

/// Resolved catalogue identity bound into the expansion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CatalogueReference {
    pub name: String,
    pub version: String,
    pub digest: ContentDigest,
    pub trust: CatalogueTrust,
}

/// One resolved authored structure declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedStructureBinding {
    pub side: ReactionSideKind,
    pub name: String,
    pub coefficient: u32,
    pub structure: StructureId,
    pub formula: BTreeMap<String, u64>,
    pub representation: RepresentationKind,
    pub provenance: Provenance,
}

/// Resolved representative/explanatory model disclosure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedModel {
    pub event: EventModel,
    pub sequence: SequenceModel,
    pub provenance: Provenance,
}

/// One validated equation presentation term.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedEquationTerm {
    pub side: ReactionSideKind,
    pub coefficient: u32,
    pub formula: BTreeMap<String, u64>,
    pub representation: RepresentationKind,
    pub binding: String,
    pub provenance: Provenance,
}

/// One rule role resolved to an authored binding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedRuleBinding {
    pub role: String,
    pub binding: String,
    pub side: ReactionSideKind,
    pub provenance: Provenance,
}

/// Rule-owned applicability fact used to admit the authored request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedApplicability {
    pub request_relation: RequestRelation,
    pub required_context: String,
    pub reactant_structures: BTreeSet<StructureId>,
    pub premise: PremiseId,
    pub provenance: Provenance,
}

/// Resolved rule and its total role binding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedRuleApplication {
    pub rule: ReactionRuleId,
    pub bindings: BTreeMap<String, ResolvedRuleBinding>,
    pub applicability: ResolvedApplicability,
    pub provenance: Provenance,
}

/// One typed observation linked to source, packet claim, and catalogue premise.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedObservation {
    pub predicate: ObservationPredicate,
    pub subject_binding: String,
    pub value: Option<String>,
    pub claim: ClaimId,
    pub evidence_subject: String,
    pub provenance: Provenance,
}

/// Evidence packet identity and typed observations retained by HIR.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedEvidence {
    pub packet: EvidencePacketReference,
    pub digest: ContentDigest,
    pub trust: EvidenceTrust,
    pub observations: Vec<ResolvedObservation>,
}

/// One coefficient-expanded catalogue structure instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExpandedInstance {
    pub binding: String,
    pub ordinal: u32,
    pub instance: StructureInstance,
    pub provenance: Provenance,
}

/// One instantiated reviewed structural operation template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExpandedOperation {
    pub ordinal: u32,
    pub operation: StructuralOperation,
    pub electron_contribution: Option<ExpandedElectronContribution>,
    pub ionic_components: Vec<ExpandedIonicComponent>,
    pub provenance: Provenance,
}

/// Explicit endpoint contribution retained from a bond-formation template.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ExpandedElectronContribution {
    pub left: u8,
    pub right: u8,
}

/// Exact group membership and charge expected by an ionic association step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExpandedIonicComponent {
    pub group: AtomGroup,
    pub expected_charge: i16,
}

/// Fully resolved authored claim before structural execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedReactionClaim {
    pub source: SourceReference,
    pub catalogue: CatalogueReference,
    pub reaction: String,
    pub reactants: BTreeMap<String, ResolvedStructureBinding>,
    pub products: BTreeMap<String, ResolvedStructureBinding>,
    pub equation: Vec<ResolvedEquationTerm>,
    pub model: ResolvedModel,
    pub evidence: ResolvedEvidence,
    pub rule: ResolvedRuleApplication,
}

/// Typed deterministic structural HIR. It contains no executed graph states.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExpandedStructuralReaction {
    pub schema_version: u32,
    pub claim: ResolvedReactionClaim,
    pub reactant_instances: BTreeMap<String, ExpandedInstance>,
    pub product_instances: BTreeMap<String, ExpandedInstance>,
    pub atom_provenance: BTreeMap<AtomId, Provenance>,
    pub mapping: AtomMapping,
    pub mapping_entry_provenance: BTreeMap<AtomId, CatalogueOrigin>,
    pub mapping_provenance: Provenance,
    pub operations: Vec<ExpandedOperation>,
    pub premises: BTreeSet<PremiseId>,
    pub premise_provenance: BTreeMap<PremiseId, CatalogueOrigin>,
}

/// Unforgeable production expansion. Only [`crate::expand_trusted`] can
/// construct this wrapper; Slice 5 accepts this type rather than inspecting a
/// mutable trust marker on ordinary review-candidate HIR.
#[derive(Debug, Clone)]
pub struct TrustedExpandedStructuralReaction {
    pub(crate) expanded: ExpandedStructuralReaction,
}

impl Deref for TrustedExpandedStructuralReaction {
    type Target = ExpandedStructuralReaction;

    fn deref(&self) -> &Self::Target {
        &self.expanded
    }
}

impl ExpandedStructuralReaction {
    /// Serializes the complete HIR, including exact source locations.
    ///
    /// # Errors
    ///
    /// Returns an expansion error if serialization cannot be canonicalized.
    pub fn canonical_json(&self) -> Result<Vec<u8>, ExpansionError> {
        let value = serde_json::to_value(self)
            .map_err(|error| ExpansionError::system("CHEMS-X090", error.to_string()))?;
        chem_domain::canonical_json(&value)
            .map_err(|error| ExpansionError::system("CHEMS-X090", error.to_string()))
    }

    /// Computes the canonical semantic certificate digest. Raw source bytes,
    /// path names, and byte offsets are deliberately excluded; their exact
    /// values remain inspectable in the HIR.
    ///
    /// # Errors
    ///
    /// Returns an expansion error if canonical serialization fails.
    pub fn semantic_digest(&self) -> Result<ContentDigest, ExpansionError> {
        let canonical = self.semantic_json()?;
        Ok(ContentDigest::sha256(&canonical))
    }

    /// Serializes the complete semantic expansion certificate. Physical file
    /// identity, raw-byte hashes, and source spans are deliberately omitted so
    /// equivalent declaration order and source locations produce identical
    /// bytes. Exact physical provenance remains available separately.
    ///
    /// # Errors
    ///
    /// Returns an expansion error if serialization cannot be canonicalized.
    pub fn semantic_json(&self) -> Result<Vec<u8>, ExpansionError> {
        let value = self.semantic_value()?;
        chem_domain::canonical_json(&value)
            .map_err(|error| ExpansionError::system("CHEMS-X090", error.to_string()))
    }

    /// Renders a deterministic, inspectable text certificate. This reports
    /// expansion only and never claims that structural operations executed.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn render_certificate(&self) -> String {
        let mut output = String::new();
        push_line(
            &mut output,
            0,
            "ExpandedStructuralReaction semantic certificate",
        );
        push_line(&mut output, 0, "status: unexecuted");
        push_line(
            &mut output,
            0,
            &format!(
                "source_semantic_digest: {}",
                self.claim.source.semantic_digest
            ),
        );
        push_line(
            &mut output,
            0,
            &format!(
                "catalogue: {}@{} {} trust={:?}",
                self.claim.catalogue.name,
                self.claim.catalogue.version,
                self.claim.catalogue.digest,
                self.claim.catalogue.trust
            ),
        );
        push_line(
            &mut output,
            0,
            &format!("reaction: {}", self.claim.reaction),
        );
        push_line(&mut output, 0, &format!("rule: {}", self.claim.rule.rule));
        push_line(
            &mut output,
            1,
            &format!(
                "applicability: relation={:?} context={:?} reactants={:?} premise={}",
                self.claim.rule.applicability.request_relation,
                self.claim.rule.applicability.required_context,
                self.claim.rule.applicability.reactant_structures,
                self.claim.rule.applicability.premise
            ),
        );
        push_line(
            &mut output,
            1,
            &format!(
                "model: event={:?} sequence={:?} premises={:?}",
                self.claim.model.event,
                self.claim.model.sequence,
                catalogue_premises(&self.claim.model.provenance)
            ),
        );
        push_line(&mut output, 0, "equation:");
        for term in &self.claim.equation {
            push_line(
                &mut output,
                1,
                &format!(
                    "{:?} {} {:?} {:?} binding={}",
                    term.side, term.coefficient, term.formula, term.representation, term.binding
                ),
            );
        }
        push_line(&mut output, 0, "instances:");
        for instance in self
            .reactant_instances
            .values()
            .chain(self.product_instances.values())
        {
            let graph = serde_json::to_string(instance.instance.graph())
                .unwrap_or_else(|error| format!("{{\"serialization_error\":{error:?}}}"));
            push_line(
                &mut output,
                1,
                &format!(
                    "{} binding={} ordinal={} structure={} graph={graph}",
                    instance.instance.id(),
                    instance.binding,
                    instance.ordinal,
                    instance.instance.structure()
                ),
            );
        }
        push_line(&mut output, 0, "mapping:");
        for (source, product) in self.mapping.entries() {
            let premises = self
                .mapping_entry_provenance
                .get(source)
                .map_or_else(BTreeSet::new, |origin| origin.premises.clone());
            push_line(
                &mut output,
                1,
                &format!("{source} -> {product} premises={premises:?}"),
            );
        }
        push_line(&mut output, 0, "operations (ordered, not executed):");
        for operation in &self.operations {
            let semantics = serde_json::to_string(&operation.operation)
                .unwrap_or_else(|error| format!("{{\"serialization_error\":{error:?}}}"));
            let ionic = serde_json::to_string(&operation.ionic_components)
                .unwrap_or_else(|error| format!("{{\"serialization_error\":{error:?}}}"));
            push_line(
                &mut output,
                1,
                &format!(
                    "{} semantics={} electron_contribution={:?} ionic_components={} premises={:?}",
                    operation.ordinal,
                    semantics,
                    operation.electron_contribution,
                    ionic,
                    catalogue_premises(&operation.provenance)
                ),
            );
        }
        push_line(
            &mut output,
            0,
            &format!(
                "evidence: {} digest={} trust={:?}",
                self.claim.evidence.packet.qualified(),
                self.claim.evidence.digest,
                self.claim.evidence.trust
            ),
        );
        for observation in &self.claim.evidence.observations {
            let evidence = observation.provenance.evidence.iter().next().map_or_else(
                || "none".to_owned(),
                |origin| format!("{:?}", origin.sources),
            );
            push_line(
                &mut output,
                1,
                &format!(
                    "{:?} binding={} value={:?} claim={} subject={:?} sources={} premises={:?}",
                    observation.predicate,
                    observation.subject_binding,
                    observation.value,
                    observation.claim,
                    observation.evidence_subject,
                    evidence,
                    catalogue_premises(&observation.provenance)
                ),
            );
        }
        push_line(&mut output, 0, "all rule premises:");
        for premise in &self.premises {
            push_line(&mut output, 1, premise.as_str());
        }
        output
    }

    /// Renders exact source filenames, raw-byte identity, and byte spans for
    /// audit/debug use. Unlike the semantic certificate, this report is
    /// expected to vary when a source is relocated or reordered.
    #[must_use]
    pub fn render_provenance_report(&self) -> String {
        match serde_json::to_string_pretty(self) {
            Ok(body) => format!("ExpandedStructuralReaction physical provenance\n{body}\n"),
            Err(error) => format!(
                "ExpandedStructuralReaction physical provenance\nserialization_error: {error}\n"
            ),
        }
    }

    fn semantic_value(&self) -> Result<Value, ExpansionError> {
        let mut value = serde_json::to_value(self)
            .map_err(|error| ExpansionError::system("CHEMS-X090", error.to_string()))?;
        strip_physical_source_identity(&mut value);
        Ok(value)
    }
}

fn catalogue_premises(provenance: &Provenance) -> BTreeSet<PremiseId> {
    provenance
        .catalogue
        .iter()
        .flat_map(|origin| origin.premises.iter().cloned())
        .collect()
}

fn strip_physical_source_identity(value: &mut Value) {
    match value {
        Value::Object(object) => {
            if object.contains_key("bytes_digest") && object.contains_key("semantic_digest") {
                object.remove("bytes_digest");
                object.remove("name");
            }
            if object.contains_key("source")
                && object.contains_key("catalogue")
                && object.contains_key("evidence")
            {
                object.remove("source");
            }
            for child in object.values_mut() {
                strip_physical_source_identity(child);
            }
        }
        Value::Array(array) => {
            for child in array {
                strip_physical_source_identity(child);
            }
        }
        _ => {}
    }
}

fn push_line(output: &mut String, indent: usize, value: &str) {
    output.push_str(&"  ".repeat(indent));
    output.push_str(value);
    output.push('\n');
}
