//! `ChemSpec` application shell and reaction-builder entry (`U-101`, `U-106`–`U-112`).
//!
//! Opens on the Stage 1 builder: the learner's question, composed from two
//! reactant drafts over the full periodic table. Chemistry is supplied only
//! through a reference-data fast path or a staged dynamic claim. Both static
//! and animated capabilities cross the same deterministic validation boundary.

mod animated_clip;
mod blocking;
mod chemistry;
mod composition_catalogue;
mod dynamic_reaction;
mod elements;
mod fonts;
mod gas_fluid;
mod icons;
mod nomenclature;
mod particle_visualization;
mod periodic_table;
mod product_summary;
mod reactant_composer;
mod scene_registry;
mod settings;
mod sketcher;
mod structural_2d;
mod structural_3d;
mod structural_physics;
mod theme;

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Sender},
};

// std::time::Instant panics on wasm32; web-time is std's Instant on native
// and performance.now() on the web.
use web_time::Instant;

use agent::{
    ClaimDisposition, ClaimMode, CodexProgressEvent, CodexProgressStage, CodexProvider,
    CodexProviderConfig, DynamicCachePresentation, DynamicPresentationOutcome, LatencyMilestones,
    MacroscopicProcess as AgentMacroscopicProcess, OutcomeProvenance, OutcomeSpecies,
    OxideAppearanceRequest, ReactantInput, ReactionBuildRequest, RequestIdentityResolution,
    ValidatedOxideAppearance, ValidatedStaticOutcome, enrich_static_outcome,
    load_oxide_appearance_cache, resolve_request_identities_with_catalogue,
    reviewed_species_registry, store_dynamic_cache, store_oxide_appearance_cache,
};
#[cfg(test)]
use agent::{CompiledClaimOutcome, ProviderClaim, ReactionClaim};
use chem_domain::{ContentDigest, RepresentationKind};
use chem_presentation::{
    EducationalPlan, EducationalSceneKind, EffectProfile, ExplosiveMetalWaterVariant,
    MacroscopicColourAuthority, MacroscopicMaterial, MacroscopicMaterialRole, MacroscopicProcess,
    MacroscopicReaction, MacroscopicStage, PresentationProfile, ScenePlan, SurfaceOxideColour,
    TimelinePosition, VisualColour, compile_educational_plan, compile_phase_driven_profile,
    compile_real_world_plan, complete_generic_visual_profile,
};
use iced::widget::{
    button, canvas, column, container, mouse_area, responsive, row, rule, scrollable, slider,
    space, stack, text, tooltip,
};
use iced::{Center, Element, Fill, FillPortion, Length, Padding, Size, Subscription, Task, Theme};

#[cfg(test)]
use dynamic_reaction::ClaimStageResult as DynamicClaimStageResult;
use dynamic_reaction::{
    BuildFailure as DynamicBuildFailure, BuildStage as DynamicBuildStage,
    BuildState as DynamicBuildState, IdentityChoice as DynamicIdentityChoice,
    ModalKind as DynamicModalKind, RequestContext as DynamicRequestContext,
};
use settings::{AppMode, AppSettings, ChemicalLabels, LoadOutcome};
use theme::{breakpoint, color, space as spacing, type_scale};

