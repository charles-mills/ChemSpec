//! Procedural solid–solid synthesis scene: two powders in a ceramic dish,
//! worked together with a rod, combining into a single product pile — with
//! a glowing reaction front where the profiles authorize one.
//!
//! Everything is a deterministic function of (plan, progress): fixed entity
//! populations whose sizes animate, never appearing or vanishing.

#![allow(clippy::wildcard_imports, clippy::cast_precision_loss)]

use super::*;

/// Dish footprint on the bench.
const DISH_RADIUS: f32 = 0.74;
const DISH_HEIGHT: f32 = 0.13;
/// Reactant pile offsets either side of the dish centre.
const PILE_OFFSET: f32 = 0.17;

/// How far the two reactant piles have merged into one mixed pile.
fn combination(progress: f32) -> f32 {
    smooth01((progress - 0.16) / 0.34)
}

/// The rod tip heats to ignition while it rests against the mixed pile.
fn tip_glow(progress: f32) -> f32 {
    smooth01((progress - 0.48) / 0.10) * (1.0 - smooth01((progress - 0.74) / 0.10))
}

/// How far the reaction front has swept from the ignition point.
fn front_radius(progress: f32) -> f32 {
    smooth01((progress - 0.58) / 0.32) * 0.85
}

/// The burning front line itself: lit while the sweep crosses the pile.
fn front_line(progress: f32) -> f32 {
    smooth01((progress - 0.58) / 0.05) * (1.0 - smooth01((progress - 0.88) / 0.08))
}

/// Where the rod tip ignites the pile: on the pile's edge, under the rod's
/// resting position.
fn ignition_point(floor_y: f32) -> Vec3 {
    let toward = Vec3::new(DISH_RADIUS * 0.62, 0.0, -DISH_RADIUS * 0.30).normalize_or_zero();
    Vec3::new(0.0, floor_y, 0.0) + toward * 0.30
}

/// A heap of powder grains: a golden-angle spiral of faceted shards piled
/// into a low mound. `presence` scales grain size only, so topology holds.
/// Grains alternate between the two `colours` (pass the same twice for a
/// single-species pile); if a `front` (origin, radius, product colour) is
/// given, each grain the sweep has passed converts to the product colour —
/// the burn leaves dark product behind it, grain by grain.
#[allow(clippy::too_many_arguments)]
pub(super) fn add_powder_heap(
    mesh: &mut Mesh,
    centre: Vec3,
    spread: f32,
    grain: f32,
    presence: f32,
    colours: [[f32; 4]; 2],
    front: Option<(Vec3, f32, [f32; 4])>,
    count: u32,
    seed: u64,
) {
    for index in 0..count {
        let angle = index as f32 * 2.399_963_1 + seed_phase(seed, 461);
        let radius = (index as f32 / count.max(1) as f32).sqrt();
        let position = centre
            + Vec3::new(
                angle.cos() * radius * spread,
                (1.0 - radius * radius) * spread * 0.42 * presence,
                angle.sin() * radius * spread,
            );
        let mut colour = colours[(index % 2) as usize];
        if let Some((origin, front_radius, product)) = front {
            let reach = Vec3::new(position.x - origin.x, 0.0, position.z - origin.z).length();
            colour = mix_color(colour, product, smooth01((front_radius - reach) / 0.07));
        }
        let size = grain * (0.7 + seeded_unit(seed, index, 462) * 0.6) * presence;
        add_shard(
            mesh,
            position,
            Vec3::new(size * 0.9, size * 0.7, size * 0.8).max(Vec3::splat(0.000_5)),
            Quat::from_rotation_y(angle * 1.7)
                * Quat::from_rotation_x(seeded_unit(seed, index, 463) * 0.5),
            colour,
            seed.wrapping_add(u64::from(index)),
        );
    }
}

