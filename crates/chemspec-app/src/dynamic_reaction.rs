//! Dynamic-reaction workflow state and closed transition types.
//!
//! The application owns presentation, but the finite provider/solver workflow
//! is one feature state rather than a cluster of unrelated `App` fields.

use std::{
    sync::{
        Arc, Weak,
        atomic::{AtomicBool, Ordering},
        mpsc::Receiver,
    },
    time::Instant,
};

use agent::{
    AgentError, ClaimMode, CodexProgressEvent, CodexProvider, CodexProviderConfig,
    CompiledClaimOutcome, DynamicPresentationOutcome, FAST_CLAIM_TIMEOUT, LatencyMilestones,
    ReactantIdentityAmbiguity, ReactionBuildRequest, ReactionClaim, ValidatedStaticOutcome,
    compile_claim_outcome, load_dynamic_cache, store_dynamic_cache,
};
use chem_catalogue::TrustedCatalogue;
use chem_domain::{SpeciesId, SpeciesRegistry};
use iced::widget::{button, column, container, mouse_area, row, space, stack, text};
use iced::{Center, Element, Fill, Length, Size, Task};

use crate::{
    App, Message as AppMessage, Screen, blocking, elapsed_millis,
    theme::{self, color, space as spacing, type_scale},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RequestContext {
    Heat,
    Light,
    Electricity,
    LimitedOxygen,
    Catalyst,
}

impl RequestContext {
    pub(super) const ALL: [Self; 5] = [
        Self::Heat,
        Self::Light,
        Self::Electricity,
        Self::LimitedOxygen,
        Self::Catalyst,
    ];

    pub(super) const fn value(self) -> &'static str {
        match self {
            Self::Heat => "heat",
            Self::Light => "light",
            Self::Electricity => "electricity",
            Self::LimitedOxygen => "limited oxygen",
            Self::Catalyst => "catalyst",
        }
    }

    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Heat => "Heat",
            Self::Light => "Light",
            Self::Electricity => "Electricity",
            Self::LimitedOxygen => "Limited oxygen",
            Self::Catalyst => "Catalyst",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(super) enum BuildState {
    #[default]
    Idle,
    Running {
        run_id: u64,
        elapsed_seconds: u64,
        stage: BuildStage,
    },
    Failed(BuildFailure),
}

#[derive(Debug, Clone)]
pub(super) enum BuildFailure {
    Agent(AgentError),
    Application(String),
}

impl From<AgentError> for BuildFailure {
    fn from(error: AgentError) -> Self {
        Self::Agent(error)
    }
}

impl From<String> for BuildFailure {
    fn from(error: String) -> Self {
        Self::Application(error)
    }
}

impl From<&str> for BuildFailure {
    fn from(error: &str) -> Self {
        Self::Application(error.to_owned())
    }
}

impl From<blocking::BlockingFailure> for BuildFailure {
    fn from(error: blocking::BlockingFailure) -> Self {
        Self::Application(error.to_string())
    }
}

impl std::fmt::Display for BuildFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Agent(error) => error.fmt(formatter),
            Self::Application(error) => formatter.write_str(error),
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct WorkerShutdown {
    cancellation: Option<Weak<AtomicBool>>,
}

impl WorkerShutdown {
    pub(super) fn watch(&mut self, cancellation: &Arc<AtomicBool>) {
        self.cancellation = Some(Arc::downgrade(cancellation));
    }
}

impl Drop for WorkerShutdown {
    fn drop(&mut self) {
        if let Some(cancellation) = self.cancellation.as_ref().and_then(Weak::upgrade) {
            cancellation.store(true, Ordering::Relaxed);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BuildStage {
    Claim,
    Presentation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ModalKind {
    IdentityChoice,
    StaticResult,
    Running,
    Failed,
    Verdict,
}

#[derive(Debug, Clone)]
pub(super) struct ClaimStageResult {
    pub(super) outcome: CompiledClaimOutcome,
    pub(super) presentation: Option<DynamicPresentationOutcome>,
    pub(super) latency: LatencyMilestones,
}

#[derive(Debug, Clone)]
pub(super) struct IdentityChoice {
    pub(super) request: ReactionBuildRequest,
    pub(super) ambiguity: ReactantIdentityAmbiguity,
}

#[derive(Debug, Clone)]
pub(super) enum Message {
    OverlayDismissed,
    ContextSelected(Option<RequestContext>),
    ToggleDetails,
    IdentitySelected {
        reactant_index: usize,
        species_id: SpeciesId,
    },
    ClaimFinished {
        run_id: u64,
        result: Box<Result<ClaimStageResult, BuildFailure>>,
    },
    PresentationFinished {
        run_id: u64,
        result: Box<Result<DynamicPresentationOutcome, BuildFailure>>,
    },
    BuildTick {
        run_id: u64,
    },
    TheatreTick,
    Regenerate,
    RetryPresentation,
}

#[derive(Debug)]
pub(super) struct State {
    pub(super) claim: Option<ReactionClaim>,
    pub(super) static_outcome: Option<ValidatedStaticOutcome>,
    pub(super) presentation: Option<DynamicPresentationOutcome>,
    pub(super) request: Option<ReactionBuildRequest>,
    pub(super) identity_choice: Option<IdentityChoice>,
    pub(super) context: Option<RequestContext>,
    pub(super) details_open: bool,
    pub(super) build: BuildState,
    pub(super) cancellation: Option<Arc<AtomicBool>>,
    pub(super) worker_shutdown: WorkerShutdown,
    pub(super) progress: Option<CodexProgressEvent>,
    pub(super) progress_receiver: Option<Receiver<CodexProgressEvent>>,
    pub(super) started_at: Option<Instant>,
    pub(super) latency: LatencyMilestones,
    pub(super) theatre_phase: f32,
    pub(super) next_run_id: u64,
    pub(super) overlay_dismissed: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            claim: None,
            static_outcome: None,
            presentation: None,
            request: None,
            identity_choice: None,
            context: None,
            details_open: false,
            build: BuildState::Idle,
            cancellation: None,
            worker_shutdown: WorkerShutdown::default(),
            progress: None,
            progress_receiver: None,
            started_at: None,
            latency: LatencyMilestones::default(),
            theatre_phase: 0.0,
            next_run_id: 1,
            overlay_dismissed: false,
        }
    }
}

impl State {
    pub(super) fn begin_run(&mut self) -> u64 {
        if let Some(cancellation) = self.cancellation.take() {
            cancellation.store(true, Ordering::Relaxed);
        }
        let run_id = self.next_run_id;
        self.next_run_id = self.next_run_id.saturating_add(1);
        self.claim = None;
        self.static_outcome = None;
        self.presentation = None;
        self.identity_choice = None;
        self.details_open = false;
        self.started_at = None;
        self.latency = LatencyMilestones::default();
        self.theatre_phase = 0.0;
        self.progress = None;
        self.progress_receiver = None;
        run_id
    }
}

pub(super) struct ClaimJob {
    pub(super) request: ReactionBuildRequest,
    pub(super) mode: ClaimMode,
    pub(super) local: bool,
    pub(super) regenerate: bool,
    pub(super) config: CodexProviderConfig,
    pub(super) identities: SpeciesRegistry,
    pub(super) catalogue: TrustedCatalogue,
}

pub(super) fn run_claim(job: ClaimJob) -> Result<ClaimStageResult, BuildFailure> {
    let ClaimJob {
        request,
        mode,
        local,
        regenerate,
        config,
        identities,
        catalogue,
    } = job;
    let started = Instant::now();
    let mut latency = LatencyMilestones::default();
    let provider = CodexProvider::new(config);
    // Local Mode never reads the cache: cached claims are model output, and
    // Local Mode is purely programmatic.
    if !regenerate
        && !local
        && let Some(cached) = load_dynamic_cache(
            provider.config().cache_directory.as_deref(),
            &request,
            mode,
            &identities,
            &catalogue,
        )
    {
        let elapsed = elapsed_millis(started);
        latency.static_outcome_ms = Some(elapsed);
        match &cached.presentation {
            Some(DynamicPresentationOutcome::ReviewedFamily(_)) => {
                latency.reviewed_animation_ms = Some(elapsed);
            }
            Some(DynamicPresentationOutcome::Escalated(_)) => {
                latency.mechanism_ms = Some(elapsed);
            }
            Some(DynamicPresentationOutcome::Static { .. }) | None => {}
        }
        return Ok(ClaimStageResult {
            outcome: cached.outcome,
            presentation: cached.presentation,
            latency,
        });
    }
    // Algorithmic solving comes first: deterministic families need no model.
    let solved = agent::solve_reaction_claim(&request, &identities);
    let algorithmic = solved.is_some();
    let claim = match solved {
        Some(claim) => claim,
        None if local => {
            return Err(
                "This reaction isn't supported in Local Mode — ChemSpec couldn't derive it \
                 programmatically. Switch to an AI mode to research it."
                    .to_owned()
                    .into(),
            );
        }
        // The provider uses std::time::Instant; this branch never runs on
        // wasm, where Local Mode is forced.
        None => provider
            .claim_reaction_until(
                &request,
                mode,
                std::time::Instant::now() + FAST_CLAIM_TIMEOUT,
            )
            .map_err(BuildFailure::from)?,
    };
    latency.claim_ms = Some(elapsed_millis(started));
    let outcome =
        compile_claim_outcome(&request, claim.clone(), &identities).map_err(BuildFailure::from)?;
    if matches!(outcome, CompiledClaimOutcome::Static(_)) {
        latency.static_outcome_ms = Some(elapsed_millis(started));
    }
    // Algorithmic claims are recomputed instantly; only model claims are
    // worth caching.
    if !algorithmic && let Some(directory) = provider.config().cache_directory.as_deref() {
        let _ = store_dynamic_cache(
            directory,
            &request,
            mode,
            &identities,
            &catalogue,
            &claim,
            None,
            "codex_subscription",
            provider.model_name(),
        );
    }
    Ok(ClaimStageResult {
        outcome,
        presentation: None,
        latency,
    })
}

pub(super) fn update(app: &mut App, message: Message) -> Task<AppMessage> {
    match message {
        Message::OverlayDismissed => dismiss_overlay(app),
        Message::ContextSelected(context) => select_context(app, context),
        Message::ToggleDetails => toggle_details(app),
        Message::IdentitySelected {
            reactant_index,
            species_id,
        } => return select_identity(app, reactant_index, species_id),
        Message::ClaimFinished { run_id, result } => {
            return finish_claim(app, run_id, *result);
        }
        Message::PresentationFinished { run_id, result } => {
            finish_presentation(app, run_id, *result);
        }
        Message::BuildTick { run_id } => build_tick(app, run_id),
        Message::TheatreTick => theatre_tick(app),
        Message::Regenerate => return regenerate(app),
        Message::RetryPresentation => return retry_presentation(app),
    }
    Task::none()
}

/// Composes every dynamic workflow surface in one modal boundary so progress,
/// identity choices, failures, verdicts, and validated results cannot compete
/// with builder controls.
pub(super) fn overlay(app: &App, size: Size, kind: ModalKind) -> Element<'_, AppMessage> {
    let (chip, chip_color, body) = match kind {
        ModalKind::IdentityChoice => (
            "IDENTITY CHOICE",
            color::WARNING,
            app.dynamic_identity_choice_body(),
        ),
        ModalKind::StaticResult => (
            app.dynamic_trust_label(),
            color::SUCCESS,
            app.dynamic_result_body(),
        ),
        ModalKind::Running => (
            if app.local_mode() {
                "LOCAL DERIVATION"
            } else {
                "CODEX RESEARCH"
            },
            color::ACCENT,
            app.dynamic_running_body(),
        ),
        ModalKind::Failed => ("BUILD FAILED", color::DANGER, app.dynamic_failed_body()),
        ModalKind::Verdict => ("OUTCOME", color::WARNING, app.dynamic_verdict_body()),
    };
    let mut header = row![text(chip).size(type_scale::MICRO).color(chip_color)]
        .spacing(spacing::XS)
        .align_y(Center);
    if app.dynamic.static_outcome.is_some() {
        header = header.push(
            text("VIRTUAL MODEL")
                .size(type_scale::MICRO)
                .color(color::WARNING),
        );
    }
    header = header.push(space().width(Fill)).push(
        button(text("\u{00d7}").size(type_scale::BODY_LARGE))
            .on_press(AppMessage::Dynamic(Message::OverlayDismissed))
            .padding([0.0, spacing::XS])
            .style(theme::secondary_button),
    );
    let panel = mouse_area(
        container(column![header, body].spacing(spacing::SM))
            .style(theme::overlay_panel)
            .padding(spacing::LG)
            .width(Length::Fixed((size.width - 32.0).min(640.0))),
    )
    .on_press(AppMessage::Noop);
    stack![
        mouse_area(
            container(space())
                .style(theme::overlay_scrim)
                .width(Fill)
                .height(Fill),
        )
        .on_press(AppMessage::Dynamic(Message::OverlayDismissed)),
        container(panel).center(Fill),
    ]
    .width(Fill)
    .height(Fill)
    .into()
}

fn dismiss_overlay(app: &mut App) {
    if app.dynamic_modal_kind().is_some() {
        app.dynamic.overlay_dismissed = true;
        app.sync_builder_submit_prompt();
    }
}

fn select_context(app: &mut App, context: Option<RequestContext>) {
    if app.dynamic_modal_kind().is_none() {
        app.dynamic.context = context;
        app.builder_panel = None;
        app.sync_builder_submit_prompt();
    }
}

fn toggle_details(app: &mut App) {
    app.dynamic.details_open = !app.dynamic.details_open;
}

fn select_identity(
    app: &mut App,
    reactant_index: usize,
    species_id: SpeciesId,
) -> Task<AppMessage> {
    let Some(choice) = app.dynamic.identity_choice.take() else {
        return Task::none();
    };
    if choice.ambiguity.reactant_index != reactant_index
        || !choice
            .ambiguity
            .alternatives
            .iter()
            .any(|species| species.id == species_id)
    {
        app.dynamic.identity_choice = Some(choice);
        return Task::none();
    }
    let mut request = choice.request;
    request.reactants[reactant_index].species_id = Some(species_id);
    app.start_dynamic_build_request(request, false)
}

fn finish_claim(
    app: &mut App,
    run_id: u64,
    result: Result<ClaimStageResult, BuildFailure>,
) -> Task<AppMessage> {
    if !matches!(app.dynamic.build, BuildState::Running { run_id: current, stage: BuildStage::Claim, .. } if current == run_id)
    {
        return Task::none();
    }
    app.open_dynamic_overlay();
    match result {
        Ok(ClaimStageResult {
            outcome: CompiledClaimOutcome::Static(outcome),
            presentation,
            latency,
        }) => finish_static_claim(app, run_id, outcome, presentation, latency),
        Ok(ClaimStageResult {
            outcome:
                CompiledClaimOutcome::NoReaction(claim)
                | CompiledClaimOutcome::Ambiguous(claim)
                | CompiledClaimOutcome::Unsupported(claim),
            presentation: _,
            latency,
        }) => {
            app.dynamic.latency = latency;
            app.dynamic.claim = Some(claim);
            app.dynamic.static_outcome = None;
            app.dynamic.presentation = None;
            app.validated_frames = None;
            app.dynamic.build = BuildState::Idle;
            app.dynamic.cancellation = None;
            app.dynamic.progress_receiver = None;
            Task::none()
        }
        Err(error) => {
            app.validated_frames = None;
            app.dynamic.claim = None;
            app.dynamic.static_outcome = None;
            app.dynamic.presentation = None;
            app.dynamic.build = BuildState::Failed(error);
            app.dynamic.cancellation = None;
            app.dynamic.progress_receiver = None;
            Task::none()
        }
    }
}

fn finish_static_claim(
    app: &mut App,
    run_id: u64,
    outcome: ValidatedStaticOutcome,
    presentation: Option<DynamicPresentationOutcome>,
    latency: LatencyMilestones,
) -> Task<AppMessage> {
    app.dynamic.latency = latency;
    app.dynamic.claim = Some(outcome.claim().clone());
    app.dynamic.static_outcome = Some(outcome.clone());
    app.dynamic.presentation = None;
    if let Some(presentation) = presentation {
        app.dynamic.cancellation = None;
        app.dynamic.progress_receiver = None;
        app.finish_dynamic_presentation(presentation);
        return Task::none();
    }
    app.dynamic.build = BuildState::Running {
        run_id,
        elapsed_seconds: 0,
        stage: BuildStage::Presentation,
    };
    let request = app
        .dynamic
        .request
        .clone()
        .expect("a dynamic run retains its request");
    let progress = app.reset_dynamic_progress_channel();
    App::start_dynamic_presentation(
        run_id,
        request,
        ClaimMode::Fast,
        app.local_mode(),
        outcome,
        app.dynamic
            .cancellation
            .clone()
            .expect("a running build retains cancellation"),
        progress,
    )
}

fn finish_presentation(
    app: &mut App,
    run_id: u64,
    result: Result<DynamicPresentationOutcome, BuildFailure>,
) {
    if !matches!(app.dynamic.build, BuildState::Running { run_id: current, stage: BuildStage::Presentation, .. } if current == run_id)
    {
        return;
    }
    app.open_dynamic_overlay();
    match result {
        Ok(presentation) => {
            let elapsed = app.dynamic.started_at.map_or(0, elapsed_millis);
            match &presentation {
                DynamicPresentationOutcome::ReviewedFamily(_) => {
                    app.dynamic.latency.reviewed_animation_ms = Some(elapsed);
                }
                DynamicPresentationOutcome::Escalated(_) => {
                    app.dynamic.latency.mechanism_ms = Some(elapsed);
                }
                DynamicPresentationOutcome::Static { .. } => {}
            }
            app.finish_dynamic_presentation(presentation);
        }
        Err(error) => {
            // Presentation enrichment cannot invalidate or discard an already
            // displayed static outcome.
            app.validated_frames = None;
            app.dynamic.presentation = Some(DynamicPresentationOutcome::Static {
                outcome: Box::new(
                    app.dynamic
                        .static_outcome
                        .clone()
                        .expect("presentation starts only after a static outcome"),
                ),
                diagnostic: error.to_string(),
                retryable: true,
                attempts: 0,
            });
            app.dynamic.build = BuildState::Idle;
            app.dynamic.cancellation = None;
            app.dynamic.progress_receiver = None;
        }
    }
}

fn build_tick(app: &mut App, run_id: u64) {
    let current = matches!(
        app.dynamic.build,
        BuildState::Running { run_id: current, .. } if current == run_id
    );
    if let BuildState::Running {
        run_id: current,
        elapsed_seconds,
        ..
    } = &mut app.dynamic.build
        && *current == run_id
    {
        *elapsed_seconds = elapsed_seconds.saturating_add(1);
    }
    if current {
        app.drain_dynamic_progress();
    }
}

fn theatre_tick(app: &mut App) {
    if matches!(
        app.dynamic.build,
        BuildState::Running {
            stage: BuildStage::Presentation,
            ..
        }
    ) && app.dynamic.static_outcome.is_some()
    {
        app.dynamic.theatre_phase = (app.dynamic.theatre_phase + 0.006).fract();
    }
}

fn regenerate(app: &mut App) -> Task<AppMessage> {
    let Some(request) = app.dynamic.request.clone() else {
        return Task::none();
    };
    app.enter_screen(Screen::Builder);
    app.start_dynamic_build_request(request, true)
}

fn retry_presentation(app: &mut App) -> Task<AppMessage> {
    // Re-run only presentation enrichment; the validated static outcome and
    // claim stay untouched.
    if !matches!(
        app.dynamic.presentation,
        Some(DynamicPresentationOutcome::Static {
            retryable: true,
            ..
        })
    ) || matches!(app.dynamic.build, BuildState::Running { .. })
    {
        return Task::none();
    }
    let (Some(request), Some(outcome)) = (
        app.dynamic.request.clone(),
        app.dynamic.static_outcome.clone(),
    ) else {
        return Task::none();
    };
    let run_id = app.dynamic.next_run_id;
    app.dynamic.next_run_id = app.dynamic.next_run_id.saturating_add(1);
    let cancellation = Arc::new(AtomicBool::new(false));
    app.dynamic.worker_shutdown.watch(&cancellation);
    app.dynamic.cancellation = Some(cancellation.clone());
    app.dynamic.build = BuildState::Running {
        run_id,
        elapsed_seconds: 0,
        stage: BuildStage::Presentation,
    };
    app.open_dynamic_overlay();
    let progress = app.reset_dynamic_progress_channel();
    App::start_dynamic_presentation(
        run_id,
        request,
        ClaimMode::Fast,
        app.local_mode(),
        outcome,
        cancellation,
        progress,
    )
}
