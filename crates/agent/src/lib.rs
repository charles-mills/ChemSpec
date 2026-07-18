//! Provider-neutral dynamic reaction construction for `ChemSpec`.
//!
//! Providers return a compact factual claim and, only when requested, a closed
//! mapping/operation proposal over host-labelled structures. Domain identity,
//! exact balancing, family selection, kernel validation, and frame projection
//! remain local trust boundaries.

mod cache;
mod claim;
mod codex;
mod family;
mod identity;
mod iupac_name;
mod mechanism;
mod mechanize;
mod naming;
mod organic;
mod outcome;
mod presentation;
mod solve;
mod structure;

use std::fmt;

use chem_domain::SpeciesId;
use serde::Serialize;

pub use cache::{
    DYNAMIC_CACHE_SCHEMA_VERSION, DynamicCachePresentation, LoadedDynamicCache, dynamic_cache_path,
    load_dynamic_cache, store_dynamic_cache,
};
pub use claim::{
    ClaimAlternative, ClaimAmbiguity, ClaimAmbiguityKind, ClaimDisposition, ClaimField,
    ClaimIdentityHint, ClaimIdentityHintKind, ClaimMode, ClaimObservation,
    ClaimObservationPredicate, ClaimPhase, ClaimProduct, ClaimSource, LabelledStructure,
    MechanismCleavageAllocation, MechanismEscalationRequest, MechanismEscalationResponse,
    MechanismHomolytic, MechanismMapping, MechanismOperation, MechanismSpecies, NoReactionReason,
    ReactionClaim, StructureProposalRequest, StructureProposalResponse, StructureProposalSpecies,
};
pub use codex::{
    CodexPreflight, CodexProgressEvent, CodexProgressStage, CodexProvider, CodexProviderConfig,
    FAST_CLAIM_TIMEOUT, MECHANISM_TIMEOUT,
};
pub use family::{
    FamilyMatchOutcome, ReviewedAnimationOutcome, ReviewedFamilyMatch, compile_reviewed_animation,
    match_reviewed_family,
};
pub use identity::{
    IdentityAdapterError, IdentityResolutionOutcome, NoStructureDecoder, SpeciesIdentityAdapter,
    StructureIdentityDecoder, load_identity_cache, resolve_species_identity,
    reviewed_element_registry, reviewed_species_registry, store_identity_cache,
};
pub use mechanism::{
    EscalatedMechanismOutcome, MechanismContext, MechanismEscalationOutcome, MechanismProvider,
    UnsupportedMechanismProvider, compile_mechanism_request, derive_mechanism,
    validate_escalated_response, validate_escalated_response_with_structures,
};
pub use naming::{
    composition_from_name, compound_name, ion_pair_name, molecular_graph_name, structure_name,
};
pub use outcome::{
    CompiledClaimOutcome, MacroscopicProcess, OutcomeSpecies, ReactantIdentityAmbiguity,
    RequestIdentityResolution, TrustTier, ValidatedStaticOutcome, compile_claim_outcome,
    resolve_request_identities, resolve_request_identities_with_catalogue, resolve_request_species,
};
pub use presentation::{DynamicPresentationOutcome, enrich_static_outcome};
pub use solve::solve_reaction_claim;
pub use structure::{
    AdoptedProposedStructures, adopt_proposed_structures, structure_proposal_request,
};

/// Stage timings for one dynamic reaction build.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LatencyMilestones {
    pub claim_ms: Option<u64>,
    pub evidence_ms: Option<u64>,
    pub static_outcome_ms: Option<u64>,
    pub mechanism_ms: Option<u64>,
    pub reviewed_animation_ms: Option<u64>,
}

/// One structured reactant as composed by the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReactantInput {
    pub display: String,
    pub atomic_numbers: Vec<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub species_id: Option<SpeciesId>,
}

/// Provider-neutral request for a reaction absent from the local fast path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReactionBuildRequest {
    pub reactants: Vec<ReactantInput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_context: Option<String>,
}

/// Stable provider/build failure boundary. No failure variant is a chemistry
/// result and callers must keep playback blocked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentError {
    stage: &'static str,
    message: String,
}

impl AgentError {
    fn new(stage: &'static str, message: impl Into<String>) -> Self {
        Self {
            stage,
            message: message.into(),
        }
    }

    #[must_use]
    pub const fn stage(&self) -> &'static str {
        self.stage
    }
}

impl fmt::Display for AgentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.stage, self.message)
    }
}

impl std::error::Error for AgentError {}
