//! Procedural heavy-alkali-metal-and-water scene: a lump dropped into the
//! basin detonates on contact — fireball, shock ring, ejected glass chips,
//! a crown of thrown water, then a towering steam column that decays into
//! a hydroxide-tinted calm.
//!
//! One implementation serves every metal: the variant selects independent
//! light, pressure, matter, damage, camera, and aftermath controls instead of
//! shipping three separate baked simulations.
//!
//! Everything is a deterministic function of (plan, progress): fixed entity
//! populations whose sizes animate, never appearing or vanishing.

#![allow(clippy::wildcard_imports, clippy::cast_precision_loss)]

use super::*;

/// Basin level above the bench (the shared alkali layout).
const BASIN_LEVEL: f32 = 1.543;
/// Contact and blast windows in normalized progress, preserving the
/// authored pacing (virtual frames 39 and 65 of 180).
const CONTACT: f32 = 39.0 / 179.0;
const BLAST_END: f32 = 65.0 / 179.0;
const CHURN_END: f32 = 95.0 / 179.0;

/// Independent authored controls for one explosive-water-contact variant.
/// A single scale made every layer grow into the same soft silhouette; these
/// controls let pressure, light, matter, and aftermath carry different beats.
#[derive(Debug, Clone, Copy)]
struct BlastProfile {
    camera_kick: f32,
    flash_energy: f32,
    fireball_reach: f32,
    pressure_radius: f32,
    spray_velocity: f32,
    liquid_ejection: f32,
    debris_velocity: f32,
    steam_yield: f32,
    vessel_damage: f32,
    bench_damage: f32,
    environment_response: f32,
}

const fn blast_profile(variant: ExplosiveMetalWaterVariant) -> BlastProfile {
    match variant {
        ExplosiveMetalWaterVariant::Rubidium => BlastProfile {
            camera_kick: 1.3,
            flash_energy: 1.15,
            fireball_reach: 1.00,
            pressure_radius: 1.05,
            spray_velocity: 1.10,
            liquid_ejection: 0.24,
            debris_velocity: 1.20,
            steam_yield: 1.05,
            vessel_damage: 0.90,
            bench_damage: 0.45,
            environment_response: 0.82,
        },
        ExplosiveMetalWaterVariant::Caesium => BlastProfile {
            camera_kick: 1.9,
            flash_energy: 1.60,
            fireball_reach: 1.42,
            pressure_radius: 1.50,
            spray_velocity: 1.50,
            liquid_ejection: 0.46,
            debris_velocity: 1.75,
            steam_yield: 1.40,
            vessel_damage: 1.35,
            bench_damage: 0.90,
            environment_response: 1.12,
        },
        ExplosiveMetalWaterVariant::Francium => BlastProfile {
            camera_kick: 2.6,
            flash_energy: 2.20,
            fireball_reach: 1.85,
            pressure_radius: 2.05,
            spray_velocity: 1.95,
            liquid_ejection: 0.72,
            debris_velocity: 2.45,
            steam_yield: 1.85,
            vessel_damage: 1.80,
            bench_damage: 1.45,
            environment_response: 1.48,
        },
    }
}

/// The contact flash is deliberately shorter than the body of the fireball:
/// one sharp white-hot punctuation instead of a sustained white blob.
fn contact_flash(progress: f32) -> f32 {
    smooth01((progress - CONTACT) / 0.006) * (1.0 - smooth01((progress - CONTACT - 0.014) / 0.022))
}

/// A brief pre-contact pull that makes the release read harder by contrast.
fn anticipation(progress: f32) -> f32 {
    smooth01((progress - CONTACT + 0.050) / 0.035)
        * (1.0 - smooth01((progress - CONTACT + 0.004) / 0.010))
}

/// Permanent vessel damage grows rapidly at impact and then holds.
fn fracture_age(progress: f32) -> f32 {
    smooth01((progress - CONTACT) / 0.055)
}

/// The blast envelope: detonation at contact, spent shortly after.
fn blast(progress: f32) -> f32 {
    smooth01((progress - CONTACT) / 0.015) * (1.0 - smooth01((progress - BLAST_END + 0.04) / 0.08))
}

/// Expansion of the fireball and shock ring through the blast window.
fn blast_age(progress: f32) -> f32 {
    ((progress - CONTACT) / (BLAST_END - CONTACT)).clamp(0.0, 1.0)
}

/// Surface churn and spray, alive from contact well into the aftermath.
fn churn(progress: f32) -> f32 {
    smooth01((progress - CONTACT) / 0.03) * (1.0 - smooth01((progress - CHURN_END + 0.1) / 0.25))
}

/// Steam rises hard after the blast and thins out late.
fn steam(progress: f32) -> f32 {
    smooth01((progress - CONTACT - 0.01) / 0.05) * (1.0 - smooth01((progress - 0.78) / 0.2))
}

/// Renderer-wide responses that let the blast affect framing and exposure
/// without teaching the post chain anything about reaction identity.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct BlastVisualResponse {
    pub(super) shake: f32,
    pub(super) recoil: f32,
    pub(super) exposure: f32,
    pub(super) heat: f32,
    pub(super) fog: f32,
}

pub(super) fn blast_visual_response(
    plan: &ScenePlan,
    moment: RealWorldPosition,
) -> BlastVisualResponse {
    plan.explosive_metal_water
        .as_ref()
        .map_or_else(BlastVisualResponse::default, |explosive| {
            let progress = plan.timeline.normalized_progress_at(moment);
            let profile = blast_profile(explosive.variant);
            let body = blast(progress);
            let flash = contact_flash(progress);
            BlastVisualResponse {
                shake: body * profile.camera_kick * profile.environment_response,
                recoil: flash * profile.camera_kick * profile.environment_response,
                exposure: (flash * profile.flash_energy * profile.environment_response * 0.62)
                    .min(1.0),
                heat: (body * profile.fireball_reach).min(1.0),
                fog: (steam(progress) * profile.steam_yield * profile.environment_response * 0.48)
                    .min(1.0),
            }
        })
}

/// How far the thrown water has left the basin after the blast: the level
/// drops over the churn window and stays down.
fn level_drop(progress: f32, liquid_ejection: f32) -> f32 {
    smooth01((progress - CONTACT) / 0.20) * (0.35 + liquid_ejection * 1.35)
}

/// Replayable rigid-debris motion. The launch follows gravity until the
/// fragment reaches the bench, then loses horizontal speed through friction
/// while a small damped bounce settles it into a stable pose.
#[derive(Debug, Clone, Copy, Default)]
struct DebrisMotion {
    offset: Vec3,
    rotation_time: f32,
    settled: f32,
}

#[derive(Debug, Clone, Copy)]
struct GlassWallChunk {
    angle: f32,
    half_span: f32,
    first_row: u16,
    last_row: u16,
    min_damage: f32,
    launch_scale: f32,
    lift_scale: f32,
}

impl GlassWallChunk {
    fn is_active(self, damage: f32) -> bool {
        damage >= self.min_damage
    }

