//! `ChemSpec` application shell and reaction-builder entry (`U-101`, `U-106`–`U-112`).
//!
//! Opens on the Stage 1 element library and preserves the six validated-record
//! regions—request, workflow, source, validation, sources, and simulation—using
//! the canonical silver-chloride fixture. No parsing, validation, or agent work
//! happens here yet; presentation does not confer chemistry meaning.

mod composition_catalogue;
mod elements;
mod particle_visualization;
mod periodic_table;
mod reactant_composer;
mod reaction_candidate_catalogue;
mod reaction_sequence;
mod reaction_workspace;
mod scene_registry;
mod structural_2d;
mod structural_3d;
mod theme;
mod vessel;

use chem_catalogue::CatalogueBundle;
use chem_engine::{
    ExpansionError, StructuralFrame, StructuralValidationError, ValidationDisposition,
    expand_structural_rule, structural_frames, validate_structural_reaction,
};
use chem_presentation::{
    EducationalPlan, EducationalSceneKind, ScenePlan, compile_educational_plan,
    compile_real_world_plan,
};
use iced::widget::{
    button, canvas, column, container, progress_bar, responsive, row, rule, scrollable, space,
    stack, text, text_editor, text_input,
};
use iced::{Center, Element, Fill, FillPortion, Font, Length, Size, Subscription, Theme};

use theme::{breakpoint, color, space as spacing, type_scale};
use vessel::Vessel;

const CANONICAL_SOURCE: &str = include_str!("../../../fixtures/silver-chloride.chems");
const LITHIUM_WATER_SOURCE: &str = include_str!("../../../fixtures/lithium-water.chems");
const CANONICAL_REQUEST: &str = "What happens if I mix 50 mL of 0.100 M silver nitrate \
     with 50 mL of 0.100 M sodium chloride?";
const CANONICAL_EQUATION: &str = "AgNO₃ + NaCl  →  AgCl↓ + NaNO₃";
const SIMULATION_DISCLOSURE: &str = "Explanatory particle model. Quantities and reaction \
     relationships are validated; particle scale, motion, and elapsed time are illustrative.";

fn reviewed_equation_text(equation: &chem_catalogue::ReviewedEquation) -> String {
    fn side(terms: &[chem_catalogue::StoichiometricTerm]) -> String {
        terms
            .iter()
            .map(|term| {
                let coefficient = if term.coefficient > 1 {
                    term.coefficient.to_string()
                } else {
                    String::new()
                };
                let formula = term
                    .formula
                    .chars()
                    .map(|character| match character {
                        '0' => '₀',
                        '1' => '₁',
                        '2' => '₂',
                        '3' => '₃',
                        '4' => '₄',
                        '5' => '₅',
                        '6' => '₆',
                        '7' => '₇',
                        '8' => '₈',
                        '9' => '₉',
                        other => other,
                    })
                    .collect::<String>();
                format!("{coefficient}{formula}")
            })
            .collect::<Vec<_>>()
            .join(" + ")
    }

    format!(
        "{}  →  {}",
        side(&equation.reactants),
        side(&equation.products)
    )
}

fn plan_equation(animation: &StructuralAnimation) -> Option<&chem_catalogue::ReviewedEquation> {
    animation
        .educational_plan
        .scenes
        .iter()
        .flat_map(|scene| &scene.cues)
        .find_map(|cue| match cue {
            chem_presentation::EducationalCue::ShowEquation { equation } => Some(equation),
            _ => None,
        })
}

#[allow(clippy::cast_precision_loss)]
fn educational_timeline_progress(animation: &StructuralAnimation) -> f32 {
    let total = animation
        .educational_plan
        .scenes
        .iter()
        .map(|scene| u64::from(scene.duration_ms))
        .sum::<u64>()
        .max(1);
    let elapsed = animation
        .educational_plan
        .scenes
        .iter()
        .take(animation.scene_index)
        .map(|scene| u64::from(scene.duration_ms))
        .sum::<u64>()
        .saturating_add(u64::from(animation.scene_elapsed_ms));
    (elapsed.min(total) as f32 / total as f32).clamp(0.0, 1.0)
}

