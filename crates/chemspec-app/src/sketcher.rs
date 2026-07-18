//! Molecular structure sketcher: Tier-3 organics input.
//!
//! The learner draws heavy atoms and bonds on a small canvas; the domain
//! derives implicit hydrogens from organic-subset valences and validates the
//! graph. A valid sketch fills the active composer slot as a full atom
//! inventory plus its subset SMILES, which the dynamic pipeline resolves
//! back into the exact drawn structure.

use web_time::Instant;

use chem_domain::{
    ELEMENT_SYMBOLS, StructureId, smiles_from_structure, structure_from_heavy_graph, subset_valence,
};
use iced::mouse::{self, Cursor};
use iced::widget::canvas::{Path, Stroke};
use iced::widget::{button, canvas, column, container, row, space, text};
use iced::{Center, Color, Element, Fill, Length, Point, Rectangle, Renderer, Theme};

use crate::fonts;
use crate::theme::{self, LAB_DARK, color, space as spacing, type_scale};

/// Mirrors the composer's private `MAX_ATOMS_PER_REACTANT` cap so a sketch
/// never fills a slot the rest of the pipeline would reject.
const MAX_SKETCH_ATOMS: u64 = 24;

const ATOM_RADIUS: f32 = 15.0;
const ATOM_HIT_RADIUS: f32 = 18.0;
const BOND_HIT_DISTANCE: f32 = 8.0;
const DRAG_THRESHOLD: f32 = 6.0;
const DOUBLE_CLICK_WINDOW: std::time::Duration = std::time::Duration::from_millis(350);
const CANVAS_HEIGHT: f32 = 260.0;

/// The sketchable organic-subset elements. Hydrogens are always implicit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SketchElement {
    Carbon,
    Nitrogen,
    Oxygen,
    Sulfur,
    Chlorine,
    Bromine,
}

impl SketchElement {
    const ALL: [Self; 6] = [
        Self::Carbon,
        Self::Nitrogen,
        Self::Oxygen,
        Self::Sulfur,
        Self::Chlorine,
        Self::Bromine,
    ];

    const fn symbol(self) -> &'static str {
        match self {
            Self::Carbon => "C",
            Self::Nitrogen => "N",
            Self::Oxygen => "O",
            Self::Sulfur => "S",
            Self::Chlorine => "Cl",
            Self::Bromine => "Br",
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Carbon => "carbon",
            Self::Nitrogen => "nitrogen",
            Self::Oxygen => "oxygen",
            Self::Sulfur => "sulfur",
            Self::Chlorine => "chlorine",
            Self::Bromine => "bromine",
        }
    }

    const fn color(self) -> Color {
        match self {
            Self::Carbon => LAB_DARK.chemistry.carbon,
            Self::Nitrogen => LAB_DARK.chemistry.nitrogen,
            Self::Oxygen => LAB_DARK.chemistry.oxygen,
            Self::Sulfur => LAB_DARK.chemistry.sulfur,
            Self::Chlorine => LAB_DARK.chemistry.chlorine,
            Self::Bromine => LAB_DARK.chemistry.bromine,
        }
    }

    /// The domain's subset valence, so display and validation cannot drift.
    fn valence(self) -> u8 {
        subset_valence(self.symbol()).unwrap_or(0)
    }
}

#[derive(Debug, Clone, Copy)]
struct SketchAtom {
    position: Point,
    element: SketchElement,
}

#[derive(Debug, Clone, Copy)]
struct SketchBond {
    a: usize,
    b: usize,
    order: u8,
}

