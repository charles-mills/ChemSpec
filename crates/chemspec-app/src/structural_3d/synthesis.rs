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

/// How far the two reactant piles have merged into the product.
fn combination(progress: f32) -> f32 {
    smooth01((progress - 0.18) / 0.62)
}

/// Reaction-front glow: builds after mixing starts, gone once combined.
fn front_presence(progress: f32) -> f32 {
    smooth01((progress - 0.22) / 0.16) * (1.0 - smooth01((progress - 0.72) / 0.2))
}

/// A shallow tapered ceramic dish, opaque, sitting on the bench.
fn add_ceramic_dish(mesh: &mut Mesh, bench_top: f32, colour: [f32; 4]) {
    const SEGMENTS: u32 = 28;
    let dimmed = [colour[0] * 0.66, colour[1] * 0.64, colour[2] * 0.62, 1.0];
    let inner_colour = [colour[0] * 0.55, colour[1] * 0.54, colour[2] * 0.52, 1.0];
    let base_vertex = u32::try_from(mesh.vertices.len()).unwrap_or(u32::MAX);
    // Profile rings: outer base -> outer rim -> inner rim -> inner floor.
    let profile: [(f32, f32, [f32; 4], f32); 4] = [
        (DISH_RADIUS * 0.70, 0.010, dimmed, -0.6),
        (DISH_RADIUS, DISH_HEIGHT, dimmed, 0.35),
        (DISH_RADIUS * 0.93, DISH_HEIGHT - 0.012, inner_colour, 0.7),
        (DISH_RADIUS * 0.62, 0.040, inner_colour, 0.55),
    ];
    for (radius, height, ring_colour, normal_y) in profile {
        for segment in 0..=SEGMENTS {
            let angle = std::f32::consts::TAU * segment as f32 / SEGMENTS as f32;
            let radial = Vec3::new(angle.cos(), 0.0, angle.sin());
            let normal = (radial * (1.0 - normal_y.abs()) + Vec3::Y * normal_y).normalize_or_zero();
            mesh.vertices.push(Vertex {
                position: (radial * radius + Vec3::new(0.0, bench_top + height, 0.0)).to_array(),
                normal: normal.to_array(),
                color: ring_colour,
            });
        }
    }
    for ring in 0..3_u32 {
        for segment in 0..SEGMENTS {
            let a = base_vertex + ring * (SEGMENTS + 1) + segment;
            let b = a + SEGMENTS + 1;
            mesh.indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    // Inner floor fan.
    let centre_vertex = u32::try_from(mesh.vertices.len()).unwrap_or(u32::MAX);
    mesh.vertices.push(Vertex {
        position: [0.0, bench_top + 0.040, 0.0],
        normal: Vec3::Y.to_array(),
        color: inner_colour,
    });
    let floor_ring_start = base_vertex + 3 * (SEGMENTS + 1);
    for segment in 0..SEGMENTS {
        mesh.indices.extend_from_slice(&[
            centre_vertex,
            floor_ring_start + segment + 1,
            floor_ring_start + segment,
        ]);
    }
}

/// A heap of powder grains: a golden-angle spiral of faceted shards piled
/// into a low mound. `presence` scales grain size only, so topology holds.
#[allow(clippy::too_many_arguments)]
fn add_powder_heap(
    mesh: &mut Mesh,
    centre: Vec3,
    spread: f32,
    grain: f32,
    presence: f32,
    colour: [f32; 4],
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
/// combine, then rests against the rim. Always emitted.
fn add_mixing_rod(mesh: &mut Mesh, bench_top: f32, progress: f32, colour: [f32; 4], seed: u64) {
    let working = smooth01((progress - 0.10) / 0.08) * (1.0 - smooth01((progress - 0.66) / 0.12));
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
    add_cylinder(mesh, tip, top, 0.026, colour);
    add_sphere(mesh, tip, 0.032, colour, 4, 6);
}

/// Emissive glow beads tracing the boundary where the powders react.
fn add_reaction_front(
    meshes: &mut SceneMeshes,
    bench_top: f32,
    presence: f32,
    phase: f32,
    seed: u64,
) {
    const BEADS: u32 = 14;
    const FRONT_COLOUR: [f32; 4] = [1.0, 0.22, 0.035, 0.58];
    for bead in 0..BEADS {
        let angle = std::f32::consts::TAU * bead as f32 / BEADS as f32
            + (phase * 0.4 + seeded_unit(seed, bead, 451)).sin() * 0.2;
        let radial = 0.16 + seeded_unit(seed, bead, 452) * 0.10;
        let pulse = 0.6 + 0.4 * (phase * 2.2 + seeded_unit(seed, bead, 453) * 6.0).sin();
        let position = Vec3::new(
            angle.cos() * radial,
            bench_top + 0.075 + seeded_unit(seed, bead, 454) * 0.05,
            angle.sin() * radial,
        );
        let size = (0.014 + seeded_unit(seed, bead, 455) * 0.014) * presence * pulse;
        add_sphere(
            &mut meshes.emissive,
            position,
            size.max(0.000_5),
            alpha(FRONT_COLOUR, FRONT_COLOUR[3] * presence * pulse),
            3,
            5,
        );
    }
    add_ignition_sparks(
        &mut meshes.emissive,
        Vec3::new(0.0, bench_top + 0.09, 0.0),
        presence * 0.7,
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
        [bound[0] * 0.78, bound[1] * 0.78, bound[2] * 0.78, bound[3]]
    };
    add_ceramic_dish(
        &mut meshes.opaque,
        layout.bench_top,
        colour(ClipColour::ReactionVessel),
    );
    let merge = combination(progress);
    let floor_y = layout.bench_top + 0.075;
    // The two powder charges slide together and shrink as the product takes
    // over: faceted shard heaps, so the piles read as matte grains rather
    // than washed-out smooth domes.
    let approach = PILE_OFFSET * (1.0 - merge * 0.72);
    add_powder_heap(
        &mut meshes.opaque,
        Vec3::new(-approach, floor_y, 0.0),
        0.24,
        0.055,
        (1.0 - merge).max(0.04),
        colour(ClipColour::ReactantA),
        16,
        seed.rotate_left(3),
    );
    add_powder_heap(
        &mut meshes.opaque,
        Vec3::new(approach, floor_y, 0.02),
        0.24,
        0.055,
        (1.0 - merge).max(0.04),
        colour(ClipColour::ReactantB),
        16,
        seed.rotate_left(7),
    );
    add_powder_heap(
        &mut meshes.opaque,
        Vec3::new(0.0, floor_y, 0.0),
        0.34,
        0.062,
        merge.max(0.02),
        colour(ClipColour::SynthesisProduct),
        26,
        seed.rotate_left(11),
    );
    if synthesis.show_reaction_front {
        add_reaction_front(
            meshes,
            layout.bench_top,
            front_presence(progress),
            phase,
            seed.rotate_left(15),
        );
    }
    add_mixing_rod(
        &mut meshes.opaque,
        layout.bench_top,
        progress,
        colour(ClipColour::MixingTool),
        seed,
    );
}
