//! Procedural combustion scene: a pool of fuel burning at its surface —
//! clean and blue when oxygen is plentiful, orange with rolling smoke,
//! soot flecks, and a darkening glass when it is limited.
//!
//! Everything is a deterministic function of (plan, progress): fixed entity
//! populations whose sizes animate, never appearing or vanishing.

#![allow(clippy::wildcard_imports, clippy::cast_precision_loss)]

use super::*;

/// The evaporating dish the fuel pool burns in.
const DISH_RADIUS: f32 = 0.82;
const DISH_HEIGHT: f32 = 0.16;
/// Fuel pool footprint and initial level inside the dish.
const POOL_RADIUS: f32 = 0.58;
const POOL_LEVEL: f32 = 0.115;
/// Share of the shallow pool consumed across the reaction.
const BURN_DOWN: f32 = 0.80;
/// The virtual 30 fps frame count the authored pacing was built around.
const FRAMES: u16 = 180;
/// The cool observation beaker held inverted over the flame.
const HOVER_RADIUS: f32 = 0.52;
const HOVER_OPEN: f32 = 1.02;
const HOVER_TOP: f32 = 1.50;

/// Flame envelope: ignition ramps in fast, burns steady, gutters out.
fn flame_strength(progress: f32) -> f32 {
    smooth01(progress / 0.08) * (1.0 - smooth01((progress - 0.88) / 0.12))
}

/// Fixed-population smoke puffs rolling up out of the vessel, expanding and
/// thinning as they rise. Sizes scale with `strength`.
fn add_smoke_plume(
    mesh: &mut Mesh,
    source: Vec3,
    strength: f32,
    colour: [f32; 4],
    phase: f32,
    seed: u64,
) {
    const PUFFS: u32 = 16;
    for puff in 0..PUFFS {
        let rate = 0.28 + seeded_unit(seed, puff, 381) * 0.30;
        let age = (phase * rate + seeded_unit(seed, puff, 382)).fract();
        let angle = seeded_unit(seed, puff, 383) * std::f32::consts::TAU;
        let radial = seeded_unit(seed, puff, 384).sqrt() * (0.10 + age * 0.30);
        let drift = curl_like_flow(phase * 0.6, seed, puff) * 0.16 * age;
        let position = source
            + Vec3::new(
                angle.cos() * radial + drift.x,
                0.06 + age * 1.05,
                angle.sin() * radial + drift.z,
            );
        let size = (0.055 + 0.16 * age) * strength;
        add_sphere(
            mesh,
            position,
            size.max(0.000_5),
            alpha(colour, colour[3] * (1.0 - age * 0.75) * strength),
            4,
            7,
        );
    }
}

/// Fixed-population soot flecks spiralling up inside the smoke column.
fn add_soot_flecks(
    mesh: &mut Mesh,
    source: Vec3,
    strength: f32,
    colour: [f32; 4],
    phase: f32,
    seed: u64,
) {
    const FLECKS: u32 = 14;
    for fleck in 0..FLECKS {
        let rate = 0.4 + seeded_unit(seed, fleck, 391) * 0.5;
        let age = (phase * rate + seeded_unit(seed, fleck, 392)).fract();
        let angle = seeded_unit(seed, fleck, 393) * std::f32::consts::TAU + phase * 0.8;
        let radial = 0.05 + seeded_unit(seed, fleck, 394).sqrt() * (0.08 + age * 0.24);
        let position = source
            + Vec3::new(
                angle.cos() * radial,
                0.05 + age * (0.85 + seeded_unit(seed, fleck, 395) * 0.3),
                angle.sin() * radial,
            );
        let size = (0.009 + seeded_unit(seed, fleck, 396) * 0.011)
            * (std::f32::consts::PI * age).sin().max(0.0)
            * strength;
        add_shard(
            mesh,
            position,
            Vec3::splat(size.max(0.000_5)),
            Quat::from_rotation_y(angle + age * 3.1),
            colour,
            seed.wrapping_add(u64::from(fleck)),
        );
    }
}

