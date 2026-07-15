//! Deterministic Stage 3 atomic diagrams.
//!
//! These canvases explain the learner's untrusted workspace composition. They
//! do not infer reactions, construct validated chemistry, or feed simulation.

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    f32::consts::TAU,
};

use iced::alignment;
use iced::mouse::Cursor;
use iced::widget::canvas::{self, Path, Stroke};
use iced::{Color, Point, Rectangle, Renderer, Theme, Vector};

use crate::composition_catalogue::CompositionPreview;
use crate::elements::ElementSpec;

const SHELL: Color = Color::from_rgba(0.56, 0.77, 1.0, 0.28);
const ELECTRON: Color = Color::from_rgb(0.56, 0.77, 1.0);

#[derive(Debug, Clone, Copy)]
pub struct AtomDiagram {
    element: ElementSpec,
    phase: f32,
}

impl AtomDiagram {
    pub const fn new(element: ElementSpec, phase: f32) -> Self {
        Self { element, phase }
    }
}

impl<Message> canvas::Program<Message> for AtomDiagram {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let center = Point::new(bounds.width / 2.0, bounds.height / 2.0);
        let maximum_radius = (bounds.width.min(bounds.height) / 2.0 - 5.0).max(8.0);
        draw_atomic_model(
            &mut frame,
            self.element,
            center,
            maximum_radius,
            self.phase,
            self.element.valence_electrons,
        );

        vec![frame.into_geometry()]
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct UnknownCompositionDiagram;

impl<Message> canvas::Program<Message> for UnknownCompositionDiagram {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let center = Point::new(bounds.width / 2.0, bounds.height / 2.0);
        let radius = (bounds.width.min(bounds.height) * 0.18).clamp(24.0, 42.0);
        frame.fill(
            &Path::circle(center, radius),
            Color::from_rgb(0.18, 0.22, 0.28),
        );
        frame.stroke(
            &Path::circle(center, radius),
            Stroke::default()
                .with_color(Color::from_rgb(0.96, 0.64, 0.28))
                .with_width(2.0),
        );
        draw_label(
            &mut frame,
            center,
            "?",
            Color::from_rgb(0.96, 0.72, 0.36),
            28.0,
        );
        vec![frame.into_geometry()]
    }
}

#[derive(Debug, Clone)]
pub struct CompoundAtomicDiagram {
    preview: CompositionPreview,
    phase: f32,
}

impl CompoundAtomicDiagram {
    pub const fn new(preview: CompositionPreview, phase: f32) -> Self {
        Self { preview, phase }
    }
}

impl<Message> canvas::Program<Message> for CompoundAtomicDiagram {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let positions = arranged_atoms(&self.preview, bounds);
        for bond in self.preview.covalent_bonds() {
            if let (Some(start), Some(end)) = (positions.get(bond.start), positions.get(bond.end)) {
                draw_shared_pairs(&mut frame, *start, *end, bond.order);
            }
        }
        for link in self.preview.ionic_links() {
            if let (Some(start), Some(end)) = (positions.get(link.start), positions.get(link.end)) {
                draw_ionic_link(&mut frame, *start, *end);
            }
        }
        for (index, atom) in self.preview.atoms.iter().enumerate() {
            let Some(element) = crate::elements::by_atomic_number(atom.atomic_number).copied()
            else {
                continue;
            };
            let position = positions[index];
            draw_atomic_model(
                &mut frame,
                element,
                position,
                22.0,
                self.phase,
                atom.non_bonding_electrons,
            );
            draw_formal_charge(&mut frame, position, atom.formal_charge);
        }

        vec![frame.into_geometry()]
    }
}

fn draw_shared_pairs(frame: &mut canvas::Frame, start: Point, end: Point, pairs: u8) {
    let midpoint = Point::new(start.x.midpoint(end.x), start.y.midpoint(end.y));
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let magnitude = (dx * dx + dy * dy).sqrt().max(1.0);
    let along = Vector::new(dx / magnitude, dy / magnitude);
    let perpendicular = Vector::new(-along.y, along.x);

    for pair in 0..pairs {
        let pair_offset = if pairs == 1 {
            0.0
        } else if pair == 0 {
            -5.0
        } else {
            5.0
        };
        let pair_center = midpoint + perpendicular * pair_offset;
        for direction in [-1.0, 1.0] {
            let electron = pair_center + along * (direction * 2.8);
            frame.fill(&Path::circle(electron, 2.6), ELECTRON);
            frame.fill(
                &Path::circle(electron, 5.0),
                Color::from_rgba(ELECTRON.r, ELECTRON.g, ELECTRON.b, 0.16),
            );
        }
    }
}