/// The mixing rod: enters, works the piles in a slow orbit while they
/// combine, then rests against the pile's edge — where its tip heats to a
/// glow and lights the mixture.
fn add_mixing_rod(
    meshes: &mut SceneMeshes,
    bench_top: f32,
    progress: f32,
    colour: [f32; 4],
    ignites: bool,
    seed: u64,
) {
    let working = smooth01((progress - 0.10) / 0.08) * (1.0 - smooth01((progress - 0.50) / 0.10));
    let orbit = progress * 9.0 + seed_phase(seed, 441);
    let reach = 0.20 * working;
    let tip = Vec3::new(
        orbit.cos() * reach,
        bench_top + 0.075 + 0.02 * working,
        orbit.sin() * reach,
    );
    let rest_tip = Vec3::new(DISH_RADIUS * 0.62, bench_top + 0.10, -DISH_RADIUS * 0.30);
    let blend = working;
    let tip = rest_tip.lerp(tip, blend);
    let top = tip + Vec3::new(0.34 - 0.16 * blend, 0.98, 0.30 - 0.24 * blend);
    add_cylinder(&mut meshes.opaque, tip, top, 0.026, colour);
    add_sphere(&mut meshes.opaque, tip, 0.032, colour, 4, 6);
    // The resting tip glows toward ignition, then hands over to the front.
    // Gated on the per-plan front authorization, so topology holds and a
    // frontless plan never invents an igniter.
    if ignites {
        let glow = tip_glow(progress);
        add_sphere(
            &mut meshes.emissive,
            tip,
            (0.045 * glow).max(0.000_5),
            [1.0, 0.42, 0.10, 0.65 * glow.max(0.02)],
            4,
            6,
        );
    }
}

/// The burning front line: emissive beads strung along the arc the sweep
/// has reached, with smoke curling up and sparks spitting from the line —
/// dark converted product behind, unburned mixture ahead.
fn add_reaction_front(
    meshes: &mut SceneMeshes,
    origin: Vec3,
    radius: f32,
    presence: f32,
    phase: f32,
    seed: u64,
) {
    const BEADS: u32 = 14;
    const SMOKE: u32 = 6;
    const FRONT_COLOUR: [f32; 4] = [1.0, 0.22, 0.035, 0.58];
    // Presence fades any bead whose arc position has left the pile.
    let on_pile = |position: Vec3| {
        1.0 - smooth01((Vec3::new(position.x, 0.0, position.z).length() - 0.36) / 0.08)
    };
    for bead in 0..BEADS {
        let angle = seeded_unit(seed, bead, 451) * std::f32::consts::TAU
            + (phase * 0.4 + seeded_unit(seed, bead, 456)).sin() * 0.12;
        let position = origin
            + Vec3::new(angle.cos(), 0.0, angle.sin()) * radius
            + Vec3::Y * (0.012 + seeded_unit(seed, bead, 454) * 0.03);
        let pulse = 0.6 + 0.4 * (phase * 2.2 + seeded_unit(seed, bead, 453) * 6.0).sin();
        let size =
            (0.014 + seeded_unit(seed, bead, 455) * 0.014) * presence * pulse * on_pile(position);
        add_sphere(
            &mut meshes.emissive,
            position,
            size.max(0.000_5),
            alpha(FRONT_COLOUR, FRONT_COLOUR[3] * presence * pulse),
            3,
            5,
        );
    }
    // Thin smoke rising off the burn line.
    for puff in 0..SMOKE {
        let angle = seeded_unit(seed, puff, 457) * std::f32::consts::TAU;
        let anchor = origin + Vec3::new(angle.cos(), 0.0, angle.sin()) * radius;
        let age = (phase * (0.5 + seeded_unit(seed, puff, 458) * 0.4)
            + seeded_unit(seed, puff, 459))
        .fract();
        let size = (0.030 + 0.05 * age) * presence * on_pile(anchor);
        add_sphere(
            &mut meshes.translucent,
            anchor + Vec3::Y * (0.03 + age * 0.30),
            size.max(0.000_5),
            [0.62, 0.60, 0.58, 0.30 * (1.0 - age * 0.8) * presence],
            4,
            6,
        );
    }
    // Sparks spit from a point that runs along the line as it burns.
    let spit_angle = phase * 0.35 + seed_phase(seed, 460);
    let spit = origin + Vec3::new(spit_angle.cos(), 0.0, spit_angle.sin()) * radius;
    add_ignition_sparks(
        &mut meshes.emissive,
        spit + Vec3::Y * 0.03,
        presence * 0.8 * on_pile(spit),
        phase,
        seed.rotate_left(5),
    );
}