#[allow(clippy::cast_precision_loss)]
fn real_world_timeline_progress(animation: &StructuralAnimation) -> f32 {
    const STAGE_DURATION_MS: u64 = 2_400;
    let stages = u64::try_from(animation.frames.len())
        .unwrap_or(u64::MAX)
        .max(1);
    let total = stages.saturating_mul(STAGE_DURATION_MS);
    let completed = u64::try_from(animation.frame_index)
        .unwrap_or(u64::MAX)
        .saturating_mul(STAGE_DURATION_MS)
        .saturating_add(u64::from(animation.real_world_elapsed_ms));
    (completed.min(total) as f32 / total as f32).clamp(0.0, 1.0)
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
        .title("ChemSpec — reaction builder")
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

fn launch_state() -> App {
    let mut app = App::default();
    let smoke = std::env::args().find(|argument| {
        matches!(
            argument.as_str(),
            "--structural-2d-smoke"
                | "--structural-3d-smoke"
                | "--lithium-2d-smoke"
                | "--lithium-3d-smoke"
        )
    });
    if let Some(smoke) = smoke {
        if smoke.starts_with("--lithium-") {
            LITHIUM_WATER_SOURCE.clone_into(&mut app.source);
            app.source_content = text_editor::Content::with_text(LITHIUM_WATER_SOURCE);
            app.active_catalogue = BundledCatalogue::ReactiveMetals;
        }
        app.prepare_structural_animation();
        if let Some(animation) = &mut app.structural_animation {
            let three_dimensional = smoke.ends_with("3d-smoke");
            animation.frame_index = if three_dimensional {
                animation.frames.len().saturating_sub(1)
            } else {
                1.min(animation.frames.len().saturating_sub(1))
            };
            animation.scene_index = if three_dimensional {
                0
            } else {
                animation
                    .educational_plan
                    .scenes
                    .iter()
                    .position(|scene| scene.kind == EducationalSceneKind::ExplanationPause)
                    .unwrap_or(0)
            };
            if !three_dimensional
                && let Some(scene) = animation.educational_plan.scenes.get(animation.scene_index)
            {
                animation.scene_elapsed_ms = scene.duration_ms / 2;
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
    Builder,
    ValidatedRecord,
    Structural2d,
    Structural3d,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BundledCatalogue {
    Aqueous,
    ReactiveMetals,
}

impl BundledCatalogue {
    const fn bytes(self) -> &'static [u8] {
        match self {
            Self::Aqueous => {
                include_bytes!("../../../fixtures/catalogue/silver-chloride.catalogue.json")
            }
            Self::ReactiveMetals => {
                include_bytes!("../../../fixtures/catalogue/lithium-water.catalogue.json")
            }
        }
    }
}

#[derive(Debug)]
struct StructuralAnimation {
    frames: Vec<StructuralFrame>,
    educational_plan: EducationalPlan,
    real_world_plan: ScenePlan,
    scene_index: usize,
    scene_elapsed_ms: u32,
    frame_index: usize,
    real_world_elapsed_ms: u32,
    playing: bool,
    playback_speed: PlaybackSpeed,
    disposition: ValidationDisposition,
    safety_notices: Vec<String>,
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
enum StructuralFailureKind {
    Invalid,
    Unsupported,
    SystemError,
}

impl StructuralFailureKind {
    const fn label(self) -> &'static str {
        match self {
            Self::Invalid => "INVALID",
            Self::Unsupported => "UNSUPPORTED",
            Self::SystemError => "SYSTEM ERROR",
        }
    }
}

#[derive(Debug)]
struct StructuralFailure {
    kind: StructuralFailureKind,
    message: String,
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
    PeriodicTable(periodic_table::Message),
    ReactantComposer(reactant_composer::Message),
    ReactionWorkspace(reaction_workspace::Message),
    RequestChanged(String),
    RequestSubmitted,
    SourceAction(text_editor::Action),
    RevalidateSource,
    SectionSelected(Section),
    StructuralPlaybackToggled,
    StructuralSpeedChanged,
    StructuralRestarted,
    StructuralTick,
    ContinueTo3d,
    ReturnTo2d,
    ReturnToBuilder,
}

struct App {
    screen: Screen,
    periodic_table: periodic_table::State,
    reactant_composer: reactant_composer::State,
    reaction_workspace: reaction_workspace::State,
    request: String,
    source: String,
    source_content: text_editor::Content,
    section: Section,
    vessel: Vessel,
    structural_animation: Option<StructuralAnimation>,
    structural_error: Option<StructuralFailure>,
    active_catalogue: BundledCatalogue,
}

impl Default for App {
    fn default() -> Self {
        Self {
            screen: Screen::Builder,
            periodic_table: periodic_table::State::default(),
            reactant_composer: reactant_composer::State::default(),
            reaction_workspace: reaction_workspace::State::default(),
            request: CANONICAL_REQUEST.to_owned(),
            source: CANONICAL_SOURCE.to_owned(),
            source_content: text_editor::Content::with_text(CANONICAL_SOURCE),
            section: Section::Overview,
            vessel: Vessel::new(),
            structural_animation: None,
            structural_error: None,
            active_catalogue: BundledCatalogue::Aqueous,
        }
    }
}

impl App {
    fn update(&mut self, message: Message) {
        match message {
            Message::ScreenSelected(screen) => self.screen = screen,
            Message::PeriodicTable(message) => {
                periodic_table::update(&mut self.periodic_table, message);
                if !reaction_workspace::sequence_active(&self.reaction_workspace)
                    && let periodic_table::Message::Activated(atomic_number) = message
                {
                    reactant_composer::update(
                        &mut self.reactant_composer,
                        reactant_composer::Message::AddElement(atomic_number),
                    );
                }
            }
            Message::ReactantComposer(message) => self.update_reactant_composer(message),
            Message::ReactionWorkspace(message) => {
                reaction_workspace::update(&mut self.reaction_workspace, message);
            }
            Message::RequestChanged(request) => self.request = request,
            // Agent orchestration arrives with `A-101`/`U-105`; until then
            // submitting keeps showing the canonical offline fixture.
            Message::RequestSubmitted => {}
            Message::SourceAction(action) => {
                let invalidates = action.is_edit();
                self.source_content.perform(action);
                if invalidates {
                    self.source = self.source_content.text();
                    self.structural_animation = None;
                    self.structural_error = None;
                }
            }
            Message::RevalidateSource => self.prepare_structural_animation(),
            Message::SectionSelected(section) => self.section = section,
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
                    animation.scene_index = 0;
                    animation.scene_elapsed_ms = 0;
                    animation.frame_index = 0;
                    animation.real_world_elapsed_ms = 0;
                    animation.playing = true;
                }
            }
            Message::ContinueTo3d => {
                if self.structural_animation.as_ref().is_some_and(|animation| {
                    animation.scene_index + 1 == animation.educational_plan.scenes.len()
                }) {
                    if let Some(animation) = &mut self.structural_animation {
                        animation.frame_index = 0;
                        animation.real_world_elapsed_ms = 0;
                        animation.playing = true;
                    }
                    self.screen = Screen::Structural3d;
                }
            }
            Message::ReturnTo2d => self.screen = Screen::Structural2d,
            Message::ReturnToBuilder => {
                self.screen = Screen::Builder;
                self.structural_animation = None;
            }
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
        let candidate = reaction_candidate_catalogue::recognize_drafts(first, second);
        let Some(candidate) = candidate else {
            reaction_workspace::load_reactants(&mut self.reaction_workspace, first, second);
            reaction_workspace::update(
                &mut self.reaction_workspace,
                reaction_workspace::Message::StartReaction,
            );
            return;
        };

        let supported = match candidate.id {
            "lithium-water" => Some((LITHIUM_WATER_SOURCE, BundledCatalogue::ReactiveMetals)),
            "silver-chloride-precipitation" => Some((CANONICAL_SOURCE, BundledCatalogue::Aqueous)),
            _ => None,
        };
        if let Some((source, catalogue)) = supported {
            source.clone_into(&mut self.source);
            self.source_content = text_editor::Content::with_text(source);
            self.active_catalogue = catalogue;
            self.prepare_structural_animation();
            return;
        }

        reaction_workspace::load_reactants(&mut self.reaction_workspace, first, second);
        reaction_workspace::update(
            &mut self.reaction_workspace,
            reaction_workspace::Message::StartReaction,
        );
    }

    fn prepare_structural_animation(&mut self) {
        let catalogue_bytes = self.active_catalogue.bytes();
        let result = (|| {
            let catalogue =
                CatalogueBundle::load_json(catalogue_bytes).map_err(|error| StructuralFailure {
                    kind: StructuralFailureKind::SystemError,
                    message: error.to_string(),
                })?;
            let expanded = expand_structural_rule(&self.source, &catalogue).map_err(|error| {
                let kind = match error {
                    ExpansionError::RuleInapplicable { .. } => StructuralFailureKind::Unsupported,
                    ExpansionError::CatalogueMismatch { .. }
                    | ExpansionError::MalformedCatalogueReference(_) => {
                        StructuralFailureKind::SystemError
                    }
                    _ => StructuralFailureKind::Invalid,
                };
                StructuralFailure {
                    kind,
                    message: error.to_string(),
                }
            })?;
            let validated =
                validate_structural_reaction(expanded).map_err(|error| StructuralFailure {
                    kind: if error == StructuralValidationError::Serialization {
                        StructuralFailureKind::SystemError
                    } else {
                        StructuralFailureKind::Invalid
                    },
                    message: error.to_string(),
                })?;
            let disposition = validated.disposition();
            let safety_notices = validated.safety_notices().to_vec();
            let frames = structural_frames(&validated).map_err(|error| StructuralFailure {
                kind: StructuralFailureKind::SystemError,
                message: error.to_string(),
            })?;
            let educational_plan =
                compile_educational_plan(&validated, &frames).map_err(|error| {
                    StructuralFailure {
                        kind: StructuralFailureKind::SystemError,
                        message: error.to_string(),
                    }
                })?;
            let real_world_plan =
                compile_real_world_plan(&validated).map_err(|error| StructuralFailure {
                    kind: StructuralFailureKind::SystemError,
                    message: error.to_string(),
                })?;
            Ok((
                frames,
                educational_plan,
                real_world_plan,
                disposition,
                safety_notices,
            ))
        })();
        match result {
            Ok((frames, educational_plan, real_world_plan, disposition, safety_notices)) => {
                self.structural_animation = Some(StructuralAnimation {
                    frames,
                    educational_plan,
                    real_world_plan,
                    scene_index: 0,
                    scene_elapsed_ms: 0,
                    frame_index: 0,
                    real_world_elapsed_ms: 0,
                    playing: true,
                    playback_speed: PlaybackSpeed::Normal,
                    disposition,
                    safety_notices,
                });
                self.structural_error = None;
                self.screen = Screen::Structural2d;
            }
            Err(failure) => {
                self.structural_animation = None;
                self.structural_error = Some(failure);
            }
        }
    }

    fn change_structural_frame(&mut self, delta: i8) {
        let Some(animation) = &mut self.structural_animation else {
            return;
        };
        if self.screen == Screen::Structural2d {
            if delta < 0 {
                animation.scene_index = animation.scene_index.saturating_sub(1);
                animation.playing = false;
            } else if animation.scene_index + 1 < animation.educational_plan.scenes.len() {
                animation.scene_index += 1;
            } else {
                animation.playing = false;
            }
            animation.scene_elapsed_ms = 0;
            if let Some(scene) = animation.educational_plan.scenes.get(animation.scene_index)
                && let Some(index) = animation
                    .frames
                    .iter()
                    .position(|frame| frame.id == scene.end_frame)
            {
                animation.frame_index = index;
            }
            return;
        }
        if delta < 0 {
            animation.frame_index = animation.frame_index.saturating_sub(1);
            animation.playing = false;
        } else if animation.frame_index + 1 < animation.frames.len() {
            animation.frame_index += 1;
        } else {
            animation.playing = false;
        }
        animation.real_world_elapsed_ms = 0;
    }

    fn advance_educational_playback(&mut self, elapsed_ms: u32) {
        let Some(animation) = &mut self.structural_animation else {
            return;
        };
        let Some(scene) = animation.educational_plan.scenes.get(animation.scene_index) else {
            animation.playing = false;
            return;
        };
        animation.scene_elapsed_ms = animation.scene_elapsed_ms.saturating_add(elapsed_ms);
        if animation.scene_elapsed_ms < scene.duration_ms {
            return;
        }
        animation.scene_elapsed_ms = 0;
        self.change_structural_frame(1);
    }

    fn advance_real_world_playback(&mut self, elapsed_ms: u32) {
        let Some(animation) = &mut self.structural_animation else {
            return;
        };
        animation.real_world_elapsed_ms =
            animation.real_world_elapsed_ms.saturating_add(elapsed_ms);
        if animation.real_world_elapsed_ms < 2_400 {
            return;
        }
        animation.real_world_elapsed_ms = 0;
        if animation.frame_index + 1 < animation.frames.len() {
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
            let composer = if reaction_workspace::sequence_active(&self.reaction_workspace) {
                Subscription::none()
            } else {
                reactant_composer::subscription(&self.reactant_composer)
                    .map(Message::ReactantComposer)
            };
            Subscription::batch([
                periodic_table::subscription(&self.periodic_table).map(Message::PeriodicTable),
                composer,
                reaction_workspace::subscription(&self.reaction_workspace)
                    .map(Message::ReactionWorkspace),
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
            Screen::Builder => responsive(|size| self.builder_view(size)).into(),
            Screen::ValidatedRecord => responsive(|size| self.responsive_view(size)).into(),
            Screen::Structural2d => responsive(|size| self.structural_2d_view(size)).into(),
            Screen::Structural3d => responsive(|size| self.structural_3d_view(size)).into(),
        }
    }

    #[allow(clippy::too_many_lines, clippy::cast_precision_loss)]
    fn structural_2d_view(&self, size: Size) -> Element<'_, Message> {
        let compact = size.width < breakpoint::MOBILE;
        let Some(animation) = &self.structural_animation else {
            return Self::structural_unavailable_view("Validated frames are unavailable");
        };
        let Some(frame) = animation.frames.get(animation.frame_index) else {
            return Self::structural_unavailable_view("The current frame is unavailable");
        };
        let Some(educational_scene) = animation.educational_plan.scenes.get(animation.scene_index)
        else {
            return Self::structural_unavailable_view(
                "The current educational scene is unavailable",
            );
        };
        let before_frame = animation
            .frames
            .iter()
            .find(|candidate| candidate.id == educational_scene.start_frame)
            .unwrap_or(frame);
        let scene_progress = if educational_scene.duration_ms == 0 {
            1.0
        } else {
            (animation.scene_elapsed_ms as f32 / educational_scene.duration_ms as f32)
                .clamp(0.0, 1.0)
        };
        let explanation = educational_scene.cues.iter().find_map(|cue| match cue {
            chem_presentation::EducationalCue::ShowExplanation { label } => Some(label),
            _ => None,
        });
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
        let exit = button(text("← Reactants"))
            .on_press(Message::ReturnToBuilder)
            .padding([spacing::XS, spacing::SM])
            .style(theme::secondary_button);
        let continue_3d = button(text("View in real life  →"))
            .on_press_maybe(
                (animation.scene_index + 1 == animation.educational_plan.scenes.len())
                    .then_some(Message::ContinueTo3d),
            )
            .padding([spacing::XS, spacing::MD])
            .style(theme::primary_button);

        let diagram = container(
            canvas(structural_2d::Diagram::new(
                before_frame,
                frame,
                scene_progress,
                explanation,
                scene_progress,
                matches!(
                    educational_scene.kind,
                    EducationalSceneKind::ReactantSetup | EducationalSceneKind::StructuralChange
                ),
            ))
            .width(Fill)
            .height(Fill),
        )
        .style(theme::inset)
        .width(Fill)
        .height(Fill);

        let controls = row![
            playback,
            restart,
            speed,
            text(educational_scene_title(educational_scene.kind))
                .size(type_scale::CAPTION)
                .color(color::TEXT_SOFT),
            container(progress_bar(
                0.0..=1.0,
                educational_timeline_progress(animation)
            ))
            .width(Fill),
            continue_3d,
        ]
        .spacing(spacing::XS)
        .align_y(Center);

        container(
            column![
                row![
                    exit,
                    column![
                        text("VALIDATED 2D EXPLANATION")
                            .size(type_scale::MICRO)
                            .color(color::ACCENT),
                        text(plan_equation(animation).map_or_else(
                            || "Reviewed equation unavailable".to_owned(),
                            reviewed_equation_text,
                        ))
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
                        text(match animation.disposition {
                            ValidationDisposition::Validated => "VALIDATED",
                            ValidationDisposition::ValidatedWithAssumptions => {
                                "VALIDATED WITH MODEL ASSUMPTIONS"
                            }
                        })
                        .size(type_scale::MICRO)
                        .color(color::SUCCESS),
                        text(if animation.safety_notices.is_empty() {
                            "VIRTUAL MODEL"
                        } else {
                            "VIRTUAL ONLY · SAFETY-SENSITIVE"
                        })
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
                    .on_press(Message::ReturnToBuilder)
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
        let Some(frame) = animation.frames.get(animation.frame_index) else {
            return Self::structural_unavailable_view("The current frame is unavailable");
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
        let scene = container(
            iced::widget::Shader::new(structural_3d::Scene::new(
                &animation.real_world_plan,
                frame.ordinal,
                (animation.real_world_elapsed_ms as f32 / 2_400.0).clamp(0.0, 1.0),
            ))
            .width(Fill)
            .height(Fill),
        )
        .style(theme::inset)
        .width(Fill)
        .height(Fill);
        let controls = row![
            playback,
            restart,
            speed,
            text("Continuous cinematic playback")
                .size(type_scale::CAPTION)
                .color(color::TEXT_SOFT),
            container(progress_bar(
                0.0..=1.0,
                real_world_timeline_progress(animation)
            ))
            .width(Fill),
        ]
        .spacing(spacing::XS)
        .align_y(Center);
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
                    ]
                    .spacing(spacing::XXS),
                    space().width(Fill),
                    text("DRAG TO ORBIT · SCROLL TO ZOOM")
                    .size(type_scale::MICRO)
                    .color(color::MUTED),
                ]
                .align_y(Center),
                scene,
                controls,
                text("Stylised virtual model · timing, scale, fluid motion, and camera movement are illustrative")
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
        let sequence_active = reaction_workspace::sequence_active(&self.reaction_workspace);

        let library =
            periodic_table::view(&self.periodic_table, compact).map(Message::PeriodicTable);
        let composer = reactant_composer::view(
            &self.reactant_composer,
            periodic_table::dragging_atomic_number(&self.periodic_table),
            compact,
        )
        .map(Message::ReactantComposer);
        let sequence: Element<'_, Message> =
            reaction_workspace::view(&self.reaction_workspace, None, compact)
                .map(Message::ReactionWorkspace);

        let stages: Element<'_, Message> = if sequence_active {
            container(sequence).width(Fill).height(Fill).into()
        } else {
            column![composer, library]
                .spacing(spacing::XS)
                .width(Fill)
                .height(Fill)
                .into()
        };

        let content = column![
            Self::builder_context_bar(compact, sequence_active),
            stages,
            Self::builder_status_bar(compact, sequence_active),
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
            Self::context_bar(false),
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
            Self::context_bar(false),
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
            Self::context_bar(true),
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

    fn context_bar(compact: bool) -> Element<'static, Message> {
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
            text("OFFLINE FIXTURE")
                .size(type_scale::MICRO)
                .color(color::MUTED)
        } else {
            text(CANONICAL_EQUATION)
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

    fn builder_context_bar(compact: bool, sequence_active: bool) -> Element<'static, Message> {
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
                    if sequence_active {
                        "Stage 5 · Animate"
                    } else {
                        "Stage 1 · Build"
                    }
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

    fn builder_status_bar(compact: bool, sequence_active: bool) -> Element<'static, Message> {
        container(
            row![
                text(if sequence_active {
                    "ANIMATION · VALIDATION REQUIRED"
                } else {
                    "STAGE 1 · REACTANT COMPOSER"
                })
                .size(type_scale::MICRO)
                .color(color::SUCCESS),
                space().width(Fill),
                text(if !sequence_active {
                    if compact {
                        "NEXT · REACTION PREVIEW"
                    } else {
                        "NEXT · 2D REACTION PREVIEW · LOCKED UNTIL A SUPPORTED PAIR IS SET"
                    }
                } else if compact {
                    "NEXT · VALIDATED 2D"
                } else {
                    "NEXT · VALIDATED 2D EXPLANATION · PIPELINE REQUIRED"
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
            text("Provider not configured · canonical offline fixture")
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
            text("Silver chloride formation")
                .size(type_scale::TITLE)
                .color(color::TEXT),
            text("Initial state · dissolved ions after mixing")
                .size(type_scale::CAPTION)
                .color(color::MUTED),
        ]
        .spacing(spacing::XXS);

        let status = container(
            row![
                text("●").size(type_scale::CAPTION).color(color::SUCCESS),
                text("VALIDATED WITH ASSUMPTIONS")
                    .size(type_scale::MICRO)
                    .color(color::TEXT_SOFT),
            ]
            .spacing(spacing::XS)
            .align_y(Center),
        )
        .style(theme::success_tint)
        .padding([spacing::XS, spacing::SM]);

        let stage = container(canvas(&self.vessel).width(Fill).height(Fill))
            .style(theme::inset)
            .padding(spacing::XS)
            .width(Fill)
            .height(Fill);

        container(
            column![
                row![title, space().width(Fill), status].align_y(Center),
                stage,
                Vessel::legend(),
                text(SIMULATION_DISCLOSURE)
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
            Section::Overview => Self::overview_panel(),
            Section::Source => self.source_panel(),
            Section::Validation => Self::validation_panel(),
            Section::Evidence => Self::sources_panel(),
        };

        container(column![navigation, content].spacing(spacing::SM))
            .style(theme::panel)
            .padding(spacing::SM)
            .width(Fill)
            .height(height)
            .into()
    }

    fn overview_panel() -> Element<'static, Message> {
        let workflow = Self::workflow_panel();

        let validation_summary = Self::summary_card(
            "VALIDATION",
            "6 checks passed",
            "Assumptions remain visible and inspectable.",
            Section::Validation,
        );

        let source_summary = Self::summary_card(
            "EXPERIMENT SOURCE",
            "silver-chloride.chems",
            "Human-readable source · chems 1",
            Section::Source,
        );

        let evidence_summary = Self::summary_card(
            "EVIDENCE",
            "2 linked sources",
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
                text("Offline fixture · live agent progress arrives in Phase 3")
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
        let source = text_editor(&self.source_content)
            .on_action(Message::SourceAction)
            .font(Font::MONOSPACE)
            .size(type_scale::CAPTION)
            .padding(spacing::SM)
            .height(Fill);
        let status: Element<'_, Message> = if let Some(failure) = &self.structural_error {
            row![
                text(failure.kind.label())
                    .size(type_scale::MICRO)
                    .color(color::WARNING),
                text(&failure.message)
                    .size(type_scale::CAPTION)
                    .color(color::TEXT_SOFT),
                space().width(Fill),
                button(text("Revalidate"))
                    .on_press(Message::RevalidateSource)
                    .style(theme::primary_button),
            ]
            .spacing(spacing::SM)
            .align_y(Center)
            .into()
        } else {
            row![
                text(if self.structural_animation.is_some() {
                    "CURRENT VALIDATED FRAMES"
                } else {
                    "EDITING INVALIDATES ALL STRUCTURAL FRAMES"
                })
                .size(type_scale::MICRO)
                .color(if self.structural_animation.is_some() {
                    color::SUCCESS
                } else {
                    color::MUTED
                }),
                space().width(Fill),
                button(text("Validate & open 2D"))
                    .on_press(Message::RevalidateSource)
                    .style(theme::primary_button),
            ]
            .align_y(Center)
            .into()
        };

        container(
            column![
                Self::panel_heading(
                    "EXPERIMENT SOURCE",
                    "silver-chloride.chems",
                    "Visible proposal · not trusted until validation",
                ),
                source,
                status,
            ]
            .spacing(spacing::SM),
        )
        .style(theme::inset)
        .padding(spacing::MD)
        .width(Fill)
        .height(Fill)
        .into()
    }

    fn validation_panel() -> Element<'static, Message> {
        let checks = [
            "Syntax and types",
            "Known substances",
            "Atoms conserved",
            "Charge conserved",
            "Precipitation rule established",
            "Stoichiometry solved",
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
            "Aqueous solutions",
            "25 degC",
            "1 atm",
            "Idealized complete dissociation",
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

        container(
            scrollable(
                column![
                    Self::panel_heading(
                        "VALIDATION",
                        "Validated with assumptions",
                        "Deterministic checks on the current fixture",
                    ),
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
                        "OpenStax Chemistry 2e §4.2",
                        "REFERENCE",
                        "Silver chloride is insoluble in water at 25 degC.",
                    ),
                    source_card(
                        "02",
                        "ChemSpec.Aqueous@1 catalogue",
                        "TRUSTED CATALOGUE",
                        "AgNO₃, NaCl, and NaNO₃ are soluble strong electrolytes.",
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
            "STATIC SHELL"
        } else {
            "U-101  ·  STATIC SHELL  ·  NO PROVIDER USAGE"
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
    fn request_edit_preserves_the_static_fixture() {
        let mut app = App::default();
        let source = app.source.clone();

        app.update(Message::RequestChanged("A different question".to_owned()));
        app.update(Message::RequestSubmitted);

        assert_eq!(app.request, "A different question");
        assert_eq!(app.source, source);
    }

    #[test]
    fn source_edit_immediately_invalidates_structural_frames() {
        let mut app = App::default();
        app.prepare_structural_animation();
        assert!(app.structural_animation.is_some());

        app.update(Message::SourceAction(text_editor::Action::Edit(
            text_editor::Edit::Insert(' '),
        )));

        assert!(app.structural_animation.is_none());
        assert!(app.structural_error.is_none());
        assert_ne!(app.source, CANONICAL_SOURCE);
    }

    #[test]
    fn inapplicable_reviewed_rule_is_typed_as_unsupported_and_cannot_animate() {
        let mut app = App {
            source: CANONICAL_SOURCE.replace("AgNO3(aq)", "H2O(l)"),
            ..App::default()
        };

        app.prepare_structural_animation();

        assert!(app.structural_animation.is_none());
        assert_eq!(
            app.structural_error.as_ref().map(|failure| failure.kind),
            Some(StructuralFailureKind::Unsupported)
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
    fn periodic_drag_can_drop_directly_into_workspace() {
        let mut app = App::default();

        app.update(Message::PeriodicTable(
            periodic_table::Message::DragStarted(8),
        ));
        let dragged = periodic_table::dragging_atomic_number(&app.periodic_table)
            .expect("periodic drag should remain active outside the tile");
        app.update(Message::ReactionWorkspace(
            reaction_workspace::Message::PointerMoved(iced::Point::new(0.4, 0.5)),
        ));
        app.update(Message::ReactionWorkspace(
            reaction_workspace::Message::LibraryElementDropped(dragged),
        ));
        app.update(Message::PeriodicTable(periodic_table::Message::DragEnded));

        assert_eq!(
            reaction_workspace::placed_atom_count(&app.reaction_workspace),
            1
        );
    }

    #[test]
    fn stage_one_supported_drafts_open_the_validation_gate_without_animating() {
        let mut app = App::default();

        app.update(Message::PeriodicTable(periodic_table::Message::Activated(
            6,
        )));
        app.update(Message::ReactantComposer(
            reactant_composer::Message::Activate(reactant_composer::ActiveReactant::Second),
        ));
        app.update(Message::PeriodicTable(periodic_table::Message::Activated(
            8,
        )));
        app.update(Message::PeriodicTable(periodic_table::Message::Activated(
            8,
        )));

        assert_eq!(reactant_composer::reactants(&app.reactant_composer).0, &[6]);
        assert_eq!(
            reactant_composer::reactants(&app.reactant_composer).1,
            &[8, 8]
        );

        app.update(Message::ReactantComposer(
            reactant_composer::Message::StartReactionRequested,
        ));

        assert!(reaction_workspace::sequence_active(&app.reaction_workspace));
        assert_eq!(
            reaction_workspace::placed_atom_count(&app.reaction_workspace),
            3
        );
        app.update(Message::ReactionWorkspace(
            reaction_workspace::Message::WorkspaceReturned,
        ));
        assert!(!reaction_workspace::sequence_active(
            &app.reaction_workspace
        ));
    }

    #[test]
    fn reviewed_silver_chloride_route_builds_trusted_frames_before_2d_and_3d() {
        let mut app = App::default();

        for atomic_number in [47, 7, 8, 8, 8] {
            app.update(Message::PeriodicTable(periodic_table::Message::Activated(
                atomic_number,
            )));
        }
        app.update(Message::ReactantComposer(
            reactant_composer::Message::Activate(reactant_composer::ActiveReactant::Second),
        ));
        for atomic_number in [11, 17] {
            app.update(Message::PeriodicTable(periodic_table::Message::Activated(
                atomic_number,
            )));
        }
        app.update(Message::ReactantComposer(
            reactant_composer::Message::StartReactionRequested,
        ));

        assert_eq!(app.screen, Screen::Structural2d);
        let animation = app
            .structural_animation
            .as_ref()
            .expect("reviewed rule should create trusted structural frames");
        assert_eq!(animation.frames.len(), 4);
        assert!(animation.educational_plan.scenes.len() > animation.frames.len());
        assert_eq!(
            animation.real_world_plan.profile_id,
            "presentation.aqueous-precipitation"
        );
        assert_eq!(
            animation.disposition,
            ValidationDisposition::ValidatedWithAssumptions
        );
        assert!(app.structural_error.is_none());

        app.update(Message::ContinueTo3d);
        assert_eq!(app.screen, Screen::Structural2d);
        let scene_count = app
            .structural_animation
            .as_ref()
            .map_or(0, |animation| animation.educational_plan.scenes.len());
        for _ in 1..scene_count {
            app.change_structural_frame(1);
        }
        app.update(Message::ContinueTo3d);
        assert_eq!(app.screen, Screen::Structural3d);

        let _ = app.structural_2d_view(Size::new(1_440.0, 900.0));
        let _ = app.structural_3d_view(Size::new(1_440.0, 900.0));
    }

    #[test]
    fn lithium_water_route_uses_the_same_planners_and_reusable_scene_renderer() {
        let mut app = App::default();
        app.update(Message::PeriodicTable(periodic_table::Message::Activated(
            3,
        )));
        app.update(Message::ReactantComposer(
            reactant_composer::Message::Activate(reactant_composer::ActiveReactant::Second),
        ));
        for atomic_number in [1, 8, 1] {
            app.update(Message::PeriodicTable(periodic_table::Message::Activated(
                atomic_number,
            )));
        }
        app.update(Message::ReactantComposer(
            reactant_composer::Message::StartReactionRequested,
        ));

        let animation = app
            .structural_animation
            .as_ref()
            .expect("reviewed lithium-water rule should animate");
        assert_eq!(app.screen, Screen::Structural2d);
        assert_eq!(app.active_catalogue, BundledCatalogue::ReactiveMetals);
        assert_eq!(animation.frames.len(), 11);
        assert!(animation.educational_plan.scenes.len() > animation.frames.len());
        assert_eq!(
            animation.real_world_plan.profile_id,
            "presentation.reactive-metal-on-water"
        );
        assert_eq!(animation.safety_notices.len(), 1);
    }

    #[test]
    fn real_world_playback_advances_continuously_and_speed_is_media_state() {
        let mut app = App::default();
        LITHIUM_WATER_SOURCE.clone_into(&mut app.source);
        app.source_content = text_editor::Content::with_text(LITHIUM_WATER_SOURCE);
        app.active_catalogue = BundledCatalogue::ReactiveMetals;
        app.prepare_structural_animation();
        app.screen = Screen::Structural3d;

        app.advance_real_world_playback(2_400);
        assert_eq!(
            app.structural_animation
                .as_ref()
                .map(|animation| animation.frame_index),
            Some(1)
        );
        app.update(Message::StructuralSpeedChanged);
        assert_eq!(
            app.structural_animation
                .as_ref()
                .map(|animation| animation.playback_speed),
            Some(PlaybackSpeed::OneAndHalf)
        );
    }
}
