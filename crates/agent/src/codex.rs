use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};

use crate::{
    AgentError, ReactionBuildRequest, ValidatedDynamicReaction, validate_provider_artifact,
};

const RESULT_SCHEMA: &str = r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object",
  "additionalProperties": false,
  "required": [
    "schema_version",
    "source_name",
    "source",
    "catalogue_document_json",
    "evidence_json"
  ],
  "properties": {
    "schema_version": { "type": "integer", "const": 1 },
    "source_name": { "type": "string", "minLength": 1 },
    "source": { "type": "string", "minLength": 1 },
    "catalogue_document_json": { "type": "string", "minLength": 2 },
    "evidence_json": { "type": "string", "minLength": 2 }
  }
}"#;

const PROMPT_TEMPLATE: &str = include_str!("../prompts/dynamic-reaction.md");
const CHEMS_SPECIFICATION: &str = include_str!("../../../docs/chems-specification.md");
const CHEMS_GRAMMAR: &str = include_str!("../../../grammar/chems.ebnf");
const CATALOGUE_SCHEMA: &str = include_str!("../../../schemas/chem-catalogue-1.schema.json");
const EVIDENCE_SCHEMA: &str = include_str!("../../../schemas/chem-evidence-packet-1.schema.json");
const REFERENCE_CATALOGUE: &str =
    include_str!("../../../conformance/catalogue/lithium-rule-001.catalogue.json");
const REFERENCE_EVIDENCE: &str =
    include_str!("../../../conformance/observations/lithium-observations-001.input.json");
const REFERENCE_SOURCE: &str =
    include_str!("../../../conformance/parsing/canonical-source-001.chems");

static RUN_SEQUENCE: AtomicU64 = AtomicU64::new(1);

/// Runtime configuration for the Codex subscription provider.
#[derive(Debug, Clone)]
pub struct CodexProviderConfig {
    pub executable: PathBuf,
    pub model: Option<String>,
}

impl CodexProviderConfig {
    #[must_use]
    pub fn from_environment() -> Self {
        Self {
            executable: PathBuf::from("codex"),
            model: std::env::var("CHEMSPEC_CODEX_MODEL").ok(),
        }
    }
}

/// Capability evidence returned without reading Codex credential files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexPreflight {
    pub version: String,
    pub authenticated: bool,
}

/// Real Codex CLI provider. It runs ephemerally, in a read-only sandbox, with
/// live web search enabled and a strict outer result schema.
#[derive(Debug, Clone)]
pub struct CodexProvider {
    config: CodexProviderConfig,
}

impl CodexProvider {
    #[must_use]
    pub const fn new(config: CodexProviderConfig) -> Self {
        Self { config }
    }

    /// Verifies the installed CLI, login state, and every flag used by the
    /// non-interactive invocation. Credential files are never opened.
    ///
    /// # Errors
    ///
    /// Returns a preflight error when Codex cannot run, is missing a required
    /// capability, or its status output cannot be read.
    pub fn preflight(&self) -> Result<CodexPreflight, AgentError> {
        let version = command_text(&self.config.executable, ["--version"])?;
        let authenticated = Command::new(&self.config.executable)
            .args(["login", "status"])
            .output()
            .map_err(|error| AgentError::new("Codex preflight", error.to_string()))?
            .status
            .success();
        let top_help = command_text(&self.config.executable, ["--help"])?;
        if !top_help.contains("--search") {
            return Err(AgentError::new(
                "Codex preflight",
                "installed Codex does not expose live web search",
            ));
        }
        let exec_help = command_text(&self.config.executable, ["exec", "--help"])?;
        for capability in [
            "--output-schema",
            "--sandbox",
            "--ephemeral",
            "--ignore-user-config",
            "--ignore-rules",
            "--skip-git-repo-check",
            "--output-last-message",
        ] {
            if !exec_help.contains(capability) {
                return Err(AgentError::new(
                    "Codex preflight",
                    format!("installed Codex is missing `{capability}`"),
                ));
            }
        }
        Ok(CodexPreflight {
            version: version.trim().to_owned(),
            authenticated,
        })
    }

