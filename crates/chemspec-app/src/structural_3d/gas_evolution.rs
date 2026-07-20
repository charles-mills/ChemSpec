//! Procedural gas-evolution scene: an acid basin that either receives a
//! poured second solution (liquid–liquid) or hosts a solid reactant pile
//! (solid–liquid), with product bubbles rising to a surface plume.
//!
//! Everything is a deterministic function of (plan, progress): fixed
//! entity populations whose sizes animate, never appearing or vanishing.

#![allow(clippy::wildcard_imports, clippy::cast_precision_loss)]

use super::*;

/// Receiving-basin radius, matching the shared assembly beaker (world units).
const BASIN_RADIUS: f32 = 0.88;
/// Initial free-surface height above the bench per variant, preserving the
/// authored basins the baked clips established.
const LIQUID_LIQUID_LEVEL: f32 = 1.04;
const SOLID_LIQUID_LEVEL: f32 = 1.13;

/// The virtual 30 fps frame count the authored pacing was built around.
const FRAMES: u16 = 180;

fn basin_state(variant: GasEvolutionVariant, bench_top: f32) -> LiquidState {
    let level = bench_top
        + match variant {
            GasEvolutionVariant::LiquidLiquid => LIQUID_LIQUID_LEVEL,
            GasEvolutionVariant::SolidLiquid => SOLID_LIQUID_LEVEL,
        };
    LiquidState {
        surface_centre: Vec3::new(0.0, level, 0.0),
        floor_y: bench_top + 0.09,
        radius: BASIN_RADIUS,
        colour: ClipColour::LiquidInitial,
        initial_level_y: level,
    }
}

/// The pour brackets gas generation (virtual frame 35): tilting in just
/// before it, holding through the fizz ramp, then retreating.
pub(super) fn procedural_pour_table() -> &'static PourTable {
    static TABLE: OnceLock<PourTable> = OnceLock::new();
    TABLE.get_or_init(|| {
        build_scheduled_pour_table(&PourSchedule {
            approach: (14.0, 28.0),
            tilt_in: (28.0, 42.0),
            tilt_out: (68.0, 80.0),
            retreat: (82.0, 100.0),
        })
    })
}

/// Fixed-population product bubbles rising from `source` to the surface.
/// Sizes scale with `fizz`; the count never changes.
#[allow(clippy::too_many_arguments)]
fn add_rising_bubbles(
    mesh: &mut Mesh,
    state: &LiquidState,
    lift: f32,
    source: Vec3,
    spread: f32,
    fizz: f32,
    colour: [f32; 4],
    phase: f32,
    seed: u64,
) {
    const BUBBLES: u32 = 26;
    let surface_y = state.surface_centre.y + lift;
    for bubble in 0..BUBBLES {
        let rate = 0.5 + seeded_unit(seed, bubble, 321) * 0.8;
        let age = (phase * rate + seeded_unit(seed, bubble, 322)).fract();
        let angle = seeded_unit(seed, bubble, 323) * std::f32::consts::TAU;
        let radial = seeded_unit(seed, bubble, 324).sqrt() * spread;
        let origin = source + Vec3::new(angle.cos() * radial, 0.02, angle.sin() * radial);
        let wobble = curl_like_flow(phase * (0.8 + rate), seed, bubble) * 0.05 * age;
        let position = Vec3::new(
            origin.x + wobble.x,
            origin.y + age * (surface_y - origin.y - 0.01),
            origin.z + wobble.z,
        );
        let size = (0.010 + seeded_unit(seed, bubble, 325) * 0.018) * (0.55 + 0.45 * age) * fizz;
        add_sphere(mesh, position, size.max(0.000_5), colour, 4, 6);
    }
}

/// The solid reactant: a seeded pile of faceted chips on the basin floor
/// that erodes as the reaction consumes it (never below a stub, so the
/// scene keeps its topology).
fn add_reactant_pile(
    mesh: &mut Mesh,
    floor_centre: Vec3,
    colour: [f32; 4],
    consumed: f32,
    seed: u64,
) {
    const CHIPS: u32 = 9;
    let erosion = 1.0 - consumed.clamp(0.0, 1.0) * 0.62;
    for chip in 0..CHIPS {
        let angle = f32::from(u16::try_from(chip).unwrap_or(0)) * 2.399_963 + seed_phase(seed, 341);
        let radial = seeded_unit(seed, chip, 342).sqrt() * 0.24;
        let size = (0.045 + seeded_unit(seed, chip, 343) * 0.035) * erosion;
        let centre =
            floor_centre + Vec3::new(angle.cos() * radial, size * 0.55, angle.sin() * radial);
        add_shard(
            mesh,
            centre,
            Vec3::new(size, size * 0.8, size * 0.9),
            Quat::from_rotation_y(angle * 1.7),
            colour,
            seed.wrapping_add(u64::from(chip)),
        );
    }
}

