//! Provider-neutral dynamic reaction construction for `ChemSpec`.
//!
//! Providers return a compact factual claim and, only when requested, a closed
//! mapping/operation proposal over host-labelled structures. Domain identity,
//! exact balancing, family selection, kernel validation, and frame projection
//! remain local trust boundaries.

mod appearance;
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

use std::{error::Error, fmt, sync::Arc};

use chem_domain::SpeciesId;
use serde::Serialize;

pub use appearance::{
    AppearanceSource, OXIDE_APPEARANCE_SCHEMA_VERSION, OxideAppearanceClaim,
    OxideAppearanceRequest, OxideColourFamily, ValidatedOxideAppearance,
    baseline_oxide_colour_family, load_oxide_appearance_cache, oxide_appearance_cache_path,
    store_oxide_appearance_cache,
};
pub use cache::{
    DYNAMIC_CACHE_SCHEMA_VERSION, DynamicCachePresentation, LoadedDynamicCache, dynamic_cache_path,
    load_dynamic_cache, store_dynamic_cache,
};
pub use claim::{
    ClaimAlternative, ClaimAmbiguity, ClaimAmbiguityKind, ClaimDisposition, ClaimField,
    ClaimIdentityHint, ClaimIdentityHintKind, ClaimInput, ClaimMode, ClaimObservation,
    ClaimObservationPredicate, ClaimPhase, ClaimProduct, ClaimProvenance, ClaimSource,
    LabelledStructure, MechanismCleavageAllocation, MechanismEscalationRequest,
    MechanismEscalationResponse, MechanismHomolytic, MechanismMapping, MechanismOperation,
    MechanismSpecies, NoReactionReason, ProviderClaim, ReactionClaim, SolvedClaim,
    StructureProposalRequest, StructureProposalResponse, StructureProposalSpecies,
};
pub use codex::{
    CodexPreflight, CodexProgressEvent, CodexProgressStage, CodexProvider, CodexProviderConfig,
    FAST_CLAIM_TIMEOUT, MECHANISM_TIMEOUT, OXIDE_APPEARANCE_TIMEOUT, REACTION_MORE_INFO_TIMEOUT,
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
    CompiledClaimOutcome, MacroscopicColour, MacroscopicProcess, OutcomeProvenance, OutcomeSpecies,
    ReactantIdentityAmbiguity, RequestIdentityResolution, ValidatedStaticOutcome,
    compile_claim_outcome, compile_claim_outcome_with_catalogue, resolve_request_identities,
    resolve_request_identities_with_catalogue, resolve_request_species,
};
pub use presentation::{DynamicPresentationOutcome, enrich_static_outcome};
pub use solve::{solve_reaction_claim, solve_reaction_claim_with_catalogue};
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

/// Stable, exhaustive classification for provider and dynamic-build failures.
/// No variant is a chemistry result; callers must keep playback blocked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentErrorKind {
    Cancelled,
    TimedOut,
    UnsupportedCapability,
    ProviderUnavailable,
    ProviderFailure,
    InvalidProviderOutput,
    CacheIo,
    InvalidCache,
    IdentityFailure,
    InvalidRequest,
    CompilationFailure,
    KernelRejection,
    InternalFailure,
}

/// Stable provider/build failure boundary with a closed classification and an
/// optional concrete source error for subsystem diagnostics.
#[derive(Debug, Clone)]
pub struct AgentError {
    kind: AgentErrorKind,
    context: &'static str,
    message: String,
    source: Option<Arc<dyn Error + Send + Sync>>,
}

impl AgentError {
    fn new(kind: AgentErrorKind, context: &'static str, message: impl Into<String>) -> Self {
        Self {
            kind,
            context,
            message: message.into(),
            source: None,
        }
    }

    fn from_source<E>(kind: AgentErrorKind, context: &'static str, source: E) -> Self
    where
        E: Error + Send + Sync + 'static,
    {
        Self {
            kind,
            context,
            message: source.to_string(),
            source: Some(Arc::new(source)),
        }
    }

    #[must_use]
    pub const fn kind(&self) -> AgentErrorKind {
        self.kind
    }

    #[must_use]
    pub const fn context(&self) -> &'static str {
        self.context
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for AgentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.context, self.message)
    }
}

impl Error for AgentError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source
            .as_deref()
            .map(|source| source as &(dyn Error + 'static))
    }
}

#[cfg(test)]
mod test_support {
    use chem_catalogue::{CatalogueEnvelope, ReferenceCatalogue, ReferenceIntegrityPolicy};
    use chem_domain::ContentDigest;

    pub(crate) fn reference_catalogue() -> ReferenceCatalogue {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let catalogue =
            std::fs::read(root.join("catalogue/reference/core-chemistry/catalogue.json"))
                .expect("catalogue");
        let review = std::fs::read(root.join("catalogue/reviews/core-chemistry.review.json"))
            .expect("review");
        let envelope: CatalogueEnvelope = serde_json::from_slice(&catalogue).expect("envelope");
        let review_value = serde_json::from_slice(&review).expect("review value");
        ReferenceCatalogue::from_canonical_json(
            &catalogue,
            &review,
            ReferenceIntegrityPolicy::new(
                envelope.digest,
                ContentDigest::of_json(&review_value).expect("review digest"),
            ),
        )
        .expect("reference catalogue")
    }
}

#[cfg(test)]
mod error_tests {
    use super::*;

    #[test]
    fn agent_errors_expose_closed_kinds_and_concrete_sources() {
        let source = serde_json::from_slice::<serde_json::Value>(b"{")
            .expect_err("truncated JSON must fail");
        let error = AgentError::from_source(
            AgentErrorKind::InvalidProviderOutput,
            "reaction claim",
            source,
        );

        assert_eq!(error.kind(), AgentErrorKind::InvalidProviderOutput);
        assert_eq!(error.context(), "reaction claim");
        assert!(
            Error::source(&error)
                .and_then(|source| source.downcast_ref::<serde_json::Error>())
                .is_some()
        );
        assert!(Error::source(&error.clone()).is_some());
    }

    #[test]
    fn semantic_agent_errors_do_not_fabricate_sources() {
        let error = AgentError::new(
            AgentErrorKind::UnsupportedCapability,
            "structure proposal",
            "provider does not support structure proposals",
        );

        assert_eq!(error.kind(), AgentErrorKind::UnsupportedCapability);
        assert!(Error::source(&error).is_none());
    }
}
