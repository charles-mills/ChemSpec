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
    button, canvas, column, container, mouse_area, row, space, stack, text, tooltip,
};
use iced::{
    Center, Color, Element, Fill, Font, Length, Point, Radians, Rectangle, Renderer, Subscription,
    Theme, font,
};

use crate::chemistry;
use crate::composition_catalogue;
use crate::elements::{self, ElementSpec};
use crate::particle_visualization::{AtomDiagram, CompoundAtomicDiagram};
use crate::theme::{self, color, motion, space as spacing, type_scale};

const MAX_ATOMS_PER_REACTANT: usize = 12;

const SENTENCE_FONT: Font = Font {
    weight: font::Weight::Medium,
    ..Font::DEFAULT
};
const FORMULA_FONT: Font = Font {
    weight: font::Weight::Semibold,
    ..Font::DEFAULT
};

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
}

/// An in-flight press on a slot: a quick release clicks (select or undo),
/// while holding to completion clears the slot.
#[derive(Debug, Clone, Copy)]
struct HoldState {
    slot: ActiveReactant,
    progress: f32,
    completed: bool,
}

#[derive(Debug)]
pub struct State {
    drafts: [ReactantDraft; 2],
    active: ActiveReactant,
    limit_reached: bool,
    hovered: Option<ActiveReactant>,
    holding: Option<HoldState>,
    orbital_phase: f32,
    tooltip_reveal: f32,
}

