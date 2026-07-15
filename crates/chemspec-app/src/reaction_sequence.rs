//! Deterministic renderer for trusted `SimulationFrame` values.

use std::collections::BTreeMap;

use chem_domain::{AtomId, CovalentElectronOrigin};
use chem_kernel::{SimulationFrame, SimulationFrames};
use iced::alignment;
use iced::mouse::Cursor;
use iced::widget::canvas::{self, Path, Stroke};
use iced::{Color, Point, Rectangle, Renderer, Theme, Vector};

use crate::theme::{LAB_DARK, chemistry_color, color};

const COVALENT: Color = chemistry_color::COVALENT;
const IONIC: Color = chemistry_color::IONIC;
const METALLIC: Color = chemistry_color::METALLIC;
const GRID: Color = Color {
    a: 0.045,
    ..color::ACCENT
};

#[derive(Debug, Clone, Copy)]
pub struct ReactionSequenceDiagram<'a> {
    frames: &'a SimulationFrames,
    frame_index: usize,
}

impl<'a> ReactionSequenceDiagram<'a> {
    pub const fn new(frames: &'a SimulationFrames, frame_index: usize) -> Self {
        Self {
            frames,
            frame_index,
        }
    }
}

impl<Message> canvas::Program<Message> for ReactionSequenceDiagram<'_> {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut canvas = canvas::Frame::new(renderer, bounds.size());
        draw_grid(&mut canvas);

        match self.frames.frames().get(self.frame_index) {
            Some(frame) => draw_validated_frame(&mut canvas, frame, bounds),
            None => draw_label(
                &mut canvas,
                Point::new(bounds.width / 2.0, bounds.height / 2.0),
                "Trusted frame unavailable",
                color::DANGER,
                14.0,
            ),
        }

        vec![canvas.into_geometry()]
    }
}

fn draw_validated_frame(canvas: &mut canvas::Frame, frame: &SimulationFrame, bounds: Rectangle) {
    let positions = layout_atoms(frame, bounds);
    draw_metallic_domains(canvas, frame, &positions);
    draw_ionic_associations(canvas, frame, &positions);
    draw_covalent_edges(canvas, frame, &positions);

    for atom in frame.atoms().values() {
        let Some(position) = positions.get(&atom.id).copied() else {
            continue;
        };
        let color = element_color(atom.element.as_str());
        canvas.fill(&Path::circle(position, 19.0), with_alpha(color, 0.22));
        canvas.fill(&Path::circle(position, 12.0), color);
        draw_label(canvas, position, atom.element.as_str(), Color::BLACK, 11.0);
        let charge = atom.electrons.formal_charge();
        if charge != 0 {
            draw_label(
                canvas,
                position + Vector::new(17.0, -17.0),
                &format_charge(charge),
                Color::WHITE,
                10.0,
            );
        }
    }
}

fn layout_atoms(frame: &SimulationFrame, bounds: Rectangle) -> BTreeMap<AtomId, Point> {
    let atoms = frame.atoms().keys().cloned().collect::<Vec<_>>();
    let columns = if atoms.len() <= 4 {
        atoms.len().max(1)
    } else {
        4
    };
    let rows = atoms.len().div_ceil(columns).max(1);
    let columns_f = f32::from(u16::try_from(columns).unwrap_or(u16::MAX));
    let rows_f = f32::from(u16::try_from(rows).unwrap_or(u16::MAX));
    let usable_width = (bounds.width - 100.0).max(120.0);
    let usable_height = (bounds.height - 90.0).max(100.0);

    atoms
        .into_iter()
        .enumerate()
        .map(|(index, id)| {
            let column = index % columns;
            let row = index / columns;
            let column = f32::from(u16::try_from(column).unwrap_or(u16::MAX));
            let row = f32::from(u16::try_from(row).unwrap_or(u16::MAX));
            let x = 50.0 + usable_width * (column + 0.5) / columns_f;
            let y = 45.0 + usable_height * (row + 0.5) / rows_f;
            (id, Point::new(x, y))
        })
        .collect()
}

