//! Procedural phase-aware synthesis scene: a sealed glass reaction chamber
//! on a steel base plate — one typed solid plus one typed gas, or two typed
//! gases, combining into a single gaseous product. Gas bodies are rendered
//! as translucent volumetric concentration cues, never smoke or molecule
//! sprites; inlet ports carry a colour band naming the gas they feed.
//!
//! Everything is a deterministic function of (plan, progress): fixed entity
//! populations whose sizes animate, never appearing or vanishing.

#![allow(clippy::wildcard_imports, clippy::cast_precision_loss)]

use super::*;

/// Chamber footprint on the base plate.
const CHAMBER_RADIUS: f32 = 0.58;
const CHAMBER_HEIGHT: f32 = 1.16;
const PLATE_RADIUS: f32 = 0.76;
const PLATE_HEIGHT: f32 = 0.07;
const STEEL: [f32; 4] = [0.46, 0.48, 0.52, 1.0];
const GLASS: [f32; 4] = [0.62, 0.84, 0.94, 0.20];

/// Fraction of the reactants converted into the gaseous product.
fn conversion(progress: f32) -> f32 {
    smooth01((progress - 0.18) / 0.60)
}

/// The reacting window: front glow, spark spit, agitated gas motion.
fn activity(progress: f32) -> f32 {
    smooth01((progress - 0.14) / 0.10) * (1.0 - smooth01((progress - 0.84) / 0.12))
}

/// How far the two gas charges have drifted into one another (gas–gas).
fn approach(progress: f32) -> f32 {
    smooth01((progress - 0.05) / 0.30)
}

fn bound_rgba(
    bound: &chem_presentation::BoundVisualColour,
    opacity: f32,
    ordinal: u16,
    ordinal_progress: f32,
) -> [f32; 4] {
    let (base, target, amount) = bound_colour_endpoints(bound, opacity, ordinal, ordinal_progress);
    mix_color(base, target, amount)
}

/// The sealed chamber itself: steel plate, glass shell with a flat lid,
/// collar ring, top relief valve, and one inlet port per gaseous reactant.
/// A coloured band on each inlet names the gas it admits.
fn add_reaction_chamber(
    meshes: &mut SceneMeshes,
    bench_top: f32,
    inlet_colours: &[[f32; 4]],
) {
    let plate_top = bench_top + PLATE_HEIGHT;
    add_cylinder(
        &mut meshes.metallic,
        Vec3::new(0.0, bench_top, 0.0),
        Vec3::new(0.0, plate_top, 0.0),
        PLATE_RADIUS,
        STEEL,
    );
    add_disc(
        &mut meshes.metallic,
        Vec3::new(0.0, plate_top, 0.0),
        PLATE_RADIUS,
        STEEL,
    );
    let lid_y = plate_top + CHAMBER_HEIGHT;
    add_cylinder_wall(
        &mut meshes.glass,
        Vec3::new(0.0, plate_top, 0.0),
        Vec3::new(0.0, lid_y, 0.0),
        CHAMBER_RADIUS,
        GLASS,
    );
    add_disc(&mut meshes.glass, Vec3::new(0.0, lid_y, 0.0), CHAMBER_RADIUS, GLASS);
    // Collar ring seating the glass onto the plate, and a lid ring above.
    add_cylinder(
        &mut meshes.metallic,
        Vec3::new(0.0, plate_top, 0.0),
        Vec3::new(0.0, plate_top + 0.06, 0.0),
        CHAMBER_RADIUS + 0.025,
        STEEL,
    );
    add_cylinder(
        &mut meshes.metallic,
        Vec3::new(0.0, lid_y - 0.015, 0.0),
        Vec3::new(0.0, lid_y + 0.035, 0.0),
        CHAMBER_RADIUS + 0.025,
        STEEL,
    );
    add_disc(
        &mut meshes.metallic,
        Vec3::new(0.0, lid_y + 0.035, 0.0),
        CHAMBER_RADIUS + 0.025,
        STEEL,
    );
    // Relief valve on the lid.
    add_cylinder(
        &mut meshes.metallic,
        Vec3::new(0.0, lid_y, 0.0),
        Vec3::new(0.0, lid_y + 0.16, 0.0),
        0.055,
        STEEL,
    );
    add_sphere(
        &mut meshes.metallic,
        Vec3::new(0.0, lid_y + 0.19, 0.0),
        0.065,
        STEEL,
        5,
        8,
    );
    // One inlet port per gaseous reactant, entering low on the shell where
    // the feed pipework would sit. The band colour names the gas.
    for (index, band) in inlet_colours.iter().enumerate() {
        let side = if index == 0 { -1.0 } else { 1.0 };
        let port_y = plate_top + 0.24;
        let outer = Vec3::new(side * (CHAMBER_RADIUS + 0.20), port_y, 0.16);
        let inner = Vec3::new(side * (CHAMBER_RADIUS - 0.02), port_y, 0.16);
        add_cylinder(&mut meshes.metallic, outer, inner, 0.045, STEEL);
        add_sphere(&mut meshes.metallic, outer, 0.062, STEEL, 5, 8);
        add_cylinder(
            &mut meshes.opaque,
            outer + (inner - outer) * 0.30,
            outer + (inner - outer) * 0.52,
            0.052,
            [band[0], band[1], band[2], 1.0],
        );
    }
}