fn arranged_atoms(preview: &CompositionPreview, bounds: Rectangle) -> Vec<Point> {
    let center = Point::new(bounds.width / 2.0, bounds.height / 2.0);
    let count = preview.atoms.len();
    if count == 0 {
        return Vec::new();
    }
    if count == 1 {
        return vec![center];
    }
    if count == 2 {
        return vec![
            center + Vector::new(-36.0, 0.0),
            center + Vector::new(36.0, 0.0),
        ];
    }

    let mut adjacency = vec![BTreeSet::new(); count];
    let mut bond_orders = BTreeMap::new();
    for bond in preview.covalent_bonds() {
        adjacency[bond.start].insert(bond.end);
        adjacency[bond.end].insert(bond.start);
        bond_orders.insert(sorted_indices(bond.start, bond.end), bond.order);
    }
    for link in preview.ionic_links() {
        adjacency[link.start].insert(link.end);
        adjacency[link.end].insert(link.start);
    }
    let root = (0..count)
        .max_by(|left, right| {
            adjacency[*left]
                .len()
                .cmp(&adjacency[*right].len())
                .then_with(|| right.cmp(left))
        })
        .unwrap_or(0);
    let spacing = (bounds.width.min(bounds.height) * 0.19).clamp(48.0, 72.0);
    let mut positions = vec![center; count];
    let mut seen = BTreeSet::from([root]);
    let mut queue = VecDeque::from([(root, 0_usize, 0.0_f32)]);
    while let Some((parent, depth, parent_angle)) = queue.pop_front() {
        let children = adjacency[parent]
            .iter()
            .copied()
            .filter(|child| !seen.contains(child))
            .collect::<Vec<_>>();
        let parent_position = positions[parent];
        let linear_root = depth == 0
            && children.len() == 2
            && children
                .iter()
                .map(|child| {
                    bond_orders
                        .get(&sorted_indices(parent, *child))
                        .copied()
                        .unwrap_or(0)
                })
                .sum::<u8>()
                >= 4;
        for (ordinal, child) in children.iter().copied().enumerate() {
            seen.insert(child);
            let angle = if linear_root {
                [std::f32::consts::PI, 0.0][ordinal]
            } else if depth == 0 && children.len() == 2 {
                [std::f32::consts::PI - 0.52, 0.52][ordinal]
            } else if depth == 0 {
                -std::f32::consts::FRAC_PI_2 + TAU * ordinal as f32 / children.len().max(1) as f32
            } else {
                parent_angle - 0.55 + 1.1 * (ordinal as f32 + 0.5) / children.len().max(1) as f32
            };
            positions[child] =
                parent_position + Vector::new(angle.cos() * spacing, angle.sin() * spacing);
            queue.push_back((child, depth + 1, angle));
        }
    }
    for index in 0..count {
        if !seen.contains(&index) {
            let angle = TAU * index as f32 / count as f32;
            positions[index] = center + Vector::new(angle.cos() * spacing, angle.sin() * spacing);
        }
    }
    positions
}

const fn sorted_indices(left: usize, right: usize) -> (usize, usize) {
    if left <= right {
        (left, right)
    } else {
        (right, left)
    }
}

fn draw_ionic_link(frame: &mut canvas::Frame, start: Point, end: Point) {
    let delta = end - start;
    for step in 2_u8..10 {
        let progress = f32::from(step) / 11.0;
        frame.fill(
            &Path::circle(start + delta * progress, 1.5),
            Color::from_rgba(0.96, 0.72, 0.36, 0.72),
        );
    }
}

fn draw_formal_charge(frame: &mut canvas::Frame, center: Point, charge: i16) {
    if charge == 0 {
        return;
    }
    let sign = if charge > 0 { "+" } else { "-" };
    let magnitude = charge.unsigned_abs();
    let label = if magnitude == 1 {
        sign.to_owned()
    } else {
        format!("{magnitude}{sign}")
    };
    draw_label(
        frame,
        center + Vector::new(14.0, -14.0),
        &label,
        Color::from_rgb(0.96, 0.72, 0.36),
        10.0,
    );
}