fn draw_covalent_edges(
    canvas: &mut canvas::Frame,
    frame: &SimulationFrame,
    positions: &BTreeMap<AtomId, Point>,
) {
    for edge in frame.covalent_edges().values() {
        let (Some(left), Some(right)) = (positions.get(&edge.left), positions.get(&edge.right))
        else {
            continue;
        };
        let delta = Vector::new(right.x - left.x, right.y - left.y);
        let length = (delta.x * delta.x + delta.y * delta.y).sqrt().max(1.0);
        let perpendicular = Vector::new(-delta.y / length, delta.x / length);
        let count = edge.order.order();
        for index in 0..count {
            let offset = (f32::from(index) - (f32::from(count) - 1.0) / 2.0) * 5.0;
            canvas.stroke(
                &Path::line(
                    *left + perpendicular * offset,
                    *right + perpendicular * offset,
                ),
                Stroke::default().with_color(COVALENT).with_width(2.0),
            );
        }
        if let CovalentElectronOrigin::Dative { donor, acceptor } = &edge.electron_origin
            && let (Some(donor), Some(acceptor)) = (positions.get(donor), positions.get(acceptor))
        {
            let marker = Point::new(
                donor.x + (acceptor.x - donor.x) * 0.68,
                donor.y + (acceptor.y - donor.y) * 0.68,
            );
            canvas.fill(&Path::circle(marker, 4.0), METALLIC);
        }
    }
}

fn draw_ionic_associations(
    canvas: &mut canvas::Frame,
    frame: &SimulationFrame,
    positions: &BTreeMap<AtomId, Point>,
) {
    for association in frame.ionic_associations().values() {
        let centers = association
            .components
            .values()
            .filter_map(|atoms| component_center(atoms.iter(), positions))
            .collect::<Vec<_>>();
        for center in &centers {
            canvas.stroke(
                &Path::circle(*center, 29.0),
                Stroke::default().with_color(IONIC).with_width(2.0),
            );
        }
        if centers.len() >= 2 {
            canvas.stroke(
                &Path::line(centers[0], centers[1]),
                Stroke::default()
                    .with_color(with_alpha(IONIC, 0.58))
                    .with_width(1.5),
            );
        }
    }
}

fn draw_metallic_domains(
    canvas: &mut canvas::Frame,
    frame: &SimulationFrame,
    positions: &BTreeMap<AtomId, Point>,
) {
    for domain in frame.metallic_domains().values() {
        for site in &domain.sites {
            if let Some(position) = positions.get(site) {
                canvas.fill(&Path::circle(*position, 27.0), with_alpha(METALLIC, 0.12));
                canvas.stroke(
                    &Path::circle(*position, 27.0),
                    Stroke::default().with_color(METALLIC).with_width(1.5),
                );
            }
        }
    }
}

fn component_center<'a>(
    atoms: impl Iterator<Item = &'a AtomId>,
    positions: &BTreeMap<AtomId, Point>,
) -> Option<Point> {
    let points = atoms
        .filter_map(|atom| positions.get(atom).copied())
        .collect::<Vec<_>>();
    (!points.is_empty()).then(|| {
        let (x, y) = points
            .iter()
            .fold((0.0, 0.0), |(x, y), point| (x + point.x, y + point.y));
        let count = f32::from(u16::try_from(points.len()).unwrap_or(u16::MAX));
        Point::new(x / count, y / count)
    })
}

fn draw_grid(canvas: &mut canvas::Frame) {
    let size = canvas.size();
    let mut x = 0.0;
    while x < size.width {
        canvas.stroke(
            &Path::line(Point::new(x, 0.0), Point::new(x, size.height)),
            Stroke::default().with_color(GRID).with_width(1.0),
        );
        x += 32.0;
    }
    let mut y = 0.0;
    while y < size.height {
        canvas.stroke(
            &Path::line(Point::new(0.0, y), Point::new(size.width, y)),
            Stroke::default().with_color(GRID).with_width(1.0),
        );
        y += 32.0;
    }
}

fn draw_label(canvas: &mut canvas::Frame, position: Point, content: &str, color: Color, size: f32) {
    canvas.fill_text(canvas::Text {
        content: content.to_owned(),
        position,
        color,
        size: iced::Pixels(size),
        align_x: iced::alignment::Horizontal::Center.into(),
        align_y: alignment::Vertical::Center,
        ..canvas::Text::default()
    });
}

fn format_charge(charge: i16) -> String {
    match charge {
        1 => "+".to_owned(),
        -1 => "−".to_owned(),
        value if value > 0 => format!("{value}+"),
        value => format!("{}−", value.unsigned_abs()),
    }
}

const fn element_color(symbol: &str) -> Color {
    match symbol.as_bytes() {
        b"H" => LAB_DARK.chemistry.hydrogen,
        b"Li" => LAB_DARK.chemistry.lithium,
        b"O" => LAB_DARK.chemistry.oxygen,
        _ => LAB_DARK.chemistry.element_default,
    }
}

fn with_alpha(color: Color, alpha: f32) -> Color {
    Color::from_rgba(color.r, color.g, color.b, color.a * alpha.clamp(0.0, 1.0))
}
