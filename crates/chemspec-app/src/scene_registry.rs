//! Reusable low-poly asset, effect, and camera behaviour registry.
//!
//! Registry entries are reaction-agnostic. Reviewed scene plans select these
//! typed profiles and the renderer instantiates their shared mesh recipes.

use chem_catalogue::{AssetProfile, CameraBehaviour, EffectProfile};

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
        EffectProfile::BubbleEmitter | EffectProfile::GasRelease => EffectGeometry::RisingBubbles,
        EffectProfile::SurfaceDisturbance => EffectGeometry::SurfaceRipples,
        EffectProfile::SplashEmitter => EffectGeometry::SplashDroplets,
        EffectProfile::ObjectShrinkage
        | EffectProfile::ColourTransition
        | EffectProfile::HeatDistortion => EffectGeometry::PresentationOnly,
    }
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
            zoom: 5.5,
        },
        CameraBehaviour::ObservationCloseUp => CameraPose {
            yaw: -0.50,
            pitch: -0.78,
            zoom: 4.9,
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
    use chem_catalogue::{AssetProfile, CameraBehaviour, EffectProfile};

    use super::{asset_geometry, camera_pose, effect_geometry};

    #[test]
    fn reusable_profiles_resolve_without_reaction_identity() {
        let beaker = asset_geometry(AssetProfile::Beaker);
        assert_eq!(beaker, asset_geometry(AssetProfile::TestTube));
        let bubbles = effect_geometry(EffectProfile::BubbleEmitter);
        assert_eq!(bubbles, effect_geometry(EffectProfile::GasRelease));
        assert!(camera_pose(CameraBehaviour::WideEstablishingShot).zoom > 0.0);
        assert!(
            camera_pose(CameraBehaviour::WideEstablishingShot).pitch < -0.5,
            "the default vessel camera must begin above the reaction surface"
        );
    }
}
