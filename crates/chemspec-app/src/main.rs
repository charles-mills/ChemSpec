//! `ChemSpec` application shell and reaction-builder entry (`U-101`, `U-106`–`U-112`).
//!
//! Opens on the Stage 1 builder: the learner's question, composed from two
//! reactant drafts over the full periodic table. Chemistry is supplied only
//! through the catalogue fast path or a staged dynamic claim whose static and
//! animated capabilities cross separate deterministic validation boundaries.

mod chemistry;
mod composition_catalogue;
mod elements;
mod fonts;
mod icons;
mod nomenclature;
mod particle_visualization;
mod periodic_table;
mod reactant_composer;
mod scene_registry;
mod structural_2d;
mod structural_physics;
mod structural_3d;
mod theme;

use std::{
    ops::Deref,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    time::Instant,
};

use agent::{
    ClaimDisposition, ClaimMode, CodexProgressEvent, CodexProgressStage, CodexProvider,
    CodexProviderConfig, CompiledClaimOutcome, DynamicCachePresentation,
    DynamicPresentationOutcome, FAST_CLAIM_TIMEOUT,
    LatencyMilestones, OutcomeSpecies, RESEARCHER_CLAIM_TIMEOUT, ReactantIdentityAmbiguity,
    ReactantInput, ReactionBuildRequest, ReactionClaim, RequestIdentityResolution, TrustTier,
    ValidatedStaticOutcome, compile_claim_outcome, enrich_static_outcome, load_claim_mode,
    load_dynamic_cache, resolve_request_identities_with_catalogue, reviewed_species_registry,
    store_claim_mode, store_dynamic_cache,
};
use chem_domain::SpeciesId;
use chem_presentation::{
    AppearanceProfile, AssetProfile, EducationalPlan, EducationalSceneKind, EffectProfile,
    PresentationObject, PresentationProfile, PresentationTransform, ScenePlan, SceneRole,
    TimelinePosition, compile_educational_plan, compile_real_world_plan,
};
use iced::widget::{
    button, canvas, column, container, responsive, row, rule, scrollable, slider, space, stack,
    text, text_input,
};
use iced::{Center, Element, Fill, Length, Size, Subscription, Task, Theme};

use theme::{breakpoint, color, space as spacing, type_scale};

fn plan_equation(animation: &StructuralAnimation) -> Option<&str> {
    (!animation.equation.is_empty()).then_some(animation.equation.as_str())
}

fn elapsed_millis(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn reviewed_outcome_choice(
    request: chemistry::ReactionRequest,
    compact: bool,
) -> Element<'static, Message> {
    let labels = column![
        text(request.name())
            .size(type_scale::BODY_LARGE)
            .color(color::TEXT),
        text(nomenclature::display_equation(&request.equation()))
            .size(type_scale::CAPTION)
            .color(color::MUTED),
    ]
    .spacing(spacing::XXS)
    .width(Fill);
    let choice: Element<'static, Message> = if let Some(preview) = request.product_preview() {
        row![
            canvas(
                particle_visualization::CompoundAtomicDiagram::new(preview, 0.0).structure_only()
            )
            .width(Length::Fixed(if compact { 132.0 } else { 176.0 }))
            .height(Length::Fixed(if compact { 84.0 } else { 104.0 })),
            labels,
        ]
        .spacing(spacing::MD)
        .align_y(Center)
        .width(Fill)
        .into()
    } else {
        labels.into()
    };
    button(choice)
        .on_press(Message::OutcomeSelected(request))
        .padding(spacing::MD)
        .width(Fill)
        .style(theme::secondary_button)
        .into()
}

fn dynamic_species_theatre_card(
    species: &OutcomeSpecies,
    term: &chem_domain::ReactionTerm,
    phase: f32,
) -> Element<'static, Message> {
    let acid_sites = species
        .bronsted_acid_profile()
        .map_or(0, |profile| profile.proton_donor_sites().len());
    let (model, capability): (Element<'static, Message>, &'static str) = match species {
        OutcomeSpecies::Resolved(species) => species
            .structure
            .as_ref()
            .and_then(|structure| {
                composition_catalogue::preview_from_validated_structure(
                    structure,
                    term.formula_text(),
                )
            })
            .map_or_else(
                || (formula_inventory_theatre(term, phase), "FORMULA INVENTORY"),
                |preview| {
                    (
                        canvas(particle_visualization::CompoundAtomicDiagram::new(
                            preview, phase,
                        ))
                        .width(Fill)
                        .height(Length::Fixed(76.0))
                        .into(),
                        "VALIDATED GRAPH",
                    )
                },
            ),
        OutcomeSpecies::FormulaOnly { .. } => {
            (formula_inventory_theatre(term, phase), "FORMULA INVENTORY")
        }
    };
    let capability = if acid_sites == 0 {
        capability.to_owned()
    } else {
        format!("{capability} · {acid_sites} PROTON-DONOR SITE(S)")
    };
    container(
        column![
            model,
            text(nomenclature::display_equation(term.formula_text()))
                .size(type_scale::CAPTION)
                .color(color::TEXT),
            text(capability).size(type_scale::MICRO).color(color::MUTED),
        ]
        .spacing(spacing::XXS)
        .align_x(Center),
    )
    .style(theme::frame)
    .padding(spacing::XXS)
    .width(Fill)
    .into()
}

fn formula_inventory_theatre(
    term: &chem_domain::ReactionTerm,
    phase: f32,
) -> Element<'static, Message> {
    let mut models = row![].spacing(spacing::XXS).align_y(Center);
    for (symbol, count) in term.formula().elements() {
        let Some(element) = elements::SUPPORTED
            .iter()
            .find(|candidate| candidate.symbol == symbol.as_str())
            .copied()
        else {
            continue;
        };
        models = models.push(
            column![
                canvas(particle_visualization::AtomDiagram::new(element, phase))
                    .width(Length::Fixed(44.0))
                    .height(Length::Fixed(44.0)),
                text(format!("{} ×{count}", element.symbol))
                    .size(type_scale::MICRO)
                    .color(color::TEXT_SOFT),
            ]
            .spacing(spacing::XXS)
            .align_x(Center),
        );
    }
    container(models)
        .width(Fill)
        .height(Length::Fixed(76.0))
        .center(Fill)
        .into()
}

#[allow(clippy::cast_precision_loss)]
fn educational_timeline_progress(animation: &StructuralAnimation) -> f32 {
    let total = animation.educational_plan.duration_ms().max(1);
    (animation.educational_playhead_ms.min(total) as f32 / total as f32).clamp(0.0, 1.0)
}

fn format_media_time(milliseconds: u64) -> String {
    let seconds = milliseconds / 1_000;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn sync_educational_frame(animation: &mut StructuralAnimation) {
    let Some(position) = animation
        .educational_plan
        .locate(animation.educational_playhead_ms)
    else {
        return;
    };
    let Some(scene) = animation.educational_plan.scenes.get(position.scene_index) else {
        return;
    };
    if let Some(frame_index) = animation
        .frames
        .frames()
        .iter()
        .position(|frame| frame.trace().state_digest == scene.end_frame)
    {
        animation.frame_index = frame_index;
    }
}

const fn macroscopic_effect_label(effect: EffectProfile) -> &'static str {
    match effect {
        EffectProfile::BubbleEmitter => "Interface bubbles",
        EffectProfile::GasRelease => "Gas release",
        EffectProfile::SurfaceDisturbance => "Surface motion",
        EffectProfile::ObjectShrinkage => "Reactant consumption",
        EffectProfile::PrecipitateFormation => "Precipitate formation",
        EffectProfile::Clouding => "Solution clouding",
        EffectProfile::ColourTransition => "Colour transition",
        EffectProfile::SplashEmitter => "Fine droplets",
        EffectProfile::HeatDistortion => "Heat distortion",
    }
}

const fn educational_scene_title(kind: EducationalSceneKind) -> &'static str {
    match kind {
        EducationalSceneKind::Introduction => "Introduce",
        EducationalSceneKind::ReactantSetup => "Reactants",
        EducationalSceneKind::Equation => "Equation",
        EducationalSceneKind::StructuralChange => "Explain change",
        EducationalSceneKind::ExplanationPause => "Understand",
        EducationalSceneKind::ObservationConnection => "Observe",
        EducationalSceneKind::Summary => "Summarise",
    }
}

/// The window size the interface's fixed pixel tokens were designed against;
/// larger windows zoom the whole interface up instead of stretching layouts.
const DESIGN_SIZE: Size = Size::new(1_440.0, 900.0);
/// Upper bound on the adaptive zoom so very large monitors stay reasonable.
const MAX_UI_ZOOM: f32 = 2.0;

fn main() -> iced::Result {
    let arguments = std::env::args().collect::<Vec<_>>();
    if let Some(validation) = arguments
        .iter()
        .find_map(|argument| validate_smoke_request_from_argument(argument))
    {
        if let Err(error) = validation {
            eprintln!("{error}");
            std::process::exit(2);
        }
        return Ok(());
    }
    if let Err(error) = validate_structural_smoke_arguments(arguments.iter().map(String::as_str)) {
        eprintln!("{error}");
        std::process::exit(2);
    }
    iced::application(launch_state, App::update_with_task, App::view)
        .title(App::title)
        .subscription(App::subscription)
        .theme(App::theme)
        .font(fonts::INTER_REGULAR_BYTES)
        .font(fonts::INTER_MEDIUM_BYTES)
        .font(fonts::INTER_SEMIBOLD_BYTES)
        .font(fonts::INTER_BOLD_BYTES)
        .default_font(fonts::REGULAR)
        .scale_factor(|app| app.ui_zoom)
        .window(iced::window::Settings {
            size: DESIGN_SIZE,
            min_size: Some(Size::new(560.0, 760.0)),
            position: iced::window::Position::Centered,
            ..iced::window::Settings::default()
        })
        .run()
}

/// Resize events report sizes already divided by the active zoom, so the new
/// factor is computed from zoom-invariant design units. Windows at or below
/// the design size keep the 1:1 layout.
fn adaptive_zoom(reported: Size, current_zoom: f32) -> f32 {
    let width = reported.width * current_zoom;
    let height = reported.height * current_zoom;
    (width / DESIGN_SIZE.width)
        .min(height / DESIGN_SIZE.height)
        .clamp(1.0, MAX_UI_ZOOM)
}

fn codex_available() -> bool {
    CodexProvider::new(CodexProviderConfig::from_environment())
        .preflight()
        .is_ok_and(|preflight| preflight.authenticated)
}

fn dynamic_presentation_profile(
    _frames: &chem_kernel::SimulationFrames,
    equation: &str,
) -> PresentationProfile {
    let transform = |translation, scale| PresentationTransform {
        translation,
        rotation: [0, 0, 0],
        scale,
    };
    PresentationProfile {
        id: "dynamic-validated".to_owned(),
        environment: AssetProfile::LaboratoryBench,
        objects: vec![
            PresentationObject {
                id: "vessel".to_owned(),
                asset: AssetProfile::Beaker,
                semantic_identity: "virtual reaction vessel".to_owned(),
                appearance: AppearanceProfile::ClearGlass,
                role: SceneRole::Vessel,
                transform: transform([0, 0, 0], [1_100, 1_100, 1_100]),
                visible_from_ordinal: 0,
                observation: None,
            },
            PresentationObject {
                id: "contents".to_owned(),
                asset: AssetProfile::LiquidVolume,
                semantic_identity: "representative reaction contents".to_owned(),
                appearance: AppearanceProfile::AqueousColourless,
                role: SceneRole::Contents,
                transform: transform([0, -150, 0], [1_000, 850, 1_000]),
                visible_from_ordinal: 0,
                observation: None,
            },
        ],
        effects: Vec::new(),
        camera: Vec::new(),
        equation: equation.to_owned(),
        disclosure: "Representative virtual presentation.".to_owned(),
    }
}

fn launch_state() -> App {
    let mut app = App::default();
    let smoke_mode = std::env::args().find_map(|argument| SmokeMode::from_argument(&argument));
    let smoke_request =
        std::env::args().find_map(|argument| smoke_request_from_argument(&argument));
    if let Some(smoke_mode) = smoke_mode {
        app.smoke_mode = Some(smoke_mode);
        if smoke_mode == SmokeMode::Builder {
            app.screen = Screen::Builder;
            return app;
        }
        if let Some(request) = smoke_request {
            match request {
                Ok(request) => app.select_request(request),
                Err(error) => {
                    app.screen = Screen::Builder;
                    app.structural_error = Some(error);
                    return app;
                }
            }
        }
        app.open_structural_animation();
        if let Some(animation) = &mut app.structural_animation {
            let three_dimensional = smoke_mode == SmokeMode::Structural3d;
            animation.frame_index = 1.min(animation.frames.frames().len().saturating_sub(1));
            if three_dimensional {
                let plan = &animation.real_world_plan;
                animation.real_world_playhead_ms =
                    plan.timeline.duration_ms().saturating_mul(2) / 3;
                if let Some(position) = plan.timeline.locate(animation.real_world_playhead_ms)
                    && let Some(frame_index) = animation
                        .frames
                        .frames()
                        .iter()
                        .position(|frame| frame.ordinal() == u32::from(position.ordinal))
                {
                    animation.frame_index = frame_index;
                }
            } else {
                let scene_index = animation
                    .educational_plan
                    .scenes
                    .iter()
                    .position(|scene| scene.kind == EducationalSceneKind::StructuralChange)
                    .unwrap_or(0);
                if let Some(scene) = animation.educational_plan.scenes.get(scene_index) {
                    animation.educational_playhead_ms = animation
                        .educational_plan
                        .elapsed_at(TimelinePosition {
                            scene_index,
                            scene_elapsed_ms: scene.duration_ms / 2,
                        })
                        .unwrap_or(0);
                }
            }
            animation.playing = false;
            app.screen = if three_dimensional {
                Screen::Structural3d
            } else {
                Screen::Structural2d
            };
        }
    }
    app
}

fn smoke_request_from_argument(
    argument: &str,
) -> Option<Result<chemistry::ReactionRequest, String>> {
    argument
        .strip_prefix("--smoke-reaction=")
        .map(smoke_request_from_id)
}

fn validate_smoke_request_from_argument(
    argument: &str,
) -> Option<Result<chemistry::ReactionRequest, String>> {
    argument
        .strip_prefix("--validate-smoke-reaction=")
        .map(smoke_request_from_id)
}

fn smoke_request_from_id(id: &str) -> Result<chemistry::ReactionRequest, String> {
    chemistry::ReactionRequest::from_id(id)
        .ok_or_else(|| format!("unsupported smoke reaction `{id}`"))
}