impl Default for State {
    fn default() -> Self {
        Self {
            drafts: [ReactantDraft::default(), ReactantDraft::default()],
            active: ActiveReactant::First,
            limit_reached: false,
            hovered: None,
            holding: None,
            orbital_phase: 0.0,
            tooltip_reveal: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Message {
    AddElement(u8),
    DropElement(ActiveReactant, u8),
    SlotPressed(ActiveReactant),
    SlotReleased(ActiveReactant),
    SlotHovered(Option<ActiveReactant>),
    Undo,
    ClearActive,
    StartReactionRequested,
    AnimationTick,
}

pub fn update(state: &mut State, message: Message) {
    match message {
        Message::AddElement(atomic_number) => add_element(state, state.active, atomic_number),
        Message::DropElement(reactant, atomic_number) => {
            state.active = reactant;
            add_element(state, reactant, atomic_number);
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
                state.drafts[reactant.index()].atoms.pop();
            } else {
                state.active = reactant;
            }
            state.limit_reached = false;
        }
        Message::SlotHovered(hovered) => {
            if state.hovered != hovered {
                state.tooltip_reveal = 0.0;
                state.holding = None;
            }
            state.hovered = hovered;
        }
        Message::Undo => {
            state.drafts[state.active.index()].atoms.pop();
            state.limit_reached = false;
        }
        Message::ClearActive => {
            state.drafts[state.active.index()].atoms.clear();
            state.limit_reached = false;
        }
        Message::StartReactionRequested => {}
        Message::AnimationTick => {
            if state.hovered.is_some() {
                state.orbital_phase = (state.orbital_phase + motion::ORBIT_STEP) % 1.0;
                state.tooltip_reveal = (state.tooltip_reveal + motion::REVEAL_STEP).min(1.0);
            }
            if let Some(hold) = &mut state.holding
                && !hold.completed
                && !state.drafts[hold.slot.index()].atoms.is_empty()
            {
                hold.progress += motion::HOLD_CLEAR_STEP;
                if hold.progress >= 1.0 {
                    hold.completed = true;
                    state.drafts[hold.slot.index()].atoms.clear();
                    state.limit_reached = false;
                }
            }
        }
    }
}

/// The composer only animates while a slot tooltip is open or a hold-to-clear
/// gesture is running, so moving the cursor away is the pause control.
pub fn subscription(state: &State) -> Subscription<Message> {
    let tooltip_open = state
        .hovered
        .is_some_and(|slot| !state.drafts[slot.index()].atoms.is_empty());
    let hold_running = state
        .holding
        .is_some_and(|hold| !hold.completed && !state.drafts[hold.slot.index()].atoms.is_empty());
    if tooltip_open || hold_running {
        iced::time::every(motion::TICK).map(|_| Message::AnimationTick)
    } else {
        Subscription::none()
    }
}

fn add_element(state: &mut State, reactant: ActiveReactant, atomic_number: u8) {
    if elements::by_atomic_number(atomic_number).is_none() {
        return;
    }
    let atoms = &mut state.drafts[reactant.index()].atoms;
    if atoms.len() >= MAX_ATOMS_PER_REACTANT {
        state.limit_reached = true;
        return;
    }
    atoms.push(atomic_number);
    state.limit_reached = false;
}

pub fn can_start_reaction(state: &State) -> bool {
    matches!(resolution(state), chemistry::DraftResolution::Supported(_))
}

pub fn resolution(state: &State) -> chemistry::DraftResolution {
    chemistry::resolve_drafts(&state.drafts[0].atoms, &state.drafts[1].atoms)
}

#[cfg(test)]
pub fn reactants(state: &State) -> (&[u8], &[u8]) {
    (&state.drafts[0].atoms, &state.drafts[1].atoms)
}

#[cfg(test)]
pub fn replace_reactants(state: &mut State, drafts: [Vec<u8>; 2]) {
    state.drafts = drafts.map(|atoms| ReactantDraft { atoms });
    state.active = ActiveReactant::First;
    state.limit_reached = false;
}

pub fn view(state: &State, library_drag: Option<u8>, compact: bool) -> Element<'static, Message> {
    let sentence = sentence(state, library_drag, compact);
    let actions = action_row(state);

    container(
        column![sentence, actions]
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

fn action_row(state: &State) -> Element<'static, Message> {
    let active_atoms = &state.drafts[state.active.index()].atoms;
    let run = button(text("Run reaction  →").size(type_scale::BODY))
        .on_press_maybe(can_start_reaction(state).then_some(Message::StartReactionRequested))
        .padding([spacing::XS, spacing::MD])
        .style(theme::primary_button);
    let undo = button(text("Undo").size(type_scale::BODY))
        .on_press_maybe((!active_atoms.is_empty()).then_some(Message::Undo))
        .padding([spacing::XS, spacing::MD])
        .style(theme::secondary_button);
    let clear = button(text("Clear").size(type_scale::BODY))
        .on_press_maybe((!active_atoms.is_empty()).then_some(Message::ClearActive))
        .padding([spacing::XS, spacing::MD])
        .style(theme::secondary_button);

    let controls = row![
        run,
        action_hint(undo, "You can also click the selected box to undo."),
        action_hint(clear, "You can also hold a box to clear it."),
    ]
    .spacing(spacing::XS)
    .align_y(Center);
    let both_present = state.drafts.iter().all(|draft| !draft.atoms.is_empty());
    let resolution = resolution(state);
    let status_color = if resolution.is_system_error() {
        color::DANGER
    } else {
        color::WARNING
    };
    let status = both_present
        .then(|| resolution.message())
        .flatten()
        .map(|message| {
            text(message.to_owned())
                .size(type_scale::CAPTION)
                .color(status_color)
        });
    let mut content = column![controls].spacing(spacing::XS).align_x(Center);
    if let Some(status) = status {
        content = content.push(status);
    }
    content.into()
}

/// A small gesture hint under an action button.
fn action_hint(
    control: iced::widget::Button<'static, Message>,
    hint: &'static str,
) -> Element<'static, Message> {
    tooltip(
        control,
        text(hint).size(type_scale::CAPTION).color(color::TEXT_SOFT),
        tooltip::Position::Bottom,
    )
    .gap(spacing::XS)
    .padding(spacing::XS)
    .style(|_| theme::tooltip_surface(1.0))
    .into()
}

fn slot(
    state: &State,
    reactant: ActiveReactant,
    library_drag: Option<u8>,
    compact: bool,
) -> Element<'static, Message> {
    let atoms = &state.drafts[reactant.index()].atoms;
    let selected = state.active == reactant;
    let hovered = state.hovered == Some(reactant);
    let state_color = slot_state_color(atoms);
    let draft_formula = formula(atoms);