#[derive(Debug)]
pub struct State {
    atoms: Vec<SketchAtom>,
    bonds: Vec<SketchBond>,
    palette: SketchElement,
    selected: Option<usize>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            atoms: Vec::new(),
            bonds: Vec::new(),
            palette: SketchElement::Carbon,
            selected: None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Message {
    PaletteSelected(SketchElement),
    Canvas(CanvasEvent),
    Cleared,
    /// The app intercepts this to fill the active composer slot.
    UseAsReactant,
}

#[derive(Debug, Clone, Copy)]
pub enum CanvasEvent {
    Placed(Point),
    AtomClicked(usize),
    AtomRemoved(usize),
    BondClicked(usize),
    Bonded { from: usize, to: usize },
}

pub fn update(state: &mut State, message: Message) {
    match message {
        Message::PaletteSelected(element) => state.palette = element,
        Message::Cleared => {
            state.atoms.clear();
            state.bonds.clear();
            state.selected = None;
        }
        Message::UseAsReactant => {}
        Message::Canvas(event) => canvas_event(state, event),
    }
}

fn canvas_event(state: &mut State, event: CanvasEvent) {
    match event {
        CanvasEvent::Placed(position) => {
            state.atoms.push(SketchAtom {
                position,
                element: state.palette,
            });
        }
        CanvasEvent::AtomClicked(index) if index < state.atoms.len() => match state.selected {
            Some(selected) if selected == index => state.selected = None,
            Some(selected) => {
                add_bond(state, selected, index);
                state.selected = Some(index);
            }
            None => state.selected = Some(index),
        },
        CanvasEvent::AtomRemoved(index) if index < state.atoms.len() => {
            state.atoms.remove(index);
            state
                .bonds
                .retain(|bond| bond.a != index && bond.b != index);
            for bond in &mut state.bonds {
                if bond.a > index {
                    bond.a -= 1;
                }
                if bond.b > index {
                    bond.b -= 1;
                }
            }
            state.selected = None;
        }
        CanvasEvent::BondClicked(index) if index < state.bonds.len() => {
            if state.bonds[index].order >= 3 {
                state.bonds.remove(index);
            } else {
                state.bonds[index].order += 1;
            }
        }
        CanvasEvent::Bonded { from, to } => add_bond(state, from, to),
        CanvasEvent::AtomClicked(_) | CanvasEvent::AtomRemoved(_) | CanvasEvent::BondClicked(_) => {
        }
    }
}

fn add_bond(state: &mut State, a: usize, b: usize) {
    let exists = state
        .bonds
        .iter()
        .any(|bond| (bond.a == a && bond.b == b) || (bond.a == b && bond.b == a));
    if a == b || a >= state.atoms.len() || b >= state.atoms.len() || exists {
        return;
    }
    state.bonds.push(SketchBond { a, b, order: 1 });
}

/// A validated sketch ready for the composer: the full atom inventory
/// (implicit hydrogens included) and the drawn structure's subset SMILES.
#[derive(Debug)]
pub struct Sketch {
    pub atoms: Vec<u8>,
    pub smiles: String,
}

#[must_use]
pub fn submission(state: &State) -> Option<Sketch> {
    evaluate(state).ok().map(|(sketch, _)| sketch)
}

fn order_sum(bonds: &[SketchBond], index: usize) -> u8 {
    bonds
        .iter()
        .filter(|bond| bond.a == index || bond.b == index)
        .map(|bond| bond.order)
        .sum()
}

fn count_word(count: u8) -> String {
    match count {
        1 => "one".to_owned(),
        2 => "two".to_owned(),
        3 => "three".to_owned(),
        4 => "four".to_owned(),
        5 => "five".to_owned(),
        6 => "six".to_owned(),
        _ => count.to_string(),
    }
}

/// Validates the sketch: either the composer payload plus a display formula,
/// or a one-line explanation of what is wrong.
fn evaluate(state: &State) -> Result<(Sketch, String), String> {
    if state.atoms.is_empty() {
        return Err("click the canvas to place an atom".to_owned());
    }
    for (index, atom) in state.atoms.iter().enumerate() {
        let orders = order_sum(&state.bonds, index);
        let valence = atom.element.valence();
        if orders > valence {
            return Err(format!(
                "{} has {} bonds — it supports {}",
                atom.element.name(),
                count_word(orders),
                count_word(valence)
            ));
        }
    }
    if !connected(state) {
        return Err("connect every atom into one molecule".to_owned());
    }
    let symbols: Vec<&str> = state
        .atoms
        .iter()
        .map(|atom| atom.element.symbol())
        .collect();
    let bonds: Vec<(usize, usize, u8)> = state
        .bonds
        .iter()
        .map(|bond| (bond.a, bond.b, bond.order))
        .collect();
    let id = StructureId::new("sketch.draft")
        .map_err(|_| "the sketch could not be validated".to_owned())?;
    let structure = structure_from_heavy_graph(id, &symbols, &bonds)
        .ok_or_else(|| "this sketch isn't valid subset chemistry".to_owned())?;
    let total: u64 = structure.formula().elements().values().sum();
    if total > MAX_SKETCH_ATOMS {
        return Err(format!(
            "too many atoms for one reactant (max {MAX_SKETCH_ATOMS} including hydrogens)"
        ));
    }
    let mut atoms = Vec::new();
    let mut formula = String::new();
    for (element, count) in structure.formula().elements() {
        let atomic_number = atomic_number_of(element.as_str())
            .ok_or_else(|| "the sketch could not be validated".to_owned())?;
        for _ in 0..*count {
            atoms.push(atomic_number);
        }
        formula.push_str(element.as_str());
        if *count > 1 {
            formula.push_str(&count.to_string());
        }
    }
    let smiles = smiles_from_structure(&structure)
        .ok_or_else(|| "the sketch could not be written as SMILES".to_owned())?;
    Ok((Sketch { atoms, smiles }, formula))
}

fn connected(state: &State) -> bool {
    let mut reached = vec![false; state.atoms.len()];
    let mut queue = vec![0_usize];
    reached[0] = true;
    while let Some(next) = queue.pop() {
        for bond in &state.bonds {
            let neighbour = if bond.a == next {
                bond.b
            } else if bond.b == next {
                bond.a
            } else {
                continue;
            };
            if !reached[neighbour] {
                reached[neighbour] = true;
                queue.push(neighbour);
            }
        }
    }
    reached.into_iter().all(|reached| reached)
}

fn atomic_number_of(symbol: &str) -> Option<u8> {
    ELEMENT_SYMBOLS
        .iter()
        .position(|candidate| *candidate == symbol)
        .and_then(|index| u8::try_from(index + 1).ok())
}

/// The sketch panel: palette, canvas, live validity, and actions.
pub fn view(state: &State, slot_available: bool) -> Element<'_, Message> {
    let mut palette = row![].spacing(spacing::XXS);
    for element in SketchElement::ALL {
        palette = palette.push(
            button(text(element.symbol()).size(type_scale::BODY))
                .on_press(Message::PaletteSelected(element))
                .padding([spacing::XXS, spacing::XS])
                .style(if state.palette == element {
                    theme::primary_button
                } else {
                    theme::secondary_button
                }),
        );
    }

