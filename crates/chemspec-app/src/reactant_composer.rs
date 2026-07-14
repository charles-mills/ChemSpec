//! Stage 1 structured reactant composer (`U-112`).
//!
//! This module records user intent only. Formulae and recognised compositions
//! are presentation previews and never become validated chemistry here.

use std::collections::BTreeMap;
use std::time::Duration;

use iced::widget::{button, canvas, column, container, mouse_area, row, space, text};
use iced::{Element, Fill, FillPortion, Length, Padding, Subscription};

use crate::composition_catalogue::{self, CompositionPreview};
use crate::elements;
use crate::particle_visualization::{AtomDiagram, CompoundAtomicDiagram};
use crate::reaction_candidate_catalogue;
use crate::theme::{self, color, space as spacing, type_scale};

const MAX_ATOMS_PER_REACTANT: usize = 12;

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

    const fn number(self) -> u8 {
        match self {
            Self::First => 1,
            Self::Second => 2,
        }
    }
}

#[derive(Debug, Default)]
struct ReactantDraft {
    atoms: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SelectionEvent {
    atomic_number: u8,
    reactant: ActiveReactant,
}

#[derive(Debug)]
pub struct State {
    drafts: [ReactantDraft; 2],
    active: ActiveReactant,
    history: Vec<SelectionEvent>,
    limit_reached: bool,
    reduced_motion: bool,
    orbital_phase: f32,
}

impl Default for State {
    fn default() -> Self {
        Self {
            drafts: [ReactantDraft::default(), ReactantDraft::default()],
            active: ActiveReactant::First,
            history: Vec::new(),
            limit_reached: false,
            reduced_motion: false,
            orbital_phase: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Message {
    AddElement(u8),
    DropElement(ActiveReactant, u8),
    Activate(ActiveReactant),
    Undo,
    ClearActive,
    Swap,
    StartReactionRequested,
    MotionToggled,
    AnimationTick,
}

pub fn update(state: &mut State, message: Message) {
    match message {
        Message::AddElement(atomic_number) => add_element(state, state.active, atomic_number),
        Message::DropElement(reactant, atomic_number) => {
            state.active = reactant;
            add_element(state, reactant, atomic_number);
        }
        Message::Activate(reactant) => {
            state.active = reactant;
            state.limit_reached = false;
        }
        Message::Undo => {
            let active = state.active;
            if state.drafts[active.index()].atoms.pop().is_some()
                && let Some(index) = state
                    .history
                    .iter()
                    .rposition(|event| event.reactant == active)
            {
                state.history.remove(index);
            }
            state.limit_reached = false;
        }
        Message::ClearActive => {
            let active = state.active;
            state.drafts[active.index()].atoms.clear();
            state.history.retain(|event| event.reactant != active);
            state.limit_reached = false;
        }
        Message::Swap => {
            state.drafts.swap(0, 1);
            for event in &mut state.history {
                event.reactant = match event.reactant {
                    ActiveReactant::First => ActiveReactant::Second,
                    ActiveReactant::Second => ActiveReactant::First,
                };
            }
            state.active = match state.active {
                ActiveReactant::First => ActiveReactant::Second,
                ActiveReactant::Second => ActiveReactant::First,
            };
        }
        Message::StartReactionRequested => {}
        Message::MotionToggled => {
            state.reduced_motion = !state.reduced_motion;
            if state.reduced_motion {
                state.orbital_phase = 0.0;
            }
        }
        Message::AnimationTick => {
            if !state.reduced_motion {
                state.orbital_phase = (state.orbital_phase + 0.004) % 1.0;
            }
        }
    }
}

pub fn subscription(state: &State) -> Subscription<Message> {
    let has_atoms = state.drafts.iter().any(|draft| !draft.atoms.is_empty());
    if has_atoms && !state.reduced_motion {
        iced::time::every(Duration::from_millis(50)).map(|_| Message::AnimationTick)
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
    state.history.push(SelectionEvent {
        atomic_number,
        reactant,
    });
    state.limit_reached = false;
}

pub fn can_start_reaction(state: &State) -> bool {
    reaction_candidate_catalogue::recognize_drafts(&state.drafts[0].atoms, &state.drafts[1].atoms)
        .is_some()
}

pub fn reactants(state: &State) -> (&[u8], &[u8]) {
    (&state.drafts[0].atoms, &state.drafts[1].atoms)
}

pub fn view(state: &State, library_drag: Option<u8>, compact: bool) -> Element<'static, Message> {
    let active_atoms = &state.drafts[state.active.index()].atoms;
    let preview = composition_catalogue::recognize(active_atoms.iter().copied());
    let preview_panel = active_preview(state, active_atoms, preview);
    let equation = equation_panel(state, library_drag, compact);
    let history = history_panel(state);

    let body: Element<'static, Message> = if compact {
        column![
            container(equation).height(FillPortion(7)),
            container(row![preview_panel, history].spacing(spacing::XS)).height(FillPortion(3)),
        ]
        .spacing(spacing::XS)
        .height(Fill)
        .into()
    } else {
        row![preview_panel, equation, history]
            .spacing(spacing::XS)
            .height(Fill)
            .into()
    };

    container(
        column![
            row![
                text("STAGE 1  /  BUILD REACTANTS")
                    .size(type_scale::MICRO)
                    .color(color::ACCENT),
                text("Construct the reaction question")
                    .size(type_scale::BODY_LARGE)
                    .color(color::TEXT),
                space().width(Fill),
                text(if compact {
                    "PREVIEW · UNVALIDATED"
                } else {
                    "COMPOSITION PREVIEW · VALIDATION REQUIRED"
                })
                .size(type_scale::MICRO)
                .color(color::MUTED),
            ]
            .spacing(spacing::SM)
            .align_y(iced::Center),
            body,
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

fn active_preview(
    state: &State,
    atoms: &[u8],
    preview: Option<CompositionPreview>,
) -> Element<'static, Message> {
    let formula = formula(atoms);
    let status = if state.limit_reached {
        "ATOM LIMIT REACHED"
    } else if preview.is_some() {
        "RECOGNISED COMPOSITION PREVIEW"
    } else if atoms.is_empty() {
        "WAITING FOR AN ELEMENT"
    } else {
        "UNRECOGNISED OR INTERMEDIATE DRAFT"
    };
    let status_color = if preview.is_some() {
        color::SUCCESS
    } else if state.limit_reached {
        color::WARNING
    } else {
        color::MUTED
    };

    let model_content = atomic_model_preview(atoms, preview, state.orbital_phase);
    let motion = button(text(if state.reduced_motion {
        "Resume electrons"
    } else {
        "Pause electrons"
    }))
    .on_press_maybe((!atoms.is_empty()).then_some(Message::MotionToggled))
    .padding([spacing::XXS, spacing::XS])
    .style(theme::secondary_button);

    container(
        column![
            text(format!("REACTANT {} MODEL", state.active.number()))
                .size(type_scale::MICRO)
                .color(color::FAINT),
            container(model_content)
                .center_x(Fill)
                .center_y(Fill)
                .height(Fill),
            text(if formula.is_empty() {
                "—".to_owned()
            } else {
                formula.clone()
            })
            .size(type_scale::TITLE)
            .color(color::TEXT),
            row![
                text(preview.map_or("Current atomic draft", |item| item.name))
                    .size(type_scale::CAPTION)
                    .color(status_color),
                space().width(Fill),
                motion,
            ]
            .align_y(iced::Center),
            text(status).size(type_scale::MICRO).color(status_color),
        ]
        .spacing(spacing::XXS)
        .height(Fill),
    )
    .style(theme::inset)
    .padding(spacing::SM)
    .width(FillPortion(3))
    .height(Fill)
    .into()
}

fn atomic_model_preview(
    atoms: &[u8],
    preview: Option<CompositionPreview>,
    orbital_phase: f32,
) -> Element<'static, Message> {
    if atoms.is_empty() {
        text("Select an element below")
            .size(type_scale::BODY)
            .color(color::MUTED)
            .into()
    } else if let Some(preview) = preview {
        let members = atoms
            .iter()
            .filter_map(|number| elements::by_atomic_number(*number).copied());
        canvas(CompoundAtomicDiagram::new(preview, members, orbital_phase))
            .width(Fill)
            .height(Fill)
            .into()
    } else {
        atoms
            .chunks(4)
            .take(2)
            .fold(column![].spacing(spacing::XXS), |models, chunk| {
                models.push(
                    chunk
                        .iter()
                        .fold(row![].spacing(spacing::XXS), |row, number| {
                            let model: Element<'static, Message> =
                                elements::by_atomic_number(*number).map_or_else(
                                    || space().into(),
                                    |element| {
                                        canvas(AtomDiagram::new(*element, orbital_phase))
                                            .width(Length::Fixed(52.0))
                                            .height(Length::Fixed(52.0))
                                            .into()
                                    },
                                );
                            row.push(model)
                        }),
                )
            })
            .into()
    }
}

fn equation_panel(
    state: &State,
    library_drag: Option<u8>,
    compact: bool,
) -> Element<'static, Message> {
    let first = formula_slot(state, ActiveReactant::First, library_drag);
    let second = formula_slot(state, ActiveReactant::Second, library_drag);
    let equation = row![
        first,
        text("+").size(type_scale::DISPLAY).color(color::TEXT_SOFT),
        second,
        text("→").size(type_scale::DISPLAY).color(color::ACCENT),
    ]
    .spacing(if compact { spacing::XS } else { spacing::MD })
    .align_y(iced::Center);

    let undo = button(text("Undo"))
        .on_press_maybe(
            (!state.drafts[state.active.index()].atoms.is_empty()).then_some(Message::Undo),
        )
        .style(theme::secondary_button);
    let clear = button(text("Clear active"))
        .on_press_maybe(
            (!state.drafts[state.active.index()].atoms.is_empty()).then_some(Message::ClearActive),
        )
        .style(theme::secondary_button);
    let swap = button(text("Swap"))
        .on_press_maybe(
            state
                .drafts
                .iter()
                .all(|draft| !draft.atoms.is_empty())
                .then_some(Message::Swap),
        )
        .style(theme::secondary_button);
    let continue_button = button(text(if can_start_reaction(state) {
        "Start reaction  →"
    } else {
        "Set a supported reaction"
    }))
    .on_press_maybe(can_start_reaction(state).then_some(Message::StartReactionRequested))
    .style(theme::primary_button);

    container(
        column![
            text("REACTION EQUATION")
                .size(type_scale::MICRO)
                .color(color::FAINT),
            equation,
            text("Select a slot, then click or drag elements from the table")
                .size(type_scale::CAPTION)
                .color(color::MUTED),
            row![undo, clear, swap, space().width(Fill), continue_button].spacing(spacing::XS),
        ]
        .spacing(spacing::XS)
        .height(Fill),
    )
    .style(theme::panel)
    .padding(spacing::SM)
    .width(FillPortion(7))
    .height(Fill)
    .into()
}

fn formula_slot(
    state: &State,
    reactant: ActiveReactant,
    library_drag: Option<u8>,
) -> Element<'static, Message> {
    let active = state.active == reactant;
    let formula = formula(&state.drafts[reactant.index()].atoms);
    let content = container(
        column![
            text(format!("REACTANT {}", reactant.number()))
                .size(type_scale::MICRO)
                .color(if active { color::ACCENT } else { color::FAINT }),
            text(if formula.is_empty() {
                "Select".to_owned()
            } else {
                formula.clone()
            })
            .size(type_scale::DISPLAY)
            .color(if formula.is_empty() {
                color::MUTED
            } else {
                color::TEXT
            }),
            text(if active {
                "ACTIVE INPUT"
            } else {
                "SELECT TO EDIT"
            })
            .size(type_scale::MICRO)
            .color(if active { color::ACCENT } else { color::MUTED }),
        ]
        .spacing(spacing::XXS),
    )
    .style(if active {
        theme::accent_tint
    } else {
        theme::raised
    })
    .padding(Padding::new(spacing::XS))
    .width(Fill)
    .height(Length::Fixed(76.0));

    let area = mouse_area(content).on_press(Message::Activate(reactant));
    if let Some(atomic_number) = library_drag {
        area.on_release(Message::DropElement(reactant, atomic_number))
            .into()
    } else {
        area.into()
    }
}

fn history_panel(state: &State) -> Element<'static, Message> {
    let events: iced::widget::Column<'static, Message> = state.history.iter().rev().take(6).fold(
        column![].spacing(spacing::XXS),
        |events, event| {
            let label = elements::by_atomic_number(event.atomic_number)
                .map_or("Unknown selected".to_owned(), |element| {
                    format!("{} added · R{}", element.name, event.reactant.number())
                });
            events.push(
                text(label)
                    .size(type_scale::CAPTION)
                    .color(color::TEXT_SOFT),
            )
        },
    );
    let event_content: Element<'static, Message> = if state.history.is_empty() {
        text("Selections appear here")
            .size(type_scale::CAPTION)
            .color(color::MUTED)
            .into()
    } else {
        events.into()
    };

    container(
        column![
            text("INPUT HISTORY")
                .size(type_scale::MICRO)
                .color(color::FAINT),
            event_content,
            space().height(Fill),
            text("History is interface feedback, not evidence")
                .size(type_scale::MICRO)
                .color(color::FAINT),
        ]
        .spacing(spacing::XS)
        .height(Fill),
    )
    .style(theme::inset)
    .padding(spacing::SM)
    .width(FillPortion(3))
    .height(Fill)
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
        update(&mut state, Message::Activate(ActiveReactant::Second));
        update(&mut state, Message::AddElement(3));

        assert_eq!(formula(reactants(&state).0), "Rb");
        assert_eq!(formula(reactants(&state).1), "Li");
        assert!(!can_start_reaction(&state));
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
    fn visible_model_orbits_slowly_and_can_be_paused() {
        let mut state = State::default();
        update(&mut state, Message::AddElement(8));
        update(&mut state, Message::AnimationTick);
        assert!((state.orbital_phase - 0.004).abs() < f32::EPSILON);

        update(&mut state, Message::MotionToggled);
        update(&mut state, Message::AnimationTick);
        assert!(state.reduced_motion);
        assert!(state.orbital_phase.abs() < f32::EPSILON);
    }
}
