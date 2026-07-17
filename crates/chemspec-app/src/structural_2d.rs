//! Educational 2D rendering of trusted structural frames.
//!
//! This module performs deterministic presentation layout only. It never
//! parses source, resolves catalogue rules, or infers a chemical relationship.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use chem_domain::StructuralOperationView;
use chem_kernel::SimulationFrame;
use chem_presentation::{
    ContextLabel, EducationalPlan, EducationalSceneKind, ExplanationLabel, ExplanationLabelKind,
};
use iced::mouse::Cursor;
use iced::widget::canvas::{self, Path, Stroke};
use iced::{Color, Point, Rectangle, Renderer, Size, Theme, Vector, border};

use crate::elements;
use crate::fonts;
use crate::structural_physics::{Anchor, AtomSpec, DragTarget, Spring, VIRTUAL, WorldSpec};
use crate::theme::{LAB_DARK, chemistry_color, color};
use iced::mouse;

const ACCENT: Color = color::ACCENT;
const ACCENT_BRIGHT: Color = color::ACCENT_HOVER;
const IONIC: Color = chemistry_color::IONIC;
const GOLD: Color = color::WARNING;
const CANVAS: Color = chemistry_color::STRUCTURAL_CANVAS;
const PANEL: Color = chemistry_color::STRUCTURAL_PANEL;
const TEXT: Color = color::TEXT;
const TEXT_SOFT: Color = color::TEXT_SOFT;

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

#[derive(Debug, Clone, PartialEq, Eq)]
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
    effective_order: Option<(u8, u8)>,
}

#[derive(Debug, Clone)]
struct RenderIonicAssociation {
    id: String,
    components: Vec<RenderIonicComponent>,
}

#[derive(Debug, Clone)]
struct RenderIonicComponent {
    atoms: Vec<String>,
    charge: i64,
}

#[derive(Debug, Clone)]
struct RenderMetallicDomain {
    id: String,
    sites: Vec<String>,
    delocalized_electrons: u16,
}

#[derive(Debug, Clone)]
enum RenderOperation {
    TransferMetallicElectron {
        domain: String,
        donor_site: String,
        acceptor: String,
        count: u8,
    },
    CleaveCovalent {
        left: String,
        right: String,
        order: u8,
        endpoint_states: [Option<RenderAtom>; 4],
    },
    FormCovalent {
        left: String,
        right: String,
        order: u8,
        endpoint_states: [Option<RenderAtom>; 4],
    },
    AssociateIonic {
        atoms: Vec<String>,
    },
    AssignProduct {
        atoms: Vec<String>,
    },
    Other {
        atoms: Vec<String>,
    },
}

type StructuralFrame = RenderFrame;
type AtomState = RenderAtom;
type StructuralOperation = RenderOperation;

#[derive(Debug, Clone)]
struct RenderFrame {
    atoms: Vec<RenderAtom>,
    covalent_bonds: Vec<RenderBond>,
    ionic_associations: Vec<RenderIonicAssociation>,
    metallic_domains: Vec<RenderMetallicDomain>,
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
                effective_order: bond.delocalization.as_ref().map(|value| {
                    let order = value.effective_order();
                    (order.numerator(), order.denominator())
                }),
            })
            .collect();
        let ionic_associations = frame
            .ionic_associations()
            .values()
            .map(|association| RenderIonicAssociation {
                id: association.id.as_str().to_owned(),
                components: association
                    .components
                    .iter()
                    .map(|(group, atoms)| RenderIonicComponent {
                        atoms: atoms.iter().map(|atom| atom.as_str().to_owned()).collect(),
                        charge: association
                            .component_charges
                            .get(group)
                            .copied()
                            .unwrap_or(0),
                    })
                    .collect(),
            })
            .collect();
        let metallic_domains = frame
            .metallic_domains()
            .values()
            .map(|domain| RenderMetallicDomain {
                id: domain.id.as_str().to_owned(),
                sites: domain
                    .sites
                    .iter()
                    .map(|site| site.as_str().to_owned())
                    .collect(),
                delocalized_electrons: u16::try_from(domain.delocalized_electrons)
                    .unwrap_or(u16::MAX),
            })
            .collect();
        Self {
            atoms,
            covalent_bonds,
            ionic_associations,
            metallic_domains,
        }
    }
}

fn render_operation(
    operation: StructuralOperationView<'_>,
    before: &SimulationFrame,
    after: &SimulationFrame,
) -> RenderOperation {
    match operation {
        StructuralOperationView::ReconfigureElectrons { transition } => RenderOperation::Other {
            atoms: vec![transition.atom().as_str().to_owned()],
        },
        StructuralOperationView::CleaveCovalent {
            left,
            right,
            expected_order,
            ..
        } => RenderOperation::CleaveCovalent {
            left: left.as_str().to_owned(),
            right: right.as_str().to_owned(),
            order: expected_order.order(),
            endpoint_states: render_endpoint_states(before, after, left, right),
        },
        StructuralOperationView::FormCovalent {
            left, right, order, ..
        } => RenderOperation::FormCovalent {
            left: left.as_str().to_owned(),
            right: right.as_str().to_owned(),
            order: order.order(),
            endpoint_states: render_endpoint_states(before, after, left, right),
        },
        StructuralOperationView::CleaveDative {
            donor, acceptor, ..
        }
        | StructuralOperationView::FormDative {
            donor, acceptor, ..
        } => RenderOperation::Other {
            atoms: vec![donor.as_str().to_owned(), acceptor.as_str().to_owned()],
        },
        StructuralOperationView::ChangeCovalent { left, right, .. }
        | StructuralOperationView::ChangeCovalentDelocalization { left, right, .. } => {
            RenderOperation::Other {
                atoms: vec![left.as_str().to_owned(), right.as_str().to_owned()],
            }
        }
        StructuralOperationView::AssociateIonic { association } => {
            RenderOperation::AssociateIonic {
                atoms: association
                    .components()
                    .iter()
                    .filter_map(|group| after.groups().get(group))
                    .flat_map(|group| group.atoms.iter())
                    .map(|atom| atom.as_str().to_owned())
                    .collect(),
            }
        }
        StructuralOperationView::DissociateIonic { .. } => {
            RenderOperation::Other { atoms: Vec::new() }
        }
        StructuralOperationView::ReleaseMetallic { site, .. }
        | StructuralOperationView::JoinMetallic { site, .. } => RenderOperation::Other {
            atoms: vec![site.as_str().to_owned()],
        },
        StructuralOperationView::TransferElectron {
            donor,
            acceptor,
            count,
            ..
        } => RenderOperation::TransferMetallicElectron {
            domain: before
                .metallic_domains()
                .values()
                .find(|domain| domain.sites.contains(donor))
                .or_else(|| {
                    after
                        .metallic_domains()
                        .values()
                        .find(|domain| domain.sites.contains(donor))
                })
                .map_or_else(String::new, |domain| domain.id.as_str().to_owned()),
            donor_site: donor.as_str().to_owned(),
            acceptor: acceptor.as_str().to_owned(),
            count,
        },
        StructuralOperationView::AssignProduct { atoms, .. } => RenderOperation::AssignProduct {
            atoms: atoms.iter().map(|atom| atom.as_str().to_owned()).collect(),
        },
    }
}

