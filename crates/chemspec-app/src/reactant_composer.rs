//! Stage 1 structured reactant composer (`U-112`).
//!
//! The composer is the learner's question: "What happens when X reacts
//! with Y?". It records user intent only. Formulae and recognised
//! compositions are presentation previews and never become validated
//! chemistry here.

use std::collections::BTreeMap;
use std::f32::consts::{FRAC_PI_2, TAU};

use iced::mouse::Cursor;
use iced::widget::canvas::path::Arc;
use iced::widget::canvas::{Path, Stroke};
use iced::widget::{
    button, canvas, column, container, mouse_area, responsive, row, stack, text, text_input,
};
use iced::{
    Center, Color, Element, Fill, Font, Length, Point, Radians, Rectangle, Renderer, Size,
    Subscription, Theme, Vector,
};

use crate::chemistry;
use crate::composition_catalogue;
use crate::elements;
use crate::fonts;
use crate::particle_visualization::{AmbientReactantDiagram, ambient_footprint};
use crate::theme::{self, color, motion, space as spacing, type_scale};

// Matches the domain generator's structure cap so name-resolved organics
// (butane, esters) fit in one slot.
const MAX_ATOMS_PER_REACTANT: usize = 24;

const SENTENCE_FONT: Font = fonts::MEDIUM;
const FORMULA_FONT: Font = fonts::SEMIBOLD;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveReactant {
    First,
    Second,
}

impl ActiveReactant {
    const fn index(self) -> usize {
        match self {
            Self::First => 0,
            Self::Second => 1,
        }
    }
}

#[derive(Debug, Default)]
struct ReactantDraft {
    atoms: Vec<u8>,
    /// The user's typed name when the draft came from name entry. Names can
    /// identify a species its element inventory alone cannot (ammonium
    /// cyanate vs urea), so the name travels with the draft until any
    /// manual atom edit invalidates it.
    name: Option<String>,
    /// Canonical presentation formula resolved once with the typed identity.
    /// Keeping it beside the name avoids rebuilding a structure on every
    /// animation frame just to display conventional element order.
    display_formula: Option<String>,
}

/// An in-flight press on a slot: a quick release clicks (select or undo),
/// while holding to completion clears the slot.
#[derive(Debug, Clone, Copy)]
struct HoldState {
    slot: ActiveReactant,
    progress: f32,
    completed: bool,
}

#[derive(Debug, Clone, Copy)]
struct AmbientResize {
    current: Size,
    target: Size,
    velocity: Vector,
}