    fn contains(self, angle: f32, row: u16, damage: f32) -> bool {
        if !self.is_active(damage) || row < self.first_row || row > self.last_row {
            return false;
        }
        let row_center = (f32::from(self.first_row) + f32::from(self.last_row)) * 0.5;
        let half_height = f32::from(self.last_row - self.first_row + 1) * 0.5;
        let edge_distance = ((f32::from(row) - row_center).abs() / half_height).min(1.0);
        let tapered_span = self.half_span * (1.0 - edge_distance * 0.38);
        let row_offset = match (row - self.first_row) % 3 {
            0 => -self.half_span * 0.30,
            1 => self.half_span * 0.12,
            _ => self.half_span * 0.27,
        };
        angular_distance(angle, self.angle + row_offset) <= tapered_span
    }
}

fn angular_distance(left: f32, right: f32) -> f32 {
    let distance = (left - right).rem_euclid(std::f32::consts::TAU);
    distance.min(std::f32::consts::TAU - distance)
}

/// A few contiguous portions of the cylindrical wall detach as recognisable
/// shell pieces. Smaller variants activate fewer sections; the strongest
/// variant opens both rupture sites without atomising the whole vessel.
fn glass_wall_chunks(first_gap: f32, second_gap: f32, damage: f32) -> [GlassWallChunk; 5] {
    let span = 0.13 + damage * 0.055;
    [
        GlassWallChunk {
            angle: first_gap,
            half_span: span * 1.18,
            first_row: 5,
            last_row: 7,
            min_damage: 0.70,
            launch_scale: 0.24,
            lift_scale: 0.54,
        },
        GlassWallChunk {
            angle: first_gap - span * 1.55,
            half_span: span * 0.82,
            first_row: 3,
            last_row: 5,
            min_damage: 1.02,
            launch_scale: 0.29,
            lift_scale: 0.47,
        },
        GlassWallChunk {
            angle: second_gap,
            half_span: span,
            first_row: 4,
            last_row: 7,
            min_damage: 1.18,
            launch_scale: 0.27,
            lift_scale: 0.51,
        },
        GlassWallChunk {
            angle: second_gap + span * 1.45,
            half_span: span * 0.72,
            first_row: 2,
            last_row: 4,
            min_damage: 1.48,
            launch_scale: 0.22,
            lift_scale: 0.42,
        },
        GlassWallChunk {
            angle: first_gap + span * 1.62,
            half_span: span * 0.66,
            first_row: 1,
            last_row: 3,
            min_damage: 1.68,
            launch_scale: 0.20,
            lift_scale: 0.38,
        },
    ]
}

fn fractured_wall_vertex(
    point: Vec3,
    angle: f32,
    grid_row: u16,
    grid_segment: u16,
    wall_segments: u16,
    fracture: f32,
    seed: u64,
) -> Vec3 {
    let wrapped_segment = grid_segment % wall_segments;
    let index = u32::from(grid_row) * u32::from(wall_segments) + u32::from(wrapped_segment);
    let tangent = Vec3::new(-angle.sin(), 0.0, angle.cos());
    point
        + tangent * seeded_variation(seed, usize::try_from(index).unwrap_or(0)) * 0.095 * fracture
        + Vec3::Y
            * seeded_variation(seed.rotate_left(11), usize::try_from(index).unwrap_or(0))
            * 0.082
            * fracture
}

fn debris_motion(
    origin: Vec3,
    velocity: Vec3,
    progress: f32,
    floor_y: f32,
    drag: f32,
    restitution: f32,
) -> DebrisMotion {
    const GRAVITY: f32 = 3.8;
    const SECONDS_PER_PROGRESS: f32 = 7.5;
    let time = (progress - CONTACT).max(0.0) * SECONDS_PER_PROGRESS;
    let height = (origin.y - floor_y).max(0.0);
    let impact_time =
        (velocity.y + (velocity.y * velocity.y + 2.0 * GRAVITY * height).sqrt()) / GRAVITY;
    if time <= impact_time {
        return DebrisMotion {
            offset: velocity * time - Vec3::Y * (GRAVITY * time * time * 0.5),
            rotation_time: time,
            settled: 0.0,
        };
    }

    let after_impact = time - impact_time;
    let horizontal_velocity = Vec3::new(velocity.x, 0.0, velocity.z);
    let impact_offset = horizontal_velocity * impact_time + Vec3::Y * (floor_y - origin.y);
    let skid_time = (1.0 - (-drag * after_impact).exp()) / drag.max(0.01);
    let impact_speed = (velocity.y - GRAVITY * impact_time).abs();
    let bounce =
        (after_impact * 8.5).sin().abs() * (-4.6 * after_impact).exp() * impact_speed * restitution;
    DebrisMotion {
        offset: impact_offset + horizontal_velocity * skid_time + Vec3::Y * bounce,
        rotation_time: impact_time + skid_time,
        settled: smooth01((after_impact - 0.18) / 0.90),
    }
}

