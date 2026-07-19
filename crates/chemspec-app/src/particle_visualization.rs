//! Deterministic Stage 3 atomic diagrams.
//!
//! These canvases explain the learner's untrusted workspace composition. They
//! do not infer reactions, construct validated chemistry, or feed simulation.

use std::f32::consts::TAU;

use iced::alignment;
use iced::mouse::Cursor;
use iced::widget::canvas::{self, Path, Stroke};
use iced::{Color, Point, Rectangle, Renderer, Size, Theme, Vector};

use crate::composition_catalogue::ReferenceCompositionPreview;
use crate::elements::ElementSpec;
use crate::fonts;
use crate::theme::{LAB_DARK, chemistry_color};

const SHELL: Color = Color {
    a: 0.28,
    ..chemistry_color::ELECTRON
};
const ELECTRON: Color = chemistry_color::ELECTRON;

#[derive(Debug, Clone, Copy)]
pub struct AtomDiagram {
    element: ElementSpec,
    phase: f32,
    reveal: f32,
}

impl AtomDiagram {
    pub const fn new(element: ElementSpec, phase: f32) -> Self {
        Self {
            element,
            phase,
            reveal: 1.0,
        }
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
        draw_atomic_model_revealed(
            &mut frame,
            self.element,
            center,
            maximum_radius,
            self.phase,
            self.reveal,
        );

        vec![frame.into_geometry()]
    }
}

#[derive(Debug, Clone)]
pub struct CompoundAtomicDiagram {
    preview: ReferenceCompositionPreview,
    elements: Vec<ElementSpec>,
    phase: f32,
    reveal: f32,
    show_shells: bool,
}

impl CompoundAtomicDiagram {
    pub fn new(preview: ReferenceCompositionPreview, phase: f32) -> Self {
        let elements = preview
            .atoms
            .iter()
            .filter_map(|atom| crate::elements::by_atomic_number(atom.atomic_number).copied())
            .collect();
        Self {
            preview,
            elements,
            phase,
            reveal: 1.0,
            show_shells: true,
        }
    }

    /// Uses a compact ball-and-stick treatment for selection cards where a
    /// full shell model would make larger molecules illegible.
    pub const fn structure_only(mut self) -> Self {
        self.show_shells = false;
        self
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
        draw_compound(
            &mut frame,
            &self.preview,
            &self.elements,
            bounds,
            self.phase,
            self.reveal,
            self.show_shells,
        );

        vec![frame.into_geometry()]
    }
}

/// A quiet, non-interactive reactant model intended to sit behind the element
/// library. Its drawing envelope is normalized independently from atom count,
/// so a single helium atom remains legible while larger compounds stay bounded.
#[derive(Debug, Clone)]
pub struct AmbientReactantDiagram {
    atoms: Vec<u8>,
    preview: Option<ReferenceCompositionPreview>,
    elements: Vec<ElementSpec>,
    phase: f32,
    reveal: f32,
    scale: f32,
    anchor: Point,
    offset: Vector,
    direction: f32,
}

impl AmbientReactantDiagram {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        atoms: Vec<u8>,
        name: Option<&str>,
        phase: f32,
        reveal: f32,
        scale: f32,
        anchor: Point,
        offset: Vector,
        direction: f32,
    ) -> Self {
        let preview = name.map_or_else(
            || crate::composition_catalogue::reference_preview(atoms.iter().copied()),
            |name| {
                crate::composition_catalogue::reference_preview_named(name, atoms.iter().copied())
            },
        );
        let elements = atoms
            .iter()
            .filter_map(|number| crate::elements::by_atomic_number(*number).copied())
            .collect();

        Self {
            atoms,
            preview,
            elements,
            phase,
            reveal,
            scale,
            anchor,
            offset,
            direction,
        }
    }
}

impl<Message> canvas::Program<Message> for AmbientReactantDiagram {
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
        if self.atoms.is_empty() || self.elements.is_empty() || self.reveal <= 0.0 {
            return vec![frame.into_geometry()];
        }

