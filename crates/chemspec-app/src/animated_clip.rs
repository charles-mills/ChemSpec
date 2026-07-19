//! Compact, deterministic playback for authored macroscopic mesh clips.
//!
//! Blender evaluates modifiers, armatures, and object transforms offline. The
//! application keeps only quantized samples and interpolates adjacent authored
//! frames at display time, avoiding a runtime scene-format dependency.

use glam::Vec3;

const MAGIC: &[u8; 8] = b"CMSCLIP1";
const VERSION: u32 = 1;
const HEADER_SIZE: usize = 24;
const TRACK_HEADER_SIZE: usize = 36;
const SAMPLE_SIZE: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClipModule {
    Beaker,
    Water,
    Metal,
    Flame,
    Bubbles,
    Splashes,
    Vapour,
    Mixing,
    Salt,
    Stirrer,
    VesselAnchor,
    Sparks,
    Plume,
    Soot,
    PrecipitateCloud,
    FallingPrecipitate,
    PouringVessel,
    Sediment,
    SurfaceBursts,
    SolidReactant,
    InitialSolution,
    FinalSolution,
    OriginalMetal,
    MetalErosion,
    MetalDeposit,
    MetalFlakes,
    SynthesisReactantA,
    SynthesisReactantB,
    SynthesisProduct,
    SynthesisReactionFront,
    SynthesisVessel,
    SynthesisMixingTool,
    // Appended stable IDs for the high-energy metal/water source clips.
    BeakerShards,
    Explosion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClipPass {
    Opaque,
    Translucent,
    Emissive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClipColour {
    Glass,
    Water,
    WaterHighlight,
    ReactiveMetal,
    FlameOuter,
    FlameInner,
    FlameCore,
    FizzBubble,
    Vapour,
    MixtureA,
    MixtureB,
    SaltResidue,
    Fuel,
    IgnitionSpark,
    ProductPlume,
    CombustionSmoke,
    Soot,
    SootDeposit,
    LiquidInitial,
    LiquidAdded,
    PrecipitateCloud,
    Precipitate,
    GasBubble,
    GasCloud,
    SolidReactant,
    SolutionInitial,
    SolutionFinal,
    OriginalMetal,
    DepositedMetal,
    MetalErosion,
    ReactantA,
    ReactantB,
    SynthesisProduct,
    ReactionFront,
    ReactionVessel,
    MixingTool,
}

#[derive(Debug)]
pub(crate) struct ClipTrack {
    pub module: ClipModule,
    pub pass: ClipPass,
    pub colour: ClipColour,
    pub vertex_count: usize,
    pub indices: Box<[u32]>,
    origin: Vec3,
    scale: Vec3,
    sample_offset: usize,
    frame_stride: usize,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ClipVertex {
    pub position: Vec3,
    pub normal: Vec3,
}

#[derive(Debug)]
pub(crate) struct AnimatedClip {
    pub frame_count: u16,
    pub frames_per_second: u16,
    pub tracks: Box<[ClipTrack]>,
    bytes: &'static [u8],
}

impl AnimatedClip {
    pub(crate) fn parse(bytes: &'static [u8]) -> Result<Self, &'static str> {
        if bytes.len() < HEADER_SIZE || bytes.get(..8) != Some(MAGIC) {
            return Err("invalid clip header");
        }
        if read_u32(bytes, 8)? != VERSION {
            return Err("unsupported clip version");
        }
        let frame_count =
            u16::try_from(read_u32(bytes, 12)?).map_err(|_| "frame count exceeds limit")?;
        let frames_per_second =
            u16::try_from(read_u32(bytes, 16)?).map_err(|_| "frame rate exceeds limit")?;
        let track_count =
            usize::try_from(read_u32(bytes, 20)?).map_err(|_| "invalid track count")?;
        if frame_count < 2 || frames_per_second == 0 || track_count == 0 {
            return Err("empty clip");
        }

        let frame_count_usize = usize::from(frame_count);
        let mut offset = HEADER_SIZE;
        let mut tracks = Vec::with_capacity(track_count);
        for _ in 0..track_count {
            let header_end = offset
                .checked_add(TRACK_HEADER_SIZE)
                .ok_or("track header overflow")?;
            if header_end > bytes.len() {
                return Err("truncated track header");
            }
            let module = ClipModule::try_from(bytes[offset])?;
            let pass = ClipPass::try_from(bytes[offset + 1])?;
            let colour = ClipColour::try_from(bytes[offset + 2])?;
            if bytes[offset + 3] != 0 {
                return Err("unsupported track flags");
            }
            let vertex_count = usize::try_from(read_u32(bytes, offset + 4)?)
                .map_err(|_| "invalid vertex count")?;
            let index_count =
                usize::try_from(read_u32(bytes, offset + 8)?).map_err(|_| "invalid index count")?;
            let is_anchor = module == ClipModule::VesselAnchor;
            if vertex_count == 0
                || (!is_anchor && index_count == 0)
                || index_count % 3 != 0
                || (is_anchor && (vertex_count != 1 || index_count != 0))
            {
                return Err("empty or non-triangular track");
            }
            let origin = read_vec3(bytes, offset + 12)?;
            let scale = read_vec3(bytes, offset + 24)?;
            if !origin.is_finite() || !scale.is_finite() || scale.min_element() <= f32::EPSILON {
                return Err("invalid track quantization");
            }
            offset = header_end;

            let index_bytes = index_count.checked_mul(4).ok_or("index byte overflow")?;
            let indices_end = offset
                .checked_add(index_bytes)
                .ok_or("index range overflow")?;
            if indices_end > bytes.len() {
                return Err("truncated indices");
            }
            let mut indices = Vec::with_capacity(index_count);
            for index in 0..index_count {
                let value = read_u32(bytes, offset + index * 4)?;
                if usize::try_from(value).map_or(true, |value| value >= vertex_count) {
                    return Err("clip index is out of bounds");
                }
                indices.push(value);
            }
            offset = indices_end;

            let frame_stride = vertex_count
                .checked_mul(SAMPLE_SIZE)
                .ok_or("sample stride overflow")?;
            let sample_bytes = frame_stride
                .checked_mul(frame_count_usize)
                .ok_or("sample stream overflow")?;
            let samples_end = offset
                .checked_add(sample_bytes)
                .ok_or("sample range overflow")?;
            if samples_end > bytes.len() {
                return Err("truncated sample stream");
            }
            tracks.push(ClipTrack {
                module,
                pass,
                colour,
                vertex_count,
                indices: indices.into_boxed_slice(),
                origin,
                scale,
                sample_offset: offset,
                frame_stride,
            });
            offset = samples_end;
        }
        if offset != bytes.len() {
            return Err("trailing clip bytes");
        }
        Ok(Self {
            frame_count,
            frames_per_second,
            tracks: tracks.into_boxed_slice(),
            bytes,
        })
    }

    pub(crate) fn frame_at_progress(&self, progress: f32) -> f32 {
        progress.clamp(0.0, 1.0) * f32::from(self.frame_count.saturating_sub(1))
    }

    pub(crate) fn sample(&self, track: &ClipTrack, vertex_index: usize, frame: f32) -> ClipVertex {
        debug_assert!(vertex_index < track.vertex_count);
        let last = self.frame_count.saturating_sub(1);
        let frame = frame.clamp(0.0, f32::from(last));
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let first = frame.floor() as u16;
        let second = first.saturating_add(1).min(last);
        let fraction = frame - f32::from(first);
        let first = self.sample_exact(track, vertex_index, first);
        let second = self.sample_exact(track, vertex_index, second);
        ClipVertex {
            position: first.position.lerp(second.position, fraction),
            normal: first
                .normal
                .lerp(second.normal, fraction)
                .normalize_or_zero(),
        }
    }

    fn sample_exact(&self, track: &ClipTrack, vertex_index: usize, frame: u16) -> ClipVertex {
        let offset = track.sample_offset
            + usize::from(frame) * track.frame_stride
            + vertex_index * SAMPLE_SIZE;
        let position = Vec3::new(
            f32::from(read_i16(self.bytes, offset)),
            f32::from(read_i16(self.bytes, offset + 2)),
            f32::from(read_i16(self.bytes, offset + 4)),
        ) * track.scale
            + track.origin;
        let normal = Vec3::new(
            f32::from(read_i8(self.bytes, offset + 6)),
            f32::from(read_i8(self.bytes, offset + 7)),
            f32::from(read_i8(self.bytes, offset + 8)),
        ) / 127.0;
        ClipVertex {
            position,
            normal: normal.normalize_or_zero(),
        }
    }
}

impl TryFrom<u8> for ClipModule {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Beaker),
            1 => Ok(Self::Water),
            2 => Ok(Self::Metal),
            3 => Ok(Self::Flame),
            4 => Ok(Self::Bubbles),
            5 => Ok(Self::Splashes),
            6 => Ok(Self::Vapour),
            7 => Ok(Self::Mixing),
            8 => Ok(Self::Salt),
            9 => Ok(Self::Stirrer),
            10 => Ok(Self::VesselAnchor),
            11 => Ok(Self::Sparks),
            12 => Ok(Self::Plume),
            13 => Ok(Self::Soot),
            14 => Ok(Self::PrecipitateCloud),
            15 => Ok(Self::FallingPrecipitate),
            16 => Ok(Self::PouringVessel),
            17 => Ok(Self::Sediment),
            18 => Ok(Self::SurfaceBursts),
            19 => Ok(Self::SolidReactant),
            20 => Ok(Self::InitialSolution),
            21 => Ok(Self::FinalSolution),
            22 => Ok(Self::OriginalMetal),
            23 => Ok(Self::MetalErosion),
            24 => Ok(Self::MetalDeposit),
            25 => Ok(Self::MetalFlakes),
            26 => Ok(Self::SynthesisReactantA),
            27 => Ok(Self::SynthesisReactantB),
            28 => Ok(Self::SynthesisProduct),
            29 => Ok(Self::SynthesisReactionFront),
            30 => Ok(Self::SynthesisVessel),
            31 => Ok(Self::SynthesisMixingTool),
            32 => Ok(Self::BeakerShards),
            33 => Ok(Self::Explosion),
            _ => Err("unsupported clip module"),
        }
    }
}

impl TryFrom<u8> for ClipPass {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Opaque),
            1 => Ok(Self::Translucent),
            2 => Ok(Self::Emissive),
            _ => Err("unsupported clip pass"),
        }
    }
}

