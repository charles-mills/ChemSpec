//! Procedural heavy-alkali-metal-and-water scene: a lump dropped into the
//! basin detonates on contact — fireball, shock ring, ejected glass chips,
//! a crown of thrown water, then a towering steam column that decays into
//! a hydroxide-tinted calm.
//!
//! One implementation serves every metal: the variant sets an intensity
//! knob instead of shipping three separate baked simulations.
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

/// How hard each metal goes.
fn variant_intensity(variant: ExplosiveMetalWaterVariant) -> f32 {
    match variant {
        ExplosiveMetalWaterVariant::Rubidium => 1.0,
        ExplosiveMetalWaterVariant::Caesium => 1.35,
        ExplosiveMetalWaterVariant::Francium => 1.75,
    }
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
    smooth01((progress - CONTACT) / 0.03)
        * (1.0 - smooth01((progress - CHURN_END + 0.1) / 0.25))
}

/// Steam rises hard after the blast and thins out late.
fn steam(progress: f32) -> f32 {
    smooth01((progress - CONTACT - 0.01) / 0.05) * (1.0 - smooth01((progress - 0.78) / 0.2))
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

/// The fireball: layered emissive lobes and a translucent envelope that
/// expand from the impact point and burn out.
fn add_fireball(
    meshes: &mut SceneMeshes,
    impact: Vec3,
    presence: f32,
    age: f32,
    intensity: f32,
    phase: f32,
    seed: u64,
) {
    const CORES: u32 = 9;
    const SHELL: u32 = 7;
    let flame = flame_colours(FlamePalette::Natural);
    let reach = (0.14 + age * 0.55) * intensity;
    for core in 0..CORES {
        let direction = Vec3::new(
            seeded_unit(seed, core, 481) * 2.0 - 1.0,
            seeded_unit(seed, core, 482) * 1.4,
            seeded_unit(seed, core, 483) * 2.0 - 1.0,
        )
        .normalize_or_zero();
        let throw = seeded_unit(seed, core, 484);
        let position = impact + direction * reach * (0.25 + throw * 0.75);
        let size = (0.10 + seeded_unit(seed, core, 485) * 0.12)
            * intensity
            * presence
            * (1.0 - age * 0.45);
        let colour = mix_color(flame.core, flame.body_high, throw * 0.7);
        add_sphere(
            &mut meshes.emissive,
            position + Vec3::Y * (age * 0.22 * intensity),
            size.max(0.000_5),
            alpha(colour, (presence * (1.0 - age * 0.6)).max(0.02)),
            4,
            6,
        );
    }
    for lobe in 0..SHELL {
        let angle = seeded_unit(seed, lobe, 486) * std::f32::consts::TAU + phase * 0.4;
        let radial = reach * (0.6 + seeded_unit(seed, lobe, 487) * 0.5);
        let position = impact
            + Vec3::new(
                angle.cos() * radial,
                (0.08 + seeded_unit(seed, lobe, 488) * 0.30) * intensity * age,
                angle.sin() * radial,
            );
        let size = (0.12 + age * 0.16) * intensity * presence;
        add_sphere(
            &mut meshes.translucent,
            position,
            size.max(0.000_5),
            alpha(flame.body_low, (0.5 * presence * (1.0 - age * 0.7)).max(0.02)),
            4,
            7,
        );
    }
}

/// A thin expanding shock ring racing across the water surface.
fn add_shock_ring(mesh: &mut Mesh, impact: Vec3, age: f32, presence: f32, intensity: f32) {
    let radius = (0.10 + age * 0.85 * intensity).min(0.87);
    add_ring(
        mesh,
        impact + Vec3::Y * 0.015,
        radius,
        0.012 * (1.0 - age * 0.5),
        [0.94, 0.97, 1.0, (0.6 * presence * (1.0 - age)).max(0.02)],
    );
}

/// Glass chips blasted off the rim, flying on ballistic arcs.
fn add_glass_chips(
    mesh: &mut Mesh,
    impact: Vec3,
    bench_top: f32,
    age: f32,
    presence: f32,
    intensity: f32,
    seed: u64,
) {
    const CHIPS: u32 = 12;
    for chip in 0..CHIPS {
        let angle = seeded_unit(seed, chip, 491) * std::f32::consts::TAU;
        let launch = Vec3::new(
            angle.cos() * (0.5 + seeded_unit(seed, chip, 492) * 0.7),
            1.1 + seeded_unit(seed, chip, 493) * 0.9,
            angle.sin() * (0.5 + seeded_unit(seed, chip, 494) * 0.7),
        ) * intensity;
        let start = Vec3::new(
            angle.cos() * 0.9,
            bench_top + 1.70,
            angle.sin() * 0.9,
        );
        let time = age * 1.1;
        let position = start + launch * time - Vec3::Y * (4.4 * time * time * 0.5);
        let spin = Quat::from_rotation_y(time * 9.0 + seed_phase(seed, 495 + chip))
            * Quat::from_rotation_z(time * 7.0);
        let size = (0.020 + seeded_unit(seed, chip, 496) * 0.020) * presence;
        add_shard(
            mesh,
            position.max(Vec3::new(f32::MIN, bench_top + 0.02, f32::MIN)),
            Vec3::new(size, size * 0.5, size * 0.8).max(Vec3::splat(0.000_5)),
            spin,
            [0.62, 0.84, 0.94, 0.55],
            seed.wrapping_add(u64::from(chip)),
        );
    }
    let _ = impact;
}

/// The crown of water hurled upward at contact: tall spikes and a ring of
/// ballistic droplets, all scaled by the blast.
fn add_blast_crown(
    mesh: &mut Mesh,
    impact: Vec3,
    age: f32,
    presence: f32,
    intensity: f32,
    colour: [f32; 4],
    seed: u64,
) {
    const SPIKES: u32 = 11;
    const DROPLETS: u32 = 16;
    for spike in 0..SPIKES {
        let angle = std::f32::consts::TAU * spike as f32 / SPIKES as f32
            + seeded_unit(seed, spike, 501) * 0.4;
        let radial = 0.12 + seeded_unit(seed, spike, 502) * 0.16;
        let height = (0.5 + seeded_unit(seed, spike, 503) * 0.5)
            * intensity
            * presence
            * (std::f32::consts::PI * age.min(0.9)).sin();
        let base = impact + Vec3::new(angle.cos() * radial, 0.0, angle.sin() * radial);
        add_shard(
            mesh,
            base + Vec3::Y * (height * 0.5),
            Vec3::new(0.022, height.max(0.001) * 0.5, 0.022),
            Quat::from_rotation_y(angle) * Quat::from_rotation_z(0.16),
            alpha(colour, 0.62),
            seed.wrapping_add(u64::from(spike)),
        );
    }
    for droplet in 0..DROPLETS {
        let angle = seeded_unit(seed, droplet, 505) * std::f32::consts::TAU;
        let velocity = (0.8 + seeded_unit(seed, droplet, 506) * 0.9) * intensity;
        let time = age * 1.2;
        let position = impact
            + Vec3::new(
                angle.cos() * velocity * time * 0.7,
                velocity * time - 4.4 * time * time * 0.5,
                angle.sin() * velocity * time * 0.7,
            );
        let size = (0.014 + seeded_unit(seed, droplet, 507) * 0.012)
            * presence
            * (1.0 - age * 0.5);
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
    let intensity = variant_intensity(explosive.variant);
    let colour = |clip_colour| {
        explosive_metal_water_track_colour(clip_colour, explosive, ordinal, ordinal_progress)
    };
    add_assembly_beaker(&mut meshes.glass, layout.bench_top, Vec3::ZERO);
    let surface = Vec3::new(0.0, layout.bench_top + BASIN_LEVEL, 0.0);
    let impact = surface;
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
        (0.2 + churning * 0.8) * intensity.min(1.3),
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
    add_falling_lump(
        &mut meshes.opaque,
        surface,
        progress,
        intensity,
        colour(ClipColour::ReactiveMetal),
        seed,
    );
    add_fireball(meshes, impact + Vec3::Y * 0.06, presence, age, intensity, phase, seed);
    add_shock_ring(&mut meshes.translucent, impact, age, presence, intensity);
    add_glass_chips(
        &mut meshes.glass,
        impact,
        layout.bench_top,
        age,
        presence,
        intensity,
        seed.rotate_left(5),
    );
    add_blast_crown(
        &mut meshes.translucent,
        impact,
        age,
        presence.max(churning * 0.4),
        intensity,
        colour(ClipColour::WaterHighlight),
        seed.rotate_left(9),
    );
    add_ignition_sparks(
        &mut meshes.emissive,
        impact + Vec3::Y * 0.10,
        presence * intensity,
        phase,
        seed.rotate_left(13),
    );
    // The steam column: a strong rising plume plus fizz around the surface.
    add_rising_plume(
        &mut meshes.translucent,
        surface,
        steaming * (0.9 + intensity * 0.4),
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