fn draw_atomic_model(
    frame: &mut canvas::Frame,
    element: ElementSpec,
    center: Point,
    maximum_radius: f32,
    phase: f32,
    outer_shell_electrons: u8,
) {
    let shell_count = element.period.max(1);
    for shell in 1..=shell_count {
        let radius = maximum_radius * f32::from(shell) / f32::from(shell_count);
        frame.stroke(
            &Path::circle(center, radius),
            Stroke::default().with_color(SHELL).with_width(1.0),
        );
    }

    let nucleus_color = element_color(element.atomic_number);
    frame.fill(
        &Path::circle(center, (maximum_radius * 0.28).clamp(6.0, 14.0)),
        nucleus_color,
    );
    draw_label(frame, center, element.symbol, Color::BLACK, 11.0);

    let count = outer_shell_electrons;
    for electron in 0..count {
        let angle = phase * TAU + f32::from(electron) * TAU / f32::from(count);
        let position = Point::new(
            center.x + angle.cos() * maximum_radius,
            center.y + angle.sin() * maximum_radius,
        );
        frame.fill(&Path::circle(position, 2.5), ELECTRON);
        frame.fill(
            &Path::circle(position, 4.5),
            Color::from_rgba(ELECTRON.r, ELECTRON.g, ELECTRON.b, 0.12),
        );
    }
}

fn draw_label(frame: &mut canvas::Frame, position: Point, content: &str, color: Color, size: f32) {
    frame.fill_text(canvas::Text {
        content: content.to_owned(),
        position,
        color,
        size: iced::Pixels(size),
        align_x: iced::alignment::Horizontal::Center.into(),
        align_y: alignment::Vertical::Center,
        ..canvas::Text::default()
    });
}

const fn element_color(atomic_number: u8) -> Color {
    match atomic_number {
        1 => Color::from_rgb(0.94, 0.96, 0.98),
        3 => Color::from_rgb(0.79, 0.56, 0.95),
        6 => Color::from_rgb(0.39, 0.46, 0.54),
        8 => Color::from_rgb(0.96, 0.39, 0.43),
        11 => Color::from_rgb(0.56, 0.48, 0.94),
        17 => Color::from_rgb(0.42, 0.86, 0.58),
        _ => Color::from_rgb(0.56, 0.77, 1.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::composition_catalogue;

    #[test]
    fn catalogue_graph_places_water_around_oxygen_without_formula_switches() {
        let preview = composition_catalogue::recognize([1, 8, 1]).expect("water preview");
        let atoms = arranged_atoms(
            &preview,
            Rectangle::new(Point::ORIGIN, iced::Size::new(180.0, 120.0)),
        );
        let oxygen = preview
            .atoms
            .iter()
            .position(|atom| atom.atomic_number == 8)
            .expect("oxygen");
        let hydrogens = preview
            .atoms
            .iter()
            .enumerate()
            .filter_map(|(index, atom)| (atom.atomic_number == 1).then_some(index))
            .collect::<Vec<_>>();

        assert_eq!(atoms.len(), 3);
        assert!(atoms[hydrogens[0]].x < atoms[oxygen].x);
        assert!(atoms[oxygen].x < atoms[hydrogens[1]].x);
        assert_eq!(preview.covalent_bonds().len(), 2);
    }

    #[test]
    fn carbon_dioxide_catalogue_graph_is_linear() {
        let preview = composition_catalogue::recognize([8, 6, 8]).expect("CO2 preview");
        let positions = arranged_atoms(
            &preview,
            Rectangle::new(Point::ORIGIN, iced::Size::new(220.0, 120.0)),
        );
        let carbon = preview
            .atoms
            .iter()
            .position(|atom| atom.atomic_number == 6)
            .expect("carbon");
        let oxygens = preview
            .atoms
            .iter()
            .enumerate()
            .filter_map(|(index, atom)| (atom.atomic_number == 8).then_some(index))
            .collect::<Vec<_>>();
        assert!(positions[oxygens[0]].x < positions[carbon].x);
        assert!(positions[carbon].x < positions[oxygens[1]].x);
        assert!((positions[oxygens[0]].y - positions[oxygens[1]].y).abs() < 0.01);
    }

    #[test]
    fn lithium_hydroxide_layout_uses_covalent_and_ionic_topology() {
        let preview = composition_catalogue::recognize([3, 8, 1]).expect("LiOH preview");
        assert_eq!(preview.covalent_bonds().len(), 1);
        assert_eq!(preview.ionic_links().len(), 1);
        let linked_atoms = preview
            .ionic_links()
            .iter()
            .flat_map(|link| [link.start, link.end])
            .map(|index| preview.atoms[index].atomic_number)
            .collect::<BTreeSet<_>>();
        assert_eq!(linked_atoms, BTreeSet::from([3, 8]));
    }
}
