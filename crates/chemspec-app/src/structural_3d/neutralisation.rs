//! Procedural neutralisation-and-evaporation scene: two solutions stirred
//! together in the basin, then the vessel lifted over heat while the
//! solvent boils away and the salt crystallises out.
//!
//! The heavy lifting is delegated: the plan's authored effects drive the
//! stirrer and mixing currents through the shared effect engine, and the
//! post-process state drives the proven evaporation pipeline (heating rig,
//! nucleate boiling, escaping vapour, crystallising salt).

#![allow(clippy::wildcard_imports)]

use super::*;

/// Initial free-surface height above the bench, preserving the authored
/// basin the baked clip established.
const BASIN_LEVEL: f32 = 1.53;
/// The virtual 30 fps frame count the authored 240-frame pacing used.
const FRAMES: u16 = 240;

// A linear choreography list; splitting it would scatter one scene's
// reading order.
#[allow(clippy::too_many_lines)]
pub(super) fn add_neutralisation_assembly(
    meshes: &mut SceneMeshes,
    moment: NeutralisationAssemblyMoment<'_>,
) {
    let NeutralisationAssemblyMoment {
        plan,
        layout,
        stage,
        progress,
        post_process,
        stage_progress,
        seed,
        visual_inputs,
        effect_colours,
        ordinal,
        ordinal_progress,
    } = moment;
    let frame = progress.clamp(0.0, 1.0) * f32::from(FRAMES - 1);
    let colours = neutralisation_colours(plan, effect_colours, frame);
    // The evaporation choreography lives in post_process: the vessel rides
    // its lift, and the boiled-off solvent shortens the liquid column.
    let animated = layout
        .with_vessel_motion(Vec3::Y * post_process.lift)
        .with_liquid_fraction(post_process.liquid_fraction);
    let vessel_motion = Vec3::Y * (post_process.lift / 0.45);
    add_assembly_beaker(&mut meshes.glass, layout.bench_top, vessel_motion);
    let state = LiquidState {
        surface_centre: Vec3::new(0.0, animated.liquid_surface, 0.0),
        floor_y: layout.bench_top + 0.09,
        radius: 0.88,
        colour: ClipColour::Water,
        initial_level_y: layout.bench_top + BASIN_LEVEL,
    };
    add_neutralisation_liquid(
        meshes,
        &state,
        NeutralisationLiquidLife {
            bench_top: layout.bench_top,
            vessel_motion,
            liquid_colour: neutralisation_track_colour(state.colour, colours),
            surface_colour: colours.liquid,
            turbulence: visual_inputs.liquid_turbulence,
            boiling: post_process.boiling,
            vapour: post_process.vapour,
        },
        frame / 30.0 * 2.0,
        seed,
    );
    // The plan's authored mixing effect drives the stirrer directly. Both
    // pieces stay emitted at every moment (visibility floored) because scene
    // topology must not change across ordinals.
    let _ = stage;
    if let Some(mixing) = plan
        .effects
        .iter()
        .find(|effect| effect.effect == EffectProfile::LiquidMixing)
    {
        let span = f32::from(mixing.end_ordinal.saturating_sub(mixing.start_ordinal)) + 1.0;
        let stir = ((f32::from(ordinal) - f32::from(mixing.start_ordinal) + ordinal_progress)
            / span)
            .clamp(0.0, 1.0);
        let mut pose = stirring_pose(animated, stir, seed);
        pose.visibility = pose.visibility.max(0.001_1);
        let dynamics = scene_registry::effect_dynamics(mixing.effect, mixing.intensity);
        add_mixing_currents(
            &mut meshes.translucent,
            Vec3::new(pose.lower.x, animated.liquid_center.y, pose.lower.z),
            dynamics,
            (pose.activity * 0.9).max(0.001_1),
            frame / 30.0 * 2.0,
            seed.rotate_left(3),
            colours.liquid,
        );
        add_stirring_apparatus(meshes, animated, pose, stir, seed, colours.liquid);
        // Schlieren where the two solutions fold together, fading once the
        // stirrer's window is past.
        let past_mixing =
            (f32::from(ordinal) + ordinal_progress - f32::from(mixing.end_ordinal) - 1.0).max(0.0);
        let mixing_life = smooth01((stir - 0.05) / 0.15) * (1.0 - smooth01(past_mixing / 1.5));
        add_schlieren_swirls(
            &mut meshes.translucent,
            Vec3::new(0.0, animated.liquid_surface, 0.0),
            0.88,
            mixing_life,
            frame / 30.0 * 2.0,
            seed.rotate_left(7),
        );
        // The neutralisation is exothermic: warm haze wisps rise off the
        // mixture, and the warmth mists the upper glass early — clearing
        // once the boil takes over and drives the glass hot.
        add_rising_plume(
            &mut meshes.translucent,
            Vec3::new(0.0, animated.liquid_surface, 0.0),
            mixing_life * 0.35,
            [0.90, 0.93, 0.95, 0.16],
            frame / 30.0 * 2.0,
            seed.rotate_left(11),
        );
        let warm_mist =
            smooth01((stir - 0.35) / 0.4) * (1.0 - smooth01(post_process.boiling / 0.3));
        add_condensation_mist(
            &mut meshes.translucent,
            Vec3::ZERO,
            0.925,
            animated.liquid_surface + 0.14,
            layout.bench_top + 1.72 + post_process.lift,
            warm_mist,
            seed.rotate_left(15),
        );
    }
    add_neutralisation_supplemental_reactants(meshes, moment, vessel_motion);
    add_neutralisation_reaction_gas(meshes, moment, vessel_motion);
    if post_process.active {
        add_evaporation_crystallization_process(
            meshes,
            animated,
            post_process,
            seed,
            effect_colours,
            stage_progress,
        );
    }
}