    let empty = draft_formula.is_empty();
    let label = text(if empty { "?".to_owned() } else { draft_formula })
        .size(if compact {
            type_scale::TITLE
        } else {
            type_scale::DISPLAY
        })
        .font(FORMULA_FONT)
        .color(if empty { color::MUTED } else { color::TEXT });

    let chip = container(label)
        .style(move |_| theme::slot_chip(state_color, selected, hovered))
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

    let area = mouse_area(chip)
        .on_enter(Message::SlotHovered(Some(reactant)))
        .on_exit(Message::SlotHovered(None))
        .interaction(iced::mouse::Interaction::Pointer);
    let area: Element<'static, Message> = if let Some(atomic_number) = library_drag {
        area.on_release(Message::DropElement(reactant, atomic_number))
            .into()
    } else {
        area.on_press(Message::SlotPressed(reactant))
            .on_release(Message::SlotReleased(reactant))
            .into()
    };

    // An empty slot has no model to explain, so it carries no tooltip.
    if atoms.is_empty() {
        return area;
    }

    let reveal = theme::ease_out(state.tooltip_reveal);
    tooltip(
        area,
        model_card(state, reactant, reveal),
        tooltip::Position::Bottom,
    )
    .gap(spacing::SM)
    .padding(spacing::SM)
    .style(move |_| theme::tooltip_surface(reveal))
    .into()
}

/// The draft state each slot border colour communicates; the matching words
/// live in the slot tooltip.
fn slot_state_color(atoms: &[u8]) -> Color {
    if atoms.is_empty() {
        color::LINE_STRONG
    } else if atoms.len() == 1 || composition_catalogue::recognize(atoms.iter().copied()).is_some()
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

/// The hover tooltip: the atomic or compound model, its chemical name, the
/// state the slot border colour stands for, and the click/hold affordances.
fn model_card(state: &State, reactant: ActiveReactant, reveal: f32) -> Element<'static, Message> {
    let mut card = column![draft_body(state, reactant, reveal)].spacing(spacing::XS);
    if state.limit_reached && state.active == reactant {
        card = card.push(
            row![
                text("●")
                    .size(type_scale::MICRO)
                    .color(color::WARNING.scale_alpha(reveal)),
                text("Atom limit reached — undo or clear")
                    .size(type_scale::CAPTION)
                    .color(color::WARNING.scale_alpha(reveal)),
            ]
            .spacing(spacing::XXS)
            .align_y(Center),
        );
    }
    card.into()
}

/// The model row of a non-empty slot tooltip.
fn draft_body(state: &State, reactant: ActiveReactant, reveal: f32) -> Element<'static, Message> {
    let atoms = &state.drafts[reactant.index()].atoms;
    let phase = state.orbital_phase;

    if let [atomic_number] = atoms.as_slice()
        && let Some(element) = elements::by_atomic_number(*atomic_number)
    {
        return element_card(*element, phase, reveal);
    }

    let members = atoms
        .iter()
        .filter_map(|number| elements::by_atomic_number(*number).copied());
    let preview = composition_catalogue::recognize(atoms.iter().copied());
    let (model, name, status, status_color): (
        Element<'static, Message>,
        Option<&'static str>,
        &'static str,
        _,
    ) = if let Some(preview) = preview {
        (
            canvas(CompoundAtomicDiagram::new(preview, members, phase).with_reveal(reveal))
                .width(Length::Fixed(200.0))
                .height(Length::Fixed(110.0))
                .into(),
            Some(preview.name),
            preview.kind().recognition_label(),
            color::SUCCESS,
        )
    } else {
        (
            draft_model_grid(atoms, phase, reveal),
            None,
            "Unrecognised draft",
            color::WARNING,
        )
    };

    let mut details = column![].spacing(spacing::XXS);
    if let Some(name) = name {
        details = details.push(
            text(name)
                .size(type_scale::BODY_LARGE)
                .font(FORMULA_FONT)
                .color(color::TEXT.scale_alpha(reveal)),
        );
    }
    details = details.push(
        row![
            text("●")
                .size(type_scale::MICRO)
                .color(status_color.scale_alpha(reveal)),
            text(status)
                .size(type_scale::CAPTION)
                .color(status_color.scale_alpha(reveal)),
        ]
        .spacing(spacing::XXS)
        .align_y(Center),
    );

    row![model, details]
        .spacing(spacing::SM)
        .align_y(Center)
        .into()
}

