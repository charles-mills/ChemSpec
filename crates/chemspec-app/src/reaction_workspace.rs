//! Stages 2–5: direct manipulation, atomic models, guarded reaction trigger,
//! and the illustrative 2D reaction storyboard.

use std::collections::BTreeSet;
use std::time::Duration;

use iced::event;
use iced::mouse;
use iced::widget::{
    button, canvas, column, container, mouse_area, responsive, row, space, stack, text,
};
use iced::{
    Background, Border, Color, Element, Fill, Length, Padding, Point, Shadow, Size, Subscription,
    Vector, border,
};

use crate::composition_catalogue::{self, CompositionPreview};
use crate::elements::{self, ElementSpec};
use crate::particle_visualization::{AtomDiagram, CompoundAtomicDiagram};
use crate::reaction_candidate_catalogue::{self, Participant, ReactionCandidate};
use crate::reaction_sequence::{self, ReactionSequenceDiagram};
use crate::theme::{self, color, radius, space as spacing, type_scale};

const ATOM_WIDTH: f32 = 104.0;
const ATOM_HEIGHT: f32 = 112.0;
const COMPOUND_WIDTH: f32 = 190.0;
const COMPOUND_HEIGHT: f32 = 118.0;
const GROUP_DISTANCE: f32 = 0.115;
const MAX_ATOMS: usize = 24;
const SETTLE_EPSILON: f32 = 0.000_1;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedAtom {
    id: u32,
    atomic_number: u8,
    position: Point,
    target: Point,
}