pub(super) fn add_synthesis_assembly(
    meshes: &mut SceneMeshes,
    plan: &ScenePlan,
    layout: SceneLayout,
    progress: f32,
    ordinal: u16,
    ordinal_progress: f32,
) {
    let synthesis = plan
        .solid_solid_synthesis
        .as_ref()
        .expect("validated solid-solid synthesis assembly has material bindings");
    let seed = plan_seed(plan);
    let progress = progress.clamp(0.0, 1.0);
    let phase = progress * 12.0;
    let colour = |clip_colour| {
        let bound =
            synthesis_combination_track_colour(clip_colour, synthesis, ordinal, ordinal_progress);
        // Matte powder reads darker than the fired dish it sits in.
        [bound[0] * 0.62, bound[1] * 0.62, bound[2] * 0.62, bound[3]]
    };
    add_ceramic_dish(
        &mut meshes.opaque,
        layout.bench_top,
        DISH_RADIUS,
        DISH_HEIGHT,
        colour(ClipColour::ReactionVessel),
    );
    let merge = combination(progress);
    let floor_y = layout.bench_top + 0.075;
    let reactant_a = colour(ClipColour::ReactantA);
    let reactant_b = colour(ClipColour::ReactantB);
    let product = colour(ClipColour::SynthesisProduct);
    // With an authorized front the burn sweeps out from the ignition point;
    // without one the conversion is a quiet uniform darkening from the
    // pile's own centre.
    let ignition = ignition_point(floor_y);
    let front = if synthesis.show_reaction_front {
        (ignition, front_radius(progress), product)
    } else {
        (
            Vec3::new(0.0, floor_y, 0.0),
            smooth01((progress - 0.55) / 0.30) * 2.0,
            product,
        )
    };
    // The two powder charges slide together into one salt-and-pepper pile:
    // faceted shard heaps, so the piles read as matte grains rather than
    // washed-out smooth domes.
    let approach = PILE_OFFSET * (1.0 - merge * 0.72);
    add_powder_heap(
        &mut meshes.opaque,
        Vec3::new(-approach, floor_y, 0.0),
        0.24,
        0.055,
        (1.0 - merge).max(0.04),
        [reactant_a, reactant_a],
        Some(front),
        16,
        seed.rotate_left(3),
    );
    add_powder_heap(
        &mut meshes.opaque,
        Vec3::new(approach, floor_y, 0.02),
        0.24,
        0.055,
        (1.0 - merge).max(0.04),
        [reactant_b, reactant_b],
        Some(front),
        16,
        seed.rotate_left(7),
    );
    // The mixed pile: alternating grains of the two reactants, converted to
    // dark product grain-by-grain as the front passes.
    add_powder_heap(
        &mut meshes.opaque,
        Vec3::new(0.0, floor_y, 0.0),
        0.34,
        0.062,
        merge.max(0.02),
        [reactant_a, reactant_b],
        Some(front),
        26,
        seed.rotate_left(11),
    );
    if synthesis.show_reaction_front {
        add_reaction_front(
            meshes,
            ignition,
            front_radius(progress),
            front_line(progress),
            phase,
            seed.rotate_left(15),
        );
    }
    add_mixing_rod(
        meshes,
        layout.bench_top,
        progress,
        colour(ClipColour::MixingTool),
        synthesis.show_reaction_front,
        seed,
    );
}
