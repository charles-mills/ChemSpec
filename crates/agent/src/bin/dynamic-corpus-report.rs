use std::{collections::BTreeMap, env, fs, process::ExitCode};

use agent::{
    BenchmarkClass, CorpusManifest, CorpusMetrics, CorpusObservation, FailureClassification,
};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
struct ProviderModel {
    provider: String,
    model: String,
    provider_version: String,
}

#[derive(Debug, Serialize)]
struct LatencyBudget {
    local_hit_samples: usize,
    local_hit_p95_ms: Option<u64>,
    local_hit_target_met: Option<bool>,
    fast_samples: usize,
    fast_p50_ms: Option<u64>,
    fast_p95_ms: Option<u64>,
    fast_target_met: Option<bool>,
    researcher_samples: usize,
    researcher_p50_ms: Option<u64>,
    researcher_p95_ms: Option<u64>,
    researcher_target_met: Option<bool>,
    escalated_samples: usize,
    escalated_p50_ms: Option<u64>,
    escalated_p95_ms: Option<u64>,
    escalated_target_met: Option<bool>,
}

#[derive(Debug, Serialize)]
struct ModelReport {
    identity: ProviderModel,
    metrics: CorpusMetrics,
    latency: LatencyBudget,
}

#[derive(Debug, Serialize)]
struct BenchmarkReport {
    corpus_version: String,
    reports: Vec<ModelReport>,
    selected_default: Option<ProviderModel>,
    selection_note: String,
}

fn main() -> ExitCode {
    match run() {
        Ok(report) => match serde_json::to_string_pretty(&report) {
            Ok(json) => {
                println!("{json}");
                ExitCode::SUCCESS
            }
            Err(error) => fail(&error.to_string()),
        },
        Err(error) => fail(&error),
    }
}

fn fail(message: &str) -> ExitCode {
    eprintln!("dynamic corpus report: {message}");
    ExitCode::FAILURE
}

