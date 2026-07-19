use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use chem_catalogue::TrustedCatalogue;
use chem_domain::{ContentDigest, ReactionRuleId, SpeciesRegistry};
use serde::{Deserialize, Serialize};

use crate::claim::{
    MECHANISM_ESCALATION_SCHEMA_VERSION, REACTION_CLAIM_SCHEMA_VERSION,
    STRUCTURE_PROPOSAL_SCHEMA_VERSION,
};
use crate::{
    AgentError, AgentErrorKind, ClaimMode, CompiledClaimOutcome, DynamicPresentationOutcome,
    FamilyMatchOutcome, MechanismEscalationResponse, ProviderClaim, ReactionBuildRequest,
    StructureProposalResponse, TrustTier, compile_claim_outcome_with_catalogue,
    compile_reviewed_animation, match_reviewed_family, resolve_request_species,
    validate_escalated_response_with_structures,
};

pub const DYNAMIC_CACHE_SCHEMA_VERSION: u32 = 3;
// Version 6 refreshes researched claims under the closed visible-colour
// observation contract in addition to the phase-aware compiler boundary.
const DYNAMIC_COMPILER_CONTRACT_VERSION: u32 = 6;
const MAX_CACHE_BYTES: u64 = 2 * 1024 * 1024;
static CACHE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

/// Untrusted presentation recipe persisted after a validated build. It gains
/// no authority from storage and is revalidated on every load.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DynamicCachePresentation {
    ReviewedFamily {
        rule_id: ReactionRuleId,
    },
    Escalated {
        response: MechanismEscalationResponse,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        structures: Option<StructureProposalResponse>,
    },
    Static {
        diagnostic: String,
        retryable: bool,
    },
}

