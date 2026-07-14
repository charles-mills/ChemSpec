//! Educational 2D rendering of trusted structural frames.
//!
//! This module performs presentation layout only. It never parses source,
//! resolves catalogue rules, or infers a chemical relationship.

use std::collections::{BTreeMap, BTreeSet};

use chem_domain::StructuralOperationView;
use chem_kernel::SimulationFrame;
use chem_presentation::{ExplanationLabel, ExplanationLabelKind};
use iced::mouse::Cursor;
use iced::widget::canvas::{self, Path, Stroke};
use iced::{Color, Point, Rectangle, Renderer, Theme, Vector};

const ACCENT: Color = Color::from_rgb(0.56, 0.77, 1.0);
const IONIC: Color = Color::from_rgb(0.48, 0.89, 0.69);
const MUTED: Color = Color::from_rgb(0.40, 0.48, 0.56);

#[derive(Debug, Clone)]
struct RenderAtom {
    id: String,
    element: String,
    formal_charge: i16,
    non_bonding_electrons: u8,
    unpaired_electrons: u8,
}

#[derive(Debug, Clone)]
struct RenderBond {
    left: String,
    right: String,
    order: u8,
}

#[derive(Debug, Clone)]
struct RenderIonicAssociation {
    left: String,
    right: String,
}

#[derive(Debug, Clone)]
struct RenderMetallicDomain {
    sites: Vec<String>,
    delocalized_electrons: u16,
}

#[derive(Debug, Clone)]
struct RenderOperation {
    label: &'static str,
    atoms: Vec<String>,
    ionic: bool,
}

#[derive(Debug, Clone)]
struct RenderFrame {
    atoms: Vec<RenderAtom>,
    covalent_bonds: Vec<RenderBond>,
    ionic_associations: Vec<RenderIonicAssociation>,
    metallic_domains: Vec<RenderMetallicDomain>,
    active_operation: Option<RenderOperation>,
}

impl From<&SimulationFrame> for RenderFrame {
    fn from(frame: &SimulationFrame) -> Self {
        let atoms = frame
            .atoms()
            .values()
            .map(|atom| RenderAtom {
                id: atom.id.as_str().to_owned(),
                element: atom.element.as_str().to_owned(),
                formal_charge: atom.electrons.formal_charge(),
                non_bonding_electrons: atom.electrons.non_bonding_electrons(),
                unpaired_electrons: atom.electrons.unpaired_electrons(),
            })
            .collect();
        let covalent_bonds = frame
            .covalent_edges()
            .values()
            .map(|bond| RenderBond {
                left: bond.left.as_str().to_owned(),
                right: bond.right.as_str().to_owned(),
                order: bond.order.order(),
            })
            .collect();
        let ionic_associations = frame
            .ionic_associations()
            .values()
            .filter_map(|association| {
                let mut components = association.components.values();
                let left = components.next()?.iter().next()?;
                let right = components.next()?.iter().next()?;
                Some(RenderIonicAssociation {
                    left: left.as_str().to_owned(),
                    right: right.as_str().to_owned(),
                })
            })
            .collect();
        let metallic_domains = frame
            .metallic_domains()
            .values()
            .map(|domain| RenderMetallicDomain {
                sites: domain
                    .sites
                    .iter()
                    .map(|site| site.as_str().to_owned())
                    .collect(),
                delocalized_electrons: u16::try_from(domain.delocalized_electrons)
                    .unwrap_or(u16::MAX),
            })
            .collect();
        let active_operation = frame
            .active_operation()
            .map(|active| render_operation(active.operation.view(), frame));
        Self {
            atoms,
            covalent_bonds,
            ionic_associations,
            metallic_domains,
            active_operation,
        }
    }
}

