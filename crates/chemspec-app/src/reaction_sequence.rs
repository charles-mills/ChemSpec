//! Deterministic Stage 5 2D reaction storyboard.
//!
//! This renders an illustrative preview from curated candidate data. It never
//! constructs or consumes `ValidatedExperiment` or `SimulationFrame`.

use iced::alignment;
use iced::mouse::Cursor;
use iced::widget::canvas::{self, Path, Stroke};
use iced::{Color, Point, Rectangle, Renderer, Theme, Vector};

use crate::elements;
use crate::reaction_candidate_catalogue::{Participant, ReactionCandidate};

const ACCENT: Color = Color::from_rgb(0.56, 0.77, 1.0);
const SHELL: Color = Color::from_rgb(0.35, 0.52, 0.70);
const GRID: Color = Color::from_rgba(0.56, 0.77, 1.0, 0.045);

#[derive(Debug, Clone, Copy)]
pub struct ReactionSequenceDiagram {
    candidate: ReactionCandidate,
    progress: f32,
}

impl ReactionSequenceDiagram {
    pub const fn new(candidate: ReactionCandidate, progress: f32) -> Self {
        Self {
            candidate,
            progress,
        }
    }
}

impl<Message> canvas::Program<Message> for ReactionSequenceDiagram {
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
        draw_grid(&mut frame);

        let progress = self.progress.clamp(0.0, 1.0);
        let reactant_alpha = if progress < 0.42 {
            1.0
        } else {
            ((0.64 - progress) / 0.22).clamp(0.0, 1.0)
        };
        let product_alpha = ((progress - 0.42) / 0.24).clamp(0.0, 1.0);
        let approach = smoothstep((progress / 0.50).clamp(0.0, 1.0));
        let emerge = smoothstep(((progress - 0.50) / 0.36).clamp(0.0, 1.0));
        let center = Point::new(bounds.width / 2.0, bounds.height / 2.0);

        draw_side(
            &mut frame,
            self.candidate.visual_reactants,
            bounds,
            center,
            approach,
            reactant_alpha,
            true,
        );
        draw_side(
            &mut frame,
            self.candidate.visual_products,
            bounds,
            center,
            emerge,
            product_alpha,
            false,
        );

        vec![frame.into_geometry()]
    }
}

pub fn stage_index(progress: f32, stage_count: usize) -> usize {
    if stage_count == 0 {
        return 0;
    }
    let progress = progress.clamp(0.0, 0.999_9);
    let scaled = progress * stage_count_as_f32(stage_count);
    floor_stage(scaled, stage_count)
}

fn stage_count_as_f32(stage_count: usize) -> f32 {
    match stage_count {
        1 => 1.0,
        2 => 2.0,
        3 => 3.0,
        _ => 4.0,
    }
}

fn floor_stage(scaled: f32, stage_count: usize) -> usize {
    if scaled < 1.0 {
        0
    } else if scaled < 2.0 {
        1.min(stage_count - 1)
    } else if scaled < 3.0 {
        2.min(stage_count - 1)
    } else {
        3.min(stage_count - 1)
    }
}

fn draw_side(
    frame: &mut canvas::Frame,
    participants: &[Participant],
    bounds: Rectangle,
    center: Point,
    movement: f32,
    alpha: f32,
    reactants: bool,
) {
    if alpha <= 0.0 || participants.is_empty() {
        return;
    }
    let start_y = bounds.height * if reactants { 0.34 } else { 0.66 };
    let start_span = (bounds.width * 0.64).clamp(180.0, 760.0);
    let spacing = participant_spacing(participants.len(), start_span);
    let first_x = center.x - spacing * participant_offset(participants.len());

    for (index, participant) in participants.iter().copied().enumerate() {
        let index_offset = index_offset(index);
        let start = Point::new(first_x + spacing * index_offset, start_y);
        let spread_target = Point::new(first_x + spacing * index_offset, center.y);
        let target = if reactants { center } else { spread_target };
        let position = if reactants {
            lerp_point(start, target, movement * 0.78)
        } else {
            lerp_point(center, target, movement)
        };
        draw_participant(frame, participant, position, alpha);
    }
}

fn participant_spacing(count: usize, span: f32) -> f32 {
    match count {
        0 | 1 => 0.0,
        2 => span * 0.42,
        3 => span * 0.30,
        _ => span * 0.22,
    }
}

fn participant_offset(count: usize) -> f32 {
    match count {
        2 => 0.5,
        3 => 1.0,
        4 => 1.5,
        _ => 0.0,
    }
}

fn index_offset(index: usize) -> f32 {
    match index {
        0 => 0.0,
        1 => 1.0,
        2 => 2.0,
        _ => 3.0,
    }
}

fn draw_participant(
    frame: &mut canvas::Frame,
    participant: Participant,
    center: Point,
    alpha: f32,
) {
    match participant {
        Participant::Atom(atomic_number) => {
            draw_atom(frame, atomic_number, center, 25.0, alpha);
        }
        Participant::Composition(formula) => {
            let atomic_numbers = arranged_atomic_numbers(formula);
            let offsets = atom_offsets(atomic_numbers.len());
            let positions = atomic_numbers
                .iter()
                .copied()
                .zip(offsets.iter().copied())
                .map(|(atomic_number, offset)| {
                    let position = center + offset;
                    draw_atom(frame, atomic_number, position, 18.0, alpha);
                    position
                })
                .collect::<Vec<_>>();
            draw_shared_electrons(frame, formula, &positions, alpha);
            draw_label(
                frame,
                Point::new(center.x, center.y + 37.0),
                formula,
                with_alpha(Color::WHITE, alpha),
                12.0,
            );
        }
    }
}