/// Soot film settling on a cool wall band above the flame: a fixed seeded
/// speckle population whose opacity grows with `deposit`.
fn add_soot_deposit(
    mesh: &mut Mesh,
    wall_radius: f32,
    surface_y: f32,
    rim_y: f32,
    deposit: f32,
    colour: [f32; 4],
    seed: u64,
) {
    const SPECKLES: u32 = 24;
    let band = (rim_y - surface_y - 0.08).max(0.05);
    for speckle in 0..SPECKLES {
        let angle = seeded_unit(seed, speckle, 401) * std::f32::consts::TAU;
        let height = surface_y + 0.06 + seeded_unit(seed, speckle, 402) * band;
        let inset = wall_radius - 0.012 * seeded_unit(seed, speckle, 403);
        let position = Vec3::new(angle.cos() * inset, height, angle.sin() * inset);
        let size = 0.014 + seeded_unit(seed, speckle, 404) * 0.022;
        add_sphere(
            mesh,
            position,
            size,
            alpha(colour, colour[3] * deposit),
            3,
            5,
        );
    }
}

/// The premixed two-cone flame of a clean burn: a tall translucent outer
/// cone sheathing a short bright inner cone, with a faint violet mantle.
/// Steady, breathing slightly — nothing like the ragged diffusion flame of
/// the starved burn.
fn add_premixed_flame(meshes: &mut SceneMeshes, source: Vec3, strength: f32, phase: f32) {
    let colours = flame_colours(FlamePalette::BurnerBlue);
    let breathe = 1.0 + (phase * 7.3).sin() * 0.05 + (phase * 11.1).sin() * 0.03;
    let outer_height = (0.60 * strength * breathe).max(0.02);
    let inner_height = (0.26 * strength * (2.0 - breathe)).max(0.02);
    add_flame_lobe(
        &mut meshes.translucent,
        source,
        source + Vec3::Y * (outer_height * 1.12),
        (0.26 * strength).max(0.002),
        alpha([0.45, 0.30, 0.95, 0.18], (0.30 * strength).max(0.02)),
    );
    add_flame_lobe(
        &mut meshes.translucent,
        source,
        source + Vec3::Y * outer_height,
        (0.20 * strength).max(0.002),
        alpha(colours.body_low, (0.55 * strength.min(1.0)).max(0.02)),
    );
    add_flame_lobe(
        &mut meshes.emissive,
        source + Vec3::Y * 0.012,
        source + Vec3::Y * inner_height,
        (0.105 * strength).max(0.002),
        alpha(colours.core, (0.75 * strength.min(1.0)).max(0.02)),
    );
}

/// The retort stand holding the observation beaker inverted over the flame:
/// a foot, an upright, a reaching arm, and a clamp ring around the glass.
fn add_hover_stand(mesh: &mut Mesh, bench_top: f32) {
    const METAL: [f32; 4] = [0.20, 0.24, 0.28, 1.0];
    let foot = Vec3::new(1.42, bench_top, 0.30);
    add_disc(mesh, foot + Vec3::Y * 0.02, 0.17, [0.15, 0.18, 0.22, 1.0]);
    add_cylinder(
        mesh,
        foot + Vec3::Y * 0.02,
        foot + Vec3::Y * 1.88,
        0.024,
        METAL,
    );
    // The arm reaches to the clamp ring at the beaker's edge, not across it.
    let clamp_y = bench_top + HOVER_TOP - 0.05;
    let toward = Vec3::new(-foot.x, 0.0, -foot.z).normalize_or_zero();
    let clamp_edge = toward * -(HOVER_RADIUS + 0.018);
    add_cylinder(
        mesh,
        Vec3::new(foot.x, clamp_y, foot.z),
        Vec3::new(clamp_edge.x, clamp_y, clamp_edge.z),
        0.018,
        METAL,
    );
    add_ring(
        mesh,
        Vec3::new(0.0, clamp_y, 0.0),
        HOVER_RADIUS + 0.018,
        0.015,
        METAL,
    );
}

/// The cool inverted beaker over the flame: the classic residue probe. A
/// clean burn mists its inside with condensed water; a starved burn paints
/// it with soot instead.
fn add_hover_beaker(mesh: &mut Mesh, bench_top: f32) {
    const GLASS: [f32; 4] = [0.62, 0.84, 0.94, 0.22];
    let open = Vec3::new(0.0, bench_top + HOVER_OPEN, 0.0);
    let top = Vec3::new(0.0, bench_top + HOVER_TOP, 0.0);
    add_cylinder_wall(mesh, open, top, HOVER_RADIUS, GLASS);
    add_disc(mesh, top, HOVER_RADIUS, [0.66, 0.86, 0.95, 0.30]);
}