fn render_operation(
    operation: StructuralOperationView<'_>,
    frame: &SimulationFrame,
) -> RenderOperation {
    let (label, atoms, ionic) = match operation {
        StructuralOperationView::CleaveCovalent { left, right, .. } => (
            "Covalent bond cleaved",
            vec![left.as_str().to_owned(), right.as_str().to_owned()],
            false,
        ),
        StructuralOperationView::FormCovalent { left, right, .. } => (
            "Shared electron pair",
            vec![left.as_str().to_owned(), right.as_str().to_owned()],
            false,
        ),
        StructuralOperationView::CleaveDative {
            donor, acceptor, ..
        } => (
            "Coordinate bond cleaved",
            vec![donor.as_str().to_owned(), acceptor.as_str().to_owned()],
            false,
        ),
        StructuralOperationView::FormDative {
            donor, acceptor, ..
        } => (
            "Coordinate bond formed",
            vec![donor.as_str().to_owned(), acceptor.as_str().to_owned()],
            false,
        ),
        StructuralOperationView::ChangeCovalent { left, right, .. } => (
            "Bond order changed",
            vec![left.as_str().to_owned(), right.as_str().to_owned()],
            false,
        ),
        StructuralOperationView::AssociateIonic { association } => (
            "Ionic association",
            association
                .components()
                .iter()
                .filter_map(|group| frame.groups().get(group))
                .flat_map(|group| group.atoms.iter())
                .map(|atom| atom.as_str().to_owned())
                .collect(),
            true,
        ),
        StructuralOperationView::DissociateIonic { .. } => ("Ionic dissociation", Vec::new(), true),
        StructuralOperationView::ReleaseMetallic { site, .. } => (
            "Metallic electron release",
            vec![site.as_str().to_owned()],
            false,
        ),
        StructuralOperationView::JoinMetallic { site, .. } => (
            "Metallic-domain join",
            vec![site.as_str().to_owned()],
            false,
        ),
        StructuralOperationView::TransferElectron {
            donor, acceptor, ..
        } => (
            "Electron transfer",
            vec![donor.as_str().to_owned(), acceptor.as_str().to_owned()],
            false,
        ),
        StructuralOperationView::AssignProduct { atoms, .. } => (
            "Validated product",
            atoms.iter().map(|atom| atom.as_str().to_owned()).collect(),
            false,
        ),
    };
    RenderOperation {
        label,
        atoms,
        ionic,
    }
}

#[derive(Debug, Clone)]
pub struct Diagram {
    before: RenderFrame,
    after: RenderFrame,
    progress: f32,
    explanation: Option<ExplanationLabel>,
    explanation_progress: f32,
    show_structure_labels: bool,
}

impl Diagram {
    pub fn new(
        before: &SimulationFrame,
        after: &SimulationFrame,
        progress: f32,
        explanation: Option<&ExplanationLabel>,
        explanation_progress: f32,
        show_structure_labels: bool,
    ) -> Self {
        Self {
            before: RenderFrame::from(before),
            after: RenderFrame::from(after),
            progress: progress.clamp(0.0, 1.0),
            explanation: explanation.cloned(),
            explanation_progress: explanation_progress.clamp(0.0, 1.0),
            show_structure_labels,
        }
    }
}

impl<Message> canvas::Program<Message> for Diagram {
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
        draw_grid(&mut canvas, bounds);
        let before_positions = layout(&self.before, bounds);
        let after_positions = layout(&self.after, bounds);
        let eased = ease_in_out(self.progress);
        let positions = interpolated_positions(&before_positions, &after_positions, eased);
        let visible_frame = if self.progress < 0.52 {
            &self.before
        } else {
            &self.after
        };
        let active = active_atoms(self.after.active_operation.as_ref());

        draw_metallic_domains(&mut canvas, visible_frame, &positions);

        for bond in &visible_frame.covalent_bonds {
            let (Some(left), Some(right)) = (positions.get(&bond.left), positions.get(&bond.right))
            else {
                continue;
            };
            draw_covalent(&mut canvas, *left, *right, bond.order);
        }
        for association in &visible_frame.ionic_associations {
            let (Some(left), Some(right)) = (
                positions.get(&association.left),
                positions.get(&association.right),
            ) else {
                continue;
            };
            draw_ionic(&mut canvas, *left, *right);
        }
        for atom in &visible_frame.atoms {
            if let Some(position) = positions.get(&atom.id) {
                draw_atom(
                    &mut canvas,
                    atom,
                    *position,
                    active.contains(atom.id.as_str()),
                );
            }
        }
        if self.show_structure_labels {
            draw_structure_labels(&mut canvas, visible_frame, &positions, bounds);
        }
        if let Some(explanation) = &self.explanation {
            draw_explanation_label(
                &mut canvas,
                explanation,
                &positions,
                bounds,
                self.explanation_progress,
            );
        }