#[derive(Debug, Clone, PartialEq)]
struct DragState {
    anchor_id: u32,
    members: Vec<DragMember>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct DragMember {
    atom_id: u32,
    offset: Point,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Feedback {
    Ready,
    Added,
    Repositioned,
    Removed,
    Cleared,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TriggerState {
    Idle,
    Queued(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlaybackState {
    Playing,
    Paused,
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct SequenceState {
    candidate: ReactionCandidate,
    progress: f32,
    playback: PlaybackState,
}

#[derive(Debug)]
pub struct State {
    atoms: Vec<PlacedAtom>,
    selected: Option<u32>,
    hovered: Option<u32>,
    dragging: Option<DragState>,
    pointer: Point,
    next_id: u32,
    feedback: Feedback,
    reduced_motion: bool,
    orbital_phase: f32,
    trigger: TriggerState,
    trigger_reveal: f32,
    sequence: Option<SequenceState>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            atoms: Vec::new(),
            selected: None,
            hovered: None,
            dragging: None,
            pointer: Point::new(0.5, 0.5),
            next_id: 1,
            feedback: Feedback::Ready,
            reduced_motion: false,
            orbital_phase: 0.0,
            trigger: TriggerState::Idle,
            trigger_reveal: 0.0,
            sequence: None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Message {
    PointerMoved(Point),
    LibraryElementDropped(u8),
    AtomHovered(Option<u32>),
    AtomDragStarted(u32),
    AtomDragEnded,
    RemoveSelected,
    ClearAll,
    MotionToggled,
    StartReaction,
    PlaybackToggled,
    SequenceRestarted,
    SequenceSkipped,
    WorkspaceReturned,
    AnimationTick,
}

pub fn update(state: &mut State, message: Message) {
    match message {
        Message::PointerMoved(position) => {
            state.pointer = normalized(position);

            if let Some(drag) = &state.dragging {
                for member in &drag.members {
                    if let Some(atom) = state
                        .atoms
                        .iter_mut()
                        .find(|atom| atom.id == member.atom_id)
                    {
                        let position = normalized(Point::new(
                            state.pointer.x + member.offset.x,
                            state.pointer.y + member.offset.y,
                        ));
                        atom.position = position;
                        atom.target = position;
                    }
                }
                state.feedback = Feedback::Repositioned;
                state.trigger = TriggerState::Idle;
            }
        }
        Message::LibraryElementDropped(atomic_number) => {
            if elements::by_atomic_number(atomic_number).is_some() && state.atoms.len() < MAX_ATOMS
            {
                let id = state.next_id;
                state.next_id = state.next_id.saturating_add(1);
                state.atoms.push(PlacedAtom {
                    id,
                    atomic_number,
                    position: state.pointer,
                    target: state.pointer,
                });
                state.selected = Some(id);
                state.feedback = Feedback::Added;
                prepare_group_snap(state);
                composition_changed(state);
            }
        }
        Message::AtomHovered(atom_id) => state.hovered = atom_id,
        Message::AtomDragStarted(atom_id) => begin_drag(state, atom_id),
        Message::AtomDragEnded => {
            state.dragging = None;
            prepare_group_snap(state);
            composition_changed(state);
        }
        Message::RemoveSelected => remove_selected(state),
        Message::ClearAll => {
            state.atoms.clear();
            state.selected = None;
            state.dragging = None;
            state.feedback = Feedback::Cleared;
            composition_changed(state);
        }
        Message::MotionToggled => {
            state.reduced_motion = !state.reduced_motion;
            if state.reduced_motion {
                state.orbital_phase = 0.0;
                settle_atoms_immediately(state);
                state.trigger_reveal = trigger_target(state);
            }
        }
        Message::StartReaction
        | Message::PlaybackToggled
        | Message::SequenceRestarted
        | Message::SequenceSkipped
        | Message::WorkspaceReturned => update_sequence_control(state, message),
        Message::AnimationTick => {
            settle_atoms(state);
            if let Some(sequence) = &mut state.sequence
                && sequence.playback == PlaybackState::Playing
            {
                sequence.progress = (sequence.progress + 0.006_25).min(1.0);
                if sequence.progress >= 1.0 {
                    sequence.playback = PlaybackState::Complete;
                }
            } else if !state.reduced_motion && !state.atoms.is_empty() {
                state.orbital_phase = (state.orbital_phase + 0.004) % 1.0;
            }
            state.trigger_reveal = approach(state.trigger_reveal, trigger_target(state), 0.2);
        }
    }
}

fn update_sequence_control(state: &mut State, message: Message) {
    match message {
        Message::StartReaction => start_sequence(state),
        Message::PlaybackToggled => {
            if let Some(sequence) = &mut state.sequence {
                sequence.playback = match sequence.playback {
                    PlaybackState::Playing => PlaybackState::Paused,
                    PlaybackState::Paused => PlaybackState::Playing,
                    PlaybackState::Complete => PlaybackState::Complete,
                };
            }
        }
        Message::SequenceRestarted => restart_sequence(state),
        Message::SequenceSkipped => {
            if let Some(sequence) = &mut state.sequence {
                sequence.progress = 1.0;
                sequence.playback = PlaybackState::Complete;
            }
        }
        Message::WorkspaceReturned => {
            state.sequence = None;
            state.trigger = TriggerState::Idle;
        }
        _ => {}
    }
}

fn start_sequence(state: &mut State) {
    if state.sequence.is_some() {
        return;
    }
    let Some(candidate) = reaction_candidate(&state.atoms) else {
        return;
    };
    state.trigger = TriggerState::Queued(candidate.id);
    state.sequence = Some(SequenceState {
        candidate,
        progress: if state.reduced_motion { 1.0 } else { 0.0 },
        playback: if state.reduced_motion {
            PlaybackState::Complete
        } else {
            PlaybackState::Playing
        },
    });
}

fn restart_sequence(state: &mut State) {
    if let Some(sequence) = &mut state.sequence {
        sequence.progress = if state.reduced_motion { 1.0 } else { 0.0 };
        sequence.playback = if state.reduced_motion {
            PlaybackState::Complete
        } else {
            PlaybackState::Playing
        };
    }
}

fn begin_drag(state: &mut State, atom_id: u32) {
    state.selected = Some(atom_id);
    let groups = recognized_groups(&state.atoms);
    let member_ids = groups
        .iter()
        .find(|group| group.atom_ids.contains(&atom_id))
        .map_or_else(
            || vec![atom_id],
            |group| group.atom_ids.iter().copied().collect(),
        );
    let center = member_center(&state.atoms, &member_ids);
    let members = member_ids
        .into_iter()
        .filter_map(|member_id| {
            state
                .atoms
                .iter()
                .find(|atom| atom.id == member_id)
                .map(|atom| DragMember {
                    atom_id: member_id,
                    offset: Point::new(atom.position.x - center.x, atom.position.y - center.y),
                })
        })
        .collect();
    state.dragging = Some(DragState {
        anchor_id: atom_id,
        members,
    });
    state.trigger = TriggerState::Idle;
}

fn remove_selected(state: &mut State) {
    let Some(selected) = state.selected.take() else {
        return;
    };
    let groups = recognized_groups(&state.atoms);
    let removed_ids = groups
        .iter()
        .find(|group| group.atom_ids.contains(&selected))
        .map_or_else(
            || BTreeSet::from([selected]),
            |group| group.atom_ids.clone(),
        );
    state.atoms.retain(|atom| !removed_ids.contains(&atom.id));
    state.dragging = None;
    state.feedback = Feedback::Removed;
    composition_changed(state);
}

fn composition_changed(state: &mut State) {
    state.trigger = TriggerState::Idle;
    state.sequence = None;
    if state.reduced_motion {
        state.trigger_reveal = trigger_target(state);
    }
}

pub fn subscription(state: &State) -> Subscription<Message> {
    let release = if state.dragging.is_some() {
        event::listen_with(|event, _status, _window| match event {
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
            | iced::Event::Touch(
                iced::touch::Event::FingerLifted { .. } | iced::touch::Event::FingerLost { .. },
            ) => Some(Message::AtomDragEnded),
            _ => None,
        })
    } else {
        Subscription::none()
    };
    let animation = if sequence_is_playing(state)
        || is_settling(state)
        || should_animate_orbits(state)
        || needs_trigger_transition(state)
    {
        iced::time::every(Duration::from_millis(50)).map(|_| Message::AnimationTick)
    } else {
        Subscription::none()
    };

    Subscription::batch([release, animation])
}

fn needs_trigger_transition(state: &State) -> bool {
    !state.reduced_motion && (state.trigger_reveal - trigger_target(state)).abs() > 0.01
}

fn should_animate_orbits(state: &State) -> bool {
    state.sequence.is_none() && !state.reduced_motion && !state.atoms.is_empty()
}

fn sequence_is_playing(state: &State) -> bool {
    state
        .sequence
        .is_some_and(|sequence| sequence.playback == PlaybackState::Playing)
}

pub fn sequence_active(state: &State) -> bool {
    state.sequence.is_some()
}

#[cfg(test)]
pub fn placed_atom_count(state: &State) -> usize {
    state.atoms.len()
}

pub fn view(state: &State, library_drag: Option<u8>, compact: bool) -> Element<'_, Message> {
    if let Some(sequence) = state.sequence {
        return sequence_view(sequence, compact);
    }

    let atom_count = state.atoms.len();
    let groups = recognized_groups(&state.atoms);

    let heading = row![
        column![
            text("STAGE 4  /  REACTION TRIGGER")
                .size(type_scale::MICRO)
                .color(color::ACCENT),
            text("Build, inspect, and start")
                .size(type_scale::TITLE)
                .color(color::TEXT),
        ]
        .spacing(spacing::XXS),
        space().width(Fill),
        text(if compact {
            "ATOMIC MODELS"
        } else {
            "SLOW ORBIT  ·  GROUPED ATOMIC MODELS  ·  VALIDATION REQUIRED"
        })
        .size(type_scale::MICRO)
        .color(color::MUTED),
    ]
    .align_y(iced::Center);

    let remove = button(text("Remove selected"))
        .on_press_maybe(state.selected.map(|_| Message::RemoveSelected))
        .padding([spacing::XS, spacing::SM])
        .style(theme::secondary_button);
    let clear = button(text("Clear workspace"))
        .on_press_maybe((!state.atoms.is_empty()).then_some(Message::ClearAll))
        .padding([spacing::XS, spacing::SM])
        .style(theme::secondary_button);
    let motion = button(text(if state.reduced_motion {
        "Reduced motion"
    } else {
        "Motion on"
    }))
    .on_press(Message::MotionToggled)
    .padding([spacing::XS, spacing::SM])
    .style(theme::secondary_button);

    let toolbar = column![row![remove, clear, motion].spacing(spacing::XS)];

    // The periodic library shrinks to its measured grid height. This
    // responsive surface receives every remaining pixel, keeping the table at
    // the bottom while turning spare vertical room into usable workspace.
    let reaction_box = responsive(move |size| workspace_canvas(state, library_drag, size));

    let summary = composition_summary(state, &groups, atom_count);

    container(
        column![heading, toolbar, reaction_box, summary]
            .spacing(spacing::XS)
            .width(Fill)
            .height(Fill),
    )
    .style(theme::frame)
    .padding(spacing::SM)
    .width(Fill)
    .height(Fill)
    .into()
}

fn sequence_view(sequence: SequenceState, compact: bool) -> Element<'static, Message> {
    let candidate = sequence.candidate;
    let stage_index = reaction_sequence::stage_index(sequence.progress, candidate.stages.len());
    let stage = candidate
        .stages
        .get(stage_index)
        .copied()
        .unwrap_or(candidate.stages[0]);
    let controls = sequence_controls(sequence.playback);
    let stages = sequence_stages(candidate, stage_index);

    let diagram = container(
        canvas(ReactionSequenceDiagram::new(candidate, sequence.progress))
            .width(Fill)
            .height(Fill),
    )
    .style(theme::inset)
    .width(Fill)
    .height(Fill);
    let explanation = container(
        row![
            column![
                text(stage.title)
                    .size(type_scale::BODY_LARGE)
                    .color(color::TEXT),
                text(stage.explanation)
                    .size(type_scale::CAPTION)
                    .color(color::TEXT_SOFT),
            ]
            .spacing(spacing::XXS),
            space().width(Fill),
            text(format!("{:.0}%", sequence.progress * 100.0))
                .size(type_scale::BODY_LARGE)
                .color(color::ACCENT),
        ]
        .align_y(iced::Center),
    )
    .style(theme::panel)
    .padding([spacing::XS, spacing::SM])
    .width(Fill);
    let heading = sequence_heading(candidate, compact);

    container(
        column![heading, controls, stages, diagram, explanation]
            .spacing(spacing::XS)
            .height(Fill),
    )
    .style(theme::frame)
    .padding(spacing::SM)
    .width(Fill)
    .height(Fill)
    .into()
}

fn sequence_controls(playback: PlaybackState) -> Element<'static, Message> {
    let play_pause = button(text(match playback {
        PlaybackState::Playing => "Pause",
        PlaybackState::Paused => "Play",
        PlaybackState::Complete => "Complete",
    }))
    .on_press_maybe((playback != PlaybackState::Complete).then_some(Message::PlaybackToggled))
    .padding([spacing::XS, spacing::MD])
    .style(theme::primary_button);
    let restart = button(text("Restart"))
        .on_press(Message::SequenceRestarted)
        .padding([spacing::XS, spacing::SM])
        .style(theme::secondary_button);
    let skip = button(text("Skip to products"))
        .on_press_maybe((playback != PlaybackState::Complete).then_some(Message::SequenceSkipped))
        .padding([spacing::XS, spacing::SM])
        .style(theme::secondary_button);
    let back = button(text("Return to workspace"))
        .on_press(Message::WorkspaceReturned)
        .padding([spacing::XS, spacing::SM])
        .style(theme::secondary_button);

    row![play_pause, restart, skip, back]
        .spacing(spacing::XS)
        .into()
}

fn sequence_stages(candidate: ReactionCandidate, active_index: usize) -> Element<'static, Message> {
    candidate
        .stages
        .iter()
        .enumerate()
        .fold(
            row![].spacing(spacing::XS).width(Fill),
            |row, (index, item)| {
                row.push(
                    container(
                        column![
                            text(format!("0{}", index + 1))
                                .size(type_scale::MICRO)
                                .color(if index == active_index {
                                    color::ACCENT
                                } else {
                                    color::FAINT
                                }),
                            text(item.title).size(type_scale::CAPTION).color(
                                if index <= active_index {
                                    color::TEXT
                                } else {
                                    color::MUTED
                                }
                            ),
                        ]
                        .spacing(spacing::XXS),
                    )
                    .style(if index == active_index {
                        theme::accent_tint
                    } else {
                        theme::raised
                    })
                    .padding([spacing::XS, spacing::SM])
                    .width(Fill),
                )
            },
        )
        .into()
}

fn sequence_heading(candidate: ReactionCandidate, compact: bool) -> Element<'static, Message> {
    row![
        column![
            text("STAGE 5  /  2D REACTION SEQUENCE")
                .size(type_scale::MICRO)
                .color(color::ACCENT),
            text(candidate.name)
                .size(if compact {
                    type_scale::TITLE
                } else {
                    type_scale::DISPLAY
                })
                .color(color::TEXT),
            text(candidate.equation_preview)
                .size(type_scale::BODY)
                .color(color::TEXT_SOFT),
        ]
        .spacing(spacing::XXS),
        space().width(Fill),
        column![
            text("ILLUSTRATIVE REACTION PREVIEW")
                .size(type_scale::MICRO)
                .color(color::WARNING),
            text("Validation is required before simulation")
                .size(type_scale::CAPTION)
                .color(color::MUTED),
        ]
        .spacing(spacing::XXS),
    ]
    .align_y(iced::Center)
    .into()
}

fn workspace_canvas(
    state: &State,
    library_drag: Option<u8>,
    size: Size,
) -> Element<'static, Message> {
    let width = size.width.max(1.0);
    let height = size.height.max(1.0);
    let drop_active = library_drag.is_some();
    let groups = recognized_groups(&state.atoms);

    let base = container(
        column![
            text(if drop_active {
                "RELEASE TO PLACE ATOM"
            } else if state.atoms.is_empty() {
                "DRAG ELEMENTS HERE"
            } else {
                "REACTION BOX"
            })
            .size(type_scale::MICRO)
            .color(if drop_active {
                color::ACCENT
            } else {
                color::FAINT
            }),
            space().height(Fill),
            text(feedback_text(state.feedback))
                .size(type_scale::CAPTION)
                .color(color::MUTED),
        ]
        .padding(spacing::SM),
    )
    .style(move |_| workspace_style(drop_active))
    .width(Fill)
    .height(Length::Fixed(height));

    let grouped_ids = groups
        .iter()
        .flat_map(|group| group.atom_ids.iter().copied())
        .collect::<BTreeSet<_>>();
    let mut layers = state
        .atoms
        .iter()
        .filter(|atom| !grouped_ids.contains(&atom.id))
        .fold(stack![base], |layers, atom| {
            layers.push(positioned_atom(state, *atom, width, height))
        });
    for group in &groups {
        layers = layers.push(positioned_compound(state, group, width, height));
    }

    let area = mouse_area(layers.width(Fill).height(Length::Fixed(height)).clip(true))
        .on_move(move |point| Message::PointerMoved(Point::new(point.x / width, point.y / height)));
    let area = if let Some(atomic_number) = library_drag {
        area.on_release(Message::LibraryElementDropped(atomic_number))
    } else {
        area
    };

    area.interaction(if drop_active || state.dragging.is_some() {
        mouse::Interaction::Grabbing
    } else {
        mouse::Interaction::Crosshair
    })
    .into()
}

fn positioned_atom(
    state: &State,
    atom: PlacedAtom,
    width: f32,
    height: f32,
) -> Element<'static, Message> {
    let Some(element) = elements::by_atomic_number(atom.atomic_number) else {
        return space().into();
    };
    let left = atom.position.x * (width - ATOM_WIDTH).max(0.0);
    let top = atom.position.y * (height - ATOM_HEIGHT).max(0.0);
    let selected = state.selected == Some(atom.id);
    let hovered = state.hovered == Some(atom.id);
    let dragging = state
        .dragging
        .as_ref()
        .is_some_and(|drag| drag.members.iter().any(|member| member.atom_id == atom.id));
    let emphasis = if dragging {
        AtomEmphasis::Dragging
    } else if selected {
        AtomEmphasis::Selected
    } else if hovered {
        AtomEmphasis::Hovered
    } else {
        AtomEmphasis::Idle
    };