fn render_endpoint_states(
    before: &SimulationFrame,
    after: &SimulationFrame,
    left: &chem_domain::AtomId,
    right: &chem_domain::AtomId,
) -> [Option<RenderAtom>; 4] {
    [
        render_atom_state(before, left),
        render_atom_state(after, left),
        render_atom_state(before, right),
        render_atom_state(after, right),
    ]
}

fn render_atom_state(frame: &SimulationFrame, id: &chem_domain::AtomId) -> Option<RenderAtom> {
    frame.atoms().get(id).map(|atom| RenderAtom {
        id: atom.id.as_str().to_owned(),
        element: atom.element.as_str().to_owned(),
        formal_charge: atom.electrons.formal_charge(),
        non_bonding_electrons: atom.electrons.non_bonding_electrons(),
        unpaired_electrons: atom.electrons.unpaired_electrons(),
    })
}

/// Drawn atom radius in virtual units, scaled by the cube root of atomic
/// mass so heavier elements read as heavier without dwarfing hydrogen.
#[must_use]
pub fn atom_visual_radius(element: &str) -> f32 {
    let mass = elements::atomic_mass(element).unwrap_or(16.0);
    24.0 * (mass / 16.0).cbrt().clamp(0.62, 1.5)
}

/// Structural action progress for a scene: explanation beats compress the
/// structural motion into the first 55% of the scene.
#[must_use]
pub fn scene_action(kind: EducationalSceneKind, has_explanation: bool, progress: f32) -> f32 {
    let structural = if kind == EducationalSceneKind::StructuralChange && has_explanation {
        (progress / 0.55).clamp(0.0, 1.0)
    } else {
        progress
    };
    animation_phase(structural).action
}

/// A drag interaction on the structural canvas, in virtual coordinates.
#[derive(Debug, Clone)]
pub enum DragEvent {
    Started { target: DragTarget, cursor: Point },
    Moved { cursor: Point },
    Ended,
}

fn virtual_rectangle() -> Rectangle {
    Rectangle::new(Point::ORIGIN, VIRTUAL)
}

/// The whole virtual world: the camera before any scene has framed it.
#[must_use]
pub fn default_camera() -> Rectangle {
    virtual_rectangle()
}

/// Uniform fit of a camera's world rectangle into the widget bounds.
fn camera_transform(bounds: Rectangle, camera: Rectangle) -> (f32, Vector) {
    let fit = (bounds.width / camera.width.max(1.0))
        .min(bounds.height / camera.height.max(1.0))
        .max(0.05);
    let offset = Vector::new(
        (bounds.width - camera.width * fit) * 0.5 - camera.x * fit,
        (bounds.height - camera.height * fit) * 0.5 - camera.y * fit,
    );
    (fit, offset)
}

/// Camera rectangle framing everything one scene touches: the union of the
/// before and after home layouts — never the live physics positions, so
/// user drags and mid-scene motion cannot make the camera chase. Padded,
/// with a zoom ceiling so scale changes stay gentle.
#[must_use]
pub fn chapter_camera(
    before: &SimulationFrame,
    after: &SimulationFrame,
    before_homes: &BTreeMap<String, Point>,
    after_homes: &BTreeMap<String, Point>,
) -> Rectangle {
    let before_frame = RenderFrame::from(before);
    let after_frame = RenderFrame::from(after);
    let bounds = virtual_rectangle();
    let mut elements = BTreeMap::new();
    for atom in before_frame.atoms.iter().chain(&after_frame.atoms) {
        elements.insert(atom.id.clone(), atom.element.clone());
    }
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for (id, point) in before_homes.iter().chain(after_homes.iter()) {
        let radius = elements
            .get(id)
            .map_or(24.0, |element| atom_visual_radius(element));
        min_x = min_x.min(point.x - radius);
        min_y = min_y.min(point.y - radius);
        max_x = max_x.max(point.x + radius);
        max_y = max_y.max(point.y + radius);
    }
    if !(min_x.is_finite() && min_y.is_finite() && max_x.is_finite() && max_y.is_finite()) {
        return bounds;
    }
    let padding = 130.0;
    let width = (max_x - min_x + padding * 2.0).max(680.0).min(bounds.width);
    let height = (max_y - min_y + padding * 2.0)
        .max(420.0)
        .min(bounds.height);
    let center_x = f32::midpoint(min_x, max_x);
    let center_y = f32::midpoint(min_y, max_y);
    Rectangle::new(
        Point::new(
            (center_x - width * 0.5).clamp(0.0, bounds.width - width),
            (center_y - height * 0.5).clamp(0.0, bounds.height - height),
        ),
        Size::new(width, height),
    )
}

/// Eases a camera toward its target with a small deadband, so micro
/// re-framings never move the view and real chapter changes glide.
pub fn ease_camera(camera: &mut Rectangle, target: Rectangle) {
    let tolerance = 0.05 * target.width.max(target.height);
    if (camera.x - target.x).abs() < tolerance
        && (camera.y - target.y).abs() < tolerance
        && (camera.width - target.width).abs() < tolerance
        && (camera.height - target.height).abs() < tolerance
    {
        return;
    }
    // 33ms ticks: settles in roughly 1.3 seconds, no overshoot.
    let ease = 0.075;
    camera.x += (target.x - camera.x) * ease;
    camera.y += (target.y - camera.y) * ease;
    camera.width += (target.width - camera.width) * ease;
    camera.height += (target.height - camera.height) * ease;
}

fn to_screen(point: Point, fit: f32, offset: Vector) -> Point {
    Point::new(point.x * fit, point.y * fit) + offset
}

fn to_virtual(point: Point, fit: f32, offset: Vector) -> Point {
    Point::new((point.x - offset.x) / fit, (point.y - offset.y) / fit)
}

