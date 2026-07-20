//! Procedural aqueous-precipitation scene: a second solution poured into
//! the basin, an insoluble product clouding the water where the two mix,
//! flecks sinking out of the cloud, and a sediment mound that persists.
//!
//! Everything is a deterministic function of (plan, progress): fixed entity
//! populations whose sizes animate, never appearing or vanishing.

#![allow(clippy::wildcard_imports, clippy::cast_precision_loss)]

use super::*;

/// Receiving-basin geometry, matching the shared assembly beaker.
const BASIN_RADIUS: f32 = 0.88;
/// Initial free-surface height above the bench, preserving the authored
/// basin the baked clip established.
const BASIN_LEVEL: f32 = 1.04;
/// The virtual 30 fps frame count the authored pacing was built around.
const FRAMES: u16 = 180;

fn basin_state(bench_top: f32) -> LiquidState {
    LiquidState {
        surface_centre: Vec3::new(0.0, bench_top + BASIN_LEVEL, 0.0),
        floor_y: bench_top + 0.09,
        radius: BASIN_RADIUS,
        colour: ClipColour::LiquidInitial,
        initial_level_y: bench_top + BASIN_LEVEL,
    }
}

/// The pour leads this scene: the vessel is already arriving as the window
/// opens, so the mixing cloud has the rest of the window to develop.
pub(super) fn procedural_pour_table() -> &'static PourTable {
    static TABLE: OnceLock<PourTable> = OnceLock::new();
    TABLE.get_or_init(|| {
        build_scheduled_pour_table(&PourSchedule {
            approach: (0.0, 10.0),
            tilt_in: (10.0, 24.0),
            tilt_out: (46.0, 58.0),
            retreat: (60.0, 76.0),
        })
    })
}

/// How much product cloud hangs in the water: none before the pour has
/// mixed, full while the product forms, settled out by the end.
pub(super) fn cloud_presence(progress: f32) -> f32 {
    smooth01((progress - 0.12) / 0.10) * (1.0 - smooth01((progress - 0.72) / 0.22))
}

/// How much material is mid-fall: trails the cloud and reaches zero once
/// everything rests in the mound.
pub(super) fn fleck_presence(progress: f32) -> f32 {
    smooth01((progress - 0.16) / 0.10) * (1.0 - smooth01((progress - 0.80) / 0.17))
}

/// The mound accumulates while flecks deliver material, then persists.
pub(super) fn mound_settle(progress: f32) -> f32 {
    smooth01((progress - 0.20) / 0.55)
}

/// Fixed-population haze puffs swirling slowly in the mixing zone below the
/// surface. Sizes scale with `presence`; the count never changes.
fn add_precipitate_cloud(
    mesh: &mut Mesh,
    centre: Vec3,
    colour: [f32; 4],
    presence: f32,
    phase: f32,
    seed: u64,
) {
    const PUFFS: u32 = 14;
    for puff in 0..PUFFS {
        let angle = seeded_unit(seed, puff, 351) * std::f32::consts::TAU + phase * 0.16;
        let radial = seeded_unit(seed, puff, 352).sqrt() * 0.34;
        let depth = 0.10 + seeded_unit(seed, puff, 353) * 0.26;
        let drift = curl_like_flow(phase * 0.5, seed, puff) * 0.05;
        let position = centre
            + Vec3::new(
                angle.cos() * radial + drift.x,
                -depth + (phase * 0.7 + seeded_unit(seed, puff, 354)).sin() * 0.02,
                angle.sin() * radial + drift.z,
            );
        let size = (0.09 + seeded_unit(seed, puff, 355) * 0.08) * presence;
        add_sphere(
            mesh,
            position,
            size.max(0.000_5),
            alpha(colour, colour[3] * (0.55 + 0.45 * presence)),
            4,
            7,
        );
    }
}

/// Fixed-population product flecks sinking out of the cloud — not a uniform
/// rain but streamers: each fleck belongs to one of a few slowly-wandering
/// curtain columns, the way settling precipitate actually channels. Each
/// fleck cycles from the mixing depth to the floor, swelling in from nothing
/// at the top of its cycle and vanishing at the bottom, so the population is
/// constant while material visibly sinks.
fn add_falling_flecks(
    mesh: &mut Mesh,
    state: &LiquidState,
    colour: [f32; 4],
    presence: f32,
    phase: f32,
    seed: u64,
) {
    const STREAMERS: u32 = 6;
    const FLECKS: u32 = 40;
    for fleck in 0..FLECKS {
        let streamer = fleck % STREAMERS;
        let column_angle = seeded_unit(seed, streamer, 367) * std::f32::consts::TAU;
        let column_radial = (0.12 + seeded_unit(seed, streamer, 368) * 0.55) * state.radius;
        let sway = curl_like_flow(phase * 0.35, seed, streamer) * 0.07;
        let rate = 0.22 + seeded_unit(seed, fleck, 361) * 0.24;
        let age = (phase * rate + seeded_unit(seed, fleck, 362)).fract();
        let scatter_angle = seeded_unit(seed, fleck, 363) * std::f32::consts::TAU;
        let scatter = seeded_unit(seed, fleck, 364).sqrt() * 0.07;
        let start_y = state.surface_centre.y - 0.14 - seeded_unit(seed, fleck, 365) * 0.16;
        let fall = age.powf(1.35);
        let drift = curl_like_flow(phase * 0.8, seed, fleck) * 0.02 * age;
        let position = Vec3::new(
            state.surface_centre.x
                + column_angle.cos() * column_radial
                + scatter_angle.cos() * scatter
                + sway.x
                + drift.x,
            start_y + (state.floor_y + 0.03 - start_y) * fall,
            state.surface_centre.z
                + column_angle.sin() * column_radial
                + scatter_angle.sin() * scatter
                + sway.z
                + drift.z,
        );
        let size = (0.014 + seeded_unit(seed, fleck, 366) * 0.018)
            * (std::f32::consts::PI * age).sin().max(0.0).sqrt()
            * presence;
        // Elongated: a falling streak, not a tumbling grain.
        add_shard(
            mesh,
            position,
            Vec3::new(size * 0.7, size * 1.9, size * 0.7).max(Vec3::splat(0.000_5)),
            Quat::from_rotation_y(scatter_angle + age * 2.4),
            colour,
            seed.wrapping_add(u64::from(fleck)),
        );
    }
}

