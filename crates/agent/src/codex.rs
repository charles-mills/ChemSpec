use std::{
    collections::BTreeMap,
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::Sender,
    },
    time::{Duration, Instant},
};

use crate::{
    AgentError, AgentErrorKind, ClaimMode, MechanismEscalationRequest, MechanismEscalationResponse,
    MechanismProvider, OxideAppearanceClaim, OxideAppearanceRequest, ProviderClaim,
    ReactionBuildRequest, StructureProposalRequest, StructureProposalResponse,
    ValidatedOxideAppearance,
};

const CLAIM_RESULT_SCHEMA: &str = r##"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object",
  "additionalProperties": false,
  "required": [
    "schema_version", "disposition", "reactant_phases", "products", "required_context",
    "observations", "sources", "ambiguity"
  ],
  "properties": {
    "schema_version": {"type": "integer", "const": 1},
    "disposition": {
      "type": "string",
      "enum": ["reaction", "no_reaction", "ambiguous", "unsupported"]
    },
    "reactant_phases": {
      "type": "array",
      "items": {
        "type": "string",
        "enum": ["aqueous", "solid", "liquid", "gas", "unknown"]
      },
      "minItems": 1,
      "maxItems": 2
    },
    "products": {
      "type": "array",
      "items": {"$ref": "#/$defs/product"},
      "maxItems": 16
    },
    "required_context": {"type": "string", "maxLength": 1000},
    "observations": {
      "type": "array",
      "items": {"$ref": "#/$defs/observation"},
      "maxItems": 16
    },
    "sources": {
      "type": "array",
      "items": {"$ref": "#/$defs/source"},
      "maxItems": 4
    },
    "ambiguity": {
      "anyOf": [{"type": "null"}, {"$ref": "#/$defs/ambiguity"}]
    }
  },
  "$defs": {
    "identity_hint": {
      "type": "object",
      "additionalProperties": false,
      "required": ["kind", "value"],
      "properties": {
        "kind": {
          "type": "string",
          "enum": [
            "inchi", "inchi_key", "canonical_smiles", "isomeric_smiles",
            "pub_chem_cid", "registry_id"
          ]
        },
        "value": {"type": "string", "minLength": 1, "maxLength": 500}
      }
    },
    "product": {
      "type": "object",
      "additionalProperties": false,
      "required": ["name", "formula", "phase", "identity_hints"],
      "properties": {
        "name": {"type": "string", "minLength": 1, "maxLength": 300},
        "formula": {"type": "string", "minLength": 1, "maxLength": 200},
        "phase": {
          "type": "string",
          "enum": ["aqueous", "solid", "liquid", "gas", "unknown"]
        },
        "identity_hints": {
          "type": "array",
          "items": {"$ref": "#/$defs/identity_hint"},
          "maxItems": 12
        }
      }
    },
    "observation": {
      "type": "object",
      "additionalProperties": false,
      "required": ["predicate", "subject", "value"],
      "properties": {
        "predicate": {
          "type": "string",
          "enum": ["evolves", "disappears", "forms", "colour"]
        },
        "subject": {"type": "string", "minLength": 1, "maxLength": 300},
        "value": {"type": ["string", "null"], "maxLength": 300}
      }
    },
    "source": {
      "type": "object",
      "additionalProperties": false,
      "required": [
        "id", "title", "publisher", "url", "supporting_excerpt", "supports"
      ],
      "properties": {
        "id": {"type": "string", "minLength": 1, "maxLength": 40},
        "title": {"type": "string", "minLength": 1, "maxLength": 500},
        "publisher": {"type": "string", "minLength": 1, "maxLength": 300},
        "url": {"type": "string", "pattern": "^https://", "maxLength": 2000},
        "supporting_excerpt": {"type": "string", "minLength": 1, "maxLength": 1200},
        "supports": {
          "type": "array",
          "items": {
            "type": "string",
            "enum": ["products", "required_context", "observations", "no_reaction"]
          },
          "minItems": 1,
          "maxItems": 4
        }
      }
    },
    "alternative": {
      "type": "object",
      "additionalProperties": false,
      "required": ["label", "products", "required_context"],
      "properties": {
        "label": {"type": "string", "minLength": 1, "maxLength": 300},
        "products": {
          "type": "array",
          "items": {"$ref": "#/$defs/product"},
          "maxItems": 16
        },
        "required_context": {"type": "string", "minLength": 1, "maxLength": 1000}
      }
    },
    "ambiguity": {
      "type": "object",
      "additionalProperties": false,
      "required": ["kind", "summary", "alternatives"],
      "properties": {
        "kind": {
          "type": "string",
          "enum": [
            "conditions", "reactant_identity", "multiple_outcomes", "conflicting_evidence"
          ]
        },
        "summary": {"type": "string", "minLength": 1, "maxLength": 1000},
        "alternatives": {
          "type": "array",
          "items": {"$ref": "#/$defs/alternative"},
          "minItems": 2,
          "maxItems": 8
        }
      }
    }
  }
}"##;

const MECHANISM_RESULT_SCHEMA: &str = include_str!("../schemas/mechanism-response.json");
const STRUCTURE_RESULT_SCHEMA: &str = include_str!("../schemas/structure-response.json");
const OXIDE_APPEARANCE_RESULT_SCHEMA: &str =
    include_str!("../schemas/oxide-appearance-response.json");
const REACTION_MORE_INFO_RESULT_SCHEMA: &str =
    include_str!("../schemas/reaction-more-info-response.json");

const CLAIM_PROMPT_TEMPLATE: &str = include_str!("../prompts/dynamic-reaction.md");
const MECHANISM_PROMPT_TEMPLATE: &str = include_str!("../prompts/dynamic-mechanism.md");
const STRUCTURE_PROMPT_TEMPLATE: &str = include_str!("../prompts/dynamic-structure.md");
const OXIDE_APPEARANCE_PROMPT_TEMPLATE: &str = include_str!("../prompts/oxide-appearance.md");
const REACTION_MORE_INFO_PROMPT_TEMPLATE: &str = include_str!("../prompts/reaction-more-info.md");
pub const FAST_CLAIM_TIMEOUT: Duration = Duration::from_secs(30);
pub const MECHANISM_TIMEOUT: Duration = Duration::from_mins(3);
// Appearance research includes live-source discovery and one bounded repair
// pass. A one-minute shared deadline can expire before the validated repair
// finishes, delaying the exact product-bound material update.
pub const OXIDE_APPEARANCE_TIMEOUT: Duration = Duration::from_mins(3);
pub const REACTION_MORE_INFO_TIMEOUT: Duration = Duration::from_mins(1);
const MAX_ARTIFACT_BYTES: u64 = 2 * 1024 * 1024;
const PREFLIGHT_PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const PREFLIGHT_TOTAL_TIMEOUT: Duration = Duration::from_secs(20);

