use chem_catalogue::{
    AtomRecord, BinaryElectronStateRecord, BondOrderRecord, BondRecord, ComponentRecord,
    ElectronContributionRecord, ElectronStateRecord, ElementValenceRecord, GroupRecord,
    IonicAssociationRecord, MetallicDomainRecord, MetallicElectronStateRecord,
    MetallicJoinAllocationRecord, MetallicReleaseAllocationRecord, MetallicValenceStateRecord,
    TransferElectronStateRecord, ValenceStateRecord,
};
use serde::{Deserialize, Serialize};

use crate::AgentError;

pub const REACTION_CLAIM_SCHEMA_VERSION: u32 = 1;
pub const MECHANISM_ESCALATION_SCHEMA_VERSION: u32 = 1;
pub const STRUCTURE_PROPOSAL_SCHEMA_VERSION: u32 = 1;
pub const MAX_REACTION_CLAIM_BYTES: usize = 64 * 1024;
pub const MAX_MECHANISM_RESPONSE_BYTES: usize = 256 * 1024;
pub const MAX_STRUCTURE_RESPONSE_BYTES: usize = 128 * 1024;
pub const MAX_CLAIM_SOURCES: usize = 4;

/// Fixed factual-claim policy retained in cache bindings.
///
/// The application exposes one low-latency path; this enum remains serialized
/// so existing Fast cache entries retain a stable contract marker.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimMode {
    #[default]
    Fast,
}