// A linear choreography list; splitting it would scatter one scene's
// reading order.
#[allow(clippy::too_many_lines)]
pub(super) fn add_precipitation_assembly(
    meshes: &mut SceneMeshes,
    plan: &ScenePlan,
    layout: SceneLayout,
    progress: f32,
    ordinal: u16,
    ordinal_progress: f32,
) {
    let precipitation = plan
        .precipitation
        .as_ref()
        .expect("validated precipitation assembly has material bindings");
    let seed = plan_seed(plan);
    let frame = progress.clamp(0.0, 1.0) * f32::from(FRAMES - 1);
    let phase = frame / 30.0 * 2.0;
    add_assembly_beaker(&mut meshes.glass, layout.bench_top, Vec3::ZERO);
    let state = basin_state(layout.bench_top);
    let pour = pour_state_from(procedural_pour_table(), frame, layout.bench_top);
    let receiving_lift = pour.map_or(0.0, |pour| pour.poured * 0.055);
    // As the suspension settles into the mound the liquid clears again,
    // which is also what lets the settled product read on the floor.
    let cleared = mound_settle(progress) * (1.0 - cloud_presence(progress));
    // Conservation made visible: the poured volume raises the level, the
    // falling stream stirs the surface, and the mixed colour spreads from
    // where the stream lands.
    add_receiving_liquid(
        meshes,
        &state,
        pour.as_ref(),
        receiving_lift,
        bound_colour_endpoints(
            &precipitation.initial_liquid,
            0.42 - 0.16 * cleared,
            ordinal,
            ordinal_progress,
        ),
        phase,
        seed,
    );
    // The mixing zone sits where the stream lands while the pour runs, and
    // relaxes to the basin centre once mixing is general.
    let mix_centre = pour.filter(|pour| pour.flow > 0.05).map_or(
        state.surface_centre + Vec3::Y * receiving_lift,
        |pour| {
            Vec3::new(
                pour.lip.x + pour.downhill.x * 0.12,
                state.surface_centre.y + receiving_lift,
                pour.lip.z + pour.downhill.z * 0.12,
            )
        },
    );
    let cloud_colour = precipitation_track_colour(
        ClipColour::PrecipitateCloud,
        precipitation,
        ordinal,
        ordinal_progress,
    );
    let solid_colour = precipitation_track_colour(
        ClipColour::Precipitate,
        precipitation,
        ordinal,
        ordinal_progress,
    );
    add_precipitate_cloud(
        &mut meshes.translucent,
        mix_centre,
        cloud_colour,
        cloud_presence(progress),
        phase,
        seed.rotate_left(9),
    );
    add_falling_flecks(
        &mut meshes.opaque,
        &state,
        solid_colour,
        fleck_presence(progress),
        phase,
        seed.rotate_left(15),
    );
    // The formed product settles into a growing mound on the floor. Its
    // exposed surface ages toward neutral grey under the key light late in
    // the scene — a restrained presentation tint layered over (and always
    // subordinate to) the reviewed product colour.
    let aging = 0.22 * smooth01((progress - 0.55) / 0.35);
    let aged_colour = mix_color(solid_colour, [0.42, 0.41, 0.44, solid_colour[3]], aging);
    let (_, _, growth) =
        bound_colour_endpoints(&precipitation.precipitate, 1.0, ordinal, ordinal_progress);
    add_sediment_mound(
        &mut meshes.opaque,
        Vec3::new(0.0, state.floor_y + 0.004, 0.0),
        state.radius,
        growth * mound_settle(progress),
        0.30,
        aged_colour,
        seed.rotate_left(27),
    );
    // Once the pour has landed its charge, faint schlieren swirls mark the
    // two solutions still mixing, fading as they homogenize.
    add_schlieren_swirls(
        &mut meshes.translucent,
        state.surface_centre + Vec3::Y * receiving_lift,
        state.radius,
        smooth01((frame - 56.0) / 10.0) * (1.0 - smooth01((frame - 115.0) / 40.0)),
        phase,
        seed.rotate_left(21),
    );
    if let Some(pour) = pour {
        add_pouring_vessel_glass(&mut meshes.glass, &pour);
        let added_colour = precipitation_track_colour(
            ClipColour::LiquidAdded,
            precipitation,
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