/// The glowing reaction zone. Gas–gas: a shimmering curtain of emissive
/// beads along the mixing interface. Solid–gas: a ring of embers hugging
/// the solid charge, spitting sparks while the surface reacts.
fn add_reaction_front(
    meshes: &mut SceneMeshes,
    floor_y: f32,
    variant: PhaseSynthesisVariant,
    presence: f32,
    phase: f32,
    seed: u64,
) {
    const BEADS: u32 = 14;
    const FRONT_COLOUR: [f32; 4] = [1.0, 0.30, 0.05, 0.55];
    for bead in 0..BEADS {
        let unit = bead as f32 / (BEADS - 1) as f32;
        let wobble = (phase * (1.6 + seeded_unit(seed, bead, 521) * 1.2)
            + seeded_unit(seed, bead, 522) * 6.0)
            .sin();
        let position = match variant {
            PhaseSynthesisVariant::GasGas => Vec3::new(
                wobble * 0.05,
                floor_y + 0.16 + unit * 0.74,
                (seeded_unit(seed, bead, 523) - 0.5) * 0.62,
            ),
            PhaseSynthesisVariant::SolidGas => {
                let angle = unit * std::f32::consts::TAU + phase * 0.22;
                Vec3::new(
                    angle.cos() * 0.26,
                    floor_y + 0.05 + wobble.abs() * 0.02,
                    angle.sin() * 0.26,
                )
            }
        };
        let pulse = 0.6 + 0.4 * (phase * 2.4 + seeded_unit(seed, bead, 524) * 6.0).sin();
        add_sphere(
            &mut meshes.emissive,
            position,
            (0.020 * presence * pulse).max(0.000_5),
            alpha(FRONT_COLOUR, FRONT_COLOUR[3] * presence * pulse),
            3,
            5,
        );
    }
    if variant == PhaseSynthesisVariant::SolidGas {
        add_ignition_sparks(
            &mut meshes.emissive,
            Vec3::new(0.0, floor_y + 0.10, 0.0),
            presence * 0.8,
            phase,
            seed.rotate_left(9),
        );
    }
}

