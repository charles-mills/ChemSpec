//! Procedural metal-displacement scene: a metal strip leaning against the
//! beaker wall in a salt solution, eroding below the waterline while the
//! displaced metal nucleates in patches at the waterline, crusts downward,
//! flakes off, and settles — as the solution's colour drains from around
//! the strip.
//!
//! Everything is a deterministic function of (plan, progress): fixed entity
//! populations whose sizes animate, never appearing or vanishing.

#![allow(clippy::wildcard_imports, clippy::cast_precision_loss)]

use super::*;

/// Solution level above the bench, preserving the authored basin.
const BASIN_LEVEL: f32 = 1.13;
/// Strip cross-section and length.
const STRIP_WIDTH: f32 = 0.30;
const STRIP_THICKNESS: f32 = 0.055;
const STRIP_LENGTH: f32 = 1.45;
/// The strip leans against the beaker wall: foot near the middle of the
/// basin, top resting on the inner glass.
const STRIP_LEAN: f32 = 0.57;
/// Deposits crust from here; flakes shed later, as authored.
const DEPOSIT_START: f32 = 53.0 / 179.0;
const FLAKE_START: f32 = 103.0 / 179.0;

/// The leaning strip's frame: foot centre, direction along its length,
/// direction across its width, and its face normal.
struct StripFrame {
    base: Vec3,
    along: Vec3,
    across: Vec3,
    normal: Vec3,
}

fn strip_frame(bench_top: f32) -> StripFrame {
    let along = Vec3::new(STRIP_LEAN.sin(), STRIP_LEAN.cos(), 0.0);
    StripFrame {
        base: Vec3::new(0.12, bench_top + 0.12, 0.0),
        along,
        across: Vec3::Z,
        normal: Vec3::new(along.y, -along.x, 0.0),
    }
}

/// How much deposit has crusted on by `progress`.
fn deposit_growth(progress: f32) -> f32 {
    smooth01((progress - DEPOSIT_START) / (1.0 - DEPOSIT_START))
}

