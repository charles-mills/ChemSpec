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
    AgentError, ClaimMode, CompiledClaimOutcome, DynamicPresentationOutcome, EvidenceSnapshot,
    FamilyMatchOutcome, MechanismEscalationResponse, ReactionBuildRequest, ReactionClaim,
    StructureProposalResponse, TrustTier, compile_claim_outcome, compile_reviewed_animation,
    match_reviewed_family, resolve_request_species, restore_evidence_backed,
    validate_escalated_response_with_structures,
};

pub const DYNAMIC_CACHE_SCHEMA_VERSION: u32 = 3;
const DYNAMIC_COMPILER_CONTRACT_VERSION: u32 = 2;
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
    pub evidence: Option<EvidenceSnapshot>,
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
    claim: ReactionClaim,
    trust_tier: TrustTier,
    evidence: Option<EvidenceSnapshot>,
    presentation: Option<DynamicCachePresentation>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DynamicPreferences {
    schema_version: u32,
    claim_mode: ClaimMode,
}

#[must_use]
pub fn load_claim_mode(directory: Option<&Path>) -> ClaimMode {
    let Some(directory) = directory else {
        return ClaimMode::Fast;
    };
    fs::read(directory.join("preferences-v1.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<DynamicPreferences>(&bytes).ok())
        .filter(|preferences| preferences.schema_version == 1)
        .map_or(ClaimMode::Fast, |preferences| preferences.claim_mode)
}

/// Persists the user-selected claim mode independently of chemistry cache
/// entries.
///
/// # Errors
///
/// Returns a directory, serialization, or atomic-write error.
pub fn store_claim_mode(directory: &Path, mode: ClaimMode) -> Result<(), AgentError> {
    fs::create_dir_all(directory)
        .map_err(|error| AgentError::new("dynamic preferences", error.to_string()))?;
    let bytes = serde_json::to_vec(&DynamicPreferences {
        schema_version: 1,
        claim_mode: mode,
    })
    .map_err(|error| AgentError::new("dynamic preferences", error.to_string()))?;
    let temporary = directory.join(format!(
        ".preferences-{}-{}.tmp",
        std::process::id(),
        CACHE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&temporary, bytes)
        .map_err(|error| AgentError::new("dynamic preferences", error.to_string()))?;
    atomic_replace(
        &temporary,
        &directory.join("preferences-v1.json"),
        "dynamic preferences",
    )
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
    let identity_snapshot = identities
        .snapshot_digest()
        .map_err(|error| AgentError::new("reaction cache", error.to_string()))?;
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
    .map_err(|error| AgentError::new("reaction cache", error.to_string()))?;
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
        evidence,
        presentation: cached_presentation,
        ..
    } = cached;
    let mut outcome = compile_claim_outcome(request, claim, identities).ok()?;
    match (&mut outcome, trust_tier, &evidence) {
        (
            CompiledClaimOutcome::Static(static_outcome),
            TrustTier::EvidenceBacked,
            Some(snapshot),
        ) => {
            *static_outcome = restore_evidence_backed(static_outcome.clone(), snapshot).ok()?;
        }
        (_, TrustTier::ModelAsserted, None) => {}
        _ => return None,
    }
    let presentation = match (&outcome, cached_presentation) {
        (CompiledClaimOutcome::Static(static_outcome), Some(recipe)) => {
            Some(revalidate_presentation(static_outcome.clone(), recipe, catalogue).ok()?)
        }
        (_, None) => None,
        (_, Some(_)) => return None,
    };
    Some(LoadedDynamicCache {
        outcome,
        presentation,
        evidence,
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
    claim: &ReactionClaim,
    evidence: Option<&EvidenceSnapshot>,
    presentation: Option<DynamicCachePresentation>,
    provider: &str,
    model: &str,
) -> Result<(), AgentError> {
    // Recompile before persistence so a provider response alone can never
    // populate the cache.
    let compiled = compile_claim_outcome(request, claim.clone(), identities)?;
    let trust_tier = match (&compiled, evidence) {
        (CompiledClaimOutcome::Static(outcome), Some(snapshot)) => {
            restore_evidence_backed(outcome.clone(), snapshot)
                .map_err(|error| AgentError::new("reaction cache", error.to_string()))?;
            TrustTier::EvidenceBacked
        }
        (_, None) => TrustTier::ModelAsserted,
        (_, Some(_)) => {
            return Err(AgentError::new(
                "reaction cache",
                "evidence snapshot requires a static outcome",
            ));
        }
    };
    let path = dynamic_cache_path(directory, request, mode, identities, catalogue)?;
    let envelope = DynamicCacheEnvelope {
        schema_version: DYNAMIC_CACHE_SCHEMA_VERSION,
        compiler_contract_version: DYNAMIC_COMPILER_CONTRACT_VERSION,
        provider: provider.to_owned(),
        model: model.to_owned(),
        mode,
        request_binding: request_binding(request, identities)?,
        identity_snapshot: identities
            .snapshot_digest()
            .map_err(|error| AgentError::new("reaction cache", error.to_string()))?,
        catalogue_digest: catalogue.digest(),
        claim_digest: claim_digest(claim)?,
        claim: claim.clone(),
        trust_tier,
        evidence: evidence.cloned(),
        presentation,
    };
    let bytes = serde_json::to_vec(&envelope)
        .map_err(|error| AgentError::new("reaction cache", error.to_string()))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_CACHE_BYTES {
        return Err(AgentError::new(
            "reaction cache",
            "entry exceeds size limit",
        ));
    }
    fs::create_dir_all(directory)
        .map_err(|error| AgentError::new("reaction cache", error.to_string()))?;
    let sequence = CACHE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = directory.join(format!(".dynamic-v3-{}-{sequence}.tmp", std::process::id()));
    fs::write(&temporary, bytes)
        .map_err(|error| AgentError::new("reaction cache", error.to_string()))?;
    atomic_replace(&temporary, &path, "reaction cache")
}

/// Transactionally upgrades an existing entry with a fetched evidence
/// snapshot while preserving its validated presentation recipe.
///
/// # Errors
///
/// Returns an error if the prior entry is absent/corrupt or if the upgraded
/// claim and snapshot fail the normal v3 store validation.
#[allow(clippy::too_many_arguments)]
pub fn upgrade_dynamic_cache_evidence(
    directory: &Path,
    request: &ReactionBuildRequest,
    mode: ClaimMode,
    identities: &SpeciesRegistry,
    catalogue: &TrustedCatalogue,
    claim: &ReactionClaim,
    evidence: &EvidenceSnapshot,
    provider: &str,
    model: &str,
) -> Result<(), AgentError> {
    let path = dynamic_cache_path(directory, request, mode, identities, catalogue)?;
    let bytes =
        fs::read(&path).map_err(|error| AgentError::new("reaction cache", error.to_string()))?;
    let existing: DynamicCacheEnvelope = serde_json::from_slice(&bytes)
        .map_err(|error| AgentError::new("reaction cache", error.to_string()))?;
    if existing.schema_version != DYNAMIC_CACHE_SCHEMA_VERSION
        || existing.compiler_contract_version != DYNAMIC_COMPILER_CONTRACT_VERSION
        || existing.request_binding != request_binding(request, identities)?
        || existing.identity_snapshot
            != identities
                .snapshot_digest()
                .map_err(|error| AgentError::new("reaction cache", error.to_string()))?
        || existing.catalogue_digest != catalogue.digest()
    {
        return Err(AgentError::new(
            "reaction cache",
            "existing entry is stale or bound to different governing inputs",
        ));
    }
    store_dynamic_cache(
        directory,
        request,
        mode,
        identities,
        catalogue,
        claim,
        Some(evidence),
        existing.presentation,
        provider,
        model,
    )
}

#[cfg(not(target_os = "windows"))]
fn atomic_replace(
    temporary: &Path,
    destination: &Path,
    stage: &'static str,
) -> Result<(), AgentError> {
    fs::rename(temporary, destination).map_err(|error| AgentError::new(stage, error.to_string()))
}

#[cfg(target_os = "windows")]
fn atomic_replace(
    temporary: &Path,
    destination: &Path,
    stage: &'static str,
) -> Result<(), AgentError> {
    if !destination.exists() {
        return fs::rename(temporary, destination)
            .map_err(|error| AgentError::new(stage, error.to_string()));
    }
    let backup = destination.with_extension(format!(
        "backup-{}",
        CACHE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::rename(destination, &backup).map_err(|error| AgentError::new(stage, error.to_string()))?;
    if let Err(error) = fs::rename(temporary, destination) {
        let _ = fs::rename(&backup, destination);
        return Err(AgentError::new(stage, error.to_string()));
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
                    "reaction cache",
                    "cached reviewed family is no longer uniquely applicable",
                ));
            };
            if family.rule_id() != &rule_id {
                return Err(AgentError::new(
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
        .map(|species| species.id)
        .collect::<Vec<_>>();
    ids.sort();
    let selected_context = request.selected_context.as_deref().map(|context| {
        context
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    });
    let bytes = serde_json::to_vec(&(ids, selected_context))
        .map_err(|error| AgentError::new("reaction cache", error.to_string()))?;
    Ok(ContentDigest::sha256(&bytes))
}

fn claim_digest(claim: &ReactionClaim) -> Result<ContentDigest, AgentError> {
    let bytes = serde_json::to_vec(claim)
        .map_err(|error| AgentError::new("reaction cache", error.to_string()))?;
    Ok(ContentDigest::sha256(&bytes))
}

#[cfg(test)]
mod tests {
    use chem_catalogue::TrustedCatalogue;
    use serde_json::json;

    use super::*;
    use crate::{ReactantInput, reviewed_species_registry};

    fn trusted() -> TrustedCatalogue {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        TrustedCatalogue::from_canonical_json(
            &fs::read(root.join("catalogue/trusted/core-chemistry/catalogue.json"))
                .expect("catalogue"),
            &fs::read(root.join("catalogue/trusted/core-chemistry/review.json")).expect("review"),
        )
        .expect("trusted catalogue")
    }

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
            ],
            selected_context: None,
        }
    }

    fn claim() -> ReactionClaim {
        let value = json!({
            "schema_version": 1,
            "disposition": "reaction",
            "products": [
                {"name":"lithium hydroxide","formula":"LiOH","phase":"aqueous","identity_hints":[]},
                {"name":"hydrogen","formula":"H2","phase":"gas","identity_hints":[]}
            ],
            "required_context":"representative educational outcome under the reviewed standard-outcome premise",
            "observations":[], "sources":[], "ambiguity":null
        });
        ReactionClaim::from_json(
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
        assert!(matches!(loaded.outcome, CompiledClaimOutcome::Static(_)));
        assert!(loaded.presentation.is_none());
        let CompiledClaimOutcome::Static(static_outcome) =
            compile_claim_outcome(&request, claim.clone(), &identities).expect("compiled")
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
            None,
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
        assert_ne!(
            dynamic_cache_path(&directory, &request, ClaimMode::Fast, &identities, &trusted)
                .expect("fast path"),
            dynamic_cache_path(
                &directory,
                &request,
                ClaimMode::Researcher,
                &identities,
                &trusted
            )
            .expect("researcher path")
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
        assert_eq!(load_claim_mode(Some(&directory)), ClaimMode::Fast);
        store_claim_mode(&directory, ClaimMode::Researcher).expect("store preference");
        assert_eq!(load_claim_mode(Some(&directory)), ClaimMode::Researcher);
        fs::remove_dir_all(directory).expect("cleanup");
    }
}
