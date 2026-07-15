//! Deterministic Stage 3 atomic diagrams.
//!
//! These canvases explain the learner's untrusted workspace composition. They
//! do not infer reactions, construct validated chemistry, or feed simulation.

use std::f32::consts::TAU;

use iced::alignment;
use iced::mouse::Cursor;
use iced::widget::canvas::{self, Path, Stroke};
use iced::{Color, Point, Rectangle, Renderer, Theme, Vector};

use crate::composition_catalogue::CompositionPreview;
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
        draw_atomic_model(&mut frame, self.element, center, maximum_radius, self.phase);

        vec![frame.into_geometry()]
    }
}

#[derive(Debug, Clone)]
pub struct CompoundAtomicDiagram {
    preview: CompositionPreview,
    elements: Vec<ElementSpec>,
    phase: f32,
}

impl CompoundAtomicDiagram {
    pub fn new(
        preview: CompositionPreview,
        elements: impl IntoIterator<Item = ElementSpec>,
        phase: f32,
    ) -> Self {
        Self {
            preview,
            elements: elements.into_iter().collect(),
            phase,
        }
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
        let atoms = arranged_atoms(self.preview.formula, &self.elements, bounds);
        for bond in covalent_bonds(self.preview.formula) {
            if let (Some((_, start)), Some((_, end))) = (atoms.get(bond.start), atoms.get(bond.end))
            {
                draw_shared_pairs(&mut frame, *start, *end, bond.pairs);
            }
        }
        for (element, position) in atoms {
            draw_atomic_model(&mut frame, element, position, 22.0, self.phase);
        }

        vec![frame.into_geometry()]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CovalentBond {
    start: usize,
    end: usize,
    pairs: u8,
}

fn covalent_bonds(formula: &str) -> &'static [CovalentBond] {
    match formula {
        "H₂" => &[CovalentBond {
            start: 0,
            end: 1,
            pairs: 1,
        }],
        "O₂" => &[CovalentBond {
            start: 0,
            end: 1,
            pairs: 2,
        }],
        "H₂O" => &[
            CovalentBond {
                start: 0,
                end: 1,
                pairs: 1,
            },
            CovalentBond {
                start: 1,
                end: 2,
                pairs: 1,
            },
        ],
        "LiOH" => &[CovalentBond {
            start: 1,
            end: 2,
            pairs: 1,
        }],
        "CO₂" => &[
            CovalentBond {
                start: 0,
                end: 1,
                pairs: 2,
            },
            CovalentBond {
                start: 1,
                end: 2,
                pairs: 2,
            },
        ],
        _ => &[],
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
            frame.fill(&Path::circle(electron, 5.0), ELECTRON.scale_alpha(0.16));
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
        _ => {
            let offsets: &[f32] = match elements.len() {
                2 => &[-34.0, 34.0],
                3 => &[-50.0, 0.0, 50.0],
                _ => &[0.0],
            };
            elements
                .iter()
                .copied()
                .zip(offsets.iter().copied())
                .map(|(element, offset)| entry(element, Vector::new(offset, 0.0)))
                .collect()
        }
    }
}

fn draw_atomic_model(
    frame: &mut canvas::Frame,
    element: ElementSpec,
    center: Point,
    maximum_radius: f32,
    phase: f32,
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

    let count = element.valence_electrons.max(1);
    for electron in 0..count {
        let angle = phase * TAU + f32::from(electron) * TAU / f32::from(count);
        let position = Point::new(
            center.x + angle.cos() * maximum_radius,
            center.y + angle.sin() * maximum_radius,
        );
        frame.fill(&Path::circle(position, 2.5), ELECTRON);
        frame.fill(&Path::circle(position, 4.5), ELECTRON.scale_alpha(0.12));
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
        let preview = composition_catalogue::recognize([1, 8, 1]).expect("water preview");
        let elements = [
            *elements::by_atomic_number(1).expect("hydrogen"),
            *elements::by_atomic_number(8).expect("oxygen"),
            *elements::by_atomic_number(1).expect("hydrogen"),
        ];
        let atoms = arranged_atoms(
            preview.formula,
            &elements,
            Rectangle::new(Point::ORIGIN, iced::Size::new(140.0, 80.0)),
        );

        assert_eq!(atoms.len(), 3);
        assert_eq!(atoms[1].0.atomic_number, 8);
        assert!(atoms[0].1.x < atoms[1].1.x && atoms[1].1.x < atoms[2].1.x);
        assert_eq!(covalent_bonds(preview.formula).len(), 2);
        assert_eq!(covalent_bonds("NaCl").len(), 0);
    }
}