/// Closed factual outcome returned by the provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReactionClaim {
    pub schema_version: u32,
    pub disposition: ClaimDisposition,
    pub products: Vec<ClaimProduct>,
    pub required_context: String,
    pub observations: Vec<ClaimObservation>,
    pub sources: Vec<ClaimSource>,
    pub ambiguity: Option<ClaimAmbiguity>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimDisposition {
    Reaction,
    NoReaction,
    Ambiguous,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimProduct {
    pub name: String,
    pub formula: String,
    pub phase: ClaimPhase,
    pub identity_hints: Vec<ClaimIdentityHint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimPhase {
    Aqueous,
    Solid,
    Liquid,
    Gas,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimIdentityHint {
    pub kind: ClaimIdentityHintKind,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimIdentityHintKind {
    Inchi,
    InchiKey,
    CanonicalSmiles,
    IsomericSmiles,
    PubChemCid,
    RegistryId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimObservation {
    pub predicate: ClaimObservationPredicate,
    pub subject: String,
    pub value: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimObservationPredicate {
    Evolves,
    Disappears,
    Forms,
    Colour,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimSource {
    pub id: String,
    pub title: String,
    pub publisher: String,
    pub url: String,
    pub supporting_excerpt: String,
    pub supports: Vec<ClaimField>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimField {
    Products,
    RequiredContext,
    Observations,
    NoReaction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimAmbiguity {
    pub kind: ClaimAmbiguityKind,
    pub summary: String,
    pub alternatives: Vec<ClaimAlternative>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimAmbiguityKind {
    Conditions,
    ReactantIdentity,
    MultipleOutcomes,
    ConflictingEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimAlternative {
    pub label: String,
    pub products: Vec<ClaimProduct>,
    pub required_context: String,
}

impl ReactionClaim {
    /// Decodes and validates one bounded provider claim.
    ///
    /// # Errors
    ///
    /// Returns a typed provider-output error for oversized JSON, schema drift,
    /// contradictory disposition fields, malformed evidence, or procedural
    /// content outside `ChemSpec`'s virtual-only boundary.
    pub fn from_json(bytes: &[u8], _mode: ClaimMode) -> Result<Self, AgentError> {
        if bytes.len() > MAX_REACTION_CLAIM_BYTES {
            return Err(AgentError::new(
                "reaction claim",
                format!("claim exceeds the {MAX_REACTION_CLAIM_BYTES}-byte contract limit"),
            ));
        }
        let claim: Self = serde_json::from_slice(bytes)
            .map_err(|error| AgentError::new("reaction claim", error.to_string()))?;
        claim.validate()?;
        Ok(claim)
    }

    fn validate(&self) -> Result<(), AgentError> {
        if self.schema_version != REACTION_CLAIM_SCHEMA_VERSION {
            return Err(AgentError::new(
                "reaction claim",
                format!("unsupported claim schema {}", self.schema_version),
            ));
        }
        match self.disposition {
            ClaimDisposition::Reaction => {
                if self.products.is_empty() || self.ambiguity.is_some() {
                    return Err(AgentError::new(
                        "reaction claim",
                        "a reaction requires products and cannot carry ambiguity",
                    ));
                }
            }
            ClaimDisposition::NoReaction | ClaimDisposition::Unsupported => {
                if !self.products.is_empty()
                    || !self.observations.is_empty()
                    || self.ambiguity.is_some()
                {
                    return Err(AgentError::new(
                        "reaction claim",
                        "no-reaction and unsupported claims cannot carry products, observations, or ambiguity",
                    ));
                }
            }
            ClaimDisposition::Ambiguous => {
                let ambiguity = self.ambiguity.as_ref().ok_or_else(|| {
                    AgentError::new(
                        "reaction claim",
                        "an ambiguous claim requires ambiguity details",
                    )
                })?;
                if !self.products.is_empty()
                    || !self.observations.is_empty()
                    || ambiguity.alternatives.len() < 2
                {
                    return Err(AgentError::new(
                        "reaction claim",
                        "an ambiguous claim requires at least two alternatives and no selected outcome",
                    ));
                }
            }
        }
        self.validate_fields()
    }

    fn validate_fields(&self) -> Result<(), AgentError> {
        if self.sources.len() > MAX_CLAIM_SOURCES {
            return Err(AgentError::new(
                "reaction claim",
                format!("a claim may cite at most {MAX_CLAIM_SOURCES} direct sources"),
            ));
        }
        require_text(&self.required_context, "required context")?;
        let mut text = vec![self.required_context.as_str()];
        for product in &self.products {
            require_text(&product.name, "product name")?;
            require_text(&product.formula, "product formula")?;
            text.push(&product.name);
            text.push(&product.formula);
            for hint in &product.identity_hints {
                require_text(&hint.value, "identity hint")?;
                text.push(&hint.value);
            }
        }
        for observation in &self.observations {
            require_text(&observation.subject, "observation subject")?;
            let needs_value = observation.predicate == ClaimObservationPredicate::Colour;
            if needs_value != observation.value.is_some() {
                return Err(AgentError::new(
                    "reaction claim",
                    "only colour observations require a value",
                ));
            }
            text.push(&observation.subject);
            if let Some(value) = &observation.value {
                text.push(value);
            }
        }
        let mut source_ids = std::collections::BTreeSet::new();
        for source in &self.sources {
            if !source_ids.insert(source.id.as_str())
                || source.supports.is_empty()
                || !source.url.starts_with("https://")
            {
                return Err(AgentError::new(
                    "reaction claim",
                    "sources require unique IDs, HTTPS URLs, and claim-level coverage",
                ));
            }
            for value in [
                &source.id,
                &source.title,
                &source.publisher,
                &source.url,
                &source.supporting_excerpt,
            ] {
                require_text(value, "source field")?;
                text.push(value);
            }
        }
        if let Some(ambiguity) = &self.ambiguity {
            require_text(&ambiguity.summary, "ambiguity summary")?;
            text.push(&ambiguity.summary);
            for alternative in &ambiguity.alternatives {
                require_text(&alternative.label, "ambiguity label")?;
                require_text(&alternative.required_context, "alternative context")?;
                text.push(&alternative.label);
                text.push(&alternative.required_context);
                for product in &alternative.products {
                    require_text(&product.name, "alternative product name")?;
                    require_text(&product.formula, "alternative product formula")?;
                    text.push(&product.name);
                    text.push(&product.formula);
                }
            }
        }
        reject_procedural_content(&text)
    }
}

fn require_text(value: &str, label: &str) -> Result<(), AgentError> {
    if value.trim().is_empty() {
        Err(AgentError::new(
            "reaction claim",
            format!("{label} cannot be empty"),
        ))
    } else {
        Ok(())
    }
}

fn reject_procedural_content(values: &[&str]) -> Result<(), AgentError> {
    const BLOCKED: [&str; 18] = [
        "apparatus",
        "procedure",
        "milliliter",
        "millilitre",
        " grams",
        "temperature",
        "heat to",
        "stir for",
        "add slowly",
        "concentration",
        "purif",
        "collect the gas",
        "reaction vessel",
        "protective equipment",
        "safety goggles",
        "wear gloves",
        "ventilation",
        "yield percent",
    ];
    if values.iter().any(|value| {
        let normalized = value.to_ascii_lowercase();
        BLOCKED.iter().any(|blocked| normalized.contains(blocked))
    }) {
        return Err(AgentError::new(
            "reaction claim",
            "provider output contains disallowed procedural or hazard-control content",
        ));
    }
    Ok(())
}

/// Fully labelled, locally resolved structures supplied to mechanism
/// escalation. The provider cannot add or replace any of these structures.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MechanismEscalationRequest {
    pub schema_version: u32,
    pub reaction_id: String,
    pub reactants: Vec<MechanismSpecies>,
    pub products: Vec<MechanismSpecies>,
    /// Every reactant atom path across all coefficient instances. The mapping
    /// must use each exactly once; listing them removes instance-indexing
    /// ambiguity for the provider.
    pub reactant_atom_paths: Vec<String>,
    /// Every product atom path across all coefficient instances.
    pub product_atom_paths: Vec<String>,
    /// Reviewed neutral-valence axioms used by `ChemSpec` to derive, rather
    /// than accept, any provisional operation state.
    pub neutral_valence: Vec<ElementValenceRecord>,
    /// Every reviewed covalent electron state an involved element may pass
    /// through. Operations whose before/after states leave this set fail
    /// kernel validation, so the closed vocabulary is stated up front.
    pub supported_states: Vec<ValenceStateRecord>,
    /// Every reviewed metallic site state for the involved elements.
    pub metallic_states: Vec<MetallicValenceStateRecord>,
    /// Signals that the model may propose an electron state outside the
    /// reviewed list; `ChemSpec` still derives and validates its valence record.
    pub provisional_states_allowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MechanismSpecies {
    pub role: String,
    pub coefficient: u32,
    pub structure: LabelledStructure,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "representation", rename_all = "snake_case", deny_unknown_fields)]
pub enum LabelledStructure {
    Molecular {
        id: String,
        formula: String,
        atoms: Vec<AtomRecord>,
        bonds: Vec<BondRecord>,
        groups: Vec<GroupRecord>,
    },
    Ion {
        id: String,
        formula: String,
        atoms: Vec<AtomRecord>,
        bonds: Vec<BondRecord>,
        groups: Vec<GroupRecord>,
    },
    Ionic {
        id: String,
        formula: String,
        components: Vec<ComponentRecord>,
        associations: Vec<IonicAssociationRecord>,
    },
    Metallic {
        id: String,
        formula: String,
        sites: Vec<AtomRecord>,
        domains: Vec<MetallicDomainRecord>,
    },
}

/// Request for structural graphs of claimed products absent from the reviewed
/// structure library. `ChemSpec` names the species and the exact formulas;
/// the model may only fill in one graph per requested species.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StructureProposalRequest {
    pub schema_version: u32,
    pub species: Vec<StructureProposalSpecies>,
    pub neutral_valence: Vec<ElementValenceRecord>,
    pub supported_states: Vec<ValenceStateRecord>,
    pub metallic_states: Vec<MetallicValenceStateRecord>,
    pub provisional_states_allowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StructureProposalSpecies {
    pub id: String,
    pub name: String,
    pub formula: String,
}

/// The only model-authored content accepted during structure escalation. Every
/// proposed structure is untrusted until it crosses the full catalogue
/// validation boundary inside an isolated working bundle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StructureProposalResponse {
    pub schema_version: u32,
    pub structures: Vec<LabelledStructure>,
}

impl StructureProposalResponse {
    /// Strictly decodes a bounded structure proposal. Graph, formula, charge,
    /// and valence validation intentionally happens later inside an isolated
    /// working catalogue bundle.
    ///
    /// # Errors
    ///
    /// Returns an error for oversized JSON, unknown fields or variants, or an
    /// unsupported schema version.
    pub fn from_json(bytes: &[u8]) -> Result<Self, AgentError> {
        if bytes.len() > MAX_STRUCTURE_RESPONSE_BYTES {
            return Err(AgentError::new(
                "structure proposal",
                format!("response exceeds the {MAX_STRUCTURE_RESPONSE_BYTES}-byte contract limit"),
            ));
        }
        let response: Self = serde_json::from_slice(bytes)
            .map_err(|error| AgentError::new("structure proposal", error.to_string()))?;
        if response.schema_version != STRUCTURE_PROPOSAL_SCHEMA_VERSION {
            return Err(AgentError::new(
                "structure proposal",
                format!("unsupported structure schema {}", response.schema_version),
            ));
        }
        if response.structures.is_empty() {
            return Err(AgentError::new(
                "structure proposal",
                "structures must be non-empty",
            ));
        }
        Ok(response)
    }
}

/// The only model-authored content accepted during mechanism escalation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MechanismEscalationResponse {
    pub schema_version: u32,
    pub mapping: Vec<MechanismMapping>,
    pub operations: Vec<MechanismOperation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MechanismMapping {
    pub reactant: String,
    pub product: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MechanismOperation {
    ReconfigureElectrons {
        atom: String,
        before: ElectronStateRecord,
        after: ElectronStateRecord,
    },
    CleaveCovalent {
        edge: (String, String, BondOrderRecord),
        allocation: MechanismCleavageAllocation,
        before: BinaryElectronStateRecord,
        after: BinaryElectronStateRecord,
    },
    FormCovalent {
        edge: (String, String, BondOrderRecord),
        electron_contribution: ElectronContributionRecord,
        before: BinaryElectronStateRecord,
        after: BinaryElectronStateRecord,
    },
    CleaveDative {
        donor: String,
        acceptor: String,
        allocation: MechanismCleavageAllocation,
        before: BinaryElectronStateRecord,
        after: BinaryElectronStateRecord,
    },
    FormDative {
        donor: String,
        acceptor: String,
        before: BinaryElectronStateRecord,
        after: BinaryElectronStateRecord,
    },
    ChangeCovalent {
        edge: (String, String),
        old_order: BondOrderRecord,
        new_order: BondOrderRecord,
        allocation: MechanismCleavageAllocation,
        before: BinaryElectronStateRecord,
        after: BinaryElectronStateRecord,
    },
    AssociateIonic {
        label: String,
        components: Vec<Vec<String>>,
        component_charges: Vec<i16>,
    },
    DissociateIonic {
        association: String,
    },
    ReleaseMetallic {
        site: String,
        domain: String,
        allocation: MetallicReleaseAllocationRecord,
        before: MetallicElectronStateRecord,
        after: MetallicElectronStateRecord,
    },
    JoinMetallic {
        site: String,
        domain: String,
        allocation: MetallicJoinAllocationRecord,
        before: MetallicElectronStateRecord,
        after: MetallicElectronStateRecord,
    },
    TransferElectron {
        count: u8,
        donor: String,
        acceptor: String,
        before: TransferElectronStateRecord,
        after: TransferElectronStateRecord,
    },
    AssignProduct {
        atoms: Vec<String>,
        product: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MechanismCleavageAllocation {
    Homolytic(MechanismHomolytic),
    Heterolytic { heterolytic_to: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MechanismHomolytic {
    Homolytic,
}

impl MechanismEscalationResponse {
    /// Strictly decodes a bounded mechanism response. Structural and label
    /// validation intentionally happens later against its exact request.
    ///
    /// # Errors
    ///
    /// Returns an error for oversized JSON, unknown fields or variants, or an
    /// unsupported schema version.
    pub fn from_json(bytes: &[u8]) -> Result<Self, AgentError> {
        if bytes.len() > MAX_MECHANISM_RESPONSE_BYTES {
            return Err(AgentError::new(
                "mechanism response",
                format!("response exceeds the {MAX_MECHANISM_RESPONSE_BYTES}-byte contract limit"),
            ));
        }
        let response: Self = serde_json::from_slice(bytes)
            .map_err(|error| AgentError::new("mechanism response", error.to_string()))?;
        if response.schema_version != MECHANISM_ESCALATION_SCHEMA_VERSION {
            return Err(AgentError::new(
                "mechanism response",
                format!("unsupported mechanism schema {}", response.schema_version),
            ));
        }
        if response.mapping.is_empty() || response.operations.is_empty() {
            return Err(AgentError::new(
                "mechanism response",
                "mapping and operations must be non-empty",
            ));
        }
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rubidium_claim() -> serde_json::Value {
        json!({
            "schema_version": 1,
            "disposition": "reaction",
            "products": [
                {
                    "name": "rubidium hydroxide",
                    "formula": "RbOH",
                    "phase": "aqueous",
                    "identity_hints": []
                },
                {
                    "name": "hydrogen",
                    "formula": "H2",
                    "phase": "gas",
                    "identity_hints": []
                }
            ],
            "required_context": "Representative reaction with liquid water",
            "observations": [{
                "predicate": "evolves",
                "subject": "hydrogen",
                "value": null
            }],
            "sources": [],
            "ambiguity": null
        })
    }

    #[test]
    fn rubidium_water_claim_has_no_structural_ledger() {
        let bytes = serde_json::to_vec(&rubidium_claim()).expect("claim JSON");
        let claim = ReactionClaim::from_json(&bytes, ClaimMode::Fast).expect("valid claim");
        assert_eq!(claim.products.len(), 2);
        let text = String::from_utf8(bytes).expect("UTF-8 JSON");
        for forbidden in [
            "coefficients",
            "structures",
            "valence",
            "mapping",
            "operations",
            "catalogue",
            ".chems",
        ] {
            assert!(!text.contains(forbidden));
        }
    }

    #[test]
    fn claim_contract_rejects_unknown_missing_and_procedural_content() {
        let mut unknown = rubidium_claim();
        unknown["procedure"] = json!("none");
        assert!(
            ReactionClaim::from_json(
                &serde_json::to_vec(&unknown).expect("unknown JSON"),
                ClaimMode::Fast
            )
            .is_err()
        );

        let mut missing = rubidium_claim();
        missing.as_object_mut().expect("object").remove("products");
        assert!(
            ReactionClaim::from_json(
                &serde_json::to_vec(&missing).expect("missing JSON"),
                ClaimMode::Fast
            )
            .is_err()
        );

        let mut procedural = rubidium_claim();
        procedural["required_context"] = json!("Heat to 80 C and stir for ten minutes");
        let error = ReactionClaim::from_json(
            &serde_json::to_vec(&procedural).expect("procedural JSON"),
            ClaimMode::Fast,
        )
        .expect_err("procedural content must fail closed");
        assert!(error.to_string().contains("procedural"));

        let mut empty_context = rubidium_claim();
        empty_context["required_context"] = json!("");
        assert!(
            ReactionClaim::from_json(
                &serde_json::to_vec(&empty_context).expect("empty context JSON"),
                ClaimMode::Fast
            )
            .is_err()
        );
    }

    #[test]
    fn closed_dispositions_represent_no_reaction_condition_and_conflict() {
        let no_reaction = json!({
            "schema_version": 1,
            "disposition": "no_reaction",
            "products": [],
            "required_context": "Ordinary contact",
            "observations": [],
            "sources": [],
            "ambiguity": null
        });
        ReactionClaim::from_json(
            &serde_json::to_vec(&no_reaction).expect("no reaction"),
            ClaimMode::Fast,
        )
        .expect("no-reaction claim");

        let ambiguous = json!({
            "schema_version": 1,
            "disposition": "ambiguous",
            "products": [],
            "required_context": "Outcome depends on unspecified conditions",
            "observations": [],
            "sources": [],
            "ambiguity": {
                "kind": "conflicting_evidence",
                "summary": "Direct sources disagree",
                "alternatives": [
                    {"label":"outcome A","products":[],"required_context":"condition A"},
                    {"label":"outcome B","products":[],"required_context":"condition B"}
                ]
            }
        });
        ReactionClaim::from_json(
            &serde_json::to_vec(&ambiguous).expect("ambiguous"),
            ClaimMode::Fast,
        )
        .expect("conflicting claim");
    }

    #[test]
    fn mechanism_contract_rejects_extra_output_authority() {
        let valid = json!({
            "schema_version": 1,
            "mapping": [{"reactant":"hydrogen[1].h1","product":"hydrogen[1].h1"}],
            "operations": [{
                "kind":"assign_product",
                "atoms":["hydrogen[1].h1"],
                "product":"hydrogen[1]"
            }]
        });
        MechanismEscalationResponse::from_json(
            &serde_json::to_vec(&valid).expect("valid mechanism"),
        )
        .expect("closed mechanism");

        let mut invalid = valid;
        invalid["structures"] = json!([]);
        assert!(
            MechanismEscalationResponse::from_json(
                &serde_json::to_vec(&invalid).expect("invalid mechanism")
            )
            .is_err()
        );
    }
}