    /// Researches, authors, and deterministically validates one dynamic
    /// reaction. Provider success alone never produces this return type.
    ///
    /// # Errors
    ///
    /// Returns a typed stage error for preflight, invocation, structured
    /// output, catalogue, evidence, source, kernel, or frame failure.
    pub fn build_reaction(
        &self,
        request: &ReactionBuildRequest,
    ) -> Result<ValidatedDynamicReaction, AgentError> {
        let preflight = self.preflight()?;
        if !preflight.authenticated {
            return Err(AgentError::new(
                "Codex preflight",
                "Codex is installed but not authenticated",
            ));
        }
        let temporary = TemporaryRun::create()?;
        let schema_path = temporary.path.join("result.schema.json");
        let result_path = temporary.path.join("result.json");
        fs::write(&schema_path, RESULT_SCHEMA)
            .map_err(|error| AgentError::new("Codex run setup", error.to_string()))?;

        let mut command = Command::new(&self.config.executable);
        command
            .arg("--search")
            .arg("exec")
            .arg("--json")
            .arg("--output-schema")
            .arg(&schema_path)
            .arg("--output-last-message")
            .arg(&result_path)
            .arg("--sandbox")
            .arg("read-only")
            .arg("--ephemeral")
            .arg("--ignore-user-config")
            .arg("--ignore-rules")
            .arg("--skip-git-repo-check")
            .arg("-C")
            .arg(&temporary.path);
        if let Some(model) = &self.config.model {
            command.arg("--model").arg(model);
        }
        let mut child = command
            .arg("-")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| AgentError::new("Codex invocation", error.to_string()))?;
        let prompt = build_prompt(request)?;
        if let Some(mut stdin) = child.stdin.take()
            && let Err(error) = stdin.write_all(prompt.as_bytes())
        {
            let _ = child.kill();
            let _ = child.wait();
            return Err(AgentError::new("Codex invocation", error.to_string()));
        }
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| AgentError::new("Codex invocation", "failed to capture Codex stderr"))?;
        let stderr_reader = std::thread::spawn(move || drain_bounded(stderr, 2_000));
        let status = child
            .wait()
            .map_err(|error| AgentError::new("Codex invocation", error.to_string()))?;
        let stderr = stderr_reader
            .join()
            .map_err(|_| AgentError::new("Codex invocation", "stderr reader failed"))?
            .map_err(|error| AgentError::new("Codex invocation", error.to_string()))?;
        if !status.success() {
            return Err(AgentError::new(
                "Codex invocation",
                String::from_utf8_lossy(&stderr).into_owned(),
            ));
        }
        let artifact = read_bounded(&result_path, 8 * 1024 * 1024)?;
        validate_provider_artifact(
            &artifact,
            "codex_subscription",
            self.config.model.as_deref().unwrap_or("codex_default"),
        )
    }
}

fn drain_bounded(mut reader: impl Read, limit: usize) -> std::io::Result<Vec<u8>> {
    let mut kept = Vec::with_capacity(limit);
    let mut buffer = [0_u8; 8 * 1024];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        let remaining = limit.saturating_sub(kept.len());
        kept.extend_from_slice(&buffer[..count.min(remaining)]);
    }
    Ok(kept)
}

fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, AgentError> {
    let length = fs::metadata(path)
        .map_err(|error| AgentError::new("Codex result", error.to_string()))?
        .len();
    if length > limit {
        return Err(AgentError::new(
            "Codex result",
            format!("result exceeded the {limit}-byte limit"),
        ));
    }
    fs::read(path).map_err(|error| AgentError::new("Codex result", error.to_string()))
}

fn command_text<const N: usize>(
    executable: &Path,
    arguments: [&str; N],
) -> Result<String, AgentError> {
    let output = Command::new(executable)
        .args(arguments)
        .output()
        .map_err(|error| AgentError::new("Codex preflight", error.to_string()))?;
    if !output.status.success() {
        return Err(AgentError::new(
            "Codex preflight",
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn build_prompt(request: &ReactionBuildRequest) -> Result<String, AgentError> {
    let request = serde_json::to_string_pretty(request)
        .map_err(|error| AgentError::new("Codex prompt", error.to_string()))?;
    let replacements = [
        ("{{REQUEST_JSON}}", request.as_str()),
        ("{{CHEMS_SPECIFICATION}}", CHEMS_SPECIFICATION),
        ("{{CHEMS_GRAMMAR}}", CHEMS_GRAMMAR),
        ("{{CATALOGUE_SCHEMA}}", CATALOGUE_SCHEMA),
        ("{{EVIDENCE_SCHEMA}}", EVIDENCE_SCHEMA),
        ("{{REFERENCE_CATALOGUE}}", REFERENCE_CATALOGUE),
        ("{{REFERENCE_EVIDENCE}}", REFERENCE_EVIDENCE),
        ("{{REFERENCE_SOURCE}}", REFERENCE_SOURCE),
    ];
    let mut prompt = PROMPT_TEMPLATE.to_owned();
    for (placeholder, value) in replacements {
        prompt = prompt.replace(placeholder, value);
    }
    if prompt.contains("{{") {
        return Err(AgentError::new(
            "Codex prompt",
            "runtime prompt contains an unresolved placeholder",
        ));
    }
    Ok(prompt)
}

struct TemporaryRun {
    path: PathBuf,
}

impl TemporaryRun {
    fn create() -> Result<Self, AgentError> {
        let sequence = RUN_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("chemspec-codex-{}-{sequence}", std::process::id()));
        fs::create_dir(&path)
            .map_err(|error| AgentError::new("Codex run setup", error.to_string()))?;
        Ok(Self { path })
    }
}

impl Drop for TemporaryRun {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn result_schema_is_valid_json() {
        let schema: serde_json::Value = serde_json::from_str(RESULT_SCHEMA).expect("schema");
        assert_eq!(schema["properties"]["schema_version"]["const"], json!(1));
    }

    #[test]
    fn prompt_contains_request_and_safety_boundary() {
        let request = ReactionBuildRequest {
            reactants: [
                crate::ReactantInput {
                    display: "Ca".to_owned(),
                    atomic_numbers: vec![20],
                },
                crate::ReactantInput {
                    display: "H2O".to_owned(),
                    atomic_numbers: vec![1, 1, 8],
                },
            ],
        };
        let prompt = build_prompt(&request).expect("prompt");
        assert!(prompt.contains("\"Ca\""));
        assert!(prompt.contains("not a laboratory procedure"));
        assert!(prompt.contains("reaction-declaration"));
        assert!(prompt.contains("chem-catalogue-1.json"));
        assert!(prompt.contains("reaction LithiumAndWater where"));
        assert!(!prompt.contains("{{"));
        assert!(prompt.contains("cannot inspect the application's source repository"));
        assert!(!prompt.contains("Read these governing files"));
    }
}