    let sketchpad = container(
        canvas(Sketchpad {
            atoms: state.atoms.clone(),
            bonds: state.bonds.clone(),
            selected: state.selected,
        })
        .width(Fill)
        .height(Length::Fixed(CANVAS_HEIGHT)),
    )
    .style(theme::inset)
    .width(Fill);

    let (status, status_color, valid) = match evaluate(state) {
        Ok((_, formula)) => (
            format!("valid: {}", crate::nomenclature::display_formula(&formula)),
            color::SUCCESS,
            true,
        ),
        Err(problem) => (problem, color::MUTED, false),
    };

    let controls = row![
        button(text("Clear").size(type_scale::BODY))
            .on_press(Message::Cleared)
            .padding([spacing::XS, spacing::SM])
            .style(theme::secondary_button),
        space().width(Fill),
        button(text("Use as reactant").size(type_scale::BODY))
            .on_press_maybe((valid && slot_available).then_some(Message::UseAsReactant))
            .padding([spacing::XS, spacing::SM])
            .style(theme::primary_button),
    ]
    .spacing(spacing::XS)
    .align_y(Center);

    column![
        text("Sketch a molecule")
            .size(type_scale::BODY_LARGE)
            .color(color::TEXT),
        text("Click to place atoms, drag atom to atom to bond, click a bond to raise its order. Double-click removes an atom.")
            .size(type_scale::CAPTION)
            .color(color::MUTED),
        palette,
        sketchpad,
        text(status).size(type_scale::CAPTION).color(status_color),
        controls,
    ]
    .spacing(spacing::XS)
    .into()
}