/// The physics world for one scene moment: blended bond springs, per-atom
/// homes from the story-anchored layout timeline, and mass-scaled radii.
#[must_use]
pub fn world_spec(
    before: &SimulationFrame,
    after: &SimulationFrame,
    action: f32,
    before_homes: &BTreeMap<String, Point>,
    after_homes: &BTreeMap<String, Point>,
) -> WorldSpec {
    let before_frame = RenderFrame::from(before);
    let after_frame = RenderFrame::from(after);

    let mut atoms = BTreeMap::new();
    for atom in before_frame.atoms.iter().chain(&after_frame.atoms) {
        atoms.insert(atom.id.clone(), atom.element.clone());
    }
    let radius_of = |id: &str| {
        atoms
            .get(id)
            .map_or(24.0, |element| atom_visual_radius(element))
    };
    let home_of = |id: &str| {
        let start = before_homes.get(id).or_else(|| after_homes.get(id));
        let end = after_homes.get(id).or_else(|| before_homes.get(id));
        match (start, end) {
            (Some(start), Some(end)) => Some(lerp_point(*start, *end, action)),
            _ => None,
        }
    };

    // Covalent springs, blended across the transition so breaking bonds
    // release and forming bonds tighten as the operation plays.
    let mut springs = BTreeMap::<(String, String), Spring>::new();
    let mut add_bonds = |frame: &RenderFrame, weight: f32| {
        for bond in &frame.covalent_bonds {
            let key = sorted_pair(&bond.left, &bond.right);
            let rest = radius_of(&bond.left) + radius_of(&bond.right) + 22.0
                - 3.0 * f32::from(bond.order.saturating_sub(1));
            let entry = springs.entry(key.clone()).or_insert(Spring {
                a: key.0,
                b: key.1,
                rest,
                strength: 0.0,
            });
            entry.strength = (entry.strength + weight).min(1.0);
        }
    };
    add_bonds(&before_frame, 1.0 - action);
    add_bonds(&after_frame, action);

    // Ionic partners attract loosely between their charged anchor atoms.
    let mut add_ionic = |frame: &RenderFrame, weight: f32| {
        for association in &frame.ionic_associations {
            let render = RenderIonicAssociation {
                id: association.id.clone(),
                components: association.components.clone(),
            };
            for (left, right) in ionic_component_pairs(&render) {
                let (Some(anchor_left), Some(anchor_right)) = (
                    ionic_anchor_id(&association.components[left], frame),
                    ionic_anchor_id(&association.components[right], frame),
                ) else {
                    continue;
                };
                let key = sorted_pair(anchor_left, anchor_right);
                let rest = radius_of(anchor_left) + radius_of(anchor_right) + 34.0;
                let entry = springs.entry(key.clone()).or_insert(Spring {
                    a: key.0,
                    b: key.1,
                    rest,
                    strength: 0.0,
                });
                entry.strength = (entry.strength + weight * 0.55).min(1.0);
            }
        }
    };
    add_ionic(&before_frame, 1.0 - action);
    add_ionic(&after_frame, action);

    let spec_atoms = atoms
        .iter()
        .filter_map(|(id, element)| {
            home_of(id).map(|home| AtomSpec {
                id: id.clone(),
                radius: atom_visual_radius(element),
                seed: home,
            })
        })
        .collect::<Vec<_>>();
    let anchors = spec_atoms
        .iter()
        .map(|atom| Anchor {
            atom: atom.id.clone(),
            home: atom.seed,
            strength: 10.0,
        })
        .collect();
    WorldSpec {
        atoms: spec_atoms,
        springs: springs
            .into_values()
            .filter(|spring| spring.strength > 0.04)
            .collect(),
        anchors,
    }
}

#[derive(Debug, Clone)]
pub struct Diagram {
    before: RenderFrame,
    after: RenderFrame,
    operations: Vec<RenderOperation>,
    progress: f32,
    explanation: Option<ExplanationLabel>,
    context_labels: Vec<ContextLabel>,
    context: SceneContext,
    ambient_progress: f32,
    show_structure_labels: bool,
    /// Live simulation positions, in virtual coordinates.
    positions: BTreeMap<String, Point>,
    /// World rectangle the view frames, in virtual coordinates.
    camera: Rectangle,
}

