//! Reusable low-poly asset, effect, and camera behaviour registry.
//!
//! Registry entries are reaction-agnostic. Reviewed scene plans select these
//! typed profiles and the renderer instantiates their shared mesh recipes.

use chem_presentation::{AssetProfile, EffectIntensity, EffectProfile};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetGeometry {
    Bench,
    AnimatedAssembly,
    CylindricalVessel,
    LiquidCylinder,
    ImportedMetal,
    ShardCluster,
    GasCluster,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectGeometry {
    ReactionFront,
    SettlingShards,
    NucleatingSolid,
    RisingBubbles,
    EscapingGas,
    EscapingVapour,
    SurfaceRipples,
    MixingCurrents,
    SplashDroplets,
    FlamePlume,
    PresentationOnly,
}

/// Reaction-agnostic motion parameters for one reviewed effect profile.
///
/// The catalogue selects the typed effect and intensity. These reusable
/// dynamics determine only how that already-authorized phenomenon moves.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EffectDynamics {
    pub particle_count: u8,
    pub rate: f32,
    pub spread: f32,
    pub lift: f32,
    pub turbulence: f32,
    pub fade_in: f32,
    pub fade_out: f32,
}

const fn dynamics(
    particle_count: u8,
    rate: f32,
    spread: f32,
    lift: f32,
    turbulence: f32,
    fade_in: f32,
    fade_out: f32,
) -> EffectDynamics {
    EffectDynamics {
        particle_count,
        rate,
        spread,
        lift,
        turbulence,
        fade_in,
        fade_out,
    }
}

pub const fn asset_geometry(profile: AssetProfile) -> AssetGeometry {
    match profile {
        AssetProfile::LaboratoryBench | AssetProfile::DarkPresentationPlatform => {
            AssetGeometry::Bench
        }
        AssetProfile::ReactiveMetalWaterAssembly
        | AssetProfile::NeutralisationEvaporationAssembly
        | AssetProfile::CompleteCombustionAssembly
        | AssetProfile::IncompleteCombustionAssembly
        | AssetProfile::AqueousPrecipitationAssembly
        | AssetProfile::MetalDisplacementAssembly => AssetGeometry::AnimatedAssembly,
        AssetProfile::Beaker
        | AssetProfile::TestTube
        | AssetProfile::ConicalFlask
        | AssetProfile::MeasuringCylinder => AssetGeometry::CylindricalVessel,
        AssetProfile::LiquidVolume => AssetGeometry::LiquidCylinder,
        AssetProfile::MetalChunk | AssetProfile::MetalStrip => AssetGeometry::ImportedMetal,
        AssetProfile::PrecipitateCloud
        | AssetProfile::CrystalCluster
        | AssetProfile::PowderPile => AssetGeometry::ShardCluster,
        AssetProfile::GasCloud => AssetGeometry::GasCluster,
    }
}

pub const fn effect_geometry(profile: EffectProfile) -> EffectGeometry {
    match profile {
        EffectProfile::ReactionActivity => EffectGeometry::ReactionFront,
        EffectProfile::SolidFormation => EffectGeometry::NucleatingSolid,
        EffectProfile::PrecipitateFormation | EffectProfile::Clouding => {
            EffectGeometry::SettlingShards
        }
        EffectProfile::BubbleEmitter => EffectGeometry::RisingBubbles,
        EffectProfile::GasRelease => EffectGeometry::EscapingGas,
        EffectProfile::VapourRelease => EffectGeometry::EscapingVapour,
        EffectProfile::SurfaceDisturbance => EffectGeometry::SurfaceRipples,
        EffectProfile::LiquidMixing => EffectGeometry::MixingCurrents,
        EffectProfile::SplashEmitter => EffectGeometry::SplashDroplets,
        EffectProfile::FlameEmitter(_) => EffectGeometry::FlamePlume,
        EffectProfile::ObjectShrinkage
        | EffectProfile::SurfaceOxidation
        | EffectProfile::ColourTransition
        | EffectProfile::HeatDistortion => EffectGeometry::PresentationOnly,
    }
}