impl TryFrom<u8> for ClipColour {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Glass),
            1 => Ok(Self::Water),
            2 => Ok(Self::WaterHighlight),
            3 => Ok(Self::ReactiveMetal),
            4 => Ok(Self::FlameOuter),
            5 => Ok(Self::FlameInner),
            6 => Ok(Self::FlameCore),
            7 => Ok(Self::FizzBubble),
            8 => Ok(Self::Vapour),
            9 => Ok(Self::MixtureA),
            10 => Ok(Self::MixtureB),
            11 => Ok(Self::SaltResidue),
            12 => Ok(Self::Fuel),
            13 => Ok(Self::IgnitionSpark),
            14 => Ok(Self::ProductPlume),
            15 => Ok(Self::CombustionSmoke),
            16 => Ok(Self::Soot),
            17 => Ok(Self::SootDeposit),
            18 => Ok(Self::LiquidInitial),
            19 => Ok(Self::LiquidAdded),
            20 => Ok(Self::PrecipitateCloud),
            21 => Ok(Self::Precipitate),
            22 => Ok(Self::GasBubble),
            23 => Ok(Self::GasCloud),
            24 => Ok(Self::SolidReactant),
            25 => Ok(Self::SolutionInitial),
            26 => Ok(Self::SolutionFinal),
            27 => Ok(Self::OriginalMetal),
            28 => Ok(Self::DepositedMetal),
            29 => Ok(Self::MetalErosion),
            30 => Ok(Self::ReactantA),
            31 => Ok(Self::ReactantB),
            32 => Ok(Self::SynthesisProduct),
            33 => Ok(Self::ReactionFront),
            34 => Ok(Self::ReactionVessel),
            35 => Ok(Self::MixingTool),
            _ => Err("unsupported clip colour"),
        }
    }
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, &'static str> {
    bytes
        .get(offset..offset + 4)
        .and_then(|value| <[u8; 4]>::try_from(value).ok())
        .map(u32::from_le_bytes)
        .ok_or("truncated integer")
}