        let count = self.elements.len();
        let scale = self.scale.clamp(0.78, 1.18);
        let footprint = ambient_footprint(count, bounds.size(), scale);
        let drift = Vector::new(
            (self.phase * TAU).sin() * 5.0 * self.direction,
            (self.phase * TAU * 0.73).cos() * 4.0,
        );
        // Most of the model floats in the open air above the key grid; its
        // lower edge is the part that passes behind the periodic table.
        let center = Point::new(bounds.width * self.anchor.x, bounds.height * self.anchor.y)
            + self.offset
            + drift;
        // The low alpha keeps the model atmospheric behind the key grid, not
        // a second foreground control. Reveal is animated by the composer.
        let reveal = self.reveal.clamp(0.0, 1.0) * 0.20;

        if count == 1 {
            draw_atomic_model_revealed(
                &mut frame,
                self.elements[0],
                center,
                footprint.width / 2.0,
                self.phase,
                reveal,
            );
        } else {
            let model_bounds = Rectangle {
                x: center.x - footprint.width / 2.0,
                y: center.y - footprint.height / 2.0,
                width: footprint.width,
                height: footprint.height,
            };

            if let Some(preview) = &self.preview {
                draw_compound(
                    &mut frame,
                    preview,
                    &self.elements,
                    model_bounds,
                    self.phase,
                    reveal,
                    false,
                );
            } else {
                draw_draft_structure(&mut frame, &self.elements, model_bounds, reveal);
            }
        }

        vec![frame.into_geometry()]
    }
}

fn ambient_base_extent(bounds: Rectangle) -> f32 {
    let short_side = bounds.width.min(bounds.height).max(0.0);
    (short_side * 0.54)
        .clamp(96.0, 300.0)
        .min((short_side - 12.0).max(24.0))
}

/// Pixel footprint used by both the Canvas renderer and the builder's ambient
/// placement solver. Keeping one measurement prevents layout decisions from
/// drifting away from what is actually drawn.
pub(crate) fn ambient_footprint(atom_count: usize, bounds: Size, scale: f32) -> Size {
    let bounds = Rectangle::new(Point::ORIGIN, bounds);
    let base_extent = ambient_base_extent(bounds);
    let scale = scale.clamp(0.78, 1.18);
    if atom_count <= 1 {
        let diameter = base_extent * 0.86 * scale;
        Size::new(diameter, diameter)
    } else {
        let complexity = match atom_count {
            0..=2 => 1.4,
            3..=4 => 2.0,
            5..=8 => 2.8,
            _ => 3.5,
        };
        Size::new(
            (base_extent * (1.08 + complexity * 0.10) * scale).min(bounds.width * 0.78),
            (base_extent * 0.82 * scale).min(bounds.height * 0.68),
        )
    }
}

fn draw_compound(
    frame: &mut canvas::Frame,
    preview: &ReferenceCompositionPreview,
    elements: &[ElementSpec],
    bounds: Rectangle,
    phase: f32,
    reveal: f32,
    show_shells: bool,
) {
    let atoms = arranged_atoms(preview, elements, bounds);
    for bond in preview.covalent_bonds() {
        if let (Some((_, start)), Some((_, end))) = (atoms.get(bond.start), atoms.get(bond.end)) {
            draw_shared_pairs(frame, *start, *end, bond.order, reveal);
        }
    }
    for link in preview.ionic_links() {
        if let (Some((_, start)), Some((_, end))) = (atoms.get(link.start), atoms.get(link.end)) {
            frame.stroke(
                &Path::line(*start, *end),
                Stroke::default()
                    .with_color(chemistry_color::IONIC.scale_alpha(0.35 * reveal))
                    .with_width(2.0),
            );
        }
    }
    for (element, position) in atoms {
        let radius = compound_atom_radius(element).min(layout_atom_radius(bounds, elements.len()));
        if show_shells {
            draw_atomic_model_revealed(frame, element, position, radius, phase, reveal);
        } else {
            draw_structure_atom(frame, element, position, radius, reveal);
        }
    }
}