/// The vessel wall and lip are deterministic panels. Selected upper-wall
/// panels leave the cylinder on ballistic paths after contact, creating actual
/// holes rather than drawing cracks over an intact beaker. The panels always
/// exist, so seeking preserves the topology invariant.
// Keeping the wall and lip loops together makes their shared fracture wedges
// auditable; splitting this linear geometry builder would duplicate that state.
#[allow(clippy::too_many_lines)]
fn add_explosion_beaker(
    mesh: &mut Mesh,
    bench_top: f32,
    progress: f32,
    profile: BlastProfile,
    seed: u64,
) {
    const GLASS: [f32; 4] = [0.62, 0.84, 0.94, 0.22];
    const RADIUS: f32 = 0.94;
    const RIM_SEGMENTS: u16 = 24;
    const WALL_SEGMENTS: u16 = 32;
    const WALL_ROWS: u16 = 8;
    let damage = profile.vessel_damage;
    let bottom = Vec3::new(0.0, bench_top + 0.02, 0.0);
    let top = Vec3::new(0.0, bench_top + 1.80, 0.0);
    add_ring(
        mesh,
        bottom + Vec3::Y * 0.012,
        RADIUS * 0.985,
        0.020,
        [0.52, 0.76, 0.88, 0.16],
    );
    add_disc(
        mesh,
        bottom + Vec3::Y * 0.016,
        RADIUS * 0.97,
        [0.48, 0.72, 0.84, 0.10],
    );

    let first_gap = seed_phase(seed, 601);
    let second_gap = (first_gap + std::f32::consts::PI * 0.82) % std::f32::consts::TAU;
    let gap_width = 0.22 + damage * 0.22;
    let chunks = glass_wall_chunks(first_gap, second_gap, damage);

    for row in 0..WALL_ROWS {
        let low_y = bottom.y + (top.y - bottom.y) * f32::from(row) / f32::from(WALL_ROWS);
        let high_y = bottom.y + (top.y - bottom.y) * f32::from(row + 1) / f32::from(WALL_ROWS);
        for segment in 0..WALL_SEGMENTS {
            let start_angle = std::f32::consts::TAU * f32::from(segment) / f32::from(WALL_SEGMENTS);
            let end_angle =
                std::f32::consts::TAU * f32::from(segment + 1) / f32::from(WALL_SEGMENTS);
            let middle = (start_angle + end_angle) * 0.5;
            let mut local_a = Vec3::new(
                start_angle.cos() * RADIUS,
                low_y,
                start_angle.sin() * RADIUS,
            );
            let mut local_b = Vec3::new(
                start_angle.cos() * RADIUS,
                high_y,
                start_angle.sin() * RADIUS,
            );
            let mut local_c = Vec3::new(end_angle.cos() * RADIUS, high_y, end_angle.sin() * RADIUS);
            let mut local_d = Vec3::new(end_angle.cos() * RADIUS, low_y, end_angle.sin() * RADIUS);
            let chunk_index = chunks
                .iter()
                .position(|chunk| chunk.contains(middle, row, damage));
            if chunk_index.is_some() {
                let fracture = fracture_age(progress);
                local_a = fractured_wall_vertex(
                    local_a,
                    start_angle,
                    row,
                    segment,
                    WALL_SEGMENTS,
                    fracture,
                    seed,
                );
                local_b = fractured_wall_vertex(
                    local_b,
                    start_angle,
                    row + 1,
                    segment,
                    WALL_SEGMENTS,
                    fracture,
                    seed,
                );
                local_c = fractured_wall_vertex(
                    local_c,
                    end_angle,
                    row + 1,
                    segment + 1,
                    WALL_SEGMENTS,
                    fracture,
                    seed,
                );
                local_d = fractured_wall_vertex(
                    local_d,
                    end_angle,
                    row,
                    segment + 1,
                    WALL_SEGMENTS,
                    fracture,
                    seed,
                );
            }
            let (motion, rotation, center) = chunk_index.map_or_else(
                || (DebrisMotion::default(), Quat::IDENTITY, Vec3::ZERO),
                |chunk_index| {
                    let chunk = chunks[chunk_index];
                    let radial = Vec3::new(chunk.angle.cos(), 0.0, chunk.angle.sin());
                    let tangent = radial.cross(Vec3::Y);
                    let center_y = bottom.y
                        + (top.y - bottom.y)
                            * (f32::from(chunk.first_row + chunk.last_row + 1) * 0.5)
                            / f32::from(WALL_ROWS);
                    let center = radial * RADIUS + Vec3::Y * center_y;
                    let stagger = seeded_unit(seed, u32::try_from(chunk_index).unwrap_or(0), 600);
                    let velocity = radial
                        * profile.debris_velocity
                        * chunk.launch_scale
                        * (0.82 + stagger * 0.24)
                        + tangent * (stagger - 0.5) * 0.24
                        + Vec3::Y * damage * chunk.lift_scale;
                    let motion =
                        debris_motion(center, velocity, progress, bench_top + 0.045, 5.4, 0.055);
                    let flying_rotation = Quat::from_axis_angle(
                        (tangent + Vec3::Y * (0.18 + stagger * 0.20)).normalize_or_zero(),
                        motion.rotation_time * (1.25 + stagger * 1.35),
                    );
                    let resting_rotation = Quat::from_rotation_y(stagger * std::f32::consts::TAU)
                        * Quat::from_rotation_arc(radial, Vec3::Y);
                    (
                        motion,
                        flying_rotation.slerp(resting_rotation, motion.settled),
                        center,
                    )
                },
            );
            let transform = |point: Vec3| center + motion.offset + rotation * (point - center);
            add_flat_triangle(
                mesh,
                transform(local_a),
                transform(local_b),
                transform(local_c),
                GLASS,
            );
            add_flat_triangle(
                mesh,
                transform(local_a),
                transform(local_c),
                transform(local_d),
                GLASS,
            );
        }
    }

    for segment in 0..RIM_SEGMENTS {
        let start_angle = std::f32::consts::TAU * f32::from(segment) / f32::from(RIM_SEGMENTS);
        let end_angle = std::f32::consts::TAU * f32::from(segment + 1) / f32::from(RIM_SEGMENTS);
        let middle = (start_angle + end_angle) * 0.5;
        let fragment = angular_distance(middle, first_gap) < gap_width
            || angular_distance(middle, second_gap) < gap_width * 0.78;
        let radial = Vec3::new(middle.cos(), 0.0, middle.sin());
        let stagger = seeded_unit(seed, u32::from(segment), 602);
        let local_start =
            top + Vec3::new(start_angle.cos() * RADIUS, 0.0, start_angle.sin() * RADIUS);
        let local_end = top + Vec3::new(end_angle.cos() * RADIUS, 0.0, end_angle.sin() * RADIUS);
        let center = local_start.lerp(local_end, 0.5);
        let velocity = radial * (0.48 + damage * (0.38 + stagger * 0.14))
            + Vec3::Y * ((0.62 + stagger * 0.54) * damage);
        let motion = if fragment {
            debris_motion(center, velocity, progress, bench_top + 0.045, 5.2, 0.055)
        } else {
            DebrisMotion::default()
        };
        let tangent = radial.cross(Vec3::Y);
        let flying_rotation = Quat::from_axis_angle(
            (tangent + Vec3::Y * 0.2).normalize_or_zero(),
            motion.rotation_time * (3.0 + stagger * 4.0),
        );
        let resting_rotation = Quat::from_rotation_y(stagger * std::f32::consts::TAU)
            * Quat::from_rotation_arc(radial, Vec3::Y);
        let rotation = flying_rotation.slerp(resting_rotation, motion.settled);
        let start = center + motion.offset + rotation * (local_start - center);
        let end = center + motion.offset + rotation * (local_end - center);
        add_cylinder(mesh, start, end, 0.022, [0.62, 0.84, 0.94, 0.28]);
    }
}

/// The falling lump: a heavy drop straight into the basin, gone at contact.
fn add_falling_lump(
    mesh: &mut Mesh,
    surface: Vec3,
    progress: f32,
    intensity: f32,
    colour: [f32; 4],
    seed: u64,
) {
    // Held poised over the water, then released for a fast, heavy drop.
    let fall = ((progress - (CONTACT - 0.045)) / 0.045).clamp(0.0, 1.0);
    let consumed = smooth01((progress - CONTACT) / 0.02);
    let size = 0.15 * (0.85 + intensity * 0.2) * (1.0 - consumed);
    let position = surface + Vec3::Y * ((1.0 - fall * fall) * 0.95 + 0.02);
    let tumble = Quat::from_rotation_y(fall * 3.1 + seed_phase(seed, 471))
        * Quat::from_rotation_x(fall * 2.2);
    add_sphere(mesh, position, size.max(0.000_5), colour, 5, 8);
    add_shard(
        mesh,
        position + Vec3::new(size * 0.4, size * 0.2, 0.0),
        Vec3::splat((size * 0.55).max(0.000_5)),
        tumble,
        colour,
        seed ^ 0x77,
    );
}

/// Concentric surface-tension rings pull inward immediately before contact.
fn add_contact_dimple(mesh: &mut Mesh, impact: Vec3, progress: f32, pressure_radius: f32) {
    let pull = anticipation(progress);
    for ring in 0..3_u16 {
        let ring_factor = f32::from(ring);
        add_ring(
            mesh,
            impact + Vec3::Y * (0.008 - ring_factor * 0.003),
            (0.08 + ring_factor * 0.075) * (1.0 - pull * 0.34),
            0.008 - ring_factor * 0.0015,
            [
                0.76,
                0.91,
                0.98,
                pull * (0.46 - ring_factor * 0.10) * pressure_radius.min(1.2),
            ],
        );
    }
}

