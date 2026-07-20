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
const STEEL: [f32; 4] = [0.56, 0.58, 0.62, 1.0];
/// Matte powder-coated housing: large horizontal metal surfaces bloom badly
/// under the key light, so the plate and lid stay dark and diffuse.
const HOUSING: [f32; 4] = [0.235, 0.255, 0.285, 1.0];
const GLASS: [f32; 4] = [0.62, 0.84, 0.94, 0.10];

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

/// A colourless gas is close to invisible in a real chamber. Concentration
/// cues keep a faint educational presence, while visibly coloured gases
/// (chlorine, bromine vapour, nitrogen dioxide) carry full weight.
fn colour_visibility(colour: [f32; 4]) -> f32 {
    let chroma = colour[0].max(colour[1]).max(colour[2]) - colour[0].min(colour[1]).min(colour[2]);
    0.45 + 0.55 * smooth01(chroma / 0.22)
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

/// The sealed chamber: a chamfered housing plate, a tall glass shell with
/// a glass lid, bright rim lips top and bottom like the house beaker, thin
/// steel clamp bands, a relief valve with a handwheel, and one flanged
/// inlet port per gaseous reactant with a colour band naming its gas.
#[allow(clippy::too_many_lines)]
fn add_reaction_chamber(
    meshes: &mut SceneMeshes,
    bench_top: f32,
    inlet_colours: &[[f32; 4]],
) {
    const RIM_GLASS: [f32; 4] = [0.80, 0.93, 1.0, 0.42];
    let plate_top = bench_top + PLATE_HEIGHT;
    add_cylinder(
        &mut meshes.opaque,
        Vec3::new(0.0, bench_top, 0.0),
        Vec3::new(0.0, plate_top, 0.0),
        PLATE_RADIUS,
        HOUSING,
    );
    add_disc(
        &mut meshes.opaque,
        Vec3::new(0.0, plate_top, 0.0),
        PLATE_RADIUS,
        HOUSING,
    );
    // A polished chamfer edge lifts the plate off the bench visually.
    add_ring(
        &mut meshes.metallic,
        Vec3::new(0.0, plate_top, 0.0),
        PLATE_RADIUS - 0.015,
        0.014,
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
    // Bright glass lips where the shell meets the lid and the plate: the
    // same rim-highlight language as the shared laboratory beaker.
    add_ring(
        &mut meshes.glass,
        Vec3::new(0.0, lid_y, 0.0),
        CHAMBER_RADIUS,
        0.020,
        RIM_GLASS,
    );
    add_ring(
        &mut meshes.glass,
        Vec3::new(0.0, plate_top + 0.012, 0.0),
        CHAMBER_RADIUS,
        0.014,
        RIM_GLASS,
    );
    // Thin steel clamp bands rather than dark slabs.
    add_ring(
        &mut meshes.metallic,
        Vec3::new(0.0, plate_top + 0.045, 0.0),
        CHAMBER_RADIUS + 0.006,
        0.011,
        STEEL,
    );
    add_ring(
        &mut meshes.metallic,
        Vec3::new(0.0, lid_y - 0.045, 0.0),
        CHAMBER_RADIUS + 0.006,
        0.011,
        STEEL,
    );
    // Relief valve: stem, handwheel, and cap finial.
    add_cylinder(
        &mut meshes.metallic,
        Vec3::new(0.0, lid_y, 0.0),
        Vec3::new(0.0, lid_y + 0.17, 0.0),
        0.042,
        STEEL,
    );
    add_ring(
        &mut meshes.metallic,
        Vec3::new(0.0, lid_y + 0.115, 0.0),
        0.085,
        0.017,
        STEEL,
    );
    add_sphere(
        &mut meshes.metallic,
        Vec3::new(0.0, lid_y + 0.185, 0.0),
        0.038,
        STEEL,
        5,
        8,
    );
    // One inlet port per gaseous reactant: barrel, wall flange, end cap,
    // and the colour band naming the gas it admits.
    for (index, band) in inlet_colours.iter().enumerate() {
        let side = if index == 0 { -1.0 } else { 1.0 };
        let port_y = plate_top + 0.26;
        let outer = Vec3::new(side * (CHAMBER_RADIUS + 0.21), port_y, 0.16);
        let inner = Vec3::new(side * (CHAMBER_RADIUS - 0.02), port_y, 0.16);
        let axis = (inner - outer).normalize_or_zero();
        add_cylinder(&mut meshes.metallic, outer, inner, 0.042, STEEL);
        add_sphere(&mut meshes.metallic, outer, 0.055, STEEL, 5, 8);
        // Flange washer where the barrel meets the glass.
        add_cylinder(
            &mut meshes.metallic,
            inner - axis * 0.045,
            inner - axis * 0.015,
            0.062,
            STEEL,
        );
        add_cylinder(
            &mut meshes.opaque,
            outer + (inner - outer) * 0.28,
            outer + (inner - outer) * 0.50,
            0.048,
            [band[0], band[1], band[2], 1.0],
        );
    }
}

/// The glowing reaction zone. A coherent seam of soft emissive lobes
/// carries the glow — a shimmering curtain along the mixing interface for
/// gas–gas, a ring hugging the solid charge for solid–gas — with finer
/// pulsing beads riding the seam and sparks spitting off the solid burn.
#[allow(clippy::too_many_lines)]
fn add_reaction_front(
    meshes: &mut SceneMeshes,
    floor_y: f32,
    variant: PhaseSynthesisVariant,
    presence: f32,
    phase: f32,
    seed: u64,
) {
    const LOBES: u32 = 7;
    const BEADS: u32 = 14;
    const FRONT_COLOUR: [f32; 4] = [1.0, 0.30, 0.05, 0.55];
    const CORE_COLOUR: [f32; 4] = [1.0, 0.52, 0.16, 0.30];
    let seam_point = |unit: f32, wobble: f32, lift: f32| match variant {
        PhaseSynthesisVariant::GasGas => Vec3::new(
            wobble * 0.05,
            floor_y + 0.18 + unit * 0.68 + lift,
            (unit - 0.5) * 0.52 + wobble * 0.04,
        ),
        PhaseSynthesisVariant::SolidGas => {
            let angle = unit * std::f32::consts::TAU + phase * 0.22;
            Vec3::new(
                angle.cos() * 0.26,
                floor_y + 0.05 + lift + wobble.abs() * 0.02,
                angle.sin() * 0.26,
            )
        }
    };
    // The soft body of the seam: overlapping translucent-emissive lobes
    // breathing on offset rhythms so the glow reads as one living front.
    for lobe in 0..LOBES {
        let unit = lobe as f32 / (LOBES - 1) as f32;
        let wobble = (phase * (1.1 + seeded_unit(seed, lobe, 531) * 0.7)
            + seeded_unit(seed, lobe, 532) * 6.0)
            .sin();
        let breath = 0.72 + 0.28 * (phase * 1.7 + seeded_unit(seed, lobe, 533) * 6.0).sin();
        add_sphere(
            &mut meshes.emissive,
            seam_point(unit, wobble, 0.0),
            (0.075 * presence * breath).max(0.000_5),
            alpha(CORE_COLOUR, CORE_COLOUR[3] * presence * breath),
            4,
            6,
        );
    }
    for bead in 0..BEADS {
        let wobble = (phase * (1.6 + seeded_unit(seed, bead, 521) * 1.2)
            + seeded_unit(seed, bead, 522) * 6.0)
            .sin();
        let pulse = 0.6 + 0.4 * (phase * 2.4 + seeded_unit(seed, bead, 524) * 6.0).sin();
        add_sphere(
            &mut meshes.emissive,
            seam_point(
                seeded_unit(seed, bead, 523),
                wobble,
                seeded_unit(seed, bead, 525) * 0.05,
            ),
            (0.014 * presence * pulse).max(0.000_5),
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

#[allow(clippy::too_many_lines)]
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
    let reactant_a = bound_rgba(&synthesis.reactant_a, 0.55, ordinal, ordinal_progress);
    let reactant_b = bound_rgba(&synthesis.reactant_b, 0.55, ordinal, ordinal_progress);
    let product = bound_rgba(&synthesis.product, 0.50, ordinal, ordinal_progress);
    let inlet_colours: &[[f32; 4]] = match synthesis.variant {
        PhaseSynthesisVariant::SolidGas => &[reactant_b],
        PhaseSynthesisVariant::GasGas => &[reactant_a, reactant_b],
    };
    add_reaction_chamber(meshes, layout.bench_top, inlet_colours);
    let gas_start = meshes.gas.len();
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
                0.32,
                0.066,
                (1.0 - converted * 0.78).max(0.06),
                [solid, solid],
                None,
                26,
                seed.rotate_left(3),
            );
            let reactant_density =
                (1.0 - converted) * (0.75 + active * 0.25) * colour_visibility(reactant_b);
            if reactant_density > 0.001 {
                add_gas_density_field(
                    &mut meshes.gas,
                    Vec3::new(0.0, gas_centre_y, 0.0),
                    Vec3::new(0.40, 0.50, 0.40),
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
            let drift = 0.22 * (1.0 - approach(progress) * 0.85);
            let reactant_density = (1.0 - converted) * (0.75 + active * 0.25);
            for (channel, (colour, side)) in
                [(11_u32, (reactant_a, -1.0_f32)), (27, (reactant_b, 1.0))]
            {
                if reactant_density <= 0.001 {
                    continue;
                }
                add_gas_density_field(
                    &mut meshes.gas,
                    Vec3::new(side * drift, gas_centre_y, 0.0),
                    Vec3::new(0.32, 0.48, 0.36),
                    alpha(colour, colour[3] * (1.0 - converted).max(0.05)),
                    seed.rotate_left(channel),
                    phase,
                    reactant_density * colour_visibility(colour),
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
    let product_density = converted * 0.95 * colour_visibility(product);
    if product_density > 0.001 {
        add_gas_density_field(
            &mut meshes.gas,
            Vec3::new(0.0, gas_centre_y, 0.0),
            Vec3::new(0.40, 0.52, 0.40),
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
    // The chamber is sealed: press every concentration splat back inside
    // the shell instead of letting soft splat footprints leak past the
    // glass onto the bench.
    for splat in &mut meshes.gas[gas_start..] {
        let centre = Vec3::from_array(splat.center);
        let inner = CHAMBER_RADIUS - 0.05 - splat.radius * 0.5;
        let radial = Vec3::new(centre.x, 0.0, centre.z);
        let clamped = if radial.length() > inner {
            radial.normalize_or_zero() * inner
        } else {
            radial
        };
        let y = centre.y.clamp(
            floor_y + 0.05 + splat.radius * 0.4,
            floor_y + CHAMBER_HEIGHT - 0.06 - splat.radius * 0.4,
        );
        splat.center = [clamped.x, y, clamped.z];
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
