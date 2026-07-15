//! Deterministic Stage 3 atomic diagrams.
//!
//! These canvases explain the learner's untrusted workspace composition. They
//! do not infer reactions, construct validated chemistry, or feed simulation.

use std::f32::consts::TAU;

use iced::alignment;
use iced::mouse::Cursor;
use iced::widget::canvas::{self, Path, Stroke};
use iced::{Color, Point, Rectangle, Renderer, Theme, Vector};

use crate::composition_catalogue::TrustedCompositionPreview;
use crate::elements::ElementSpec;
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

    /// Fades the whole diagram with a 0..=1 reveal progress.
    pub const fn with_reveal(mut self, reveal: f32) -> Self {
        self.reveal = reveal;
        self
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
    preview: TrustedCompositionPreview,
    elements: Vec<ElementSpec>,
    phase: f32,
    reveal: f32,
}

impl CompoundAtomicDiagram {
    pub fn new(preview: TrustedCompositionPreview, phase: f32) -> Self {
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
        }
    }

    /// Fades the whole diagram with a 0..=1 reveal progress.
    pub const fn with_reveal(mut self, reveal: f32) -> Self {
        self.reveal = reveal;
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
        let atoms = arranged_atoms(&self.preview.formula, &self.elements, bounds);
        for bond in self.preview.covalent_bonds() {
            if let (Some((_, start)), Some((_, end))) = (atoms.get(bond.start), atoms.get(bond.end))
            {
                draw_shared_pairs(&mut frame, *start, *end, bond.order, self.reveal);
            }
        }
        for link in self.preview.ionic_links() {
            if let (Some((_, start)), Some((_, end))) = (atoms.get(link.start), atoms.get(link.end))
            {
                frame.stroke(
                    &Path::line(*start, *end),
                    Stroke::default()
                        .with_color(chemistry_color::IONIC.scale_alpha(0.35 * self.reveal))
                        .with_width(2.0),
                );
            }
        }
        for (element, position) in atoms {
            draw_atomic_model_revealed(
                &mut frame,
                element,
                position,
                compound_atom_radius(element),
                self.phase,
                self.reveal,
            );
        }

        vec![frame.into_geometry()]
    }
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

fn arranged_atoms(
    formula: &str,
    elements: &[ElementSpec],
    bounds: Rectangle,
) -> Vec<(ElementSpec, Point)> {
    let center = Point::new(bounds.width / 2.0, bounds.height / 2.0);
    let find = |atomic_number| {
        elements
            .iter()
            .copied()
            .find(|element| element.atomic_number == atomic_number)
    };
    let entry = |element, offset: Vector| (element, center + offset);

    match formula {
        "H₂O" => [
            find(1).map(|element| entry(element, Vector::new(-48.0, 12.0))),
            find(8).map(|element| entry(element, Vector::new(0.0, -8.0))),
            find(1).map(|element| entry(element, Vector::new(48.0, 12.0))),
        ]
        .into_iter()
        .flatten()
        .collect(),
        "LiOH" => [
            find(3).map(|element| entry(element, Vector::new(-50.0, 0.0))),
            find(8).map(|element| entry(element, Vector::new(0.0, 0.0))),
            find(1).map(|element| entry(element, Vector::new(50.0, 0.0))),
        ]
        .into_iter()
        .flatten()
        .collect(),
        "CO₂" => [
            find(8).map(|element| entry(element, Vector::new(-50.0, 0.0))),
            find(6).map(|element| entry(element, Vector::new(0.0, 0.0))),
            find(8).map(|element| entry(element, Vector::new(50.0, 0.0))),
        ]
        .into_iter()
        .flatten()
        .collect(),
        _ if elements.len() <= 3 => {
            let spacing = 50.0;
            let element_count = u16::try_from(elements.len()).unwrap_or(u16::MAX);
            let origin = -(f32::from(element_count.saturating_sub(1)) * spacing) / 2.0;
            elements
                .iter()
                .copied()
                .enumerate()
                .map(|(index, element)| {
                    let index = u16::try_from(index).unwrap_or(u16::MAX);
                    entry(
                        element,
                        Vector::new(origin + f32::from(index) * spacing, 0.0),
                    )
                })
                .collect()
        }
        _ => {
            let radius = (bounds.width.min(bounds.height) * 0.30).max(34.0);
            let element_count = u16::try_from(elements.len()).unwrap_or(u16::MAX);
            elements
                .iter()
                .copied()
                .enumerate()
                .map(|(index, element)| {
                    let index = u16::try_from(index).unwrap_or(u16::MAX);
                    let angle = f32::from(index) * TAU / f32::from(element_count) - TAU / 4.0;
                    entry(
                        element,
                        Vector::new(angle.cos() * radius, angle.sin() * radius),
                    )
                })
                .collect()
        }
    }
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
            composition_catalogue::trusted_preview([1, 8, 1]).expect("trusted water preview");
        let elements = preview
            .atoms
            .iter()
            .map(|atom| {
                *elements::by_atomic_number(atom.atomic_number).expect("catalogued element")
            })
            .collect::<Vec<_>>();
        let atoms = arranged_atoms(
            &preview.formula,
            &elements,
            Rectangle::new(Point::ORIGIN, iced::Size::new(140.0, 80.0)),
        );

        assert_eq!(atoms.len(), 3);
        assert_eq!(atoms[1].0.atomic_number, 8);
        assert!(atoms[0].1.x < atoms[1].1.x && atoms[1].1.x < atoms[2].1.x);
        assert_eq!(preview.covalent_bonds().len(), 2);
        assert!(preview.ionic_links().is_empty());
    }
}
