use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

pub const CORPUS_SCHEMA_VERSION: u32 = 1;
const REQUIRED_CATEGORIES: [&str; 19] = [
    "inorganic",
    "organic",
    "ionic",
    "molecular",
    "metallic",
    "acid_base",
    "redox",
    "precipitation",
    "complexation",
    "combustion",
    "substitution",
    "elimination",
    "addition",
    "biochemical",
    "photochemical",
    "electrochemical",
    "no_reaction",
    "conditional",
    "ambiguous",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusManifest {
    pub schema_version: u32,
    pub corpus_version: String,
    pub scenarios: Vec<CorpusScenario>,
    pub cases: Vec<CorpusCase>,
    pub live_smoke_case_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusScenario {
    pub id: String,
    pub category: String,
    pub reactants: [String; 2],
    pub identity_oracle: [String; 2],
    pub outcome_oracle: String,
    pub balance_oracle: Vec<u32>,
    pub evidence_expectation: String,
    pub expected_state: CorpusExpectedState,
    pub presentation: CorpusPresentation,
    /// Must name a reviewer independent of the implementation before this
    /// scenario can contribute to a release-accuracy claim.
    pub oracle_reviewed_by: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusCase {
    pub id: String,
    pub scenario_id: String,
    pub request_context: String,
    pub adversarial_mutation: Option<String>,
    pub request: String,
    pub expected_state: CorpusExpectedState,
    pub expected_presentation: CorpusPresentation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CorpusExpectedState {
    Reviewed,
    EvidenceBacked,
    ModelAsserted,
    Ambiguous,
    Unsupported,
    Invalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CorpusPresentation {
    ReviewedFamily,
    EscalatedMechanism,
    MechanismUnavailableStatic,
    Static,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CorpusTrustTier {
    Reviewed,
    EvidenceBacked,
    ModelAsserted,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BenchmarkClass {
    LocalHit,
    FastCold,
    ResearcherCold,
    EscalatedMechanism,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureClassification {
    None,
    Identity,
    Claim,
    Evidence,
    Balance,
    Mapping,
    Presentation,
    Timeout,
    Cancelled,
    Provider,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LatencyMilestones {
    pub claim_ms: Option<u64>,
    pub evidence_ms: Option<u64>,
    pub static_outcome_ms: Option<u64>,
    pub mechanism_ms: Option<u64>,
    pub reviewed_animation_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusObservation {
    pub case_id: String,
    pub provider: String,
    pub model: String,
    pub provider_version: String,
    pub benchmark_class: BenchmarkClass,
    pub observed_state: CorpusExpectedState,
    pub trust_tier: CorpusTrustTier,
    pub presentation: CorpusPresentation,
    pub identity_pass: bool,
    pub balance_pass: bool,
    pub evidence_coverage_pass: Option<bool>,
    pub mapping_pass: Option<bool>,
    pub failure: FailureClassification,
    pub latency: LatencyMilestones,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusMetrics {
    pub corpus_version: String,
    pub provider_versions: BTreeSet<String>,
    pub total: usize,
    pub expected_state_matches: usize,
    pub identity_passes: usize,
    pub balance_passes: usize,
    pub evidence_coverage_passes: usize,
    pub mapping_passes: usize,
    pub presentation_matches: usize,
    pub failure_counts: BTreeMap<FailureClassification, usize>,
    pub model_asserted_matches: usize,
    pub evidence_backed_matches: usize,
    pub unreviewed_oracle_cases: usize,
    pub claim_latency_p50_ms: Option<u64>,
    pub claim_latency_p95_ms: Option<u64>,
    pub static_latency_p50_ms: Option<u64>,
    pub static_latency_p95_ms: Option<u64>,
    pub mechanism_latency_p50_ms: Option<u64>,
    pub mechanism_latency_p95_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CorpusError {
    InvalidJson(String),
    InvalidManifest(String),
    ObservationMismatch(String),
}

impl std::fmt::Display for CorpusError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidJson(message) => write!(formatter, "invalid corpus JSON: {message}"),
            Self::InvalidManifest(message) => write!(formatter, "invalid corpus: {message}"),
            Self::ObservationMismatch(message) => {
                write!(formatter, "invalid corpus observations: {message}")
            }
        }
    }
}

impl std::error::Error for CorpusError {}

impl CorpusManifest {
    /// Decodes and validates the versioned breadth-corpus contract.
    ///
    /// # Errors
    ///
    /// Rejects schema drift, missing categories, fewer than 250 explicit
    /// requests, duplicate IDs, absent oracles, and an invalid live-smoke set.
    pub fn from_json(bytes: &[u8]) -> Result<Self, CorpusError> {
        let manifest: Self = serde_json::from_slice(bytes)
            .map_err(|error| CorpusError::InvalidJson(error.to_string()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    fn validate(&self) -> Result<(), CorpusError> {
        if self.schema_version != CORPUS_SCHEMA_VERSION {
            return Err(CorpusError::InvalidManifest(format!(
                "unsupported schema {}",
                self.schema_version
            )));
        }
        if self.cases.len() < 250 {
            return Err(CorpusError::InvalidManifest(
                "fewer than 250 explicit requests".into(),
            ));
        }
        let scenarios = self
            .scenarios
            .iter()
            .map(|scenario| (scenario.id.as_str(), scenario))
            .collect::<BTreeMap<_, _>>();
        if scenarios.len() != self.scenarios.len() {
            return Err(CorpusError::InvalidManifest(
                "duplicate scenario identity".into(),
            ));
        }
        let categories = self
            .scenarios
            .iter()
            .map(|scenario| scenario.category.as_str())
            .collect::<BTreeSet<_>>();
        for category in REQUIRED_CATEGORIES {
            if !categories.contains(category) {
                return Err(CorpusError::InvalidManifest(format!(
                    "missing category `{category}`"
                )));
            }
        }
        for scenario in &self.scenarios {
            if scenario
                .reactants
                .iter()
                .any(|value| value.trim().is_empty())
                || scenario
                    .identity_oracle
                    .iter()
                    .any(|value| value.trim().is_empty())
                || scenario.outcome_oracle.trim().is_empty()
                || scenario.balance_oracle.is_empty()
                || scenario.evidence_expectation.trim().is_empty()
            {
                return Err(CorpusError::InvalidManifest(format!(
                    "scenario `{}` has an incomplete independent oracle",
                    scenario.id
                )));
            }
        }
        let mut case_ids = BTreeSet::new();
        let mut requests = BTreeSet::new();
        for case in &self.cases {
            if !case_ids.insert(case.id.as_str())
                || !scenarios.contains_key(case.scenario_id.as_str())
            {
                return Err(CorpusError::InvalidManifest(format!(
                    "case `{}` is duplicate or references an absent scenario",
                    case.id
                )));
            }
            if case.request.trim().is_empty()
                || case.request_context.trim().is_empty()
                || !requests.insert(case.request.as_str())
            {
                return Err(CorpusError::InvalidManifest(format!(
                    "case `{}` has an empty or duplicate explicit request",
                    case.id
                )));
            }
        }
        if !self
            .cases
            .iter()
            .any(|case| case.adversarial_mutation.is_some())
        {
            return Err(CorpusError::InvalidManifest(
                "no explicit adversarial mutations".into(),
            ));
        }
        self.validate_live_selection(&scenarios, &case_ids)
    }

    fn validate_live_selection(
        &self,
        scenarios: &BTreeMap<&str, &CorpusScenario>,
        case_ids: &BTreeSet<&str>,
    ) -> Result<(), CorpusError> {
        if self.live_smoke_case_ids.len() < 25
            || self
                .live_smoke_case_ids
                .iter()
                .any(|id| !case_ids.contains(id.as_str()))
        {
            return Err(CorpusError::InvalidManifest(
                "live-smoke selection must name at least 25 corpus cases".into(),
            ));
        }
        let live_categories = self
            .live_smoke_case_ids
            .iter()
            .filter_map(|id| self.cases.iter().find(|case| &case.id == id))
            .filter_map(|case| scenarios.get(case.scenario_id.as_str()))
            .map(|scenario| scenario.category.as_str())
            .collect::<BTreeSet<_>>();
        if REQUIRED_CATEGORIES
            .iter()
            .any(|category| !live_categories.contains(category))
        {
            return Err(CorpusError::InvalidManifest(
                "live-smoke selection does not span every required category".into(),
            ));
        }
        Ok(())
    }

    /// Computes dimension-separated metrics. A case cannot count as an
    /// expected-state match merely because its equation balanced.
    ///
    /// # Errors
    ///
    /// Requires exactly one observation for every corpus case and no extras.
    pub fn evaluate(
        &self,
        observations: &[CorpusObservation],
    ) -> Result<CorpusMetrics, CorpusError> {
        let observations = observations
            .iter()
            .map(|observation| (observation.case_id.as_str(), observation))
            .collect::<BTreeMap<_, _>>();
        if observations.len() != self.cases.len() {
            return Err(CorpusError::ObservationMismatch(
                "expected exactly one observation per case".into(),
            ));
        }
        let scenarios = self
            .scenarios
            .iter()
            .map(|scenario| (scenario.id.as_str(), scenario))
            .collect::<BTreeMap<_, _>>();
        let mut metrics = CorpusMetrics {
            corpus_version: self.corpus_version.clone(),
            provider_versions: BTreeSet::new(),
            total: self.cases.len(),
            expected_state_matches: 0,
            identity_passes: 0,
            balance_passes: 0,
            evidence_coverage_passes: 0,
            mapping_passes: 0,
            presentation_matches: 0,
            failure_counts: BTreeMap::new(),
            model_asserted_matches: 0,
            evidence_backed_matches: 0,
            unreviewed_oracle_cases: 0,
            claim_latency_p50_ms: None,
            claim_latency_p95_ms: None,
            static_latency_p50_ms: None,
            static_latency_p95_ms: None,
            mechanism_latency_p50_ms: None,
            mechanism_latency_p95_ms: None,
        };
        let mut claim_latencies = Vec::new();
        let mut static_latencies = Vec::new();
        let mut mechanism_latencies = Vec::new();
        for case in &self.cases {
            let observation = observations.get(case.id.as_str()).ok_or_else(|| {
                CorpusError::ObservationMismatch(format!("missing `{}`", case.id))
            })?;
            let scenario = scenarios[case.scenario_id.as_str()];
            metrics.provider_versions.insert(format!(
                "{}:{}:{}",
                observation.provider, observation.model, observation.provider_version
            ));
            if observation.observed_state == case.expected_state {
                metrics.expected_state_matches += 1;
                match observation.trust_tier {
                    CorpusTrustTier::ModelAsserted => metrics.model_asserted_matches += 1,
                    CorpusTrustTier::EvidenceBacked => metrics.evidence_backed_matches += 1,
                    CorpusTrustTier::Reviewed | CorpusTrustTier::None => {}
                }
            }
            metrics.identity_passes += usize::from(observation.identity_pass);
            metrics.balance_passes += usize::from(observation.balance_pass);
            metrics.evidence_coverage_passes +=
                usize::from(observation.evidence_coverage_pass == Some(true));
            metrics.mapping_passes += usize::from(observation.mapping_pass == Some(true));
            metrics.presentation_matches +=
                usize::from(observation.presentation == case.expected_presentation);
            *metrics
                .failure_counts
                .entry(observation.failure)
                .or_default() += 1;
            metrics.unreviewed_oracle_cases += usize::from(scenario.oracle_reviewed_by.is_none());
            claim_latencies.extend(observation.latency.claim_ms);
            static_latencies.extend(observation.latency.static_outcome_ms);
            mechanism_latencies.extend(observation.latency.mechanism_ms);
        }
        (metrics.claim_latency_p50_ms, metrics.claim_latency_p95_ms) =
            percentiles(&mut claim_latencies);
        (metrics.static_latency_p50_ms, metrics.static_latency_p95_ms) =
            percentiles(&mut static_latencies);
        (
            metrics.mechanism_latency_p50_ms,
            metrics.mechanism_latency_p95_ms,
        ) = percentiles(&mut mechanism_latencies);
        Ok(metrics)
    }
}

fn percentiles(values: &mut [u64]) -> (Option<u64>, Option<u64>) {
    if values.is_empty() {
        return (None, None);
    }
    values.sort_unstable();
    let percentile = |numerator: usize| {
        let rank = (values.len() * numerator).div_ceil(100).saturating_sub(1);
        values[rank]
    };
    (Some(percentile(50)), Some(percentile(95)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versioned_manifest_has_breadth_and_explicit_live_selection() {
        let manifest =
            CorpusManifest::from_json(include_bytes!("../../../corpus/dynamic-reactions-v1.json"))
                .expect("valid breadth corpus");
        assert!(manifest.cases.len() >= 250);
        assert!(manifest.live_smoke_case_ids.len() >= 25);
        assert_eq!(
            manifest
                .cases
                .iter()
                .map(|case| case.request.as_str())
                .collect::<BTreeSet<_>>()
                .len(),
            manifest.cases.len()
        );
        assert!(
            manifest
                .cases
                .iter()
                .any(|case| case.adversarial_mutation.is_some())
        );
    }

    #[test]
    fn balance_alone_cannot_count_as_a_successful_corpus_result() {
        let manifest =
            CorpusManifest::from_json(include_bytes!("../../../corpus/dynamic-reactions-v1.json"))
                .expect("valid breadth corpus");
        let observations = manifest
            .cases
            .iter()
            .map(|case| CorpusObservation {
                case_id: case.id.clone(),
                provider: "offline-fixture".into(),
                model: "none".into(),
                provider_version: "1".into(),
                benchmark_class: BenchmarkClass::LocalHit,
                observed_state: case.expected_state,
                trust_tier: CorpusTrustTier::None,
                presentation: case.expected_presentation,
                identity_pass: true,
                balance_pass: true,
                evidence_coverage_pass: None,
                mapping_pass: None,
                failure: FailureClassification::None,
                latency: LatencyMilestones::default(),
            })
            .collect::<Vec<_>>();
        let mut wrong = observations;
        wrong[0].observed_state = CorpusExpectedState::Unsupported;
        let metrics = manifest.evaluate(&wrong).expect("metrics");
        assert_eq!(metrics.balance_passes, manifest.cases.len());
        assert_eq!(metrics.expected_state_matches, manifest.cases.len() - 1);
        assert_eq!(metrics.unreviewed_oracle_cases, manifest.cases.len());
    }
}
