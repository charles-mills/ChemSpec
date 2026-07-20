//! Procedural metal-displacement scene: a metal strip standing in a salt
//! solution, eroding below the waterline while the displaced metal crusts
//! onto it, flakes off, and settles — as the solution's colour drains from
//! around the strip.
//!
//! Everything is a deterministic function of (plan, progress): fixed entity
//! populations whose sizes animate, never appearing or vanishing.

#![allow(clippy::wildcard_imports, clippy::cast_precision_loss)]

use super::*;

/// Solution level above the bench, preserving the authored basin.
const BASIN_LEVEL: f32 = 1.13;
/// Strip centre offset from the basin axis.
const STRIP_X: f32 = 0.20;
/// Strip footprint.
const STRIP_WIDTH: f32 = 0.30;
const STRIP_THICKNESS: f32 = 0.055;
const STRIP_BOTTOM: f32 = 0.30;
const STRIP_TOP: f32 = 1.46;
/// Deposits crust from here; flakes shed later, as authored.
const DEPOSIT_START: f32 = 53.0 / 179.0;
const FLAKE_START: f32 = 103.0 / 179.0;

/// How much deposit has crusted on by `progress`.
fn deposit_growth(progress: f32) -> f32 {
    smooth01((progress - DEPOSIT_START) / (1.0 - DEPOSIT_START))
}

/// The strip: a plain slab standing off-centre in the basin, its top
/// leaning a few degrees for life.
fn add_metal_strip(mesh: &mut Mesh, bench_top: f32, colour: [f32; 4]) {
    let centre = Vec3::new(
        STRIP_X,
        bench_top + (STRIP_BOTTOM + STRIP_TOP) * 0.5,
        0.0,
    );
    add_box(
        mesh,
        centre,
        Vec3::new(STRIP_WIDTH, STRIP_TOP - STRIP_BOTTOM, STRIP_THICKNESS),
        colour,
    );
}

/// Seeded points on the submerged strip faces, for pits and deposit nubs.
fn strip_point(bench_top: f32, surface_y: f32, index: u32, channel: u32, seed: u64) -> Vec3 {
    let u = seeded_unit(seed, index, channel);
    let v = seeded_unit(seed, index, channel + 1);
    let front = seeded_unit(seed, index, channel + 2) > 0.5;
    let z = if front { 1.0 } else { -1.0 } * (STRIP_THICKNESS * 0.5 + 0.004);
    Vec3::new(
        STRIP_X - STRIP_WIDTH * 0.5 + u * STRIP_WIDTH,
        bench_top + STRIP_BOTTOM + 0.04 + v * (surface_y - bench_top - STRIP_BOTTOM - 0.10),
        z,
    )
}

/// Dark pitting where the strip dissolves below the waterline.
fn add_erosion_pits(
    mesh: &mut Mesh,
    bench_top: f32,
    surface_y: f32,
    growth: f32,
    colour: [f32; 4],
    seed: u64,
) {
    const PITS: u32 = 12;
    for pit in 0..PITS {
        let position = strip_point(bench_top, surface_y, pit, 411, seed);
        let size = (0.020 + seeded_unit(seed, pit, 414) * 0.024) * growth;
        add_shard(
            mesh,
            position,
            Vec3::new(size, size * 0.7, 0.006).max(Vec3::splat(0.000_5)),
            Quat::from_rotation_z(seeded_unit(seed, pit, 415) * 1.2),
            colour,
            seed.wrapping_add(u64::from(pit)),
        );
    }
}

/// The displaced metal crusting onto the submerged strip, with emissive
/// glints so the fresh deposit sparkles against the eroding base.
fn add_deposit_crust(
    meshes: &mut SceneMeshes,
    bench_top: f32,
    surface_y: f32,
    growth: f32,
    colour: [f32; 4],
    seed: u64,
) {
    const NUBS: u32 = 22;
    for nub in 0..NUBS {
        let position = strip_point(bench_top, surface_y, nub, 421, seed);
        let stagger = seeded_unit(seed, nub, 424);
        let local = ((growth - stagger * 0.4) / 0.6).clamp(0.0, 1.0);
        let size = (0.024 + seeded_unit(seed, nub, 425) * 0.028) * local;
        add_sphere(
            &mut meshes.opaque,
            position,
            size.max(0.000_5),
            colour,
            4,
            6,
        );
        if nub % 4 == 0 {
            add_sphere(
                &mut meshes.emissive,
                position + Vec3::new(0.0, size * 0.4, position.z.signum() * 0.006),
                (size * 0.4).max(0.000_5),
                deposit_highlight_colour(colour),
                3,
                5,
            );
        }
    }
}