    let tile = atom_tile(*element, state.orbital_phase, emphasis);
    let interactive = mouse_area(tile)
        .on_press(Message::AtomDragStarted(atom.id))
        .on_release(Message::AtomDragEnded)
        .on_enter(Message::AtomHovered(Some(atom.id)))
        .on_exit(Message::AtomHovered(None))
        .interaction(if dragging {
            mouse::Interaction::Grabbing
        } else {
            mouse::Interaction::Grab
        });

    container(interactive)
        .padding(Padding {
            top,
            right: 0.0,
            bottom: 0.0,
            left,
        })
        .width(Fill)
        .height(Length::Fixed(height))
        .into()
}

fn positioned_compound(
    state: &State,
    group: &RecognizedGroup,
    width: f32,
    height: f32,
) -> Element<'static, Message> {
    let member_ids = group.atom_ids.iter().copied().collect::<Vec<_>>();
    let center = member_center(&state.atoms, &member_ids);
    let left = center.x * (width - COMPOUND_WIDTH).max(0.0);
    let top = center.y * (height - COMPOUND_HEIGHT).max(0.0);
    let anchor_id = member_ids.first().copied().unwrap_or_default();
    let selected = member_ids.iter().any(|id| state.selected == Some(*id));
    let hovered = member_ids.iter().any(|id| state.hovered == Some(*id));
    let dragging = state.dragging.as_ref().is_some_and(|drag| {
        drag.anchor_id == anchor_id
            || drag
                .members
                .iter()
                .any(|member| group.atom_ids.contains(&member.atom_id))
    });
    let emphasis = if dragging {
        AtomEmphasis::Dragging
    } else if selected {
        AtomEmphasis::Selected
    } else if hovered {
        AtomEmphasis::Hovered
    } else {
        AtomEmphasis::Idle
    };
    let elements = member_ids.iter().filter_map(|id| {
        state
            .atoms
            .iter()
            .find(|atom| atom.id == *id)
            .and_then(|atom| elements::by_atomic_number(atom.atomic_number))
            .copied()
    });
    let diagram = canvas(CompoundAtomicDiagram::new(
        group.preview,
        elements,
        state.orbital_phase,
    ))
    .width(Fill)
    .height(Length::Fixed(66.0));
    let tile = container(
        column![
            row![
                text("GROUPED ATOMS")
                    .size(type_scale::MICRO)
                    .color(color::SUCCESS),
                space().width(Fill),
                text(format!("{} ATOMS", member_ids.len()))
                    .size(type_scale::MICRO)
                    .color(color::MUTED),
            ],
            diagram,
            row![
                text(group.preview.formula)
                    .size(type_scale::BODY_LARGE)
                    .color(color::TEXT),
                space().width(Fill),
                text(group.preview.name)
                    .size(type_scale::MICRO)
                    .color(color::TEXT_SOFT),
            ]
            .align_y(iced::Center),
        ]
        .spacing(spacing::XXS),
    )
    .padding([spacing::XXS, spacing::XS])
    .width(Length::Fixed(COMPOUND_WIDTH))
    .height(Length::Fixed(COMPOUND_HEIGHT))
    .style(move |_| atom_style(true, emphasis));
    let interactive = mouse_area(tile)
        .on_press(Message::AtomDragStarted(anchor_id))
        .on_release(Message::AtomDragEnded)
        .on_enter(Message::AtomHovered(Some(anchor_id)))
        .on_exit(Message::AtomHovered(None))
        .interaction(if dragging {
            mouse::Interaction::Grabbing
        } else {
            mouse::Interaction::Grab
        });

    container(interactive)
        .padding(Padding {
            top,
            right: 0.0,
            bottom: 0.0,
            left,
        })
        .width(Fill)
        .height(Length::Fixed(height))
        .into()
}