impl Default for AmbientResize {
    fn default() -> Self {
        Self {
            current: Size::ZERO,
            target: Size::ZERO,
            velocity: Vector::new(0.0, 0.0),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct AmbientPlacement {
    current: Point,
    target: Point,
    velocity: Vector,
}

impl Default for AmbientPlacement {
    fn default() -> Self {
        let anchor = Point::new(0.5, 0.42);
        Self {
            current: anchor,
            target: anchor,
            velocity: Vector::new(0.0, 0.0),
        }
    }
}

#[derive(Debug, Default)]
struct AmbientPresentation {
    atoms: Vec<u8>,
    name: Option<String>,
    reveal: f32,
    placement: AmbientPlacement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PromptTarget {
    Hidden,
    Submit,
    TryCodex,
}

#[derive(Debug)]
pub struct State {
    drafts: [ReactantDraft; 2],
    active: ActiveReactant,
    limit_reached: bool,
    holding: Option<HoldState>,
    orbital_phase: f32,
    ambient: [AmbientPresentation; 2],
    ambient_resize: AmbientResize,
    editing: Option<ActiveReactant>,
    name_input: String,
    name_feedback: Option<String>,
    prompt_target: PromptTarget,
    prompt_reveal: f32,
}

impl Default for State {
    fn default() -> Self {
        Self {
            drafts: [ReactantDraft::default(), ReactantDraft::default()],
            active: ActiveReactant::First,
            limit_reached: false,
            holding: None,
            orbital_phase: 0.0,
            ambient: [
                AmbientPresentation::default(),
                AmbientPresentation::default(),
            ],
            ambient_resize: AmbientResize::default(),
            editing: None,
            name_input: String::new(),
            name_feedback: None,
            prompt_target: PromptTarget::Hidden,
            prompt_reveal: 0.0,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    AddElement(u8),
    DropElement(ActiveReactant, u8),
    SelectReactant(ActiveReactant),
    SlotPressed(ActiveReactant),
    SlotReleased(ActiveReactant),
    SlotExited(ActiveReactant),
    Undo,
    ClearActive,
    BeginNameEntry(ActiveReactant),
    NameInput(String),
    NameSubmitted,
    NameEntryCancelled,
    StartReactionRequested,
    AnimationTick,
    PromptAnimationTick,
}

impl Message {
    /// Presentation-only messages animate the composer without changing any
    /// draft content. The app must not treat them as edits: edits cancel an
    /// in-flight dynamic build and clear its result, and these fire on a
    /// timer whenever ambient models are on screen.
    #[must_use]
    pub const fn is_presentation_only(&self) -> bool {
        matches!(self, Self::AnimationTick | Self::PromptAnimationTick)
    }
}

pub fn update(state: &mut State, message: Message) {
    match message {
        Message::AddElement(atomic_number) => {
            state.editing = None;
            state.name_input.clear();
            state.name_feedback = None;
            add_element(state, state.active, atomic_number);
        }
        Message::DropElement(reactant, atomic_number) => {
            state.active = reactant;
            state.editing = None;
            state.name_input.clear();
            state.name_feedback = None;
            add_element(state, reactant, atomic_number);
        }
        Message::SelectReactant(reactant) => {
            state.active = reactant;
            state.holding = None;
        }
        Message::SlotPressed(reactant) => {
            state.holding = Some(HoldState {
                slot: reactant,
                progress: 0.0,
                completed: false,
            });
        }
        // A quick release is a click: select an inactive slot, undo the last
        // element of the active one. A hold that already cleared is consumed.
        Message::SlotReleased(reactant) => {
            let Some(hold) = state.holding.take() else {
                return;
            };
            if hold.slot != reactant || hold.completed {
                return;
            }
            if state.active == reactant {
                let draft = &mut state.drafts[reactant.index()];
                draft.atoms.pop();
                draft.name = None;
                draft.display_formula = None;
            } else {
                state.active = reactant;
            }
            state.limit_reached = false;
        }
        Message::SlotExited(reactant) => {
            if state.holding.is_some_and(|hold| hold.slot == reactant) {
                state.holding = None;
            }
        }
        Message::Undo => {
            let draft = &mut state.drafts[state.active.index()];
            draft.atoms.pop();
            draft.name = None;
            draft.display_formula = None;
            state.limit_reached = false;
        }
        Message::ClearActive => {
            let draft = &mut state.drafts[state.active.index()];
            draft.atoms.clear();
            draft.name = None;
            draft.display_formula = None;
            state.limit_reached = false;
        }
        Message::BeginNameEntry(reactant) => {
            state.active = reactant;
            state.editing = Some(reactant);
            state.holding = None;
            state.name_input.clear();
            state.name_feedback = None;
        }
        Message::NameInput(value) => {
            state.name_input = value;
            state.name_feedback = None;
        }
        Message::NameSubmitted => submit_name(state),
        Message::NameEntryCancelled => {
            state.editing = None;
            state.name_input.clear();
            state.name_feedback = None;
        }
        Message::StartReactionRequested => {}
        Message::AnimationTick => animation_tick(state),
        Message::PromptAnimationTick => {
            if state.prompt_target == PromptTarget::Hidden {
                state.prompt_reveal = (state.prompt_reveal - motion::PROMPT_FADE_STEP).max(0.0);
            } else {
                state.prompt_reveal = (state.prompt_reveal + motion::PROMPT_FADE_STEP).min(1.0);
            }
        }
    }
    sync_ambient_presentations(state);
}

fn animation_tick(state: &mut State) {
    if state
        .ambient
        .iter()
        .any(|ambient| !ambient.atoms.is_empty())
    {
        state.orbital_phase = (state.orbital_phase + motion::ORBIT_STEP) % 1.0;
    }
    step_ambient_presentations(state);
    if let Some(hold) = &mut state.holding
        && !hold.completed
        && !state.drafts[hold.slot.index()].atoms.is_empty()
    {
        hold.progress += motion::HOLD_CLEAR_STEP;
        if hold.progress >= 1.0 {
            hold.completed = true;
            let draft = &mut state.drafts[hold.slot.index()];
            draft.atoms.clear();
            draft.name = None;
            draft.display_formula = None;
            state.limit_reached = false;
        }
    }
}

/// Each motion family subscribes only while it is active. Ambient models use
/// the calmer 30 fps cadence; the prompt keeps a dedicated 60 fps fade.
pub fn subscription(state: &State) -> Subscription<Message> {
    let hold_running = state
        .holding
        .is_some_and(|hold| !hold.completed && !state.drafts[hold.slot.index()].atoms.is_empty());
    let ambient_active = state
        .ambient
        .iter()
        .any(|ambient| !ambient.atoms.is_empty())
        || ambient_resize_is_settling(&state.ambient_resize);
    let model_motion = if ambient_active || hold_running {
        iced::time::every(motion::TICK).map(|_| Message::AnimationTick)
    } else {
        Subscription::none()
    };
    let prompt_motion = if prompt_is_animating(state) {
        iced::time::every(motion::PROMPT_TICK).map(|_| Message::PromptAnimationTick)
    } else {
        Subscription::none()
    };
    Subscription::batch([model_motion, prompt_motion])
}

fn prompt_is_animating(state: &State) -> bool {
    (state.prompt_target != PromptTarget::Hidden && state.prompt_reveal < 1.0)
        || (state.prompt_target == PromptTarget::Hidden && state.prompt_reveal > 0.0)
}

fn sync_ambient_presentations(state: &mut State) {
    let mut appeared = [false; 2];
    for (index, (draft, ambient)) in state.drafts.iter().zip(&mut state.ambient).enumerate() {
        if !draft.atoms.is_empty() {
            appeared[index] = ambient.atoms.is_empty();
            ambient.atoms.clone_from(&draft.atoms);
            ambient.name.clone_from(&draft.name);
        } else if ambient.reveal <= f32::EPSILON {
            ambient.atoms.clear();
            ambient.name = None;
        }
    }
    refresh_ambient_targets(state);
    for (index, appeared) in appeared.into_iter().enumerate() {
        if appeared {
            state.ambient[index].placement.current = state.ambient[index].placement.target;
            state.ambient[index].placement.velocity = Vector::new(0.0, 0.0);
        }
    }
}

fn step_ambient_presentations(state: &mut State) {
    refresh_ambient_targets(state);
    for (draft, ambient) in state.drafts.iter().zip(&mut state.ambient) {
        let reveal_target = if draft.atoms.is_empty() { 0.0 } else { 1.0 };
        if ambient.reveal < reveal_target {
            ambient.reveal = (ambient.reveal + 0.08).min(reveal_target);
        } else if ambient.reveal > reveal_target {
            ambient.reveal = (ambient.reveal - 0.08).max(reveal_target);
        }

        let placement_delta = Vector::new(
            ambient.placement.target.x - ambient.placement.current.x,
            ambient.placement.target.y - ambient.placement.current.y,
        );
        ambient.placement.velocity = (ambient.placement.velocity + placement_delta * 0.14) * 0.74;
        ambient.placement.current += ambient.placement.velocity;
        if placement_delta.x.abs() < 0.000_5 && ambient.placement.velocity.x.abs() < 0.000_5 {
            ambient.placement.current.x = ambient.placement.target.x;
            ambient.placement.velocity.x = 0.0;
        }
        if placement_delta.y.abs() < 0.000_5 && ambient.placement.velocity.y.abs() < 0.000_5 {
            ambient.placement.current.y = ambient.placement.target.y;
            ambient.placement.velocity.y = 0.0;
        }
    }
    step_ambient_resize(&mut state.ambient_resize);
}

fn refresh_ambient_targets(state: &mut State) {
    let viewport = state.ambient_resize.target;
    if viewport.width <= 0.0 || viewport.height <= 0.0 {
        return;
    }
    for (index, ambient) in state.ambient.iter_mut().enumerate() {
        if !ambient.atoms.is_empty() {
            let side = if index == 0 {
                ActiveReactant::First
            } else {
                ActiveReactant::Second
            };
            ambient.placement.target = solve_ambient_anchor(&ambient.atoms, side, viewport);
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct OccupiedZone {
    bounds: Rectangle,
    weight: f32,
    table: bool,
}

fn solve_ambient_anchor(atoms: &[u8], side: ActiveReactant, viewport: Size) -> Point {
    if atoms.is_empty() || viewport.width <= 0.0 || viewport.height <= 0.0 {
        return Point::new(0.5, 0.42);
    }

    let half_viewport = Size::new(viewport.width * 0.5, viewport.height);
    let footprint = ambient_footprint(atoms.len(), half_viewport, 1.0);
    let width = (footprint.width / half_viewport.width).clamp(0.08, 0.86);
    let height = (footprint.height / half_viewport.height).clamp(0.06, 0.72);
    let table_top = (300.0 / viewport.height).clamp(0.36, 0.54);
    let table_bottom = 0.96;
    let table_row = (table_bottom - table_top) / 9.0;
    let ideal_y = (table_top - height * 0.22).clamp(height / 2.0 + 0.035, 0.58);
    let zones = ambient_occupied_zones(side, table_top, table_bottom, table_row);
    let min_x = width / 2.0 + 0.035;
    let max_x = 1.0 - min_x;
    let min_y = height / 2.0 + 0.035;
    let max_y = (0.72_f32).min(1.0 - height / 2.0 - 0.035);
    let x_candidates: [f32; 5] = [0.30, 0.40, 0.50, 0.60, 0.70];
    let y_candidates: [f32; 4] = [
        ideal_y - 0.09,
        ideal_y - 0.03,
        ideal_y + 0.04,
        ideal_y + 0.10,
    ];

    let mut best = Point::new(0.5_f32.clamp(min_x, max_x), ideal_y.clamp(min_y, max_y));
    let mut best_score = f32::INFINITY;
    for (x, y) in x_candidates
        .into_iter()
        .flat_map(|x| y_candidates.into_iter().map(move |y| (x, y)))
    {
        let candidate = Point::new(x.clamp(min_x, max_x), y.clamp(min_y, max_y));
        let model = Rectangle {
            x: candidate.x - width / 2.0,
            y: candidate.y - height / 2.0,
            width,
            height,
        };
        let model_area = (width * height).max(0.001);
        let mut protected_overlap = 0.0;
        let mut table_overlap = 0.0;
        for zone in zones {
            let overlap = rectangle_overlap_area(model, zone.bounds) / model_area;
            if zone.table {
                table_overlap += overlap;
            } else {
                protected_overlap += overlap * zone.weight;
            }
        }
        let table_overlap = table_overlap.clamp(0.0, 1.0);
        let desired_table_overlap = if atoms.len() == 1 { 0.16 } else { 0.12 };
        let score = protected_overlap * 8.0
            + (table_overlap - desired_table_overlap).powi(2) * 2.2
            + (candidate.y - ideal_y).abs() * 0.20
            + (candidate.x - 0.5).abs() * 0.035;
        if score < best_score {
            best = candidate;
            best_score = score;
        }
    }
    best
}

fn ambient_occupied_zones(
    side: ActiveReactant,
    table_top: f32,
    table_bottom: f32,
    row: f32,
) -> [OccupiedZone; 5] {
    let question = match side {
        ActiveReactant::First => Rectangle {
            x: 0.42,
            y: 0.08,
            width: 0.58,
            height: 0.27,
        },
        ActiveReactant::Second => Rectangle {
            x: 0.0,
            y: 0.08,
            width: 0.58,
            height: 0.27,
        },
    };
    let toolbar = match side {
        ActiveReactant::First => Rectangle {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        },
        ActiveReactant::Second => Rectangle {
            x: 0.76,
            y: 0.0,
            width: 0.24,
            height: 0.17,
        },
    };
    let (upper_block, middle_block, lower_block) = match side {
        ActiveReactant::First => (
            Rectangle {
                x: 0.08,
                y: table_top,
                width: 0.24,
                height: table_bottom - table_top,
            },
            Rectangle {
                x: 0.28,
                y: table_top + row * 3.0,
                width: 0.72,
                height: row * 4.2,
            },
            Rectangle {
                x: 0.38,
                y: table_top + row * 7.1,
                width: 0.62,
                height: table_bottom - (table_top + row * 7.1),
            },
        ),
        ActiveReactant::Second => (
            Rectangle {
                x: 0.28,
                y: table_top,
                width: 0.66,
                height: row * 6.2,
            },
            Rectangle {
                x: 0.0,
                y: table_top + row * 3.0,
                width: 0.30,
                height: row * 4.2,
            },
            Rectangle {
                x: 0.0,
                y: table_top + row * 7.1,
                width: 0.94,
                height: table_bottom - (table_top + row * 7.1),
            },
        ),
    };
    [
        OccupiedZone {
            bounds: question,
            weight: 1.0,
            table: false,
        },
        OccupiedZone {
            bounds: toolbar,
            weight: 1.0,
            table: false,
        },
        OccupiedZone {
            bounds: upper_block,
            weight: 1.0,
            table: true,
        },
        OccupiedZone {
            bounds: middle_block,
            weight: 1.0,
            table: true,
        },
        OccupiedZone {
            bounds: lower_block,
            weight: 1.0,
            table: true,
        },
    ]
}

fn rectangle_overlap_area(left: Rectangle, right: Rectangle) -> f32 {
    let width = (left.x + left.width).min(right.x + right.width) - left.x.max(right.x);
    let height = (left.y + left.height).min(right.y + right.height) - left.y.max(right.y);
    width.max(0.0) * height.max(0.0)
}

fn step_ambient_resize(resize: &mut AmbientResize) {
    if !ambient_resize_is_settling(resize) {
        return;
    }

    let delta = Vector::new(
        resize.target.width - resize.current.width,
        resize.target.height - resize.current.height,
    );
    resize.velocity = (resize.velocity + delta * 0.13) * 0.72;
    resize.current.width = (resize.current.width + resize.velocity.x).max(1.0);
    resize.current.height = (resize.current.height + resize.velocity.y).max(1.0);

    if delta.x.abs() < 0.08 && resize.velocity.x.abs() < 0.08 {
        resize.current.width = resize.target.width;
        resize.velocity.x = 0.0;
    }
    if delta.y.abs() < 0.08 && resize.velocity.y.abs() < 0.08 {
        resize.current.height = resize.target.height;
        resize.velocity.y = 0.0;
    }
}

fn ambient_resize_is_settling(resize: &AmbientResize) -> bool {
    (resize.current.width - resize.target.width).abs() >= 0.08
        || (resize.current.height - resize.target.height).abs() >= 0.08
        || resize.velocity.x.abs() >= 0.08
        || resize.velocity.y.abs() >= 0.08
}

/// Updates the spring destination without disturbing its current position or
/// velocity. A stream of live-resize events therefore moves one continuous
/// target instead of stacking fresh impulses on every pointer movement.
pub fn resize_ambient(state: &mut State, size: Size) {
    if size.width <= 0.0 || size.height <= 0.0 {
        return;
    }
    if state.ambient_resize.current.width <= 0.0 || state.ambient_resize.current.height <= 0.0 {
        state.ambient_resize.current = size;
        state.ambient_resize.target = size;
        state.ambient_resize.velocity = Vector::new(0.0, 0.0);
    } else {
        state.ambient_resize.target = size;
    }
}

/// Fills slots from typed compound names or formulas. A separator
/// ("oxygen + water", "zinc, hydrochloric acid", "iron and sulfur")
/// fills both boxes at once; a single compound fills the active one.
fn submit_name(state: &mut State) {
    let input = state.name_input.trim().to_owned();
    if input.is_empty() {
        return;
    }
    let filled = match split_reactant_names(&input).as_slice() {
        [only] => resolve_named(only).map(|draft| {
            state.drafts[state.active.index()] = draft;
        }),
        [first, second] => resolve_named(first).and_then(|first_draft| {
            resolve_named(second).map(|second_draft| {
                state.drafts = [first_draft, second_draft];
            })
        }),
        _ => Err("Reactions here take at most two reactants".to_owned()),
    };
    match filled {
        Ok(()) => {
            state.editing = None;
            state.name_input.clear();
            state.name_feedback = None;
            state.limit_reached = false;
        }
        Err(feedback) => state.name_feedback = Some(feedback),
    }
}

fn split_reactant_names(input: &str) -> Vec<&str> {
    let symbols = input
        .split(['+', ','])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if symbols.len() > 1 {
        return symbols;
    }
    let words = input
        .split(" and ")
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if words.len() > 1 { words } else { vec![input] }
}

fn resolve_named(input: &str) -> Result<ReactantDraft, String> {
    let Some(atoms) = chemistry::atoms_from_name(input) else {
        return Err(format!(
            "“{input}” isn’t recognised — try a name like “copper(II) sulfate” or a formula like CuSO4"
        ));
    };
    if atoms.len() > MAX_ATOMS_PER_REACTANT {
        return Err(format!("“{input}” has too many atoms for one reactant"));
    }
    if let Some(&unknown) = atoms
        .iter()
        .find(|number| elements::by_atomic_number(**number).is_none())
    {
        return Err(format!("Element {unknown} is not in the library yet"));
    }
    let display_formula =
        composition_catalogue::trusted_preview_named(input, atoms.iter().copied())
            .map_or_else(|| formula(&atoms), |preview| preview.formula);
    Ok(ReactantDraft {
        atoms,
        name: Some(input.to_owned()),
        display_formula: Some(display_formula),
    })
}

fn add_element(state: &mut State, reactant: ActiveReactant, atomic_number: u8) {
    if elements::by_atomic_number(atomic_number).is_none() {
        return;
    }
    let draft = &mut state.drafts[reactant.index()];
    if draft.atoms.len() >= MAX_ATOMS_PER_REACTANT {
        state.limit_reached = true;
        return;
    }
    draft.atoms.push(atomic_number);
    draft.name = None;
    draft.display_formula = None;
    state.limit_reached = false;
}

pub fn can_start_reaction(state: &State) -> bool {
    state.drafts.iter().all(|draft| !draft.atoms.is_empty())
        && !matches!(
            resolution(state),
            chemistry::DraftResolution::SystemError(_)
        )
}

pub fn resolution(state: &State) -> chemistry::DraftResolution {
    chemistry::resolve_drafts(&state.drafts[0].atoms, &state.drafts[1].atoms)
}

pub fn reactants(state: &State) -> (&[u8], &[u8]) {
    (&state.drafts[0].atoms, &state.drafts[1].atoms)
}

/// The typed name behind each draft, when the draft came from name entry
/// and survives unedited. A name can identify a species its inventory
/// cannot (ammonium cyanate vs urea).
#[must_use]
pub fn draft_names(state: &State) -> [Option<&str>; 2] {
    [
        state.drafts[0].name.as_deref(),
        state.drafts[1].name.as_deref(),
    ]
}

/// User-facing formulae for the current drafts. Name-resolved drafts retain
/// conventional formula order (`NaCl`, not inventory-sorted `ClNa`).
#[must_use]
pub fn draft_formulae(state: &State) -> [String; 2] {
    state.drafts.each_ref().map(|draft| {
        draft
            .display_formula
            .clone()
            .unwrap_or_else(|| formula(&draft.atoms))
    })
}

#[must_use]
pub const fn editing(state: &State) -> Option<ActiveReactant> {
    state.editing
}

#[must_use]
pub fn name_input_is_empty(state: &State) -> bool {
    state.name_input.trim().is_empty()
}

#[must_use]
pub fn name_input_id(reactant: ActiveReactant) -> iced::widget::Id {
    iced::widget::Id::new(match reactant {
        ActiveReactant::First => "reactant-name-input-first",
        ActiveReactant::Second => "reactant-name-input-second",
    })
}

pub fn set_submit_available(state: &mut State, available: bool) {
    state.prompt_target = if available {
        PromptTarget::Submit
    } else {
        PromptTarget::Hidden
    };
}

pub fn show_try_codex_notice(state: &mut State) {
    state.prompt_target = PromptTarget::TryCodex;
}

#[cfg(test)]
#[must_use]
pub const fn submit_available(state: &State) -> bool {
    matches!(state.prompt_target, PromptTarget::Submit)
}

#[cfg(test)]
#[must_use]
pub const fn try_codex_notice_visible(state: &State) -> bool {
    matches!(state.prompt_target, PromptTarget::TryCodex)
}

#[cfg(test)]
#[must_use]
pub const fn prompt_reveal(state: &State) -> f32 {
    state.prompt_reveal
}

/// Starts a builder-entry prompt from the beginning of its normal fade. The
/// app calls this only when crossing back into the builder; prompt intent is
/// still derived separately from the current reaction and provider.
pub fn restart_prompt_reveal(state: &mut State) {
    state.prompt_reveal = 0.0;
}

/// Begins a genuinely new composition while preserving the ambient atoms long
/// enough for their ordinary exit animation. This is intentionally different
/// from returning to the builder, which keeps the current drafts intact.
pub fn clear_reaction(state: &mut State) {
    state.drafts = [ReactantDraft::default(), ReactantDraft::default()];
    state.active = ActiveReactant::First;
    state.limit_reached = false;
    state.holding = None;
    state.editing = None;
    state.name_input.clear();
    state.name_feedback = None;
    state.prompt_target = PromptTarget::Hidden;
    state.prompt_reveal = 0.0;
    sync_ambient_presentations(state);
}

/// Fills the active slot with a sketched structure: the full atom inventory
/// (implicit hydrogens included) plus its SMILES as the draft name, which
/// the dynamic pipeline resolves back into the exact drawn structure.
pub fn set_sketched_reactant(state: &mut State, atoms: Vec<u8>, smiles: String) {
    let draft = &mut state.drafts[state.active.index()];
    let display_formula =
        composition_catalogue::trusted_preview_named(&smiles, atoms.iter().copied())
            .map_or_else(|| formula(&atoms), |preview| preview.formula);
    draft.atoms = atoms;
    draft.name = Some(smiles);
    draft.display_formula = Some(display_formula);
    state.limit_reached = false;
    state.editing = None;
    state.name_input.clear();
    state.name_feedback = None;
    for ambient in &mut state.ambient {
        *ambient = AmbientPresentation::default();
    }
    sync_ambient_presentations(state);
}

#[cfg(test)]
pub fn replace_reactants(state: &mut State, drafts: [Vec<u8>; 2]) {
    state.drafts = drafts.map(|atoms| ReactantDraft {
        atoms,
        name: None,
        display_formula: None,
    });
    state.active = ActiveReactant::First;
    state.limit_reached = false;
    state.editing = None;
    state.name_input.clear();
    state.name_feedback = None;
    for ambient in &mut state.ambient {
        *ambient = AmbientPresentation::default();
    }
    sync_ambient_presentations(state);
}

/// Two independently clipped, non-interactive model surfaces for the library
/// background. Each Canvas receives exactly one half of the available width.
pub fn ambient_view(state: &State) -> Element<'static, Message> {
    let first_atoms = state.ambient[0].atoms.clone();
    let second_atoms = state.ambient[1].atoms.clone();
    let first_name = state.ambient[0].name.clone();
    let second_name = state.ambient[1].name.clone();
    let first_reveal = theme::ease_in_out(state.ambient[0].reveal);
    let second_reveal = theme::ease_in_out(state.ambient[1].reveal);
    let first_anchor = state.ambient[0].placement.current;
    let second_anchor = state.ambient[1].placement.current;
    let resize_current = state.ambient_resize.current;
    let resize_target = state.ambient_resize.target;
    let phase = state.orbital_phase;

    responsive(move |size| {
        let resize_delta = if resize_current.width > 0.0 && resize_target.width > 0.0 {
            Vector::new(
                resize_current.width - resize_target.width,
                resize_current.height - resize_target.height,
            )
        } else {
            Vector::new(0.0, 0.0)
        };
        let visual_size = Size::new(
            (size.width + resize_delta.x).max(1.0),
            (size.height + resize_delta.y).max(1.0),
        );
        let scale =
            (ambient_layout_extent(visual_size) / ambient_layout_extent(size)).clamp(0.82, 1.18);
        let vertical_offset = (resize_delta.y * 0.42).clamp(-64.0, 64.0);
        let first_offset = Vector::new((resize_delta.x * 0.25).clamp(-96.0, 96.0), vertical_offset);
        let second_offset =
            Vector::new((resize_delta.x * 0.75).clamp(-96.0, 96.0), vertical_offset);

        row![
            canvas(AmbientReactantDiagram::new(
                first_atoms.clone(),
                first_name.as_deref(),
                phase,
                first_reveal,
                scale,
                first_anchor,
                first_offset,
                -1.0,
            ))
            .width(Fill)
            .height(Fill),
            canvas(AmbientReactantDiagram::new(
                second_atoms.clone(),
                second_name.as_deref(),
                phase + 0.31,
                second_reveal,
                scale,
                second_anchor,
                second_offset,
                1.0,
            ))
            .width(Fill)
            .height(Fill),
        ]
        .width(Fill)
        .height(Fill)
        .into()
    })
    .width(Fill)
    .height(Fill)
    .into()
}

fn ambient_layout_extent(size: Size) -> f32 {
    let short_side = (size.width * 0.5).min(size.height).max(24.0);
    (short_side * 0.54)
        .clamp(96.0, 300.0)
        .min((short_side - 12.0).max(24.0))
}

pub fn view(
    state: &State,
    library_drag: Option<u8>,
    local: bool,
    compact: bool,
) -> Element<'static, Message> {
    let sentence = sentence(state, library_drag, compact);
    let prompt = reaction_prompt(state, local, compact);

    container(
        column![sentence, prompt]
            .spacing(spacing::LG)
            .align_x(Center)
            .width(Fill),
    )
    .padding(iced::Padding {
        top: if compact {
            spacing::LG
        } else {
            spacing::XL * 2.0
        },
        bottom: spacing::MD,
        left: spacing::MD,
        right: spacing::MD,
    })
    .width(Fill)
    .into()
}

/// Wide layouts phrase the equation as the product's canonical question;
/// compact layouts fall back to the denser `X + Y` equation form.
fn sentence(state: &State, library_drag: Option<u8>, compact: bool) -> Element<'static, Message> {
    let word = |content: &'static str| {
        text(content)
            .size(if compact {
                type_scale::TITLE
            } else {
                type_scale::HERO
            })
            .font(SENTENCE_FONT)
            .color(color::TEXT_SOFT)
    };
    let first = slot(state, ActiveReactant::First, library_drag, compact);
    let second = slot(state, ActiveReactant::Second, library_drag, compact);

    let sentence = if compact {
        row![
            first,
            text("+").size(type_scale::TITLE).color(color::MUTED),
            second,
            text("→").size(type_scale::TITLE).color(color::MUTED),
            text("?").size(type_scale::TITLE).color(color::MUTED),
        ]
    } else {
        let mut sentence = row![
            word("What happens when"),
            first,
            word("reacts with"),
            second
        ];
        // An empty second slot already shows "?", which completes the
        // question; the sentence's own mark appears once the slot fills.
        if !state.drafts[ActiveReactant::Second.index()]
            .atoms
            .is_empty()
        {
            sentence = sentence.push(word("?"));
        }
        sentence
    };

    sentence
        .spacing(if compact { spacing::SM } else { spacing::MD })
        .align_y(Center)
        .into()
}

fn reaction_prompt(state: &State, local: bool, compact: bool) -> Element<'static, Message> {
    let resolution = resolution(state);
    let both_present = state.drafts.iter().all(|draft| !draft.atoms.is_empty());
    let status_color = if resolution.is_system_error() {
        color::DANGER
    } else {
        color::WARNING
    };
    let resolution_status = resolution_status_message(&resolution, both_present).map(|message| {
        text(message.to_owned())
            .size(type_scale::CAPTION)
            .color(status_color)
    });
    let show_prompt = state.prompt_target != PromptTarget::Hidden
        || state.prompt_reveal > 0.0
        || resolution_status.is_none();
    let mut content = column![].spacing(spacing::XS).align_x(Center);
    if let Some(feedback) = &state.name_feedback {
        content = content.push(
            text(feedback.clone())
                .size(type_scale::CAPTION)
                .color(color::WARNING),
        );
    } else if show_prompt {
        let reveal = theme::ease_in_out(state.prompt_reveal);
        content = content.push(
            button(
                text(if state.prompt_target == PromptTarget::TryCodex {
                    "Try using Codex mode for this reaction"
                } else {
                    prompt_copy(&resolution, local)
                })
                .size(if compact {
                    type_scale::BODY_LARGE
                } else {
                    type_scale::TITLE
                })
                .font(SENTENCE_FONT),
            )
            .on_press_maybe(
                (state.prompt_target == PromptTarget::Submit)
                    .then_some(Message::StartReactionRequested),
            )
            .padding(0)
            .style(move |app_theme, status| theme::run_prompt(app_theme, status, reveal)),
        );
    } else if let Some(status) = resolution_status {
        content = content.push(status);
    }
    container(content)
        .width(Fill)
        .height(Length::Fixed(if compact { 38.0 } else { 44.0 }))
        .center_x(Fill)
        .center_y(Length::Fixed(if compact { 38.0 } else { 44.0 }))
        .into()
}

fn uses_codex_prompt(resolution: &chemistry::DraftResolution, local: bool) -> bool {
    !local && is_dynamic_resolution(resolution)
}

fn is_dynamic_resolution(resolution: &chemistry::DraftResolution) -> bool {
    matches!(
        resolution,
        chemistry::DraftResolution::ExplicitlyUnsupported(_)
            | chemistry::DraftResolution::Uncatalogued
            | chemistry::DraftResolution::Unrecognized
    )
}

fn prompt_copy(resolution: &chemistry::DraftResolution, local: bool) -> &'static str {
    if uses_codex_prompt(resolution, local) {
        "Press space to ask Codex"
    } else {
        "Press space to find out"
    }
}

fn resolution_status_message(
    resolution: &chemistry::DraftResolution,
    both_present: bool,
) -> Option<&str> {
    both_present
        .then_some(resolution)
        .filter(|resolution| {
            !matches!(
                resolution,
                chemistry::DraftResolution::Supported(_)
                    | chemistry::DraftResolution::Multiple(_)
                    | chemistry::DraftResolution::Screened(_)
            ) && !is_dynamic_resolution(resolution)
        })
        .and_then(chemistry::DraftResolution::inline_message)
}

fn slot(
    state: &State,
    reactant: ActiveReactant,
    library_drag: Option<u8>,
    compact: bool,
) -> Element<'static, Message> {
    let atoms = &state.drafts[reactant.index()].atoms;
    let selected = state.active == reactant;
    let state_color = slot_state_color(atoms);
    let draft_formula = state.drafts[reactant.index()]
        .display_formula
        .clone()
        .unwrap_or_else(|| formula(atoms));

    let empty = draft_formula.is_empty();
    if empty && state.editing == Some(reactant) {
        return text_input("Type a name or formula…", &state.name_input)
            .id(name_input_id(reactant))
            .on_input(Message::NameInput)
            .on_submit(Message::NameSubmitted)
            .size(if compact {
                type_scale::BODY_LARGE
            } else {
                type_scale::TITLE
            })
            .padding([spacing::XS, spacing::SM])
            .width(Length::Fixed(if compact { 190.0 } else { 280.0 }))
            .style(theme::request_input)
            .into();
    }
    let label = text(if empty { "?".to_owned() } else { draft_formula })
        .size(if compact {
            type_scale::TITLE
        } else {
            type_scale::DISPLAY
        })
        .font(FORMULA_FONT)
        .color(if empty { color::MUTED } else { color::TEXT });

    let chip = container(label)
        .style(move |_| theme::slot_chip(state_color, selected, false))
        .padding([spacing::XS, spacing::LG])
        .center_y(Length::Fixed(if compact { 44.0 } else { 58.0 }));

    // The hold-to-clear wheel fills over the chip while the press is held.
    let chip: Element<'static, Message> = match state.holding {
        Some(hold) if hold.slot == reactant && hold.progress > 0.12 && !atoms.is_empty() => stack![
            chip,
            container(
                canvas(HoldWheel {
                    progress: hold.progress,
                })
                .width(Length::Fixed(26.0))
                .height(Length::Fixed(26.0)),
            )
            .center(Fill),
        ]
        .into(),
        _ => chip.into(),
    };

    if empty && library_drag.is_none() {
        return mouse_area(
            button(chip)
                .on_press(Message::BeginNameEntry(reactant))
                .padding(0)
                .style(theme::bare_button),
        )
        .on_exit(Message::SlotExited(reactant))
        .interaction(iced::mouse::Interaction::Pointer)
        .into();
    }

    let area = mouse_area(chip)
        .on_exit(Message::SlotExited(reactant))
        .interaction(iced::mouse::Interaction::Pointer);
    let area: Element<'static, Message> = if let Some(atomic_number) = library_drag {
        area.on_release(Message::DropElement(reactant, atomic_number))
            .into()
    } else {
        area.on_press(Message::SlotPressed(reactant))
            .on_release(Message::SlotReleased(reactant))
            .into()
    };