fn validate_structural_smoke_arguments<'a>(
    arguments: impl IntoIterator<Item = &'a str>,
) -> Result<(), String> {
    let arguments = arguments.into_iter().collect::<Vec<_>>();
    let structural_smoke = arguments.iter().any(|argument| {
        matches!(
            SmokeMode::from_argument(argument),
            Some(SmokeMode::Structural2d | SmokeMode::Structural3d)
        )
    });
    if !structural_smoke {
        return Ok(());
    }
    arguments
        .iter()
        .find_map(|argument| smoke_request_from_argument(argument))
        .transpose()
        .map(|_| ())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
    ProviderSetup,
    Builder,
    OutcomeChoice,
    Structural2d,
    Structural3d,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SmokeMode {
    Builder,
    Structural2d,
    Structural3d,
}

impl SmokeMode {
    fn from_argument(argument: &str) -> Option<Self> {
        match argument {
            "--builder-smoke" => Some(Self::Builder),
            "--structural-2d-smoke" | "--lithium-2d-smoke" => Some(Self::Structural2d),
            "--structural-3d-smoke" | "--lithium-3d-smoke" => Some(Self::Structural3d),
            _ => None,
        }
    }

    const fn title(self) -> &'static str {
        match self {
            Self::Builder => "Builder",
            Self::Structural2d => "Structural 2D",
            Self::Structural3d => "Structural 3D",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProviderChoice {
    Local,
    CodexSubscription,
    ApiKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlaybackSpeed {
    Half,
    Normal,
    OneAndHalf,
}

impl PlaybackSpeed {
    const fn label(self) -> &'static str {
        match self {
            Self::Half => "0.5×",
            Self::Normal => "1×",
            Self::OneAndHalf => "1.5×",
        }
    }

    const fn next(self) -> Self {
        match self {
            Self::Half => Self::Normal,
            Self::Normal => Self::OneAndHalf,
            Self::OneAndHalf => Self::Half,
        }
    }

    const fn scale_millis(self, milliseconds: u32) -> u32 {
        match self {
            Self::Half => milliseconds / 2,
            Self::Normal => milliseconds,
            Self::OneAndHalf => milliseconds + milliseconds / 2,
        }
    }
}

#[derive(Debug, Clone)]
enum Message {
    WindowResized(Size),
    KeyboardEvent(iced::keyboard::Event),
    ScreenSelected(Screen),
    ProviderSelected(ProviderChoice),
    ApiKeyChanged(String),
    ProviderContinue,
    PeriodicTable(periodic_table::Message),
    ReactantComposer(reactant_composer::Message),
    ClaimModeSelected(ClaimMode),
    DynamicContextSelected(Option<DynamicRequestContext>),
    StartContextReaction,
    ToggleDynamicDetails,
    DynamicIdentitySelected {
        reactant_index: usize,
        species_id: SpeciesId,
    },
    CancelDynamicWork,
    DynamicClaimFinished {
        run_id: u64,
        result: Box<Result<DynamicClaimStageResult, String>>,
    },
    DynamicPresentationFinished {
        run_id: u64,
        result: Box<Result<DynamicPresentationOutcome, String>>,
    },
    DynamicBuildTick {
        run_id: u64,
    },
    DynamicTheatreTick,
    RegenerateDynamicReaction,
    RetryDynamicPresentation,
    OutcomeSelected(chemistry::ReactionRequest),
    StructuralPlaybackToggled,
    StructuralSpeedChanged,
    StructuralTimelineScrubbed(u32),
    StructuralRealWorldTimelineScrubbed(u32),
    StructuralChapterChanged(i8),
    StructuralRestarted,
    StructuralTick,
    StructuralDrag(structural_2d::DragEvent),
    ContinueTo3d,
    ReturnTo2d,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DynamicRequestContext {
    Heat,
    Light,
    Electricity,
}

impl DynamicRequestContext {
    const ALL: [Self; 3] = [Self::Heat, Self::Light, Self::Electricity];

    const fn value(self) -> &'static str {
        match self {
            Self::Heat => "heat",
            Self::Light => "light",
            Self::Electricity => "electricity",
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Heat => "Heat",
            Self::Light => "Light",
            Self::Electricity => "Electricity",
        }
    }
}

fn builder_keyboard_message(screen: Screen, event: iced::keyboard::Event) -> Option<Message> {
    let iced::keyboard::Event::KeyPressed { key, modifiers, .. } = event else {
        return None;
    };
    builder_shortcut(screen, &key, modifiers)
}

fn builder_shortcut(
    screen: Screen,
    key: &iced::keyboard::Key,
    modifiers: iced::keyboard::Modifiers,
) -> Option<Message> {
    use iced::keyboard::{Key, key::Named};

    if screen != Screen::Builder {
        return None;
    }
    if key == &Key::Named(Named::Escape) {
        return Some(Message::CancelDynamicWork);
    }
    if !modifiers.command() {
        return None;
    }
    match key.as_ref() {
        Key::Character("1") => Some(Message::ReactantComposer(
            reactant_composer::Message::SelectReactant(reactant_composer::ActiveReactant::First),
        )),
        Key::Character("2") => Some(Message::ReactantComposer(
            reactant_composer::Message::SelectReactant(reactant_composer::ActiveReactant::Second),
        )),
        Key::Character(value) if value.eq_ignore_ascii_case("z") => {
            Some(Message::ReactantComposer(reactant_composer::Message::Undo))
        }
        Key::Named(Named::Backspace) => Some(Message::ReactantComposer(
            reactant_composer::Message::ClearActive,
        )),
        Key::Named(Named::Enter) => Some(Message::ReactantComposer(
            reactant_composer::Message::StartReactionRequested,
        )),
        _ => None,
    }
}

#[derive(Debug, Clone)]
enum RenderableFrames {
    Catalogue(chem_kernel::SimulationFrames),
    Dynamic(chem_kernel::ValidatedDynamicFrames),
}

impl Deref for RenderableFrames {
    type Target = chem_kernel::SimulationFrames;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Catalogue(frames) => frames,
            Self::Dynamic(frames) => frames,
        }
    }
}

#[derive(Debug, Clone, Default)]
enum DynamicBuildState {
    #[default]
    Idle,
    Running {
        run_id: u64,
        elapsed_seconds: u64,
        stage: DynamicBuildStage,
    },
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DynamicBuildStage {
    Claim,
    Presentation,
}

#[derive(Debug, Clone)]
struct DynamicClaimStageResult {
    outcome: CompiledClaimOutcome,
    presentation: Option<DynamicPresentationOutcome>,
    latency: LatencyMilestones,
}

#[derive(Debug, Clone)]
struct DynamicIdentityChoice {
    request: ReactionBuildRequest,
    ambiguity: ReactantIdentityAmbiguity,
}

#[derive(Debug)]
struct StructuralAnimation {
    frames: RenderableFrames,
    educational_plan: EducationalPlan,
    real_world_plan: ScenePlan,
    reactant_previews: Vec<composition_catalogue::TrustedCompositionPreview>,
    product_preview: Option<composition_catalogue::TrustedCompositionPreview>,
    equation: String,
    educational_playhead_ms: u64,
    frame_index: usize,
    real_world_playhead_ms: u64,
    playing: bool,
    playback_speed: PlaybackSpeed,
    physics: structural_physics::Simulation,
}

struct App {
    screen: Screen,
    smoke_mode: Option<SmokeMode>,
    codex_available: bool,
    provider: Option<ProviderChoice>,
    api_key: String,
    periodic_table: periodic_table::State,
    reactant_composer: reactant_composer::State,
    pending_requests: Vec<chemistry::ReactionRequest>,
    oxygen_assessment: Option<chemistry::OxygenAssessment>,
    active_request: chemistry::ReactionRequest,
    validated_frames: Option<RenderableFrames>,
    dynamic_claim: Option<ReactionClaim>,
    dynamic_static: Option<ValidatedStaticOutcome>,
    dynamic_presentation: Option<DynamicPresentationOutcome>,
    dynamic_request: Option<ReactionBuildRequest>,
    dynamic_identity_choice: Option<DynamicIdentityChoice>,
    dynamic_context: Option<DynamicRequestContext>,
    dynamic_details_open: bool,
    claim_mode: ClaimMode,
    dynamic_build: DynamicBuildState,
    dynamic_cancellation: Option<Arc<AtomicBool>>,
    dynamic_progress: Option<CodexProgressEvent>,
    dynamic_progress_receiver: Option<Receiver<CodexProgressEvent>>,
    dynamic_started_at: Option<Instant>,
    dynamic_latency: LatencyMilestones,
    dynamic_theatre_phase: f32,
    next_dynamic_run_id: u64,
    structural_animation: Option<StructuralAnimation>,
    structural_error: Option<String>,
    /// Interface zoom applied on top of the system scale factor.
    ui_zoom: f32,
}

impl Default for App {
    fn default() -> Self {
        let codex_available = codex_available();
        let active_request = chemistry::ReactionRequest::DEFAULT;
        let provider_config = CodexProviderConfig::from_environment();
        Self {
            screen: Screen::ProviderSetup,
            smoke_mode: None,
            codex_available,
            provider: Some(ProviderChoice::Local),
            api_key: String::new(),
            periodic_table: periodic_table::State::default(),
            reactant_composer: reactant_composer::State::default(),
            pending_requests: Vec::new(),
            oxygen_assessment: None,
            active_request,
            validated_frames: chemistry::run(active_request)
                .ok()
                .map(|run| RenderableFrames::Catalogue(run.frames().clone())),
            dynamic_claim: None,
            dynamic_static: None,
            dynamic_presentation: None,
            dynamic_request: None,
            dynamic_identity_choice: None,
            dynamic_context: None,
            dynamic_details_open: false,
            claim_mode: load_claim_mode(provider_config.cache_directory.as_deref()),
            dynamic_build: DynamicBuildState::Idle,
            dynamic_cancellation: None,
            dynamic_progress: None,
            dynamic_progress_receiver: None,
            dynamic_started_at: None,
            dynamic_latency: LatencyMilestones::default(),
            dynamic_theatre_phase: 0.0,
            next_dynamic_run_id: 1,
            structural_animation: None,
            structural_error: None,
            ui_zoom: 1.0,
        }
    }
}

#[cfg(test)]
fn api_key_format_is_valid(api_key: &str) -> bool {
    let Some(secret) = api_key.strip_prefix("sk-") else {
        return false;
    };

    (20..=256).contains(&api_key.len())
        && secret
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

impl App {
    /// Local Mode is a state for the whole app: no model integration, no
    /// model-facing copy, and anything only a model could do is unsupported.
    fn local_mode(&self) -> bool {
        self.provider == Some(ProviderChoice::Local)
    }

    fn title(&self) -> String {
        let base = self.smoke_mode.map_or_else(
            || "ChemSpec — reaction builder".to_owned(),
            |mode| format!("ChemSpec Agent Smoke — {}", mode.title()),
        );
        self.builder_accessibility_summary()
            .map_or(base.clone(), |summary| format!("{base} — {summary}"))
    }

    fn builder_accessibility_summary(&self) -> Option<String> {
        if self.screen != Screen::Builder {
            return None;
        }
        let (first, second) = reactant_composer::reactants(&self.reactant_composer);
        if first.is_empty() && second.is_empty() && self.dynamic_static.is_none() {
            return None;
        }
        let first = if first.is_empty() {
            "empty".to_owned()
        } else {
            reactant_composer::formula(first)
        };
        let second = if second.is_empty() {
            "empty".to_owned()
        } else {
            reactant_composer::formula(second)
        };
        let reactants = format!("Reactants {first} + {second}");
        let state = if let Some(outcome) = &self.dynamic_static {
            let capability = match &self.dynamic_presentation {
                Some(
                    DynamicPresentationOutcome::ReviewedFamily(_)
                    | DynamicPresentationOutcome::Escalated(_),
                ) => "animation ready",
                Some(DynamicPresentationOutcome::Static { .. }) => "static result only",
                None => "static result ready",
            };
            format!("{}; {capability}", outcome.equation())
        } else {
            match &self.dynamic_build {
                DynamicBuildState::Idle => "idle".to_owned(),
                DynamicBuildState::Running { stage, .. } => match stage {
                    DynamicBuildStage::Claim => "building factual outcome".to_owned(),
                    DynamicBuildStage::Presentation => "building presentation".to_owned(),
                },
                DynamicBuildState::Failed(_) => "build failed".to_owned(),
            }
        };
        Some(format!("{reactants}; {state}"))
    }

    #[allow(clippy::too_many_lines)]
    fn update_with_task(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::WindowResized(size) => self.ui_zoom = adaptive_zoom(size, self.ui_zoom),
            Message::KeyboardEvent(event) => {
                if let Some(message) = builder_keyboard_message(self.screen, event) {
                    return self.update_with_task(message);
                }
            }
            Message::ScreenSelected(screen) => self.screen = screen,
            Message::ProviderSelected(provider) => self.provider = Some(provider),
            Message::ApiKeyChanged(api_key) => self.api_key = api_key,
            Message::ProviderContinue => {
                let ready = match self.provider {
                    Some(ProviderChoice::Local) => true,
                    Some(ProviderChoice::CodexSubscription) => self.codex_available,
                    Some(ProviderChoice::ApiKey) | None => false,
                };
                if ready {
                    self.screen = Screen::Builder;
                }
            }
            Message::PeriodicTable(message) => {
                periodic_table::update(&mut self.periodic_table, message);
                if let periodic_table::Message::Activated(atomic_number) = message {
                    reactant_composer::update(
                        &mut self.reactant_composer,
                        reactant_composer::Message::AddElement(atomic_number),
                    );
                }
            }
            Message::ReactantComposer(message) => {
                return self.update_reactant_composer(message);
            }
            Message::ClaimModeSelected(mode) => {
                if matches!(self.dynamic_build, DynamicBuildState::Running { .. }) {
                    return Task::none();
                }
                self.claim_mode = mode;
                if let Some(directory) = CodexProviderConfig::from_environment()
                    .cache_directory
                    .as_deref()
                {
                    let _ = store_claim_mode(directory, mode);
                }
            }
            Message::DynamicContextSelected(context) => self.dynamic_context = context,
            Message::StartContextReaction => return self.start_dynamic_build(),
            Message::ToggleDynamicDetails => {
                self.dynamic_details_open = !self.dynamic_details_open;
            }
            Message::DynamicIdentitySelected {
                reactant_index,
                species_id,
            } => {
                let Some(choice) = self.dynamic_identity_choice.take() else {
                    return Task::none();
                };
                if choice.ambiguity.reactant_index != reactant_index
                    || !choice
                        .ambiguity
                        .alternatives
                        .iter()
                        .any(|species| species.id == species_id)
                {
                    self.dynamic_identity_choice = Some(choice);
                    return Task::none();
                }
                let mut request = choice.request;
                request.reactants[reactant_index].species_id = Some(species_id);
                return self.start_dynamic_build_request(request, false);
            }
            Message::CancelDynamicWork => {
                if let Some(cancellation) = self.dynamic_cancellation.take() {
                    cancellation.store(true, Ordering::Relaxed);
                }
                self.next_dynamic_run_id = self.next_dynamic_run_id.saturating_add(1);
                self.dynamic_identity_choice = None;
                if matches!(
                    self.dynamic_build,
                    DynamicBuildState::Running {
                        stage: DynamicBuildStage::Presentation,
                        ..
                    }
                ) && let Some(outcome) = self.dynamic_static.clone()
                {
                    self.dynamic_presentation = Some(DynamicPresentationOutcome::Static {
                        outcome: Box::new(outcome),
                        diagnostic: "Animation enrichment was cancelled".into(),
                        retryable: true,
                        attempts: 0,
                    });
                    self.dynamic_build = DynamicBuildState::Idle;
                } else if matches!(self.dynamic_build, DynamicBuildState::Running { .. }) {
                    self.dynamic_build =
                        DynamicBuildState::Failed("Cancelled by the learner".into());
                }
                self.dynamic_progress_receiver = None;
            }
            Message::DynamicClaimFinished { run_id, result } => {
                if !matches!(self.dynamic_build, DynamicBuildState::Running { run_id: current, stage: DynamicBuildStage::Claim, .. } if current == run_id)
                {
                    return Task::none();
                }
                match *result {
                    Ok(DynamicClaimStageResult {
                        outcome: CompiledClaimOutcome::Static(outcome),
                        presentation,
                        latency,
                    }) => {
                        self.dynamic_latency = latency;
                        self.dynamic_claim = Some(outcome.claim().clone());
                        self.dynamic_static = Some(outcome.clone());
                        self.dynamic_presentation = None;
                        if let Some(presentation) = presentation {
                            self.dynamic_cancellation = None;
                            self.dynamic_progress_receiver = None;
                            self.finish_dynamic_presentation(presentation);
                            return Task::none();
                        }
                        self.dynamic_build = DynamicBuildState::Running {
                            run_id,
                            elapsed_seconds: 0,
                            stage: DynamicBuildStage::Presentation,
                        };
                        let request = self
                            .dynamic_request
                            .clone()
                            .expect("a dynamic run retains its request");
                        let progress = self.reset_dynamic_progress_channel();
                        return Self::start_dynamic_presentation(
                            run_id,
                            request,
                            self.claim_mode,
                            self.local_mode(),
                            outcome,
                            self.dynamic_cancellation
                                .clone()
                                .expect("a running build retains cancellation"),
                            progress,
                        );
                    }
                    Ok(DynamicClaimStageResult {
                        outcome:
                            CompiledClaimOutcome::NoReaction(claim)
                            | CompiledClaimOutcome::Ambiguous(claim)
                            | CompiledClaimOutcome::Unsupported(claim),
                        presentation: _,
                        latency,
                    }) => {
                        self.dynamic_latency = latency;
                        self.dynamic_claim = Some(claim);
                        self.dynamic_static = None;
                        self.dynamic_presentation = None;
                        self.validated_frames = None;
                        self.dynamic_build = DynamicBuildState::Idle;
                        self.dynamic_cancellation = None;
                        self.dynamic_progress_receiver = None;
                    }
                    Err(error) => {
                        self.validated_frames = None;
                        self.dynamic_claim = None;
                        self.dynamic_static = None;
                        self.dynamic_presentation = None;
                        self.dynamic_build = DynamicBuildState::Failed(error);
                        self.dynamic_cancellation = None;
                        self.dynamic_progress_receiver = None;
                    }
                }
            }
            Message::DynamicPresentationFinished { run_id, result } => {
                if !matches!(self.dynamic_build, DynamicBuildState::Running { run_id: current, stage: DynamicBuildStage::Presentation, .. } if current == run_id)
                {
                    return Task::none();
                }
                match *result {
                    Ok(presentation) => {
                        let elapsed = self.dynamic_started_at.map_or(0, elapsed_millis);
                        match &presentation {
                            DynamicPresentationOutcome::ReviewedFamily(_) => {
                                self.dynamic_latency.reviewed_animation_ms = Some(elapsed);
                            }
                            DynamicPresentationOutcome::Escalated(_) => {
                                self.dynamic_latency.mechanism_ms = Some(elapsed);
                            }
                            DynamicPresentationOutcome::Static { .. } => {}
                        }
                        self.finish_dynamic_presentation(presentation);
                    }
                    Err(error) => {
                        // Presentation enrichment cannot invalidate or discard
                        // an already displayed static outcome.
                        self.validated_frames = None;
                        self.dynamic_presentation = Some(DynamicPresentationOutcome::Static {
                            outcome: Box::new(
                                self.dynamic_static
                                    .clone()
                                    .expect("presentation starts only after a static outcome"),
                            ),
                            diagnostic: error,
                            retryable: true,
                            attempts: 0,
                        });
                        self.dynamic_build = DynamicBuildState::Idle;
                        self.dynamic_cancellation = None;
                        self.dynamic_progress_receiver = None;
                    }
                }
            }
            Message::DynamicBuildTick { run_id } => {
                let current = matches!(
                    self.dynamic_build,
                    DynamicBuildState::Running { run_id: current, .. } if current == run_id
                );
                if let DynamicBuildState::Running {
                    run_id: current,
                    elapsed_seconds,
                    ..
                } = &mut self.dynamic_build
                    && *current == run_id
                {
                    *elapsed_seconds = elapsed_seconds.saturating_add(1);
                }
                if current {
                    self.drain_dynamic_progress();
                }
            }
            Message::DynamicTheatreTick => {
                if matches!(
                    self.dynamic_build,
                    DynamicBuildState::Running {
                        stage: DynamicBuildStage::Presentation,
                        ..
                    }
                ) && self.dynamic_static.is_some()
                {
                    self.dynamic_theatre_phase = (self.dynamic_theatre_phase + 0.006).fract();
                }
            }
            Message::RegenerateDynamicReaction => {
                let Some(request) = self.dynamic_request.clone() else {
                    return Task::none();
                };
                self.screen = Screen::Builder;
                return self.start_dynamic_build_request(request, true);
            }
            Message::RetryDynamicPresentation => {
                // Re-run only the presentation enrichment (structure and
                // mechanism escalation); the validated static outcome and its
                // claim stay untouched.
                if !matches!(
                    self.dynamic_presentation,
                    Some(DynamicPresentationOutcome::Static {
                        retryable: true,
                        ..
                    })
                ) || matches!(self.dynamic_build, DynamicBuildState::Running { .. })
                {
                    return Task::none();
                }
                let (Some(request), Some(outcome)) =
                    (self.dynamic_request.clone(), self.dynamic_static.clone())
                else {
                    return Task::none();
                };
                let run_id = self.next_dynamic_run_id;
                self.next_dynamic_run_id = self.next_dynamic_run_id.saturating_add(1);
                let cancellation = Arc::new(AtomicBool::new(false));
                self.dynamic_cancellation = Some(cancellation.clone());
                self.dynamic_build = DynamicBuildState::Running {
                    run_id,
                    elapsed_seconds: 0,
                    stage: DynamicBuildStage::Presentation,
                };
                let progress = self.reset_dynamic_progress_channel();
                return Self::start_dynamic_presentation(
                    run_id,
                    request,
                    self.claim_mode,
                    self.local_mode(),
                    outcome,
                    cancellation,
                    progress,
                );
            }
            Message::OutcomeSelected(request) => {
                self.pending_requests.clear();
                self.oxygen_assessment = None;
                self.select_request(request);
                self.open_structural_animation();
            }
            Message::StructuralPlaybackToggled => {
                if let Some(animation) = &mut self.structural_animation {
                    animation.playing = !animation.playing;
                }
            }
            Message::StructuralSpeedChanged => {
                if let Some(animation) = &mut self.structural_animation {
                    animation.playback_speed = animation.playback_speed.next();
                }
            }
            Message::StructuralTimelineScrubbed(progress) => {
                if let Some(animation) = &mut self.structural_animation {
                    animation.playing = false;
                }
                self.seek_educational_timeline(u64::from(progress));
            }
            Message::StructuralRealWorldTimelineScrubbed(progress) => {
                if let Some(animation) = &mut self.structural_animation {
                    animation.playing = false;
                }
                self.seek_real_world_timeline(u64::from(progress));
            }
            Message::StructuralChapterChanged(delta) => self.change_structural_frame(delta),
            Message::StructuralTick => {
                let (elapsed, playing) = self
                    .structural_animation
                    .as_ref()
                    .map_or((33, false), |animation| {
                        (animation.playback_speed.scale_millis(33), animation.playing)
                    });
                if self.screen == Screen::Structural3d {
                    self.advance_real_world_playback(elapsed);
                } else {
                    if playing {
                        self.advance_educational_playback(elapsed);
                    }
                    self.step_structural_physics();
                }
            }
            Message::StructuralDrag(event) => {
                if let Some(animation) = &mut self.structural_animation {
                    match event {
                        structural_2d::DragEvent::Started { target, cursor } => {
                            animation.physics.begin_drag(&target, cursor);
                        }
                        structural_2d::DragEvent::Moved { cursor } => {
                            animation.physics.move_drag(cursor);
                        }
                        structural_2d::DragEvent::Ended => {
                            animation.physics.end_drag();
                        }
                    }
                }
            }
            Message::StructuralRestarted => {
                if let Some(animation) = &mut self.structural_animation {
                    animation.educational_playhead_ms = 0;
                    animation.frame_index = 0;
                    animation.real_world_playhead_ms = 0;
                    animation.playing = true;
                }
            }
            Message::ContinueTo3d => {
                if self.structural_animation.as_ref().is_some_and(|animation| {
                    animation
                        .educational_plan
                        .locate(animation.educational_playhead_ms)
                        .is_some_and(|position| {
                            position.scene_index + 1 == animation.educational_plan.scenes.len()
                        })
                }) {
                    if let Some(animation) = &mut self.structural_animation {
                        animation.frame_index = 0;
                        animation.real_world_playhead_ms = 0;
                        animation.playing = true;
                    }
                    self.screen = Screen::Structural3d;
                }
            }
            Message::ReturnTo2d => self.screen = Screen::Structural2d,
        }
        Task::none()
    }

    #[cfg(test)]
    fn update(&mut self, message: Message) {
        drop(self.update_with_task(message));
    }

    fn update_reactant_composer(&mut self, message: reactant_composer::Message) -> Task<Message> {
        if matches!(message, reactant_composer::Message::StartReactionRequested)
            && matches!(self.dynamic_build, DynamicBuildState::Running { .. })
        {
            return Task::none();
        }
        if !matches!(message, reactant_composer::Message::StartReactionRequested) {
            if let Some(cancellation) = self.dynamic_cancellation.take() {
                cancellation.store(true, Ordering::Relaxed);
            }
            self.next_dynamic_run_id = self.next_dynamic_run_id.saturating_add(1);
            self.dynamic_build = DynamicBuildState::Idle;
            self.dynamic_identity_choice = None;
            self.dynamic_started_at = None;
            self.dynamic_latency = LatencyMilestones::default();
            self.dynamic_progress = None;
            self.dynamic_progress_receiver = None;
            if self.dynamic_static.take().is_some()
                || self.dynamic_claim.take().is_some()
                || self.dynamic_presentation.take().is_some()
                || matches!(&self.validated_frames, Some(RenderableFrames::Dynamic(_)))
            {
                self.validated_frames = None;
                self.structural_animation = None;
            }

            self.dynamic_request = None;
            reactant_composer::update(&mut self.reactant_composer, message);
            return Task::none();
        }
        match reactant_composer::resolution(&self.reactant_composer) {
            chemistry::DraftResolution::Supported(request) => {
                self.pending_requests.clear();
                self.oxygen_assessment = None;
                self.select_request(request);
                self.open_structural_animation();
                Task::none()
            }
            chemistry::DraftResolution::Multiple(requests) => {
                self.pending_requests = requests;
                self.oxygen_assessment = None;
                self.screen = Screen::OutcomeChoice;
                Task::none()
            }
            chemistry::DraftResolution::Screened(assessment) => {
                self.pending_requests.clear();
                self.oxygen_assessment = Some(assessment);
                self.screen = Screen::OutcomeChoice;
                Task::none()
            }
            chemistry::DraftResolution::ExplicitlyUnsupported(_)
            | chemistry::DraftResolution::Uncatalogued
            | chemistry::DraftResolution::Unrecognized => self.start_dynamic_build(),
            chemistry::DraftResolution::SystemError(_) => Task::none(),
        }
    }

    fn start_dynamic_build(&mut self) -> Task<Message> {
        let (first, second) = reactant_composer::reactants(&self.reactant_composer);
        let single_context = second.is_empty().then_some(self.dynamic_context).flatten();
        let drafts = if single_context.is_some() {
            vec![first]
        } else {
            vec![first, second]
        };
        let request = ReactionBuildRequest {
            reactants: drafts
                .into_iter()
                .map(|atoms| ReactantInput {
                    display: reactant_composer::formula(atoms),
                    // Keep the identity inventory aligned with the standard-state
                    // formula shown by the composer (H₂, N₂, O₂, P₄, S₈, ...).
                    atomic_numbers: chemistry::standardize_elemental_draft(atoms),
                    species_id: None,
                })
                .collect(),
            selected_context: single_context.map(|context| context.value().to_owned()),
        };
        self.start_dynamic_build_request(request, false)
    }

    #[allow(clippy::too_many_lines)]
    fn start_dynamic_build_request(
        &mut self,
        mut request: ReactionBuildRequest,
        regenerate: bool,
    ) -> Task<Message> {
        let local = self.local_mode();
        if !local && !matches!(self.provider, Some(ProviderChoice::CodexSubscription)) {
            self.dynamic_build = DynamicBuildState::Failed(
                "Direct API reaction building is not available yet; choose Codex subscription."
                    .to_owned(),
            );
            return Task::none();
        }
        if let Some(cancellation) = self.dynamic_cancellation.take() {
            cancellation.store(true, Ordering::Relaxed);
        }
        let run_id = self.next_dynamic_run_id;
        self.next_dynamic_run_id = self.next_dynamic_run_id.saturating_add(1);
        self.validated_frames = None;
        self.dynamic_claim = None;
        self.dynamic_static = None;
        self.dynamic_presentation = None;

        self.dynamic_identity_choice = None;
        self.dynamic_details_open = false;
        self.dynamic_started_at = None;
        self.dynamic_latency = LatencyMilestones::default();
        self.dynamic_theatre_phase = 0.0;
        self.dynamic_progress = None;
        self.dynamic_progress_receiver = None;
        self.structural_animation = None;
        self.structural_error = None;
        let mode = self.claim_mode;
        let mut config = CodexProviderConfig::from_environment();
        let catalogue = match chemistry::trusted_catalogue() {
            Ok(catalogue) => catalogue.clone(),
            Err(error) => {
                self.dynamic_build = DynamicBuildState::Failed(error.to_owned());
                return Task::none();
            }
        };
        let identities = match reviewed_species_registry(&catalogue) {
            Ok(identities) => identities,
            Err(error) => {
                self.dynamic_build = DynamicBuildState::Failed(error.to_string());
                return Task::none();
            }
        };
        match resolve_request_identities_with_catalogue(&request, &identities, &catalogue) {
            Ok(RequestIdentityResolution::Resolved(resolved)) => {
                for (input, species) in request.reactants.iter_mut().zip(resolved) {
                    // Generated identities are rebuilt deterministically on
                    // each resolution, so only registry-pinned ids persist.
                    if let agent::OutcomeSpecies::Resolved(species) = species
                        && identities.get(&species.id).is_some()
                    {
                        input.species_id = Some(species.id);
                    }
                }
            }
            Ok(RequestIdentityResolution::Ambiguous(ambiguity)) => {
                self.dynamic_request = Some(request.clone());
                self.dynamic_identity_choice = Some(DynamicIdentityChoice { request, ambiguity });
                self.dynamic_build = DynamicBuildState::Idle;
                return Task::none();
            }
            Err(error) => {
                self.dynamic_request = Some(request);
                self.dynamic_build = DynamicBuildState::Failed(error.to_string());
                return Task::none();
            }
        }
        self.dynamic_request = Some(request.clone());
        self.dynamic_started_at = Some(Instant::now());
        self.dynamic_latency = LatencyMilestones::default();
        self.dynamic_build = DynamicBuildState::Running {
            run_id,
            elapsed_seconds: 0,
            stage: DynamicBuildStage::Claim,
        };
        let cancellation = Arc::new(AtomicBool::new(false));
        config.cancellation = Some(cancellation.clone());
        config.progress = Some(self.reset_dynamic_progress_channel());
        self.dynamic_cancellation = Some(cancellation);
        Task::perform(
            async move {
                let started = Instant::now();
                let deadline = started
                    + match mode {
                        ClaimMode::Fast => FAST_CLAIM_TIMEOUT,
                        ClaimMode::Researcher => RESEARCHER_CLAIM_TIMEOUT,
                    };
                let mut latency = LatencyMilestones::default();
                let provider = CodexProvider::new(config);
                // Local Mode never reads the cache: cached claims are model
                // output, and Local Mode is purely programmatic.
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
                    return Ok(DynamicClaimStageResult {
                        outcome: cached.outcome,
                        presentation: cached.presentation,
                        latency,
                    });
                }
                // Algorithmic solving comes first: deterministic reaction
                // families never need the model at all.
                let solved = agent::solve_reaction_claim(&request, &identities);
                let algorithmic = solved.is_some();
                let claim = match solved {
                    Some(claim) => claim,
                    None if local => {
                        return Err(
                            "This reaction isn't supported in Local Mode — ChemSpec couldn't \
                             derive it programmatically. Switch to an AI mode to research it."
                                .to_owned(),
                        );
                    }
                    None => provider
                        .claim_reaction_until(&request, mode, deadline)
                        .map_err(|error| error.to_string())?,
                };
                latency.claim_ms = Some(elapsed_millis(started));
                let outcome = compile_claim_outcome(&request, claim.clone(), &identities)
                    .map_err(|error| error.to_string())?;
                if matches!(outcome, CompiledClaimOutcome::Static(_)) {
                    latency.static_outcome_ms = Some(elapsed_millis(started));
                }
                // Algorithmic claims are recomputed instantly; only model
                // claims are worth caching.
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
                Ok(DynamicClaimStageResult {
                    outcome,
                    presentation: None,
                    latency,
                })
            },
            move |result| Message::DynamicClaimFinished {
                run_id,
                result: Box::new(result),
            },
        )
    }

    fn start_dynamic_presentation(
        run_id: u64,
        request: ReactionBuildRequest,
        mode: ClaimMode,
        local: bool,
        outcome: ValidatedStaticOutcome,
        cancellation: Arc<AtomicBool>,
        progress: Sender<CodexProgressEvent>,
    ) -> Task<Message> {
        let mut config = CodexProviderConfig::from_environment();
        config.cancellation = Some(cancellation);
        config.progress = Some(progress);
        let catalogue = match chemistry::trusted_catalogue() {
            Ok(catalogue) => catalogue.clone(),
            Err(error) => {
                return Task::done(Message::DynamicPresentationFinished {
                    run_id,
                    result: Box::new(Err(error.to_owned())),
                });
            }
        };
        Task::perform(
            async move {
                if local {
                    // Reviewed-family and algorithmic mechanisms only; model
                    // escalation is explicitly unsupported, so a static
                    // settle is final rather than retryable.
                    let presentation = enrich_static_outcome(
                        outcome,
                        &catalogue,
                        &mut agent::UnsupportedMechanismProvider,
                    )
                    .map_err(|error| error.to_string())?;
                    return Ok(match presentation {
                        DynamicPresentationOutcome::Static {
                            outcome,
                            diagnostic,
                            attempts,
                            retryable: _,
                        } => DynamicPresentationOutcome::Static {
                            outcome,
                            diagnostic,
                            attempts,
                            retryable: false,
                        },
                        animated => animated,
                    });
                }
                let mut provider = CodexProvider::new(config);
                let claim = outcome.claim().clone();
                let presentation = enrich_static_outcome(outcome, &catalogue, &mut provider)
                    .map_err(|error| error.to_string())?;
                let recipe = match &presentation {
                    DynamicPresentationOutcome::ReviewedFamily(outcome) => {
                        Some(DynamicCachePresentation::ReviewedFamily {
                            rule_id: outcome.family_rule().clone(),
                        })
                    }
                    // An escalation with no retained model response was
                    // derived algorithmically; it recomputes instantly and
                    // is not worth caching.
                    DynamicPresentationOutcome::Escalated(_) => provider
                        .take_last_mechanism_response()
                        .map(|response| DynamicCachePresentation::Escalated {
                            response,
                            structures: provider.take_last_structure_response(),
                        }),
                    DynamicPresentationOutcome::Static {
                        diagnostic,
                        retryable,
                        ..
                    } => Some(DynamicCachePresentation::Static {
                        diagnostic: diagnostic.clone(),
                        retryable: *retryable,
                    }),
                };
                if let (Some(recipe), Some(directory)) =
                    (recipe, provider.config().cache_directory.as_deref())
                {
                    let identities =
                        reviewed_species_registry(&catalogue).map_err(|error| error.to_string())?;
                    let _ = store_dynamic_cache(
                        directory,
                        &request,
                        mode,
                        &identities,
                        &catalogue,
                        &claim,
                        Some(recipe),
                        "codex_subscription",
                        provider.model_name(),
                    );
                }
                Ok(presentation)
            },
            move |result| Message::DynamicPresentationFinished {
                run_id,
                result: Box::new(result),
            },
        )
    }

    fn finish_dynamic_presentation(&mut self, presentation: DynamicPresentationOutcome) {
        self.dynamic_static = Some(presentation.static_outcome().clone());
        self.validated_frames = match &presentation {
            DynamicPresentationOutcome::ReviewedFamily(outcome) => {
                Some(RenderableFrames::Catalogue(outcome.frames().clone()))
            }
            DynamicPresentationOutcome::Escalated(outcome) => {
                Some(RenderableFrames::Dynamic(outcome.frames().clone()))
            }
            DynamicPresentationOutcome::Static { .. } => None,
        };
        let animated = self.validated_frames.is_some();
        self.dynamic_presentation = Some(presentation);
        self.dynamic_build = DynamicBuildState::Idle;
        self.dynamic_cancellation = None;
        self.dynamic_progress_receiver = None;
        self.structural_animation = None;
        self.structural_error = None;
        if animated {
            self.open_structural_animation();
        }
    }

    fn reset_dynamic_progress_channel(&mut self) -> Sender<CodexProgressEvent> {
        let (sender, receiver) = mpsc::channel();
        self.dynamic_progress = None;
        self.dynamic_progress_receiver = Some(receiver);
        sender
    }

    fn drain_dynamic_progress(&mut self) {
        let Some(receiver) = &self.dynamic_progress_receiver else {
            return;
        };
        while let Ok(event) = receiver.try_recv() {
            self.dynamic_progress = Some(event);
        }
    }

    fn dynamic_progress_label(&self) -> Option<&'static str> {
        self.dynamic_progress.map(|event| match event.stage {
            CodexProgressStage::Started => "preparing the virtual model",
            CodexProgressStage::Working => "working out where the electrons go",
            CodexProgressStage::SearchingSources => "checking the supporting evidence",
            CodexProgressStage::Completed => "the next view is ready",
            CodexProgressStage::Failed => "this pass needs another try",
        })
    }

    // The offline fixture crosses the same trusted language/kernel boundary
    // that live provider output must cross later.
    fn select_request(&mut self, request: chemistry::ReactionRequest) {
        self.active_request = request;
        self.validated_frames = chemistry::run(request)
            .ok()
            .map(|run| RenderableFrames::Catalogue(run.frames().clone()));
        self.dynamic_claim = None;
        self.dynamic_static = None;
        self.dynamic_presentation = None;

        self.dynamic_request = None;
        self.dynamic_build = DynamicBuildState::Idle;
        self.structural_animation = None;
        self.structural_error = None;
    }

    fn change_structural_frame(&mut self, delta: i8) {
        let Some(animation) = &mut self.structural_animation else {
            return;
        };
        if self.screen == Screen::Structural2d {
            let Some(position) = animation
                .educational_plan
                .locate(animation.educational_playhead_ms)
            else {
                animation.playing = false;
                return;
            };
            let target_scene = if delta < 0 {
                if position.scene_elapsed_ms > 650 {
                    position.scene_index
                } else {
                    position.scene_index.saturating_sub(1)
                }
            } else if position.scene_index + 1 < animation.educational_plan.scenes.len() {
                position.scene_index + 1
            } else {
                animation.playing = false;
                animation.educational_plan.scenes.len().saturating_sub(1)
            };
            animation.educational_playhead_ms = animation
                .educational_plan
                .elapsed_at(TimelinePosition {
                    scene_index: target_scene,
                    scene_elapsed_ms: 0,
                })
                .unwrap_or(0);
            animation.playing = false;
            sync_educational_frame(animation);
            return;
        }
        if delta < 0 {
            animation.frame_index = animation.frame_index.saturating_sub(1);
            animation.playing = false;
        } else if animation.frame_index + 1 < animation.frames.frames().len() {
            animation.frame_index += 1;
        } else {
            animation.playing = false;
        }
    }

    fn theme(_: &Self) -> Theme {
        theme::app_theme()
    }

    fn subscription(&self) -> Subscription<Message> {
        let resize = iced::window::resize_events().map(|(_id, size)| Message::WindowResized(size));
        let screen = if self.screen == Screen::Builder {
            Subscription::batch([
                periodic_table::subscription(&self.periodic_table).map(Message::PeriodicTable),
                reactant_composer::subscription(&self.reactant_composer)
                    .map(Message::ReactantComposer),
            ])
        } else if self.screen == Screen::Structural2d
            || (self.screen == Screen::Structural3d
                && self
                    .structural_animation
                    .as_ref()
                    .is_some_and(|animation| animation.playing))
        {
            iced::time::every(std::time::Duration::from_millis(33)).map(|_| Message::StructuralTick)
        } else {
            Subscription::none()
        };

        let dynamic_build = if self.screen == Screen::Builder {
            match &self.dynamic_build {
                DynamicBuildState::Running { run_id, .. } => {
                    iced::time::every(std::time::Duration::from_secs(1))
                        .with(*run_id)
                        .map(|(run_id, _)| Message::DynamicBuildTick { run_id })
                }
                DynamicBuildState::Idle | DynamicBuildState::Failed(_) => Subscription::none(),
            }
        } else {
            Subscription::none()
        };

        let keyboard = if self.screen == Screen::Builder {
            iced::keyboard::listen().map(Message::KeyboardEvent)
        } else {
            Subscription::none()
        };

        let dynamic_theatre = if self.screen == Screen::Builder
            && self.dynamic_static.is_some()
            && matches!(
                self.dynamic_build,
                DynamicBuildState::Running {
                    stage: DynamicBuildStage::Presentation,
                    ..
                }
            ) {
            iced::time::every(std::time::Duration::from_millis(33))
                .map(|_| Message::DynamicTheatreTick)
        } else {
            Subscription::none()
        };

        Subscription::batch([resize, screen, dynamic_build, dynamic_theatre, keyboard])
    }

    fn view(&self) -> Element<'_, Message> {
        match self.screen {
            Screen::ProviderSetup => responsive(|size| self.provider_setup_view(size)).into(),
            Screen::Builder => responsive(|size| self.builder_view(size)).into(),
            Screen::OutcomeChoice => responsive(|size| self.outcome_choice_view(size)).into(),
            Screen::Structural2d => responsive(|size| self.structural_2d_view(size)).into(),
            Screen::Structural3d => responsive(|size| self.structural_3d_view(size)).into(),
        }
    }

    fn outcome_choice_view(&self, size: Size) -> Element<'_, Message> {
        use chem_catalogue::{OxygenOutcome, StructuralSupport};

        let compact = size.width < breakpoint::MOBILE || size.height < 760.0;

        let back = button(text("← Reactants"))
            .on_press(Message::ScreenSelected(Screen::Builder))
            .padding([spacing::XS, spacing::SM])
            .style(theme::secondary_button);

        let content: Element<'_, Message> = if !self.pending_requests.is_empty() {
            let mut choices = column![].spacing(spacing::SM).width(Fill);
            for request in &self.pending_requests {
                choices = choices.push(reviewed_outcome_choice(*request, compact));
            }
            column![
                row![
                    back,
                    text("Choose the product")
                        .size(if compact {
                            type_scale::TITLE
                        } else {
                            type_scale::DISPLAY
                        })
                        .color(color::TEXT),
                ]
                .spacing(spacing::SM)
                .align_y(Center),
                scrollable(choices).width(Fill).height(Fill),
            ]
            .spacing(spacing::MD)
            .height(Fill)
            .into()
        } else if let Some(assessment) = &self.oxygen_assessment {
            let (status, detail, equation) = match &assessment.outcome {
                OxygenOutcome::Representative {
                    equation,
                    structural_support,
                    ..
                } => {
                    let detail = match structural_support {
                        StructuralSupport::PendingReviewedModel => {
                            "A structural simulation has not been reviewed yet."
                        }
                        StructuralSupport::UnsupportedBondModel => {
                            "This bonding model is not supported yet."
                        }
                    };
                    ("Representative outcome", detail, Some(equation.as_str()))
                }
                OxygenOutcome::NoDirectReaction { reason } => {
                    ("No direct reaction", reason.as_str(), None)
                }
                OxygenOutcome::Ambiguous { reason } => ("Ambiguous outcome", reason.as_str(), None),
                OxygenOutcome::Unsupported { reason } => ("Unsupported", reason.as_str(), None),
            };
            let equation: Element<'_, Message> = equation.map_or_else(
                || space().height(Length::Shrink).into(),
                |equation| {
                    container(
                        text(nomenclature::display_equation(equation))
                            .size(type_scale::BODY_LARGE)
                            .color(color::TEXT),
                    )
                    .padding(spacing::MD)
                    .style(theme::inset)
                    .width(Fill)
                    .into()
                },
            );
            column![
                text(format!("{} + O₂", assessment.subject))
                    .size(type_scale::DISPLAY)
                    .color(color::TEXT),
                text(status).size(type_scale::MICRO).color(color::WARNING),
                equation,
                text(detail).size(type_scale::BODY).color(color::MUTED),
                back,
            ]
            .spacing(spacing::MD)
            .width(Fill)
            .into()
        } else {
            column![
                text("Outcome unavailable")
                    .size(type_scale::TITLE)
                    .color(color::TEXT),
                back,
            ]
            .spacing(spacing::MD)
            .into()
        };

        container(container(content).width(Fill).height(Fill).max_width(768.0))
            .center(Fill)
            .padding(if compact { spacing::MD } else { spacing::XL })
            .style(theme::app_background)
            .into()
    }

    #[allow(clippy::too_many_lines)]
    fn provider_setup_view(&self, size: Size) -> Element<'_, Message> {
        let compact = size.width < breakpoint::MOBILE;
        let local_selected = self.provider == Some(ProviderChoice::Local);
        let codex_selected = self.provider == Some(ProviderChoice::CodexSubscription);
        let api_selected = self.provider == Some(ProviderChoice::ApiKey);

        let local = button(
            row![
                icons::chip(20.0, color::ACCENT),
                text("Local Mode").size(type_scale::BODY_LARGE),
            ]
            .spacing(spacing::SM)
            .align_y(Center),
        )
        .on_press(Message::ProviderSelected(ProviderChoice::Local))
        .padding([spacing::SM, spacing::MD])
        .width(Fill)
        .style(move |_, status| theme::provider_button(local_selected, status));

        let codex_icon_color = if self.codex_available {
            color::ACCENT
        } else {
            color::FAINT
        };

        let codex = button(
            row![
                icons::codex(20.0, codex_icon_color),
                text("Codex subscription").size(type_scale::BODY_LARGE),
            ]
            .spacing(spacing::SM)
            .align_y(Center),
        )
        .on_press_maybe(
            self.codex_available
                .then_some(Message::ProviderSelected(ProviderChoice::CodexSubscription)),
        )
        .padding([spacing::SM, spacing::MD])
        .width(Fill)
        .style(move |_, status| theme::provider_button(codex_selected, status));

        let api = button(
            row![
                icons::api_key(20.0, color::ACCENT),
                text("OpenAI API key").size(type_scale::BODY_LARGE),
            ]
            .spacing(spacing::SM)
            .align_y(Center),
        )
        .on_press(Message::ProviderSelected(ProviderChoice::ApiKey))
        .padding([spacing::SM, spacing::MD])
        .width(Fill)
        .style(move |_, status| theme::provider_button(api_selected, status));

        let choices: Element<'_, Message> = column![
            local,
            rule::horizontal(1).style(theme::soft_divider),
            text("Supercharge ChemSpec with AI")
                .size(type_scale::BODY)
                .color(color::MUTED),
            codex,
            api,
        ]
        .spacing(spacing::SM)
        .into();
        let ready = local_selected || (codex_selected && self.codex_available);
        let continue_label = if local_selected {
            "Continue with Local Mode"
        } else if api_selected {
            "API provider coming next"
        } else {
            "Continue with Codex"
        };
        let continue_icon_color = if ready { color::CANVAS } else { color::FAINT };
        let continue_button = button(
            row![
                text(continue_label),
                icons::arrow_right(16.0, continue_icon_color),
            ]
            .spacing(spacing::XS)
            .align_y(Center),
        )
        .on_press_maybe(ready.then_some(Message::ProviderContinue))
        .padding([spacing::SM, spacing::MD])
        .style(theme::primary_button);

        let action: Element<'_, Message> = if api_selected {
            let input = text_input("sk-…", &self.api_key)
                .on_input(Message::ApiKeyChanged)
                .secure(true)
                .padding(spacing::SM)
                .width(Fill)
                .style(theme::request_input);

            if compact {
                column![input, continue_button]
                    .spacing(spacing::SM)
                    .width(Fill)
                    .into()
            } else {
                row![input, continue_button]
                    .spacing(spacing::SM)
                    .align_y(Center)
                    .width(Fill)
                    .into()
            }
        } else {
            row![space().width(Fill), continue_button]
                .align_y(Center)
                .width(Fill)
                .into()
        };

        let mut sections: Vec<Element<'_, Message>> = vec![
            text("How should ChemSpec research?")
                .size(if compact {
                    type_scale::TITLE
                } else {
                    type_scale::DISPLAY
                })
                .color(color::TEXT)
                .into(),
            choices,
        ];

        if !self.codex_available {
            sections.push(
                row![
                    container(rule::vertical(2).style(theme::danger_divider)).height(48),
                    icons::alert(20.0, color::DANGER),
                    column![
                        text("Codex wasn’t found")
                            .size(type_scale::BODY_LARGE)
                            .color(color::TEXT),
                        text("Use an API key or install Codex.")
                            .size(type_scale::CAPTION)
                            .color(color::MUTED),
                    ]
                    .spacing(spacing::XXS),
                ]
                .spacing(spacing::SM)
                .align_y(Center)
                .into(),
            );
        }

        sections.push(action);

        let content = container(column(sections).spacing(spacing::LG))
            .width(Fill)
            .max_width(768.0);

        container(content)
            .center(Fill)
            .style(theme::app_background)
            .padding(if compact { spacing::LG } else { spacing::XL })
            .width(Fill)
            .height(Fill)
            .into()
    }

    fn open_structural_animation(&mut self) {
        let result = (|| {
            let frames = self
                .validated_frames
                .as_ref()
                .ok_or_else(|| "validated frames are unavailable".to_owned())?
                .clone();
            let educational_plan =
                compile_educational_plan(&frames).map_err(|error| error.to_string())?;
            let (profile, reactant_previews, product_preview, equation) =
                if let Some(dynamic) = &self.dynamic_static {
                    (
                        dynamic_presentation_profile(&frames, dynamic.equation()),
                        Vec::new(),
                        None,
                        dynamic.equation().to_owned(),
                    )
                } else {
                    (
                        chemistry::presentation_profile(self.active_request, &frames)?,
                        self.active_request.reactant_previews(),
                        self.active_request.product_preview(),
                        self.active_request.equation(),
                    )
                };
            let real_world_plan =
                compile_real_world_plan(&frames, &profile).map_err(|error| error.to_string())?;
            Ok::<_, String>(StructuralAnimation {
                frames,
                educational_plan,
                real_world_plan,
                reactant_previews,
                product_preview,
                equation,
                educational_playhead_ms: 0,
                frame_index: 0,
                real_world_playhead_ms: 0,
                playing: true,
                playback_speed: PlaybackSpeed::Normal,
                physics: structural_physics::Simulation::default(),
            })
        })();
        match result {
            Ok(animation) => {
                self.structural_animation = Some(animation);
                self.structural_error = None;
                // Seed the simulation so the first paint has settled-ish
                // positions instead of an empty canvas.
                for _ in 0..24 {
                    self.step_structural_physics();
                }
            }
            Err(error) => {
                self.structural_animation = None;
                self.structural_error = Some(error);
            }
        }
        self.screen = Screen::Structural2d;
    }

    fn advance_educational_playback(&mut self, elapsed_ms: u32) {
        let Some(animation) = &mut self.structural_animation else {
            return;
        };
        let duration = animation.educational_plan.duration_ms();
        if duration == 0 {
            animation.playing = false;
            return;
        }
        animation.educational_playhead_ms = animation
            .educational_playhead_ms
            .saturating_add(u64::from(elapsed_ms))
            .min(duration);
        if animation.educational_playhead_ms == duration {
            animation.playing = false;
        }
        sync_educational_frame(animation);
    }

    /// Advances the 2D force simulation against the current scene's world.
    fn step_structural_physics(&mut self) {
        let Some(animation) = &mut self.structural_animation else {
            return;
        };
        let Some(position) = animation
            .educational_plan
            .locate(animation.educational_playhead_ms)
        else {
            return;
        };
        let Some(scene) = animation
            .educational_plan
            .scenes
            .get(position.scene_index)
        else {
            return;
        };
        let frames = animation.frames.frames();
        let Some(after) = frames
            .iter()
            .find(|candidate| candidate.trace().state_digest == scene.end_frame)
        else {
            return;
        };
        let before = frames
            .iter()
            .find(|candidate| candidate.trace().state_digest == scene.start_frame)
            .unwrap_or(after);
        #[allow(clippy::cast_precision_loss)]
        let scene_progress = if scene.duration_ms == 0 {
            1.0
        } else {
            (position.scene_elapsed_ms as f32 / scene.duration_ms as f32).clamp(0.0, 1.0)
        };
        let has_explanation = scene.cues.iter().any(|cue| {
            matches!(
                cue,
                chem_presentation::EducationalCue::ShowExplanation { .. }
            )
        });
        let action = structural_2d::scene_action(scene.kind, has_explanation, scene_progress);
        let spec = structural_2d::world_spec(before, after, action);
        animation.physics.step(&spec);
    }

    fn seek_educational_timeline(&mut self, elapsed_ms: u64) {
        let Some(animation) = &mut self.structural_animation else {
            return;
        };
        animation.educational_playhead_ms =
            elapsed_ms.min(animation.educational_plan.duration_ms());
        sync_educational_frame(animation);
    }

    fn advance_real_world_playback(&mut self, elapsed_ms: u32) {
        let Some(animation) = &mut self.structural_animation else {
            return;
        };
        let real_world_plan = &animation.real_world_plan;
        let duration = real_world_plan.timeline.duration_ms();
        animation.real_world_playhead_ms = animation
            .real_world_playhead_ms
            .saturating_add(u64::from(elapsed_ms))
            .min(duration);
        if let Some(position) = real_world_plan
            .timeline
            .locate(animation.real_world_playhead_ms)
            && let Some(frame_index) = animation
                .frames
                .frames()
                .iter()
                .position(|frame| frame.ordinal() == u32::from(position.ordinal))
        {
            animation.frame_index = frame_index;
        }
        if animation.real_world_playhead_ms == duration {
            animation.playing = false;
        }
    }

    fn seek_real_world_timeline(&mut self, elapsed_ms: u64) {
        let Some(animation) = &mut self.structural_animation else {
            return;
        };
        let real_world_plan = &animation.real_world_plan;
        animation.real_world_playhead_ms = elapsed_ms.min(real_world_plan.timeline.duration_ms());
        if let Some(position) = real_world_plan
            .timeline
            .locate(animation.real_world_playhead_ms)
            && let Some(frame_index) = animation
                .frames
                .frames()
                .iter()
                .position(|frame| frame.ordinal() == u32::from(position.ordinal))
        {
            animation.frame_index = frame_index;
        }
    }

    #[allow(clippy::too_many_lines, clippy::cast_precision_loss)]
    fn structural_2d_view(&self, size: Size) -> Element<'_, Message> {
        let Some(animation) = &self.structural_animation else {
            return Self::structural_unavailable_view("Trusted frames are unavailable");
        };
        let Some(timeline_position) = animation
            .educational_plan
            .locate(animation.educational_playhead_ms)
        else {
            return Self::structural_unavailable_view("The educational timeline is unavailable");
        };
        let Some(educational_scene) = animation
            .educational_plan
            .scenes
            .get(timeline_position.scene_index)
        else {
            return Self::structural_unavailable_view(
                "The current educational scene is unavailable",
            );
        };
        let frames = animation.frames.frames();
        let Some(frame) = animation
            .frames
            .frames()
            .iter()
            .find(|candidate| candidate.trace().state_digest == educational_scene.end_frame)
        else {
            return Self::structural_unavailable_view("The current frame is unavailable");
        };
        let before_frame = animation
            .frames
            .frames()
            .iter()
            .find(|candidate| candidate.trace().state_digest == educational_scene.start_frame)
            .or_else(|| frames.get(animation.frame_index))
            .unwrap_or(frame);
        let operation_transitions = educational_scene
            .cues
            .iter()
            .filter_map(|cue| match cue {
                chem_presentation::EducationalCue::ApplyOperations { operations } => {
                    Some(operations)
                }
                _ => None,
            })
            .flatten()
            .filter_map(|operation| {
                let before = frames
                    .iter()
                    .find(|candidate| candidate.trace().state_digest == operation.before)?;
                let after = frames
                    .iter()
                    .find(|candidate| candidate.trace().state_digest == operation.after)?;
                Some((before, after))
            })
            .collect::<Vec<_>>();
        let scene_progress = if educational_scene.duration_ms == 0 {
            1.0
        } else {
            (timeline_position.scene_elapsed_ms as f32 / educational_scene.duration_ms as f32)
                .clamp(0.0, 1.0)
        };
        let compact = size.width < breakpoint::MOBILE;
        let explanation = educational_scene.cues.iter().find_map(|cue| match cue {
            chem_presentation::EducationalCue::ShowExplanation { label } => Some(label),
            _ => None,
        });
        let context_labels = educational_scene
            .cues
            .iter()
            .filter_map(|cue| match cue {
                chem_presentation::EducationalCue::ShowContext { label } => Some(label.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let playback = button(text(if animation.playing {
            "Ⅱ  Pause"
        } else {
            "▶  Play"
        }))
        .on_press(Message::StructuralPlaybackToggled)
        .padding([spacing::XS, spacing::SM])
        .style(theme::primary_button);
        let previous = button(text("‹"))
            .on_press_maybe(
                (timeline_position.scene_index > 0 || timeline_position.scene_elapsed_ms > 0)
                    .then_some(Message::StructuralChapterChanged(-1)),
            )
            .padding([spacing::XS, spacing::SM])
            .style(theme::secondary_button);
        let next = button(text("›"))
            .on_press_maybe(
                (timeline_position.scene_index + 1 < animation.educational_plan.scenes.len())
                    .then_some(Message::StructuralChapterChanged(1)),
            )
            .padding([spacing::XS, spacing::SM])
            .style(theme::secondary_button);
        let restart = button(text("Restart"))
            .on_press(Message::StructuralRestarted)
            .padding([spacing::XS, spacing::SM])
            .style(theme::secondary_button);
        let speed = button(text(animation.playback_speed.label()))
            .on_press(Message::StructuralSpeedChanged)
            .padding([spacing::XS, spacing::SM])
            .style(theme::secondary_button);
        let exit = button(text("← Reactants"))
            .on_press(Message::ScreenSelected(Screen::Builder))
            .padding([spacing::XS, spacing::SM])
            .style(theme::secondary_button);
        // Local Mode derivations are deterministic; regenerating would only
        // recompute the identical result.
        let regenerate: Element<'_, Message> = if self.dynamic_request.is_some()
            && !self.local_mode()
        {
            button(text("Regenerate"))
                .on_press(Message::RegenerateDynamicReaction)
                .padding([spacing::XS, spacing::SM])
                .style(theme::secondary_button)
                .into()
        } else {
            space().width(Length::Shrink).into()
        };
        let continue_3d: Element<'_, Message> =
            if timeline_position.scene_index + 1 == animation.educational_plan.scenes.len() {
                button(text("View 3D model  →"))
                    .on_press(Message::ContinueTo3d)
                    .padding([spacing::XS, spacing::MD])
                    .style(theme::primary_button)
                    .into()
            } else {
                space().width(Length::Shrink).into()
            };

        let equation = plan_equation(animation).map(nomenclature::display_equation);
        let scene_context = structural_2d::SceneContext::new(
            educational_scene.kind,
            timeline_position.scene_index,
            animation.educational_plan.scenes.len(),
        )
        .with_equation(equation.clone());
        let diagram_canvas: Element<'_, structural_2d::DragEvent> = canvas(
            structural_2d::Diagram::new(
                before_frame,
                frame,
                &operation_transitions,
                scene_progress,
                explanation,
                &context_labels,
                scene_context,
                educational_timeline_progress(animation),
                matches!(
                    educational_scene.kind,
                    EducationalSceneKind::ReactantSetup
                        | EducationalSceneKind::StructuralChange
                        | EducationalSceneKind::ExplanationPause
                ),
                animation.physics.positions(),
            ),
        )
        .width(Fill)
        .height(Fill)
        .into();
        let diagram = container(diagram_canvas.map(Message::StructuralDrag))
            .style(theme::inset)
            .width(Fill)
            .height(Fill);

        let duration_ms = animation.educational_plan.duration_ms();
        let slider_duration = u32::try_from(duration_ms).unwrap_or(u32::MAX).max(1);
        let slider_playhead = u32::try_from(animation.educational_playhead_ms)
            .unwrap_or(u32::MAX)
            .min(slider_duration);
        let scrubber = slider(
            0_u32..=slider_duration,
            slider_playhead,
            Message::StructuralTimelineScrubbed,
        )
        .step(50_u32)
        .shift_step(1_000_u32)
        .height(28.0)
        .width(Fill)
        .style(theme::timeline_slider);
        let timeline = stack![
            canvas(structural_2d::TimelineGuide::new(
                &animation.educational_plan,
                timeline_position.scene_index,
            ))
            .width(Fill)
            .height(Length::Fixed(28.0)),
            scrubber,
        ]
        .width(Fill)
        .height(Length::Fixed(28.0));
        let transport: Element<'_, Message> = if compact {
            column![
                row![playback, previous, next, speed]
                    .spacing(spacing::XS)
                    .align_y(Center),
                row![restart, space().width(Fill), continue_3d]
                    .spacing(spacing::XS)
                    .align_y(Center),
            ]
            .spacing(spacing::XXS)
            .into()
        } else {
            row![
                playback,
                previous,
                next,
                restart,
                speed,
                space().width(Fill),
                continue_3d,
            ]
            .spacing(spacing::XS)
            .align_y(Center)
            .into()
        };
        let controls = container(
            column![
                transport,
                row![
                    column![
                        text(format!(
                            "CHAPTER {:02}  ·  {}",
                            timeline_position.scene_index + 1,
                            educational_scene_title(educational_scene.kind)
                        ))
                        .size(type_scale::MICRO)
                        .color(color::ACCENT),
                        text(if compact {
                            "Drag to inspect"
                        } else {
                            "Drag the timeline to inspect any moment · arrows move between chapters"
                        })
                        .size(type_scale::MICRO)
                        .color(color::MUTED),
                    ]
                    .spacing(spacing::XXS),
                    space().width(Fill),
                    text(format!(
                        "{}  /  {}",
                        format_media_time(animation.educational_playhead_ms),
                        format_media_time(duration_ms)
                    ))
                    .size(type_scale::CAPTION)
                    .color(color::TEXT_SOFT),
                ]
                .align_y(Center),
                timeline,
            ]
            .spacing(spacing::XXS),
        )
        .style(theme::media_bar)
        .padding([spacing::XS, spacing::SM]);

        container(
            column![
                row![
                    exit,
                    regenerate,
                    text(
                        equation
                            .clone()
                            .unwrap_or_else(|| "Reviewed equation unavailable".to_owned())
                    )
                    .size(if compact {
                        type_scale::BODY_LARGE
                    } else {
                        type_scale::TITLE
                    })
                    .color(color::TEXT),
                    space().width(Fill),
                ]
                .align_y(Center),
                diagram,
                controls,
            ]
            .spacing(spacing::XS)
            .height(Fill),
        )
        .style(theme::frame)
        .padding(spacing::SM)
        .width(Fill)
        .height(Fill)
        .into()
    }

    fn structural_unavailable_view(message: &'static str) -> Element<'static, Message> {
        container(
            column![
                text("ANIMATION UNAVAILABLE")
                    .size(type_scale::MICRO)
                    .color(color::WARNING),
                text(message).size(type_scale::TITLE).color(color::TEXT),
                button(text("Return to builder"))
                    .on_press(Message::ScreenSelected(Screen::Builder))
                    .style(theme::secondary_button),
            ]
            .spacing(spacing::SM),
        )
        .style(theme::frame)
        .padding(spacing::MD)
        .width(Fill)
        .height(Fill)
        .into()
    }

    #[allow(clippy::too_many_lines, clippy::cast_precision_loss)]
    fn structural_3d_view(&self, size: Size) -> Element<'_, Message> {
        let compact = size.width < breakpoint::MOBILE;
        let Some(animation) = &self.structural_animation else {
            return Self::structural_unavailable_view("Validated frames are unavailable");
        };
        let real_world_plan = &animation.real_world_plan;
        let Some(moment) = real_world_plan
            .timeline
            .locate(animation.real_world_playhead_ms)
        else {
            return Self::structural_unavailable_view("The macroscopic timeline is unavailable");
        };
        let back = button(text("← 2D explanation"))
            .on_press(Message::ReturnTo2d)
            .padding([spacing::XS, spacing::SM])
            .style(theme::secondary_button);
        // Local Mode derivations are deterministic; regenerating would only
        // recompute the identical result.
        let regenerate: Element<'_, Message> = if self.dynamic_request.is_some()
            && !self.local_mode()
        {
            button(text("Regenerate"))
                .on_press(Message::RegenerateDynamicReaction)
                .padding([spacing::XS, spacing::SM])
                .style(theme::secondary_button)
                .into()
        } else {
            space().width(Length::Shrink).into()
        };
        let playback = button(text(if animation.playing { "Pause" } else { "Play" }))
            .on_press(Message::StructuralPlaybackToggled)
            .padding([spacing::XS, spacing::SM])
            .style(theme::primary_button);
        let restart = button(text("Restart"))
            .on_press(Message::StructuralRestarted)
            .padding([spacing::XS, spacing::SM])
            .style(theme::secondary_button);
        let speed = button(text(animation.playback_speed.label()))
            .on_press(Message::StructuralSpeedChanged)
            .padding([spacing::XS, spacing::SM])
            .style(theme::secondary_button);
        let active_annotation = real_world_plan.annotations.iter().rfind(|annotation| {
            annotation.start_ordinal <= moment.ordinal && moment.ordinal <= annotation.end_ordinal
        });
        let active_effects = real_world_plan
            .effects
            .iter()
            .filter(|effect| {
                effect.start_ordinal <= moment.ordinal && moment.ordinal <= effect.end_ordinal
            })
            .map(|effect| macroscopic_effect_label(effect.effect))
            .collect::<Vec<_>>()
            .join("  ·  ");
        let mut annotation = active_annotation.map_or_else(
            || {
                column![
                    text("REVIEWED SCENE")
                        .size(type_scale::MICRO)
                        .color(color::ACCENT),
                    text(nomenclature::display_equation(&real_world_plan.equation))
                        .size(type_scale::BODY_LARGE)
                        .color(color::TEXT),
                ]
            },
            |annotation| {
                let mut content = column![
                    text(annotation.title.as_str())
                        .size(type_scale::MICRO)
                        .color(color::ACCENT),
                    text(annotation.text.as_str())
                        .size(type_scale::BODY_LARGE)
                        .color(color::TEXT),
                ]
                .spacing(spacing::XXS);
                if !active_effects.is_empty() {
                    content = content.push(
                        text(active_effects.clone())
                            .size(type_scale::MICRO)
                            .color(color::TEXT_SOFT),
                    );
                }
                content
            },
        );
        let product_visible = real_world_plan.objects.iter().any(|object| {
            object.role == chem_presentation::SceneRole::Product
                && object.visible_from_ordinal <= moment.ordinal
        });
        if product_visible && let Some(preview) = &animation.product_preview {
            annotation = annotation.push(
                text(format!("Molecular model · {}", preview.formula))
                    .size(type_scale::MICRO)
                    .color(color::TEXT_SOFT),
            );
        }
        let scene_view = iced::widget::Shader::new(structural_3d::Scene::new(
            real_world_plan,
            moment,
            &animation.reactant_previews,
            animation.product_preview.as_ref(),
        ))
        .width(Fill)
        .height(Fill);
        let annotation_layer = container(
            column![
                space().height(Fill),
                container(annotation)
                    .style(theme::media_bar)
                    .padding([spacing::SM, spacing::MD])
                    .width(if compact { Fill } else { Length::Fixed(440.0) }),
            ]
            .height(Fill),
        )
        .padding(spacing::SM)
        .width(Fill)
        .height(Fill);
        let scene = container(
            stack![scene_view, annotation_layer]
                .width(Fill)
                .height(Fill),
        )
        .style(theme::inset)
        .width(Fill)
        .height(Fill);
        let duration_ms = real_world_plan.timeline.duration_ms();
        let slider_duration = u32::try_from(duration_ms).unwrap_or(u32::MAX).max(1);
        let slider_playhead = u32::try_from(animation.real_world_playhead_ms)
            .unwrap_or(u32::MAX)
            .min(slider_duration);
        let scrubber = slider(
            0_u32..=slider_duration,
            slider_playhead,
            Message::StructuralRealWorldTimelineScrubbed,
        )
        .step(50_u32)
        .shift_step(1_000_u32)
        .height(28.0)
        .width(Fill)
        .style(theme::timeline_slider);
        let transport: Element<'_, Message> = if compact {
            column![
                row![playback, restart, speed]
                    .spacing(spacing::XS)
                    .align_y(Center),
                scrubber,
            ]
            .spacing(spacing::XXS)
            .into()
        } else {
            row![playback, restart, speed, scrubber]
                .spacing(spacing::XS)
                .align_y(Center)
                .into()
        };
        let controls = container(
            column![
                transport,
                row![
                    text(format!(
                        "SCENE {:02} / {:02}  ·  CINEMATIC TIMELINE",
                        moment.beat_index + 1,
                        real_world_plan.timeline.beats.len()
                    ))
                    .size(type_scale::MICRO)
                    .color(color::ACCENT),
                    space().width(Fill),
                    text(format!(
                        "{}  /  {}",
                        format_media_time(animation.real_world_playhead_ms),
                        format_media_time(duration_ms)
                    ))
                    .size(type_scale::CAPTION)
                    .color(color::TEXT_SOFT),
                ]
                .align_y(Center),
            ]
            .spacing(spacing::XXS),
        )
        .style(theme::media_bar)
        .padding([spacing::XS, spacing::SM]);
        container(
            column![
                row![
                    back,
                    regenerate,
                    column![
                        text("VALIDATED 3D MODEL")
                            .size(type_scale::MICRO)
                            .color(color::ACCENT),
                        text("Illustrative molecular and macroscopic view")
                            .size(if compact {
                                type_scale::BODY_LARGE
                            } else {
                                type_scale::TITLE
                            })
                            .color(color::TEXT),
                    ],
                    space().width(Fill),
                    if compact {
                        text("").size(type_scale::MICRO)
                    } else {
                        text("DRAG TO ORBIT · SCROLL TO ZOOM")
                            .size(type_scale::MICRO)
                            .color(color::MUTED)
                    },
                ]
                .align_y(Center),
                scene,
                controls,
                text(real_world_plan.virtual_only_disclosure.as_str())
                    .size(type_scale::MICRO)
                    .color(color::WARNING),
            ]
            .spacing(spacing::XS)
            .height(Fill),
        )
        .style(theme::frame)
        .padding(spacing::SM)
        .width(Fill)
        .height(Fill)
        .into()
    }

    #[allow(clippy::too_many_lines)]
    fn dynamic_result_view(&self) -> Element<'_, Message> {
        if let Some(outcome) = &self.dynamic_static {
            let trust = match outcome.trust_tier() {
                TrustTier::Reviewed => "REVIEWED",
                // Local Mode claims come from the algorithmic solver, so the
                // unreviewed tier is derived rather than model-asserted.
                TrustTier::ModelAsserted if self.local_mode() => "DERIVED",
                TrustTier::ModelAsserted => "MODEL ASSERTED",
            };
            let presentation = match (&self.dynamic_build, &self.dynamic_presentation) {
                (
                    DynamicBuildState::Running {
                        stage: DynamicBuildStage::Presentation,
                        ..
                    },
                    _,
                ) => "Balanced static result ready · mechanism pending".to_owned(),
                (_, Some(DynamicPresentationOutcome::ReviewedFamily(outcome))) => {
                    format!("Reviewed family animation · {}", outcome.family_rule())
                }
                (_, Some(DynamicPresentationOutcome::Escalated(_))) => {
                    "Validated mechanism ready".to_owned()
                }
                (_, Some(DynamicPresentationOutcome::Static { retryable, .. })) => {
                    if *retryable {
                        "Animation is not available yet · retry available".to_owned()
                    } else {
                        "Validated static result".to_owned()
                    }
                }
                (_, None) => "Validated static result".to_owned(),
            };
            let retry: Element<'_, Message> = if matches!(
                (&self.dynamic_presentation, &self.dynamic_build),
                (
                    Some(DynamicPresentationOutcome::Static {
                        retryable: true,
                        ..
                    }),
                    DynamicBuildState::Idle | DynamicBuildState::Failed(_),
                )
            ) {
                button(text("Retry mechanism"))
                    .on_press(Message::RetryDynamicPresentation)
                    .style(theme::secondary_button)
                    .into()
            } else {
                space().height(Length::Shrink).into()
            };
            let mut species = row![].spacing(spacing::XXS).align_y(Center).width(Fill);
            for (species_capability, term) in outcome
                .reactants()
                .iter()
                .zip(outcome.declaration().reactants())
                .chain(
                    outcome
                        .products()
                        .iter()
                        .zip(outcome.declaration().products()),
                )
            {
                species = species.push(dynamic_species_theatre_card(
                    species_capability,
                    term,
                    self.dynamic_theatre_phase,
                ));
            }
            let observation_copy = outcome
                .claim()
                .observations
                .iter()
                .map(|observation| {
                    let action = match observation.predicate {
                        agent::ClaimObservationPredicate::Evolves => "evolves",
                        agent::ClaimObservationPredicate::Disappears => "disappears",
                        agent::ClaimObservationPredicate::Forms => "forms",
                        agent::ClaimObservationPredicate::Colour => "colour",
                    };
                    observation.value.as_ref().map_or_else(
                        || format!("{} {action}", observation.subject),
                        |value| format!("{} {action}: {value}", observation.subject),
                    )
                })
                .collect::<Vec<_>>()
                .join("  ·  ");
            let diagnostic = match &self.dynamic_presentation {
                Some(DynamicPresentationOutcome::Static { diagnostic, .. }) => {
                    Some(format!("Presentation: {diagnostic}"))
                }
                _ => match &self.dynamic_build {
                    DynamicBuildState::Failed(error) => Some(format!("Build: {error}")),
                    _ => None,
                },
            };
            let details_button: Element<'_, Message> = diagnostic.as_ref().map_or_else(
                || space().height(Length::Shrink).into(),
                |_| {
                    button(text(if self.dynamic_details_open {
                        "Hide details"
                    } else {
                        "Details"
                    }))
                    .on_press(Message::ToggleDynamicDetails)
                    .style(theme::secondary_button)
                    .into()
                },
            );
            let details: Element<'_, Message> = if self.dynamic_details_open {
                diagnostic.map_or_else(
                    || space().height(Length::Shrink).into(),
                    |diagnostic| {
                        text(diagnostic)
                            .size(type_scale::MICRO)
                            .color(color::MUTED)
                            .into()
                    },
                )
            } else {
                space().height(Length::Shrink).into()
            };
            return container(
                column![
                    row![
                        text(trust).size(type_scale::MICRO).color(color::SUCCESS),
                        space().width(Fill),
                        text("VIRTUAL MODEL")
                            .size(type_scale::MICRO)
                            .color(color::WARNING),
                    ],
                    text(nomenclature::display_equation(outcome.equation()))
                        .size(type_scale::BODY_LARGE)
                        .color(color::TEXT),
                    text(outcome.claim().required_context.as_str())
                        .size(type_scale::CAPTION)
                        .color(color::MUTED),
                    species,
                    text(observation_copy)
                        .size(type_scale::CAPTION)
                        .color(color::TEXT_SOFT),
                    text(presentation)
                        .size(type_scale::CAPTION)
                        .color(color::TEXT_SOFT),
                    text(self.dynamic_latency_summary())
                        .size(type_scale::MICRO)
                        .color(color::MUTED),
                    retry,
                    details_button,
                    details,
                ]
                .spacing(spacing::XXS),
            )
            .style(theme::inset)
            .padding(spacing::SM)
            .width(Fill)
            .into();
        }
        if let Some(claim) = &self.dynamic_claim {
            let (title, detail) = match claim.disposition {
                ClaimDisposition::NoReaction => {
                    ("No supported reaction", claim.required_context.as_str())
                }
                ClaimDisposition::Ambiguous => (
                    "More detail is needed",
                    claim
                        .ambiguity
                        .as_ref()
                        .map_or(claim.required_context.as_str(), |value| {
                            value.summary.as_str()
                        }),
                ),
                ClaimDisposition::Unsupported => (
                    "Outside the current chemistry capability",
                    claim.required_context.as_str(),
                ),
                ClaimDisposition::Reaction => ("Outcome claim", claim.required_context.as_str()),
            };
            return container(
                column![
                    text(title).size(type_scale::BODY_LARGE).color(color::TEXT),
                    text(detail).size(type_scale::CAPTION).color(color::MUTED),
                ]
                .spacing(spacing::XXS),
            )
            .style(theme::inset)
            .padding(spacing::SM)
            .width(Fill)
            .into();
        }
        space().height(Length::Shrink).into()
    }

    fn dynamic_latency_summary(&self) -> String {
        let mut milestones = Vec::new();
        for (label, value) in [
            ("claim", self.dynamic_latency.claim_ms),
            ("static", self.dynamic_latency.static_outcome_ms),
            ("evidence", self.dynamic_latency.evidence_ms),
            ("mechanism", self.dynamic_latency.mechanism_ms),
            (
                "reviewed animation",
                self.dynamic_latency.reviewed_animation_ms,
            ),
        ] {
            if let Some(milliseconds) = value {
                milestones.push(format!("{label} {milliseconds} ms"));
            }
        }
        if milestones.is_empty() {
            "Timing pending".into()
        } else {
            milestones.join(" · ")
        }
    }

    fn dynamic_identity_choice_view(&self) -> Element<'_, Message> {
        let Some(choice) = &self.dynamic_identity_choice else {
            return space().height(Length::Shrink).into();
        };
        let input = &choice.request.reactants[choice.ambiguity.reactant_index];
        let mut alternatives = column![
            text(format!("Choose the identity for {}", input.display))
                .size(type_scale::BODY_LARGE)
                .color(color::TEXT),
            text("The request is preserved; ChemSpec will not guess between these structures.")
                .size(type_scale::CAPTION)
                .color(color::MUTED),
        ]
        .spacing(spacing::XXS);
        for species in &choice.ambiguity.alternatives {
            alternatives = alternatives.push(
                button(
                    column![
                        text(species.display_name.as_str())
                            .size(type_scale::BODY)
                            .color(color::TEXT),
                        text(format!(
                            "{} · charge {} · {:?}",
                            species.formula_text,
                            species.charge.value(),
                            species.phase
                        ))
                        .size(type_scale::MICRO)
                        .color(color::MUTED),
                    ]
                    .spacing(spacing::XXS),
                )
                .on_press(Message::DynamicIdentitySelected {
                    reactant_index: choice.ambiguity.reactant_index,
                    species_id: species.id.clone(),
                })
                .style(theme::secondary_button)
                .width(Fill),
            );
        }
        container(alternatives)
            .style(theme::inset)
            .padding(spacing::SM)
            .width(Fill)
            .into()
    }

    /// Stage 1: the question sentence above the full periodic table, with no
    /// chrome competing for attention.
    #[allow(clippy::too_many_lines)]
    fn builder_view(&self, size: Size) -> Element<'_, Message> {
        let compact = size.width < breakpoint::MOBILE;
        let progress = self
            .dynamic_progress_label()
            .map_or_else(String::new, |label| format!(" · {label}"));

        let composer = reactant_composer::view(
            &self.reactant_composer,
            periodic_table::dragging_atomic_number(&self.periodic_table),
            match &self.dynamic_build {
                DynamicBuildState::Idle => None,
                DynamicBuildState::Running {
                    elapsed_seconds,
                    stage,
                    ..
                } => Some(match stage {
                    DynamicBuildStage::Claim => format!(
                        "Checking the outcome claim{progress}… {elapsed_seconds}s"
                    ),
                    DynamicBuildStage::Presentation => format!(
                        "The balanced result is ready; checking animation capability{progress}… {elapsed_seconds}s"
                    ),
                }),
                DynamicBuildState::Failed(_) => Some("Couldn’t build this result".to_owned()),
            },
            self.local_mode(),
            compact,
        )
        .map(Message::ReactantComposer);
        let library = container(
            periodic_table::view(&self.periodic_table, compact).map(Message::PeriodicTable),
        )
        .width(Fill)
        .height(Fill);

        let dynamic_busy = matches!(self.dynamic_build, DynamicBuildState::Running { .. });
        let (first, second) = reactant_composer::reactants(&self.reactant_composer);
        let context_controls: Element<'_, Message> = if !first.is_empty() && second.is_empty() {
            let mut controls = row![
                text("ONE REACTANT")
                    .size(type_scale::MICRO)
                    .color(color::MUTED)
            ]
            .spacing(spacing::XS)
            .align_y(Center);
            for context in DynamicRequestContext::ALL {
                controls = controls.push(
                    button(text(context.label()))
                        .on_press_maybe(
                            (!dynamic_busy)
                                .then_some(Message::DynamicContextSelected(Some(context))),
                        )
                        .style(if self.dynamic_context == Some(context) {
                            theme::primary_button
                        } else {
                            theme::secondary_button
                        }),
                );
            }
            controls = controls.push(space().width(Fill)).push(
                button(text("Build with context  →"))
                    .on_press_maybe(
                        (!dynamic_busy && self.dynamic_context.is_some())
                            .then_some(Message::StartContextReaction),
                    )
                    .style(theme::primary_button),
            );
            controls.into()
        } else {
            space().height(Length::Shrink).into()
        };
        // Fast/Researcher only tunes the model claim budget; Local Mode has
        // no model, so the toggle disappears entirely.
        let claim_mode_toggle: Element<'_, Message> = if self.local_mode() {
            space().width(Fill).into()
        } else {
            row![
                text("MODE").size(type_scale::MICRO).color(color::MUTED),
                button(text("Fast"))
                    .on_press_maybe(
                        (!dynamic_busy && self.claim_mode != ClaimMode::Fast)
                            .then_some(Message::ClaimModeSelected(ClaimMode::Fast)),
                    )
                    .style(if self.claim_mode == ClaimMode::Fast {
                        theme::primary_button
                    } else {
                        theme::secondary_button
                    }),
                button(text("Researcher"))
                    .on_press_maybe(
                        (!dynamic_busy && self.claim_mode != ClaimMode::Researcher)
                            .then_some(Message::ClaimModeSelected(ClaimMode::Researcher)),
                    )
                    .style(if self.claim_mode == ClaimMode::Researcher {
                        theme::primary_button
                    } else {
                        theme::secondary_button
                    }),
                space().width(Fill),
            ]
            .spacing(spacing::XS)
            .align_y(Center)
            .into()
        };
        let mode_toggle = row![
            claim_mode_toggle,
            button(text("Cancel"))
                .on_press_maybe(
                    matches!(self.dynamic_build, DynamicBuildState::Running { .. })
                        .then_some(Message::CancelDynamicWork),
                )
                .style(theme::secondary_button),
        ]
        .spacing(spacing::XS)
        .align_y(Center);
        let result = self.dynamic_result_view();
        let identity_choice = self.dynamic_identity_choice_view();
        let build_details: Element<'_, Message> =
            if let DynamicBuildState::Failed(error) = &self.dynamic_build {
                let detail: Element<'_, Message> = if self.dynamic_details_open {
                    text(error)
                        .size(type_scale::MICRO)
                        .color(color::MUTED)
                        .into()
                } else {
                    space().height(Length::Shrink).into()
                };
                column![
                    button(text(if self.dynamic_details_open {
                        "Hide details"
                    } else {
                        "Details"
                    }))
                    .on_press(Message::ToggleDynamicDetails)
                    .style(theme::secondary_button),
                    detail,
                ]
                .spacing(spacing::XXS)
                .into()
            } else {
                space().height(Length::Shrink).into()
            };
        let application = container(
            column![
                mode_toggle,
                context_controls,
                composer,
                build_details,
                identity_choice,
                result,
                library
            ]
            .spacing(spacing::XS)
            .width(Fill)
            .height(Fill),
        )
        .style(theme::app_background)
        .padding(if compact { spacing::XS } else { spacing::SM })
        .width(Fill)
        .height(Fill);
        let drag_overlay =
            periodic_table::drag_overlay(&self.periodic_table, size).map(Message::PeriodicTable);

        stack![application, drag_overlay]
            .width(Fill)
            .height(Fill)
            .clip(false)
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type DraftCase = (&'static str, &'static [u8], &'static [u8]);

    // Independently authored UI fixtures. These deliberately do not use
    // ReactionRequest::participants(), so a wrong production mapping cannot
    // make the routing test prove itself.
    const SUPPORTED_DRAFT_CASES: [DraftCase; 36] = [
        ("alkali-water-lithium", &[3], &[1, 1, 8]),
        ("alkali-water-sodium", &[11], &[1, 1, 8]),
        ("alkali-water-potassium", &[19], &[1, 1, 8]),
        (
            "silver-halide-precipitation-chloride",
            &[47, 7, 8, 8, 8],
            &[11, 17],
        ),
        (
            "silver-halide-precipitation-bromide",
            &[47, 7, 8, 8, 8],
            &[11, 35],
        ),
        (
            "silver-halide-precipitation-iodide",
            &[47, 7, 8, 8, 8],
            &[11, 53],
        ),
        ("acid-base-lithium-chloride", &[1, 17], &[3, 8, 1]),
        ("acid-base-lithium-bromide", &[1, 35], &[3, 8, 1]),
        ("acid-base-lithium-iodide", &[1, 53], &[3, 8, 1]),
        ("acid-base-sodium-chloride", &[1, 17], &[11, 8, 1]),
        ("acid-base-sodium-bromide", &[1, 35], &[11, 8, 1]),
        ("acid-base-sodium-iodide", &[1, 53], &[11, 8, 1]),
        ("acid-base-potassium-chloride", &[1, 17], &[19, 8, 1]),
        ("acid-base-potassium-bromide", &[1, 35], &[19, 8, 1]),
        ("acid-base-potassium-iodide", &[1, 53], &[19, 8, 1]),
        (
            "acid-bicarbonate-lithium-chloride",
            &[1, 17],
            &[3, 1, 6, 8, 8, 8],
        ),
        (
            "acid-bicarbonate-lithium-bromide",
            &[1, 35],
            &[3, 1, 6, 8, 8, 8],
        ),
        (
            "acid-bicarbonate-lithium-iodide",
            &[1, 53],
            &[3, 1, 6, 8, 8, 8],
        ),
        (
            "acid-bicarbonate-sodium-chloride",
            &[1, 17],
            &[11, 1, 6, 8, 8, 8],
        ),
        (
            "acid-bicarbonate-sodium-bromide",
            &[1, 35],
            &[11, 1, 6, 8, 8, 8],
        ),
        (
            "acid-bicarbonate-sodium-iodide",
            &[1, 53],
            &[11, 1, 6, 8, 8, 8],
        ),
        (
            "acid-bicarbonate-potassium-chloride",
            &[1, 17],
            &[19, 1, 6, 8, 8, 8],
        ),
        (
            "acid-bicarbonate-potassium-bromide",
            &[1, 35],
            &[19, 1, 6, 8, 8, 8],
        ),
        (
            "acid-bicarbonate-potassium-iodide",
            &[1, 53],
            &[19, 1, 6, 8, 8, 8],
        ),
        (
            "acid-carbonate-lithium-chloride",
            &[1, 17],
            &[3, 3, 6, 8, 8, 8],
        ),
        (
            "acid-carbonate-lithium-bromide",
            &[1, 35],
            &[3, 3, 6, 8, 8, 8],
        ),
        (
            "acid-carbonate-lithium-iodide",
            &[1, 53],
            &[3, 3, 6, 8, 8, 8],
        ),
        (
            "acid-carbonate-sodium-chloride",
            &[1, 17],
            &[11, 11, 6, 8, 8, 8],
        ),
        (
            "acid-carbonate-sodium-bromide",
            &[1, 35],
            &[11, 11, 6, 8, 8, 8],
        ),
        (
            "acid-carbonate-sodium-iodide",
            &[1, 53],
            &[11, 11, 6, 8, 8, 8],
        ),
        (
            "acid-carbonate-potassium-chloride",
            &[1, 17],
            &[19, 19, 6, 8, 8, 8],
        ),
        (
            "acid-carbonate-potassium-bromide",
            &[1, 35],
            &[19, 19, 6, 8, 8, 8],
        ),
        (
            "acid-carbonate-potassium-iodide",
            &[1, 53],
            &[19, 19, 6, 8, 8, 8],
        ),
        (
            "halogen-displacement-chlorine-bromide",
            &[17, 17],
            &[11, 35],
        ),
        ("halogen-displacement-chlorine-iodide", &[17, 17], &[11, 53]),
        ("halogen-displacement-bromine-iodide", &[35, 35], &[11, 53]),
    ];

    fn dynamic_lithium_static() -> ValidatedStaticOutcome {
        let catalogue = chemistry::trusted_catalogue().expect("trusted catalogue");
        let identities = reviewed_species_registry(catalogue).expect("identities");
        let claim = serde_json::json!({
            "schema_version": 1,
            "disposition": "reaction",
            "products": [
                {"name":"lithium hydroxide","formula":"LiOH","phase":"aqueous","identity_hints":[]},
                {"name":"hydrogen","formula":"H2","phase":"gas","identity_hints":[]}
            ],
            "required_context":"representative educational outcome under the reviewed standard-outcome premise",
            "observations":[], "sources":[], "ambiguity":null
        });
        let claim = ReactionClaim::from_json(
            &serde_json::to_vec(&claim).expect("claim JSON"),
            ClaimMode::Fast,
        )
        .expect("claim");
        let request = ReactionBuildRequest {
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
        };
        let CompiledClaimOutcome::Static(outcome) =
            compile_claim_outcome(&request, claim, &identities).expect("compiled")
        else {
            panic!("static outcome")
        };
        outcome
    }

    #[test]
    fn local_mode_is_preselected_and_continues_without_codex() {
        let mut app = App::default();
        assert_eq!(app.provider, Some(ProviderChoice::Local));
        assert_eq!(app.screen, Screen::ProviderSetup);
        app.update(Message::ProviderContinue);
        assert_eq!(app.screen, Screen::Builder);
    }


    #[test]
    fn uncatalogued_reaction_starts_generation_scoped_codex_build() {
        let mut app = App {
            provider: Some(ProviderChoice::CodexSubscription),
            ..App::default()
        };
        reactant_composer::replace_reactants(&mut app.reactant_composer, [vec![20], vec![1, 1, 8]]);

        app.update(Message::ReactantComposer(
            reactant_composer::Message::StartReactionRequested,
        ));

        assert!(
            matches!(
                app.dynamic_build,
                DynamicBuildState::Running { run_id: 1, .. }
            ),
            "state: {:?}, identity choice: {:?}",
            app.dynamic_build,
            app.dynamic_identity_choice
        );
        assert!(app.validated_frames.is_none());
        assert!(app.dynamic_request.is_some());
        assert!(app.dynamic_identity_choice.is_none());
    }

    #[test]
    fn uncatalogued_sulfuric_acid_pair_starts_a_dynamic_build_with_generated_identities() {
        let mut app = App {
            provider: Some(ProviderChoice::CodexSubscription),
            ..App::default()
        };
        reactant_composer::replace_reactants(
            &mut app.reactant_composer,
            [vec![1, 1, 16, 8, 8, 8, 8], vec![11, 8, 1]],
        );

        app.update(Message::ReactantComposer(
            reactant_composer::Message::StartReactionRequested,
        ));

        assert!(matches!(
            app.dynamic_build,
            DynamicBuildState::Running { run_id: 1, .. }
        ));
        let request = app.dynamic_request.as_ref().expect("dynamic request");
        assert_eq!(request.reactants.len(), 2);
        assert_eq!(request.reactants[0].display, "H₂SO₄");
        assert_eq!(request.reactants[1].display, "NaOH");
        assert!(request.reactants[0].species_id.is_none());
        assert!(request.reactants[1].species_id.is_some());
    }

    #[test]
    fn dynamic_handoff_uses_the_same_elemental_standard_state_as_the_display() {
        for (atomic_number, count) in [
            (1, 2),
            (7, 2),
            (8, 2),
            (9, 2),
            (15, 4),
            (16, 8),
            (17, 2),
            (35, 2),
            (53, 2),
        ] {
            let mut app = App {
                provider: Some(ProviderChoice::CodexSubscription),
                ..App::default()
            };
            reactant_composer::replace_reactants(
                &mut app.reactant_composer,
                [vec![37], vec![atomic_number]],
            );

            let _ = app.start_dynamic_build();

            let request = app.dynamic_request.as_ref().expect("captured request");
            assert_eq!(
                request.reactants[1].atomic_numbers,
                vec![atomic_number; count],
                "standard-state mismatch for atomic number {atomic_number}"
            );
        }
    }

    #[test]
    fn equivalent_reviewed_hydrogen_records_do_not_create_a_user_facing_choice() {
        let mut app = App {
            provider: Some(ProviderChoice::CodexSubscription),
            ..App::default()
        };
        reactant_composer::replace_reactants(&mut app.reactant_composer, [vec![37], vec![1]]);

        let _ = app.start_dynamic_build();

        assert!(app.dynamic_identity_choice.is_none());
        assert!(matches!(
            app.dynamic_build,
            DynamicBuildState::Running {
                stage: DynamicBuildStage::Claim,
                ..
            }
        ));
        assert_eq!(
            app.dynamic_request.as_ref().expect("request").reactants[1].atomic_numbers,
            vec![1, 1]
        );
    }

    #[test]
    fn builder_keyboard_shortcuts_cover_selection_edit_run_and_cancel() {
        use iced::keyboard::{Key, Modifiers, key::Named};

        assert!(matches!(
            builder_shortcut(
                Screen::Builder,
                &Key::Character("2".into()),
                Modifiers::COMMAND
            ),
            Some(Message::ReactantComposer(
                reactant_composer::Message::SelectReactant(
                    reactant_composer::ActiveReactant::Second
                )
            ))
        ));
        assert!(matches!(
            builder_shortcut(
                Screen::Builder,
                &Key::Character("z".into()),
                Modifiers::COMMAND
            ),
            Some(Message::ReactantComposer(reactant_composer::Message::Undo))
        ));
        assert!(matches!(
            builder_shortcut(
                Screen::Builder,
                &Key::Named(Named::Enter),
                Modifiers::COMMAND
            ),
            Some(Message::ReactantComposer(
                reactant_composer::Message::StartReactionRequested
            ))
        ));
        assert!(matches!(
            builder_shortcut(
                Screen::Builder,
                &Key::Named(Named::Escape),
                Modifiers::empty()
            ),
            Some(Message::CancelDynamicWork)
        ));
        assert!(
            builder_shortcut(
                Screen::Structural2d,
                &Key::Named(Named::Escape),
                Modifiers::empty()
            )
            .is_none()
        );
    }

    #[test]
    fn native_window_title_exposes_builder_state_without_changing_initial_smoke_title() {
        let mut app = App {
            smoke_mode: Some(SmokeMode::Builder),
            screen: Screen::Builder,
            ..App::default()
        };
        assert_eq!(app.title(), "ChemSpec Agent Smoke — Builder");

        reactant_composer::replace_reactants(&mut app.reactant_composer, [vec![37], vec![1]]);

        let title = app.title();
        assert!(title.starts_with("ChemSpec Agent Smoke — Builder"));
        assert!(title.contains("Reactants Rb + H₂; idle"));
    }

    #[test]
    fn static_completion_is_visible_before_presentation_enrichment() {
        let outcome = dynamic_lithium_static();
        let mut app = App {
            validated_frames: None,
            dynamic_request: Some(ReactionBuildRequest {
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
            }),
            dynamic_build: DynamicBuildState::Running {
                run_id: 4,
                elapsed_seconds: 2,
                stage: DynamicBuildStage::Claim,
            },
            dynamic_cancellation: Some(Arc::new(AtomicBool::new(false))),
            ..App::default()
        };

        app.update(Message::DynamicClaimFinished {
            run_id: 4,
            result: Box::new(Ok(DynamicClaimStageResult {
                outcome: CompiledClaimOutcome::Static(outcome),
                presentation: None,
                latency: LatencyMilestones {
                    claim_ms: Some(1_200),
                    static_outcome_ms: Some(1_250),
                    ..LatencyMilestones::default()
                },
            })),
        });

        assert!(app.dynamic_static.is_some());
        assert_eq!(app.dynamic_latency.static_outcome_ms, Some(1_250));
        assert!(app.validated_frames.is_none());
        assert!(matches!(
            app.dynamic_build,
            DynamicBuildState::Running {
                run_id: 4,
                stage: DynamicBuildStage::Presentation,
                ..
            }
        ));

        app.update(Message::DynamicClaimFinished {
            run_id: 4,
            result: Box::new(Err("duplicate completion".into())),
        });
        assert!(app.dynamic_static.is_some());
        assert!(matches!(
            app.dynamic_build,
            DynamicBuildState::Running {
                stage: DynamicBuildStage::Presentation,
                ..
            }
        ));
    }

    #[test]
    fn wait_as_show_motion_runs_only_while_presentation_is_pending() {
        let mut app = App {
            screen: Screen::Builder,
            dynamic_static: Some(dynamic_lithium_static()),
            dynamic_build: DynamicBuildState::Running {
                run_id: 4,
                elapsed_seconds: 0,
                stage: DynamicBuildStage::Presentation,
            },
            ..App::default()
        };

        {
            let _view = app.dynamic_result_view();
        }
        app.update(Message::DynamicTheatreTick);
        assert!(app.dynamic_theatre_phase > 0.0);

        app.dynamic_build = DynamicBuildState::Idle;
        let stopped = app.dynamic_theatre_phase;
        app.update(Message::DynamicTheatreTick);
        assert!((app.dynamic_theatre_phase - stopped).abs() < f32::EPSILON);
    }

    #[test]
    fn retryable_static_presentation_relaunches_only_enrichment() {
        let outcome = dynamic_lithium_static();
        let mut app = App {
            provider: Some(ProviderChoice::CodexSubscription),
            dynamic_request: Some(ReactionBuildRequest {
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
            }),
            dynamic_static: Some(outcome.clone()),
            dynamic_presentation: Some(DynamicPresentationOutcome::Static {
                outcome: Box::new(outcome),
                diagnostic: "structure proposal remained invalid".into(),
                retryable: true,
                attempts: 3,
            }),
            ..App::default()
        };

        app.update(Message::RetryDynamicPresentation);

        assert!(matches!(
            app.dynamic_build,
            DynamicBuildState::Running {
                stage: DynamicBuildStage::Presentation,
                ..
            }
        ));
        assert!(
            app.dynamic_static.is_some(),
            "retry must not discard the validated static outcome"
        );

        // A non-retryable presentation must not relaunch.
        let outcome = dynamic_lithium_static();
        let mut blocked = App {
            provider: Some(ProviderChoice::CodexSubscription),
            dynamic_static: Some(outcome.clone()),
            dynamic_presentation: Some(DynamicPresentationOutcome::Static {
                outcome: Box::new(outcome),
                diagnostic: "multiple reviewed families remain applicable".into(),
                retryable: false,
                attempts: 0,
            }),
            ..App::default()
        };
        blocked.update(Message::RetryDynamicPresentation);
        assert!(matches!(blocked.dynamic_build, DynamicBuildState::Idle));
    }

    #[test]
    fn regenerate_bypasses_cache_in_a_new_generation() {
        let request = ReactionBuildRequest {
            reactants: [
                ReactantInput {
                    display: "Rb".to_owned(),
                    atomic_numbers: vec![37],
                    species_id: None,
                },
                ReactantInput {
                    display: "H2O".to_owned(),
                    atomic_numbers: vec![1, 1, 8],
                    species_id: None,
                },
            ]
            .to_vec(),
            selected_context: None,
        };
        let mut app = App {
            screen: Screen::Structural2d,
            provider: Some(ProviderChoice::CodexSubscription),
            dynamic_request: Some(request.clone()),
            ..App::default()
        };

        app.update(Message::RegenerateDynamicReaction);

        assert_eq!(app.screen, Screen::Builder);
        let rebuilt = app.dynamic_request.as_ref().expect("retained request");
        assert_eq!(rebuilt.reactants[0].display, request.reactants[0].display);
        assert_eq!(
            rebuilt.reactants[0].atomic_numbers,
            request.reactants[0].atomic_numbers
        );
        assert!(
            rebuilt
                .reactants
                .iter()
                .all(|reactant| reactant.species_id.is_some())
        );
        assert!(matches!(
            app.dynamic_build,
            DynamicBuildState::Running { run_id: 1, .. }
        ));
    }

    #[test]
    fn stale_dynamic_completion_cannot_replace_current_build() {
        let mut app = App {
            dynamic_build: DynamicBuildState::Running {
                run_id: 9,
                elapsed_seconds: 12,
                stage: DynamicBuildStage::Claim,
            },
            ..App::default()
        };

        app.update(Message::DynamicClaimFinished {
            run_id: 8,
            result: Box::new(Err("stale failure".to_owned())),
        });

        assert!(matches!(
            app.dynamic_build,
            DynamicBuildState::Running { run_id: 9, .. }
        ));
    }

    #[test]
    fn normalized_provider_progress_is_generation_scoped_and_visible() {
        let (sender, receiver) = mpsc::channel();
        let mut app = App {
            dynamic_build: DynamicBuildState::Running {
                run_id: 9,
                elapsed_seconds: 0,
                stage: DynamicBuildStage::Claim,
            },
            dynamic_progress_receiver: Some(receiver),
            ..App::default()
        };
        sender
            .send(CodexProgressEvent {
                stage: CodexProgressStage::SearchingSources,
                elapsed_ms: 42,
            })
            .expect("progress event");

        app.update(Message::DynamicBuildTick { run_id: 8 });
        assert!(app.dynamic_progress.is_none());
        app.update(Message::DynamicBuildTick { run_id: 9 });
        assert_eq!(
            app.dynamic_progress_label(),
            Some("checking the supporting evidence")
        );
        assert!(matches!(
            app.dynamic_build,
            DynamicBuildState::Running {
                elapsed_seconds: 1,
                ..
            }
        ));
    }

    #[test]
    fn cancelling_claim_work_terminates_generation_and_rejects_late_completion() {
        let cancellation = Arc::new(AtomicBool::new(false));
        let mut app = App {
            dynamic_build: DynamicBuildState::Running {
                run_id: 9,
                elapsed_seconds: 3,
                stage: DynamicBuildStage::Claim,
            },
            dynamic_cancellation: Some(cancellation.clone()),
            next_dynamic_run_id: 10,
            ..App::default()
        };

        app.update(Message::CancelDynamicWork);

        assert!(cancellation.load(Ordering::Relaxed));
        assert!(matches!(
            app.dynamic_build,
            DynamicBuildState::Failed(ref error) if error == "Cancelled by the learner"
        ));
        assert_eq!(app.next_dynamic_run_id, 11);
        app.update(Message::DynamicClaimFinished {
            run_id: 9,
            result: Box::new(Err("late completion".into())),
        });
        assert!(matches!(
            app.dynamic_build,
            DynamicBuildState::Failed(ref error) if error == "Cancelled by the learner"
        ));
    }

    #[test]
    fn cancelling_optional_presentation_preserves_static_result() {
        let outcome = dynamic_lithium_static();
        let cancellation = Arc::new(AtomicBool::new(false));
        let mut app = App {
            dynamic_static: Some(outcome),
            validated_frames: None,
            dynamic_build: DynamicBuildState::Running {
                run_id: 4,
                elapsed_seconds: 1,
                stage: DynamicBuildStage::Presentation,
            },
            dynamic_cancellation: Some(cancellation.clone()),
            ..App::default()
        };

        app.update(Message::CancelDynamicWork);

        assert!(cancellation.load(Ordering::Relaxed));
        assert!(app.dynamic_static.is_some());
        assert!(app.validated_frames.is_none());
        assert!(matches!(
            app.dynamic_presentation,
            Some(DynamicPresentationOutcome::Static {
                retryable: true,
                ..
            })
        ));
    }

    #[test]
    fn selecting_identity_preserves_request_and_starts_the_same_build() {
        let catalogue = chemistry::trusted_catalogue().expect("trusted catalogue");
        let identities = reviewed_species_registry(catalogue).expect("identities");
        let lithium = identities
            .records()
            .values()
            .find(|species| species.formula_text == "Li")
            .expect("lithium")
            .clone();
        let sodium = identities
            .records()
            .values()
            .find(|species| species.formula_text == "Na")
            .expect("sodium")
            .clone();
        let water = identities
            .records()
            .values()
            .find(|species| species.formula_text == "H2O")
            .expect("water")
            .clone();
        let request = ReactionBuildRequest {
            reactants: [
                ReactantInput {
                    display: "Li".into(),
                    atomic_numbers: vec![3],
                    species_id: None,
                },
                ReactantInput {
                    display: "H2O".into(),
                    atomic_numbers: vec![1, 1, 8],
                    species_id: Some(water.id),
                },
            ]
            .to_vec(),
            selected_context: None,
        };
        let lithium_id = lithium.id.clone();
        let mut app = App {
            provider: Some(ProviderChoice::CodexSubscription),
            dynamic_request: Some(request.clone()),
            dynamic_identity_choice: Some(DynamicIdentityChoice {
                request,
                ambiguity: ReactantIdentityAmbiguity {
                    reactant_index: 0,
                    query: chem_domain::SpeciesQuery {
                        name: None,
                        formula: Some("Li".into()),
                        charge: None,
                        phase: None,
                        external_identifier: None,
                    },
                    alternatives: vec![lithium, sodium],
                },
            }),
            ..App::default()
        };

        app.update(Message::DynamicIdentitySelected {
            reactant_index: 0,
            species_id: lithium_id.clone(),
        });

        assert!(app.dynamic_identity_choice.is_none());
        assert_eq!(
            app.dynamic_request.as_ref().unwrap().reactants[0].species_id,
            Some(lithium_id)
        );
        assert!(matches!(
            app.dynamic_build,
            DynamicBuildState::Running { .. }
        ));
    }

    #[test]
    fn adaptive_zoom_scales_windows_beyond_the_design_size() {
        // At or below the design size the layout stays 1:1.
        assert!((adaptive_zoom(DESIGN_SIZE, 1.0) - 1.0).abs() < f32::EPSILON);
        assert!((adaptive_zoom(Size::new(560.0, 760.0), 1.0) - 1.0).abs() < f32::EPSILON);

        // A 32in 4K-class window zooms by its most constrained axis (height).
        let zoom = adaptive_zoom(Size::new(2_650.0, 1_490.0), 1.0);
        assert!((zoom - 1_490.0 / DESIGN_SIZE.height).abs() < 0.001);

        // Zoom never exceeds the cap, however large the window.
        assert!((adaptive_zoom(Size::new(7_680.0, 4_320.0), 1.0) - MAX_UI_ZOOM).abs() < 0.001);
    }

    #[test]
    fn adaptive_zoom_is_stable_across_already_zoomed_resize_events() {
        // Resize events report design units (physical over total scale), so a
        // window that settled on a zoom keeps it: 2880x1800 physical reads as
        // 1440x900 under zoom 2.0 and recomputes to exactly 2.0 again.
        let settled = adaptive_zoom(Size::new(2_880.0, 1_800.0), 1.0);
        assert!((settled - 2.0).abs() < f32::EPSILON);
        let recomputed = adaptive_zoom(Size::new(1_440.0, 900.0), settled);
        assert!((recomputed - settled).abs() < f32::EPSILON);
    }

    #[test]
    fn smoke_arguments_select_a_unique_mode_and_window_title() {
        assert_eq!(
            SmokeMode::from_argument("--structural-2d-smoke"),
            Some(SmokeMode::Structural2d)
        );
        assert_eq!(
            SmokeMode::from_argument("--structural-3d-smoke"),
            Some(SmokeMode::Structural3d)
        );
        assert_eq!(SmokeMode::from_argument("--not-a-smoke"), None);
        assert_eq!(
            smoke_request_from_argument("--smoke-reaction=silver-halide-precipitation-bromide")
                .and_then(Result::ok)
                .map(chemistry::ReactionRequest::family),
            Some(chemistry::ReactionFamily::SilverHalidePrecipitation),
        );
        assert!(
            smoke_request_from_argument("--smoke-reaction=not-supported")
                .is_some_and(|request| request.is_err())
        );
        assert!(
            validate_smoke_request_from_argument(
                "--validate-smoke-reaction=acid-base-sodium-chloride"
            )
            .is_some_and(|request| request.is_ok())
        );
        assert!(
            validate_structural_smoke_arguments([
                "chemspec-app",
                "--structural-3d-smoke",
                "--smoke-reaction=acid-base-sodium-chloride",
            ])
            .is_ok()
        );
        assert!(
            validate_structural_smoke_arguments([
                "chemspec-app",
                "--structural-3d-smoke",
                "--smoke-reaction=not-supported",
            ])
            .is_err()
        );

        let mut app = App::default();
        assert_eq!(app.title(), "ChemSpec — reaction builder");
        app.smoke_mode = Some(SmokeMode::Structural2d);
        assert_eq!(app.title(), "ChemSpec Agent Smoke — Structural 2D");
        app.smoke_mode = Some(SmokeMode::Structural3d);
        assert_eq!(app.title(), "ChemSpec Agent Smoke — Structural 3D");
    }

    #[test]
    fn api_key_provider_remains_blocked_until_dynamic_api_is_connected() {
        let mut app = App::default();
        assert_eq!(app.screen, Screen::ProviderSetup);
        app.update(Message::ProviderSelected(ProviderChoice::ApiKey));
        app.update(Message::ProviderContinue);
        assert_eq!(app.screen, Screen::ProviderSetup);
        app.update(Message::ApiKeyChanged(
            "sk-proj-abcdefghijklmnopqrstuvwxyz0123456789".to_owned(),
        ));
        app.update(Message::ProviderContinue);
        assert_eq!(app.screen, Screen::ProviderSetup);
    }

    #[test]
    fn api_key_format_rejects_obvious_invalid_values() {
        assert!(!api_key_format_is_valid(""));
        assert!(!api_key_format_is_valid("sk-short"));
        assert!(!api_key_format_is_valid(
            "not-an-openai-key-abcdefghijklmnopqrstuvwxyz"
        ));
        assert!(!api_key_format_is_valid(
            "sk-proj-abcdefghijklmnopqrstuvwxyz with-space"
        ));
        assert!(api_key_format_is_valid(
            "sk-proj-abcdefghijklmnopqrstuvwxyz0123456789"
        ));
    }

    #[test]
    fn guided_animation_is_compiled_from_the_current_trusted_generation() {
        let mut app = App::default();
        let expected = app
            .validated_frames
            .as_ref()
            .expect("canonical frames validate")
            .digest()
            .expect("frame digest is available");

        app.open_structural_animation();

        assert_eq!(app.screen, Screen::Structural2d);
        let animation = app
            .structural_animation
            .as_ref()
            .expect("animation planning succeeds");
        assert_eq!(animation.educational_plan.id, expected);
        assert_eq!(animation.real_world_plan.reaction, expected);
    }

    #[test]
    fn educational_context_does_not_repeat_what_changed_copy() {
        for request in chemistry::ReactionRequest::ALL {
            let run = chemistry::run(request).expect("pinned request validates");
            let plan = compile_educational_plan(run.frames()).expect("educational plan compiles");
            let mut compared = 0;

            for scene in &plan.scenes {
                let context = scene.cues.iter().find_map(|cue| match cue {
                    chem_presentation::EducationalCue::ShowContext { label } => {
                        Some(label.text.as_str())
                    }
                    _ => None,
                });
                let explanation = scene.cues.iter().find_map(|cue| match cue {
                    chem_presentation::EducationalCue::ShowExplanation { label } => {
                        Some(label.text.as_str())
                    }
                    _ => None,
                });

                if let (Some(context), Some(explanation)) = (context, explanation) {
                    compared += 1;
                    assert_ne!(
                        normalize_copy(context),
                        normalize_copy(explanation),
                        "{request:?} emitted duplicate context and explanation copy",
                    );
                }
            }
            assert!(compared > 0, "{request:?} emitted no narrated scenes");
        }
    }

    fn normalize_copy(copy: &str) -> String {
        copy.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    }

    #[test]
    fn educational_scrubbing_pauses_and_synchronizes_the_trusted_frame() {
        let mut app = App::default();
        app.open_structural_animation();
        let target = app
            .structural_animation
            .as_ref()
            .map(|animation| animation.educational_plan.duration_ms() / 2)
            .expect("animation exists");

        app.update(Message::StructuralTimelineScrubbed(
            u32::try_from(target).expect("fixture timeline fits u32"),
        ));

        let animation = app.structural_animation.as_ref().expect("animation exists");
        let position = animation
            .educational_plan
            .locate(target)
            .expect("timeline position exists");
        let scene = &animation.educational_plan.scenes[position.scene_index];
        assert_eq!(animation.educational_playhead_ms, target);
        assert!(!animation.playing);
        assert_eq!(
            animation.frames.frames()[animation.frame_index]
                .trace()
                .state_digest,
            scene.end_frame
        );
    }

    #[test]
    fn playback_preserves_overshoot_and_restart_resets_both_timelines() {
        let mut app = App::default();
        app.open_structural_animation();
        let first_duration = app
            .structural_animation
            .as_ref()
            .and_then(|animation| animation.educational_plan.scenes.first())
            .map(|scene| u64::from(scene.duration_ms))
            .expect("first scene exists");
        app.seek_educational_timeline(first_duration.saturating_sub(10));
        app.advance_educational_playback(50);

        let animation = app.structural_animation.as_ref().expect("animation exists");
        assert_eq!(animation.educational_playhead_ms, first_duration + 40);

        app.update(Message::StructuralRestarted);
        let animation = app.structural_animation.as_ref().expect("animation exists");
        assert_eq!(animation.educational_playhead_ms, 0);
        assert_eq!(animation.real_world_playhead_ms, 0);
        assert_eq!(animation.frame_index, 0);
        assert!(animation.playing);
    }

    #[test]
    fn macroscopic_scrubbing_uses_the_same_trusted_ordinal() {
        let mut app = App::default();
        app.open_structural_animation();
        app.screen = Screen::Structural3d;
        let target = app
            .structural_animation
            .as_ref()
            .map(|animation| animation.real_world_plan.timeline.duration_ms() / 2)
            .expect("animation exists");

        app.update(Message::StructuralRealWorldTimelineScrubbed(
            u32::try_from(target).expect("fixture timeline fits u32"),
        ));

        let animation = app.structural_animation.as_ref().expect("animation exists");
        let position = animation
            .real_world_plan
            .timeline
            .locate(target)
            .expect("timeline position exists");
        assert_eq!(animation.real_world_playhead_ms, target);
        assert!(!animation.playing);
        assert_eq!(
            animation.frames.frames()[animation.frame_index].ordinal(),
            u32::from(position.ordinal)
        );
    }

    #[test]
    fn selecting_a_request_refreshes_the_trusted_frames() {
        let mut app = App::default();
        for request in [
            chemistry::ReactionRequest::alkali_water(chemistry::AlkaliMetal::Sodium),
            chemistry::ReactionRequest::alkali_water(chemistry::AlkaliMetal::Potassium),
        ] {
            app.select_request(request);
            assert_eq!(app.active_request, request);
            assert!(app.validated_frames.is_some());
            assert!(app.structural_animation.is_none());
        }
    }

    #[test]
    fn newly_integrated_families_open_with_both_presentation_plans() {
        let mut app = App::default();
        let request =
            chemistry::ReactionRequest::silver_halide_precipitation(chemistry::Halogen::Chlorine);
        app.select_request(request);
        app.open_structural_animation();

        assert_eq!(app.screen, Screen::Structural2d);
        assert!(app.structural_error.is_none());
        let animation = app
            .structural_animation
            .as_ref()
            .expect("trusted educational animation compiles");
        assert_eq!(animation.equation, request.equation());
        assert!(!animation.real_world_plan.timeline.beats.is_empty());

        let duration = animation.educational_plan.duration_ms();
        app.seek_educational_timeline(duration);
        app.update(Message::ContinueTo3d);
        assert_eq!(app.screen, Screen::Structural3d);
    }

    #[test]
    fn every_supported_binding_crosses_the_complete_app_path() {
        let mut families = std::collections::BTreeSet::new();
        let mut requests = std::collections::BTreeSet::new();
        for (request_id, first, second) in SUPPORTED_DRAFT_CASES {
            let request = chemistry::ReactionRequest::from_id(request_id)
                .expect("independent fixture names a supported request");
            let mut app = App {
                screen: Screen::Builder,
                ..App::default()
            };
            reactant_composer::replace_reactants(
                &mut app.reactant_composer,
                [first.to_vec(), second.to_vec()],
            );

            app.update(Message::ReactantComposer(
                reactant_composer::Message::StartReactionRequested,
            ));

            assert_eq!(app.active_request, request);
            assert_eq!(app.screen, Screen::Structural2d);
            assert!(app.structural_error.is_none());
            let animation = app
                .structural_animation
                .as_ref()
                .expect("supported draft compiles both presentation plans");
            let digest = animation
                .frames
                .digest()
                .expect("trusted frames have a digest");
            assert_eq!(animation.educational_plan.id, digest);
            assert_eq!(animation.real_world_plan.reaction, digest);
            assert_eq!(animation.equation, request.equation());

            let duration = animation.educational_plan.duration_ms();
            app.seek_educational_timeline(duration);
            app.update(Message::ContinueTo3d);
            assert_eq!(app.screen, Screen::Structural3d);
            families.insert(request.family());
            assert!(requests.insert(request.id()));
        }
        assert_eq!(families.len(), 6);
        assert_eq!(requests.len(), chemistry::ReactionRequest::ALL.len());
    }

    #[test]
    fn ambiguous_reviewed_products_require_an_explicit_choice() {
        let mut app = App {
            screen: Screen::Builder,
            ..App::default()
        };
        reactant_composer::replace_reactants(&mut app.reactant_composer, [vec![26], vec![8]]);

        app.update(Message::ReactantComposer(
            reactant_composer::Message::StartReactionRequested,
        ));

        assert_eq!(app.screen, Screen::OutcomeChoice);
        assert_eq!(app.pending_requests.len(), 3);
        assert!(app.oxygen_assessment.is_none());

        let selected = app.pending_requests[0];
        app.update(Message::OutcomeSelected(selected));
        assert_eq!(app.screen, Screen::Structural2d);
        assert_eq!(app.active_request, selected);
        assert!(app.pending_requests.is_empty());
        assert!(app.structural_animation.is_some());
    }

    #[test]
    fn repeated_if7_changes_share_teaching_scenes_without_losing_operations() {
        let request = chemistry::ReactionRequest::from_id("covalent-i-f-if7")
            .expect("reviewed IF7 request exists");
        let mut app = App::default();
        app.select_request(request);
        app.open_structural_animation();
        let animation = app
            .structural_animation
            .as_ref()
            .expect("IF7 presentation plans compile");
        let group_sizes = animation
            .educational_plan
            .scenes
            .iter()
            .flat_map(|scene| &scene.cues)
            .filter_map(|cue| match cue {
                chem_presentation::EducationalCue::ApplyOperations { operations } => {
                    Some(operations.len())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let largest_group = group_sizes.iter().copied().max().unwrap_or(0);

        assert!(
            largest_group >= 7,
            "equivalent IF7 changes should be grouped"
        );
        assert!(
            animation.educational_plan.duration_ms() <= 60_000,
            "the grouped IF7 explanation should remain under one minute, got {} ms across {group_sizes:?}",
            animation.educational_plan.duration_ms(),
        );
    }

    #[test]
    fn reviewed_oxygen_screening_is_visible_without_fabricating_frames() {
        let mut app = App {
            screen: Screen::Builder,
            ..App::default()
        };
        reactant_composer::replace_reactants(&mut app.reactant_composer, [vec![79], vec![8]]);

        app.update(Message::ReactantComposer(
            reactant_composer::Message::StartReactionRequested,
        ));

        assert_eq!(app.screen, Screen::OutcomeChoice);
        assert!(app.pending_requests.is_empty());
        assert!(app.oxygen_assessment.is_some());
        assert!(app.structural_animation.is_none());
    }

    #[test]
    fn all_responsive_compositions_build() {
        let app = App::default();

        let choice_app = App {
            pending_requests: chemistry::requests_for_drafts(&[53], &[9]),
            screen: Screen::OutcomeChoice,
            ..App::default()
        };

        for size in [
            Size::new(560.0, 620.0),
            Size::new(900.0, 800.0),
            Size::new(1_440.0, 900.0),
        ] {
            let _ = app.builder_view(size);
            let _ = app.provider_setup_view(size);
            let _ = choice_app.outcome_choice_view(size);
        }
    }

    #[test]
    fn stage_one_supported_drafts_open_the_guided_animation_directly() {
        let mut app = App::default();

        app.update(Message::PeriodicTable(periodic_table::Message::Activated(
            3,
        )));
        app.update(Message::ReactantComposer(
            reactant_composer::Message::SlotPressed(reactant_composer::ActiveReactant::Second),
        ));
        app.update(Message::ReactantComposer(
            reactant_composer::Message::SlotReleased(reactant_composer::ActiveReactant::Second),
        ));
        app.update(Message::PeriodicTable(periodic_table::Message::Activated(
            1,
        )));
        app.update(Message::PeriodicTable(periodic_table::Message::Activated(
            1,
        )));
        app.update(Message::PeriodicTable(periodic_table::Message::Activated(
            8,
        )));

        assert_eq!(reactant_composer::reactants(&app.reactant_composer).0, &[3]);
        assert_eq!(
            reactant_composer::reactants(&app.reactant_composer).1,
            &[1, 1, 8]
        );

        app.update(Message::ReactantComposer(
            reactant_composer::Message::StartReactionRequested,
        ));

        assert_eq!(app.screen, Screen::Structural2d);
        assert_eq!(app.active_request, chemistry::ReactionRequest::DEFAULT);
        let animation = app
            .structural_animation
            .as_ref()
            .expect("guided animation compiles from the trusted frames");
        assert!(animation.playing);

        app.update(Message::ScreenSelected(Screen::Builder));
        assert_eq!(app.screen, Screen::Builder);
    }
}