fn atom_tile(
    element: ElementSpec,
    orbital_phase: f32,
    emphasis: AtomEmphasis,
) -> Element<'static, Message> {
    let diagram = canvas(AtomDiagram::new(element, orbital_phase))
        .width(Fill)
        .height(Length::Fixed(68.0));
    let content = column![
        row![
            text(element.atomic_number.to_string())
                .size(type_scale::MICRO)
                .color(color::MUTED),
            space().width(Fill),
            text("ATOM").size(type_scale::MICRO).color(color::ACCENT),
        ],
        diagram,
        row![
            text(element.name)
                .size(type_scale::MICRO)
                .color(color::TEXT_SOFT),
            space().width(Fill),
            text(format!("{}e⁻", element.valence_electrons))
                .size(type_scale::MICRO)
                .color(color::ACCENT),
        ],
    ]
    .spacing(spacing::XXS);

    container(content)
        .padding([spacing::XXS, spacing::XS])
        .width(Length::Fixed(ATOM_WIDTH))
        .height(Length::Fixed(ATOM_HEIGHT))
        .style(move |_| atom_style(false, emphasis))
        .into()
}

fn composition_summary(
    state: &State,
    groups: &[RecognizedGroup],
    atom_count: usize,
) -> Element<'static, Message> {
    let object_count = reactant_object_count(&state.atoms);
    let candidate = reaction_candidate(&state.atoms);
    let queued_id = match state.trigger {
        TriggerState::Idle => None,
        TriggerState::Queued(id) => Some(id),
    };
    let queued = candidate.is_some_and(|candidate| queued_id == Some(candidate.id));
    let trigger_reveal = state.trigger_reveal;
    let (title, detail) = if let Some(candidate) = candidate {
        if queued {
            (
                "Reaction queued",
                "Validation must complete before animation begins".to_owned(),
            )
        } else {
            (
                "Ready to start",
                format!("{}  ·  {}", candidate.name, candidate.equation_preview),
            )
        }
    } else if object_count >= 2 {
        (
            "Unsupported reactant combination",
            "Rearrange the current atoms into a supported reaction candidate".to_owned(),
        )
    } else if groups.is_empty() {
        (
            "Add at least two reactants",
            "Combine atoms into compositions, then add another reactant".to_owned(),
        )
    } else {
        (
            "Add another reactant",
            groups
                .iter()
                .map(|group| group.preview.formula)
                .collect::<Vec<_>>()
                .join("  ·  "),
        )
    };
    let action: Element<'static, Message> = button(text(if queued {
        "Reaction queued"
    } else {
        "Start Reaction"
    }))
    .on_press_maybe((candidate.is_some() && !queued).then_some(Message::StartReaction))
    .padding([spacing::XS, spacing::MD])
    .style(theme::primary_button)
    .into();

    container(
        row![
            column![
                text(if object_count >= 2 {
                    "REACTION READINESS"
                } else {
                    "COMPOSITION STATUS"
                })
                .size(type_scale::MICRO)
                .color(color::ACCENT),
                text(title).size(type_scale::BODY_LARGE).color(color::TEXT),
                text(detail)
                    .size(type_scale::CAPTION)
                    .color(color::TEXT_SOFT),
            ]
            .spacing(spacing::XXS),
            space().width(Fill),
            if object_count >= 2 {
                action
            } else {
                text(format!("{} GROUPS  ·  {atom_count} ATOMS", groups.len()))
                    .size(type_scale::MICRO)
                    .color(color::MUTED)
                    .into()
            },
        ]
        .align_y(iced::Center),
    )
    .style(move |_| reaction_status_style(candidate.is_some(), queued, trigger_reveal))
    .padding([spacing::XS, spacing::SM])
    .width(Fill)
    .into()
}