    area
}

/// The draft state each slot border colour communicates.
fn slot_state_color(atoms: &[u8]) -> Color {
    if atoms.is_empty() {
        color::LINE_STRONG
    } else if atoms.len() == 1
        || composition_catalogue::trusted_preview(atoms.iter().copied()).is_some()
    {
        color::ACCENT
    } else {
        color::WARNING
    }
}

/// Radial progress shown while a slot is held; completing the ring clears it.
#[derive(Debug, Clone, Copy)]
struct HoldWheel {
    progress: f32,
}

impl<Message> iced::widget::canvas::Program<Message> for HoldWheel {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<iced::widget::canvas::Geometry> {
        let mut frame = iced::widget::canvas::Frame::new(renderer, bounds.size());
        let center = Point::new(bounds.width / 2.0, bounds.height / 2.0);
        let radius = bounds.width.min(bounds.height) / 2.0 - 3.0;

        frame.fill(
            &Path::circle(center, radius + 3.0),
            color::CANVAS.scale_alpha(0.6),
        );
        frame.stroke(
            &Path::circle(center, radius),
            Stroke::default()
                .with_color(color::LINE_STRONG)
                .with_width(2.5),
        );
        let sweep = self.progress.clamp(0.0, 1.0) * TAU;
        frame.stroke(
            &Path::new(|builder| {
                builder.arc(Arc {
                    center,
                    radius,
                    start_angle: Radians(-FRAC_PI_2),
                    end_angle: Radians(-FRAC_PI_2 + sweep),
                });
            }),
            Stroke::default().with_color(color::DANGER).with_width(2.5),
        );