fn elapsed_millis(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn reviewed_outcome_choice(
    request: chemistry::ReactionRequest,
    labels: ChemicalLabels,
    compact: bool,
    keyboard_selected: bool,
) -> Element<'static, Message> {
    let label = match labels {
        ChemicalLabels::Formulae => nomenclature::display_equation(&request.equation()),
        ChemicalLabels::Names => request.name(),
    };
    let labels = column![text(label).size(type_scale::BODY_LARGE).color(color::TEXT),]
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
    labels: ChemicalLabels,
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
            text(nomenclature::display_species(
                labels,
                Some(term.display_name()),
                term.formula_text(),
            ))
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
    labels: ChemicalLabels,
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
        text("Properties are compiled locally from the validated final frame and bundled element metadata.")
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
                    text(product.primary_label(labels))
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
                .width(Fill),
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
                text("VALIDATION BOUNDARY")
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
        EffectProfile::ReactionActivity => "Reaction motion",
        EffectProfile::BubbleEmitter => "Interface bubbles",
        EffectProfile::GasRelease => "Gas release",
        EffectProfile::VapourRelease => "Hot vapour",
        EffectProfile::SurfaceDisturbance => "Surface motion",
        EffectProfile::LiquidMixing => "Liquid mixing",
        EffectProfile::ObjectShrinkage => "Reactant consumption",
        EffectProfile::SurfaceOxidation => "Oxide layer formation",
        EffectProfile::SolidFormation => "Solid formation",
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
const BUILDER_TOOLBAR_ICON_SIZE: f32 = 20.0;

fn chromeless_page_padding(normal: f32, macos_top_extra: f32) -> Padding {
    if cfg!(target_os = "macos") {
        Padding {
            top: macos_top_extra + normal,
            right: normal,
            bottom: normal,
            left: normal,
        }
    } else {
        Padding::from(normal)
    }
}

const fn builder_toolbar_panel_top(page_top: f32) -> f32 {
    page_top + BUILDER_TOOLBAR_ICON_SIZE + 2.0 * spacing::XS + spacing::XXS
}

fn window_settings() -> iced::window::Settings {
    let settings = iced::window::Settings {
        size: DESIGN_SIZE,
        min_size: Some(Size::new(560.0, 760.0)),
        position: iced::window::Position::Centered,
        ..iced::window::Settings::default()
    };

    #[cfg(target_os = "macos")]
    let settings = iced::window::Settings {
        platform_specific: iced::window::settings::PlatformSpecific {
            title_hidden: true,
            titlebar_transparent: true,
            fullsize_content_view: true,
        },
        ..settings
    };

    settings
}

fn main() -> iced::Result {
    #[cfg(target_arch = "wasm32")]
    console_error_panic_hook::set_once();
    let arguments = std::env::args().collect::<Vec<_>>();
    if arguments.get(1).map(String::as_str) == Some("react") {
        std::process::exit(react_command(&arguments[2..]));
    }
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
        .window(window_settings())
        .run()
}

/// Headless reaction verification for agents and CI: resolve two reactants
/// (names or formulae) through the exact path the GUI uses and print the
/// outcome as JSON, without booting the window. Pass `--verbose`/`-v` to also
/// emit the full renderer-independent frame artifact (the actual animation
/// contents) plus its stable digest. Exit 0 = a reaction ran, 1 = resolved but
/// no single reaction, 2 = bad input or catalogue error.
fn react_command(arguments: &[String]) -> i32 {
    use chemistry::DraftResolution;
    use serde_json::json;

    let verbose = arguments
        .iter()
        .any(|argument| argument == "--verbose" || argument == "-v");
    let reactants = arguments
        .iter()
        .filter(|argument| !argument.starts_with('-'))
        .collect::<Vec<_>>();
    let [first, second] = reactants.as_slice() else {
        eprintln!(
            "usage: chemspec-app react [--verbose] <reactant> <reactant>   (names or formulae)"
        );
        return 2;
    };
    let parse = |input: &str| {
        chemistry::atoms_from_name(input).ok_or_else(|| format!("unrecognized reactant: {input}"))
    };
    let (first_atoms, second_atoms) = match (parse(first), parse(second)) {
        (Ok(a), Ok(b)) => (a, b),
        (Err(error), _) | (_, Err(error)) => {
            eprintln!("{error}");
            return 2;
        }
    };

    let resolution = chemistry::resolve_drafts(&first_atoms, &second_atoms);
    let (status, mut detail, code) = match &resolution {
        DraftResolution::Supported(request) => match chemistry::run(*request) {
            Ok(run) => {
                let mut detail = json!({
                    "id": request.id(),
                    "equation": request.equation(),
                    "products": crate::nomenclature::product_names(run.frames()),
                    "frames": run.frames().frames().len(),
                });
                if verbose {
                    let frames = run.frames();
                    detail["digest"] = frames
                        .digest()
                        .map_or(json!(null), |digest| json!(digest.to_hex()));
                    detail["animation"] =
                        serde_json::to_value(frames).unwrap_or(serde_json::Value::Null);
                }
                ("reaction", detail, 0)
            }
            Err(error) => ("system_error", json!({ "error": error }), 2),
        },
        DraftResolution::Multiple(requests) => (
            "multiple",
            json!({ "candidates": requests.iter().map(|r| r.id()).collect::<Vec<_>>() }),
            1,
        ),
        DraftResolution::Screened(assessment) => {
            ("screened", json!({ "subject": assessment.subject }), 1)
        }
        DraftResolution::ExplicitlyUnsupported(_) => ("unsupported", json!({}), 1),
        DraftResolution::Uncatalogued => ("uncatalogued", json!({}), 1),
        DraftResolution::Unrecognized => ("unrecognized", json!({}), 1),
        DraftResolution::SystemError(error) => ("system_error", json!({ "error": error }), 2),
    };
    if let (Some(message), Some(object)) = (resolution.inline_message(), detail.as_object_mut()) {
        object.insert("message".to_owned(), json!(message));
    }

    let output = json!({
        "reactants": [first, second],
        "status": status,
        "detail": detail,
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&output).unwrap_or_else(|_| output.to_string())
    );
    code
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
    // No subprocesses (and no std::time) in the browser: Codex is never
    // available on wasm.
    if cfg!(target_arch = "wasm32") {
        return false;
    }
    CodexProvider::new(CodexProviderConfig::from_environment())
        .preflight()
        .is_ok_and(|preflight| preflight.authenticated)
}

fn dynamic_presentation_profile(
    frames: &chem_kernel::SimulationFrames,
    outcome: &ValidatedStaticOutcome,
    surface_oxide_colour: Option<SurfaceOxideColour>,
) -> Result<PresentationProfile, String> {
    let mut reaction = dynamic_macroscopic_reaction(outcome)?;
    reaction.surface_oxide_colour = surface_oxide_colour;
    let profile =
        compile_phase_driven_profile(frames, &reaction).map_err(|error| error.to_string())?;
    complete_generic_visual_profile(frames, profile).map_err(|error| error.to_string())
}

fn dynamic_macroscopic_reaction(
    outcome: &ValidatedStaticOutcome,
) -> Result<MacroscopicReaction, String> {
    let catalogue = chemistry::reference_catalogue().ok();
    let materials = outcome
        .reactants()
        .iter()
        .filter_map(|species| {
            dynamic_macroscopic_material(
                outcome,
                catalogue,
                species,
                MacroscopicMaterialRole::Reactant,
            )
        })
        .chain(outcome.products().iter().filter_map(|species| {
            dynamic_macroscopic_material(
                outcome,
                catalogue,
                species,
                MacroscopicMaterialRole::Product,
            )
        }))
        .collect::<Vec<_>>();
    if materials.len() != outcome.reactants().len() + outcome.products().len() {
        return Err("validated dynamic species lack renderer-readable structures".to_owned());
    }
    let process = outcome.macroscopic_process().map(|process| match process {
        AgentMacroscopicProcess::AqueousPrecipitation => MacroscopicProcess::AqueousPrecipitation,
        AgentMacroscopicProcess::GasEvolutionLiquidLiquid => {
            MacroscopicProcess::GasEvolutionLiquidLiquid
        }
        AgentMacroscopicProcess::GasEvolutionSolidLiquid => {
            MacroscopicProcess::GasEvolutionSolidLiquid
        }
        AgentMacroscopicProcess::MetalDisplacement => MacroscopicProcess::MetalDisplacement,
        AgentMacroscopicProcess::ExplosiveMetalWater(variant) => {
            MacroscopicProcess::ExplosiveMetalWater(match variant {
                chem_catalogue::ExplosiveWaterContactVariantRecord::Rubidium => {
                    ExplosiveMetalWaterVariant::Rubidium
                }
                chem_catalogue::ExplosiveWaterContactVariantRecord::Caesium => {
                    ExplosiveMetalWaterVariant::Caesium
                }
                chem_catalogue::ExplosiveWaterContactVariantRecord::Francium => {
                    ExplosiveMetalWaterVariant::Francium
                }
            })
        }
        AgentMacroscopicProcess::SolidSolidSynthesis => MacroscopicProcess::SolidSolidSynthesis,
        AgentMacroscopicProcess::CompleteCombustion => MacroscopicProcess::CompleteCombustion,
        AgentMacroscopicProcess::IncompleteCombustion => MacroscopicProcess::IncompleteCombustion,
        AgentMacroscopicProcess::SolventEvaporationCrystallization => {
            MacroscopicProcess::SolventEvaporationCrystallization
        }
        AgentMacroscopicProcess::SurfaceOxidation => MacroscopicProcess::SurfaceOxidation,
    });
    Ok(MacroscopicReaction {
        profile_id: format!(
            "presentation.dynamic.{}",
            outcome.declaration().digest().to_hex()
        ),
        equation: outcome.equation().to_owned(),
        materials,
        intensity: chemistry::macroscopic_process_intensity(process),
        process,
        fuel_carbon_count: outcome.combustion_fuel_carbon_count(),
        surface_oxide_colour: None,
    })
}

fn dynamic_macroscopic_material(
    outcome: &ValidatedStaticOutcome,
    catalogue: Option<&chem_catalogue::ReferenceCatalogue>,
    species: &OutcomeSpecies,
    role: MacroscopicMaterialRole,
) -> Option<MacroscopicMaterial> {
    let OutcomeSpecies::Resolved(resolved) = species else {
        return None;
    };
    let structure = resolved.structure.as_ref()?;
    let reviewed_material =
        catalogue.and_then(|catalogue| catalogue.macroscopic_material(structure.id(), None));
    let colour = reviewed_material
        .and_then(|material| material.colour)
        .or_else(|| {
            outcome
                .macroscopic_colour(species)
                .map(agent::MacroscopicColour::srgb)
        })
        .map(|[red, green, blue]| VisualColour { red, green, blue });
    Some(MacroscopicMaterial {
        binding: species.id().to_string(),
        semantic_identity: species.display_name().to_owned(),
        structure_id: structure.id().to_string(),
        formula: resolved.formula_text.clone(),
        role,
        phase: reviewed_material.map_or_else(
            || outcome.macroscopic_phase(species),
            |material| material.phase,
        ),
        representation: species.representation()?,
        colour,
        explosive_water_contact: reviewed_material
            .and_then(|material| material.water_contact)
            .map(chemistry::explosive_metal_water_variant),
    })
}

fn launch_state() -> App {
    let mut app = App {
        dump_frame_path: std::env::args()
            .find_map(|argument| argument.strip_prefix("--dump-frame=").map(Into::into)),
        ..App::default()
    };
    app.apply_settings_load(settings::load());
    let smoke_mode = std::env::args().find_map(|argument| SmokeMode::from_argument(&argument));
    let smoke_from_start = std::env::args().any(|argument| argument == "--smoke-from-start");
    let smoke_request =
        std::env::args().find_map(|argument| smoke_request_from_argument(&argument));
    if let Some(smoke_mode) = smoke_mode {
        app.smoke_mode = Some(smoke_mode);
        if smoke_mode == SmokeMode::Builder {
            app.enter_screen(Screen::Builder);
            return app;
        }
        if let Some(request) = smoke_request {
            match request {
                Ok(request) => app.select_request(request),
                Err(error) => {
                    app.enter_screen(Screen::Builder);
                    app.structural_error = Some(error);
                    return app;
                }
            }
        }
        app.open_structural_animation();
        let three_dimensional = smoke_mode == SmokeMode::Structural3d;
        // Optional exact playhead for frame-dump verification of a specific
        // moment (e.g. mid-pour), instead of the default two-thirds seek.
        let smoke_playhead_ms = std::env::args().find_map(|argument| {
            argument
                .strip_prefix("--smoke-playhead-ms=")
                .and_then(|value| value.parse::<u64>().ok())
        });
        if let Some(animation) = &mut app.structural_animation {
            animation.frame_index = 1.min(animation.frames.frames().len().saturating_sub(1));
            if three_dimensional && !smoke_from_start {
                let plan = &animation.real_world_plan;
                animation.real_world_playhead_ms = smoke_playhead_ms
                    .unwrap_or_else(|| plan.timeline.duration_ms().saturating_mul(2) / 3)
                    .min(plan.timeline.duration_ms());
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
        }
        app.enter_screen(if three_dimensional {
            Screen::Structural3d
        } else {
            Screen::Structural2d
        });
        // Smoke launch has no preceding key event to consume.
        app.structural_shortcut_state = StructuralShortcutState::Ready;
    }
    // The web demo is Local Mode only (no subprocesses in a browser), so the
    // provider screen is meaningless there: boot into the builder with the
    // showcase reactants pre-filled, ready for "Press space to find out".
    #[cfg(target_arch = "wasm32")]
    {
        reactant_composer::replace_reactants(&mut app.reactant_composer, [vec![3], vec![1, 1, 8]]);
        // enter_screen syncs the "Press space to find out" prompt from the
        // composer state, so the reactants must be in place first.
        app.enter_screen(Screen::Builder);
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

impl Screen {
    const fn smoke_title(self) -> &'static str {
        match self {
            Self::ProviderSetup => "Provider Setup",
            Self::Builder => "Builder",
            Self::OutcomeChoice => "Outcome Choice",
            Self::Structural2d => "Structural 2D",
            Self::Structural3d => "Structural 3D",
            Self::ProductSummary => "Product Summary",
        }
    }
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlaybackSpeed {
    Half,
    Normal,
    OneAndHalf,
}

const STRUCTURAL_SHORTCUT_SETTLE_MS: u32 = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StructuralShortcutState {
    Inactive,
    Settling(u32),
    Ready,
}

impl StructuralShortcutState {
    fn tick(&mut self, elapsed_ms: u32) {
        let Self::Settling(elapsed) = self else {
            return;
        };
        *elapsed = elapsed.saturating_add(elapsed_ms);
        if *elapsed >= STRUCTURAL_SHORTCUT_SETTLE_MS {
            *self = Self::Ready;
        }
    }
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
    /// Web demo only: open the ChemSpec repository in a new tab.
    #[cfg(target_arch = "wasm32")]
    DemoRepoLinkOpened,
    WindowResized(Size),
    DumpFrame,
    FrameCaptured(std::path::PathBuf, iced::window::Screenshot),
    Dynamic(dynamic_reaction::Message),
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
    ReturnToBuilder,
    StartNewReaction,
    ProviderSelected(AppMode),
    ProviderContinue,
    SettingsOpened,
    SettingsClosed,
    SettingsAppModeSelected(AppMode),
    SettingsChemicalLabelsSelected(ChemicalLabels),
    SettingsSaveRequested,
    SettingsSaveFinished {
        save_id: u64,
        destination: SettingsSaveDestination,
        settings: AppSettings,
        result: Result<(), String>,
    },
    PeriodicTable(periodic_table::Message),
    ReactantComposer(reactant_composer::Message),
    Sketcher(sketcher::Message),
    BuilderPanelToggled(BuilderPanel),
    BuilderPanelClosed,
    OxideAppearanceFinished {
        run_id: u64,
        request_binding: ContentDigest,
        result: Box<Result<ValidatedOxideAppearance, String>>,
    },
    RetryOxideAppearance,
    OutcomeSelected(chemistry::ReactionRequest),
    StructuralPlaybackShortcut,
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
    Sketch,
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsSaveDestination {
    FirstLaunch,
    Dialog,
}

#[derive(Debug, Clone)]
struct SettingsDialog {
    draft: AppSettings,
    error: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum SettingsSaveState {
    #[default]
    Idle,
    Saving {
        save_id: u64,
        destination: SettingsSaveDestination,
    },
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

fn dynamic_modal_keyboard_message(event: iced::keyboard::Event) -> Option<Message> {
    use iced::keyboard::{Key, key::Named};

    let iced::keyboard::Event::KeyPressed { key, .. } = event else {
        return None;
    };
    (key == Key::Named(Named::Escape)).then_some(Message::Dynamic(
        dynamic_reaction::Message::OverlayDismissed,
    ))
}

fn settings_modal_keyboard_message(
    event: iced::keyboard::Event,
    status: iced::event::Status,
) -> Option<Message> {
    use iced::keyboard::{Key, key::Named};

    if status == iced::event::Status::Captured {
        return None;
    }
    let iced::keyboard::Event::KeyPressed { key, repeat, .. } = event else {
        return None;
    };
    (key == Key::Named(Named::Escape) && !repeat).then_some(Message::SettingsClosed)
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
    provider: Option<AppMode>,
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
        [Some(AppMode::Local), Some(AppMode::CodexBinary)]
    } else {
        [Some(AppMode::Local), None]
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
        Key::Character("1") if !repeat => Some(Message::ProviderSelected(AppMode::Local)),
        Key::Character("2") if !repeat && codex_available => {
            Some(Message::ProviderSelected(AppMode::CodexBinary))
        }
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
            Key::Named(Named::Escape) if !repeat => Some(Message::ReturnToBuilder),
            _ => None,
        },
        Screen::Structural2d | Screen::Structural3d => match key.as_ref() {
            Key::Named(Named::Space) | Key::Character(" ") if !repeat => {
                Some(Message::StructuralPlaybackShortcut)
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
                Some(Message::ReturnToBuilder)
            }
            Key::Named(Named::Escape) if !repeat => Some(Message::ReturnTo2d),
            _ => None,
        },
        Screen::ProductSummary => match key.as_ref() {
            Key::Named(Named::Escape | Named::ArrowLeft) if !repeat => Some(Message::ReturnTo3d),
            Key::Character(value) if !repeat && value.eq_ignore_ascii_case("n") => {
                Some(Message::StartNewReaction)
            }
            _ => None,
        },
        Screen::ProviderSetup | Screen::Builder => None,
    }
}

type RenderableFrames = chem_kernel::SimulationFrames;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BuilderOverlayKind {
    Dynamic(DynamicModalKind),
    Toolbar,
    Drag,
    None,
}

#[derive(Debug)]
struct StructuralAnimation {
    frames: RenderableFrames,
    declaration: chem_domain::ReactionDeclaration,
    educational_plan: EducationalPlan,
    real_world_plan: ScenePlan,
    product_preview: Option<composition_catalogue::ReferenceCompositionPreview>,
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
    /// Wall-clock instant of the previous 3D tick: the playhead advances by
    /// measured time so a slow frame drops visuals, not reaction pace.
    last_structural_tick: Option<Instant>,
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
    settings: AppSettings,
    settings_dialog: Option<SettingsDialog>,
    settings_save_state: SettingsSaveState,
    settings_load_error: Option<String>,
    next_settings_save_id: u64,
    provider: Option<AppMode>,
    periodic_table: periodic_table::State,
    reactant_composer: reactant_composer::State,
    sketcher: sketcher::State,
    pending_requests: Vec<chemistry::ReactionRequest>,
    oxygen_assessment: Option<chemistry::OxygenAssessment>,
    active_request: chemistry::ReactionRequest,
    validated_frames: Option<RenderableFrames>,
    validated_macroscopic: Option<chem_presentation::MacroscopicReaction>,
    validated_declaration: Option<chem_domain::ReactionDeclaration>,
    dynamic: dynamic_reaction::State,
    builder_panel: Option<BuilderPanel>,
    oxide_appearance_request: Option<OxideAppearanceRequest>,
    oxide_appearance: Option<ValidatedOxideAppearance>,
    oxide_appearance_error: Option<String>,
    oxide_appearance_cancellation: Option<Arc<AtomicBool>>,
    active_oxide_appearance_run: Option<u64>,
    next_oxide_appearance_run_id: u64,
    structural_animation: Option<StructuralAnimation>,
    structural_error: Option<String>,
    /// A structural destination settles through animation time before keyboard
    /// playback becomes active. Pointer playback can explicitly arm it sooner.
    structural_shortcut_state: StructuralShortcutState,
    /// Interface zoom applied on top of the system scale factor.
    ui_zoom: f32,
    /// Debug harness: dump one frame to this path, then keep running.
    dump_frame_path: Option<std::path::PathBuf>,
}

impl Default for App {
    fn default() -> Self {
        let codex_available = codex_available();
        let active_request = chemistry::ReactionRequest::DEFAULT;
        let validated_run = chemistry::run(active_request).ok();
        Self {
            screen: Screen::ProviderSetup,
            keyboard_navigation_active: false,
            keyboard_outcome_index: None,
            smoke_mode: None,
            codex_available,
            settings: AppSettings::default(),
            settings_dialog: None,
            settings_save_state: SettingsSaveState::Idle,
            settings_load_error: None,
            next_settings_save_id: 1,
            provider: Some(AppMode::Local),
            periodic_table: periodic_table::State::default(),
            reactant_composer: reactant_composer::State::default(),
            sketcher: sketcher::State::default(),
            pending_requests: Vec::new(),
            oxygen_assessment: None,
            active_request,
            validated_frames: validated_run.as_ref().map(|run| run.frames().clone()),
            validated_macroscopic: validated_run
                .as_ref()
                .and_then(|run| run.macroscopic().cloned()),
            validated_declaration: validated_run.as_ref().map(|run| run.declaration().clone()),
            dynamic: dynamic_reaction::State::default(),
            builder_panel: None,
            oxide_appearance_request: None,
            oxide_appearance: None,
            oxide_appearance_error: None,
            oxide_appearance_cancellation: None,
            active_oxide_appearance_run: None,
            next_oxide_appearance_run_id: 1,
            structural_animation: None,
            structural_error: None,
            structural_shortcut_state: StructuralShortcutState::Inactive,
            ui_zoom: 1.0,
            dump_frame_path: None,
        }
    }
}

impl App {
    fn mode_available(&self, mode: AppMode) -> bool {
        match mode {
            AppMode::Local => true,
            AppMode::CodexBinary => self.codex_available,
            AppMode::Api => false,
        }
    }

    fn apply_settings_load(&mut self, outcome: LoadOutcome) {
        match outcome {
            LoadOutcome::Loaded(settings) => {
                self.settings = settings;
                self.provider = Some(settings.app_mode);
                if self.mode_available(settings.app_mode) {
                    self.enter_screen(Screen::Builder);
                } else {
                    self.screen = Screen::ProviderSetup;
                    self.settings_load_error = Some(match settings.app_mode {
                        AppMode::CodexBinary => {
                            "Codex is not available. Choose another app mode to continue."
                                .to_owned()
                        }
                        AppMode::Api => {
                            "API mode is not available in this version of ChemSpec.".to_owned()
                        }
                        AppMode::Local => unreachable!("local mode is always available"),
                    });
                }
            }
            LoadOutcome::Missing => {
                let app_mode = if self.codex_available {
                    AppMode::CodexBinary
                } else {
                    AppMode::Local
                };
                self.settings = AppSettings {
                    app_mode,
                    chemical_labels: ChemicalLabels::Formulae,
                };
                self.provider = Some(app_mode);
                self.screen = Screen::ProviderSetup;
            }
            LoadOutcome::Invalid(error) => {
                let app_mode = if self.codex_available {
                    AppMode::CodexBinary
                } else {
                    AppMode::Local
                };
                self.settings = AppSettings {
                    app_mode,
                    chemical_labels: ChemicalLabels::Formulae,
                };
                self.provider = Some(app_mode);
                self.screen = Screen::ProviderSetup;
                self.settings_load_error = Some(format!(
                    "Saved settings could not be loaded. Choose an app mode to repair them. {error}"
                ));
            }
        }
    }

    fn start_settings_save(
        &mut self,
        destination: SettingsSaveDestination,
        settings: AppSettings,
    ) -> Task<Message> {
        if !matches!(self.settings_save_state, SettingsSaveState::Idle) {
            return Task::none();
        }
        let save_id = self.next_settings_save_id;
        self.next_settings_save_id = self.next_settings_save_id.saturating_add(1);
        self.settings_save_state = SettingsSaveState::Saving {
            save_id,
            destination,
        };
        if let Some(dialog) = &mut self.settings_dialog {
            dialog.error = None;
        }
        Task::perform(async move { settings::save(settings) }, move |result| {
            Message::SettingsSaveFinished {
                save_id,
                destination,
                settings,
                result,
            }
        })
    }

    fn finish_settings_save(
        &mut self,
        save_id: u64,
        destination: SettingsSaveDestination,
        settings: AppSettings,
        result: Result<(), String>,
    ) {
        if self.settings_save_state
            != (SettingsSaveState::Saving {
                save_id,
                destination,
            })
        {
            return;
        }
        self.settings_save_state = SettingsSaveState::Idle;
        match result {
            Ok(()) => {
                let mode_changed = self.provider != Some(settings.app_mode);
                if mode_changed {
                    self.cancel_dynamic_work();
                }
                self.settings = settings;
                self.provider = Some(settings.app_mode);
                self.settings_load_error = None;
                self.settings_dialog = None;
                if destination == SettingsSaveDestination::FirstLaunch {
                    self.enter_screen(Screen::Builder);
                } else {
                    self.sync_builder_submit_prompt();
                }
            }
            Err(error) => match destination {
                SettingsSaveDestination::FirstLaunch => self.settings_load_error = Some(error),
                SettingsSaveDestination::Dialog => {
                    if let Some(dialog) = &mut self.settings_dialog {
                        dialog.error = Some(error);
                    }
                }
            },
        }
    }

    fn settings_saving(&self, destination: SettingsSaveDestination) -> bool {
        matches!(
            self.settings_save_state,
            SettingsSaveState::Saving {
                destination: active,
                ..
            } if active == destination
        )
    }

    /// Local Mode is a state for the whole app: no model integration, no
    /// model-facing copy, and anything only a model could do is unsupported.
    fn local_mode(&self) -> bool {
        self.provider == Some(AppMode::Local)
    }

    /// The only runtime boundary for changing product screens. It reconciles
    /// screen-owned transient state before the next view or subscription can
    /// observe the destination.
    fn enter_screen(&mut self, screen: Screen) {
        let resuming_builder = self.screen != Screen::Builder && screen == Screen::Builder;
        self.screen = screen;
        self.keyboard_outcome_index = None;
        self.builder_panel = None;
        if resuming_builder {
            reactant_composer::restart_prompt_reveal(&mut self.reactant_composer);
        }
        self.structural_shortcut_state =
            if matches!(screen, Screen::Structural2d | Screen::Structural3d) {
                StructuralShortcutState::Settling(0)
            } else {
                StructuralShortcutState::Inactive
            };
        self.sync_builder_submit_prompt();
    }

    /// Starts a new builder session. Navigation back to the builder deliberately
    /// does not call this: Return preserves the reaction; Build another clears
    /// the completed question, its result surfaces, and transient tool state.
    fn start_new_reaction(&mut self) {
        self.cancel_dynamic_work();
        reactant_composer::clear_reaction(&mut self.reactant_composer);
        self.periodic_table = periodic_table::State::default();
        self.sketcher = sketcher::State::default();
        self.pending_requests.clear();
        self.oxygen_assessment = None;
        self.validated_frames = None;
        self.validated_macroscopic = None;
        self.validated_declaration = None;
        self.dynamic.context = None;
        self.dynamic.details_open = false;
        self.dynamic.overlay_dismissed = false;
        self.structural_animation = None;
        self.structural_error = None;
        self.structural_shortcut_state = StructuralShortcutState::Inactive;
        self.keyboard_navigation_active = false;
        self.enter_screen(Screen::Builder);
    }

    fn pending_dynamic_modal_kind(&self) -> Option<DynamicModalKind> {
        if self.dynamic.identity_choice.is_some() {
            Some(DynamicModalKind::IdentityChoice)
        } else if self.dynamic.static_outcome.is_some() {
            Some(DynamicModalKind::StaticResult)
        } else if matches!(self.dynamic.build, DynamicBuildState::Running { .. }) {
            Some(DynamicModalKind::Running)
        } else if matches!(self.dynamic.build, DynamicBuildState::Failed(_)) {
            Some(DynamicModalKind::Failed)
        } else if self.dynamic.claim.is_some() {
            Some(DynamicModalKind::Verdict)
        } else {
            None
        }
    }

    fn dynamic_modal_kind(&self) -> Option<DynamicModalKind> {
        if self.dynamic.overlay_dismissed {
            None
        } else {
            self.pending_dynamic_modal_kind()
        }
    }

    fn open_dynamic_overlay(&mut self) {
        self.dynamic.overlay_dismissed = false;
        self.builder_panel = None;
        reactant_composer::set_submit_available(&mut self.reactant_composer, false);
    }

    fn builder_overlay_kind(&self) -> BuilderOverlayKind {
        if let Some(kind) = self.dynamic_modal_kind() {
            BuilderOverlayKind::Dynamic(kind)
        } else if self.builder_panel.is_some() {
            BuilderOverlayKind::Toolbar
        } else if periodic_table::dragging_atomic_number(&self.periodic_table).is_some() {
            BuilderOverlayKind::Drag
        } else {
            BuilderOverlayKind::None
        }
    }

    fn title(&self) -> String {
        let Some(_) = self.smoke_mode else {
            return "ChemSpec".to_owned();
        };
        let base = format!("ChemSpec Agent Smoke — {}", self.screen.smoke_title());
        self.builder_accessibility_summary()
            .map_or(base.clone(), |summary| format!("{base} — {summary}"))
    }

    fn builder_accessibility_summary(&self) -> Option<String> {
        if self.screen != Screen::Builder {
            return None;
        }
        let (first, second) = reactant_composer::reactants(&self.reactant_composer);
        if first.is_empty() && second.is_empty() && self.dynamic.static_outcome.is_none() {
            return None;
        }
        let formulae =
            reactant_composer::draft_labels(&self.reactant_composer, self.settings.chemical_labels);
        let first = if first.is_empty() {
            "empty".to_owned()
        } else {
            formulae[0].clone()
        };
        let second = if second.is_empty() {
            "empty".to_owned()
        } else {
            formulae[1].clone()
        };
        let reactants = format!("Reactants {first} + {second}");
        let static_summary = || {
            let outcome = self
                .dynamic
                .static_outcome
                .as_ref()
                .expect("a static-result modal retains its outcome");
            let capability = match &self.dynamic.presentation {
                Some(
                    DynamicPresentationOutcome::ReviewedFamily(_)
                    | DynamicPresentationOutcome::Escalated(_),
                ) => "animation ready",
                Some(DynamicPresentationOutcome::Static { .. }) => "static result only",
                None => "static result ready",
            };
            format!(
                "{}; {capability}",
                nomenclature::display_declaration(
                    outcome.declaration(),
                    self.settings.chemical_labels,
                )
            )
        };
        let running_summary = || match &self.dynamic.build {
            DynamicBuildState::Running { stage, .. } => match stage {
                DynamicBuildStage::Claim => "building factual outcome",
                DynamicBuildStage::Presentation => "building presentation",
            },
            DynamicBuildState::Idle | DynamicBuildState::Failed(_) => "working",
        };
        let state = match self.dynamic_modal_kind() {
            Some(DynamicModalKind::IdentityChoice) => "identity choice modal open".to_owned(),
            Some(DynamicModalKind::StaticResult) => {
                format!("result modal open: {}", static_summary())
            }
            Some(DynamicModalKind::Running) => {
                format!("progress modal open: {}", running_summary())
            }
            Some(DynamicModalKind::Failed) => "failure modal open".to_owned(),
            Some(DynamicModalKind::Verdict) => {
                let verdict = self.dynamic.claim.as_ref().map_or("outcome", |claim| {
                    match claim.disposition {
                        ClaimDisposition::NoReaction => "no reaction",
                        ClaimDisposition::Ambiguous => "ambiguous outcome",
                        ClaimDisposition::Unsupported => "unsupported outcome",
                        ClaimDisposition::Reaction => "reaction outcome",
                    }
                });
                format!("outcome modal open: {verdict}")
            }
            None if self.dynamic.static_outcome.is_some() => static_summary(),
            None => match &self.dynamic.build {
                DynamicBuildState::Idle => "idle".to_owned(),
                DynamicBuildState::Running { .. } => running_summary().to_owned(),
                DynamicBuildState::Failed(_) => "build failed".to_owned(),
            },
        };
        Some(format!("{reactants}; {state}"))
    }

    fn update_with_task(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Dynamic(message) => return dynamic_reaction::update(self, message),
            Message::OxideAppearanceFinished {
                run_id,
                request_binding,
                result,
            } => {
                if self.active_oxide_appearance_run != Some(run_id)
                    || self
                        .oxide_appearance_request
                        .as_ref()
                        .and_then(|request| request.binding_digest().ok())
                        != Some(request_binding)
                {
                    return Task::none();
                }
                self.active_oxide_appearance_run = None;
                self.oxide_appearance_cancellation = None;
                match *result {
                    Ok(appearance) => {
                        self.oxide_appearance_error = None;
                        self.oxide_appearance = Some(appearance);
                        self.refresh_real_world_plan();
                    }
                    Err(error) => self.oxide_appearance_error = Some(error),
                }
            }
            Message::RetryOxideAppearance => {
                self.cancel_oxide_appearance_enrichment();
                return self.start_oxide_appearance_enrichment();
            }
            message @ (Message::WindowResized(_)
            | Message::DumpFrame
            | Message::FrameCaptured(..)
            | Message::Noop
            | Message::KeyboardEvent { .. }
            | Message::PointerPressed
            | Message::BuilderInputFocusChecked { .. }) => {
                return self.update_input_message(message);
            }
            Message::ReturnToBuilder => self.enter_screen(Screen::Builder),
            Message::StartNewReaction => self.start_new_reaction(),
            message @ (Message::ProviderSelected(_)
            | Message::ProviderContinue
            | Message::SettingsOpened
            | Message::SettingsClosed
            | Message::SettingsAppModeSelected(_)
            | Message::SettingsChemicalLabelsSelected(_)
            | Message::SettingsSaveRequested
            | Message::SettingsSaveFinished { .. }) => {
                return self.update_settings_message(message);
            }
            message @ (Message::PeriodicTable(_)
            | Message::ReactantComposer(_)
            | Message::Sketcher(_)
            | Message::BuilderPanelToggled(_)
            | Message::BuilderPanelClosed) => return self.update_builder_message(message),
            #[cfg(target_arch = "wasm32")]
            Message::DemoRepoLinkOpened => {
                const REPO_URL: &str = "https://github.com/charles-mills/ChemSpec";
                if let Some(window) = web_sys::window() {
                    // Popup blockers may reject the new tab (returns None);
                    // fall back to navigating in place so the link always works.
                    let opened = window
                        .open_with_url_and_target(REPO_URL, "_blank")
                        .ok()
                        .flatten();
                    if opened.is_none() {
                        let _ = window.location().set_href(REPO_URL);
                    }
                }
            }
            message @ (Message::OutcomeSelected(_)
            | Message::StructuralPlaybackShortcut
            | Message::StructuralPlaybackToggled
            | Message::StructuralSpeedChanged
            | Message::StructuralTimelineScrubbed(_)
            | Message::StructuralRealWorldTimelineScrubbed(_)
            | Message::StructuralChapterChanged(_)
            | Message::StructuralSkipRequested(_)
            | Message::StructuralRestarted
            | Message::StructuralTick
            | Message::StructuralDrag(_)
            | Message::ContinueTo3d
            | Message::ContinueToSummary
            | Message::ReturnTo2d
            | Message::ReturnTo3d
            | Message::OutcomeChoiceMoved(_)
            | Message::OutcomeChoiceConfirmed) => return self.update_structural_message(message),
        }
        Task::none()
    }

    fn update_input_message(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::WindowResized(size) => {
                self.ui_zoom = adaptive_zoom(size, self.ui_zoom);
                reactant_composer::resize_ambient(&mut self.reactant_composer, size);
            }
            Message::DumpFrame => {
                if let Some(path) = self.dump_frame_path.take() {
                    return iced::window::latest()
                        .and_then(iced::window::screenshot)
                        .map(move |shot| Message::FrameCaptured(path.clone(), shot));
                }
            }
            Message::FrameCaptured(path, shot) => {
                let mut ppm =
                    format!("P6\n{} {}\n255\n", shot.size.width, shot.size.height).into_bytes();
                for pixel in shot.rgba.chunks_exact(4) {
                    ppm.extend_from_slice(&pixel[..3]);
                }
                let _ = std::fs::write(&path, ppm);
                let _ = std::fs::write(
                    path.with_extension("meta"),
                    format!(
                        "scale_factor={}\nui_zoom={}\n",
                        shot.scale_factor, self.ui_zoom
                    ),
                );
            }
            Message::Noop => {}
            Message::KeyboardEvent { event, status } => {
                let routed = if self.settings_dialog.is_some() {
                    settings_modal_keyboard_message(event, status)
                } else {
                    match self.screen {
                        Screen::Builder if self.dynamic_modal_kind().is_some() => {
                            dynamic_modal_keyboard_message(event)
                        }
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
                        | Screen::ProductSummary => {
                            screen_keyboard_message(self.screen, event, status)
                        }
                    }
                };
                if let Some(routed) = routed {
                    self.keyboard_navigation_active = true;
                    return self.update_with_task(routed);
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
            _ => unreachable!("input messages are routed by update_with_task"),
        }
        Task::none()
    }

    fn update_settings_message(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ProviderSelected(provider) => {
                if self.mode_available(provider) {
                    self.provider = Some(provider);
                    self.settings.app_mode = provider;
                    self.settings_load_error = None;
                }
            }
            Message::ProviderContinue => {
                let ready = match self.provider {
                    Some(AppMode::Local) => true,
                    Some(AppMode::CodexBinary) => self.codex_available,
                    Some(AppMode::Api) | None => false,
                };
                if ready {
                    let settings = AppSettings {
                        app_mode: self.provider.expect("a ready provider exists"),
                        chemical_labels: self.settings.chemical_labels,
                    };
                    return self
                        .start_settings_save(SettingsSaveDestination::FirstLaunch, settings);
                }
            }
            Message::SettingsOpened => {
                if self.settings_dialog.is_none() && self.dynamic_modal_kind().is_none() {
                    self.builder_panel = None;
                    self.settings_dialog = Some(SettingsDialog {
                        draft: self.settings,
                        error: None,
                    });
                }
            }
            Message::SettingsClosed => {
                if !self.settings_saving(SettingsSaveDestination::Dialog) {
                    self.settings_dialog = None;
                }
            }
            Message::SettingsAppModeSelected(mode) => {
                let available = self.mode_available(mode);
                if available && let Some(dialog) = &mut self.settings_dialog {
                    dialog.draft.app_mode = mode;
                    dialog.error = None;
                }
            }
            Message::SettingsChemicalLabelsSelected(labels) => {
                if let Some(dialog) = &mut self.settings_dialog {
                    dialog.draft.chemical_labels = labels;
                    dialog.error = None;
                }
            }
            Message::SettingsSaveRequested => {
                let Some(dialog) = &self.settings_dialog else {
                    return Task::none();
                };
                if !self.mode_available(dialog.draft.app_mode) {
                    return Task::none();
                }
                return self.start_settings_save(SettingsSaveDestination::Dialog, dialog.draft);
            }
            Message::SettingsSaveFinished {
                save_id,
                destination,
                settings,
                result,
            } => self.finish_settings_save(save_id, destination, settings, result),
            _ => unreachable!("settings messages are routed by update_with_task"),
        }
        Task::none()
    }

    fn update_builder_message(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::PeriodicTable(message) => {
                if self.dynamic_modal_kind().is_some() {
                    return Task::none();
                }
                periodic_table::update(&mut self.periodic_table, message);
                if let periodic_table::Message::Activated(atomic_number) = message {
                    return self.update_reactant_composer(reactant_composer::Message::AddElement(
                        atomic_number,
                    ));
                }
            }
            Message::ReactantComposer(message) => return self.update_reactant_composer(message),
            Message::Sketcher(message) => {
                if self.dynamic_modal_kind().is_some() {
                    return Task::none();
                }
                let submit = matches!(message, sketcher::Message::UseAsReactant);
                sketcher::update(&mut self.sketcher, message);
                if submit && let Some(sketch) = sketcher::submission(&self.sketcher) {
                    self.cancel_dynamic_work();
                    reactant_composer::set_sketched_reactant(
                        &mut self.reactant_composer,
                        sketch.atoms,
                        sketch.smiles,
                    );
                    self.builder_panel = None;
                    self.sync_builder_submit_prompt();
                }
            }
            Message::BuilderPanelToggled(panel) => {
                if self.dynamic_modal_kind().is_none() {
                    self.builder_panel = (self.builder_panel != Some(panel)).then_some(panel);
                }
            }
            Message::BuilderPanelClosed => self.builder_panel = None,
            _ => unreachable!("builder messages are routed by update_with_task"),
        }
        Task::none()
    }

    fn update_structural_message(&mut self, message: Message) -> Task<Message> {
        match message {
            message @ (Message::StructuralPlaybackShortcut
            | Message::StructuralPlaybackToggled
            | Message::StructuralSpeedChanged
            | Message::StructuralTimelineScrubbed(_)
            | Message::StructuralRealWorldTimelineScrubbed(_)
            | Message::StructuralChapterChanged(_)
            | Message::StructuralSkipRequested(_)
            | Message::StructuralRestarted) => self.update_playback_message(&message),
            message @ (Message::StructuralTick | Message::StructuralDrag(_)) => {
                self.update_structural_effect(message)
            }
            message @ (Message::OutcomeSelected(_)
            | Message::ContinueTo3d
            | Message::ContinueToSummary
            | Message::ReturnTo2d
            | Message::ReturnTo3d
            | Message::OutcomeChoiceMoved(_)
            | Message::OutcomeChoiceConfirmed) => self.update_structural_navigation(&message),
            _ => unreachable!("structural messages are routed by update_with_task"),
        }
    }

    fn update_playback_message(&mut self, message: &Message) -> Task<Message> {
        match message {
            Message::StructuralPlaybackShortcut => {
                if self.structural_shortcut_state == StructuralShortcutState::Ready {
                    return self.update_playback_message(&Message::StructuralPlaybackToggled);
                }
            }
            Message::StructuralPlaybackToggled => {
                self.structural_shortcut_state = StructuralShortcutState::Ready;
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
                self.seek_educational_timeline(u64::from(*progress));
            }
            Message::StructuralRealWorldTimelineScrubbed(progress) => {
                if let Some(animation) = &mut self.structural_animation {
                    animation.playing = false;
                }
                self.seek_real_world_timeline(u64::from(*progress));
            }
            Message::StructuralChapterChanged(delta) => self.change_structural_frame(*delta),
            Message::StructuralSkipRequested(delta) => {
                if self.screen == Screen::Structural2d {
                    self.change_structural_frame(*delta);
                } else if self.screen == Screen::Structural3d {
                    let Some(animation) = &mut self.structural_animation else {
                        return Task::none();
                    };
                    animation.playing = false;
                    let playhead = animation.real_world_playhead_ms;
                    let target = if *delta < 0 {
                        playhead.saturating_sub(5_000)
                    } else {
                        playhead.saturating_add(5_000)
                    };
                    self.seek_real_world_timeline(target);
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
            _ => unreachable!("playback messages are routed by update_structural_message"),
        }
        Task::none()
    }

    fn update_structural_effect(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::StructuralTick => {
                let frame_millis = if self.screen == Screen::Structural3d {
                    16
                } else {
                    33
                };
                if matches!(self.screen, Screen::Structural2d | Screen::Structural3d) {
                    self.structural_shortcut_state.tick(frame_millis);
                }
                let (elapsed, playing) =
                    self.structural_animation
                        .as_ref()
                        .map_or((frame_millis, false), |animation| {
                            (
                                animation.playback_speed.scale_millis(frame_millis),
                                animation.playing,
                            )
                        });
                if self.screen == Screen::ProductSummary {
                    if let Some(animation) = &mut self.structural_animation {
                        animation.summary_elapsed_ms =
                            animation.summary_elapsed_ms.saturating_add(33);
                    }
                } else if self.screen == Screen::Structural3d {
                    let now = Instant::now();
                    let measured =
                        self.structural_animation
                            .as_mut()
                            .map_or(elapsed, |animation| {
                                // The cap keeps a stall or resume from jumping
                                // the playhead by more than one coarse step.
                                let raw =
                                    animation.last_structural_tick.map_or(frame_millis, |last| {
                                        u32::try_from(now.duration_since(last).as_millis())
                                            .unwrap_or(u32::MAX)
                                            .clamp(1, 100)
                                    });
                                animation.last_structural_tick = Some(now);
                                animation.playback_speed.scale_millis(raw)
                            });
                    self.advance_real_world_playback(measured);
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
                        structural_2d::DragEvent::Ended => animation.physics.end_drag(),
                    }
                }
            }
            _ => unreachable!("effect messages are routed by update_structural_message"),
        }
        Task::none()
    }

    fn update_structural_navigation(&mut self, message: &Message) -> Task<Message> {
        match message {
            Message::OutcomeSelected(request) => {
                self.pending_requests.clear();
                self.oxygen_assessment = None;
                self.select_request(*request);
                self.open_structural_animation();
                return self.start_oxide_appearance_enrichment();
            }
            Message::ContinueTo3d => {
                let Some(animation) = &mut self.structural_animation else {
                    return Task::none();
                };
                animation.frame_index = 0;
                animation.real_world_playhead_ms = 0;
                animation.playing = true;
                self.enter_screen(Screen::Structural3d);
            }
            Message::ContinueToSummary => {
                let Some(animation) = &mut self.structural_animation else {
                    return Task::none();
                };
                animation.summary_elapsed_ms = 0;
                animation.playing = false;
                self.enter_screen(Screen::ProductSummary);
            }
            Message::ReturnTo2d => self.enter_screen(Screen::Structural2d),
            Message::ReturnTo3d => self.enter_screen(Screen::Structural3d),
            Message::OutcomeChoiceMoved(delta) => {
                if self.pending_requests.is_empty() {
                    return Task::none();
                }
                let len = self.pending_requests.len();
                self.keyboard_outcome_index = Some(match self.keyboard_outcome_index {
                    Some(current) if *delta < 0 => current.checked_sub(1).unwrap_or(len - 1),
                    Some(current) => (current + 1) % len,
                    None if *delta < 0 => len - 1,
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
                return self.update_structural_navigation(&Message::OutcomeSelected(request));
            }
            _ => unreachable!("navigation messages are routed by update_structural_message"),
        }
        Task::none()
    }

    #[cfg(test)]
    fn update(&mut self, message: Message) {
        drop(self.update_with_task(message));
    }

    fn builder_input_ready(&self) -> bool {
        if matches!(self.dynamic.build, DynamicBuildState::Running { .. })
            || reactant_composer::editing(&self.reactant_composer).is_some()
            || reactant_composer::roll_engaged(&self.reactant_composer)
        {
            return false;
        }
        let (first, second) = reactant_composer::reactants(&self.reactant_composer);
        reactant_composer::can_start_reaction(&self.reactant_composer)
            || (self.dynamic.context.is_some() && (!first.is_empty() || !second.is_empty()))
    }

    fn local_solver_declines(&self) -> bool {
        let requires_dynamic = self.dynamic.context.is_some()
            || matches!(
                reactant_composer::resolution(&self.reactant_composer),
                chemistry::DraftResolution::ExplicitlyUnsupported(_)
                    | chemistry::DraftResolution::Uncatalogued
                    | chemistry::DraftResolution::Unrecognized
            );
        if !self.local_mode() || !requires_dynamic {
            return false;
        }
        let Ok(catalogue) = chemistry::reference_catalogue() else {
            return false;
        };
        let Ok(identities) = reviewed_species_registry(catalogue) else {
            return false;
        };
        let request = self.dynamic_build_request();
        let Ok(RequestIdentityResolution::Resolved(_)) =
            resolve_request_identities_with_catalogue(&request, &identities, catalogue)
        else {
            return false;
        };
        agent::solve_reaction_claim_with_catalogue(&request, &identities, catalogue).is_none()
    }

    fn builder_can_submit(&self) -> bool {
        self.screen == Screen::Builder
            && self.dynamic_modal_kind().is_none()
            && self.builder_input_ready()
            && !self.local_solver_declines()
    }

    fn sync_builder_submit_prompt(&mut self) {
        if self.screen != Screen::Builder || self.dynamic_modal_kind().is_some() {
            reactant_composer::set_submit_available(&mut self.reactant_composer, false);
            return;
        }
        let input_ready = self.builder_input_ready();
        if input_ready && self.local_solver_declines() {
            reactant_composer::show_try_codex_notice(&mut self.reactant_composer);
        } else {
            reactant_composer::set_submit_available(&mut self.reactant_composer, input_ready);
        }
    }

    /// Cancels any in-flight dynamic build and clears its results. Every
    /// draft edit — typed, clicked, or sketched — must route through this so
    /// stale dynamic chemistry never survives a changed question.
    fn cancel_dynamic_work(&mut self) {
        self.cancel_oxide_appearance_enrichment();
        if let Some(cancellation) = self.dynamic.cancellation.take() {
            cancellation.store(true, Ordering::Relaxed);
        }
        self.dynamic.next_run_id = self.dynamic.next_run_id.saturating_add(1);
        self.dynamic.build = DynamicBuildState::Idle;
        self.dynamic.identity_choice = None;
        self.dynamic.started_at = None;
        self.dynamic.latency = LatencyMilestones::default();
        self.dynamic.progress = None;
        self.dynamic.progress_receiver = None;
        if self.dynamic.static_outcome.take().is_some()
            || self.dynamic.claim.take().is_some()
            || self.dynamic.presentation.take().is_some()
            || self.validated_frames.as_ref().is_some_and(|frames| {
                frames.provenance() == chem_kernel::DerivationProvenance::Provisional
            })
        {
            self.validated_frames = None;
            self.validated_declaration = None;
            self.structural_animation = None;
        }

        self.dynamic.request = None;
    }

    fn update_reactant_composer(&mut self, message: reactant_composer::Message) -> Task<Message> {
        if self.dynamic_modal_kind().is_some() && !message.is_presentation_only() {
            return Task::none();
        }
        let focus_target = match &message {
            reactant_composer::Message::BeginNameEntry(reactant) => {
                Some(reactant_composer::name_input_id(*reactant))
            }
            _ => None,
        };
        if matches!(message, reactant_composer::Message::StartReactionRequested)
            && matches!(self.dynamic.build, DynamicBuildState::Running { .. })
        {
            return Task::none();
        }
        if !matches!(message, reactant_composer::Message::StartReactionRequested) {
            // Presentation-only motion (ambient orbit, prompt fades, dice
            // rolls) must never cancel a running build or wipe a finished
            // result; only actual draft edits invalidate dynamic state.
            if message.is_presentation_only() {
                let was_rolling = reactant_composer::roll_engaged(&self.reactant_composer);
                reactant_composer::update(&mut self.reactant_composer, message);
                // A finished roll has written both drafts; surface the
                // "press space" prompt for the settled reaction.
                if was_rolling && !reactant_composer::roll_engaged(&self.reactant_composer) {
                    self.sync_builder_submit_prompt();
                }
                return Task::none();
            }
            self.cancel_dynamic_work();
            reactant_composer::update(&mut self.reactant_composer, message);
            let (first, second) = reactant_composer::reactants(&self.reactant_composer);
            if first.is_empty() && second.is_empty() {
                self.dynamic.context = None;
                if self.builder_panel == Some(BuilderPanel::Conditions) {
                    self.builder_panel = None;
                }
            }
            self.sync_builder_submit_prompt();
            return focus_target.map_or_else(Task::none, iced::widget::operation::focus);
        }
        reactant_composer::set_submit_available(&mut self.reactant_composer, false);
        self.builder_panel = None;
        if self.dynamic.context.is_some() {
            return self.start_dynamic_build();
        }
        match reactant_composer::resolution(&self.reactant_composer) {
            chemistry::DraftResolution::Supported(request) => {
                self.pending_requests.clear();
                self.oxygen_assessment = None;
                self.select_request(request);
                self.open_structural_animation();
                self.start_oxide_appearance_enrichment()
            }
            chemistry::DraftResolution::Multiple(requests) => {
                self.pending_requests = requests;
                self.oxygen_assessment = None;
                self.enter_screen(Screen::OutcomeChoice);
                Task::none()
            }
            chemistry::DraftResolution::Screened(assessment) => {
                self.pending_requests.clear();
                self.oxygen_assessment = Some(assessment);
                self.enter_screen(Screen::OutcomeChoice);
                Task::none()
            }
            chemistry::DraftResolution::ExplicitlyUnsupported(_)
            | chemistry::DraftResolution::Uncatalogued
            | chemistry::DraftResolution::Unrecognized => self.start_dynamic_build(),
            chemistry::DraftResolution::SystemError(_) => Task::none(),
        }
    }

    fn start_dynamic_build(&mut self) -> Task<Message> {
        let request = self.dynamic_build_request();
        self.start_dynamic_build_request(request, false)
    }

    fn dynamic_build_request(&self) -> ReactionBuildRequest {
        let (first, second) = reactant_composer::reactants(&self.reactant_composer);
        let names = reactant_composer::draft_names(&self.reactant_composer);
        // A selected condition rides along for any reactant count; the
        // submit gate already requires one for single-reactant requests.
        let context = self.dynamic.context;
        let drafts = [(first, names[0]), (second, names[1])]
            .into_iter()
            .filter(|(atoms, _)| !atoms.is_empty());
        ReactionBuildRequest {
            reactants: drafts
                .map(|(atoms, name)| ReactantInput {
                    // A typed name outranks the formula: it can identify a
                    // species the inventory alone cannot (ammonium cyanate
                    // vs urea), and the resolver accepts either form.
                    display: name.map_or_else(
                        || reactant_composer::formula(atoms),
                        std::borrow::ToOwned::to_owned,
                    ),
                    // Keep the identity inventory aligned with the standard-state
                    // formula shown by the composer (H₂, N₂, O₂, P₄, S₈, ...).
                    atomic_numbers: chemistry::standardize_elemental_draft(atoms),
                    species_id: None,
                })
                .collect(),
            selected_context: context.map(|context| context.value().to_owned()),
        }
    }

    fn start_dynamic_build_request(
        &mut self,
        mut request: ReactionBuildRequest,
        regenerate: bool,
    ) -> Task<Message> {
        self.cancel_oxide_appearance_enrichment();
        let local = self.local_mode();
        self.open_dynamic_overlay();
        if !local && !matches!(self.provider, Some(AppMode::CodexBinary)) {
            self.dynamic.build = DynamicBuildState::Failed(
                "Direct API reaction building is not available yet; choose Codex subscription."
                    .to_owned()
                    .into(),
            );
            return Task::none();
        }
        let run_id = self.dynamic.begin_run();
        self.validated_frames = None;
        self.validated_macroscopic = None;
        self.validated_declaration = None;
        self.structural_animation = None;
        self.structural_error = None;
        let mode = ClaimMode::Fast;
        let mut config = CodexProviderConfig::from_environment();
        let catalogue = match chemistry::reference_catalogue() {
            Ok(catalogue) => catalogue.clone(),
            Err(error) => {
                self.dynamic.build = DynamicBuildState::Failed(error.to_owned().into());
                return Task::none();
            }
        };
        let identities = match reviewed_species_registry(&catalogue) {
            Ok(identities) => identities,
            Err(error) => {
                self.dynamic.build = DynamicBuildState::Failed(error.into());
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
                self.dynamic.request = Some(request.clone());
                self.dynamic.identity_choice = Some(DynamicIdentityChoice { request, ambiguity });
                self.dynamic.build = DynamicBuildState::Idle;
                return Task::none();
            }
            Err(error) => {
                self.dynamic.request = Some(request);
                self.dynamic.build = DynamicBuildState::Failed(error.into());
                return Task::none();
            }
        }
        self.dynamic.request = Some(request.clone());
        self.dynamic.started_at = Some(Instant::now());
        self.dynamic.latency = LatencyMilestones::default();
        self.dynamic.build = DynamicBuildState::Running {
            run_id,
            elapsed_seconds: 0,
            stage: DynamicBuildStage::Claim,
        };
        let cancellation = Arc::new(AtomicBool::new(false));
        self.dynamic.worker_shutdown.watch(&cancellation);
        config.cancellation = Some(cancellation.clone());
        config.progress = Some(self.reset_dynamic_progress_channel());
        self.dynamic.cancellation = Some(cancellation);
        Task::perform(
            blocking::run::<_, DynamicBuildFailure, _>(move || {
                dynamic_reaction::run_claim(dynamic_reaction::ClaimJob {
                    request,
                    mode,
                    local,
                    regenerate,
                    config,
                    identities,
                    catalogue,
                })
            }),
            move |result| {
                Message::Dynamic(dynamic_reaction::Message::ClaimFinished {
                    run_id,
                    result: Box::new(result),
                })
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
        let catalogue = match chemistry::reference_catalogue() {
            Ok(catalogue) => catalogue.clone(),
            Err(error) => {
                return Task::done(Message::Dynamic(
                    dynamic_reaction::Message::PresentationFinished {
                        run_id,
                        result: Box::new(Err(error.to_owned().into())),
                    },
                ));
            }
        };
        Task::perform(
            blocking::run::<_, DynamicBuildFailure, _>(move || {
                if local {
                    // Reviewed-family and algorithmic mechanisms only; model
                    // escalation is explicitly unsupported, so a static
                    // settle is final rather than retryable.
                    let presentation = enrich_static_outcome(
                        outcome,
                        &catalogue,
                        &mut agent::UnsupportedMechanismProvider,
                    )
                    .map_err(DynamicBuildFailure::from)?;
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
                let provider_claim = outcome.provider_claim();
                let presentation = enrich_static_outcome(outcome, &catalogue, &mut provider)
                    .map_err(DynamicBuildFailure::from)?;
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
                if let (Some(recipe), Some(claim), Some(directory)) = (
                    recipe,
                    provider_claim.as_ref(),
                    provider.config().cache_directory.as_deref(),
                ) {
                    let identities =
                        reviewed_species_registry(&catalogue).map_err(DynamicBuildFailure::from)?;
                    let _ = store_dynamic_cache(
                        directory,
                        &request,
                        mode,
                        &identities,
                        &catalogue,
                        claim,
                        Some(recipe),
                        "codex_subscription",
                        provider.model_name(),
                    );
                }
                Ok(presentation)
            }),
            move |result| {
                Message::Dynamic(dynamic_reaction::Message::PresentationFinished {
                    run_id,
                    result: Box::new(result),
                })
            },
        )
    }

    fn finish_dynamic_presentation(&mut self, presentation: DynamicPresentationOutcome) {
        self.dynamic.static_outcome = Some(presentation.static_outcome().clone());
        self.validated_macroscopic =
            dynamic_macroscopic_reaction(presentation.static_outcome()).ok();

        self.validated_declaration = Some(presentation.static_outcome().declaration().clone());
        self.validated_frames = match &presentation {
            DynamicPresentationOutcome::ReviewedFamily(outcome) => Some(outcome.frames().clone()),
            DynamicPresentationOutcome::Escalated(outcome) => Some(outcome.frames().clone()),
            DynamicPresentationOutcome::Static { .. } => None,
        };
        let animated = self.validated_frames.is_some();
        self.dynamic.presentation = Some(presentation);
        self.dynamic.build = DynamicBuildState::Idle;
        self.dynamic.cancellation = None;
        self.dynamic.progress_receiver = None;
        self.structural_animation = None;
        self.structural_error = None;
        // Auto-navigating into the animation replaces the overlay; a
        // static-only result surfaces it on the builder instead.
        if animated {
            self.dynamic.overlay_dismissed = true;
            self.open_structural_animation();
        } else {
            self.open_dynamic_overlay();
        }
    }

    fn reset_dynamic_progress_channel(&mut self) -> Sender<CodexProgressEvent> {
        let (sender, receiver) = mpsc::channel();
        self.dynamic.progress = None;
        self.dynamic.progress_receiver = Some(receiver);
        sender
    }

    fn drain_dynamic_progress(&mut self) {
        let Some(receiver) = &self.dynamic.progress_receiver else {
            return;
        };
        while let Ok(event) = receiver.try_recv() {
            self.dynamic.progress = Some(event);
        }
    }

    fn dynamic_progress_label(&self) -> Option<&'static str> {
        self.dynamic.progress.map(|event| match event.stage {
            CodexProgressStage::Started => "preparing the virtual model",
            CodexProgressStage::Working => "working out where the electrons go",
            CodexProgressStage::SearchingSources => "checking the supporting evidence",
            CodexProgressStage::Completed => "the next view is ready",
            CodexProgressStage::Failed => "this pass needs another try",
        })
    }

    // The offline fixture crosses the same language/kernel boundary that live
    // provider output must cross later.
    fn select_request(&mut self, request: chemistry::ReactionRequest) {
        self.cancel_oxide_appearance_enrichment();
        self.active_request = request;
        let validated_run = chemistry::run(request).ok();
        self.validated_frames = validated_run.as_ref().map(|run| run.frames().clone());
        self.validated_macroscopic = validated_run
            .as_ref()
            .and_then(|run| run.macroscopic().cloned());
        self.validated_declaration = validated_run.as_ref().map(|run| run.declaration().clone());
        self.dynamic.claim = None;
        self.dynamic.static_outcome = None;
        self.dynamic.presentation = None;

        self.dynamic.request = None;
        self.dynamic.build = DynamicBuildState::Idle;
        self.structural_animation = None;
        self.structural_error = None;
    }

    fn oxide_appearance_request(&self) -> Option<OxideAppearanceRequest> {
        let reaction = self.validated_macroscopic.as_ref()?;
        if reaction.process != Some(MacroscopicProcess::SurfaceOxidation) {
            return None;
        }
        let product = reaction.materials.iter().find(|material| {
            material.role == MacroscopicMaterialRole::Product
                && material.representation == RepresentationKind::Ionic
        })?;
        let catalogue_digest = chemistry::reference_catalogue().ok()?.digest();
        Some(OxideAppearanceRequest::new(
            product.binding.clone(),
            product.structure_id.clone(),
            product.formula.clone(),
            product.semantic_identity.clone(),
            catalogue_digest,
        ))
    }

    fn start_oxide_appearance_enrichment(&mut self) -> Task<Message> {
        let Some(request) = self.oxide_appearance_request() else {
            return Task::none();
        };
        if self.oxide_appearance_request.as_ref() == Some(&request)
            && (self.active_oxide_appearance_run.is_some()
                || self.oxide_appearance.is_some()
                || self.oxide_appearance_error.is_some())
        {
            return Task::none();
        }
        self.cancel_oxide_appearance_enrichment();
        self.oxide_appearance_request = Some(request.clone());
        if self.provider != Some(AppMode::CodexBinary) {
            self.oxide_appearance_error = Some(
                "A reviewed oxide colour is not available. Runtime colour research requires \
                 Codex mode; the surface keeps its original metal colour until validated appearance \
                 data is available."
                    .to_owned(),
            );
            return Task::none();
        }
        if !self.codex_available {
            self.oxide_appearance_error = Some(
                "Runtime oxide-colour research is unavailable because Codex is not authenticated."
                    .to_owned(),
            );
            return Task::none();
        }
        let Ok(request_binding) = request.binding_digest() else {
            self.oxide_appearance_error =
                Some("The validated oxide identity could not be bound for colour research.".into());
            return Task::none();
        };
        let run_id = self.next_oxide_appearance_run_id;
        self.next_oxide_appearance_run_id = self.next_oxide_appearance_run_id.saturating_add(1);
        self.active_oxide_appearance_run = Some(run_id);
        let cancellation = Arc::new(AtomicBool::new(false));
        self.oxide_appearance_cancellation = Some(cancellation.clone());
        let mut config = CodexProviderConfig::from_environment();
        config.cancellation = Some(cancellation);
        Task::perform(
            async move {
                let provider = CodexProvider::new(config);
                if let Some(cached) = load_oxide_appearance_cache(
                    provider.config().cache_directory.as_deref(),
                    &request,
                ) {
                    return Ok(cached);
                }
                let appearance = provider
                    .research_oxide_appearance(&request)
                    .map_err(|error| error.to_string())?;
                if let Some(directory) = provider.config().cache_directory.as_deref() {
                    let _ = store_oxide_appearance_cache(
                        directory,
                        &request,
                        &appearance,
                        "codex_subscription",
                        provider.model_name(),
                    );
                }
                Ok(appearance)
            },
            move |result| Message::OxideAppearanceFinished {
                run_id,
                request_binding,
                result: Box::new(result),
            },
        )
    }

    fn cancel_oxide_appearance_enrichment(&mut self) {
        if let Some(cancellation) = self.oxide_appearance_cancellation.take() {
            cancellation.store(true, Ordering::Relaxed);
        }
        self.active_oxide_appearance_run = None;
        self.next_oxide_appearance_run_id = self.next_oxide_appearance_run_id.saturating_add(1);
        self.oxide_appearance_request = None;
        self.oxide_appearance = None;
        self.oxide_appearance_error = None;
    }

    fn accepted_surface_oxide_colour(&self) -> Option<SurfaceOxideColour> {
        let request = self.oxide_appearance_request.as_ref()?;
        let appearance = self.oxide_appearance.as_ref()?;
        let [red, green, blue] = appearance.colour_family().srgb();
        Some(SurfaceOxideColour {
            product_binding: request.product_binding.clone(),
            target: VisualColour { red, green, blue },
            authority: MacroscopicColourAuthority::ModelAsserted,
        })
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
        } else if self.screen == Screen::Structural3d
            && self
                .structural_animation
                .as_ref()
                .is_some_and(|animation| animation.playing)
        {
            // Authored 3D clips interpolate continuously, so request a 60 Hz
            // presentation tick instead of exposing their 30 FPS source cadence.
            iced::time::every(theme::motion::PROMPT_TICK).map(|_| Message::StructuralTick)
        } else if self.screen == Screen::Structural2d
            && self
                .structural_animation
                .as_ref()
                .is_none_or(|animation| animation.playing || !animation.settled)
        {
            iced::time::every(std::time::Duration::from_millis(33)).map(|_| Message::StructuralTick)
        } else if self.screen == Screen::ProductSummary {
            iced::time::every(theme::motion::TICK).map(|_| Message::StructuralTick)
        } else {
            Subscription::none()
        };

        let dynamic_build = if self.screen == Screen::Builder {
            match &self.dynamic.build {
                DynamicBuildState::Running { run_id, .. } => {
                    iced::time::every(std::time::Duration::from_secs(1))
                        .with(*run_id)
                        .map(|(run_id, _)| {
                            Message::Dynamic(dynamic_reaction::Message::BuildTick { run_id })
                        })
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
            && self.dynamic.static_outcome.is_some()
            && matches!(
                self.dynamic.build,
                DynamicBuildState::Running {
                    stage: DynamicBuildStage::Presentation,
                    ..
                }
            ) {
            iced::time::every(std::time::Duration::from_millis(33))
                .map(|_| Message::Dynamic(dynamic_reaction::Message::TheatreTick))
        } else {
            Subscription::none()
        };

        Subscription::batch([
            resize,
            frame_dump,
            screen,
            dynamic_build,
            dynamic_theatre,
            input,
        ])
    }

    fn view(&self) -> Element<'_, Message> {
        responsive(|size| {
            let application = match self.screen {
                Screen::ProviderSetup => self.provider_setup_view(size),
                Screen::Builder => self.builder_view(size),
                Screen::OutcomeChoice => self.outcome_choice_view(size),
                Screen::Structural2d => self.structural_2d_view(size),
                Screen::Structural3d => self.structural_3d_view(size),
                Screen::ProductSummary => self.product_summary_view(size),
            };
            if self.settings_dialog.is_some() {
                stack![application, self.settings_overlay(size)]
                    .width(Fill)
                    .height(Fill)
                    .into()
            } else {
                application
            }
        })
        .into()
    }

    #[allow(clippy::too_many_lines)]
    fn outcome_choice_view(&self, size: Size) -> Element<'_, Message> {
        use chem_catalogue::{OxygenOutcome, StructuralSupport};

        let compact = size.width < breakpoint::MOBILE || size.height < 760.0;

        let back = button(text("← Reactants"))
            .on_press(Message::ReturnToBuilder)
            .padding([spacing::XS, spacing::SM])
            .style(theme::secondary_button);

        let content: Element<'_, Message> = if !self.pending_requests.is_empty() {
            let mut choices = column![].spacing(spacing::SM).width(Fill);
            for (index, request) in self.pending_requests.iter().enumerate() {
                choices = choices.push(reviewed_outcome_choice(
                    *request,
                    self.settings.chemical_labels,
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
        let local_selected = self.provider == Some(AppMode::Local);
        let codex_selected = self.provider == Some(AppMode::CodexBinary);

        let local = button(
            row![
                icons::chip(20.0, color::ACCENT),
                text("Local").size(type_scale::BODY_LARGE),
            ]
            .spacing(spacing::SM)
            .align_y(Center),
        )
        .on_press(Message::ProviderSelected(AppMode::Local))
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
                text("Codex").size(type_scale::BODY_LARGE),
            ]
            .spacing(spacing::SM)
            .align_y(Center),
        )
        .on_press_maybe(
            self.codex_available
                .then_some(Message::ProviderSelected(AppMode::CodexBinary)),
        )
        .padding([spacing::SM, spacing::MD])
        .width(Fill)
        .style(move |_, status| theme::provider_button(codex_selected, status));

        let api = button(
            row![
                icons::api_key(20.0, color::FAINT),
                text("API").size(type_scale::BODY_LARGE).color(color::FAINT),
                space().width(Fill),
                text("UNAVAILABLE")
                    .size(type_scale::MICRO)
                    .color(color::FAINT),
            ]
            .spacing(spacing::SM)
            .align_y(Center),
        )
        .padding([spacing::SM, spacing::MD])
        .width(Fill)
        .style(theme::secondary_button);

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
            "Continue with Local"
        } else {
            "Continue with Codex"
        };
        let saving = self.settings_saving(SettingsSaveDestination::FirstLaunch);
        let continue_icon_color = if ready { color::CANVAS } else { color::FAINT };
        let continue_button = button(
            row![
                text(if saving { "Saving…" } else { continue_label }),
                icons::arrow_right(16.0, continue_icon_color),
            ]
            .spacing(spacing::XS)
            .align_y(Center),
        )
        .on_press_maybe((ready && !saving).then_some(Message::ProviderContinue))
        .padding([spacing::SM, spacing::MD])
        .style(theme::primary_button);

        let action: Element<'_, Message> = row![space().width(Fill), continue_button]
            .align_y(Center)
            .width(Fill)
            .into();

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
                        text("Install Codex, or continue in Local mode.")
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

        if let Some(error) = &self.settings_load_error {
            sections.push(
                text(error)
                    .size(type_scale::CAPTION)
                    .color(color::DANGER)
                    .into(),
            );
        }

        sections.push(action);
        if self.keyboard_navigation_active {
            sections.push(
                text("↑ ↓ choose  ·  1–2 select  ·  Enter continue")
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
            let required_context = self
                .dynamic
                .static_outcome
                .as_ref()
                .map_or_else(
                    || {
                        self.validated_declaration
                            .as_ref()
                            .map(chem_domain::ReactionDeclaration::required_context)
                    },
                    |dynamic| Some(dynamic.declaration().required_context()),
                )
                .ok_or_else(|| "validated declaration is unavailable".to_owned())?;
            let educational_plan = compile_educational_plan(&frames, required_context)
                .map_err(|error| error.to_string())?;
            let surface_oxide_colour = self.accepted_surface_oxide_colour();
            let (profile, product_preview, declaration) = if let Some(dynamic) =
                &self.dynamic.static_outcome
            {
                (
                    dynamic_presentation_profile(&frames, dynamic, surface_oxide_colour.clone())?,
                    None,
                    dynamic.declaration().clone(),
                )
            } else {
                let mut macroscopic = self.validated_macroscopic.clone();
                if let Some(reaction) = &mut macroscopic {
                    reaction.surface_oxide_colour = surface_oxide_colour;
                }
                (
                    chemistry::presentation_profile_with_catalogue(
                        self.active_request,
                        &frames,
                        macroscopic.as_ref(),
                    )?,
                    self.active_request.product_preview(),
                    self.validated_declaration
                        .clone()
                        .ok_or_else(|| "validated declaration is unavailable".to_owned())?,
                )
            };
            let real_world_plan =
                compile_real_world_plan(&frames, &profile).map_err(|error| error.to_string())?;
            let home_timeline = structural_2d::home_timeline(frames.frames());
            Ok::<_, String>(StructuralAnimation {
                frames,
                declaration,
                educational_plan,
                real_world_plan,
                product_preview,
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
                last_structural_tick: None,
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
        self.enter_screen(Screen::Structural2d);
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

    fn refresh_real_world_plan(&mut self) {
        let Some(animation) = self.structural_animation.as_ref() else {
            return;
        };
        let frames = animation.frames.clone();
        let surface_oxide_colour = self.accepted_surface_oxide_colour();
        let profile = if let Some(dynamic) = &self.dynamic.static_outcome {
            dynamic_presentation_profile(&frames, dynamic, surface_oxide_colour)
        } else {
            let mut macroscopic = self.validated_macroscopic.clone();
            if let Some(reaction) = &mut macroscopic {
                reaction.surface_oxide_colour = surface_oxide_colour;
            }
            chemistry::presentation_profile_with_catalogue(
                self.active_request,
                &frames,
                macroscopic.as_ref(),
            )
        };
        let result = profile.and_then(|profile| {
            compile_real_world_plan(&frames, &profile).map_err(|error| error.to_string())
        });
        match result {
            Ok(plan) => {
                if let Some(animation) = &mut self.structural_animation {
                    animation.real_world_playhead_ms = animation
                        .real_world_playhead_ms
                        .min(plan.timeline.duration_ms());
                    animation.real_world_plan = plan;
                }
            }
            Err(error) => self.structural_error = Some(error),
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
            return Self::structural_unavailable_view("Validated frames are unavailable");
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
            .on_press(Message::ReturnToBuilder)
            .padding([spacing::XS, spacing::SM])
            .style(theme::secondary_button);
        // Local Mode derivations are deterministic; regenerating would only
        // recompute the identical result.
        let regenerate: Element<'_, Message> =
            if self.dynamic.request.is_some() && !self.local_mode() {
                button(text("Regenerate"))
                    .on_press(Message::Dynamic(dynamic_reaction::Message::Regenerate))
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

        let equation = Some(nomenclature::display_declaration(
            &animation.declaration,
            self.settings.chemical_labels,
        ));
        let scene_context = structural_2d::SceneContext::new(
            educational_scene.kind,
            timeline_position.scene_index,
            animation.educational_plan.scenes.len(),
        )
        .with_equation(equation.clone())
        .with_electricity(
            animation
                .declaration
                .required_context()
                .eq_ignore_ascii_case("electricity"),
        );
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
        .style(theme::app_background)
        .padding(chromeless_page_padding(spacing::SM, spacing::LG))
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
        .style(theme::app_background)
        .padding(chromeless_page_padding(spacing::MD, spacing::LG))
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
            if self.dynamic.request.is_some() && !self.local_mode() {
                button(text("Regenerate"))
                    .on_press(Message::Dynamic(dynamic_reaction::Message::Regenerate))
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
        let at_end = animation.real_world_playhead_ms == real_world_plan.timeline.duration_ms();
        let review_products = button(text(if at_end {
            "Review products  →"
        } else {
            "Complete simulation to review"
        }))
        .on_press_maybe(at_end.then_some(Message::ContinueToSummary))
        .padding([spacing::XS, spacing::SM])
        .style(theme::primary_button);
        let process_annotation = match moment.stage {
            MacroscopicStage::Reaction => None,
            MacroscopicStage::HeatingPreparation => Some((
                "VIRTUAL SEPARATION · HEATING",
                "The completed solution is positioned over a virtual heat source; this is a separate presentation step, not further reaction chemistry.",
            )),
            MacroscopicStage::SolventBoiling => Some((
                "VIRTUAL SEPARATION · EVAPORATION",
                "Nucleate boiling lowers the solvent level while buoyant vapour leaves the open vessel.",
            )),
            MacroscopicStage::CrystalGrowth => Some((
                "VIRTUAL SEPARATION · CRYSTALLISATION",
                "As the remaining solvent disappears, the already-validated dissolved ionic product nucleates and grows as a crystal residue.",
            )),
        };
        let active_annotation = real_world_plan.annotations.iter().rfind(|annotation| {
            annotation.start_ordinal <= moment.ordinal && moment.ordinal <= annotation.end_ordinal
        });
        let active_effects = match moment.stage {
            MacroscopicStage::Reaction => real_world_plan
                .effects
                .iter()
                .filter(|effect| {
                    effect.start_ordinal <= moment.ordinal && moment.ordinal <= effect.end_ordinal
                })
                .map(|effect| macroscopic_effect_label(effect.effect))
                .collect::<Vec<_>>()
                .join("  ·  "),
            MacroscopicStage::HeatingPreparation => "Beaker lift  ·  Burner ignition".to_owned(),
            MacroscopicStage::SolventBoiling => {
                "Wall nucleation  ·  Bubble detachment  ·  Vapour flow".to_owned()
            }
            MacroscopicStage::CrystalGrowth => {
                "Solvent decay  ·  Crystal nucleation  ·  Faceted growth".to_owned()
            }
        };
        let mut annotation = process_annotation.map_or_else(
            || {
                active_annotation.map_or_else(
                    || {
                        column![
                            text("REVIEWED SCENE")
                                .size(type_scale::MICRO)
                                .color(color::ACCENT),
                            text(nomenclature::display_declaration(
                                &animation.declaration,
                                self.settings.chemical_labels,
                            ))
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
                )
            },
            |(title, body)| {
                column![
                    text(title).size(type_scale::MICRO).color(color::ACCENT),
                    text(body).size(type_scale::BODY_LARGE).color(color::TEXT),
                    text(active_effects.clone())
                        .size(type_scale::MICRO)
                        .color(color::TEXT_SOFT),
                ]
            },
        );
        let product_visible = real_world_plan.objects.iter().any(|object| {
            object.role == chem_presentation::SceneRole::Product
                && object.visible_from_ordinal <= moment.ordinal
        });
        if product_visible && let Some(preview) = &animation.product_preview {
            annotation = annotation.push(
                text(format!("Macroscopic product · {}", preview.formula))
                    .size(type_scale::MICRO)
                    .color(color::TEXT_SOFT),
            );
        }
        if self.oxide_appearance_request.is_some() {
            let appearance_status: Element<'_, Message> =
                if self.active_oxide_appearance_run.is_some() {
                    text("OXIDE COLOUR · CHECKING REVIEWABLE SOURCES…")
                        .size(type_scale::MICRO)
                        .color(color::ACCENT)
                        .into()
                } else if self.oxide_appearance.is_some() {
                    text("OXIDE COLOUR · VALIDATED RUNTIME APPEARANCE APPLIED")
                        .size(type_scale::MICRO)
                        .color(color::ACCENT)
                        .into()
                } else if let Some(error) = &self.oxide_appearance_error {
                    let retry: Element<'_, Message> =
                        if self.provider == Some(AppMode::CodexBinary) && self.codex_available {
                            button(text("Retry colour"))
                                .on_press(Message::RetryOxideAppearance)
                                .padding([spacing::XXS, spacing::XS])
                                .style(theme::secondary_button)
                                .into()
                        } else {
                            space().width(Length::Shrink).into()
                        };
                    row![
                        text(error)
                            .size(type_scale::MICRO)
                            .color(color::TEXT_SOFT)
                            .width(Fill),
                        retry,
                    ]
                    .spacing(spacing::XS)
                    .align_y(Center)
                    .into()
                } else {
                    space().height(Length::Shrink).into()
                };
            annotation = annotation.push(appearance_status);
        }
        let scene_view =
            iced::widget::Shader::new(structural_3d::Scene::new(real_world_plan, moment))
                .width(Fill)
                .height(Fill);
        let model_disclosure = if compact {
            "VIRTUAL MODEL · NOT A LAB PROCEDURE"
        } else {
            "VIRTUAL MODEL · NOT A LAB PROCEDURE · TIMING, SCALE & MOTION ARE ILLUSTRATIVE"
        };
        let inset_caption: Element<'_, Message> = space().width(Length::Shrink).into();
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
                text(nomenclature::display_declaration(
                    &animation.declaration,
                    self.settings.chemical_labels,
                ))
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
        .style(theme::app_background)
        .padding(chromeless_page_padding(spacing::SM, spacing::LG))
        .width(Fill)
        .height(Fill)
        .into()
    }

    /// Provenance chip for the current dynamic outcome.
    fn dynamic_provenance_label(&self) -> &'static str {
        self.dynamic.static_outcome.as_ref().map_or("", |outcome| {
            match outcome.claim_provenance() {
                OutcomeProvenance::Reviewed => "REVIEWED",
                OutcomeProvenance::Derived => "DERIVED",
                OutcomeProvenance::ModelAsserted => "MODEL ASSERTED",
            }
        })
    }

    #[allow(clippy::too_many_lines)]
    fn dynamic_result_body(&self) -> Element<'_, Message> {
        let Some(outcome) = &self.dynamic.static_outcome else {
            return space().height(Length::Shrink).into();
        };
        let presentation = match (&self.dynamic.build, &self.dynamic.presentation) {
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
                    self.dynamic.theatre_phase,
                    self.settings.chemical_labels,
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
        let diagnostic = match &self.dynamic.presentation {
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
            (&self.dynamic.presentation, &self.dynamic.build),
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
                    .on_press(Message::Dynamic(
                        dynamic_reaction::Message::RetryPresentation,
                    ))
                    .padding([spacing::XS, spacing::SM])
                    .style(theme::secondary_button),
            );
        }
        if let Some(diagnostic) = &diagnostic {
            actions = actions.push(
                button(text(if self.dynamic.details_open {
                    "Hide details"
                } else {
                    "Details"
                }))
                .on_press(Message::Dynamic(dynamic_reaction::Message::ToggleDetails))
                .padding([spacing::XS, spacing::SM])
                .style(theme::secondary_button),
            );
            let _ = diagnostic;
        }
        let details: Element<'_, Message> = if self.dynamic.details_open {
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
                text(nomenclature::display_declaration(
                    outcome.declaration(),
                    self.settings.chemical_labels,
                ))
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
        let Some(claim) = &self.dynamic.claim else {
            return space().height(Length::Shrink).into();
        };
        let (title, detail) = match claim.disposition {
            ClaimDisposition::NoReaction => match claim.no_reaction_reason() {
                Some(reason) => ("No reaction", reason.learner_explanation()),
                None => ("No supported reaction", claim.required_context.clone()),
            },
            ClaimDisposition::Ambiguous => (
                "More detail is needed",
                claim.ambiguity.as_ref().map_or_else(
                    || claim.required_context.clone(),
                    |value| value.summary.clone(),
                ),
            ),
            ClaimDisposition::Unsupported => (
                "Outside the current chemistry capability",
                claim.required_context.clone(),
            ),
            ClaimDisposition::Reaction => ("Outcome claim", claim.required_context.clone()),
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
        } = &self.dynamic.build
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
        let reactants = self
            .dynamic
            .request
            .as_ref()
            .map_or_else(String::new, |request| {
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
        let DynamicBuildState::Failed(error) = &self.dynamic.build else {
            return space().height(Length::Shrink).into();
        };
        column![
            text("Couldn\u{2019}t build this result")
                .size(type_scale::BODY_LARGE)
                .color(color::TEXT),
            text(error.to_string())
                .size(type_scale::CAPTION)
                .color(color::MUTED),
        ]
        .spacing(spacing::XXS)
        .into()
    }

    /// The dynamic-build modal: every Tier B/C surface (progress, results,
    /// verdicts, identity choices, failures) lives here instead of inline
    /// cards that squeeze the builder.
    fn dynamic_overlay(&self, size: Size, kind: DynamicModalKind) -> Element<'_, Message> {
        dynamic_reaction::overlay(self, size, kind)
    }

    #[allow(clippy::too_many_lines)]
    fn settings_overlay(&self, size: Size) -> Element<'_, Message> {
        let Some(dialog) = &self.settings_dialog else {
            return space().height(Length::Shrink).into();
        };
        let compact = size.width < breakpoint::MOBILE || size.height < 680.0;
        let saving = self.settings_saving(SettingsSaveDestination::Dialog);
        let option = |label: &'static str,
                      detail: &'static str,
                      selected: bool,
                      enabled: bool,
                      message: Message| {
            let label_color = if enabled { color::TEXT } else { color::FAINT };
            button(
                row![
                    column![
                        text(label).size(type_scale::BODY_LARGE).color(label_color),
                        text(detail).size(type_scale::CAPTION).color(if enabled {
                            color::TEXT_SOFT
                        } else {
                            color::FAINT
                        }),
                    ]
                    .spacing(spacing::XXS)
                    .width(Fill),
                    text(if selected { "✓" } else { "" })
                        .size(type_scale::BODY_LARGE)
                        .color(color::ACCENT),
                ]
                .spacing(spacing::SM)
                .align_y(Center),
            )
            .on_press_maybe((enabled && !saving).then_some(message))
            .padding([spacing::SM, spacing::MD])
            .width(Fill)
            .style(move |_, status| theme::provider_button(selected, status))
        };

        let local = option(
            "Local",
            "On-device solver only",
            dialog.draft.app_mode == AppMode::Local,
            true,
            Message::SettingsAppModeSelected(AppMode::Local),
        );
        let codex = option(
            "Codex",
            if self.codex_available {
                "Codex binary"
            } else {
                "Codex not found"
            },
            dialog.draft.app_mode == AppMode::CodexBinary,
            self.codex_available,
            Message::SettingsAppModeSelected(AppMode::CodexBinary),
        );
        let api = option(
            "API",
            "Unavailable",
            dialog.draft.app_mode == AppMode::Api,
            false,
            Message::SettingsAppModeSelected(AppMode::Api),
        );
        let app_modes: Element<'_, Message> = if compact {
            column![local, codex, api]
                .spacing(spacing::XS)
                .width(Fill)
                .into()
        } else {
            row![local, codex, api]
                .spacing(spacing::XS)
                .width(Fill)
                .into()
        };

        let formulae = option(
            "Formulae",
            "H₂O",
            dialog.draft.chemical_labels == ChemicalLabels::Formulae,
            true,
            Message::SettingsChemicalLabelsSelected(ChemicalLabels::Formulae),
        );
        let names = option(
            "Names",
            "water",
            dialog.draft.chemical_labels == ChemicalLabels::Names,
            true,
            Message::SettingsChemicalLabelsSelected(ChemicalLabels::Names),
        );
        let chemical_labels: Element<'_, Message> = row![formulae, names]
            .spacing(spacing::XS)
            .width(Fill)
            .into();

        let changed = dialog.draft != self.settings;
        let can_save = changed && self.mode_available(dialog.draft.app_mode);
        let save = button(text(if saving { "Saving…" } else { "Save" }))
            .on_press_maybe((can_save && !saving).then_some(Message::SettingsSaveRequested))
            .padding([spacing::XS, spacing::MD])
            .style(theme::primary_button);
        let cancel = button(text("Cancel"))
            .on_press_maybe((!saving).then_some(Message::SettingsClosed))
            .padding([spacing::XS, spacing::MD])
            .style(theme::secondary_button);
        let error: Element<'_, Message> = dialog.error.as_ref().map_or_else(
            || space().height(Length::Shrink).into(),
            |error| {
                text(error)
                    .size(type_scale::CAPTION)
                    .color(color::DANGER)
                    .into()
            },
        );
        let content = column![
            row![
                text("Settings")
                    .size(type_scale::TITLE)
                    .font(fonts::SEMIBOLD)
                    .color(color::TEXT),
                space().width(Fill),
                button(text("×").size(type_scale::BODY_LARGE))
                    .on_press_maybe((!saving).then_some(Message::SettingsClosed))
                    .padding([0.0, spacing::XS])
                    .style(theme::secondary_button),
            ]
            .align_y(Center),
            rule::horizontal(1).style(theme::soft_divider),
            text("APP MODE")
                .size(type_scale::MICRO)
                .color(color::TEXT_SOFT),
            app_modes,
            text("CHEMICAL LABELS")
                .size(type_scale::MICRO)
                .color(color::TEXT_SOFT),
            chemical_labels,
            error,
            row![space().width(Fill), cancel, save]
                .spacing(spacing::XS)
                .align_y(Center),
        ]
        .spacing(if compact { spacing::SM } else { spacing::MD })
        .width(Fill);
        let panel_width = (size.width - spacing::LG * 2.0).clamp(280.0, 620.0);
        let available_height = (size.height - spacing::LG * 2.0).max(280.0);
        let panel_height = available_height.min(if compact { 560.0 } else { 390.0 });
        let panel = mouse_area(
            container(scrollable(content).height(Fill))
                .style(theme::overlay_panel)
                .padding(if compact { spacing::MD } else { spacing::LG })
                .width(Length::Fixed(panel_width))
                .height(Length::Fixed(panel_height)),
        )
        .on_press(Message::Noop);
        stack![
            mouse_area(
                container(space())
                    .style(theme::overlay_scrim)
                    .width(Fill)
                    .height(Fill),
            )
            .on_press(if saving {
                Message::Noop
            } else {
                Message::SettingsClosed
            }),
            container(panel).center(Fill),
        ]
        .width(Fill)
        .height(Fill)
        .into()
    }

    fn dynamic_latency_summary(&self) -> String {
        let mut milestones = Vec::new();
        for (label, value) in [
            ("claim", self.dynamic.latency.claim_ms),
            ("static", self.dynamic.latency.static_outcome_ms),
            ("evidence", self.dynamic.latency.evidence_ms),
            ("mechanism", self.dynamic.latency.mechanism_ms),
            (
                "reviewed animation",
                self.dynamic.latency.reviewed_animation_ms,
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
        let Some(choice) = &self.dynamic.identity_choice else {
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
                .on_press(Message::Dynamic(
                    dynamic_reaction::Message::IdentitySelected {
                        reactant_index: choice.ambiguity.reactant_index,
                        species_id: species.id.clone(),
                    },
                ))
                .style(theme::secondary_button)
                .width(Fill),
            );
        }
        alternatives.into()
    }

    /// Wraps a toolbar control in the shared bottom-anchored tooltip chrome.
    fn toolbar_tooltip<'a>(
        control: iced::widget::Button<'a, Message>,
        label: &'a str,
    ) -> Element<'a, Message> {
        tooltip(
            control,
            text(label)
                .size(type_scale::CAPTION)
                .color(color::TEXT_SOFT),
            tooltip::Position::Bottom,
        )
        .gap(spacing::XS)
        .padding(spacing::XS)
        .style(|_| theme::tooltip_surface(1.0))
        .into()
    }

    #[allow(clippy::too_many_lines)]
    fn builder_toolbar(&self, conditions_enabled: bool) -> Element<'_, Message> {
        // The die tumbles through its faces while a roll spins; idle it rests
        // on one face and a press starts a roll.
        let spin_ticks = reactant_composer::roll_spin_ticks(&self.reactant_composer);
        let dice = Self::toolbar_tooltip(
            button(icons::dice(
                spin_ticks.map_or(0, |ticks| usize::try_from(ticks / 5).unwrap_or(0)),
                BUILDER_TOOLBAR_ICON_SIZE,
                if spin_ticks.is_some() {
                    color::ACCENT
                } else {
                    color::TEXT_SOFT
                },
            ))
            .on_press_maybe(spin_ticks.is_none().then_some(Message::ReactantComposer(
                reactant_composer::Message::RollRequested,
            )))
            .padding(spacing::XS)
            .style(theme::secondary_button),
            "Roll a random reaction",
        );

        let conditions_selected =
            self.builder_panel == Some(BuilderPanel::Conditions) || self.dynamic.context.is_some();
        let conditions_color = if conditions_selected {
            color::CANVAS
        } else if conditions_enabled {
            color::TEXT_SOFT
        } else {
            color::FAINT
        };
        let conditions = Self::toolbar_tooltip(
            button(icons::atom(BUILDER_TOOLBAR_ICON_SIZE, conditions_color))
                .on_press_maybe(
                    conditions_enabled
                        .then_some(Message::BuilderPanelToggled(BuilderPanel::Conditions)),
                )
                .padding(spacing::XS)
                .style(if conditions_selected {
                    theme::primary_button
                } else {
                    theme::secondary_button
                }),
            if conditions_enabled {
                "Reaction conditions"
            } else {
                "Conditions unlock once a reactant is composed"
            },
        );

        let sketch_selected = self.builder_panel == Some(BuilderPanel::Sketch);
        let sketch = Self::toolbar_tooltip(
            button(icons::pencil(
                BUILDER_TOOLBAR_ICON_SIZE,
                if sketch_selected {
                    color::CANVAS
                } else {
                    color::TEXT_SOFT
                },
            ))
            .on_press(Message::BuilderPanelToggled(BuilderPanel::Sketch))
            .padding(spacing::XS)
            .style(if sketch_selected {
                theme::primary_button
            } else {
                theme::secondary_button
            }),
            "Draw a molecule",
        );

        let help_selected = self.builder_panel == Some(BuilderPanel::Help);
        let help = Self::toolbar_tooltip(
            button(icons::help(
                BUILDER_TOOLBAR_ICON_SIZE,
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
            }),
            "Help and shortcuts",
        );

        let settings = Self::toolbar_tooltip(
            button(icons::settings(BUILDER_TOOLBAR_ICON_SIZE, color::TEXT_SOFT))
                .on_press(Message::SettingsOpened)
                .padding(spacing::XS)
                .style(theme::secondary_button),
            "Settings",
        );

        #[cfg(target_arch = "wasm32")]
        let demo_disclaimer = {
            use iced::widget::{rich_text, span};
            row![
                icons::alert(16.0, color::MUTED),
                rich_text![
                    span("This is a demo of ChemSpec, ").color(color::MUTED),
                    span("download the app")
                        .color(color::ACCENT)
                        .underline(true)
                        .link(()),
                    span(" for the full experience, including integration with Codex.")
                        .color(color::MUTED),
                ]
                .on_link_click(|()| Message::DemoRepoLinkOpened)
                .size(type_scale::CAPTION),
            ]
            .spacing(spacing::XS)
            .align_y(Center)
        };
        #[cfg(not(target_arch = "wasm32"))]
        let demo_disclaimer = row![];

        row![
            demo_disclaimer,
            space().width(Fill),
            dice,
            sketch,
            conditions,
            help,
            settings,
        ]
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
                    text("A selected condition becomes part of the reaction request.")
                        .size(type_scale::CAPTION)
                        .color(color::MUTED),
                    button(text("No condition").size(type_scale::BODY))
                        .on_press(Message::Dynamic(
                            dynamic_reaction::Message::ContextSelected(None),
                        ))
                        .padding([spacing::XS, spacing::SM])
                        .width(Fill)
                        .style(if self.dynamic.context.is_none() {
                            theme::primary_button
                        } else {
                            theme::secondary_button
                        }),
                ]
                .spacing(spacing::XS);
                for context in DynamicRequestContext::ALL {
                    choices = choices.push(
                        button(text(context.label()).size(type_scale::BODY))
                            .on_press(Message::Dynamic(
                                dynamic_reaction::Message::ContextSelected(Some(context)),
                            ))
                            .padding([spacing::XS, spacing::SM])
                            .width(Fill)
                            .style(if self.dynamic.context == Some(context) {
                                theme::primary_button
                            } else {
                                theme::secondary_button
                            }),
                    );
                }
                choices.into()
            }
            Some(BuilderPanel::Sketch) => {
                let slot_available =
                    !matches!(self.dynamic.build, DynamicBuildState::Running { .. });
                sketcher::view(&self.sketcher, slot_available).map(Message::Sketcher)
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

        // The sketch canvas needs more room than the text panels.
        let width = if self.builder_panel == Some(BuilderPanel::Sketch) {
            420.0
        } else {
            340.0
        };
        container(content)
            .padding(spacing::MD)
            .width(Length::Fixed(width))
            .style(|_| theme::tooltip_surface(1.0))
            .into()
    }

    #[allow(clippy::too_many_lines)]
    fn product_summary_view(&self, size: Size) -> Element<'_, Message> {
        let Some(animation) = &self.structural_animation else {
            return Self::structural_unavailable_view("Validated product frames are unavailable");
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
            .on_press(Message::StartNewReaction)
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
                text(nomenclature::display_declaration(
                    &animation.declaration,
                    self.settings.chemical_labels,
                ))
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
                    self.settings.chemical_labels,
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

        let properties = product_properties_view(
            &summary,
            elapsed_ms,
            self.settings.chemical_labels,
            compact,
            dense,
        );
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
                    text("SOURCE · CURRENT .CHEMS + VALIDATED FRAME + ELEMENT REFERENCE")
                        .size(type_scale::MICRO)
                        .color(color::ACCENT),
                ]
                .align_y(Center),
            ]
            .spacing(spacing::SM)
            .height(Fill),
        )
        .style(theme::app_background)
        .padding(chromeless_page_padding(spacing::SM, spacing::LG))
        .width(Fill)
        .height(Fill)
        .into()
    }

    /// Stage 1: the question sentence above the full periodic table, with no
    /// chrome competing for attention.
    #[allow(clippy::too_many_lines)]
    fn builder_view(&self, size: Size) -> Element<'_, Message> {
        let compact = size.width < breakpoint::MOBILE;
        let composer = reactant_composer::view(
            &self.reactant_composer,
            periodic_table::dragging_atomic_number(&self.periodic_table),
            self.local_mode(),
            self.settings.chemical_labels,
            compact,
        )
        .map(Message::ReactantComposer);
        let ambient_models =
            reactant_composer::ambient_view(&self.reactant_composer).map(Message::ReactantComposer);
        let element_library =
            periodic_table::view(&self.periodic_table, compact).map(Message::PeriodicTable);
        let library = container(element_library).width(Fill).height(Fill);

        let modal_open = self.dynamic_modal_kind().is_some();
        let (first, second) = reactant_composer::reactants(&self.reactant_composer);
        let conditions_enabled = !modal_open && (!first.is_empty() || !second.is_empty());
        let toolbar = self.builder_toolbar(conditions_enabled);
        let foreground = column![toolbar, composer, library]
            .spacing(spacing::XS)
            .width(Fill)
            .height(Fill);
        let page_padding =
            chromeless_page_padding(if compact { spacing::XS } else { spacing::SM }, spacing::XS);
        let application = container(stack![ambient_models, foreground].width(Fill).height(Fill))
            .style(theme::app_background)
            .padding(page_padding)
            .width(Fill)
            .height(Fill);
        let overlay = match self.builder_overlay_kind() {
            BuilderOverlayKind::Dynamic(kind) => self.dynamic_overlay(size, kind),
            BuilderOverlayKind::Toolbar => {
                container(row![space().width(Fill), self.builder_toolbar_panel()].width(Fill))
                    .padding(iced::Padding {
                        top: builder_toolbar_panel_top(page_padding.top),
                        right: page_padding.right,
                        bottom: 0.0,
                        left: page_padding.left,
                    })
                    .width(Fill)
                    .height(Fill)
                    .into()
            }
            BuilderOverlayKind::Drag => {
                periodic_table::drag_overlay(&self.periodic_table, size).map(Message::PeriodicTable)
            }
            BuilderOverlayKind::None => space().height(Length::Shrink).into(),
        };

        stack![application, overlay]
            .width(Fill)
            .height(Fill)
            .clip(false)
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chemistry::{HeavyAlkaliMetal, ReactionRequest};
    use agent::{compile_claim_outcome, compile_claim_outcome_with_catalogue};

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

    fn validated_oxide_appearance(
        request: &OxideAppearanceRequest,
        colour_family: agent::OxideColourFamily,
    ) -> ValidatedOxideAppearance {
        let claim = agent::OxideAppearanceClaim {
            schema_version: agent::OXIDE_APPEARANCE_SCHEMA_VERSION,
            product_binding: request.product_binding.clone(),
            product_structure_id: request.product_structure_id.clone(),
            product_formula: request.product_formula.clone(),
            catalogue_digest: request.catalogue_digest,
            colour_family,
            representative_condition: "Representative dry solid at ordinary ambient conditions"
                .to_owned(),
            sources: vec![agent::AppearanceSource {
                title: "Reviewed oxide reference".to_owned(),
                publisher: "Reference publisher".to_owned(),
                url: "https://example.org/oxide-reference".to_owned(),
                supporting_excerpt:
                    "The reference directly describes the representative solid colour.".to_owned(),
            }],
            limitations: "Colour can vary with impurities and physical form.".to_owned(),
        };
        agent::OxideAppearanceClaim::from_json_for(
            &serde_json::to_vec(&claim).expect("claim JSON"),
            request,
        )
        .expect("claim is exactly bound")
    }

    // Independently authored UI fixtures. These deliberately do not use
    // ReactionRequest::participants(), so a wrong production mapping cannot
    // make the routing test prove itself.
    const SUPPORTED_DRAFT_CASES: [DraftCase; 39] = [
        ("alkali-water-lithium", &[3], &[1, 1, 8]),
        ("alkali-water-sodium", &[11], &[1, 1, 8]),
        ("alkali-water-potassium", &[19], &[1, 1, 8]),
        ("alkali-water-rubidium", &[37], &[1, 1, 8]),
        ("alkali-water-caesium", &[55], &[1, 1, 8]),
        ("alkali-water-francium", &[87], &[1, 1, 8]),
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
        let catalogue = chemistry::reference_catalogue().expect("reference catalogue");
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
        let claim = ProviderClaim::from_json(
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

    fn dynamic_methane_static() -> ValidatedStaticOutcome {
        let catalogue = chemistry::reference_catalogue().expect("reference catalogue");
        let identities = reviewed_species_registry(catalogue).expect("identities");
        let claim = serde_json::json!({
            "schema_version": 1,
            "disposition": "reaction",
            "products": [
                {"name":"carbon dioxide","formula":"CO2","phase":"gas","identity_hints":[]},
                {"name":"water","formula":"H2O","phase":"gas","identity_hints":[]}
            ],
            "required_context":"complete combustion in oxygen",
            "observations":[
                {"predicate":"forms","subject":"carbon dioxide and water vapour","value":null}
            ],
            "sources":[],
            "ambiguity":null
        });
        let claim = ProviderClaim::from_json(
            &serde_json::to_vec(&claim).expect("claim JSON"),
            ClaimMode::Fast,
        )
        .expect("claim");
        let request = ReactionBuildRequest {
            reactants: [
                ReactantInput {
                    display: "CH4".into(),
                    atomic_numbers: vec![6, 1, 1, 1, 1],
                    species_id: None,
                },
                ReactantInput {
                    display: "O2".into(),
                    atomic_numbers: vec![8, 8],
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

    fn dynamic_iron_oxide_presentation() -> DynamicPresentationOutcome {
        let catalogue = chemistry::reference_catalogue().expect("reference catalogue");
        let identities = reviewed_species_registry(catalogue).expect("identities");
        let request = ReactionBuildRequest {
            reactants: vec![
                ReactantInput {
                    display: "Fe".into(),
                    atomic_numbers: vec![26],
                    species_id: None,
                },
                ReactantInput {
                    display: "O2".into(),
                    atomic_numbers: vec![8, 8],
                    species_id: None,
                },
            ],
            selected_context: None,
        };
        let claim = serde_json::json!({
            "schema_version": 1,
            "disposition": "reaction",
            "products": [
                {
                    "name": "iron(III) oxide",
                    "formula": "Fe2O3",
                    "phase": "solid",
                    "identity_hints": []
                }
            ],
            "required_context": "representative theoretical oxidation outcome selected by the reviewed oxygen catalogue",
            "observations": [
                {"predicate": "forms", "subject": "solid iron(III) oxide", "value": null}
            ],
            "sources": [],
            "ambiguity": null
        });
        let claim = ProviderClaim::from_json(
            &serde_json::to_vec(&claim).expect("claim JSON"),
            ClaimMode::Fast,
        )
        .expect("claim");
        let CompiledClaimOutcome::Static(outcome) =
            compile_claim_outcome_with_catalogue(&request, claim, &identities, catalogue)
                .expect("iron oxide outcome compiles")
        else {
            panic!("iron oxidation is a static outcome")
        };
        assert_eq!(
            outcome.macroscopic_process(),
            Some(AgentMacroscopicProcess::SurfaceOxidation)
        );
        let mut provider = agent::UnsupportedMechanismProvider;
        enrich_static_outcome(outcome, catalogue, &mut provider)
            .expect("surface oxidation animation is derived")
    }

    fn dynamic_incomplete_methane_static() -> ValidatedStaticOutcome {
        let catalogue = chemistry::reference_catalogue().expect("reference catalogue");
        let identities = reviewed_species_registry(catalogue).expect("identities");
        let request = ReactionBuildRequest {
            reactants: [
                ReactantInput {
                    display: "CH4".into(),
                    atomic_numbers: vec![6, 1, 1, 1, 1],
                    species_id: None,
                },
                ReactantInput {
                    display: "O2".into(),
                    atomic_numbers: vec![8, 8],
                    species_id: None,
                },
            ]
            .to_vec(),
            selected_context: Some("limited oxygen".to_owned()),
        };
        let claim = agent::solve_reaction_claim(&request, &identities)
            .expect("limited-oxygen methane combustion solves");
        let CompiledClaimOutcome::Static(outcome) =
            compile_claim_outcome(&request, claim, &identities).expect("compiled")
        else {
            panic!("static outcome")
        };
        outcome
    }

    fn dynamic_copper_oxide_neutralisation_static() -> ValidatedStaticOutcome {
        let catalogue = chemistry::reference_catalogue().expect("catalogue");
        let identities = reviewed_species_registry(catalogue).expect("identities");
        let request = ReactionBuildRequest {
            reactants: vec![
                ReactantInput {
                    display: "CuO".to_owned(),
                    atomic_numbers: vec![29, 8],
                    species_id: None,
                },
                ReactantInput {
                    display: "H2SO4".to_owned(),
                    atomic_numbers: vec![1, 1, 16, 8, 8, 8, 8],
                    species_id: None,
                },
            ],
            selected_context: None,
        };
        let claim =
            agent::solve_reaction_claim(&request, &identities).expect("neutralisation solves");
        let CompiledClaimOutcome::Static(outcome) =
            compile_claim_outcome(&request, claim, &identities).expect("outcome compiles")
        else {
            panic!("neutralisation is static")
        };
        outcome
    }

    fn no_reaction_claim() -> ReactionClaim {
        let claim = serde_json::json!({
            "schema_version": 1,
            "disposition": "no_reaction",
            "products": [],
            "required_context": "Ordinary contact",
            "observations": [],
            "sources": [],
            "ambiguity": null
        });
        ProviderClaim::from_json(
            &serde_json::to_vec(&claim).expect("claim JSON"),
            ClaimMode::Fast,
        )
        .expect("no-reaction claim")
        .into_claim()
    }

    #[test]
    fn dynamic_provenance_label_follows_claim_provenance_not_selected_mode() {
        let catalogue = chemistry::reference_catalogue().expect("reference catalogue");
        let identities = reviewed_species_registry(catalogue).expect("identities");
        let request = ReactionBuildRequest {
            reactants: vec![
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
            ],
            selected_context: None,
        };
        let solved = agent::solve_reaction_claim(&request, &identities).expect("solved claim");
        let CompiledClaimOutcome::Static(derived) =
            compile_claim_outcome(&request, solved, &identities).expect("derived outcome")
        else {
            panic!("static derived outcome")
        };
        let mut app = App {
            provider: Some(AppMode::CodexBinary),
            ..App::default()
        };
        app.dynamic.static_outcome = Some(derived);
        assert_eq!(app.dynamic_provenance_label(), "DERIVED");

        app.provider = Some(AppMode::Local);
        app.dynamic.static_outcome = Some(dynamic_lithium_static());
        assert_eq!(app.dynamic_provenance_label(), "MODEL ASSERTED");
    }

    #[test]
    fn local_mode_is_preselected_and_continues_without_codex() {
        let mut app = App::default();
        assert_eq!(app.provider, Some(AppMode::Local));
        assert_eq!(app.screen, Screen::ProviderSetup);
        app.update(Message::ProviderContinue);
        assert_eq!(app.screen, Screen::ProviderSetup);
        app.update(Message::SettingsSaveFinished {
            save_id: 1,
            destination: SettingsSaveDestination::FirstLaunch,
            settings: AppSettings::default(),
            result: Ok(()),
        });
        assert_eq!(app.screen, Screen::Builder);
    }

    #[test]
    fn dynamic_combustion_compiles_phase_aware_flame_vapour_and_gas_reactants() {
        let catalogue = chemistry::reference_catalogue().expect("reference catalogue");
        let mut provider = agent::UnsupportedMechanismProvider;
        let presentation =
            enrich_static_outcome(dynamic_methane_static(), catalogue, &mut provider)
                .expect("combustion animation");
        let (outcome, frames): (&ValidatedStaticOutcome, &chem_kernel::SimulationFrames) =
            match &presentation {
                DynamicPresentationOutcome::ReviewedFamily(animated) => {
                    (animated.static_outcome(), animated.frames())
                }
                DynamicPresentationOutcome::Escalated(animated) => {
                    (animated.static_outcome(), animated.frames())
                }
                DynamicPresentationOutcome::Static { diagnostic, .. } => {
                    panic!("combustion should animate: {diagnostic}")
                }
            };
        let profile = dynamic_presentation_profile(frames, outcome, None)
            .expect("phase-driven combustion profile");

        let assembly = profile
            .objects
            .iter()
            .find(|object| {
                object.role == chem_presentation::SceneRole::Vessel
                    && object.asset == chem_presentation::AssetProfile::CompleteCombustionAssembly
            })
            .expect("validated complete combustion selects its authored assembly");
        assert_eq!(
            assembly.appearance,
            chem_presentation::AppearanceProfile::ReviewedColour(
                chem_presentation::hydrocarbon_fuel_colour(1)
            )
        );
        assert!(profile.objects.iter().any(|object| {
            object.role == chem_presentation::SceneRole::Reactant
                && object.asset == chem_presentation::AssetProfile::GasCloud
        }));
        assert!(profile.effects.iter().any(|effect| {
            matches!(
                effect.effect,
                EffectProfile::FlameEmitter(chem_presentation::FlamePalette::Natural)
            )
        }));
        assert!(
            profile
                .effects
                .iter()
                .any(|effect| effect.effect == EffectProfile::VapourRelease)
        );
        compile_real_world_plan(frames, &profile)
            .expect("typed process effects cross macroscopic plan validation");
    }

    #[test]
    fn carbon_monoxide_product_selects_incomplete_combustion_assembly() {
        let catalogue = chemistry::reference_catalogue().expect("reference catalogue");
        let mut provider = agent::UnsupportedMechanismProvider;
        let incomplete = dynamic_incomplete_methane_static();
        let presentation = enrich_static_outcome(incomplete, catalogue, &mut provider)
            .expect("incomplete combustion animation");
        let (outcome, frames): (&ValidatedStaticOutcome, &chem_kernel::SimulationFrames) =
            match &presentation {
                DynamicPresentationOutcome::ReviewedFamily(animated) => {
                    (animated.static_outcome(), animated.frames())
                }
                DynamicPresentationOutcome::Escalated(animated) => {
                    (animated.static_outcome(), animated.frames())
                }
                DynamicPresentationOutcome::Static { diagnostic, .. } => {
                    panic!("incomplete combustion should animate: {diagnostic}")
                }
            };
        let profile = dynamic_presentation_profile(frames, outcome, None)
            .expect("phase-driven incomplete combustion profile");
        assert_eq!(
            outcome.macroscopic_process(),
            Some(AgentMacroscopicProcess::IncompleteCombustion)
        );
        assert!(profile.objects.iter().any(|object| {
            object.role == chem_presentation::SceneRole::Vessel
                && object.asset == chem_presentation::AssetProfile::IncompleteCombustionAssembly
        }));
        assert!(!profile.objects.iter().any(|object| {
            object.asset == chem_presentation::AssetProfile::CompleteCombustionAssembly
        }));
        compile_real_world_plan(frames, &profile)
            .expect("typed incomplete-combustion process crosses plan validation");
    }

    #[test]
    fn dynamic_neutralisation_selects_shared_assembly_and_copper_colour() {
        let catalogue = chemistry::reference_catalogue().expect("reference catalogue");
        let mut provider = agent::UnsupportedMechanismProvider;
        let presentation = enrich_static_outcome(
            dynamic_copper_oxide_neutralisation_static(),
            catalogue,
            &mut provider,
        )
        .expect("neutralisation animation");
        let (outcome, frames): (&ValidatedStaticOutcome, &chem_kernel::SimulationFrames) =
            match &presentation {
                DynamicPresentationOutcome::ReviewedFamily(animated) => {
                    (animated.static_outcome(), animated.frames())
                }
                DynamicPresentationOutcome::Escalated(animated) => {
                    (animated.static_outcome(), animated.frames())
                }
                DynamicPresentationOutcome::Static { diagnostic, .. } => {
                    panic!("neutralisation should animate: {diagnostic}")
                }
            };
        let profile = dynamic_presentation_profile(frames, outcome, None)
            .expect("phase-driven neutralisation profile");
        assert!(profile.objects.iter().any(|object| {
            object.role == chem_presentation::SceneRole::Vessel
                && object.asset
                    == chem_presentation::AssetProfile::NeutralisationEvaporationAssembly
        }));
        assert!(profile.objects.iter().any(|object| {
            object.role == chem_presentation::SceneRole::Contents
                && matches!(
                    object.appearance,
                    chem_presentation::AppearanceProfile::ReviewedColour(VisualColour {
                        red: 0x63,
                        green: 0x9d,
                        blue: 0xd0,
                    })
                )
        }));
        assert_eq!(
            profile.post_process,
            Some(MacroscopicProcess::SolventEvaporationCrystallization)
        );
        compile_real_world_plan(frames, &profile)
            .expect("typed neutralisation process crosses plan validation");
    }

    #[test]
    fn generated_precipitations_select_shared_assembly_and_structural_colours() {
        let catalogue = chemistry::reference_catalogue().expect("reference catalogue");
        let identities = reviewed_species_registry(catalogue).expect("identities");
        let cases = [
            (
                [
                    ("CuSO4", vec![29, 16, 8, 8, 8, 8]),
                    ("NaOH", vec![11, 8, 1]),
                ],
                VisualColour {
                    red: 0x4f,
                    green: 0x92,
                    blue: 0xd0,
                },
            ),
            (
                [
                    ("Pb(NO3)2", vec![82, 7, 8, 8, 8, 7, 8, 8, 8]),
                    ("KI", vec![19, 53]),
                ],
                VisualColour {
                    red: 0xef,
                    green: 0xd1,
                    blue: 0x47,
                },
            ),
        ];

        for (reactants, expected_precipitate_colour) in cases {
            let request = ReactionBuildRequest {
                reactants: reactants
                    .into_iter()
                    .map(|(display, atomic_numbers)| ReactantInput {
                        display: display.to_owned(),
                        atomic_numbers,
                        species_id: None,
                    })
                    .collect(),
                selected_context: None,
            };
            let claim =
                agent::solve_reaction_claim_with_catalogue(&request, &identities, catalogue)
                    .unwrap_or_else(|| {
                        panic!(
                            "precipitation solves for {} + {}",
                            request.reactants[0].display, request.reactants[1].display
                        )
                    });
            let CompiledClaimOutcome::Static(outcome) =
                compile_claim_outcome_with_catalogue(&request, claim, &identities, catalogue)
                    .expect("outcome compiles")
            else {
                panic!("precipitation is static");
            };
            assert_eq!(
                outcome.macroscopic_process(),
                Some(AgentMacroscopicProcess::AqueousPrecipitation)
            );
            let mut provider = agent::UnsupportedMechanismProvider;
            let presentation = enrich_static_outcome(outcome, catalogue, &mut provider)
                .expect("precipitation animation");
            let (outcome, frames): (&ValidatedStaticOutcome, &chem_kernel::SimulationFrames) =
                match &presentation {
                    DynamicPresentationOutcome::ReviewedFamily(animated) => {
                        (animated.static_outcome(), animated.frames())
                    }
                    DynamicPresentationOutcome::Escalated(animated) => {
                        (animated.static_outcome(), animated.frames())
                    }
                    DynamicPresentationOutcome::Static { diagnostic, .. } => {
                        panic!("precipitation should animate: {diagnostic}")
                    }
                };
            let profile = dynamic_presentation_profile(frames, outcome, None)
                .expect("phase-driven precipitation profile");
            assert!(profile.objects.iter().any(|object| {
                object.role == chem_presentation::SceneRole::Vessel
                    && object.asset == chem_presentation::AssetProfile::AqueousPrecipitationAssembly
            }));
            assert_eq!(
                profile
                    .precipitation
                    .as_ref()
                    .expect("precipitation material bindings")
                    .precipitate
                    .colour,
                expected_precipitate_colour
            );
            assert!(profile.effects.iter().any(|effect| {
                effect.effect == EffectProfile::PrecipitateFormation
                    && effect.authorization
                        == chem_presentation::EffectAuthorization::Process(
                            MacroscopicProcess::AqueousPrecipitation,
                        )
            }));
            compile_real_world_plan(frames, &profile)
                .expect("typed precipitation process crosses plan validation");
        }
    }

    #[test]
    fn generated_gas_evolution_examples_select_supported_authored_variants() {
        let catalogue = chemistry::reference_catalogue().expect("reference catalogue");
        let identities = reviewed_species_registry(catalogue).expect("identities");
        let cases = [
            (
                [("NaHCO3", vec![11, 1, 6, 8, 8, 8]), ("HCl", vec![1, 17])],
                chem_presentation::GasEvolutionVariant::LiquidLiquid,
            ),
            (
                [("Mg", vec![12]), ("HCl", vec![1, 17])],
                chem_presentation::GasEvolutionVariant::SolidLiquid,
            ),
            (
                [("CaCO3", vec![20, 6, 8, 8, 8]), ("HCl", vec![1, 17])],
                chem_presentation::GasEvolutionVariant::SolidLiquid,
            ),
            (
                [("FeS", vec![26, 16]), ("HCl", vec![1, 17])],
                chem_presentation::GasEvolutionVariant::SolidLiquid,
            ),
            (
                [("Zn", vec![30]), ("H2SO4", vec![1, 1, 16, 8, 8, 8, 8])],
                chem_presentation::GasEvolutionVariant::SolidLiquid,
            ),
        ];

        for (reactants, expected_variant) in cases {
            let request = ReactionBuildRequest {
                reactants: reactants
                    .into_iter()
                    .map(|(display, atomic_numbers)| ReactantInput {
                        display: display.to_owned(),
                        atomic_numbers,
                        species_id: None,
                    })
                    .collect(),
                selected_context: None,
            };
            let claim =
                agent::solve_reaction_claim_with_catalogue(&request, &identities, catalogue)
                    .unwrap_or_else(|| {
                        panic!(
                            "gas evolution solves for {} + {}",
                            request.reactants[0].display, request.reactants[1].display
                        )
                    });
            let CompiledClaimOutcome::Static(outcome) =
                compile_claim_outcome_with_catalogue(&request, claim, &identities, catalogue)
                    .expect("gas-evolution outcome compiles")
            else {
                panic!("gas evolution is static");
            };
            let expected_process = match expected_variant {
                chem_presentation::GasEvolutionVariant::LiquidLiquid => {
                    AgentMacroscopicProcess::GasEvolutionLiquidLiquid
                }
                chem_presentation::GasEvolutionVariant::SolidLiquid => {
                    AgentMacroscopicProcess::GasEvolutionSolidLiquid
                }
            };
            assert_eq!(outcome.macroscopic_process(), Some(expected_process));

            let mut provider = agent::UnsupportedMechanismProvider;
            let presentation = enrich_static_outcome(outcome, catalogue, &mut provider)
                .expect("gas-evolution animation");
            let (outcome, frames): (&ValidatedStaticOutcome, &chem_kernel::SimulationFrames) =
                match &presentation {
                    DynamicPresentationOutcome::ReviewedFamily(animated) => {
                        (animated.static_outcome(), animated.frames())
                    }
                    DynamicPresentationOutcome::Escalated(animated) => {
                        (animated.static_outcome(), animated.frames())
                    }
                    DynamicPresentationOutcome::Static { diagnostic, .. } => {
                        panic!("gas evolution should animate: {diagnostic}")
                    }
                };
            let profile = dynamic_presentation_profile(frames, outcome, None)
                .expect("phase-driven gas-evolution profile");
            assert_eq!(
                profile.gas_evolution.as_ref().map(|visual| visual.variant),
                Some(expected_variant)
            );
            compile_real_world_plan(frames, &profile)
                .expect("typed gas-evolution process crosses plan validation");
        }
    }

    #[test]
    fn local_mode_suggests_codex_immediately_when_the_solver_declines() {
        let mut app = App {
            screen: Screen::Builder,
            ..App::default()
        };
        reactant_composer::replace_reactants(&mut app.reactant_composer, [vec![12], vec![1, 1, 8]]);

        app.sync_builder_submit_prompt();

        assert!(!app.builder_can_submit());
        assert!(reactant_composer::try_codex_notice_visible(
            &app.reactant_composer
        ));
    }

    #[test]
    fn local_mode_keeps_programmatically_solvable_misses_actionable() {
        let mut app = App {
            screen: Screen::Builder,
            ..App::default()
        };
        reactant_composer::replace_reactants(&mut app.reactant_composer, [vec![20], vec![1, 1, 8]]);

        app.sync_builder_submit_prompt();

        assert!(app.builder_can_submit());
        assert!(!reactant_composer::try_codex_notice_visible(
            &app.reactant_composer
        ));
    }

    #[test]
    fn closing_a_dynamic_overlay_restores_the_codex_prompt() {
        let mut app = App {
            screen: Screen::Builder,
            provider: Some(AppMode::CodexBinary),
            dynamic: dynamic_reaction::State {
                build: DynamicBuildState::Failed("test failure".to_owned().into()),
                overlay_dismissed: false,
                ..dynamic_reaction::State::default()
            },
            ..App::default()
        };
        reactant_composer::replace_reactants(&mut app.reactant_composer, [vec![12], vec![1, 1, 8]]);
        reactant_composer::set_submit_available(&mut app.reactant_composer, false);

        app.update(Message::Dynamic(
            dynamic_reaction::Message::OverlayDismissed,
        ));

        assert!(app.dynamic.overlay_dismissed);
        assert!(reactant_composer::submit_available(&app.reactant_composer));
    }

    #[test]
    fn dynamic_modal_exclusively_owns_builder_overlays_and_input() {
        let mut app = App {
            screen: Screen::Builder,
            builder_panel: Some(BuilderPanel::Help),
            dynamic: dynamic_reaction::State {
                build: DynamicBuildState::Failed("test failure".to_owned().into()),
                ..dynamic_reaction::State::default()
            },
            ..App::default()
        };
        reactant_composer::replace_reactants(&mut app.reactant_composer, [vec![26], vec![3]]);

        app.open_dynamic_overlay();

        assert_eq!(
            app.builder_overlay_kind(),
            BuilderOverlayKind::Dynamic(DynamicModalKind::Failed)
        );
        assert!(app.builder_panel.is_none());
        assert!(!app.builder_can_submit());
        assert!(!reactant_composer::submit_available(&app.reactant_composer));

        app.update(Message::ReactantComposer(
            reactant_composer::Message::AddElement(8),
        ));
        assert_eq!(
            reactant_composer::reactants(&app.reactant_composer),
            (&[26][..], &[3][..])
        );
    }

    #[test]
    fn dynamic_metal_displacement_reaches_the_generic_authored_scene() {
        let catalogue = chemistry::reference_catalogue().expect("reference catalogue");
        let identities = reviewed_species_registry(catalogue).expect("identities");
        let request = ReactionBuildRequest {
            reactants: vec![
                ReactantInput {
                    display: "Zn".to_owned(),
                    atomic_numbers: vec![30],
                    species_id: None,
                },
                ReactantInput {
                    display: "CuSO4".to_owned(),
                    atomic_numbers: vec![29, 16, 8, 8, 8, 8],
                    species_id: None,
                },
            ],
            selected_context: None,
        };
        let claim =
            agent::solve_reaction_claim(&request, &identities).expect("displacement solves");
        let CompiledClaimOutcome::Static(outcome) =
            compile_claim_outcome_with_catalogue(&request, claim, &identities, catalogue)
                .expect("displacement compiles")
        else {
            panic!("displacement is a static outcome")
        };
        assert_eq!(
            outcome.macroscopic_process(),
            Some(AgentMacroscopicProcess::MetalDisplacement)
        );
        let mut provider = agent::UnsupportedMechanismProvider;
        let presentation = enrich_static_outcome(outcome, catalogue, &mut provider)
            .expect("displacement animation is derived");
        let (outcome, frames): (&ValidatedStaticOutcome, &chem_kernel::SimulationFrames) =
            match &presentation {
                DynamicPresentationOutcome::ReviewedFamily(animated) => {
                    (animated.static_outcome(), animated.frames())
                }
                DynamicPresentationOutcome::Escalated(animated) => {
                    (animated.static_outcome(), animated.frames())
                }
                DynamicPresentationOutcome::Static { diagnostic, .. } => {
                    panic!("displacement should animate: {diagnostic}")
                }
            };
        let profile =
            dynamic_presentation_profile(frames, outcome, None).expect("presentation compiles");
        assert!(profile.precipitation.is_none());
        assert!(profile.gas_evolution.is_none());
        let displacement = profile
            .metal_displacement
            .as_ref()
            .expect("metal displacement has exact material bindings");
        assert_eq!(
            displacement.deposited_metal.colour,
            VisualColour {
                red: 0xb8,
                green: 0x6a,
                blue: 0x47,
            }
        );
        assert_ne!(
            displacement.original_metal.colour,
            displacement.deposited_metal.colour
        );
        assert!(profile.objects.iter().any(|object| {
            object.role == chem_presentation::SceneRole::Vessel
                && object.asset == chem_presentation::AssetProfile::MetalDisplacementAssembly
        }));
        let plan = compile_real_world_plan(frames, &profile).expect("scene plan compiles");
        assert_eq!(plan.timeline.duration_ms(), 9_600);
    }

    #[test]
    fn catalogue_aware_dynamic_heavy_alkali_outcomes_reach_the_local_assembly() {
        let catalogue = chemistry::reference_catalogue().expect("reference catalogue");
        let identities = reviewed_species_registry(catalogue).expect("identities");
        let cases = [
            (
                ReactionRequest::heavy_alkali_water(HeavyAlkaliMetal::Rubidium),
                "RubidiumMetal",
                "Rb",
                37,
            ),
            (
                ReactionRequest::heavy_alkali_water(HeavyAlkaliMetal::Caesium),
                "CaesiumMetal",
                "Cs",
                55,
            ),
            (
                ReactionRequest::heavy_alkali_water(HeavyAlkaliMetal::Francium),
                "FranciumMetal",
                "Fr",
                87,
            ),
        ];
        for (local_request, display, symbol, atomic_number) in cases {
            let request = ReactionBuildRequest {
                reactants: vec![
                    ReactantInput {
                        display: display.to_owned(),
                        atomic_numbers: vec![atomic_number],
                        species_id: None,
                    },
                    ReactantInput {
                        display: "water".to_owned(),
                        atomic_numbers: vec![1, 1, 8],
                        species_id: None,
                    },
                ],
                selected_context: None,
            };
            let claim = ProviderClaim::from_json(
                &serde_json::to_vec(&serde_json::json!({
                    "schema_version": 1,
                    "disposition": "reaction",
                    "products": [
                        {"name":"aqueous hydroxide", "formula":format!("{symbol}OH"), "phase":"aqueous", "identity_hints":[]},
                        {"name":"hydrogen", "formula":"H2", "phase":"gas", "identity_hints":[]}
                    ],
                    "required_context":"representative educational outcome",
                    "observations": [],
                    "sources": [],
                    "ambiguity": null
                }))
                .expect("provider claim JSON"),
                ClaimMode::Fast,
            )
            .expect("provider claim");
            let CompiledClaimOutcome::Static(outcome) =
                compile_claim_outcome_with_catalogue(&request, claim, &identities, catalogue)
                    .expect("catalogue-aware dynamic outcome")
            else {
                panic!("heavy alkali water is a static outcome")
            };
            assert!(matches!(
                outcome.macroscopic_process(),
                Some(AgentMacroscopicProcess::ExplosiveMetalWater(_))
            ));
            let dynamic =
                dynamic_macroscopic_reaction(&outcome).expect("typed material projection");
            let local_run = chemistry::run(local_request).expect("local request validates");
            let local = local_run.macroscopic().expect("local material projection");
            assert_eq!(dynamic.process, local.process);
            assert!(matches!(
                dynamic.process,
                Some(MacroscopicProcess::ExplosiveMetalWater(_))
            ));
        }
    }

    #[test]
    fn dynamic_completion_replaces_a_dismissed_toolbar_panel() {
        let mut app = App {
            screen: Screen::Builder,
            builder_panel: Some(BuilderPanel::Help),
            dynamic: dynamic_reaction::State {
                build: DynamicBuildState::Running {
                    run_id: 9,
                    elapsed_seconds: 2,
                    stage: DynamicBuildStage::Claim,
                },
                overlay_dismissed: true,
                ..dynamic_reaction::State::default()
            },
            ..App::default()
        };

        let provider_error = ProviderClaim::from_json(b"{}", ClaimMode::Fast)
            .expect_err("an empty provider claim must be rejected");
        app.update(Message::Dynamic(dynamic_reaction::Message::ClaimFinished {
            run_id: 9,
            result: Box::new(Err(provider_error.into())),
        }));

        assert!(app.builder_panel.is_none());
        assert_eq!(
            app.builder_overlay_kind(),
            BuilderOverlayKind::Dynamic(DynamicModalKind::Failed)
        );
        let DynamicBuildState::Failed(DynamicBuildFailure::Agent(error)) = &app.dynamic.build
        else {
            panic!("provider errors remain typed in application state")
        };
        assert_eq!(error.kind(), agent::AgentErrorKind::InvalidProviderOutput);
    }

    #[test]
    fn dynamic_modal_keyboard_route_only_accepts_escape() {
        assert!(
            dynamic_modal_keyboard_message(key_pressed(
                iced::keyboard::Key::Named(iced::keyboard::key::Named::Space),
                false,
            ))
            .is_none()
        );
        assert!(matches!(
            dynamic_modal_keyboard_message(key_pressed(
                iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                false,
            )),
            Some(Message::Dynamic(
                dynamic_reaction::Message::OverlayDismissed
            ))
        ));
    }

    #[test]
    fn no_reaction_modal_is_not_reported_as_idle() {
        let mut app = App {
            screen: Screen::Builder,
            dynamic: dynamic_reaction::State {
                claim: Some(no_reaction_claim()),
                ..dynamic_reaction::State::default()
            },
            ..App::default()
        };
        reactant_composer::replace_reactants(&mut app.reactant_composer, [vec![26], vec![3]]);

        assert_eq!(
            app.builder_accessibility_summary().as_deref(),
            Some("Reactants Fe + Li; outcome modal open: no reaction")
        );
    }

    #[test]
    fn uncatalogued_reaction_starts_generation_scoped_codex_build() {
        let mut app = App {
            provider: Some(AppMode::CodexBinary),
            ..App::default()
        };
        reactant_composer::replace_reactants(&mut app.reactant_composer, [vec![20], vec![1, 1, 8]]);

        app.update(Message::ReactantComposer(
            reactant_composer::Message::StartReactionRequested,
        ));

        assert!(
            matches!(
                app.dynamic.build,
                DynamicBuildState::Running { run_id: 1, .. }
            ),
            "state: {:?}, identity choice: {:?}",
            app.dynamic.build,
            app.dynamic.identity_choice
        );
        assert!(app.validated_frames.is_none());
        assert!(app.dynamic.request.is_some());
        assert!(app.dynamic.identity_choice.is_none());
    }

    #[test]
    fn selected_conditions_never_fall_through_to_an_unconditioned_catalogue_result() {
        let mut app = App {
            screen: Screen::Builder,
            provider: Some(AppMode::CodexBinary),
            ..App::default()
        };
        reactant_composer::replace_reactants(&mut app.reactant_composer, [vec![3], vec![1, 1, 8]]);
        app.dynamic.context = Some(DynamicRequestContext::Heat);

        app.update(Message::ReactantComposer(
            reactant_composer::Message::StartReactionRequested,
        ));

        assert_eq!(app.screen, Screen::Builder);
        assert!(matches!(
            app.dynamic.build,
            DynamicBuildState::Running { .. }
        ));
        let request = app.dynamic.request.as_ref().expect("conditioned request");
        assert_eq!(request.selected_context.as_deref(), Some("heat"));
        assert_eq!(request.reactants.len(), 2);
    }

    #[test]
    fn conditioned_single_reactant_requests_ignore_either_empty_slot() {
        let mut app = App {
            screen: Screen::Builder,
            dynamic: dynamic_reaction::State {
                context: Some(DynamicRequestContext::Light),
                ..dynamic_reaction::State::default()
            },
            ..App::default()
        };
        reactant_composer::replace_reactants(
            &mut app.reactant_composer,
            [Vec::new(), vec![6, 1, 1, 1, 1]],
        );

        assert!(app.builder_input_ready());
        let request = app.dynamic_build_request();
        assert_eq!(request.reactants.len(), 1);
        assert_eq!(request.reactants[0].display, "CH₄");
        assert_eq!(request.selected_context.as_deref(), Some("light"));
    }

    #[test]
    fn sodium_chloride_electrolysis_crosses_the_reviewed_identity_path() {
        let catalogue = chemistry::reference_catalogue().expect("catalogue");
        let identities = reviewed_species_registry(catalogue).expect("identities");
        let mut request = ReactionBuildRequest {
            reactants: vec![ReactantInput {
                display: "NaCl".to_owned(),
                atomic_numbers: vec![11, 17],
                species_id: None,
            }],
            selected_context: Some("electricity".to_owned()),
        };
        let RequestIdentityResolution::Resolved(resolved) =
            resolve_request_identities_with_catalogue(&request, &identities, catalogue)
                .expect("reviewed resolution")
        else {
            panic!("NaCl should resolve to one reviewed identity")
        };
        let OutcomeSpecies::Resolved(species) = &resolved[0] else {
            panic!("NaCl should have a reviewed structure")
        };
        request.reactants[0].species_id = Some(species.id.clone());
        let claim = agent::solve_reaction_claim(&request, &identities)
            .expect("NaCl electrolysis should solve locally");
        assert_eq!(
            claim
                .products
                .iter()
                .map(|product| product.formula.as_str())
                .collect::<Vec<_>>(),
            ["NaOH", "H2", "Cl2"]
        );
        let outcome = compile_claim_outcome(&request, claim, &identities)
            .expect("NaCl electrolysis should balance");
        let CompiledClaimOutcome::Static(outcome) = outcome else {
            panic!("NaCl electrolysis should be static")
        };
        let mut provider = CodexProvider::new(CodexProviderConfig::from_environment());
        let presentation = enrich_static_outcome(outcome, catalogue, &mut provider)
            .expect("NaCl electrolysis should animate");
        let DynamicPresentationOutcome::Escalated(animated) = presentation else {
            panic!("NaCl electrolysis should use its algorithmic mechanism")
        };
        let plan = compile_educational_plan(animated.frames(), "electricity")
            .expect("NaCl electrolysis plan");
        let labels = plan
            .scenes
            .iter()
            .flat_map(|scene| &scene.cues)
            .filter_map(|cue| match cue {
                chem_presentation::EducationalCue::ShowContext { label } => {
                    Some(label.text.as_str())
                }
                chem_presentation::EducationalCue::ShowExplanation { label } => {
                    Some(label.text.as_str())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(labels.iter().any(|text| text.starts_with("Anode:")));
        assert!(labels.iter().any(|text| text.starts_with("Cathode:")));

        let mut app = App {
            provider: Some(AppMode::Local),
            screen: Screen::Builder,
            ..App::default()
        };
        app.dynamic.context = Some(DynamicRequestContext::Electricity);
        reactant_composer::replace_reactants(
            &mut app.reactant_composer,
            [vec![11, 17], Vec::new()],
        );
        assert!(app.builder_input_ready());
        assert!(!app.local_solver_declines());
        assert!(app.builder_can_submit());
    }
    /// Catalogue graphs pack all cations of one kind into a single atom
    /// group (both `Na+` of `Na2CO3` share one group), which used to defeat
    /// `ionic_salt`'s one-atom-per-cation-group assumption and return None
    /// for every polyprotic acid + carbonate pair.
    #[test]
    fn polyprotic_acids_neutralize_catalogue_carbonates_and_hydroxides() {
        type PolyproticCase = (&'static str, Vec<u8>, &'static str, Vec<u8>, &'static str);
        let catalogue = chemistry::reference_catalogue().expect("catalogue");
        let identities = agent::reviewed_species_registry(catalogue).expect("registry");
        let cases: [PolyproticCase; 5] = [
            // Monoprotic control: this already worked before the fix.
            (
                "HCl",
                vec![1, 17],
                "Na\u{2082}CO\u{2083}",
                vec![11, 11, 6, 8, 8, 8],
                "NaCl",
            ),
            (
                "H\u{2082}SO\u{2084}",
                vec![1, 1, 16, 8, 8, 8, 8],
                "Na\u{2082}CO\u{2083}",
                vec![11, 11, 6, 8, 8, 8],
                "Na2SO4",
            ),
            (
                "H\u{2082}SO\u{2084}",
                vec![1, 1, 16, 8, 8, 8, 8],
                "K\u{2082}CO\u{2083}",
                vec![19, 19, 6, 8, 8, 8],
                "K2SO4",
            ),
            (
                "H\u{2083}PO\u{2084}",
                vec![1, 1, 1, 15, 8, 8, 8, 8],
                "Na\u{2082}CO\u{2083}",
                vec![11, 11, 6, 8, 8, 8],
                "Na3PO4",
            ),
            (
                "H\u{2083}PO\u{2084}",
                vec![1, 1, 1, 15, 8, 8, 8, 8],
                "NaOH",
                vec![11, 8, 1],
                "Na3PO4",
            ),
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
            provider: Some(AppMode::Local),
            screen: Screen::Builder,
            ..App::default()
        };
        reactant_composer::replace_reactants(&mut app.reactant_composer, [vec![26], vec![17]]);
        app.update(Message::ReactantComposer(
            reactant_composer::Message::StartReactionRequested,
        ));
        assert!(matches!(
            app.dynamic.build,
            DynamicBuildState::Running { .. }
        ));
        app.update(Message::ReactantComposer(
            reactant_composer::Message::AnimationTick,
        ));
        app.update(Message::ReactantComposer(
            reactant_composer::Message::PromptAnimationTick,
        ));
        assert!(
            matches!(app.dynamic.build, DynamicBuildState::Running { .. }),
            "presentation ticks must not cancel a running build"
        );
        app.dynamic.build = DynamicBuildState::Idle;
        app.dynamic.static_outcome = Some(dynamic_lithium_static());
        app.update(Message::ReactantComposer(
            reactant_composer::Message::AnimationTick,
        ));
        assert!(
            app.dynamic.static_outcome.is_some(),
            "presentation ticks must not clear a finished result"
        );
        app.dynamic.overlay_dismissed = true;
        app.update(Message::ReactantComposer(
            reactant_composer::Message::AddElement(8),
        ));
        assert!(
            app.dynamic.static_outcome.is_none(),
            "a real draft edit still invalidates the result"
        );
    }

    #[test]
    fn derived_copper_displacement_plays_through_both_timelines() {
        // Fe + CuSO4 crashed mid-animation once (metallic acceptor with an
        // empty shell delta); the full derived pipeline must build and play
        // both timelines to completion.
        let catalogue = chemistry::reference_catalogue().expect("catalogue");
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
            provider: Some(AppMode::Local),
            screen: Screen::Builder,
            ..App::default()
        };
        app.dynamic.request = Some(request);
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
        let catalogue = chemistry::reference_catalogue().expect("catalogue");
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
            provider: Some(AppMode::CodexBinary),
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
            app.dynamic.build,
            DynamicBuildState::Running { run_id: 1, .. }
        ));
        let request = app.dynamic.request.as_ref().expect("dynamic request");
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
                provider: Some(AppMode::CodexBinary),
                ..App::default()
            };
            reactant_composer::replace_reactants(
                &mut app.reactant_composer,
                [vec![37], vec![atomic_number]],
            );

            let _ = app.start_dynamic_build();

            let request = app.dynamic.request.as_ref().expect("captured request");
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
            provider: Some(AppMode::CodexBinary),
            ..App::default()
        };
        reactant_composer::replace_reactants(&mut app.reactant_composer, [vec![37], vec![1]]);

        let _ = app.start_dynamic_build();

        assert!(app.dynamic.identity_choice.is_none());
        assert!(matches!(
            app.dynamic.build,
            DynamicBuildState::Running {
                stage: DynamicBuildStage::Claim,
                ..
            }
        ));
        assert_eq!(
            app.dynamic.request.as_ref().expect("request").reactants[1].atomic_numbers,
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
            Some(Message::StructuralPlaybackShortcut)
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
    fn structural_space_is_ignored_during_a_fresh_screen_transition() {
        let mut app = App::default();
        app.open_structural_animation();
        assert!(
            app.structural_animation
                .as_ref()
                .expect("animation")
                .playing
        );

        app.update(Message::StructuralPlaybackShortcut);
        assert!(
            app.structural_animation
                .as_ref()
                .expect("animation")
                .playing,
            "the builder submit key must not leak into 2D playback"
        );

        app.update(Message::StructuralTick);
        app.update(Message::StructuralPlaybackShortcut);
        assert!(
            app.structural_animation
                .as_ref()
                .expect("animation")
                .playing,
            "one scheduling tick must not arm a queued key"
        );

        while app.structural_shortcut_state != StructuralShortcutState::Ready {
            app.update(Message::StructuralTick);
        }
        app.update(Message::StructuralPlaybackShortcut);
        assert!(
            !app.structural_animation
                .as_ref()
                .expect("animation")
                .playing,
            "space must remain available after the transition settles"
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
        assert_eq!(app.provider, Some(AppMode::Local));

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
    fn smoke_window_title_exposes_builder_state_without_changing_initial_title() {
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
    fn main_window_title_stays_static_across_app_state() {
        let mut app = App::default();
        assert_eq!(app.title(), "ChemSpec");

        reactant_composer::replace_reactants(&mut app.reactant_composer, [vec![37], vec![1]]);
        app.screen = Screen::Structural2d;

        assert_eq!(app.title(), "ChemSpec");
    }

    #[test]
    fn static_completion_is_visible_before_presentation_enrichment() {
        let outcome = dynamic_lithium_static();
        let mut app = App {
            validated_frames: None,
            dynamic: dynamic_reaction::State {
                request: Some(ReactionBuildRequest {
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
                build: DynamicBuildState::Running {
                    run_id: 4,
                    elapsed_seconds: 2,
                    stage: DynamicBuildStage::Claim,
                },
                cancellation: Some(Arc::new(AtomicBool::new(false))),
                ..dynamic_reaction::State::default()
            },
            ..App::default()
        };

        app.update(Message::Dynamic(dynamic_reaction::Message::ClaimFinished {
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
        }));

        assert!(app.dynamic.static_outcome.is_some());
        assert_eq!(app.dynamic.latency.static_outcome_ms, Some(1_250));
        assert!(app.validated_frames.is_none());
        assert!(matches!(
            app.dynamic.build,
            DynamicBuildState::Running {
                run_id: 4,
                stage: DynamicBuildStage::Presentation,
                ..
            }
        ));

        app.update(Message::Dynamic(dynamic_reaction::Message::ClaimFinished {
            run_id: 4,
            result: Box::new(Err("duplicate completion".into())),
        }));
        assert!(app.dynamic.static_outcome.is_some());
        assert!(matches!(
            app.dynamic.build,
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
            dynamic: dynamic_reaction::State {
                static_outcome: Some(dynamic_lithium_static()),
                build: DynamicBuildState::Running {
                    run_id: 4,
                    elapsed_seconds: 0,
                    stage: DynamicBuildStage::Presentation,
                },
                ..dynamic_reaction::State::default()
            },
            ..App::default()
        };

        {
            let _view = app.dynamic_result_body();
        }
        app.update(Message::Dynamic(dynamic_reaction::Message::TheatreTick));
        assert!(app.dynamic.theatre_phase > 0.0);

        app.dynamic.build = DynamicBuildState::Idle;
        let stopped = app.dynamic.theatre_phase;
        app.update(Message::Dynamic(dynamic_reaction::Message::TheatreTick));
        assert!((app.dynamic.theatre_phase - stopped).abs() < f32::EPSILON);
    }

    #[test]
    fn retryable_static_presentation_relaunches_only_enrichment() {
        let outcome = dynamic_lithium_static();
        let mut app = App {
            provider: Some(AppMode::CodexBinary),
            dynamic: dynamic_reaction::State {
                request: Some(ReactionBuildRequest {
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
                static_outcome: Some(outcome.clone()),
                presentation: Some(DynamicPresentationOutcome::Static {
                    outcome: Box::new(outcome),
                    diagnostic: "structure proposal remained invalid".into(),
                    retryable: true,
                    attempts: 3,
                }),
                ..dynamic_reaction::State::default()
            },
            ..App::default()
        };

        app.update(Message::Dynamic(
            dynamic_reaction::Message::RetryPresentation,
        ));

        assert!(matches!(
            app.dynamic.build,
            DynamicBuildState::Running {
                stage: DynamicBuildStage::Presentation,
                ..
            }
        ));
        assert!(
            app.dynamic.static_outcome.is_some(),
            "retry must not discard the validated static outcome"
        );

        // A non-retryable presentation must not relaunch.
        let outcome = dynamic_lithium_static();
        let mut blocked = App {
            provider: Some(AppMode::CodexBinary),
            dynamic: dynamic_reaction::State {
                static_outcome: Some(outcome.clone()),
                presentation: Some(DynamicPresentationOutcome::Static {
                    outcome: Box::new(outcome),
                    diagnostic: "multiple reviewed families remain applicable".into(),
                    retryable: false,
                    attempts: 0,
                }),
                ..dynamic_reaction::State::default()
            },
            ..App::default()
        };
        blocked.update(Message::Dynamic(
            dynamic_reaction::Message::RetryPresentation,
        ));
        assert!(matches!(blocked.dynamic.build, DynamicBuildState::Idle));
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
            provider: Some(AppMode::CodexBinary),
            dynamic: dynamic_reaction::State {
                request: Some(request.clone()),
                ..dynamic_reaction::State::default()
            },
            ..App::default()
        };

        app.update(Message::Dynamic(dynamic_reaction::Message::Regenerate));

        assert_eq!(app.screen, Screen::Builder);
        let rebuilt = app.dynamic.request.as_ref().expect("retained request");
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
            app.dynamic.build,
            DynamicBuildState::Running { run_id: 1, .. }
        ));
    }

    #[test]
    fn stale_dynamic_completion_cannot_replace_current_build() {
        let mut app = App {
            dynamic: dynamic_reaction::State {
                build: DynamicBuildState::Running {
                    run_id: 9,
                    elapsed_seconds: 12,
                    stage: DynamicBuildStage::Claim,
                },
                ..dynamic_reaction::State::default()
            },
            ..App::default()
        };

        app.update(Message::Dynamic(dynamic_reaction::Message::ClaimFinished {
            run_id: 8,
            result: Box::new(Err("stale failure".to_owned().into())),
        }));

        assert!(matches!(
            app.dynamic.build,
            DynamicBuildState::Running { run_id: 9, .. }
        ));
    }

    #[test]
    fn normalized_provider_progress_is_generation_scoped_and_visible() {
        let (sender, receiver) = mpsc::channel();
        let mut app = App {
            dynamic: dynamic_reaction::State {
                build: DynamicBuildState::Running {
                    run_id: 9,
                    elapsed_seconds: 0,
                    stage: DynamicBuildStage::Claim,
                },
                progress_receiver: Some(receiver),
                ..dynamic_reaction::State::default()
            },
            ..App::default()
        };
        sender
            .send(CodexProgressEvent {
                stage: CodexProgressStage::SearchingSources,
                elapsed_ms: 42,
            })
            .expect("progress event");

        app.update(Message::Dynamic(dynamic_reaction::Message::BuildTick {
            run_id: 8,
        }));
        assert!(app.dynamic.progress.is_none());
        app.update(Message::Dynamic(dynamic_reaction::Message::BuildTick {
            run_id: 9,
        }));
        assert_eq!(
            app.dynamic_progress_label(),
            Some("checking the supporting evidence")
        );
        assert!(matches!(
            app.dynamic.build,
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
            dynamic: dynamic_reaction::State {
                build: DynamicBuildState::Running {
                    run_id: 9,
                    elapsed_seconds: 3,
                    stage: DynamicBuildStage::Claim,
                },
                overlay_dismissed: true,
                cancellation: Some(cancellation.clone()),
                next_run_id: 10,
                ..dynamic_reaction::State::default()
            },
            ..App::default()
        };

        app.update(Message::ReactantComposer(
            reactant_composer::Message::SelectReactant(reactant_composer::ActiveReactant::Second),
        ));

        assert!(cancellation.load(Ordering::Relaxed));
        assert!(matches!(app.dynamic.build, DynamicBuildState::Idle));
        assert_eq!(app.dynamic.next_run_id, 11);
        app.update(Message::Dynamic(dynamic_reaction::Message::ClaimFinished {
            run_id: 9,
            result: Box::new(Err("late completion".into())),
        }));
        assert!(matches!(app.dynamic.build, DynamicBuildState::Idle));
    }

    #[test]
    fn dropping_app_signals_dynamic_cancellation() {
        let cancellation = Arc::new(AtomicBool::new(false));
        {
            let mut app = App::default();
            app.dynamic.worker_shutdown.watch(&cancellation);
            app.dynamic.cancellation = Some(cancellation.clone());
        }

        assert!(cancellation.load(Ordering::Relaxed));
    }

    #[test]
    fn editing_reactants_invalidates_optional_presentation_and_static_result() {
        let outcome = dynamic_lithium_static();
        let cancellation = Arc::new(AtomicBool::new(false));
        let mut app = App {
            validated_frames: None,
            dynamic: dynamic_reaction::State {
                static_outcome: Some(outcome),
                build: DynamicBuildState::Running {
                    run_id: 4,
                    elapsed_seconds: 1,
                    stage: DynamicBuildStage::Presentation,
                },
                overlay_dismissed: true,
                cancellation: Some(cancellation.clone()),
                ..dynamic_reaction::State::default()
            },
            ..App::default()
        };

        app.update(Message::ReactantComposer(
            reactant_composer::Message::ClearActive,
        ));

        assert!(cancellation.load(Ordering::Relaxed));
        assert!(app.dynamic.static_outcome.is_none());
        assert!(app.validated_frames.is_none());
        assert!(app.dynamic.presentation.is_none());
        assert!(matches!(app.dynamic.build, DynamicBuildState::Idle));
    }

    #[test]
    fn selecting_identity_preserves_request_and_starts_the_same_build() {
        let catalogue = chemistry::reference_catalogue().expect("reference catalogue");
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
            provider: Some(AppMode::CodexBinary),
            dynamic: dynamic_reaction::State {
                request: Some(request.clone()),
                identity_choice: Some(DynamicIdentityChoice {
                    request,
                    ambiguity: agent::ReactantIdentityAmbiguity {
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
                ..dynamic_reaction::State::default()
            },
            ..App::default()
        };

        app.update(Message::Dynamic(
            dynamic_reaction::Message::IdentitySelected {
                reactant_index: 0,
                species_id: lithium_id.clone(),
            },
        ));

        assert!(app.dynamic.identity_choice.is_none());
        assert_eq!(
            app.dynamic.request.as_ref().unwrap().reactants[0].species_id,
            Some(lithium_id)
        );
        assert!(matches!(
            app.dynamic.build,
            DynamicBuildState::Running { .. }
        ));
    }

    #[test]
    fn adaptive_zoom_scales_windows_beyond_the_design_size() {
        // At or below the design size the layout stays 1:1.
        assert!((adaptive_zoom(DESIGN_SIZE, 1.0) - 1.0).abs() < f32::EPSILON);
        assert!((adaptive_zoom(Size::new(560.0, 760.0), 1.0) - 1.0).abs() < f32::EPSILON);

        // A 32in 4K-class window zooms by its most constrained axis (height:
        // 1490 / 900 = 1.655…).
        let zoom = adaptive_zoom(Size::new(2_650.0, 1_490.0), 1.0);
        assert!((zoom - 1_490.0 / DESIGN_SIZE.height).abs() < 0.001);

        // Zoom never exceeds the cap, however large the window.
        assert!((adaptive_zoom(Size::new(7_680.0, 4_320.0), 1.0) - MAX_UI_ZOOM).abs() < 0.001);
    }

    #[test]
    fn adaptive_zoom_scales_the_complete_ui_at_common_fullscreen_sizes() {
        let full_hd = adaptive_zoom(Size::new(1_920.0, 1_080.0), 1.0);
        assert!((full_hd - 1.2).abs() < f32::EPSILON);

        let quad_hd = adaptive_zoom(Size::new(2_560.0, 1_440.0), 1.0);
        assert!((quad_hd - 1.6).abs() < f32::EPSILON);
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
    fn window_keeps_native_decorations_and_an_opaque_surface() {
        let settings = window_settings();
        assert!(settings.decorations);
        assert!(!settings.transparent);
        assert_eq!(settings.size, DESIGN_SIZE);
        assert_eq!(settings.min_size, Some(Size::new(560.0, 760.0)));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_window_extends_content_behind_a_hidden_transparent_titlebar() {
        let settings = window_settings();
        assert!(settings.platform_specific.title_hidden);
        assert!(settings.platform_specific.titlebar_transparent);
        assert!(settings.platform_specific.fullsize_content_view);
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
        assert_eq!(app.title(), "ChemSpec");
        app.smoke_mode = Some(SmokeMode::Structural2d);
        app.screen = Screen::Structural2d;
        assert_eq!(app.title(), "ChemSpec Agent Smoke — Structural 2D");
        app.smoke_mode = Some(SmokeMode::Structural3d);
        app.screen = Screen::Structural3d;
        assert_eq!(app.title(), "ChemSpec Agent Smoke — Structural 3D");
    }

    #[test]
    fn smoke_title_tracks_the_live_screen_instead_of_the_launch_route() {
        let mut app = App {
            smoke_mode: Some(SmokeMode::Builder),
            screen: Screen::Builder,
            ..App::default()
        };

        assert_eq!(app.title(), "ChemSpec Agent Smoke — Builder");
        app.enter_screen(Screen::OutcomeChoice);
        assert_eq!(app.title(), "ChemSpec Agent Smoke — Outcome Choice");
        app.enter_screen(Screen::Structural2d);
        assert_eq!(app.title(), "ChemSpec Agent Smoke — Structural 2D");
        app.enter_screen(Screen::Structural3d);
        assert_eq!(app.title(), "ChemSpec Agent Smoke — Structural 3D");
        app.enter_screen(Screen::ProductSummary);
        assert_eq!(app.title(), "ChemSpec Agent Smoke — Product Summary");
    }

    #[test]
    fn api_mode_remains_unavailable_until_a_provider_is_connected() {
        let mut app = App::default();
        assert_eq!(app.screen, Screen::ProviderSetup);
        assert_eq!(app.provider, Some(AppMode::Local));
        app.update(Message::ProviderSelected(AppMode::Api));
        assert_eq!(app.provider, Some(AppMode::Local));
        app.update(Message::ProviderContinue);
        assert_eq!(app.screen, Screen::ProviderSetup);
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
            let plan = compile_educational_plan(run.frames(), run.declaration().required_context())
                .expect("educational plan compiles");
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
            let plan = compile_educational_plan(run.frames(), run.declaration().required_context())
                .expect("educational plan compiles");
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
    fn validated_runtime_oxide_colour_reaches_generic_3d_plan() {
        let mut app = App::default();
        app.finish_dynamic_presentation(dynamic_iron_oxide_presentation());
        let request = app
            .oxide_appearance_request()
            .expect("validated surface product produces a bounded request");
        let appearance = validated_oxide_appearance(&request, agent::OxideColourFamily::White);
        app.oxide_appearance_request = Some(request);
        app.oxide_appearance = Some(appearance);
        app.open_structural_animation();

        let colour = app
            .structural_animation
            .as_ref()
            .expect("animation")
            .real_world_plan
            .effects
            .iter()
            .find(|effect| effect.effect == EffectProfile::SurfaceOxidation)
            .and_then(|effect| effect.surface_oxide_colour.as_ref())
            .expect("accepted appearance reaches process effect");
        assert_eq!(colour.authority, MacroscopicColourAuthority::ModelAsserted);
        assert_eq!(
            colour.target,
            VisualColour {
                red: 0xee,
                green: 0xf1,
                blue: 0xef,
            }
        );
    }

    #[test]
    fn failed_oxide_colour_research_is_visible_and_retryable() {
        let mut app = App {
            provider: Some(AppMode::CodexBinary),
            codex_available: true,
            ..App::default()
        };
        app.finish_dynamic_presentation(dynamic_iron_oxide_presentation());
        let request = app
            .oxide_appearance_request()
            .expect("surface oxidation produces a bounded request");
        let request_binding = request.binding_digest().expect("request binding");
        app.oxide_appearance_request = Some(request);
        app.active_oxide_appearance_run = Some(41);

        app.update(Message::OxideAppearanceFinished {
            run_id: 41,
            request_binding,
            result: Box::new(Err("live source lookup timed out".to_owned())),
        });

        assert_eq!(
            app.oxide_appearance_error.as_deref(),
            Some("live source lookup timed out")
        );
        assert!(app.active_oxide_appearance_run.is_none());

        app.update(Message::RetryOxideAppearance);
        assert!(app.oxide_appearance_error.is_none());
        assert!(app.oxide_appearance_request.is_some());
        assert!(app.active_oxide_appearance_run.is_some());
    }

    #[test]
    fn accepted_oxide_colour_replaces_the_live_plan_without_restarting_playback() {
        let mut app = App::default();
        app.finish_dynamic_presentation(dynamic_iron_oxide_presentation());
        app.seek_real_world_timeline(2_500);
        let request = app
            .oxide_appearance_request()
            .expect("surface oxidation produces a bounded request");
        let request_binding = request.binding_digest().expect("request binding");
        let appearance = validated_oxide_appearance(&request, agent::OxideColourFamily::RedBrown);
        app.oxide_appearance_request = Some(request);
        app.active_oxide_appearance_run = Some(52);

        app.update(Message::OxideAppearanceFinished {
            run_id: 52,
            request_binding,
            result: Box::new(Ok(appearance)),
        });

        let animation = app.structural_animation.as_ref().expect("animation");
        assert_eq!(animation.real_world_playhead_ms, 2_500);
        let colour = animation
            .real_world_plan
            .effects
            .iter()
            .find(|effect| effect.effect == EffectProfile::SurfaceOxidation)
            .and_then(|effect| effect.surface_oxide_colour.as_ref())
            .expect("accepted appearance reaches the live process effect");
        assert_eq!(
            colour.target,
            VisualColour {
                red: 0x8e,
                green: 0x46,
                blue: 0x36,
            }
        );
    }

    #[test]
    fn dynamic_iron_oxide_uses_the_validated_runtime_colour_in_the_live_plan() {
        let presentation = dynamic_iron_oxide_presentation();
        let mut app = App {
            provider: Some(AppMode::CodexBinary),
            codex_available: true,
            ..App::default()
        };
        app.finish_dynamic_presentation(presentation);
        let request = app
            .oxide_appearance_request()
            .expect("dynamic iron oxide produces an exact appearance request");
        assert_eq!(request.product_formula, "Fe2O3");
        let request_binding = request.binding_digest().expect("request binding");
        let appearance = validated_oxide_appearance(&request, agent::OxideColourFamily::RedBrown);
        app.oxide_appearance_request = Some(request);
        app.active_oxide_appearance_run = Some(63);

        app.update(Message::OxideAppearanceFinished {
            run_id: 63,
            request_binding,
            result: Box::new(Ok(appearance)),
        });

        let colour = app
            .structural_animation
            .as_ref()
            .expect("animation")
            .real_world_plan
            .effects
            .iter()
            .find(|effect| effect.effect == EffectProfile::SurfaceOxidation)
            .and_then(|effect| effect.surface_oxide_colour.as_ref())
            .expect("dynamic iron oxidation keeps the accepted colour");
        assert_eq!(
            colour.target,
            VisualColour {
                red: 0x8e,
                green: 0x46,
                blue: 0x36,
            }
        );
    }

    #[test]
    fn macroscopic_playback_uses_the_same_16_ms_delta_as_its_60_hz_tick() {
        let mut app = App::default();
        app.open_structural_animation();
        app.screen = Screen::Structural3d;

        app.update(Message::StructuralTick);

        assert_eq!(
            app.structural_animation
                .as_ref()
                .expect("animation exists")
                .real_world_playhead_ms,
            16,
            "a 60 Hz redraw must not advance the 3D playhead by the old 33 ms cadence"
        );
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
            .expect("reference educational animation compiles");
        assert_eq!(
            animation.declaration.digest(),
            app.validated_declaration
                .as_ref()
                .expect("reference declaration is retained")
                .digest()
        );
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
                .expect("validated frames have a digest");
            assert_eq!(animation.educational_plan.id, digest);
            assert_eq!(animation.real_world_plan.reaction, digest);
            assert_eq!(
                animation.declaration.digest(),
                app.validated_declaration
                    .as_ref()
                    .expect("reference declaration is retained")
                    .digest()
            );

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

        app.screen = Screen::Builder;
        app.settings_dialog = Some(SettingsDialog {
            draft: app.settings,
            error: None,
        });
        for size in [
            Size::new(360.0, 480.0),
            Size::new(560.0, 620.0),
            Size::new(1_440.0, 900.0),
        ] {
            let _ = app.settings_overlay(size);
            let _ = app.view();
        }
    }

    #[test]
    fn settings_load_controls_first_launch_without_silent_mode_fallback() {
        let mut first_launch = App {
            codex_available: true,
            ..App::default()
        };
        first_launch.apply_settings_load(LoadOutcome::Missing);
        assert_eq!(first_launch.screen, Screen::ProviderSetup);
        assert_eq!(first_launch.provider, Some(AppMode::CodexBinary));

        let mut returning = App {
            codex_available: false,
            ..App::default()
        };
        returning.apply_settings_load(LoadOutcome::Loaded(AppSettings {
            app_mode: AppMode::Local,
            chemical_labels: ChemicalLabels::Names,
        }));
        assert_eq!(returning.screen, Screen::Builder);
        assert_eq!(returning.settings.chemical_labels, ChemicalLabels::Names);

        returning.apply_settings_load(LoadOutcome::Loaded(AppSettings {
            app_mode: AppMode::CodexBinary,
            chemical_labels: ChemicalLabels::Formulae,
        }));
        assert_eq!(returning.screen, Screen::ProviderSetup);
        assert_eq!(returning.provider, Some(AppMode::CodexBinary));
        assert!(returning.settings_load_error.is_some());
    }

    #[test]
    fn settings_apply_only_after_the_matching_atomic_save_finishes() {
        let mut app = App {
            screen: Screen::Builder,
            ..App::default()
        };
        app.update(Message::SettingsOpened);
        app.update(Message::SettingsChemicalLabelsSelected(
            ChemicalLabels::Names,
        ));
        app.update(Message::SettingsSaveRequested);
        assert_eq!(app.settings.chemical_labels, ChemicalLabels::Formulae);
        assert!(app.settings_saving(SettingsSaveDestination::Dialog));

        let saved = AppSettings {
            app_mode: AppMode::Local,
            chemical_labels: ChemicalLabels::Names,
        };
        app.update(Message::SettingsSaveFinished {
            save_id: 99,
            destination: SettingsSaveDestination::Dialog,
            settings: saved,
            result: Ok(()),
        });
        assert_eq!(app.settings.chemical_labels, ChemicalLabels::Formulae);

        app.update(Message::SettingsSaveFinished {
            save_id: 1,
            destination: SettingsSaveDestination::Dialog,
            settings: saved,
            result: Ok(()),
        });
        assert_eq!(app.settings, saved);
        assert!(app.settings_dialog.is_none());
    }

    #[test]
    fn app_mode_change_cancels_in_flight_dynamic_work_after_save() {
        let cancellation = Arc::new(AtomicBool::new(false));
        let mut app = App {
            screen: Screen::Builder,
            codex_available: true,
            settings: AppSettings {
                app_mode: AppMode::CodexBinary,
                chemical_labels: ChemicalLabels::Formulae,
            },
            provider: Some(AppMode::CodexBinary),
            dynamic: dynamic_reaction::State {
                build: DynamicBuildState::Running {
                    run_id: 7,
                    elapsed_seconds: 0,
                    stage: DynamicBuildStage::Claim,
                },
                cancellation: Some(cancellation.clone()),
                ..dynamic_reaction::State::default()
            },
            ..App::default()
        };
        let saved = AppSettings {
            app_mode: AppMode::Local,
            chemical_labels: ChemicalLabels::Formulae,
        };
        app.settings_save_state = SettingsSaveState::Saving {
            save_id: 1,
            destination: SettingsSaveDestination::Dialog,
        };
        app.settings_dialog = Some(SettingsDialog {
            draft: saved,
            error: None,
        });
        app.update(Message::SettingsSaveFinished {
            save_id: 1,
            destination: SettingsSaveDestination::Dialog,
            settings: saved,
            result: Ok(()),
        });
        assert!(cancellation.load(Ordering::Relaxed));
        assert!(matches!(app.dynamic.build, DynamicBuildState::Idle));
        assert_eq!(app.provider, Some(AppMode::Local));
    }

    #[test]
    fn validated_equations_switch_labels_without_changing_the_declaration() {
        let run = chemistry::run(chemistry::ReactionRequest::DEFAULT).expect("validated run");
        let digest = run.declaration().digest();
        let formulae =
            nomenclature::display_declaration(run.declaration(), ChemicalLabels::Formulae);
        let names = nomenclature::display_declaration(run.declaration(), ChemicalLabels::Names);
        assert!(formulae.contains("H₂O"));
        assert!(names.contains("water"));
        assert_eq!(run.declaration().digest(), digest);
    }

    #[test]
    fn stage_one_supported_drafts_open_the_guided_animation_directly() {
        let mut app = App::default();
        app.enter_screen(Screen::Builder);

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
        for _ in 0..64 {
            app.update(Message::ReactantComposer(
                reactant_composer::Message::PromptAnimationTick,
            ));
        }
        assert!(reactant_composer::submit_available(&app.reactant_composer));
        assert!(
            (reactant_composer::prompt_reveal(&app.reactant_composer) - 1.0).abs() < f32::EPSILON
        );

        app.update(Message::ReactantComposer(
            reactant_composer::Message::StartReactionRequested,
        ));

        assert_eq!(app.screen, Screen::Structural2d);
        assert_eq!(app.active_request, chemistry::ReactionRequest::DEFAULT);
        let animation = app
            .structural_animation
            .as_ref()
            .expect("guided animation compiles from the validated frames");
        assert!(animation.playing);
        assert!(!reactant_composer::submit_available(&app.reactant_composer));
        assert!(reactant_composer::prompt_reveal(&app.reactant_composer) > 0.0);

        app.update(Message::ReturnToBuilder);
        assert_eq!(app.screen, Screen::Builder);
        assert!(reactant_composer::submit_available(&app.reactant_composer));
        assert!(reactant_composer::prompt_reveal(&app.reactant_composer).abs() < f32::EPSILON);
        for _ in 0..64 {
            app.update(Message::ReactantComposer(
                reactant_composer::Message::PromptAnimationTick,
            ));
        }
        assert!(
            (reactant_composer::prompt_reveal(&app.reactant_composer) - 1.0).abs() < f32::EPSILON,
            "returning to the builder must not resume an obsolete fade-out"
        );
    }

    #[test]
    fn screen_entry_reconciles_builder_owned_transient_state() {
        let mut app = App::default();
        app.enter_screen(Screen::Builder);
        reactant_composer::replace_reactants(&mut app.reactant_composer, [vec![12], vec![1, 1, 8]]);
        app.sync_builder_submit_prompt();
        app.builder_panel = Some(BuilderPanel::Help);
        assert!(reactant_composer::try_codex_notice_visible(
            &app.reactant_composer
        ));

        app.enter_screen(Screen::Structural2d);

        assert!(app.builder_panel.is_none());
        assert!(!reactant_composer::submit_available(&app.reactant_composer));
        assert!(!reactant_composer::try_codex_notice_visible(
            &app.reactant_composer
        ));

        app.enter_screen(Screen::Builder);

        assert!(reactant_composer::try_codex_notice_visible(
            &app.reactant_composer
        ));
        assert!(reactant_composer::prompt_reveal(&app.reactant_composer).abs() < f32::EPSILON);
    }

    #[test]
    fn build_another_starts_fresh_while_return_preserves_the_reaction() {
        let mut app = App::default();
        app.enter_screen(Screen::Builder);
        reactant_composer::replace_reactants(&mut app.reactant_composer, [vec![3], vec![1, 1, 8]]);
        app.sync_builder_submit_prompt();
        app.open_structural_animation();

        app.update(Message::ReturnToBuilder);
        assert_eq!(
            reactant_composer::reactants(&app.reactant_composer),
            (&[3][..], &[1, 1, 8][..])
        );

        sketcher::update(
            &mut app.sketcher,
            sketcher::Message::Canvas(sketcher::CanvasEvent::Placed(iced::Point::new(40.0, 40.0))),
        );
        assert!(sketcher::submission(&app.sketcher).is_some());
        app.dynamic.context = Some(DynamicRequestContext::Heat);
        app.enter_screen(Screen::ProductSummary);

        app.update(Message::StartNewReaction);

        assert_eq!(app.screen, Screen::Builder);
        assert_eq!(
            reactant_composer::reactants(&app.reactant_composer),
            (&[][..], &[][..])
        );
        assert!(app.pending_requests.is_empty());
        assert!(app.oxygen_assessment.is_none());
        assert!(app.validated_frames.is_none());
        assert!(app.validated_macroscopic.is_none());
        assert!(app.structural_animation.is_none());
        assert!(app.structural_error.is_none());
        assert!(app.dynamic.context.is_none());
        assert!(app.builder_panel.is_none());
        assert!(sketcher::submission(&app.sketcher).is_none());
        assert!(!reactant_composer::submit_available(&app.reactant_composer));
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