#[allow(clippy::cast_precision_loss)]
fn draw_draft_structure(
    frame: &mut canvas::Frame,
    elements: &[ElementSpec],
    bounds: Rectangle,
    reveal: f32,
) {
    let columns = elements.len().min(4);
    let rows = elements.len().div_ceil(columns);
    let cell_width = bounds.width / columns as f32;
    let cell_height = bounds.height / rows as f32;
    let radius = (cell_width.min(cell_height) * 0.24).clamp(9.0, 18.0);

    for (index, element) in elements.iter().copied().enumerate() {
        let column = index % columns;
        let row = index / columns;
        let position = Point::new(
            bounds.x + (column as f32 + 0.5) * cell_width,
            bounds.y + (row as f32 + 0.5) * cell_height,
        );
        draw_structure_atom(frame, element, position, radius, reveal);
    }
}

fn draw_structure_atom(
    frame: &mut canvas::Frame,
    element: ElementSpec,
    center: Point,
    radius: f32,
    reveal: f32,
) {
    let radius = (radius * 0.72).clamp(9.0, 18.0);
    let color = element_color(element.atomic_number);
    frame.fill(
        &Path::circle(center, radius + 2.0),
        color.scale_alpha(0.20 * reveal),
    );
    frame.fill(&Path::circle(center, radius), color.scale_alpha(reveal));
    frame.stroke(
        &Path::circle(center, radius),
        Stroke::default()
            .with_color(Color::WHITE.scale_alpha(0.28 * reveal))
            .with_width(1.0),
    );
    draw_label(
        frame,
        center,
        element.symbol,
        symbol_color(color).scale_alpha(reveal),
        11.0,
    );
}

fn draw_shared_pairs(frame: &mut canvas::Frame, start: Point, end: Point, pairs: u8, reveal: f32) {
    // A soft bridge under the electron pairs makes the bond itself legible
    // instead of leaving four dots floating in space.
    let bond = Path::line(start, end);
    frame.stroke(
        &bond,
        Stroke::default()
            .with_color(ELECTRON.scale_alpha(0.06 * reveal))
            .with_width(11.0),
    );
    frame.stroke(
        &bond,
        Stroke::default()
            .with_color(ELECTRON.scale_alpha(0.14 * reveal))
            .with_width(4.0),
    );

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
            frame.fill(&Path::circle(electron, 2.6), ELECTRON.scale_alpha(reveal));
            frame.fill(
                &Path::circle(electron, 5.0),
                ELECTRON.scale_alpha(0.16 * reveal),
            );
        }
    }
}

#[allow(clippy::cast_precision_loss)]
fn arranged_atoms(
    preview: &ReferenceCompositionPreview,
    elements: &[ElementSpec],
    bounds: Rectangle,
) -> Vec<(ElementSpec, Point)> {
    let center = Point::new(
        bounds.x + bounds.width / 2.0,
        bounds.y + bounds.height / 2.0,
    );
    if elements.is_empty() {
        return Vec::new();
    }
    let count = elements.len();
    let mut adjacency = vec![Vec::<usize>::new(); count];
    for (start, end) in preview
        .covalent_bonds()
        .iter()
        .map(|bond| (bond.start, bond.end))
        .chain(
            preview
                .ionic_links()
                .iter()
                .map(|link| (link.start, link.end)),
        )
    {
        if start < count && end < count && !adjacency[start].contains(&end) {
            adjacency[start].push(end);
            adjacency[end].push(start);
        }
    }
    let radius = (bounds.width.min(bounds.height) * 0.34).max(30.0);
    let mut positions = vec![center; count];

    if count == 2 {
        positions[0] = center + Vector::new(-radius * 0.62, 0.0);
        positions[1] = center + Vector::new(radius * 0.62, 0.0);
    } else if let Some(root) = adjacency
        .iter()
        .position(|neighbours| neighbours.len() == count - 1)
    {
        let neighbours = &adjacency[root];
        let bent = matches!(preview.formula.as_str(), "H₂O" | "H₂S") && neighbours.len() == 2;
        positions[root] = center + Vector::new(0.0, if bent { -radius * 0.18 } else { 0.0 });
        for (ordinal, neighbour) in neighbours.iter().copied().enumerate() {
            let angle = if bent {
                [TAU * 0.08, TAU * 0.42][ordinal]
            } else {
                ordinal as f32 * TAU / neighbours.len() as f32 - TAU / 4.0
            };
            positions[neighbour] = center
                + Vector::new(
                    angle.cos() * radius,
                    angle.sin() * radius + if bent { radius * 0.16 } else { 0.0 },
                );
        }
    } else if let Some(start) = adjacency
        .iter()
        .position(|neighbours| neighbours.len() == 1)
        && adjacency.iter().map(Vec::len).sum::<usize>() / 2 == count - 1
    {
        let mut order = Vec::with_capacity(count);
        let mut previous = None;
        let mut current = start;
        loop {
            order.push(current);
            let next = adjacency[current]
                .iter()
                .copied()
                .find(|candidate| Some(*candidate) != previous);
            let Some(next) = next else { break };
            previous = Some(current);
            current = next;
        }
        let spacing = (bounds.width * 0.68 / (count.saturating_sub(1) as f32)).min(52.0);
        let origin = -(count.saturating_sub(1) as f32 * spacing) / 2.0;
        for (ordinal, index) in order.into_iter().enumerate() {
            positions[index] = center
                + Vector::new(
                    origin + ordinal as f32 * spacing,
                    if ordinal % 2 == 0 { -5.0 } else { 5.0 },
                );
        }
    } else {
        for (index, position) in positions.iter_mut().enumerate() {
            let angle = index as f32 * TAU / count as f32 - TAU / 4.0;
            *position = center + Vector::new(angle.cos() * radius, angle.sin() * radius);
        }
    }

    elements.iter().copied().zip(positions).collect()
}