pub fn effect_dynamics(profile: EffectProfile, intensity: EffectIntensity) -> EffectDynamics {
    let intensity_scale = match intensity {
        EffectIntensity::Subtle => 0.72,
        EffectIntensity::Moderate => 1.0,
        EffectIntensity::Strong => 1.34,
    };
    let particle_count = match intensity {
        EffectIntensity::Subtle => 7,
        EffectIntensity::Moderate => 13,
        EffectIntensity::Strong => 21,
    };
    let mut dynamics = match profile {
        EffectProfile::ReactionActivity => {
            dynamics(particle_count.min(9), 0.52, 0.46, 0.10, 0.24, 0.10, 0.24)
        }
        EffectProfile::SolidFormation => {
            dynamics(particle_count, 0.38, 0.48, 0.22, 0.18, 0.12, 0.12)
        }
        EffectProfile::PrecipitateFormation | EffectProfile::Clouding => {
            dynamics(particle_count, 0.24, 0.72, 0.18, 0.16, 0.20, 0.0)
        }
        EffectProfile::BubbleEmitter => {
            dynamics(particle_count, 0.68, 0.28, 0.38, 0.20, 0.14, 0.18)
        }
        EffectProfile::GasRelease => dynamics(particle_count, 0.34, 0.34, 0.64, 0.34, 0.16, 0.26),
        EffectProfile::VapourRelease => {
            dynamics(particle_count.min(15), 0.48, 0.42, 0.82, 0.38, 0.10, 0.30)
        }
        EffectProfile::SurfaceDisturbance => {
            dynamics(particle_count.min(13), 0.46, 0.74, 0.08, 0.24, 0.12, 0.22)
        }
        EffectProfile::LiquidMixing => {
            dynamics(particle_count.min(9), 0.64, 0.62, 0.26, 0.34, 0.10, 0.30)
        }
        EffectProfile::SplashEmitter => {
            dynamics(particle_count.min(15), 0.76, 0.52, 0.72, 0.28, 0.10, 0.24)
        }
        EffectProfile::FlameEmitter(_) => {
            dynamics(particle_count, 1.18, 0.25, 0.82, 0.44, 0.08, 0.22)
        }
        EffectProfile::ObjectShrinkage
        | EffectProfile::SurfaceOxidation
        | EffectProfile::ColourTransition
        | EffectProfile::HeatDistortion => dynamics(0, 0.28, 0.0, 0.0, 0.10, 0.16, 0.18),
    };
    dynamics.rate *= intensity_scale;
    dynamics.spread *= intensity_scale;
    dynamics.lift *= intensity_scale;
    dynamics.turbulence *= intensity_scale;
    dynamics
}

#[cfg(test)]
mod tests {
    use chem_presentation::{AssetProfile, EffectIntensity, EffectProfile, FlamePalette};

    use super::{asset_geometry, effect_dynamics, effect_geometry};

    #[test]
    fn reusable_profiles_resolve_without_reaction_identity() {
        let beaker = asset_geometry(AssetProfile::Beaker);
        assert_eq!(beaker, asset_geometry(AssetProfile::TestTube));
        assert_eq!(
            asset_geometry(AssetProfile::AqueousPrecipitationAssembly),
            super::AssetGeometry::AnimatedAssembly
        );
        let bubbles = effect_geometry(EffectProfile::BubbleEmitter);
        assert_ne!(bubbles, effect_geometry(EffectProfile::GasRelease));
        assert_ne!(
            effect_geometry(EffectProfile::SolidFormation),
            effect_geometry(EffectProfile::PrecipitateFormation)
        );
        let subtle = effect_dynamics(EffectProfile::BubbleEmitter, EffectIntensity::Subtle);
        let strong = effect_dynamics(EffectProfile::BubbleEmitter, EffectIntensity::Strong);
        assert!(strong.particle_count > subtle.particle_count);
        assert!(strong.rate > subtle.rate);
        let flame = EffectProfile::FlameEmitter(FlamePalette::Lilac);
        assert_eq!(effect_geometry(flame), super::EffectGeometry::FlamePlume);
        assert!(effect_dynamics(flame, EffectIntensity::Strong).lift > strong.lift);
    }
}