/// A compact white-hot core and radial needles mark the instant of contact.
fn add_contact_flash(mesh: &mut Mesh, impact: Vec3, progress: f32, energy: f32, seed: u64) {
    const RAYS: u32 = 12;
    let flash = contact_flash(progress);
    add_sphere(
        mesh,
        impact + Vec3::Y * 0.045,
        (0.025 + flash * 0.15 * energy).max(0.000_5),
        [1.0, 0.99, 0.88, (flash * 0.98).max(0.02)],
        4,
        7,
    );
    for ray in 0..RAYS {
        let angle = seeded_unit(seed, ray, 475) * std::f32::consts::TAU;
        let lift = 0.08 + seeded_unit(seed, ray, 476) * 0.55;
        let reach = 0.000_5 + flash * energy * (0.25 + seeded_unit(seed, ray, 477) * 0.48);
        let base = impact + Vec3::Y * 0.04;
        let tip = base + Vec3::new(angle.cos() * reach, lift * reach, angle.sin() * reach);
        add_flame_lobe(
            mesh,
            base,
            tip,
            (0.002 + flash * (0.010 + seeded_unit(seed, ray, 478) * 0.014)).max(0.002),
            [1.0, 0.91, 0.58, (flash * 0.86).max(0.02)],
        );
    }
}

/// A compact HDR core anchors a low, asymmetric fireball made from tapered
/// bodies rather than a stack of translucent spheres. Hot inner tongues lead;
/// the coloured shell follows.
fn add_fireball(
    meshes: &mut SceneMeshes,
    impact: Vec3,
    presence: f32,
    age: f32,
    reach_scale: f32,
    phase: f32,
    seed: u64,
) {
    const LOBES: u32 = 9;
    let flame = flame_colours(FlamePalette::Natural);
    let lifecycle = presence * (1.0 - age * 0.52);
    let core_lifecycle = presence * (1.0 - age * 0.72);
    let reach = (0.40 + age * 1.10) * reach_scale * (0.22 + presence * 0.78);
    add_sphere(
        &mut meshes.translucent,
        impact + Vec3::Y * (0.08 + age * 0.16 * reach_scale),
        (0.018 + core_lifecycle * (0.18 + age * 0.12) * reach_scale).max(0.000_5),
        alpha(flame.body_high, (core_lifecycle * 0.78).max(0.002)),
        5,
        8,
    );
    add_sphere(
        &mut meshes.emissive,
        impact + Vec3::Y * (0.075 + age * 0.12 * reach_scale),
        (0.012 + core_lifecycle * (0.11 + age * 0.07) * reach_scale).max(0.000_5),
        [1.0, 0.93, 0.58, (core_lifecycle * 0.94).max(0.002)],
        4,
        7,
    );
    for lobe in 0..LOBES {
        let lobe_factor = u16::try_from(lobe).map_or(0.0, f32::from);
        let angle = seeded_unit(seed, lobe, 481) * std::f32::consts::TAU
            + phase * 0.09
            + lobe_factor * 0.07;
        let radial = Vec3::new(angle.cos(), 0.0, angle.sin());
        let lift = 0.20 + seeded_unit(seed, lobe, 482) * 0.52;
        let throw = 0.48 + seeded_unit(seed, lobe, 483) * 0.62;
        let base =
            impact + radial * (0.025 + seeded_unit(seed, lobe, 484) * 0.08) + Vec3::Y * 0.025;
        let tip = base + radial * reach * throw + Vec3::Y * reach * lift;
        let width =
            (0.075 + age * 0.13 + seeded_unit(seed, lobe, 485) * 0.055) * reach_scale * lifecycle;
        let body_colour = mix_color(
            flame.body_low,
            flame.body_high,
            seeded_unit(seed, lobe, 486) * 0.86,
        );
        add_flame_lobe(
            &mut meshes.translucent,
            base,
            tip,
            width.max(0.002),
            alpha(body_colour, (lifecycle * 0.90).max(0.002)),
        );
        add_flame_lobe(
            &mut meshes.emissive,
            base + Vec3::Y * 0.008,
            base.lerp(tip, 0.64),
            (width * 0.42).max(0.002),
            alpha(flame.core, (lifecycle * (1.0 - age) * 0.94).max(0.002)),
        );
    }
}

/// A pressure volume plus two ground intersections: the faint volume reads as
/// displaced air while the surface and bench rings anchor it in the scene.
fn add_shock_rings(
    mesh: &mut Mesh,
    impact: Vec3,
    bench_top: f32,
    age: f32,
    presence: f32,
    radius_scale: f32,
) {
    let surface_radius = 0.10 + age * 1.08 * radius_scale;
    add_sphere(
        mesh,
        impact + Vec3::Y * 0.04,
        surface_radius * 0.86,
        [0.82, 0.92, 0.98, 0.10 * presence * (1.0 - age)],
        7,
        12,
    );
    add_ring(
        mesh,
        impact + Vec3::Y * 0.015,
        surface_radius,
        0.014 * (1.0 - age * 0.45),
        [0.94, 0.98, 1.0, 0.72 * presence * (1.0 - age)],
    );
    let bench_radius = 0.18 + age * 1.55 * radius_scale;
    add_ring(
        mesh,
        Vec3::new(impact.x, bench_top + 0.025, impact.z),
        bench_radius,
        0.010 * (1.0 - age * 0.62),
        [0.74, 0.88, 0.96, 0.34 * presence * (1.0 - age)],
    );
}

/// A coherent annular sheet carries the apparent bulk of the water upward
/// before it tears into the smaller crown and droplets. The sheet opens,
/// stretches, thins, and falls; it is not a collection of vertical spikes.
fn add_bulk_liquid_ejection(
    mesh: &mut Mesh,
    impact: Vec3,
    progress: f32,
    profile: BlastProfile,
    colour: [f32; 4],
    seed: u64,
) {
    const SEGMENTS: u16 = 20;
    let time = ((progress - CONTACT) / 0.50).clamp(0.0, 1.0);
    let launch = smooth01((progress - CONTACT) / 0.012);
    let tear = smooth01((time - 0.40) / 0.42);
    let height = 4.0 * time * (1.0 - time) * (0.72 + profile.spray_velocity * 0.55);
    let outer_radius = 0.24 + time * (0.78 + profile.spray_velocity * 0.58);
    let inner_radius = 0.10 + time * 0.42;
    let sheet_alpha = launch * (1.0 - tear * 0.88) * (0.48 + profile.liquid_ejection * 0.48);
    for segment in 0..SEGMENTS {
        let start_angle = std::f32::consts::TAU * f32::from(segment) / f32::from(SEGMENTS);
        let end_angle = std::f32::consts::TAU * f32::from(segment + 1) / f32::from(SEGMENTS);
        let middle = (start_angle + end_angle) * 0.5;
        let variation = seeded_unit(seed, u32::from(segment), 530);
        let torn = smooth01((tear - variation * 0.38) / 0.34);
        let segment_alpha = sheet_alpha * (1.0 - torn * 0.72);
        let lower_y = impact.y + height * (0.18 + variation * 0.12);
        let upper_y = impact.y + height * (0.76 + variation * 0.24);
        let local_outer = outer_radius * (0.88 + variation * 0.20);
        let lower_start = impact
            + Vec3::new(
                start_angle.cos() * inner_radius,
                lower_y - impact.y,
                start_angle.sin() * inner_radius,
            );
        let lower_end = impact
            + Vec3::new(
                end_angle.cos() * inner_radius,
                lower_y - impact.y,
                end_angle.sin() * inner_radius,
            );
        let upper = impact
            + Vec3::new(
                middle.cos() * local_outer,
                upper_y - impact.y,
                middle.sin() * local_outer,
            );
        let sheet_colour = alpha(colour, segment_alpha.max(0.0));
        add_flat_triangle(mesh, lower_start, upper, lower_end, sheet_colour);
        // A narrower highlight plane gives the sheet thickness and curvature
        // without turning it into an opaque solid fan.
        let highlight = lower_start.lerp(lower_end, 0.5).lerp(upper, 0.68);
        add_flat_triangle(
            mesh,
            lower_start.lerp(upper, 0.28),
            highlight + Vec3::Y * 0.018,
            lower_end.lerp(upper, 0.28),
            alpha(colour, segment_alpha * 0.56),
        );
    }
}

