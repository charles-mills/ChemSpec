//! Reusable low-poly asset, effect, and camera behaviour registry.
//!
//! Registry entries are reaction-agnostic. Reviewed scene plans select these
//! typed profiles and the renderer instantiates their shared mesh recipes.

use chem_presentation::{AssetProfile, EffectIntensity, EffectProfile};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetGeometry {
    Bench,
    CylindricalVessel,
    LiquidCylinder,
    LowPolyChunk,
    ShardCluster,
    GasCluster,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectGeometry {
    SettlingShards,
    RisingBubbles,
    EscapingGas,
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

pub const fn asset_geometry(profile: AssetProfile) -> AssetGeometry {
    match profile {
        AssetProfile::LaboratoryBench | AssetProfile::DarkPresentationPlatform => {
            AssetGeometry::Bench
        }
        AssetProfile::Beaker
        | AssetProfile::TestTube
        | AssetProfile::ConicalFlask
        | AssetProfile::MeasuringCylinder => AssetGeometry::CylindricalVessel,
        AssetProfile::LiquidVolume => AssetGeometry::LiquidCylinder,
        AssetProfile::MetalChunk | AssetProfile::MetalStrip => AssetGeometry::LowPolyChunk,
        AssetProfile::PrecipitateCloud
        | AssetProfile::CrystalCluster
        | AssetProfile::PowderPile => AssetGeometry::ShardCluster,
        AssetProfile::GasCloud => AssetGeometry::GasCluster,
    }
}

pub const fn effect_geometry(profile: EffectProfile) -> EffectGeometry {
    match profile {
        EffectProfile::PrecipitateFormation | EffectProfile::Clouding => {
            EffectGeometry::SettlingShards
        }
        EffectProfile::BubbleEmitter => EffectGeometry::RisingBubbles,
        EffectProfile::GasRelease => EffectGeometry::EscapingGas,
        EffectProfile::SurfaceDisturbance => EffectGeometry::SurfaceRipples,
        EffectProfile::LiquidMixing => EffectGeometry::MixingCurrents,
        EffectProfile::SplashEmitter => EffectGeometry::SplashDroplets,
        EffectProfile::FlameEmitter(_) => EffectGeometry::FlamePlume,
        EffectProfile::ObjectShrinkage
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
        EffectProfile::PrecipitateFormation | EffectProfile::Clouding => EffectDynamics {
            particle_count,
            rate: 0.24,
            spread: 0.72,
            lift: 0.18,
            turbulence: 0.16,
            fade_in: 0.20,
            fade_out: 0.0,
        },
        EffectProfile::BubbleEmitter => EffectDynamics {
            particle_count,
            rate: 0.68,
            spread: 0.28,
            lift: 0.38,
            turbulence: 0.20,
            fade_in: 0.14,
            fade_out: 0.18,
        },
        EffectProfile::GasRelease => EffectDynamics {
            particle_count,
            rate: 0.34,
            spread: 0.34,
            lift: 0.64,
            turbulence: 0.34,
            fade_in: 0.16,
            fade_out: 0.26,
        },
        EffectProfile::SurfaceDisturbance => EffectDynamics {
            particle_count: particle_count.min(13),
            rate: 0.46,
            spread: 0.74,
            lift: 0.08,
            turbulence: 0.24,
            fade_in: 0.12,
            fade_out: 0.22,
        },
        EffectProfile::LiquidMixing => EffectDynamics {
            particle_count: particle_count.min(9),
            rate: 0.64,
            spread: 0.62,
            lift: 0.26,
            turbulence: 0.34,
            fade_in: 0.10,
            fade_out: 0.30,
        },
        EffectProfile::SplashEmitter => EffectDynamics {
            particle_count: particle_count.min(15),
            rate: 0.76,
            spread: 0.52,
            lift: 0.72,
            turbulence: 0.28,
            fade_in: 0.10,
            fade_out: 0.24,
        },
        EffectProfile::FlameEmitter(_) => EffectDynamics {
            particle_count,
            rate: 1.18,
            spread: 0.25,
            lift: 0.82,
            turbulence: 0.44,
            fade_in: 0.08,
            fade_out: 0.22,
        },
        EffectProfile::ObjectShrinkage
        | EffectProfile::ColourTransition
        | EffectProfile::HeatDistortion => EffectDynamics {
            particle_count: 0,
            rate: 0.28,
            spread: 0.0,
            lift: 0.0,
            turbulence: 0.10,
            fade_in: 0.16,
            fade_out: 0.18,
        },
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
        let bubbles = effect_geometry(EffectProfile::BubbleEmitter);
        assert_ne!(bubbles, effect_geometry(EffectProfile::GasRelease));
        let subtle = effect_dynamics(EffectProfile::BubbleEmitter, EffectIntensity::Subtle);
        let strong = effect_dynamics(EffectProfile::BubbleEmitter, EffectIntensity::Strong);
        assert!(strong.particle_count > subtle.particle_count);
        assert!(strong.rate > subtle.rate);
        let flame = EffectProfile::FlameEmitter(FlamePalette::Lilac);
        assert_eq!(effect_geometry(flame), super::EffectGeometry::FlamePlume);
        assert!(effect_dynamics(flame, EffectIntensity::Strong).lift > strong.lift);
    }
}
