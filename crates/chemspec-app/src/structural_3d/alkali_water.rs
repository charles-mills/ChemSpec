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

/// Deterministic pellet choreography: a short gravity drop onto the water,
/// then a wandering skitter that sweeps around the basin — faster laps for
/// livelier metals — while the lump shrinks away to nothing.
fn alkali_pellet(progress: f32, activity: f32, seed: u64, surface: Vec3) -> AlkaliPellet {
    let entry_angle = seed_phase(seed, 71);
    let entry = surface
        + Vec3::new(entry_angle.cos(), 0.0, entry_angle.sin()) * (ALKALI_BASIN_RADIUS * 0.22);
    if progress < ALKALI_DROP_END {
        let fall = (progress / ALKALI_DROP_END).max(0.0);
        return AlkaliPellet {
            position: entry + Vec3::Y * ((1.0 - fall * fall) * 0.85),
            remaining: 1.0,
            contact: 0.0,
        };
    }
    let life =
        ((progress - ALKALI_DROP_END) / (ALKALI_CONSUMED - ALKALI_DROP_END)).clamp(0.0, 1.0);
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
/// Always emitted; the radius carries the consumption down to nothing.
fn add_alkali_pellet_lump(mesh: &mut Mesh, pellet: &AlkaliPellet, seed: u64) {
    const METAL: [f32; 4] = [0.88, 0.90, 0.92, 1.0];
    let size = (0.14 * pellet.remaining.sqrt()).max(0.000_5);
    let centre = pellet.position + Vec3::Y * (size * 0.35);
    add_sphere(mesh, centre, size, METAL, 5, 8);
    let bulge = Vec3::new(
        (seed_phase(seed, 74) + pellet.position.x * 2.4).sin(),
        0.3,
        (seed_phase(seed, 75) + pellet.position.z * 1.7).cos(),
    ) * (size * 0.55);
    add_sphere(mesh, centre + bulge, size * 0.62, METAL, 5, 8);
    add_sphere(mesh, centre - bulge * 0.7, size * 0.5, METAL, 4, 7);
}

/// Fixed-population hydrogen fizz around the reacting lump. Every bubble is
/// always emitted; each breathes through a rise-and-pop cycle whose size
/// scales with `strength`, so intensity never changes the vertex count.
pub(super) fn add_alkali_fizz(mesh: &mut Mesh, centre: Vec3, strength: f32, phase: f32, seed: u64) {
    const BUBBLES: u32 = 14;
    for bubble in 0..BUBBLES {
        let rate = 0.9 + seeded_unit(seed, bubble, 301) * 1.4;
        let age = (phase * rate + seeded_unit(seed, bubble, 302)).fract();
        let angle =
            seeded_unit(seed, bubble, 303) * std::f32::consts::TAU + phase * 0.3;
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
    add_alkali_pellet_lump(&mut meshes.opaque, &pellet, seed);
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
