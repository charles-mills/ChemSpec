//! Live force simulation for the 2D structural view.
//!
//! Two regimes per atom, blended by an "excitement" level. Calm atoms track
//! the choreographed layout exactly: home motion is carried straight into
//! their position, strong anchors absorb drift, and heavy damping kills any
//! wobble — so scripted playback looks authored, not simulated. Atoms the
//! user grabs (and anything they knock into) become excited: weak anchors,
//! light damping, springy bonds — that's where the wiggle lives. Excitement
//! decays back to calm about a second after the interaction ends.

use std::collections::BTreeMap;

use iced::{Point, Size, Vector};

/// The simulation runs in this fixed coordinate space regardless of widget
/// size; the view fits it into its bounds with one uniform transform.
pub const VIRTUAL: Size = Size::new(1600.0, 900.0);

const SUBSTEPS: u32 = 3;
const DT: f32 = 1.0 / 90.0;
const SPRING_STRENGTH: f32 = 220.0;
const REPULSION: f32 = 350.0;
const DRAG_STRENGTH: f32 = 1_400.0;
const MAX_SPEED: f32 = 2_400.0;
/// Damping per substep: calm atoms settle almost immediately, excited atoms
/// stay springy.
const CALM_DAMPING: f32 = 0.78;
const EXCITED_DAMPING: f32 = 0.93;
/// Anchor-strength multipliers over the spec's base strength.
const CALM_ANCHOR: f32 = 5.5;
const EXCITED_ANCHOR: f32 = 0.6;
/// Excitement decay per substep (~1.5 s back to calm at 30 fps).
const EXCITEMENT_DECAY: f32 = 0.985;
/// Speed (virtual px/s) at which a knocked atom counts as fully excited.
const EXCITING_SPEED: f32 = 700.0;

#[derive(Debug, Clone)]
pub struct AtomSpec {
    pub id: String,
    pub radius: f32,
    pub seed: Point,
}

#[derive(Debug, Clone)]
pub struct Spring {
    pub a: String,
    pub b: String,
    pub rest: f32,
    pub strength: f32,
}

/// A weak per-atom pull toward its choreographed layout position. Keeps
/// the global arrangement while springs and repulsion own local geometry.
#[derive(Debug, Clone)]
pub struct Anchor {
    pub atom: String,
    pub home: Point,
    pub strength: f32,
}

#[derive(Debug, Clone, Default)]
pub struct WorldSpec {
    pub atoms: Vec<AtomSpec>,
    pub springs: Vec<Spring>,
    pub anchors: Vec<Anchor>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DragTarget {
    Atom(String),
    Bond(String, String),
}

#[derive(Debug, Clone)]
struct Drag {
    cursor: Point,
    /// Grabbed atoms with their offset from the cursor at grab time.
    holds: Vec<(String, Vector)>,
}

#[derive(Debug, Clone, Copy)]
struct Body {
    position: Point,
    velocity: Vector,
    radius: f32,
    /// 0 = calm (tracks choreography exactly), 1 = fully springy.
    excitement: f32,
}

#[derive(Debug, Default)]
pub struct Simulation {
    bodies: BTreeMap<String, Body>,
    drag: Option<Drag>,
    /// Home positions from the previous step, for carrying scripted motion.
    last_homes: BTreeMap<String, Point>,
}

impl Simulation {
    #[must_use]
    pub fn positions(&self) -> BTreeMap<String, Point> {
        self.bodies
            .iter()
            .map(|(id, body)| (id.clone(), body.position))
            .collect()
    }

    pub fn begin_drag(&mut self, target: &DragTarget, cursor: Point) {
        let atoms: Vec<&str> = match target {
            DragTarget::Atom(atom) => vec![atom],
            DragTarget::Bond(left, right) => vec![left, right],
        };
        let holds = atoms
            .into_iter()
            .filter_map(|atom| {
                self.bodies
                    .get(atom)
                    .map(|body| (atom.to_owned(), body.position - cursor))
            })
            .collect::<Vec<_>>();
        if !holds.is_empty() {
            self.drag = Some(Drag { cursor, holds });
        }
    }

    pub fn move_drag(&mut self, cursor: Point) {
        if let Some(drag) = &mut self.drag {
            drag.cursor = cursor;
        }
    }

    pub fn end_drag(&mut self) {
        self.drag = None;
    }