#[derive(Debug, Clone)]
pub struct LoadedDynamicCache {
    pub outcome: CompiledClaimOutcome,
    pub presentation: Option<DynamicPresentationOutcome>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DynamicCacheEnvelope {
    schema_version: u32,
    compiler_contract_version: u32,
    provider: String,
    model: String,
    mode: ClaimMode,
    request_binding: ContentDigest,
    identity_snapshot: ContentDigest,
    catalogue_digest: ContentDigest,
    claim_digest: ContentDigest,
    claim: ProviderClaim,
    trust_tier: TrustTier,
    presentation: Option<DynamicCachePresentation>,
}

/// Computes the cache path from stable request identities and every governing
/// local contract.
///
/// # Errors
///
/// Returns an identity-binding or canonical serialization error.
pub fn dynamic_cache_path(
    directory: &Path,
    request: &ReactionBuildRequest,
    mode: ClaimMode,
    identities: &SpeciesRegistry,
    catalogue: &TrustedCatalogue,
) -> Result<PathBuf, AgentError> {
    let request_binding = request_binding(request, identities)?;
    let identity_snapshot = identities.snapshot_digest().map_err(|error| {
        AgentError::from_source(AgentErrorKind::InvalidCache, "reaction cache", error)
    })?;
    let material = serde_json::to_vec(&(
        DYNAMIC_CACHE_SCHEMA_VERSION,
        DYNAMIC_COMPILER_CONTRACT_VERSION,
        mode,
        request_binding,
        identity_snapshot,
        catalogue.digest(),
        REACTION_CLAIM_SCHEMA_VERSION,
        MECHANISM_ESCALATION_SCHEMA_VERSION,
        STRUCTURE_PROPOSAL_SCHEMA_VERSION,
    ))
    .map_err(|error| {
        AgentError::from_source(AgentErrorKind::InvalidCache, "reaction cache", error)
    })?;
    Ok(directory.join(format!(
        "{}.json",
        ContentDigest::sha256(&material).to_hex()
    )))
}

/// Loads and fully revalidates a v3 entry. Invalid, corrupt, and old-format
/// entries are cache misses and remain untouched.
#[must_use]
pub fn load_dynamic_cache(
    directory: Option<&Path>,
    request: &ReactionBuildRequest,
    mode: ClaimMode,
    identities: &SpeciesRegistry,
    catalogue: &TrustedCatalogue,
) -> Option<LoadedDynamicCache> {
    let path = dynamic_cache_path(directory?, request, mode, identities, catalogue).ok()?;
    let metadata = fs::metadata(&path).ok()?;
    if metadata.len() > MAX_CACHE_BYTES {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    let cached: DynamicCacheEnvelope = serde_json::from_slice(&bytes).ok()?;
    cached.claim.validate_wire().ok()?;
    cached
        .presentation
        .as_ref()
        .map_or(Ok(()), validate_cached_presentation)
        .ok()?;
    let expected_request = request_binding(request, identities).ok()?;
    let expected_identities = identities.snapshot_digest().ok()?;
    if cached.schema_version != DYNAMIC_CACHE_SCHEMA_VERSION
        || cached.compiler_contract_version != DYNAMIC_COMPILER_CONTRACT_VERSION
        || cached.mode != mode
        || cached.request_binding != expected_request
        || cached.identity_snapshot != expected_identities
        || cached.catalogue_digest != catalogue.digest()
        || cached.claim_digest != claim_digest(&cached.claim).ok()?
    {
        return None;
    }
    let DynamicCacheEnvelope {
        claim,
        trust_tier,
        presentation: cached_presentation,
        ..
    } = cached;
    if trust_tier != TrustTier::ModelAsserted {
        return None;
    }
    let outcome =
        compile_claim_outcome_with_catalogue(request, claim, identities, catalogue).ok()?;
    let presentation = match (&outcome, cached_presentation) {
        (CompiledClaimOutcome::Static(static_outcome), Some(recipe)) => {
            Some(revalidate_presentation(static_outcome.clone(), recipe, catalogue).ok()?)
        }
        (_, None) => None,
        (_, Some(_)) => return None,
    };
    let presentation = match presentation {
        Some(DynamicPresentationOutcome::Static {
            retryable: true, ..
        }) => None,
        other => other,
    };
    Some(LoadedDynamicCache {
        outcome,
        presentation,
    })
}

/// Atomically writes a v3 entry after its claim has compiled. Callers may
/// replace the initial static entry with a validated presentation recipe.
///
/// # Errors
///
/// Returns an identity, serialization, directory, or atomic-write error.
#[allow(clippy::too_many_arguments)]
pub fn store_dynamic_cache(
    directory: &Path,
    request: &ReactionBuildRequest,
    mode: ClaimMode,
    identities: &SpeciesRegistry,
    catalogue: &TrustedCatalogue,
    claim: &ProviderClaim,
    presentation: Option<DynamicCachePresentation>,
    provider: &str,
    model: &str,
) -> Result<(), AgentError> {
    claim.validate_wire()?;
    if let Some(presentation) = &presentation {
        validate_cached_presentation(presentation)?;
    }
    // Recompile before persistence so a provider response alone can never
    // populate the cache.
    compile_claim_outcome_with_catalogue(request, claim.clone(), identities, catalogue)?;
    let trust_tier = TrustTier::ModelAsserted;
    let path = dynamic_cache_path(directory, request, mode, identities, catalogue)?;
    let envelope = DynamicCacheEnvelope {
        schema_version: DYNAMIC_CACHE_SCHEMA_VERSION,
        compiler_contract_version: DYNAMIC_COMPILER_CONTRACT_VERSION,
        provider: provider.to_owned(),
        model: model.to_owned(),
        mode,
        request_binding: request_binding(request, identities)?,
        identity_snapshot: identities.snapshot_digest().map_err(|error| {
            AgentError::from_source(AgentErrorKind::InvalidCache, "reaction cache", error)
        })?,
        catalogue_digest: catalogue.digest(),
        claim_digest: claim_digest(claim)?,
        claim: claim.clone(),
        trust_tier,
        presentation,
    };
    let bytes = serde_json::to_vec(&envelope).map_err(|error| {
        AgentError::from_source(AgentErrorKind::InvalidCache, "reaction cache", error)
    })?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_CACHE_BYTES {
        return Err(AgentError::new(
            AgentErrorKind::InvalidCache,
            "reaction cache",
            "entry exceeds size limit",
        ));
    }
    fs::create_dir_all(directory).map_err(|error| {
        AgentError::from_source(AgentErrorKind::CacheIo, "reaction cache", error)
    })?;
    let sequence = CACHE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = directory.join(format!(".dynamic-v3-{}-{sequence}.tmp", std::process::id()));
    fs::write(&temporary, bytes).map_err(|error| {
        AgentError::from_source(AgentErrorKind::CacheIo, "reaction cache", error)
    })?;
    atomic_replace(&temporary, &path, "reaction cache")
}

fn validate_cached_presentation(presentation: &DynamicCachePresentation) -> Result<(), AgentError> {
    if let DynamicCachePresentation::Escalated {
        response,
        structures,
    } = presentation
    {
        response.validate_wire()?;
        if let Some(structures) = structures {
            structures.validate_wire()?;
        }
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn atomic_replace(
    temporary: &Path,
    destination: &Path,
    context: &'static str,
) -> Result<(), AgentError> {
    fs::rename(temporary, destination)
        .map_err(|error| AgentError::from_source(AgentErrorKind::CacheIo, context, error))
}

#[cfg(target_os = "windows")]
fn atomic_replace(
    temporary: &Path,
    destination: &Path,
    context: &'static str,
) -> Result<(), AgentError> {
    if !destination.exists() {
        return fs::rename(temporary, destination)
            .map_err(|error| AgentError::from_source(AgentErrorKind::CacheIo, context, error));
    }
    let backup = destination.with_extension(format!(
        "backup-{}",
        CACHE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::rename(destination, &backup)
        .map_err(|error| AgentError::from_source(AgentErrorKind::CacheIo, context, error))?;
    if let Err(error) = fs::rename(temporary, destination) {
        let _ = fs::rename(&backup, destination);
        return Err(AgentError::from_source(
            AgentErrorKind::CacheIo,
            context,
            error,
        ));
    }
    let _ = fs::remove_file(backup);
    Ok(())
}

fn revalidate_presentation(
    outcome: crate::ValidatedStaticOutcome,
    recipe: DynamicCachePresentation,
    catalogue: &TrustedCatalogue,
) -> Result<DynamicPresentationOutcome, AgentError> {
    match recipe {
        DynamicCachePresentation::ReviewedFamily { rule_id } => {
            let matched = match_reviewed_family(&outcome, catalogue)?;
            let FamilyMatchOutcome::Matched(family) = matched else {
                return Err(AgentError::new(
                    AgentErrorKind::InvalidCache,
                    "reaction cache",
                    "cached reviewed family is no longer uniquely applicable",
                ));
            };
            if family.rule_id() != &rule_id {
                return Err(AgentError::new(
                    AgentErrorKind::InvalidCache,
                    "reaction cache",
                    "cached reviewed family binding changed",
                ));
            }
            Ok(DynamicPresentationOutcome::ReviewedFamily(Box::new(
                compile_reviewed_animation(outcome, *family, catalogue)?,
            )))
        }
        DynamicCachePresentation::Escalated {
            response,
            structures,
        } => Ok(DynamicPresentationOutcome::Escalated(Box::new(
            validate_escalated_response_with_structures(
                outcome,
                structures.as_ref(),
                &response,
                catalogue,
            )?,
        ))),
        DynamicCachePresentation::Static {
            diagnostic,
            retryable,
        } => Ok(DynamicPresentationOutcome::Static {
            outcome: Box::new(outcome),
            diagnostic,
            retryable,
            attempts: 0,
        }),
    }
}

fn request_binding(
    request: &ReactionBuildRequest,
    identities: &SpeciesRegistry,
) -> Result<ContentDigest, AgentError> {
    let mut ids = resolve_request_species(request, identities)?
        .into_iter()
        .map(|species| species.id().clone())
        .collect::<Vec<_>>();
    ids.sort();
    let selected_context = request.selected_context.as_deref().map(|context| {
        context
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    });
    let bytes = serde_json::to_vec(&(ids, selected_context)).map_err(|error| {
        AgentError::from_source(AgentErrorKind::InvalidCache, "reaction cache", error)
    })?;
    Ok(ContentDigest::sha256(&bytes))
}

fn claim_digest(claim: &ProviderClaim) -> Result<ContentDigest, AgentError> {
    let bytes = serde_json::to_vec(claim).map_err(|error| {
        AgentError::from_source(AgentErrorKind::InvalidCache, "reaction cache", error)
    })?;
    Ok(ContentDigest::sha256(&bytes))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::{
        ReactantInput, reviewed_species_registry, test_support::trusted_catalogue as trusted,
    };

    fn request() -> ReactionBuildRequest {
        ReactionBuildRequest {
            reactants: [
                ReactantInput {
                    display: "LithiumMetal".into(),
                    atomic_numbers: vec![3],
                    species_id: None,
                },
                ReactantInput {
                    display: "H2O".into(),
                    atomic_numbers: vec![1, 1, 8],
                    species_id: None,
                },
            ]
            .to_vec(),
            selected_context: None,
        }
    }

    fn claim() -> ProviderClaim {
        let value = json!({
            "schema_version": 1,
            "disposition": "reaction",
            "products": [
                {"name":"lithium hydroxide","formula":"LiOH","phase":"aqueous","identity_hints":[]},
                {"name":"hydrogen","formula":"H2","phase":"gas","identity_hints":[]}
            ],
            "required_context":"representative educational outcome under the reviewed standard-outcome premise",
            "observations":[
                {"predicate":"evolves","subject":"hydrogen","value":null}
            ],
            "sources":[], "ambiguity":null
        });
        ProviderClaim::from_json(
            &serde_json::to_vec(&value).expect("claim JSON"),
            ClaimMode::Fast,
        )
        .expect("claim")
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn v3_cache_revalidates_and_ignores_corrupt_entries_without_deleting() {
        let trusted = trusted();
        let identities = reviewed_species_registry(&trusted).expect("identities");
        let request = request();
        let claim = claim();
        let directory = std::env::temp_dir().join(format!(
            "chemspec-cache-v3-test-{}-{}",
            std::process::id(),
            CACHE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        store_dynamic_cache(
            &directory,
            &request,
            ClaimMode::Fast,
            &identities,
            &trusted,
            &claim,
            None,
            "fake",
            "offline",
        )
        .expect("store static");
        let loaded = load_dynamic_cache(
            Some(&directory),
            &request,
            ClaimMode::Fast,
            &identities,
            &trusted,
        )
        .expect("load validated");
        let CompiledClaimOutcome::Static(loaded_outcome) = &loaded.outcome else {
            panic!("cached outcome remains static")
        };
        assert_eq!(
            loaded_outcome.macroscopic_process(),
            Some(crate::MacroscopicProcess::GasEvolutionSolidLiquid),
            "cache loads must retain catalogue-backed phases and generic visual classification"
        );
        assert!(loaded.presentation.is_none());
        let CompiledClaimOutcome::Static(static_outcome) =
            crate::compile_claim_outcome(&request, claim.clone(), &identities).expect("compiled")
        else {
            panic!("static outcome")
        };
        let FamilyMatchOutcome::Matched(family) =
            match_reviewed_family(&static_outcome, &trusted).expect("family")
        else {
            panic!("reviewed family")
        };
        store_dynamic_cache(
            &directory,
            &request,
            ClaimMode::Fast,
            &identities,
            &trusted,
            &claim,
            Some(DynamicCachePresentation::ReviewedFamily {
                rule_id: family.rule_id().clone(),
            }),
            "fake",
            "offline",
        )
        .expect("transactional presentation replacement");
        let loaded = load_dynamic_cache(
            Some(&directory),
            &request,
            ClaimMode::Fast,
            &identities,
            &trusted,
        )
        .expect("load reviewed presentation");
        assert!(matches!(
            loaded.presentation,
            Some(DynamicPresentationOutcome::ReviewedFamily(_))
        ));
        let started = std::time::Instant::now();
        assert!(
            load_dynamic_cache(
                Some(&directory),
                &request,
                ClaimMode::Fast,
                &identities,
                &trusted
            )
            .is_some()
        );
        assert!(
            started.elapsed() < std::time::Duration::from_millis(250),
            "one validated offline replay exceeded the 250 ms local-hit budget"
        );
        let mut changed_context = request.clone();
        changed_context.selected_context = Some("aqueous context selected by the learner".into());
        assert_ne!(
            dynamic_cache_path(&directory, &request, ClaimMode::Fast, &identities, &trusted)
                .expect("unqualified context path"),
            dynamic_cache_path(
                &directory,
                &changed_context,
                ClaimMode::Fast,
                &identities,
                &trusted
            )
            .expect("selected context path")
        );

        let path = dynamic_cache_path(&directory, &request, ClaimMode::Fast, &identities, &trusted)
            .expect("path");
        fs::write(&path, br#"{"schema_version":2}"#).expect("corrupt old entry");
        assert!(
            load_dynamic_cache(
                Some(&directory),
                &request,
                ClaimMode::Fast,
                &identities,
                &trusted
            )
            .is_none()
        );
        assert!(path.exists(), "a rejected entry must not be deleted");
        fs::remove_dir_all(directory).expect("cleanup");
    }

    #[test]
    fn formula_only_reactants_keep_order_independent_cache_binding() {
        let trusted = trusted();
        let identities = reviewed_species_registry(&trusted).expect("identities");
        let methane = ReactantInput {
            display: "CH4".into(),
            atomic_numbers: vec![6, 1, 1, 1, 1],
            species_id: None,
        };
        let oxygen = ReactantInput {
            display: "O2".into(),
            atomic_numbers: vec![8, 8],
            species_id: None,
        };
        let first = ReactionBuildRequest {
            reactants: vec![methane.clone(), oxygen.clone()],
            selected_context: None,
        };
        let swapped = ReactionBuildRequest {
            reactants: vec![oxygen, methane],
            selected_context: None,
        };
        let directory = std::path::Path::new("/unused/cache-binding-test");
        assert_eq!(
            dynamic_cache_path(directory, &first, ClaimMode::Fast, &identities, &trusted).unwrap(),
            dynamic_cache_path(directory, &swapped, ClaimMode::Fast, &identities, &trusted)
                .unwrap()
        );
    }

    #[test]
    fn retryable_static_presentation_reuses_claim_but_retries_enrichment() {
        let trusted = trusted();
        let identities = reviewed_species_registry(&trusted).expect("identities");
        let request = request();
        let claim = claim();
        let directory = std::env::temp_dir().join(format!(
            "chemspec-cache-retry-test-{}-{}",
            std::process::id(),
            CACHE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        store_dynamic_cache(
            &directory,
            &request,
            ClaimMode::Fast,
            &identities,
            &trusted,
            &claim,
            Some(DynamicCachePresentation::Static {
                diagnostic: "temporary provider failure".to_owned(),
                retryable: true,
            }),
            "fake",
            "offline",
        )
        .expect("store retryable presentation");

        let loaded = load_dynamic_cache(
            Some(&directory),
            &request,
            ClaimMode::Fast,
            &identities,
            &trusted,
        )
        .expect("reuse the validated claim");
        assert!(matches!(loaded.outcome, CompiledClaimOutcome::Static(_)));
        assert!(
            loaded.presentation.is_none(),
            "a retryable failure must relaunch presentation enrichment"
        );
        fs::remove_dir_all(directory).expect("cleanup");
    }

    #[test]
    fn cache_rejects_a_digest_consistent_claim_outside_wire_limits() {
        let trusted = trusted();
        let identities = reviewed_species_registry(&trusted).expect("identities");
        let request = request();
        let directory = std::env::temp_dir().join(format!(
            "chemspec-cache-wire-limit-test-{}-{}",
            std::process::id(),
            CACHE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        store_dynamic_cache(
            &directory,
            &request,
            ClaimMode::Fast,
            &identities,
            &trusted,
            &claim(),
            None,
            "fake",
            "offline",
        )
        .expect("store valid cache entry");
        let path = dynamic_cache_path(&directory, &request, ClaimMode::Fast, &identities, &trusted)
            .expect("cache path");
        let mut envelope: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).expect("cache bytes")).expect("cache entry");
        envelope["claim"]["products"][0]["name"] = json!("x".repeat(301));
        let claim_bytes = serde_json::to_vec(&envelope["claim"]).expect("claim bytes");
        envelope["claim_digest"] =
            serde_json::to_value(ContentDigest::sha256(&claim_bytes)).expect("updated digest");
        fs::write(
            &path,
            serde_json::to_vec(&envelope).expect("tampered cache entry"),
        )
        .expect("write tampered entry");

        assert!(
            load_dynamic_cache(
                Some(&directory),
                &request,
                ClaimMode::Fast,
                &identities,
                &trusted,
            )
            .is_none(),
            "a matching digest must not confer schema validity"
        );
        assert!(
            path.exists(),
            "a rejected entry remains a cache miss artifact"
        );
        fs::remove_dir_all(directory).expect("cleanup");
    }

    #[test]
    fn v3_cache_rejects_provider_injected_solver_reason_without_shape_drift() {
        let trusted = trusted();
        let identities = reviewed_species_registry(&trusted).expect("identities");
        let request = request();
        let directory = std::env::temp_dir().join(format!(
            "chemspec-cache-provenance-test-{}-{}",
            std::process::id(),
            CACHE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        store_dynamic_cache(
            &directory,
            &request,
            ClaimMode::Fast,
            &identities,
            &trusted,
            &claim(),
            None,
            "fake",
            "offline",
        )
        .expect("store v3 provider claim");
        let path = dynamic_cache_path(&directory, &request, ClaimMode::Fast, &identities, &trusted)
            .expect("cache path");
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).expect("cache bytes")).expect("cache JSON");
        assert_eq!(value["schema_version"], json!(3));
        assert_eq!(value["compiler_contract_version"], json!(6));
        assert!(value["claim"].get("origin").is_none());
        assert!(value["claim"].get("solver_reason").is_none());

        value["claim"]["no_reaction_reason"] = json!({"below_hydrogen":{"metal":"copper"}});
        let hostile = serde_json::to_vec(&value).expect("hostile cache JSON");
        let error = serde_json::from_slice::<DynamicCacheEnvelope>(&hostile)
            .expect_err("cache decoding must reject solver-only provider fields");
        assert!(error.to_string().contains("no_reaction_reason"));
        fs::write(&path, hostile).expect("write hostile cache entry");
        assert!(
            load_dynamic_cache(
                Some(&directory),
                &request,
                ClaimMode::Fast,
                &identities,
                &trusted,
            )
            .is_none()
        );
        fs::remove_dir_all(directory).expect("cleanup");
    }
}