#[derive(Debug, Clone)]
struct RecognizedGroup {
    atom_ids: BTreeSet<u32>,
    preview: CompositionPreview,
}

fn recognized_groups(atoms: &[PlacedAtom]) -> Vec<RecognizedGroup> {
    let mut unvisited = atoms.iter().map(|atom| atom.id).collect::<BTreeSet<_>>();
    let mut groups = Vec::new();

    while let Some(seed) = unvisited.pop_first() {
        let mut cluster = BTreeSet::from([seed]);
        let mut frontier = vec![seed];

        while let Some(current_id) = frontier.pop() {
            let Some(current) = atoms.iter().find(|atom| atom.id == current_id) else {
                continue;
            };
            let neighbours = unvisited
                .iter()
                .copied()
                .filter(|candidate_id| {
                    atoms
                        .iter()
                        .find(|atom| atom.id == *candidate_id)
                        .is_some_and(|candidate| {
                            distance_squared(current.position, candidate.position)
                                <= GROUP_DISTANCE * GROUP_DISTANCE
                        })
                })
                .collect::<Vec<_>>();

            for neighbour in neighbours {
                unvisited.remove(&neighbour);
                cluster.insert(neighbour);
                frontier.push(neighbour);
            }
        }

        let atomic_numbers = cluster.iter().filter_map(|id| {
            atoms
                .iter()
                .find(|atom| atom.id == *id)
                .map(|atom| atom.atomic_number)
        });
        if let Some(preview) = composition_catalogue::recognize(atomic_numbers) {
            groups.push(RecognizedGroup {
                atom_ids: cluster,
                preview,
            });
        }
    }

    groups
}