        vec![canvas.into_geometry()]
    }
}

fn draw_structure_labels(
    frame: &mut canvas::Frame,
    state: &RenderFrame,
    positions: &BTreeMap<String, Point>,
    bounds: Rectangle,
) {
    if let Some(domain) = state.metallic_domains.first() {
        let target = average_position(domain.sites.iter().map(String::as_str), positions);
        if let Some(target) = target {
            let anchor = Point::new(bounds.width * 0.08, bounds.height * 0.16);
            draw_connector_label(frame, "Metallic structure", anchor, target, ACCENT);
            let electron_anchor = Point::new(bounds.width * 0.08, bounds.height * 0.23);
            draw_connector_label(
                frame,
                "Delocalised electrons",
                electron_anchor,
                target + Vector::new(0.0, -34.0),
                Color::WHITE,
            );
        }
    }

    let Some(operation) = state.active_operation.as_ref() else {
        return;
    };
    if let Some(target) = average_position(operation.atoms.iter().map(String::as_str), positions) {
        let anchor = Point::new(bounds.width * 0.74, bounds.height * 0.16);
        draw_connector_label(frame, operation.label, anchor, target, IONIC);
    }
}

fn draw_connector_label(
    frame: &mut canvas::Frame,
    label: &str,
    anchor: Point,
    target: Point,
    color: Color,
) {
    frame.fill_text(canvas::Text {
        content: label.to_owned(),
        position: anchor,
        color,
        size: iced::Pixels(14.0),
        ..canvas::Text::default()
    });
    let line_start = anchor + Vector::new(4.0, 20.0);
    let elbow = Point::new(line_start.x, target.y);
    frame.stroke(
        &Path::new(|path| {
            path.move_to(line_start);
            path.line_to(elbow);
            path.line_to(target);
        }),
        Stroke::default().with_color(color).with_width(1.2),
    );
    frame.fill(&Path::circle(target, 2.4), color);
}

#[allow(clippy::cast_precision_loss)]
fn average_position<'a>(
    atoms: impl IntoIterator<Item = &'a str>,
    positions: &BTreeMap<String, Point>,
) -> Option<Point> {
    let points = atoms
        .into_iter()
        .filter_map(|atom| positions.get(atom).copied())
        .collect::<Vec<_>>();
    if points.is_empty() {
        return None;
    }
    Some(Point::new(
        points.iter().map(|point| point.x).sum::<f32>() / points.len() as f32,
        points.iter().map(|point| point.y).sum::<f32>() / points.len() as f32,
    ))
}

#[allow(clippy::cast_precision_loss)]
fn draw_explanation_label(
    frame: &mut canvas::Frame,
    label: &ExplanationLabel,
    positions: &BTreeMap<String, Point>,
    bounds: Rectangle,
    progress: f32,
) {
    let target = average_position(label.target_atoms.iter().map(String::as_str), positions);
    let width = bounds.width.clamp(280.0, 440.0);
    let lines = wrap_words(&label.text, if width > 390.0 { 48 } else { 34 });
    let height = 58.0 + lines.len() as f32 * 20.0;
    let (x, y) = if let Some(target) = target {
        let x = if target.x < bounds.width * 0.52 {
            (target.x + 92.0).min(bounds.width - width - 28.0)
        } else {
            (target.x - width - 92.0).max(28.0)
        };
        let y = if target.y > bounds.height * 0.42 {
            34.0
        } else {
            bounds.height - height - 34.0
        };
        (x.max(28.0), y)
    } else {
        ((bounds.width - width) * 0.5, bounds.height * 0.62)
    };
    let rect = Rectangle::new(Point::new(x, y), iced::Size::new(width, height));
    // Leave a short settling beat before the outline starts tracing.
    let enter = ((progress - 0.07) / 0.16).clamp(0.0, 1.0);
    let exit = ((1.0 - progress) / 0.14).clamp(0.0, 1.0);
    let outline_alpha = exit;
    let text_alpha =
        ((progress - 0.22) / 0.12).clamp(0.0, 1.0) * ((0.86 - progress) / 0.12).clamp(0.0, 1.0);
    let accent = explanation_color(label.kind);

    frame.fill(
        &Path::rectangle(rect.position(), rect.size()),
        Color::from_rgba(0.035, 0.055, 0.075, 0.92 * text_alpha.max(enter * 0.55)),
    );
    draw_traced_outline(frame, rect, enter, accent.scale_alpha(outline_alpha));
    if label.connector
        && let Some(target) = target
    {
        let start = if target.x < rect.x {
            Point::new(rect.x, rect.y + rect.height * 0.5)
        } else {
            Point::new(rect.x + rect.width, rect.y + rect.height * 0.5)
        };
        let elbow = Point::new((start.x + target.x) * 0.5, start.y);
        frame.stroke(
            &Path::new(|path| {
                path.move_to(start);
                path.line_to(elbow);
                path.line_to(target);
            }),
            Stroke::default()
                .with_color(accent.scale_alpha(outline_alpha * enter))
                .with_width(1.2),
        );
        frame.fill(
            &Path::circle(target, 2.8),
            accent.scale_alpha(outline_alpha * enter),
        );
    }
    for (index, line) in lines.iter().enumerate() {
        frame.fill_text(canvas::Text {
            content: line.clone(),
            position: Point::new(rect.x + 22.0, rect.y + 28.0 + index as f32 * 20.0),
            color: Color::WHITE.scale_alpha(text_alpha),
            size: iced::Pixels(15.0),
            ..canvas::Text::default()
        });
    }
}