pub(super) fn add_phase_synthesis_assembly(
    meshes: &mut SceneMeshes,
    plan: &ScenePlan,
    layout: SceneLayout,
    progress: f32,
    ordinal: u16,
    ordinal_progress: f32,
) {
    let synthesis = plan
        .phase_synthesis
        .as_ref()
        .expect("validated phase-synthesis assembly has material bindings");
    let seed = plan_seed(plan);
    let progress = progress.clamp(0.0, 1.0);
    let phase = progress * 9.0;
    let converted = conversion(progress);
    let active = activity(progress);
    let floor_y = layout.bench_top + PLATE_HEIGHT;
    let gas_centre_y = floor_y + CHAMBER_HEIGHT * 0.52;
    let reactant_a = bound_rgba(&synthesis.reactant_a, 0.32, ordinal, ordinal_progress);
    let reactant_b = bound_rgba(&synthesis.reactant_b, 0.32, ordinal, ordinal_progress);
    let product = bound_rgba(&synthesis.product, 0.34, ordinal, ordinal_progress);
    let inlet_colours: &[[f32; 4]] = match synthesis.variant {
        PhaseSynthesisVariant::SolidGas => &[reactant_b],
        PhaseSynthesisVariant::GasGas => &[reactant_a, reactant_b],
    };
    add_reaction_chamber(meshes, layout.bench_top, inlet_colours);
    let turbulence = 0.16 + active * 0.36;
    match synthesis.variant {
        PhaseSynthesisVariant::SolidGas => {
            // The solid charge sits on the plate and is eaten by the gas:
            // matte faceted grains that shrink toward a residue.
            let solid = {
                let bound = bound_rgba(&synthesis.reactant_a, 1.0, ordinal, ordinal_progress);
                [bound[0] * 0.62, bound[1] * 0.62, bound[2] * 0.62, 1.0]
            };
            synthesis::add_powder_heap(
                &mut meshes.opaque,
                Vec3::new(0.0, floor_y, 0.0),
                0.30,
                0.060,
                (1.0 - converted * 0.78).max(0.06),
                [solid, solid],
                None,
                18,
                seed.rotate_left(3),
            );
            let reactant_density = (1.0 - converted) * (0.55 + active * 0.25);
            if reactant_density > 0.001 {
                add_gas_density_field(
                    &mut meshes.gas,
                    Vec3::new(0.0, gas_centre_y, 0.0),
                    Vec3::new(0.50, 0.52, 0.50),
                    alpha(reactant_b, reactant_b[3] * (1.0 - converted).max(0.05)),
                    seed.rotate_left(11),
                    phase,
                    reactant_density,
                    GasFlowControls::contained(
                        0.45 + active * 0.30,
                        turbulence,
                        active * 0.5,
                        0.0,
                        seed.rotate_left(11),
                    ),
                );
            }
        }
        PhaseSynthesisVariant::GasGas => {
            // The two charges drift from their inlet sides into the middle
            // of the chamber, thinning as the product takes over.
            let drift = 0.26 * (1.0 - approach(progress) * 0.85);
            let reactant_density = (1.0 - converted) * (0.55 + active * 0.25);
            for (index, (colour, side)) in
                [(reactant_a, -1.0_f32), (reactant_b, 1.0)].into_iter().enumerate()
            {
                if reactant_density <= 0.001 {
                    continue;
                }
                let channel = 11 + 8 * index as u32;
                add_gas_density_field(
                    &mut meshes.gas,
                    Vec3::new(side * drift, gas_centre_y, 0.0),
                    Vec3::new(0.38, 0.48, 0.44),
                    alpha(colour, colour[3] * (1.0 - converted).max(0.05)),
                    seed.rotate_left(channel),
                    phase,
                    reactant_density,
                    GasFlowControls::contained(
                        0.45 + active * 0.30,
                        turbulence,
                        active * 0.5,
                        0.0,
                        seed.rotate_left(channel),
                    ),
                );
            }
        }
    }
    // The gaseous product builds from the reaction zone outward until it
    // fills the chamber.
    let product_density = converted * 0.85;
    if product_density > 0.001 {
        add_gas_density_field(
            &mut meshes.gas,
            Vec3::new(0.0, gas_centre_y, 0.0),
            Vec3::new(0.50, 0.54, 0.50),
            alpha(product, product[3] * converted.max(0.05)),
            seed.rotate_left(19),
            phase,
            product_density,
            GasFlowControls::contained(
                0.40 + active * 0.25,
                turbulence * 0.8,
                active * 0.35,
                0.0,
                seed.rotate_left(19),
            ),
        );
    }
    if synthesis.show_reaction_front {
        add_reaction_front(
            meshes,
            floor_y,
            synthesis.variant,
            active,
            phase,
            seed.rotate_left(15),
        );
    }
}
