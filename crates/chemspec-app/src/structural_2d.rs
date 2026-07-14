//! Educational 2D rendering of trusted structural frames.
//!
//! This module performs deterministic presentation layout only. It never
//! parses source, resolves catalogue rules, or infers a chemical relationship.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use chem_catalogue::{AtomState, StructuralOperation};
use chem_engine::StructuralFrame;
use chem_presentation::{
    ContextLabel, EducationalPlan, EducationalSceneKind, ExplanationLabel, ExplanationLabelKind,
};
use iced::mouse::Cursor;
use iced::widget::canvas::{self, Path, Stroke};
use iced::{Color, Point, Rectangle, Renderer, Size, Theme, Vector, border};

const ACCENT: Color = Color::from_rgb(0.56, 0.77, 1.0);
const ACCENT_BRIGHT: Color = Color::from_rgb(0.72, 0.87, 1.0);
const IONIC: Color = Color::from_rgb(0.48, 0.89, 0.69);
const GOLD: Color = Color::from_rgb(0.95, 0.72, 0.35);
const CANVAS: Color = Color::from_rgb(0.027, 0.036, 0.048);
const PANEL: Color = Color::from_rgb(0.055, 0.076, 0.099);
const TEXT: Color = Color::from_rgb(0.94, 0.96, 0.98);
const TEXT_SOFT: Color = Color::from_rgb(0.70, 0.76, 0.82);

#[derive(Debug, Clone)]
pub struct SceneContext {
    kind: EducationalSceneKind,
    equation: Option<String>,
}

impl SceneContext {
    pub fn new(kind: EducationalSceneKind, _index: usize, _total: usize) -> Self {
        Self {
            kind,
            equation: None,
        }
    }

    pub fn with_equation(mut self, equation: Option<String>) -> Self {
        self.equation = equation;
        self
    }
}

#[derive(Debug, Clone)]
pub struct TimelineGuide {
    boundaries: Vec<f32>,
    current_scene: usize,
}

impl TimelineGuide {
    #[allow(clippy::cast_precision_loss)]
    pub fn new(plan: &EducationalPlan, current_scene: usize) -> Self {
        let total = plan.duration_ms().max(1) as f32;
        let mut elapsed = 0_u64;
        let mut boundaries = Vec::with_capacity(plan.scenes.len() + 1);
        boundaries.push(0.0);
        for scene in &plan.scenes {
            elapsed = elapsed.saturating_add(u64::from(scene.duration_ms));
            boundaries.push((elapsed as f32 / total).clamp(0.0, 1.0));
        }
        Self {
            boundaries,
            current_scene,
        }
    }
}

impl<Message> canvas::Program<Message> for TimelineGuide {
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
        let y = bounds.height * 0.5;
        let start = 9.0;
        let width = (bounds.width - start * 2.0).max(1.0);
        for (index, boundary) in self.boundaries.iter().enumerate().skip(1) {
            if index == self.boundaries.len().saturating_sub(1) {
                continue;
            }
            let x = start + width * boundary;
            let active = index == self.current_scene || index == self.current_scene + 1;
            frame.stroke(
                &Path::line(
                    Point::new(x, y - if active { 6.0 } else { 4.0 }),
                    Point::new(x, y + if active { 6.0 } else { 4.0 }),
                ),
                Stroke::default()
                    .with_color(if active {
                        ACCENT_BRIGHT.scale_alpha(0.62)
                    } else {
                        TEXT_SOFT.scale_alpha(0.28)
                    })
                    .with_width(if active { 1.5 } else { 1.0 }),
            );
        }
        vec![frame.into_geometry()]
    }
}

#[derive(Debug, Clone)]
pub struct Diagram {
    before: StructuralFrame,
    after: StructuralFrame,
    progress: f32,
    explanation: Option<ExplanationLabel>,
    context_labels: Vec<ContextLabel>,
    context: SceneContext,
    ambient_progress: f32,
    show_structure_labels: bool,
}

impl Diagram {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        before: &StructuralFrame,
        after: &StructuralFrame,
        progress: f32,
        explanation: Option<&ExplanationLabel>,
        context_labels: &[ContextLabel],
        context: SceneContext,
        ambient_progress: f32,
        show_structure_labels: bool,
    ) -> Self {
        Self {
            before: before.clone(),
            after: after.clone(),
            progress: progress.clamp(0.0, 1.0),
            explanation: explanation.cloned(),
            context_labels: context_labels.to_vec(),
            context,
            ambient_progress: ambient_progress.clamp(0.0, 1.0),
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
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let scale = visual_scale(bounds);
        draw_atmosphere(&mut frame, bounds, self.ambient_progress);

        let combined_learning_beat = self.context.kind == EducationalSceneKind::StructuralChange
            && self.explanation.is_some();
        let structural_progress = if combined_learning_beat {
            (self.progress / 0.55).clamp(0.0, 1.0)
        } else {
            self.progress
        };
        let explanation_progress = if combined_learning_beat {
            ((self.progress - 0.50) / 0.50).clamp(0.0, 1.0)
        } else {
            self.progress
        };
        let (before_positions, after_positions) =
            layout_transition(&self.before, &self.after, bounds);
        let action = animation_phase(structural_progress).action;
        let positions = interpolated_positions(&before_positions, &after_positions, action);
        let active = active_atoms(self.after.active_operation.as_ref());
        let content_alpha = if self.context.kind == EducationalSceneKind::Equation {
            0.34
        } else {
            1.0
        };

        draw_metallic_transition(
            &mut frame,
            &self.before,
            &self.after,
            &positions,
            self.after.active_operation.as_ref(),
            action,
            self.ambient_progress,
            content_alpha,
            scale,
        );
        draw_relationship_transition(
            &mut frame,
            &self.before,
            &self.after,
            &positions,
            action,
            content_alpha,
            scale,
        );
        draw_atom_transition(
            &mut frame,
            &self.before,
            &self.after,
            &positions,
            &active,
            self.after.active_operation.as_ref(),
            action,
            self.ambient_progress,
            content_alpha,
            self.context.kind == EducationalSceneKind::StructuralChange,
            scale,
        );
        if self.context.kind == EducationalSceneKind::StructuralChange {
            draw_operation_motion(
                &mut frame,
                &self.before,
                &self.after,
                self.after.active_operation.as_ref(),
                &positions,
                action,
                self.ambient_progress,
                scale,
            );
        }

        if self.show_structure_labels {
            draw_structure_labels(
                &mut frame,
                &self.context_labels,
                &positions,
                bounds,
                structural_progress,
                scale,
            );
        }
        draw_scene_context(&mut frame, &self.context, bounds, self.progress, scale);
        draw_observation_context(
            &mut frame,
            &self.context_labels,
            self.context.kind,
            bounds,
            self.progress,
            scale,
        );
        if let Some(explanation) = &self.explanation {
            draw_explanation_label(
                &mut frame,
                explanation,
                &positions,
                bounds,
                explanation_progress,
                scale,
            );
        }

        vec![frame.into_geometry()]
    }
}

#[derive(Debug, Clone, Copy)]
struct AnimationPhase {
    action: f32,
    context: f32,
}

fn animation_phase(progress: f32) -> AnimationPhase {
    AnimationPhase {
        action: smoother_step(((progress - 0.10) / 0.62).clamp(0.0, 1.0)),
        context: smoother_step(((progress - 0.42) / 0.16).clamp(0.0, 1.0))
            * smoother_step(((1.0 - progress) / 0.05).clamp(0.0, 1.0)),
    }
}

fn visual_scale(bounds: Rectangle) -> f32 {
    (bounds.width / 1_180.0)
        .min(bounds.height / 650.0)
        .clamp(0.72, 1.28)
}