fn explanation_color(kind: ExplanationLabelKind) -> Color {
    match kind {
        ExplanationLabelKind::ObservationExplanation | ExplanationLabelKind::ImportantResult => {
            IONIC
        }
        ExplanationLabelKind::EquationExplanation => Color::from_rgb(0.90, 0.72, 0.40),
        ExplanationLabelKind::SummaryExplanation => Color::from_rgb(0.70, 0.88, 0.78),
        ExplanationLabelKind::ConceptExplanation
        | ExplanationLabelKind::StructuralChangeExplanation => ACCENT,
    }
}

fn draw_traced_outline(frame: &mut canvas::Frame, rect: Rectangle, trace: f32, color: Color) {
    let corners = [
        Point::new(rect.x, rect.y),
        Point::new(rect.x + rect.width, rect.y),
        Point::new(rect.x + rect.width, rect.y + rect.height),
        Point::new(rect.x, rect.y + rect.height),
        Point::new(rect.x, rect.y),
    ];
    let lengths = [rect.width, rect.height, rect.width, rect.height];
    let mut remaining = trace * lengths.iter().sum::<f32>();
    for index in 0..4 {
        if remaining <= 0.0 {
            break;
        }
        let segment = remaining.min(lengths[index]);
        let ratio = segment / lengths[index].max(1.0);
        let end = Point::new(
            corners[index].x + (corners[index + 1].x - corners[index].x) * ratio,
            corners[index].y + (corners[index + 1].y - corners[index].y) * ratio,
        );
        frame.stroke(
            &Path::line(corners[index], end),
            Stroke::default().with_color(color).with_width(1.6),
        );
        remaining -= segment;
    }
}