/// The blast leaves localized structural evidence on the stage: branching
/// fissures, lifted surface chips, dust at the pressure front, and liquid that
/// has escaped the vessel. This is bounded tabletop damage rather than an
/// untyped claim that the laboratory itself has collapsed.
fn add_bench_aftermath(
    meshes: &mut SceneMeshes,
    impact: Vec3,
    bench_top: f32,
    progress: f32,
    profile: BlastProfile,
    water_colour: [f32; 4],
    seed: u64,
) {
    const CRACKS: u32 = 12;
    const SEGMENTS: u32 = 5;
    const CHUNKS: u32 = 14;
    const DUST: u32 = 28;
    let damage_age = fracture_age(progress);
    let damage = damage_age * profile.bench_damage;

    for crack in 0..CRACKS {
        let angle = seeded_unit(seed, crack, 540) * std::f32::consts::TAU;
        let reach = (0.58 + seeded_unit(seed, crack, 541) * 1.20) * damage;
        let mut node = Vec3::new(angle.cos() * 0.76, bench_top + 0.032, angle.sin() * 0.76);
        let mut heading = angle;
        for segment in 0..SEGMENTS {
            let index = crack * SEGMENTS + segment;
            let growth = smooth01((progress - CONTACT - 0.004 * (segment + 1) as f32) / 0.035);
            heading += (seeded_unit(seed, index, 542) - 0.5) * 0.44;
            let length =
                (reach * (0.12 + seeded_unit(seed, index, 543) * 0.10) * growth).max(0.000_5);
            let next = node + Vec3::new(heading.cos() * length, 0.0, heading.sin() * length);
            add_cylinder(
                &mut meshes.opaque,
                node,
                next,
                0.006 + damage * 0.002,
                [0.035, 0.040, 0.052, damage_age * 0.82],
            );
            add_cylinder(
                &mut meshes.opaque,
                node + Vec3::new(0.0, 0.004, 0.010),
                next + Vec3::new(0.0, 0.004, 0.010),
                0.002,
                [0.30, 0.32, 0.36, damage_age * 0.68],
            );
            node = next;
        }
    }

    let flight = ((progress - CONTACT) / 0.34).clamp(0.0, 1.0);
    for chunk in 0..CHUNKS {
        let angle = seeded_unit(seed, chunk, 545) * std::f32::consts::TAU;
        let velocity = (0.35 + seeded_unit(seed, chunk, 546) * 0.60) * profile.bench_damage;
        let launch = Vec3::new(
            angle.cos() * velocity,
            0.32 + seeded_unit(seed, chunk, 547) * 0.58,
            angle.sin() * velocity,
        );
        let start = Vec3::new(angle.cos() * 0.88, bench_top + 0.04, angle.sin() * 0.88);
        let mut position = start + launch * flight - Vec3::Y * (1.55 * flight * flight);
        position.y = position.y.max(bench_top + 0.035);
        let size = (0.018 + seeded_unit(seed, chunk, 548) * 0.030) * damage.max(0.000_5);
        add_shard(
            &mut meshes.opaque,
            position,
            Vec3::new(size * 1.4, size * 0.45, size),
            Quat::from_rotation_y(angle + flight * 5.0) * Quat::from_rotation_z(flight * 2.8),
            [0.13, 0.15, 0.18, damage_age],
            seed.wrapping_add(u64::from(chunk)),
        );
    }

    let pressure_age = blast_age(progress);
    let dust_presence = blast(progress) * profile.environment_response;
    let dust_radius = 0.62 + pressure_age * 2.25 * profile.pressure_radius;
    for mote in 0..DUST {
        let angle = seeded_unit(seed, mote, 550) * std::f32::consts::TAU;
        let spread = 0.72 + seeded_unit(seed, mote, 551) * 0.32;
        let position = Vec3::new(
            angle.cos() * dust_radius * spread,
            bench_top + 0.04 + seeded_unit(seed, mote, 552) * 0.22 * dust_presence,
            angle.sin() * dust_radius * spread,
        );
        let size = (0.014 + seeded_unit(seed, mote, 553) * 0.034) * dust_presence;
        add_sphere(
            &mut meshes.translucent,
            position,
            size.max(0.000_5),
            [0.38, 0.40, 0.43, (dust_presence * 0.34).min(0.42)],
            3,
            5,
        );
    }

    let wet = smooth01((progress - CONTACT - 0.12) / 0.22);
    for pool in 0..9_u32 {
        let angle = seeded_unit(seed, pool, 555) * std::f32::consts::TAU;
        let distance = 0.72 + seeded_unit(seed, pool, 556) * 1.28 * profile.liquid_ejection;
        let center = Vec3::new(
            angle.cos() * distance,
            bench_top + 0.037,
            angle.sin() * distance,
        );
        let radius = (0.10 + seeded_unit(seed, pool, 557) * 0.22) * profile.liquid_ejection * wet;
        add_disc(
            &mut meshes.translucent,
            center,
            radius.max(0.000_5),
            alpha(water_colour, wet * 0.20),
        );
    }

    let _ = impact;
}

/// A tall, widening aftermath column. Steam is allowed to use overlapping
/// soft volumes: unlike the flame, its readable cue is mass and lift rather
/// than a hard silhouette.
fn add_blast_steam_column(
    mesh: &mut Mesh,
    surface: Vec3,
    strength: f32,
    colour: [f32; 4],
    phase: f32,
    seed: u64,
) {
    const PUFFS: u32 = 24;
    for puff in 0..PUFFS {
        let rate = 0.20 + seeded_unit(seed, puff, 489) * 0.24;
        let age = (phase * rate + seeded_unit(seed, puff, 490)).fract();
        let angle = seeded_unit(seed, puff, 491) * std::f32::consts::TAU;
        let radial = seeded_unit(seed, puff, 492).sqrt() * (0.12 + age * 0.34);
        let curl = curl_like_flow(phase * 0.42, seed, puff) * (0.10 + age * 0.20);
        let position = surface
            + Vec3::new(
                angle.cos() * radial,
                0.08 + age * (1.18 + strength * 0.58),
                angle.sin() * radial,
            )
            + Vec3::new(curl.x, 0.0, curl.z);
        let size = (0.045 + age * 0.13) * (0.30 + strength * 0.70);
        let fade = alpha(colour, strength.min(1.25) * (1.0 - age).powf(0.72));
        add_sphere(mesh, position, size.max(0.000_5), fade, 4, 7);
    }
}