fn reaction_participants(atoms: &[PlacedAtom]) -> Vec<Participant> {
    let groups = recognized_groups(atoms);
    let grouped_ids = groups
        .iter()
        .flat_map(|group| group.atom_ids.iter().copied())
        .collect::<BTreeSet<_>>();
    let compositions = groups
        .iter()
        .map(|group| Participant::Composition(group.preview.formula));
    let loose_atoms = atoms
        .iter()
        .filter(|atom| !grouped_ids.contains(&atom.id))
        .map(|atom| Participant::Atom(atom.atomic_number));

    compositions.chain(loose_atoms).collect()
}

fn reaction_candidate(atoms: &[PlacedAtom]) -> Option<ReactionCandidate> {
    reaction_candidate_catalogue::recognize(reaction_participants(atoms))
}

fn reactant_object_count(atoms: &[PlacedAtom]) -> usize {
    reaction_participants(atoms).len()
}

fn trigger_target(state: &State) -> f32 {
    if reactant_object_count(&state.atoms) >= 2 {
        1.0
    } else {
        0.0
    }
}

fn approach(current: f32, target: f32, amount: f32) -> f32 {
    if (current - target).abs() <= amount {
        target
    } else if current < target {
        current + amount
    } else {
        current - amount
    }
}

fn prepare_group_snap(state: &mut State) {
    let groups = recognized_groups(&state.atoms);
    let mut targets = Vec::new();

    for group in groups {
        let members = group.atom_ids.iter().copied().collect::<Vec<_>>();
        let center = member_center(&state.atoms, &members);
        targets.extend(members.into_iter().map(|id| (id, center)));
    }

    for (id, target) in targets {
        if let Some(atom) = state.atoms.iter_mut().find(|atom| atom.id == id) {
            atom.target = target;
        }
    }

    if state.reduced_motion {
        settle_atoms_immediately(state);
    }
}

fn member_center(atoms: &[PlacedAtom], member_ids: &[u32]) -> Point {
    let (sum_x, sum_y) = member_ids.iter().fold((0.0, 0.0), |sum, id| {
        atoms
            .iter()
            .find(|atom| atom.id == *id)
            .map_or(sum, |atom| {
                (sum.0 + atom.position.x, sum.1 + atom.position.y)
            })
    });
    let divisor = match member_ids.len() {
        2 => 2.0,
        3 => 3.0,
        _ => 1.0,
    };
    normalized(Point::new(sum_x / divisor, sum_y / divisor))
}

fn is_settling(state: &State) -> bool {
    state
        .atoms
        .iter()
        .any(|atom| distance_squared(atom.position, atom.target) > SETTLE_EPSILON)
}

fn settle_atoms(state: &mut State) {
    for atom in &mut state.atoms {
        if distance_squared(atom.position, atom.target) <= SETTLE_EPSILON {
            atom.position = atom.target;
            continue;
        }

        atom.position = Point::new(
            atom.position.x + (atom.target.x - atom.position.x) * 0.24,
            atom.position.y + (atom.target.y - atom.position.y) * 0.24,
        );
    }
}

fn settle_atoms_immediately(state: &mut State) {
    for atom in &mut state.atoms {
        atom.position = atom.target;
    }
}

fn normalized(point: Point) -> Point {
    Point::new(point.x.clamp(0.0, 1.0), point.y.clamp(0.0, 1.0))
}

fn distance_squared(a: Point, b: Point) -> f32 {
    let x = a.x - b.x;
    let y = a.y - b.y;
    x * x + y * y
}

const fn feedback_text(feedback: Feedback) -> &'static str {
    match feedback {
        Feedback::Ready => "Drop atoms anywhere in the box",
        Feedback::Added => "Atom placed · duplicate elements are supported",
        Feedback::Repositioned => "Composition moved · overlap compatible atoms to combine",
        Feedback::Removed => "Selected atom or compound removed",
        Feedback::Cleared => "Workspace cleared",
    }
}

fn workspace_style(drop_active: bool) -> container::Style {
    container::Style::default()
        .background(if drop_active {
            color::ACCENT_FAINT
        } else {
            color::CANVAS
        })
        .border(Border {
            color: if drop_active {
                color::ACCENT
            } else {
                color::LINE_STRONG
            },
            width: if drop_active { 2.0 } else { 1.0 },
            radius: border::Radius::new(radius::PANEL),
        })
}