// A linear choreography list; splitting it would scatter one scene's
// reading order.
#[allow(clippy::too_many_lines)]
pub(super) fn add_gas_evolution_assembly(
    meshes: &mut SceneMeshes,
    plan: &ScenePlan,
    layout: SceneLayout,
    progress: f32,
    ordinal: u16,
    ordinal_progress: f32,
) {
    let gas_evolution = plan
        .gas_evolution
        .as_ref()
        .expect("validated gas-evolution assembly has material bindings");
    let seed = plan_seed(plan);
    let frame = progress.clamp(0.0, 1.0) * f32::from(FRAMES - 1);
    let phase = frame / 30.0 * 2.0;
    add_assembly_beaker(&mut meshes.glass, layout.bench_top, Vec3::ZERO);
    let state = basin_state(gas_evolution.variant, layout.bench_top);
    let pour = match gas_evolution.variant {
        GasEvolutionVariant::LiquidLiquid => {
            pour_state_from(procedural_pour_table(), frame, layout.bench_top)
        }
        GasEvolutionVariant::SolidLiquid => None,
    };
    let receiving_lift = pour.map_or(0.0, |pour| pour.poured * 0.055);
    add_gas_evolution_liquid(
        meshes,
        plan,
        gas_evolution,
        &state,
        pour.as_ref(),
        receiving_lift,
        frame,
        ordinal,
        ordinal_progress,
    );
    let fizz = match ordinal.cmp(&gas_evolution.generation_ordinal) {
        std::cmp::Ordering::Less => 0.0,
        std::cmp::Ordering::Equal => normalized_exponential_response(ordinal_progress, 3.4),
        std::cmp::Ordering::Greater => 1.0,
    };
    let bubble_colour = gas_evolution_track_colour(
        ClipColour::GasBubble,
        gas_evolution,
        ordinal,
        ordinal_progress,
    );
    let cloud_colour = gas_evolution_track_colour(
        ClipColour::GasCloud,
        gas_evolution,
        ordinal,
        ordinal_progress,
    );
    let floor_centre = Vec3::new(0.0, state.floor_y, 0.0);
    if gas_evolution.variant == GasEvolutionVariant::SolidLiquid {
        // CO2 nucleates on the solid and rises; the surface agitation from
        // the shared liquid handles the liquid-liquid effervescence.
        add_rising_bubbles(
            &mut meshes.translucent,
            &state,
            receiving_lift,
            floor_centre,
            0.30,
            fizz,
            bubble_colour,
            phase,
            seed.rotate_left(5),
        );
    }
    add_rising_plume(
        &mut meshes.translucent,
        state.surface_centre + Vec3::Y * receiving_lift,
        fizz,
        cloud_colour,
        phase,
        seed.rotate_left(3),
    );
    // The evolved gas is denser than the air above it: it pools in the
    // headspace as a stratified layer, then spills over the rim and puddles
    // on the bench beside the beaker.
    let surface_y = state.surface_centre.y + receiving_lift;
    if fizz > 0.001 {
        add_gas_density_field(
            &mut meshes.gas,
            Vec3::new(0.0, surface_y + 0.46, 0.0),
            Vec3::new(0.74, 0.44, 0.74),
            alpha(cloud_colour, 0.34 * fizz),
            seed.rotate_left(11),
            phase,
            fizz,
            GasFlowControls::retained_product(
                fizz,
                0.22,
                0.0,
                0.08,
                smooth01((frame - 40.0) / 70.0),
                seed.rotate_left(11),
            ),
        );
        let spill = smooth01((frame - 75.0) / 40.0) * fizz;
        let spill_angle = seed_phase(seed, 91);
        if spill > 0.001 {
            add_gas_density_field(
                &mut meshes.gas,
                Vec3::new(
                    spill_angle.cos() * 1.08,
                    layout.bench_top + 0.10,
                    spill_angle.sin() * 1.08,
                ),
                Vec3::new(0.62, 0.13, 0.62),
                alpha(cloud_colour, 0.30 * spill),
                seed.rotate_left(23),
                phase,
                spill,
                GasFlowControls::retained_product(
                    spill * 0.9,
                    0.10,
                    0.0,
                    0.0,
                    0.9,
                    seed.rotate_left(23),
                ),
            );
        }
    }
    // Post-pour schlieren: the added solution still mixing into the acid.
    add_schlieren_swirls(
        &mut meshes.translucent,
        state.surface_centre + Vec3::Y * receiving_lift,
        BASIN_RADIUS,
        smooth01((frame - 78.0) / 10.0) * (1.0 - smooth01((frame - 140.0) / 35.0)),
        phase,
        seed.rotate_left(27),
    );
    if gas_evolution.variant == GasEvolutionVariant::SolidLiquid {
        let solid_colour = gas_evolution_track_colour(
            ClipColour::SolidReactant,
            gas_evolution,
            ordinal,
            ordinal_progress,
        );
        add_reactant_pile(
            &mut meshes.opaque,
            floor_centre,
            solid_colour,
            progress,
            seed.rotate_left(7),
        );
    }
    if let Some(pour) = pour {
        add_pouring_vessel_glass(&mut meshes.glass, &pour);
        let added_colour = gas_evolution_track_colour(
            ClipColour::LiquidAdded,
            gas_evolution,
            ordinal,
            ordinal_progress,
        );
        add_state_driven_pour(
            meshes,
            &pour,
            added_colour,
            state.surface_centre.y + receiving_lift,
            progress * 9.6,
            seed,
        );
    }
}
