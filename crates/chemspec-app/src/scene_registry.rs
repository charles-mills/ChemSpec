//! Reusable low-poly asset, effect, and camera behaviour registry.
//!
//! Registry entries are reaction-agnostic. Reviewed scene plans select these
//! typed profiles and the renderer instantiates their shared mesh recipes.

use chem_catalogue::{AssetProfile, CameraBehaviour, EffectIntensity, EffectProfile};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetGeometry {
    Bench,
    CylindricalVessel,
    LiquidCylinder,
    LowPolyChunk,
    ParticleCluster,
    GasCluster,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectGeometry {
    ParticleCloud,
    RisingBubbles,
    EscapingGas,
    SurfaceRipples,
    SplashDroplets,
    PresentationOnly,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraPose {
    pub yaw: f32,
    pub pitch: f32,
    pub zoom: f32,
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
    pub camera_energy: f32,
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
        | AssetProfile::PowderPile => AssetGeometry::ParticleCluster,
        AssetProfile::GasCloud => AssetGeometry::GasCluster,
    }
}

pub const fn effect_geometry(profile: EffectProfile) -> EffectGeometry {
    match profile {
        EffectProfile::PrecipitateFormation | EffectProfile::Clouding => {
            EffectGeometry::ParticleCloud
        }
        EffectProfile::BubbleEmitter => EffectGeometry::RisingBubbles,
        EffectProfile::GasRelease => EffectGeometry::EscapingGas,
        EffectProfile::SurfaceDisturbance => EffectGeometry::SurfaceRipples,
        EffectProfile::SplashEmitter => EffectGeometry::SplashDroplets,
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
            camera_energy: 0.08,
        },
        EffectProfile::BubbleEmitter => EffectDynamics {
            particle_count,
            rate: 0.68,
            spread: 0.28,
            lift: 0.38,
            turbulence: 0.20,
            fade_in: 0.14,
            fade_out: 0.18,
            camera_energy: 0.16,
        },
        EffectProfile::GasRelease => EffectDynamics {
            particle_count,
            rate: 0.34,
            spread: 0.34,
            lift: 0.64,
            turbulence: 0.34,
            fade_in: 0.16,
            fade_out: 0.26,
            camera_energy: 0.09,
        },
        EffectProfile::SurfaceDisturbance => EffectDynamics {
            particle_count: particle_count.min(13),
            rate: 0.46,
            spread: 0.74,
            lift: 0.08,
            turbulence: 0.24,
            fade_in: 0.12,
            fade_out: 0.22,
            camera_energy: 0.18,
        },
        EffectProfile::SplashEmitter => EffectDynamics {
            particle_count: particle_count.min(15),
            rate: 0.76,
            spread: 0.52,
            lift: 0.72,
            turbulence: 0.28,
            fade_in: 0.10,
            fade_out: 0.24,
            camera_energy: 0.28,
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
            camera_energy: 0.10,
        },
    };
    dynamics.rate *= intensity_scale;
    dynamics.spread *= intensity_scale;
    dynamics.lift *= intensity_scale;
    dynamics.turbulence *= intensity_scale;
    dynamics.camera_energy *= intensity_scale;
    dynamics
}

pub const fn camera_pose(behaviour: CameraBehaviour) -> CameraPose {
    match behaviour {
        CameraBehaviour::WideEstablishingShot => CameraPose {
            yaw: -0.78,
            pitch: -0.64,
            zoom: 7.2,
        },
        CameraBehaviour::SlowPushIn => CameraPose {
            yaw: -0.70,
            pitch: -0.69,
            zoom: 6.3,
        },
        CameraBehaviour::ReactionFocus => CameraPose {
            yaw: -0.60,
            pitch: -0.74,
            zoom: 5.8,
        },
        CameraBehaviour::ObservationCloseUp => CameraPose {
            yaw: -0.50,
            pitch: -0.78,
            zoom: 5.4,
        },
        CameraBehaviour::SlowPullBack => CameraPose {
            yaw: -0.64,
            pitch: -0.68,
            zoom: 6.5,
        },
        CameraBehaviour::FinalHeroShot => CameraPose {
            yaw: -0.72,
            pitch: -0.70,
            zoom: 5.9,
        },
    }
}

#[cfg(test)]
mod tests {
    use chem_catalogue::{AssetProfile, CameraBehaviour, EffectIntensity, EffectProfile};

    use super::{asset_geometry, camera_pose, effect_dynamics, effect_geometry};

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
        assert!(strong.camera_energy > subtle.camera_energy);
        assert!(camera_pose(CameraBehaviour::WideEstablishingShot).zoom > 0.0);
        assert!(
            camera_pose(CameraBehaviour::WideEstablishingShot).pitch < -0.5,
            "the default vessel camera must begin above the reaction surface"
        );
    }
}
