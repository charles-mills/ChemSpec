//! Procedural alkali-metal-and-water scene: a pellet dropped onto the
//! basin, skittering as it reacts away, with hydrogen fizz and an ignition
//! flame for the metals that burn.
//!
//! Everything is a deterministic function of (plan, progress): fixed entity
//! populations whose sizes animate, never appearing or vanishing.

#![allow(clippy::wildcard_imports, clippy::cast_precision_loss)]

use super::*;

/// The alkali basin the former Blender clip authored, in world units.
const ALKALI_BASIN_RADIUS: f32 = 0.88;
/// Progress at which the dropped pellet reaches the water.
const ALKALI_DROP_END: f32 = 0.055;
/// Progress at which the pellet is fully consumed.
const ALKALI_CONSUMED: f32 = 0.90;
/// Colour of the aqueous basin across the alkali scene.
const ALKALI_WATER_COLOUR: [f32; 4] = [0.40, 0.68, 0.82, 0.52];

/// World-space pose of the alkali pellet at one moment of the reaction.
/// Always defined: after consumption the lump renders at zero size, because
/// scene topology must stay constant across playhead moments (no entity
/// churn), matching the renderer-wide invariant the assemblies rely on.
struct AlkaliPellet {
    position: Vec3,
    /// Remaining fraction of the original lump, 1 at entry down to 0.
    remaining: f32,
    /// 0 while airborne, easing to 1 once the pellet reacts on the water.
    contact: f32,
}

/// Where the dropped pellet meets the water, on the basin surface.
fn alkali_entry(seed: u64, surface: Vec3) -> Vec3 {
    let entry_angle = seed_phase(seed, 71);
    surface + Vec3::new(entry_angle.cos(), 0.0, entry_angle.sin()) * (ALKALI_BASIN_RADIUS * 0.22)
}

/// Deterministic pellet choreography: a short gravity drop onto the water,
/// then a wandering skitter that sweeps around the basin — faster laps for
/// livelier metals — while the lump shrinks away to nothing.
fn alkali_pellet(progress: f32, activity: f32, seed: u64, surface: Vec3) -> AlkaliPellet {
    let entry_angle = seed_phase(seed, 71);
    let entry = alkali_entry(seed, surface);
    if progress < ALKALI_DROP_END {
        let fall = (progress / ALKALI_DROP_END).max(0.0);
        return AlkaliPellet {
            position: entry + Vec3::Y * ((1.0 - fall * fall) * 0.85),
            remaining: 1.0,
            contact: 0.0,
        };
    }
    let life = ((progress - ALKALI_DROP_END) / (ALKALI_CONSUMED - ALKALI_DROP_END)).clamp(0.0, 1.0);
    let laps = 1.4 + activity * 2.2;
    let angle = entry_angle
        + life * laps * std::f32::consts::TAU
        + (life * 23.0 + seed_phase(seed, 72)).sin() * 0.35;
    let radial = ALKALI_BASIN_RADIUS
        * (0.18 + 0.38 * (0.5 + 0.5 * (life * 9.0 + seed_phase(seed, 73)).sin()));
    let bob = (life * 40.0).sin() * 0.012 * activity;
    AlkaliPellet {
        position: Vec3::new(
            surface.x + angle.cos() * radial,
            surface.y + bob,
            surface.z + angle.sin() * radial,
        ),
        remaining: (1.0 - life).sqrt(),
        contact: ((progress - ALKALI_DROP_END) / 0.03).clamp(0.0, 1.0),
    }
}