fn read_vec3(bytes: &[u8], offset: usize) -> Result<Vec3, &'static str> {
    Ok(Vec3::new(
        read_f32(bytes, offset)?,
        read_f32(bytes, offset + 4)?,
        read_f32(bytes, offset + 8)?,
    ))
}

fn read_f32(bytes: &[u8], offset: usize) -> Result<f32, &'static str> {
    bytes
        .get(offset..offset + 4)
        .and_then(|value| <[u8; 4]>::try_from(value).ok())
        .map(f32::from_le_bytes)
        .ok_or("truncated scalar")
}

fn read_i16(bytes: &[u8], offset: usize) -> i16 {
    bytes
        .get(offset..offset + 2)
        .and_then(|value| <[u8; 2]>::try_from(value).ok())
        .map_or(0, i16::from_le_bytes)
}

fn read_i8(bytes: &[u8], offset: usize) -> i8 {
    bytes
        .get(offset)
        .map_or(0, |value| i8::from_le_bytes([*value]))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLIP_BYTES: &[u8] = include_bytes!("../assets/models/alkali_water.clip");
    const NEUTRALISATION_BYTES: &[u8] = include_bytes!("../assets/models/neutralisation.clip");
    const COMPLETE_COMBUSTION_BYTES: &[u8] =
        include_bytes!("../assets/models/complete_combustion.clip");
    const INCOMPLETE_COMBUSTION_BYTES: &[u8] =
        include_bytes!("../assets/models/incomplete_combustion.clip");
    const PRECIPITATION_BYTES: &[u8] = include_bytes!("../assets/models/precipitation.clip");
    const GAS_EVOLUTION_LIQUID_LIQUID_BYTES: &[u8] =
        include_bytes!("../assets/models/gas_evolution_liquid_liquid.clip");
    const GAS_EVOLUTION_SOLID_LIQUID_BYTES: &[u8] =
        include_bytes!("../assets/models/gas_evolution_solid_liquid.clip");
    const METAL_DISPLACEMENT_BYTES: &[u8] =
        include_bytes!("../assets/models/metal_displacement.clip");

    #[test]
    fn bundled_clip_is_complete_bounded_and_uses_all_reaction_modules() {
        let clip = AnimatedClip::parse(CLIP_BYTES).expect("bundled clip parses");
        assert_eq!(clip.frame_count, 180);
        assert_eq!(clip.frames_per_second, 30);
        assert_eq!(clip.tracks.len(), 92);
        assert!(
            clip.tracks
                .iter()
                .map(|track| track.vertex_count)
                .sum::<usize>()
                < 8_000
        );
        assert!(
            clip.tracks
                .iter()
                .map(|track| track.indices.len())
                .sum::<usize>()
                < 32_000
        );
        for module in [
            ClipModule::Beaker,
            ClipModule::Water,
            ClipModule::Metal,
            ClipModule::Flame,
            ClipModule::Bubbles,
            ClipModule::Splashes,
            ClipModule::Vapour,
        ] {
            assert!(clip.tracks.iter().any(|track| track.module == module));
        }
    }

    #[test]
    fn playback_interpolates_between_authored_frames() {
        let clip = AnimatedClip::parse(CLIP_BYTES).expect("bundled clip parses");
        let track = clip
            .tracks
            .iter()
            .find(|track| track.module == ClipModule::Metal)
            .expect("metal track exists");
        let first = clip.sample(track, 0, 40.0);
        let second = clip.sample(track, 0, 41.0);
        let midpoint = clip.sample(track, 0, 40.5);
        assert!(
            midpoint
                .position
                .abs_diff_eq(first.position.lerp(second.position, 0.5), 0.000_01)
        );
        assert_ne!(first.position, second.position);
    }

    #[test]
    fn neutralisation_clip_references_the_shared_beaker_and_keeps_modular_tracks() {
        let clip = AnimatedClip::parse(NEUTRALISATION_BYTES).expect("neutralisation clip parses");
        assert_eq!(clip.frame_count, 240);
        assert_eq!(clip.frames_per_second, 30);
        assert_eq!(clip.tracks.len(), 65);
        assert!(
            clip.tracks
                .iter()
                .map(|track| track.vertex_count)
                .sum::<usize>()
                < 8_000
        );
        assert!(
            clip.tracks
                .iter()
                .map(|track| track.indices.len())
                .sum::<usize>()
                < 40_000
        );
        assert!(
            !clip
                .tracks
                .iter()
                .any(|track| track.module == ClipModule::Beaker),
            "the exact shared beaker must not be duplicated in this clip"
        );
        for module in [
            ClipModule::Water,
            ClipModule::Flame,
            ClipModule::Bubbles,
            ClipModule::Mixing,
            ClipModule::Salt,
            ClipModule::Stirrer,
            ClipModule::VesselAnchor,
        ] {
            assert!(clip.tracks.iter().any(|track| track.module == module));
        }
        let anchors = clip
            .tracks
            .iter()
            .filter(|track| track.module == ClipModule::VesselAnchor)
            .collect::<Vec<_>>();
        assert_eq!(anchors.len(), 1);
        assert!(anchors[0].indices.is_empty());
        let initial = clip.sample(anchors[0], 0, 0.0).position;
        let lifted = clip.sample(anchors[0], 0, 139.0).position;
        assert!(lifted.y > initial.y + 0.5);
    }

    #[test]
    fn combustion_clips_are_bounded_modular_and_share_the_existing_beaker() {
        let complete =
            AnimatedClip::parse(COMPLETE_COMBUSTION_BYTES).expect("complete combustion parses");
        let incomplete =
            AnimatedClip::parse(INCOMPLETE_COMBUSTION_BYTES).expect("incomplete combustion parses");
        for clip in [&complete, &incomplete] {
            assert_eq!(clip.frame_count, 180);
            assert_eq!(clip.frames_per_second, 30);
            assert!(
                !clip
                    .tracks
                    .iter()
                    .any(|track| track.module == ClipModule::Beaker)
            );
            assert!(
                clip.tracks
                    .iter()
                    .map(|track| track.vertex_count)
                    .sum::<usize>()
                    < 12_000
            );
            for module in [
                ClipModule::Water,
                ClipModule::Flame,
                ClipModule::Sparks,
                ClipModule::Plume,
            ] {
                assert!(clip.tracks.iter().any(|track| track.module == module));
            }
        }
        assert!(
            !complete
                .tracks
                .iter()
                .any(|track| track.module == ClipModule::Soot)
        );
        assert!(
            incomplete
                .tracks
                .iter()
                .any(|track| track.module == ClipModule::Soot)
        );
    }

    #[test]
    fn precipitation_clip_preserves_timeline_modules_and_shared_beaker() {
        let clip = AnimatedClip::parse(PRECIPITATION_BYTES).expect("precipitation clip parses");
        assert_eq!(clip.frame_count, 180);
        assert_eq!(clip.frames_per_second, 30);
        assert_eq!(clip.tracks.len(), 133);
        assert!(
            !clip
                .tracks
                .iter()
                .any(|track| track.module == ClipModule::Beaker),
            "the authored precipitation clip must reuse the shared beaker"
        );
        for module in [
            ClipModule::Water,
            ClipModule::PouringVessel,
            ClipModule::Mixing,
            ClipModule::PrecipitateCloud,
            ClipModule::FallingPrecipitate,
            ClipModule::Sediment,
        ] {
            assert!(clip.tracks.iter().any(|track| track.module == module));
        }
        for colour in [
            ClipColour::LiquidInitial,
            ClipColour::LiquidAdded,
            ClipColour::PrecipitateCloud,
            ClipColour::Precipitate,
        ] {
            assert!(clip.tracks.iter().any(|track| track.colour == colour));
        }
    }

    #[test]
    fn gas_evolution_clips_preserve_variants_timeline_and_shared_beaker() {
        let liquid = AnimatedClip::parse(GAS_EVOLUTION_LIQUID_LIQUID_BYTES)
            .expect("liquid-liquid gas-evolution clip parses");
        let solid = AnimatedClip::parse(GAS_EVOLUTION_SOLID_LIQUID_BYTES)
            .expect("solid-liquid gas-evolution clip parses");
        for clip in [&liquid, &solid] {
            assert_eq!(clip.frame_count, 180);
            assert_eq!(clip.frames_per_second, 30);
            assert!(
                !clip
                    .tracks
                    .iter()
                    .any(|track| track.module == ClipModule::Beaker),
                "gas-evolution clips reuse the shared main beaker"
            );
            for colour in [
                ClipColour::LiquidInitial,
                ClipColour::GasBubble,
                ClipColour::GasCloud,
            ] {
                assert!(clip.tracks.iter().any(|track| track.colour == colour));
            }
            for module in [
                ClipModule::Water,
                ClipModule::Bubbles,
                ClipModule::SurfaceBursts,
                ClipModule::Plume,
            ] {
                assert!(clip.tracks.iter().any(|track| track.module == module));
            }
        }
        assert!(
            liquid
                .tracks
                .iter()
                .any(|track| track.module == ClipModule::PouringVessel)
        );
        assert!(
            liquid
                .tracks
                .iter()
                .any(|track| track.colour == ClipColour::LiquidAdded)
        );
        assert!(
            solid
                .tracks
                .iter()
                .any(|track| track.module == ClipModule::SolidReactant)
        );
        assert!(
            solid
                .tracks
                .iter()
                .any(|track| track.colour == ClipColour::SolidReactant)
        );
    }

    #[test]
    fn metal_displacement_clip_is_deterministic_modular_and_reuses_the_beaker() {
        let clip =
            AnimatedClip::parse(METAL_DISPLACEMENT_BYTES).expect("metal displacement clip parses");
        assert_eq!(clip.frame_count, 180);
        assert_eq!(clip.frames_per_second, 30);
        assert_eq!(clip.tracks.len(), 42);
        assert!(
            !clip
                .tracks
                .iter()
                .any(|track| track.module == ClipModule::Beaker),
            "metal displacement must use the existing shared beaker"
        );
        for module in [
            ClipModule::InitialSolution,
            ClipModule::FinalSolution,
            ClipModule::OriginalMetal,
            ClipModule::MetalErosion,
            ClipModule::MetalDeposit,
            ClipModule::MetalFlakes,
        ] {
            assert!(clip.tracks.iter().any(|track| track.module == module));
        }
        for colour in [
            ClipColour::SolutionInitial,
            ClipColour::SolutionFinal,
            ClipColour::OriginalMetal,
            ClipColour::DepositedMetal,
            ClipColour::MetalErosion,
        ] {
            assert!(clip.tracks.iter().any(|track| track.colour == colour));
        }
        let track = clip
            .tracks
            .iter()
            .find(|track| track.module == ClipModule::MetalDeposit)
            .expect("deposit track exists");
        let first = clip.sample(track, 0, 108.25);
        let replay = clip.sample(track, 0, 108.25);
        assert_eq!(first.position, replay.position);
        assert_eq!(first.normal, replay.normal);
    }
}