fn reaction_status_style(ready: bool, queued: bool, reveal: f32) -> container::Style {
    let accent = if queued {
        color::WARNING
    } else if ready {
        color::SUCCESS
    } else {
        color::LINE_STRONG
    };
    let alpha = reveal.clamp(0.18, 1.0);

    container::Style::default()
        .background(Color::from_rgba(accent.r, accent.g, accent.b, 0.08 * alpha))
        .border(Border {
            color: Color::from_rgba(accent.r, accent.g, accent.b, 0.65 * alpha),
            width: 1.0,
            radius: border::Radius::new(radius::PANEL),
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AtomEmphasis {
    Idle,
    Hovered,
    Selected,
    Dragging,
}

fn atom_style(recognized: bool, emphasis: AtomEmphasis) -> container::Style {
    let highlighted = emphasis != AtomEmphasis::Idle;
    let selected = emphasis == AtomEmphasis::Selected;
    let dragging = emphasis == AtomEmphasis::Dragging;
    let accent = if recognized {
        color::SUCCESS
    } else {
        color::ACCENT
    };
    container::Style {
        background: Some(Background::Color(if highlighted {
            color::SURFACE_ACTIVE
        } else {
            color::SURFACE
        })),
        text_color: Some(color::TEXT),
        border: Border {
            color: if highlighted || recognized {
                accent
            } else {
                color::LINE_STRONG
            },
            width: if selected || dragging { 2.0 } else { 1.0 },
            radius: border::Radius::new(radius::CONTROL),
        },
        shadow: if dragging {
            Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.48),
                offset: Vector::new(0.0, 10.0),
                blur_radius: 22.0,
            }
        } else {
            Shadow::default()
        },
        ..container::Style::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_atoms_can_be_added_and_removed() {
        let mut state = State::default();
        update(&mut state, Message::PointerMoved(Point::new(0.2, 0.2)));
        update(&mut state, Message::LibraryElementDropped(8));
        update(&mut state, Message::PointerMoved(Point::new(0.8, 0.8)));
        update(&mut state, Message::LibraryElementDropped(8));
        assert_eq!(state.atoms.len(), 2);

        update(&mut state, Message::RemoveSelected);
        assert_eq!(state.atoms.len(), 1);
    }

    #[test]
    fn nearby_supported_atoms_create_only_a_preview() {
        let atoms = vec![
            PlacedAtom {
                id: 1,
                atomic_number: 1,
                position: Point::new(0.5, 0.5),
                target: Point::new(0.5, 0.5),
            },
            PlacedAtom {
                id: 2,
                atomic_number: 1,
                position: Point::new(0.51, 0.5),
                target: Point::new(0.51, 0.5),
            },
            PlacedAtom {
                id: 3,
                atomic_number: 8,
                position: Point::new(0.49, 0.5),
                target: Point::new(0.49, 0.5),
            },
        ];
        let groups = recognized_groups(&atoms);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].preview.formula, "H₂O");
    }

    #[test]
    fn unsupported_clusters_remain_unrecognized() {
        let atoms = vec![
            PlacedAtom {
                id: 1,
                atomic_number: 6,
                position: Point::new(0.5, 0.5),
                target: Point::new(0.5, 0.5),
            },
            PlacedAtom {
                id: 2,
                atomic_number: 6,
                position: Point::new(0.51, 0.5),
                target: Point::new(0.51, 0.5),
            },
        ];
        assert!(recognized_groups(&atoms).is_empty());
    }

    #[test]
    fn recognized_group_settles_to_deterministic_targets() {
        let mut state = State {
            atoms: vec![
                PlacedAtom {
                    id: 1,
                    atomic_number: 8,
                    position: Point::new(0.5, 0.5),
                    target: Point::new(0.5, 0.5),
                },
                PlacedAtom {
                    id: 2,
                    atomic_number: 8,
                    position: Point::new(0.58, 0.5),
                    target: Point::new(0.58, 0.5),
                },
            ],
            ..State::default()
        };

        prepare_group_snap(&mut state);
        assert!(is_settling(&state));
        for _ in 0..64 {
            settle_atoms(&mut state);
        }
        assert!(!is_settling(&state));
        assert_eq!(state.atoms[0].position, state.atoms[0].target);
        assert_eq!(state.atoms[1].position, state.atoms[1].target);
    }

    #[test]
    fn dragging_one_workspace_atom_onto_another_recognizes_the_group() {
        let mut state = State::default();
        update(&mut state, Message::PointerMoved(Point::new(0.2, 0.3)));
        update(&mut state, Message::LibraryElementDropped(8));
        update(&mut state, Message::PointerMoved(Point::new(0.8, 0.7)));
        update(&mut state, Message::LibraryElementDropped(8));

        let dragged_id = state.atoms[1].id;
        update(&mut state, Message::AtomDragStarted(dragged_id));
        update(&mut state, Message::PointerMoved(Point::new(0.2, 0.3)));
        update(&mut state, Message::AtomDragEnded);

        let groups = recognized_groups(&state.atoms);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].preview.formula, "O₂");
    }

    #[test]
    fn recognized_compound_moves_as_one_object() {
        let mut state = State {
            atoms: vec![
                PlacedAtom {
                    id: 1,
                    atomic_number: 1,
                    position: Point::new(0.5, 0.5),
                    target: Point::new(0.5, 0.5),
                },
                PlacedAtom {
                    id: 2,
                    atomic_number: 1,
                    position: Point::new(0.5, 0.5),
                    target: Point::new(0.5, 0.5),
                },
                PlacedAtom {
                    id: 3,
                    atomic_number: 8,
                    position: Point::new(0.5, 0.5),
                    target: Point::new(0.5, 0.5),
                },
            ],
            ..State::default()
        };

        update(&mut state, Message::AtomDragStarted(1));
        update(&mut state, Message::PointerMoved(Point::new(0.75, 0.25)));
        update(&mut state, Message::AtomDragEnded);

        assert!(
            state
                .atoms
                .iter()
                .all(|atom| atom.position == Point::new(0.75, 0.25))
        );
        let groups = recognized_groups(&state.atoms);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].preview.formula, "H₂O");
    }

    #[test]
    fn removing_a_recognized_compound_removes_all_member_atoms() {
        let mut state = State {
            atoms: vec![
                PlacedAtom {
                    id: 1,
                    atomic_number: 8,
                    position: Point::new(0.4, 0.4),
                    target: Point::new(0.4, 0.4),
                },
                PlacedAtom {
                    id: 2,
                    atomic_number: 8,
                    position: Point::new(0.4, 0.4),
                    target: Point::new(0.4, 0.4),
                },
            ],
            selected: Some(1),
            ..State::default()
        };

        update(&mut state, Message::RemoveSelected);

        assert!(state.atoms.is_empty());
    }

    #[test]
    fn reduced_motion_stops_orbital_animation_without_removing_atoms() {
        let mut state = State::default();
        update(&mut state, Message::LibraryElementDropped(8));
        assert!(should_animate_orbits(&state));

        update(&mut state, Message::MotionToggled);

        assert!(!should_animate_orbits(&state));
        assert_eq!(state.atoms.len(), 1);
        assert!(state.orbital_phase.abs() < f32::EPSILON);
    }

    #[test]
    fn electron_orbit_advances_at_the_slower_stage_four_rate() {
        let mut state = State::default();
        update(&mut state, Message::LibraryElementDropped(8));

        update(&mut state, Message::AnimationTick);

        assert!((state.orbital_phase - 0.004).abs() < f32::EPSILON);
    }

    #[test]
    fn hydrogen_and_oxygen_enable_and_queue_the_reaction_trigger() {
        let mut state = State {
            atoms: vec![
                placed_atom(1, 1, 0.20, 0.30),
                placed_atom(2, 1, 0.21, 0.30),
                placed_atom(3, 8, 0.78, 0.70),
                placed_atom(4, 8, 0.79, 0.70),
            ],
            ..State::default()
        };

        let candidate = reaction_candidate(&state.atoms).expect("supported reaction candidate");
        assert_eq!(candidate.id, "hydrogen-oxygen");
        update(&mut state, Message::StartReaction);
        assert_eq!(state.trigger, TriggerState::Queued("hydrogen-oxygen"));
        assert!(sequence_active(&state));

        update(&mut state, Message::StartReaction);
        assert_eq!(state.trigger, TriggerState::Queued("hydrogen-oxygen"));

        update(&mut state, Message::LibraryElementDropped(1));
        assert_eq!(state.trigger, TriggerState::Idle);
        assert!(!sequence_active(&state));
    }

    #[test]
    fn unsupported_reactants_do_not_queue_a_reaction() {
        let mut state = State {
            atoms: vec![placed_atom(1, 1, 0.20, 0.30), placed_atom(2, 8, 0.80, 0.70)],
            ..State::default()
        };

        assert_eq!(reactant_object_count(&state.atoms), 2);
        assert!(reaction_candidate(&state.atoms).is_none());
        update(&mut state, Message::StartReaction);
        assert_eq!(state.trigger, TriggerState::Idle);
    }

    #[test]
    fn stage_five_playback_controls_are_deterministic() {
        let mut state = hydrogen_oxygen_state();
        update(&mut state, Message::StartReaction);
        update(&mut state, Message::AnimationTick);
        let advanced = state.sequence.expect("active sequence").progress;
        assert!(advanced > 0.0);

        update(&mut state, Message::PlaybackToggled);
        update(&mut state, Message::AnimationTick);
        assert!(
            (state.sequence.expect("paused sequence").progress - advanced).abs() < f32::EPSILON
        );

        update(&mut state, Message::SequenceRestarted);
        assert!(state.sequence.expect("restarted sequence").progress.abs() < f32::EPSILON);
        update(&mut state, Message::SequenceSkipped);
        let skipped = state.sequence.expect("skipped sequence");
        assert!((skipped.progress - 1.0).abs() < f32::EPSILON);
        assert_eq!(skipped.playback, PlaybackState::Complete);

        update(&mut state, Message::WorkspaceReturned);
        assert!(!sequence_active(&state));
    }

    #[test]
    fn reduced_motion_starts_on_the_static_product_frame() {
        let mut state = hydrogen_oxygen_state();
        update(&mut state, Message::MotionToggled);
        update(&mut state, Message::StartReaction);

        let sequence = state.sequence.expect("reduced-motion sequence");
        assert!((sequence.progress - 1.0).abs() < f32::EPSILON);
        assert_eq!(sequence.playback, PlaybackState::Complete);
    }

    fn hydrogen_oxygen_state() -> State {
        State {
            atoms: vec![
                placed_atom(1, 1, 0.20, 0.30),
                placed_atom(2, 1, 0.21, 0.30),
                placed_atom(3, 8, 0.78, 0.70),
                placed_atom(4, 8, 0.79, 0.70),
            ],
            ..State::default()
        }
    }

    fn placed_atom(id: u32, atomic_number: u8, x: f32, y: f32) -> PlacedAtom {
        PlacedAtom {
            id,
            atomic_number,
            position: Point::new(x, y),
            target: Point::new(x, y),
        }
    }
}