fn element_card(element: ElementSpec, phase: f32, reveal: f32) -> Element<'static, Message> {
    let family_color = theme::category_color(element.category);
    let details = column![
        row![
            text(element.name)
                .size(type_scale::BODY_LARGE)
                .font(FORMULA_FONT)
                .color(color::TEXT.scale_alpha(reveal)),
            text(format!("· {}", element.atomic_number))
                .size(type_scale::CAPTION)
                .color(color::MUTED.scale_alpha(reveal)),
        ]
        .spacing(spacing::XS)
        .align_y(Center),
        row![
            text("■")
                .size(type_scale::MICRO)
                .color(family_color.scale_alpha(reveal)),
            text(element.category.label())
                .size(type_scale::CAPTION)
                .color(color::TEXT_SOFT.scale_alpha(reveal)),
        ]
        .spacing(spacing::XXS)
        .align_y(Center),
    ]
    .spacing(spacing::XXS);

    row![
        canvas(AtomDiagram::new(element, phase).with_reveal(reveal))
            .width(Length::Fixed(92.0))
            .height(Length::Fixed(92.0)),
        details,
    ]
    .spacing(spacing::SM)
    .align_y(Center)
    .into()
}

/// Unrecognised drafts show every member atom's shell model in a small grid.
fn draft_model_grid(atoms: &[u8], phase: f32, reveal: f32) -> Element<'static, Message> {
    atoms
        .chunks(4)
        .take(3)
        .fold(column![].spacing(spacing::XXS), |models, chunk| {
            models.push(
                chunk
                    .iter()
                    .fold(row![].spacing(spacing::XXS), |model_row, number| {
                        let model: Element<'static, Message> = elements::by_atomic_number(*number)
                            .map_or_else(
                                || space().into(),
                                |element| {
                                    canvas(AtomDiagram::new(*element, phase).with_reveal(reveal))
                                        .width(Length::Fixed(48.0))
                                        .height(Length::Fixed(48.0))
                                        .into()
                                },
                            );
                        model_row.push(model)
                    }),
            )
        })
        .into()
}

fn formula(atoms: &[u8]) -> String {
    let mut order = Vec::new();
    let mut counts = BTreeMap::<u8, usize>::new();
    for atomic_number in atoms {
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
        assert!(!can_start_reaction(&state));
    }

    #[test]
    fn broader_trusted_pairs_enable_run_and_unsupported_pairs_explain_the_block() {
        let mut state = State::default();
        state.drafts[0].atoms = vec![47, 7, 8, 8, 8];
        state.drafts[1].atoms = vec![11, 17];
        assert!(can_start_reaction(&state));
        assert!(matches!(
            resolution(&state),
            chemistry::DraftResolution::Supported(_)
        ));

        state.drafts[1].atoms = vec![11, 9];
        assert!(!can_start_reaction(&state));
        let resolution = resolution(&state);
        assert!(
            resolution
                .message()
                .is_some_and(|message| message.starts_with("Silver fluoride is soluble"))
        );
        let _ = action_row(&state);
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
    fn model_motion_runs_only_while_a_slot_tooltip_is_hovered() {
        let mut state = State::default();
        update(&mut state, Message::AddElement(8));

        // No hover: ticks are inert.
        update(&mut state, Message::AnimationTick);
        assert!(state.orbital_phase.abs() < f32::EPSILON);

        update(
            &mut state,
            Message::SlotHovered(Some(ActiveReactant::First)),
        );
        update(&mut state, Message::AnimationTick);
        assert!(state.orbital_phase > 0.0);
        assert!(state.tooltip_reveal > 0.0);

        // Leaving the slot resets the reveal so the next open fades in again.
        update(&mut state, Message::SlotHovered(None));
        assert!(state.tooltip_reveal.abs() < f32::EPSILON);
        let frozen = state.orbital_phase;
        update(&mut state, Message::AnimationTick);
        assert!((state.orbital_phase - frozen).abs() < f32::EPSILON);
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
        update(&mut state, Message::SlotHovered(None));
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