#[derive(Debug)]
struct Sketchpad {
    atoms: Vec<SketchAtom>,
    bonds: Vec<SketchBond>,
    selected: Option<usize>,
}

#[derive(Debug, Clone, Copy)]
enum PressTarget {
    Atom(usize),
    Bond(usize),
    Empty,
}

#[derive(Debug, Clone, Copy)]
struct Press {
    target: PressTarget,
    origin: Point,
    moved: bool,
}

#[derive(Debug, Default, Clone, Copy)]
struct Interaction {
    press: Option<Press>,
    last_click: Option<(usize, Instant)>,
}

impl Sketchpad {
    fn hit_test(&self, point: Point) -> PressTarget {
        let mut best_atom: Option<(f32, usize)> = None;
        for (index, atom) in self.atoms.iter().enumerate() {
            let distance = point.distance(atom.position);
            if distance <= ATOM_HIT_RADIUS
                && best_atom.is_none_or(|(closest, _)| distance < closest)
            {
                best_atom = Some((distance, index));
            }
        }
        if let Some((_, index)) = best_atom {
            return PressTarget::Atom(index);
        }
        let mut best_bond: Option<(f32, usize)> = None;
        for (index, bond) in self.bonds.iter().enumerate() {
            let (Some(a), Some(b)) = (self.atoms.get(bond.a), self.atoms.get(bond.b)) else {
                continue;
            };
            let distance = segment_distance(point, a.position, b.position);
            if distance <= BOND_HIT_DISTANCE
                && best_bond.is_none_or(|(closest, _)| distance < closest)
            {
                best_bond = Some((distance, index));
            }
        }
        best_bond.map_or(PressTarget::Empty, |(_, index)| PressTarget::Bond(index))
    }
}

fn segment_distance(point: Point, start: Point, end: Point) -> f32 {
    let segment = end - start;
    let length_squared = segment.x * segment.x + segment.y * segment.y;
    if length_squared <= f32::EPSILON {
        return point.distance(start);
    }
    let t = (((point.x - start.x) * segment.x + (point.y - start.y) * segment.y) / length_squared)
        .clamp(0.0, 1.0);
    point.distance(Point::new(start.x + segment.x * t, start.y + segment.y * t))
}

impl canvas::Program<Message> for Sketchpad {
    type State = Interaction;