/// Flakes shedding off the crust and settling at the strip's foot.
fn add_shed_flakes(
    mesh: &mut Mesh,
    bench_top: f32,
    surface_y: f32,
    progress: f32,
    colour: [f32; 4],
    phase: f32,
    seed: u64,
) {
    const FLAKES: u32 = 12;
    let shedding = smooth01((progress - FLAKE_START) / 0.12);
    let floor_y = bench_top + 0.11;
    for flake in 0..FLAKES {
        let rate = 0.18 + seeded_unit(seed, flake, 431) * 0.16;
        let age = (phase * rate + seeded_unit(seed, flake, 432)).fract();
        let start = strip_point(bench_top, surface_y, flake, 433, seed);
        let drift = curl_like_flow(phase * 0.5, seed, flake) * 0.04 * age;
        let landing_spread = 0.10 + seeded_unit(seed, flake, 436) * 0.16;
        let fall = age.powf(1.4);
        let position = Vec3::new(
            start.x + (landing_spread - 0.13) * fall + drift.x,
            start.y + (floor_y - start.y) * fall,
            start.z * (1.0 + fall * 2.2) + drift.z,
        );
        let size = (0.012 + seeded_unit(seed, flake, 437) * 0.012)
            * (std::f32::consts::PI * age).sin().max(0.0).sqrt()
            * shedding;
        add_shard(
            mesh,
            position,
            Vec3::splat(size.max(0.000_5)),
            Quat::from_rotation_y(age * 4.2),
            colour,
            seed.wrapping_add(u64::from(flake)),
        );
    }
    // The settled pile at the strip's foot grows as flakes land.
    add_sediment_mound(
        mesh,
        Vec3::new(STRIP_X, floor_y, 0.0),
        0.34,
        shedding * smooth01((progress - FLAKE_START) / (1.0 - FLAKE_START)),
        0.10,
        colour,
        seed.rotate_left(19),
    );
}

pub(super) fn add_displacement_assembly(
    meshes: &mut SceneMeshes,
    plan: &ScenePlan,
    layout: SceneLayout,
    progress: f32,
    ordinal: u16,
    ordinal_progress: f32,
) {
    let displacement = plan
        .metal_displacement
        .as_ref()
        .expect("validated metal-displacement assembly has material bindings");
    let seed = plan_seed(plan);
    let progress = progress.clamp(0.0, 1.0);
    let phase = progress * 12.0;
    add_assembly_beaker(&mut meshes.glass, layout.bench_top, Vec3::ZERO);
    let surface_y = layout.bench_top + BASIN_LEVEL;
    let state = LiquidState {
        surface_centre: Vec3::new(0.0, surface_y, 0.0),
        floor_y: layout.bench_top + 0.09,
        radius: 0.88,
        colour: ClipColour::SolutionInitial,
        initial_level_y: surface_y,
    };
    let colour = |clip_colour| {
        metal_displacement_track_colour(clip_colour, displacement, ordinal, ordinal_progress)
    };
    add_metal_strip(&mut meshes.opaque, layout.bench_top, colour(ClipColour::OriginalMetal));
    let growth = deposit_growth(progress);
    add_erosion_pits(
        &mut meshes.opaque,
        layout.bench_top,
        surface_y,
        smooth01(progress / 0.5),
        colour(ClipColour::MetalErosion),
        seed.rotate_left(3),
    );
    add_deposit_crust(
        meshes,
        layout.bench_top,
        surface_y,
        growth,
        colour(ClipColour::DepositedMetal),
        seed.rotate_left(7),
    );
    add_shed_flakes(
        &mut meshes.opaque,
        layout.bench_top,
        surface_y,
        progress,
        colour(ClipColour::DepositedMetal),
        phase,
        seed.rotate_left(11),
    );
    add_displacement_solution(
        meshes,
        &state,
        colour(ClipColour::SolutionInitial),
        colour(ClipColour::SolutionFinal),
        progress,
        phase,
        seed,
    );
}
