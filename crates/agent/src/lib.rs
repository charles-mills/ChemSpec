//! Provider-neutral dynamic reaction construction for `ChemSpec`.
//!
//! Providers return source, a working catalogue document, and evidence. This
//! crate treats all three as untrusted input and only exposes renderer-readable
//! dynamic frames after catalogue, language, evidence, and kernel validation.

mod codex;

use std::fmt;

use chem_catalogue::{
    CatalogueDocument, CatalogueEnvelope, PublicationKind, ValidatedCatalogueBundle,
};
use chem_domain::ContentDigest;
use chem_kernel::{
    EvidencePacket, ValidatedDynamicFrames, expand_review_candidate,
    inspect_review_candidate_frames, validate_review_candidate,
};
use chems_lang::{SourceEquationTerm, parse_source};
use serde::{Deserialize, Serialize};

pub use codex::{CodexPreflight, CodexProvider, CodexProviderConfig};

/// One structured reactant as composed by the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReactantInput {
    pub display: String,
    pub atomic_numbers: Vec<u8>,
}

/// Provider-neutral request for a reaction absent from the local fast path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReactionBuildRequest {
    pub reactants: [ReactantInput; 2],
}

/// Provider identity retained separately from chemistry source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DynamicReactionProvenance {
    pub provider: String,
    pub model: String,
    pub source_name: String,
    pub source_digest: ContentDigest,
    pub catalogue_digest: ContentDigest,
    pub evidence_digest: ContentDigest,
}

/// A dynamic reaction that crossed every deterministic chemistry boundary.
#[derive(Debug, Clone)]
pub struct ValidatedDynamicReaction {
    frames: ValidatedDynamicFrames,
    equation: String,
    reaction_name: String,
    provenance: DynamicReactionProvenance,
}

impl ValidatedDynamicReaction {
    #[must_use]
    pub const fn frames(&self) -> &ValidatedDynamicFrames {
        &self.frames
    }

    #[must_use]
    pub fn equation(&self) -> &str {
        &self.equation
    }

    #[must_use]
    pub fn reaction_name(&self) -> &str {
        &self.reaction_name
    }