fn layout_atom_radius(bounds: Rectangle, atom_count: usize) -> f32 {
    let scale = match atom_count {
        0..=3 => 0.22,
        4..=5 => 0.16,
        _ => 0.11,
    };
    (bounds.width.min(bounds.height) * scale).clamp(10.0, 28.0)
}

/// Compound members scale with their shell count, so hydrogen reads smaller
/// than oxygen without touching the trusted element data.
fn compound_atom_radius(element: ElementSpec) -> f32 {
    10.0 + 6.0 * f32::from(element.period.min(4))
}

fn draw_atomic_model_revealed(
    frame: &mut canvas::Frame,
    element: ElementSpec,
    center: Point,
    maximum_radius: f32,
    phase: f32,
    reveal: f32,
) {
    // Inner shells stay faint; the valence shell — where the chemistry
    // happens — reads strongest.
    let shell_count = element.period.max(1);
    for shell in 1..=shell_count {
        let radius = maximum_radius * f32::from(shell) / f32::from(shell_count);
        let emphasis = if shell == shell_count { 1.25 } else { 0.55 };
        frame.stroke(
            &Path::circle(center, radius),
            Stroke::default()
                .with_color(SHELL.scale_alpha(emphasis * reveal))
                .with_width(1.0),
        );
    }

    // The nucleus gets simple depth: a darker rim, the element colour, and a
    // small specular lift toward the light.
    let nucleus_color = element_color(element.atomic_number);
    let nucleus_radius = (maximum_radius * 0.28).clamp(6.0, 14.0);
    let rim = Color {
        r: nucleus_color.r * 0.55,
        g: nucleus_color.g * 0.55,
        b: nucleus_color.b * 0.55,
        a: nucleus_color.a,
    };
    frame.fill(
        &Path::circle(center, nucleus_radius + 1.2),
        rim.scale_alpha(reveal),
    );
    frame.fill(
        &Path::circle(center, nucleus_radius),
        nucleus_color.scale_alpha(reveal),
    );
    let highlight_center = Point::new(
        center.x - nucleus_radius * 0.30,
        center.y - nucleus_radius * 0.30,
    );
    frame.fill(
        &Path::circle(highlight_center, nucleus_radius * 0.55),
        Color::WHITE.scale_alpha(0.16 * reveal),
    );
    draw_label(
        frame,
        center,
        element.symbol,
        symbol_color(nucleus_color).scale_alpha(reveal),
        11.0,
    );

    let count = element.valence_electrons.max(1);
    for electron in 0..count {
        let angle = phase * TAU + f32::from(electron) * TAU / f32::from(count);
        let position = Point::new(
            center.x + angle.cos() * maximum_radius,
            center.y + angle.sin() * maximum_radius,
        );
        frame.fill(&Path::circle(position, 2.5), ELECTRON.scale_alpha(reveal));
        frame.fill(
            &Path::circle(position, 4.5),
            ELECTRON.scale_alpha(0.12 * reveal),
        );
    }
}

