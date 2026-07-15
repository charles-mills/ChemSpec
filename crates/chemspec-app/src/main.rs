//! `ChemSpec` application shell and reaction-builder entry (`U-101`, `U-106`–`U-112`).
//!
//! Opens on the Stage 1 builder: the learner's question, composed from two
//! reactant drafts over the full periodic table. Chemistry is supplied only
//! through the host-pinned language/kernel boundary.

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
mod structural_3d;
mod theme;

use chem_presentation::{
    EducationalPlan, EducationalSceneKind, EffectProfile, ScenePlan, TimelinePosition,
    compile_educational_plan, compile_real_world_plan,
};
use iced::widget::{
    button, canvas, column, container, responsive, row, rule, slider, space, stack, text,
    text_input,
};
use iced::{Center, Element, Fill, Length, Size, Subscription, Theme};

use theme::{breakpoint, color, space as spacing, type_scale};

fn plan_equation(animation: &StructuralAnimation) -> Option<&str> {
    (!animation.equation.is_empty()).then_some(animation.equation.as_str())
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
    iced::application(launch_state, App::update, App::view)
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
    std::process::Command::new("codex")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

const fn initial_provider(codex_available: bool) -> ProviderChoice {
    if codex_available {
        ProviderChoice::CodexSubscription
    } else {
        ProviderChoice::ApiKey
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
    ScreenSelected(Screen),
    ProviderSelected(ProviderChoice),
    ApiKeyChanged(String),
    ProviderContinue,
    PeriodicTable(periodic_table::Message),
    ReactantComposer(reactant_composer::Message),
    OutcomeSelected(chemistry::ReactionRequest),
    StructuralPlaybackToggled,
    StructuralSpeedChanged,
    StructuralTimelineScrubbed(u32),
    StructuralRealWorldTimelineScrubbed(u32),
    StructuralChapterChanged(i8),
    StructuralRestarted,
    StructuralTick,
    ContinueTo3d,
    ReturnTo2d,
}

#[derive(Debug)]
struct StructuralAnimation {
    frames: chem_kernel::SimulationFrames,
    educational_plan: EducationalPlan,
    real_world_plan: ScenePlan,
    equation: String,
    educational_playhead_ms: u64,
    frame_index: usize,
    real_world_playhead_ms: u64,
    playing: bool,
    playback_speed: PlaybackSpeed,
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
    validated_frames: Option<chem_kernel::SimulationFrames>,
    structural_animation: Option<StructuralAnimation>,
    structural_error: Option<String>,
    /// Interface zoom applied on top of the system scale factor.
    ui_zoom: f32,
}

impl Default for App {
    fn default() -> Self {
        let codex_available = codex_available();
        let active_request = chemistry::ReactionRequest::DEFAULT;
        Self {
            screen: Screen::ProviderSetup,
            smoke_mode: None,
            codex_available,
            provider: Some(initial_provider(codex_available)),
            api_key: String::new(),
            periodic_table: periodic_table::State::default(),
            reactant_composer: reactant_composer::State::default(),
            pending_requests: Vec::new(),
            oxygen_assessment: None,
            active_request,
            validated_frames: chemistry::run(active_request)
                .ok()
                .map(|run| run.frames().clone()),
            structural_animation: None,
            structural_error: None,
            ui_zoom: 1.0,
        }
    }
}

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
    fn title(&self) -> String {
        self.smoke_mode.map_or_else(
            || "ChemSpec — reaction builder".to_owned(),
            |mode| format!("ChemSpec Agent Smoke — {}", mode.title()),
        )
    }

    #[allow(clippy::too_many_lines)]
    fn update(&mut self, message: Message) {
        match message {
            Message::WindowResized(size) => self.ui_zoom = adaptive_zoom(size, self.ui_zoom),
            Message::ScreenSelected(screen) => self.screen = screen,
            Message::ProviderSelected(provider) => self.provider = Some(provider),
            Message::ApiKeyChanged(api_key) => self.api_key = api_key,
            Message::ProviderContinue => {
                let ready = match self.provider {
                    Some(ProviderChoice::CodexSubscription) => self.codex_available,
                    Some(ProviderChoice::ApiKey) => api_key_format_is_valid(&self.api_key),
                    None => false,
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
                self.update_reactant_composer(message);
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
                let elapsed = self
                    .structural_animation
                    .as_ref()
                    .map_or(33, |animation| animation.playback_speed.scale_millis(33));
                if self.screen == Screen::Structural3d {
                    self.advance_real_world_playback(elapsed);
                } else {
                    self.advance_educational_playback(elapsed);
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
    }

    fn update_reactant_composer(&mut self, message: reactant_composer::Message) {
        if !matches!(message, reactant_composer::Message::StartReactionRequested) {
            reactant_composer::update(&mut self.reactant_composer, message);
            return;
        }
        match reactant_composer::resolution(&self.reactant_composer) {
            chemistry::DraftResolution::Supported(request) => {
                self.pending_requests.clear();
                self.oxygen_assessment = None;
                self.select_request(request);
                self.open_structural_animation();
            }
            chemistry::DraftResolution::Multiple(requests) => {
                self.pending_requests = requests;
                self.oxygen_assessment = None;
                self.screen = Screen::OutcomeChoice;
            }
            chemistry::DraftResolution::Screened(assessment) => {
                self.pending_requests.clear();
                self.oxygen_assessment = Some(assessment);
                self.screen = Screen::OutcomeChoice;
            }
            chemistry::DraftResolution::ExplicitlyUnsupported(_)
            | chemistry::DraftResolution::Uncatalogued
            | chemistry::DraftResolution::Unrecognized
            | chemistry::DraftResolution::SystemError(_) => {}
        }
    }

    // The offline fixture crosses the same trusted language/kernel boundary
    // that live provider output must cross later.
    fn select_request(&mut self, request: chemistry::ReactionRequest) {
        self.active_request = request;
        self.validated_frames = chemistry::run(request).ok().map(|run| run.frames().clone());
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
        } else if matches!(self.screen, Screen::Structural2d | Screen::Structural3d)
            && self
                .structural_animation
                .as_ref()
                .is_some_and(|animation| animation.playing)
        {
            iced::time::every(std::time::Duration::from_millis(33)).map(|_| Message::StructuralTick)
        } else {
            Subscription::none()
        };

        Subscription::batch([resize, screen])
    }

    fn view(&self) -> Element<'_, Message> {
        match self.screen {
            Screen::ProviderSetup => responsive(|size| self.provider_setup_view(size)).into(),
            Screen::Builder => responsive(|size| self.builder_view(size)).into(),
            Screen::OutcomeChoice => self.outcome_choice_view(),
            Screen::Structural2d => responsive(|size| self.structural_2d_view(size)).into(),
            Screen::Structural3d => responsive(|size| self.structural_3d_view(size)).into(),
        }
    }

    fn outcome_choice_view(&self) -> Element<'_, Message> {
        use chem_catalogue::{OxygenOutcome, StructuralSupport};

        let back = button(text("← Reactants"))
            .on_press(Message::ScreenSelected(Screen::Builder))
            .padding([spacing::XS, spacing::SM])
            .style(theme::secondary_button);

        let content: Element<'_, Message> = if !self.pending_requests.is_empty() {
            let mut choices = column![
                text("Choose the product")
                    .size(type_scale::DISPLAY)
                    .color(color::TEXT),
            ]
            .spacing(spacing::MD)
            .width(Fill);
            for request in &self.pending_requests {
                choices = choices.push(
                    button(
                        column![
                            text(request.name())
                                .size(type_scale::BODY_LARGE)
                                .color(color::TEXT),
                            text(request.equation())
                                .size(type_scale::CAPTION)
                                .color(color::MUTED),
                        ]
                        .spacing(spacing::XXS)
                        .width(Fill),
                    )
                    .on_press(Message::OutcomeSelected(*request))
                    .padding(spacing::MD)
                    .width(Fill)
                    .style(theme::secondary_button),
                );
            }
            choices.push(back).into()
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
                        text(equation)
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

        container(container(content).width(Fill).max_width(768.0))
            .center(Fill)
            .padding(spacing::XL)
            .style(theme::app_background)
            .into()
    }

    #[allow(clippy::too_many_lines)]
    fn provider_setup_view(&self, size: Size) -> Element<'_, Message> {
        let compact = size.width < breakpoint::MOBILE;
        let codex_selected = self.provider == Some(ProviderChoice::CodexSubscription);
        let api_selected = self.provider == Some(ProviderChoice::ApiKey);

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

        let choices: Element<'_, Message> = column![codex, api].spacing(spacing::SM).into();
        let ready = (codex_selected && self.codex_available)
            || (api_selected && api_key_format_is_valid(&self.api_key));
        let continue_label = if api_selected {
            "Continue with API key"
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

        sections.extend([choices, action]);

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
                .ok_or_else(|| "trusted frames are unavailable".to_owned())?
                .clone();
            let educational_plan =
                compile_educational_plan(&frames).map_err(|error| error.to_string())?;
            let profile = chemistry::presentation_profile(self.active_request, &frames)?;
            let real_world_plan =
                compile_real_world_plan(&frames, &profile).map_err(|error| error.to_string())?;
            Ok::<_, String>(StructuralAnimation {
                frames,
                educational_plan,
                real_world_plan,
                equation: self.active_request.equation(),
                educational_playhead_ms: 0,
                frame_index: 0,
                real_world_playhead_ms: 0,
                playing: true,
                playback_speed: PlaybackSpeed::Normal,
            })
        })();
        match result {
            Ok(animation) => {
                self.structural_animation = Some(animation);
                self.structural_error = None;
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
        let continue_3d: Element<'_, Message> =
            if timeline_position.scene_index + 1 == animation.educational_plan.scenes.len() {
                button(text("View in real life  →"))
                    .on_press(Message::ContinueTo3d)
                    .padding([spacing::XS, spacing::MD])
                    .style(theme::primary_button)
                    .into()
            } else {
                space().width(Length::Shrink).into()
            };

        let equation = plan_equation(animation).map(str::to_owned);
        let scene_context = structural_2d::SceneContext::new(
            educational_scene.kind,
            timeline_position.scene_index,
            animation.educational_plan.scenes.len(),
        )
        .with_equation(equation.clone());
        let diagram = container(
            canvas(structural_2d::Diagram::new(
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
            ))
            .width(Fill)
            .height(Fill),
        )
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
                    column![
                        text("VALIDATED 2D EXPLANATION")
                            .size(type_scale::MICRO)
                            .color(color::ACCENT),
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
                    ]
                    .spacing(spacing::XXS),
                    space().width(Fill),
                    column![
                        text("VALIDATED WITH MODEL ASSUMPTIONS")
                            .size(type_scale::MICRO)
                            .color(color::SUCCESS),
                        text("VIRTUAL MODEL")
                            .size(type_scale::MICRO)
                            .color(color::WARNING),
                    ]
                    .spacing(spacing::XXS),
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
        let annotation = active_annotation.map_or_else(
            || {
                column![
                    text("REVIEWED SCENE")
                        .size(type_scale::MICRO)
                        .color(color::ACCENT),
                    text(real_world_plan.equation.as_str())
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
        let scene_view =
            iced::widget::Shader::new(structural_3d::Scene::new(real_world_plan, moment))
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
                    column![
                        text("VALIDATED MACROSCOPIC VIEW")
                            .size(type_scale::MICRO)
                            .color(color::ACCENT),
                        text("Cinematic real-world approximation")
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
                text(real_world_plan.disclosure.as_str())
                    .size(type_scale::MICRO)
                    .color(color::TEXT_SOFT),
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

    /// Stage 1: the question sentence above the full periodic table, with no
    /// chrome competing for attention.
    fn builder_view(&self, size: Size) -> Element<'_, Message> {
        let compact = size.width < breakpoint::MOBILE;

        let composer = reactant_composer::view(
            &self.reactant_composer,
            periodic_table::dragging_atomic_number(&self.periodic_table),
            compact,
        )
        .map(Message::ReactantComposer);
        let library = container(
            periodic_table::view(&self.periodic_table, compact).map(Message::PeriodicTable),
        )
        .width(Fill)
        .height(Fill);

        let application = container(column![composer, library].width(Fill).height(Fill))
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

    #[test]
    fn initial_provider_follows_codex_availability() {
        assert_eq!(initial_provider(true), ProviderChoice::CodexSubscription);
        assert_eq!(initial_provider(false), ProviderChoice::ApiKey);
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
    fn api_key_provider_requires_an_in_memory_key_before_continuing() {
        let mut app = App::default();
        assert_eq!(app.screen, Screen::ProviderSetup);
        app.update(Message::ProviderSelected(ProviderChoice::ApiKey));
        app.update(Message::ProviderContinue);
        assert_eq!(app.screen, Screen::ProviderSetup);
        app.update(Message::ApiKeyChanged(
            "sk-proj-abcdefghijklmnopqrstuvwxyz0123456789".to_owned(),
        ));
        app.update(Message::ProviderContinue);
        assert_eq!(app.screen, Screen::Builder);
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

        for size in [
            Size::new(560.0, 620.0),
            Size::new(900.0, 800.0),
            Size::new(1_440.0, 900.0),
        ] {
            let _ = app.builder_view(size);
            let _ = app.provider_setup_view(size);
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