    /// Advances the simulation one display tick against the current world.
    pub fn step(&mut self, spec: &WorldSpec) {
        self.sync(spec);
        self.carry_home_motion(spec);
        for _ in 0..SUBSTEPS {
            self.substep(spec);
        }
    }

    /// Moves calm atoms by exactly as much as their choreographed home moved
    /// this tick, so scripted playback has no lag and injects no velocity.
    fn carry_home_motion(&mut self, spec: &WorldSpec) {
        for anchor in &spec.anchors {
            if let Some(body) = self.bodies.get_mut(&anchor.atom) {
                if let Some(previous) = self.last_homes.get(&anchor.atom) {
                    let delta = anchor.home - *previous;
                    body.position += delta * (1.0 - body.excitement);
                }
                self.last_homes.insert(anchor.atom.clone(), anchor.home);
            }
        }
        self.last_homes.retain(|id, _| self.bodies.contains_key(id));
    }

    /// Inserts new atoms at their seeds, refreshes radii, drops stale atoms.
    fn sync(&mut self, spec: &WorldSpec) {
        let mut kept = BTreeMap::new();
        for atom in &spec.atoms {
            let body = self.bodies.get(&atom.id).copied().map_or(
                Body {
                    position: atom.seed,
                    velocity: Vector::new(0.0, 0.0),
                    radius: atom.radius,
                    excitement: 0.0,
                },
                |existing| Body {
                    radius: atom.radius,
                    ..existing
                },
            );
            kept.insert(atom.id.clone(), body);
        }
        self.bodies = kept;
    }