    fn update(
        &self,
        interaction: &mut Interaction,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: Cursor,
    ) -> Option<canvas::Action<Message>> {
        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let point = cursor.position_in(bounds)?;
                interaction.press = Some(Press {
                    target: self.hit_test(point),
                    origin: point,
                    moved: false,
                });
                Some(canvas::Action::capture())
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let press = interaction.press.as_mut()?;
                let point = cursor.position_in(bounds)?;
                if point.distance(press.origin) > DRAG_THRESHOLD {
                    press.moved = true;
                }
                matches!(press.target, PressTarget::Atom(_)).then(canvas::Action::request_redraw)
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let press = interaction.press.take()?;
                let released = self.release_event(interaction, press, cursor, bounds);
                Some(
                    released.map_or_else(canvas::Action::request_redraw, |event| {
                        canvas::Action::publish(Message::Canvas(event)).and_capture()
                    }),
                )
            }
            _ => None,
        }
    }

    fn mouse_interaction(
        &self,
        interaction: &Interaction,
        bounds: Rectangle,
        cursor: Cursor,
    ) -> mouse::Interaction {
        if interaction
            .press
            .is_some_and(|press| press.moved && matches!(press.target, PressTarget::Atom(_)))
        {
            return mouse::Interaction::Grabbing;
        }
        cursor
            .position_in(bounds)
            .map_or_else(mouse::Interaction::default, |point| {
                match self.hit_test(point) {
                    PressTarget::Atom(_) => mouse::Interaction::Grab,
                    PressTarget::Bond(_) => mouse::Interaction::Pointer,
                    PressTarget::Empty => mouse::Interaction::Crosshair,
                }
            })
    }

    fn draw(
        &self,
        interaction: &Interaction,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        cursor: Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());

        for bond in &self.bonds {
            let (Some(a), Some(b)) = (self.atoms.get(bond.a), self.atoms.get(bond.b)) else {
                continue;
            };
            draw_bond(&mut frame, a.position, b.position, bond.order);
        }

        if let Some(press) = interaction.press
            && press.moved
            && let PressTarget::Atom(index) = press.target
            && let Some(atom) = self.atoms.get(index)
            && let Some(point) = cursor.position_in(bounds)
        {
            frame.stroke(
                &Path::line(atom.position, point),
                Stroke::default()
                    .with_color(color::SELECTION.scale_alpha(0.6))
                    .with_width(1.5),
            );
        }

        for (index, atom) in self.atoms.iter().enumerate() {
            let orders = order_sum(&self.bonds, index);
            let valence = atom.element.valence();
            let over_valence = orders > valence;
            let fill = if over_valence {
                theme::mix(color::SURFACE, color::DANGER, 0.35)
            } else {
                color::SURFACE
            };
            let ring = if over_valence {
                color::DANGER
            } else {
                atom.element.color()
            };
            let circle = Path::circle(atom.position, ATOM_RADIUS);
            frame.fill(&circle, fill);
            frame.stroke(&circle, Stroke::default().with_color(ring).with_width(2.0));
            if self.selected == Some(index) {
                frame.stroke(
                    &Path::circle(atom.position, ATOM_RADIUS + 3.5),
                    Stroke::default()
                        .with_color(color::SELECTION)
                        .with_width(1.5),
                );
            }
            frame.fill_text(canvas::Text {
                content: atom_label(atom.element, orders),
                position: atom.position,
                color: if over_valence {
                    color::DANGER
                } else {
                    color::TEXT
                },
                size: iced::Pixels(11.0),
                align_x: iced::alignment::Horizontal::Center.into(),
                align_y: iced::alignment::Vertical::Center,
                font: fonts::SEMIBOLD,
                ..canvas::Text::default()
            });
        }

        vec![frame.into_geometry()]
    }
}

impl Sketchpad {
    /// Resolves a completed press into a sketch edit, if any.
    fn release_event(
        &self,
        interaction: &mut Interaction,
        press: Press,
        cursor: Cursor,
        bounds: Rectangle,
    ) -> Option<CanvasEvent> {
        match press.target {
            PressTarget::Atom(index) => {
                if press.moved {
                    let target = cursor.position_in(bounds).map(|point| self.hit_test(point));
                    if let Some(PressTarget::Atom(other)) = target
                        && other != index
                    {
                        return Some(CanvasEvent::Bonded {
                            from: index,
                            to: other,
                        });
                    }
                    return None;
                }
                let now = Instant::now();
                let double_click = interaction
                    .last_click
                    .take()
                    .is_some_and(|(last, at)| last == index && now - at < DOUBLE_CLICK_WINDOW);
                if double_click {
                    Some(CanvasEvent::AtomRemoved(index))
                } else {
                    interaction.last_click = Some((index, now));
                    Some(CanvasEvent::AtomClicked(index))
                }
            }
            PressTarget::Bond(index) if !press.moved => Some(CanvasEvent::BondClicked(index)),
            PressTarget::Empty if !press.moved => Some(CanvasEvent::Placed(Point::new(
                press
                    .origin
                    .x
                    .clamp(ATOM_RADIUS, (bounds.width - ATOM_RADIUS).max(ATOM_RADIUS)),
                press
                    .origin
                    .y
                    .clamp(ATOM_RADIUS, (bounds.height - ATOM_RADIUS).max(ATOM_RADIUS)),
            ))),
            PressTarget::Bond(_) | PressTarget::Empty => None,
        }
    }
}

