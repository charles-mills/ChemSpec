//! `ChemSpec` application shell and reaction-builder entry (`U-101`, `U-106`–`U-112`).
//!
//! Opens on the Stage 1 element library and preserves the six validated-record
//! regions—request, workflow, source, validation, sources, and simulation.
//! Chemistry is supplied only through the host-pinned language/kernel boundary.

mod chemistry;
mod composition_catalogue;
mod elements;
mod icons;
mod particle_visualization;
mod periodic_table;
mod reactant_composer;
mod reaction_sequence;
mod scene_registry;
mod structural_2d;
mod structural_3d;
mod theme;

use chem_presentation::{
    EducationalPlan, EducationalSceneKind, EffectProfile, ScenePlan, TimelinePosition,
    compile_educational_plan, compile_real_world_plan,
};
use iced::widget::{
    button, canvas, column, container, responsive, row, rule, scrollable, slider, space, stack,
    text, text_editor, text_input,
};
use iced::{Center, Element, Fill, FillPortion, Font, Length, Size, Subscription, Theme};

use theme::{breakpoint, color, space as spacing, type_scale};

fn plan_equation(animation: &StructuralAnimation) -> Option<&str> {
    (!animation.real_world_plan.equation.is_empty())
        .then_some(animation.real_world_plan.equation.as_str())
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

fn main() -> iced::Result {
    iced::application(launch_state, App::update, App::view)
        .title(App::title)
        .subscription(App::subscription)
        .theme(App::theme)
        .window(iced::window::Settings {
            size: Size::new(1_440.0, 900.0),
            min_size: Some(Size::new(560.0, 760.0)),
            position: iced::window::Position::Centered,
            ..iced::window::Settings::default()
        })
        .run()
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
    if let Some(smoke_mode) = smoke_mode {
        app.smoke_mode = Some(smoke_mode);
        app.open_structural_animation();
        if let Some(animation) = &mut app.structural_animation {
            let three_dimensional = smoke_mode == SmokeMode::Structural3d;
            animation.frame_index = 1.min(animation.frames.frames().len().saturating_sub(1));
            if three_dimensional {
                animation.real_world_playhead_ms = animation
                    .real_world_plan
                    .timeline
                    .duration_ms()
                    .saturating_mul(2)
                    / 3;
                if let Some(position) = animation
                    .real_world_plan
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
    ProviderSetup,
    Builder,
    ValidatedRecord,
    Structural2d,
    Structural3d,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SmokeMode {
    Structural2d,
    Structural3d,
}

impl SmokeMode {
    fn from_argument(argument: &str) -> Option<Self> {
        match argument {
            "--structural-2d-smoke" | "--lithium-2d-smoke" => Some(Self::Structural2d),
            "--structural-3d-smoke" | "--lithium-3d-smoke" => Some(Self::Structural3d),
            _ => None,
        }
    }

    const fn title(self) -> &'static str {
        match self {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Overview,
    Source,
    Validation,
    Evidence,
}

impl Section {
    const ALL: [Self; 4] = [
        Self::Overview,
        Self::Source,
        Self::Validation,
        Self::Evidence,
    ];

    const fn label(self, compact: bool) -> &'static str {
        match (self, compact) {
            (Self::Overview, true) => "Run",
            (Self::Overview, false) => "Overview",
            (Self::Source, true) => ".chems",
            (Self::Source, false) => "Source",
            (Self::Validation, true) => "Checks",
            (Self::Validation, false) => "Validation",
            (Self::Evidence, true) => "Sources",
            (Self::Evidence, false) => "Evidence",
        }
    }
}

#[derive(Debug, Clone)]
enum Message {
    ScreenSelected(Screen),
    ProviderSelected(ProviderChoice),
    ApiKeyChanged(String),
    ProviderContinue,
    PeriodicTable(periodic_table::Message),
    ReactantComposer(reactant_composer::Message),
    RequestChanged(String),
    RequestSubmitted,
    SourceEdited(text_editor::Action),
    SourceRevalidate,
    SectionSelected(Section),
    OpenStructural2d,
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
    active_experience: chemistry::Experience,
    request: String,
    source: text_editor::Content,
    validated_frames: Option<chem_kernel::SimulationFrames>,
    validation_error: Option<String>,
    source_stale: bool,
    section: Section,
    structural_animation: Option<StructuralAnimation>,
    structural_error: Option<String>,
}

impl Default for App {
    fn default() -> Self {
        let codex_available = codex_available();
        let active_experience = chemistry::Experience::DEFAULT;
        let (validated_frames, validation_error) = match chemistry::run(active_experience) {
            Ok(run) => (Some(run.frames().clone()), None),
            Err(error) => (None, Some(error.to_owned())),
        };
        Self {
            screen: Screen::ProviderSetup,
            smoke_mode: None,
            codex_available,
            provider: Some(initial_provider(codex_available)),
            api_key: String::new(),
            periodic_table: periodic_table::State::default(),
            reactant_composer: reactant_composer::State::default(),
            active_experience,
            request: active_experience.request().to_owned(),
            source: text_editor::Content::with_text(active_experience.source()),
            validated_frames,
            validation_error,
            source_stale: false,
            section: Section::Overview,
            structural_animation: None,
            structural_error: None,
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
            Message::RequestChanged(request) => self.request = request,
            // The offline fixture crosses the same trusted language/kernel
            // boundary that live provider output must cross later.
            Message::RequestSubmitted => {
                self.revalidate_source();
                if self.validated_frames.is_some() {
                    self.screen = Screen::ValidatedRecord;
                }
            }
            Message::SourceEdited(action) => {
                let is_edit = action.is_edit();
                self.source.perform(action);
                if is_edit {
                    self.source_stale = true;
                    self.validated_frames = None;
                    self.validation_error = None;
                    self.structural_animation = None;
                    self.structural_error = None;
                }
            }
            Message::SourceRevalidate => self.revalidate_source(),
            Message::SectionSelected(section) => self.section = section,
            Message::OpenStructural2d => self.open_structural_animation(),
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
        if !matches!(message, reactant_composer::Message::StartReactionRequested)
            || !reactant_composer::can_start_reaction(&self.reactant_composer)
        {
            reactant_composer::update(&mut self.reactant_composer, message);
            return;
        }
        let (first, second) = reactant_composer::reactants(&self.reactant_composer);
        let Some(experience) = chemistry::experience_for_drafts(first, second) else {
            return;
        };
        self.select_experience(experience);
        self.open_structural_animation();
    }

    fn revalidate_source(&mut self) {
        match chemistry::validate_experience_source(self.active_experience, &self.source.text()) {
            Ok(frames) => {
                self.validated_frames = Some(frames);
                self.validation_error = None;
                self.source_stale = false;
                self.structural_animation = None;
                self.structural_error = None;
            }
            Err(error) => {
                self.validated_frames = None;
                self.validation_error = Some(error);
                self.source_stale = false;
                self.structural_animation = None;
                self.structural_error = None;
            }
        }
    }

    fn select_experience(&mut self, experience: chemistry::Experience) {
        self.active_experience = experience;
        experience.request().clone_into(&mut self.request);
        self.source = text_editor::Content::with_text(experience.source());
        match chemistry::run(experience) {
            Ok(run) => {
                self.validated_frames = Some(run.frames().clone());
                self.validation_error = None;
            }
            Err(error) => {
                self.validated_frames = None;
                self.validation_error = Some(error.to_owned());
            }
        }
        self.source_stale = false;
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
        if self.screen == Screen::Builder {
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
        }
    }

    fn view(&self) -> Element<'_, Message> {
        match self.screen {
            Screen::ProviderSetup => responsive(|size| self.provider_setup_view(size)).into(),
            Screen::Builder => responsive(|size| self.builder_view(size)).into(),
            Screen::ValidatedRecord => responsive(|size| self.responsive_view(size)).into(),
            Screen::Structural2d => responsive(|size| self.structural_2d_view(size)).into(),
            Screen::Structural3d => responsive(|size| self.structural_3d_view(size)).into(),
        }
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
            let last_ordinal = frames
                .frames()
                .last()
                .and_then(|frame| u16::try_from(frame.ordinal()).ok())
                .ok_or_else(|| "trusted frames exceed the presentation range".to_owned())?;
            let profile = chemistry::presentation_profile(self.active_experience, last_ordinal);
            let real_world_plan =
                compile_real_world_plan(&frames, &profile).map_err(|error| error.to_string())?;
            Ok::<_, String>(StructuralAnimation {
                frames,
                educational_plan,
                real_world_plan,
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
        let duration = animation.real_world_plan.timeline.duration_ms();
        animation.real_world_playhead_ms = animation
            .real_world_playhead_ms
            .saturating_add(u64::from(elapsed_ms))
            .min(duration);
        if let Some(position) = animation
            .real_world_plan
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
        animation.real_world_playhead_ms =
            elapsed_ms.min(animation.real_world_plan.timeline.duration_ms());
        if let Some(position) = animation
            .real_world_plan
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
        let Some(moment) = animation
            .real_world_plan
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
        let active_annotation = animation
            .real_world_plan
            .annotations
            .iter()
            .rfind(|annotation| {
                annotation.start_ordinal <= moment.ordinal
                    && moment.ordinal <= annotation.end_ordinal
            });
        let active_effects = animation
            .real_world_plan
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
                    text(animation.real_world_plan.equation.as_str())
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
        let scene_view = iced::widget::Shader::new(structural_3d::Scene::new(
            &animation.real_world_plan,
            moment,
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
        let duration_ms = animation.real_world_plan.timeline.duration_ms();
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
                        animation.real_world_plan.timeline.beats.len()
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
                text(animation.real_world_plan.disclosure.as_str())
                    .size(type_scale::MICRO)
                    .color(color::TEXT_SOFT),
                text(animation.real_world_plan.virtual_only_disclosure.as_str())
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

    fn builder_view(&self, size: Size) -> Element<'_, Message> {
        let compact = size.width < breakpoint::MOBILE;
        let outer_padding = if compact { spacing::XS } else { spacing::SM };

        let library =
            periodic_table::view(&self.periodic_table, compact).map(Message::PeriodicTable);
        let composer = reactant_composer::view(
            &self.reactant_composer,
            periodic_table::dragging_atomic_number(&self.periodic_table),
            compact,
        )
        .map(Message::ReactantComposer);

        let stages: Element<'_, Message> = column![composer, library]
            .spacing(spacing::XS)
            .width(Fill)
            .height(Fill)
            .into();

        let content = column![
            Self::builder_context_bar(compact),
            stages,
            Self::builder_status_bar(compact),
        ]
        .spacing(spacing::XS)
        .height(Fill);

        let application = container(content)
            .style(theme::app_background)
            .padding(outer_padding)
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

    fn responsive_view(&self, size: Size) -> Element<'_, Message> {
        if size.width >= breakpoint::DESKTOP {
            self.desktop_view()
        } else if size.width >= breakpoint::MOBILE {
            self.tablet_view()
        } else {
            self.mobile_view()
        }
    }

    fn desktop_view(&self) -> Element<'_, Message> {
        let workspace = row![
            container(self.simulation_panel(Fill))
                .width(FillPortion(7))
                .height(Fill),
            container(self.inspector(false, Fill))
                .width(FillPortion(5))
                .height(Fill),
        ]
        .spacing(spacing::MD)
        .height(Fill);

        let content = column![
            self.context_bar(false),
            self.request_panel(false),
            workspace,
            Self::status_bar(false),
        ]
        .spacing(spacing::SM)
        .height(Fill);

        Self::application_frame(content.into(), spacing::XL)
    }

    fn tablet_view(&self) -> Element<'_, Message> {
        let content = column![
            self.context_bar(false),
            self.request_panel(false),
            self.simulation_panel(Length::Fixed(480.0)),
            self.inspector(false, Length::Fixed(590.0)),
            Self::status_bar(false),
        ]
        .spacing(spacing::SM);

        Self::scrollable_frame(content.into(), spacing::MD)
    }

    fn mobile_view(&self) -> Element<'_, Message> {
        let content = column![
            self.context_bar(true),
            self.request_panel(true),
            self.simulation_panel(Length::Fixed(420.0)),
            self.inspector(true, Length::Fixed(650.0)),
            Self::status_bar(true),
        ]
        .spacing(spacing::SM);

        Self::scrollable_frame(content.into(), spacing::SM)
    }

    fn application_frame(
        content: Element<'_, Message>,
        outer_padding: f32,
    ) -> Element<'_, Message> {
        container(
            container(content)
                .style(theme::frame)
                .padding(spacing::MD)
                .width(Fill)
                .height(Fill),
        )
        .style(theme::app_background)
        .padding(outer_padding)
        .width(Fill)
        .height(Fill)
        .into()
    }

    fn scrollable_frame(content: Element<'_, Message>, outer_padding: f32) -> Element<'_, Message> {
        let page = container(content)
            .style(theme::frame)
            .padding(spacing::SM)
            .width(Fill);

        container(scrollable(page).width(Fill))
            .style(theme::app_background)
            .padding(outer_padding)
            .width(Fill)
            .height(Fill)
            .into()
    }

    fn context_bar(&self, compact: bool) -> Element<'static, Message> {
        let brand = row![
            container(text("CS").size(type_scale::CAPTION).color(color::ACCENT))
                .style(theme::accent_tint)
                .center_x(34)
                .center_y(30),
            column![
                text(if compact {
                    "CHEMSPEC"
                } else {
                    "CHEMSPEC  /  VALIDATED REACTION WORKSPACE"
                })
                .size(type_scale::MICRO)
                .color(color::TEXT_SOFT),
                text("Virtual chemistry laboratory")
                    .size(type_scale::CAPTION)
                    .color(color::MUTED),
            ]
            .spacing(spacing::XXS),
        ]
        .spacing(spacing::SM)
        .align_y(Center);

        let context = if compact {
            text("AI-REVIEWED FIXTURE")
                .size(type_scale::MICRO)
                .color(color::MUTED)
        } else {
            text(self.active_experience.equation())
                .size(type_scale::BODY)
                .color(color::TEXT_SOFT)
        };

        let builder = button(text(if compact {
            "Build"
        } else {
            "← Reaction builder"
        }))
        .on_press(Message::ScreenSelected(Screen::Builder))
        .padding([spacing::XS, spacing::SM])
        .style(theme::secondary_button);

        container(
            row![brand, space().width(Fill), context, builder]
                .spacing(spacing::SM)
                .align_y(Center),
        )
        .style(theme::chrome)
        .padding([spacing::XS, spacing::SM])
        .width(Fill)
        .into()
    }

    fn builder_context_bar(compact: bool) -> Element<'static, Message> {
        let brand = row![
            container(text("CS").size(type_scale::CAPTION).color(color::ACCENT))
                .style(theme::accent_tint)
                .center_x(34)
                .center_y(30),
            column![
                text("CHEMSPEC  /  REACTION BUILDER")
                    .size(type_scale::MICRO)
                    .color(color::TEXT_SOFT),
                text(if compact {
                    "Stage 1 · Build"
                } else {
                    "Elements  →  Build reactants  →  Animate  →  Result"
                })
                .size(type_scale::CAPTION)
                .color(color::MUTED),
            ]
            .spacing(spacing::XXS),
        ]
        .spacing(spacing::SM)
        .align_y(Center);

        let record = button(text(if compact {
            "Record"
        } else {
            "Validated record  →"
        }))
        .on_press(Message::ScreenSelected(Screen::ValidatedRecord))
        .padding([spacing::XS, spacing::SM])
        .style(theme::secondary_button);

        container(row![brand, space().width(Fill), record].align_y(Center))
            .style(theme::chrome)
            .padding([spacing::XS, spacing::SM])
            .width(Fill)
            .into()
    }

    fn builder_status_bar(compact: bool) -> Element<'static, Message> {
        container(
            row![
                text("STAGE 1 · REACTANT COMPOSER")
                    .size(type_scale::MICRO)
                    .color(color::SUCCESS),
                space().width(Fill),
                text(if compact {
                    "NEXT · GUIDED ANIMATION"
                } else {
                    "NEXT · GUIDED 2D ANIMATION · LOCKED UNTIL A SUPPORTED PAIR IS SET"
                })
                .size(type_scale::MICRO)
                .color(color::MUTED),
            ]
            .align_y(Center),
        )
        .style(theme::chrome)
        .padding([spacing::XS, spacing::SM])
        .width(Fill)
        .into()
    }

    fn request_panel(&self, compact: bool) -> Element<'_, Message> {
        let heading = column![
            text("ASK THE LAB")
                .size(type_scale::MICRO)
                .color(color::ACCENT),
            text("Explore a reaction")
                .size(if compact {
                    type_scale::TITLE
                } else {
                    type_scale::DISPLAY
                })
                .color(color::TEXT),
            text("Describe the substances and quantities in ordinary language.")
                .size(type_scale::BODY)
                .color(color::MUTED),
        ]
        .spacing(spacing::XXS);

        let input = text_input("Ask what happens when substances mix…", &self.request)
            .on_input(Message::RequestChanged)
            .on_submit(Message::RequestSubmitted)
            .padding([spacing::SM, spacing::MD])
            .size(type_scale::BODY_LARGE)
            .style(theme::request_input)
            .width(Fill);

        let submit = button(
            row![text("Run fixture"), text("→").size(type_scale::BODY_LARGE)]
                .spacing(spacing::XS)
                .align_y(Center),
        )
        .on_press(Message::RequestSubmitted)
        .padding([spacing::SM, spacing::MD])
        .style(theme::primary_button);

        let controls: Element<'_, Message> = if compact {
            column![input, submit.width(Fill)]
                .spacing(spacing::XS)
                .into()
        } else {
            row![input, submit]
                .spacing(spacing::XS)
                .align_y(Center)
                .into()
        };

        let provider = row![
            text("●").size(type_scale::CAPTION).color(color::WARNING),
            text(match self.provider {
                Some(ProviderChoice::CodexSubscription) => {
                    "Codex subscription selected · trusted fixture active"
                }
                Some(ProviderChoice::ApiKey) => "API key selected · trusted fixture active",
                None => "Provider not configured · trusted canonical fixture",
            })
            .size(type_scale::CAPTION)
            .color(color::MUTED),
        ]
        .spacing(spacing::XS)
        .align_y(Center);

        container(column![heading, controls, provider].spacing(spacing::SM))
            .style(theme::panel)
            .padding(if compact { spacing::MD } else { spacing::LG })
            .width(Fill)
            .into()
    }

    fn simulation_panel(&self, height: Length) -> Element<'_, Message> {
        let title = column![
            text("REACTION STAGE")
                .size(type_scale::MICRO)
                .color(color::ACCENT),
            text(self.active_experience.name())
                .size(type_scale::TITLE)
                .color(color::TEXT),
            text("Final validated state · products assigned")
                .size(type_scale::CAPTION)
                .color(color::MUTED),
        ]
        .spacing(spacing::XXS);

        let valid = self.validated_frames.is_some();
        let status = container(
            row![
                text("●").size(type_scale::CAPTION).color(color::SUCCESS),
                text(if valid {
                    "VALIDATED · AI-REVIEWED CATALOGUE"
                } else if self.source_stale {
                    "STALE · REVALIDATION REQUIRED"
                } else {
                    "INVALID OR UNSUPPORTED"
                })
                .size(type_scale::MICRO)
                .color(color::TEXT_SOFT),
            ]
            .spacing(spacing::XS)
            .align_y(Center),
        )
        .style(if valid {
            theme::success_tint
        } else {
            theme::accent_tint
        })
        .padding([spacing::XS, spacing::SM]);

        let stage: Element<'_, Message> = if let Some(frames) = &self.validated_frames {
            let final_frame = frames.frames().len().saturating_sub(1);
            container(
                canvas(reaction_sequence::ReactionSequenceDiagram::new(
                    frames,
                    final_frame,
                ))
                .width(Fill)
                .height(Fill),
            )
            .style(theme::inset)
            .padding(spacing::XS)
            .width(Fill)
            .height(Fill)
            .into()
        } else {
            container(
                text(
                    self.validation_error
                        .as_deref()
                        .unwrap_or("Source changed. Revalidate before playback."),
                )
                .size(type_scale::BODY)
                .color(color::WARNING),
            )
            .style(theme::inset)
            .padding(spacing::MD)
            .center(Fill)
            .into()
        };

        container(
            column![
                row![title, space().width(Fill), status].align_y(Center),
                stage,
                row![
                    text("━ covalent").color(color::ACCENT),
                    text("◯ ionic association").color(color::SUCCESS),
                    text("◉ metallic domain").color(color::WARNING),
                    space().width(Fill),
                    button(text("Open guided animation  →"))
                        .on_press_maybe(valid.then_some(Message::OpenStructural2d))
                        .style(theme::primary_button),
                ]
                .spacing(spacing::MD),
                text(chemistry::DISCLOSURE)
                    .size(type_scale::CAPTION)
                    .color(color::MUTED),
            ]
            .spacing(spacing::SM),
        )
        .style(theme::panel)
        .padding(spacing::MD)
        .width(Fill)
        .height(height)
        .into()
    }

    fn inspector(&self, compact: bool, height: Length) -> Element<'_, Message> {
        let navigation =
            Section::ALL
                .into_iter()
                .fold(row![].spacing(spacing::XXS), |navigation, section| {
                    let selected = section == self.section;
                    navigation.push(
                        button(text(section.label(compact)).size(type_scale::CAPTION))
                            .on_press(Message::SectionSelected(section))
                            .padding([spacing::XS, spacing::SM])
                            .style(move |_, status| theme::navigation_button(selected, status)),
                    )
                });

        let content = match self.section {
            Section::Overview => self.overview_panel(),
            Section::Source => self.source_panel(),
            Section::Validation => self.validation_panel(),
            Section::Evidence => Self::sources_panel(),
        };

        container(column![navigation, content].spacing(spacing::SM))
            .style(theme::panel)
            .padding(spacing::SM)
            .width(Fill)
            .height(height)
            .into()
    }

    fn overview_panel(&self) -> Element<'static, Message> {
        let workflow = Self::workflow_panel();

        let validation_summary = Self::summary_card(
            "VALIDATION",
            "Trusted structural derivation",
            "Exact source, catalogue, evidence, and frame identities are bound.",
            Section::Validation,
        );

        let source_summary = Self::summary_card(
            "EXPERIMENT SOURCE",
            self.active_experience.source_name(),
            "Human-readable source · chems 1",
            Section::Source,
        );

        let evidence_summary = Self::summary_card(
            "EVIDENCE",
            "3 linked catalogue sources",
            "Claims remain separate from trusted catalogue facts.",
            Section::Evidence,
        );

        scrollable(
            column![
                workflow,
                validation_summary,
                source_summary,
                evidence_summary,
            ]
            .spacing(spacing::XS),
        )
        .height(Fill)
        .into()
    }

    fn workflow_panel() -> Element<'static, Message> {
        let steps = [
            ("01", "Identified the requested substances"),
            ("02", "Researched aqueous behaviour"),
            ("03", "Predicted the reaction"),
            ("04", "Wrote .chems"),
            ("05", "Validated"),
        ];

        let list =
            steps
                .into_iter()
                .fold(column![].spacing(spacing::XS), |list, (number, label)| {
                    let marker =
                        container(text(number).size(type_scale::MICRO).color(color::SUCCESS))
                            .style(theme::success_tint)
                            .center_x(30)
                            .center_y(30);

                    list.push(
                        row![
                            marker,
                            column![
                                text(label).size(type_scale::BODY).color(color::TEXT_SOFT),
                                text("Complete").size(type_scale::MICRO).color(color::MUTED),
                            ]
                            .spacing(spacing::XXS),
                        ]
                        .spacing(spacing::SM)
                        .align_y(Center),
                    )
                });

        container(
            column![
                row![
                    column![
                        text("WORKFLOW")
                            .size(type_scale::MICRO)
                            .color(color::ACCENT),
                        text("Research to trusted result")
                            .size(type_scale::BODY_LARGE)
                            .color(color::TEXT),
                    ]
                    .spacing(spacing::XXS),
                    space().width(Fill),
                    text("5 / 5")
                        .size(type_scale::CAPTION)
                        .color(color::SUCCESS),
                ]
                .align_y(Center),
                rule::horizontal(1).style(|current| iced::widget::rule::Style {
                    color: color::LINE,
                    ..iced::widget::rule::default(current)
                }),
                list,
                text("Offline trusted fixture · live provider events remain to be connected")
                    .size(type_scale::CAPTION)
                    .color(color::MUTED),
            ]
            .spacing(spacing::SM),
        )
        .style(theme::inset)
        .padding(spacing::MD)
        .width(Fill)
        .into()
    }

    fn summary_card(
        eyebrow: &'static str,
        title: &'static str,
        detail: &'static str,
        section: Section,
    ) -> Element<'static, Message> {
        let content = column![
            text(eyebrow).size(type_scale::MICRO).color(color::ACCENT),
            text(title).size(type_scale::BODY_LARGE).color(color::TEXT),
            text(detail).size(type_scale::CAPTION).color(color::MUTED),
        ]
        .spacing(spacing::XXS)
        .width(Fill);

        container(
            row![
                content,
                button(text("Open  →").size(type_scale::CAPTION))
                    .on_press(Message::SectionSelected(section))
                    .padding([spacing::XS, spacing::SM])
                    .style(theme::secondary_button),
            ]
            .spacing(spacing::SM)
            .align_y(Center),
        )
        .style(theme::raised)
        .padding(spacing::SM)
        .width(Fill)
        .into()
    }

    fn source_panel(&self) -> Element<'_, Message> {
        let source = text_editor(&self.source)
            .on_action(Message::SourceEdited)
            .font(Font::MONOSPACE)
            .size(type_scale::CAPTION)
            .padding(spacing::MD)
            .height(Fill);
        let status = if self.source_stale {
            "Edited · downstream validation and frames invalidated"
        } else if self.validated_frames.is_some() {
            "Current · source identity matches the trusted frame artifact"
        } else {
            "Validation failed · inspect the diagnostic below"
        };
        let diagnostic = self.validation_error.as_deref().unwrap_or(status);

        container(
            column![
                Self::panel_heading(
                    "EXPERIMENT SOURCE",
                    self.active_experience.source_name(),
                    "Parsed source · trusted only after expansion and kernel validation",
                ),
                source,
                row![
                    text(diagnostic).size(type_scale::CAPTION).color(
                        if self.validated_frames.is_some() {
                            color::SUCCESS
                        } else {
                            color::WARNING
                        }
                    ),
                    space().width(Fill),
                    button(text("Revalidate"))
                        .on_press(Message::SourceRevalidate)
                        .padding([spacing::XS, spacing::SM])
                        .style(theme::primary_button),
                ]
                .align_y(Center),
            ]
            .spacing(spacing::SM),
        )
        .style(theme::inset)
        .padding(spacing::MD)
        .width(Fill)
        .height(Fill)
        .into()
    }

    fn validation_panel(&self) -> Element<'_, Message> {
        let checks = [
            "Complete .chems 1 parse",
            "Host-pinned catalogue and AI attestation",
            "Total atom mapping",
            "Atom and charge conservation",
            "Electron and valence invariants",
            "Product graph and frame projection",
        ];

        let list = checks
            .into_iter()
            .fold(column![].spacing(spacing::XS), |list, check| {
                list.push(
                    row![
                        text("✓").size(type_scale::BODY).color(color::SUCCESS),
                        text(check).size(type_scale::BODY).color(color::TEXT_SOFT),
                        space().width(Fill),
                        text("PASS").size(type_scale::MICRO).color(color::MUTED),
                    ]
                    .spacing(spacing::XS)
                    .align_y(Center),
                )
            });

        let assumptions = [
            "Representative educational outcome",
            "Explanatory sequence, not mechanism",
            "AI-reviewed catalogue trust decision",
            "Not laboratory or safety guidance",
        ]
        .into_iter()
        .fold(column![].spacing(spacing::XS), |list, item| {
            list.push(
                container(
                    row![
                        text("◆").size(type_scale::MICRO).color(color::WARNING),
                        text(item).size(type_scale::CAPTION).color(color::TEXT_SOFT),
                    ]
                    .spacing(spacing::XS)
                    .align_y(Center),
                )
                .style(theme::raised)
                .padding([spacing::XS, spacing::SM])
                .width(Fill),
            )
        });

        let identity = self.validated_frames.as_ref().map_or_else(
            || "No current validated identity".to_owned(),
            |frames| {
                format!(
                    "frame digest {}",
                    frames
                        .digest()
                        .map_or_else(|_| "unavailable".to_owned(), |digest| digest.to_string())
                )
            },
        );

        container(
            scrollable(
                column![
                    Self::panel_heading(
                        "VALIDATION",
                        "Valid · exact trusted capability",
                        "Deterministic checks over the current source and catalogue",
                    ),
                    text(identity).size(type_scale::CAPTION).color(color::MUTED),
                    container(list)
                        .style(theme::raised)
                        .padding(spacing::MD)
                        .width(Fill),
                    text("ASSUMPTIONS")
                        .size(type_scale::MICRO)
                        .color(color::WARNING),
                    assumptions,
                ]
                .spacing(spacing::SM),
            )
            .height(Fill),
        )
        .style(theme::inset)
        .padding(spacing::MD)
        .width(Fill)
        .height(Fill)
        .into()
    }

    fn sources_panel() -> Element<'static, Message> {
        let source_card =
            |index: &'static str, title: &'static str, kind: &'static str, claim: &'static str| {
                container(
                    column![
                        row![
                            text(index).size(type_scale::MICRO).color(color::ACCENT),
                            text(kind).size(type_scale::MICRO).color(color::MUTED),
                        ]
                        .spacing(spacing::XS),
                        text(title).size(type_scale::BODY_LARGE).color(color::TEXT),
                        text(claim).size(type_scale::BODY).color(color::TEXT_SOFT),
                    ]
                    .spacing(spacing::XS),
                )
                .style(theme::raised)
                .padding(spacing::MD)
                .width(Fill)
            };

        container(
            scrollable(
                column![
                    Self::panel_heading(
                        "EVIDENCE",
                        "Sources and catalogue claims",
                        "Provenance stays separate from .chems source",
                    ),
                    source_card(
                        "01",
                        "OpenStax Chemistry 2e",
                        "REFERENCE",
                        "Supports the structural and representative alkali-metal-with-water premises.",
                    ),
                    source_card(
                        "02",
                        "IUPAC Gold Book",
                        "REFERENCE",
                        "Supports ionic, metallic, and bonding terminology used by the model.",
                    ),
                    source_card(
                        "03",
                        "IUPAC Periodic Table of the Elements",
                        "REFERENCE",
                        "Supports the 118 element symbols, names, atomic numbers, and table identities.",
                    ),
                    container(
                        text(
                            "Evidence supports claims; it does not bypass deterministic validation."
                        )
                        .size(type_scale::CAPTION)
                        .color(color::MUTED),
                    )
                    .style(theme::accent_tint)
                    .padding(spacing::SM)
                    .width(Fill),
                ]
                .spacing(spacing::SM),
            )
            .height(Fill),
        )
        .style(theme::inset)
        .padding(spacing::MD)
        .width(Fill)
        .height(Fill)
        .into()
    }

    fn panel_heading(
        eyebrow: &'static str,
        title: &'static str,
        subtitle: &'static str,
    ) -> Element<'static, Message> {
        column![
            text(eyebrow).size(type_scale::MICRO).color(color::ACCENT),
            text(title).size(type_scale::TITLE).color(color::TEXT),
            text(subtitle).size(type_scale::CAPTION).color(color::MUTED),
        ]
        .spacing(spacing::XXS)
        .into()
    }

    fn status_bar(compact: bool) -> Element<'static, Message> {
        let right = if compact {
            "TRUSTED FIXTURE"
        } else {
            "TRUSTED KERNEL FRAMES  ·  AI-REVIEWED CATALOGUE  ·  OFFLINE"
        };

        container(
            row![
                text("EXPLANATORY MODEL")
                    .size(type_scale::MICRO)
                    .color(color::MUTED),
                space().width(Fill),
                text(right).size(type_scale::MICRO).color(color::FAINT),
            ]
            .align_y(Center),
        )
        .style(theme::chrome)
        .padding([spacing::XS, spacing::SM])
        .width(Fill)
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_provider_follows_codex_availability() {
        assert_eq!(initial_provider(true), ProviderChoice::CodexSubscription);
        assert_eq!(initial_provider(false), ProviderChoice::ApiKey);
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
    fn request_edit_preserves_the_canonical_source() {
        let mut app = App::default();
        let source = app.source.text();

        app.update(Message::RequestChanged("A different question".to_owned()));
        app.update(Message::RequestSubmitted);

        assert_eq!(app.request, "A different question");
        assert_eq!(app.source.text(), source);
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

        app.update(Message::OpenStructural2d);

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
        for experience in chemistry::Experience::ALL {
            let frames = chemistry::run(experience)
                .expect("pinned experience validates")
                .frames();
            let plan = compile_educational_plan(frames).expect("educational plan compiles");
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
                        "{} emitted duplicate context and explanation copy",
                        experience.name()
                    );
                }
            }
            assert!(
                compared > 0,
                "{} emitted no narrated scenes",
                experience.name()
            );
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
        app.update(Message::OpenStructural2d);
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
        app.update(Message::OpenStructural2d);
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
        app.update(Message::OpenStructural2d);
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
    fn every_inspector_region_is_reachable() {
        let mut app = App::default();

        for section in Section::ALL {
            app.update(Message::SectionSelected(section));
            assert_eq!(app.section, section);
        }
    }

    #[test]
    fn selecting_an_experience_updates_source_request_and_trusted_frames_together() {
        let mut app = App::default();
        for experience in [
            chemistry::Experience::Sodium,
            chemistry::Experience::Potassium,
        ] {
            app.select_experience(experience);
            assert_eq!(app.active_experience, experience);
            assert_eq!(app.request, experience.request());
            assert_eq!(app.source.text(), experience.source());
            assert!(app.validated_frames.is_some());
            assert!(app.validation_error.is_none());
        }
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
            let _ = app.responsive_view(size);
        }
    }

    #[test]
    fn stage_one_supported_drafts_open_the_guided_animation_directly() {
        let mut app = App::default();

        app.update(Message::PeriodicTable(periodic_table::Message::Activated(
            3,
        )));
        app.update(Message::ReactantComposer(
            reactant_composer::Message::Activate(reactant_composer::ActiveReactant::Second),
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
        assert_eq!(app.active_experience, chemistry::Experience::Lithium);
        let animation = app
            .structural_animation
            .as_ref()
            .expect("guided animation compiles from the trusted frames");
        assert!(animation.playing);

        app.update(Message::ScreenSelected(Screen::Builder));
        assert_eq!(app.screen, Screen::Builder);
    }
}