fn smoother_step(value: f32) -> f32 {
    value * value * value * (value * (value * 6.0 - 15.0) + 10.0)
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
fn draw_atmosphere(frame: &mut canvas::Frame, bounds: Rectangle, phase: f32) {
    frame.fill_rectangle(Point::ORIGIN, bounds.size(), CANVAS);
    let center = Point::new(bounds.width * 0.52, bounds.height * 0.50);
    let pulse = 0.5 + 0.5 * (phase * std::f32::consts::TAU * 2.0).sin();
    for (index, radius) in [110.0_f32, 210.0, 340.0, 500.0].iter().enumerate() {
        frame.fill(
            &Path::circle(center, *radius),
            Color::from_rgba(
                0.11,
                0.25,
                0.38,
                (0.021 + pulse * 0.006) * (1.0 - index as f32 * 0.16),
            ),
        );
    }
    let spacing = 54.0;
    let columns = (bounds.width / spacing).ceil() as u16;
    let rows = (bounds.height / spacing).ceil() as u16;
    for row in 0..=rows {
        for column in 0..=columns {
            if (row + column) % 2 != 0 {
                continue;
            }
            let position = Point::new(
                f32::from(column) * spacing + 14.0,
                f32::from(row) * spacing + 12.0,
            );
            frame.fill(
                &Path::circle(position, 0.8),
                Color::from_rgba(0.56, 0.77, 1.0, 0.10),
            );
        }
    }
    frame.stroke(
        &Path::line(
            Point::new(24.0, bounds.height - 24.0),
            Point::new(82.0, bounds.height - 24.0),
        ),
        Stroke::default()
            .with_color(Color::from_rgba(0.56, 0.77, 1.0, 0.25))
            .with_width(1.0),
    );
}

#[allow(clippy::cast_precision_loss)]
fn layout(frame: &StructuralFrame, bounds: Rectangle) -> BTreeMap<String, Point> {
    let components = connected_components(frame);
    let mut positions = BTreeMap::new();
    for (component, (center, extent)) in components
        .iter()
        .zip(component_slots(components.len(), bounds))
    {
        layout_component(frame, component, center, extent, &mut positions);
    }
    positions
}

fn layout_transition(
    before: &StructuralFrame,
    after: &StructuralFrame,
    bounds: Rectangle,
) -> (BTreeMap<String, Point>, BTreeMap<String, Point>) {
    let before_components = connected_components(before);
    let after_components = connected_components(after);
    if before_components.is_empty() {
        return (BTreeMap::new(), layout(after, bounds));
    }
    if after_components.is_empty() {
        return (layout(before, bounds), BTreeMap::new());
    }

    let before_slots = component_slots(before_components.len(), bounds);
    let fallback_after_slots = component_slots(after_components.len(), bounds);
    let matches = after_components
        .iter()
        .map(|after_component| {
            before_components
                .iter()
                .enumerate()
                .map(|(index, before_component)| {
                    let overlap = after_component
                        .iter()
                        .filter(|atom| before_component.contains(atom))
                        .count();
                    (index, overlap)
                })
                .filter(|(_, overlap)| *overlap > 0)
                .max_by(|left, right| left.1.cmp(&right.1).then_with(|| right.0.cmp(&left.0)))
                .map(|(index, _)| index)
        })
        .collect::<Vec<_>>();

    let mut matched_groups = BTreeMap::<usize, Vec<usize>>::new();
    for (after_index, before_index) in matches.iter().enumerate() {
        if let Some(before_index) = before_index {
            matched_groups
                .entry(*before_index)
                .or_default()
                .push(after_index);
        }
    }

    let mut before_positions = BTreeMap::new();
    for (component, (center, extent)) in before_components.iter().zip(&before_slots) {
        layout_component(before, component, *center, *extent, &mut before_positions);
    }
    let mut after_positions = BTreeMap::new();
    for (after_index, component) in after_components.iter().enumerate() {
        let (center, extent) =
            matches[after_index].map_or(fallback_after_slots[after_index], |index| {
                let (base, extent) = before_slots[index];
                let group = &matched_groups[&index];
                let rank = group
                    .iter()
                    .position(|candidate| *candidate == after_index)
                    .unwrap_or(0);
                (base + split_offset(rank, group.len(), extent), extent)
            });
        layout_component(after, component, center, extent, &mut after_positions);
    }
    (before_positions, after_positions)
}

#[allow(clippy::cast_precision_loss)]
fn component_slots(component_count: usize, bounds: Rectangle) -> Vec<(Point, f32)> {
    if component_count == 0 {
        return Vec::new();
    }
    let compact = bounds.width < 720.0;
    let columns = if compact {
        component_count.min(2)
    } else {
        component_count.min(3)
    }
    .max(1);
    let rows = component_count.div_ceil(columns).max(1);
    let safe_left = if compact { 74.0 } else { 120.0 };
    let safe_right = safe_left;
    let safe_top = if compact { 118.0 } else { 132.0 };
    let safe_bottom = if compact { 100.0 } else { 112.0 };
    let available_width = (bounds.width - safe_left - safe_right).max(160.0);
    let available_height = (bounds.height - safe_top - safe_bottom).max(160.0);
    let cell_width = available_width / columns as f32;
    let cell_height = available_height / rows as f32;

    (0..component_count)
        .map(|index| {
            let column = index % columns;
            let row = index / columns;
            let center = Point::new(
                safe_left + cell_width * (column as f32 + 0.5),
                safe_top + cell_height * (row as f32 + 0.5),
            );
            (center, cell_width.min(cell_height))
        })
        .collect()
}

#[allow(clippy::cast_precision_loss)]
fn split_offset(rank: usize, count: usize, extent: f32) -> Vector {
    if count <= 1 {
        return Vector::new(0.0, 0.0);
    }
    let radius = (extent * 0.18).clamp(42.0, 66.0);
    if count == 2 {
        return Vector::new(if rank == 0 { -radius } else { radius }, 0.0);
    }
    let angle = -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * rank as f32 / count as f32;
    Vector::new(angle.cos() * radius, angle.sin() * radius)
}

fn connected_components(frame: &StructuralFrame) -> Vec<Vec<String>> {
    let mut adjacency = frame
        .atoms
        .iter()
        .map(|atom| (atom.id.clone(), BTreeSet::new()))
        .collect::<BTreeMap<_, _>>();
    let mut link = |left: &str, right: &str| {
        if let Some(neighbours) = adjacency.get_mut(left) {
            neighbours.insert(right.to_owned());
        }
        if let Some(neighbours) = adjacency.get_mut(right) {
            neighbours.insert(left.to_owned());
        }
    };
    for bond in &frame.covalent_bonds {
        link(&bond.left, &bond.right);
    }
    for association in &frame.ionic_associations {
        link(&association.left, &association.right);
    }
    for domain in &frame.metallic_domains {
        if let Some(first) = domain.sites.first() {
            for site in domain.sites.iter().skip(1) {
                link(first, site);
            }
        }
    }

    let mut seen = BTreeSet::new();
    let mut components = Vec::new();
    for start in adjacency.keys() {
        if seen.contains(start) {
            continue;
        }
        let mut queue = VecDeque::from([start.clone()]);
        let mut component = Vec::new();
        seen.insert(start.clone());
        while let Some(atom) = queue.pop_front() {
            component.push(atom.clone());
            if let Some(neighbours) = adjacency.get(&atom) {
                for neighbour in neighbours {
                    if seen.insert(neighbour.clone()) {
                        queue.push_back(neighbour.clone());
                    }
                }
            }
        }
        component.sort();
        components.push(component);
    }
    components.sort_by(|left, right| left.first().cmp(&right.first()));
    components
}

#[allow(clippy::cast_precision_loss)]
fn layout_component(
    frame: &StructuralFrame,
    component: &[String],
    center: Point,
    cell_extent: f32,
    positions: &mut BTreeMap<String, Point>,
) {
    if component.len() == 1 {
        positions.insert(component[0].clone(), center);
        return;
    }
    let spacing = (cell_extent * 0.24).clamp(52.0, 82.0);
    if component.len() == 2 {
        positions.insert(
            component[0].clone(),
            center + Vector::new(-spacing * 0.5, 0.0),
        );
        positions.insert(
            component[1].clone(),
            center + Vector::new(spacing * 0.5, 0.0),
        );
        return;
    }

    let adjacency = component_adjacency(frame, component);
    let root = component
        .iter()
        .max_by(|left, right| {
            adjacency
                .get(*left)
                .map_or(0, BTreeSet::len)
                .cmp(&adjacency.get(*right).map_or(0, BTreeSet::len))
                .then_with(|| right.cmp(left))
        })
        .expect("non-empty component");
    positions.insert(root.clone(), center);
    let mut seen = BTreeSet::from([root.clone()]);
    let mut queue = VecDeque::from([(root.clone(), 0_usize, -std::f32::consts::FRAC_PI_2)]);
    while let Some((parent, depth, parent_angle)) = queue.pop_front() {
        let children = adjacency
            .get(&parent)
            .into_iter()
            .flatten()
            .filter(|neighbour| !seen.contains(*neighbour))
            .cloned()
            .collect::<Vec<_>>();
        let Some(parent_position) = positions.get(&parent).copied() else {
            continue;
        };
        for (index, child) in children.iter().enumerate() {
            seen.insert(child.clone());
            let count = children.len();
            let angle = if depth == 0 {
                root_child_angle(index, count)
            } else {
                parent_angle - 0.62 + 1.24 * (index as f32 + 0.5) / count as f32
            };
            let distance = spacing * if depth == 0 { 1.0 } else { 0.86 };
            positions.insert(
                child.clone(),
                parent_position + Vector::new(angle.cos() * distance, angle.sin() * distance),
            );
            queue.push_back((child.clone(), depth + 1, angle));
        }
    }
}

#[allow(clippy::cast_precision_loss)]
fn root_child_angle(index: usize, count: usize) -> f32 {
    match count {
        1 => 0.0,
        2 => [-2.42, -0.72][index],
        3 => -std::f32::consts::FRAC_PI_2 + (index as f32 - 1.0) * std::f32::consts::TAU / 3.0,
        _ => -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * index as f32 / count as f32,
    }
}

fn component_adjacency(
    frame: &StructuralFrame,
    component: &[String],
) -> BTreeMap<String, BTreeSet<String>> {
    let members = component.iter().cloned().collect::<BTreeSet<_>>();
    let mut adjacency = component
        .iter()
        .map(|atom| (atom.clone(), BTreeSet::new()))
        .collect::<BTreeMap<_, _>>();
    let mut link = |left: &str, right: &str| {
        if !members.contains(left) || !members.contains(right) {
            return;
        }
        if let Some(neighbours) = adjacency.get_mut(left) {
            neighbours.insert(right.to_owned());
        }
        if let Some(neighbours) = adjacency.get_mut(right) {
            neighbours.insert(left.to_owned());
        }
    };
    for bond in &frame.covalent_bonds {
        link(&bond.left, &bond.right);
    }
    for association in &frame.ionic_associations {
        link(&association.left, &association.right);
    }
    for domain in &frame.metallic_domains {
        if let Some(first) = domain.sites.first() {
            for site in domain.sites.iter().skip(1) {
                link(first, site);
            }
        }
    }
    adjacency
}

fn interpolated_positions(
    before: &BTreeMap<String, Point>,
    after: &BTreeMap<String, Point>,
    progress: f32,
) -> BTreeMap<String, Point> {
    before
        .keys()
        .chain(after.keys())
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter_map(|id| {
            let start = before.get(&id).or_else(|| after.get(&id))?;
            let end = after.get(&id).or_else(|| before.get(&id))?;
            Some((id, lerp_point(*start, *end, progress)))
        })
        .collect()
}

fn lerp_point(start: Point, end: Point, progress: f32) -> Point {
    Point::new(
        start.x + (end.x - start.x) * progress,
        start.y + (end.y - start.y) * progress,
    )
}

fn draw_relationship_transition(
    frame: &mut canvas::Frame,
    before: &StructuralFrame,
    after: &StructuralFrame,
    positions: &BTreeMap<String, Point>,
    progress: f32,
    opacity: f32,
    scale: f32,
) {
    let before_bonds = covalent_map(before);
    let after_bonds = covalent_map(after);
    for key in before_bonds
        .keys()
        .chain(after_bonds.keys())
        .cloned()
        .collect::<BTreeSet<_>>()
    {
        let bond = after_bonds.get(&key).or_else(|| before_bonds.get(&key));
        let Some((left_id, right_id, order)) = bond else {
            continue;
        };
        let (Some(left), Some(right)) = (positions.get(left_id), positions.get(right_id)) else {
            continue;
        };
        let in_before = before_bonds.contains_key(&key);
        let in_after = after_bonds.contains_key(&key);
        let (reveal, alpha) = match (in_before, in_after) {
            (true, true) => (1.0, 1.0),
            (false, true) => (progress, progress),
            (true, false) => (1.0 - progress, 1.0 - progress),
            (false, false) => continue,
        };
        draw_covalent(
            frame,
            *left,
            *right,
            *order,
            reveal,
            alpha * opacity,
            in_before == in_after,
            scale,
        );
    }

    let before_ionic = ionic_map(before);
    let after_ionic = ionic_map(after);
    for key in before_ionic
        .keys()
        .chain(after_ionic.keys())
        .cloned()
        .collect::<BTreeSet<_>>()
    {
        let pair = after_ionic.get(&key).or_else(|| before_ionic.get(&key));
        let Some((left_id, right_id)) = pair else {
            continue;
        };
        let (Some(left), Some(right)) = (positions.get(left_id), positions.get(right_id)) else {
            continue;
        };
        let in_before = before_ionic.contains_key(&key);
        let in_after = after_ionic.contains_key(&key);
        let reveal = match (in_before, in_after) {
            (true, true) => 1.0,
            (false, true) => progress,
            (true, false) => 1.0 - progress,
            (false, false) => 0.0,
        };
        draw_ionic(frame, *left, *right, reveal, opacity, scale);
    }
}

type CovalentMap = BTreeMap<String, (String, String, u8)>;

fn covalent_map(frame: &StructuralFrame) -> CovalentMap {
    frame
        .covalent_bonds
        .iter()
        .map(|bond| {
            let (left, right) = sorted_pair(&bond.left, &bond.right);
            (
                format!("{left}|{right}|{}", bond.order),
                (left, right, bond.order),
            )
        })
        .collect()
}

fn ionic_map(frame: &StructuralFrame) -> BTreeMap<String, (String, String)> {
    frame
        .ionic_associations
        .iter()
        .map(|association| {
            let pair = sorted_pair(&association.left, &association.right);
            (format!("{}|{}", pair.0, pair.1), pair)
        })
        .collect()
}

fn sorted_pair(left: &str, right: &str) -> (String, String) {
    if left <= right {
        (left.to_owned(), right.to_owned())
    } else {
        (right.to_owned(), left.to_owned())
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_covalent(
    frame: &mut canvas::Frame,
    left: Point,
    right: Point,
    order: u8,
    reveal: f32,
    alpha: f32,
    show_electrons: bool,
    scale: f32,
) {
    let direction = right - left;
    let magnitude = vector_magnitude(direction).max(1.0);
    let along = direction / magnitude;
    let perpendicular = Vector::new(-along.y, along.x);
    let atom_clearance = 25.0 * scale;
    let start = left + along * atom_clearance;
    let end = right - along * atom_clearance;
    let midpoint = lerp_point(start, end, 0.5);
    let offsets: &[f32] = match order {
        1 => &[0.0],
        2 => &[-4.0, 4.0],
        _ => &[-6.0, 0.0, 6.0],
    };
    for offset in offsets {
        let visible_start = lerp_point(midpoint, start, reveal);
        let visible_end = lerp_point(midpoint, end, reveal);
        frame.stroke(
            &Path::line(
                visible_start + perpendicular * *offset * scale,
                visible_end + perpendicular * *offset * scale,
            ),
            Stroke::default()
                .with_color(ACCENT.scale_alpha(alpha * 0.88))
                .with_width(2.4 * scale),
        );
        if show_electrons {
            let electron_center = midpoint + perpendicular * *offset * scale;
            frame.fill(
                &Path::circle(electron_center - along * 4.0 * scale, 2.3 * scale),
                Color::WHITE.scale_alpha(alpha * reveal),
            );
            frame.fill(
                &Path::circle(electron_center + along * 4.0 * scale, 2.3 * scale),
                Color::WHITE.scale_alpha(alpha * reveal),
            );
        }
    }
}

fn draw_ionic(
    frame: &mut canvas::Frame,
    left: Point,
    right: Point,
    reveal: f32,
    opacity: f32,
    scale: f32,
) {
    let delta = right - left;
    for step in 1_u8..12 {
        let t = f32::from(step) / 12.0;
        if t > reveal {
            continue;
        }
        frame.fill(
            &Path::circle(left + delta * t, 1.8 * scale),
            IONIC.scale_alpha(opacity * (0.48 + t * 0.38)),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_atom_transition(
    frame: &mut canvas::Frame,
    before: &StructuralFrame,
    after: &StructuralFrame,
    positions: &BTreeMap<String, Point>,
    active: &BTreeSet<&str>,
    operation: Option<&StructuralOperation>,
    progress: f32,
    ambient_progress: f32,
    opacity: f32,
    focus_active: bool,
    scale: f32,
) {
    let before_atoms = before
        .atoms
        .iter()
        .map(|atom| (atom.id.as_str(), atom))
        .collect::<BTreeMap<_, _>>();
    let after_atoms = after
        .atoms
        .iter()
        .map(|atom| (atom.id.as_str(), atom))
        .collect::<BTreeMap<_, _>>();
    for id in before_atoms
        .keys()
        .chain(after_atoms.keys())
        .copied()
        .collect::<BTreeSet<_>>()
    {
        let Some(position) = positions.get(id).copied() else {
            continue;
        };
        let before_atom = before_atoms.get(id).copied();
        let after_atom = after_atoms.get(id).copied();
        let focus_opacity = if focus_active && !active.is_empty() && !active.contains(id) {
            0.60
        } else {
            1.0
        };
        let alpha = match (before_atom.is_some(), after_atom.is_some()) {
            (true, true) => 1.0,
            (true, false) => 1.0 - progress,
            (false, true) => progress,
            (false, false) => 0.0,
        } * opacity
            * focus_opacity;
        let atom = after_atom
            .or(before_atom)
            .expect("atom exists in frame union");
        draw_atom_shell(
            frame,
            atom,
            position,
            active.contains(id),
            ambient_progress,
            alpha,
            scale,
        );
        draw_electron_transition(
            frame,
            before_atom,
            after_atom,
            operation,
            id,
            position,
            progress,
            ambient_progress,
            alpha,
            scale,
        );
        draw_charge_transition(
            frame,
            before_atom,
            after_atom,
            position,
            progress,
            alpha,
            scale,
        );
    }
}

fn draw_atom_shell(
    frame: &mut canvas::Frame,
    atom: &AtomState,
    center: Point,
    active: bool,
    phase: f32,
    alpha: f32,
    scale: f32,
) {
    let radius = (if active { 27.0 } else { 24.0 }) * scale;
    let pulse = 0.5 + 0.5 * (phase * std::f32::consts::TAU * 3.0).sin();
    frame.fill(
        &Path::circle(center, radius + (10.0 + pulse * 4.0) * scale),
        element_color(&atom.element).scale_alpha(alpha * if active { 0.13 } else { 0.05 }),
    );
    if active {
        frame.stroke(
            &Path::circle(center, radius + 7.0 * scale),
            Stroke::default()
                .with_color(ACCENT_BRIGHT.scale_alpha(alpha * (0.42 + pulse * 0.28)))
                .with_width((1.4 + pulse * 0.8) * scale),
        );
    }
    frame.fill(
        &Path::circle(center + Vector::new(0.0, 3.0 * scale), radius + 2.0 * scale),
        Color::from_rgba(0.0, 0.0, 0.0, alpha * 0.24),
    );
    frame.fill(
        &Path::circle(center, radius),
        element_color(&atom.element).scale_alpha(alpha),
    );
    frame.stroke(
        &Path::circle(center, radius),
        Stroke::default()
            .with_color(Color::WHITE.scale_alpha(alpha * 0.26))
            .with_width(scale),
    );
    frame.fill_text(canvas::Text {
        content: atom.element.clone(),
        position: center,
        color: Color::from_rgb(0.025, 0.040, 0.055).scale_alpha(alpha),
        size: iced::Pixels(15.0 * scale),
        align_x: iced::alignment::Horizontal::Center.into(),
        align_y: iced::alignment::Vertical::Center,
        ..canvas::Text::default()
    });
}

#[allow(clippy::too_many_arguments)]
fn draw_electron_transition(
    frame: &mut canvas::Frame,
    before: Option<&AtomState>,
    after: Option<&AtomState>,
    operation: Option<&StructuralOperation>,
    atom_id: &str,
    center: Point,
    progress: f32,
    phase: f32,
    alpha: f32,
    scale: f32,
) {
    let delta = electron_state_delta(before, after);
    if operation_moves_atom_electrons(operation, atom_id) {
        let before_positions = before.map_or_else(Vec::new, |atom| {
            electron_positions(center, atom, phase, scale)
        });
        let after_positions = after.map_or_else(Vec::new, |atom| {
            electron_positions(center, atom, phase, scale)
        });
        for index in 0..usize::from(delta.persistent) {
            let position = match (before_positions.get(index), after_positions.get(index)) {
                (Some(start), Some(end)) => lerp_point(*start, *end, progress),
                (Some(position), None) | (None, Some(position)) => *position,
                (None, None) => continue,
            };
            draw_electron_dot(frame, position, alpha, scale);
        }
    } else if before == after {
        if let Some(atom) = after.or(before) {
            draw_electrons(frame, center, atom, phase, alpha, scale);
        }
    } else {
        if let Some(atom) = before {
            draw_electrons(frame, center, atom, phase, alpha * (1.0 - progress), scale);
        }
        if let Some(atom) = after {
            draw_electrons(frame, center, atom, phase, alpha * progress, scale);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ElectronStateDelta {
    persistent: u8,
    leaves_shell: u8,
    enters_shell: u8,
}

fn electron_state_delta(
    before: Option<&AtomState>,
    after: Option<&AtomState>,
) -> ElectronStateDelta {
    let before_count = before.map_or(0, |atom| atom.non_bonding_electrons);
    let after_count = after.map_or(0, |atom| atom.non_bonding_electrons);
    let persistent = before_count.min(after_count);
    ElectronStateDelta {
        persistent,
        leaves_shell: before_count - persistent,
        enters_shell: after_count - persistent,
    }
}

fn operation_moves_atom_electrons(operation: Option<&StructuralOperation>, atom_id: &str) -> bool {
    match operation {
        Some(StructuralOperation::TransferMetallicElectron { acceptor, .. }) => acceptor == atom_id,
        Some(
            StructuralOperation::CleaveCovalent { left, right, .. }
            | StructuralOperation::FormCovalent { left, right, .. },
        ) => left == atom_id || right == atom_id,
        Some(
            StructuralOperation::AssociateIonic { .. } | StructuralOperation::AssignProduct { .. },
        )
        | None => false,
    }
}

fn draw_electrons(
    frame: &mut canvas::Frame,
    center: Point,
    atom: &AtomState,
    phase: f32,
    alpha: f32,
    scale: f32,
) {
    for position in electron_positions(center, atom, phase, scale) {
        draw_electron_dot(frame, position, alpha, scale);
    }
}

fn electron_positions(center: Point, atom: &AtomState, phase: f32, scale: f32) -> Vec<Point> {
    let radius = 31.0 * scale;
    let drift = (phase * std::f32::consts::TAU * 0.45).sin() * 0.055;
    let mut positions = Vec::with_capacity(usize::from(atom.non_bonding_electrons.min(8)));
    for (domain, occupancy) in
        electron_domain_occupancies(atom.non_bonding_electrons, atom.unpaired_electrons)
            .into_iter()
            .enumerate()
    {
        let domain = u8::try_from(domain).unwrap_or(u8::MAX);
        let base_angle = std::f32::consts::FRAC_PI_2 * f32::from(domain) + drift;
        for electron in 0..occupancy {
            let offset = match occupancy {
                1 => 0.0,
                _ if electron == 0 => -0.07,
                _ => 0.07,
            };
            let angle = base_angle + offset;
            positions.push(center + Vector::new(angle.cos() * radius, angle.sin() * radius));
        }
    }
    positions
}

fn electron_domain_occupancies(count: u8, unpaired: u8) -> Vec<u8> {
    let count = count.min(8);
    let unpaired = unpaired.min(count).min(4);
    let paired_electrons = count - unpaired;
    if !paired_electrons.is_multiple_of(2) || paired_electrons / 2 + unpaired > 4 {
        let mut fallback = vec![0_u8; 4];
        for electron in 0..count {
            let domain = usize::from(if electron < 4 { electron } else { electron - 4 });
            fallback[domain] += 1;
        }
        fallback.retain(|occupancy| *occupancy > 0);
        return fallback;
    }

    let mut occupancies = vec![2; usize::from(paired_electrons / 2)];
    occupancies.extend(std::iter::repeat_n(1, usize::from(unpaired)));
    occupancies
}

fn draw_electron_dot(frame: &mut canvas::Frame, position: Point, alpha: f32, scale: f32) {
    frame.fill(
        &Path::circle(position, 2.0 * scale),
        Color::WHITE.scale_alpha(alpha * 0.92),
    );
}

fn draw_charge_transition(
    frame: &mut canvas::Frame,
    before: Option<&AtomState>,
    after: Option<&AtomState>,
    center: Point,
    progress: f32,
    alpha: f32,
    scale: f32,
) {
    let before_charge = before.map_or(0, |atom| atom.formal_charge);
    let after_charge = after.map_or(0, |atom| atom.formal_charge);
    if before_charge == after_charge {
        draw_charge(frame, center, after_charge, alpha, scale);
    } else {
        draw_charge(
            frame,
            center,
            before_charge,
            alpha * (1.0 - progress),
            scale,
        );
        draw_charge(frame, center, after_charge, alpha * progress, scale);
    }
}

fn draw_charge(frame: &mut canvas::Frame, center: Point, charge: i8, alpha: f32, scale: f32) {
    let Some(label) = charge_label(charge) else {
        return;
    };
    let badge = center + Vector::new(19.0 * scale, -19.0 * scale);
    frame.fill(
        &Path::circle(badge, 9.0 * scale),
        Color::from_rgb(0.025, 0.040, 0.055).scale_alpha(alpha),
    );
    frame.stroke(
        &Path::circle(badge, 9.0 * scale),
        Stroke::default()
            .with_color(Color::WHITE.scale_alpha(alpha * 0.18))
            .with_width(scale),
    );
    frame.fill_text(canvas::Text {
        content: label,
        position: badge,
        color: TEXT.scale_alpha(alpha),
        size: iced::Pixels(11.0 * scale),
        align_x: iced::alignment::Horizontal::Center.into(),
        align_y: iced::alignment::Vertical::Center,
        ..canvas::Text::default()
    });
}

fn charge_label(charge: i8) -> Option<String> {
    match charge {
        0 => None,
        1 => Some("+".to_owned()),
        -1 => Some("−".to_owned()),
        value if value > 0 => Some(format!("{value}+")),
        value => Some(format!("{}−", value.unsigned_abs())),
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_metallic_transition(
    frame: &mut canvas::Frame,
    before: &StructuralFrame,
    after: &StructuralFrame,
    positions: &BTreeMap<String, Point>,
    operation: Option<&StructuralOperation>,
    progress: f32,
    phase: f32,
    opacity: f32,
    scale: f32,
) {
    let before_domains = before
        .metallic_domains
        .iter()
        .map(|domain| (domain.id.as_str(), domain))
        .collect::<BTreeMap<_, _>>();
    let after_domains = after
        .metallic_domains
        .iter()
        .map(|domain| (domain.id.as_str(), domain))
        .collect::<BTreeMap<_, _>>();
    for id in before_domains
        .keys()
        .chain(after_domains.keys())
        .copied()
        .collect::<BTreeSet<_>>()
    {
        let before_domain = before_domains.get(id).copied();
        let after_domain = after_domains.get(id).copied();
        let domain = after_domain.or(before_domain).expect("domain exists");
        let alpha = match (before_domain.is_some(), after_domain.is_some()) {
            (true, true) => 1.0,
            (true, false) => 1.0 - progress,
            (false, true) => progress,
            (false, false) => 0.0,
        } * opacity;
        let sites = domain
            .sites
            .iter()
            .filter_map(|site| positions.get(site).copied())
            .collect::<Vec<_>>();
        if sites.is_empty() {
            continue;
        }
        let min_x = sites
            .iter()
            .map(|site| site.x)
            .fold(f32::INFINITY, f32::min);
        let max_x = sites
            .iter()
            .map(|site| site.x)
            .fold(f32::NEG_INFINITY, f32::max);
        let min_y = sites
            .iter()
            .map(|site| site.y)
            .fold(f32::INFINITY, f32::min);
        let max_y = sites
            .iter()
            .map(|site| site.y)
            .fold(f32::NEG_INFINITY, f32::max);
        let padding = 52.0 * scale;
        let rect = Rectangle::new(
            Point::new(min_x - padding, min_y - padding),
            Size::new(max_x - min_x + padding * 2.0, max_y - min_y + padding * 2.0),
        );
        let path = rounded_rectangle(rect, 28.0 * scale);
        frame.fill(&path, ACCENT.scale_alpha(alpha * 0.055));
        frame.stroke(
            &path,
            Stroke::default()
                .with_color(ACCENT.scale_alpha(alpha * 0.34))
                .with_width(1.2 * scale),
        );
        let perimeter = rect.width * 2.0 + rect.height * 2.0;
        let active_transfer = matches!(
            operation,
            Some(StructuralOperation::TransferMetallicElectron { domain, .. }) if domain == id
        );
        let stationary_electrons = if active_transfer {
            after_domain.map_or(0, |domain| domain.delocalized_electrons)
        } else {
            domain.delocalized_electrons
        };
        for electron in 0..stationary_electrons {
            let offset = (phase * 0.16
                + f32::from(electron) / f32::from(stationary_electrons.max(1)))
            .fract();
            let electron_position = point_on_rect_perimeter(rect, offset * perimeter);
            frame.fill(
                &Path::circle(electron_position, 3.0 * scale),
                Color::WHITE.scale_alpha(alpha * 0.94),
            );
            frame.fill(
                &Path::circle(electron_position, 7.0 * scale),
                ACCENT.scale_alpha(alpha * 0.09),
            );
        }
    }
}

fn point_on_rect_perimeter(rect: Rectangle, distance: f32) -> Point {
    let mut remaining = distance;
    if remaining <= rect.width {
        return Point::new(rect.x + remaining, rect.y);
    }
    remaining -= rect.width;
    if remaining <= rect.height {
        return Point::new(rect.x + rect.width, rect.y + remaining);
    }
    remaining -= rect.height;
    if remaining <= rect.width {
        return Point::new(rect.x + rect.width - remaining, rect.y + rect.height);
    }
    Point::new(rect.x, rect.y + rect.height - (remaining - rect.width))
}

#[allow(clippy::too_many_arguments)]
fn draw_operation_motion(
    frame: &mut canvas::Frame,
    before: &StructuralFrame,
    after: &StructuralFrame,
    operation: Option<&StructuralOperation>,
    positions: &BTreeMap<String, Point>,
    progress: f32,
    phase: f32,
    scale: f32,
) {
    match operation {
        Some(StructuralOperation::TransferMetallicElectron {
            donor_site,
            acceptor,
            count,
            ..
        }) => draw_metallic_electron_transfer(
            frame, before, after, donor_site, acceptor, *count, positions, progress, phase, scale,
        ),
        Some(
            operation @ (StructuralOperation::CleaveCovalent { .. }
            | StructuralOperation::FormCovalent { .. }),
        ) => draw_covalent_electron_motion(
            frame, before, after, operation, positions, progress, phase, scale,
        ),
        Some(
            StructuralOperation::AssociateIonic { .. } | StructuralOperation::AssignProduct { .. },
        )
        | None => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_metallic_electron_transfer(
    frame: &mut canvas::Frame,
    before: &StructuralFrame,
    after: &StructuralFrame,
    donor_site: &str,
    acceptor: &str,
    count: u8,
    positions: &BTreeMap<String, Point>,
    progress: f32,
    phase: f32,
    scale: f32,
) {
    let (Some(donor), Some(acceptor_center)) = (positions.get(donor_site), positions.get(acceptor))
    else {
        return;
    };
    let (Some(before_acceptor), Some(after_acceptor)) =
        (atom(before, acceptor), atom(after, acceptor))
    else {
        return;
    };
    let delta = electron_state_delta(Some(before_acceptor), Some(after_acceptor));
    let targets = electron_positions(*acceptor_center, after_acceptor, phase, scale)
        .into_iter()
        .skip(usize::from(delta.persistent))
        .take(usize::from(delta.enters_shell))
        .collect::<Vec<_>>();
    debug_assert_eq!(targets.len(), usize::from(count));

    let direction = *acceptor_center - *donor;
    let magnitude = vector_magnitude(direction).max(1.0);
    let along = direction / magnitude;
    let perpendicular = Vector::new(-along.y, along.x);
    for (index, target) in targets.into_iter().enumerate() {
        let index = u8::try_from(index).unwrap_or(u8::MAX);
        let offset = f32::from(index) - (f32::from(count) - 1.0) * 0.5;
        let source = *donor + along * 33.0 * scale + perpendicular * offset * 7.0 * scale;
        draw_electron_route(frame, source, target, progress, -30.0 + offset * 8.0, scale);
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_covalent_electron_motion(
    frame: &mut canvas::Frame,
    before: &StructuralFrame,
    after: &StructuralFrame,
    operation: &StructuralOperation,
    positions: &BTreeMap<String, Point>,
    progress: f32,
    phase: f32,
    scale: f32,
) {
    let (left, right, order, forming) = match operation {
        StructuralOperation::FormCovalent {
            left, right, order, ..
        } => (left.as_str(), right.as_str(), *order, true),
        StructuralOperation::CleaveCovalent {
            left, right, order, ..
        } => (left.as_str(), right.as_str(), *order, false),
        _ => return,
    };
    let (Some(left_center), Some(right_center)) = (positions.get(left), positions.get(right))
    else {
        return;
    };
    let (Some(before_left), Some(after_left), Some(before_right), Some(after_right)) = (
        atom(before, left),
        atom(after, left),
        atom(before, right),
        atom(after, right),
    ) else {
        return;
    };

    let left_delta = electron_state_delta(Some(before_left), Some(after_left));
    let right_delta = electron_state_delta(Some(before_right), Some(after_right));
    let bond_positions = bond_electron_positions(*left_center, *right_center, order, scale);
    let (sources, targets) = if forming {
        let left_sources = moving_shell_electrons(
            *left_center,
            before_left,
            left_delta.persistent,
            left_delta.leaves_shell,
            phase,
            scale,
        );
        let right_sources = moving_shell_electrons(
            *right_center,
            before_right,
            right_delta.persistent,
            right_delta.leaves_shell,
            phase,
            scale,
        );
        (
            interleave_points(left_sources, right_sources),
            bond_positions,
        )
    } else {
        let left_targets = moving_shell_electrons(
            *left_center,
            after_left,
            left_delta.persistent,
            left_delta.enters_shell,
            phase,
            scale,
        );
        let right_targets = moving_shell_electrons(
            *right_center,
            after_right,
            right_delta.persistent,
            right_delta.enters_shell,
            phase,
            scale,
        );
        (
            bond_positions,
            interleave_points(left_targets, right_targets),
        )
    };
    debug_assert_eq!(sources.len(), usize::from(order) * 2);
    debug_assert_eq!(sources.len(), targets.len());
    for (index, (source, target)) in sources.into_iter().zip(targets).enumerate() {
        let bend = if index.is_multiple_of(2) { -8.0 } else { 8.0 };
        draw_electron_route(frame, source, target, progress, bend, scale);
    }
}

fn atom<'a>(frame: &'a StructuralFrame, id: &str) -> Option<&'a AtomState> {
    frame.atoms.iter().find(|atom| atom.id == id)
}

fn moving_shell_electrons(
    center: Point,
    atom: &AtomState,
    persistent: u8,
    moving: u8,
    phase: f32,
    scale: f32,
) -> Vec<Point> {
    electron_positions(center, atom, phase, scale)
        .into_iter()
        .skip(usize::from(persistent))
        .take(usize::from(moving))
        .collect()
}

fn interleave_points(left: Vec<Point>, right: Vec<Point>) -> Vec<Point> {
    let mut result = Vec::with_capacity(left.len() + right.len());
    let mut left = left.into_iter();
    let mut right = right.into_iter();
    loop {
        let left_next = left.next();
        let right_next = right.next();
        if left_next.is_none() && right_next.is_none() {
            break;
        }
        result.extend(left_next);
        result.extend(right_next);
    }
    result
}

fn bond_electron_positions(left: Point, right: Point, order: u8, scale: f32) -> Vec<Point> {
    let direction = right - left;
    let magnitude = vector_magnitude(direction).max(1.0);
    let along = direction / magnitude;
    let perpendicular = Vector::new(-along.y, along.x);
    let midpoint = lerp_point(left, right, 0.5);
    let offsets: &[f32] = match order {
        1 => &[0.0],
        2 => &[-4.0, 4.0],
        _ => &[-6.0, 0.0, 6.0],
    };
    offsets
        .iter()
        .flat_map(|offset| {
            let center = midpoint + perpendicular * *offset * scale;
            [center - along * 4.0 * scale, center + along * 4.0 * scale]
        })
        .collect()
}

fn draw_electron_route(
    frame: &mut canvas::Frame,
    start: Point,
    end: Point,
    progress: f32,
    bend: f32,
    scale: f32,
) {
    let direction = end - start;
    let magnitude = vector_magnitude(direction).max(1.0);
    let perpendicular = Vector::new(-direction.y / magnitude, direction.x / magnitude);
    let control = lerp_point(start, end, 0.5) + perpendicular * bend * scale;
    let path = Path::new(|builder| {
        builder.move_to(start);
        builder.quadratic_curve_to(control, end);
    });
    frame.stroke(
        &path,
        Stroke::default()
            .with_color(ACCENT.scale_alpha(0.16 + progress * 0.12))
            .with_width(scale),
    );
    let moving = quadratic_point(start, control, end, progress);
    frame.fill(&Path::circle(moving, 8.0 * scale), ACCENT.scale_alpha(0.10));
    frame.fill(
        &Path::circle(moving, 3.0 * scale),
        Color::WHITE.scale_alpha(0.96),
    );
}

fn quadratic_point(start: Point, control: Point, end: Point, progress: f32) -> Point {
    let inverse = 1.0 - progress;
    Point::new(
        inverse * inverse * start.x
            + 2.0 * inverse * progress * control.x
            + progress * progress * end.x,
        inverse * inverse * start.y
            + 2.0 * inverse * progress * control.y
            + progress * progress * end.y,
    )
}

fn draw_structure_labels(
    frame: &mut canvas::Frame,
    labels: &[ContextLabel],
    positions: &BTreeMap<String, Point>,
    bounds: Rectangle,
    progress: f32,
    scale: f32,
) {
    let reveal = animation_phase(progress).context;
    if reveal <= 0.0 {
        return;
    }
    for (index, label) in labels
        .iter()
        .filter(|label| label.kind != ExplanationLabelKind::ObservationExplanation)
        .take(2)
        .enumerate()
    {
        let target = average_position(label.target_atoms.iter().map(String::as_str), positions)
            .unwrap_or_else(|| Point::new(bounds.center_x(), bounds.center_y()));
        let direction = if index.is_multiple_of(2) { -1.0 } else { 1.0 };
        draw_context_card(
            frame,
            &label.title,
            &label.text,
            target,
            bounds,
            direction,
            explanation_color(label.kind),
            label.connector,
            reveal,
            scale,
        );
    }
}

#[allow(clippy::too_many_arguments, clippy::cast_precision_loss)]
fn draw_context_card(
    frame: &mut canvas::Frame,
    title: &str,
    body: &str,
    target: Point,
    bounds: Rectangle,
    direction: f32,
    accent: Color,
    connector: bool,
    reveal: f32,
    scale: f32,
) {
    let width = 214.0 * scale;
    let lines = wrap_words(body, 30);
    let height = (42.0 + lines.len() as f32 * 16.0) * scale;
    let preferred_x =
        target.x + direction * 92.0 * scale - if direction < 0.0 { width } else { 0.0 };
    let x = preferred_x.clamp(18.0, (bounds.width - width - 18.0).max(18.0));
    let above = target.y > bounds.height * 0.48;
    let y = if above {
        target.y - height - 72.0 * scale
    } else {
        target.y + 68.0 * scale
    }
    .clamp(76.0, (bounds.height - height - 70.0).max(76.0));
    let slide = (1.0 - reveal) * 12.0 * direction;
    let rect = Rectangle::new(Point::new(x + slide, y), Size::new(width, height));
    draw_glass_panel(frame, rect, accent, reveal, 11.0 * scale);
    frame.fill_text(canvas::Text {
        content: title.to_owned(),
        position: Point::new(rect.x + 14.0 * scale, rect.y + 13.0 * scale),
        color: accent.scale_alpha(reveal),
        size: iced::Pixels(9.5 * scale),
        ..canvas::Text::default()
    });
    for (index, line) in lines.iter().enumerate() {
        frame.fill_text(canvas::Text {
            content: line.clone(),
            position: Point::new(
                rect.x + 14.0 * scale,
                rect.y + (36.0 + index as f32 * 16.0) * scale,
            ),
            color: TEXT.scale_alpha(reveal),
            size: iced::Pixels(11.5 * scale),
            ..canvas::Text::default()
        });
    }
    if connector {
        let start = Point::new(
            if target.x < rect.x {
                rect.x
            } else {
                rect.x + rect.width
            },
            rect.y + rect.height * 0.5,
        );
        let end = lerp_point(start, target, reveal);
        frame.stroke(
            &Path::line(start, end),
            Stroke::default()
                .with_color(accent.scale_alpha(reveal * 0.56))
                .with_width(scale),
        );
        frame.fill(&Path::circle(end, 2.6 * scale), accent.scale_alpha(reveal));
    }
}

#[allow(clippy::cast_precision_loss)]
fn draw_scene_context(
    frame: &mut canvas::Frame,
    context: &SceneContext,
    bounds: Rectangle,
    progress: f32,
    scale: f32,
) {
    let enter = smoother_step((progress / 0.14).clamp(0.0, 1.0));
    let title = scene_title(context.kind);
    let chip_width = (title.len() as f32 * 7.2 + 86.0).clamp(160.0, 250.0) * scale;
    let chip_height = 34.0 * scale;
    let rect = Rectangle::new(
        Point::new(22.0, 18.0 - (1.0 - enter) * 10.0),
        Size::new(chip_width, chip_height),
    );
    draw_glass_panel(frame, rect, ACCENT, enter, 17.0 * scale);
    frame.fill(
        &Path::circle(
            Point::new(rect.x + 16.0 * scale, rect.y + rect.height * 0.5),
            3.0 * scale,
        ),
        ACCENT.scale_alpha(enter),
    );
    frame.fill_text(canvas::Text {
        content: title.to_owned(),
        position: Point::new(rect.x + 28.0 * scale, rect.y + rect.height * 0.5),
        color: TEXT.scale_alpha(enter),
        size: iced::Pixels(11.0 * scale),
        align_y: iced::alignment::Vertical::Center,
        ..canvas::Text::default()
    });
    if context.kind == EducationalSceneKind::Equation
        && let Some(equation) = &context.equation
    {
        draw_equation_card(frame, equation, bounds, progress, scale);
    }
}

fn scene_title(kind: EducationalSceneKind) -> &'static str {
    match kind {
        EducationalSceneKind::Introduction => "VALIDATED REACTION",
        EducationalSceneKind::ReactantSetup => "REACTANT STRUCTURES",
        EducationalSceneKind::Equation => "BALANCED EQUATION",
        EducationalSceneKind::StructuralChange => "STRUCTURAL CHANGE",
        EducationalSceneKind::ExplanationPause => "PAUSE & UNDERSTAND",
        EducationalSceneKind::ObservationConnection => "OBSERVATION LINK",
        EducationalSceneKind::Summary => "VALIDATED OUTCOME",
    }
}

fn draw_equation_card(
    frame: &mut canvas::Frame,
    equation: &str,
    bounds: Rectangle,
    progress: f32,
    scale: f32,
) {
    let reveal = smoother_step(((progress - 0.10) / 0.30).clamp(0.0, 1.0));
    let width = (bounds.width * 0.62).clamp(300.0, 760.0);
    let height = 116.0 * scale;
    let rect = Rectangle::new(
        Point::new(
            (bounds.width - width) * 0.5,
            (bounds.height - height) * 0.5 + (1.0 - reveal) * 14.0,
        ),
        Size::new(width, height),
    );
    draw_glass_panel(frame, rect, GOLD, reveal, 18.0 * scale);
    frame.fill_text(canvas::Text {
        content: "REVIEWED STOICHIOMETRY".to_owned(),
        position: Point::new(rect.center_x(), rect.y + 29.0 * scale),
        color: GOLD.scale_alpha(reveal),
        size: iced::Pixels(10.0 * scale),
        align_x: iced::alignment::Horizontal::Center.into(),
        ..canvas::Text::default()
    });
    frame.fill_text(canvas::Text {
        content: equation.to_owned(),
        position: Point::new(rect.center_x(), rect.y + 72.0 * scale),
        color: TEXT.scale_alpha(reveal),
        size: iced::Pixels(21.0 * scale),
        align_x: iced::alignment::Horizontal::Center.into(),
        ..canvas::Text::default()
    });
}

#[allow(clippy::cast_precision_loss)]
fn draw_observation_context(
    frame: &mut canvas::Frame,
    labels: &[ContextLabel],
    kind: EducationalSceneKind,
    bounds: Rectangle,
    progress: f32,
    scale: f32,
) {
    if kind != EducationalSceneKind::Summary {
        return;
    }
    let observations = labels
        .iter()
        .filter(|label| label.kind == ExplanationLabelKind::ObservationExplanation)
        .collect::<Vec<_>>();
    if observations.is_empty() {
        return;
    }
    let reveal = smoother_step(((progress - 0.18) / 0.24).clamp(0.0, 1.0));
    let mut x = 24.0;
    for observation in observations.iter().take(3) {
        let width = (observation.text.len() as f32 * 6.6 + 34.0).clamp(150.0, 310.0) * scale;
        if x + width > bounds.width - 24.0 {
            break;
        }
        let rect = Rectangle::new(
            Point::new(x, bounds.height - 52.0 * scale),
            Size::new(width, 30.0 * scale),
        );
        draw_glass_panel(frame, rect, IONIC, reveal, 15.0 * scale);
        frame.fill(
            &Path::circle(
                Point::new(rect.x + 13.0 * scale, rect.center_y()),
                2.6 * scale,
            ),
            IONIC.scale_alpha(reveal),
        );
        frame.fill_text(canvas::Text {
            content: observation.text.clone(),
            position: Point::new(rect.x + 23.0 * scale, rect.center_y()),
            color: TEXT.scale_alpha(reveal),
            size: iced::Pixels(10.5 * scale),
            align_y: iced::alignment::Vertical::Center,
            ..canvas::Text::default()
        });
        x += width + 8.0 * scale;
    }
}

#[allow(clippy::cast_precision_loss)]
fn draw_explanation_label(
    frame: &mut canvas::Frame,
    label: &ExplanationLabel,
    positions: &BTreeMap<String, Point>,
    bounds: Rectangle,
    progress: f32,
    scale: f32,
) {
    let target = average_position(label.target_atoms.iter().map(String::as_str), positions);
    let max_width = (bounds.width - 40.0).max(240.0);
    let width = (410.0 * scale).clamp(260.0, max_width.min(460.0));
    let lines = wrap_words(&label.text, if width > 390.0 { 47 } else { 33 });
    let height = (70.0 + lines.len() as f32 * 20.0) * scale;
    let (x, base_y) = explanation_position(target, bounds, width, height);
    let enter = smoother_step(((progress - 0.04) / 0.14).clamp(0.0, 1.0));
    let alpha = enter;
    let rect = Rectangle::new(
        Point::new(x, base_y + (1.0 - enter) * 18.0 * scale),
        Size::new(width, height),
    );
    let accent = explanation_color(label.kind);
    draw_glass_panel(frame, rect, accent, alpha, 16.0 * scale);
    frame.fill(
        &rounded_rectangle(
            Rectangle::new(
                Point::new(rect.x + 14.0 * scale, rect.y + 15.0 * scale),
                Size::new(3.0 * scale, rect.height - 30.0 * scale),
            ),
            2.0 * scale,
        ),
        accent.scale_alpha(alpha),
    );
    frame.fill_text(canvas::Text {
        content: explanation_title(label.kind).to_owned(),
        position: Point::new(rect.x + 30.0 * scale, rect.y + 22.0 * scale),
        color: accent.scale_alpha(alpha),
        size: iced::Pixels(9.5 * scale),
        ..canvas::Text::default()
    });
    for (index, line) in lines.iter().enumerate() {
        frame.fill_text(canvas::Text {
            content: line.clone(),
            position: Point::new(
                rect.x + 30.0 * scale,
                rect.y + (50.0 + index as f32 * 20.0) * scale,
            ),
            color: TEXT.scale_alpha(alpha),
            size: iced::Pixels(13.5 * scale),
            ..canvas::Text::default()
        });
    }
    if label.connector
        && let Some(target) = target
    {
        let start = nearest_edge_point(rect, target);
        let line_progress = smoother_step(((progress - 0.12) / 0.24).clamp(0.0, 1.0));
        let end = lerp_point(start, target, line_progress);
        frame.stroke(
            &Path::line(start, end),
            Stroke::default()
                .with_color(accent.scale_alpha(alpha * 0.62))
                .with_width(1.2 * scale),
        );
        frame.stroke(
            &Path::circle(target, (6.0 + 3.0 * (1.0 - line_progress)) * scale),
            Stroke::default()
                .with_color(accent.scale_alpha(alpha * line_progress))
                .with_width(1.2 * scale),
        );
        frame.fill(
            &Path::circle(target, 2.4 * scale),
            accent.scale_alpha(alpha * line_progress),
        );
    }
}

fn explanation_position(
    target: Option<Point>,
    bounds: Rectangle,
    width: f32,
    height: f32,
) -> (f32, f32) {
    let horizontal_margin = 22.0;
    let x = target.map_or((bounds.width - width) * 0.5, |point| {
        if point.x < bounds.width * 0.5 {
            bounds.width - width - horizontal_margin
        } else {
            horizontal_margin
        }
    });
    let y = target.map_or(bounds.height * 0.62, |point| {
        if point.y > bounds.height * 0.52 {
            76.0
        } else {
            bounds.height - height - 68.0
        }
    });
    (
        x.clamp(
            horizontal_margin,
            (bounds.width - width - horizontal_margin).max(horizontal_margin),
        ),
        y.clamp(70.0, (bounds.height - height - 60.0).max(70.0)),
    )
}

fn nearest_edge_point(rect: Rectangle, target: Point) -> Point {
    let x = target.x.clamp(rect.x, rect.x + rect.width);
    let y = target.y.clamp(rect.y, rect.y + rect.height);
    let distances = [
        (target.x - rect.x).abs(),
        (target.x - (rect.x + rect.width)).abs(),
        (target.y - rect.y).abs(),
        (target.y - (rect.y + rect.height)).abs(),
    ];
    let edge = distances
        .iter()
        .enumerate()
        .min_by(|left, right| left.1.total_cmp(right.1))
        .map_or(0, |(index, _)| index);
    match edge {
        0 => Point::new(rect.x, y),
        1 => Point::new(rect.x + rect.width, y),
        2 => Point::new(x, rect.y),
        _ => Point::new(x, rect.y + rect.height),
    }
}

fn draw_glass_panel(
    frame: &mut canvas::Frame,
    rect: Rectangle,
    accent: Color,
    alpha: f32,
    radius: f32,
) {
    frame.fill(
        &rounded_rectangle(
            Rectangle::new(
                Point::new(rect.x, rect.y + 7.0),
                Size::new(rect.width, rect.height),
            ),
            radius,
        ),
        Color::from_rgba(0.0, 0.0, 0.0, alpha * 0.24),
    );
    let path = rounded_rectangle(rect, radius);
    frame.fill(&path, PANEL.scale_alpha(alpha * 0.96));
    frame.stroke(
        &path,
        Stroke::default()
            .with_color(accent.scale_alpha(alpha * 0.42))
            .with_width(1.0),
    );
    frame.fill(
        &rounded_rectangle(
            Rectangle::new(rect.position(), Size::new(rect.width, rect.height * 0.45)),
            radius,
        ),
        Color::WHITE.scale_alpha(alpha * 0.018),
    );
}

fn rounded_rectangle(rect: Rectangle, radius: f32) -> Path {
    Path::rounded_rectangle(rect.position(), rect.size(), border::Radius::new(radius))
}

fn explanation_color(kind: ExplanationLabelKind) -> Color {
    match kind {
        ExplanationLabelKind::ObservationExplanation | ExplanationLabelKind::ImportantResult => {
            IONIC
        }
        ExplanationLabelKind::EquationExplanation => GOLD,
        ExplanationLabelKind::SummaryExplanation => Color::from_rgb(0.70, 0.88, 0.78),
        ExplanationLabelKind::ConceptExplanation
        | ExplanationLabelKind::StructuralChangeExplanation => ACCENT,
    }
}

fn explanation_title(kind: ExplanationLabelKind) -> &'static str {
    match kind {
        ExplanationLabelKind::ConceptExplanation => "KEY CONCEPT",
        ExplanationLabelKind::StructuralChangeExplanation => "WHAT CHANGED",
        ExplanationLabelKind::ObservationExplanation => "WHY YOU OBSERVE THIS",
        ExplanationLabelKind::EquationExplanation => "EQUATION CONTEXT",
        ExplanationLabelKind::ImportantResult => "IMPORTANT RESULT",
        ExplanationLabelKind::SummaryExplanation => "REACTION SUMMARY",
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

fn active_atoms(operation: Option<&StructuralOperation>) -> BTreeSet<&str> {
    match operation {
        Some(
            StructuralOperation::AssociateIonic { left, right }
            | StructuralOperation::CleaveCovalent { left, right, .. }
            | StructuralOperation::FormCovalent { left, right, .. },
        ) => BTreeSet::from([left.as_str(), right.as_str()]),
        Some(StructuralOperation::AssignProduct { atoms, .. }) => {
            atoms.iter().map(String::as_str).collect()
        }
        Some(StructuralOperation::TransferMetallicElectron {
            donor_site,
            acceptor,
            ..
        }) => BTreeSet::from([donor_site.as_str(), acceptor.as_str()]),
        None => BTreeSet::new(),
    }
}

fn element_color(symbol: &str) -> Color {
    match symbol {
        "H" => Color::from_rgb(0.86, 0.91, 0.96),
        "Li" => Color::from_rgb(0.71, 0.58, 0.96),
        "Ag" => Color::from_rgb(0.78, 0.83, 0.88),
        "Cl" => Color::from_rgb(0.48, 0.89, 0.69),
        "Na" => Color::from_rgb(0.67, 0.54, 0.94),
        "N" => Color::from_rgb(0.45, 0.66, 0.96),
        "O" => Color::from_rgb(0.95, 0.40, 0.42),
        _ => Color::from_rgb(0.62, 0.68, 0.74),
    }
}

fn vector_magnitude(vector: Vector) -> f32 {
    (vector.x * vector.x + vector.y * vector.y).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn atom_state(id: &str, non_bonding_electrons: u8, unpaired_electrons: u8) -> AtomState {
        AtomState {
            id: id.to_owned(),
            element: "X".to_owned(),
            formal_charge: 0,
            non_bonding_electrons,
            unpaired_electrons,
        }
    }

    #[test]
    fn animation_phases_are_smooth_and_bounded() {
        let mut previous = 0.0;
        for step in 0_u8..=100 {
            let phase = animation_phase(f32::from(step) / 100.0);
            assert!((0.0..=1.0).contains(&phase.action));
            assert!(phase.action >= previous);
            previous = phase.action;
        }
        assert!(animation_phase(0.0).action.abs() < f32::EPSILON);
        assert!((animation_phase(1.0).action - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn interpolation_keeps_atoms_from_both_frames() {
        let before = BTreeMap::from([
            ("a".to_owned(), Point::new(0.0, 0.0)),
            ("b".to_owned(), Point::new(10.0, 0.0)),
        ]);
        let after = BTreeMap::from([
            ("b".to_owned(), Point::new(20.0, 0.0)),
            ("c".to_owned(), Point::new(30.0, 0.0)),
        ]);
        let positions = interpolated_positions(&before, &after, 0.5);
        assert_eq!(positions.len(), 3);
        assert_eq!(positions["a"], Point::new(0.0, 0.0));
        assert_eq!(positions["b"], Point::new(15.0, 0.0));
        assert_eq!(positions["c"], Point::new(30.0, 0.0));
    }

    #[test]
    fn word_wrapping_preserves_content() {
        let source = "A shared electron pair forms a covalent bond";
        let lines = wrap_words(source, 18);
        assert_eq!(lines.join(" "), source);
        assert!(lines.len() > 1);
    }

    #[test]
    fn shell_layout_preserves_lone_pairs_and_unpaired_electrons() {
        assert_eq!(electron_domain_occupancies(4, 0), vec![2, 2]);
        assert_eq!(electron_domain_occupancies(5, 1), vec![2, 2, 1]);
        assert_eq!(electron_domain_occupancies(6, 0), vec![2, 2, 2]);
        assert_eq!(electron_domain_occupancies(4, 4), vec![1, 1, 1, 1]);
    }

    #[test]
    fn metallic_transfer_routes_one_electron_without_duplicating_the_shell() {
        let before_acceptor = atom_state("acceptor", 4, 0);
        let after_acceptor = atom_state("acceptor", 5, 1);
        let delta = electron_state_delta(Some(&before_acceptor), Some(&after_acceptor));
        assert_eq!(
            delta,
            ElectronStateDelta {
                persistent: 4,
                leaves_shell: 0,
                enters_shell: 1,
            }
        );

        let before_domain = 2_u8;
        let after_domain = 1_u8;
        for _progress in [0.0_f32, 0.5, 1.0] {
            let visible = delta.persistent + delta.enters_shell + after_domain;
            assert_eq!(
                visible,
                before_acceptor.non_bonding_electrons + before_domain
            );
            assert_eq!(visible, after_acceptor.non_bonding_electrons + after_domain);
        }
    }

    #[test]
    fn covalent_formation_moves_atomic_electrons_into_one_shared_pair() {
        let before_left = atom_state("left", 1, 1);
        let after_left = atom_state("left", 0, 0);
        let before_right = atom_state("right", 1, 1);
        let after_right = atom_state("right", 0, 0);
        let left = electron_state_delta(Some(&before_left), Some(&after_left));
        let right = electron_state_delta(Some(&before_right), Some(&after_right));
        let moving = left.leaves_shell + right.leaves_shell;
        let shared = bond_electron_positions(Point::new(0.0, 0.0), Point::new(100.0, 0.0), 1, 1.0);
        assert_eq!(moving, 2);
        assert_eq!(shared.len(), usize::from(moving));

        for _progress in [0.0_f32, 0.5, 1.0] {
            let visible = left.persistent + right.persistent + moving;
            assert_eq!(
                visible,
                before_left.non_bonding_electrons + before_right.non_bonding_electrons
            );
            assert_eq!(
                visible,
                after_left.non_bonding_electrons + after_right.non_bonding_electrons + 2
            );
        }
    }

    #[test]
    fn covalent_cleavage_moves_the_shared_pair_to_new_shell_positions() {
        let before_left = atom_state("left", 5, 1);
        let after_left = atom_state("left", 6, 0);
        let before_right = atom_state("right", 0, 0);
        let after_right = atom_state("right", 1, 1);
        let left = electron_state_delta(Some(&before_left), Some(&after_left));
        let right = electron_state_delta(Some(&before_right), Some(&after_right));
        let moving = left.enters_shell + right.enters_shell;
        assert_eq!(left.persistent, 5);
        assert_eq!(moving, 2);

        for _progress in [0.0_f32, 0.5, 1.0] {
            let visible = left.persistent + right.persistent + moving;
            assert_eq!(
                visible,
                before_left.non_bonding_electrons + before_right.non_bonding_electrons + 2
            );
            assert_eq!(
                visible,
                after_left.non_bonding_electrons + after_right.non_bonding_electrons
            );
        }
    }
}
