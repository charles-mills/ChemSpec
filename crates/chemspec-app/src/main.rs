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
mod product_summary;
mod reactant_composer;
mod scene_registry;
mod structural_2d;
mod structural_3d;
mod structural_physics;
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
    DynamicPresentationOutcome, FAST_CLAIM_TIMEOUT, LatencyMilestones, OutcomeSpecies,
    ReactantIdentityAmbiguity, ReactantInput, ReactionBuildRequest, ReactionClaim,
    RequestIdentityResolution, TrustTier, ValidatedStaticOutcome, compile_claim_outcome,
    enrich_static_outcome, load_dynamic_cache, resolve_request_identities_with_catalogue,
    reviewed_species_registry, store_dynamic_cache,
};
use chem_domain::SpeciesId;
use chem_presentation::{
    AppearanceProfile, AssetProfile, EducationalPlan, EducationalSceneKind, EffectProfile,
    PresentationObject, PresentationProfile, PresentationTransform, ScenePlan, SceneRole,
    TimelinePosition, compile_educational_plan, compile_real_world_plan,
};
use iced::widget::{
    button, canvas, column, container, mouse_area, responsive, row, rule, scrollable, slider,
    space, stack, text, text_input, tooltip,
};
use iced::{Center, Element, Fill, FillPortion, Length, Size, Subscription, Task, Theme};

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
    keyboard_selected: bool,
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
        .style(move |_, status| theme::provider_button(keyboard_selected, status))
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

fn typewriter_value(value: &str, line: usize, elapsed_ms: u64) -> (String, bool, bool) {
    const INTRO_DELAY_MS: u64 = 520;
    const LINE_STAGGER_MS: u64 = 330;
    const CHARACTER_MS: u64 = 24;
    let start = INTRO_DELAY_MS.saturating_add(
        u64::try_from(line)
            .unwrap_or(u64::MAX)
            .saturating_mul(LINE_STAGGER_MS),
    );
    if elapsed_ms < start {
        return (String::new(), false, false);
    }
    let visible =
        usize::try_from(elapsed_ms.saturating_sub(start) / CHARACTER_MS).unwrap_or(usize::MAX);
    let total = value.chars().count();
    let complete = visible >= total;
    let mut rendered = value.chars().take(visible).collect::<String>();
    if !complete && (elapsed_ms / 260).is_multiple_of(2) {
        rendered.push('▌');
    }
    (rendered, true, complete)
}

#[allow(clippy::too_many_lines)]
fn product_properties_view(
    summary: &product_summary::SummaryData,
    elapsed_ms: u64,
    compact: bool,
    dense: bool,
) -> Element<'static, Message> {
    let pulse = (elapsed_ms / 420).is_multiple_of(2);
    let panel_spacing = if dense { spacing::XS } else { spacing::SM };
    let row_padding = if dense {
        [spacing::XXS, spacing::XS]
    } else {
        [spacing::XS, spacing::SM]
    };
    let mut content = column![
        row![
            column![
                text("MOLECULAR PROPERTIES")
                    .size(type_scale::MICRO)
                    .color(color::ACCENT),
                text("Validated product record")
                    .size(if dense {
                        type_scale::BODY_LARGE
                    } else {
                        type_scale::TITLE
                    })
                    .font(fonts::SEMIBOLD)
                    .color(color::TEXT),
            ]
            .spacing(spacing::XXS),
            space().width(Fill),
            row![
                text(if pulse { "●" } else { "○" })
                    .size(type_scale::CAPTION)
                    .color(color::ACCENT),
                text("READING")
                    .size(type_scale::MICRO)
                    .color(color::TEXT_SOFT),
            ]
            .spacing(spacing::XXS)
            .align_y(Center),
        ]
        .align_y(Center),
        rule::horizontal(1).style(theme::soft_rule),
        text("Properties are compiled locally from the trusted final frame and bundled element metadata.")
            .size(type_scale::CAPTION)
            .color(color::TEXT_SOFT),
    ]
    .spacing(panel_spacing);
    let mut line = 0_usize;
    for product in &summary.products {
        let product_start =
            260_u64.saturating_add(u64::try_from(line).unwrap_or(u64::MAX).saturating_mul(330));
        let product_visible = elapsed_ms >= product_start;
        let heading = container(
            row![
                column![
                    text(product.display_name())
                        .size(type_scale::BODY_LARGE)
                        .font(fonts::SEMIBOLD)
                        .color(if product_visible {
                            color::TEXT
                        } else {
                            color::FAINT
                        }),
                    text(if product_visible {
                        product.classification
                    } else {
                        "Awaiting validated structure…"
                    })
                    .size(type_scale::MICRO)
                    .color(if product_visible {
                        color::ACCENT
                    } else {
                        color::MUTED
                    }),
                ]
                .spacing(spacing::XXS)
                .width(FillPortion(3)),
                text(if product_visible {
                    product.formula.clone()
                } else {
                    "···".to_owned()
                })
                .size(type_scale::TITLE)
                .font(fonts::SEMIBOLD)
                .color(if product_visible {
                    color::TEXT
                } else {
                    color::FAINT
                })
                .width(FillPortion(2))
                .align_x(iced::Right),
            ]
            .spacing(panel_spacing)
            .align_y(Center),
        )
        .style(theme::summary_product_heading)
        .padding(row_padding);
        content = content.push(heading);
        for (label, value) in product.property_rows() {
            let (typed, started, complete) = typewriter_value(&value, line, elapsed_ms);
            let active = started && !complete;
            let label = text(label)
                .size(if dense {
                    type_scale::MICRO
                } else {
                    type_scale::CAPTION
                })
                .color(if started {
                    color::TEXT_SOFT
                } else {
                    color::MUTED
                });
            let value = text(if started { typed } else { "—".to_owned() })
                .size(if dense {
                    type_scale::CAPTION
                } else {
                    type_scale::BODY
                })
                .font(if active {
                    fonts::MEDIUM
                } else {
                    fonts::REGULAR
                })
                .color(if active {
                    color::ACCENT_HOVER
                } else if complete {
                    color::TEXT
                } else {
                    color::FAINT
                });
            let row_content: Element<'static, Message> = if compact {
                column![label, value]
                    .spacing(spacing::XXS)
                    .width(Fill)
                    .into()
            } else {
                row![
                    label.width(FillPortion(2)),
                    value.width(FillPortion(3)).align_x(iced::Right),
                ]
                .spacing(panel_spacing)
                .align_y(Center)
                .width(Fill)
                .into()
            };
            let row = container(row_content)
                .style(move |_| theme::summary_property_row(started, active))
                .padding(row_padding);
            content = content.push(row);
            line += 1;
        }
    }
    content = content.push(
        container(
            row![
                text("TRUST BOUNDARY")
                    .size(type_scale::MICRO)
                    .color(color::SUCCESS),
                space().width(Fill),
                text("DETERMINISTIC · OFFLINE")
                    .size(type_scale::MICRO)
                    .color(color::TEXT_SOFT),
            ]
            .align_y(Center),
        )
        .style(theme::summary_trust_strip)
        .padding(row_padding),
    );
    let content: Element<'static, Message> = if compact {
        content.into()
    } else {
        let scroll_content = container(content)
            .padding(iced::Padding {
                right: spacing::MD,
                ..iced::Padding::ZERO
            })
            .width(Fill);
        scrollable(scroll_content).height(Fill).into()
    };
    container(content)
        .style(theme::summary_properties_panel)
        .padding(if dense {
            spacing::XS
        } else if compact {
            spacing::SM
        } else {
            spacing::MD
        })
        .width(Fill)
        .height(if compact { Length::Shrink } else { Fill })
        .into()
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
        EffectProfile::LiquidMixing => "Liquid mixing",
        EffectProfile::ObjectShrinkage => "Reactant consumption",
        EffectProfile::PrecipitateFormation => "Precipitate formation",
        EffectProfile::Clouding => "Solution clouding",
        EffectProfile::ColourTransition => "Colour transition",
        EffectProfile::SplashEmitter => "Fine droplets",
        EffectProfile::HeatDistortion => "Heat distortion",
        EffectProfile::FlameEmitter(_) => "Flame plume",
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
///
/// The zoom is quantized so the total device pixel ratio (monitor scale ×
/// zoom) stays an integer. A continuous zoom puts every glyph baseline and
/// quad edge on fractional device rows, where the text renderer's Y-axis
/// pixel snapping visibly floats labels above the centre of buttons.
fn adaptive_zoom(reported: Size, current_zoom: f32, monitor_scale: f32) -> f32 {
    let width = reported.width * current_zoom;
    let height = reported.height * current_zoom;
    let desired = (width / DESIGN_SIZE.width).min(height / DESIGN_SIZE.height);
    let scale = if monitor_scale.is_finite() && monitor_scale >= 1.0 {
        monitor_scale
    } else {
        1.0
    };
    ((desired * scale).floor() / scale).clamp(1.0, MAX_UI_ZOOM)
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
                colour_transition: None,
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
                colour_transition: None,
            },
        ],
        effects: Vec::new(),
        camera: Vec::new(),
        equation: equation.to_owned(),
        disclosure: "Representative virtual presentation.".to_owned(),
    }
}

