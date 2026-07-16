//! Explicit 25-case live escalation benchmark.
//!
//! Run once per eligible model with, for example:
//! `CHEMSPEC_CODEX_MODEL=<slug> cargo test -p agent --test live_corpus -- --ignored --nocapture --test-threads=1`.
//! During prompt repair, `CHEMSPEC_LIVE_CASE_IDS` may contain a comma-separated
//! subset; release measurements must leave it unset and run all 25 cases.
//! It consumes the signed-in Codex subscription but never browses or retrieves
//! evidence: every claim uses Fast mode, then crosses only local identity,
//! balance, structure, family, kernel, and frame gates. Every case is allowed
//! to finish so the report contains a real first-try and after-repairs rate
//! instead of stopping at the first failure.

use std::{collections::BTreeMap, time::Instant};

use agent::{
    ClaimMode, CodexProvider, CodexProviderConfig, CompiledClaimOutcome, CorpusCase,
    CorpusExpectedState, CorpusManifest, CorpusPresentation, CorpusScenario,
    DynamicPresentationOutcome, FailureClassification, LatencyMilestones, OutcomeSpecies,
    ReactantInput, ReactionBuildRequest, RequestIdentityResolution, TrustTier,
    compile_claim_outcome, enrich_static_outcome, resolve_request_identities_with_catalogue,
    reviewed_species_registry,
};
use chem_catalogue::TrustedCatalogue;
use chem_domain::FormulaComposition;
use serde::Serialize;

#[derive(Debug, Serialize)]
struct LiveCaseRecord {
    case_id: String,
    expected_state: CorpusExpectedState,
    expected_presentation: CorpusPresentation,
    observed_state: CorpusExpectedState,
    presentation: CorpusPresentation,
    first_try: Option<bool>,
    structure_repairs: Option<usize>,
    mechanism_repairs: Option<usize>,
    frame_count: Option<usize>,
    failure: FailureClassification,
    diagnostic: Option<String>,
    latency: LatencyMilestones,
}

#[derive(Debug, Serialize)]
struct EscalationRates {
    selected_cases: usize,
    expected_escalations: usize,
    kernel_successes: usize,
    first_try_successes: usize,
    after_repair_successes: usize,
}

#[derive(Debug, Serialize)]
struct LiveSmokeReport {
    provider: &'static str,
    model: String,
    provider_version: String,
    claim_mode: &'static str,
    rates: EscalationRates,
    cases: Vec<LiveCaseRecord>,
}

