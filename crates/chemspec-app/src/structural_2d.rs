//! Educational 2D rendering of trusted structural frames.
//!
//! This module performs deterministic presentation layout only. It never
//! parses source, resolves catalogue rules, or infers a chemical relationship.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use chem_domain::StructuralOperationView;
use chem_kernel::SimulationFrame;
use chem_presentation::{
    ContextLabel, EducationalPlan, EducationalSceneKind, ExplanationLabel, ExplanationLabelKind,
    ProtonTransferInterpretation,
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
const CANVAS: Color = chemistry_color::STRUCTURAL_CANVAS;
const PANEL: Color = chemistry_color::STRUCTURAL_PANEL;
const TEXT: Color = color::TEXT;
const TEXT_SOFT: Color = color::TEXT_SOFT;

#[derive(Debug, Clone)]
pub struct SceneContext {
    kind: EducationalSceneKind,
    equation: Option<String>,
    electricity: bool,
}

impl SceneContext {
    pub fn new(kind: EducationalSceneKind, _index: usize, _total: usize) -> Self {
        Self {
            kind,
            equation: None,
            electricity: false,
        }
    }

    pub fn with_equation(mut self, equation: Option<String>) -> Self {
        self.equation = equation;
        self
    }

    pub fn with_electricity(mut self, electricity: bool) -> Self {
        self.electricity = electricity;
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
    ProtonTransfer {
        hydrogen: String,
        donor: String,
        acceptor: String,
    },
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
    /// A site entering (`joining`) or leaving the shared metallic domain,
    /// with its electrons travelling between the halo ring and its shell.
    MetallicMembership {
        domain: String,
        site: String,
        joining: bool,
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
            .collect::<Vec<_>>();
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
            .collect::<Vec<_>>();
        let ionic_associations = frame
            .ionic_associations()
            .values()
            .map(|association| RenderIonicAssociation {
                id: association.id.as_str().to_owned(),
                components: association
                    .components
                    .iter()
                    .flat_map(|(group, component_atoms)| {
                        split_render_ionic_component(
                            &atoms,
                            &covalent_bonds,
                            component_atoms
                                .iter()
                                .map(|atom| atom.as_str().to_owned())
                                .collect(),
                            association
                                .component_charges
                                .get(group)
                                .copied()
                                .unwrap_or(0),
                        )
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

/// Expands an aggregate catalogue component into its covalently connected
/// charged fragments for presentation. Some reviewed templates group repeated
/// monatomic cations (for example both K+ sites in K2CO3) into one +2 group;
/// treating that group as one visual ion leaves all but one site unanchored.
/// Neutral or charge-inconsistent fragments retain the validated aggregate so
/// presentation never invents ionic membership from an unsafe partition.
fn split_render_ionic_component(
    atoms: &[RenderAtom],
    covalent_bonds: &[RenderBond],
    mut component_atoms: Vec<String>,
    aggregate_charge: i64,
) -> Vec<RenderIonicComponent> {
    component_atoms.sort();
    let members = component_atoms.iter().cloned().collect::<BTreeSet<_>>();
    let mut adjacency = component_atoms
        .iter()
        .map(|atom| (atom.clone(), BTreeSet::new()))
        .collect::<BTreeMap<_, _>>();
    for bond in covalent_bonds {
        if members.contains(&bond.left) && members.contains(&bond.right) {
            if let Some(neighbours) = adjacency.get_mut(&bond.left) {
                neighbours.insert(bond.right.clone());
            }
            if let Some(neighbours) = adjacency.get_mut(&bond.right) {
                neighbours.insert(bond.left.clone());
            }
        }
    }

    let charges = atoms
        .iter()
        .map(|atom| (atom.id.as_str(), i64::from(atom.formal_charge)))
        .collect::<BTreeMap<_, _>>();
    let mut visited = BTreeSet::new();
    let mut fragments = Vec::new();
    for seed in &component_atoms {
        if !visited.insert(seed.clone()) {
            continue;
        }
        let mut queue = VecDeque::from([seed.clone()]);
        let mut fragment = Vec::new();
        while let Some(current) = queue.pop_front() {
            fragment.push(current.clone());
            for neighbour in &adjacency[&current] {
                if visited.insert(neighbour.clone()) {
                    queue.push_back(neighbour.clone());
                }
            }
        }
        fragment.sort();
        let charge = fragment
            .iter()
            .map(|atom| charges.get(atom.as_str()).copied().unwrap_or(0))
            .sum();
        fragments.push(RenderIonicComponent {
            atoms: fragment,
            charge,
        });
    }

    let partition_is_ionic = fragments.len() > 1
        && fragments.iter().all(|fragment| fragment.charge != 0)
        && fragments
            .iter()
            .map(|fragment| fragment.charge)
            .sum::<i64>()
            == aggregate_charge;
    if partition_is_ionic {
        fragments
    } else {
        vec![RenderIonicComponent {
            atoms: component_atoms,
            charge: aggregate_charge,
        }]
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
        StructuralOperationView::ReleaseMetallic { site, domain, .. } => {
            RenderOperation::MetallicMembership {
                domain: domain.as_str().to_owned(),
                site: site.as_str().to_owned(),
                joining: false,
            }
        }
        StructuralOperationView::JoinMetallic { site, domain, .. } => {
            RenderOperation::MetallicMembership {
                domain: domain.as_str().to_owned(),
                site: site.as_str().to_owned(),
                joining: true,
            }
        }
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
/// structural motion into the first 75% of the scene.
#[must_use]
pub fn scene_action(kind: EducationalSceneKind, has_explanation: bool, progress: f32) -> f32 {
    let structural = if kind == EducationalSceneKind::StructuralChange && has_explanation {
        (progress / 0.75).clamp(0.0, 1.0)
    } else {
        progress
    };
    animation_phase(structural)
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

/// Camera rectangle for one scene: the before-layout fit blended toward the
/// after-layout fit by the scene's structural progress — home layouts only,
/// never the live physics positions, so user drags and mid-scene jitter
/// cannot make the camera chase. Union framing left every travelling scene
/// swimming in dead space; the blend keeps the subject filling the shot.
#[must_use]
pub fn chapter_camera(
    before: &SimulationFrame,
    after: &SimulationFrame,
    before_homes: &BTreeMap<String, Point>,
    after_homes: &BTreeMap<String, Point>,
    progress: f32,
) -> Rectangle {
    let before_frame = RenderFrame::from(before);
    let after_frame = RenderFrame::from(after);
    let mut elements = BTreeMap::new();
    for atom in before_frame.atoms.iter().chain(&after_frame.atoms) {
        elements.insert(atom.id.clone(), atom.element.clone());
    }
    let start = homes_camera(before_homes, &elements);
    let end = homes_camera(after_homes, &elements);
    let blend = progress.clamp(0.0, 1.0);
    Rectangle::new(
        Point::new(
            start.x + (end.x - start.x) * blend,
            start.y + (end.y - start.y) * blend,
        ),
        Size::new(
            start.width + (end.width - start.width) * blend,
            start.height + (end.height - start.height) * blend,
        ),
    )
}

/// Padded fit of one home layout, clamped into the virtual world.
fn homes_camera(homes: &BTreeMap<String, Point>, elements: &BTreeMap<String, String>) -> Rectangle {
    let bounds = virtual_rectangle();
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for (id, point) in homes {
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
pub fn ease_camera(camera: &mut Rectangle, target: Rectangle) -> bool {
    let tolerance = 0.05 * target.width.max(target.height);
    if (camera.x - target.x).abs() < tolerance
        && (camera.y - target.y).abs() < tolerance
        && (camera.width - target.width).abs() < tolerance
        && (camera.height - target.height).abs() < tolerance
    {
        return false;
    }
    // 33ms ticks: settles in roughly 1.3 seconds, no overshoot.
    let ease = 0.075;
    camera.x += (target.x - camera.x) * ease;
    camera.y += (target.y - camera.y) * ease;
    camera.width += (target.width - camera.width) * ease;
    camera.height += (target.height - camera.height) * ease;
    true
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

    // Per-atom element plus formal charge at both endpoints, so the physics
    // can blend electrostatics across the transition.
    let mut atoms = BTreeMap::<String, (String, f32, f32)>::new();
    for atom in &before_frame.atoms {
        let charge = f32::from(atom.formal_charge);
        atoms.insert(atom.id.clone(), (atom.element.clone(), charge, charge));
    }
    for atom in &after_frame.atoms {
        let charge = f32::from(atom.formal_charge);
        atoms
            .entry(atom.id.clone())
            .and_modify(|(_, _, after)| *after = charge)
            .or_insert((atom.element.clone(), charge, charge));
    }
    let radius_of = |id: &str| {
        atoms
            .get(id)
            .map_or(24.0, |(element, ..)| atom_visual_radius(element))
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

    // Ionic partners attract loosely between their charged anchor atoms;
    // same-substance formula units share one lattice topology, so their
    // springs hold the giant ionic structure together.
    let mut add_ionic = |frame: &RenderFrame, weight: f32| {
        for group in lattice_groups(frame) {
            for (left, right) in &group.pairs {
                let (Some(anchor_left), Some(anchor_right)) = (
                    ionic_anchor_id(member_component(frame, group.members[*left]), frame),
                    ionic_anchor_id(member_component(frame, group.members[*right]), frame),
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
        .filter_map(|(id, (element, before_charge, after_charge))| {
            home_of(id).map(|home| AtomSpec {
                id: id.clone(),
                radius: atom_visual_radius(element),
                charge: before_charge + (after_charge - before_charge) * action,
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
        proton_transfers: &[&ProtonTransferInterpretation],
        progress: f32,
        explanation: Option<&ExplanationLabel>,
        context_labels: &[ContextLabel],
        context: SceneContext,
        ambient_progress: f32,
        positions: BTreeMap<String, Point>,
        camera: Rectangle,
    ) -> Self {
        let operations = if proton_transfers.is_empty() {
            operation_transitions
                .iter()
                .filter_map(|(before, after)| {
                    after
                        .active_operation()
                        .map(|active| render_operation(active.operation.view(), before, after))
                })
                .collect()
        } else {
            proton_transfers
                .iter()
                .map(|transfer| RenderOperation::ProtonTransfer {
                    hydrogen: transfer.hydrogen.clone(),
                    donor: transfer.donor.clone(),
                    acceptor: transfer.acceptor.clone(),
                })
                .collect()
        };
        Self {
            camera,
            before: RenderFrame::from(before),
            after: RenderFrame::from(after),
            operations,
            progress: progress.clamp(0.0, 1.0),
            explanation: explanation.cloned(),
            context_labels: context_labels.to_vec(),
            context,
            ambient_progress: ambient_progress.clamp(0.0, 1.0),
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
            (self.progress / 0.75).clamp(0.0, 1.0)
        } else {
            self.progress
        };
        let explanation_progress = if combined_learning_beat {
            ((self.progress - 0.45) / 0.55).clamp(0.0, 1.0)
        } else {
            self.progress
        };
        let action = animation_phase(structural_progress);
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
                shows_transfer_motion(self.context.electricity),
            );
        }

        if matches!(
            self.context.kind,
            EducationalSceneKind::ReactantSetup | EducationalSceneKind::Summary
        ) {
            draw_component_captions(&mut frame, &self.after, &positions, self.progress, scale);
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
            let context = self
                .context_labels
                .iter()
                .find(|label| label.kind == ExplanationLabelKind::StructuralChangeExplanation);
            draw_explanation_label(
                &mut frame,
                explanation,
                context,
                &positions,
                bounds,
                explanation_progress,
                scale,
            );
        }

        vec![frame.into_geometry()]
    }
}

const fn shows_transfer_motion(electricity: bool) -> bool {
    !electricity
}

/// Formula caption beneath every settled component, tying the particulate
/// picture back to the symbolic equation (`H₂`, `LiOH`, `AgCl`). Drawn only
/// in the bookend scenes, where nothing is mid-flight.
fn draw_component_captions(
    frame: &mut canvas::Frame,
    state: &StructuralFrame,
    positions: &BTreeMap<String, Point>,
    progress: f32,
    scale: f32,
) {
    let alpha = smoother_step(((progress - 0.08) / 0.18).clamp(0.0, 1.0)) * 0.85;
    if alpha <= 0.0 {
        return;
    }
    let ionic_members: BTreeSet<&str> = state
        .ionic_associations
        .iter()
        .flat_map(|association| &association.components)
        .flat_map(|component| &component.atoms)
        .map(String::as_str)
        .collect();
    for component in connected_components(state) {
        let mut counts: BTreeMap<&str, u64> = BTreeMap::new();
        let mut min_x = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for id in &component {
            let (Some(atom), Some(position)) = (atom(state, id), positions.get(id)) else {
                continue;
            };
            *counts.entry(atom.element.as_str()).or_default() += 1;
            let radius = atom_visual_radius(&atom.element) * scale;
            min_x = min_x.min(position.x - radius);
            max_x = max_x.max(position.x + radius);
            max_y = max_y.max(position.y + radius);
        }
        if counts.is_empty() || !max_y.is_finite() {
            continue;
        }
        // Ionic lattices caption as the formula unit (Li₂O₂H₂ → LiOH);
        // covalent molecules keep their exact counts (H₂O₂ stays H₂O₂).
        if component
            .iter()
            .any(|id| ionic_members.contains(id.as_str()))
        {
            let divisor = counts.values().copied().fold(0, gcd);
            if divisor > 1 {
                for count in counts.values_mut() {
                    *count /= divisor;
                }
            }
        }
        let formula = chem_domain::conventional_formula(
            counts.iter().map(|(symbol, count)| (*symbol, *count)),
        );
        frame.fill_text(canvas::Text {
            content: crate::nomenclature::display_formula(&formula),
            position: Point::new(f32::midpoint(min_x, max_x), max_y + 14.0 * scale),
            color: TEXT_SOFT.scale_alpha(alpha),
            size: iced::Pixels(15.0 * scale),
            align_x: iced::alignment::Horizontal::Center.into(),
            align_y: iced::alignment::Vertical::Top,
            font: fonts::REGULAR,
            ..canvas::Text::default()
        });
    }
}

fn gcd(mut left: u64, mut right: u64) -> u64 {
    while right != 0 {
        (left, right) = (right, left % right);
    }
    left
}

/// Eased structural-motion progress: a gentle lead-in, then the action
/// spans most of the beat.
fn animation_phase(progress: f32) -> f32 {
    smoother_step(((progress - 0.06) / 0.72).clamp(0.0, 1.0))
}

fn smoother_step(value: f32) -> f32 {
    (value * value * value * (value * (value * 6.0 - 15.0) + 10.0)).clamp(0.0, 1.0)
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
fn draw_atmosphere(frame: &mut canvas::Frame, bounds: Rectangle) {
    frame.fill_rectangle(Point::ORIGIN, bounds.size(), CANVAS);
}

fn layout_ordered(
    frame: &StructuralFrame,
    components: &[Vec<String>],
    bounds: Rectangle,
) -> BTreeMap<String, Point> {
    let mut positions = BTreeMap::new();
    for (component, center) in components
        .iter()
        .zip(component_slots(components.len(), bounds))
    {
        layout_component(frame, component, center, &mut positions);
    }
    positions
}

/// Story-anchored homes for every frame in playback order. The first frame
/// takes the introduction grid; each later frame settles every connected
/// component at the centroid of where its own atoms just were, relaxed
/// apart where footprints would overlap. Components linked by the frame's
/// active operation (or the next frame's, so partners meet before the
/// action starts) are additionally pulled tangent, keeping transfers and
/// forming bonds between neighbours instead of across the canvas. Reacting
/// partners therefore drift toward their meeting point, products settle
/// where their ingredients met, split ions peel away from their parent,
/// and chapter boundaries never teleport anything.
#[must_use]
pub fn home_timeline(frames: &[SimulationFrame]) -> Vec<BTreeMap<String, Point>> {
    let bounds = virtual_rectangle();
    let mut result: Vec<BTreeMap<String, Point>> = Vec::with_capacity(frames.len());
    for (index, frame) in frames.iter().enumerate() {
        let render = RenderFrame::from(frame);
        let homes = match result.last() {
            None => {
                let components = interaction_ordered(connected_components(&render), frames);
                layout_ordered(&render, &components, bounds)
            }
            Some(previous) => {
                let mut linked = affinity_atoms(frame);
                if let Some(next) = frames.get(index + 1) {
                    linked.extend(affinity_atoms(next));
                }
                flow_layout(&render, previous, &linked, bounds)
            }
        };
        result.push(homes);
    }
    result
}

/// Atoms touched by a frame's active operation: the anchors that pull
/// their components together while that operation plays.
fn affinity_atoms(frame: &SimulationFrame) -> BTreeSet<String> {
    frame
        .active_operation()
        .map_or_else(BTreeSet::new, |active| {
            let operation = render_operation(active.operation.view(), frame, frame);
            active_atoms(std::slice::from_ref(&operation))
                .iter()
                .map(|id| (*id).to_owned())
                .collect()
        })
}

/// Seats introduction-grid components so that the pairs which interact
/// earliest in the story sit next to each other, instead of in atom-id
/// discovery order. Partners are found by scanning every frame's active
/// operation for the first one that touches two distinct components.
fn interaction_ordered(
    components: Vec<Vec<String>>,
    frames: &[SimulationFrame],
) -> Vec<Vec<String>> {
    let mut membership = BTreeMap::new();
    for (index, component) in components.iter().enumerate() {
        for id in component {
            membership.insert(id.as_str(), index);
        }
    }
    let mut pairs = Vec::new();
    let mut paired = BTreeSet::new();
    for frame in frames {
        let touched = affinity_atoms(frame)
            .iter()
            .filter_map(|id| membership.get(id.as_str()).copied())
            .collect::<BTreeSet<_>>();
        let touched = touched.into_iter().collect::<Vec<_>>();
        for (slot, first) in touched.iter().enumerate() {
            for second in &touched[slot + 1..] {
                if paired.insert((*first, *second)) {
                    pairs.push((*first, *second));
                }
            }
        }
    }

    let order = seat_partners(&pairs, components.len());
    let mut slots = components.into_iter().map(Some).collect::<Vec<_>>();
    order
        .into_iter()
        .filter_map(|index| slots[index].take())
        .collect()
}

/// Greedy seating: earliest-interacting pairs sit consecutively, each
/// component only claimed once; anything never paired keeps its original
/// position at the end.
fn seat_partners(pairs: &[(usize, usize)], count: usize) -> Vec<usize> {
    let mut order = Vec::with_capacity(count);
    let mut seated = vec![false; count];
    for &(first, second) in pairs {
        if !seated[first] && !seated[second] {
            seated[first] = true;
            seated[second] = true;
            order.push(first);
            order.push(second);
        }
    }
    order.extend((0..count).filter(|index| !seated[*index]));
    order
}

/// One flow step: components inherit the centroid of their atoms' previous
/// homes, then whole components relax apart until no footprints overlap.
/// Components containing `linked` atoms — the ones the active operation
/// spans — are also pulled together until tangent, so the operation plays
/// out between neighbours.
#[allow(clippy::cast_precision_loss)]
fn flow_layout(
    frame: &StructuralFrame,
    previous: &BTreeMap<String, Point>,
    linked: &BTreeSet<String>,
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
        layout_component(frame, component, center, &mut positions);
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

    let attracted = components
        .iter()
        .map(|component| component.iter().any(|id| linked.contains(id)))
        .collect::<Vec<_>>();

    let mut offsets = vec![Vector::new(0.0, 0.0); components.len()];
    relax_components(&mut centers, &mut offsets, &radii, &attracted);
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

/// Relax overlapping components apart, and pull operation-linked
/// (`attracted`) pairs together until tangent. Coincident centers (a fresh
/// dissociation) separate along a stable per-pair angle so the push is
/// deterministic. Every move is mirrored into `offsets` so callers can
/// carry it over to individual atom positions.
#[allow(clippy::cast_precision_loss)]
fn relax_components(
    centers: &mut [Point],
    offsets: &mut [Vector],
    radii: &[f32],
    attracted: &[bool],
) {
    for _ in 0..48 {
        let mut moved = false;
        for first in 0..centers.len() {
            for second in (first + 1)..centers.len() {
                let delta = centers[second] - centers[first];
                let distance = vector_magnitude(delta);
                let required = radii[first] + radii[second];
                if distance >= required {
                    if attracted[first] && attracted[second] && distance > required + 1.0 {
                        let direction = delta * (1.0 / distance.max(1.0));
                        let pull = (distance - required) * 0.5;
                        centers[first] += direction * pull;
                        centers[second] -= direction * pull;
                        offsets[first] += direction * pull;
                        offsets[second] -= direction * pull;
                        moved = true;
                    }
                    continue;
                }
                let direction = if distance < 1.0 {
                    let angle = -std::f32::consts::FRAC_PI_2
                        + std::f32::consts::TAU * second as f32 / centers.len().max(1) as f32;
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
}

#[allow(clippy::cast_precision_loss)]
fn component_slots(component_count: usize, bounds: Rectangle) -> Vec<Point> {
    if component_count == 0 {
        return Vec::new();
    }
    let compact = bounds.width < 720.0;
    // Square-ish grid: 4 components seat 2×2, not 3+1 with a dead corner.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let balanced = (component_count as f32).sqrt().ceil() as usize;
    let columns = balanced.min(if compact { 2 } else { 3 }).max(1);
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
            // An incomplete final row sits centered, not flush-left.
            let row_items = if row + 1 == rows && !component_count.is_multiple_of(columns) {
                component_count % columns
            } else {
                columns
            };
            let row_inset = (columns - row_items) as f32 * cell_width * 0.5;
            Point::new(
                safe_left + row_inset + cell_width * (column as f32 + 0.5),
                safe_top + cell_height * (row as f32 + 0.5),
            )
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
    link_ionic(frame, &mut link);
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
    positions: &mut BTreeMap<String, Point>,
) {
    if layout_lattice(frame, component, center, positions) {
        return;
    }
    if component.len() == 1 {
        positions.insert(component[0].clone(), center);
        return;
    }
    // Bond lengths follow the atoms' visual radii (matching the physics
    // spring rests), so shells nearly touch across a visible shared pair.
    let radius_of =
        |id: &str| atom(frame, id).map_or(24.0, |state| atom_visual_radius(&state.element));
    let gap = 26.0;
    if component.len() == 2 {
        let span = radius_of(&component[0]) + radius_of(&component[1]) + gap;
        positions.insert(component[0].clone(), center + Vector::new(-span * 0.5, 0.0));
        positions.insert(component[1].clone(), center + Vector::new(span * 0.5, 0.0));
        return;
    }

    let adjacency = component_adjacency(frame, component);
    let mut seen: BTreeSet<String>;
    let mut queue: VecDeque<(String, usize, f32)>;
    let placed_rings = fused_layout(frame, &adjacency, center, gap, positions)
        .or_else(|| spiro_layout(frame, &adjacency, center, gap, positions))
        .or_else(|| bridged_layout(frame, &adjacency, center, gap, positions));
    if let Some(rings) = placed_rings {
        // Substituents hang radially outward from their ring's centre.
        seen = rings.iter().flat_map(|(ring, _)| ring).cloned().collect();
        queue = VecDeque::new();
        let mut queued = BTreeSet::new();
        for (ring, ring_center) in &rings {
            for id in ring {
                if !queued.insert(id.clone()) {
                    continue;
                }
                let Some(position) = positions.get(id) else {
                    continue;
                };
                let angle = (position.y - ring_center.y).atan2(position.x - ring_center.x);
                queue.push_back((id.clone(), 1, angle));
            }
        }
    } else {
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
        seen = BTreeSet::from([root.clone()]);
        queue = VecDeque::from([(root.clone(), 0_usize, -std::f32::consts::FRAC_PI_2)]);
    }
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

/// Places an edge-fused ring system: each ring's atoms sit on a regular
/// polygon — the first ring centred on `center` with its edge matching the
/// widest bond length, every later ring folding outward across the edge it
/// shares with an already-placed ring (the standard fused-polygon
/// construction). Returns each ring with its centre; `None` when the
/// component is not a fused system.
fn fused_layout(
    frame: &StructuralFrame,
    adjacency: &BTreeMap<String, BTreeSet<String>>,
    center: Point,
    gap: f32,
    positions: &mut BTreeMap<String, Point>,
) -> Option<Vec<(Vec<String>, Point)>> {
    let rings = ring_systems(adjacency)?;
    let mut placed = Vec::with_capacity(rings.len());
    let mut ring_centers: Vec<Point> = Vec::new();
    for ring in rings {
        let ring_center = place_ring(frame, &ring, &ring_centers, center, gap, positions);
        ring_centers.push(ring_center);
        placed.push((ring, ring_center));
    }
    Some(placed)
}

/// Follows a degree-2 chain through the 2-core from `start` via `via`
/// until it reaches a hub (a core atom of degree other than 2), returning
/// the interior atoms in walk order and the terminal hub.
fn core_chain<'a>(
    adjacency: &'a BTreeMap<String, BTreeSet<String>>,
    core: &BTreeSet<&'a str>,
    hubs: &BTreeSet<&'a str>,
    start: &'a str,
    via: &'a str,
) -> Option<(Vec<String>, &'a str)> {
    let mut previous = start;
    let mut current = via;
    let mut interior = Vec::new();
    while !hubs.contains(current) {
        if interior.len() > core.len() {
            return None;
        }
        interior.push(current.to_owned());
        let next = adjacency[current]
            .iter()
            .map(String::as_str)
            .find(|id| core.contains(id) && *id != previous)?;
        previous = current;
        current = next;
    }
    Some((interior, current))
}

/// Two rings sharing exactly one atom (a spiro junction): each ring is a
/// regular polygon on its own side of the shared atom, ring centres
/// collinear through it, so the junction's two bond pairs mirror across
/// the centre line. Returns each ring with its centre; `None` for anything
/// that is not exactly two vertex-sharing rings.
#[allow(clippy::cast_precision_loss)]
fn spiro_layout(
    frame: &StructuralFrame,
    adjacency: &BTreeMap<String, BTreeSet<String>>,
    center: Point,
    gap: f32,
    positions: &mut BTreeMap<String, Point>,
) -> Option<Vec<(Vec<String>, Point)>> {
    let degrees = core_degrees(adjacency);
    let core = degrees.keys().copied().collect::<BTreeSet<_>>();
    let hubs = degrees
        .iter()
        .filter(|(_, degree)| **degree != 2)
        .map(|(id, degree)| (*id, *degree))
        .collect::<Vec<_>>();
    let [(pivot, 4)] = hubs.as_slice() else {
        return None;
    };
    let hub_set = BTreeSet::from([*pivot]);
    let mut remaining = adjacency[*pivot]
        .iter()
        .map(String::as_str)
        .filter(|id| core.contains(id))
        .collect::<BTreeSet<_>>();
    let mut rings: Vec<Vec<String>> = Vec::new();
    while let Some(start) = remaining.pop_first() {
        let (interior, terminal) = core_chain(adjacency, &core, &hub_set, pivot, start)?;
        if terminal != *pivot {
            return None;
        }
        if let Some(last) = interior.last() {
            remaining.remove(last.as_str());
        }
        let mut ring = vec![(*pivot).to_owned()];
        ring.extend(interior);
        rings.push(ring);
    }
    let [first, second] = rings.as_slice() else {
        return None;
    };
    if first.len() + second.len() != core.len() + 1 {
        return None;
    }

    let radius_of =
        |id: &str| atom(frame, id).map_or(24.0, |state| atom_visual_radius(&state.element));
    let mut placed = Vec::with_capacity(2);
    for (ring, base_angle) in [(first, 0.0_f32), (second, std::f32::consts::PI)] {
        let count = ring.len();
        let edge = (0..count)
            .map(|index| radius_of(&ring[index]) + radius_of(&ring[(index + 1) % count]) + gap)
            .fold(0.0_f32, f32::max);
        let circumradius = edge / (2.0 * (std::f32::consts::PI / count as f32).sin());
        // The shared atom (vertex 0) sits exactly on `center`; the ring
        // centre backs away from it along the horizontal axis.
        let ring_center = center - Vector::new(base_angle.cos() * circumradius, 0.0);
        for (index, id) in ring.iter().enumerate() {
            let angle = base_angle + std::f32::consts::TAU * index as f32 / count as f32;
            positions.insert(
                id.clone(),
                ring_center + Vector::new(angle.cos() * circumradius, angle.sin() * circumradius),
            );
        }
        placed.push((ring.clone(), ring_center));
    }
    Some(placed)
}

/// A two-bridgehead bicyclic (norbornane-style): the two longest
/// bridgehead-to-bridgehead paths form one regular perimeter polygon and
/// the shortest bridge's atoms arc gently through its interior, bowed off
/// the bridgehead chord so they clear the centre and the perimeter.
/// ponytail: single-bridge bicyclics only; tri-bridged cages keep the tree
/// fallback until they need honest geometry.
#[allow(clippy::cast_precision_loss)]
fn bridged_layout(
    frame: &StructuralFrame,
    adjacency: &BTreeMap<String, BTreeSet<String>>,
    center: Point,
    gap: f32,
    positions: &mut BTreeMap<String, Point>,
) -> Option<Vec<(Vec<String>, Point)>> {
    let degrees = core_degrees(adjacency);
    let core = degrees.keys().copied().collect::<BTreeSet<_>>();
    let hubs = degrees
        .iter()
        .filter(|(_, degree)| **degree != 2)
        .map(|(id, degree)| (*id, *degree))
        .collect::<Vec<_>>();
    let [(first_head, 3), (second_head, 3)] = hubs.as_slice() else {
        return None;
    };
    let hub_set = BTreeSet::from([*first_head, *second_head]);
    let mut paths: Vec<Vec<String>> = Vec::with_capacity(3);
    for via in adjacency[*first_head]
        .iter()
        .map(String::as_str)
        .filter(|id| core.contains(id))
    {
        let (interior, terminal) = core_chain(adjacency, &core, &hub_set, first_head, via)?;
        if terminal != *second_head {
            return None;
        }
        paths.push(interior);
    }
    if paths.len() != 3 || paths.iter().map(Vec::len).sum::<usize>() + 2 != core.len() {
        return None;
    }
    paths.sort_by_key(|path| std::cmp::Reverse(path.len()));
    let bridge = paths.pop()?;
    if bridge.is_empty() {
        // A direct bridgehead-to-bridgehead edge is a fused system,
        // already handled upstream.
        return None;
    }

    let mut perimeter = vec![(*first_head).to_owned()];
    perimeter.extend(paths[0].iter().cloned());
    perimeter.push((*second_head).to_owned());
    perimeter.extend(paths[1].iter().rev().cloned());
    let ring_center = place_ring(frame, &perimeter, &[], center, gap, positions);
    let (Some(&head_a), Some(&head_b)) = (positions.get(*first_head), positions.get(*second_head))
    else {
        return None;
    };
    let edge = positions
        .get(&perimeter[1])
        .map_or(74.0, |next| vector_magnitude(*next - head_a));
    let chord = head_b - head_a;
    let length = vector_magnitude(chord).max(1.0);
    let perpendicular = Vector::new(-chord.y, chord.x) * (1.0 / length);
    let bow = edge * 0.35;
    for (index, id) in bridge.iter().enumerate() {
        let t = (index + 1) as f32 / (bridge.len() + 1) as f32;
        positions.insert(
            id.clone(),
            head_a + chord * t + perpendicular * (bow * (std::f32::consts::PI * t).sin()),
        );
    }
    let mut atoms = perimeter;
    atoms.extend(bridge);
    Some(vec![(atoms, ring_center)])
}

/// Places one ring of a fused system as a regular polygon and returns its
/// centre: the first ring lands on `center`, and every later ring folds
/// outward across the edge it shares with the already-placed rings.
#[allow(clippy::cast_precision_loss)]
fn place_ring(
    frame: &StructuralFrame,
    ring: &[String],
    prior_centers: &[Point],
    center: Point,
    gap: f32,
    positions: &mut BTreeMap<String, Point>,
) -> Point {
    let count = ring.len();
    let angle_step = std::f32::consts::PI / count as f32;
    let shared = (0..count).find_map(|index| {
        let a = positions.get(&ring[index])?;
        let b = positions.get(&ring[(index + 1) % count])?;
        Some((index, *a, *b))
    });
    let Some((start, edge_a, edge_b)) = shared else {
        let radius_of =
            |id: &str| atom(frame, id).map_or(24.0, |state| atom_visual_radius(&state.element));
        let edge = (0..count)
            .map(|index| radius_of(&ring[index]) + radius_of(&ring[(index + 1) % count]) + gap)
            .fold(0.0_f32, f32::max);
        let circumradius = edge / (2.0 * angle_step.sin());
        for (index, id) in ring.iter().enumerate() {
            let angle =
                -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * index as f32 / count as f32;
            positions.insert(
                id.clone(),
                center + Vector::new(angle.cos() * circumradius, angle.sin() * circumradius),
            );
        }
        return center;
    };
    let edge = vector_magnitude(edge_b - edge_a).max(1.0);
    let circumradius = edge / (2.0 * angle_step.sin());
    let midpoint = Point::new(
        f32::midpoint(edge_a.x, edge_b.x),
        f32::midpoint(edge_a.y, edge_b.y),
    );
    let mut normal = Vector::new(edge_a.y - edge_b.y, edge_b.x - edge_a.x) * (1.0 / edge);
    // Fold away from the ring system placed so far.
    let inside = Point::new(
        prior_centers.iter().map(|point| point.x).sum::<f32>() / prior_centers.len().max(1) as f32,
        prior_centers.iter().map(|point| point.y).sum::<f32>() / prior_centers.len().max(1) as f32,
    );
    if (midpoint.x - inside.x) * normal.x + (midpoint.y - inside.y) * normal.y < 0.0 {
        normal = Vector::new(-normal.x, -normal.y);
    }
    let apothem = edge / (2.0 * angle_step.tan());
    let ring_center = midpoint + normal * apothem;
    let angle_a = (edge_a.y - ring_center.y).atan2(edge_a.x - ring_center.x);
    let angle_b = (edge_b.y - ring_center.y).atan2(edge_b.x - ring_center.x);
    let mut step = angle_b - angle_a;
    if step > std::f32::consts::PI {
        step -= std::f32::consts::TAU;
    } else if step < -std::f32::consts::PI {
        step += std::f32::consts::TAU;
    }
    for offset in 0..count {
        let id = &ring[(start + offset) % count];
        if positions.contains_key(id) {
            continue;
        }
        let angle = angle_a + step * offset as f32;
        positions.insert(
            id.clone(),
            ring_center + Vector::new(angle.cos() * circumradius, angle.sin() * circumradius),
        );
    }
    ring_center
}

/// The 2-core of a component's adjacency — leaves iteratively peeled until
/// only ring atoms remain — with each survivor's degree within the core.
fn core_degrees(adjacency: &BTreeMap<String, BTreeSet<String>>) -> BTreeMap<&str, usize> {
    let mut degree = adjacency
        .iter()
        .map(|(id, neighbours)| (id.as_str(), neighbours.len()))
        .collect::<BTreeMap<_, _>>();
    let mut leaves = degree
        .iter()
        .filter(|(_, degree)| **degree <= 1)
        .map(|(id, _)| *id)
        .collect::<Vec<_>>();
    while let Some(leaf) = leaves.pop() {
        degree.remove(leaf);
        for neighbour in &adjacency[leaf] {
            if let Some(count) = degree.get_mut(neighbour.as_str()) {
                *count -= 1;
                if *count == 1 {
                    leaves.push(neighbour);
                }
            }
        }
    }
    degree
}

/// The smallest-cycle rings of a connected component, each in traversal
/// order, ordered so every ring after the first shares an edge with an
/// earlier one; `None` for acyclic components and for anything that does
/// not decompose into edge-sharing fused rings.
/// ponytail: spiro and single-bridge bicyclics get their own layouts
/// downstream; multi-bridge cages and mixed polycyclics keep the tree
/// fallback until they ever need honest polygons.
fn ring_systems(adjacency: &BTreeMap<String, BTreeSet<String>>) -> Option<Vec<Vec<String>>> {
    let edges = adjacency.values().map(BTreeSet::len).sum::<usize>() / 2;
    if edges < adjacency.len() {
        return None;
    }
    let core = core_degrees(adjacency).into_keys().collect::<BTreeSet<_>>();
    if core.len() < 3 {
        return None;
    }
    // A smallest-cycles basis (SSSR equivalent at this scale): the
    // deduplicated shortest cycle through every core edge, smallest first,
    // keeping each cycle that still covers a new edge.
    let ring_edges = |ring: &[String]| {
        (0..ring.len())
            .map(|index| sorted_pair(&ring[index], &ring[(index + 1) % ring.len()]))
            .collect::<BTreeSet<_>>()
    };
    let mut candidates: Vec<Vec<String>> = Vec::new();
    let mut keys = BTreeSet::new();
    for &from in &core {
        for to in adjacency[from].iter().map(String::as_str) {
            if !core.contains(to) || from >= to {
                continue;
            }
            let ring = shortest_cycle(adjacency, &core, from, to)?;
            if keys.insert(ring_edges(&ring)) {
                candidates.push(ring);
            }
        }
    }
    candidates.sort_by_key(Vec::len);
    let mut selected: Vec<Vec<String>> = Vec::new();
    let mut covered: BTreeSet<(String, String)> = BTreeSet::new();
    for ring in candidates {
        let edges = ring_edges(&ring);
        if edges.iter().any(|edge| !covered.contains(edge)) {
            covered.extend(edges);
            selected.push(ring);
        }
    }
    // Order the rings so each one fuses onto an already-placed edge; any
    // ring gluing on by more than one edge (bridged) or by no edge (spiro)
    // sends the whole component to the tree fallback.
    if selected.is_empty() {
        return None;
    }
    let mut ordered = vec![selected.remove(0)];
    let mut placed: BTreeSet<String> = ordered[0].iter().cloned().collect();
    while !selected.is_empty() {
        let next = selected.iter().position(|ring| {
            (0..ring.len()).any(|index| {
                placed.contains(&ring[index]) && placed.contains(&ring[(index + 1) % ring.len()])
            })
        })?;
        let ring = selected.remove(next);
        if ring.iter().filter(|id| placed.contains(*id)).count() > 2 {
            return None;
        }
        placed.extend(ring.iter().cloned());
        ordered.push(ring);
    }
    Some(ordered)
}

/// The shortest cycle through one core edge: BFS between its endpoints
/// with the edge itself forbidden. `None` marks a bridge between rings.
fn shortest_cycle(
    adjacency: &BTreeMap<String, BTreeSet<String>>,
    core: &BTreeSet<&str>,
    from: &str,
    to: &str,
) -> Option<Vec<String>> {
    let mut parent = BTreeMap::from([(from, from)]);
    let mut queue = VecDeque::from([from]);
    while let Some(node) = queue.pop_front() {
        for next in adjacency[node].iter().map(String::as_str) {
            if !core.contains(next) || parent.contains_key(next) || (node == from && next == to) {
                continue;
            }
            parent.insert(next, node);
            if next == to {
                let mut ring = vec![to.to_owned()];
                let mut walk = to;
                while walk != from {
                    walk = parent[walk];
                    ring.push(walk.to_owned());
                }
                return Some(ring);
            }
            queue.push_back(next);
        }
    }
    None
}

/// Lays out a component that is exactly one ionic lattice group as a
/// charge-alternating grid centred on `center`; returns false for anything
/// else so the caller falls back to the tree layout. Each ion keeps its own
/// internal geometry and occupies one uniformly sized cell, so no two ions
/// can overlap regardless of stoichiometry.
#[allow(clippy::cast_precision_loss)]
fn layout_lattice(
    frame: &StructuralFrame,
    component: &[String],
    center: Point,
    positions: &mut BTreeMap<String, Point>,
) -> bool {
    let wanted = component
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let Some(group) = lattice_groups(frame).into_iter().find(|group| {
        group.members.len() >= 2
            && !group.pairs.is_empty()
            && group
                .members
                .iter()
                .flat_map(|member| member_component(frame, *member).atoms.iter())
                .map(String::as_str)
                .collect::<BTreeSet<_>>()
                == wanted
    }) else {
        return false;
    };

    // Lay each ion out around the origin to measure its footprint, then
    // stamp it into its grid cell.
    let radius_of =
        |id: &str| atom(frame, id).map_or(24.0, |state| atom_visual_radius(&state.element));
    let mut internal = Vec::with_capacity(group.members.len());
    let mut footprint = 0.0_f32;
    for member in &group.members {
        let mut ion = member_component(frame, *member).atoms.clone();
        ion.sort();
        let mut local = BTreeMap::new();
        layout_component(frame, &ion, Point::ORIGIN, &mut local);
        for (id, position) in &local {
            footprint = footprint.max(vector_magnitude(*position - Point::ORIGIN) + radius_of(id));
        }
        internal.push(local);
    }

    let spacing = footprint * 2.0 + 34.0;
    let columns = group.columns;
    let rows = group.members.len().div_ceil(columns);
    for (index, local) in internal.iter().enumerate() {
        let row = index / columns;
        let along = index % columns;
        let column = if row % 2 == 0 {
            along
        } else {
            columns - 1 - along
        };
        let cell = center
            + Vector::new(
                (column as f32 - (columns - 1) as f32 * 0.5) * spacing,
                (row as f32 - (rows - 1) as f32 * 0.5) * spacing,
            );
        for (id, offset) in local {
            positions.insert(id.clone(), cell + (*offset - Point::ORIGIN));
        }
    }
    true
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
    link_ionic(frame, &mut link);
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

#[allow(clippy::too_many_lines)]
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
    // One union frame keeps the full lattice topology while individual
    // formula units fade with their own association's reveal.
    let mut union_frame = after.clone();
    for association in &before.ionic_associations {
        if !after_ionic.contains_key(&association.id) {
            union_frame.ionic_associations.push(association.clone());
        }
    }
    for atom in &before.atoms {
        if union_frame
            .atoms
            .iter()
            .all(|existing| existing.id != atom.id)
        {
            union_frame.atoms.push(atom.clone());
        }
    }
    let reveal_of = |id: &str| match (before_ionic.contains_key(id), after_ionic.contains_key(id)) {
        (true, true) => 1.0,
        (false, true) => progress,
        (true, false) => 1.0 - progress,
        (false, false) => 0.0,
    };
    for group in lattice_groups(&union_frame) {
        for (left, right) in &group.pairs {
            let left = group.members[*left];
            let right = group.members[*right];
            let reveal = reveal_of(&union_frame.ionic_associations[left.0].id)
                .min(reveal_of(&union_frame.ionic_associations[right.0].id));
            if reveal <= 0.0 {
                continue;
            }
            let ion_clearance = |member: (usize, usize)| {
                let component = member_component(&union_frame, member);
                ionic_anchor_id(component, &union_frame)
                    .and_then(|anchor| atom(&union_frame, anchor))
                    .map_or(24.0, |state| atom_visual_radius(&state.element))
                    + 8.0
            };
            let (left_clearance, right_clearance) = (ion_clearance(left), ion_clearance(right));
            let (Some(left), Some(right)) = (
                ionic_component_position(
                    member_component(&union_frame, left),
                    &union_frame,
                    positions,
                ),
                ionic_component_position(
                    member_component(&union_frame, right),
                    &union_frame,
                    positions,
                ),
            ) else {
                continue;
            };
            draw_ionic(
                frame,
                left,
                right,
                (left_clearance, right_clearance),
                reveal,
                opacity,
                scale,
            );
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

/// Links atoms within every ion of every association, then anchor atoms
/// across lattice-neighbour ions — fusing same-substance formula units into
/// one connected crystal.
fn link_ionic(frame: &StructuralFrame, link: &mut impl FnMut(&str, &str)) {
    for association in &frame.ionic_associations {
        for component in &association.components {
            if let Some(first) = component.atoms.first() {
                for atom in component.atoms.iter().skip(1) {
                    link(first, atom);
                }
            }
        }
    }
    for group in lattice_groups(frame) {
        for (left, right) in &group.pairs {
            if let (Some(left), Some(right)) = (
                ionic_anchor_id(member_component(frame, group.members[*left]), frame),
                ionic_anchor_id(member_component(frame, group.members[*right]), frame),
            ) {
                link(left, right);
            }
        }
    }
}

/// Ionic associations of the same substance, fused into one crystal.
///
/// Ionic compounds have no molecules: catalogue component order is an
/// identity concern, not a presentation topology, and separate formula
/// units are a bookkeeping artefact. All charged ions of a substance are
/// interleaved into a charge-alternating sequence and wrapped boustrophedon
/// into a near-square grid, so consecutive sequence entries are always
/// grid-adjacent and every horizontal and vertical neighbour pair carries
/// opposite signs wherever the stoichiometry allows.
struct LatticeGroup {
    /// `(association index, component index)` in grid sequence order.
    members: Vec<(usize, usize)>,
    columns: usize,
    /// Opposite-charge grid-neighbour pairs, as indices into `members`.
    pairs: Vec<(usize, usize)>,
}

fn lattice_cell(index: usize, columns: usize) -> (usize, usize) {
    let row = index / columns;
    let along = index % columns;
    let column = if row.is_multiple_of(2) {
        along
    } else {
        columns - 1 - along
    };
    (row, column)
}

fn attach_unpaired_lattice_members(signs: &[i64], columns: usize, pairs: &mut Vec<(usize, usize)>) {
    for index in 0..signs.len() {
        if pairs
            .iter()
            .any(|(left, right)| *left == index || *right == index)
        {
            continue;
        }
        let (row, column) = lattice_cell(index, columns);
        let nearest = signs
            .iter()
            .enumerate()
            .filter(|(_, sign)| **sign == -signs[index])
            .min_by_key(|(candidate, _)| {
                let (candidate_row, candidate_column) = lattice_cell(*candidate, columns);
                row.abs_diff(candidate_row) + column.abs_diff(candidate_column)
            })
            .map(|(candidate, _)| candidate);
        if let Some(nearest) = nearest {
            pairs.push((index, nearest));
        }
    }
}

fn member_component(frame: &StructuralFrame, member: (usize, usize)) -> &RenderIonicComponent {
    &frame.ionic_associations[member.0].components[member.1]
}

#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
#[allow(clippy::cast_sign_loss)]
fn lattice_groups(frame: &StructuralFrame) -> Vec<LatticeGroup> {
    let elements: BTreeMap<&str, &str> = frame
        .atoms
        .iter()
        .map(|atom| (atom.id.as_str(), atom.element.as_str()))
        .collect();
    // Substance identity is structural: the multiset of (charge, element
    // multiset) over an association's ions. Same signature, same salt.
    let mut by_signature: BTreeMap<Vec<(i64, Vec<&str>)>, Vec<usize>> = BTreeMap::new();
    for (index, association) in frame.ionic_associations.iter().enumerate() {
        let mut signature = association
            .components
            .iter()
            .map(|component| {
                let mut atoms = component
                    .atoms
                    .iter()
                    .map(|id| elements.get(id.as_str()).copied().unwrap_or(""))
                    .collect::<Vec<_>>();
                atoms.sort_unstable();
                (component.charge, atoms)
            })
            .collect::<Vec<_>>();
        signature.sort();
        by_signature.entry(signature).or_default().push(index);
    }

    by_signature
        .into_values()
        .map(|associations| {
            let mut positive = Vec::new();
            let mut negative = Vec::new();
            for &index in &associations {
                for (slot, component) in frame.ionic_associations[index]
                    .components
                    .iter()
                    .enumerate()
                {
                    match component.charge.signum() {
                        1 => positive.push((index, slot)),
                        -1 => negative.push((index, slot)),
                        _ => {}
                    }
                }
            }
            // Interleave with the majority sign first, so alternation runs
            // as long as the stoichiometry allows and leftovers sit at the
            // end of the sequence.
            let (major, minor, major_sign) = if negative.len() > positive.len() {
                (&negative, &positive, -1_i64)
            } else {
                (&positive, &negative, 1_i64)
            };
            let mut members = Vec::with_capacity(positive.len() + negative.len());
            let mut signs = Vec::with_capacity(members.capacity());
            for index in 0..major.len().max(minor.len()) {
                if let Some(member) = major.get(index) {
                    members.push(*member);
                    signs.push(major_sign);
                }
                if let Some(member) = minor.get(index) {
                    members.push(*member);
                    signs.push(-major_sign);
                }
            }

            let columns = ((members.len() as f32).sqrt().ceil() as usize).max(1);
            let mut pairs = Vec::new();
            for index in 0..members.len() {
                // Consecutive sequence entries are grid-adjacent by the
                // boustrophedon wrap.
                if index + 1 < members.len() && signs[index] != signs[index + 1] {
                    pairs.push((index, index + 1));
                }
                // The cell directly below, skipping row-turn duplicates.
                let (row, column) = lattice_cell(index, columns);
                let below = (row + 1) * columns
                    + if (row + 1).is_multiple_of(2) {
                        column
                    } else {
                        columns - 1 - column
                    };
                if below < members.len() && below != index + 1 && signs[index] != signs[below] {
                    pairs.push((index, below));
                }
            }
            // Non-1:1 salts exhaust the minority sign before every majority
            // ion has an opposite-charge grid neighbour. Attach each such
            // leftover to its nearest opposite-charge cell so every validated
            // component participates in the same illustrative lattice.
            attach_unpaired_lattice_members(&signs, columns, &mut pairs);
            LatticeGroup {
                members,
                columns,
                pairs,
            }
        })
        .collect()
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
        let stick = Path::line(
            visible_start + perpendicular * *offset * scale,
            visible_end + perpendicular * *offset * scale,
        );
        // A dark underlay keeps the stick crisp where it crosses discs,
        // rings, or a sibling bond.
        frame.stroke(
            &stick,
            Stroke {
                line_cap: canvas::stroke::LineCap::Round,
                ..Stroke::default()
                    .with_color(CANVAS.scale_alpha(alpha * 0.55))
                    .with_width(5.2 * scale)
            },
        );
        frame.stroke(
            &stick,
            Stroke {
                line_cap: canvas::stroke::LineCap::Round,
                ..Stroke::default()
                    .with_color(chemistry_color::COVALENT.scale_alpha(alpha * 0.96))
                    .with_width(3.0 * scale)
            },
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
    clearance: (f32, f32),
    reveal: f32,
    opacity: f32,
    scale: f32,
) {
    // Dots span only the visible gap between the ion discs: large ions
    // (Ag, Cl) used to swallow most of the centre-to-centre run, leaving
    // the association nearly invisible.
    let span = vector_magnitude(right - left);
    let gap = span - (clearance.0 + clearance.1) * scale;
    if span <= 1.0 || gap <= 4.0 * scale {
        return;
    }
    let direction = (right - left) * (1.0 / span);
    let start = left + direction * (clearance.0 * scale);
    let delta = direction * gap;
    // Dot count follows the world-space gap length so long attractions
    // don't stretch a fixed dot budget into sparse crumbs.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let count = (gap / (16.0 * scale.max(0.01))) as u8;
    let count = count.clamp(3, 24);
    for step in 1..=count {
        let t = f32::from(step) / f32::from(count + 1);
        if t > reveal {
            continue;
        }
        frame.fill(
            &Path::circle(start + delta * t, 2.4 * scale),
            IONIC.scale_alpha(opacity * (0.6 + t * 0.32)),
        );
    }
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
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
    let covalently_bonded = |frame: &StructuralFrame, id: &str| {
        frame
            .covalent_bonds
            .iter()
            .any(|bond| bond.left == id || bond.right == id)
    };
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
        // Bystanders recede but stay clearly readable: 0.60 sent settled
        // products (formed H₂) ghost-grey for entire scenes.
        let focus_opacity = if focus_active && !active.is_empty() && !active.contains(id) {
            0.75
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
        // The badge must clear whatever ring sits outside the atom: the
        // pulsing active ring, or the metallic halo around a lone site.
        let badge_clearance = {
            let radius = atom_visual_radius(&atom.element);
            let mut clearance = radius;
            if active.contains(id) {
                clearance = clearance.max(radius * 1.12 + 12.0);
            }
            let haloed = before
                .metallic_domains
                .iter()
                .chain(&after.metallic_domains)
                .any(|domain| domain.sites.len() == 1 && domain.sites[0] == *id);
            if haloed {
                clearance = clearance.max(radius + 28.0);
            }
            clearance
        };
        let before_is_metallic = is_metallic_site(before, id);
        let after_is_metallic = is_metallic_site(after, id);
        // An atom inside a molecule or polyatomic ion keeps its formal
        // charge private — the ion wears one net badge instead (below).
        // The per-atom badge still appears for the beat that changes it.
        let static_member_charge = covalently_bonded(before, id)
            && covalently_bonded(after, id)
            && displayed_charge(before_atom, before_is_metallic)
                == displayed_charge(after_atom, after_is_metallic);
        if !static_member_charge {
            draw_charge_transition(
                frame,
                before_atom,
                after_atom,
                before_is_metallic,
                after_is_metallic,
                position,
                badge_clearance,
                progress,
                alpha,
                scale,
                &bond_angles,
            );
        }
    }
    draw_polyatomic_net_charges(
        frame,
        before,
        after,
        positions,
        active,
        progress,
        opacity,
        focus_active,
        scale,
    );
}

/// One net-charge badge per covalently-connected component (union of both
/// frames' bonds), seated off the component's outermost atom in its largest
/// bond-free gap. Per-atom formal charges inside these components stay
/// hidden while static, so NO₃⁻ reads as one −1 ion, not three badges.
#[allow(clippy::too_many_arguments)]
fn draw_polyatomic_net_charges(
    frame: &mut canvas::Frame,
    before: &StructuralFrame,
    after: &StructuralFrame,
    positions: &BTreeMap<String, Point>,
    active: &BTreeSet<&str>,
    progress: f32,
    opacity: f32,
    focus_active: bool,
    scale: f32,
) {
    let mut adjacency: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for bond in before.covalent_bonds.iter().chain(&after.covalent_bonds) {
        adjacency
            .entry(bond.left.as_str())
            .or_default()
            .insert(bond.right.as_str());
        adjacency
            .entry(bond.right.as_str())
            .or_default()
            .insert(bond.left.as_str());
    }
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for start in adjacency.keys().copied().collect::<Vec<_>>() {
        if seen.contains(start) {
            continue;
        }
        let mut queue = VecDeque::from([start]);
        let mut members = Vec::new();
        seen.insert(start);
        while let Some(id) = queue.pop_front() {
            members.push(id);
            for neighbour in adjacency.get(id).into_iter().flatten() {
                if seen.insert(neighbour) {
                    queue.push_back(neighbour);
                }
            }
        }
        let net = |frame: &StructuralFrame| {
            members
                .iter()
                .map(|id| {
                    i32::from(displayed_charge(
                        atom(frame, id),
                        is_metallic_site(frame, id),
                    ))
                })
                .sum::<i32>()
        };
        let before_net = i16::try_from(net(before)).unwrap_or(0);
        let after_net = i16::try_from(net(after)).unwrap_or(0);
        if before_net == 0 && after_net == 0 {
            continue;
        }
        // Members mid-animation (charge changing, or only just bonding into
        // the component) still wear their own badge; a net badge would
        // double-report the same charge.
        let settled = members.iter().all(|id| {
            displayed_charge(atom(before, id), is_metallic_site(before, id))
                == displayed_charge(atom(after, id), is_metallic_site(after, id))
                && [before, after].into_iter().all(|frame| {
                    frame
                        .covalent_bonds
                        .iter()
                        .any(|bond| bond.left == *id || bond.right == *id)
                })
        });
        if !settled {
            continue;
        }
        // The badge belongs on the atom that actually carries the charge
        // (OH⁻ reads as H⁻ if it lands on the hydrogen); geometry only
        // breaks ties between equally-charged candidates.
        let Some((anchor, anchor_position)) = members
            .iter()
            .filter_map(|id| positions.get(*id).map(|point| (*id, *point)))
            .max_by(|left, right| {
                let magnitude = |id: &str| {
                    atom(after, id)
                        .or_else(|| atom(before, id))
                        .map_or(0, |state| state.formal_charge.unsigned_abs())
                };
                magnitude(left.0)
                    .cmp(&magnitude(right.0))
                    .then_with(|| (left.1.x - left.1.y).total_cmp(&(right.1.x - right.1.y)))
            })
        else {
            continue;
        };
        let focus_opacity = if focus_active
            && !active.is_empty()
            && members.iter().all(|id| !active.contains(id))
        {
            0.75
        } else {
            1.0
        };
        let radius = atom(after, anchor)
            .or_else(|| atom(before, anchor))
            .map_or(24.0, |state| atom_visual_radius(&state.element));
        let bond_angles = atom_bond_angles(anchor, before, after, positions, anchor_position);
        let gaps = angular_gaps(&bond_angles);
        let angle = gaps
            .first()
            .map_or(-std::f32::consts::FRAC_PI_4, |(start, span)| {
                start + span * 0.5
            });
        let offset = (radius * 1.12 + 16.0) * scale;
        let badge = anchor_position + Vector::new(angle.cos() * offset, angle.sin() * offset);
        let alpha = opacity * focus_opacity;
        if before_net == after_net {
            draw_charge(frame, badge, after_net, alpha, scale);
        } else {
            draw_charge(frame, badge, before_net, alpha * (1.0 - progress), scale);
            draw_charge(frame, badge, after_net, alpha * progress, scale);
        }
    }
}

fn is_metallic_site(frame: &StructuralFrame, atom_id: &str) -> bool {
    frame
        .metallic_domains
        .iter()
        .any(|domain| domain.sites.iter().any(|site| site == atom_id))
}

fn displayed_charge(atom: Option<&AtomState>, is_metallic: bool) -> i16 {
    if is_metallic {
        // A metallic site's positive core is balanced by its separately
        // rendered share of the delocalized electron domain. Presenting the
        // core value alone as an ionic charge would mislabel a neutral metal.
        0
    } else {
        atom.map_or(0, |atom| atom.formal_charge)
    }
}

fn shows_static_lewis_electrons(element: &str) -> bool {
    !elements::SUPPORTED.iter().any(|candidate| {
        candidate.symbol == element && candidate.category == elements::Category::TransitionMetal
    })
}

fn domain_shows_stationary_electrons(
    before: &StructuralFrame,
    after: &StructuralFrame,
    domain: &RenderMetallicDomain,
) -> bool {
    domain.sites.iter().all(|site| {
        atom(after, site)
            .or_else(|| atom(before, site))
            .is_none_or(|state| shows_static_lewis_electrons(&state.element))
    })
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

/// Darkens (factor < 1) or lightens a colour toward white (factor > 1).
fn shade(color: Color, factor: f32) -> Color {
    if factor <= 1.0 {
        Color::from_rgba(
            color.r * factor,
            color.g * factor,
            color.b * factor,
            color.a,
        )
    } else {
        let lift = factor - 1.0;
        Color::from_rgba(
            color.r + (1.0 - color.r) * lift,
            color.g + (1.0 - color.g) * lift,
            color.b + (1.0 - color.b) * lift,
            color.a,
        )
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
    let radius = atom_visual_radius(&atom.element) * (if active { 1.12 } else { 1.0 }) * scale;
    let fill = element_color(&atom.element);
    let pulse = 0.5 + 0.5 * (phase * std::f32::consts::TAU * 3.0).sin();
    // Only the atoms doing something wear a glow; the old always-on aura
    // read as a muddy charcoal blob behind every atom.
    if active {
        frame.fill(
            &Path::circle(center, radius + (10.0 + pulse * 4.0) * scale),
            fill.scale_alpha(alpha * 0.12),
        );
        frame.stroke(
            &Path::circle(center, radius + 7.0 * scale),
            Stroke::default()
                .with_color(ACCENT_BRIGHT.scale_alpha(alpha * (0.42 + pulse * 0.28)))
                .with_width((1.4 + pulse * 0.8) * scale),
        );
    }
    frame.fill(
        &Path::circle(center + Vector::new(0.0, 2.5 * scale), radius + 1.0 * scale),
        color::SHADOW.scale_alpha(alpha * 0.30),
    );
    frame.fill(&Path::circle(center, radius), fill.scale_alpha(alpha));
    // Crisp tonal rim + a soft upper glint give the disc definition
    // without pretending to be a 3D sphere.
    frame.stroke(
        &Path::circle(center, radius),
        Stroke::default()
            .with_color(shade(fill, 0.55).scale_alpha(alpha * 0.95))
            .with_width(1.6 * scale),
    );
    let glint = Path::new(|builder| {
        builder.arc(canvas::path::Arc {
            center,
            radius: radius - 3.0 * scale,
            start_angle: iced::Radians(-2.6),
            end_angle: iced::Radians(-0.55),
        });
    });
    frame.stroke(
        &glint,
        Stroke {
            line_cap: canvas::stroke::LineCap::Round,
            ..Stroke::default()
                .with_color(Color::WHITE.scale_alpha(alpha * 0.30))
                .with_width(2.0 * scale)
        },
    );
    frame.fill_text(canvas::Text {
        content: atom.element.clone(),
        position: center,
        color: shade(fill, 0.22).scale_alpha(alpha),
        size: iced::Pixels(14.5 * scale),
        align_x: iced::alignment::Horizontal::Center.into(),
        align_y: iced::alignment::Vertical::Center,
        font: fonts::SEMIBOLD,
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
    if before
        .or(after)
        .is_some_and(|atom| !shows_static_lewis_electrons(&atom.element))
    {
        return;
    }
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
        StructuralOperation::ProtonTransfer {
            hydrogen, acceptor, ..
        } => hydrogen == atom_id || acceptor == atom_id,
        StructuralOperation::TransferMetallicElectron { acceptor, .. } => acceptor == atom_id,
        StructuralOperation::MetallicMembership { site, .. } => site == atom_id,
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
    // The dark seat keeps lone pairs legible over bright discs and rims.
    frame.fill(
        &Path::circle(position, 2.9 * scale),
        CANVAS.scale_alpha(alpha * 0.65),
    );
    frame.fill(
        &Path::circle(position, 2.0 * scale),
        Color::WHITE.scale_alpha(alpha * 0.95),
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_charge_transition(
    frame: &mut canvas::Frame,
    before: Option<&AtomState>,
    after: Option<&AtomState>,
    before_is_metallic: bool,
    after_is_metallic: bool,
    center: Point,
    clearance: f32,
    progress: f32,
    alpha: f32,
    scale: f32,
    bond_angles: &[f32],
) {
    // The badge sits just off the atom rim (or its outermost ring), in the
    // second-largest bond-free gap so it stays clear of both bonds and the
    // electron arc.
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
    let offset = (clearance + 6.0) * scale;
    let badge = center + Vector::new(angle.cos() * offset, angle.sin() * offset);
    let before_charge = displayed_charge(before, before_is_metallic);
    let after_charge = displayed_charge(after, after_is_metallic);
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
        let Some((center, halo_radius)) = domain_halo(before, after, positions, domain, scale)
        else {
            continue;
        };
        let path = Path::circle(center, halo_radius);
        frame.fill(&path, ACCENT.scale_alpha(alpha * 0.07));
        frame.stroke(
            &path,
            Stroke::default()
                .with_color(ACCENT.scale_alpha(alpha * 0.38))
                .with_width(1.2 * scale),
        );
        // While an operation moves this domain's electrons, only the ones
        // not in flight orbit the halo; the moving dots are the routes.
        let active_motion = operations.iter().any(|operation| match operation {
            StructuralOperation::TransferMetallicElectron { domain, .. }
            | StructuralOperation::MetallicMembership { domain, .. } => domain == id,
            _ => false,
        });
        let stationary_electrons = if active_motion {
            match (before_domain, after_domain) {
                (Some(before), Some(after)) => before
                    .delocalized_electrons
                    .min(after.delocalized_electrons),
                (Some(only), None) | (None, Some(only)) => only.delocalized_electrons,
                (None, None) => 0,
            }
        } else {
            domain.delocalized_electrons
        };
        if domain_shows_stationary_electrons(before, after, domain) {
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
}

/// The electron sea hugs its sites as a soft halo: a circle around a lone
/// site, a stadium around a row of them. Returns the halo's center and
/// ring radius in screen space.
#[allow(clippy::cast_precision_loss)]
fn domain_halo(
    before: &StructuralFrame,
    after: &StructuralFrame,
    positions: &BTreeMap<String, Point>,
    domain: &RenderMetallicDomain,
    scale: f32,
) -> Option<(Point, f32)> {
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
        return None;
    }
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
    Some((center, spread + reach))
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
    show_transfer_motion: bool,
) {
    // Midpoint of every covalent site this beat, so twin sites (both O–H
    // bonds of 2 H₂O breaking at once) can arc away from each other
    // instead of tangling in the middle.
    let covalent_centers: Vec<Option<Point>> = operations
        .iter()
        .map(|operation| match operation {
            StructuralOperation::CleaveCovalent { left, right, .. }
            | StructuralOperation::FormCovalent { left, right, .. } => {
                match (positions.get(left), positions.get(right)) {
                    (Some(left), Some(right)) => Some(lerp_point(*left, *right, 0.5)),
                    _ => None,
                }
            }
            _ => None,
        })
        .collect();
    for (index, operation) in operations.iter().enumerate() {
        match operation {
            StructuralOperation::ProtonTransfer {
                hydrogen, acceptor, ..
            } => draw_proton_transfer(frame, hydrogen, acceptor, positions, progress, scale),
            StructuralOperation::TransferMetallicElectron {
                donor_site,
                acceptor,
                count,
                ..
            } if show_transfer_motion => draw_metallic_electron_transfer(
                frame, before, after, donor_site, acceptor, *count, positions, progress, phase,
                scale,
            ),
            StructuralOperation::MetallicMembership {
                domain,
                site,
                joining,
            } => draw_metallic_membership_motion(
                frame, before, after, domain, site, *joining, positions, progress, phase, scale,
            ),
            operation @ (StructuralOperation::CleaveCovalent { .. }
            | StructuralOperation::FormCovalent { .. }) => {
                let siblings: Vec<Point> = covalent_centers
                    .iter()
                    .enumerate()
                    .filter(|(other, _)| *other != index)
                    .filter_map(|(_, center)| *center)
                    .collect();
                #[allow(clippy::cast_precision_loss)]
                let away_from = (!siblings.is_empty()).then(|| {
                    let sum = siblings.iter().fold(Vector::new(0.0, 0.0), |sum, point| {
                        sum + Vector::new(point.x, point.y)
                    });
                    Point::new(sum.x / siblings.len() as f32, sum.y / siblings.len() as f32)
                });
                draw_covalent_electron_motion(
                    frame, operation, positions, away_from, progress, phase, scale,
                );
            }
            StructuralOperation::AssociateIonic { .. }
            | StructuralOperation::AssignProduct { .. }
            | StructuralOperation::Other { .. }
            | StructuralOperation::TransferMetallicElectron { .. } => {}
        }
    }
}

/// Electron routes between a domain's halo ring and a site's own shell:
/// outward from the ring into the shell when the site leaves, inward from
/// the shell onto the ring when it joins.
#[allow(clippy::too_many_arguments)]
fn draw_metallic_membership_motion(
    frame: &mut canvas::Frame,
    before: &StructuralFrame,
    after: &StructuralFrame,
    domain_id: &str,
    site: &str,
    joining: bool,
    positions: &BTreeMap<String, Point>,
    progress: f32,
    phase: f32,
    scale: f32,
) {
    if atom(after, site)
        .or_else(|| atom(before, site))
        .is_some_and(|state| !shows_static_lewis_electrons(&state.element))
    {
        // Releasing a transition-metal site's complete bookkeeping state is
        // not a learner-visible electron transfer. The typed transfer
        // operation draws only the electrons that actually move in reaction.
        return;
    }
    let Some(site_center) = positions.get(site).copied() else {
        return;
    };
    // The route meets the halo the viewer actually sees, which is drawn
    // from the after frame's domain when it still exists.
    let Some((center, halo_radius)) = after
        .metallic_domains
        .iter()
        .chain(&before.metallic_domains)
        .find(|domain| domain.id == domain_id)
        .and_then(|domain| domain_halo(before, after, positions, domain, scale))
    else {
        return;
    };
    let (Some(before_site), Some(after_site)) = (atom(before, site), atom(after, site)) else {
        return;
    };
    let delta = electron_state_delta(Some(before_site), Some(after_site));
    let (shell_state, moving) = if joining {
        (before_site, delta.leaves_shell)
    } else {
        (after_site, delta.enters_shell)
    };
    let shell_dots = electron_positions(site_center, shell_state, phase, scale, &[])
        .into_iter()
        .skip(usize::from(delta.persistent))
        .take(usize::from(moving));
    for (index, dot) in shell_dots.enumerate() {
        let outward = dot - center;
        let magnitude = vector_magnitude(outward).max(1.0);
        let ring_point = center + outward * (halo_radius / magnitude);
        let bend = if index.is_multiple_of(2) { -10.0 } else { 10.0 };
        if joining {
            draw_electron_route(frame, dot, ring_point, progress, bend, scale);
        } else {
            draw_electron_route(frame, ring_point, dot, progress, bend, scale);
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
    let direction = *acceptor_center - *donor;
    let magnitude = vector_magnitude(direction).max(1.0);
    let along = direction / magnitude;
    let perpendicular = Vector::new(-along.y, along.x);
    let delta = electron_state_delta(Some(before_acceptor), Some(after_acceptor));
    let mut targets = electron_positions(*acceptor_center, after_acceptor, phase, scale, &[])
        .into_iter()
        .skip(usize::from(delta.persistent))
        .take(usize::from(delta.enters_shell))
        .collect::<Vec<_>>();
    // A metallic acceptor keeps its gained electrons in the shared sea, so
    // the shell delta can be empty (Fe + CuSO₄ deposits onto Cu like this).
    // The transfer still needs a destination: land the remaining electrons
    // on the acceptor's rim facing the donor instead of asserting shell
    // growth — choreography degrades, the app never panics.
    for index in targets.len()..usize::from(count) {
        let offset =
            f32::from(u8::try_from(index).unwrap_or(u8::MAX)) - (f32::from(count) - 1.0) * 0.5;
        targets
            .push(*acceptor_center - along * 26.0 * scale + perpendicular * offset * 9.0 * scale);
    }
    for (index, target) in targets.into_iter().enumerate().take(usize::from(count)) {
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
    away_from: Option<Point>,
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
        // With a sibling site active, both of this route's electrons bow to
        // the side facing away from it (staggered so they stay distinct);
        // alone, they split across the route as before.
        let bend = away_from.map_or_else(
            || if index.is_multiple_of(2) { -8.0 } else { 8.0 },
            |other| {
                let direction = target - source;
                let magnitude = vector_magnitude(direction).max(1.0);
                let perpendicular = Vector::new(-direction.y, direction.x) * (1.0 / magnitude);
                let midpoint = lerp_point(source, target, 0.5);
                let toward_other = other - midpoint;
                let side = perpendicular.x * toward_other.x + perpendicular.y * toward_other.y;
                let magnitude = if index.is_multiple_of(2) { 8.0 } else { 15.0 };
                if side > 0.0 { -magnitude } else { magnitude }
            },
        );
        draw_electron_route(frame, source, target, progress, bend, scale);
    }
}

/// Moves one complete lone pair from the proton acceptor into the forming
/// H–acceptor bond. Every primitive kernel ledger transition remains in the
/// exact frame sequence, but the learner sees the chemically meaningful paired
/// movement represented by the composite presentation event.
fn draw_proton_transfer(
    frame: &mut canvas::Frame,
    hydrogen: &str,
    acceptor: &str,
    positions: &BTreeMap<String, Point>,
    progress: f32,
    scale: f32,
) {
    let (Some(hydrogen_center), Some(acceptor_center)) =
        (positions.get(hydrogen), positions.get(acceptor))
    else {
        return;
    };
    let direction = *hydrogen_center - *acceptor_center;
    let magnitude = vector_magnitude(direction).max(1.0);
    let along = direction / magnitude;
    let perpendicular = Vector::new(-along.y, along.x);
    let source_center = *acceptor_center + along * 30.0 * scale;
    let target_center = lerp_point(*acceptor_center, *hydrogen_center, 0.5);
    for offset in [-3.2_f32, 3.2] {
        draw_electron_route(
            frame,
            source_center + perpendicular * offset * scale,
            target_center + perpendicular * offset * scale,
            progress,
            -18.0 + offset * 2.0,
            scale,
        );
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
    // The guide arc trails the moving dot instead of pre-drawing the whole
    // journey: only the portion already travelled is stroked.
    let path = Path::new(|builder| {
        builder.move_to(start);
        for step in 1..=24_u8 {
            let t = progress * f32::from(step) / 24.0;
            builder.line_to(quadratic_point(start, control, end, t));
        }
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
        .filter(|label| {
            matches!(
                label.kind,
                ExplanationLabelKind::ObservationExplanation | ExplanationLabelKind::CompletionNote
            )
        })
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
        draw_glass_panel(frame, rect, IONIC, reveal, 15.0 * scale, true);
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

/// The scene's one narration panel: the operation title as its header, the
/// short factual line (formerly a separate floating chip) as its subtitle,
/// and the classroom sentence as its body. One panel, one leader line.
struct ExplanationTextLayout {
    title_lines: Vec<String>,
    subtitle_lines: Vec<String>,
    body_lines: Vec<String>,
    content_inset: f32,
    content_width: f32,
    title_size: f32,
    subtitle_size: f32,
    body_size: f32,
    title_step: f32,
    subtitle_step: f32,
    body_step: f32,
    title_top: f32,
    subtitle_top: f32,
    body_top: f32,
    height: f32,
}

impl ExplanationTextLayout {
    #[allow(clippy::cast_precision_loss)]
    fn new(width: f32, scale: f32, title: &str, subtitle: Option<&str>, body: &str) -> Self {
        let content_inset = 30.0 * scale;
        let content_width = (width - content_inset * 2.0).max(96.0);
        let title_size = 9.5 * scale;
        let subtitle_size = 14.0 * scale;
        let body_size = 13.5 * scale;
        let title_lines = wrap_words(title, content_width, title_size);
        let subtitle_lines = subtitle
            .map(|subtitle| wrap_words(subtitle, content_width, subtitle_size))
            .unwrap_or_default();
        let body_lines = wrap_words(body, content_width, body_size);
        let title_step = 14.0 * scale;
        let subtitle_step = 20.0 * scale;
        let body_step = 20.0 * scale;
        let title_top = 20.0 * scale;
        let subtitle_top = title_top + title_lines.len() as f32 * title_step + 8.0 * scale;
        let body_top = if subtitle_lines.is_empty() {
            title_top + title_lines.len() as f32 * title_step + 14.0 * scale
        } else {
            subtitle_top + subtitle_lines.len() as f32 * subtitle_step + 10.0 * scale
        };
        let height = body_top + body_lines.len() as f32 * body_step + 18.0 * scale;
        Self {
            title_lines,
            subtitle_lines,
            body_lines,
            content_inset,
            content_width,
            title_size,
            subtitle_size,
            body_size,
            title_step,
            subtitle_step,
            body_step,
            title_top,
            subtitle_top,
            body_top,
            height,
        }
    }
}

fn draw_explanation_label(
    frame: &mut canvas::Frame,
    label: &ExplanationLabel,
    context: Option<&ContextLabel>,
    positions: &BTreeMap<String, Point>,
    bounds: Rectangle,
    progress: f32,
    scale: f32,
) {
    let target = average_position(label.target_atoms.iter().map(String::as_str), positions);
    let max_width = (bounds.width - 40.0).max(240.0);
    let width = (410.0 * scale).clamp(260.0, max_width.min(460.0));
    let title = context.map_or_else(
        || explanation_title(label.kind).to_owned(),
        |context| context.title.clone(),
    );
    let text_layout = ExplanationTextLayout::new(
        width,
        scale,
        &title,
        context.map(|context| context.text.as_str()),
        &label.text,
    );
    let (x, base_y) = explanation_position(target, bounds, width, text_layout.height, positions);
    let enter = smoother_step(((progress - 0.04) / 0.14).clamp(0.0, 1.0));
    let alpha = enter;
    let rect = Rectangle::new(
        Point::new(x, base_y + (1.0 - enter) * 18.0 * scale),
        Size::new(width, text_layout.height),
    );
    let accent = explanation_color(label.kind);
    draw_glass_panel(frame, rect, accent, alpha, 16.0 * scale, false);
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
    draw_explanation_text(frame, rect, &text_layout, accent, alpha, scale);
    if label.connector
        && let Some(target) = target
        // A card seated beside its subject needs no leader line.
        && vector_magnitude(target - nearest_edge_point(rect, target)) > 110.0 * scale
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

#[allow(clippy::cast_precision_loss)]
fn draw_explanation_text(
    frame: &mut canvas::Frame,
    rect: Rectangle,
    layout: &ExplanationTextLayout,
    accent: Color,
    alpha: f32,
    scale: f32,
) {
    // Keep all narration inside the card even at compact viewport sizes or
    // enlarged display scales. The manual line layout sizes the card, while
    // `max_width` and the clip are final safeguards against font-metric drift.
    let text_clip = Rectangle::new(
        Point::new(rect.x + layout.content_inset, rect.y + 10.0 * scale),
        Size::new(layout.content_width, (rect.height - 20.0 * scale).max(1.0)),
    );
    frame.with_clip(text_clip, |frame| {
        for (index, line) in layout.title_lines.iter().enumerate() {
            frame.fill_text(canvas::Text {
                content: line.clone(),
                position: Point::new(
                    rect.x + layout.content_inset,
                    rect.y + layout.title_top + index as f32 * layout.title_step,
                ),
                max_width: layout.content_width,
                color: accent.scale_alpha(alpha),
                size: iced::Pixels(layout.title_size),
                font: fonts::REGULAR,
                ..canvas::Text::default()
            });
        }
        for (index, line) in layout.subtitle_lines.iter().enumerate() {
            frame.fill_text(canvas::Text {
                content: line.clone(),
                position: Point::new(
                    rect.x + layout.content_inset,
                    rect.y + layout.subtitle_top + index as f32 * layout.subtitle_step,
                ),
                max_width: layout.content_width,
                color: TEXT.scale_alpha(alpha),
                size: iced::Pixels(layout.subtitle_size),
                font: fonts::REGULAR,
                ..canvas::Text::default()
            });
        }
        let body_color = if layout.subtitle_lines.is_empty() {
            TEXT
        } else {
            TEXT_SOFT
        };
        for (index, line) in layout.body_lines.iter().enumerate() {
            frame.fill_text(canvas::Text {
                content: line.clone(),
                position: Point::new(
                    rect.x + layout.content_inset,
                    rect.y + layout.body_top + index as f32 * layout.body_step,
                ),
                max_width: layout.content_width,
                color: body_color.scale_alpha(alpha),
                size: iced::Pixels(layout.body_size),
                font: fonts::REGULAR,
                ..canvas::Text::default()
            });
        }
    });
}

fn explanation_position(
    target: Option<Point>,
    bounds: Rectangle,
    width: f32,
    height: f32,
    positions: &BTreeMap<String, Point>,
) -> (f32, f32) {
    let horizontal_margin = 22.0;
    let left = horizontal_margin;
    let right = (bounds.width - width - horizontal_margin).max(horizontal_margin);
    let top = 76.0;
    let bottom = (bounds.height - height - 68.0).max(76.0);
    // Of the four corner seats, take the one covering the fewest atoms;
    // among equally-empty seats, sit closest to the subject so the leader
    // line stays short (or vanishes entirely).
    let mut best = (left, bottom);
    let mut best_key = (usize::MAX, f32::INFINITY);
    for (x, y) in [(left, top), (right, top), (left, bottom), (right, bottom)] {
        // Inflated by a typical atom radius so a card flush against a
        // molecule still counts as covering it.
        let clearance = 46.0;
        let covered = positions
            .values()
            .filter(|point| {
                point.x > x - clearance
                    && point.x < x + width + clearance
                    && point.y > y - clearance
                    && point.y < y + height + clearance
            })
            .count();
        let center = Point::new(x + width * 0.5, y + height * 0.5);
        let closeness = target.map_or(0.0, |point| vector_magnitude(point - center));
        if (covered, closeness) < best_key {
            best_key = (covered, closeness);
            best = (x, y);
        }
    }
    best
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
    top_highlight: bool,
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
    if top_highlight {
        frame.fill(
            &rounded_rectangle(
                Rectangle::new(rect.position(), Size::new(rect.width, rect.height * 0.45)),
                radius,
            ),
            Color::WHITE.scale_alpha(alpha * 0.018),
        );
    }
}

fn rounded_rectangle(rect: Rectangle, radius: f32) -> Path {
    Path::rounded_rectangle(rect.position(), rect.size(), border::Radius::new(radius))
}

const fn explanation_color(kind: ExplanationLabelKind) -> Color {
    match kind {
        ExplanationLabelKind::ObservationExplanation => IONIC,
        ExplanationLabelKind::StructuralChangeExplanation
        | ExplanationLabelKind::CompletionNote => ACCENT,
    }
}

const fn explanation_title(kind: ExplanationLabelKind) -> &'static str {
    match kind {
        ExplanationLabelKind::StructuralChangeExplanation => "WHAT CHANGED",
        ExplanationLabelKind::ObservationExplanation => "OBSERVATION",
        ExplanationLabelKind::CompletionNote => "REACTION COMPLETE",
    }
}

fn wrap_words(text: &str, max_width: f32, font_size: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let candidate = if line.is_empty() {
            word.to_owned()
        } else {
            format!("{line} {word}")
        };
        if !line.is_empty() && estimated_text_width(&candidate, font_size) > max_width {
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

fn estimated_text_width(text: &str, font_size: f32) -> f32 {
    // Approximate Inter's advances conservatively. Canvas still receives an
    // exact `max_width`; this estimate exists to choose line breaks early
    // enough to calculate a card height that contains the resulting lines.
    text.chars()
        .map(|character| {
            let em = if character.is_whitespace() || "ilI.,'`:;!|".contains(character) {
                0.34
            } else if "mwMW@%&".contains(character) {
                0.95
            } else if character.is_ascii_uppercase() {
                0.72
            } else if character.is_ascii_digit() {
                0.64
            } else if character.is_ascii_punctuation() {
                0.58
            } else {
                0.62
            };
            em * font_size
        })
        .sum::<f32>()
        * 1.12
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
            RenderOperation::ProtonTransfer {
                hydrogen,
                donor,
                acceptor,
            } => vec![hydrogen.as_str(), donor.as_str(), acceptor.as_str()],
            RenderOperation::TransferMetallicElectron {
                donor_site,
                acceptor,
                ..
            } => vec![donor_site.as_str(), acceptor.as_str()],
            RenderOperation::MetallicMembership { site, .. } => vec![site.as_str()],
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

/// CPK-flavoured colours for the common classroom elements; everything else
/// borrows its periodic-family colour so it still matches the builder's
/// element keys instead of collapsing to one grey.
fn element_color(symbol: &str) -> Color {
    match symbol {
        "H" => LAB_DARK.chemistry.hydrogen,
        "Li" => LAB_DARK.chemistry.lithium,
        "C" => LAB_DARK.chemistry.carbon,
        "N" => LAB_DARK.chemistry.nitrogen,
        "O" => LAB_DARK.chemistry.oxygen,
        "F" => LAB_DARK.chemistry.fluorine,
        "Na" => LAB_DARK.chemistry.sodium,
        "P" => LAB_DARK.chemistry.phosphorus,
        "S" => LAB_DARK.chemistry.sulfur,
        "Cl" => LAB_DARK.chemistry.chlorine,
        "Fe" => LAB_DARK.chemistry.iron,
        "Cu" => LAB_DARK.chemistry.copper,
        "Br" => LAB_DARK.chemistry.bromine,
        "Ag" => LAB_DARK.chemistry.silver,
        "I" => LAB_DARK.chemistry.iodine,
        _ => elements::SUPPORTED
            .iter()
            .find(|element| element.symbol == symbol)
            .map_or(LAB_DARK.chemistry.element_default, |element| {
                crate::theme::category_color(element.category)
            }),
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
    fn metallic_site_core_charge_is_not_presented_as_an_ion() {
        let nickel = AtomState {
            id: "nickel".to_owned(),
            element: "Ni".to_owned(),
            formal_charge: 10,
            non_bonding_electrons: 0,
            unpaired_electrons: 0,
        };
        let metallic_frame = RenderFrame {
            atoms: vec![nickel.clone()],
            covalent_bonds: Vec::new(),
            ionic_associations: Vec::new(),
            metallic_domains: vec![RenderMetallicDomain {
                id: "nickel-domain".to_owned(),
                sites: vec![nickel.id.clone()],
                delocalized_electrons: 10,
            }],
        };

        assert!(is_metallic_site(&metallic_frame, &nickel.id));
        assert_eq!(
            displayed_charge(Some(&nickel), is_metallic_site(&metallic_frame, &nickel.id)),
            0
        );
        assert_eq!(displayed_charge(Some(&nickel), false), 10);

        let nickel_ion = AtomState {
            formal_charge: 2,
            non_bonding_electrons: 8,
            unpaired_electrons: 2,
            ..nickel
        };
        assert_eq!(displayed_charge(Some(&nickel_ion), false), 2);
    }

    #[test]
    fn transition_metals_omit_static_lewis_and_domain_electron_dots() {
        let nickel = AtomState {
            id: "nickel".to_owned(),
            element: "Ni".to_owned(),
            formal_charge: 10,
            non_bonding_electrons: 0,
            unpaired_electrons: 0,
        };
        let frame = RenderFrame {
            atoms: vec![nickel.clone()],
            covalent_bonds: Vec::new(),
            ionic_associations: Vec::new(),
            metallic_domains: vec![RenderMetallicDomain {
                id: "nickel-domain".to_owned(),
                sites: vec![nickel.id.clone()],
                delocalized_electrons: 10,
            }],
        };

        assert!(!shows_static_lewis_electrons("Ni"));
        assert!(!domain_shows_stationary_electrons(
            &frame,
            &frame,
            &frame.metallic_domains[0]
        ));
        assert!(shows_static_lewis_electrons("O"));
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
    fn grouped_same_sign_ions_expand_into_distinct_lattice_members() {
        let atoms = vec![
            ion_atom("k1", "K", 1),
            ion_atom("k2", "K", 1),
            ion_atom("c", "C", 0),
            ion_atom("o1", "O", 0),
            ion_atom("o2", "O", -1),
            ion_atom("o3", "O", -1),
        ];
        let bonds = vec![
            RenderBond {
                left: "c".to_owned(),
                right: "o1".to_owned(),
                order: 2,
                effective_order: None,
            },
            RenderBond {
                left: "c".to_owned(),
                right: "o2".to_owned(),
                order: 1,
                effective_order: None,
            },
            RenderBond {
                left: "c".to_owned(),
                right: "o3".to_owned(),
                order: 1,
                effective_order: None,
            },
        ];
        let mut components =
            split_render_ionic_component(&atoms, &bonds, vec!["k1".to_owned(), "k2".to_owned()], 2);
        components.extend(split_render_ionic_component(
            &atoms,
            &bonds,
            vec![
                "c".to_owned(),
                "o1".to_owned(),
                "o2".to_owned(),
                "o3".to_owned(),
            ],
            -2,
        ));
        let frame = RenderFrame {
            atoms,
            covalent_bonds: bonds,
            ionic_associations: vec![RenderIonicAssociation {
                id: "potassium-carbonate".to_owned(),
                components,
            }],
            metallic_domains: Vec::new(),
        };

        assert_eq!(frame.ionic_associations[0].components.len(), 3);
        assert_eq!(
            frame.ionic_associations[0]
                .components
                .iter()
                .map(|component| component.charge)
                .collect::<Vec<_>>(),
            [1, 1, -2]
        );
        let group = &lattice_groups(&frame)[0];
        assert_eq!(group.members.len(), 3);
        assert!(group.members.iter().enumerate().all(|(index, _)| {
            group
                .pairs
                .iter()
                .any(|(left, right)| *left == index || *right == index)
        }));
        assert_eq!(connected_components(&frame).len(), 1);

        let guarded = split_render_ionic_component(
            &frame.atoms,
            &frame.covalent_bonds,
            vec!["k1".to_owned(), "c".to_owned()],
            1,
        );
        assert_eq!(
            guarded.len(),
            1,
            "a neutral disconnected fragment must retain the validated aggregate"
        );
    }

    #[test]
    fn potassium_carbonate_catalogue_frame_connects_both_potassium_ions() {
        let request = crate::chemistry::ReactionRequest::acid_carbonate_gas_evolution(
            crate::chemistry::AlkaliMetal::Potassium,
            crate::chemistry::Halogen::Chlorine,
        );
        let run = crate::chemistry::run(request).expect("potassium carbonate reaction validates");
        let frame = RenderFrame::from(
            run.frames()
                .frames()
                .first()
                .expect("validated reaction has an initial frame"),
        );
        let association = frame
            .ionic_associations
            .iter()
            .find(|association| {
                association.components.iter().any(|component| {
                    component
                        .atoms
                        .iter()
                        .any(|id| atom(&frame, id).is_some_and(|atom| atom.element == "K"))
                })
            })
            .expect("initial frame contains potassium carbonate");
        let potassium_components = association
            .components
            .iter()
            .filter(|component| {
                component.atoms.len() == 1
                    && atom(&frame, &component.atoms[0]).is_some_and(|atom| atom.element == "K")
            })
            .collect::<Vec<_>>();

        assert_eq!(potassium_components.len(), 2);
        assert!(
            potassium_components
                .iter()
                .all(|component| component.charge == 1)
        );
        let group = lattice_groups(&frame)
            .into_iter()
            .find(|group| {
                group.members.iter().any(|member| {
                    member_component(&frame, *member)
                        .atoms
                        .iter()
                        .any(|id| atom(&frame, id).is_some_and(|atom| atom.element == "K"))
                })
            })
            .expect("potassium carbonate has a lattice group");
        for potassium in potassium_components {
            let member = group
                .members
                .iter()
                .position(|member| member_component(&frame, *member).atoms == potassium.atoms)
                .expect("each potassium ion is a lattice member");
            assert!(
                group
                    .pairs
                    .iter()
                    .any(|(left, right)| *left == member || *right == member),
                "each potassium ion must have an ionic connector"
            );
        }
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

        let groups = lattice_groups(&frame);
        assert_eq!(groups.len(), 1);
        let group = &groups[0];
        assert_eq!(group.members.len(), association.components.len());
        assert!(group.pairs.len() >= association.components.len() - 1);
        assert!(group.pairs.iter().all(|(left, right)| {
            member_component(&frame, group.members[*left])
                .charge
                .signum()
                != member_component(&frame, group.members[*right])
                    .charge
                    .signum()
        }));
        assert_eq!(connected_components(&frame).len(), 1);

        let bounds = Rectangle::new(Point::ORIGIN, Size::new(900.0, 600.0));
        let positions = layout_ordered(&frame, &connected_components(&frame), bounds);
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

        let previous = layout_ordered(&disconnected, &connected_components(&disconnected), bounds);
        let transition_after = flow_layout(&frame, &previous, &BTreeSet::new(), bounds);
        assert!(vector_magnitude(transition_after["mn2"] - transition_after["mn1"]) > 50.0);
    }

    fn ion_atom(id: &str, element: &str, formal_charge: i16) -> RenderAtom {
        RenderAtom {
            id: id.to_owned(),
            element: element.to_owned(),
            formal_charge,
            non_bonding_electrons: 0,
            unpaired_electrons: 0,
        }
    }

    fn salt_unit(id: &str, cation: &str, anion: &str) -> RenderIonicAssociation {
        RenderIonicAssociation {
            id: id.to_owned(),
            components: vec![
                RenderIonicComponent {
                    atoms: vec![cation.to_owned()],
                    charge: 1,
                },
                RenderIonicComponent {
                    atoms: vec![anion.to_owned()],
                    charge: -1,
                },
            ],
        }
    }

    #[test]
    fn same_salt_units_fuse_into_an_alternating_lattice() {
        let frame = RenderFrame {
            atoms: vec![
                ion_atom("na1", "Na", 1),
                ion_atom("cl1", "Cl", -1),
                ion_atom("na2", "Na", 1),
                ion_atom("cl2", "Cl", -1),
            ],
            covalent_bonds: Vec::new(),
            ionic_associations: vec![
                salt_unit("ionic[1].salt", "na1", "cl1"),
                salt_unit("ionic[2].salt", "na2", "cl2"),
            ],
            metallic_domains: Vec::new(),
        };

        let groups = lattice_groups(&frame);
        assert_eq!(groups.len(), 1, "same substance fuses into one lattice");
        assert_eq!(groups[0].members.len(), 4);
        // A 2x2 checkerboard closes into a ring of four opposite-charge
        // neighbour links.
        assert_eq!(groups[0].pairs.len(), 4);
        assert_eq!(connected_components(&frame).len(), 1);

        let bounds = Rectangle::new(Point::ORIGIN, Size::new(1600.0, 900.0));
        let positions = layout_ordered(&frame, &connected_components(&frame), bounds);
        // Every sodium's nearest neighbours are chlorides: like charges sit
        // on the diagonal, never adjacent.
        let na_gap = vector_magnitude(positions["na2"] - positions["na1"]);
        let cl_gap = vector_magnitude(positions["cl2"] - positions["cl1"]);
        for (sodium, chloride) in [
            ("na1", "cl1"),
            ("na1", "cl2"),
            ("na2", "cl1"),
            ("na2", "cl2"),
        ] {
            let gap = vector_magnitude(positions[chloride] - positions[sodium]);
            assert!(
                gap < na_gap && gap < cl_gap,
                "{sodium}-{chloride} gap {gap} vs Na-Na {na_gap} / Cl-Cl {cl_gap}"
            );
        }
    }

    #[test]
    fn non_one_to_one_lattice_keeps_every_ion_in_the_product_cluster() {
        let mut atoms = vec![ion_atom("ta1", "Ta", 5), ion_atom("ta2", "Ta", 5)];
        atoms.extend((1..=5).map(|index| ion_atom(&format!("o{index}"), "O", -2)));
        let frame = RenderFrame {
            atoms,
            covalent_bonds: Vec::new(),
            ionic_associations: vec![RenderIonicAssociation {
                id: "tantalum-pentoxide".to_owned(),
                components: vec![
                    RenderIonicComponent {
                        atoms: vec!["ta1".to_owned()],
                        charge: 5,
                    },
                    RenderIonicComponent {
                        atoms: vec!["ta2".to_owned()],
                        charge: 5,
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
                    RenderIonicComponent {
                        atoms: vec!["o4".to_owned()],
                        charge: -2,
                    },
                    RenderIonicComponent {
                        atoms: vec!["o5".to_owned()],
                        charge: -2,
                    },
                ],
            }],
            metallic_domains: Vec::new(),
        };

        let groups = lattice_groups(&frame);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].members.len(), 7);
        assert!(groups[0].members.iter().enumerate().all(|(index, _)| {
            groups[0]
                .pairs
                .iter()
                .any(|(left, right)| *left == index || *right == index)
        }));
        assert_eq!(connected_components(&frame).len(), 1);

        let bounds = Rectangle::new(Point::ORIGIN, Size::new(1600.0, 900.0));
        let positions = layout_ordered(&frame, &connected_components(&frame), bounds);
        assert_eq!(positions.len(), 7);
    }

    #[test]
    fn different_salts_keep_separate_lattices() {
        let frame = RenderFrame {
            atoms: vec![
                ion_atom("na1", "Na", 1),
                ion_atom("cl1", "Cl", -1),
                ion_atom("k1", "K", 1),
                ion_atom("br1", "Br", -1),
            ],
            covalent_bonds: Vec::new(),
            ionic_associations: vec![
                salt_unit("ionic[1].salt", "na1", "cl1"),
                salt_unit("ionic[2].other", "k1", "br1"),
            ],
            metallic_domains: Vec::new(),
        };

        assert_eq!(lattice_groups(&frame).len(), 2);
        assert_eq!(connected_components(&frame).len(), 2);
    }

    #[test]
    fn animation_phases_are_smooth_and_bounded() {
        let mut previous = 0.0;
        for step in 0_u8..=100 {
            let phase = animation_phase(f32::from(step) / 100.0);
            assert!((0.0..=1.0).contains(&phase));
            assert!(phase >= previous);
            previous = phase;
        }
        assert!(animation_phase(0.0).abs() < f32::EPSILON);
        assert!((animation_phase(1.0) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn introduction_grid_seats_first_interacting_partners_adjacent() {
        // Discovery order Li, Li, water, water with Li0 touching water1
        // first, then Li1 touching water0: partners pair up in story order
        // and nothing is seated twice.
        assert_eq!(seat_partners(&[(0, 3), (1, 2)], 4), vec![0, 3, 1, 2]);
        // A component whose partner is already taken falls back to the end.
        assert_eq!(seat_partners(&[(0, 2), (1, 2)], 3), vec![0, 2, 1]);
        // No interactions at all: original order is preserved.
        assert_eq!(seat_partners(&[], 3), vec![0, 1, 2]);
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
        let homes = flow_layout(&combined, &previous, &BTreeSet::new(), bounds);
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
        let separated = flow_layout(&split, &together, &BTreeSet::new(), bounds);
        assert!(
            vector_magnitude(separated["a"] - separated["b"]) >= 96.0,
            "split fragments relax apart: {separated:?}"
        );
    }

    #[test]
    fn operation_linked_components_are_pulled_tangent() {
        // Two unbonded atoms far apart whose components the active operation
        // links (a transfer donor and acceptor): they end tangent instead of
        // staying where their previous homes were.
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(1600.0, 900.0));
        let frame = RenderFrame {
            atoms: vec![atom_state("donor", 0, 0), atom_state("acceptor", 0, 0)],
            covalent_bonds: Vec::new(),
            ionic_associations: Vec::new(),
            metallic_domains: Vec::new(),
        };
        let previous: BTreeMap<String, Point> = [
            ("donor".to_owned(), Point::new(200.0, 450.0)),
            ("acceptor".to_owned(), Point::new(1200.0, 450.0)),
        ]
        .into();

        let unlinked = flow_layout(&frame, &previous, &BTreeSet::new(), bounds);
        let apart = vector_magnitude(unlinked["acceptor"] - unlinked["donor"]);
        assert!(apart > 900.0, "unlinked components stay put: {apart}");

        let linked = BTreeSet::from(["donor".to_owned(), "acceptor".to_owned()]);
        let pulled = flow_layout(&frame, &previous, &linked, bounds);
        let distance = vector_magnitude(pulled["acceptor"] - pulled["donor"]);
        // Tangent = the two component footprints (radius 24 + 30 padding
        // each) just touching, with the relaxation's tolerance either side.
        assert!(
            (96.0..=112.0).contains(&distance),
            "linked components meet tangent: {distance}"
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
    fn benzene_ring_lays_out_as_a_regular_hexagon_with_hydrogens_outside() {
        let carbons = (1..=6).map(|index| format!("c{index}")).collect::<Vec<_>>();
        let hydrogens = (1..=6).map(|index| format!("h{index}")).collect::<Vec<_>>();
        let mut atoms = Vec::new();
        let mut covalent_bonds = Vec::new();
        for index in 0..6 {
            atoms.push(ion_atom(&carbons[index], "C", 0));
            atoms.push(ion_atom(&hydrogens[index], "H", 0));
            covalent_bonds.push(RenderBond {
                left: carbons[index].clone(),
                right: carbons[(index + 1) % 6].clone(),
                order: if index % 2 == 0 { 2 } else { 1 },
                effective_order: None,
            });
            covalent_bonds.push(RenderBond {
                left: carbons[index].clone(),
                right: hydrogens[index].clone(),
                order: 1,
                effective_order: None,
            });
        }
        let frame = RenderFrame {
            atoms,
            covalent_bonds,
            ionic_associations: Vec::new(),
            metallic_domains: Vec::new(),
        };
        let component = frame
            .atoms
            .iter()
            .map(|atom| atom.id.clone())
            .collect::<Vec<_>>();

        let center = Point::new(300.0, 200.0);
        let mut positions = BTreeMap::new();
        layout_component(&frame, &component, center, &mut positions);
        assert_eq!(positions.len(), 12);

        // Ring atoms form a regular hexagon: equal circumradii and equal
        // consecutive edges matching the bond-length formula.
        let expected_edge = 2.0 * atom_visual_radius("C") + 26.0;
        let circumradius = expected_edge; // hexagon: edge == circumradius
        for index in 0..6 {
            let position = positions[&carbons[index]];
            assert!(
                (vector_magnitude(position - center) - circumradius).abs() < 0.1,
                "c{} off the circumcircle",
                index + 1
            );
            let edge =
                vector_magnitude(positions[&carbons[(index + 1) % 6]] - positions[&carbons[index]]);
            assert!(
                (edge - expected_edge).abs() < 0.1,
                "edge {edge} vs {expected_edge}"
            );
        }
        let ring_positions = carbons
            .iter()
            .map(|id| (positions[id].x.to_bits(), positions[id].y.to_bits()))
            .collect::<BTreeSet<_>>();
        assert_eq!(ring_positions.len(), 6, "ring atoms are distinct");

        // Each hydrogen hangs strictly outside its carbon.
        for index in 0..6 {
            assert!(
                vector_magnitude(positions[&hydrogens[index]] - center)
                    > vector_magnitude(positions[&carbons[index]] - center),
                "h{} inside the ring",
                index + 1
            );
        }
    }

    #[test]
    #[allow(clippy::cast_precision_loss)]
    fn naphthalene_lays_out_as_two_regular_hexagons_sharing_one_edge() {
        // Kekulé naphthalene: c9/c10 are the bridgeheads, h1..h8 sit on
        // c1..c8, and alternating bond orders keep every carbon at four.
        let bond = |left: &str, right: &str, order: u8| RenderBond {
            left: left.to_owned(),
            right: right.to_owned(),
            order,
            effective_order: None,
        };
        let mut atoms = (1..=10)
            .map(|index| ion_atom(&format!("c{index}"), "C", 0))
            .collect::<Vec<_>>();
        let mut covalent_bonds = [
            ("c1", "c2", 2),
            ("c2", "c3", 1),
            ("c3", "c4", 2),
            ("c4", "c9", 1),
            ("c9", "c10", 2),
            ("c10", "c1", 1),
            ("c9", "c5", 1),
            ("c5", "c6", 2),
            ("c6", "c7", 1),
            ("c7", "c8", 2),
            ("c8", "c10", 1),
        ]
        .into_iter()
        .map(|(left, right, order)| bond(left, right, order))
        .collect::<Vec<_>>();
        for index in 1..=8 {
            atoms.push(ion_atom(&format!("h{index}"), "H", 0));
            covalent_bonds.push(bond(&format!("c{index}"), &format!("h{index}"), 1));
        }
        let frame = RenderFrame {
            atoms,
            covalent_bonds,
            ionic_associations: Vec::new(),
            metallic_domains: Vec::new(),
        };
        let component = frame
            .atoms
            .iter()
            .map(|atom| atom.id.clone())
            .collect::<Vec<_>>();

        let center = Point::new(400.0, 300.0);
        let mut positions = BTreeMap::new();
        layout_component(&frame, &component, center, &mut positions);
        assert_eq!(positions.len(), 18);

        // Both rings are regular hexagons: every edge and every vertex's
        // distance from its ring centroid match the bond-length formula.
        let expected_edge = 2.0 * atom_visual_radius("C") + 26.0;
        let hexagons = [
            ["c1", "c2", "c3", "c4", "c9", "c10"],
            ["c5", "c6", "c7", "c8", "c10", "c9"],
        ];
        let centroid_of = |ids: &[&str]| {
            Point::new(
                ids.iter().map(|id| positions[*id].x).sum::<f32>() / ids.len() as f32,
                ids.iter().map(|id| positions[*id].y).sum::<f32>() / ids.len() as f32,
            )
        };
        for hexagon in hexagons {
            let ring_center = centroid_of(&hexagon);
            for index in 0..6 {
                let reach = vector_magnitude(positions[hexagon[index]] - ring_center);
                assert!(
                    (reach - expected_edge).abs() < 0.5,
                    "{} off its circumcircle: {reach} vs {expected_edge}",
                    hexagon[index]
                );
                let edge = vector_magnitude(
                    positions[hexagon[(index + 1) % 6]] - positions[hexagon[index]],
                );
                assert!(
                    (edge - expected_edge).abs() < 0.5,
                    "edge {edge} vs {expected_edge}"
                );
            }
        }
        // Exactly one shared edge: the ring centres sit two apothems apart.
        let ring_gap = vector_magnitude(centroid_of(&hexagons[1]) - centroid_of(&hexagons[0]));
        assert!(
            (ring_gap - expected_edge * 3.0_f32.sqrt()).abs() < 0.5,
            "ring centres {ring_gap} apart vs {}",
            expected_edge * 3.0_f32.sqrt()
        );

        // No overlapping atoms anywhere in the layout.
        let ids = positions.keys().cloned().collect::<Vec<_>>();
        for (slot, left) in ids.iter().enumerate() {
            for right in &ids[slot + 1..] {
                assert!(
                    vector_magnitude(positions[right] - positions[left])
                        > 0.5 * atom_visual_radius("H"),
                    "{left} and {right} overlap"
                );
            }
        }

        // Every hydrogen hangs outside the ring system.
        let system_centroid =
            centroid_of(&["c1", "c2", "c3", "c4", "c5", "c6", "c7", "c8", "c9", "c10"]);
        for index in 1..=8 {
            assert!(
                vector_magnitude(positions[&format!("h{index}")] - system_centroid)
                    > vector_magnitude(positions[&format!("c{index}")] - system_centroid),
                "h{index} inside the ring system"
            );
        }
    }

    #[test]
    #[allow(clippy::cast_precision_loss)]
    fn spiro_rings_lay_out_as_regular_polygons_on_opposite_sides() {
        // Spiro[4.4]nonane: two cyclopentane rings sharing only c5.
        let bond = |left: &str, right: &str| RenderBond {
            left: left.to_owned(),
            right: right.to_owned(),
            order: 1,
            effective_order: None,
        };
        let frame = RenderFrame {
            atoms: (1..=9)
                .map(|index| ion_atom(&format!("c{index}"), "C", 0))
                .collect(),
            covalent_bonds: [
                ("c1", "c2"),
                ("c2", "c3"),
                ("c3", "c4"),
                ("c4", "c5"),
                ("c5", "c1"),
                ("c5", "c6"),
                ("c6", "c7"),
                ("c7", "c8"),
                ("c8", "c9"),
                ("c9", "c5"),
            ]
            .into_iter()
            .map(|(left, right)| bond(left, right))
            .collect(),
            ionic_associations: Vec::new(),
            metallic_domains: Vec::new(),
        };
        let component = frame
            .atoms
            .iter()
            .map(|atom| atom.id.clone())
            .collect::<Vec<_>>();

        let center = Point::new(400.0, 300.0);
        let mut positions = BTreeMap::new();
        layout_component(&frame, &component, center, &mut positions);
        assert_eq!(positions.len(), 9);

        // Both rings are regular pentagons sharing exactly c5.
        let expected_edge = 2.0 * atom_visual_radius("C") + 26.0;
        let circumradius = expected_edge / (2.0 * (std::f32::consts::PI / 5.0).sin());
        let rings = [
            ["c5", "c1", "c2", "c3", "c4"],
            ["c5", "c6", "c7", "c8", "c9"],
        ];
        let centroid_of = |ids: &[&str]| {
            Point::new(
                ids.iter().map(|id| positions[*id].x).sum::<f32>() / ids.len() as f32,
                ids.iter().map(|id| positions[*id].y).sum::<f32>() / ids.len() as f32,
            )
        };
        for ring in rings {
            let ring_center = centroid_of(&ring);
            for index in 0..5 {
                let reach = vector_magnitude(positions[ring[index]] - ring_center);
                assert!(
                    (reach - circumradius).abs() < 0.5,
                    "{} off its circumcircle: {reach} vs {circumradius}",
                    ring[index]
                );
                let edge =
                    vector_magnitude(positions[ring[(index + 1) % 5]] - positions[ring[index]]);
                assert!(
                    (edge - expected_edge).abs() < 0.5,
                    "edge {edge} vs {expected_edge}"
                );
            }
        }

        // Ring centres sit on opposite sides of the shared atom, collinear
        // through it.
        let pivot = positions["c5"];
        let to_first = centroid_of(&rings[0]) - pivot;
        let to_second = centroid_of(&rings[1]) - pivot;
        assert!(
            to_first.x * to_second.x + to_first.y * to_second.y < 0.0,
            "ring centres share a side of the spiro atom"
        );
        assert!(
            (to_first.x * to_second.y - to_first.y * to_second.x).abs() < 1.0,
            "ring centres are not collinear through the spiro atom"
        );

        // No overlapping atoms anywhere in the layout.
        let ids = positions.keys().cloned().collect::<Vec<_>>();
        for (slot, left) in ids.iter().enumerate() {
            for right in &ids[slot + 1..] {
                assert!(
                    vector_magnitude(positions[right] - positions[left])
                        > 0.5 * atom_visual_radius("C"),
                    "{left} and {right} overlap"
                );
            }
        }
    }

    #[test]
    #[allow(clippy::cast_precision_loss)]
    fn norbornane_lays_out_as_a_perimeter_hexagon_with_the_bridge_inside() {
        // Bicyclo[2.2.1]heptane: bridgeheads c1/c4, two-carbon bridges
        // c2-c3 and c5-c6, one-carbon bridge c7.
        let bond = |left: &str, right: &str| RenderBond {
            left: left.to_owned(),
            right: right.to_owned(),
            order: 1,
            effective_order: None,
        };
        let frame = RenderFrame {
            atoms: (1..=7)
                .map(|index| ion_atom(&format!("c{index}"), "C", 0))
                .collect(),
            covalent_bonds: [
                ("c1", "c2"),
                ("c2", "c3"),
                ("c3", "c4"),
                ("c4", "c5"),
                ("c5", "c6"),
                ("c6", "c1"),
                ("c1", "c7"),
                ("c7", "c4"),
            ]
            .into_iter()
            .map(|(left, right)| bond(left, right))
            .collect(),
            ionic_associations: Vec::new(),
            metallic_domains: Vec::new(),
        };
        let component = frame
            .atoms
            .iter()
            .map(|atom| atom.id.clone())
            .collect::<Vec<_>>();

        let center = Point::new(400.0, 300.0);
        let mut positions = BTreeMap::new();
        layout_component(&frame, &component, center, &mut positions);
        assert_eq!(positions.len(), 7);

        // The perimeter (both two-carbon bridges plus the bridgeheads) is
        // a regular hexagon.
        let expected_edge = 2.0 * atom_visual_radius("C") + 26.0;
        let perimeter = ["c1", "c2", "c3", "c4", "c5", "c6"];
        let ring_center = Point::new(
            perimeter.iter().map(|id| positions[*id].x).sum::<f32>() / 6.0,
            perimeter.iter().map(|id| positions[*id].y).sum::<f32>() / 6.0,
        );
        for index in 0..6 {
            let reach = vector_magnitude(positions[perimeter[index]] - ring_center);
            assert!(
                (reach - expected_edge).abs() < 0.5,
                "{} off its circumcircle: {reach} vs {expected_edge}",
                perimeter[index]
            );
            let edge = vector_magnitude(
                positions[perimeter[(index + 1) % 6]] - positions[perimeter[index]],
            );
            assert!(
                (edge - expected_edge).abs() < 0.5,
                "edge {edge} vs {expected_edge}"
            );
        }

        // The bridge carbon sits strictly inside the perimeter: within the
        // hexagon's inscribed circle, and off the bridgehead chord's centre.
        let apothem = expected_edge * 3.0_f32.sqrt() * 0.5;
        let bridge_reach = vector_magnitude(positions["c7"] - ring_center);
        assert!(
            bridge_reach < apothem,
            "bridge outside the perimeter: {bridge_reach} vs apothem {apothem}"
        );
        assert!(
            bridge_reach > 1.0,
            "bridge atom sits exactly on the polygon centre"
        );

        // No overlapping atoms anywhere in the layout.
        let ids = positions.keys().cloned().collect::<Vec<_>>();
        for (slot, left) in ids.iter().enumerate() {
            for right in &ids[slot + 1..] {
                assert!(
                    vector_magnitude(positions[right] - positions[left])
                        > 0.5 * atom_visual_radius("C"),
                    "{left} and {right} overlap"
                );
            }
        }
    }

    #[test]
    fn acyclic_layout_keeps_the_radial_tree_shape() {
        // Methane: no cycle, so the root-centred fan is untouched.
        let frame = RenderFrame {
            atoms: vec![
                ion_atom("c", "C", 0),
                ion_atom("h1", "H", 0),
                ion_atom("h2", "H", 0),
                ion_atom("h3", "H", 0),
                ion_atom("h4", "H", 0),
            ],
            covalent_bonds: (1..=4)
                .map(|index| RenderBond {
                    left: "c".to_owned(),
                    right: format!("h{index}"),
                    order: 1,
                    effective_order: None,
                })
                .collect(),
            ionic_associations: Vec::new(),
            metallic_domains: Vec::new(),
        };
        let component = frame
            .atoms
            .iter()
            .map(|atom| atom.id.clone())
            .collect::<Vec<_>>();

        let center = Point::new(120.0, 80.0);
        let mut positions = BTreeMap::new();
        layout_component(&frame, &component, center, &mut positions);
        assert_eq!(positions["c"], center, "root carbon stays at the centre");
        let spoke = atom_visual_radius("C") + atom_visual_radius("H") + 26.0;
        for index in 1..=4 {
            let reach = vector_magnitude(positions[&format!("h{index}")] - center);
            assert!(
                (reach - spoke).abs() < 0.1,
                "h{index} reach {reach} vs {spoke}"
            );
        }
    }

    #[test]
    fn word_wrapping_preserves_content() {
        let source = "A shared electron pair forms a covalent bond";
        let lines = wrap_words(source, 120.0, 13.5);
        assert_eq!(lines.join(" "), source);
        assert!(lines.len() > 1);
    }

    #[test]
    fn explanation_wrapping_respects_available_width_at_multiple_scales() {
        let source = "The two atoms now share a pair of electrons — that shared pair is the new covalent bond holding them together.";
        for scale in [0.72_f32, 1.0, 1.35] {
            let panel_width = (410.0 * scale).clamp(260.0, 460.0);
            let content_width = panel_width - 60.0 * scale;
            let font_size = 13.5 * scale;
            let lines = wrap_words(source, content_width, font_size);

            assert_eq!(lines.join(" "), source);
            assert!(lines.len() > 1);
            assert!(
                lines
                    .iter()
                    .all(|line| estimated_text_width(line, font_size) <= content_width),
                "wrapped explanation exceeded {content_width}px at scale {scale}: {lines:?}"
            );
        }
    }

    #[test]
    fn electricity_suppresses_direct_interionic_electron_motion() {
        assert!(!shows_transfer_motion(true));
        assert!(shows_transfer_motion(false));
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