/// A thin, irregular plate used only for broken glass. Unlike the pointed
/// crystal primitive, this preserves a broad fractured face, a varied outline,
/// and a narrow edge that remains visible while the chip tumbles.
fn add_glass_chip(
    mesh: &mut Mesh,
    center: Vec3,
    radius: f32,
    thickness: f32,
    rotation: Quat,
    colour: [f32; 4],
    seed: u64,
) {
    const SIDES: usize = 6;
    let mut upper = [Vec3::ZERO; SIDES];
    let mut lower = [Vec3::ZERO; SIDES];
    for index in 0..SIDES {
        let angle = std::f32::consts::TAU * index as f32 / SIDES as f32
            + seeded_variation(seed, index) * 0.24;
        let radial =
            radius * (0.56 + seeded_unit(seed, u32::try_from(index).unwrap_or(0), 733) * 0.68);
        let stretch = 0.72 + seeded_unit(seed, u32::try_from(index).unwrap_or(0), 734) * 0.62;
        let local = Vec3::new(
            angle.cos() * radial * stretch,
            thickness,
            angle.sin() * radial,
        );
        upper[index] = center + rotation * local;
        lower[index] = center + rotation * Vec3::new(local.x, -thickness, local.z);
    }
    for index in 1..SIDES - 1 {
        add_flat_triangle(mesh, upper[0], upper[index], upper[index + 1], colour);
        add_flat_triangle(mesh, lower[0], lower[index + 1], lower[index], colour);
    }
    for index in 0..SIDES {
        let next = (index + 1) % SIDES;
        add_flat_triangle(mesh, upper[index], lower[index], lower[next], colour);
        add_flat_triangle(mesh, upper[index], lower[next], upper[next], colour);
    }
}

#[derive(Debug, Clone, Copy)]
struct GlassChipProfile {
    radius: f32,
    speed: f32,
    lift: f32,
    drag: f32,
}

fn glass_chip_profile(seed: u64, chip: u32, debris_velocity: f32) -> GlassChipProfile {
    let size_class = seeded_unit(seed, chip, 735);
    let radius = if size_class < 0.14 {
        0.045 + seeded_unit(seed, chip, 736) * 0.040
    } else {
        0.009 + seeded_unit(seed, chip, 736).powf(2.2) * 0.028
    };
    let travel = seeded_unit(seed, chip, 737).powf(2.7);
    GlassChipProfile {
        radius,
        speed: debris_velocity * (0.15 + travel * 0.54),
        lift: (0.42 + seeded_unit(seed, chip, 738) * 0.72) * (0.76 + debris_velocity * 0.16),
        drag: if size_class < 0.14 { 5.6 } else { 7.4 },
    }
}

/// Numerous small chips accompany a handful of much heavier wall sections.
/// Their heavy-tailed size and speed distributions keep most of the glass
/// close to the vessel while allowing a sparse set of energetic outliers.
fn add_glass_chips(
    mesh: &mut Mesh,
    bench_top: f32,
    progress: f32,
    profile: BlastProfile,
    seed: u64,
) {
    const CHIPS: u32 = 56;
    let damage_age = fracture_age(progress);
    for chip in 0..CHIPS {
        let angle = seeded_unit(seed, chip, 491) * std::f32::consts::TAU;
        let chip_profile = glass_chip_profile(seed, chip, profile.debris_velocity);
        let tangent = Vec3::new(-angle.sin(), 0.0, angle.cos());
        let launch = Vec3::new(
            angle.cos() * chip_profile.speed,
            chip_profile.lift,
            angle.sin() * chip_profile.speed,
        ) + tangent * (seeded_unit(seed, chip, 739) - 0.5) * chip_profile.speed * 0.36;
        let start_height = 0.56 + seeded_unit(seed, chip, 740) * 1.12;
        let start = Vec3::new(
            angle.cos() * 0.92,
            bench_top + start_height,
            angle.sin() * 0.92,
        );
        let motion = debris_motion(
            start,
            launch,
            progress,
            bench_top + 0.022,
            chip_profile.drag,
            0.035,
        );
        let flying_spin =
            Quat::from_rotation_y(motion.rotation_time * 7.0 + seed_phase(seed, 495 + chip))
                * Quat::from_rotation_z(motion.rotation_time * 5.4);
        let resting_spin = Quat::from_rotation_y(angle + seed_phase(seed, 496 + chip))
            * Quat::from_rotation_z((seeded_unit(seed, chip, 741) - 0.5) * 0.16);
        let spin = flying_spin.slerp(resting_spin, motion.settled);
        let radius = chip_profile.radius * damage_age * (0.84 + profile.vessel_damage * 0.16);
        add_glass_chip(
            mesh,
            start + motion.offset,
            radius.max(0.000_5),
            (radius * 0.075).max(0.000_4),
            spin,
            [0.62, 0.84, 0.94, 0.55 * damage_age],
            seed.wrapping_add(u64::from(chip)),
        );
    }
}

/// Molten metal droplets hurled out of the fireball, each dragging a short
/// emissive trail of cooling beads behind its arc. Fixed population: eight
/// arcs of four beads, all floored outside the blast.
fn add_molten_ejecta(
    mesh: &mut Mesh,
    impact: Vec3,
    progress: f32,
    debris_velocity: f32,
    seed: u64,
) {
    const EJECTA: u32 = 12;
    const BEADS: u32 = 5;
    let flame = flame_colours(FlamePalette::Natural);
    let flight = ((progress - CONTACT) / 0.16).clamp(0.0, 1.0);
    let presence =
        smooth01((progress - CONTACT) / 0.012) * (1.0 - smooth01((flight - 0.78) / 0.22));
    for arc in 0..EJECTA {
        let angle = seeded_unit(seed, arc, 511) * std::f32::consts::TAU;
        let out = (0.9 + seeded_unit(seed, arc, 512) * 1.1) * debris_velocity;
        let up = (1.5 + seeded_unit(seed, arc, 513) * 1.3) * debris_velocity;
        let heat = seeded_unit(seed, arc, 514);
        for bead in 0..BEADS {
            // The head leads; each bead behind it re-evaluates the same arc
            // slightly earlier, tracing the trail along the true path.
            let time = (flight - f32::from(u16::try_from(bead).unwrap_or(0)) * 0.03).max(0.0);
            let position = impact
                + Vec3::new(
                    angle.cos() * out * time * 0.55,
                    up * time - 4.4 * time * time * 0.5,
                    angle.sin() * out * time * 0.55,
                );
            let fade = 1.0 - bead as f32 / BEADS as f32;
            let size = (0.016 + heat * 0.014) * (0.5 + 0.5 * fade) * presence;
            add_sphere(
                mesh,
                position,
                size.max(0.000_5),
                alpha(
                    mix_color(flame.core, flame.body_high, (1.0 - fade) * 0.8),
                    0.85 * presence * fade,
                ),
                3,
                5,
            );
        }
    }
}