fn launch_state() -> App {
    let mut app = App {
        dump_frame_path: std::env::args()
            .find_map(|argument| argument.strip_prefix("--dump-frame=").map(Into::into)),
        ..App::default()
    };
    let smoke_mode = std::env::args().find_map(|argument| SmokeMode::from_argument(&argument));
    let smoke_from_start = std::env::args().any(|argument| argument == "--smoke-from-start");
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
            if three_dimensional && !smoke_from_start {
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
            animation.playing = smoke_from_start;
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
    ProductSummary,
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
    MonitorScaleMeasured(f32),
    DumpFrame,
    FrameCaptured(std::path::PathBuf, iced::window::Screenshot),
    DynamicOverlayDismissed,
    /// Swallows clicks on the overlay panel so they miss the scrim.
    Noop,
    KeyboardEvent {
        event: iced::keyboard::Event,
        status: iced::event::Status,
    },
    PointerPressed,
    BuilderInputFocusChecked {
        reactant: reactant_composer::ActiveReactant,
        focused: bool,
    },
    ScreenSelected(Screen),
    ProviderSelected(ProviderChoice),
    ApiKeyChanged(String),
    ProviderContinue,
    PeriodicTable(periodic_table::Message),
    ReactantComposer(reactant_composer::Message),
    BuilderPanelToggled(BuilderPanel),
    BuilderPanelClosed,
    DynamicContextSelected(Option<DynamicRequestContext>),
    ToggleDynamicDetails,
    DynamicIdentitySelected {
        reactant_index: usize,
        species_id: SpeciesId,
    },
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
    StructuralSkipRequested(i8),
    StructuralRestarted,
    StructuralTick,
    StructuralDrag(structural_2d::DragEvent),
    ContinueTo3d,
    ContinueToSummary,
    ReturnTo2d,
    ReturnTo3d,
    OutcomeChoiceMoved(i8),
    OutcomeChoiceConfirmed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BuilderPanel {
    Conditions,
    Help,
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

fn builder_keyboard_message(
    screen: Screen,
    event: iced::keyboard::Event,
    status: iced::event::Status,
    editor_open: bool,
    panel_open: bool,
    can_run: bool,
) -> Option<Message> {
    let iced::keyboard::Event::KeyPressed { key, modifiers, .. } = event else {
        return None;
    };
    if status == iced::event::Status::Captured
        && editor_open
        && key != iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape)
    {
        return None;
    }
    builder_shortcut(screen, &key, modifiers, editor_open, panel_open, can_run)
}

fn builder_shortcut(
    screen: Screen,
    key: &iced::keyboard::Key,
    modifiers: iced::keyboard::Modifiers,
    editor_open: bool,
    panel_open: bool,
    can_run: bool,
) -> Option<Message> {
    use iced::keyboard::{Key, key::Named};

    if screen != Screen::Builder {
        return None;
    }
    if key == &Key::Named(Named::Escape) {
        return if editor_open {
            Some(Message::ReactantComposer(
                reactant_composer::Message::NameEntryCancelled,
            ))
        } else if panel_open {
            Some(Message::BuilderPanelClosed)
        } else {
            None
        };
    }
    let is_space = matches!(key.as_ref(), Key::Named(Named::Space) | Key::Character(" "));
    if !modifiers.command() && is_space && can_run && !editor_open && !panel_open {
        return Some(Message::ReactantComposer(
            reactant_composer::Message::StartReactionRequested,
        ));
    }
    if !modifiers.command() && !editor_open && !panel_open {
        match key.as_ref() {
            Key::Named(Named::ArrowLeft) => {
                return Some(Message::ReactantComposer(
                    reactant_composer::Message::SelectReactant(
                        reactant_composer::ActiveReactant::First,
                    ),
                ));
            }
            Key::Named(Named::ArrowRight) => {
                return Some(Message::ReactantComposer(
                    reactant_composer::Message::SelectReactant(
                        reactant_composer::ActiveReactant::Second,
                    ),
                ));
            }
            Key::Character("?") => return Some(Message::BuilderPanelToggled(BuilderPanel::Help)),
            _ => {}
        }
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
        _ => None,
    }
}

fn provider_keyboard_message(
    event: iced::keyboard::Event,
    status: iced::event::Status,
    provider: Option<ProviderChoice>,
    codex_available: bool,
) -> Option<Message> {
    use iced::keyboard::{Key, key::Named};

    if status == iced::event::Status::Captured {
        return None;
    }
    let iced::keyboard::Event::KeyPressed {
        key,
        modifiers,
        repeat,
        ..
    } = event
    else {
        return None;
    };
    if modifiers.command() || modifiers.alt() {
        return None;
    }

    let available = if codex_available {
        [
            Some(ProviderChoice::Local),
            Some(ProviderChoice::CodexSubscription),
            Some(ProviderChoice::ApiKey),
        ]
    } else {
        [
            Some(ProviderChoice::Local),
            Some(ProviderChoice::ApiKey),
            None,
        ]
    };
    let choices = available.into_iter().flatten().collect::<Vec<_>>();
    match key.as_ref() {
        Key::Named(Named::ArrowUp | Named::ArrowLeft) => {
            let current = choices
                .iter()
                .position(|choice| Some(*choice) == provider)
                .unwrap_or(0);
            let next = current
                .checked_sub(1)
                .unwrap_or(choices.len().saturating_sub(1));
            Some(Message::ProviderSelected(choices[next]))
        }
        Key::Named(Named::ArrowDown | Named::ArrowRight) => {
            let current = choices
                .iter()
                .position(|choice| Some(*choice) == provider)
                .unwrap_or(0);
            Some(Message::ProviderSelected(
                choices[(current + 1) % choices.len()],
            ))
        }
        Key::Named(Named::Enter) if !repeat => Some(Message::ProviderContinue),
        Key::Character("1") if !repeat => Some(Message::ProviderSelected(ProviderChoice::Local)),
        Key::Character("2") if !repeat && codex_available => {
            Some(Message::ProviderSelected(ProviderChoice::CodexSubscription))
        }
        Key::Character("3") if !repeat => Some(Message::ProviderSelected(ProviderChoice::ApiKey)),
        _ => None,
    }
}

fn screen_keyboard_message(
    screen: Screen,
    event: iced::keyboard::Event,
    status: iced::event::Status,
) -> Option<Message> {
    use iced::keyboard::{Key, key::Named};

    if status == iced::event::Status::Captured {
        return None;
    }
    let iced::keyboard::Event::KeyPressed {
        key,
        modifiers,
        repeat,
        ..
    } = event
    else {
        return None;
    };
    if modifiers.command() || modifiers.alt() || modifiers.control() {
        return None;
    }

    match screen {
        Screen::OutcomeChoice => match key.as_ref() {
            Key::Named(Named::ArrowUp | Named::ArrowLeft) => Some(Message::OutcomeChoiceMoved(-1)),
            Key::Named(Named::ArrowDown | Named::ArrowRight) => {
                Some(Message::OutcomeChoiceMoved(1))
            }
            Key::Named(Named::Enter) if !repeat => Some(Message::OutcomeChoiceConfirmed),
            Key::Named(Named::Escape) if !repeat => Some(Message::ScreenSelected(Screen::Builder)),
            _ => None,
        },
        Screen::Structural2d | Screen::Structural3d => match key.as_ref() {
            Key::Named(Named::Space) | Key::Character(" ") if !repeat => {
                Some(Message::StructuralPlaybackToggled)
            }
            Key::Named(Named::ArrowLeft) => Some(Message::StructuralSkipRequested(-1)),
            Key::Named(Named::ArrowRight) => Some(Message::StructuralSkipRequested(1)),
            Key::Character(value) if !repeat && value.eq_ignore_ascii_case("r") => {
                Some(Message::StructuralRestarted)
            }
            Key::Character(value) if !repeat && value.eq_ignore_ascii_case("s") => {
                Some(Message::StructuralSpeedChanged)
            }
            Key::Named(Named::Enter) if !repeat && screen == Screen::Structural2d => {
                Some(Message::ContinueTo3d)
            }
            Key::Named(Named::Enter) if !repeat => Some(Message::ContinueToSummary),
            Key::Named(Named::Escape) if !repeat && screen == Screen::Structural2d => {
                Some(Message::ScreenSelected(Screen::Builder))
            }
            Key::Named(Named::Escape) if !repeat => Some(Message::ReturnTo2d),
            _ => None,
        },
        Screen::ProductSummary => match key.as_ref() {
            Key::Named(Named::Escape | Named::ArrowLeft) if !repeat => Some(Message::ReturnTo3d),
            Key::Character(value) if !repeat && value.eq_ignore_ascii_case("n") => {
                Some(Message::ScreenSelected(Screen::Builder))
            }
            _ => None,
        },
        Screen::ProviderSetup | Screen::Builder => None,
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
    summary_elapsed_ms: u64,
    playing: bool,
    playback_speed: PlaybackSpeed,
    physics: structural_physics::Simulation,
    /// Smoothed 2D camera rectangle, in virtual world coordinates.
    camera: iced::Rectangle,
    /// Story-anchored homes per frame, parallel to `frames.frames()`.
    home_timeline: Vec<std::collections::BTreeMap<String, iced::Point>>,
    /// Paused with physics and camera at rest: the tick subscription stops
    /// so a static scene costs nothing. Any interaction clears it.
    settled: bool,
}

#[allow(clippy::struct_excessive_bools)]
struct App {
    screen: Screen,
    /// Keyboard-only selection and shortcut hints stay hidden until a
    /// recognized key is used, then disappear on the next pointer press.
    keyboard_navigation_active: bool,
    keyboard_outcome_index: Option<usize>,
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
    validated_macroscopic: Option<chem_presentation::MacroscopicReaction>,
    dynamic_claim: Option<ReactionClaim>,
    dynamic_static: Option<ValidatedStaticOutcome>,
    dynamic_presentation: Option<DynamicPresentationOutcome>,
    dynamic_request: Option<ReactionBuildRequest>,
    dynamic_identity_choice: Option<DynamicIdentityChoice>,
    dynamic_context: Option<DynamicRequestContext>,
    builder_panel: Option<BuilderPanel>,
    dynamic_details_open: bool,
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
    /// The user closed the dynamic-build overlay; completion events reopen it.
    dynamic_overlay_dismissed: bool,
    /// Debug harness: dump one frame to this path, then keep running.
    dump_frame_path: Option<std::path::PathBuf>,
    /// The active monitor's scale factor, used to quantize `ui_zoom`.
    monitor_scale: f32,
    /// Last window size in zoom-invariant design units.
    window_design_size: Size,
}

impl Default for App {
    fn default() -> Self {
        let codex_available = codex_available();
        let active_request = chemistry::ReactionRequest::DEFAULT;
        let trusted_run = chemistry::run(active_request).ok();
        Self {
            screen: Screen::ProviderSetup,
            keyboard_navigation_active: false,
            keyboard_outcome_index: None,
            smoke_mode: None,
            codex_available,
            provider: Some(ProviderChoice::Local),
            api_key: String::new(),
            periodic_table: periodic_table::State::default(),
            reactant_composer: reactant_composer::State::default(),
            pending_requests: Vec::new(),
            oxygen_assessment: None,
            active_request,
            validated_frames: trusted_run
                .as_ref()
                .map(|run| RenderableFrames::Catalogue(run.frames().clone())),
            validated_macroscopic: trusted_run
                .as_ref()
                .and_then(|run| run.macroscopic().cloned()),
            dynamic_claim: None,
            dynamic_static: None,
            dynamic_presentation: None,
            dynamic_request: None,
            dynamic_identity_choice: None,
            dynamic_context: None,
            builder_panel: None,
            dynamic_details_open: false,
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
            dynamic_overlay_dismissed: false,
            dump_frame_path: None,
            monitor_scale: 1.0,
            window_design_size: DESIGN_SIZE,
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
            Message::WindowResized(size) => {
                self.window_design_size =
                    Size::new(size.width * self.ui_zoom, size.height * self.ui_zoom);
                self.ui_zoom = adaptive_zoom(size, self.ui_zoom, self.monitor_scale);
                reactant_composer::resize_ambient(&mut self.reactant_composer, size);
                // The quantization step depends on the monitor's own scale
                // factor, which can change when the window switches displays.
                return iced::window::latest()
                    .and_then(iced::window::scale_factor)
                    .map(Message::MonitorScaleMeasured);
            }
            Message::DumpFrame => {
                if let Some(path) = self.dump_frame_path.take() {
                    return iced::window::latest()
                        .and_then(iced::window::screenshot)
                        .map(move |shot| Message::FrameCaptured(path.clone(), shot));
                }
            }
            Message::FrameCaptured(path, shot) => {
                let mut ppm = format!("P6\n{} {}\n255\n", shot.size.width, shot.size.height)
                    .into_bytes();
                for pixel in shot.rgba.chunks_exact(4) {
                    ppm.extend_from_slice(&pixel[..3]);
                }
                let _ = std::fs::write(&path, ppm);
                let _ = std::fs::write(
                    path.with_extension("meta"),
                    format!("scale_factor={}\nui_zoom={}\n", shot.scale_factor, self.ui_zoom),
                );
            }
            Message::DynamicOverlayDismissed => {
                self.dynamic_overlay_dismissed = true;
            }
            Message::Noop => {}
            Message::MonitorScaleMeasured(scale) => {
                if (scale - self.monitor_scale).abs() > f32::EPSILON {
                    self.monitor_scale = scale;
                    self.ui_zoom = adaptive_zoom(self.window_design_size, 1.0, scale);
                }
            }
            Message::KeyboardEvent { event, status } => {
                let message = match self.screen {
                    Screen::Builder => builder_keyboard_message(
                        self.screen,
                        event,
                        status,
                        reactant_composer::editing(&self.reactant_composer).is_some(),
                        self.builder_panel.is_some(),
                        self.builder_can_submit(),
                    ),
                    Screen::ProviderSetup => provider_keyboard_message(
                        event,
                        status,
                        self.provider,
                        self.codex_available,
                    ),
                    Screen::OutcomeChoice
                    | Screen::Structural2d
                    | Screen::Structural3d
                    | Screen::ProductSummary => screen_keyboard_message(self.screen, event, status),
                };
                if let Some(message) = message {
                    self.keyboard_navigation_active = true;
                    return self.update_with_task(message);
                }
            }
            Message::PointerPressed => {
                self.keyboard_navigation_active = false;
                self.keyboard_outcome_index = None;
                let Some(reactant) = reactant_composer::editing(&self.reactant_composer) else {
                    return Task::none();
                };
                return iced::widget::operation::is_focused(reactant_composer::name_input_id(
                    reactant,
                ))
                .map(move |focused| Message::BuilderInputFocusChecked { reactant, focused });
            }
            Message::BuilderInputFocusChecked { reactant, focused } => {
                if !focused
                    && reactant_composer::editing(&self.reactant_composer) == Some(reactant)
                    && reactant_composer::name_input_is_empty(&self.reactant_composer)
                {
                    return self
                        .update_reactant_composer(reactant_composer::Message::NameEntryCancelled);
                }
            }
            Message::ScreenSelected(screen) => {
                self.screen = screen;
                self.keyboard_outcome_index = None;
            }
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
                    return self.update_reactant_composer(reactant_composer::Message::AddElement(
                        atomic_number,
                    ));
                }
            }
            Message::ReactantComposer(message) => {
                return self.update_reactant_composer(message);
            }
            Message::BuilderPanelToggled(panel) => {
                self.builder_panel = (self.builder_panel != Some(panel)).then_some(panel);
            }
            Message::BuilderPanelClosed => self.builder_panel = None,
            Message::DynamicContextSelected(context) => {
                self.dynamic_context = context;
                self.builder_panel = None;
                self.sync_builder_submit_prompt();
            }
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
            Message::DynamicClaimFinished { run_id, result } => {
                if !matches!(self.dynamic_build, DynamicBuildState::Running { run_id: current, stage: DynamicBuildStage::Claim, .. } if current == run_id)
                {
                    return Task::none();
                }
                self.dynamic_overlay_dismissed = false;
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
                            ClaimMode::Fast,
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
                self.dynamic_overlay_dismissed = false;
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
                self.dynamic_overlay_dismissed = false;
                let progress = self.reset_dynamic_progress_channel();
                return Self::start_dynamic_presentation(
                    run_id,
                    request,
                    ClaimMode::Fast,
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
                    if !animation.playing
                        && self.screen == Screen::Structural2d
                        && animation.educational_playhead_ms
                            == animation.educational_plan.duration_ms()
                    {
                        animation.educational_playhead_ms = 0;
                        sync_educational_frame(animation);
                    } else if !animation.playing
                        && self.screen == Screen::Structural3d
                        && animation.real_world_playhead_ms
                            == animation.real_world_plan.timeline.duration_ms()
                    {
                        animation.real_world_playhead_ms = 0;
                        animation.frame_index = 0;
                    }
                    animation.playing = !animation.playing;
                    animation.settled = false;
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
                    animation.settled = false;
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
            Message::StructuralSkipRequested(delta) => {
                if self.screen == Screen::Structural2d {
                    self.change_structural_frame(delta);
                } else if self.screen == Screen::Structural3d {
                    let Some(animation) = &mut self.structural_animation else {
                        return Task::none();
                    };
                    animation.playing = false;
                    let playhead = animation.real_world_playhead_ms;
                    let target = if delta < 0 {
                        playhead.saturating_sub(5_000)
                    } else {
                        playhead.saturating_add(5_000)
                    };
                    self.seek_real_world_timeline(target);
                }
            }
            Message::StructuralTick => {
                let (elapsed, playing) = self
                    .structural_animation
                    .as_ref()
                    .map_or((33, false), |animation| {
                        (animation.playback_speed.scale_millis(33), animation.playing)
                    });
                if self.screen == Screen::ProductSummary {
                    if let Some(animation) = &mut self.structural_animation {
                        animation.summary_elapsed_ms =
                            animation.summary_elapsed_ms.saturating_add(33);
                    }
                } else if self.screen == Screen::Structural3d {
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
                    animation.settled = false;
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
                    animation.settled = false;
                }
            }
            Message::ContinueTo3d => {
                // The 3D model is always reachable from the 2D explanation;
                // entering it restarts the macroscopic playback.
                if let Some(animation) = &mut self.structural_animation {
                    animation.frame_index = 0;
                    animation.real_world_playhead_ms = 0;
                    animation.playing = true;
                    self.screen = Screen::Structural3d;
                }
            }
            Message::ContinueToSummary => {
                // The product summary is always reachable from the 3D model,
                // mirroring the free 2D ⇄ 3D navigation.
                if let Some(animation) = &mut self.structural_animation {
                    animation.summary_elapsed_ms = 0;
                    animation.playing = false;
                    self.screen = Screen::ProductSummary;
                }
            }
            Message::ReturnTo2d => self.screen = Screen::Structural2d,
            Message::ReturnTo3d => self.screen = Screen::Structural3d,
            Message::OutcomeChoiceMoved(delta) => {
                if self.pending_requests.is_empty() {
                    return Task::none();
                }
                let len = self.pending_requests.len();
                self.keyboard_outcome_index = Some(match self.keyboard_outcome_index {
                    Some(current) if delta < 0 => current.checked_sub(1).unwrap_or(len - 1),
                    Some(current) => (current + 1) % len,
                    None if delta < 0 => len - 1,
                    None => 0,
                });
            }
            Message::OutcomeChoiceConfirmed => {
                let Some(request) = self
                    .keyboard_outcome_index
                    .or((self.pending_requests.len() == 1).then_some(0))
                    .and_then(|index| self.pending_requests.get(index))
                    .copied()
                else {
                    return Task::none();
                };
                return self.update_with_task(Message::OutcomeSelected(request));
            }
        }
        Task::none()
    }

    #[cfg(test)]
    fn update(&mut self, message: Message) {
        drop(self.update_with_task(message));
    }

    fn builder_can_submit(&self) -> bool {
        if matches!(self.dynamic_build, DynamicBuildState::Running { .. })
            || reactant_composer::editing(&self.reactant_composer).is_some()
        {
            return false;
        }
        let (first, second) = reactant_composer::reactants(&self.reactant_composer);
        reactant_composer::can_start_reaction(&self.reactant_composer)
            || (!first.is_empty() && second.is_empty() && self.dynamic_context.is_some())
    }

    fn sync_builder_submit_prompt(&mut self) {
        let available = self.builder_can_submit();
        reactant_composer::set_submit_available(&mut self.reactant_composer, available);
    }

    fn update_reactant_composer(&mut self, message: reactant_composer::Message) -> Task<Message> {
        let focus_target = match &message {
            reactant_composer::Message::BeginNameEntry(reactant) => {
                Some(reactant_composer::name_input_id(*reactant))
            }
            _ => None,
        };
        if matches!(message, reactant_composer::Message::StartReactionRequested)
            && matches!(self.dynamic_build, DynamicBuildState::Running { .. })
        {
            return Task::none();
        }
        if !matches!(message, reactant_composer::Message::StartReactionRequested) {
            // Presentation-only motion (ambient orbit and prompt fades) must
            // never cancel a running build or wipe a finished result; only
            // actual draft edits invalidate dynamic state.
            if message.is_presentation_only() {
                reactant_composer::update(&mut self.reactant_composer, message);
                return Task::none();
            }
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
            let (first, second) = reactant_composer::reactants(&self.reactant_composer);
            if !second.is_empty() {
                self.dynamic_context = None;
            }
            if self.builder_panel == Some(BuilderPanel::Conditions)
                && (first.is_empty() || !second.is_empty())
            {
                self.builder_panel = None;
            }
            self.sync_builder_submit_prompt();
            return focus_target.map_or_else(Task::none, iced::widget::operation::focus);
        }
        reactant_composer::set_submit_available(&mut self.reactant_composer, false);
        self.builder_panel = None;
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
        self.dynamic_overlay_dismissed = false;
        self.dynamic_started_at = None;
        self.dynamic_latency = LatencyMilestones::default();
        self.dynamic_theatre_phase = 0.0;
        self.dynamic_progress = None;
        self.dynamic_progress_receiver = None;
        self.structural_animation = None;
        self.structural_error = None;
        let mode = ClaimMode::Fast;
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
                let deadline = started + FAST_CLAIM_TIMEOUT;
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
                if !algorithmic
                    && let Some(directory) = provider.config().cache_directory.as_deref()
                {
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
        // Auto-navigating into the animation replaces the overlay; a
        // static-only result surfaces it on the builder instead.
        self.dynamic_overlay_dismissed = animated;
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
        let trusted_run = chemistry::run(request).ok();
        self.validated_frames = trusted_run
            .as_ref()
            .map(|run| RenderableFrames::Catalogue(run.frames().clone()));
        self.validated_macroscopic = trusted_run
            .as_ref()
            .and_then(|run| run.macroscopic().cloned());
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
        animation.settled = false;
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
        let frame_dump = if self.dump_frame_path.is_some() {
            iced::time::every(std::time::Duration::from_millis(1_200)).map(|_| Message::DumpFrame)
        } else {
            Subscription::none()
        };
        let screen = if self.screen == Screen::Builder {
            Subscription::batch([
                periodic_table::subscription(&self.periodic_table).map(Message::PeriodicTable),
                reactant_composer::subscription(&self.reactant_composer)
                    .map(Message::ReactantComposer),
            ])
        } else if (self.screen == Screen::Structural2d
            && self
                .structural_animation
                .as_ref()
                .is_none_or(|animation| animation.playing || !animation.settled))
            || (self.screen == Screen::Structural3d
                && self
                    .structural_animation
                    .as_ref()
                    .is_some_and(|animation| animation.playing))
        {
            iced::time::every(std::time::Duration::from_millis(33)).map(|_| Message::StructuralTick)
        } else if self.screen == Screen::ProductSummary {
            iced::time::every(theme::motion::TICK).map(|_| Message::StructuralTick)
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

        let input = iced::event::listen_with(|event, status, _window| match event {
            iced::Event::Keyboard(event) => Some(Message::KeyboardEvent { event, status }),
            iced::Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left)) => {
                Some(Message::PointerPressed)
            }
            _ => None,
        });

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

        Subscription::batch([resize, frame_dump, screen, dynamic_build, dynamic_theatre, input])
    }

    fn view(&self) -> Element<'_, Message> {
        match self.screen {
            Screen::ProviderSetup => responsive(|size| self.provider_setup_view(size)).into(),
            Screen::Builder => responsive(|size| self.builder_view(size)).into(),
            Screen::OutcomeChoice => responsive(|size| self.outcome_choice_view(size)).into(),
            Screen::Structural2d => responsive(|size| self.structural_2d_view(size)).into(),
            Screen::Structural3d => responsive(|size| self.structural_3d_view(size)).into(),
            Screen::ProductSummary => responsive(|size| self.product_summary_view(size)).into(),
        }
    }

    #[allow(clippy::too_many_lines)]
    fn outcome_choice_view(&self, size: Size) -> Element<'_, Message> {
        use chem_catalogue::{OxygenOutcome, StructuralSupport};

        let compact = size.width < breakpoint::MOBILE || size.height < 760.0;

        let back = button(text("← Reactants"))
            .on_press(Message::ScreenSelected(Screen::Builder))
            .padding([spacing::XS, spacing::SM])
            .style(theme::secondary_button);

        let content: Element<'_, Message> = if !self.pending_requests.is_empty() {
            let mut choices = column![].spacing(spacing::SM).width(Fill);
            for (index, request) in self.pending_requests.iter().enumerate() {
                choices = choices.push(reviewed_outcome_choice(
                    *request,
                    compact,
                    self.keyboard_navigation_active && self.keyboard_outcome_index == Some(index),
                ));
            }
            let keyboard_hint: Element<'_, Message> = if self.keyboard_navigation_active {
                text("↑ ↓ choose  ·  Enter open  ·  Esc reactants")
                    .size(type_scale::MICRO)
                    .color(color::ACCENT)
                    .into()
            } else {
                space().height(Length::Shrink).into()
            };
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
                keyboard_hint,
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
        if self.keyboard_navigation_active {
            sections.push(
                text("↑ ↓ choose  ·  1–3 select  ·  Enter continue")
                    .size(type_scale::MICRO)
                    .color(color::ACCENT)
                    .into(),
            );
        }

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
                        chemistry::presentation_profile_with_catalogue(
                            self.active_request,
                            &frames,
                            self.validated_macroscopic.as_ref(),
                        )?,
                        self.active_request.reactant_previews(),
                        self.active_request.product_preview(),
                        self.active_request.equation(),
                    )
                };
            let real_world_plan =
                compile_real_world_plan(&frames, &profile).map_err(|error| error.to_string())?;
            let home_timeline = structural_2d::home_timeline(frames.frames());
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
                summary_elapsed_ms: 0,
                playing: true,
                playback_speed: PlaybackSpeed::Normal,
                physics: structural_physics::Simulation::default(),
                camera: structural_2d::default_camera(),
                home_timeline,
                settled: false,
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
        let Some(scene) = animation.educational_plan.scenes.get(position.scene_index) else {
            return;
        };
        let frames = animation.frames.frames();
        let Some(after_index) = frames
            .iter()
            .position(|candidate| candidate.trace().state_digest == scene.end_frame)
        else {
            return;
        };
        let before_index = frames
            .iter()
            .position(|candidate| candidate.trace().state_digest == scene.start_frame)
            .unwrap_or(after_index);
        let after = &frames[after_index];
        let before = &frames[before_index];
        let (Some(before_homes), Some(after_homes)) = (
            animation.home_timeline.get(before_index),
            animation.home_timeline.get(after_index),
        ) else {
            return;
        };
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
        let spec = structural_2d::world_spec(before, after, action, before_homes, after_homes);
        animation.physics.step(&spec);
        // The camera frames the whole chapter (both endpoints), retargets
        // only when the chapter does, and glides — never chases.
        let target = structural_2d::chapter_camera(before, after, before_homes, after_homes);
        let camera_moved = structural_2d::ease_camera(&mut animation.camera, target);
        animation.settled = !animation.playing && !camera_moved && animation.physics.is_settled();
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
        let exit = button(text("← Return"))
            .on_press(Message::ScreenSelected(Screen::Builder))
            .padding([spacing::XS, spacing::SM])
            .style(theme::secondary_button);
        // Local Mode derivations are deterministic; regenerating would only
        // recompute the identical result.
        let regenerate: Element<'_, Message> =
            if self.dynamic_request.is_some() && !self.local_mode() {
                button(text("Regenerate"))
                    .on_press(Message::RegenerateDynamicReaction)
                    .padding([spacing::XS, spacing::SM])
                    .style(theme::secondary_button)
                    .into()
            } else {
                space().width(Length::Shrink).into()
            };
        let continue_3d = button(text("View 3D model  →"))
            .on_press(Message::ContinueTo3d)
            .padding([spacing::XS, spacing::SM])
            .style(theme::secondary_button);

        let equation = plan_equation(animation).map(nomenclature::display_equation);
        let scene_context = structural_2d::SceneContext::new(
            educational_scene.kind,
            timeline_position.scene_index,
            animation.educational_plan.scenes.len(),
        )
        .with_equation(equation.clone());
        let diagram_canvas: Element<'_, structural_2d::DragEvent> =
            canvas(structural_2d::Diagram::new(
                before_frame,
                frame,
                &operation_transitions,
                scene_progress,
                explanation,
                &context_labels,
                scene_context,
                educational_timeline_progress(animation),
                animation.physics.positions(),
                animation.camera,
            ))
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
        let elapsed = text(format!(
            "{}  /  {}",
            format_media_time(animation.educational_playhead_ms),
            format_media_time(duration_ms)
        ))
        .size(type_scale::CAPTION)
        .color(color::TEXT_SOFT);
        let transport: Element<'_, Message> = if compact {
            column![
                row![playback, previous, next, speed]
                    .spacing(spacing::XS)
                    .align_y(Center),
                row![restart, space().width(Fill), elapsed]
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
                elapsed,
            ]
            .spacing(spacing::XS)
            .align_y(Center)
            .into()
        };
        let controls = container(column![transport, timeline].spacing(spacing::XXS))
            .style(theme::media_bar)
            .padding([spacing::XS, spacing::SM]);

        // The buttons form the stack's base layer: a stack sizes itself to
        // its first child, so the row must set the height or the buttons get
        // squeezed and their labels overflow off-centre.
        let header = stack![
            row![exit, regenerate, space().width(Fill), continue_3d]
                .spacing(spacing::XS)
                .align_y(Center),
            container(
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
            )
            .center_x(Fill)
            .center_y(Fill),
        ]
        .width(Fill);

        container(
            column![header, diagram, controls]
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
        let back = button(text("← Return"))
            .on_press(Message::ReturnTo2d)
            .padding([spacing::XS, spacing::SM])
            .style(theme::secondary_button);
        // Local Mode derivations are deterministic; regenerating would only
        // recompute the identical result.
        let regenerate: Element<'_, Message> =
            if self.dynamic_request.is_some() && !self.local_mode() {
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
        let review_products = button(text("Review products  →"))
            .on_press(Message::ContinueToSummary)
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
        let inset_preview = structural_3d::active_molecular_preview(
            real_world_plan,
            moment.ordinal,
            &animation.reactant_previews,
            animation.product_preview.as_ref(),
        );
        let scene_view = iced::widget::Shader::new(structural_3d::Scene::new(
            real_world_plan,
            moment,
            &animation.reactant_previews,
            animation.product_preview.as_ref(),
        ))
        .width(Fill)
        .height(Fill);
        let model_disclosure = if compact {
            "VIRTUAL MODEL · NOT A LAB PROCEDURE"
        } else {
            "VIRTUAL MODEL · NOT A LAB PROCEDURE · TIMING, SCALE & MOTION ARE ILLUSTRATIVE"
        };
        // The caption sits directly above the renderer's molecular inset;
        // both derive their size from the same shared function.
        let inset_caption: Element<'_, Message> = match inset_preview {
            Some(preview) if !compact => {
                let inset_side = structural_3d::molecular_inset_side(size.width, size.height);
                column![
                    container(
                        text(format!("MOLECULAR MODEL · {}", preview.formula))
                            .size(type_scale::MICRO)
                            .color(color::TEXT_SOFT),
                    )
                    .style(theme::media_bar)
                    .padding([spacing::XXS, spacing::XS]),
                    space().height(Length::Fixed(inset_side + 6.0)),
                ]
                .align_x(iced::Right)
                .into()
            }
            _ => space().width(Length::Shrink).into(),
        };
        let annotation_layer = container(
            column![
                row![
                    space().width(Fill),
                    container(
                        text(model_disclosure)
                            .size(type_scale::MICRO)
                            .color(color::TEXT_SOFT),
                    )
                    .style(theme::media_bar)
                    .padding([spacing::XXS, spacing::XS]),
                ]
                .width(Fill),
                space().height(Fill),
                row![
                    container(annotation)
                        .style(theme::media_bar)
                        .padding([spacing::SM, spacing::MD])
                        .width(if compact { Fill } else { Length::Fixed(440.0) }),
                    space().width(Fill),
                    inset_caption,
                ]
                .align_y(iced::Bottom)
                .width(Fill),
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
        let elapsed = text(format!(
            "{}  /  {}",
            format_media_time(animation.real_world_playhead_ms),
            format_media_time(duration_ms)
        ))
        .size(type_scale::CAPTION)
        .color(color::TEXT_SOFT);
        let transport: Element<'_, Message> = if compact {
            column![
                row![playback, restart, speed, space().width(Fill), elapsed]
                    .spacing(spacing::XS)
                    .align_y(Center),
                scrubber,
            ]
            .spacing(spacing::XXS)
            .into()
        } else {
            row![playback, restart, speed, scrubber, elapsed]
                .spacing(spacing::XS)
                .align_y(Center)
                .into()
        };
        let controls = container(transport)
            .style(theme::media_bar)
            .padding([spacing::XS, spacing::SM]);
        // Buttons first: the stack sizes itself to its base layer, so the
        // row must set the height (see the 2D header).
        let header = stack![
            row![back, regenerate, space().width(Fill), review_products]
                .spacing(spacing::XS)
                .align_y(Center),
            container(
                text(nomenclature::display_equation(&real_world_plan.equation))
                    .size(if compact {
                        type_scale::BODY_LARGE
                    } else {
                        type_scale::TITLE
                    })
                    .color(color::TEXT),
            )
            .center_x(Fill)
            .center_y(Fill),
        ]
        .width(Fill);
        container(
            column![header, scene, controls]
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
    /// Trust-tier chip for the current dynamic outcome.
    fn dynamic_trust_label(&self) -> &'static str {
        self.dynamic_static
            .as_ref()
            .map_or("", |outcome| match outcome.trust_tier() {
                TrustTier::Reviewed => "REVIEWED",
                // Local Mode claims come from the algorithmic solver, so the
                // unreviewed tier is derived rather than model-asserted.
                TrustTier::ModelAsserted if self.local_mode() => "DERIVED",
                TrustTier::ModelAsserted => "MODEL ASSERTED",
            })
    }

    #[allow(clippy::too_many_lines)]
    fn dynamic_result_body(&self) -> Element<'_, Message> {
        let Some(outcome) = &self.dynamic_static else {
            return space().height(Length::Shrink).into();
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
        let mut species = row![].spacing(spacing::XS).align_y(Center);
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
            species = species.push(
                container(dynamic_species_theatre_card(
                    species_capability,
                    term,
                    self.dynamic_theatre_phase,
                ))
                .width(Length::Fixed(132.0)),
            );
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
            _ => None,
        };
        let mut actions = row![].spacing(spacing::XS).align_y(Center);
        if self.structural_animation.is_some() {
            actions = actions.push(
                button(text("Watch reaction  →"))
                    .on_press(Message::ReturnTo2d)
                    .padding([spacing::XS, spacing::SM])
                    .style(theme::primary_button),
            );
        }
        if matches!(
            (&self.dynamic_presentation, &self.dynamic_build),
            (
                Some(DynamicPresentationOutcome::Static {
                    retryable: true,
                    ..
                }),
                DynamicBuildState::Idle | DynamicBuildState::Failed(_),
            )
        ) {
            actions = actions.push(
                button(text("Retry mechanism"))
                    .on_press(Message::RetryDynamicPresentation)
                    .padding([spacing::XS, spacing::SM])
                    .style(theme::secondary_button),
            );
        }
        if let Some(diagnostic) = &diagnostic {
            actions = actions.push(
                button(text(if self.dynamic_details_open {
                    "Hide details"
                } else {
                    "Details"
                }))
                .on_press(Message::ToggleDynamicDetails)
                .padding([spacing::XS, spacing::SM])
                .style(theme::secondary_button),
            );
            let _ = diagnostic;
        }
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
        column![
            container(
                text(nomenclature::display_equation(outcome.equation()))
                    .size(type_scale::TITLE)
                    .color(color::TEXT),
            )
            .center_x(Fill),
            container(
                text(outcome.claim().required_context.as_str())
                    .size(type_scale::CAPTION)
                    .color(color::MUTED),
            )
            .center_x(Fill),
            container(species).center_x(Fill),
            container(
                text(observation_copy)
                    .size(type_scale::CAPTION)
                    .color(color::TEXT_SOFT),
            )
            .center_x(Fill),
            container(
                text(presentation)
                    .size(type_scale::CAPTION)
                    .color(color::TEXT_SOFT),
            )
            .center_x(Fill),
            container(actions).center_x(Fill),
            details,
            container(
                text(self.dynamic_latency_summary())
                    .size(type_scale::MICRO)
                    .color(color::MUTED),
            )
            .center_x(Fill),
        ]
        .spacing(spacing::XS)
        .into()
    }

    fn dynamic_verdict_body(&self) -> Element<'_, Message> {
        let Some(claim) = &self.dynamic_claim else {
            return space().height(Length::Shrink).into();
        };
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
        column![
            text(title).size(type_scale::BODY_LARGE).color(color::TEXT),
            text(detail).size(type_scale::CAPTION).color(color::MUTED),
        ]
        .spacing(spacing::XXS)
        .into()
    }

    fn dynamic_running_body(&self) -> Element<'_, Message> {
        let DynamicBuildState::Running {
            stage,
            elapsed_seconds,
            ..
        } = &self.dynamic_build
        else {
            return space().height(Length::Shrink).into();
        };
        let title = if self.local_mode() {
            "Deriving this reaction"
        } else {
            "Codex is researching this reaction"
        };
        let stage_line = match stage {
            DynamicBuildStage::Claim => "Checking the outcome claim",
            DynamicBuildStage::Presentation => {
                "Balanced result ready · checking animation capability"
            }
        };
        let reactants = self.dynamic_request.as_ref().map_or_else(String::new, |request| {
            request
                .reactants
                .iter()
                .map(|reactant| reactant.display.clone())
                .collect::<Vec<_>>()
                .join("  +  ")
        });
        let progress: Element<'_, Message> = self.dynamic_progress_label().map_or_else(
            || space().height(Length::Shrink).into(),
            |label| {
                text(label)
                    .size(type_scale::MICRO)
                    .color(color::ACCENT)
                    .into()
            },
        );
        column![
            text(title).size(type_scale::BODY_LARGE).color(color::TEXT),
            text(reactants).size(type_scale::TITLE).color(color::TEXT),
            text(format!("{stage_line} · {elapsed_seconds}s"))
                .size(type_scale::CAPTION)
                .color(color::TEXT_SOFT),
            progress,
        ]
        .spacing(spacing::XXS)
        .into()
    }

    fn dynamic_failed_body(&self) -> Element<'_, Message> {
        let DynamicBuildState::Failed(error) = &self.dynamic_build else {
            return space().height(Length::Shrink).into();
        };
        column![
            text("Couldn\u{2019}t build this result")
                .size(type_scale::BODY_LARGE)
                .color(color::TEXT),
            text(error).size(type_scale::CAPTION).color(color::MUTED),
        ]
        .spacing(spacing::XXS)
        .into()
    }

    /// The dynamic-build modal: every Tier B/C surface (progress, results,
    /// verdicts, identity choices, failures) lives here instead of inline
    /// cards that squeeze the builder.
    fn dynamic_overlay(&self, size: Size) -> Element<'_, Message> {
        let running = matches!(self.dynamic_build, DynamicBuildState::Running { .. });
        let failed = matches!(self.dynamic_build, DynamicBuildState::Failed(_));
        let has_content = running
            || failed
            || self.dynamic_static.is_some()
            || self.dynamic_claim.is_some()
            || self.dynamic_identity_choice.is_some();
        if self.dynamic_overlay_dismissed || !has_content {
            return space().height(Length::Shrink).into();
        }
        let (chip, chip_color, body) = if self.dynamic_identity_choice.is_some() {
            (
                "IDENTITY CHOICE",
                color::WARNING,
                self.dynamic_identity_choice_body(),
            )
        } else if self.dynamic_static.is_some() {
            (
                self.dynamic_trust_label(),
                color::SUCCESS,
                self.dynamic_result_body(),
            )
        } else if running {
            (
                if self.local_mode() {
                    "LOCAL DERIVATION"
                } else {
                    "CODEX RESEARCH"
                },
                color::ACCENT,
                self.dynamic_running_body(),
            )
        } else if failed {
            ("BUILD FAILED", color::DANGER, self.dynamic_failed_body())
        } else {
            ("OUTCOME", color::WARNING, self.dynamic_verdict_body())
        };
        let mut header = row![text(chip).size(type_scale::MICRO).color(chip_color)]
            .spacing(spacing::XS)
            .align_y(Center);
        if self.dynamic_static.is_some() {
            header = header.push(
                text("VIRTUAL MODEL")
                    .size(type_scale::MICRO)
                    .color(color::WARNING),
            );
        }
        header = header.push(space().width(Fill)).push(
            button(text("\u{00d7}").size(type_scale::BODY_LARGE))
                .on_press(Message::DynamicOverlayDismissed)
                .padding([0.0, spacing::XS])
                .style(theme::secondary_button),
        );
        let panel = mouse_area(
            container(column![header, body].spacing(spacing::SM))
                .style(theme::overlay_panel)
                .padding(spacing::LG)
                .width(Length::Fixed((size.width - 32.0).min(640.0))),
        )
        .on_press(Message::Noop);
        stack![
            mouse_area(
                container(space())
                    .style(theme::overlay_scrim)
                    .width(Fill)
                    .height(Fill),
            )
            .on_press(Message::DynamicOverlayDismissed),
            container(panel).center(Fill),
        ]
        .width(Fill)
        .height(Fill)
        .into()
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

    fn dynamic_identity_choice_body(&self) -> Element<'_, Message> {
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
        alternatives.into()
    }

    fn builder_toolbar(&self, conditions_enabled: bool) -> Element<'_, Message> {
        let conditions_selected =
            self.builder_panel == Some(BuilderPanel::Conditions) || self.dynamic_context.is_some();
        let conditions_color = if conditions_selected {
            color::CANVAS
        } else if conditions_enabled {
            color::TEXT_SOFT
        } else {
            color::FAINT
        };
        let conditions = button(icons::atom(20.0, conditions_color))
            .on_press_maybe(
                conditions_enabled
                    .then_some(Message::BuilderPanelToggled(BuilderPanel::Conditions)),
            )
            .padding(spacing::XS)
            .style(if conditions_selected {
                theme::primary_button
            } else {
                theme::secondary_button
            });
        let conditions: Element<'_, Message> = tooltip(
            conditions,
            text(if conditions_enabled {
                "Reaction conditions"
            } else {
                "Conditions are available for a single reactant"
            })
            .size(type_scale::CAPTION)
            .color(color::TEXT_SOFT),
            tooltip::Position::Bottom,
        )
        .gap(spacing::XS)
        .padding(spacing::XS)
        .style(|_| theme::tooltip_surface(1.0))
        .into();

        let help_selected = self.builder_panel == Some(BuilderPanel::Help);
        let help = button(icons::help(
            20.0,
            if help_selected {
                color::CANVAS
            } else {
                color::TEXT_SOFT
            },
        ))
        .on_press(Message::BuilderPanelToggled(BuilderPanel::Help))
        .padding(spacing::XS)
        .style(if help_selected {
            theme::primary_button
        } else {
            theme::secondary_button
        });
        let help: Element<'_, Message> = tooltip(
            help,
            text("Help and shortcuts")
                .size(type_scale::CAPTION)
                .color(color::TEXT_SOFT),
            tooltip::Position::Bottom,
        )
        .gap(spacing::XS)
        .padding(spacing::XS)
        .style(|_| theme::tooltip_surface(1.0))
        .into();

        let settings = button(icons::settings(20.0, color::FAINT))
            .padding(spacing::XS)
            .style(theme::secondary_button);
        let settings: Element<'_, Message> = tooltip(
            settings,
            text("Settings — coming soon")
                .size(type_scale::CAPTION)
                .color(color::TEXT_SOFT),
            tooltip::Position::Bottom,
        )
        .gap(spacing::XS)
        .padding(spacing::XS)
        .style(|_| theme::tooltip_surface(1.0))
        .into();

        row![space().width(Fill), conditions, help, settings,]
            .spacing(spacing::XS)
            .align_y(Center)
            .into()
    }

    fn builder_toolbar_panel(&self) -> Element<'_, Message> {
        let content: Element<'_, Message> = match self.builder_panel {
            Some(BuilderPanel::Conditions) => {
                let mut choices = column![
                    text("Reaction conditions")
                        .size(type_scale::BODY_LARGE)
                        .color(color::TEXT),
                    text("Use a condition when asking what happens to one reactant.")
                        .size(type_scale::CAPTION)
                        .color(color::MUTED),
                    button(text("No condition").size(type_scale::BODY))
                        .on_press(Message::DynamicContextSelected(None))
                        .padding([spacing::XS, spacing::SM])
                        .width(Fill)
                        .style(if self.dynamic_context.is_none() {
                            theme::primary_button
                        } else {
                            theme::secondary_button
                        }),
                ]
                .spacing(spacing::XS);
                for context in DynamicRequestContext::ALL {
                    choices = choices.push(
                        button(text(context.label()).size(type_scale::BODY))
                            .on_press(Message::DynamicContextSelected(Some(context)))
                            .padding([spacing::XS, spacing::SM])
                            .width(Fill)
                            .style(if self.dynamic_context == Some(context) {
                                theme::primary_button
                            } else {
                                theme::secondary_button
                            }),
                    );
                }
                choices.into()
            }
            Some(BuilderPanel::Help) => {
                let shortcut = |key: &'static str, description: &'static str| {
                    row![
                        container(
                            text(key)
                                .size(type_scale::CAPTION)
                                .font(fonts::SEMIBOLD)
                                .color(color::TEXT),
                        )
                        .width(Length::Fixed(92.0)),
                        text(description)
                            .size(type_scale::CAPTION)
                            .color(color::TEXT_SOFT),
                    ]
                    .spacing(spacing::SM)
                    .align_y(Center)
                };
                column![
                    text("Help").size(type_scale::BODY_LARGE).color(color::TEXT),
                    text(
                        "Click an empty reactant to type a name or formula. Press Enter to use it."
                    )
                    .size(type_scale::CAPTION)
                    .color(color::MUTED),
                    rule::horizontal(1).style(theme::soft_divider),
                    shortcut("⌘1 / ⌘2", "Select a reactant"),
                    shortcut("← / →", "Move between reactants"),
                    shortcut("Click / ⌘Z", "Undo the active reactant"),
                    shortcut("Hold / ⌘⌫", "Clear the active reactant"),
                    shortcut("Spacebar", "Find out when ready"),
                    shortcut("?", "Open this help panel"),
                    shortcut("Esc", "Close input or this panel"),
                ]
                .spacing(spacing::SM)
                .into()
            }
            None => space().height(Length::Shrink).into(),
        };

        container(content)
            .padding(spacing::MD)
            .width(Length::Fixed(340.0))
            .style(|_| theme::tooltip_surface(1.0))
            .into()
    }

    #[allow(clippy::too_many_lines)]
    fn product_summary_view(&self, size: Size) -> Element<'_, Message> {
        let Some(animation) = &self.structural_animation else {
            return Self::structural_unavailable_view("Trusted product frames are unavailable");
        };
        let Some(summary) = product_summary::SummaryData::from_frames(&animation.frames) else {
            return Self::structural_unavailable_view(
                "Validated product membership is unavailable",
            );
        };
        let compact = size.width < 1_080.0;
        let dense = size.height < 820.0 || size.width < 1_280.0;
        let elapsed_ms = animation.summary_elapsed_ms;
        let back = button(text("← Macroscopic view"))
            .on_press(Message::ReturnTo3d)
            .padding([spacing::XS, spacing::SM])
            .style(theme::secondary_button);
        let new_reaction = button(text("Build another reaction  →"))
            .on_press(Message::ScreenSelected(Screen::Builder))
            .padding([spacing::XS, spacing::SM])
            .style(theme::secondary_button);
        let header = row![
            back,
            column![
                row![
                    text("VALIDATED PRODUCT RECORD")
                        .size(type_scale::MICRO)
                        .color(color::ACCENT),
                    container(text("FINAL").size(type_scale::MICRO).color(color::SUCCESS))
                        .style(theme::summary_badge)
                        .padding([spacing::XXS, spacing::XS]),
                ]
                .spacing(spacing::XS)
                .align_y(Center),
                text("What the reaction produced")
                    .size(if compact {
                        type_scale::TITLE
                    } else {
                        type_scale::DISPLAY
                    })
                    .font(fonts::SEMIBOLD)
                    .color(color::TEXT),
            ]
            .spacing(spacing::XXS),
            space().width(Fill),
            if compact {
                text("").size(type_scale::MICRO)
            } else {
                text(animation.equation.as_str())
                    .size(type_scale::CAPTION)
                    .color(color::TEXT_SOFT)
            },
            new_reaction,
        ]
        .spacing(spacing::SM)
        .align_y(Center);

        let three_dimensional = container(
            column![
                row![
                    column![
                        text("PRODUCT SPACE")
                            .size(type_scale::MICRO)
                            .color(color::SELECTION),
                        text("Rotating 3D products")
                            .size(type_scale::BODY_LARGE)
                            .font(fonts::SEMIBOLD)
                            .color(color::TEXT),
                    ]
                    .spacing(spacing::XXS),
                    space().width(Fill),
                    row![
                        text("●").size(type_scale::MICRO).color(color::ACCENT),
                        text("LIVE · 360°")
                            .size(type_scale::MICRO)
                            .color(color::TEXT_SOFT),
                    ]
                    .spacing(spacing::XXS),
                ]
                .align_y(Center),
                canvas(product_summary::Product3dScene::new(
                    summary.clone(),
                    elapsed_ms,
                ))
                .width(Fill)
                .height(if compact {
                    Length::Fixed((size.height * 0.48).clamp(300.0, 480.0))
                } else {
                    Fill
                }),
            ]
            .spacing(spacing::XS),
        )
        .style(theme::summary_visual_panel)
        .padding(spacing::SM)
        .width(Fill)
        .height(if compact { Length::Shrink } else { Fill });

        let properties = product_properties_view(&summary, elapsed_ms, compact, dense);
        let body: Element<'_, Message> = if compact {
            scrollable(column![three_dimensional, properties].spacing(spacing::SM))
                .width(Fill)
                .height(Fill)
                .into()
        } else {
            row![
                container(three_dimensional)
                    .width(FillPortion(1))
                    .height(Fill),
                container(properties).width(FillPortion(1)).height(Fill),
            ]
            .spacing(spacing::SM)
            .height(Fill)
            .into()
        };
        let footer_help = if self.keyboard_navigation_active {
            "Esc / ← macroscopic view · N build another reaction"
        } else {
            "Representative explanatory geometry · validated composition and relationships"
        };

        container(
            column![
                header,
                body,
                row![
                    text(footer_help).size(type_scale::MICRO).color(
                        if self.keyboard_navigation_active {
                            color::ACCENT
                        } else {
                            color::TEXT_SOFT
                        }
                    ),
                    space().width(Fill),
                    text("SOURCE · CURRENT .CHEMS + TRUSTED FRAME + ELEMENT CATALOGUE")
                        .size(type_scale::MICRO)
                        .color(color::ACCENT),
                ]
                .align_y(Center),
            ]
            .spacing(spacing::SM)
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
        let ambient_models =
            reactant_composer::ambient_view(&self.reactant_composer).map(Message::ReactantComposer);
        let element_library =
            periodic_table::view(&self.periodic_table, compact).map(Message::PeriodicTable);
        let library = container(element_library).width(Fill).height(Fill);

        let dynamic_busy = matches!(self.dynamic_build, DynamicBuildState::Running { .. });
        let (first, second) = reactant_composer::reactants(&self.reactant_composer);
        let conditions_enabled = !dynamic_busy && !first.is_empty() && second.is_empty();
        let toolbar = self.builder_toolbar(conditions_enabled);
        let foreground = column![toolbar, composer, library]
            .spacing(spacing::XS)
            .width(Fill)
            .height(Fill);
        let application = container(stack![ambient_models, foreground].width(Fill).height(Fill))
            .style(theme::app_background)
            .padding(if compact { spacing::XS } else { spacing::SM })
            .width(Fill)
            .height(Fill);
        let drag_overlay =
            periodic_table::drag_overlay(&self.periodic_table, size).map(Message::PeriodicTable);
        let toolbar_overlay: Element<'_, Message> = if self.builder_panel.is_some() {
            container(row![space().width(Fill), self.builder_toolbar_panel()].width(Fill))
                .padding(iced::Padding {
                    top: 52.0,
                    right: if compact { spacing::XS } else { spacing::SM },
                    bottom: 0.0,
                    left: if compact { spacing::XS } else { spacing::SM },
                })
                .width(Fill)
                .height(Fill)
                .into()
        } else {
            space().height(Length::Shrink).into()
        };

        stack![
            application,
            drag_overlay,
            toolbar_overlay,
            self.dynamic_overlay(size)
        ]
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

    fn key_pressed(key: iced::keyboard::Key, repeat: bool) -> iced::keyboard::Event {
        iced::keyboard::Event::KeyPressed {
            modified_key: key.clone(),
            key,
            physical_key: iced::keyboard::key::Physical::Unidentified(
                iced::keyboard::key::NativeCode::Unidentified,
            ),
            location: iced::keyboard::Location::Standard,
            modifiers: iced::keyboard::Modifiers::empty(),
            text: None,
            repeat,
        }
    }

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


    /// Catalogue graphs pack all cations of one kind into a single atom
    /// group (both `Na+` of `Na2CO3` share one group), which used to defeat
    /// `ionic_salt`'s one-atom-per-cation-group assumption and return None
    /// for every polyprotic acid + carbonate pair.
    #[test]
    fn polyprotic_acids_neutralize_catalogue_carbonates_and_hydroxides() {
        type PolyproticCase = (&'static str, Vec<u8>, &'static str, Vec<u8>, &'static str);
        let catalogue = chemistry::trusted_catalogue().expect("catalogue");
        let identities = agent::reviewed_species_registry(catalogue).expect("registry");
        let cases: [PolyproticCase; 5] = [
            // Monoprotic control: this already worked before the fix.
            ("HCl", vec![1, 17], "Na\u{2082}CO\u{2083}", vec![11, 11, 6, 8, 8, 8], "NaCl"),
            ("H\u{2082}SO\u{2084}", vec![1, 1, 16, 8, 8, 8, 8], "Na\u{2082}CO\u{2083}", vec![11, 11, 6, 8, 8, 8], "Na2SO4"),
            ("H\u{2082}SO\u{2084}", vec![1, 1, 16, 8, 8, 8, 8], "K\u{2082}CO\u{2083}", vec![19, 19, 6, 8, 8, 8], "K2SO4"),
            ("H\u{2083}PO\u{2084}", vec![1, 1, 1, 15, 8, 8, 8, 8], "Na\u{2082}CO\u{2083}", vec![11, 11, 6, 8, 8, 8], "Na3PO4"),
            ("H\u{2083}PO\u{2084}", vec![1, 1, 1, 15, 8, 8, 8, 8], "NaOH", vec![11, 8, 1], "Na3PO4"),
        ];
        for (acid, acid_atoms, base, base_atoms, salt) in cases {
            let request = ReactionBuildRequest {
                reactants: vec![
                    ReactantInput {
                        display: acid.to_owned(),
                        atomic_numbers: acid_atoms,
                        species_id: None,
                    },
                    ReactantInput {
                        display: base.to_owned(),
                        atomic_numbers: base_atoms,
                        species_id: None,
                    },
                ],
                selected_context: None,
            };
            let claim = agent::solve_reaction_claim(&request, &identities)
                .unwrap_or_else(|| panic!("{acid} + {base} should solve locally"));
            // Exact balancing is the atom-conservation gate: an
            // unconservable product set cannot compile to Static.
            let outcome = compile_claim_outcome(&request, claim, &identities)
                .unwrap_or_else(|error| panic!("{acid} + {base} failed to compile: {error}"));
            let CompiledClaimOutcome::Static(outcome) = outcome else {
                panic!("{acid} + {base} should balance to a static outcome");
            };
            assert!(
                outcome.equation().contains(salt),
                "{acid} + {base} should yield {salt}: {}",
                outcome.equation()
            );
        }
    }

    #[test]
    fn ambient_animation_ticks_do_not_cancel_or_clear_dynamic_builds() {
        // The composer emits AnimationTick every 33ms while ambient models
        // are on screen; treating those as edits silently cancelled every
        // Tier B/C build the moment it started.
        let mut app = App {
            provider: Some(ProviderChoice::Local),
            screen: Screen::Builder,
            ..App::default()
        };
        reactant_composer::replace_reactants(&mut app.reactant_composer, [vec![26], vec![17]]);
        app.update(Message::ReactantComposer(
            reactant_composer::Message::StartReactionRequested,
        ));
        assert!(matches!(
            app.dynamic_build,
            DynamicBuildState::Running { .. }
        ));
        app.update(Message::ReactantComposer(
            reactant_composer::Message::AnimationTick,
        ));
        app.update(Message::ReactantComposer(
            reactant_composer::Message::PromptAnimationTick,
        ));
        assert!(
            matches!(app.dynamic_build, DynamicBuildState::Running { .. }),
            "presentation ticks must not cancel a running build"
        );
        app.dynamic_build = DynamicBuildState::Idle;
        app.dynamic_static = Some(dynamic_lithium_static());
        app.update(Message::ReactantComposer(
            reactant_composer::Message::AnimationTick,
        ));
        assert!(
            app.dynamic_static.is_some(),
            "presentation ticks must not clear a finished result"
        );
        app.update(Message::ReactantComposer(
            reactant_composer::Message::AddElement(8),
        ));
        assert!(
            app.dynamic_static.is_none(),
            "a real draft edit still invalidates the result"
        );
    }

    #[test]
    fn derived_copper_displacement_plays_through_both_timelines() {
        // Fe + CuSO4 crashed mid-animation once (metallic acceptor with an
        // empty shell delta); the full derived pipeline must build and play
        // both timelines to completion.
        let catalogue = chemistry::trusted_catalogue().expect("catalogue");
        let identities = agent::reviewed_species_registry(catalogue).expect("registry");
        let request = ReactionBuildRequest {
            reactants: vec![
                ReactantInput {
                    display: "CuSO\u{2084}".to_owned(),
                    atomic_numbers: vec![29, 16, 8, 8, 8, 8],
                    species_id: None,
                },
                ReactantInput {
                    display: "Fe".to_owned(),
                    atomic_numbers: vec![26],
                    species_id: None,
                },
            ],
            selected_context: None,
        };
        let claim = agent::solve_reaction_claim(&request, &identities).expect("solved");
        let outcome = compile_claim_outcome(&request, claim, &identities).expect("outcome");
        let CompiledClaimOutcome::Static(static_outcome) = outcome else {
            panic!("expected static outcome");
        };
        let mut provider = CodexProvider::new(CodexProviderConfig::from_environment());
        let presentation = enrich_static_outcome(static_outcome, catalogue, &mut provider)
            .expect("presentation enriches");
        let mut app = App {
            provider: Some(ProviderChoice::Local),
            screen: Screen::Builder,
            ..App::default()
        };
        app.dynamic_request = Some(request);
        app.finish_dynamic_presentation(presentation);
        assert_eq!(app.screen, Screen::Structural2d);
        assert!(app.structural_animation.is_some());
        assert!(app.structural_error.is_none());

        let mut guard = 0;
        while let Some(animation) = &app.structural_animation {
            if !animation.playing || guard > 100_000 {
                break;
            }
            guard += 1;
            app.update(Message::StructuralTick);
        }
        assert!((1..100_000).contains(&guard), "2D playback terminates");
        app.update(Message::ContinueTo3d);
        assert_eq!(app.screen, Screen::Structural3d);
        let mut guard = 0;
        while let Some(animation) = &app.structural_animation {
            if !animation.playing || guard > 100_000 {
                break;
            }
            guard += 1;
            app.update(Message::StructuralTick);
        }
        assert!((1..100_000).contains(&guard), "3D playback terminates");
    }

    #[test]
    fn typed_acids_resolve_regardless_of_case() {
        // "the app doesn't recognise acids": lowercase formulas were
        // rejected at the name-entry box before the engine ever ran.
        let hcl = chemistry::atoms_from_name("hcl").expect("hcl resolves");
        let naoh = chemistry::atoms_from_name("naoh").expect("naoh resolves");
        assert!(matches!(
            chemistry::resolve_drafts(&hcl, &naoh),
            chemistry::DraftResolution::Supported(_)
        ));
        // Oxoacids parse in any casing and reach the local solver.
        let h2so4 = chemistry::atoms_from_name("h2so4").expect("h2so4 resolves");
        let catalogue = chemistry::trusted_catalogue().expect("catalogue");
        let identities = agent::reviewed_species_registry(catalogue).expect("registry");
        let request = ReactionBuildRequest {
            reactants: vec![
                ReactantInput {
                    display: "H\u{2082}SO\u{2084}".to_owned(),
                    atomic_numbers: chemistry::standardize_elemental_draft(&h2so4),
                    species_id: None,
                },
                ReactantInput {
                    display: "NaOH".to_owned(),
                    atomic_numbers: chemistry::standardize_elemental_draft(&naoh),
                    species_id: None,
                },
            ],
            selected_context: None,
        };
        let claim = agent::solve_reaction_claim(&request, &identities)
            .expect("local solver derives sulfuric acid neutralization");
        let products = claim
            .products
            .iter()
            .map(|product| product.formula.as_str())
            .collect::<Vec<_>>();
        assert_eq!(products, ["H2O", "Na2SO4"]);
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
    #[allow(clippy::too_many_lines)]
    fn builder_keyboard_shortcuts_cover_selection_edit_run_and_dismissal() {
        use iced::keyboard::{Key, Modifiers, key::Named};

        assert!(matches!(
            builder_shortcut(
                Screen::Builder,
                &Key::Character("2".into()),
                Modifiers::COMMAND,
                false,
                false,
                false,
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
                Modifiers::COMMAND,
                false,
                false,
                false,
            ),
            Some(Message::ReactantComposer(reactant_composer::Message::Undo))
        ));
        assert!(matches!(
            builder_shortcut(
                Screen::Builder,
                &Key::Named(Named::Space),
                Modifiers::empty(),
                false,
                false,
                true,
            ),
            Some(Message::ReactantComposer(
                reactant_composer::Message::StartReactionRequested
            ))
        ));
        assert!(
            builder_shortcut(
                Screen::Builder,
                &Key::Named(Named::Space),
                Modifiers::empty(),
                true,
                false,
                true,
            )
            .is_none(),
            "Space must stay text input while inline editing is active"
        );
        assert!(
            builder_shortcut(
                Screen::Builder,
                &Key::Named(Named::Space),
                Modifiers::empty(),
                false,
                true,
                true,
            )
            .is_none(),
            "Space must not run behind an open toolbar panel"
        );
        assert!(matches!(
            builder_shortcut(
                Screen::Builder,
                &Key::Named(Named::Escape),
                Modifiers::empty(),
                true,
                false,
                false,
            ),
            Some(Message::ReactantComposer(
                reactant_composer::Message::NameEntryCancelled
            ))
        ));
        assert!(matches!(
            builder_shortcut(
                Screen::Builder,
                &Key::Named(Named::Escape),
                Modifiers::empty(),
                false,
                true,
                false,
            ),
            Some(Message::BuilderPanelClosed)
        ));
        assert!(
            builder_shortcut(
                Screen::Structural2d,
                &Key::Named(Named::Escape),
                Modifiers::empty(),
                false,
                false,
                false,
            )
            .is_none()
        );
        assert!(matches!(
            builder_shortcut(
                Screen::Builder,
                &Key::Named(Named::ArrowRight),
                Modifiers::empty(),
                false,
                false,
                false,
            ),
            Some(Message::ReactantComposer(
                reactant_composer::Message::SelectReactant(
                    reactant_composer::ActiveReactant::Second
                )
            ))
        ));
    }

    #[test]
    fn screen_keyboard_shortcuts_are_scoped_and_repeat_safe() {
        use iced::keyboard::{Key, key::Named};

        assert!(matches!(
            screen_keyboard_message(
                Screen::Structural2d,
                key_pressed(Key::Named(Named::Space), false),
                iced::event::Status::Ignored,
            ),
            Some(Message::StructuralPlaybackToggled)
        ));
        assert!(
            screen_keyboard_message(
                Screen::Structural2d,
                key_pressed(Key::Named(Named::Space), true),
                iced::event::Status::Ignored,
            )
            .is_none(),
            "key repeat must not flap playback state"
        );
        assert!(matches!(
            screen_keyboard_message(
                Screen::Structural3d,
                key_pressed(Key::Named(Named::ArrowRight), true),
                iced::event::Status::Ignored,
            ),
            Some(Message::StructuralSkipRequested(1))
        ));
        assert!(matches!(
            screen_keyboard_message(
                Screen::ProductSummary,
                key_pressed(Key::Named(Named::Escape), false),
                iced::event::Status::Ignored,
            ),
            Some(Message::ReturnTo3d)
        ));
        assert!(
            screen_keyboard_message(
                Screen::Structural2d,
                key_pressed(Key::Named(Named::ArrowRight), false),
                iced::event::Status::Captured,
            )
            .is_none(),
            "captured widget input must win over screen shortcuts"
        );
    }

    #[test]
    fn keyboard_navigation_activates_on_use_and_pointer_input_clears_it() {
        use iced::keyboard::{Key, key::Named};

        let mut app = App {
            codex_available: false,
            ..App::default()
        };
        assert!(!app.keyboard_navigation_active);

        app.update(Message::KeyboardEvent {
            event: key_pressed(Key::Named(Named::ArrowDown), false),
            status: iced::event::Status::Ignored,
        });
        assert!(app.keyboard_navigation_active);
        assert_eq!(app.provider, Some(ProviderChoice::ApiKey));

        app.update(Message::PointerPressed);
        assert!(!app.keyboard_navigation_active);
        assert_eq!(app.keyboard_outcome_index, None);
    }

    #[test]
    fn outcome_keyboard_selection_wraps_without_auto_selecting() {
        let mut app = App {
            screen: Screen::OutcomeChoice,
            pending_requests: chemistry::ReactionRequest::ALL[..2].to_vec(),
            ..App::default()
        };
        assert_eq!(app.keyboard_outcome_index, None);

        app.update(Message::OutcomeChoiceMoved(-1));
        assert_eq!(app.keyboard_outcome_index, Some(1));
        app.update(Message::OutcomeChoiceMoved(1));
        assert_eq!(app.keyboard_outcome_index, Some(0));
    }

    #[test]
    fn empty_inline_name_entry_closes_when_focus_moves_elsewhere() {
        let reactant = reactant_composer::ActiveReactant::First;
        let mut app = App {
            screen: Screen::Builder,
            ..App::default()
        };

        app.update(Message::ReactantComposer(
            reactant_composer::Message::BeginNameEntry(reactant),
        ));
        app.update(Message::BuilderInputFocusChecked {
            reactant,
            focused: false,
        });
        assert_eq!(reactant_composer::editing(&app.reactant_composer), None);

        app.update(Message::ReactantComposer(
            reactant_composer::Message::BeginNameEntry(reactant),
        ));
        app.update(Message::ReactantComposer(
            reactant_composer::Message::NameInput("nickel".into()),
        ));
        app.update(Message::BuilderInputFocusChecked {
            reactant,
            focused: false,
        });
        assert_eq!(
            reactant_composer::editing(&app.reactant_composer),
            Some(reactant),
            "non-empty text should not be discarded on blur"
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
            let _view = app.dynamic_result_body();
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
    fn editing_reactants_cancels_claim_work_and_rejects_late_completion() {
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

        app.update(Message::ReactantComposer(
            reactant_composer::Message::SelectReactant(reactant_composer::ActiveReactant::Second),
        ));

        assert!(cancellation.load(Ordering::Relaxed));
        assert!(matches!(app.dynamic_build, DynamicBuildState::Idle));
        assert_eq!(app.next_dynamic_run_id, 11);
        app.update(Message::DynamicClaimFinished {
            run_id: 9,
            result: Box::new(Err("late completion".into())),
        });
        assert!(matches!(app.dynamic_build, DynamicBuildState::Idle));
    }

    #[test]
    fn editing_reactants_invalidates_optional_presentation_and_static_result() {
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

        app.update(Message::ReactantComposer(
            reactant_composer::Message::ClearActive,
        ));

        assert!(cancellation.load(Ordering::Relaxed));
        assert!(app.dynamic_static.is_none());
        assert!(app.validated_frames.is_none());
        assert!(app.dynamic_presentation.is_none());
        assert!(matches!(app.dynamic_build, DynamicBuildState::Idle));
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
        assert!((adaptive_zoom(DESIGN_SIZE, 1.0, 2.0) - 1.0).abs() < f32::EPSILON);
        assert!((adaptive_zoom(Size::new(560.0, 760.0), 1.0, 2.0) - 1.0).abs() < f32::EPSILON);

        // A 32in 4K-class window zooms by its most constrained axis (height:
        // 1490 / 900 = 1.655…), quantized down to an integer total device
        // pixel ratio (1.5 x 2.0 = 3).
        let zoom = adaptive_zoom(Size::new(2_650.0, 1_490.0), 1.0, 2.0);
        assert!((zoom - 1.5).abs() < f32::EPSILON);

        // Zoom never exceeds the cap, however large the window.
        assert!((adaptive_zoom(Size::new(7_680.0, 4_320.0), 1.0, 2.0) - MAX_UI_ZOOM).abs() < 0.001);
    }

    #[test]
    fn adaptive_zoom_quantizes_to_integer_device_pixel_ratios() {
        // 1997x1359 on a 1x monitor wants zoom 1.387; fractional totals put
        // glyphs on fractional device rows, so it settles on 1.0.
        assert!((adaptive_zoom(Size::new(1_997.0, 1_359.0), 1.0, 1.0) - 1.0).abs() < f32::EPSILON);
        // The same window on a 2x monitor can afford half steps (total 2).
        let retina = adaptive_zoom(Size::new(1_997.0, 1_359.0), 1.0, 2.0);
        assert!((retina - 1.0).abs() < f32::EPSILON);
        // A taller window on 2x reaches the next clean total (1.5 x 2 = 3).
        let larger = adaptive_zoom(Size::new(2_400.0, 1_400.0), 1.0, 2.0);
        assert!((larger - 1.5).abs() < f32::EPSILON);
        // An undefined monitor scale falls back to integer zoom steps.
        assert!((adaptive_zoom(Size::new(2_400.0, 1_400.0), 1.0, 0.0) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn adaptive_zoom_is_stable_across_already_zoomed_resize_events() {
        // Resize events report design units (physical over total scale), so a
        // window that settled on a zoom keeps it: 2880x1800 physical reads as
        // 1440x900 under zoom 2.0 and recomputes to exactly 2.0 again.
        let settled = adaptive_zoom(Size::new(2_880.0, 1_800.0), 1.0, 2.0);
        assert!((settled - 2.0).abs() < f32::EPSILON);
        let recomputed = adaptive_zoom(Size::new(1_440.0, 900.0), settled, 2.0);
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
    fn solver_observations_reach_educational_playback() {
        let mut observed_reactions = 0;
        for request in chemistry::ReactionRequest::ALL {
            let run = chemistry::run(request).expect("pinned request validates");
            let has_observations = run
                .frames()
                .frames()
                .iter()
                .any(|frame| !frame.observations().is_empty());
            let plan = compile_educational_plan(run.frames()).expect("educational plan compiles");
            let observation_scenes = plan
                .scenes
                .iter()
                .filter(|scene| scene.kind == EducationalSceneKind::ObservationConnection)
                .collect::<Vec<_>>();
            if !has_observations {
                assert!(
                    observation_scenes.is_empty(),
                    "{request:?} invented an observation scene"
                );
                continue;
            }
            observed_reactions += 1;
            assert!(
                !observation_scenes.is_empty(),
                "{request:?} has observations but no observation scene"
            );
            for scene in &observation_scenes {
                assert!(
                    scene.cues.iter().any(|cue| matches!(
                        cue,
                        chem_presentation::EducationalCue::ShowObservation { .. }
                    )),
                    "{request:?} observation scene lacks a ShowObservation cue"
                );
                assert!(
                    scene.cues.iter().any(|cue| matches!(
                        cue,
                        chem_presentation::EducationalCue::ShowExplanation { label }
                            if label.kind
                                == chem_presentation::ExplanationLabelKind::ObservationExplanation
                                // A leader line exists exactly when there are
                                // atoms to point at (disappearances have none).
                                && label.connector != label.target_atoms.is_empty()
                    )),
                    "{request:?} observation scene lacks a coherent explanation card"
                );
            }
            let summary = plan.scenes.last().expect("plan ends with a summary");
            assert_eq!(summary.kind, EducationalSceneKind::Summary);
            assert!(
                summary.cues.iter().any(|cue| matches!(
                    cue,
                    chem_presentation::EducationalCue::ShowContext { label }
                        if label.kind
                            == chem_presentation::ExplanationLabelKind::ObservationExplanation
                )),
                "{request:?} summary lacks the observation recap chips"
            );
        }
        assert!(
            observed_reactions > 0,
            "no pinned reaction carries observations"
        );
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
    fn keyboard_skip_seeks_macroscopic_playback_and_play_restarts_at_the_end() {
        let mut app = App::default();
        app.open_structural_animation();
        app.screen = Screen::Structural3d;
        app.seek_real_world_timeline(6_000);

        app.update(Message::StructuralSkipRequested(-1));
        let animation = app.structural_animation.as_ref().expect("animation exists");
        assert_eq!(animation.real_world_playhead_ms, 1_000);
        assert!(!animation.playing);

        let duration = animation.real_world_plan.timeline.duration_ms();
        app.seek_real_world_timeline(duration);
        app.update(Message::StructuralPlaybackToggled);
        let animation = app.structural_animation.as_ref().expect("animation exists");
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
    fn product_summary_is_always_reachable_and_animates_from_zero() {
        let mut app = App::default();
        app.update(Message::ContinueToSummary);
        assert_eq!(
            app.screen,
            Screen::ProviderSetup,
            "without an animation the summary stays unreachable"
        );

        app.open_structural_animation();
        app.screen = Screen::Structural3d;
        app.update(Message::ContinueToSummary);
        assert_eq!(app.screen, Screen::ProductSummary);
        assert_eq!(
            app.structural_animation
                .as_ref()
                .expect("animation remains available")
                .summary_elapsed_ms,
            0
        );

        app.update(Message::StructuralTick);
        assert_eq!(
            app.structural_animation
                .as_ref()
                .expect("animation remains available")
                .summary_elapsed_ms,
            33
        );
        app.update(Message::ReturnTo3d);
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
        let mut app = App::default();
        app.open_structural_animation();

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
            let _ = app.structural_2d_view(size);
            let _ = app.structural_3d_view(size);
            let _ = app.product_summary_view(size);
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

    #[test]
    fn periodic_table_activation_refreshes_the_ready_prompt_without_hover() {
        let mut app = App {
            screen: Screen::Builder,
            ..App::default()
        };

        app.update(Message::PeriodicTable(periodic_table::Message::Activated(
            3,
        )));
        app.update(Message::ReactantComposer(
            reactant_composer::Message::SelectReactant(reactant_composer::ActiveReactant::Second),
        ));
        app.update(Message::PeriodicTable(periodic_table::Message::Activated(
            1,
        )));
        assert_eq!(
            reactant_composer::reactants(&app.reactant_composer),
            (&[3_u8][..], &[1_u8][..])
        );
        assert!(
            reactant_composer::submit_available(&app.reactant_composer),
            "one H click canonicalizes to H2, so the prompt must become available immediately"
        );
    }
}