/// A bevelled slab in the strip's frame: straight sides up to a chamfered
/// band, closed with a top face — a cut piece of sheet metal, not a brick.
fn add_bevelled_strip(mesh: &mut Mesh, frame: &StripFrame, colour: [f32; 4]) {
    const BEVEL: f32 = 0.05;
    let half_w = STRIP_WIDTH * 0.5;
    let half_t = STRIP_THICKNESS * 0.5;
    let corner =
        |v: f32, w: f32, t: f32| frame.base + frame.along * v + frame.across * w + frame.normal * t;
    let base_vertex = u32::try_from(mesh.vertices.len()).unwrap_or(u32::MAX);
    // Three rings of four corners: foot, bevel start, and the inset top.
    let rings: [(f32, f32, f32); 3] = [
        (0.0, half_w, half_t),
        (STRIP_LENGTH - BEVEL, half_w, half_t),
        (STRIP_LENGTH, half_w - BEVEL * 0.6, half_t - BEVEL * 0.35),
    ];
    for (v, w, t) in rings {
        for (cw, ct) in [(-w, -t), (w, -t), (w, t), (-w, t)] {
            let position = corner(v, cw, ct);
            let outward =
                (frame.across * cw.signum() + frame.normal * ct.signum()).normalize_or_zero();
            mesh.vertices.push(Vertex {
                position: position.to_array(),
                normal: outward.to_array(),
                color: colour,
            });
        }
    }
    for ring in 0..2_u32 {
        for side in 0..4_u32 {
            let a = base_vertex + ring * 4 + side;
            let b = base_vertex + ring * 4 + (side + 1) % 4;
            let c = a + 4;
            let d = b + 4;
            mesh.indices.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    // Top cap and foot cap.
    let top = base_vertex + 8;
    mesh.indices
        .extend_from_slice(&[top, top + 1, top + 2, top, top + 2, top + 3]);
    mesh.indices.extend_from_slice(&[
        base_vertex,
        base_vertex + 2,
        base_vertex + 1,
        base_vertex,
        base_vertex + 3,
        base_vertex + 2,
    ]);
}

/// Distance along the strip at which it crosses the waterline.
fn waterline_v(frame: &StripFrame, surface_y: f32) -> f32 {
    ((surface_y - frame.base.y) / frame.along.y).clamp(0.1, STRIP_LENGTH - 0.05)
}

/// A seeded point on the submerged strip faces, in the strip's own frame.
fn strip_point(frame: &StripFrame, v: f32, w: f32, front: bool) -> Vec3 {
    let side = if front { 1.0 } else { -1.0 };
    frame.base
        + frame.along * v
        + frame.across * w
        + frame.normal * (side * (STRIP_THICKNESS * 0.5 + 0.004))
}

/// Dark pitting where the strip dissolves below the waterline.
fn add_erosion_pits(
    mesh: &mut Mesh,
    frame: &StripFrame,
    surface_v: f32,
    growth: f32,
    colour: [f32; 4],
    seed: u64,
) {
    const PITS: u32 = 12;
    for pit in 0..PITS {
        let v = 0.06 + seeded_unit(seed, pit, 411) * (surface_v - 0.12);
        let w = (seeded_unit(seed, pit, 412) - 0.5) * STRIP_WIDTH * 0.9;
        let front = seeded_unit(seed, pit, 413) > 0.5;
        let position = strip_point(frame, v, w, front);
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

/// The displaced metal nucleating on the submerged strip: a few seeded
/// patches start at the waterline — where fresh solution meets the metal —
/// and each spreads downward as the reaction runs, with warm emissive
/// glints so the fresh deposit sparkles against the eroding base.
fn add_deposit_crust(
    meshes: &mut SceneMeshes,
    frame: &StripFrame,
    surface_v: f32,
    growth: f32,
    colour: [f32; 4],
    seed: u64,
) {
    const PATCHES: u32 = 4;
    const NUBS: u32 = 24;
    for nub in 0..NUBS {
        let patch = nub % PATCHES;
        // Patches nucleate staggered, each at its own waterline spot.
        let patch_start = seeded_unit(seed, patch, 421) * 0.35;
        let patch_growth = ((growth - patch_start) / (1.0 - patch_start).max(0.2)).clamp(0.0, 1.0);
        let patch_w = (seeded_unit(seed, patch, 422) - 0.5) * STRIP_WIDTH * 0.7;
        let patch_front = seeded_unit(seed, patch, 423) > 0.4;
        let patch_top = surface_v - 0.05 - seeded_unit(seed, patch, 424) * 0.06;
        // Each nub sits somewhere in its patch's downward-growing footprint.
        let v = patch_top - seeded_unit(seed, nub, 425) * (surface_v - 0.14) * 0.75 * patch_growth;
        let w = patch_w
            + (seeded_unit(seed, nub, 426) - 0.5) * STRIP_WIDTH * (0.16 + 0.24 * patch_growth);
        let position = strip_point(frame, v.max(0.05), w, patch_front);
        let stagger = seeded_unit(seed, nub, 427);
        let local = ((patch_growth - stagger * 0.3) / 0.7).clamp(0.0, 1.0);
        let size = (0.024 + seeded_unit(seed, nub, 428) * 0.028) * local;
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
                position
                    + frame.normal * (if patch_front { 0.006 } else { -0.006 })
                    + Vec3::Y * (size * 0.3),
                (size * 0.4).max(0.000_5),
                deposit_highlight_colour(colour),
                3,
                5,
            );
        }
    }
}

/// Flakes shedding off the crust and settling at the strip's foot.
#[allow(clippy::too_many_arguments)]
fn add_shed_flakes(
    mesh: &mut Mesh,
    frame: &StripFrame,
    bench_top: f32,
    surface_v: f32,
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
        let v = 0.15 + seeded_unit(seed, flake, 433) * (surface_v - 0.25);
        let w = (seeded_unit(seed, flake, 434) - 0.5) * STRIP_WIDTH * 0.8;
        let start = strip_point(frame, v, w, seeded_unit(seed, flake, 435) > 0.5);
        let drift = curl_like_flow(phase * 0.5, seed, flake) * 0.04 * age;
        let landing_spread = 0.10 + seeded_unit(seed, flake, 436) * 0.16;
        let fall = age.powf(1.4);
        let position = Vec3::new(
            start.x + (landing_spread - 0.13) * fall + drift.x,
            start.y + (floor_y - start.y) * fall,
            start.z * (1.0 + fall * 1.4) + drift.z,
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
    // The settled pile below the strip's submerged reach grows as flakes land.
    let foot = frame.base + frame.along * (surface_v * 0.45);
    add_sediment_mound(
        mesh,
        Vec3::new(foot.x, floor_y, foot.z),
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
    let frame = strip_frame(layout.bench_top);
    let surface_v = waterline_v(&frame, surface_y);
    add_bevelled_strip(
        &mut meshes.opaque,
        &frame,
        colour(ClipColour::OriginalMetal),
    );
    let growth = deposit_growth(progress);
    add_erosion_pits(
        &mut meshes.opaque,
        &frame,
        surface_v,
        smooth01(progress / 0.5),
        colour(ClipColour::MetalErosion),
        seed.rotate_left(3),
    );
    add_deposit_crust(
        meshes,
        &frame,
        surface_v,
        growth,
        colour(ClipColour::DepositedMetal),
        seed.rotate_left(7),
    );
    add_shed_flakes(
        &mut meshes.opaque,
        &frame,
        layout.bench_top,
        surface_v,
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