/// The crack web: jagged fracture lines spreading down the glass from the
/// rim once the blast hits, and staying. Fixed population; segments grow in
/// quick succession so the web visibly propagates.
fn add_glass_cracks(mesh: &mut Mesh, bench_top: f32, progress: f32, damage: f32, seed: u64) {
    const CRACKS: u32 = 9;
    const SEGMENTS: u32 = 4;
    const WALL_RADIUS: f32 = 0.945;
    for crack in 0..CRACKS {
        let angle = seeded_unit(seed, crack, 521) * std::f32::consts::TAU;
        let reach = (0.14 + seeded_unit(seed, crack, 522) * 0.12) * damage.min(1.8);
        // A connected polyline walking down the wall with angular drift:
        // each grown segment is a thin cylinder from the previous node.
        let mut node = Vec3::new(
            angle.cos() * WALL_RADIUS,
            bench_top + 1.74,
            angle.sin() * WALL_RADIUS,
        );
        let mut drift = angle;
        for segment in 0..SEGMENTS {
            let index = crack * SEGMENTS + segment;
            let grow = smooth01((progress - CONTACT - 0.006 * (segment + 1) as f32) / 0.018);
            let length = (reach * (0.8 + seeded_unit(seed, index, 523) * 0.5) * grow).max(0.000_5);
            let lean = (seeded_unit(seed, index, 524) - 0.5) * 0.35;
            drift += lean;
            let next = Vec3::new(
                drift.cos() * WALL_RADIUS,
                node.y - length,
                drift.sin() * WALL_RADIUS,
            );
            add_cylinder(mesh, node, next, 0.006, [0.92, 0.97, 1.0, 0.74]);
            node = next;
        }
    }
}

/// The crown of water hurled upward at contact: tall spikes and a ring of
/// ballistic droplets, all scaled by the blast.
fn add_blast_crown(
    mesh: &mut Mesh,
    impact: Vec3,
    age: f32,
    presence: f32,
    spray_velocity: f32,
    colour: [f32; 4],
    seed: u64,
) {
    const SPIKES: u32 = 14;
    const DROPLETS: u32 = 28;
    for spike in 0..SPIKES {
        let angle = std::f32::consts::TAU * spike as f32 / SPIKES as f32
            + seeded_unit(seed, spike, 501) * 0.4;
        // The harder the metal goes, the wider the crown throws.
        let radial = (0.12 + seeded_unit(seed, spike, 502) * 0.16) * (0.65 + spray_velocity * 0.35);
        let height = (0.5 + seeded_unit(seed, spike, 503) * 0.5)
            * spray_velocity
            * presence
            * (std::f32::consts::PI * age.min(0.9)).sin();
        let base = impact + Vec3::new(angle.cos() * radial, 0.0, angle.sin() * radial);
        let girth = 0.022 * (0.80 + spray_velocity * 0.20);
        add_shard(
            mesh,
            base + Vec3::Y * (height * 0.5),
            Vec3::new(girth, height.max(0.001) * 0.5, girth),
            Quat::from_rotation_y(angle) * Quat::from_rotation_z(0.16),
            alpha(colour, 0.62 * presence),
            seed.wrapping_add(u64::from(spike)),
        );
    }
    for droplet in 0..DROPLETS {
        let angle = seeded_unit(seed, droplet, 505) * std::f32::consts::TAU;
        let velocity = (0.8 + seeded_unit(seed, droplet, 506) * 0.9) * spray_velocity;
        let time = age * 1.2;
        let position = impact
            + Vec3::new(
                angle.cos() * velocity * time * 0.7,
                velocity * time - 4.4 * time * time * 0.5,
                angle.sin() * velocity * time * 0.7,
            );
        let size = (0.014 + seeded_unit(seed, droplet, 507) * 0.012) * presence * (1.0 - age * 0.5);
        add_sphere(
            mesh,
            position,
            size.max(0.000_5),
            alpha(colour, 0.7 * presence),
            3,
            5,
        );
    }
}