// A linear choreography list; splitting it would scatter one scene's
// reading order.
#[allow(clippy::too_many_lines)]
pub(super) fn add_combustion_assembly(
    meshes: &mut SceneMeshes,
    assembly: &PresentationObject,
    layout: SceneLayout,
    progress: f32,
) {
    let incomplete = assembly.asset == AssetProfile::IncompleteCombustionAssembly;
    let mut fuel = appearance_color(assembly.appearance);
    fuel[3] = 0.32;
    let seed = stable_seed(&assembly.id);
    let progress = progress.clamp(0.0, 1.0);
    let frame = progress * f32::from(FRAMES - 1);
    let phase = frame / 30.0 * 2.0;
    // The fuel burns in a shallow ceramic evaporating dish; a cool beaker
    // hangs inverted over the flame to catch what the burn leaves behind.
    add_ceramic_dish(
        &mut meshes.opaque,
        layout.bench_top,
        DISH_RADIUS,
        DISH_HEIGHT,
        [0.78, 0.74, 0.68, 1.0],
    );
    add_hover_stand(&mut meshes.opaque, layout.bench_top);
    add_hover_beaker(&mut meshes.glass, layout.bench_top);
    // The burning pool: the shallow charge is mostly consumed by the end.
    let floor_y = layout.bench_top + DISH_HEIGHT * 0.31 + 0.012;
    let initial_level = layout.bench_top + POOL_LEVEL;
    let burn = smooth01((progress - 0.06) / 0.82);
    let level = initial_level - (initial_level - floor_y - 0.008) * BURN_DOWN * burn;
    let state = LiquidState {
        surface_centre: Vec3::new(0.0, level, 0.0),
        floor_y,
        radius: POOL_RADIUS,
        colour: ClipColour::Fuel,
        initial_level_y: initial_level,
    };
    add_contained_liquid(
        &mut meshes.translucent,
        state.surface_centre,
        floor_y,
        state.radius,
        combustion_track_colour(ClipColour::Fuel, fuel, incomplete),
        // A gentle simmer; the flame carries the drama.
        0.22 + flame_strength(progress) * 0.2,
        phase,
        seed,
    );
    let strength = flame_strength(progress);
    let flame_source = state.surface_centre + Vec3::Y * 0.02;
    if incomplete {
        add_surface_flame(
            meshes,
            FlamePalette::Natural,
            flame_source,
            strength,
            1.6,
            phase,
            seed,
        );
    } else {
        // A clean burn is premixed: two steady cones, restrained flicker.
        add_premixed_flame(meshes, flame_source, strength, phase);
        add_surface_flame(
            meshes,
            FlamePalette::BurnerBlue,
            flame_source,
            strength * 0.45,
            0.9,
            phase,
            seed,
        );
    }
    add_ignition_sparks(
        &mut meshes.emissive,
        flame_source + Vec3::Y * 0.04,
        strength,
        phase,
        seed.rotate_left(5),
    );
    let hover_open = layout.bench_top + HOVER_OPEN;
    let hover_top = layout.bench_top + HOVER_TOP;
    if incomplete {
        add_smoke_plume(
            &mut meshes.translucent,
            flame_source,
            strength,
            combustion_track_colour(ClipColour::CombustionSmoke, fuel, incomplete),
            phase,
            seed.rotate_left(9),
        );
        add_soot_flecks(
            &mut meshes.opaque,
            flame_source,
            strength,
            combustion_track_colour(ClipColour::Soot, fuel, incomplete),
            phase,
            seed.rotate_left(13),
        );
        // The starved burn paints the cool beaker with soot.
        add_soot_deposit(
            &mut meshes.translucent,
            HOVER_RADIUS - 0.006,
            hover_open,
            hover_top,
            smooth01((progress - 0.18) / 0.6),
            combustion_track_colour(ClipColour::SootDeposit, fuel, incomplete),
            seed.rotate_left(17),
        );
    } else {
        // Clean combustion: only a faint product plume leaves the dish, and
        // its water mists the cool beaker with condensation.
        add_rising_plume(
            &mut meshes.translucent,
            state.surface_centre,
            strength * 0.8,
            combustion_track_colour(ClipColour::ProductPlume, fuel, incomplete),
            phase,
            seed.rotate_left(9),
        );
        add_condensation_mist(
            &mut meshes.translucent,
            Vec3::ZERO,
            HOVER_RADIUS - 0.006,
            hover_open + 0.05,
            hover_top - 0.04,
            smooth01((progress - 0.14) / 0.45) * (0.7 + 0.3 * strength),
            seed.rotate_left(21),
        );
    }
}