/// An irregular rounded lump riding half-proud of the water line: overlapping
/// squashed spheres read as soft freshly-cut metal rather than a crystal.
/// `melt` morphs it into a single glossy bead — the reaction heat melts the
/// livelier metals, whose lumps draw into a shining ball as they skitter —
/// by pulling the bulges inside the main sphere and lighting a hot glint.
/// Always emitted; the radius carries the consumption down to nothing.
fn add_alkali_pellet_lump(
    meshes: &mut SceneMeshes,
    pellet: &AlkaliPellet,
    melt: f32,
    can_melt: bool,
    seed: u64,
) {
    const METAL: [f32; 4] = [0.88, 0.90, 0.92, 1.0];
    const MOLTEN: [f32; 4] = [0.96, 0.94, 0.90, 1.0];
    let size = (0.14 * pellet.remaining.sqrt()).max(0.000_5);
    let centre = pellet.position + Vec3::Y * (size * 0.35);
    let skin = mix_color(METAL, MOLTEN, melt);
    add_sphere(&mut meshes.opaque, centre, size, skin, 5, 8);
    // Molten surface tension swallows the ragged bulges into the bead.
    let bulge = Vec3::new(
        (seed_phase(seed, 74) + pellet.position.x * 2.4).sin(),
        0.3,
        (seed_phase(seed, 75) + pellet.position.z * 1.7).cos(),
    ) * (size * 0.55 * (1.0 - melt));
    add_sphere(
        &mut meshes.opaque,
        centre + bulge,
        size * (0.62 + 0.2 * melt),
        skin,
        5,
        8,
    );
    add_sphere(
        &mut meshes.opaque,
        centre - bulge * 0.7,
        size * (0.5 + 0.3 * melt),
        skin,
        4,
        7,
    );
    // The glossy highlight: a hot glint riding the bead's crown. Emitted
    // only for plans whose metal can melt at all (a per-plan constant, so
    // topology holds) — a placid metal's scene stays free of emissive
    // geometry it would never light.
    if can_melt {
        add_sphere(
            &mut meshes.emissive,
            centre + Vec3::Y * (size * 0.62),
            (size * 0.30 * melt * pellet.contact).max(0.000_5),
            [1.0, 0.66, 0.34, 0.55 * melt],
            4,
            6,
        );
    }
}

/// Hydrogen bubbles lingering along the pellet's wake: the choreography is a
/// pure function of progress, so each trail bubble re-evaluates it at a
/// lagged playhead and sits where the pellet actually passed. Fixed
/// population; presence collapses to the floor size while the pellet is
/// airborne or spent.
fn add_pellet_wake_trail(
    mesh: &mut Mesh,
    progress: f32,
    activity: f32,
    seed: u64,
    surface: Vec3,
    phase: f32,
) {
    const TRAIL: u32 = 10;
    for step in 0..TRAIL {
        let lag = (step + 1) as f32 * 0.014;
        let past = alkali_pellet((progress - lag).max(0.0), activity, seed, surface);
        let fade = 1.0 - step as f32 / TRAIL as f32;
        let presence = past.contact * past.remaining * activity * fade;
        let jitter = Vec3::new(
            seeded_unit(seed, step, 311) - 0.5,
            0.0,
            seeded_unit(seed, step, 312) - 0.5,
        ) * 0.05;
        let pop = (std::f32::consts::PI
            * (phase * (1.1 + seeded_unit(seed, step, 313)) + seeded_unit(seed, step, 314))
                .fract())
        .sin()
        .max(0.0);
        add_sphere(
            mesh,
            Vec3::new(past.position.x, surface.y, past.position.z) + jitter + Vec3::Y * (lag * 0.4),
            (0.016 * pop * presence).max(0.000_5),
            [0.86, 0.95, 0.99, 0.36],
            4,
            6,
        );
    }
}

/// The entry splash: a crown of droplets thrown up as the pellet strikes the
/// water, arcing out ballistically and gone within a beat. Fixed population;
/// outside the splash window every droplet renders at the floor size.
fn add_entry_splash(mesh: &mut Mesh, entry: Vec3, progress: f32, seed: u64) {
    const DROPLETS: u32 = 9;
    // Ballistic age through the splash, and an envelope that lifts on
    // contact and dies as the crown falls back.
    let age = ((progress - ALKALI_DROP_END) / 0.06).clamp(0.0, 1.0);
    let presence = smooth01((progress - ALKALI_DROP_END) / 0.008)
        * (1.0 - smooth01((progress - ALKALI_DROP_END - 0.018) / 0.05));
    for droplet in 0..DROPLETS {
        let angle = std::f32::consts::TAU * droplet as f32 / DROPLETS as f32
            + seeded_unit(seed, droplet, 321) * 0.5;
        let launch = 0.5 + seeded_unit(seed, droplet, 322) * 0.5;
        let radial = 0.06 + age * launch * 0.45;
        let height = age * launch * 0.85 - age * age * 0.95;
        let position = entry
            + Vec3::new(
                angle.cos() * radial,
                height.max(0.0) + 0.012,
                angle.sin() * radial,
            );
        let size = (0.014 + seeded_unit(seed, droplet, 323) * 0.014)
            * presence
            * (std::f32::consts::PI * age.min(0.95)).sin().max(0.25);
        add_sphere(
            mesh,
            position,
            size.max(0.000_5),
            [0.80, 0.90, 0.96, 0.68],
            4,
            6,
        );
    }
}