/// Black or white, whichever contrasts with the nucleus colour.
fn symbol_color(nucleus: Color) -> Color {
    let luminance = 0.2126 * nucleus.r + 0.7152 * nucleus.g + 0.0722 * nucleus.b;
    if luminance > 0.55 {
        Color::BLACK
    } else {
        Color::WHITE
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
        font: fonts::REGULAR,
        ..canvas::Text::default()
    });
}

const fn element_color(atomic_number: u8) -> Color {
    match atomic_number {
        1 => LAB_DARK.chemistry.hydrogen,
        3 => LAB_DARK.chemistry.lithium,
        6 => LAB_DARK.chemistry.carbon,
        8 => LAB_DARK.chemistry.oxygen,
        11 => LAB_DARK.chemistry.sodium,
        17 => LAB_DARK.chemistry.chlorine,
        _ => LAB_DARK.chemistry.element_default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::composition_catalogue;
    use crate::elements;

    #[test]
    fn water_layout_places_oxygen_between_two_hydrogens() {
        let preview =
            composition_catalogue::reference_preview([1, 8, 1]).expect("trusted water preview");
        let elements = preview
            .atoms
            .iter()
            .map(|atom| {
                *elements::by_atomic_number(atom.atomic_number).expect("catalogued element")
            })
            .collect::<Vec<_>>();
        let atoms = arranged_atoms(
            &preview,
            &elements,
            Rectangle::new(Point::ORIGIN, iced::Size::new(140.0, 80.0)),
        );

        assert_eq!(atoms.len(), 3);
        let oxygen = atoms
            .iter()
            .find(|(element, _)| element.atomic_number == 8)
            .expect("oxygen is laid out");
        assert!(
            atoms
                .iter()
                .filter(|(element, _)| element.atomic_number == 1)
                .all(|(_, point)| point.y > oxygen.1.y)
        );
        assert_eq!(preview.covalent_bonds().len(), 2);
        assert!(preview.ionic_links().is_empty());
    }

    #[test]
    fn reviewed_if7_layout_places_iodine_at_the_bond_hub() {
        let preview = composition_catalogue::reference_preview_by_structure_id("InterhalogenIF7")
            .expect("trusted IF7 preview");
        let elements = preview
            .atoms
            .iter()
            .map(|atom| *elements::by_atomic_number(atom.atomic_number).unwrap())
            .collect::<Vec<_>>();
        let atoms = arranged_atoms(
            &preview,
            &elements,
            Rectangle::new(Point::ORIGIN, iced::Size::new(220.0, 150.0)),
        );
        let iodine = atoms
            .iter()
            .position(|(element, _)| element.atomic_number == 53)
            .expect("iodine is present");
        assert_eq!(preview.covalent_bonds().len(), 7);
        assert!(
            preview
                .covalent_bonds()
                .iter()
                .all(|bond| bond.start == iodine || bond.end == iodine)
        );
        assert_eq!(atoms[iodine].1, Point::new(110.0, 75.0));
    }

    #[test]
    fn ambient_extent_keeps_small_atoms_legible_and_large_models_bounded() {
        let compact = Rectangle::new(Point::ORIGIN, iced::Size::new(240.0, 180.0));
        let desktop = Rectangle::new(Point::ORIGIN, iced::Size::new(760.0, 520.0));
        let huge = Rectangle::new(Point::ORIGIN, iced::Size::new(2_000.0, 1_400.0));

        assert!(ambient_base_extent(compact) >= 96.0);
        assert!(ambient_base_extent(desktop) >= 240.0);
        assert!((ambient_base_extent(huge) - 300.0).abs() < f32::EPSILON);
        assert!(ambient_base_extent(compact) <= compact.height - 12.0);
    }
}