    #[must_use]
    pub const fn provenance(&self) -> &DynamicReactionProvenance {
        &self.provenance
    }
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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentBuildArtifact {
    schema_version: u32,
    source_name: String,
    source: String,
    catalogue_document_json: String,
    evidence_json: String,
}

pub(crate) fn validate_provider_artifact(
    bytes: &[u8],
    provider: &str,
    model: &str,
) -> Result<ValidatedDynamicReaction, AgentError> {
    let artifact: AgentBuildArtifact = serde_json::from_slice(bytes)
        .map_err(|error| AgentError::new("provider output", error.to_string()))?;
    if artifact.schema_version != 1 {
        return Err(AgentError::new(
            "provider output",
            format!("unsupported artifact schema {}", artifact.schema_version),
        ));
    }
    if artifact.source_name.trim().is_empty() || artifact.source.trim().is_empty() {
        return Err(AgentError::new(
            "provider output",
            "source name and source must be non-empty",
        ));
    }

    let document: CatalogueDocument = serde_json::from_str(&artifact.catalogue_document_json)
        .map_err(|error| AgentError::new("working catalogue", error.to_string()))?;
    if document.publication != PublicationKind::Working {
        return Err(AgentError::new(
            "working catalogue",
            "dynamic catalogue publication must be `working`",
        ));
    }
    let mut envelope = CatalogueEnvelope {
        digest: ContentDigest::sha256(b"uncomputed dynamic catalogue"),
        bundle: document,
    };
    envelope.digest = envelope
        .computed_digest()
        .map_err(|error| AgentError::new("working catalogue", error.to_string()))?;
    let catalogue = ValidatedCatalogueBundle::validate(envelope)
        .map_err(|error| AgentError::new("working catalogue", error.to_string()))?;

    let evidence: EvidencePacket = serde_json::from_str(&artifact.evidence_json)
        .map_err(|error| AgentError::new("evidence", error.to_string()))?;
    let evidence_bytes = serde_json::to_vec(&evidence)
        .map_err(|error| AgentError::new("evidence", error.to_string()))?;
    let expanded = expand_review_candidate(
        &artifact.source_name,
        &artifact.source,
        &catalogue,
        &evidence_bytes,
    )
    .map_err(|error| AgentError::new("source expansion", error.to_string()))?;
    let equation = source_equation(&artifact.source).ok_or_else(|| {
        AgentError::new(
            "source expansion",
            "validated source did not retain a display equation",
        )
    })?;
    let reaction_name = expanded.claim.reaction.clone();
    let provenance = DynamicReactionProvenance {
        provider: provider.to_owned(),
        model: model.to_owned(),
        source_name: expanded.claim.source.name.clone(),
        source_digest: expanded.claim.source.bytes_digest,
        catalogue_digest: expanded.claim.catalogue.digest,
        evidence_digest: expanded.claim.evidence.digest,
    };
    let derivation = validate_review_candidate(&expanded, &catalogue)
        .map_err(|error| AgentError::new("chemistry validation", error.to_string()))?;
    let frames = inspect_review_candidate_frames(&derivation)
        .map_err(|error| AgentError::new("frame projection", error.to_string()))?
        .into_validated_dynamic();

    Ok(ValidatedDynamicReaction {
        frames,
        equation,
        reaction_name,
        provenance,
    })
}

fn source_equation(source: &str) -> Option<String> {
    let parsed = parse_source(source);
    let equation = parsed.ast.reaction?.equation?;
    let side = |terms: &[SourceEquationTerm]| {
        terms
            .iter()
            .map(format_equation_term)
            .collect::<Vec<_>>()
            .join(" + ")
    };
    Some(format!(
        "{} → {}",
        side(&equation.reactants),
        side(&equation.products)
    ))
}

fn format_equation_term(term: &SourceEquationTerm) -> String {
    term.coefficient.as_ref().map_or_else(
        || term.formula.clone(),
        |value| format!("{value} {}", term.formula),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_root() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn provider_artifact_crosses_dynamic_validation_boundary() {
        let root = fixture_root();
        let envelope: CatalogueEnvelope = serde_json::from_slice(
            &std::fs::read(
                root.join("conformance/catalogue/alkali-metal-water-001.catalogue.json"),
            )
            .expect("catalogue fixture"),
        )
        .expect("catalogue envelope");
        let artifact = serde_json::json!({
            "schema_version": 1,
            "source_name": "AgentLithiumAndWater.chems",
            "source": std::fs::read_to_string(
                root.join("conformance/end-to-end/alkali-water-li-001.chems")
            ).expect("source fixture"),
            "catalogue_document_json": serde_json::to_string(&envelope.bundle)
                .expect("catalogue document"),
            "evidence_json": std::fs::read_to_string(
                root.join("conformance/observations/alkali-water-li-001.evidence.json")
            ).expect("evidence fixture"),
        });

        let reaction = validate_provider_artifact(
            &serde_json::to_vec(&artifact).expect("artifact"),
            "fake",
            "fixture",
        )
        .expect("dynamic reaction validates");

        assert_eq!(
            reaction.frames().trust(),
            chem_kernel::DerivationTrust::ReviewCandidate
        );
        assert!(!reaction.frames().frames().is_empty());
        assert_eq!(reaction.provenance().provider, "fake");
        assert!(reaction.equation().contains('→'));
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn working_catalogue_can_validate_a_member_absent_from_the_fast_path() {
        let root = fixture_root();
        let mut envelope: serde_json::Value = serde_json::from_slice(
            &std::fs::read(
                root.join("conformance/catalogue/alkali-metal-water-001.catalogue.json"),
            )
            .expect("catalogue fixture"),
        )
        .expect("catalogue envelope");
        let bundle = envelope.get_mut("bundle").expect("catalogue bundle object");
        bundle["publication"] = serde_json::json!("working");
        let elements = bundle["elements"].as_array_mut().expect("elements");
        let mut rubidium = elements
            .iter()
            .find(|element| element["symbol"] == "K")
            .cloned()
            .expect("potassium element");
        rubidium["symbol"] = serde_json::json!("Rb");
        rubidium["name"] = serde_json::json!("Rubidium");
        rubidium["atomic_number"] = serde_json::json!(37);
        rubidium["period"] = serde_json::json!(5);
        elements.push(rubidium);
        let valence = bundle["valence_premises"]
            .as_array_mut()
            .and_then(|premises| premises.first_mut())
            .expect("valence premise");
        for collection in [
            "neutral_valence",
            "supported_states",
            "metallic_domain_states",
        ] {
            let records = valence[collection].as_array_mut().expect("valence records");
            let rubidium_records = records
                .iter()
                .filter(|record| record["element"] == "K")
                .cloned()
                .map(|mut record| {
                    record["element"] = serde_json::json!("Rb");
                    record
                })
                .collect::<Vec<_>>();
            records.extend(rubidium_records);
        }
        let categories = bundle["element_categories"]
            .as_array_mut()
            .expect("categories");
        let alkali = categories
            .iter_mut()
            .find(|category| category["id"] == "Categories.AlkaliMetal")
            .expect("alkali category");
        alkali["membership"]["members"]
            .as_array_mut()
            .expect("explicit members")
            .push(serde_json::json!("Rb"));

        let applications = bundle["structure_applications"]
            .as_array_mut()
            .expect("structure applications");
        for (existing, id, formula) in [
            ("PotassiumMetal", "RubidiumMetal", "Rb"),
            ("PotassiumHydroxide", "RubidiumHydroxide", "RbOH"),
        ] {
            let mut application = applications
                .iter()
                .find(|application| application["id"] == existing)
                .cloned()
                .expect("template application");
            application["id"] = serde_json::json!(id);
            application["arguments"]["member"] = serde_json::json!("Rb");
            application["formula"] = serde_json::json!(formula);
            applications.push(application);
        }

        let source =
            std::fs::read_to_string(root.join("conformance/end-to-end/alkali-water-li-001.chems"))
                .expect("source fixture")
                .replace("Lithium", "Rubidium")
                .replace("lithium", "rubidium")
                .replace("LiOH", "RbOH")
                .replace("2 Li[", "2 Rb[");
        let evidence = std::fs::read_to_string(
            root.join("conformance/observations/alkali-water-li-001.evidence.json"),
        )
        .expect("evidence fixture")
        .replace("AlkaliWaterLithium", "AlkaliWaterRubidium");
        let artifact = serde_json::json!({
            "schema_version": 1,
            "source_name": "AgentRubidiumAndWater.chems",
            "source": source,
            "catalogue_document_json": serde_json::to_string(bundle)
                .expect("catalogue document"),
            "evidence_json": evidence,
        });

        let reaction = validate_provider_artifact(
            &serde_json::to_vec(&artifact).expect("artifact"),
            "fake",
            "fixture",
        )
        .expect("dynamic rubidium reaction validates");

        assert_eq!(reaction.reaction_name(), "RubidiumAndWater");
        assert_eq!(reaction.equation(), "2 Rb + 2 H2O → 2 RbOH + H2");
        assert_eq!(
            reaction.frames().trust(),
            chem_kernel::DerivationTrust::ReviewCandidate
        );
    }
}