fn wrap_words(text: &str, max_chars: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.len() + word.len() + 1 > max_chars {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

fn ease_in_out(value: f32) -> f32 {
    value * value * (3.0 - 2.0 * value)
}

fn interpolated_positions(
    before: &BTreeMap<String, Point>,
    after: &BTreeMap<String, Point>,
    progress: f32,
) -> BTreeMap<String, Point> {
    after
        .iter()
        .map(|(id, end)| {
            let start = before.get(id).unwrap_or(end);
            (
                id.clone(),
                Point::new(
                    start.x + (end.x - start.x) * progress,
                    start.y + (end.y - start.y) * progress,
                ),
            )
        })
        .collect()
}

#[allow(clippy::cast_precision_loss)]
fn layout(frame: &RenderFrame, bounds: Rectangle) -> BTreeMap<String, Point> {
    let width = bounds.width.max(1.0);
    let height = bounds.height.max(1.0);
    let mut positions = BTreeMap::new();
    let point = |x: f32, y: f32| Point::new(width * x, height * y);
    let mut atom_ids = frame
        .atoms
        .iter()
        .map(|atom| atom.id.as_str())
        .collect::<Vec<_>>();
    atom_ids.sort_unstable();
    let columns = usize::from(u8::try_from(atom_ids.len().min(5)).unwrap_or(5)).max(1);
    for (index, atom_id) in atom_ids.iter().enumerate() {
        let column = index % columns;
        let row = index / columns;
        let x = 0.14 + (column as f32 + 0.5) * (0.72 / columns as f32);
        let rows = atom_ids.len().div_ceil(columns).max(1);
        let y = 0.14 + (row as f32 + 0.5) * (0.70 / rows as f32);
        positions.insert((*atom_id).to_owned(), point(x, y));
    }
    for bond in &frame.covalent_bonds {
        let (Some(left), Some(right)) = (
            positions.get(&bond.left).copied(),
            positions.get(&bond.right).copied(),
        ) else {
            continue;
        };
        let midpoint = Point::new((left.x + right.x) * 0.5, (left.y + right.y) * 0.5);
        let direction = right - left;
        let magnitude = (direction.x * direction.x + direction.y * direction.y)
            .sqrt()
            .max(1.0);
        let unit = Vector::new(direction.x / magnitude, direction.y / magnitude);
        positions.insert(bond.left.clone(), midpoint - unit * 46.0);
        positions.insert(bond.right.clone(), midpoint + unit * 46.0);
    }
    if let Some(operation) = &frame.active_operation
        && operation.ionic
        && let [left, right, ..] = operation.atoms.as_slice()
        && let (Some(left_position), Some(right_position)) =
            (positions.get(left).copied(), positions.get(right).copied())
    {
        let midpoint = Point::new(
            (left_position.x + right_position.x) * 0.5,
            (left_position.y + right_position.y) * 0.5,
        );
        positions.insert(left.clone(), midpoint + Vector::new(-42.0, 0.0));
        positions.insert(right.clone(), midpoint + Vector::new(42.0, 0.0));
    }
    positions
}

fn active_atoms(operation: Option<&RenderOperation>) -> BTreeSet<&str> {
    operation
        .into_iter()
        .flat_map(|operation| operation.atoms.iter().map(String::as_str))
        .collect()
}

#[allow(clippy::cast_precision_loss)]
fn draw_metallic_domains(
    canvas: &mut canvas::Frame,
    frame: &RenderFrame,
    positions: &BTreeMap<String, Point>,
) {
    for domain in &frame.metallic_domains {
        let sites = domain
            .sites
            .iter()
            .filter_map(|site| positions.get(site).copied())
            .collect::<Vec<_>>();
        if sites.is_empty() {
            continue;
        }
        let center = Point::new(
            sites.iter().map(|site| site.x).sum::<f32>() / sites.len() as f32,
            sites.iter().map(|site| site.y).sum::<f32>() / sites.len() as f32,
        );
        let radius = sites
            .iter()
            .map(|site| ((site.x - center.x).powi(2) + (site.y - center.y).powi(2)).sqrt())
            .fold(44.0_f32, f32::max)
            + 34.0;
        canvas.fill(
            &Path::circle(center, radius),
            Color::from_rgba(0.56, 0.77, 1.0, 0.08),
        );
        canvas.stroke(
            &Path::circle(center, radius),
            Stroke::default()
                .with_color(Color::from_rgba(0.56, 0.77, 1.0, 0.42))
                .with_width(1.5),
        );
        for electron in 0..domain.delocalized_electrons {
            let angle = std::f32::consts::TAU * f32::from(electron)
                / f32::from(domain.delocalized_electrons.max(1));
            canvas.fill(
                &Path::circle(
                    center + Vector::new(angle.cos() * radius * 0.68, angle.sin() * radius * 0.68),
                    3.0,
                ),
                Color::WHITE,
            );
        }
    }
}

fn draw_grid(frame: &mut canvas::Frame, bounds: Rectangle) {
    for column in 1_u8..10 {
        let x = bounds.width * f32::from(column) / 10.0;
        frame.stroke(
            &Path::line(Point::new(x, 0.0), Point::new(x, bounds.height)),
            Stroke::default()
                .with_color(Color::from_rgba(0.56, 0.77, 1.0, 0.035))
                .with_width(1.0),
        );
    }
}

fn draw_covalent(frame: &mut canvas::Frame, left: Point, right: Point, order: u8) {
    let direction = right - left;
    let magnitude = (direction.x * direction.x + direction.y * direction.y)
        .sqrt()
        .max(1.0);
    let perpendicular = Vector::new(-direction.y / magnitude, direction.x / magnitude);
    let offsets: &[f32] = match order {
        1 => &[0.0],
        2 => &[-3.0, 3.0],
        _ => &[-5.0, 0.0, 5.0],
    };
    for offset in offsets {
        frame.stroke(
            &Path::line(
                left + perpendicular * *offset,
                right + perpendicular * *offset,
            ),
            Stroke::default().with_color(ACCENT).with_width(2.0),
        );
        let midpoint = Point::new((left.x + right.x) * 0.5, (left.y + right.y) * 0.5)
            + perpendicular * *offset;
        let along = Vector::new(direction.x / magnitude, direction.y / magnitude);
        frame.fill(&Path::circle(midpoint - along * 3.0, 1.8), Color::WHITE);
        frame.fill(&Path::circle(midpoint + along * 3.0, 1.8), Color::WHITE);
    }
}

fn draw_ionic(frame: &mut canvas::Frame, left: Point, right: Point) {
    let delta = right - left;
    for step in 1_u8..10 {
        let t = f32::from(step) / 10.0;
        let position = left + delta * t;
        frame.fill(&Path::circle(position, 2.2), IONIC);
    }
}

fn draw_atom(frame: &mut canvas::Frame, atom: &RenderAtom, center: Point, active: bool) {
    let radius = if active { 27.0 } else { 23.0 };
    if active {
        frame.stroke(
            &Path::circle(center, radius + 8.0),
            Stroke::default()
                .with_color(Color::from_rgba(0.56, 0.77, 1.0, 0.65))
                .with_width(2.0),
        );
    }
    frame.fill(&Path::circle(center, radius), element_color(&atom.element));
    frame.fill_text(canvas::Text {
        content: atom.element.clone(),
        position: center,
        color: Color::from_rgb(0.04, 0.06, 0.08),
        size: iced::Pixels(15.0),
        align_x: iced::alignment::Horizontal::Center.into(),
        align_y: iced::alignment::Vertical::Center,
        ..canvas::Text::default()
    });
    let charge = match atom.formal_charge {
        0 => None,
        1 => Some("+".to_owned()),
        -1 => Some("−".to_owned()),
        value if value > 0 => Some(format!("{value}+")),
        value => Some(format!("{}−", value.unsigned_abs())),
    };
    if let Some(charge) = charge {
        let badge = center + Vector::new(radius * 0.72, -radius * 0.72);
        frame.fill(&Path::circle(badge, 8.0), Color::from_rgb(0.04, 0.06, 0.08));
        frame.fill_text(canvas::Text {
            content: charge,
            position: badge,
            color: Color::WHITE,
            size: iced::Pixels(12.0),
            align_x: iced::alignment::Horizontal::Center.into(),
            align_y: iced::alignment::Vertical::Center,
            ..canvas::Text::default()
        });
    }
    for electron in 0..atom.non_bonding_electrons.min(8) {
        let angle = std::f32::consts::TAU * f32::from(electron) / 8.0;
        let position =
            center + Vector::new(angle.cos() * (radius + 5.0), angle.sin() * (radius + 5.0));
        frame.fill(&Path::circle(position, 1.8), Color::WHITE);
    }
    if atom.unpaired_electrons > 0 {
        frame.stroke(
            &Path::circle(center, radius + 3.0),
            Stroke::default().with_color(MUTED).with_width(1.0),
        );
    }
}

fn element_color(symbol: &str) -> Color {
    match symbol {
        "Ag" => Color::from_rgb(0.78, 0.83, 0.88),
        "Cl" => Color::from_rgb(0.48, 0.89, 0.69),
        "Na" => Color::from_rgb(0.67, 0.54, 0.94),
        "N" => Color::from_rgb(0.45, 0.66, 0.96),
        "O" => Color::from_rgb(0.95, 0.40, 0.42),
        _ => Color::from_rgb(0.62, 0.68, 0.74),
    }
}