// A linear choreography list: splitting it would only scatter the reading
// order of one continuous scene.
#[allow(clippy::too_many_lines)]
pub(super) fn add_explosive_metal_water_assembly(
    meshes: &mut SceneMeshes,
    plan: &ScenePlan,
    layout: SceneLayout,
    progress: f32,
    ordinal: u16,
    ordinal_progress: f32,
) {
    let explosive = plan
        .explosive_metal_water
        .as_ref()
        .expect("validated high-energy metal/water assembly has material bindings");
    let seed = plan_seed(plan);
    let progress = progress.clamp(0.0, 1.0);
    let phase = progress * 12.0;
    let profile = blast_profile(explosive.variant);
    let colour = |clip_colour| {
        explosive_metal_water_track_colour(clip_colour, explosive, ordinal, ordinal_progress)
    };
    add_explosion_beaker(
        &mut meshes.glass,
        layout.bench_top,
        progress,
        profile,
        seed.rotate_left(3),
    );
    // The blast throws a share of the water clean out of the basin: the
    // level drops through the churn and stays down, leaving a high-water
    // mark ring where it stood. The impact stays at the original level —
    // that is where the lump met the water.
    let original_surface = Vec3::new(0.0, layout.bench_top + BASIN_LEVEL, 0.0);
    let drop = level_drop(progress, profile.liquid_ejection);
    let surface = original_surface - Vec3::Y * drop;
    let impact = original_surface;
    let presence = blast(progress);
    let age = blast_age(progress);
    let churning = churn(progress);
    let steaming = steam(progress);
    // The basin: violent churn through the blast, settling to hydroxide calm.
    add_contained_liquid(
        &mut meshes.translucent,
        surface,
        layout.bench_top + 0.09,
        0.88,
        colour(ClipColour::Water),
        (0.2 + churning * 0.8) * profile.spray_velocity.min(1.35),
        phase,
        seed,
    );
    add_foam_ring(
        &mut meshes.translucent,
        surface,
        0.88,
        0.3 + churning * 0.6,
        phase,
        seed,
    );
    // High-water mark: a faint film ring at the original level, revealed as
    // the water beneath it recedes.
    add_ring(
        &mut meshes.translucent,
        original_surface + Vec3::Y * 0.004,
        0.925,
        0.008,
        [0.88, 0.93, 0.96, (0.50 * smooth01(drop / 0.05)).max(0.02)],
    );
    add_falling_lump(
        &mut meshes.opaque,
        surface,
        progress,
        profile.camera_kick,
        colour(ClipColour::ReactiveMetal),
        seed,
    );
    add_contact_dimple(
        &mut meshes.translucent,
        impact,
        progress,
        profile.pressure_radius,
    );
    add_contact_flash(
        &mut meshes.emissive,
        impact,
        progress,
        profile.flash_energy,
        seed.rotate_left(4),
    );
    add_fireball(
        meshes,
        impact + Vec3::Y * 0.06,
        presence,
        age,
        profile.fireball_reach,
        phase,
        seed,
    );
    add_shock_rings(
        &mut meshes.translucent,
        impact,
        layout.bench_top,
        age,
        presence,
        profile.pressure_radius,
    );
    add_bulk_liquid_ejection(
        &mut meshes.translucent,
        impact,
        progress,
        profile,
        colour(ClipColour::Water),
        seed.rotate_left(7),
    );
    add_glass_chips(
        &mut meshes.glass,
        layout.bench_top,
        progress,
        profile,
        seed.rotate_left(5),
    );
    add_blast_crown(
        &mut meshes.translucent,
        impact,
        age,
        presence.max(churning * 0.4),
        profile.spray_velocity,
        colour(ClipColour::WaterHighlight),
        seed.rotate_left(9),
    );
    add_ignition_sparks(
        &mut meshes.emissive,
        impact + Vec3::Y * 0.10,
        presence * profile.flash_energy,
        phase,
        seed.rotate_left(13),
    );
    add_molten_ejecta(
        &mut meshes.emissive,
        impact + Vec3::Y * 0.05,
        progress,
        profile.debris_velocity,
        seed.rotate_left(15),
    );
    add_glass_cracks(
        &mut meshes.glass,
        layout.bench_top,
        progress,
        profile.vessel_damage,
        seed.rotate_left(19),
    );
    add_bench_aftermath(
        meshes,
        impact,
        layout.bench_top,
        progress,
        profile,
        colour(ClipColour::Water),
        seed.rotate_left(23),
    );
    // The flame collapses into one massive rising steam column while surface
    // fizz persists below it.
    add_blast_steam_column(
        &mut meshes.translucent,
        surface,
        steaming * profile.steam_yield,
        colour(ClipColour::Vapour),
        phase,
        seed.rotate_left(17),
    );
    alkali_water::add_alkali_fizz(
        &mut meshes.translucent,
        surface,
        churning * 0.8,
        phase,
        seed.rotate_left(21),
    );
    add_glass_condensation(
        &mut meshes.translucent,
        surface,
        0.94,
        layout.bench_top + 1.78,
        (steaming * 1.2).min(1.0),
        seed.rotate_left(25),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variants_author_independent_monotonic_blast_controls() {
        let rubidium = blast_profile(ExplosiveMetalWaterVariant::Rubidium);
        let caesium = blast_profile(ExplosiveMetalWaterVariant::Caesium);
        let francium = blast_profile(ExplosiveMetalWaterVariant::Francium);
        for extract in [
            |profile: BlastProfile| profile.camera_kick,
            |profile: BlastProfile| profile.flash_energy,
            |profile: BlastProfile| profile.fireball_reach,
            |profile: BlastProfile| profile.pressure_radius,
            |profile: BlastProfile| profile.spray_velocity,
            |profile: BlastProfile| profile.liquid_ejection,
            |profile: BlastProfile| profile.debris_velocity,
            |profile: BlastProfile| profile.steam_yield,
            |profile: BlastProfile| profile.vessel_damage,
            |profile: BlastProfile| profile.bench_damage,
            |profile: BlastProfile| profile.environment_response,
        ] {
            assert!(extract(rubidium) < extract(caesium));
            assert!(extract(caesium) < extract(francium));
        }
    }

    #[test]
    fn contact_flash_is_short_and_anticipation_precedes_it() {
        assert!(anticipation(CONTACT - 0.025) > 0.5);
        assert!(contact_flash(CONTACT - 0.001) <= f32::EPSILON);
        assert!(contact_flash(CONTACT + 0.010) > 0.9);
        assert!(contact_flash(CONTACT + 0.045) <= f32::EPSILON);
        assert!(fracture_age(CONTACT - 0.001) <= f32::EPSILON);
        assert!((fracture_age(CONTACT + 0.060) - 1.0).abs() <= f32::EPSILON);
    }

    #[test]
    fn liquid_loss_is_persistent_and_tracks_authored_ejection() {
        let rubidium = blast_profile(ExplosiveMetalWaterVariant::Rubidium);
        let francium = blast_profile(ExplosiveMetalWaterVariant::Francium);
        assert!(level_drop(CONTACT - 0.001, francium.liquid_ejection) <= f32::EPSILON);
        assert!(
            level_drop(1.0, rubidium.liquid_ejection) < level_drop(1.0, francium.liquid_ejection)
        );
        assert!(level_drop(1.0, francium.liquid_ejection) > 0.45);
    }

    #[test]
    fn debris_falls_hits_the_bench_and_converges_to_rest() {
        let origin = Vec3::new(0.0, 1.2, 0.0);
        let velocity = Vec3::new(1.1, 1.6, -0.4);
        let floor_y = -0.72;
        let before = debris_motion(origin, velocity, CONTACT - 0.01, floor_y, 5.0, 0.06);
        let airborne = debris_motion(origin, velocity, CONTACT + 0.05, floor_y, 5.0, 0.06);
        let settled = debris_motion(origin, velocity, 1.0, floor_y, 5.0, 0.06);
        let later = debris_motion(origin, velocity, 1.2, floor_y, 5.0, 0.06);

        assert_eq!(before.offset, Vec3::ZERO);
        assert!(origin.y + airborne.offset.y > floor_y);
        assert!((origin.y + settled.offset.y - floor_y).abs() < 0.001);
        assert!(settled.settled > 0.99);
        assert!((later.offset - settled.offset).length() < 0.001);
    }

    #[test]
    fn stronger_variants_detach_a_few_more_contiguous_wall_sections() {
        let first_gap = std::f32::consts::TAU - 0.08;
        let second_gap = 2.1;
        let active = |damage| {
            glass_wall_chunks(first_gap, second_gap, damage)
                .into_iter()
                .filter(|chunk| chunk.is_active(damage))
                .count()
        };

        assert_eq!(
            active(blast_profile(ExplosiveMetalWaterVariant::Rubidium).vessel_damage),
            1
        );
        assert_eq!(
            active(blast_profile(ExplosiveMetalWaterVariant::Caesium).vessel_damage),
            3
        );
        assert_eq!(
            active(blast_profile(ExplosiveMetalWaterVariant::Francium).vessel_damage),
            5
        );
        assert!(angular_distance(0.03, std::f32::consts::TAU - 0.03) < 0.07);
    }

    #[test]
    fn glass_chips_are_mostly_tiny_and_low_velocity() {
        const COUNT: u32 = 56;
        let debris_velocity = blast_profile(ExplosiveMetalWaterVariant::Francium).debris_velocity;
        let profiles = (0..COUNT)
            .map(|chip| glass_chip_profile(0x51a7_d00d, chip, debris_velocity))
            .collect::<Vec<_>>();
        let tiny = profiles
            .iter()
            .filter(|profile| profile.radius < 0.04)
            .count();
        let near = profiles
            .iter()
            .filter(|profile| profile.speed < debris_velocity * 0.42)
            .count();

        assert!(tiny >= 42);
        assert!(near >= 38);
        assert!(profiles.iter().any(|profile| profile.radius > 0.06));
        assert!(
            profiles
                .iter()
                .any(|profile| profile.speed > debris_velocity * 0.58)
        );
    }
}