    #[allow(clippy::cast_precision_loss)]
    fn substep(&mut self, spec: &WorldSpec) {
        let ids: Vec<String> = self.bodies.keys().cloned().collect();
        let mut forces: BTreeMap<&str, Vector> = ids
            .iter()
            .map(|id| (id.as_str(), Vector::new(0.0, 0.0)))
            .collect();

        // Local pairwise repulsion: strong inside overlap, fading to nothing
        // beyond about two diameters. Clusters keep molecules together, so
        // repulsion never needs long range.
        for (index, a) in ids.iter().enumerate() {
            for b in ids.iter().skip(index + 1) {
                let (body_a, body_b) = (self.bodies[a], self.bodies[b]);
                let delta = body_a.position - body_b.position;
                let distance = length(delta).max(1.0);
                let desired = body_a.radius + body_b.radius + 26.0;
                if distance < desired * 2.2 {
                    // Hard contact always wins over anchors.
                    let contact = if distance < body_a.radius + body_b.radius {
                        4.0
                    } else {
                        1.0
                    };
                    let push = contact * REPULSION * (desired * desired) / (distance * distance);
                    let direction = delta * (1.0 / distance);
                    add(&mut forces, a, direction * push);
                    add(&mut forces, b, direction * -push);
                }
            }
        }

        for spring in &spec.springs {
            let (Some(body_a), Some(body_b)) =
                (self.bodies.get(&spring.a), self.bodies.get(&spring.b))
            else {
                continue;
            };
            let delta = body_b.position - body_a.position;
            let distance = length(delta).max(1.0);
            let stretch = distance - spring.rest;
            let pull = delta * (1.0 / distance) * (stretch * SPRING_STRENGTH * spring.strength);
            add(&mut forces, &spring.a, pull);
            add(&mut forces, &spring.b, pull * -1.0);
        }

        for anchor in &spec.anchors {
            if let Some(body) = self.bodies.get(&anchor.atom) {
                let strength = anchor.strength
                    * (CALM_ANCHOR + (EXCITED_ANCHOR - CALM_ANCHOR) * body.excitement);
                let correction = (anchor.home - body.position) * strength;
                add(&mut forces, &anchor.atom, correction);
            }
        }

        if let Some(drag) = &self.drag {
            for (atom, offset) in &drag.holds {
                if let Some(body) = self.bodies.get(atom) {
                    let goal = drag.cursor + *offset;
                    let correction = (goal - body.position) * DRAG_STRENGTH;
                    add(&mut forces, atom, correction);
                }
            }
        }

        let dragged: Vec<String> = self
            .drag
            .iter()
            .flat_map(|drag| drag.holds.iter().map(|(atom, _)| atom.clone()))
            .collect();
        for id in &ids {
            let force = forces[id.as_str()];
            let body = self.bodies.get_mut(id).expect("body exists");
            // Heavier atoms accelerate less: mass grows with drawn area.
            let mass = (body.radius / 24.0).powi(2).max(0.2);
            let damping = CALM_DAMPING + (EXCITED_DAMPING - CALM_DAMPING) * body.excitement;
            let mut velocity = (body.velocity + force * (DT / mass)) * damping;
            let speed = length(velocity);
            if speed > MAX_SPEED {
                velocity *= MAX_SPEED / speed;
            }
            body.velocity = velocity;
            body.position += velocity * DT;
            // Carried home motion adds no velocity, so only real pokes —
            // dragging, or being knocked by a dragged neighbour — excite.
            body.excitement = if dragged.contains(id) {
                1.0
            } else {
                (body.excitement * EXCITEMENT_DECAY).max((speed / EXCITING_SPEED).min(1.0))
            };
            body.position.x = body.position.x.clamp(40.0, VIRTUAL.width - 40.0);
            body.position.y = body.position.y.clamp(40.0, VIRTUAL.height - 40.0);
        }
    }
}

fn add(forces: &mut BTreeMap<&str, Vector>, id: &str, force: Vector) {
    if let Some(entry) = forces.get_mut(id) {
        *entry += force;
    }
}

fn length(vector: Vector) -> f32 {
    vector.x.hypot(vector.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(atoms: &[(&str, f32, Point)], springs: &[(&str, &str, f32)]) -> WorldSpec {
        WorldSpec {
            atoms: atoms
                .iter()
                .map(|(id, radius, seed)| AtomSpec {
                    id: (*id).to_owned(),
                    radius: *radius,
                    seed: *seed,
                })
                .collect(),
            springs: springs
                .iter()
                .map(|(a, b, rest)| Spring {
                    a: (*a).to_owned(),
                    b: (*b).to_owned(),
                    rest: *rest,
                    strength: 1.0,
                })
                .collect(),
            anchors: atoms
                .iter()
                .map(|(id, _, seed)| Anchor {
                    atom: (*id).to_owned(),
                    home: *seed,
                    strength: 10.0,
                })
                .collect(),
        }
    }

    #[test]
    fn springs_settle_atoms_near_their_rest_length() {
        let world = spec(
            &[
                ("a", 24.0, Point::new(700.0, 450.0)),
                ("b", 24.0, Point::new(702.0, 452.0)),
            ],
            &[("a", "b", 90.0)],
        );
        let mut simulation = Simulation::default();
        for _ in 0..600 {
            simulation.step(&world);
        }
        let positions = simulation.positions();
        let delta = positions["a"] - positions["b"];
        let distance = delta.x.hypot(delta.y);
        // Coincident anchors compress the spring slightly; near rest is fine.
        assert!(
            (distance - 90.0).abs() < 14.0,
            "settled distance {distance}"
        );
    }

    #[test]
    fn unbonded_atoms_do_not_overlap() {
        let world = spec(
            &[
                ("a", 24.0, Point::new(795.0, 450.0)),
                ("b", 24.0, Point::new(805.0, 450.0)),
            ],
            &[],
        );
        let mut simulation = Simulation::default();
        for _ in 0..600 {
            simulation.step(&world);
        }
        let positions = simulation.positions();
        let delta = positions["a"] - positions["b"];
        // Contact is acceptable; visible interpenetration is not.
        assert!(delta.x.hypot(delta.y) > 46.0, "atoms separated");
    }

    #[test]
    fn dragging_pins_and_release_relaxes() {
        let world = spec(
            &[
                ("a", 24.0, Point::new(800.0, 450.0)),
                ("b", 24.0, Point::new(890.0, 450.0)),
            ],
            &[("a", "b", 90.0)],
        );
        let mut simulation = Simulation::default();
        for _ in 0..200 {
            simulation.step(&world);
        }
        simulation.begin_drag(
            &DragTarget::Atom("a".to_owned()),
            simulation.positions()["a"],
        );
        simulation.move_drag(Point::new(500.0, 300.0));
        for _ in 0..200 {
            simulation.step(&world);
        }
        let dragged = simulation.positions()["a"];
        assert!(
            dragged.distance(Point::new(500.0, 300.0)) < 30.0,
            "{dragged:?}"
        );
        simulation.end_drag();
        for _ in 0..600 {
            simulation.step(&world);
        }
        let positions = simulation.positions();
        let delta = positions["a"] - positions["b"];
        let distance = delta.x.hypot(delta.y);
        assert!((distance - 90.0).abs() < 14.0, "relaxed to {distance}");
    }
}