        vec![frame.into_geometry()]
    }
}

pub fn formula(atoms: &[u8]) -> String {
    let atoms = chemistry::standardize_elemental_draft(atoms);
    let mut order = Vec::new();
    let mut counts = BTreeMap::<u8, usize>::new();
    for atomic_number in &atoms {
        if !counts.contains_key(atomic_number) {
            order.push(*atomic_number);
        }
        *counts.entry(*atomic_number).or_default() += 1;
    }

    order
        .into_iter()
        .fold(String::new(), |mut formula, atomic_number| {
            if let Some(element) = elements::by_atomic_number(atomic_number) {
                formula.push_str(element.symbol);
                let count = counts.get(&atomic_number).copied().unwrap_or(1);
                if count > 1 {
                    formula.push_str(&subscript(count));
                }
            }
            formula
        })
}

fn subscript(number: usize) -> String {
    number
        .to_string()
        .chars()
        .map(|digit| match digit {
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
            _ => digit,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn click_slot(state: &mut State, slot: ActiveReactant) {
        update(state, Message::SlotPressed(slot));
        update(state, Message::SlotReleased(slot));
    }

    #[test]
    fn sketched_reactants_fill_the_active_slot() {
        let mut state = State::default();
        update(&mut state, Message::SelectReactant(ActiveReactant::Second));
        set_sketched_reactant(&mut state, vec![6, 6, 1, 1, 1, 1, 1, 1], "CC".to_owned());

        assert_eq!(reactants(&state).1, [6, 6, 1, 1, 1, 1, 1, 1]);
        assert_eq!(draft_names(&state), [None, Some("CC")]);
    }

    #[test]
    fn typed_names_fill_the_active_slot() {
        let mut state = State::default();
        update(
            &mut state,
            Message::NameInput("copper(II) sulfate".to_owned()),
        );
        update(&mut state, Message::NameSubmitted);
        assert_eq!(reactants(&state).0, [29, 8, 8, 8, 8, 16]);
        assert!(state.name_feedback.is_none());
        assert!(state.name_input.is_empty());

        // The second slot fills once selected; formulas work too.
        update(&mut state, Message::SelectReactant(ActiveReactant::Second));
        update(&mut state, Message::NameInput("NaOH".to_owned()));
        update(&mut state, Message::NameSubmitted);
        assert_eq!(reactants(&state).1, [1, 11, 8]);

        // Gibberish leaves the draft alone and explains itself.
        update(&mut state, Message::NameInput("unobtainium".to_owned()));
        update(&mut state, Message::NameSubmitted);
        assert!(state.name_feedback.is_some());
        assert_eq!(reactants(&state).1, [1, 11, 8]);
    }

    #[test]
    fn typed_names_keep_their_canonical_display_formula_until_manually_edited() {
        let mut state = State::default();
        update(&mut state, Message::NameInput("sodium chloride".to_owned()));
        update(&mut state, Message::NameSubmitted);

        assert_eq!(draft_formulae(&state), ["NaCl", ""]);

        update(&mut state, Message::AddElement(1));
        assert_eq!(draft_names(&state), [None, None]);
        assert_eq!(draft_formulae(&state)[0], formula(reactants(&state).0));
    }

    #[test]
    fn typed_names_travel_with_drafts_until_edited() {
        let mut state = State::default();
        update(
            &mut state,
            Message::NameInput("ammonium cyanate".to_owned()),
        );
        update(&mut state, Message::NameSubmitted);
        assert_eq!(draft_names(&state), [Some("ammonium cyanate"), None]);

        // Any manual atom edit invalidates the name.
        update(&mut state, Message::AddElement(1));
        assert_eq!(draft_names(&state), [None, None]);

        // Separator entry names both slots.
        update(&mut state, Message::NameInput("urea + water".to_owned()));
        update(&mut state, Message::NameSubmitted);
        assert_eq!(draft_names(&state), [Some("urea"), Some("water")]);
        update(&mut state, Message::Undo);
        assert_eq!(draft_names(&state), [None, Some("water")]);
    }

    #[test]
    fn empty_slot_becomes_inline_input_and_ready_prompt_fades_both_ways() {
        let mut state = State::default();
        update(&mut state, Message::BeginNameEntry(ActiveReactant::First));
        assert_eq!(editing(&state), Some(ActiveReactant::First));

        update(&mut state, Message::NameInput("nickel".into()));
        update(&mut state, Message::NameSubmitted);
        assert_eq!(formula(reactants(&state).0), "Ni");
        assert_eq!(editing(&state), None);

        set_submit_available(&mut state, true);
        assert!(state.prompt_reveal.abs() < f32::EPSILON);
        update(&mut state, Message::PromptAnimationTick);
        assert!(state.prompt_reveal > 0.0);
        assert!(
            theme::ease_in_out(state.prompt_reveal) < 0.01,
            "the first opacity frame must not visibly pop"
        );

        set_submit_available(&mut state, false);
        let visible = state.prompt_reveal;
        assert!(visible > 0.0);
        assert!(prompt_is_animating(&state));
        update(&mut state, Message::PromptAnimationTick);
        assert!(state.prompt_reveal < visible);
        while state.prompt_reveal > 0.0 {
            update(&mut state, Message::PromptAnimationTick);
        }
        assert!(state.prompt_reveal.abs() < f32::EPSILON);
    }

    #[test]
    fn try_codex_notice_uses_the_prompt_fade_without_enabling_submission() {
        let mut state = State::default();

        show_try_codex_notice(&mut state);

        assert!(!submit_available(&state));
        assert_eq!(state.prompt_target, PromptTarget::TryCodex);
        assert!(prompt_is_animating(&state));
        update(&mut state, Message::PromptAnimationTick);
        assert!(state.prompt_reveal > 0.0);
    }

    #[test]
    fn separators_fill_both_slots_at_once() {
        let mut state = State::default();
        update(&mut state, Message::NameInput("oxygen + water".to_owned()));
        update(&mut state, Message::NameSubmitted);
        assert_eq!(reactants(&state), (&[8_u8, 8][..], &[1_u8, 1, 8][..]));
        assert!(state.name_input.is_empty());

        // "and" and commas separate too.
        update(
            &mut state,
            Message::NameInput("zinc and hydrochloric acid".to_owned()),
        );
        update(&mut state, Message::NameSubmitted);
        assert_eq!(reactants(&state), (&[30_u8][..], &[17_u8, 1][..]));

        // One bad half fails the whole submission and names the culprit.
        update(
            &mut state,
            Message::NameInput("oxygen + unobtainium".to_owned()),
        );
        update(&mut state, Message::NameSubmitted);
        assert!(
            state
                .name_feedback
                .as_deref()
                .is_some_and(|f| f.contains("unobtainium"))
        );
        assert_eq!(reactants(&state), (&[30_u8][..], &[17_u8, 1][..]));

        // Three reactants is one too many.
        update(
            &mut state,
            Message::NameInput("iron, sulfur, oxygen".to_owned()),
        );
        update(&mut state, Message::NameSubmitted);
        assert!(
            state
                .name_feedback
                .as_deref()
                .is_some_and(|f| f.contains("two"))
        );
    }

    #[test]
    fn progressive_selection_builds_the_annotated_carbon_dioxide_flow() {
        let mut state = State::default();
        update(&mut state, Message::AddElement(6));
        assert_eq!(formula(reactants(&state).0), "C");
        update(&mut state, Message::AddElement(8));
        assert_eq!(formula(reactants(&state).0), "CO");
        update(&mut state, Message::AddElement(8));
        assert_eq!(formula(reactants(&state).0), "CO₂");
        assert_eq!(
            composition_catalogue::recognize(reactants(&state).0.iter().copied())
                .map(|item| item.formula),
            Some("CO₂")
        );
    }

    #[test]
    fn switching_slots_keeps_reactants_independent() {
        let mut state = State::default();
        update(&mut state, Message::AddElement(37));
        click_slot(&mut state, ActiveReactant::Second);
        update(&mut state, Message::AddElement(3));

        assert_eq!(formula(reactants(&state).0), "Rb");
        assert_eq!(formula(reactants(&state).1), "Li");
        assert!(can_start_reaction(&state));
        assert_eq!(resolution(&state), chemistry::DraftResolution::Uncatalogued);
    }

    #[test]
    fn catalogue_pairs_run_immediately_and_missing_pairs_offer_codex() {
        let mut state = State::default();
        state.drafts[0].atoms = vec![47, 7, 8, 8, 8];
        state.drafts[1].atoms = vec![11, 17];
        assert!(can_start_reaction(&state));
        assert!(matches!(
            resolution(&state),
            chemistry::DraftResolution::Supported(_)
        ));
        assert_eq!(
            prompt_copy(&resolution(&state), false),
            "Press space to find out"
        );

        state.drafts[1].atoms = vec![11, 9];
        assert!(can_start_reaction(&state));
        let resolution = resolution(&state);
        assert_eq!(
            resolution.inline_message(),
            None,
            "dynamic states must never supply a competing inline status"
        );
        assert_eq!(prompt_copy(&resolution, false), "Press space to ask Codex");
        assert_eq!(prompt_copy(&resolution, true), "Press space to find out");
        assert_eq!(resolution_status_message(&resolution, true), None);
        set_submit_available(&mut state, true);
        let _ = reaction_prompt(&state, false, false);
    }

    #[test]
    fn unrecognised_drafts_remain_unrecognised() {
        let mut state = State::default();
        for atomic_number in [37, 6, 8, 8] {
            update(&mut state, Message::AddElement(atomic_number));
        }
        assert_eq!(formula(reactants(&state).0), "RbCO₂");
        assert!(composition_catalogue::recognize(reactants(&state).0.iter().copied()).is_none());
    }

    #[test]
    fn two_generated_compounds_resolve_as_uncatalogued_derivation_input() {
        let mut state = State::default();
        state.drafts[0].atoms = vec![1, 1, 16, 8, 8, 8, 8];
        state.drafts[1].atoms = vec![11, 8, 1];

        assert_eq!(formula(&state.drafts[0].atoms), "H₂SO₄");
        assert_eq!(formula(&state.drafts[1].atoms), "NaOH");
        assert_eq!(resolution(&state), chemistry::DraftResolution::Uncatalogued);
        assert!(can_start_reaction(&state));
        set_submit_available(&mut state, true);
        let _ = reaction_prompt(&state, true, false);
    }

    #[test]
    fn ambient_model_motion_runs_while_a_reactant_is_present() {
        let mut state = State::default();
        update(&mut state, Message::AnimationTick);
        assert!(state.orbital_phase.abs() < f32::EPSILON);

        update(&mut state, Message::AddElement(8));
        update(&mut state, Message::AnimationTick);
        assert!(state.orbital_phase > 0.0);
        assert!(state.ambient[0].reveal > 0.0);

        update(&mut state, Message::ClearActive);
        for _ in 0..16 {
            update(&mut state, Message::AnimationTick);
        }
        assert!(state.ambient[0].atoms.is_empty());
        assert!(state.ambient[0].reveal.abs() < f32::EPSILON);
    }

    #[test]
    fn ambient_resize_keeps_one_continuous_spring_across_live_events() {
        let mut state = State::default();
        update(&mut state, Message::AddElement(2));
        resize_ambient(&mut state, Size::new(1_200.0, 800.0));
        resize_ambient(&mut state, Size::new(760.0, 620.0));

        assert!((state.ambient_resize.current.width - 1_200.0).abs() < f32::EPSILON);
        assert!((state.ambient_resize.target.width - 760.0).abs() < f32::EPSILON);
        update(&mut state, Message::AnimationTick);
        assert!(state.ambient_resize.current.width < 1_200.0);
        let velocity = state.ambient_resize.velocity;

        // A new drag event updates only the destination. It does not reset or
        // add another impulse to the spring already in flight.
        resize_ambient(&mut state, Size::new(800.0, 640.0));
        assert!((state.ambient_resize.velocity.x - velocity.x).abs() < f32::EPSILON);
        assert!((state.ambient_resize.velocity.y - velocity.y).abs() < f32::EPSILON);

        for _ in 0..80 {
            update(&mut state, Message::AnimationTick);
        }
        assert!((state.ambient_resize.current.width - 800.0).abs() < 0.2);
        assert!((state.ambient_resize.current.height - 640.0).abs() < 0.2);
    }

    #[test]
    fn ambient_solver_adapts_to_footprint_viewport_and_screen_side() {
        let helium = [2];
        let water = [1, 1, 8];
        let large = [53, 9, 9, 9, 9, 9, 9, 9];

        for viewport in [Size::new(760.0, 620.0), Size::new(1_188.0, 768.0)] {
            for atoms in [&helium[..], &water[..], &large[..]] {
                for side in [ActiveReactant::First, ActiveReactant::Second] {
                    let anchor = solve_ambient_anchor(atoms, side, viewport);
                    let footprint = ambient_footprint(
                        atoms.len(),
                        Size::new(viewport.width * 0.5, viewport.height),
                        1.0,
                    );
                    let half_width = footprint.width / viewport.width;
                    let half_height = footprint.height / viewport.height / 2.0;
                    assert!(anchor.x - half_width >= 0.02);
                    assert!(anchor.x + half_width <= 0.98);
                    assert!(anchor.y - half_height >= 0.02);
                    assert!(anchor.y + half_height <= 0.74);
                }
            }
        }

        let viewport = Size::new(1_188.0, 768.0);
        let water_left = solve_ambient_anchor(&water, ActiveReactant::First, viewport);
        let water_right = solve_ambient_anchor(&water, ActiveReactant::Second, viewport);
        assert!(
            (water_left.x - water_right.x).abs() > 0.05
                || (water_left.y - water_right.y).abs() > 0.05,
            "asymmetric table blocks must produce side-specific placement"
        );
    }

    #[test]
    fn slot_clicks_select_then_undo_and_holding_clears() {
        let mut state = State::default();
        update(&mut state, Message::AddElement(3));
        click_slot(&mut state, ActiveReactant::Second);
        for atomic_number in [1, 1, 8] {
            update(&mut state, Message::AddElement(atomic_number));
        }

        // Clicking the already-active slot undoes its last element.
        click_slot(&mut state, ActiveReactant::Second);
        assert_eq!(reactants(&state).1, &[1, 1]);

        // Holding a slot to completion clears it, and the release that ends
        // the hold is consumed rather than treated as an undo click.
        update(&mut state, Message::SlotPressed(ActiveReactant::First));
        for _ in 0..24 {
            update(&mut state, Message::AnimationTick);
        }
        update(&mut state, Message::SlotReleased(ActiveReactant::First));
        assert!(reactants(&state).0.is_empty());
        assert_eq!(reactants(&state).1, &[1, 1]);

        // Leaving the slot cancels an in-flight hold.
        update(&mut state, Message::SlotPressed(ActiveReactant::Second));
        update(&mut state, Message::AnimationTick);
        update(&mut state, Message::SlotExited(ActiveReactant::Second));
        update(&mut state, Message::AnimationTick);
        assert_eq!(reactants(&state).1, &[1, 1]);
    }

    #[test]
    fn undo_and_clear_edit_only_the_active_draft() {
        let mut state = State::default();
        update(&mut state, Message::AddElement(3));
        click_slot(&mut state, ActiveReactant::Second);
        for atomic_number in [1, 1, 8] {
            update(&mut state, Message::AddElement(atomic_number));
        }

        update(&mut state, Message::Undo);
        assert_eq!(reactants(&state).1, &[1, 1]);
        update(&mut state, Message::ClearActive);
        assert!(reactants(&state).1.is_empty());
        assert_eq!(reactants(&state).0, &[3]);
    }
}