static RUN_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static CAPABILITY_CACHE: OnceLock<Mutex<BTreeMap<PathBuf, CodexCapabilities>>> = OnceLock::new();

#[derive(Debug, Clone)]
struct CodexCapabilities {
    version: String,
}

/// Runtime configuration for the Codex subscription provider.
#[derive(Debug, Clone)]
pub struct CodexProviderConfig {
    pub executable: PathBuf,
    pub model: Option<String>,
    pub cache_directory: Option<PathBuf>,
    pub progress: Option<Sender<CodexProgressEvent>>,
    /// Generation-scoped cancellation checked while the child is alive.
    pub cancellation: Option<Arc<AtomicBool>>,
}

impl CodexProviderConfig {
    #[must_use]
    pub fn from_environment() -> Self {
        Self {
            executable: PathBuf::from("codex"),
            model: std::env::var("CHEMSPEC_CODEX_MODEL").ok(),
            cache_directory: cache_directory_from_environment(),
            progress: None,
            cancellation: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodexProgressStage {
    Started,
    Working,
    SearchingSources,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodexProgressEvent {
    pub stage: CodexProgressStage,
    pub elapsed_ms: u64,
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
    last_mechanism_response: Option<MechanismEscalationResponse>,
    last_structure_response: Option<StructureProposalResponse>,
    mechanism_deadline: Option<Instant>,
}

impl CodexProvider {
    #[must_use]
    pub const fn new(config: CodexProviderConfig) -> Self {
        Self {
            config,
            last_mechanism_response: None,
            last_structure_response: None,
            mechanism_deadline: None,
        }
    }

    #[must_use]
    pub const fn config(&self) -> &CodexProviderConfig {
        &self.config
    }

    #[must_use]
    pub fn take_last_mechanism_response(&mut self) -> Option<MechanismEscalationResponse> {
        self.last_mechanism_response.take()
    }

    #[must_use]
    pub fn take_last_structure_response(&mut self) -> Option<StructureProposalResponse> {
        self.last_structure_response.take()
    }

    #[must_use]
    pub fn model_name(&self) -> &str {
        self.config.model.as_deref().unwrap_or("codex_default")
    }

    /// Verifies the installed CLI, login state, and every flag used by the
    /// non-interactive invocation. Credential files are never opened.
    ///
    /// # Errors
    ///
    /// Returns a preflight error when Codex cannot run, is missing a required
    /// capability, or its status output cannot be read.
    pub fn preflight(&self) -> Result<CodexPreflight, AgentError> {
        self.preflight_until(Instant::now() + PREFLIGHT_TOTAL_TIMEOUT)
    }

    fn preflight_until(&self, deadline: Instant) -> Result<CodexPreflight, AgentError> {
        let capabilities = cached_capabilities(&self.config.executable, deadline)?;
        let authenticated = bounded_command_output(
            &self.config.executable,
            ["login", "status"],
            remaining_preflight_time(deadline)?,
        )?
        .status
        .success();
        Ok(CodexPreflight {
            version: capabilities.version,
            authenticated,
        })
    }

    /// Obtains one compact factual outcome claim. The model is not asked for
    /// structures, coefficients, mappings, operations, catalogue records, or
    /// `.chems` source.
    ///
    /// # Errors
    ///
    /// Returns a typed preflight, invocation, schema, safety, or disposition
    /// error. One complete-result retry is allowed for a rejected claim.
    pub fn claim_reaction(
        &self,
        request: &ReactionBuildRequest,
        mode: ClaimMode,
    ) -> Result<ProviderClaim, AgentError> {
        let timeout = FAST_CLAIM_TIMEOUT;
        self.claim_reaction_until(request, mode, Instant::now() + timeout)
    }

    /// Obtains a claim within a caller-owned end-to-end deadline.
    ///
    /// # Errors
    ///
    /// Returns the same closed preflight, invocation, and claim errors as
    /// [`Self::claim_reaction`].
    pub fn claim_reaction_until(
        &self,
        request: &ReactionBuildRequest,
        mode: ClaimMode,
        deadline: Instant,
    ) -> Result<ProviderClaim, AgentError> {
        let preflight = self.preflight_until(deadline)?;
        if !preflight.authenticated {
            return Err(AgentError::new(
                AgentErrorKind::ProviderUnavailable,
                "Codex preflight",
                "Codex is installed but not authenticated",
            ));
        }
        let temporary = TemporaryRun::create()?;
        let schema_path = temporary.path.join("claim.schema.json");
        let result_path = temporary.path.join("claim.json");
        fs::write(&schema_path, CLAIM_RESULT_SCHEMA).map_err(|error| {
            AgentError::from_source(AgentErrorKind::ProviderFailure, "Codex run setup", error)
        })?;
        let mut prompt = build_claim_prompt(request, mode, None)?;
        for attempt in 0..=1 {
            let bytes = self.invoke(
                &temporary,
                &schema_path,
                &result_path,
                &prompt,
                deadline,
                false,
            )?;
            match ProviderClaim::from_json(&bytes, mode) {
                Ok(claim) => return Ok(claim),
                Err(error) if attempt == 0 => {
                    prompt = build_claim_prompt(request, mode, Some((&error, &bytes)))?;
                }
                Err(error) => {
                    return Err(AgentError::new(
                        AgentErrorKind::InvalidProviderOutput,
                        "Codex claim retry",
                        format!("claim remained invalid after one retry; {error}"),
                    ));
                }
            }
        }
        unreachable!("the bounded claim loop always returns")
    }

    /// Researches only the representative visible colour of one exact,
    /// already-validated oxide product. Live search is enabled, but the
    /// response remains a provisional model assertion and cannot modify the
    /// reference catalogue.
    ///
    /// # Errors
    ///
    /// Returns a preflight, invocation, schema, identity-binding, source, or
    /// safety error. One full-result correction is allowed.
    pub fn research_oxide_appearance(
        &self,
        request: &OxideAppearanceRequest,
    ) -> Result<ValidatedOxideAppearance, AgentError> {
        let deadline = Instant::now() + OXIDE_APPEARANCE_TIMEOUT;
        let preflight = self.preflight_until(deadline)?;
        if !preflight.authenticated {
            return Err(AgentError::new(
                AgentErrorKind::ProviderUnavailable,
                "Codex preflight",
                "Codex is installed but not authenticated",
            ));
        }
        let temporary = TemporaryRun::create()?;
        let schema_path = temporary.path.join("oxide-appearance.schema.json");
        let result_path = temporary.path.join("oxide-appearance.json");
        fs::write(&schema_path, OXIDE_APPEARANCE_RESULT_SCHEMA).map_err(|error| {
            AgentError::from_source(AgentErrorKind::ProviderFailure, "Codex run setup", error)
        })?;
        let request_json = serde_json::to_string_pretty(request).map_err(|error| {
            AgentError::from_source(
                AgentErrorKind::InvalidRequest,
                "oxide appearance prompt",
                error,
            )
        })?;
        let mut correction = String::new();
        for attempt in 0..=1 {
            let prompt = OXIDE_APPEARANCE_PROMPT_TEMPLATE
                .replace("{{REQUEST_JSON}}", &request_json)
                .replace("{{CORRECTION}}", &correction);
            let bytes = self.invoke(
                &temporary,
                &schema_path,
                &result_path,
                &prompt,
                deadline,
                true,
            )?;
            match OxideAppearanceClaim::from_json_for(&bytes, request) {
                Ok(appearance) => return Ok(appearance),
                Err(error) if attempt == 0 => {
                    correction = format!(
                        "The previous complete JSON result was rejected by the local validator: \
                         {error}. Return a fresh complete result bound exactly to the request."
                    );
                }
                Err(error) => {
                    return Err(AgentError::new(
                        AgentErrorKind::InvalidProviderOutput,
                        "oxide appearance retry",
                        format!("appearance claim remained invalid after one retry; {error}"),
                    ));
                }
            }
        }
        unreachable!("the bounded oxide appearance loop always returns")
    }

    /// Produces a short, presentation-only note about the operating context
    /// of an already-validated reaction. The response cannot alter chemistry,
    /// frames, validation, or catalogue content.
    ///
    /// # Errors
    ///
    /// Returns a preflight, invocation, schema, or paragraph-contract error.
    pub fn reaction_more_info(&self, reaction: &str) -> Result<String, AgentError> {
        let reaction = reaction.trim();
        if reaction.is_empty() || reaction.chars().count() > 1_000 {
            return Err(AgentError::new(
                AgentErrorKind::InvalidRequest,
                "reaction more info",
                "the validated reaction label is empty or too long",
            ));
        }
        let deadline = Instant::now() + REACTION_MORE_INFO_TIMEOUT;
        let preflight = self.preflight_until(deadline)?;
        if !preflight.authenticated {
            return Err(AgentError::new(
                AgentErrorKind::ProviderUnavailable,
                "Codex preflight",
                "Codex is installed but not authenticated",
            ));
        }
        let temporary = TemporaryRun::create()?;
        let schema_path = temporary.path.join("reaction-more-info.schema.json");
        let result_path = temporary.path.join("reaction-more-info.json");
        fs::write(&schema_path, REACTION_MORE_INFO_RESULT_SCHEMA).map_err(|error| {
            AgentError::from_source(AgentErrorKind::ProviderFailure, "Codex run setup", error)
        })?;
        let prompt = REACTION_MORE_INFO_PROMPT_TEMPLATE.replace("{{REACTION}}", reaction);
        let bytes = self.invoke(
            &temporary,
            &schema_path,
            &result_path,
            &prompt,
            deadline,
            false,
        )?;
        parse_reaction_more_info(&bytes)
    }

    /// Requests one mapping/operation proposal for an immutable, locally
    /// compiled labelled reaction. Search is deliberately disabled.
    ///
    /// # Errors
    ///
    /// Returns a preflight, invocation, schema, size, or response-contract
    /// error. Chemical validity is established later by the kernel.
    pub fn propose_mechanism(
        &self,
        request: &MechanismEscalationRequest,
        diagnostic: Option<&str>,
    ) -> Result<MechanismEscalationResponse, AgentError> {
        self.propose_mechanism_until(request, diagnostic, Instant::now() + MECHANISM_TIMEOUT)
    }

    fn propose_mechanism_until(
        &self,
        request: &MechanismEscalationRequest,
        diagnostic: Option<&str>,
        deadline: Instant,
    ) -> Result<MechanismEscalationResponse, AgentError> {
        let preflight = self.preflight_until(deadline)?;
        if !preflight.authenticated {
            return Err(AgentError::new(
                AgentErrorKind::ProviderUnavailable,
                "Codex preflight",
                "Codex is installed but not authenticated",
            ));
        }
        let temporary = TemporaryRun::create()?;
        let schema_path = temporary.path.join("mechanism.schema.json");
        let result_path = temporary.path.join("mechanism.json");
        fs::write(&schema_path, MECHANISM_RESULT_SCHEMA).map_err(|error| {
            AgentError::from_source(AgentErrorKind::ProviderFailure, "Codex run setup", error)
        })?;
        let prompt = build_mechanism_prompt(request, diagnostic)?;
        let bytes = self.invoke(
            &temporary,
            &schema_path,
            &result_path,
            &prompt,
            deadline,
            false,
        )?;
        MechanismEscalationResponse::from_json(&bytes)
    }

    fn propose_structures_until(
        &self,
        request: &StructureProposalRequest,
        diagnostic: Option<&str>,
        deadline: Instant,
    ) -> Result<StructureProposalResponse, AgentError> {
        let preflight = self.preflight_until(deadline)?;
        if !preflight.authenticated {
            return Err(AgentError::new(
                AgentErrorKind::ProviderUnavailable,
                "Codex preflight",
                "Codex is installed but not authenticated",
            ));
        }
        let temporary = TemporaryRun::create()?;
        let schema_path = temporary.path.join("structure.schema.json");
        let result_path = temporary.path.join("structure.json");
        fs::write(&schema_path, STRUCTURE_RESULT_SCHEMA).map_err(|error| {
            AgentError::from_source(AgentErrorKind::ProviderFailure, "Codex run setup", error)
        })?;
        let prompt = build_structure_prompt(request, diagnostic)?;
        let bytes = self.invoke(
            &temporary,
            &schema_path,
            &result_path,
            &prompt,
            deadline,
            false,
        )?;
        StructureProposalResponse::from_json(&bytes)
    }

    #[allow(clippy::too_many_lines)]
    fn invoke(
        &self,
        temporary: &TemporaryRun,
        schema_path: &Path,
        result_path: &Path,
        prompt: &str,
        deadline: Instant,
        live_search: bool,
    ) -> Result<Vec<u8>, AgentError> {
        if result_path.exists() {
            fs::remove_file(result_path).map_err(|error| {
                AgentError::from_source(AgentErrorKind::ProviderFailure, "Codex run setup", error)
            })?;
        }
        let started = Instant::now();
        self.send_progress(CodexProgressStage::Started, started);
        let mut command = Command::new(&self.config.executable);
        if live_search {
            command.arg("--search");
        }
        command
            .arg("exec")
            .arg("--config")
            .arg("model_reasoning_effort=\"low\"")
            .arg("--config")
            .arg("service_tier=\"default\"")
            .arg("--json")
            .arg("--output-schema")
            .arg(schema_path)
            .arg("--output-last-message")
            .arg(result_path)
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
        command.arg("-");
        configure_child_process(&mut command);
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| {
                AgentError::from_source(AgentErrorKind::ProviderFailure, "Codex invocation", error)
            })?;
        if let Some(mut stdin) = child.stdin.take()
            && let Err(error) = stdin.write_all(prompt.as_bytes())
        {
            terminate_child_tree(&mut child);
            return Err(AgentError::from_source(
                AgentErrorKind::ProviderFailure,
                "Codex invocation",
                error,
            ));
        }
        let stdout = child.stdout.take().ok_or_else(|| {
            AgentError::new(
                AgentErrorKind::ProviderFailure,
                "Codex invocation",
                "failed to capture Codex stdout",
            )
        })?;
        let progress = self.config.progress.clone();
        let progress_reader =
            std::thread::spawn(move || drain_progress(stdout, progress.as_ref(), started));
        let stderr = child.stderr.take().ok_or_else(|| {
            AgentError::new(
                AgentErrorKind::ProviderFailure,
                "Codex invocation",
                "failed to capture Codex stderr",
            )
        })?;
        let stderr_reader = std::thread::spawn(move || drain_bounded(stderr, 2_000));
        let status = loop {
            if let Some(status) = child.try_wait().map_err(|error| {
                AgentError::from_source(AgentErrorKind::ProviderFailure, "Codex invocation", error)
            })? {
                break status;
            }
            if self
                .config
                .cancellation
                .as_ref()
                .is_some_and(|cancellation| cancellation.load(Ordering::Relaxed))
            {
                terminate_child_tree(&mut child);
                let _ = stderr_reader.join();
                let _ = progress_reader.join();
                self.send_progress(CodexProgressStage::Failed, started);
                return Err(AgentError::new(
                    AgentErrorKind::Cancelled,
                    "Codex cancellation",
                    "provider invocation was cancelled",
                ));
            }
            if Instant::now() >= deadline {
                terminate_child_tree(&mut child);
                let _ = stderr_reader.join();
                let _ = progress_reader.join();
                self.send_progress(CodexProgressStage::Failed, started);
                return Err(AgentError::new(
                    AgentErrorKind::TimedOut,
                    "Codex timeout",
                    "provider invocation exceeded its bounded deadline",
                ));
            }
            std::thread::sleep(Duration::from_millis(100));
        };
        let stderr = stderr_reader
            .join()
            .map_err(|_| {
                AgentError::new(
                    AgentErrorKind::ProviderFailure,
                    "Codex invocation",
                    "stderr reader failed",
                )
            })?
            .map_err(|error| {
                AgentError::from_source(AgentErrorKind::ProviderFailure, "Codex invocation", error)
            })?;
        let stream_error = progress_reader
            .join()
            .map_err(|_| {
                AgentError::new(
                    AgentErrorKind::ProviderFailure,
                    "Codex invocation",
                    "progress reader failed",
                )
            })?
            .map_err(|error| {
                AgentError::from_source(AgentErrorKind::ProviderFailure, "Codex invocation", error)
            })?;
        if !status.success() {
            self.send_progress(CodexProgressStage::Failed, started);
            let stderr = String::from_utf8_lossy(&stderr).trim().to_owned();
            let message = if stderr.is_empty() {
                stream_error.unwrap_or_else(|| "Codex exited without diagnostic output".to_owned())
            } else {
                stderr
            };
            return Err(AgentError::new(
                AgentErrorKind::ProviderFailure,
                "Codex invocation",
                message,
            ));
        }
        self.send_progress(CodexProgressStage::Completed, started);
        read_bounded(result_path, MAX_ARTIFACT_BYTES)
    }

    fn send_progress(&self, stage: CodexProgressStage, started: Instant) {
        if let Some(sender) = &self.config.progress {
            let _ = sender.send(CodexProgressEvent {
                stage,
                elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            });
        }
    }

    /// One escalation window per provider instance: structure proposals,
    /// mechanism proposals, and every repair share a single 180-second
    /// end-to-end deadline. The application constructs one provider per
    /// presentation generation, so retries and regeneration start fresh.
    fn shared_mechanism_deadline(&mut self) -> Instant {
        *self
            .mechanism_deadline
            .get_or_insert_with(|| Instant::now() + MECHANISM_TIMEOUT)
    }
}

fn parse_reaction_more_info(bytes: &[u8]) -> Result<String, AgentError> {
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|error| {
        AgentError::from_source(
            AgentErrorKind::InvalidProviderOutput,
            "reaction more info",
            error,
        )
    })?;
    let answer = value
        .as_object()
        .filter(|object| object.len() == 1)
        .and_then(|object| object.get("answer"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|answer| !answer.is_empty())
        .ok_or_else(|| {
            AgentError::new(
                AgentErrorKind::InvalidProviderOutput,
                "reaction more info",
                "Codex returned an invalid answer object",
            )
        })?;
    let paragraphs = answer
        .split("\n\n")
        .filter(|paragraph| !paragraph.trim().is_empty())
        .count();
    if !(2..=3).contains(&paragraphs) {
        return Err(AgentError::new(
            AgentErrorKind::InvalidProviderOutput,
            "reaction more info",
            "Codex must return two or three short paragraphs",
        ));
    }
    Ok(answer.to_owned())
}

impl MechanismProvider for CodexProvider {
    fn propose(
        &mut self,
        request: &MechanismEscalationRequest,
        diagnostic: Option<&str>,
    ) -> Result<MechanismEscalationResponse, AgentError> {
        let deadline = self.shared_mechanism_deadline();
        let response = self.propose_mechanism_until(request, diagnostic, deadline)?;
        self.last_mechanism_response = Some(response.clone());
        Ok(response)
    }

    fn propose_structures(
        &mut self,
        request: &StructureProposalRequest,
        diagnostic: Option<&str>,
    ) -> Result<StructureProposalResponse, AgentError> {
        let deadline = self.shared_mechanism_deadline();
        let response = self.propose_structures_until(request, diagnostic, deadline)?;
        self.last_structure_response = Some(response.clone());
        Ok(response)
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

/// Streams closed progress events and captures the last provider error event.
/// Codex reports request failures as stdout JSONL with an empty stderr, so
/// this is the only place an actionable diagnostic exists.
fn drain_progress(
    reader: impl Read,
    sender: Option<&Sender<CodexProgressEvent>>,
    started: Instant,
) -> std::io::Result<Option<String>> {
    let mut reader = BufReader::new(reader);
    let mut line = Vec::new();
    let mut last_error = None;
    loop {
        line.clear();
        let count = reader.read_until(b'\n', &mut line)?;
        if count == 0 {
            return Ok(last_error);
        }
        if line.len() > 64 * 1024 {
            continue;
        }
        if let Some(message) = extract_error(&line) {
            last_error = Some(message);
        }
        let Some(stage) = classify_progress(&line) else {
            continue;
        };
        if let Some(sender) = &sender {
            let _ = sender.send(CodexProgressEvent {
                stage,
                elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            });
        }
    }
}

fn extract_error(line: &[u8]) -> Option<String> {
    let value: serde_json::Value = serde_json::from_slice(line).ok()?;
    let event_type = value.get("type")?.as_str()?;
    let message = match event_type {
        "error" => value.get("message")?.as_str()?,
        "turn.failed" => value.get("error")?.get("message")?.as_str()?,
        _ => return None,
    };
    let mut message = message.to_owned();
    message.truncate(1_000);
    Some(message)
}

fn classify_progress(line: &[u8]) -> Option<CodexProgressStage> {
    let value: serde_json::Value = serde_json::from_slice(line).ok()?;
    let event_type = value
        .get("type")
        .or_else(|| value.get("event"))?
        .as_str()?
        .to_ascii_lowercase();
    if event_type.contains("web_search") || event_type.contains("search") {
        Some(CodexProgressStage::SearchingSources)
    } else if event_type.contains("started")
        || event_type.contains("delta")
        || event_type.contains("tool")
        || event_type.contains("item")
        || event_type.contains("turn")
    {
        Some(CodexProgressStage::Working)
    } else {
        None
    }
}

fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, AgentError> {
    let length = fs::metadata(path)
        .map_err(|error| {
            AgentError::from_source(AgentErrorKind::InvalidProviderOutput, "Codex result", error)
        })?
        .len();
    if length > limit {
        return Err(AgentError::new(
            AgentErrorKind::InvalidProviderOutput,
            "Codex result",
            format!("result exceeded the {limit}-byte limit"),
        ));
    }
    fs::read(path).map_err(|error| {
        AgentError::from_source(AgentErrorKind::InvalidProviderOutput, "Codex result", error)
    })
}

fn cache_directory_from_environment() -> Option<PathBuf> {
    std::env::var_os("CHEMSPEC_CACHE_DIR")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(default_cache_directory)
}

#[cfg(target_os = "macos")]
fn default_cache_directory() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| {
        PathBuf::from(home).join("Library/Caches/dev.charlesmills.chemspec/dynamic-reactions")
    })
}

#[cfg(target_os = "windows")]
fn default_cache_directory() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|path| path.join("ChemSpec/Cache/dynamic-reactions"))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn default_cache_directory() -> Option<PathBuf> {
    std::env::var_os("XDG_CACHE_HOME")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .map(|path| path.join("chemspec/dynamic-reactions"))
}

#[cfg(not(any(unix, target_os = "windows")))]
fn default_cache_directory() -> Option<PathBuf> {
    None
}

fn command_text<const N: usize>(
    executable: &Path,
    arguments: [&str; N],
    deadline: Instant,
) -> Result<String, AgentError> {
    let output =
        bounded_command_output(executable, arguments, remaining_preflight_time(deadline)?)?;
    if !output.status.success() {
        return Err(AgentError::new(
            AgentErrorKind::UnsupportedCapability,
            "Codex preflight",
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn remaining_preflight_time(deadline: Instant) -> Result<Duration, AgentError> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|remaining| !remaining.is_zero())
        .map(|remaining| remaining.min(PREFLIGHT_PROBE_TIMEOUT))
        .ok_or_else(|| {
            AgentError::new(
                AgentErrorKind::TimedOut,
                "Codex preflight timeout",
                "the caller-owned deadline expired during preflight",
            )
        })
}

fn bounded_command_output<const N: usize>(
    executable: &Path,
    arguments: [&str; N],
    timeout: Duration,
) -> Result<std::process::Output, AgentError> {
    let mut command = Command::new(executable);
    command.args(arguments);
    configure_child_process(&mut command);
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            AgentError::from_source(
                AgentErrorKind::ProviderUnavailable,
                "Codex preflight",
                error,
            )
        })?;
    let deadline = Instant::now() + timeout;
    loop {
        if child
            .try_wait()
            .map_err(|error| {
                AgentError::from_source(
                    AgentErrorKind::ProviderUnavailable,
                    "Codex preflight",
                    error,
                )
            })?
            .is_some()
        {
            return child.wait_with_output().map_err(|error| {
                AgentError::from_source(
                    AgentErrorKind::ProviderUnavailable,
                    "Codex preflight",
                    error,
                )
            });
        }
        if Instant::now() >= deadline {
            terminate_child_tree(&mut child);
            return Err(AgentError::new(
                AgentErrorKind::TimedOut,
                "Codex preflight timeout",
                "capability or authentication probe exceeded five seconds",
            ));
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[cfg(unix)]
fn configure_child_process(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(windows)]
fn configure_child_process(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    command.creation_flags(CREATE_NEW_PROCESS_GROUP);
}

#[cfg(not(any(unix, windows)))]
fn configure_child_process(_: &mut Command) {}

#[cfg(unix)]
fn terminate_child_tree(child: &mut Child) {
    use rustix::process::{Pid, Signal, kill_process_group};

    let group = Pid::from_child(child);
    let _ = kill_process_group(group, Signal::TERM);
    for _ in 0..4 {
        std::thread::sleep(Duration::from_millis(25));
        if child.try_wait().ok().flatten().is_some() {
            break;
        }
    }
    // Kill the process group even when the leader already exited: a spawned
    // descendant may still hold the captured pipes open and otherwise make a
    // 90-second provider deadline unbounded.
    let _ = kill_process_group(group, Signal::KILL);
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(windows)]
fn terminate_child_tree(child: &mut Child) {
    let _ = Command::new("taskkill")
        .args(["/PID", &child.id().to_string(), "/T", "/F"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(not(any(unix, windows)))]
fn terminate_child_tree(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn cached_capabilities(
    executable: &Path,
    deadline: Instant,
) -> Result<CodexCapabilities, AgentError> {
    let cache = CAPABILITY_CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
    if let Some(capabilities) = cache
        .lock()
        .map_err(|_| {
            AgentError::new(
                AgentErrorKind::ProviderUnavailable,
                "Codex preflight",
                "capability cache lock was poisoned",
            )
        })?
        .get(executable)
        .cloned()
    {
        return Ok(capabilities);
    }
    let version = command_text(executable, ["--version"], deadline)?;
    let top_help = command_text(executable, ["--help"], deadline)?;
    if !top_help.contains("--search") {
        return Err(AgentError::new(
            AgentErrorKind::ProviderUnavailable,
            "Codex preflight",
            "installed Codex does not expose live web search",
        ));
    }
    let exec_help = command_text(executable, ["exec", "--help"], deadline)?;
    for capability in [
        "--config",
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
                AgentErrorKind::UnsupportedCapability,
                "Codex preflight",
                format!("installed Codex is missing `{capability}`"),
            ));
        }
    }
    let capabilities = CodexCapabilities {
        version: version.trim().to_owned(),
    };
    cache
        .lock()
        .map_err(|_| {
            AgentError::new(
                AgentErrorKind::ProviderUnavailable,
                "Codex preflight",
                "capability cache lock was poisoned",
            )
        })?
        .insert(executable.to_owned(), capabilities.clone());
    Ok(capabilities)
}

fn build_claim_prompt(
    request: &ReactionBuildRequest,
    mode: ClaimMode,
    correction: Option<(&AgentError, &[u8])>,
) -> Result<String, AgentError> {
    let request = serde_json::to_string_pretty(request).map_err(|error| {
        AgentError::from_source(AgentErrorKind::InternalFailure, "Codex prompt", error)
    })?;
    let mode_name = match mode {
        ClaimMode::Fast => "Fast",
    };
    let source_policy = "Use model knowledge only. Do not browse or invent citations; return sources as an empty array.";
    let mut prompt = CLAIM_PROMPT_TEMPLATE
        .replace("{{REQUEST_JSON}}", &request)
        .replace("{{MODE}}", mode_name)
        .replace("{{SOURCE_POLICY}}", source_policy);
    if let Some((diagnostic, previous)) = correction {
        let previous = std::str::from_utf8(previous).map_err(|error| {
            AgentError::from_source(AgentErrorKind::InternalFailure, "Codex prompt", error)
        })?;
        prompt.push_str("\n\n## One targeted correction\n\n");
        prompt.push_str("The previous complete claim failed strict validation: ");
        prompt.push_str(&diagnostic.to_string());
        prompt.push_str(
            "\nReturn one complete corrected claim. Do not add fields. Previous claim:\n",
        );
        prompt.push_str(previous);
    }
    if prompt.contains("{{") {
        return Err(AgentError::new(
            AgentErrorKind::InternalFailure,
            "Codex prompt",
            "runtime prompt contains an unresolved placeholder",
        ));
    }
    Ok(prompt)
}

fn build_mechanism_prompt(
    request: &MechanismEscalationRequest,
    diagnostic: Option<&str>,
) -> Result<String, AgentError> {
    let request = serde_json::to_string_pretty(request).map_err(|error| {
        AgentError::from_source(AgentErrorKind::InternalFailure, "Codex prompt", error)
    })?;
    let repair = diagnostic.map_or_else(
        || "This is the first proposal.".to_owned(),
        |diagnostic| {
            format!(
                "The prior operations failed local validation: {diagnostic}\nReturn one complete corrected mapping and operation list without changing the fixed request."
            )
        },
    );
    let prompt = MECHANISM_PROMPT_TEMPLATE
        .replace("{{MECHANISM_REQUEST_JSON}}", &request)
        .replace("{{REPAIR_CONTEXT}}", &repair);
    if prompt.contains("{{") {
        return Err(AgentError::new(
            AgentErrorKind::InternalFailure,
            "Codex prompt",
            "runtime mechanism prompt contains an unresolved placeholder",
        ));
    }
    Ok(prompt)
}

fn build_structure_prompt(
    request: &StructureProposalRequest,
    diagnostic: Option<&str>,
) -> Result<String, AgentError> {
    let request = serde_json::to_string_pretty(request).map_err(|error| {
        AgentError::from_source(AgentErrorKind::InternalFailure, "Codex prompt", error)
    })?;
    let repair = diagnostic.map_or_else(
        || "This is the first proposal.".to_owned(),
        |diagnostic| {
            format!(
                "The prior structures failed local validation: {diagnostic}\nReturn one complete corrected structure list without changing the requested species, ids, or formulas."
            )
        },
    );
    let prompt = STRUCTURE_PROMPT_TEMPLATE
        .replace("{{STRUCTURE_REQUEST_JSON}}", &request)
        .replace("{{REPAIR_CONTEXT}}", &repair);
    if prompt.contains("{{") {
        return Err(AgentError::new(
            AgentErrorKind::InternalFailure,
            "Codex prompt",
            "runtime structure prompt contains an unresolved placeholder",
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
        fs::create_dir(&path).map_err(|error| {
            AgentError::from_source(AgentErrorKind::ProviderFailure, "Codex run setup", error)
        })?;
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

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn result_schema_is_valid_json() {
        let schema: serde_json::Value = serde_json::from_str(CLAIM_RESULT_SCHEMA).expect("schema");
        assert_eq!(schema["properties"]["schema_version"]["const"], json!(1));
        assert!(
            schema["required"]
                .as_array()
                .expect("required fields")
                .contains(&json!("reactant_phases"))
        );
        assert_eq!(
            schema["properties"]["reactant_phases"]["maxItems"],
            json!(2)
        );
        assert_eq!(schema["properties"]["sources"]["maxItems"], json!(4));
        assert!(
            CLAIM_RESULT_SCHEMA.len() < 12_000,
            "claim schema unexpectedly grew to {} bytes",
            CLAIM_RESULT_SCHEMA.len()
        );
        let mechanism: serde_json::Value =
            serde_json::from_str(MECHANISM_RESULT_SCHEMA).expect("mechanism schema");
        assert_eq!(mechanism["properties"]["schema_version"]["const"], json!(1));
        assert_eq!(
            mechanism["properties"]["operations"]["items"]["anyOf"]
                .as_array()
                .expect("closed operation variants")
                .len(),
            13
        );
        let appearance: serde_json::Value =
            serde_json::from_str(OXIDE_APPEARANCE_RESULT_SCHEMA).expect("appearance schema");
        assert_eq!(
            appearance["properties"]["schema_version"]["const"],
            json!(1)
        );
        let more_info: serde_json::Value = serde_json::from_str(REACTION_MORE_INFO_RESULT_SCHEMA)
            .expect("reaction more-info schema");
        assert_eq!(more_info["properties"]["answer"]["maxLength"], json!(2400));
        assert_eq!(appearance["properties"]["sources"]["maxItems"], json!(3));
    }

    #[test]
    fn prompt_contains_request_and_safety_boundary() {
        let request = ReactionBuildRequest {
            reactants: [
                crate::ReactantInput {
                    display: "Ca".to_owned(),
                    atomic_numbers: vec![20],
                    species_id: None,
                },
                crate::ReactantInput {
                    display: "H2O".to_owned(),
                    atomic_numbers: vec![1, 1, 8],
                    species_id: None,
                },
            ]
            .to_vec(),
            selected_context: Some("aqueous learner context".into()),
        };
        let prompt = build_claim_prompt(&request, ClaimMode::Fast, None).expect("prompt");
        assert!(prompt.contains("\"Ca\""));
        assert!(prompt.contains("aqueous learner context"));
        assert!(prompt.contains("not a laboratory procedure"));
        assert!(prompt.contains("factual reaction claim"));
        assert!(prompt.contains("Do not output any of those"));
        assert!(prompt.contains("Use model knowledge only"));
        assert!(prompt.contains("do not return `ambiguous` solely because quantities were"));
        assert!(prompt.contains("characteristic visible bulk colour"));
        assert!(prompt.contains("Rgb.HexRRGGBB"));
        assert!(!prompt.contains("{{"));
        for forbidden in [
            "release_metallic",
            "operation_template",
            "mapping_template",
            "valence_premises",
            "Catalogue schema",
            "Normative `.chems`",
        ] {
            assert!(!prompt.contains(forbidden));
        }
        assert!(
            prompt.len() < 8_000,
            "runtime prompt unexpectedly grew to {} bytes",
            prompt.len()
        );
    }

    #[test]
    fn single_reactant_prompt_keeps_energy_as_exact_context() {
        let request = ReactionBuildRequest {
            reactants: vec![crate::ReactantInput {
                display: "AgCl".to_owned(),
                atomic_numbers: vec![47, 17],
                species_id: None,
            }],
            selected_context: Some("light".into()),
        };
        let prompt = build_claim_prompt(&request, ClaimMode::Fast, None).expect("prompt");
        assert!(prompt.contains("one or two reactants"));
        assert!(prompt.contains("preserve that exact"));
        assert!(prompt.contains("never turn energy into a reactant or product"));
        assert!(prompt.contains("\"selected_context\": \"light\""));
    }

    #[test]
    fn progress_stream_exposes_only_closed_product_events() {
        assert_eq!(
            classify_progress(br#"{"type":"web_search.started","query":"ignored"}"#),
            Some(CodexProgressStage::SearchingSources)
        );
        assert_eq!(
            classify_progress(br#"{"type":"item.started","text":"ignored"}"#),
            Some(CodexProgressStage::Working)
        );
        assert_eq!(
            classify_progress(br#"{"type":"reasoning","text":"must not surface"}"#),
            None
        );
    }

    #[test]
    fn output_schemas_stay_strict_structured_output_compatible() {
        // Codex forwards the schema to strict structured outputs, which
        // rejects `oneOf`, tuple `prefixItems`/`items: false`, and bare
        // `const` without an explicit `type`. A violation fails live with an
        // empty stderr, so it must be caught here instead.
        fn assert_consts_typed(value: &serde_json::Value) {
            match value {
                serde_json::Value::Object(map) => {
                    if map.contains_key("const") {
                        assert!(
                            map.contains_key("type"),
                            "const without explicit type: {map:?}"
                        );
                    }
                    for nested in map.values() {
                        assert_consts_typed(nested);
                    }
                }
                serde_json::Value::Array(values) => {
                    for nested in values {
                        assert_consts_typed(nested);
                    }
                }
                _ => {}
            }
        }
        for schema in [
            CLAIM_RESULT_SCHEMA,
            MECHANISM_RESULT_SCHEMA,
            STRUCTURE_RESULT_SCHEMA,
            OXIDE_APPEARANCE_RESULT_SCHEMA,
            REACTION_MORE_INFO_RESULT_SCHEMA,
        ] {
            assert!(!schema.contains("oneOf"), "strict mode rejects oneOf");
            assert!(
                !schema.contains("prefixItems"),
                "strict mode rejects tuple prefixItems"
            );
            assert!(
                !schema.contains("\"items\": false"),
                "strict mode rejects items: false"
            );
            let value: serde_json::Value = serde_json::from_str(schema).expect("schema JSON");
            assert_consts_typed(&value);
        }
    }

    #[test]
    fn reaction_more_info_prompt_is_brief_and_safety_bounded() {
        let prompt =
            REACTION_MORE_INFO_PROMPT_TEMPLATE.replace("{{REACTION}}", "2 H₂ + O₂ → 2 H₂O");
        assert!(prompt.contains("temperature"));
        assert!(prompt.contains("pressure"));
        assert!(prompt.contains("catalysts"));
        assert!(prompt.contains("industry or the environment"));
        assert!(prompt.contains("2–3 short paragraphs"));
        assert!(prompt.contains("Do not provide step-by-step"));
        assert!(!prompt.contains("{{"));
    }

    #[test]
    fn reaction_more_info_requires_two_or_three_paragraphs() {
        let valid = br#"{"answer":"Conditions paragraph.\n\nOccurrence paragraph."}"#;
        assert_eq!(
            parse_reaction_more_info(valid).expect("two paragraphs are valid"),
            "Conditions paragraph.\n\nOccurrence paragraph."
        );
        assert!(parse_reaction_more_info(br#"{"answer":"Only one paragraph."}"#).is_err());
        assert!(
            parse_reaction_more_info(br#"{"answer":"One.\n\nTwo.\n\nThree.\n\nFour."}"#).is_err()
        );
    }

    #[test]
    fn empty_stderr_failures_surface_the_stdout_error_event() {
        assert_eq!(
            extract_error(br#"{"type":"error","message":"invalid_json_schema: details"}"#),
            Some("invalid_json_schema: details".to_owned())
        );
        assert_eq!(
            extract_error(br#"{"type":"turn.failed","error":{"message":"request failed"}}"#),
            Some("request failed".to_owned())
        );
        assert_eq!(extract_error(br#"{"type":"item.completed"}"#), None);
    }

    #[test]
    fn structure_and_mechanism_escalation_share_one_total_deadline() {
        let mut provider = CodexProvider::new(CodexProviderConfig::from_environment());
        let initial = provider.shared_mechanism_deadline();
        assert_eq!(provider.shared_mechanism_deadline(), initial);
        assert_eq!(provider.shared_mechanism_deadline(), initial);
    }

    #[cfg(unix)]
    #[test]
    fn unix_termination_kills_descendants_that_hold_pipes() {
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", "(trap '' HUP TERM; sleep 30) & echo ready; wait"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        configure_child_process(&mut command);
        let mut child = command.spawn().expect("pipe-holding child tree");
        let stdout = child.stdout.take().expect("captured stdout");
        let mut reader = BufReader::new(stdout);
        let mut ready = String::new();
        reader.read_line(&mut ready).expect("descendant ready line");
        assert_eq!(ready, "ready\n");
        let drain = std::thread::spawn(move || {
            let mut remaining = Vec::new();
            reader.read_to_end(&mut remaining).map(|_| remaining)
        });

        let started = Instant::now();
        terminate_child_tree(&mut child);
        drain
            .join()
            .expect("pipe reader thread")
            .expect("pipe reader result");
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "descendant-held pipe survived termination: {:?}",
            started.elapsed()
        );
    }

    #[cfg(unix)]
    #[test]
    fn capability_probes_are_cached_but_authentication_is_rechecked() {
        let directory = std::env::temp_dir().join(format!(
            "chemspec-preflight-cache-{}-{}",
            std::process::id(),
            RUN_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).expect("temporary preflight directory");
        let executable = directory.join("codex");
        let log = directory.join("calls.log");
        let script = format!(
            r#"#!/bin/sh
printf '%s\n' "$*" >> '{}'
case "$*" in
  "--version") echo "codex-test 1.0" ;;
  "--help") echo "--search" ;;
  "exec --help") echo "--config --output-schema --sandbox --ephemeral --ignore-user-config --ignore-rules --skip-git-repo-check --output-last-message" ;;
  "login status") exit 0 ;;
  *) exit 1 ;;
esac
"#,
            log.display()
        );
        fs::write(&executable, script).expect("fake Codex executable");
        let mut permissions = fs::metadata(&executable).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&executable, permissions).expect("executable permissions");

        let mut config = CodexProviderConfig::from_environment();
        config.executable = executable.clone();
        let provider = CodexProvider::new(config);
        assert!(provider.preflight().expect("first preflight").authenticated);
        assert!(
            provider
                .preflight()
                .expect("second preflight")
                .authenticated
        );

        let calls = fs::read_to_string(log).expect("probe log");
        assert_eq!(calls.matches("--version").count(), 1);
        assert_eq!(calls.matches("exec --help").count(), 1);
        assert_eq!(calls.lines().filter(|line| *line == "--help").count(), 1);
        assert_eq!(calls.matches("login status").count(), 2);

        fs::write(
            &executable,
            "#!/bin/sh\n(trap '' HUP TERM; sleep 30) &\nwait\n",
        )
        .expect("slow fake Codex executable");
        let started = Instant::now();
        let error = bounded_command_output(&executable, ["--version"], Duration::from_millis(20))
            .expect_err("slow preflight must be bounded");
        assert_eq!(error.kind(), AgentErrorKind::TimedOut);
        assert_eq!(error.context(), "Codex preflight timeout");
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "descendant-held pipes exceeded the deadline: {:?}",
            started.elapsed()
        );
        fs::remove_dir_all(directory).expect("remove temporary preflight directory");
    }

    #[cfg(unix)]
    #[test]
    fn provider_cancellation_has_a_closed_error_kind() {
        let directory = std::env::temp_dir().join(format!(
            "chemspec-cancellation-kind-{}-{}",
            std::process::id(),
            RUN_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).expect("temporary cancellation directory");
        let executable = directory.join("codex");
        fs::write(
            &executable,
            r#"#!/bin/sh
case "$*" in
  "--version") echo "codex-test 1.0" ;;
  "--help") echo "--search" ;;
  "exec --help") echo "--config --output-schema --sandbox --ephemeral --ignore-user-config --ignore-rules --skip-git-repo-check --output-last-message" ;;
  "login status") exit 0 ;;
  "exec "*)
    (trap '' HUP TERM; sleep 30) &
    wait
    ;;
  *) exit 1 ;;
esac
"#,
        )
        .expect("fake Codex executable");
        let mut permissions = fs::metadata(&executable).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&executable, permissions).expect("executable permissions");

        let mut config = CodexProviderConfig::from_environment();
        config.executable = executable;
        config.cancellation = Some(Arc::new(AtomicBool::new(true)));
        let provider = CodexProvider::new(config);
        let request = ReactionBuildRequest {
            reactants: vec![crate::ReactantInput {
                display: "H2".to_owned(),
                atomic_numbers: vec![1, 1],
                species_id: None,
            }],
            selected_context: Some("electricity".to_owned()),
        };
        let started = Instant::now();
        let error = provider
            .claim_reaction(&request, ClaimMode::Fast)
            .expect_err("pre-cancelled invocation must stop");

        assert_eq!(error.kind(), AgentErrorKind::Cancelled);
        assert_eq!(error.context(), "Codex cancellation");
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "descendant-held pipes exceeded cancellation: {:?}",
            started.elapsed()
        );
        fs::remove_dir_all(directory).expect("remove cancellation directory");
    }
}