fn atom_label(element: SketchElement, orders: u8) -> String {
    let hydrogens = element.valence().saturating_sub(orders);
    match hydrogens {
        0 => element.symbol().to_owned(),
        1 => format!("{}H", element.symbol()),
        count => format!("{}H{count}", element.symbol()),
    }
}

fn draw_bond(frame: &mut canvas::Frame, start: Point, end: Point, order: u8) {
    let direction = end - start;
    let length = direction.x.hypot(direction.y);
    if length <= f32::EPSILON {
        return;
    }
    let normal = iced::Vector::new(-direction.y / length, direction.x / length);
    let offsets: &[f32] = match order {
        1 => &[0.0],
        2 => &[-2.6, 2.6],
        _ => &[-5.2, 0.0, 5.2],
    };
    for offset in offsets {
        let shift = iced::Vector::new(normal.x * *offset, normal.y * *offset);
        frame.stroke(
            &Path::line(start + shift, end + shift),
            Stroke::default()
                .with_color(color::TEXT_SOFT)
                .with_width(2.0),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(state: &mut State, x: f32, y: f32) {
        update(
            state,
            Message::Canvas(CanvasEvent::Placed(Point::new(x, y))),
        );
    }

    #[test]
    fn bonded_carbons_validate_to_ethane() {
        let mut state = State::default();
        place(&mut state, 40.0, 60.0);
        place(&mut state, 100.0, 60.0);
        update(
            &mut state,
            Message::Canvas(CanvasEvent::Bonded { from: 0, to: 1 }),
        );

        let sketch = submission(&state).expect("ethane is valid");
        let mut atoms = sketch.atoms.clone();
        atoms.sort_unstable();
        assert_eq!(atoms, vec![1, 1, 1, 1, 1, 1, 6, 6]);
        assert_eq!(sketch.smiles, "CC");
    }

    #[test]
    fn cycling_a_bond_past_triple_removes_it() {
        let mut state = State::default();
        place(&mut state, 40.0, 60.0);
        place(&mut state, 100.0, 60.0);
        update(
            &mut state,
            Message::Canvas(CanvasEvent::Bonded { from: 0, to: 1 }),
        );

        update(&mut state, Message::Canvas(CanvasEvent::BondClicked(0)));
        assert_eq!(state.bonds[0].order, 2);
        update(&mut state, Message::Canvas(CanvasEvent::BondClicked(0)));
        assert_eq!(state.bonds[0].order, 3);
        update(&mut state, Message::Canvas(CanvasEvent::BondClicked(0)));
        assert!(state.bonds.is_empty());
    }

    #[test]
    fn over_valent_carbon_is_invalid() {
        let mut state = State::default();
        for index in 0..6_u8 {
            place(&mut state, 40.0 + 30.0 * f32::from(index), 60.0);
        }
        for to in 1..6 {
            update(
                &mut state,
                Message::Canvas(CanvasEvent::Bonded { from: 0, to }),
            );
        }

        assert!(submission(&state).is_none());
        assert_eq!(
            evaluate(&state).unwrap_err(),
            "carbon has five bonds — it supports four"
        );
    }

    #[test]
    fn click_click_bonds_and_double_click_removes() {
        let mut state = State::default();
        place(&mut state, 40.0, 60.0);
        place(&mut state, 100.0, 60.0);

        update(&mut state, Message::Canvas(CanvasEvent::AtomClicked(0)));
        assert_eq!(state.selected, Some(0));
        update(&mut state, Message::Canvas(CanvasEvent::AtomClicked(1)));
        assert_eq!(state.bonds.len(), 1);

        update(&mut state, Message::Canvas(CanvasEvent::AtomRemoved(1)));
        assert_eq!(state.atoms.len(), 1);
        assert!(state.bonds.is_empty());
        assert_eq!(state.selected, None);
    }

    #[test]
    fn disconnected_sketches_are_invalid() {
        let mut state = State::default();
        place(&mut state, 40.0, 60.0);
        place(&mut state, 120.0, 60.0);

        assert!(submission(&state).is_none());
        assert_eq!(
            evaluate(&state).unwrap_err(),
            "connect every atom into one molecule"
        );
    }
}