fn trusted() -> TrustedCatalogue {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    TrustedCatalogue::from_canonical_json(
        &std::fs::read(root.join("catalogue/trusted/core-chemistry/catalogue.json"))
            .expect("trusted catalogue"),
        &std::fs::read(root.join("catalogue/trusted/core-chemistry/review.json"))
            .expect("trusted review"),
    )
    .expect("trusted catalogue attestation")
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn live_request(
    scenario: &CorpusScenario,
    case: &CorpusCase,
    catalogue: &TrustedCatalogue,
) -> Result<ReactionBuildRequest, String> {
    let atomic_numbers = catalogue
        .document()
        .elements
        .iter()
        .filter_map(|element| {
            u8::try_from(element.atomic_number)
                .ok()
                .map(|number| (element.symbol.to_string(), number))
        })
        .collect::<BTreeMap<_, _>>();
    let parts = scenario.reactants[1].split(" + ").collect::<Vec<_>>();
    if !(1..=2).contains(&parts.len()) {
        return Err(format!(
            "symbolic request must have one or two reactants: {}",
            scenario.reactants[1]
        ));
    }
    let selected_context = match (parts.len(), scenario.category.as_str()) {
        (1, "photochemical") => Some("light".to_owned()),
        (1, "electrochemical") => Some("electricity".to_owned()),
        // Two-reactant corpus wording remains visible to the model, including
        // the selected adversarial cases. It is context, never chemistry.
        (2, _) => Some(case.request.clone()),
        _ => {
            return Err(format!(
                "single-reactant scenario `{}` has no closed energy context",
                scenario.id
            ));
        }
    };
    let reactants = parts
        .into_iter()
        .map(|display| {
            let formula = FormulaComposition::parse(display)
                .map_err(|error| format!("cannot parse `{display}`: {error}"))?;
            let mut atoms = Vec::new();
            for (symbol, count) in formula.elements() {
                let number = atomic_numbers
                    .get(&symbol.to_string())
                    .copied()
                    .ok_or_else(|| format!("unknown element `{symbol}`"))?;
                atoms.extend(std::iter::repeat_n(
                    number,
                    usize::try_from(*count).map_err(|_| "atom count overflow".to_owned())?,
                ));
            }
            Ok(ReactantInput {
                display: display.to_owned(),
                atomic_numbers: atoms,
                species_id: None,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(ReactionBuildRequest {
        reactants,
        selected_context,
    })
}

fn state_for_trust(trust: TrustTier) -> CorpusExpectedState {
    match trust {
        TrustTier::Reviewed => CorpusExpectedState::Reviewed,
        TrustTier::EvidenceBacked => CorpusExpectedState::EvidenceBacked,
        TrustTier::ModelAsserted => CorpusExpectedState::ModelAsserted,
    }
}

fn failure_record(
    case: &CorpusCase,
    observed_state: CorpusExpectedState,
    failure: FailureClassification,
    diagnostic: impl Into<String>,
    latency: LatencyMilestones,
) -> LiveCaseRecord {
    LiveCaseRecord {
        case_id: case.id.clone(),
        expected_state: case.expected_state,
        expected_presentation: case.expected_presentation,
        observed_state,
        presentation: CorpusPresentation::None,
        first_try: None,
        structure_repairs: None,
        mechanism_repairs: None,
        frame_count: None,
        failure,
        diagnostic: Some(diagnostic.into()),
        latency,
    }
}

fn claim_failure(error: &agent::AgentError) -> FailureClassification {
    let diagnostic = error.to_string().to_ascii_lowercase();
    if diagnostic.contains("deadline") || diagnostic.contains("timed out") {
        FailureClassification::Timeout
    } else if error.stage().to_ascii_lowercase().contains("claim") {
        FailureClassification::Claim
    } else {
        FailureClassification::Provider
    }
}

#[allow(clippy::too_many_lines)]
fn run_live_case(
    case: &CorpusCase,
    scenario: &CorpusScenario,
    catalogue: &TrustedCatalogue,
    config: &CodexProviderConfig,
) -> LiveCaseRecord {
    let mut latency = LatencyMilestones::default();
    let mut request = match live_request(scenario, case, catalogue) {
        Ok(request) => request,
        Err(error) => {
            return failure_record(
                case,
                CorpusExpectedState::Invalid,
                FailureClassification::Identity,
                error,
                latency,
            );
        }
    };
    let identities = match reviewed_species_registry(catalogue) {
        Ok(identities) => identities,
        Err(error) => {
            return failure_record(
                case,
                CorpusExpectedState::Invalid,
                FailureClassification::Identity,
                error.to_string(),
                latency,
            );
        }
    };
    match resolve_request_identities_with_catalogue(&request, &identities, catalogue) {
        Ok(RequestIdentityResolution::Resolved(resolved)) => {
            for (input, species) in request.reactants.iter_mut().zip(resolved) {
                if let OutcomeSpecies::Resolved(species) = species {
                    input.species_id = Some(species.id);
                }
            }
        }
        Ok(RequestIdentityResolution::Ambiguous(_)) => {
            return LiveCaseRecord {
                case_id: case.id.clone(),
                expected_state: case.expected_state,
                expected_presentation: case.expected_presentation,
                observed_state: CorpusExpectedState::Ambiguous,
                presentation: CorpusPresentation::None,
                first_try: None,
                structure_repairs: None,
                mechanism_repairs: None,
                frame_count: None,
                failure: FailureClassification::None,
                diagnostic: None,
                latency,
            };
        }
        Err(error) => {
            return failure_record(
                case,
                CorpusExpectedState::Invalid,
                FailureClassification::Identity,
                error.to_string(),
                latency,
            );
        }
    }

    let mut provider = CodexProvider::new(config.clone());
    let started = Instant::now();
    let claim = match provider.claim_reaction(&request, ClaimMode::Fast) {
        Ok(claim) => claim,
        Err(error) => {
            latency.claim_ms = Some(elapsed_ms(started));
            return failure_record(
                case,
                CorpusExpectedState::Invalid,
                claim_failure(&error),
                error.to_string(),
                latency,
            );
        }
    };
    latency.claim_ms = Some(elapsed_ms(started));
    let compiled = match compile_claim_outcome(&request, claim, &identities) {
        Ok(compiled) => compiled,
        Err(error) => {
            latency.static_outcome_ms = Some(elapsed_ms(started));
            let failure = if error.stage() == "outcome balance" {
                FailureClassification::Balance
            } else {
                FailureClassification::Claim
            };
            return failure_record(
                case,
                CorpusExpectedState::Invalid,
                failure,
                error.to_string(),
                latency,
            );
        }
    };
    latency.static_outcome_ms = Some(elapsed_ms(started));
    let outcome = match compiled {
        CompiledClaimOutcome::Static(outcome) => outcome,
        CompiledClaimOutcome::NoReaction(_) => {
            return LiveCaseRecord {
                case_id: case.id.clone(),
                expected_state: case.expected_state,
                expected_presentation: case.expected_presentation,
                observed_state: CorpusExpectedState::ModelAsserted,
                presentation: CorpusPresentation::None,
                first_try: None,
                structure_repairs: None,
                mechanism_repairs: None,
                frame_count: None,
                failure: FailureClassification::None,
                diagnostic: None,
                latency,
            };
        }
        CompiledClaimOutcome::Ambiguous(_) => {
            return LiveCaseRecord {
                case_id: case.id.clone(),
                expected_state: case.expected_state,
                expected_presentation: case.expected_presentation,
                observed_state: CorpusExpectedState::Ambiguous,
                presentation: CorpusPresentation::None,
                first_try: None,
                structure_repairs: None,
                mechanism_repairs: None,
                frame_count: None,
                failure: FailureClassification::None,
                diagnostic: None,
                latency,
            };
        }
        CompiledClaimOutcome::Unsupported(_) => {
            return LiveCaseRecord {
                case_id: case.id.clone(),
                expected_state: case.expected_state,
                expected_presentation: case.expected_presentation,
                observed_state: CorpusExpectedState::Unsupported,
                presentation: CorpusPresentation::None,
                first_try: None,
                structure_repairs: None,
                mechanism_repairs: None,
                frame_count: None,
                failure: FailureClassification::None,
                diagnostic: None,
                latency,
            };
        }
    };
    let presentation_started = Instant::now();
    let presentation = match enrich_static_outcome(outcome, catalogue, &mut provider) {
        Ok(presentation) => presentation,
        Err(error) => {
            return failure_record(
                case,
                CorpusExpectedState::Invalid,
                FailureClassification::Presentation,
                error.to_string(),
                latency,
            );
        }
    };
    match presentation {
        DynamicPresentationOutcome::ReviewedFamily(animation) => {
            latency.reviewed_animation_ms = Some(elapsed_ms(presentation_started));
            LiveCaseRecord {
                case_id: case.id.clone(),
                expected_state: case.expected_state,
                expected_presentation: case.expected_presentation,
                observed_state: state_for_trust(animation.static_outcome().trust_tier()),
                presentation: CorpusPresentation::ReviewedFamily,
                first_try: None,
                structure_repairs: None,
                mechanism_repairs: None,
                frame_count: Some(animation.frames().frames().len()),
                failure: FailureClassification::None,
                diagnostic: None,
                latency,
            }
        }
        DynamicPresentationOutcome::Escalated(animation) => {
            latency.mechanism_ms = Some(elapsed_ms(presentation_started));
            LiveCaseRecord {
                case_id: case.id.clone(),
                expected_state: case.expected_state,
                expected_presentation: case.expected_presentation,
                observed_state: state_for_trust(animation.static_outcome().trust_tier()),
                presentation: CorpusPresentation::EscalatedMechanism,
                first_try: Some(animation.total_repair_count() == 0),
                structure_repairs: Some(animation.structure_repair_count()),
                mechanism_repairs: Some(animation.repair_count()),
                frame_count: Some(animation.frames().frames().len()),
                failure: FailureClassification::None,
                diagnostic: None,
                latency,
            }
        }
        DynamicPresentationOutcome::Static {
            outcome,
            mut diagnostic,
            retryable,
            ..
        } => {
            if std::env::var_os("CHEMSPEC_LIVE_DEBUG_RESPONSES").is_some() {
                if let Some(response) = provider.take_last_structure_response() {
                    diagnostic.push_str("\nlast structure response: ");
                    diagnostic.push_str(
                        &serde_json::to_string(&response).expect("serialize structure response"),
                    );
                }
                if let Some(response) = provider.take_last_mechanism_response() {
                    diagnostic.push_str("\nlast mechanism response: ");
                    diagnostic.push_str(
                        &serde_json::to_string(&response).expect("serialize mechanism response"),
                    );
                }
            }
            latency.mechanism_ms = Some(elapsed_ms(presentation_started));
            LiveCaseRecord {
                case_id: case.id.clone(),
                expected_state: case.expected_state,
                expected_presentation: case.expected_presentation,
                observed_state: state_for_trust(outcome.trust_tier()),
                presentation: if retryable {
                    CorpusPresentation::MechanismUnavailableStatic
                } else {
                    CorpusPresentation::Static
                },
                first_try: Some(false),
                structure_repairs: None,
                mechanism_repairs: None,
                frame_count: None,
                failure: FailureClassification::Presentation,
                diagnostic: Some(diagnostic),
                latency,
            }
        }
    }
}

#[test]
fn live_selection_request_shapes_include_context_only_cases() {
    let manifest =
        CorpusManifest::from_json(include_bytes!("../../../corpus/dynamic-reactions-v1.json"))
            .expect("valid corpus");
    let catalogue = trusted();
    assert_eq!(manifest.live_smoke_case_ids.len(), 25);
    for (case_id, expected_context) in [
        ("photochemical-001", "light"),
        ("electrochemical-001", "electricity"),
    ] {
        let case = manifest
            .cases
            .iter()
            .find(|case| case.id == case_id)
            .expect("selected context case");
        let scenario = manifest
            .scenarios
            .iter()
            .find(|scenario| scenario.id == case.scenario_id)
            .expect("context scenario");
        let request = live_request(scenario, case, &catalogue).expect("context request");
        assert_eq!(request.reactants.len(), 1);
        assert_eq!(request.selected_context.as_deref(), Some(expected_context));
    }
}

#[test]
#[ignore = "25-case live Fast-claim/kernel benchmark; consumes Codex subscription"]
fn representative_live_escalation_smoke() {
    let manifest =
        CorpusManifest::from_json(include_bytes!("../../../corpus/dynamic-reactions-v1.json"))
            .expect("valid corpus");
    let catalogue = trusted();
    let config = CodexProviderConfig::from_environment();
    let probe = CodexProvider::new(config.clone());
    let preflight = probe.preflight().expect("live Codex preflight");
    assert!(preflight.authenticated, "Codex must be signed in");

    let selected_case_ids = std::env::var("CHEMSPEC_LIVE_CASE_IDS").map_or_else(
        |_| manifest.live_smoke_case_ids.clone(),
        |value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|case_id| !case_id.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        },
    );
    assert!(
        !selected_case_ids.is_empty(),
        "CHEMSPEC_LIVE_CASE_IDS selected no cases"
    );
    let mut cases = Vec::new();
    for case_id in &selected_case_ids {
        let case = manifest
            .cases
            .iter()
            .find(|case| &case.id == case_id)
            .expect("selected case");
        let scenario = manifest
            .scenarios
            .iter()
            .find(|scenario| scenario.id == case.scenario_id)
            .expect("selected scenario");
        let record = run_live_case(case, scenario, &catalogue, &config);
        eprintln!(
            "{}: {:?} / {:?}{}",
            case.id,
            record.observed_state,
            record.presentation,
            record
                .diagnostic
                .as_deref()
                .map_or_else(String::new, |diagnostic| format!(" — {diagnostic}"))
        );
        cases.push(record);
    }
    let expected_escalations = cases
        .iter()
        .filter(|record| record.expected_presentation == CorpusPresentation::EscalatedMechanism)
        .count();
    let kernel_successes = cases
        .iter()
        .filter(|record| {
            record.expected_presentation == CorpusPresentation::EscalatedMechanism
                && record.presentation == CorpusPresentation::EscalatedMechanism
        })
        .count();
    let first_try_successes = cases
        .iter()
        .filter(|record| {
            record.expected_presentation == CorpusPresentation::EscalatedMechanism
                && record.first_try == Some(true)
        })
        .count();
    let provider_failures = cases
        .iter()
        .filter(|record| record.failure == FailureClassification::Provider)
        .count();
    let report = LiveSmokeReport {
        provider: "codex_subscription",
        model: probe.model_name().to_owned(),
        provider_version: preflight.version,
        claim_mode: "fast",
        rates: EscalationRates {
            selected_cases: cases.len(),
            expected_escalations,
            kernel_successes,
            first_try_successes,
            after_repair_successes: kernel_successes.saturating_sub(first_try_successes),
        },
        cases,
    };
    let report_json = serde_json::to_string_pretty(&report).expect("live smoke report");
    println!("{report_json}");
    if let Some(path) = std::env::var_os("CHEMSPEC_LIVE_REPORT_PATH") {
        std::fs::write(&path, format!("{report_json}\n")).unwrap_or_else(|error| {
            panic!(
                "write live benchmark report to {}: {error}",
                std::path::Path::new(&path).display()
            )
        });
    }
    assert_eq!(
        provider_failures, 0,
        "live benchmark could not measure the model because {provider_failures} selected cases failed at the provider boundary"
    );
}