/// Fixed-population hydrogen fizz around the reacting lump. Every bubble is
/// always emitted; each breathes through a rise-and-pop cycle whose size
/// scales with `strength`, so intensity never changes the vertex count.
pub(super) fn add_alkali_fizz(mesh: &mut Mesh, centre: Vec3, strength: f32, phase: f32, seed: u64) {
    const BUBBLES: u32 = 14;
    for bubble in 0..BUBBLES {
        let rate = 0.9 + seeded_unit(seed, bubble, 301) * 1.4;
        let age = (phase * rate + seeded_unit(seed, bubble, 302)).fract();
        let angle = seeded_unit(seed, bubble, 303) * std::f32::consts::TAU + phase * 0.3;
        let radial = 0.05 + seeded_unit(seed, bubble, 304).sqrt() * 0.24;
        let pop = (std::f32::consts::PI * age).sin().max(0.0);
        add_sphere(
            mesh,
            centre + Vec3::new(angle.cos() * radial, age * 0.06, angle.sin() * radial),
            ((0.006 + 0.022 * pop) * strength).max(0.000_5),
            [0.86, 0.95, 0.99, 0.42],
            4,
            6,
        );
    }
}

pub(super) fn add_alkali_water_assembly(
    meshes: &mut SceneMeshes,
    plan: &ScenePlan,
    layout: SceneLayout,
    progress: f32,
) {
    let style = animated_alkali_water_style(plan);
    let seed = plan_seed(plan);
    // Continuous scene time; the pacing the 6 s authored clip established.
    let phase = progress * 12.0;
    add_assembly_beaker(&mut meshes.glass, layout.bench_top, Vec3::ZERO);
    let surface_centre = Vec3::new(0.0, layout.liquid_surface, 0.0);
    let floor_y = layout.bench_top + 0.09;
    let pellet = alkali_pellet(progress, style.activity, seed, surface_centre);
    // 0 while airborne, peaking on contact, back to 0 as the metal is spent.
    let reacting = pellet.contact * pellet.remaining;
    add_contained_liquid(
        &mut meshes.translucent,
        surface_centre,
        floor_y,
        ALKALI_BASIN_RADIUS,
        ALKALI_WATER_COLOUR,
        style.activity * (0.35 + 0.65 * reacting),
        phase,
        seed,
    );
    // The evolving hydrogen collects a foam collar at the glass.
    add_foam_ring(
        &mut meshes.translucent,
        surface_centre,
        ALKALI_BASIN_RADIUS,
        style.activity * 0.7,
        phase,
        seed,
    );
    let fizz_centre = Vec3::new(pellet.position.x, surface_centre.y, pellet.position.z);
    add_alkali_fizz(
        &mut meshes.translucent,
        fizz_centre,
        style.activity * pellet.contact * (0.3 + 0.7 * pellet.remaining),
        phase,
        seed.rotate_left(5),
    );
    add_pellet_wake_trail(
        &mut meshes.translucent,
        progress,
        style.activity,
        seed,
        surface_centre,
        phase,
    );
    add_entry_splash(
        &mut meshes.translucent,
        alkali_entry(seed, surface_centre),
        progress,
        seed.rotate_left(9),
    );
    if let Some(palette) = style.flame {
        add_surface_flame(
            meshes,
            palette,
            pellet.position + Vec3::Y * 0.03,
            pellet.contact * pellet.remaining.sqrt(),
            1.0,
            phase,
            seed.rotate_left(11),
        );
    }
    // Reaction heat melts the livelier metals into a glossy rolling bead;
    // the placid one keeps its ragged freshly-cut shape. Capacity is a
    // per-plan constant, so the glint's presence never churns topology.
    let melt_capacity = smooth01((style.activity - 0.45) / 0.2);
    let melt = melt_capacity * smooth01((progress - ALKALI_DROP_END) / 0.06);
    add_alkali_pellet_lump(meshes, &pellet, melt, melt_capacity > 0.0, seed);
    // Steam fogs the glass; a seeded constant population keeps topology fixed.
    add_glass_condensation(
        &mut meshes.translucent,
        surface_centre,
        0.94,
        layout.bench_top + 1.78,
        style.activity * 0.8,
        seed.rotate_left(13),
    );
}