fn run() -> Result<BenchmarkReport, String> {
    let mut arguments = env::args_os().skip(1);
    let manifest_path = arguments.next().ok_or_else(usage)?;
    let observations_path = arguments.next().ok_or_else(usage)?;
    if arguments.next().is_some() {
        return Err(usage());
    }
    let manifest =
        CorpusManifest::from_json(&fs::read(&manifest_path).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
    let observations: Vec<CorpusObservation> =
        serde_json::from_slice(&fs::read(&observations_path).map_err(|error| error.to_string())?)
            .map_err(|error| format!("invalid observation JSON: {error}"))?;
    build_report(manifest, observations)
}

fn build_report(
    manifest: CorpusManifest,
    observations: Vec<CorpusObservation>,
) -> Result<BenchmarkReport, String> {
    let mut grouped = BTreeMap::<ProviderModel, Vec<CorpusObservation>>::new();
    for observation in observations {
        grouped
            .entry(ProviderModel {
                provider: observation.provider.clone(),
                model: observation.model.clone(),
                provider_version: observation.provider_version.clone(),
            })
            .or_default()
            .push(observation);
    }
    let mut reports = Vec::new();
    for (identity, observations) in grouped {
        let metrics = manifest
            .evaluate(&observations)
            .map_err(|error| format!("{}: {error}", identity.model))?;
        let latency = latency_budget(&observations);
        reports.push(ModelReport {
            identity,
            metrics,
            latency,
        });
    }
    reports.sort_by(|left, right| {
        right
            .metrics
            .expected_state_matches
            .cmp(&left.metrics.expected_state_matches)
            .then_with(|| {
                left.latency
                    .fast_p95_ms
                    .unwrap_or(u64::MAX)
                    .cmp(&right.latency.fast_p95_ms.unwrap_or(u64::MAX))
            })
            .then_with(|| left.identity.cmp(&right.identity))
    });
    let selected_default = reports
        .iter()
        .find(|report| eligible_for_default(report))
        .map(|report| report.identity.clone());
    let selection_note = selected_default.as_ref().map_or_else(
        || {
            "No default selected: each candidate needs complete independently reviewed corpus observations, at least 25 Fast cold samples, exact state matches, and the Fast latency budget.".into()
        },
        |identity| {
            format!(
                "Selected {} from measured valid-result accuracy, then Fast cold p95 latency; low reasoning and default service tier remain mandatory.",
                identity.model
            )
        },
    );
    Ok(BenchmarkReport {
        corpus_version: manifest.corpus_version,
        reports,
        selected_default,
        selection_note,
    })
}

fn usage() -> String {
    "usage: dynamic-corpus-report <manifest.json> <observations.json>".into()
}

fn eligible_for_default(report: &ModelReport) -> bool {
    report.metrics.unreviewed_oracle_cases == 0
        && report.metrics.expected_state_matches == report.metrics.total
        && report.metrics.presentation_matches == report.metrics.total
        && report.latency.fast_samples >= 25
        && report.latency.fast_target_met == Some(true)
        && report
            .metrics
            .failure_counts
            .iter()
            .all(|(failure, count)| *failure == FailureClassification::None || *count == 0)
}

fn latency_budget(observations: &[CorpusObservation]) -> LatencyBudget {
    let mut local = Vec::new();
    let mut fast = Vec::new();
    let mut researcher = Vec::new();
    let mut escalated = Vec::new();
    for observation in observations {
        match observation.benchmark_class {
            BenchmarkClass::LocalHit => {
                local.extend(observation.latency.static_outcome_ms);
            }
            BenchmarkClass::FastCold => {
                fast.extend(observation.latency.static_outcome_ms);
            }
            BenchmarkClass::ResearcherCold => {
                researcher.extend(
                    observation
                        .latency
                        .evidence_ms
                        .or(observation.latency.static_outcome_ms),
                );
            }
            BenchmarkClass::EscalatedMechanism => {
                escalated.extend(observation.latency.mechanism_ms);
            }
        }
    }
    let (local_p50, local_p95) = percentiles(&mut local);
    let (fast_p50, fast_p95) = percentiles(&mut fast);
    let (researcher_p50, researcher_p95) = percentiles(&mut researcher);
    let (escalated_p50, escalated_p95) = percentiles(&mut escalated);
    LatencyBudget {
        local_hit_samples: local.len(),
        local_hit_p95_ms: local_p95,
        local_hit_target_met: local_p50.map(|_| local_p95.is_some_and(|value| value <= 250)),
        fast_samples: fast.len(),
        fast_p50_ms: fast_p50,
        fast_p95_ms: fast_p95,
        fast_target_met: fast_p50
            .map(|p50| p50 <= 15_000 && fast_p95.is_some_and(|p95| p95 <= 30_000)),
        researcher_samples: researcher.len(),
        researcher_p50_ms: researcher_p50,
        researcher_p95_ms: researcher_p95,
        researcher_target_met: researcher_p50
            .map(|p50| p50 <= 30_000 && researcher_p95.is_some_and(|p95| p95 <= 60_000)),
        escalated_samples: escalated.len(),
        escalated_p50_ms: escalated_p50,
        escalated_p95_ms: escalated_p95,
        escalated_target_met: escalated_p50
            .map(|p50| p50 <= 60_000 && escalated_p95.is_some_and(|p95| p95 <= 100_000)),
    }
}

fn percentiles(values: &mut [u64]) -> (Option<u64>, Option<u64>) {
    if values.is_empty() {
        return (None, None);
    }
    values.sort_unstable();
    let at = |percent: usize| {
        let index = (values.len() * percent).div_ceil(100).saturating_sub(1);
        values[index]
    };
    (Some(at(50)), Some(at(95)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent::{CorpusExpectedState, CorpusTrustTier, LatencyMilestones};

    fn perfect_observations(manifest: &CorpusManifest) -> Vec<CorpusObservation> {
        manifest
            .cases
            .iter()
            .map(|case| {
                let observed_state = case.expected_state;
                CorpusObservation {
                    case_id: case.id.clone(),
                    provider: "codex_subscription".into(),
                    model: "candidate-model".into(),
                    provider_version: "codex-test 1.0".into(),
                    benchmark_class: BenchmarkClass::FastCold,
                    observed_state,
                    trust_tier: match observed_state {
                        CorpusExpectedState::Reviewed => CorpusTrustTier::Reviewed,
                        CorpusExpectedState::EvidenceBacked => CorpusTrustTier::EvidenceBacked,
                        CorpusExpectedState::ModelAsserted => CorpusTrustTier::ModelAsserted,
                        CorpusExpectedState::Ambiguous
                        | CorpusExpectedState::Unsupported
                        | CorpusExpectedState::Invalid => CorpusTrustTier::None,
                    },
                    presentation: case.expected_presentation,
                    identity_pass: true,
                    balance_pass: true,
                    evidence_coverage_pass: Some(true),
                    mapping_pass: Some(true),
                    failure: FailureClassification::None,
                    latency: LatencyMilestones {
                        claim_ms: Some(2_000),
                        evidence_ms: None,
                        static_outcome_ms: Some(3_000),
                        mechanism_ms: None,
                        reviewed_animation_ms: None,
                    },
                }
            })
            .collect()
    }

    #[test]
    fn nearest_rank_percentiles_are_deterministic() {
        let mut values = (1..=20).collect::<Vec<_>>();
        assert_eq!(percentiles(&mut values), (Some(10), Some(19)));
    }

    #[test]
    fn unreviewed_oracles_cannot_select_a_release_default() {
        let manifest = CorpusManifest::from_json(include_bytes!(
            "../../../../corpus/dynamic-reactions-v1.json"
        ))
        .expect("corpus");
        let observations = perfect_observations(&manifest);
        let report = build_report(manifest, observations).expect("report");
        assert!(report.selected_default.is_none());
        assert!(report.selection_note.contains("independently reviewed"));
        assert_eq!(report.reports[0].latency.fast_target_met, Some(true));
    }

    #[test]
    fn reviewed_complete_candidate_can_be_selected_from_measured_results() {
        let mut manifest = CorpusManifest::from_json(include_bytes!(
            "../../../../corpus/dynamic-reactions-v1.json"
        ))
        .expect("corpus");
        for scenario in &mut manifest.scenarios {
            scenario.oracle_reviewed_by = Some("independent-reviewer".into());
        }
        let observations = perfect_observations(&manifest);
        let report = build_report(manifest, observations).expect("report");
        assert_eq!(
            report
                .selected_default
                .as_ref()
                .map(|value| value.model.as_str()),
            Some("candidate-model")
        );
    }
}