fn draw_atom(frame: &mut canvas::Frame, atomic_number: u8, center: Point, radius: f32, alpha: f32) {
    let Some(element) = elements::by_atomic_number(atomic_number) else {
        return;
    };
    let shell_count = element.period.max(1);
    for shell in 1..=shell_count {
        let shell_radius = radius * f32::from(shell) / f32::from(shell_count);
        frame.stroke(
            &Path::circle(center, shell_radius),
            Stroke::default()
                .with_color(with_alpha(SHELL, 0.72 * alpha))
                .with_width(1.0),
        );
    }
    let color = element_color(atomic_number);
    frame.fill(&Path::circle(center, 7.0), with_alpha(color, alpha));
    draw_label(
        frame,
        center,
        element.symbol,
        with_alpha(Color::BLACK, alpha),
        10.0,
    );
}

fn draw_shared_electrons(
    frame: &mut canvas::Frame,
    formula: &str,
    positions: &[Point],
    alpha: f32,
) {
    for &(start, end, pairs) in shared_pairs(formula) {
        let (Some(start), Some(end)) = (positions.get(start), positions.get(end)) else {
            continue;
        };
        let midpoint = Point::new(start.x.midpoint(end.x), start.y.midpoint(end.y));
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let magnitude = (dx * dx + dy * dy).sqrt().max(1.0);
        let along = Vector::new(dx / magnitude, dy / magnitude);
        let perpendicular = Vector::new(-along.y, along.x);
        for pair in 0..pairs {
            let offset = if pairs == 1 {
                0.0
            } else if pair == 0 {
                -4.0
            } else {
                4.0
            };
            let pair_center = midpoint + perpendicular * offset;
            for direction in [-1.0, 1.0] {
                frame.fill(
                    &Path::circle(pair_center + along * (direction * 2.3), 2.4),
                    with_alpha(ACCENT, alpha),
                );
            }
        }
    }
}

fn arranged_atomic_numbers(formula: &str) -> &'static [u8] {
    match formula {
        "H₂" => &[1, 1],
        "O₂" => &[8, 8],
        "H₂O" => &[1, 8, 1],
        "LiOH" => &[3, 8, 1],
        "NaCl" => &[11, 17],
        "CO₂" => &[8, 6, 8],
        _ => &[],
    }
}

fn atom_offsets(count: usize) -> Vec<Vector> {
    match count {
        1 => vec![Vector::new(0.0, 0.0)],
        2 => vec![Vector::new(-20.0, 0.0), Vector::new(20.0, 0.0)],
        _ => vec![
            Vector::new(-30.0, 7.0),
            Vector::new(0.0, -5.0),
            Vector::new(30.0, 7.0),
        ],
    }
}

fn shared_pairs(formula: &str) -> &'static [(usize, usize, u8)] {
    match formula {
        "H₂" => &[(0, 1, 1)],
        "O₂" => &[(0, 1, 2)],
        "H₂O" => &[(0, 1, 1), (1, 2, 1)],
        "LiOH" => &[(1, 2, 1)],
        "CO₂" => &[(0, 1, 2), (1, 2, 2)],
        _ => &[],
    }
}

fn draw_grid(frame: &mut canvas::Frame) {
    let size = frame.size();
    let stroke = Stroke::default().with_color(GRID).with_width(1.0);
    let mut x = 40.0;
    while x < size.width {
        frame.stroke(
            &Path::line(Point::new(x, 0.0), Point::new(x, size.height)),
            stroke,
        );
        x += 40.0;
    }
    let mut y = 40.0;
    while y < size.height {
        frame.stroke(
            &Path::line(Point::new(0.0, y), Point::new(size.width, y)),
            stroke,
        );
        y += 40.0;
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

fn lerp_point(start: Point, end: Point, progress: f32) -> Point {
    Point::new(
        start.x + (end.x - start.x) * progress,
        start.y + (end.y - start.y) * progress,
    )
}

fn smoothstep(value: f32) -> f32 {
    value * value * (3.0 - 2.0 * value)
}

fn with_alpha(color: Color, alpha: f32) -> Color {
    Color::from_rgba(color.r, color.g, color.b, color.a * alpha.clamp(0.0, 1.0))
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

    #[test]
    fn timeline_maps_progress_to_four_stages() {
        assert_eq!(stage_index(0.0, 4), 0);
        assert_eq!(stage_index(0.26, 4), 1);
        assert_eq!(stage_index(0.51, 4), 2);
        assert_eq!(stage_index(1.0, 4), 3);
    }

    #[test]
    fn multi_product_layout_keeps_every_product() {
        let candidate = crate::reaction_candidate_catalogue::SUPPORTED
            .iter()
            .find(|candidate| candidate.id == "lithium-water")
            .expect("lithium-water candidate");
        assert_eq!(candidate.visual_products.len(), 3);
        assert!(
            candidate
                .visual_products
                .contains(&Participant::Composition("LiOH"))
        );
        assert!(
            candidate
                .visual_products
                .contains(&Participant::Composition("H₂"))
        );
    }
}