impl Diagram {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        before: &SimulationFrame,
        after: &SimulationFrame,
        operation_transitions: &[(&SimulationFrame, &SimulationFrame)],
        progress: f32,
        explanation: Option<&ExplanationLabel>,
        context_labels: &[ContextLabel],
        context: SceneContext,
        ambient_progress: f32,
        show_structure_labels: bool,
        positions: BTreeMap<String, Point>,
        camera: Rectangle,
    ) -> Self {
        Self {
            camera,
            before: RenderFrame::from(before),
            after: RenderFrame::from(after),
            operations: operation_transitions
                .iter()
                .filter_map(|(before, after)| {
                    after
                        .active_operation()
                        .map(|active| render_operation(active.operation.view(), before, after))
                })
                .collect(),
            progress: progress.clamp(0.0, 1.0),
            explanation: explanation.cloned(),
            context_labels: context_labels.to_vec(),
            context,
            ambient_progress: ambient_progress.clamp(0.0, 1.0),
            show_structure_labels,
            positions,
        }
    }

    /// Nearest draggable thing to a virtual-space point: an atom within its
    /// circle (plus slack), else a bond segment within grab range.
    fn hit_test(&self, point: Point) -> Option<DragTarget> {
        let mut elements = BTreeMap::new();
        for atom in self.before.atoms.iter().chain(&self.after.atoms) {
            elements.insert(atom.id.as_str(), atom.element.as_str());
        }
        let mut best_atom: Option<(f32, &str)> = None;
        for (id, position) in &self.positions {
            let Some(element) = elements.get(id.as_str()) else {
                continue;
            };
            let distance = position.distance(point);
            let reach = atom_visual_radius(element) + 10.0;
            if distance <= reach && best_atom.is_none_or(|(closest, _)| distance < closest) {
                best_atom = Some((distance, id));
            }
        }
        if let Some((_, id)) = best_atom {
            return Some(DragTarget::Atom(id.to_owned()));
        }

        let mut best_bond: Option<(f32, (&str, &str))> = None;
        for bond in self
            .before
            .covalent_bonds
            .iter()
            .chain(&self.after.covalent_bonds)
        {
            let (Some(left), Some(right)) = (
                self.positions.get(&bond.left),
                self.positions.get(&bond.right),
            ) else {
                continue;
            };
            let distance = segment_distance(point, *left, *right);
            if distance <= 14.0 && best_bond.is_none_or(|(closest, _)| distance < closest) {
                best_bond = Some((distance, (&bond.left, &bond.right)));
            }
        }
        best_bond.map(|(_, (left, right))| DragTarget::Bond(left.to_owned(), right.to_owned()))
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

impl canvas::Program<DragEvent> for Diagram {
    type State = bool;

    fn update(
        &self,
        dragging: &mut Self::State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: Cursor,
    ) -> Option<canvas::Action<DragEvent>> {
        let (fit, offset) = camera_transform(bounds, self.camera);
        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let point = cursor.position_in(bounds)?;
                let target = self.hit_test(to_virtual(point, fit, offset))?;
                *dragging = true;
                Some(
                    canvas::Action::publish(DragEvent::Started {
                        target,
                        cursor: to_virtual(point, fit, offset),
                    })
                    .and_capture(),
                )
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) if *dragging => {
                let point = cursor.position_in(bounds)?;
                Some(canvas::Action::publish(DragEvent::Moved {
                    cursor: to_virtual(point, fit, offset),
                }))
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if *dragging =>
            {
                *dragging = false;
                Some(canvas::Action::publish(DragEvent::Ended).and_capture())
            }
            _ => None,
        }
    }

    fn mouse_interaction(
        &self,
        dragging: &Self::State,
        bounds: Rectangle,
        cursor: Cursor,
    ) -> mouse::Interaction {
        if *dragging {
            return mouse::Interaction::Grabbing;
        }
        let (fit, offset) = camera_transform(bounds, self.camera);
        cursor
            .position_in(bounds)
            .and_then(|point| self.hit_test(to_virtual(point, fit, offset)))
            .map_or(mouse::Interaction::default(), |_| mouse::Interaction::Grab)
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let (fit, offset) = camera_transform(bounds, self.camera);
        let scale = fit;
        draw_atmosphere(&mut frame, bounds);

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
        let action = animation_phase(structural_progress).action;
        let positions: BTreeMap<String, Point> = self
            .positions
            .iter()
            .map(|(id, point)| (id.clone(), to_screen(*point, fit, offset)))
            .collect();
        let active = active_atoms(&self.operations);
        let content_alpha = 1.0;

        draw_metallic_transition(
            &mut frame,
            &self.before,
            &self.after,
            &positions,
            &self.operations,
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
            &self.operations,
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
                &self.operations,
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

fn smoother_step(value: f32) -> f32 {
    value * value * value * (value * (value * 6.0 - 15.0) + 10.0)
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
fn draw_atmosphere(frame: &mut canvas::Frame, bounds: Rectangle) {
    frame.fill_rectangle(Point::ORIGIN, bounds.size(), CANVAS);
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

/// Story-anchored homes for every frame in playback order. The first frame
/// takes the introduction grid; each later frame settles every connected
/// component at the centroid of where its own atoms just were, relaxed
/// apart where footprints would overlap. Reacting partners therefore drift
/// toward their meeting point, products settle where their ingredients met,
/// split ions peel away from their parent, and chapter boundaries never
/// teleport anything.
#[must_use]
pub fn home_timeline(frames: &[SimulationFrame]) -> Vec<BTreeMap<String, Point>> {
    let bounds = virtual_rectangle();
    let mut result: Vec<BTreeMap<String, Point>> = Vec::with_capacity(frames.len());
    for frame in frames {
        let render = RenderFrame::from(frame);
        let homes = match result.last() {
            None => layout(&render, bounds),
            Some(previous) => flow_layout(&render, previous, bounds),
        };
        result.push(homes);
    }
    result
}

/// One flow step: components inherit the centroid of their atoms' previous
/// homes, then whole components relax apart until no footprints overlap.
#[allow(clippy::cast_precision_loss)]
fn flow_layout(
    frame: &StructuralFrame,
    previous: &BTreeMap<String, Point>,
    bounds: Rectangle,
) -> BTreeMap<String, Point> {
    let components = connected_components(frame);
    let fallback = Point::new(bounds.width * 0.5, bounds.height * 0.5);
    let mut positions = BTreeMap::new();
    let mut centers = Vec::with_capacity(components.len());
    for component in &components {
        let known = component
            .iter()
            .filter_map(|id| previous.get(id))
            .collect::<Vec<_>>();
        let center = if known.is_empty() {
            fallback
        } else {
            Point::new(
                known.iter().map(|point| point.x).sum::<f32>() / known.len() as f32,
                known.iter().map(|point| point.y).sum::<f32>() / known.len() as f32,
            )
        };
        layout_component(frame, component, center, 0.0, &mut positions);
        centers.push(center);
    }
    let radii = components
        .iter()
        .zip(&centers)
        .map(|(component, center)| {
            component
                .iter()
                .map(|id| {
                    let reach = positions
                        .get(id)
                        .map_or(0.0, |point| vector_magnitude(*point - *center));
                    let radius =
                        atom(frame, id).map_or(24.0, |state| atom_visual_radius(&state.element));
                    reach + radius
                })
                .fold(0.0_f32, f32::max)
                + 30.0
        })
        .collect::<Vec<_>>();

    // Relax overlapping components apart. Coincident centers (a fresh
    // dissociation) separate along a stable per-pair angle so the push is
    // deterministic.
    let mut offsets = vec![Vector::new(0.0, 0.0); components.len()];
    for _ in 0..48 {
        let mut moved = false;
        for first in 0..components.len() {
            for second in (first + 1)..components.len() {
                let delta = centers[second] - centers[first];
                let distance = vector_magnitude(delta);
                let required = radii[first] + radii[second];
                if distance >= required {
                    continue;
                }
                let direction = if distance < 1.0 {
                    let angle = -std::f32::consts::FRAC_PI_2
                        + std::f32::consts::TAU * second as f32 / components.len().max(1) as f32;
                    Vector::new(angle.cos(), angle.sin())
                } else {
                    delta * (1.0 / distance)
                };
                let push = (required - distance) * 0.5 + 0.5;
                centers[first] -= direction * push;
                centers[second] += direction * push;
                offsets[first] -= direction * push;
                offsets[second] += direction * push;
                moved = true;
            }
        }
        if !moved {
            break;
        }
    }
    // Keep every footprint inside the world.
    for (index, radius) in radii.iter().enumerate() {
        let margin_x = radius.min(bounds.width * 0.5);
        let margin_y = radius.min(bounds.height * 0.5);
        let clamped = Point::new(
            centers[index].x.clamp(margin_x, bounds.width - margin_x),
            centers[index].y.clamp(margin_y, bounds.height - margin_y),
        );
        offsets[index] += clamped - centers[index];
        centers[index] = clamped;
    }
    for (component, offset) in components.iter().zip(&offsets) {
        if offset.x == 0.0 && offset.y == 0.0 {
            continue;
        }
        for id in component {
            if let Some(position) = positions.get_mut(id) {
                *position += *offset;
            }
        }
    }
    positions
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
        link_ionic_components(association, frame, &mut link);
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
    charge_ordered_components(frame, components)
}

fn charge_ordered_components(
    frame: &StructuralFrame,
    components: Vec<Vec<String>>,
) -> Vec<Vec<String>> {
    let mut positive = VecDeque::new();
    let mut negative = VecDeque::new();
    let mut neutral = Vec::new();
    for component in components {
        match component_charge(frame, &component).signum() {
            1 => positive.push_back(component),
            -1 => negative.push_back(component),
            _ => neutral.push(component),
        }
    }
    if positive.is_empty() || negative.is_empty() {
        positive.extend(negative);
        positive.extend(neutral);
        return positive.into_iter().collect();
    }

    let mut ordered = Vec::with_capacity(positive.len() + negative.len() + neutral.len());
    let start_positive = positive.len() >= negative.len();
    while !positive.is_empty() || !negative.is_empty() {
        if start_positive {
            if let Some(component) = positive.pop_front() {
                ordered.push(component);
            }
            if let Some(component) = negative.pop_front() {
                ordered.push(component);
            }
        } else {
            if let Some(component) = negative.pop_front() {
                ordered.push(component);
            }
            if let Some(component) = positive.pop_front() {
                ordered.push(component);
            }
        }
    }
    ordered.extend(neutral);
    ordered
}

fn component_charge(frame: &StructuralFrame, component: &[String]) -> i64 {
    component
        .iter()
        .filter_map(|id| atom(frame, id))
        .map(|atom| i64::from(atom.formal_charge))
        .sum()
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
    // Bond lengths follow the atoms' visual radii (matching the physics
    // spring rests), so shells nearly touch across a visible shared pair.
    let radius_of =
        |id: &str| atom(frame, id).map_or(24.0, |state| atom_visual_radius(&state.element));
    let gap = 26.0;
    let _ = cell_extent;
    if component.len() == 2 {
        let span = radius_of(&component[0]) + radius_of(&component[1]) + gap;
        positions.insert(component[0].clone(), center + Vector::new(-span * 0.5, 0.0));
        positions.insert(component[1].clone(), center + Vector::new(span * 0.5, 0.0));
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
            let distance = radius_of(&parent) + radius_of(child) + gap;
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
        link_ionic_components(association, frame, &mut link);
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
        let Some((left_id, right_id, order, effective_order)) = bond else {
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
        let clearance = |id: &str| {
            atom(after, id)
                .or_else(|| atom(before, id))
                .map_or(24.0, |state| atom_visual_radius(&state.element))
                + 3.0
        };
        draw_covalent(
            frame,
            *left,
            *right,
            (clearance(left_id), clearance(right_id)),
            *order,
            reveal,
            alpha * opacity,
            in_before == in_after,
            scale,
        );
        if effective_order.is_some() {
            draw_delocalized_overlay(frame, *left, *right, reveal, alpha * opacity, scale);
        }
    }

    let before_ionic = ionic_map(before);
    let after_ionic = ionic_map(after);
    for key in before_ionic
        .keys()
        .chain(after_ionic.keys())
        .cloned()
        .collect::<BTreeSet<_>>()
    {
        let association = after_ionic.get(&key).or_else(|| before_ionic.get(&key));
        let Some(association) = association else {
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
        for (left, right) in ionic_component_pairs(association) {
            let (Some(left), Some(right)) = (
                ionic_component_position(&association.components[left], after, positions),
                ionic_component_position(&association.components[right], after, positions),
            ) else {
                continue;
            };
            draw_ionic(frame, left, right, reveal, opacity, scale);
        }
    }
}

type CovalentMap = BTreeMap<String, (String, String, u8, Option<(u8, u8)>)>;

fn covalent_map(frame: &StructuralFrame) -> CovalentMap {
    frame
        .covalent_bonds
        .iter()
        .map(|bond| {
            let (left, right) = sorted_pair(&bond.left, &bond.right);
            (
                format!("{left}|{right}|{}|{:?}", bond.order, bond.effective_order),
                (left, right, bond.order, bond.effective_order),
            )
        })
        .collect()
}

fn ionic_map(frame: &StructuralFrame) -> BTreeMap<String, RenderIonicAssociation> {
    frame
        .ionic_associations
        .iter()
        .map(|association| (association.id.clone(), association.clone()))
        .collect()
}

fn link_ionic_components(
    association: &RenderIonicAssociation,
    frame: &StructuralFrame,
    link: &mut impl FnMut(&str, &str),
) {
    for component in &association.components {
        if let Some(first) = component.atoms.first() {
            for atom in component.atoms.iter().skip(1) {
                link(first, atom);
            }
        }
    }
    let anchors = association
        .components
        .iter()
        .map(|component| ionic_anchor_id(component, frame))
        .collect::<Vec<_>>();
    for (left, right) in ionic_component_pairs(association) {
        if let (Some(left), Some(right)) = (anchors[left], anchors[right]) {
            link(left, right);
        }
    }
}

/// Returns a deterministic, connected bipartite topology for an ionic
/// association. Catalogue component order is an identity concern, not a
/// presentation topology: linking adjacent records could place cation beside
/// cation and anion beside anion. This tree alternates charge signs and then
/// attaches any surplus ions only to oppositely charged components.
fn ionic_component_pairs(association: &RenderIonicAssociation) -> Vec<(usize, usize)> {
    let positive = association
        .components
        .iter()
        .enumerate()
        .filter_map(|(index, component)| (component.charge > 0).then_some(index))
        .collect::<Vec<_>>();
    let negative = association
        .components
        .iter()
        .enumerate()
        .filter_map(|(index, component)| (component.charge < 0).then_some(index))
        .collect::<Vec<_>>();
    if positive.is_empty() || negative.is_empty() {
        return Vec::new();
    }

    let shared = positive.len().min(negative.len());
    let mut pairs = Vec::with_capacity(positive.len() + negative.len() - 1);
    for index in 0..shared {
        pairs.push((positive[index], negative[index]));
        if index + 1 < shared {
            pairs.push((negative[index], positive[index + 1]));
        }
    }
    for (offset, component) in positive.iter().copied().skip(shared).enumerate() {
        pairs.push((component, negative[offset % negative.len()]));
    }
    for (offset, component) in negative.iter().copied().skip(shared).enumerate() {
        pairs.push((component, positive[offset % positive.len()]));
    }
    pairs
}

fn ionic_component_position(
    component: &RenderIonicComponent,
    frame: &StructuralFrame,
    positions: &BTreeMap<String, Point>,
) -> Option<Point> {
    ionic_anchor_id(component, frame)
        .and_then(|anchor| positions.get(anchor).copied())
        .or_else(|| average_position(component.atoms.iter().map(String::as_str), positions))
}

fn ionic_anchor_id<'a>(
    component: &'a RenderIonicComponent,
    frame: &'a StructuralFrame,
) -> Option<&'a str> {
    component
        .atoms
        .iter()
        .filter_map(|id| atom(frame, id).map(|atom| (id.as_str(), atom.formal_charge)))
        .max_by(|left, right| {
            let left_matches = i64::from(left.1.signum()) == component.charge.signum();
            let right_matches = i64::from(right.1.signum()) == component.charge.signum();
            left_matches
                .cmp(&right_matches)
                .then_with(|| left.1.unsigned_abs().cmp(&right.1.unsigned_abs()))
                .then_with(|| right.0.cmp(left.0))
        })
        .map(|(id, _)| id)
}

fn sorted_pair(left: &str, right: &str) -> (String, String) {
    if left <= right {
        (left.to_owned(), right.to_owned())
    } else {
        (right.to_owned(), left.to_owned())
    }
}

fn draw_delocalized_overlay(
    frame: &mut canvas::Frame,
    left: Point,
    right: Point,
    reveal: f32,
    alpha: f32,
    scale: f32,
) {
    let direction = right - left;
    let magnitude = vector_magnitude(direction).max(1.0);
    let along = direction / magnitude;
    let perpendicular = Vector::new(-along.y, along.x);
    let start = left + along * 25.0 * scale + perpendicular * 6.0 * scale;
    let end = right - along * 25.0 * scale + perpendicular * 6.0 * scale;
    for step in 1_u8..=9 {
        let progress = f32::from(step) / 10.0;
        if progress <= reveal {
            frame.fill(
                &Path::circle(lerp_point(start, end, progress), 1.7 * scale),
                ACCENT.scale_alpha(alpha * 0.78),
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_covalent(
    frame: &mut canvas::Frame,
    left: Point,
    right: Point,
    clearances: (f32, f32),
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
    let start = left + along * (clearances.0 * scale);
    let end = right - along * (clearances.1 * scale);
    let midpoint = lerp_point(start, end, 0.5);
    let offsets: &[f32] = match order {
        1 => &[0.0],
        2 => &[-4.0, 4.0],
        _ => &[-6.0, 0.0, 6.0],
    };
    // Shared pairs fade in once the growing bond has room for them.
    let electron_alpha = ((reveal - 0.45) / 0.25).clamp(0.0, 1.0);
    for offset in offsets {
        let visible_start = lerp_point(midpoint, start, reveal);
        let visible_end = lerp_point(midpoint, end, reveal);
        frame.stroke(
            &Path::line(
                visible_start + perpendicular * *offset * scale,
                visible_end + perpendicular * *offset * scale,
            ),
            Stroke::default()
                .with_color(chemistry_color::COVALENT.scale_alpha(alpha * 0.96))
                .with_width(2.8 * scale),
        );
        if show_electrons && electron_alpha > 0.0 {
            let electron_center = midpoint + perpendicular * *offset * scale;
            frame.fill(
                &Path::circle(electron_center - along * 4.0 * scale, 2.3 * scale),
                Color::WHITE.scale_alpha(alpha * electron_alpha),
            );
            frame.fill(
                &Path::circle(electron_center + along * 4.0 * scale, 2.3 * scale),
                Color::WHITE.scale_alpha(alpha * electron_alpha),
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
    operations: &[StructuralOperation],
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
        let bond_angles = atom_bond_angles(id, before, after, positions, position);
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
            operations,
            id,
            position,
            progress,
            ambient_progress,
            alpha,
            scale,
            &bond_angles,
        );
        draw_charge_transition(
            frame,
            before_atom,
            after_atom,
            position,
            progress,
            alpha,
            scale,
            &bond_angles,
        );
    }
}

/// Directions of every covalent bond touching an atom in either frame.
fn atom_bond_angles(
    id: &str,
    before: &StructuralFrame,
    after: &StructuralFrame,
    positions: &BTreeMap<String, Point>,
    center: Point,
) -> Vec<f32> {
    let mut angles = Vec::new();
    for bond in before.covalent_bonds.iter().chain(&after.covalent_bonds) {
        let other = if bond.left == id {
            &bond.right
        } else if bond.right == id {
            &bond.left
        } else {
            continue;
        };
        if let Some(neighbour) = positions.get(other) {
            let delta = *neighbour - center;
            if delta.x.abs() > f32::EPSILON || delta.y.abs() > f32::EPSILON {
                angles.push(delta.y.atan2(delta.x));
            }
        }
    }
    angles
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
    let radius = atom_visual_radius(&atom.element) * (if active { 1.12 } else { 1.0 }) * scale;
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
        color::SHADOW.scale_alpha(alpha * 0.24),
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
        color: color::CANVAS.scale_alpha(alpha),
        size: iced::Pixels(15.0 * scale),
        align_x: iced::alignment::Horizontal::Center.into(),
        align_y: iced::alignment::Vertical::Center,
        font: fonts::REGULAR,
        ..canvas::Text::default()
    });
}

#[allow(clippy::too_many_arguments)]
fn draw_electron_transition(
    frame: &mut canvas::Frame,
    before: Option<&AtomState>,
    after: Option<&AtomState>,
    operations: &[StructuralOperation],
    atom_id: &str,
    center: Point,
    progress: f32,
    phase: f32,
    alpha: f32,
    scale: f32,
    bond_angles: &[f32],
) {
    let delta = electron_state_delta(before, after);
    if operation_moves_atom_electrons(operations, atom_id) {
        let before_positions = before.map_or_else(Vec::new, |atom| {
            electron_positions(center, atom, phase, scale, bond_angles)
        });
        let after_positions = after.map_or_else(Vec::new, |atom| {
            electron_positions(center, atom, phase, scale, bond_angles)
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
            draw_electrons(frame, center, atom, phase, alpha, scale, bond_angles);
        }
    } else {
        if let Some(atom) = before {
            draw_electrons(
                frame,
                center,
                atom,
                phase,
                alpha * (1.0 - progress),
                scale,
                bond_angles,
            );
        }
        if let Some(atom) = after {
            draw_electrons(
                frame,
                center,
                atom,
                phase,
                alpha * progress,
                scale,
                bond_angles,
            );
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

fn operation_moves_atom_electrons(operations: &[StructuralOperation], atom_id: &str) -> bool {
    operations.iter().any(|operation| match operation {
        StructuralOperation::TransferMetallicElectron { acceptor, .. } => acceptor == atom_id,
        StructuralOperation::CleaveCovalent { left, right, .. }
        | StructuralOperation::FormCovalent { left, right, .. } => {
            left == atom_id || right == atom_id
        }
        StructuralOperation::AssociateIonic { .. }
        | StructuralOperation::AssignProduct { .. }
        | StructuralOperation::Other { .. } => false,
    })
}

fn draw_electrons(
    frame: &mut canvas::Frame,
    center: Point,
    atom: &AtomState,
    phase: f32,
    alpha: f32,
    scale: f32,
    bond_angles: &[f32],
) {
    for position in electron_positions(center, atom, phase, scale, bond_angles) {
        draw_electron_dot(frame, position, alpha, scale);
    }
}

/// Angular gaps between a set of bond directions, largest first, as
/// (start angle, span). No bonds yields one full-circle gap.
fn angular_gaps(bond_angles: &[f32]) -> Vec<(f32, f32)> {
    if bond_angles.is_empty() {
        return vec![(-std::f32::consts::FRAC_PI_2, std::f32::consts::TAU)];
    }
    let mut angles = bond_angles
        .iter()
        .map(|angle| angle.rem_euclid(std::f32::consts::TAU))
        .collect::<Vec<_>>();
    angles.sort_by(f32::total_cmp);
    let mut gaps = Vec::with_capacity(angles.len());
    for (index, start) in angles.iter().enumerate() {
        let end = angles
            .get(index + 1)
            .copied()
            .unwrap_or(angles.first().copied().unwrap_or(0.0) + std::f32::consts::TAU);
        gaps.push((*start, (end - start).max(0.0)));
    }
    gaps.sort_by(|left, right| right.1.total_cmp(&left.1));
    gaps
}

#[allow(clippy::cast_precision_loss)]
fn electron_positions(
    center: Point,
    atom: &AtomState,
    phase: f32,
    scale: f32,
    bond_angles: &[f32],
) -> Vec<Point> {
    let radius = (atom_visual_radius(&atom.element) + 8.0) * scale;
    let drift = (phase * std::f32::consts::TAU * 0.45).sin() * 0.055;
    let occupancies =
        electron_domain_occupancies(atom.non_bonding_electrons, atom.unpaired_electrons);
    // Domains spread across the largest bond-free arc so dots never sit on
    // a bond line; with no bonds they keep the classic four-way cross.
    let gaps = angular_gaps(bond_angles);
    let (gap_start, gap_span) = gaps
        .first()
        .copied()
        .unwrap_or((0.0, std::f32::consts::TAU));
    let domain_count = occupancies.len().max(1) as f32;
    let mut positions = Vec::with_capacity(usize::from(atom.non_bonding_electrons.min(8)));
    for (domain, occupancy) in occupancies.into_iter().enumerate() {
        let base_angle = if bond_angles.is_empty() {
            std::f32::consts::FRAC_PI_2 * domain as f32 + drift
        } else {
            gap_start + gap_span * (domain as f32 + 1.0) / (domain_count + 1.0) + drift
        };
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

#[allow(clippy::too_many_arguments)]
fn draw_charge_transition(
    frame: &mut canvas::Frame,
    before: Option<&AtomState>,
    after: Option<&AtomState>,
    center: Point,
    progress: f32,
    alpha: f32,
    scale: f32,
    bond_angles: &[f32],
) {
    let element = after.or(before).map_or("O", |atom| atom.element.as_str());
    // The badge sits just off the atom rim, in the second-largest bond-free
    // gap so it stays clear of both bonds and the electron arc.
    let gaps = angular_gaps(bond_angles);
    let angle = gaps.get(1).or_else(|| gaps.first()).map_or(
        -std::f32::consts::FRAC_PI_4,
        |(start, span)| {
            if bond_angles.is_empty() {
                -std::f32::consts::FRAC_PI_4
            } else if gaps.len() > 1 {
                start + span * 0.5
            } else {
                start + span * 0.16
            }
        },
    );
    let offset = (atom_visual_radius(element) + 6.0) * scale;
    let badge = center + Vector::new(angle.cos() * offset, angle.sin() * offset);
    let before_charge = before.map_or(0, |atom| atom.formal_charge);
    let after_charge = after.map_or(0, |atom| atom.formal_charge);
    if before_charge == after_charge {
        draw_charge(frame, badge, after_charge, alpha, scale);
    } else {
        draw_charge(frame, badge, before_charge, alpha * (1.0 - progress), scale);
        draw_charge(frame, badge, after_charge, alpha * progress, scale);
    }
}

fn draw_charge(frame: &mut canvas::Frame, badge: Point, charge: i16, alpha: f32, scale: f32) {
    let Some(label) = charge_label(charge) else {
        return;
    };
    frame.fill(
        &Path::circle(badge, 9.0 * scale),
        color::CANVAS.scale_alpha(alpha),
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
        font: fonts::REGULAR,
        ..canvas::Text::default()
    });
}

fn charge_label(charge: i16) -> Option<String> {
    match charge {
        0 => None,
        1 => Some("+".to_owned()),
        -1 => Some("−".to_owned()),
        value if value > 0 => Some(format!("{value}+")),
        value => Some(format!("{}−", value.unsigned_abs())),
    }
}

#[allow(clippy::too_many_arguments, clippy::cast_precision_loss)]
fn draw_metallic_transition(
    frame: &mut canvas::Frame,
    before: &StructuralFrame,
    after: &StructuralFrame,
    positions: &BTreeMap<String, Point>,
    operations: &[StructuralOperation],
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
            .filter_map(|site| {
                let position = positions.get(site).copied()?;
                let radius = atom(after, site)
                    .or_else(|| atom(before, site))
                    .map_or(24.0, |state| atom_visual_radius(&state.element));
                Some((position, radius))
            })
            .collect::<Vec<_>>();
        if sites.is_empty() {
            continue;
        }
        // The electron sea hugs its sites as a soft halo: a circle around a
        // lone site, a stadium around a row of them. Delocalized electrons
        // orbit the halo ring itself.
        let reach = sites
            .iter()
            .map(|(_, radius)| radius + 22.0)
            .fold(0.0_f32, f32::max)
            * scale;
        let center = Point::new(
            sites.iter().map(|(site, _)| site.x).sum::<f32>() / sites.len() as f32,
            sites.iter().map(|(site, _)| site.y).sum::<f32>() / sites.len() as f32,
        );
        let spread = sites
            .iter()
            .map(|(site, _)| vector_magnitude(*site - center))
            .fold(0.0_f32, f32::max);
        let halo_radius = spread + reach;
        let path = Path::circle(center, halo_radius);
        frame.fill(&path, ACCENT.scale_alpha(alpha * 0.07));
        frame.stroke(
            &path,
            Stroke::default()
                .with_color(ACCENT.scale_alpha(alpha * 0.38))
                .with_width(1.2 * scale),
        );
        let active_transfer = operations.iter().any(|operation| {
            matches!(
                operation,
                StructuralOperation::TransferMetallicElectron { domain, .. } if domain == id
            )
        });
        let stationary_electrons = if active_transfer {
            after_domain.map_or(0, |domain| domain.delocalized_electrons)
        } else {
            domain.delocalized_electrons
        };
        for electron in 0..stationary_electrons {
            let angle = (phase * 0.16
                + f32::from(electron) / f32::from(stationary_electrons.max(1)))
            .fract()
                * std::f32::consts::TAU;
            let electron_position =
                center + Vector::new(angle.cos() * halo_radius, angle.sin() * halo_radius);
            frame.fill(
                &Path::circle(electron_position, 3.0 * scale),
                Color::WHITE.scale_alpha(alpha * 0.94),
            );
            frame.fill(
                &Path::circle(electron_position, 7.0 * scale),
                ACCENT.scale_alpha(alpha * 0.12),
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_operation_motion(
    frame: &mut canvas::Frame,
    before: &StructuralFrame,
    after: &StructuralFrame,
    operations: &[StructuralOperation],
    positions: &BTreeMap<String, Point>,
    progress: f32,
    phase: f32,
    scale: f32,
) {
    for operation in operations {
        match operation {
            StructuralOperation::TransferMetallicElectron {
                donor_site,
                acceptor,
                count,
                ..
            } => draw_metallic_electron_transfer(
                frame, before, after, donor_site, acceptor, *count, positions, progress, phase,
                scale,
            ),
            operation @ (StructuralOperation::CleaveCovalent { .. }
            | StructuralOperation::FormCovalent { .. }) => {
                draw_covalent_electron_motion(frame, operation, positions, progress, phase, scale);
            }
            StructuralOperation::AssociateIonic { .. }
            | StructuralOperation::AssignProduct { .. }
            | StructuralOperation::Other { .. } => {}
        }
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
    let targets = electron_positions(*acceptor_center, after_acceptor, phase, scale, &[])
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
    operation: &StructuralOperation,
    positions: &BTreeMap<String, Point>,
    progress: f32,
    phase: f32,
    scale: f32,
) {
    let (left, right, order, forming, endpoint_states) = match operation {
        StructuralOperation::FormCovalent {
            left,
            right,
            order,
            endpoint_states,
        } => (left.as_str(), right.as_str(), *order, true, endpoint_states),
        StructuralOperation::CleaveCovalent {
            left,
            right,
            order,
            endpoint_states,
        } => (
            left.as_str(),
            right.as_str(),
            *order,
            false,
            endpoint_states,
        ),
        _ => return,
    };
    let (Some(left_center), Some(right_center)) = (positions.get(left), positions.get(right))
    else {
        return;
    };
    let [
        Some(before_left),
        Some(after_left),
        Some(before_right),
        Some(after_right),
    ] = endpoint_states
    else {
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
    electron_positions(center, atom, phase, scale, &[])
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
            .with_color(chemistry_color::ELECTRON.scale_alpha(0.36 + progress * 0.22))
            .with_width(1.6 * scale),
    );
    let moving = quadratic_point(start, control, end, progress);
    frame.fill(
        &Path::circle(moving, 8.0 * scale),
        chemistry_color::ELECTRON.scale_alpha(0.18),
    );
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
        font: fonts::REGULAR,
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
            font: fonts::REGULAR,
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
            font: fonts::REGULAR,
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
    if label.kind == ExplanationLabelKind::ObservationExplanation {
        return;
    }
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
        font: fonts::REGULAR,
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
            font: fonts::REGULAR,
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
        color::SHADOW.scale_alpha(alpha * 0.24),
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
        ExplanationLabelKind::SummaryExplanation => chemistry_color::SUMMARY,
        ExplanationLabelKind::ConceptExplanation
        | ExplanationLabelKind::StructuralChangeExplanation => ACCENT,
    }
}

fn explanation_title(kind: ExplanationLabelKind) -> &'static str {
    match kind {
        ExplanationLabelKind::ConceptExplanation => "KEY CONCEPT",
        ExplanationLabelKind::StructuralChangeExplanation => "WHAT CHANGED",
        ExplanationLabelKind::ObservationExplanation => "OBSERVATION",
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

fn active_atoms(operations: &[RenderOperation]) -> BTreeSet<&str> {
    operations
        .iter()
        .flat_map(|operation| match operation {
            RenderOperation::TransferMetallicElectron {
                donor_site,
                acceptor,
                ..
            } => vec![donor_site.as_str(), acceptor.as_str()],
            RenderOperation::CleaveCovalent { left, right, .. }
            | RenderOperation::FormCovalent { left, right, .. } => {
                vec![left.as_str(), right.as_str()]
            }
            RenderOperation::AssociateIonic { atoms }
            | RenderOperation::AssignProduct { atoms }
            | RenderOperation::Other { atoms } => atoms.iter().map(String::as_str).collect(),
        })
        .collect()
}

fn element_color(symbol: &str) -> Color {
    match symbol {
        "H" => LAB_DARK.chemistry.hydrogen,
        "Li" => LAB_DARK.chemistry.lithium,
        "Ag" => LAB_DARK.chemistry.silver,
        "Cl" => LAB_DARK.chemistry.chlorine,
        "Na" => LAB_DARK.chemistry.sodium,
        "N" => LAB_DARK.chemistry.nitrogen,
        "O" => LAB_DARK.chemistry.oxygen,
        _ => LAB_DARK.chemistry.element_default,
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
    fn ionic_association_anchors_to_the_charged_atom_in_a_polyatomic_component() {
        let frame = RenderFrame {
            atoms: vec![
                RenderAtom {
                    id: "li".to_owned(),
                    element: "Li".to_owned(),
                    formal_charge: 1,
                    non_bonding_electrons: 0,
                    unpaired_electrons: 0,
                },
                RenderAtom {
                    id: "o".to_owned(),
                    element: "O".to_owned(),
                    formal_charge: -1,
                    non_bonding_electrons: 6,
                    unpaired_electrons: 0,
                },
                RenderAtom {
                    id: "h".to_owned(),
                    element: "H".to_owned(),
                    formal_charge: 0,
                    non_bonding_electrons: 0,
                    unpaired_electrons: 0,
                },
            ],
            covalent_bonds: Vec::new(),
            ionic_associations: Vec::new(),
            metallic_domains: Vec::new(),
        };
        let hydroxide = RenderIonicComponent {
            atoms: vec!["h".to_owned(), "o".to_owned()],
            charge: -1,
        };

        assert_eq!(ionic_anchor_id(&hydroxide, &frame), Some("o"));
    }

    #[test]
    fn ionic_layout_uses_opposite_charge_topology_in_every_frame() {
        let atoms = [
            ("mn1", "Mn", 3),
            ("mn2", "Mn", 3),
            ("o1", "O", -2),
            ("o2", "O", -2),
            ("o3", "O", -2),
        ]
        .into_iter()
        .map(|(id, element, formal_charge)| RenderAtom {
            id: id.to_owned(),
            element: element.to_owned(),
            formal_charge,
            non_bonding_electrons: if formal_charge < 0 { 8 } else { 5 },
            unpaired_electrons: if formal_charge < 0 { 0 } else { 5 },
        })
        .collect();
        let association = RenderIonicAssociation {
            id: "manganese-oxide".to_owned(),
            components: vec![
                RenderIonicComponent {
                    atoms: vec!["mn1".to_owned()],
                    charge: 3,
                },
                RenderIonicComponent {
                    atoms: vec!["mn2".to_owned()],
                    charge: 3,
                },
                RenderIonicComponent {
                    atoms: vec!["o1".to_owned()],
                    charge: -2,
                },
                RenderIonicComponent {
                    atoms: vec!["o2".to_owned()],
                    charge: -2,
                },
                RenderIonicComponent {
                    atoms: vec!["o3".to_owned()],
                    charge: -2,
                },
            ],
        };
        let frame = RenderFrame {
            atoms,
            covalent_bonds: Vec::new(),
            ionic_associations: vec![association.clone()],
            metallic_domains: Vec::new(),
        };

        let pairs = ionic_component_pairs(&association);
        assert_eq!(pairs.len(), association.components.len() - 1);
        assert!(pairs.iter().all(|(left, right)| {
            association.components[*left].charge.signum()
                != association.components[*right].charge.signum()
        }));
        assert_eq!(connected_components(&frame).len(), 1);

        let bounds = Rectangle::new(Point::ORIGIN, Size::new(900.0, 600.0));
        let positions = layout(&frame, bounds);
        assert!(vector_magnitude(positions["mn2"] - positions["mn1"]) > 50.0);

        let disconnected = RenderFrame {
            atoms: frame.atoms.clone(),
            covalent_bonds: Vec::new(),
            ionic_associations: Vec::new(),
            metallic_domains: Vec::new(),
        };
        let disconnected_components = connected_components(&disconnected);
        let charge_signs = disconnected_components
            .iter()
            .map(|component| component_charge(&disconnected, component).signum())
            .collect::<Vec<_>>();
        assert_eq!(charge_signs, vec![-1, 1, -1, 1, -1]);

        let previous = layout(&disconnected, bounds);
        let transition_after = flow_layout(&frame, &previous, bounds);
        assert!(vector_magnitude(transition_after["mn2"] - transition_after["mn1"]) > 50.0);
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
    fn flow_layout_meets_in_the_middle_and_separates_splits() {
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(1600.0, 900.0));
        let bond = |left: &str, right: &str| RenderBond {
            left: left.to_owned(),
            right: right.to_owned(),
            order: 1,
            effective_order: None,
        };

        // Two lone atoms far apart combine: the product settles at their
        // midpoint, so both partners drift toward the meeting point.
        let combined = RenderFrame {
            atoms: vec![atom_state("a", 0, 0), atom_state("b", 0, 0)],
            covalent_bonds: vec![bond("a", "b")],
            ionic_associations: Vec::new(),
            metallic_domains: Vec::new(),
        };
        let previous: BTreeMap<String, Point> = [
            ("a".to_owned(), Point::new(200.0, 450.0)),
            ("b".to_owned(), Point::new(1000.0, 450.0)),
        ]
        .into();
        let homes = flow_layout(&combined, &previous, bounds);
        let centroid = Point::new(
            f32::midpoint(homes["a"].x, homes["b"].x),
            f32::midpoint(homes["a"].y, homes["b"].y),
        );
        assert!(
            (centroid.x - 600.0).abs() < 1.0 && (centroid.y - 450.0).abs() < 1.0,
            "product settles where the partners meet: {centroid:?}"
        );

        // The reverse split: coincident fragments relax apart until their
        // footprints no longer overlap.
        let split = RenderFrame {
            atoms: vec![atom_state("a", 0, 0), atom_state("b", 0, 0)],
            covalent_bonds: Vec::new(),
            ionic_associations: Vec::new(),
            metallic_domains: Vec::new(),
        };
        let together: BTreeMap<String, Point> = [
            ("a".to_owned(), Point::new(600.0, 450.0)),
            ("b".to_owned(), Point::new(600.0, 450.0)),
        ]
        .into();
        let separated = flow_layout(&split, &together, bounds);
        assert!(
            vector_magnitude(separated["a"] - separated["b"]) >= 96.0,
            "split fragments relax apart: {separated:?}"
        );
    }

    #[test]
    fn crowded_diatomic_layout_keeps_multiple_bonds_visible() {
        let frame = RenderFrame {
            atoms: vec![atom_state("left", 2, 0), atom_state("right", 2, 0)],
            covalent_bonds: vec![RenderBond {
                left: "left".to_owned(),
                right: "right".to_owned(),
                order: 3,
                effective_order: None,
            }],
            ionic_associations: Vec::new(),
            metallic_domains: Vec::new(),
        };
        let mut positions = BTreeMap::new();
        layout_component(
            &frame,
            &["left".to_owned(), "right".to_owned()],
            Point::new(100.0, 100.0),
            120.0,
            &mut positions,
        );

        let distance = vector_magnitude(positions["right"] - positions["left"]);
        assert!(distance >= 74.0);
        assert!(
            distance - 52.0 >= 22.0,
            "the bond must clear both atom shells"
        );
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
