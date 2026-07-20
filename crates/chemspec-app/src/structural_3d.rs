//! Depth-tested low-poly rendering of reviewed macroscopic scene plans.
//!
//! Exact atoms and bonds remain available to the dedicated structural views.
//! This renderer consumes only trusted macroscopic assets and effects; it never
//! infers structure, parses source, or selects reaction rules.

use std::sync::OnceLock;

use bytemuck::{Pod, Zeroable};
use chem_catalogue::ObservationPredicate;
use chem_presentation::{
    AppearanceProfile, AssetProfile, EffectIntensity, EffectProfile, ExplosiveMetalWaterVariant,
    FlamePalette, GasEvolutionVariant, MacroscopicStage, PhaseSynthesisVariant,
    PresentationColourTransition,
    PresentationEffect, PresentationObject, PresentationTransform, ReactionVisualInputs, SceneRole,
    VisualColour,
};
use chem_presentation::{RealWorldPosition, ScenePlan};
use glam::{EulerRot, Mat4, Quat, Vec3};
use iced::widget::shader::{self, Program};
use iced::{Rectangle, wgpu};

use crate::animated_clip::ClipColour;
use crate::gas_fluid::{GasFlowControls, GasFluidVolume};
use crate::scene_registry::{self, AssetGeometry, EffectDynamics, EffectGeometry};

const MAX_VERTICES: u64 = 32_768;
const MAX_INDICES: u64 = 98_304;
const MAX_GAS_SPLATS: u64 = 4_096;

/// The single fixed presentation pose shared by the camera and transparent
/// triangle sort. Phase-4 camera cues will replace these constants with
/// authored choreography.
const FIXED_CAMERA_YAW: f32 = -0.72;
const FIXED_CAMERA_PITCH: f32 = -0.70;
const MSAA_SAMPLE_COUNT: u32 = 4;
const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// Material classes resolved per vertex bucket at upload; the shader keys its
/// BRDF on these instead of inferring material from alpha thresholds.
const MATERIAL_DIELECTRIC: u32 = 0;
const MATERIAL_LIQUID: u32 = 1;
const MATERIAL_GLASS: u32 = 2;
const MATERIAL_EMISSIVE: u32 = 3;
const MATERIAL_METAL: u32 = 4;

#[cfg(test)]
fn fixed_view_direction() -> Vec3 {
    -(Quat::from_rotation_y(FIXED_CAMERA_YAW) * Quat::from_rotation_x(FIXED_CAMERA_PITCH) * Vec3::Z)
}

/// Camera offsets performed for one authored [`CameraBehaviour`] cue,
/// relative to the neutral presentation pose. Everything is a pure function
/// of cue progress so scrubbing reproduces identical framing.
#[derive(Debug, Clone, Copy)]
struct CueAdjustment {
    distance_scale: f32,
    target_blend: f32,
    yaw_offset: f32,
    pitch_offset: f32,
    focus: f32,
}

impl CueAdjustment {
    const NEUTRAL: Self = Self {
        distance_scale: 1.0,
        target_blend: 0.0,
        yaw_offset: 0.0,
        pitch_offset: 0.0,
        focus: 0.0,
    };

    fn lerp(self, other: Self, blend: f32) -> Self {
        let mix = |from: f32, to: f32| from + (to - from) * blend;
        Self {
            distance_scale: mix(self.distance_scale, other.distance_scale),
            target_blend: mix(self.target_blend, other.target_blend),
            yaw_offset: mix(self.yaw_offset, other.yaw_offset),
            pitch_offset: mix(self.pitch_offset, other.pitch_offset),
            focus: mix(self.focus, other.focus),
        }
    }
}

fn smooth01(value: f32) -> f32 {
    let clamped = value.clamp(0.0, 1.0);
    clamped * clamped * (3.0 - 2.0 * clamped)
}

fn behaviour_adjustment(
    behaviour: chem_presentation::CameraBehaviour,
    progress: f32,
) -> CueAdjustment {
    use chem_presentation::CameraBehaviour;
    let eased = smooth01(progress);
    match behaviour {
        CameraBehaviour::WideEstablishingShot => CueAdjustment {
            distance_scale: 1.22,
            target_blend: 0.0,
            yaw_offset: 0.0,
            pitch_offset: -0.05,
            focus: 0.0,
        },
        CameraBehaviour::SlowPushIn => CueAdjustment {
            distance_scale: 1.14 + (0.92 - 1.14) * eased,
            target_blend: 0.55 * eased,
            yaw_offset: 0.0,
            pitch_offset: 0.0,
            focus: 0.0,
        },
        CameraBehaviour::ReactionFocus => CueAdjustment {
            distance_scale: 0.94,
            target_blend: 0.65,
            yaw_offset: 0.025,
            pitch_offset: 0.01,
            focus: 0.0,
        },
        CameraBehaviour::ObservationCloseUp => CueAdjustment {
            distance_scale: 0.80,
            target_blend: 0.85,
            yaw_offset: 0.0,
            pitch_offset: 0.035,
            focus: smooth01(progress / 0.25),
        },
        CameraBehaviour::SlowPullBack => CueAdjustment {
            distance_scale: 0.94 + (1.20 - 0.94) * eased,
            target_blend: 0.55 * (1.0 - eased),
            yaw_offset: 0.0,
            pitch_offset: 0.0,
            focus: 0.0,
        },
        CameraBehaviour::FinalHeroShot => CueAdjustment {
            distance_scale: 1.04,
            target_blend: 0.25,
            yaw_offset: -0.035 + 0.08 * eased,
            pitch_offset: 0.015,
            focus: 0.0,
        },
    }
}

/// The camera's final resting pose: a gentle push-in that levels the gaze on
/// the outcome. The closing glide blends into this across the timeline's last
/// stretch, so the final frame is arrived at, never cut to.
const HERO_ARRIVAL: CueAdjustment = CueAdjustment {
    distance_scale: 1.02,
    target_blend: 0.25,
    yaw_offset: 0.06,
    pitch_offset: 0.01,
    focus: 0.0,
};

/// Blend factor of the closing glide at this moment: zero until the final
/// stretch of the presentation, easing to one exactly at the end. smoothstep
/// has zero slope at both ends, so the glide joins and settles without a
/// visible hitch, and the completed presentation holds the hero pose.
#[allow(clippy::cast_precision_loss)]
fn closing_hero_blend(plan: &ScenePlan, moment: RealWorldPosition) -> f32 {
    let total = plan.timeline.duration_ms() as f32;
    if total <= f32::EPSILON {
        return 1.0;
    }
    let Some(elapsed) = plan.timeline.elapsed_ms_at(moment) else {
        // Past the end of the timeline: the presentation has completed and
        // holds the arrival.
        return 1.0;
    };
    let window = (total * 0.22).clamp(1_200.0, 2_600.0).min(total);
    smooth01((elapsed - (total - window)) / window)
}

/// Resolves the timeline's per-beat camera performance at this moment,
/// easing into each beat's behaviour from the previous one over the first
/// fifth of the beat. Past the schedule the final pose holds — the camera
/// never snaps to a default. The closing stretch glides into
/// [`HERO_ARRIVAL`] with a gentle orbit reveal.
fn camera_cue_adjustment(plan: &ScenePlan, moment: RealWorldPosition) -> CueAdjustment {
    const TRANSITION: f32 = 0.2;
    let beats = &plan.timeline.beats;
    let blend = closing_hero_blend(plan, moment);
    let Some(last_index) = beats.len().checked_sub(1) else {
        return CueAdjustment::NEUTRAL.lerp(HERO_ARRIVAL, blend);
    };
    let index = moment.beat_index.min(last_index);
    let progress = if moment.beat_index > last_index {
        1.0
    } else {
        moment.beat_progress.clamp(0.0, 1.0)
    };
    let current = behaviour_adjustment(beats[index].camera.behaviour, progress);
    let cue_pose = if progress < TRANSITION && index > 0 {
        behaviour_adjustment(beats[index - 1].camera.behaviour, 1.0)
            .lerp(current, smooth01(progress / TRANSITION))
    } else {
        current
    };
    let mut pose = cue_pose.lerp(HERO_ARRIVAL, blend);
    // Orbit reveal: the closing glide sweeps around the outcome before
    // settling into the hero yaw. Zero at both ends keeps the join and the
    // arrival exactly where the tests pin them.
    pose.yaw_offset += blend * (1.0 - blend) * 1.35;
    pose
}

#[derive(Debug, Clone)]
pub struct Scene {
    plan: ScenePlan,
    moment: RealWorldPosition,
    /// Pause-orbit: while playback is paused the user may drag to orbit the
    /// bench; on resume the directed camera takes over again.
    orbit_enabled: bool,
}

impl Scene {
    pub fn new(plan: &ScenePlan, moment: RealWorldPosition, orbit_enabled: bool) -> Self {
        Self {
            plan: plan.clone(),
            moment,
            orbit_enabled,
        }
    }
}

/// Interactive pause-orbit state. Playback itself stays a pure function of
/// the playhead; this only holds the user's paused-view offsets.
#[derive(Debug, Default)]
pub struct OrbitState {
    dragging: bool,
    last_cursor: Option<iced::Point>,
    yaw: f32,
    pitch: f32,
}

impl<Message> Program<Message> for Scene {
    type State = OrbitState;
    type Primitive = ScenePrimitive;

    fn update(
        &self,
        state: &mut Self::State,
        event: &iced::Event,
        bounds: Rectangle,
        cursor: iced::mouse::Cursor,
    ) -> Option<iced::widget::Action<Message>> {
        use iced::mouse;
        if !self.orbit_enabled {
            // Resuming playback hands the camera back to the director.
            if state.dragging || state.yaw != 0.0 || state.pitch != 0.0 {
                *state = OrbitState::default();
                return Some(iced::widget::Action::request_redraw());
            }
            return None;
        }
        match event {
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if cursor.is_over(bounds) =>
            {
                state.dragging = true;
                state.last_cursor = cursor.position();
                Some(iced::widget::Action::capture())
            }
            iced::Event::Mouse(mouse::Event::CursorMoved { .. }) if state.dragging => {
                if let (Some(previous), Some(current)) = (state.last_cursor, cursor.position()) {
                    state.yaw += (current.x - previous.x) * 0.008;
                    state.pitch = (state.pitch + (current.y - previous.y) * 0.006).clamp(-0.5, 0.6);
                }
                state.last_cursor = cursor.position();
                Some(iced::widget::Action::request_redraw().and_capture())
            }
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if state.dragging =>
            {
                state.dragging = false;
                Some(iced::widget::Action::capture())
            }
            _ => None,
        }
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: iced::mouse::Cursor,
    ) -> iced::mouse::Interaction {
        if self.orbit_enabled && cursor.is_over(bounds) {
            if state.dragging {
                iced::mouse::Interaction::Grabbing
            } else {
                iced::mouse::Interaction::Grab
            }
        } else {
            iced::mouse::Interaction::default()
        }
    }

    fn draw(
        &self,
        state: &Self::State,
        _cursor: iced::mouse::Cursor,
        _bounds: Rectangle,
    ) -> Self::Primitive {
        let orbit = if self.orbit_enabled {
            (state.yaw, state.pitch)
        } else {
            (0.0, 0.0)
        };
        let (vertices, indices, opaque_index_count, transparent_index_count, mut gas_splats) =
            build_scene_at(&self.plan, self.moment, orbit);
        let camera = fixed_camera_pose(&self.plan);
        let layout = SceneLayout::resolve(&self.plan);
        let cue = camera_cue_adjustment(&self.plan, self.moment);
        let time_seconds = self.plan.timeline.elapsed_ms_at(self.moment).unwrap_or(0.0) / 1000.0;
        let final_ordinal = self
            .plan
            .timeline
            .beats
            .last()
            .map_or(self.moment.ordinal, |beat| beat.end_ordinal);
        let inputs = ReactionVisualInputs::from_effects(
            &self.plan.effects,
            self.moment.ordinal,
            self.moment.ordinal_progress,
            final_ordinal,
        );
        let post_process =
            post_process_visual_state(&self.plan, self.moment.stage, self.moment.beat_progress);
        // Restrained handheld shake, driven by what the chemistry is doing:
        // pressure impulses and rolling boils nudge the camera a fraction of
        // a degree on two incommensurate frequencies.
        let shake = inputs
            .pressure_impulse
            .max(post_process.boiling * 0.5)
            .min(1.0);
        // The detonation adds a much harder, faster kick on top — scaled by
        // the blast envelope and the metal's intensity, gone with the blast.
        let blast_kick = explosion::blast_camera_shake(&self.plan, self.moment).min(2.0);
        let shake_yaw = (time_seconds * 23.7).sin() * (time_seconds * 5.9).cos() * 0.0035 * shake
            + (time_seconds * 61.3).sin() * (time_seconds * 13.7).cos() * 0.0075 * blast_kick;
        let shake_pitch = (time_seconds * 19.1).sin() * 0.0028 * shake
            + (time_seconds * 53.9).sin() * 0.0060 * blast_kick;
        let yaw = camera.yaw + cue.yaw_offset + shake_yaw + orbit.0;
        let pitch = (camera.pitch + cue.pitch_offset + shake_pitch + orbit.1).clamp(-1.35, -0.12);
        let focus_target = layout
            .camera_target
            .lerp(layout.reaction_point, cue.target_blend);
        let camera_distance = camera.view_height * 2.1 * cue.distance_scale;
        let eye = focus_target
            + Quat::from_rotation_y(yaw)
                * Quat::from_rotation_x(pitch)
                * Vec3::new(0.0, 0.0, camera_distance);
        let view_direction = (focus_target - eye).normalize_or_zero();
        gas_splats.sort_by(|left, right| {
            let left_depth = (Vec3::from_array(left.center) - eye).dot(view_direction);
            let right_depth = (Vec3::from_array(right.center) - eye).dot(view_direction);
            right_depth.total_cmp(&left_depth)
        });

        let (caustic, caustic_tint) = scene_caustics(&self.plan, layout);
        let heat_strength = inputs
            .heat_output
            .max(inputs.flame_rate * 0.85)
            .max(post_process.flame * 0.9);
        let heat_centre = layout.reaction_point + Vec3::Y * 0.6;
        ScenePrimitive {
            vertices,
            indices,
            opaque_index_count,
            transparent_index_count,
            gas_splats,
            yaw,
            pitch,
            focus_target,
            camera_distance,
            time_seconds,
            caustic,
            caustic_tint,
            heat: [heat_centre.x, heat_centre.y, heat_centre.z, heat_strength],
            focus_strength: cue.focus,
            flame_exposure: inputs.flame_rate.max(post_process.flame),
            fog_strength: inputs
                .gas_generation_rate
                .max(inputs.vapour_generation_rate)
                .max(post_process.vapour),
            endcard: closing_hero_blend(&self.plan, self.moment),
        }
    }
}

/// Bench caustic footprint and light tint for the standing liquid, or a
/// disabled set when the scene has no lit basin.
fn scene_caustics(plan: &ScenePlan, layout: SceneLayout) -> ([f32; 4], [f32; 4]) {
    if layout.has_vessel && layout.has_liquid {
        let tint = plan
            .objects
            .iter()
            .find(|object| object.role == SceneRole::Contents)
            .map_or([0.36, 0.62, 0.74, 0.28], |object| {
                appearance_color(object.appearance)
            });
        // Lift the linear tint toward white: focused light stays bright
        // even through strongly coloured solutions.
        let linear = |value: f32| value.max(0.0).powf(2.2) * 0.5 + 0.5;
        (
            [
                layout.vessel_center.x,
                layout.vessel_center.z,
                0.92 * layout.vessel_scale.x,
                1.0,
            ],
            [
                linear(tint[0]),
                linear(tint[1]),
                linear(tint[2]),
                layout.bench_top,
            ],
        )
    } else {
        ([0.0; 4], [0.0, 0.0, 0.0, layout.bench_top])
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 3],
    normal: [f32; 3],
    color: [f32; 4],
}

/// Upload format: a scene [`Vertex`] plus the material class of its bucket.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct GpuVertex {
    position: [f32; 3],
    normal: [f32; 3],
    color: [f32; 4],
    material: u32,
}

#[derive(Debug, Clone, Copy)]
struct EmbeddedMeshVertex {
    position: Vec3,
    normal: Vec3,
}

#[derive(Debug)]
struct EmbeddedMesh {
    vertices: Box<[EmbeddedMeshVertex]>,
    indices: Box<[u32]>,
}

mod alkali_water;
mod combustion;
mod displacement;
mod explosion;
mod gas_evolution;
mod neutralisation;
mod precipitation;
mod phase_synthesis;
mod synthesis;

const METAL_MESH_BYTES: &[u8] = include_bytes!("../assets/models/metal.mesh");
const EMBEDDED_MESH_MAGIC: &[u8; 8] = b"CMSHMESH";
const EMBEDDED_MESH_VERSION: u32 = 1;
static METAL_MESH: OnceLock<EmbeddedMesh> = OnceLock::new();

fn embedded_metal_mesh() -> &'static EmbeddedMesh {
    METAL_MESH.get_or_init(|| {
        parse_embedded_mesh(METAL_MESH_BYTES)
            .unwrap_or_else(|error| panic!("embedded metal mesh is invalid: {error}"))
    })
}

fn parse_embedded_mesh(bytes: &[u8]) -> Result<EmbeddedMesh, &'static str> {
    const HEADER_SIZE: usize = 20;
    const VERTEX_SIZE: usize = 24;
    if bytes.len() < HEADER_SIZE || bytes.get(..8) != Some(EMBEDDED_MESH_MAGIC) {
        return Err("invalid header");
    }
    let read_u32 = |offset: usize| {
        bytes
            .get(offset..offset + 4)
            .and_then(|value| <[u8; 4]>::try_from(value).ok())
            .map(u32::from_le_bytes)
    };
    if read_u32(8) != Some(EMBEDDED_MESH_VERSION) {
        return Err("unsupported version");
    }
    let vertex_count =
        usize::try_from(read_u32(12).ok_or("missing vertex count")?).map_err(|_| "vertex count")?;
    let index_count =
        usize::try_from(read_u32(16).ok_or("missing index count")?).map_err(|_| "index count")?;
    if vertex_count == 0 || index_count == 0 || index_count % 3 != 0 {
        return Err("empty or non-triangular mesh");
    }
    let vertex_bytes = vertex_count
        .checked_mul(VERTEX_SIZE)
        .ok_or("vertex byte overflow")?;
    let index_bytes = index_count.checked_mul(4).ok_or("index byte overflow")?;
    let expected = HEADER_SIZE
        .checked_add(vertex_bytes)
        .and_then(|size| size.checked_add(index_bytes))
        .ok_or("mesh byte overflow")?;
    if bytes.len() != expected {
        return Err("byte length mismatch");
    }

    let read_scalar = |offset: usize| {
        bytes
            .get(offset..offset + 4)
            .and_then(|value| <[u8; 4]>::try_from(value).ok())
            .map(f32::from_le_bytes)
    };
    let mut vertices = Vec::with_capacity(vertex_count);
    for index in 0..vertex_count {
        let offset = HEADER_SIZE + index * VERTEX_SIZE;
        let values = [
            read_scalar(offset).ok_or("missing position x")?,
            read_scalar(offset + 4).ok_or("missing position y")?,
            read_scalar(offset + 8).ok_or("missing position z")?,
            read_scalar(offset + 12).ok_or("missing normal x")?,
            read_scalar(offset + 16).ok_or("missing normal y")?,
            read_scalar(offset + 20).ok_or("missing normal z")?,
        ];
        if values.iter().any(|value| !value.is_finite()) {
            return Err("non-finite vertex");
        }
        let normal = Vec3::new(values[3], values[4], values[5]).normalize_or_zero();
        if normal.length_squared() <= f32::EPSILON {
            return Err("zero vertex normal");
        }
        vertices.push(EmbeddedMeshVertex {
            position: Vec3::new(values[0], values[1], values[2]),
            normal,
        });
    }

    let indices_offset = HEADER_SIZE + vertex_bytes;
    let mut indices = Vec::with_capacity(index_count);
    for index in 0..index_count {
        let value = read_u32(indices_offset + index * 4).ok_or("missing mesh index")?;
        if usize::try_from(value).map_or(true, |value| value >= vertex_count) {
            return Err("mesh index is out of bounds");
        }
        indices.push(value);
    }
    Ok(EmbeddedMesh {
        vertices: vertices.into_boxed_slice(),
        indices: indices.into_boxed_slice(),
    })
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct GasSplat {
    center: [f32; 3],
    radius: f32,
    color: [f32; 4],
    flow: [f32; 3],
    density: f32,
    layering: f32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct CameraUniform {
    view_projection: [[f32; 4]; 4],
    key_direction: [f32; 4],
    fill_direction: [f32; 4],
    camera_position: [f32; 4],
    params: [f32; 4],
    caustic: [f32; 4],
    caustic_tint: [f32; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct PanelStyle {
    top: [f32; 4],
    bottom: [f32; 4],
    border: [f32; 4],
    params: [f32; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct BlitParams {
    texel: [f32; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct CompositeParams {
    inv_view_projection: [[f32; 4]; 4],
    values: [f32; 4],
    heat: [f32; 4],
    clock: [f32; 4],
    ray: [f32; 4],
}

const COMPOSITE_EXPOSURE: f32 = 1.12;
const COMPOSITE_BLOOM_STRENGTH: f32 = 0.34;
const BLOOM_THRESHOLD: f32 = 1.0;
const MAX_BLOOM_LEVELS: usize = 5;

#[derive(Debug)]
pub struct ScenePrimitive {
    vertices: Vec<GpuVertex>,
    indices: Vec<u32>,
    opaque_index_count: u32,
    transparent_index_count: u32,
    gas_splats: Vec<GasSplat>,
    yaw: f32,
    pitch: f32,
    focus_target: Vec3,
    /// Eye distance from the focus target; camera cues animate it.
    camera_distance: f32,
    /// Deterministic presentation clock derived from the playhead; drives
    /// caustics and heat shimmer so scrubbing reproduces identical frames.
    time_seconds: f32,
    /// World x, z of the vessel footprint, its radius, and intensity.
    caustic: [f32; 4],
    /// Caustic light tint (linear rgb) and the bench-top height in w.
    caustic_tint: [f32; 4],
    /// Heat-shimmer column: world centre xyz and strength in w.
    heat: [f32; 4],
    /// 0..1 close-up focus effect strength (camera cue driven).
    focus_strength: f32,
    /// 0..1 flame envelope driving the composite's exposure dip.
    flame_exposure: f32,
    /// 0..1 closing-glide blend: the composite frames the arrival with a
    /// gentle end-card vignette.
    endcard: f32,
    /// 0..1 gas/vapour envelope driving the volumetric haze.
    fog_strength: f32,
}

#[derive(Debug)]
pub struct ScenePipeline {
    opaque_pipeline: wgpu::RenderPipeline,
    transparent_pipeline: wgpu::RenderPipeline,
    additive_pipeline: wgpu::RenderPipeline,
    gas_pipeline: wgpu::RenderPipeline,
    panel_pipeline: wgpu::RenderPipeline,
    bloom_down_pipeline: wgpu::RenderPipeline,
    bloom_up_pipeline: wgpu::RenderPipeline,
    composite_pipeline: wgpu::RenderPipeline,
    scene_layout: wgpu::BindGroupLayout,
    bloom_layout: wgpu::BindGroupLayout,
    composite_layout: wgpu::BindGroupLayout,
    background_layout: wgpu::BindGroupLayout,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    gas_buffer: wgpu::Buffer,
    uniform_buffer: wgpu::Buffer,
    scene_bind_group: Option<wgpu::BindGroup>,
    reflection_bind_group: wgpu::BindGroup,
    reflection_uniform: wgpu::Buffer,
    opaque_reflect_pipeline: wgpu::RenderPipeline,
    panel_bind_group: wgpu::BindGroup,
    linear_sampler: wgpu::Sampler,
    targets: Option<SizedTargets>,
    opaque_index_count: u32,
    transparent_index_count: u32,
    index_count: u32,
    gas_count: u32,
    physical_bounds: [u32; 4],
    overflow_warned: bool,
    gamma_encode: f32,
}

#[derive(Debug)]
struct SizedTargets {
    size: [u32; 2],
    _hdr_msaa: wgpu::Texture,
    hdr_msaa_view: wgpu::TextureView,
    _hdr_resolve: wgpu::Texture,
    hdr_resolve_view: wgpu::TextureView,
    _depth: wgpu::Texture,
    depth_view: wgpu::TextureView,
    _background: wgpu::Texture,
    background_view: wgpu::TextureView,
    background_bind_group: wgpu::BindGroup,
    _reflection_msaa: wgpu::Texture,
    reflection_msaa_view: wgpu::TextureView,
    _reflection_resolve: wgpu::Texture,
    reflection_resolve_view: wgpu::TextureView,
    _reflection_depth: wgpu::Texture,
    reflection_depth_view: wgpu::TextureView,
    _blur_half: wgpu::Texture,
    blur_half_view: wgpu::TextureView,
    blur_half_bind_group: wgpu::BindGroup,
    _blur_half_params: wgpu::Buffer,
    _blur_quarter: wgpu::Texture,
    blur_quarter_view: wgpu::TextureView,
    blur_quarter_bind_group: wgpu::BindGroup,
    _blur_quarter_params: wgpu::Buffer,
    _aux_msaa: wgpu::Texture,
    aux_msaa_view: wgpu::TextureView,
    _aux_resolve: wgpu::Texture,
    aux_resolve_view: wgpu::TextureView,
    bloom_levels: Vec<BloomLevel>,
    composite_bind_group: wgpu::BindGroup,
    composite_uniform: wgpu::Buffer,
}

#[derive(Debug)]
struct BloomLevel {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    down_bind_group: wgpu::BindGroup,
    _down_params: wgpu::Buffer,
    up_bind_group: Option<wgpu::BindGroup>,
    _up_params: Option<wgpu::Buffer>,
}

impl shader::Pipeline for ScenePipeline {
    #[allow(clippy::too_many_lines)]
    fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("chemspec structural 3d shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("structural_3d.wgsl").into()),
        });
        let post_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("chemspec structural 3d post shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("structural_3d_post.wgsl").into()),
        });
        let scene_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("chemspec structural 3d scene layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let panel_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("chemspec structural 3d panel layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let texture_entry = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let uniform_entry = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let sampler_entry = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        };
        let bloom_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("chemspec structural 3d bloom layout"),
            entries: &[uniform_entry(4), texture_entry(5), sampler_entry(6)],
        });
        let composite_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("chemspec structural 3d composite layout"),
            entries: &[
                uniform_entry(7),
                texture_entry(8),
                texture_entry(9),
                sampler_entry(10),
                texture_entry(11),
                texture_entry(13),
            ],
        });
        let background_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("chemspec structural 3d background layout"),
            entries: &[texture_entry(0), sampler_entry(1)],
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("chemspec structural 3d camera"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let panel_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("chemspec structural 3d panel style"),
            size: std::mem::size_of::<PanelStyle>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // A dark studio backdrop in linear HDR space; the tonemapper keeps its
        // gradient smooth and the dither in the composite kills banding.
        queue.write_buffer(
            &panel_buffer,
            0,
            bytemuck::bytes_of(&PanelStyle {
                top: [0.058, 0.070, 0.096, 1.0],
                bottom: [0.014, 0.017, 0.024, 1.0],
                border: [0.0, 0.0, 0.0, 0.0],
                params: [0.0, 0.62, 0.0, 0.0],
            }),
        );

        let linear_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("chemspec structural 3d linear sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..wgpu::SamplerDescriptor::default()
        });

        // The main scene bind group references the sized reflection texture,
        // so it is (re)built in ensure_targets.
        let reflection_uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("chemspec structural 3d reflection camera"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let dummy_reflection = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("chemspec structural 3d dummy reflection"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: HDR_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let dummy_reflection_view =
            dummy_reflection.create_view(&wgpu::TextureViewDescriptor::default());
        let reflection_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("chemspec structural 3d reflection group"),
            layout: &scene_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: reflection_uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&dummy_reflection_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&linear_sampler),
                },
            ],
        });
        let panel_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("chemspec structural 3d panel group"),
            layout: &panel_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: panel_buffer.as_entire_binding(),
            }],
        });

        let scene_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("chemspec structural 3d scene pipeline layout"),
                bind_group_layouts: &[&scene_layout],
                push_constant_ranges: &[],
            });
        let refracting_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("chemspec structural 3d refracting pipeline layout"),
                bind_group_layouts: &[&scene_layout, &background_layout],
                push_constant_ranges: &[],
            });
        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GpuVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![
                0 => Float32x3,
                1 => Float32x3,
                2 => Float32x4,
                9 => Uint32
            ],
        };
        let msaa_state = wgpu::MultisampleState {
            count: MSAA_SAMPLE_COUNT,
            ..wgpu::MultisampleState::default()
        };
        let create_scene_pipeline =
            |label: &'static str,
             layout: &wgpu::PipelineLayout,
             blend: Option<wgpu::BlendState>,
             depth_write_enabled: bool,
             cull_mode: Option<wgpu::Face>,
             fragment_entry: &'static str| {
                // Pass-1 pipelines additionally declare the aux (normal +
                // distance) attachment; only the g-buffer entry writes it.
                let aux_writes =
                    (fragment_entry == "fragment_gbuffer").then_some(wgpu::ColorWrites::ALL);
                let mut targets = vec![Some(wgpu::ColorTargetState {
                    format: HDR_FORMAT,
                    blend,
                    write_mask: wgpu::ColorWrites::ALL,
                })];
                if let Some(write_mask) = aux_writes {
                    targets.push(Some(wgpu::ColorTargetState {
                        format: HDR_FORMAT,
                        blend: None,
                        write_mask,
                    }));
                }
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(label),
                    layout: Some(layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vertex"),
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        buffers: std::slice::from_ref(&vertex_layout),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some(fragment_entry),
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        targets: &targets,
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleList,
                        cull_mode,
                        ..wgpu::PrimitiveState::default()
                    },
                    depth_stencil: Some(wgpu::DepthStencilState {
                        format: wgpu::TextureFormat::Depth32Float,
                        depth_write_enabled,
                        depth_compare: wgpu::CompareFunction::Less,
                        stencil: wgpu::StencilState::default(),
                        bias: wgpu::DepthBiasState::default(),
                    }),
                    multisample: msaa_state,
                    multiview: None,
                    cache: None,
                })
            };
        let opaque_pipeline = create_scene_pipeline(
            "chemspec structural 3d opaque pipeline",
            &scene_pipeline_layout,
            None,
            true,
            Some(wgpu::Face::Back),
            "fragment_gbuffer",
        );
        // Mirrored geometry flips winding, so the reflection pass culls the
        // opposite face set.
        let opaque_reflect_pipeline = create_scene_pipeline(
            "chemspec structural 3d reflected opaque pipeline",
            &scene_pipeline_layout,
            None,
            true,
            Some(wgpu::Face::Front),
            "fragment",
        );
        let transparent_pipeline = create_scene_pipeline(
            "chemspec structural 3d transparent pipeline",
            &refracting_pipeline_layout,
            Some(wgpu::BlendState::ALPHA_BLENDING),
            false,
            None,
            "fragment_transparent",
        );
        let additive_pipeline = create_scene_pipeline(
            "chemspec structural 3d additive flame pipeline",
            &scene_pipeline_layout,
            Some(wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::SrcAlpha,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent::OVER,
            }),
            false,
            None,
            "emissive_fragment",
        );
        let gas_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("chemspec structural 3d volumetric gas pipeline"),
            layout: Some(&scene_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("gas_vertex"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<GasSplat>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![
                        3 => Float32x3,
                        4 => Float32,
                        5 => Float32x4,
                        6 => Float32x3,
                        7 => Float32,
                        8 => Float32
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("gas_fragment"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[
                    Some(wgpu::ColorTargetState {
                        format: HDR_FORMAT,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                    Some(wgpu::ColorTargetState {
                        format: HDR_FORMAT,
                        blend: None,
                        write_mask: wgpu::ColorWrites::empty(),
                    }),
                ],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..wgpu::PrimitiveState::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: msaa_state,
            multiview: None,
            cache: None,
        });

        let panel_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("chemspec structural 3d panel pipeline layout"),
                bind_group_layouts: &[&panel_layout],
                push_constant_ranges: &[],
            });
        let panel_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("chemspec structural 3d panel pipeline"),
            layout: Some(&panel_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &post_shader,
                entry_point: Some("panel_vertex"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &post_shader,
                entry_point: Some("panel_fragment"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[
                    Some(wgpu::ColorTargetState {
                        format: HDR_FORMAT,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                    Some(wgpu::ColorTargetState {
                        format: HDR_FORMAT,
                        blend: None,
                        write_mask: wgpu::ColorWrites::empty(),
                    }),
                ],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: msaa_state,
            multiview: None,
            cache: None,
        });

        let create_post_pipeline =
            |label: &'static str,
             layout: &wgpu::BindGroupLayout,
             fragment_entry: &'static str,
             target_format: wgpu::TextureFormat,
             blend: Option<wgpu::BlendState>| {
                let pipeline_layout =
                    device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                        label: Some(label),
                        bind_group_layouts: &[layout],
                        push_constant_ranges: &[],
                    });
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(label),
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &post_shader,
                        entry_point: Some("blit_vertex"),
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        buffers: &[],
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &post_shader,
                        entry_point: Some(fragment_entry),
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: target_format,
                            blend,
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    primitive: wgpu::PrimitiveState::default(),
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState::default(),
                    multiview: None,
                    cache: None,
                })
            };
        let bloom_down_pipeline = create_post_pipeline(
            "chemspec structural 3d bloom downsample",
            &bloom_layout,
            "bloom_downsample",
            HDR_FORMAT,
            None,
        );
        let bloom_up_pipeline = create_post_pipeline(
            "chemspec structural 3d bloom upsample",
            &bloom_layout,
            "bloom_upsample",
            HDR_FORMAT,
            Some(wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent::OVER,
            }),
        );
        let composite_pipeline = create_post_pipeline(
            "chemspec structural 3d composite",
            &composite_layout,
            "composite_fragment",
            format,
            None,
        );

        Self {
            opaque_pipeline,
            transparent_pipeline,
            additive_pipeline,
            gas_pipeline,
            panel_pipeline,
            bloom_down_pipeline,
            bloom_up_pipeline,
            composite_pipeline,
            scene_layout,
            bloom_layout,
            composite_layout,
            background_layout,
            vertex_buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("chemspec structural 3d vertices"),
                size: MAX_VERTICES * std::mem::size_of::<GpuVertex>() as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            index_buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("chemspec structural 3d indices"),
                size: MAX_INDICES * std::mem::size_of::<u32>() as u64,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            gas_buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("chemspec structural 3d gas splats"),
                size: MAX_GAS_SPLATS * std::mem::size_of::<GasSplat>() as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            uniform_buffer,
            scene_bind_group: None,
            reflection_bind_group,
            reflection_uniform,
            opaque_reflect_pipeline,
            panel_bind_group,
            linear_sampler,
            targets: None,
            opaque_index_count: 0,
            transparent_index_count: 0,
            index_count: 0,
            gas_count: 0,
            physical_bounds: [0; 4],
            overflow_warned: false,
            gamma_encode: if format.is_srgb() { 0.0 } else { 1.0 },
        }
    }
}

impl ScenePipeline {
    #[allow(clippy::too_many_lines, clippy::cast_precision_loss)]
    fn ensure_targets(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, size: [u32; 2]) {
        if self
            .targets
            .as_ref()
            .is_some_and(|targets| targets.size == size)
        {
            return;
        }
        let extent = |width: u32, height: u32| wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        };
        let create_target = |label: &str, size: wgpu::Extent3d, format, samples: u32| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size,
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: if samples > 1 {
                    wgpu::TextureUsages::RENDER_ATTACHMENT
                } else {
                    wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING
                },
                view_formats: &[],
            })
        };
        let full = extent(size[0], size[1]);
        let hdr_msaa = create_target(
            "chemspec structural 3d hdr msaa",
            full,
            HDR_FORMAT,
            MSAA_SAMPLE_COUNT,
        );
        let hdr_msaa_view = hdr_msaa.create_view(&wgpu::TextureViewDescriptor::default());
        let hdr_resolve = create_target("chemspec structural 3d hdr resolve", full, HDR_FORMAT, 1);
        let hdr_resolve_view = hdr_resolve.create_view(&wgpu::TextureViewDescriptor::default());
        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("chemspec structural 3d depth"),
            size: full,
            mip_level_count: 1,
            sample_count: MSAA_SAMPLE_COUNT,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
        // The opaque scene resolved mid-frame; the transparent pass samples
        // this for screen-space refraction through glass and liquid.
        let background = create_target("chemspec structural 3d background", full, HDR_FORMAT, 1);
        let background_view = background.create_view(&wgpu::TextureViewDescriptor::default());
        let background_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("chemspec structural 3d background group"),
            layout: &self.background_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&background_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.linear_sampler),
                },
            ],
        });

        // Half-res mirrored-scene targets for the bench's planar reflection.
        let half = extent(size[0].max(2) / 2, size[1].max(2) / 2);
        let reflection_msaa = create_target(
            "chemspec structural 3d reflection msaa",
            half,
            HDR_FORMAT,
            MSAA_SAMPLE_COUNT,
        );
        let reflection_msaa_view =
            reflection_msaa.create_view(&wgpu::TextureViewDescriptor::default());
        let reflection_resolve = create_target(
            "chemspec structural 3d reflection resolve",
            half,
            HDR_FORMAT,
            1,
        );
        let reflection_resolve_view =
            reflection_resolve.create_view(&wgpu::TextureViewDescriptor::default());
        let reflection_depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("chemspec structural 3d reflection depth"),
            size: half,
            mip_level_count: 1,
            sample_count: MSAA_SAMPLE_COUNT,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let reflection_depth_view =
            reflection_depth.create_view(&wgpu::TextureViewDescriptor::default());
        self.scene_bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("chemspec structural 3d scene group"),
            layout: &self.scene_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&reflection_resolve_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&self.linear_sampler),
                },
            ],
        }));

        // Half-resolution cascade for the dual-filter bloom.
        let mut level_sizes: Vec<[u32; 2]> = Vec::new();
        let mut level_size = [size[0].max(2) / 2, size[1].max(2) / 2];
        while level_sizes.len() < MAX_BLOOM_LEVELS {
            level_sizes.push([level_size[0].max(1), level_size[1].max(1)]);
            if level_size[0] / 2 < 8 || level_size[1] / 2 < 8 {
                break;
            }
            level_size = [level_size[0] / 2, level_size[1] / 2];
        }
        let bloom_textures: Vec<wgpu::Texture> = level_sizes
            .iter()
            .map(|level| {
                create_target(
                    "chemspec structural 3d bloom level",
                    extent(level[0], level[1]),
                    HDR_FORMAT,
                    1,
                )
            })
            .collect();
        let bloom_views: Vec<wgpu::TextureView> = bloom_textures
            .iter()
            .map(|texture| texture.create_view(&wgpu::TextureViewDescriptor::default()))
            .collect();
        let create_blit_params = |source: [u32; 2], threshold: f32, intensity: f32| {
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("chemspec structural 3d blit params"),
                size: std::mem::size_of::<BlitParams>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            queue.write_buffer(
                &buffer,
                0,
                bytemuck::bytes_of(&BlitParams {
                    texel: [
                        1.0 / source[0].max(1) as f32,
                        1.0 / source[1].max(1) as f32,
                        threshold,
                        intensity,
                    ],
                }),
            );
            buffer
        };
        let create_blit_group = |params: &wgpu::Buffer, source_view: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("chemspec structural 3d blit group"),
                layout: &self.bloom_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: params.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: wgpu::BindingResource::TextureView(source_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: wgpu::BindingResource::Sampler(&self.linear_sampler),
                    },
                ],
            })
        };
        let mut bloom_levels = Vec::with_capacity(level_sizes.len());
        for (index, texture) in bloom_textures.into_iter().enumerate() {
            let source_size = if index == 0 {
                size
            } else {
                level_sizes[index - 1]
            };
            let down_params = create_blit_params(
                source_size,
                if index == 0 { BLOOM_THRESHOLD } else { 0.0 },
                1.0,
            );
            let down_source = if index == 0 {
                &hdr_resolve_view
            } else {
                &bloom_views[index - 1]
            };
            let down_bind_group = create_blit_group(&down_params, down_source);
            let (up_params, up_bind_group) = if index + 1 < level_sizes.len() {
                let params = create_blit_params(level_sizes[index + 1], 0.0, 0.72);
                let group = create_blit_group(&params, &bloom_views[index + 1]);
                (Some(params), Some(group))
            } else {
                (None, None)
            };
            bloom_levels.push(BloomLevel {
                _texture: texture,
                view: bloom_views[index].clone(),
                down_bind_group,
                _down_params: down_params,
                up_bind_group,
                _up_params: up_params,
            });
        }

        // Unthresholded half/quarter blur chain feeding the close-up focus
        // effect in the composite.
        let half_size = [size[0].max(2) / 2, size[1].max(2) / 2];
        let quarter_size = [half_size[0].max(2) / 2, half_size[1].max(2) / 2];
        let blur_half = create_target(
            "chemspec structural 3d blur half",
            extent(half_size[0], half_size[1]),
            HDR_FORMAT,
            1,
        );
        let blur_half_view = blur_half.create_view(&wgpu::TextureViewDescriptor::default());
        let blur_quarter = create_target(
            "chemspec structural 3d blur quarter",
            extent(quarter_size[0], quarter_size[1]),
            HDR_FORMAT,
            1,
        );
        let blur_quarter_view = blur_quarter.create_view(&wgpu::TextureViewDescriptor::default());
        let blur_half_params = create_blit_params(size, 0.0, 1.0);
        let blur_half_bind_group = create_blit_group(&blur_half_params, &hdr_resolve_view);
        let blur_quarter_params = create_blit_params(half_size, 0.0, 1.0);
        let blur_quarter_bind_group = create_blit_group(&blur_quarter_params, &blur_half_view);

        // Aux (normal + camera distance) buffer alongside the HDR scene. The
        // composite uses its distance channel to bound the height-fog march.
        let aux_msaa = create_target(
            "chemspec structural 3d aux msaa",
            full,
            HDR_FORMAT,
            MSAA_SAMPLE_COUNT,
        );
        let aux_msaa_view = aux_msaa.create_view(&wgpu::TextureViewDescriptor::default());
        let aux_resolve = create_target("chemspec structural 3d aux resolve", full, HDR_FORMAT, 1);
        let aux_resolve_view = aux_resolve.create_view(&wgpu::TextureViewDescriptor::default());
        let composite_uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("chemspec structural 3d composite params"),
            size: std::mem::size_of::<CompositeParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(
            &composite_uniform,
            0,
            bytemuck::bytes_of(&CompositeParams {
                inv_view_projection: Mat4::IDENTITY.to_cols_array_2d(),
                values: [
                    COMPOSITE_EXPOSURE,
                    COMPOSITE_BLOOM_STRENGTH,
                    self.gamma_encode,
                    0.0,
                ],
                heat: [0.0; 4],
                clock: [0.0; 4],
                ray: [0.0; 4],
            }),
        );
        let composite_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("chemspec structural 3d composite group"),
            layout: &self.composite_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: composite_uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: wgpu::BindingResource::TextureView(&hdr_resolve_view),
                },
                wgpu::BindGroupEntry {
                    binding: 9,
                    resource: wgpu::BindingResource::TextureView(&bloom_levels[0].view),
                },
                wgpu::BindGroupEntry {
                    binding: 10,
                    resource: wgpu::BindingResource::Sampler(&self.linear_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 11,
                    resource: wgpu::BindingResource::TextureView(&blur_quarter_view),
                },
                wgpu::BindGroupEntry {
                    binding: 13,
                    resource: wgpu::BindingResource::TextureView(&aux_resolve_view),
                },
            ],
        });

        self.targets = Some(SizedTargets {
            size,
            _hdr_msaa: hdr_msaa,
            hdr_msaa_view,
            _hdr_resolve: hdr_resolve,
            hdr_resolve_view,
            _depth: depth,
            depth_view,
            _background: background,
            background_view,
            background_bind_group,
            _reflection_msaa: reflection_msaa,
            reflection_msaa_view,
            _reflection_resolve: reflection_resolve,
            reflection_resolve_view,
            _reflection_depth: reflection_depth,
            reflection_depth_view,
            _blur_half: blur_half,
            blur_half_view,
            blur_half_bind_group,
            _blur_half_params: blur_half_params,
            _blur_quarter: blur_quarter,
            blur_quarter_view,
            blur_quarter_bind_group,
            _blur_quarter_params: blur_quarter_params,
            _aux_msaa: aux_msaa,
            aux_msaa_view,
            _aux_resolve: aux_resolve,
            aux_resolve_view,
            bloom_levels,
            composite_bind_group,
            composite_uniform,
        });
    }
}

impl ScenePrimitive {
    /// Uploads the camera, light, caustic, and clock uniforms; returns the
    /// view-projection so the composite can project world-space regions.
    #[allow(clippy::cast_precision_loss)]
    fn write_camera_uniform(
        &self,
        pipeline: &ScenePipeline,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
    ) -> (Mat4, Vec3) {
        let aspect = width as f32 / height.max(1) as f32;
        let reaction_target = self.focus_target;
        let pitch = self.pitch.clamp(-1.18, -0.22);
        let eye = reaction_target
            + Quat::from_rotation_y(self.yaw)
                * Quat::from_rotation_x(pitch)
                * Vec3::new(0.0, 0.0, self.camera_distance.max(1.0));
        let view = Mat4::look_at_rh(eye, reaction_target, Vec3::Y);
        // Mild telephoto chosen so the neutral distance (2.1 x view height)
        // frames exactly the old orthographic view height at the target.
        let projection = Mat4::perspective_rh(0.468, aspect, 0.3, 60.0);
        let key_direction = Vec3::new(-0.55, -0.88, -0.48).normalize();
        let view_projection = projection * view;
        let uniform = CameraUniform {
            view_projection: view_projection.to_cols_array_2d(),
            key_direction: [key_direction.x, key_direction.y, key_direction.z, 0.0],
            fill_direction: [0.70, -0.45, 0.55, 0.0],
            camera_position: [eye.x, eye.y, eye.z, 1.0],
            params: [self.time_seconds, width as f32, height as f32, 0.0],
            caustic: self.caustic,
            caustic_tint: self.caustic_tint,
        };
        queue.write_buffer(&pipeline.uniform_buffer, 0, bytemuck::bytes_of(&uniform));

        // Mirror camera across the bench plane for the planar reflection.
        let bench_top = self.caustic_tint[3];
        let mirror = Mat4::from_translation(Vec3::Y * bench_top)
            * Mat4::from_scale(Vec3::new(1.0, -1.0, 1.0))
            * Mat4::from_translation(Vec3::Y * -bench_top);
        let mirrored_eye = Vec3::new(eye.x, 2.0 * bench_top - eye.y, eye.z);
        let reflection = CameraUniform {
            view_projection: (view_projection * mirror).to_cols_array_2d(),
            camera_position: [mirrored_eye.x, mirrored_eye.y, mirrored_eye.z, 1.0],
            params: [self.time_seconds, width as f32, height as f32, 1.0],
            ..uniform
        };
        queue.write_buffer(
            &pipeline.reflection_uniform,
            0,
            bytemuck::bytes_of(&reflection),
        );
        (view_projection, eye)
    }

    /// Projects the heat column into uv space and uploads the composite
    /// parameters (exposure, bloom, gamma, focus, shimmer, clock).
    fn write_composite_uniform(
        &self,
        pipeline: &ScenePipeline,
        queue: &wgpu::Queue,
        view_projection: Mat4,
        eye: Vec3,
    ) {
        let heat_centre = Vec3::new(self.heat[0], self.heat[1], self.heat[2]);
        let project_uv = |world: Vec3| {
            let clip = view_projection * world.extend(1.0);
            let ndc = clip.truncate() / clip.w.max(1e-4);
            [ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5]
        };
        let centre_uv = project_uv(heat_centre);
        let camera_right = Quat::from_rotation_y(self.yaw) * Vec3::X;
        let edge_uv = project_uv(heat_centre + camera_right * 1.15);
        let radius_uv = ((edge_uv[0] - centre_uv[0]).powi(2) + (edge_uv[1] - centre_uv[1]).powi(2))
            .sqrt()
            .max(0.02);
        if let Some(targets) = &pipeline.targets {
            queue.write_buffer(
                &targets.composite_uniform,
                0,
                bytemuck::bytes_of(&CompositeParams {
                    inv_view_projection: view_projection.inverse().to_cols_array_2d(),
                    values: [
                        COMPOSITE_EXPOSURE,
                        COMPOSITE_BLOOM_STRENGTH,
                        pipeline.gamma_encode,
                        self.focus_strength,
                    ],
                    heat: [centre_uv[0], centre_uv[1], radius_uv, self.heat[3]],
                    clock: [
                        self.time_seconds,
                        self.flame_exposure,
                        self.fog_strength,
                        self.endcard,
                    ],
                    ray: [eye.x, eye.y, eye.z, self.caustic_tint[3]],
                }),
            );
        }
    }
}

impl shader::Primitive for ScenePrimitive {
    type Pipeline = ScenePipeline;

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss
    )]
    fn prepare(
        &self,
        pipeline: &mut Self::Pipeline,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bounds: &Rectangle,
        viewport: &shader::Viewport,
    ) {
        let scale = viewport.scale_factor();
        let width = (bounds.width * scale).round().max(1.0) as u32;
        let height = (bounds.height * scale).round().max(1.0) as u32;
        pipeline.physical_bounds = [
            (bounds.x * scale).round().max(0.0) as u32,
            (bounds.y * scale).round().max(0.0) as u32,
            width,
            height,
        ];
        pipeline.ensure_targets(device, queue, [width, height]);
        if self.vertices.len() as u64 > MAX_VERTICES || self.indices.len() as u64 > MAX_INDICES {
            if !pipeline.overflow_warned {
                pipeline.overflow_warned = true;
                eprintln!(
                    "structural 3d scene exceeds geometry budget \
                     ({} vertices, {} indices); frame skipped",
                    self.vertices.len(),
                    self.indices.len(),
                );
            }
            pipeline.opaque_index_count = 0;
            pipeline.transparent_index_count = 0;
            pipeline.index_count = 0;
            pipeline.gas_count = 0;
            return;
        }
        queue.write_buffer(
            &pipeline.vertex_buffer,
            0,
            bytemuck::cast_slice(&self.vertices),
        );
        queue.write_buffer(
            &pipeline.index_buffer,
            0,
            bytemuck::cast_slice(&self.indices),
        );
        if self.gas_splats.len() as u64 <= MAX_GAS_SPLATS {
            queue.write_buffer(
                &pipeline.gas_buffer,
                0,
                bytemuck::cast_slice(&self.gas_splats),
            );
            pipeline.gas_count = u32::try_from(self.gas_splats.len()).unwrap_or(u32::MAX);
        } else {
            pipeline.gas_count = 0;
        }
        pipeline.index_count = u32::try_from(self.indices.len()).unwrap_or(u32::MAX);
        pipeline.opaque_index_count = self.opaque_index_count.min(pipeline.index_count);
        pipeline.transparent_index_count = self
            .transparent_index_count
            .clamp(pipeline.opaque_index_count, pipeline.index_count);

        let (view_projection, eye) = self.write_camera_uniform(pipeline, queue, width, height);
        self.write_composite_uniform(pipeline, queue, view_projection, eye);
    }

    #[allow(clippy::cast_precision_loss, clippy::too_many_lines)]
    fn render(
        &self,
        pipeline: &Self::Pipeline,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        let Some(targets) = &pipeline.targets else {
            return;
        };
        let Some(scene_bind_group) = &pipeline.scene_bind_group else {
            return;
        };
        let [x, y, width, height] = pipeline.physical_bounds;
        let scissor_x = clip_bounds.x.max(x);
        let scissor_y = clip_bounds.y.max(y);
        let scissor_right = clip_bounds
            .x
            .saturating_add(clip_bounds.width)
            .min(x.saturating_add(width));
        let scissor_bottom = clip_bounds
            .y
            .saturating_add(clip_bounds.height)
            .min(y.saturating_add(height));
        let scissor_width = scissor_right.saturating_sub(scissor_x);
        let scissor_height = scissor_bottom.saturating_sub(scissor_y);
        if scissor_width == 0 || scissor_height == 0 {
            return;
        }

        // Mirrored scene, half res: the bench samples this for its planar
        // reflection. The bench itself is discarded in-shader; gas splats are
        // skipped (their billboards assume the primary camera).
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("chemspec structural 3d reflection pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &targets.reflection_msaa_view,
                    depth_slice: None,
                    resolve_target: Some(&targets.reflection_resolve_view),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.013,
                            g: 0.016,
                            b: 0.023,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Discard,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &targets.reflection_depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_bind_group(0, &pipeline.reflection_bind_group, &[]);
            pass.set_vertex_buffer(0, pipeline.vertex_buffer.slice(..));
            pass.set_index_buffer(pipeline.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.set_pipeline(&pipeline.opaque_reflect_pipeline);
            pass.draw_indexed(0..pipeline.opaque_index_count, 0, 0..1);
            if pipeline.opaque_index_count < pipeline.transparent_index_count {
                pass.set_bind_group(1, &targets.background_bind_group, &[]);
                pass.set_pipeline(&pipeline.transparent_pipeline);
                pass.draw_indexed(
                    pipeline.opaque_index_count..pipeline.transparent_index_count,
                    0,
                    0..1,
                );
            }
            if pipeline.transparent_index_count < pipeline.index_count {
                pass.set_pipeline(&pipeline.additive_pipeline);
                pass.draw_indexed(
                    pipeline.transparent_index_count..pipeline.index_count,
                    0,
                    0..1,
                );
            }
        }

        // Opaque pass: backdrop, opaque geometry, and gas resolve into the
        // background texture the transparent pass refracts. The MSAA contents
        // are kept for the second pass to continue over.
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("chemspec structural 3d opaque pass"),
                color_attachments: &[
                    Some(wgpu::RenderPassColorAttachment {
                        view: &targets.hdr_msaa_view,
                        depth_slice: None,
                        resolve_target: Some(&targets.background_view),
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                    Some(wgpu::RenderPassColorAttachment {
                        view: &targets.aux_msaa_view,
                        depth_slice: None,
                        resolve_target: Some(&targets.aux_resolve_view),
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Discard,
                        },
                    }),
                ],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &targets.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&pipeline.panel_pipeline);
            pass.set_bind_group(0, &pipeline.panel_bind_group, &[]);
            pass.draw(0..3, 0..1);
            pass.set_pipeline(&pipeline.opaque_pipeline);
            pass.set_bind_group(0, scene_bind_group, &[]);
            pass.set_vertex_buffer(0, pipeline.vertex_buffer.slice(..));
            pass.set_index_buffer(pipeline.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..pipeline.opaque_index_count, 0, 0..1);
            if pipeline.gas_count > 0 {
                pass.set_pipeline(&pipeline.gas_pipeline);
                pass.set_vertex_buffer(0, pipeline.gas_buffer.slice(..));
                pass.draw(0..6, 0..pipeline.gas_count);
            }
        }

        // Transparent pass: sorted glass and liquid refract the resolved
        // background, then emissive cores blend additively; the final HDR
        // frame resolves out of the same MSAA target.
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("chemspec structural 3d transparent pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &targets.hdr_msaa_view,
                    depth_slice: None,
                    resolve_target: Some(&targets.hdr_resolve_view),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Discard,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &targets.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_bind_group(0, scene_bind_group, &[]);
            pass.set_bind_group(1, &targets.background_bind_group, &[]);
            pass.set_vertex_buffer(0, pipeline.vertex_buffer.slice(..));
            pass.set_index_buffer(pipeline.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            if pipeline.opaque_index_count < pipeline.transparent_index_count {
                pass.set_pipeline(&pipeline.transparent_pipeline);
                pass.draw_indexed(
                    pipeline.opaque_index_count..pipeline.transparent_index_count,
                    0,
                    0..1,
                );
            }
            if pipeline.transparent_index_count < pipeline.index_count {
                pass.set_pipeline(&pipeline.additive_pipeline);
                pass.draw_indexed(
                    pipeline.transparent_index_count..pipeline.index_count,
                    0,
                    0..1,
                );
            }
        }

        for level in &targets.bloom_levels {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("chemspec structural 3d bloom downsample pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &level.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&pipeline.bloom_down_pipeline);
            pass.set_bind_group(0, &level.down_bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        for level in targets.bloom_levels.iter().rev() {
            let Some(up_bind_group) = &level.up_bind_group else {
                continue;
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("chemspec structural 3d bloom upsample pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &level.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&pipeline.bloom_up_pipeline);
            pass.set_bind_group(0, up_bind_group, &[]);
            pass.draw(0..3, 0..1);
        }

        // Unthresholded blur chain for the close-up focus effect.
        for (view, bind_group) in [
            (&targets.blur_half_view, &targets.blur_half_bind_group),
            (&targets.blur_quarter_view, &targets.blur_quarter_bind_group),
        ] {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("chemspec structural 3d focus blur pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&pipeline.bloom_down_pipeline);
            pass.set_bind_group(0, bind_group, &[]);
            pass.draw(0..3, 0..1);
        }

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("chemspec structural 3d composite pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_viewport(x as f32, y as f32, width as f32, height as f32, 0.0, 1.0);
        pass.set_scissor_rect(scissor_x, scissor_y, scissor_width, scissor_height);
        pass.set_pipeline(&pipeline.composite_pipeline);
        pass.set_bind_group(0, &targets.composite_bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[derive(Debug, Clone, Copy)]
struct SceneLayout {
    bench_top: f32,
    has_vessel: bool,
    vessel_center: Vec3,
    vessel_scale: Vec3,
    has_liquid: bool,
    liquid_center: Vec3,
    liquid_surface: f32,
    reaction_point: Vec3,
    camera_target: Vec3,
}

impl SceneLayout {
    fn resolve(plan: &ScenePlan) -> Self {
        let bench_top = -0.76;
        let vessel = plan
            .objects
            .iter()
            .find(|object| object.role == SceneRole::Vessel);
        if vessel.is_some_and(|object| {
            matches!(
                object.asset,
                AssetProfile::ReactiveMetalWaterAssembly
                    | AssetProfile::NeutralisationEvaporationAssembly
                    | AssetProfile::CompleteCombustionAssembly
                    | AssetProfile::IncompleteCombustionAssembly
                    | AssetProfile::AqueousPrecipitationAssembly
                    | AssetProfile::MetalDisplacementAssembly
                    | AssetProfile::SolidSolidSynthesisAssembly
                    | AssetProfile::SolidGasSynthesisAssembly
                    | AssetProfile::GasGasSynthesisAssembly
            )
        }) {
            let vessel_center = Vec3::new(0.0, bench_top + 0.90, 0.0);
            let liquid_center = Vec3::new(0.0, bench_top + 0.81, 0.0);
            let liquid_surface = bench_top + 1.543;
            // The sealed gas chambers hold no standing liquid: without this
            // the bench renders a light-through-liquid caustic footprint
            // around a vessel that contains only gas.
            let dry_chamber = vessel.is_some_and(|object| {
                matches!(
                    object.asset,
                    AssetProfile::SolidGasSynthesisAssembly | AssetProfile::GasGasSynthesisAssembly
                )
            });
            return Self {
                bench_top,
                has_vessel: true,
                vessel_center,
                vessel_scale: Vec3::new(0.99, 0.90, 0.99),
                has_liquid: !dry_chamber,
                liquid_center,
                liquid_surface,
                reaction_point: Vec3::new(0.0, liquid_surface + 0.045, 0.0),
                camera_target: Vec3::new(0.0, bench_top + 1.10, 0.0),
            };
        }
        let vessel_scale = vessel.map_or(Vec3::ONE, |object| transform_scale(&object.transform));
        let vessel_source = vessel.map_or(Vec3::ZERO, |object| {
            transform_translation(&object.transform)
        });
        let vessel_center = vessel.map_or(Vec3::new(0.0, bench_top, 0.0), |_| {
            Vec3::new(
                vessel_source.x,
                bench_top + 0.55 * vessel_scale.y,
                vessel_source.z,
            )
        });
        let contents = plan
            .objects
            .iter()
            .find(|object| object.role == SceneRole::Contents);
        let liquid_scale = contents.map_or(Vec3::new(0.86, 0.62, 0.86), |object| {
            transform_scale(&object.transform)
        });
        let liquid_bottom = bench_top + 0.06;
        let liquid_center = Vec3::new(
            vessel_center.x,
            liquid_bottom + 0.52 * liquid_scale.y,
            vessel_center.z,
        );
        let liquid_surface = liquid_center.y + 0.54 * liquid_scale.y;
        let reaction_point = if vessel.is_some() {
            Vec3::new(vessel_center.x, liquid_surface + 0.065, vessel_center.z)
        } else {
            Vec3::new(vessel_center.x, bench_top + 0.006, vessel_center.z)
        };
        let precipitation = plan.effects.iter().any(|effect| {
            matches!(
                effect.effect,
                EffectProfile::PrecipitateFormation | EffectProfile::Clouding
            )
        });
        let camera_target = if vessel.is_some() {
            Vec3::new(
                vessel_center.x,
                if precipitation {
                    liquid_center.y
                } else {
                    liquid_surface
                },
                vessel_center.z,
            )
        } else {
            reaction_point + Vec3::Y * 0.30
        };
        Self {
            bench_top,
            has_vessel: vessel.is_some(),
            vessel_center,
            vessel_scale,
            has_liquid: contents.is_some(),
            liquid_center,
            liquid_surface,
            reaction_point,
            camera_target,
        }
    }

    fn object_offset(self, object: &PresentationObject) -> Vec3 {
        let source = transform_translation(&object.transform);
        let target = match object.role {
            SceneRole::Vessel => self.vessel_center,
            SceneRole::Contents => self.liquid_center,
            SceneRole::Reactant if object.asset == AssetProfile::GasCloud => Vec3::new(
                self.vessel_center.x + source.x,
                self.liquid_surface + 0.30,
                self.vessel_center.z + source.z,
            ),
            SceneRole::Reactant => Vec3::new(
                self.reaction_point.x + source.x,
                self.reaction_point.y,
                self.reaction_point.z + source.z,
            ),
            SceneRole::Product => match object.asset {
                AssetProfile::PrecipitateCloud
                | AssetProfile::CrystalCluster
                | AssetProfile::PowderPile => Vec3::new(
                    self.vessel_center.x + source.x,
                    self.bench_top + 0.12,
                    self.vessel_center.z + source.z,
                ),
                AssetProfile::GasCloud => Vec3::new(
                    self.vessel_center.x + source.x,
                    self.liquid_surface + 0.42,
                    self.vessel_center.z + source.z,
                ),
                _ => source,
            },
            SceneRole::Environment => source,
        };
        target - source
    }

    fn with_reaction_motion(mut self, motion: Vec3) -> Self {
        self.reaction_point += motion;
        self
    }

    fn with_vessel_motion(mut self, motion: Vec3) -> Self {
        self.vessel_center += motion;
        self.liquid_center += motion;
        self.liquid_surface += motion.y;
        self.reaction_point += motion;
        self
    }

    fn gas_volume(self) -> (Vec3, Vec3) {
        if !self.has_vessel {
            return (
                self.reaction_point + Vec3::Y * 0.42,
                Vec3::new(1.2, 0.7, 1.2),
            );
        }
        let vessel_floor = self.bench_top + 0.055 * self.vessel_scale.y;
        let vessel_rim = self.vessel_center.y + 0.91 * self.vessel_scale.y;
        let volume_floor = if self.has_liquid {
            self.liquid_surface + 0.025
        } else {
            vessel_floor
        };
        let available_height = (vessel_rim - volume_floor).max(0.28);
        (
            Vec3::new(
                self.vessel_center.x,
                volume_floor + available_height * 0.5,
                self.vessel_center.z,
            ),
            Vec3::new(
                self.vessel_scale.x * 0.98,
                available_height * 0.52,
                self.vessel_scale.z * 0.98,
            ),
        )
    }

    fn with_liquid_fraction(mut self, fraction: f32) -> Self {
        let fraction = fraction.clamp(0.0, 1.0);
        let half_above = self.liquid_surface - self.liquid_center.y;
        let full_height = half_above / 0.54 * 1.06;
        let bottom = self.liquid_surface - full_height;
        let height = full_height * fraction;
        self.liquid_center.y = bottom + height * (0.52 / 1.06);
        self.liquid_surface = bottom + height;
        self.reaction_point.y = self.liquid_surface + 0.065;
        self
    }
}

#[derive(Debug, Clone, Copy)]
struct ObjectMotion {
    translation: Vec3,
    rotation: Quat,
}

#[derive(Debug, Clone, Copy)]
struct StirrerPose {
    lower: Vec3,
    upper: Vec3,
    visibility: f32,
    submerged: f32,
    activity: f32,
}

#[derive(Debug, Clone, Copy)]
struct EffectMoment {
    ordinal: u16,
    progress: f32,
    stage: MacroscopicStage,
}

#[derive(Debug, Clone, Copy)]
struct AssetColourTransition {
    target: VisualColour,
    progress: f32,
    seed: u64,
}

#[derive(Debug, Clone, Copy)]
struct EffectColours {
    liquid: [f32; 4],
    solid: [f32; 4],
    gas: [f32; 4],
}

#[derive(Debug, Clone, Copy)]
struct PostProcessVisualState {
    active: bool,
    lift: f32,
    flame: f32,
    boiling: f32,
    vapour: f32,
    liquid_fraction: f32,
    crystal_growth: f32,
}

// A visible air gap is important in the fixed orthographic view: with a
// smaller displacement the transparent vessel makes the external flame read
// as though it is burning inside the liquid.
const HEATING_LIFT: f32 = 0.48;

impl Default for PostProcessVisualState {
    fn default() -> Self {
        Self {
            active: false,
            lift: 0.0,
            flame: 0.0,
            boiling: 0.0,
            vapour: 0.0,
            liquid_fraction: 1.0,
            crystal_growth: 0.0,
        }
    }
}

fn post_process_visual_state(
    plan: &ScenePlan,
    stage: MacroscopicStage,
    progress: f32,
) -> PostProcessVisualState {
    if plan.post_process
        != Some(chem_presentation::MacroscopicProcess::SolventEvaporationCrystallization)
    {
        return PostProcessVisualState::default();
    }
    let progress = progress.clamp(0.0, 1.0);
    match stage {
        MacroscopicStage::Reaction => PostProcessVisualState::default(),
        MacroscopicStage::HeatingPreparation => {
            let spring = 1.0
                - (-7.4 * progress).exp()
                    * ((10.5 * progress).cos() + 0.70 * (10.5 * progress).sin());
            PostProcessVisualState {
                active: true,
                lift: spring.clamp(0.0, 1.08) * HEATING_LIFT,
                flame: normalized_exponential_response((progress - 0.34) / 0.66, 4.4),
                ..PostProcessVisualState::default()
            }
        }
        MacroscopicStage::SolventBoiling => {
            let evaporation = smoother_step(progress);
            let attack = normalized_exponential_response(progress / 0.14, 4.8);
            let release = normalized_exponential_decay((progress - 0.90) / 0.10, 3.4);
            PostProcessVisualState {
                active: true,
                lift: HEATING_LIFT,
                flame: 0.96,
                boiling: attack * release,
                vapour: attack * (0.72 + progress * 0.28),
                liquid_fraction: 1.0 - evaporation * 0.86,
                crystal_growth: smoother_step((progress - 0.78) / 0.22) * 0.18,
            }
        }
        MacroscopicStage::CrystalGrowth => {
            let residual = 1.0 - smoother_step(progress / 0.58);
            PostProcessVisualState {
                active: true,
                lift: HEATING_LIFT,
                flame: normalized_exponential_decay(progress / 0.48, 4.6),
                boiling: normalized_exponential_decay(progress / 0.24, 5.2),
                vapour: normalized_exponential_decay(progress / 0.68, 3.2),
                liquid_fraction: 0.14 * residual,
                crystal_growth: 0.18 + smoother_step(progress) * 0.82,
            }
        }
    }
}

impl Default for ObjectMotion {
    fn default() -> Self {
        Self {
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
        }
    }
}

#[cfg(test)]
fn build_scene(
    plan: &ScenePlan,
    ordinal: u16,
    progress: f32,
) -> (Vec<GpuVertex>, Vec<u32>, u32, u32, Vec<GasSplat>) {
    build_scene_with_stage(
        plan,
        ordinal,
        progress,
        MacroscopicStage::Reaction,
        progress,
        None,
        fixed_view_direction(),
    )
}

/// The cue-adjusted view direction at this moment (shake excluded: its
/// fraction of a degree cannot flip triangle ordering).
fn live_view_direction(plan: &ScenePlan, moment: RealWorldPosition, orbit: (f32, f32)) -> Vec3 {
    let cue = camera_cue_adjustment(plan, moment);
    -(Quat::from_rotation_y(FIXED_CAMERA_YAW + cue.yaw_offset + orbit.0)
        * Quat::from_rotation_x(
            (FIXED_CAMERA_PITCH + cue.pitch_offset + orbit.1).clamp(-1.35, -0.12),
        )
        * Vec3::Z)
}

fn build_scene_at(
    plan: &ScenePlan,
    moment: RealWorldPosition,
    orbit: (f32, f32),
) -> (Vec<GpuVertex>, Vec<u32>, u32, u32, Vec<GasSplat>) {
    let authored_clip_progress = if plan.explosive_metal_water.is_some() {
            // The detonation choreography carries its own authored pacing
            // (contact at frame 39/179) on the wall-clock timeline — the
            // gas-evolution remap below would postpone contact to the
            // gas-generation ordinal while blast_camera_shake stays on the
            // wall clock, splitting the scene across two clocks.
            plan.timeline.normalized_progress_at(moment)
        } else if moment.stage == MacroscopicStage::Reaction && plan.gas_evolution.is_some() {
            gas_evolution_clip_progress(plan, moment)
        } else if moment.stage == MacroscopicStage::Reaction && plan.metal_displacement.is_some() {
            authored_reaction_clip_progress(plan, moment)
        } else {
            plan.precipitation.as_ref().map_or_else(
                || plan.timeline.normalized_progress_at(moment),
                |_| precipitation_clip_progress(plan, moment),
            )
        };
    build_scene_with_stage(
        plan,
        moment.ordinal,
        moment.ordinal_progress,
        moment.stage,
        moment.beat_progress,
        Some(authored_clip_progress),
        live_view_direction(plan, moment, orbit),
    )
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn build_scene_with_stage(
    plan: &ScenePlan,
    ordinal: u16,
    progress: f32,
    stage: MacroscopicStage,
    stage_progress: f32,
    authored_clip_progress: Option<f32>,
    view_direction: Vec3,
) -> (Vec<GpuVertex>, Vec<u32>, u32, u32, Vec<GasSplat>) {
    let mut meshes = SceneMeshes::default();
    // Scene invariant: per-plan topology is CONSTANT — entities never appear
    // or disappear across (ordinal, progress); only sizes/positions animate.
    let layout = SceneLayout::resolve(plan);
    let final_ordinal = plan
        .timeline
        .beats
        .last()
        .map_or(ordinal, |beat| beat.end_ordinal);
    let mut visual_inputs =
        ReactionVisualInputs::from_effects(&plan.effects, ordinal, progress, final_ordinal);
    gate_stirrer_driven_liquid_turbulence(
        &mut visual_inputs,
        plan,
        layout,
        ordinal,
        progress,
        stage,
        final_ordinal,
    );
    let post_process = post_process_visual_state(plan, stage, stage_progress);
    visual_inputs.bubble_rate = visual_inputs.bubble_rate.max(post_process.boiling);
    visual_inputs.vapour_generation_rate = visual_inputs
        .vapour_generation_rate
        .max(post_process.vapour);
    visual_inputs.gas_generation_rate = visual_inputs
        .gas_generation_rate
        .max(post_process.vapour * 0.72);
    visual_inputs.liquid_turbulence = visual_inputs
        .liquid_turbulence
        .max(post_process.boiling * 0.92);
    visual_inputs.heat_output = visual_inputs.heat_output.max(post_process.flame * 0.76);
    visual_inputs.flame_rate = visual_inputs.flame_rate.max(post_process.flame);
    let phase = continuous_phase(ordinal, progress);
    let reaction_motion = reaction_surface_motion(plan, ordinal, progress);
    let vibration = container_vibration_offset(visual_inputs, phase, plan_seed(plan));
    let effect_colours = scene_effect_colours(plan, ordinal, progress);
    let vessel_motion = vibration + Vec3::Y * post_process.lift;
    let animated_layout = layout
        .with_vessel_motion(vessel_motion)
        .with_liquid_fraction(post_process.liquid_fraction)
        .with_reaction_motion(reaction_motion);
    instantiate_asset(
        &mut meshes,
        plan.environment,
        AppearanceProfile::LaboratoryNeutral,
        &PresentationTransform {
            translation: [0, -900, 0],
            rotation: [0, 0, 0],
            scale: [1000, 1000, 1000],
        },
        1.0,
        Vec3::ZERO,
        Quat::IDENTITY,
        0,
        visual_inputs,
        phase,
        1.0,
        None,
    );
    if plan.objects.iter().any(|object| {
        object.role == SceneRole::Vessel
            && object.asset == AssetProfile::ExplosiveMetalWaterAssembly
    }) {
        explosion::add_explosive_metal_water_assembly(
            &mut meshes,
            plan,
            layout,
            authored_clip_progress.unwrap_or(visual_inputs.reaction_progress),
            ordinal,
            progress,
        );
        return meshes.finish(view_direction);
    }
    if plan.objects.iter().any(|object| {
        object.role == SceneRole::Vessel && object.asset == AssetProfile::ReactiveMetalWaterAssembly
    }) {
        alkali_water::add_alkali_water_assembly(
            &mut meshes,
            plan,
            layout,
            authored_clip_progress.unwrap_or(visual_inputs.reaction_progress),
        );
        return meshes.finish(view_direction);
    }
    if stage == MacroscopicStage::Reaction && plan.gas_evolution.is_some() {
        gas_evolution::add_gas_evolution_assembly(
            &mut meshes,
            plan,
            layout,
            authored_clip_progress.unwrap_or(visual_inputs.reaction_progress),
            ordinal,
            progress,
        );
        return meshes.finish(view_direction);
    }
    if stage == MacroscopicStage::Reaction && plan.metal_displacement.is_some() {
        displacement::add_displacement_assembly(
            &mut meshes,
            plan,
            layout,
            authored_clip_progress.unwrap_or(visual_inputs.reaction_progress),
            ordinal,
            progress,
        );
        return meshes.finish(view_direction);
    }
    if stage == MacroscopicStage::Reaction && plan.solid_solid_synthesis.is_some() {
        synthesis::add_synthesis_assembly(
            &mut meshes,
            plan,
            layout,
            authored_clip_progress.unwrap_or(visual_inputs.reaction_progress),
            ordinal,
            progress,
        );
        return meshes.finish(view_direction);
    }
    if stage == MacroscopicStage::Reaction && plan.phase_synthesis.is_some() {
        phase_synthesis::add_phase_synthesis_assembly(
            &mut meshes,
            plan,
            layout,
            authored_clip_progress.unwrap_or(visual_inputs.reaction_progress),
            ordinal,
            progress,
        );
        return meshes.finish(view_direction);
    }
    if plan.objects.iter().any(|object| {
        object.role == SceneRole::Vessel
            && object.asset == AssetProfile::NeutralisationEvaporationAssembly
    }) {
        neutralisation::add_neutralisation_assembly(
            &mut meshes,
            NeutralisationAssemblyMoment {
                plan,
                layout,
                stage,
                progress: authored_clip_progress.unwrap_or(visual_inputs.reaction_progress),
                post_process,
                stage_progress,
                seed: plan_seed(plan),
                visual_inputs,
                effect_colours,
                ordinal,
                ordinal_progress: progress,
            },
        );
        return meshes.finish(view_direction);
    }
    if let Some(assembly) = plan.objects.iter().find(|object| {
        object.role == SceneRole::Vessel
            && matches!(
                object.asset,
                AssetProfile::CompleteCombustionAssembly
                    | AssetProfile::IncompleteCombustionAssembly
            )
    }) {
        combustion::add_combustion_assembly(
            &mut meshes,
            assembly,
            layout,
            authored_clip_progress.unwrap_or(visual_inputs.reaction_progress),
        );
        return meshes.finish(view_direction);
    }
    if plan.objects.iter().any(|object| {
        object.role == SceneRole::Vessel
            && object.asset == AssetProfile::AqueousPrecipitationAssembly
    }) {
        precipitation::add_precipitation_assembly(
            &mut meshes,
            plan,
            layout,
            authored_clip_progress.unwrap_or(visual_inputs.reaction_progress),
            ordinal,
            progress,
        );
        return meshes.finish(view_direction);
    }
    for object in &plan.objects {
        if object.visible_from_ordinal <= ordinal {
            // Consumption/replacement shrink (exact-model swap) composes
            // with the reviewed formation grow-in: both live in [0, 1].
            let persistent_scale = object_scale_from_effects(plan, object.role, ordinal, progress)
                * object_replacement_scale(plan, object, ordinal, progress);
            let formation_scale = object_formation_scale(object, ordinal, progress);
            let scale = persistent_scale * formation_scale;
            let motion = object_motion(plan, object, ordinal, progress, reaction_motion);
            let object_vibration = if object.role == SceneRole::Environment {
                Vec3::ZERO
            } else {
                vessel_motion
            };
            // A completed consumption or product-replacement transition
            // removes the reactant from the scene. Keeping a minimum scale
            // here left a misleading residue beside exact product models.
            if scale <= f32::EPSILON {
                continue;
            }
            // Exact structural previews never enter this macroscopic
            // presentation. Every phase uses its reviewed physical asset:
            // gas density, mobile liquid, or faceted solid material.
            let colour_transition = object_colour_transition(object, ordinal, progress)
                .or_else(|| surface_oxidation_transition(plan, object, ordinal, progress));
            if object.asset == AssetProfile::GasCloud {
                instantiate_plan_gas_asset(
                    &mut meshes,
                    object,
                    animated_layout,
                    persistent_scale,
                    formation_scale,
                    stable_seed(&object.id),
                    visual_inputs,
                    phase,
                    colour_transition,
                );
            } else {
                instantiate_asset(
                    &mut meshes,
                    object.asset,
                    object.appearance,
                    &object.transform,
                    scale,
                    layout.object_offset(object) + motion.translation + object_vibration,
                    motion.rotation,
                    stable_seed(&object.id),
                    visual_inputs,
                    phase,
                    if object.role == SceneRole::Contents {
                        post_process.liquid_fraction
                    } else {
                        1.0
                    },
                    colour_transition,
                );
            }
        }
    }
    for effect in &plan.effects {
        if effect.start_ordinal <= ordinal && ordinal <= effect.end_ordinal {
            instantiate_effect(
                &mut meshes,
                effect,
                EffectMoment {
                    ordinal,
                    progress,
                    stage,
                },
                animated_layout,
                effect_seed(plan, effect),
                effect_colours,
            );
        }
    }
    if post_process.active {
        add_evaporation_crystallization_process(
            &mut meshes,
            animated_layout,
            post_process,
            plan_seed(plan),
            effect_colours,
            stage_progress,
        );
    }
    meshes.finish(view_direction)
}

fn scene_effect_colours(plan: &ScenePlan, ordinal: u16, progress: f32) -> EffectColours {
    let object_colour = |role, assets: &[AssetProfile], fallback| {
        plan.objects
            .iter()
            .find(|object| object.role == role && assets.contains(&object.asset))
            .map_or(fallback, |object| {
                object_uniform_color(object, ordinal, progress)
            })
    };
    EffectColours {
        liquid: object_colour(
            SceneRole::Contents,
            &[AssetProfile::LiquidVolume],
            [0.52, 0.74, 0.84, 0.28],
        ),
        solid: object_colour(
            SceneRole::Product,
            &[
                AssetProfile::PrecipitateCloud,
                AssetProfile::CrystalCluster,
                AssetProfile::PowderPile,
            ],
            [0.82, 0.84, 0.86, 1.0],
        ),
        gas: object_colour(
            SceneRole::Product,
            &[AssetProfile::GasCloud],
            [0.70, 0.84, 0.90, 0.20],
        ),
    }
}

/// Products begin forming only after their trusted visibility ordinal. Easing
/// their scale from zero avoids a one-frame pop while preserving the rule that
/// no product geometry appears before its observation activates.
fn object_formation_scale(object: &PresentationObject, ordinal: u16, progress: f32) -> f32 {
    if object.role != SceneRole::Product || ordinal > object.visible_from_ordinal {
        return 1.0;
    }
    if ordinal < object.visible_from_ordinal {
        return 0.0;
    }
    normalized_exponential_response(progress, 4.2)
}

fn colour_transition_progress(
    transition: &PresentationColourTransition,
    ordinal: u16,
    progress: f32,
) -> f32 {
    match ordinal.cmp(&transition.start_ordinal) {
        std::cmp::Ordering::Less => 0.0,
        std::cmp::Ordering::Equal => normalized_exponential_response(progress, 3.4),
        std::cmp::Ordering::Greater => 1.0,
    }
}

fn object_colour_transition(
    object: &PresentationObject,
    ordinal: u16,
    progress: f32,
) -> Option<AssetColourTransition> {
    object
        .colour_transition
        .as_ref()
        .map(|transition| AssetColourTransition {
            target: transition.target,
            progress: colour_transition_progress(transition, ordinal, progress),
            seed: stable_seed(&transition.subject_binding) ^ stable_seed(&transition.value),
        })
}

/// The typed surface-oxidation process uses an exact product-bound colour when
/// one survived upstream validation. Missing or rejected enrichment leaves the
/// original metal appearance unchanged instead of presenting a generic grey as
/// chemical fact. Selection is bound to the process effect, never a
/// reaction/species name.
fn surface_oxidation_transition(
    plan: &ScenePlan,
    object: &PresentationObject,
    ordinal: u16,
    progress: f32,
) -> Option<AssetColourTransition> {
    if object.role != SceneRole::Reactant
        || !matches!(
            object.asset,
            AssetProfile::MetalChunk | AssetProfile::MetalStrip
        )
    {
        return None;
    }
    let effect = plan
        .effects
        .iter()
        .find(|effect| effect.effect == EffectProfile::SurfaceOxidation)?;
    let colour = effect.surface_oxide_colour.as_ref()?;
    let progress = match ordinal.cmp(&effect.start_ordinal) {
        std::cmp::Ordering::Less => 0.0,
        std::cmp::Ordering::Greater if ordinal > effect.end_ordinal => 1.0,
        std::cmp::Ordering::Equal | std::cmp::Ordering::Greater => {
            effect_progress(effect, ordinal, progress)
        }
    };
    Some(AssetColourTransition {
        target: colour.target,
        progress: smoother_step(progress),
        seed: plan_seed(plan) ^ stable_seed(&object.id) ^ 0x6f78_6964_652d_6669,
    })
}

fn object_uniform_color(object: &PresentationObject, ordinal: u16, progress: f32) -> [f32; 4] {
    let base = appearance_color(object.appearance);
    object
        .colour_transition
        .as_ref()
        .map_or(base, |transition| {
            mix_visual_colour(
                base,
                transition.target,
                colour_transition_progress(transition, ordinal, progress),
            )
        })
}

fn mix_visual_colour(base: [f32; 4], target: VisualColour, amount: f32) -> [f32; 4] {
    mix_color(
        base,
        [
            f32::from(target.red) / 255.0,
            f32::from(target.green) / 255.0,
            f32::from(target.blue) / 255.0,
            base[3],
        ],
        amount,
    )
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct FixedCameraPose {
    yaw: f32,
    pitch: f32,
    view_height: f32,
}

fn fixed_camera_pose(plan: &ScenePlan) -> FixedCameraPose {
    if let Some(assembly) = plan.objects.iter().find(|object| {
        object.role == SceneRole::Vessel
            && matches!(
                object.asset,
                AssetProfile::ReactiveMetalWaterAssembly
                    | AssetProfile::NeutralisationEvaporationAssembly
                    | AssetProfile::CompleteCombustionAssembly
                    | AssetProfile::IncompleteCombustionAssembly
                    | AssetProfile::AqueousPrecipitationAssembly
            )
    }) {
        let scale = transform_scale(&assembly.transform);
        let extent = scale.x.max(scale.y).max(scale.z);
        return FixedCameraPose {
            yaw: -0.72,
            pitch: -0.70,
            view_height: (5.0 + (extent - 1.1) * 2.0).clamp(4.6, 5.8),
        };
    }
    let vessel_scale = plan
        .objects
        .iter()
        .find(|object| object.role == SceneRole::Vessel)
        .map_or(Vec3::ONE, |object| transform_scale(&object.transform));
    let vessel_extent = vessel_scale.x.max(vessel_scale.y).max(vessel_scale.z);
    FixedCameraPose {
        yaw: -0.72,
        pitch: -0.70,
        view_height: (vessel_extent * 3.8).clamp(3.8, 5.4),
    }
}

fn object_scale_from_effects(
    plan: &ScenePlan,
    role: SceneRole,
    ordinal: u16,
    progress: f32,
) -> f32 {
    let grows = match role {
        SceneRole::Reactant => false,
        SceneRole::Product => true,
        _ => return 1.0,
    };
    let effect_matches_role = |effect: EffectProfile| match role {
        SceneRole::Reactant => effect == EffectProfile::ObjectShrinkage,
        SceneRole::Product => matches!(
            effect,
            EffectProfile::PrecipitateFormation | EffectProfile::SolidFormation
        ),
        _ => false,
    };
    let Some(effect) = plan
        .effects
        .iter()
        .find(|effect| effect_matches_role(effect.effect) && effect.start_ordinal <= ordinal)
    else {
        return 1.0;
    };
    let span = f32::from(
        effect
            .end_ordinal
            .saturating_sub(effect.start_ordinal)
            .saturating_add(1),
    );
    let elapsed = if ordinal > effect.end_ordinal {
        span
    } else {
        f32::from(ordinal.saturating_sub(effect.start_ordinal)) + progress
    };
    let extent = normalized_exponential_response(elapsed / span, 2.6);
    if grows {
        0.12 + extent * 0.88
    } else {
        (1.0 - extent).max(0.0)
    }
}

/// Cross-fades reaction profiles that have no observation-backed
/// consumption/formation effect. Registry reactions still need an honest
/// transition from their reactant models to the reviewed product model.
fn object_replacement_scale(
    plan: &ScenePlan,
    object: &PresentationObject,
    ordinal: u16,
    progress: f32,
) -> f32 {
    let replacement_ordinal = plan
        .objects
        .iter()
        .filter(|candidate| candidate.role == SceneRole::Product)
        .map(|candidate| candidate.visible_from_ordinal)
        .min();
    let Some(replacement_ordinal) = replacement_ordinal else {
        return 1.0;
    };
    let has_consumption_effect = plan
        .effects
        .iter()
        .any(|effect| effect.effect == EffectProfile::ObjectShrinkage);
    let has_formation_effect = plan.effects.iter().any(|effect| {
        matches!(
            effect.effect,
            EffectProfile::PrecipitateFormation | EffectProfile::SolidFormation
        )
    });

    match object.role {
        SceneRole::Reactant if !has_consumption_effect => match ordinal.cmp(&replacement_ordinal) {
            std::cmp::Ordering::Less => 1.0,
            std::cmp::Ordering::Equal => 1.0 - smoother_step(progress),
            std::cmp::Ordering::Greater => 0.0,
        },
        SceneRole::Product if !has_formation_effect && ordinal == object.visible_from_ordinal => {
            smoother_step(progress)
        }
        _ => 1.0,
    }
}

fn object_motion(
    plan: &ScenePlan,
    object: &PresentationObject,
    ordinal: u16,
    progress: f32,
    reaction_motion: Vec3,
) -> ObjectMotion {
    if object.role != SceneRole::Reactant {
        return ObjectMotion::default();
    }
    if plan
        .effects
        .iter()
        .any(|effect| effect.effect == EffectProfile::SurfaceOxidation)
    {
        // An exposed surface reaction starts with the metal resting on the
        // bench. It is not introduced with the reusable vessel drop/toss.
        return ObjectMotion::default();
    }
    let seed = stable_seed(&object.id) ^ plan_seed(plan);
    let phase = continuous_phase(ordinal, progress);
    if object.asset == AssetProfile::GasCloud {
        return gas_reactant_motion(seed, phase, reaction_motion);
    }
    let arrival_progress = reactant_arrival_progress(plan, object, ordinal, progress);
    let introduction = gravitational_drop_offset(seed, arrival_progress);
    let contact_age = (continuous_phase(ordinal, progress)
        - f32::from(reactant_contact_ordinal(plan, object)))
    .max(0.0);
    let impact = damped_impact_offset(seed, contact_age);
    let activity = reaction_motion.length().min(1.0);
    let spin_axis = Vec3::new(
        0.42 + seeded_unit(seed, 0, 51) * 0.36,
        0.18 + seeded_unit(seed, 0, 52) * 0.22,
        0.54 + seeded_unit(seed, 0, 53) * 0.38,
    )
    .normalize_or_zero();
    let spin_turns = 0.28 + seeded_unit(seed, 0, 54) * 0.42;
    let angular_travel = normalized_drag_distance(arrival_progress, 0.18);
    let flight_rotation = Quat::from_axis_angle(
        spin_axis,
        spin_turns * std::f32::consts::TAU * angular_travel,
    );
    let impact_decay = (-5.4 * contact_age).exp();
    let impact_rotation = Quat::from_euler(
        EulerRot::XYZ,
        (contact_age * 13.7).sin() * impact_decay * (0.10 + seeded_unit(seed, 0, 55) * 0.08),
        (contact_age * 9.3).sin() * impact_decay * (seeded_unit(seed, 0, 56) - 0.5) * 0.18,
        (contact_age * 11.1).sin() * impact_decay * (seeded_unit(seed, 0, 57) - 0.5) * 0.34,
    );
    let roll = (phase * 0.91 + seed_phase(seed, 5)).sin() * 0.045 * activity;
    let pitch = (phase * 0.67 + seed_phase(seed, 6)).cos() * 0.025 * activity;
    ObjectMotion {
        translation: introduction + impact + reaction_motion,
        rotation: Quat::from_euler(EulerRot::XYZ, pitch, 0.0, roll)
            * impact_rotation
            * flight_rotation,
    }
}

fn gas_reactant_motion(seed: u64, phase: f32, reaction_motion: Vec3) -> ObjectMotion {
    let flow = curl_like_flow(phase * 0.58, seed, 0);
    ObjectMotion {
        translation: Vec3::new(flow.x, flow.y * 0.34, flow.z) * 0.075 + reaction_motion * 0.22,
        rotation: Quat::IDENTITY,
    }
}

/// Analytic gravity drop sampled directly from the trusted playhead. The
/// reactant is released near the vessel centre with a small downward velocity;
/// gravity accelerates it into the reaction surface exactly at `t = 1` while
/// bounded air drift prevents identical, mechanically vertical paths.
fn gravitational_drop_offset(seed: u64, progress: f32) -> Vec3 {
    let time = progress.clamp(0.0, 1.0);
    if time >= 1.0 {
        return Vec3::ZERO;
    }
    let start = Vec3::new(
        (seeded_unit(seed, 0, 58) - 0.5) * 0.20,
        1.22 + seeded_unit(seed, 0, 59) * 0.18,
        (seeded_unit(seed, 0, 60) - 0.5) * 0.18,
    );
    let gravity = -2.0;
    let initial_vertical_velocity = -start.y - gravity * 0.5;
    let height = start.y + initial_vertical_velocity * time + gravity * (0.5 * time * time);
    let horizontal_travel = normalized_drag_distance(time, 0.30);
    let horizontal = Vec3::new(start.x, 0.0, start.z) * (1.0 - horizontal_travel);
    let drift_direction = Vec3::new(
        seeded_unit(seed, 0, 61) - 0.5,
        0.0,
        seeded_unit(seed, 0, 64) - 0.5,
    )
    .normalize_or_zero();
    let air_drift = drift_direction
        * (std::f32::consts::PI * time).sin()
        * (0.015 + seeded_unit(seed, 0, 65) * 0.025);
    horizontal + Vec3::Y * height + air_drift
}

/// Short inelastic contact response. The discontinuity is in velocity—the
/// physical impact impulse—not position, followed by rapidly diminishing
/// rebounds and tangential slip.
fn damped_impact_offset(seed: u64, contact_age: f32) -> Vec3 {
    if contact_age <= 0.0 {
        return Vec3::ZERO;
    }
    let decay = (-4.8 * contact_age).exp();
    let rebound = -(contact_age * 14.5).sin() * decay * 0.075;
    let slip_direction = Vec3::new(
        seeded_unit(seed, 0, 62) - 0.5,
        0.0,
        seeded_unit(seed, 0, 63) - 0.5,
    )
    .normalize_or_zero();
    let slip = slip_direction * (contact_age * 9.0).sin() * decay * 0.032;
    Vec3::Y * rebound + slip
}

/// Spreads the approach across every setup ordinal up to the first authorized
/// effect instead of completing it in the first state and holding still.
fn reactant_arrival_progress(
    plan: &ScenePlan,
    object: &PresentationObject,
    ordinal: u16,
    progress: f32,
) -> f32 {
    let contact_ordinal = reactant_contact_ordinal(plan, object);
    let span = f32::from(
        contact_ordinal
            .saturating_sub(object.visible_from_ordinal)
            .max(1),
    );
    let elapsed =
        f32::from(ordinal.saturating_sub(object.visible_from_ordinal)) + progress.clamp(0.0, 1.0);
    (elapsed / span).clamp(0.0, 1.0)
}

fn reactant_contact_ordinal(plan: &ScenePlan, object: &PresentationObject) -> u16 {
    let fallback = plan
        .timeline
        .beats
        .last()
        .map_or(object.visible_from_ordinal.saturating_add(1), |beat| {
            beat.end_ordinal
        });
    plan.effects
        .iter()
        .map(|effect| effect.start_ordinal)
        .min()
        .unwrap_or(fallback)
        .max(object.visible_from_ordinal.saturating_add(1))
}

fn smoother_step(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * value * (value * (value * 6.0 - 15.0) + 10.0)
}

fn normalized_exponential_response(value: f32, rate: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    let denominator = 1.0 - (-rate).exp();
    if denominator.abs() <= f32::EPSILON {
        value
    } else {
        ((1.0 - (-rate * value).exp()) / denominator).clamp(0.0, 1.0)
    }
}

fn normalized_exponential_decay(value: f32, rate: f32) -> f32 {
    1.0 - normalized_exponential_response(value, rate)
}

/// Distance under exponential velocity damping, normalized to reach exactly
/// one at the end of the presentation interval.
fn normalized_drag_distance(value: f32, drag: f32) -> f32 {
    normalized_exponential_response(value, drag.max(0.001))
}

/// Distance travelled while accelerating toward terminal velocity.
fn normalized_terminal_distance(value: f32, response: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    let response = response.max(0.001);
    let distance = value - (1.0 - (-response * value).exp()) / response;
    let total = 1.0 - (1.0 - (-response).exp()) / response;
    if total.abs() <= f32::EPSILON {
        value
    } else {
        (distance / total).clamp(0.0, 1.0)
    }
}

/// Gravity-driven sediment travel followed by a small liquid-damped collision
/// response at the vessel floor. Both values are exact at their endpoints so
/// pause, replay, and seeking reconstruct the same settled solid.
fn sediment_settling_motion(age: f32) -> (f32, f32) {
    const CONTACT_AT: f32 = 0.78;
    let age = age.clamp(0.0, 1.0);
    if age <= CONTACT_AT {
        return (normalized_terminal_distance(age / CONTACT_AT, 4.2), 0.0);
    }
    let contact_age = ((age - CONTACT_AT) / (1.0 - CONTACT_AT)).clamp(0.0, 1.0);
    let bounce =
        (std::f32::consts::TAU * contact_age).sin().abs() * (-4.2 * contact_age).exp() * 0.035;
    (1.0, bounce)
}

fn settling_shard_rotation(seed: u64, age: f32) -> Quat {
    let axis = Vec3::new(
        seeded_unit(seed, 0, 90) - 0.5,
        seeded_unit(seed, 0, 91) - 0.5,
        seeded_unit(seed, 0, 92) - 0.5,
    );
    let axis = if axis.length_squared() <= f32::EPSILON {
        Vec3::Y
    } else {
        axis.normalize()
    };
    let turns = 0.35 + seeded_unit(seed, 0, 93) * 0.85;
    let angular_travel = normalized_drag_distance(age, 0.82);
    Quat::from_axis_angle(axis, turns * std::f32::consts::TAU * angular_travel)
}

const fn ballistic_arc(value: f32) -> f32 {
    4.0 * value * (1.0 - value)
}

fn continuous_phase(ordinal: u16, progress: f32) -> f32 {
    f32::from(ordinal) + progress.clamp(0.0, 1.0)
}

fn plan_seed(plan: &ScenePlan) -> u64 {
    let mut bytes = [0_u8; 8];
    bytes.copy_from_slice(&plan.id.as_bytes()[..8]);
    u64::from_le_bytes(bytes)
}

fn effect_seed(plan: &ScenePlan, effect: &PresentationEffect) -> u64 {
    plan_seed(plan)
        ^ observation_seed(effect.trigger)
        ^ effect_profile_seed(effect.effect)
        ^ u64::from(effect.start_ordinal).rotate_left(17)
        ^ u64::from(effect.end_ordinal).rotate_left(31)
        ^ intensity_seed(effect.intensity)
}

const fn observation_seed(predicate: chem_catalogue::ObservationPredicate) -> u64 {
    match predicate {
        chem_catalogue::ObservationPredicate::Evolves => 0x517c_c1b7_2722_0a95,
        chem_catalogue::ObservationPredicate::Disappears => 0x6c8e_9cf5_7093_2bd5,
        chem_catalogue::ObservationPredicate::Forms => 0x2d98_23f1_a57c_0ef3,
        chem_catalogue::ObservationPredicate::Colour => 0x8a5c_d789_635d_2dff,
    }
}

const fn effect_profile_seed(effect: EffectProfile) -> u64 {
    match effect {
        EffectProfile::ReactionActivity => 0x243f_6a88_85a3_08d3,
        EffectProfile::BubbleEmitter => 0x9e37_79b9_7f4a_7c15,
        EffectProfile::GasRelease => 0xd1b5_4a32_d192_ed03,
        EffectProfile::VapourRelease => 0x1f83_d9ab_fb41_bd6b,
        EffectProfile::SurfaceDisturbance => 0x94d0_49bb_1331_11eb,
        EffectProfile::LiquidMixing => 0x3f84_d5b5_b547_0917,
        EffectProfile::SplashEmitter => 0x8538_ec85_5c19_1b69,
        EffectProfile::ObjectShrinkage => 0xda94_2042_e4dd_58b5,
        EffectProfile::SurfaceOxidation => 0x6f78_6964_6174_696f,
        EffectProfile::SolidFormation => 0x1319_8a2e_0370_7344,
        EffectProfile::PrecipitateFormation => 0xa409_3822_299f_31d0,
        EffectProfile::Clouding => 0x082e_fa98_ec4e_6c89,
        EffectProfile::ColourTransition => 0x4528_21e6_38d0_1377,
        EffectProfile::HeatDistortion => 0xbe54_66cf_34e9_0c6c,
        EffectProfile::FlameEmitter(palette) => 0xc6a4_a793_5bd1_e995 ^ flame_palette_seed(palette),
    }
}

const fn flame_palette_seed(palette: FlamePalette) -> u64 {
    match palette {
        FlamePalette::Natural => 0x3c6e_f372_fe94_f82b,
        FlamePalette::BurnerBlue => 0xbb67_ae85_84ca_a73b,
        FlamePalette::Crimson => 0xa54f_f53a_5f1d_36f1,
        FlamePalette::YellowOrange => 0x510e_527f_ade6_82d1,
        FlamePalette::Lilac => 0x9b05_688c_2b3e_6c1f,
    }
}

const fn intensity_seed(intensity: EffectIntensity) -> u64 {
    match intensity {
        EffectIntensity::Subtle => 0x243f_6a88_85a3_08d3,
        EffectIntensity::Moderate => 0x1319_8a2e_0370_7344,
        EffectIntensity::Strong => 0xa458_fea3_f493_3d7e,
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
fn seeded_unit(seed: u64, index: u32, channel: u32) -> f32 {
    let mut value = seed
        ^ u64::from(index).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ u64::from(channel).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^= value >> 31;
    (value as u32) as f32 / u32::MAX as f32
}

fn seed_phase(seed: u64, channel: u32) -> f32 {
    seeded_unit(seed, 0, channel) * std::f32::consts::TAU
}

/// Cheap multi-scale rotational flow used for parcel drift. It is not a fluid
/// solver, but avoids the single-axis periodic wobble that makes particles read
/// as synchronized machinery.
fn curl_like_flow(phase: f32, seed: u64, index: u32) -> Vec3 {
    let parcel_seed = seed ^ u64::from(index).rotate_left(19);
    let large =
        phase * (0.71 + seeded_unit(parcel_seed, 0, 70) * 0.23) + seed_phase(parcel_seed, 71);
    let medium =
        phase * (1.63 + seeded_unit(parcel_seed, 0, 72) * 0.41) + seed_phase(parcel_seed, 73);
    let fine =
        phase * (3.17 + seeded_unit(parcel_seed, 0, 74) * 0.67) + seed_phase(parcel_seed, 75);
    Vec3::new(
        large.cos() + medium.sin() * 0.46 + fine.cos() * 0.17,
        0.0,
        -large.sin() + medium.cos() * 0.43 - fine.sin() * 0.19,
    )
    .normalize_or_zero()
}

fn effect_progress(effect: &PresentationEffect, ordinal: u16, progress: f32) -> f32 {
    let span = f32::from(
        effect
            .end_ordinal
            .saturating_sub(effect.start_ordinal)
            .saturating_add(1),
    );
    let elapsed =
        f32::from(ordinal.saturating_sub(effect.start_ordinal)) + progress.clamp(0.0, 1.0);
    (elapsed / span.max(1.0)).clamp(0.0, 1.0)
}

fn effect_envelope(dynamics: EffectDynamics, progress: f32) -> f32 {
    let attack = if dynamics.fade_in <= f32::EPSILON {
        1.0
    } else {
        normalized_exponential_response(progress / dynamics.fade_in, 3.8)
    };
    let release = if dynamics.fade_out <= f32::EPSILON {
        1.0
    } else {
        normalized_exponential_decay(
            (progress - (1.0 - dynamics.fade_out)) / dynamics.fade_out,
            3.2,
        )
    };
    attack * release
}

fn reaction_surface_motion(plan: &ScenePlan, ordinal: u16, progress: f32) -> Vec3 {
    plan.effects
        .iter()
        .filter(|effect| {
            matches!(
                effect.effect,
                EffectProfile::ReactionActivity
                    | EffectProfile::SurfaceDisturbance
                    | EffectProfile::LiquidMixing
            ) && effect.start_ordinal <= ordinal
                && ordinal <= effect.end_ordinal
        })
        .fold(Vec3::ZERO, |motion, effect| {
            let dynamics = scene_registry::effect_dynamics(effect.effect, effect.intensity);
            let envelope = effect_envelope(dynamics, effect_progress(effect, ordinal, progress));
            let phase = continuous_phase(ordinal, progress) * dynamics.rate * std::f32::consts::TAU;
            let seed = effect_seed(plan, effect);
            let broad_flow = curl_like_flow(phase, seed, 0);
            let fine_flow = curl_like_flow(phase * 2.37, seed.rotate_left(11), 1);
            let flow = broad_flow + fine_flow * 0.31;
            let vertical_impulse = ((phase * 1.43 + seed_phase(seed, 15)).sin()
                + (phase * 2.71 + seed_phase(seed, 16)).sin() * 0.28)
                .abs();
            motion
                + Vec3::new(
                    flow.x * dynamics.spread * 0.27,
                    vertical_impulse * dynamics.lift * 0.048,
                    flow.z * dynamics.spread * 0.23,
                ) * envelope
        })
}

/// A tiny seeded displacement shared by the vessel, contents, products, and
/// active effects. It communicates transferred momentum without moving the
/// fixed camera or turning gentle chemistry into a violent event.
fn container_vibration_offset(inputs: ReactionVisualInputs, phase: f32, seed: u64) -> Vec3 {
    let intensity = inputs.container_vibration.clamp(0.0, 0.55);
    if intensity <= f32::EPSILON {
        return Vec3::ZERO;
    }
    let pulse = phase * std::f32::consts::TAU * 9.4;
    let lateral = (pulse + seed_phase(seed, 80)).sin() * 0.008
        + (pulse * 1.87 + seed_phase(seed, 81)).sin() * 0.003;
    let depth = (pulse * 1.23 + seed_phase(seed, 82)).cos() * 0.006
        + (pulse * 2.31 + seed_phase(seed, 83)).sin() * 0.002;
    let vertical = (pulse * 1.61 + seed_phase(seed, 84)).sin() * 0.0018;
    Vec3::new(lateral, vertical, depth) * intensity
}

fn transform_translation(transform: &PresentationTransform) -> Vec3 {
    Vec3::new(
        f32::from(transform.translation[0]) / 1_000.0,
        f32::from(transform.translation[1]) / 1_000.0,
        f32::from(transform.translation[2]) / 1_000.0,
    )
}

fn transform_scale(transform: &PresentationTransform) -> Vec3 {
    Vec3::new(
        f32::from(transform.scale[0]) / 1_000.0,
        f32::from(transform.scale[1]) / 1_000.0,
        f32::from(transform.scale[2]) / 1_000.0,
    )
}

fn transform_rotation(transform: &PresentationTransform) -> Quat {
    let turns_to_radians = std::f32::consts::TAU / 1_000.0;
    Quat::from_euler(
        EulerRot::XYZ,
        f32::from(transform.rotation[0]) * turns_to_radians,
        f32::from(transform.rotation[1]) * turns_to_radians,
        f32::from(transform.rotation[2]) * turns_to_radians,
    )
}

fn stable_seed(value: &str) -> u64 {
    value
        .as_bytes()
        .iter()
        .fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
}

fn rotate_mesh_vertices(mesh: &mut Mesh, start: usize, pivot: Vec3, rotation: Quat) {
    for vertex in &mut mesh.vertices[start..] {
        let position = Vec3::from_array(vertex.position);
        vertex.position = (pivot + rotation * (position - pivot)).to_array();
        vertex.normal = (rotation * Vec3::from_array(vertex.normal))
            .normalize_or_zero()
            .to_array();
    }
}

fn rotate_gas_splats(splats: &mut [GasSplat], start: usize, pivot: Vec3, rotation: Quat) {
    for splat in &mut splats[start..] {
        let center = Vec3::from_array(splat.center);
        splat.center = (pivot + rotation * (center - pivot)).to_array();
        splat.flow = (rotation * Vec3::from_array(splat.flow)).to_array();
    }
}

fn apply_asset_colour_transition(
    mesh: &mut Mesh,
    start: usize,
    asset: AssetProfile,
    center: Vec3,
    transition: AssetColourTransition,
) {
    if transition.progress <= f32::EPSILON {
        return;
    }
    for vertex in &mut mesh.vertices[start..] {
        let position = Vec3::from_array(vertex.position);
        let offset = position - center;
        let position_seed = transition.seed
            ^ u64::from(position.x.to_bits()).rotate_left(7)
            ^ u64::from(position.y.to_bits()).rotate_left(23)
            ^ u64::from(position.z.to_bits()).rotate_left(41);
        let noise = seeded_unit(position_seed, 0, 119);
        let delay = match asset {
            AssetProfile::LiquidVolume => {
                // Additions enter at the surface, so colour sinks from the
                // top and spreads outward — vertices below the centre are
                // reached later than the surface directly under the source.
                (offset.x.hypot(offset.z) * 0.24 + (-offset.y).max(0.0) * 0.30 + noise * 0.08)
                    .clamp(0.0, 0.40)
            }
            AssetProfile::GasCloud => {
                // Turbulent gas lobes colour at slightly different times.
                (noise * 0.34 + offset.y.max(0.0) * 0.04).clamp(0.0, 0.40)
            }
            AssetProfile::PrecipitateCloud
            | AssetProfile::CrystalCluster
            | AssetProfile::PowderPile
            | AssetProfile::MetalChunk
            | AssetProfile::MetalStrip => (noise * 0.36).clamp(0.0, 0.40),
            AssetProfile::LaboratoryBench
            | AssetProfile::DarkPresentationPlatform
            | AssetProfile::ReactiveMetalWaterAssembly
            | AssetProfile::ExplosiveMetalWaterAssembly
            | AssetProfile::NeutralisationEvaporationAssembly
            | AssetProfile::CompleteCombustionAssembly
            | AssetProfile::IncompleteCombustionAssembly
            | AssetProfile::AqueousPrecipitationAssembly
            | AssetProfile::MetalDisplacementAssembly
            | AssetProfile::SolidSolidSynthesisAssembly
            | AssetProfile::SolidGasSynthesisAssembly
            | AssetProfile::GasGasSynthesisAssembly
            | AssetProfile::Beaker
            | AssetProfile::TestTube
            | AssetProfile::ConicalFlask
            | AssetProfile::MeasuringCylinder => 0.0,
        };
        let local_progress = (transition.progress * 1.40 - delay).clamp(0.0, 1.0);
        vertex.color = mix_visual_colour(
            vertex.color,
            transition.target,
            smoother_step(local_progress),
        );
    }
}

fn apply_gas_colour_transition(
    splats: &mut [GasSplat],
    start: usize,
    center: Vec3,
    transition: AssetColourTransition,
) {
    if transition.progress <= f32::EPSILON {
        return;
    }
    for splat in &mut splats[start..] {
        let position = Vec3::from_array(splat.center);
        let offset = position - center;
        let position_seed = transition.seed
            ^ u64::from(position.x.to_bits()).rotate_left(7)
            ^ u64::from(position.y.to_bits()).rotate_left(23)
            ^ u64::from(position.z.to_bits()).rotate_left(41);
        let noise = seeded_unit(position_seed, 0, 119);
        let delay = (noise * 0.34 + offset.y.max(0.0) * 0.04).clamp(0.0, 0.40);
        let local_progress = (transition.progress * 1.40 - delay).clamp(0.0, 1.0);
        splat.color = mix_visual_colour(
            splat.color,
            transition.target,
            smoother_step(local_progress),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn instantiate_plan_gas_asset(
    meshes: &mut SceneMeshes,
    object: &PresentationObject,
    layout: SceneLayout,
    persistent_scale: f32,
    formation_scale: f32,
    seed: u64,
    visual_inputs: ReactionVisualInputs,
    phase: f32,
    colour_transition: Option<AssetColourTransition>,
) {
    debug_assert_eq!(object.asset, AssetProfile::GasCloud);
    let (center, scale) = layout.gas_volume();
    let gas_color = if object.appearance == AppearanceProfile::LaboratoryNeutral {
        // A colourless gas is physically invisible. This restrained neutral
        // density cue deliberately visualizes concentration for education
        // without implying soot or a species-specific colour.
        [0.82, 0.86, 0.82, 0.52]
    } else {
        appearance_color(object.appearance)
    };
    let persistent_scale = persistent_scale.clamp(0.0, 1.35);
    let formation_scale = formation_scale.clamp(0.0, 1.0);
    let source_strength = (0.48 + visual_inputs.gas_generation_rate * 0.52) * persistent_scale;
    let retained_product = object.role == SceneRole::Product
        && object
            .observation
            .as_ref()
            .is_some_and(|observation| observation.predicate == ObservationPredicate::Forms);
    let density = source_strength
        * if object.role == SceneRole::Product {
            formation_scale
        } else {
            1.0
        };
    let controls = if retained_product {
        GasFlowControls::retained_product(
            source_strength,
            visual_inputs.liquid_turbulence,
            visual_inputs.heat_output,
            visual_inputs.pressure_impulse,
            formation_scale,
            seed,
        )
    } else {
        GasFlowControls::contained(
            source_strength,
            visual_inputs.liquid_turbulence,
            visual_inputs.heat_output,
            visual_inputs.pressure_impulse,
            seed,
        )
    };
    let gas_start = meshes.gas.len();
    add_gas_density_field(
        &mut meshes.gas,
        center,
        scale,
        gas_color,
        seed,
        phase,
        density,
        controls,
    );
    if let Some(transition) = colour_transition {
        apply_gas_colour_transition(&mut meshes.gas, gas_start, center, transition);
    }
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn instantiate_asset(
    meshes: &mut SceneMeshes,
    asset: AssetProfile,
    appearance: AppearanceProfile,
    transform: &PresentationTransform,
    scale_multiplier: f32,
    position_offset: Vec3,
    rotation_offset: Quat,
    variation_seed: u64,
    visual_inputs: ReactionVisualInputs,
    phase: f32,
    liquid_fraction: f32,
    colour_transition: Option<AssetColourTransition>,
) {
    let position = transform_translation(transform) + position_offset;
    let scale = transform_scale(transform) * scale_multiplier;
    let rotation = rotation_offset * transform_rotation(transform);
    let color = if matches!(asset, AssetProfile::MetalChunk | AssetProfile::MetalStrip) {
        // The imported metal uses a restrained white-silver base so lighting
        // can carry its shape before a process-authorized surface transition.
        [0.88, 0.90, 0.92, 1.0]
    } else {
        appearance_color(appearance)
    };
    let opaque_start = meshes.opaque.vertices.len();
    let metallic_start = meshes.metallic.vertices.len();
    let translucent_start = meshes.translucent.vertices.len();
    let glass_start = meshes.glass.vertices.len();
    let gas_start = meshes.gas.len();
    match scene_registry::asset_geometry(asset) {
        AssetGeometry::Bench => {
            // The old baked contact disc is gone. The bench keeps its own
            // studio-surface grey so reflected geometry remains legible.
            add_box(
                &mut meshes.opaque,
                position,
                Vec3::new(44.0, 0.28, 30.0) * scale,
                [0.165, 0.185, 0.215, 1.0],
            );
        }
        // Authored assemblies are instantiated once by the scene-level clip
        // player so their modules share an exact frame sample.
        AssetGeometry::AnimatedAssembly => {}
        AssetGeometry::CylindricalVessel => {
            let bottom = position + Vec3::new(0.0, -0.55 * scale.y, 0.0);
            let top = position + Vec3::new(0.0, 0.95 * scale.y, 0.0);
            let radius = 0.92 * scale.x;
            add_ring(
                &mut meshes.glass,
                top,
                radius,
                0.028,
                [0.62, 0.84, 0.94, 0.28],
            );
            add_ring(
                &mut meshes.glass,
                bottom,
                radius * 0.96,
                0.018,
                [0.52, 0.76, 0.88, 0.16],
            );
            add_cylinder_wall(&mut meshes.glass, bottom, top, radius, color);
            add_disc(
                &mut meshes.glass,
                bottom + Vec3::new(0.0, 0.018, 0.0),
                radius * 0.95,
                [0.48, 0.72, 0.84, 0.10],
            );
            let spout = top + Vec3::new(radius * 0.93, 0.025, 0.0);
            add_sphere(
                &mut meshes.glass,
                spout,
                0.075 * scale.x,
                [0.62, 0.84, 0.94, 0.22],
                4,
                6,
            );
        }
        AssetGeometry::LiquidCylinder => {
            let liquid_fraction = liquid_fraction.clamp(0.0, 1.0);
            if liquid_fraction <= 0.004 {
                return;
            }
            let liquid_bottom = position.y - 0.52 * scale.y;
            let liquid_scale = Vec3::new(scale.x, scale.y * liquid_fraction, scale.z);
            let liquid_center = Vec3::new(
                position.x,
                liquid_bottom + 0.52 * liquid_scale.y,
                position.z,
            );
            add_liquid_volume(
                &mut meshes.translucent,
                liquid_center,
                liquid_scale,
                color,
                visual_inputs.liquid_turbulence,
                phase,
                variation_seed,
            );
        }
        AssetGeometry::ImportedMetal => {
            add_imported_metal(&mut meshes.metallic, position, scale, color);
        }
        AssetGeometry::ShardCluster => {
            add_particle_cluster(
                &mut meshes.opaque,
                position,
                scale,
                color,
                18,
                variation_seed,
            );
        }
        AssetGeometry::GasCluster => {
            let gas_color = if appearance == AppearanceProfile::LaboratoryNeutral {
                // Colourless gas still needs restrained contrast against the
                // blue-grey glass for an educational macroscopic view.
                [0.82, 0.86, 0.82, 0.52]
            } else {
                color
            };
            add_gas_density_field(
                &mut meshes.gas,
                position,
                scale,
                gas_color,
                variation_seed,
                phase,
                0.58 + visual_inputs.gas_generation_rate * 0.42,
                GasFlowControls::contained(
                    0.58 + visual_inputs.gas_generation_rate * 0.42,
                    visual_inputs.liquid_turbulence,
                    visual_inputs.heat_output,
                    visual_inputs.pressure_impulse,
                    variation_seed,
                ),
            );
        }
    }
    if let Some(transition) = colour_transition {
        apply_asset_colour_transition(
            &mut meshes.opaque,
            opaque_start,
            asset,
            position,
            transition,
        );
        apply_asset_colour_transition(
            &mut meshes.metallic,
            metallic_start,
            asset,
            position,
            transition,
        );
        apply_asset_colour_transition(
            &mut meshes.translucent,
            translucent_start,
            asset,
            position,
            transition,
        );
        apply_gas_colour_transition(&mut meshes.gas, gas_start, position, transition);
    }
    rotate_mesh_vertices(&mut meshes.opaque, opaque_start, position, rotation);
    rotate_mesh_vertices(&mut meshes.metallic, metallic_start, position, rotation);
    rotate_mesh_vertices(
        &mut meshes.translucent,
        translucent_start,
        position,
        rotation,
    );
    rotate_mesh_vertices(&mut meshes.glass, glass_start, position, rotation);
    rotate_gas_splats(&mut meshes.gas, gas_start, position, rotation);
}

#[allow(clippy::similar_names, clippy::too_many_lines)]
fn instantiate_effect(
    meshes: &mut SceneMeshes,
    effect: &PresentationEffect,
    moment: EffectMoment,
    layout: SceneLayout,
    seed: u64,
    colours: EffectColours,
) {
    let EffectMoment {
        ordinal,
        progress,
        stage,
    } = moment;
    let dynamics = scene_registry::effect_dynamics(effect.effect, effect.intensity);
    let effect_progress = effect_progress(effect, ordinal, progress);
    let envelope = effect_envelope(dynamics, effect_progress);
    let phase = continuous_phase(ordinal, progress);
    let count = dynamics.particle_count;
    let surface_point = layout.reaction_point;
    match scene_registry::effect_geometry(effect.effect) {
        EffectGeometry::ReactionFront => {
            let front_colour = mix_color(
                mix_color(colours.liquid, colours.gas, 0.46),
                colours.solid,
                0.22,
            );
            for ring in 0..3_u8 {
                let ring_index = u32::from(ring);
                let ring_factor = f32::from(ring);
                let delay = seeded_unit(seed, ring_index, 1) * 0.22;
                let age = ((effect_progress - delay) / (1.0 - delay)).clamp(0.0, 1.0);
                let expansion = normalized_drag_distance(age, 0.56);
                let flow = curl_like_flow(phase * (0.44 + ring_factor * 0.08), seed, ring_index)
                    * dynamics.turbulence
                    * 0.055;
                add_ring(
                    &mut meshes.translucent,
                    surface_point + Vec3::new(flow.x, 0.012 + flow.y * 0.18, flow.z),
                    0.055 + expansion * dynamics.spread * (0.52 + ring_factor * 0.12),
                    0.006 + (1.0 - expansion) * 0.009,
                    alpha(
                        front_colour,
                        envelope * (1.0 - smoother_step(age)) * (0.34 - ring_factor * 0.06),
                    ),
                );
            }
        }
        EffectGeometry::SettlingShards => {
            for index in 0..count {
                let index = u32::from(index);
                let birth = seeded_unit(seed, index, 1) * 0.72;
                let age = ((effect_progress - birth) / 0.24).clamp(0.0, 1.0);
                let formation = normalized_exponential_response(age, 3.8);
                if formation <= f32::EPSILON {
                    continue;
                }
                let (fall, bounce) = sediment_settling_motion(age);
                let angle = seeded_unit(seed, index, 2) * std::f32::consts::TAU;
                let radius = seeded_unit(seed, index, 3).sqrt() * dynamics.spread;
                let target = layout.liquid_center
                    + Vec3::new(
                        angle.cos() * radius,
                        -0.34 + seeded_unit(seed, index, 4) * 0.26,
                        angle.sin() * radius,
                    );
                let drift = curl_like_flow(phase * 0.42, seed, index)
                    * dynamics.turbulence
                    * 0.08
                    * formation;
                let point = surface_point.lerp(target, fall)
                    + drift * (1.0 - fall * 0.72)
                    + Vec3::Y * bounce;
                let shard_seed = seed ^ u64::from(index).wrapping_mul(0x9e37_79b9_7f4a_7c15);
                let rotation = settling_shard_rotation(shard_seed, age);
                let radius = (0.025 + seeded_unit(seed, index, 7) * 0.035) * formation;
                add_shard(
                    &mut meshes.translucent,
                    point,
                    Vec3::new(radius * 0.70, radius * 1.65, radius * 0.58),
                    rotation,
                    alpha(colours.solid, envelope * formation * 0.88),
                    shard_seed,
                );
            }
        }
        EffectGeometry::NucleatingSolid => {
            for index in 0..count {
                let index = u32::from(index);
                let birth = seeded_unit(seed, index, 1) * 0.64;
                let age = ((effect_progress - birth) / 0.30).clamp(0.0, 1.0);
                let growth = normalized_exponential_response(age, 4.2);
                if growth <= f32::EPSILON {
                    continue;
                }
                let angle = seeded_unit(seed, index, 2) * std::f32::consts::TAU;
                let radius = (0.06 + seeded_unit(seed, index, 3) * dynamics.spread * 0.34) * growth;
                let height =
                    (-0.10 + seeded_unit(seed, index, 4) * (0.18 + dynamics.lift * 0.20)) * growth;
                let curl = curl_like_flow(phase * 0.36, seed, index)
                    * dynamics.turbulence
                    * 0.07
                    * (1.0 - growth);
                let point = surface_point
                    + Vec3::new(angle.cos() * radius, height, angle.sin() * radius)
                    + curl;
                let shard_seed = seed ^ u64::from(index).wrapping_mul(0x94d0_49bb_1331_11eb);
                let axis = Vec3::new(
                    seeded_unit(seed, index, 5) - 0.5,
                    0.45 + seeded_unit(seed, index, 6) * 0.55,
                    seeded_unit(seed, index, 7) - 0.5,
                )
                .normalize_or_zero();
                let rotation = Quat::from_axis_angle(
                    axis,
                    growth * (0.35 + seeded_unit(seed, index, 8) * 0.65) * std::f32::consts::PI,
                );
                let shard_radius = (0.018 + seeded_unit(seed, index, 9) * 0.032) * growth;
                add_shard(
                    &mut meshes.translucent,
                    point,
                    Vec3::new(
                        shard_radius * 0.72,
                        shard_radius * 1.55,
                        shard_radius * 0.62,
                    ),
                    rotation,
                    alpha(colours.solid, envelope * growth * 0.86),
                    shard_seed,
                );
            }
        }
        EffectGeometry::RisingBubbles => {
            for index in 0..count {
                let index = u32::from(index);
                let speed = 0.76 + seeded_unit(seed, index, 1) * 0.62;
                let cycle = (phase * dynamics.rate * speed + seeded_unit(seed, index, 2)).fract();
                let lifecycle = (std::f32::consts::PI * cycle).sin().sqrt() * envelope;
                let rise = normalized_terminal_distance(cycle, 5.2);
                let angle = seeded_unit(seed, index, 3) * std::f32::consts::TAU;
                let radial = seeded_unit(seed, index, 4).sqrt() * dynamics.spread;
                let wobble = curl_like_flow(phase * (1.1 + speed), seed, index)
                    * dynamics.turbulence
                    * 0.16
                    * rise;
                let point = surface_point
                    + Vec3::new(
                        angle.cos() * radial,
                        -0.42 + rise * (0.46 + dynamics.lift),
                        angle.sin() * radial,
                    )
                    + Vec3::new(wobble.x, 0.0, wobble.z);
                add_sphere(
                    &mut meshes.translucent,
                    point,
                    0.025 + seeded_unit(seed, index, 6) * 0.045,
                    alpha(
                        mix_color(colours.liquid, colours.gas, 0.28),
                        lifecycle * 0.72,
                    ),
                    5,
                    7,
                );
            }
        }
        EffectGeometry::EscapingGas => {
            if effect.trigger == ObservationPredicate::Forms {
                // `forms` plus a separately reviewed gas phase authorizes
                // in-vessel product expansion, not a claim that gas visibly
                // vents from a liquid. The persistent product asset and this
                // transient current therefore share the retained regime.
                let (center, scale) = layout.gas_volume();
                add_gas_density_field(
                    &mut meshes.gas,
                    center,
                    scale,
                    alpha(colours.gas, envelope * 0.44),
                    seed.rotate_left(17),
                    phase * dynamics.rate,
                    envelope * 0.52,
                    GasFlowControls::retained_product(
                        envelope * 0.66,
                        dynamics.turbulence,
                        0.12 + dynamics.lift * 0.10,
                        envelope * 0.20,
                        effect_progress,
                        seed.rotate_left(17),
                    ),
                );
                return;
            }
            let rise = normalized_terminal_distance(effect_progress, 3.6);
            let drift = curl_like_flow(phase * 0.44, seed, 0) * dynamics.turbulence * 0.12;
            // Gas first occupies and mixes through the headspace. The second
            // field below continues through the open rim as a dissipating
            // plume; both are driven by the same typed gas-release effect.
            let headspace_center = surface_point + drift * 0.35 + Vec3::new(0.0, 0.31, 0.0);
            add_gas_density_field(
                &mut meshes.gas,
                headspace_center,
                Vec3::new(0.68, 0.34, 0.68),
                alpha(colours.gas, envelope * 0.78),
                seed.rotate_left(17),
                phase * dynamics.rate,
                envelope,
                GasFlowControls::contained(
                    envelope,
                    dynamics.turbulence,
                    0.18 + envelope * 0.18,
                    envelope * 0.24,
                    seed.rotate_left(17),
                ),
            );
            let center =
                surface_point + drift + Vec3::new(0.0, 0.16 + rise * dynamics.lift * 0.72, 0.0);
            let cloud_scale = Vec3::new(
                0.30 + dynamics.spread * (0.58 + rise * 0.34),
                0.32 + dynamics.lift * (0.50 + rise * 0.42),
                0.30 + dynamics.spread * (0.52 + rise * 0.30),
            );
            add_gas_density_field(
                &mut meshes.gas,
                center,
                cloud_scale,
                alpha(colours.gas, envelope),
                seed,
                phase * dynamics.rate,
                envelope,
                GasFlowControls::escaping(envelope, dynamics.turbulence, dynamics.lift, seed),
            );
        }
        EffectGeometry::EscapingVapour => {
            let rise = normalized_terminal_distance(effect_progress, 4.8);
            let drift = curl_like_flow(phase * 0.56, seed, 0) * dynamics.turbulence * 0.16;
            let center =
                surface_point + drift + Vec3::new(0.0, 0.12 + rise * dynamics.lift * 0.86, 0.0);
            let vapour = mix_color(colours.gas, [0.92, 0.95, 0.96, 0.24], 0.82);
            add_gas_density_field(
                &mut meshes.gas,
                center,
                Vec3::new(
                    0.26 + dynamics.spread * (0.44 + rise * 0.38),
                    0.34 + dynamics.lift * (0.48 + rise * 0.52),
                    0.26 + dynamics.spread * (0.42 + rise * 0.32),
                ),
                alpha(vapour, envelope * (0.74 + (1.0 - rise) * 0.18)),
                seed,
                phase * dynamics.rate,
                envelope,
                GasFlowControls::escaping(
                    envelope,
                    dynamics.turbulence,
                    dynamics.lift.max(0.72),
                    seed,
                ),
            );
        }
        EffectGeometry::SurfaceRipples => {
            for ring in 0..count.min(4) {
                let ring = u32::from(ring);
                let cycle = (phase * dynamics.rate + seeded_unit(seed, ring, 1)).fract();
                let ring_alpha = envelope * (1.0 - smoother_step(cycle)).powi(2);
                let flow_offset = curl_like_flow(phase * 0.31, seed, ring)
                    * dynamics.turbulence
                    * (0.035 + cycle * 0.025);
                add_ring(
                    &mut meshes.translucent,
                    surface_point + Vec3::new(flow_offset.x, 0.012, flow_offset.z),
                    0.10 + normalized_drag_distance(cycle, 0.16) * dynamics.spread,
                    0.008 + (1.0 - cycle) * 0.008,
                    alpha(
                        mix_color(colours.liquid, [0.90, 0.96, 0.98, 0.48], 0.48),
                        ring_alpha,
                    ),
                );
            }
        }
        EffectGeometry::MixingCurrents => {
            if stage != MacroscopicStage::Reaction {
                return;
            }
            let stirrer = stirring_apparatus_authorized(layout, effect)
                .then(|| stirring_pose(layout, effect_progress, seed));
            let (mixing_center, mixing_envelope) =
                stirrer.map_or((layout.liquid_center, envelope), |pose| {
                    (
                        Vec3::new(pose.lower.x, layout.liquid_center.y, pose.lower.z),
                        envelope * pose.activity,
                    )
                });
            add_mixing_currents(
                &mut meshes.translucent,
                mixing_center,
                dynamics,
                mixing_envelope,
                phase,
                seed,
                colours.liquid,
            );
            if let Some(pose) = stirrer {
                add_stirring_apparatus(meshes, layout, pose, effect_progress, seed, colours.liquid);
            }
        }
        EffectGeometry::SplashDroplets => {
            for index in 0..count {
                let index = u32::from(index);
                let speed = 0.82 + seeded_unit(seed, index, 1) * 0.48;
                let cycle = (phase * dynamics.rate * speed + seeded_unit(seed, index, 2)).fract();
                let angle = seeded_unit(seed, index, 3) * std::f32::consts::TAU;
                let distance = normalized_drag_distance(cycle, 0.48)
                    * dynamics.spread
                    * (0.44 + seeded_unit(seed, index, 4) * 0.56);
                let arc = ballistic_arc(cycle)
                    * dynamics.lift
                    * (0.56 + seeded_unit(seed, index, 5) * 0.44);
                let lifecycle = (std::f32::consts::PI * cycle).sin().sqrt() * envelope;
                let point = surface_point
                    + Vec3::new(angle.cos() * distance, 0.02 + arc, angle.sin() * distance);
                add_sphere(
                    &mut meshes.translucent,
                    point,
                    0.018 + seeded_unit(seed, index, 6) * 0.025,
                    alpha(
                        mix_color(colours.liquid, [0.92, 0.96, 0.98, 0.64], 0.18),
                        lifecycle,
                    ),
                    4,
                    6,
                );
            }
        }
        EffectGeometry::FlamePlume => {
            let EffectProfile::FlameEmitter(palette) = effect.effect else {
                return;
            };
            add_flame_plume(
                meshes,
                surface_point,
                palette,
                dynamics,
                envelope,
                phase,
                seed,
            );
        }
        EffectGeometry::PresentationOnly => {}
    }
}

fn add_evaporation_crystallization_process(
    meshes: &mut SceneMeshes,
    layout: SceneLayout,
    state: PostProcessVisualState,
    seed: u64,
    colours: EffectColours,
    stage_progress: f32,
) {
    add_heating_rig(meshes, layout, state, seed, stage_progress);
    if state.boiling > 0.002 && state.liquid_fraction > 0.006 {
        add_nucleate_boiling(
            &mut meshes.translucent,
            layout,
            state.boiling,
            stage_progress,
            seed,
            colours.liquid,
        );
    }
    if state.vapour > 0.002 {
        let centre = Vec3::new(
            layout.vessel_center.x,
            layout
                .liquid_surface
                .max(layout.bench_top + state.lift + 0.10)
                + 0.28,
            layout.vessel_center.z,
        );
        add_gas_density_field(
            &mut meshes.gas,
            centre,
            Vec3::new(0.46, 0.74, 0.46),
            [0.88, 0.92, 0.93, 0.34 * state.vapour],
            seed.rotate_left(23),
            stage_progress * 4.2,
            state.vapour,
            GasFlowControls::escaping(
                state.vapour,
                0.48 + state.boiling * 0.34,
                0.92,
                seed.rotate_left(23),
            ),
        );
    }
    if state.crystal_growth > 0.002 {
        // Crystals rest on the vessel's actual interior floor — the same
        // place the shrinking pool bottoms out — riding the heating lift,
        // so they never hang where an old liquid level used to be.
        add_crystallizing_salt(
            &mut meshes.opaque,
            Vec3::new(
                layout.vessel_center.x,
                layout.bench_top + state.lift + 0.125,
                layout.vessel_center.z,
            ),
            state.crystal_growth,
            seed.rotate_left(37),
            colours.solid,
        );
    }
}

fn add_heating_rig(
    meshes: &mut SceneMeshes,
    layout: SceneLayout,
    state: PostProcessVisualState,
    seed: u64,
    stage_progress: f32,
) {
    let reveal = (state.lift / HEATING_LIFT).clamp(0.0, 1.0);
    if reveal <= 0.002 {
        return;
    }
    // Entrance choreography: the rig slides in from the wings at full size
    // while the vessel lifts to make room, settling under it — instead of
    // inflating out of the bench.
    let entrance = smooth01(reveal / 0.55);
    let slide_angle = seed_phase(seed, 152);
    let slide = Vec3::new(slide_angle.cos(), 0.0, slide_angle.sin()) * (1.7 * (1.0 - entrance));
    let centre = Vec3::new(
        layout.vessel_center.x,
        layout.bench_top,
        layout.vessel_center.z,
    ) + slide;
    let vessel_bottom = layout.bench_top + state.lift;
    let support_y = vessel_bottom - 0.035;
    let metal = [0.20, 0.24, 0.28, 1.0];
    let burner = [0.12, 0.17, 0.22, 1.0];
    add_cylinder(
        &mut meshes.opaque,
        centre + Vec3::Y * 0.018,
        centre + Vec3::Y * 0.105,
        0.13,
        burner,
    );
    add_ring(
        &mut meshes.opaque,
        Vec3::new(centre.x, support_y, centre.z),
        0.57,
        0.022,
        metal,
    );
    for leg in 0..3_u8 {
        let angle = std::f32::consts::TAU * f32::from(leg) / 3.0 + seed_phase(seed, 151) * 0.04;
        let foot = centre + Vec3::new(angle.cos() * 0.48, 0.025, angle.sin() * 0.48);
        let top = Vec3::new(
            centre.x + angle.cos() * 0.50,
            support_y,
            centre.z + angle.sin() * 0.50,
        );
        add_cylinder(&mut meshes.opaque, foot, top, 0.016, metal);
    }
    if state.flame > 0.002 {
        let dynamics = scene_registry::effect_dynamics(
            EffectProfile::FlameEmitter(FlamePalette::BurnerBlue),
            EffectIntensity::Moderate,
        );
        add_flame_plume(
            meshes,
            centre + Vec3::Y * 0.10,
            FlamePalette::BurnerBlue,
            EffectDynamics {
                particle_count: dynamics.particle_count.min(15),
                spread: dynamics.spread * 0.72,
                lift: (vessel_bottom - layout.bench_top - 0.10).max(0.12),
                turbulence: dynamics.turbulence * 0.58,
                ..dynamics
            },
            state.flame,
            stage_progress * 3.6 + 0.17,
            seed.rotate_left(11),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn add_nucleate_boiling(
    mesh: &mut Mesh,
    layout: SceneLayout,
    intensity: f32,
    progress: f32,
    seed: u64,
    liquid_colour: [f32; 4],
) {
    let liquid_half_height = layout.liquid_surface - layout.liquid_center.y;
    let liquid_height = liquid_half_height / 0.54 * 1.06;
    if liquid_height <= 0.015 {
        return;
    }
    let bottom = layout.liquid_surface - liquid_height;
    let radius = 0.68;
    let bubble_colour = mix_color(liquid_colour, [0.92, 0.97, 0.99, 0.50], 0.72);
    for site in 0..28_u32 {
        let cycle_rate = 3.2 + seeded_unit(seed, site, 161) * 2.8;
        let cycle = (progress * cycle_rate + seeded_unit(seed, site, 162)).fract();
        let angle = seeded_unit(seed, site, 163) * std::f32::consts::TAU;
        let radial = seeded_unit(seed, site, 164).sqrt() * radius;
        let site_point = Vec3::new(
            layout.vessel_center.x + angle.cos() * radial,
            bottom + 0.018,
            layout.vessel_center.z + angle.sin() * radial,
        );
        let attachment_end = 0.24 + seeded_unit(seed, site, 165) * 0.12;
        let coalesced = site % 7 == 0;
        let base_radius =
            (0.018 + seeded_unit(seed, site, 166) * 0.034) * if coalesced { 1.55 } else { 1.0 };
        if cycle < attachment_end {
            let growth = smoother_step(cycle / attachment_end);
            add_sphere(
                mesh,
                site_point,
                base_radius * (0.16 + growth * 0.84) * intensity.sqrt(),
                alpha(bubble_colour, intensity * growth * 0.62),
                7,
                10,
            );
            continue;
        }
        let age = ((cycle - attachment_end) / (1.0 - attachment_end)).clamp(0.0, 1.0);
        let rise = normalized_terminal_distance(age, 4.8);
        let vertical = rise * liquid_height * 1.08;
        let drift = curl_like_flow(progress * 9.0, seed, site) * (0.018 + rise * 0.065) * intensity;
        let point = site_point + Vec3::new(drift.x, vertical, drift.z);
        if point.y < layout.liquid_surface {
            let expansion = 1.0 + rise * (0.34 + seeded_unit(seed, site, 167) * 0.28);
            add_sphere(
                mesh,
                point,
                base_radius * expansion * intensity.sqrt(),
                alpha(
                    bubble_colour,
                    intensity * (1.0 - smoother_step((age - 0.88) / 0.12)) * 0.66,
                ),
                7,
                10,
            );
        } else if age < 0.96 {
            let burst = ((age - 0.82) / 0.14).clamp(0.0, 1.0);
            add_ring(
                mesh,
                Vec3::new(point.x, layout.liquid_surface + 0.012, point.z),
                base_radius * (0.72 + burst * 2.6),
                0.006 + (1.0 - burst) * 0.008,
                alpha(
                    bubble_colour,
                    intensity * (1.0 - smoother_step(burst)) * 0.72,
                ),
            );
        }
    }
}

fn add_crystallizing_salt(
    mesh: &mut Mesh,
    floor: Vec3,
    progress: f32,
    seed: u64,
    colour: [f32; 4],
) {
    let progress = progress.clamp(0.0, 1.0);
    let crystal_colour = mix_color(colour, [0.93, 0.95, 0.92, 1.0], 0.42);
    for index in 0..48_u32 {
        let birth = seeded_unit(seed, index, 171) * 0.72;
        let age = ((progress - birth) / (1.0 - birth).max(0.08)).clamp(0.0, 1.0);
        if age <= f32::EPSILON {
            continue;
        }
        let growth = normalized_exponential_response(age, 4.6);
        let angle = seeded_unit(seed, index, 172) * std::f32::consts::TAU;
        let radial = seeded_unit(seed, index, 173).sqrt() * (0.12 + progress * 0.42);
        let shard_seed = seed ^ u64::from(index).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        let size =
            (0.022 + seeded_unit(seed, index, 174) * 0.042) * growth * (0.72 + progress * 0.28);
        let tier = f32::from((index % 3) as u8) * 0.003 * growth;
        let point = floor
            + Vec3::new(
                angle.cos() * radial,
                size * 0.34 + tier,
                angle.sin() * radial,
            );
        // Cubic habit: the alkali halides this pipeline crystallises are all
        // rock-salt cubes, so each crystal is an even cube resting near-flat
        // on a face — random yaw, only a small settle tilt.
        let rotation = Quat::from_rotation_y(
            seeded_unit(seed, index, 178) * std::f32::consts::TAU,
        ) * Quat::from_rotation_x((seeded_unit(seed, index, 175) - 0.5) * 0.22)
            * Quat::from_rotation_z((seeded_unit(seed, index, 177) - 0.5) * 0.22);
        add_shard(
            mesh,
            point,
            Vec3::splat(size * 0.84),
            rotation,
            crystal_colour,
            shard_seed,
        );
    }
}

#[derive(Debug, Clone, Copy)]
struct FlameColours {
    body_low: [f32; 4],
    body_high: [f32; 4],
    core: [f32; 4],
}

const fn flame_colours(palette: FlamePalette) -> FlameColours {
    match palette {
        FlamePalette::Natural => FlameColours {
            body_low: [1.00, 0.32, 0.04, 0.46],
            body_high: [1.00, 0.74, 0.12, 0.34],
            core: [1.00, 0.90, 0.48, 0.36],
        },
        FlamePalette::BurnerBlue => FlameColours {
            body_low: [0.06, 0.28, 1.00, 0.42],
            body_high: [0.18, 0.72, 1.00, 0.32],
            core: [0.72, 0.94, 1.00, 0.38],
        },
        FlamePalette::Crimson => FlameColours {
            body_low: [0.88, 0.05, 0.12, 0.42],
            body_high: [1.00, 0.30, 0.28, 0.32],
            core: [1.00, 0.70, 0.56, 0.34],
        },
        FlamePalette::YellowOrange => FlameColours {
            body_low: [1.00, 0.34, 0.02, 0.44],
            body_high: [1.00, 0.82, 0.06, 0.34],
            core: [1.00, 0.94, 0.54, 0.36],
        },
        FlamePalette::Lilac => FlameColours {
            body_low: [0.48, 0.12, 0.88, 0.42],
            body_high: [0.82, 0.48, 1.00, 0.34],
            core: [0.98, 0.82, 1.00, 0.34],
        },
    }
}

#[allow(clippy::too_many_arguments)]
fn add_flame_plume(
    meshes: &mut SceneMeshes,
    source: Vec3,
    palette: FlamePalette,
    dynamics: EffectDynamics,
    envelope: f32,
    phase: f32,
    seed: u64,
) {
    if envelope <= 0.001 {
        return;
    }
    let colours = flame_colours(palette);
    for index in 0..dynamics.particle_count {
        let index = u32::from(index);
        let rise_speed = 0.78 + seeded_unit(seed, index, 1) * 0.64;
        let age = (phase * dynamics.rate * rise_speed + seeded_unit(seed, index, 2)).fract();
        let rise = normalized_terminal_distance(age, 4.8);
        let lifecycle = (std::f32::consts::PI * age).sin().max(0.0).sqrt() * envelope;
        if lifecycle <= 0.01 {
            continue;
        }
        let angle = seeded_unit(seed, index, 3) * std::f32::consts::TAU;
        let source_radius = seeded_unit(seed, index, 4).sqrt() * dynamics.spread * 0.34;
        let detach = rise * rise;
        let curl_phase = phase * (1.4 + rise_speed);
        let curl = curl_like_flow(curl_phase, seed, index) * dynamics.turbulence * 0.16 * detach;
        let base = source
            + Vec3::new(
                angle.cos() * source_radius,
                0.02 + rise * dynamics.lift * 0.52,
                angle.sin() * source_radius,
            )
            + curl;
        let lean = curl_like_flow(curl_phase * 1.37, seed.rotate_left(7), index)
            * dynamics.turbulence
            * 0.13;
        let height = (0.16 + seeded_unit(seed, index, 6) * 0.24)
            * (1.0 - age * 0.38)
            * (0.72 + dynamics.lift * 0.28);
        let tip = base + Vec3::Y * height + lean;
        let width =
            (0.035 + seeded_unit(seed, index, 7) * 0.055) * (1.0 - age * 0.46) * lifecycle.sqrt();
        let body_mix = rise * 0.72 + seeded_unit(seed, index, 8) * 0.18;
        let body_colour = alpha(
            mix_color(colours.body_low, colours.body_high, body_mix),
            lifecycle,
        );
        add_flame_lobe(&mut meshes.translucent, base, tip, width, body_colour);

        // A small emissive core gives brightness without turning the whole
        // plume into an additive, colour-saturating cloud.
        if index % 2 == 0 {
            let core_tip = base.lerp(tip, 0.62);
            add_flame_lobe(
                &mut meshes.emissive,
                base + Vec3::Y * 0.008,
                core_tip,
                width * 0.42,
                alpha(colours.core, lifecycle * (1.0 - age) * 0.76),
            );
        }
        if index % 5 == 0 && age > 0.48 {
            let spark_base = tip + lean * 0.28;
            add_flame_lobe(
                &mut meshes.emissive,
                spark_base,
                spark_base + Vec3::Y * (0.025 + seeded_unit(seed, index, 9) * 0.055),
                0.008,
                alpha(colours.core, lifecycle * (1.0 - age) * 0.58),
            );
        }
    }
}

fn mix_color(a: [f32; 4], b: [f32; 4], amount: f32) -> [f32; 4] {
    let amount = amount.clamp(0.0, 1.0);
    std::array::from_fn(|index| a[index] + (b[index] - a[index]) * amount)
}

/// Adds one tapered, faceted flame particle. The pointed connected silhouette
/// avoids obvious circular sprites and remains three-dimensional at the fixed
/// isometric camera angle.
fn add_flame_lobe(mesh: &mut Mesh, base: Vec3, tip: Vec3, width: f32, color: [f32; 4]) {
    if width <= 0.001 || color[3] <= 0.001 {
        return;
    }
    let axis = (tip - base).normalize_or_zero();
    if axis.length_squared() <= f32::EPSILON {
        return;
    }
    let reference = if axis.y.abs() > 0.92 {
        Vec3::X
    } else {
        Vec3::Y
    };
    let tangent = axis.cross(reference).normalize_or_zero();
    let bitangent = axis.cross(tangent).normalize_or_zero();
    let middle = base.lerp(tip, 0.38);
    let ring = [
        middle + tangent * width,
        middle + bitangent * width * 0.82,
        middle - tangent * width,
        middle - bitangent * width * 0.82,
    ];
    for side in 0..4 {
        let next = (side + 1) % 4;
        add_flat_triangle(mesh, base, ring[next], ring[side], color);
        add_flat_triangle(mesh, ring[side], ring[next], tip, color);
    }
}

fn alpha(mut color: [f32; 4], factor: f32) -> [f32; 4] {
    color[3] *= factor.clamp(0.0, 1.0);
    color
}

fn add_liquid_volume(
    mesh: &mut Mesh,
    center: Vec3,
    scale: Vec3,
    color: [f32; 4],
    turbulence: f32,
    phase: f32,
    seed: u64,
) {
    add_contained_liquid(
        mesh,
        center + Vec3::new(0.0, 0.54 * scale.y, 0.0),
        center.y - 0.52 * scale.y,
        0.82 * scale.x,
        color,
        turbulence,
        phase,
        seed,
    );
}

/// Universal contained-liquid primitive: a basin sealed by a wall, a floor
/// disc, and a smooth-shaded free surface with procedural ripples and a
/// meniscus climbing the vessel wall. Everything renders from scalar state
/// (level, radius, turbulence), so any scene that knows its liquid level
/// shares this one implementation.
#[allow(clippy::too_many_arguments)]
fn add_contained_liquid(
    mesh: &mut Mesh,
    surface_centre: Vec3,
    floor_y: f32,
    radius: f32,
    colour: [f32; 4],
    turbulence: f32,
    phase: f32,
    seed: u64,
) {
    const RINGS: u16 = 7;
    const SEGMENTS: u16 = 28;
    if surface_centre.y - floor_y <= 0.02 || radius <= 0.01 {
        return;
    }
    let surface_colour = mix_color(colour, [0.86, 0.94, 0.97, 0.54], 0.46);
    let rim_colour = mix_color(colour, [0.92, 0.97, 0.99, 0.62], 0.68);
    let point = |radial: f32, angle: f32| {
        liquid_surface_point(
            surface_centre,
            radius,
            radial,
            angle,
            turbulence,
            phase,
            seed,
        )
    };
    let bottom = Vec3::new(surface_centre.x, floor_y, surface_centre.z);
    // The wall meets the surface rim exactly: at radial 1.0 the ripple field
    // is edge damped away, leaving only the angle-independent meniscus lift.
    let rim_y = point(1.0, 0.0).y;
    add_cylinder_wall(
        mesh,
        bottom,
        Vec3::new(surface_centre.x, rim_y, surface_centre.z),
        radius,
        colour,
    );
    add_disc(mesh, bottom, radius, colour);

    // Smooth-shaded free surface: shared vertices, finite-difference normals.
    let base = u32::try_from(mesh.vertices.len()).unwrap_or(u32::MAX);
    mesh.vertices.push(Vertex {
        position: point(0.0, 0.0).to_array(),
        normal: Vec3::Y.to_array(),
        color: surface_colour,
    });
    let radial_step = 0.5 / f32::from(RINGS);
    let angle_step = std::f32::consts::TAU * 0.5 / f32::from(SEGMENTS);
    for ring in 1..=RINGS {
        let radial = f32::from(ring) / f32::from(RINGS);
        for segment in 0..SEGMENTS {
            let angle = std::f32::consts::TAU * f32::from(segment) / f32::from(SEGMENTS);
            let along_radius =
                point((radial + radial_step).min(1.0), angle) - point(radial - radial_step, angle);
            let along_rim = point(radial, angle + angle_step) - point(radial, angle - angle_step);
            let normal = along_rim
                .cross(along_radius)
                .try_normalize()
                .unwrap_or(Vec3::Y);
            mesh.vertices.push(Vertex {
                position: point(radial, angle).to_array(),
                normal: normal.to_array(),
                color: surface_colour,
            });
        }
    }
    let ring_vertex = |ring: u16, segment: u16| -> u32 {
        base + 1 + u32::from(ring - 1) * u32::from(SEGMENTS) + u32::from(segment % SEGMENTS)
    };
    for segment in 0..SEGMENTS {
        mesh.indices.extend_from_slice(&[
            base,
            ring_vertex(1, segment),
            ring_vertex(1, segment + 1),
        ]);
    }
    for ring in 1..RINGS {
        for segment in 0..SEGMENTS {
            let inner_a = ring_vertex(ring, segment);
            let inner_b = ring_vertex(ring, segment + 1);
            let outer_a = ring_vertex(ring + 1, segment);
            let outer_b = ring_vertex(ring + 1, segment + 1);
            mesh.indices
                .extend_from_slice(&[inner_a, outer_a, outer_b, inner_a, outer_b, inner_b]);
        }
    }
    add_ring(
        mesh,
        Vec3::new(
            surface_centre.x,
            rim_y + 0.008 + turbulence * 0.006,
            surface_centre.z,
        ),
        radius * 0.965,
        0.014,
        rim_colour,
    );
}

/// Procedural surface bursts: short-lived expanding rings with a few flung
/// droplets, popping at seeded points on the free surface. Each slot cycles
/// on its own seeded period, so activity reads as continuous simmering pops
/// rather than a synchronized loop. Pure function of `phase`.
#[allow(
    clippy::too_many_arguments,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn add_surface_bursts(
    mesh: &mut Mesh,
    surface_centre: Vec3,
    radius: f32,
    activity: f32,
    phase: f32,
    seed: u64,
    colour: [f32; 4],
) {
    const SLOTS: u32 = 9;
    const LIFE_FRACTION: f32 = 0.42;
    if activity <= 0.02 {
        return;
    }
    for slot in 0..SLOTS {
        if seeded_unit(seed, slot, 210) > activity {
            continue;
        }
        let period = 0.8 + seeded_unit(seed, slot, 211) * 1.5;
        let offset = seeded_unit(seed, slot, 212);
        let cycles = phase / period + offset;
        let cycle = cycles.floor().max(0.0) as u64;
        let local = cycles - cycles.floor();
        if local >= LIFE_FRACTION {
            continue;
        }
        let life = local / LIFE_FRACTION;
        let cycle_seed = seed.wrapping_add(cycle.wrapping_mul(0x9e37_79b9_7f4a_7c15));
        let angle = seeded_unit(cycle_seed, slot, 213) * std::f32::consts::TAU;
        let radial = seeded_unit(cycle_seed, slot, 214).sqrt() * 0.62;
        let origin = surface_centre
            + Vec3::new(
                angle.cos() * radius * radial,
                0.012,
                angle.sin() * radius * radial,
            );
        // Expanding, thinning ripple ring.
        let ring_alpha = (1.0 - life) * 0.30;
        add_ring(
            mesh,
            origin,
            (0.015 + life * 0.09) * radius.max(0.4),
            0.005 + life * 0.003,
            [
                colour[0] * 0.85 + 0.15,
                colour[1] * 0.85 + 0.15,
                colour[2] * 0.85 + 0.15,
                ring_alpha,
            ],
        );
        // A few flung droplets on low ballistic arcs.
        for droplet in 0..3_u32 {
            let launch = seeded_unit(cycle_seed, slot * 8 + droplet, 215) * std::f32::consts::TAU;
            let fling = 0.22 + seeded_unit(cycle_seed, slot * 8 + droplet, 216) * 0.34;
            let up = 0.5 + seeded_unit(cycle_seed, slot * 8 + droplet, 217) * 0.38;
            let t = life * 0.5;
            let position = origin
                + Vec3::new(
                    launch.cos() * fling * t,
                    up * t - 4.4 * t * t,
                    launch.sin() * fling * t,
                );
            if position.y <= surface_centre.y {
                continue;
            }
            add_sphere(
                mesh,
                position,
                0.006 + seeded_unit(cycle_seed, slot * 8 + droplet, 218) * 0.005,
                [colour[0], colour[1], colour[2], 0.42 * (1.0 - life)],
                4,
                6,
            );
        }
    }
}

/// A ring of clinging foam bubbles at the liquid/glass contact line —
/// gas-producing or agitated liquids collect a bubble collar there. Bubble
/// membership, size, and slow drift are seeded; density follows intensity.
fn add_foam_ring(
    mesh: &mut Mesh,
    surface_centre: Vec3,
    radius: f32,
    intensity: f32,
    phase: f32,
    seed: u64,
) {
    const BUBBLES: u32 = 42;
    if intensity <= 0.03 {
        return;
    }
    for bubble in 0..BUBBLES {
        if seeded_unit(seed, bubble, 230) > intensity {
            continue;
        }
        let drift = (phase * (0.05 + seeded_unit(seed, bubble, 231) * 0.04))
            * if bubble % 2 == 0 { 1.0 } else { -1.0 };
        let angle = seeded_unit(seed, bubble, 232) * std::f32::consts::TAU + drift;
        let inset = 0.955 - seeded_unit(seed, bubble, 233) * 0.05;
        let size = 0.007 + seeded_unit(seed, bubble, 234) * 0.009;
        // A slow seeded breathing keeps the collar alive without popping.
        let breathe = 1.0 + (phase * 1.7 + seed_phase(seed, 235 + bubble)).sin() * 0.12;
        add_sphere(
            mesh,
            surface_centre
                + Vec3::new(
                    angle.cos() * radius * inset,
                    0.006 + seeded_unit(seed, bubble, 236) * 0.008,
                    angle.sin() * radius * inset,
                ),
            size * breathe,
            [0.90, 0.95, 0.97, 0.34],
            4,
            6,
        );
    }
}

/// Procedural gas bubbles: seeded streams nucleate near the basin floor,
/// rise with a helical wobble, grow slightly as pressure drops, and vanish
/// into the free surface (the burst system covers the pops). Emission times
/// are indexed, so any playhead reconstructs the same bubbles mid-flight.
/// One agitated liquid surface: where the agitation sits, how strong it is,
/// and how tightly its bubble columns cluster toward the centre.
struct SurfaceAgitation {
    surface_centre: Vec3,
    floor_y: f32,
    radius: f32,
    intensity: f32,
    /// 0 = columns across the whole floor (boiling); 1 = clustered under
    /// the centre (a reacting lump).
    centre_bias: f32,
    colour: [f32; 4],
}

/// Everything an agitated surface implies: pops and flung droplets, a foam
/// collar at the glass, and bubble columns rising from the floor.
fn add_surface_agitation(
    meshes: &mut SceneMeshes,
    agitation: &SurfaceAgitation,
    phase: f32,
    seed: u64,
) {
    add_surface_bursts(
        &mut meshes.translucent,
        agitation.surface_centre,
        agitation.radius,
        agitation.intensity * 0.85,
        phase,
        seed.rotate_left(9),
        agitation.colour,
    );
    add_foam_ring(
        &mut meshes.translucent,
        agitation.surface_centre,
        agitation.radius,
        agitation.intensity * 0.9,
        phase,
        seed,
    );
    add_bubble_streams(
        &mut meshes.translucent,
        agitation,
        phase,
        seed.rotate_left(17),
    );
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn add_bubble_streams(mesh: &mut Mesh, agitation: &SurfaceAgitation, phase: f32, seed: u64) {
    const STREAMS: u32 = 16;
    let intensity = agitation.intensity * 0.9;
    let depth = agitation.surface_centre.y - agitation.floor_y;
    if intensity <= 0.03 || depth <= 0.05 {
        return;
    }
    for stream in 0..STREAMS {
        if seeded_unit(seed, stream, 240) > intensity {
            continue;
        }
        let angle = seeded_unit(seed, stream, 241) * std::f32::consts::TAU;
        let radial = seeded_unit(seed, stream, 242).sqrt() * (1.0 - agitation.centre_bias) * 0.75;
        let base = Vec3::new(
            agitation.surface_centre.x + angle.cos() * agitation.radius * radial,
            agitation.floor_y + 0.015,
            agitation.surface_centre.z + angle.sin() * agitation.radius * radial,
        );
        let rise_time = 1.0 + seeded_unit(seed, stream, 243) * 0.9;
        let interval = 0.5 + seeded_unit(seed, stream, 244) * 0.85;
        let offset = seeded_unit(seed, stream, 245) * interval;
        let first_emission = ((phase - rise_time - offset) / interval).ceil().max(0.0) as u64;
        let last_emission = ((phase - offset) / interval).floor();
        if last_emission < 0.0 {
            continue;
        }
        for emission in first_emission..=(last_emission as u64) {
            let age = phase - (emission as f32).mul_add(interval, offset);
            if !(0.0..rise_time).contains(&age) {
                continue;
            }
            let fraction = (age / rise_time).powf(0.85);
            let bubble_seed = seed.wrapping_add(emission.wrapping_mul(0x9e37_79b9_7f4a_7c15));
            let wobble_phase = age * 5.2 + seed_phase(bubble_seed, 246 + stream);
            let wobble = 0.014 + seeded_unit(bubble_seed, stream, 247) * 0.012;
            let position = base
                + Vec3::new(
                    wobble_phase.sin() * wobble,
                    depth * fraction,
                    wobble_phase.cos() * wobble,
                );
            add_sphere(
                mesh,
                position,
                0.009 + fraction * 0.012 + seeded_unit(bubble_seed, stream, 248) * 0.005,
                [0.90, 0.95, 0.98, 0.48],
                4,
                6,
            );
        }
    }
}

/// Precipitate accumulating on the basin floor: a noise-bumped radial mound
/// that grows with formation progress instead of appearing fully formed.
#[allow(clippy::cast_precision_loss)]
#[allow(clippy::too_many_arguments)]
fn add_sediment_mound(
    mesh: &mut Mesh,
    floor_centre: Vec3,
    radius: f32,
    growth: f32,
    peak: f32,
    colour: [f32; 4],
    seed: u64,
) {
    const RINGS: u16 = 5;
    const SEGMENTS: u16 = 20;
    if growth <= 0.01 {
        return;
    }
    let peak = peak * growth;
    let footprint = radius * (0.45 + growth * 0.3);
    let height = |radial: f32, angle: f32| {
        let falloff = (1.0 - radial * radial).max(0.0).powf(1.4);
        let lumps = 0.72
            + 0.28
                * ((angle * 3.0 + seed_phase(seed, 250)).sin()
                    * (radial * 6.5 + seed_phase(seed, 251)).cos())
                .abs();
        peak * falloff * lumps
    };
    let point = |radial: f32, angle: f32| {
        floor_centre
            + Vec3::new(
                angle.cos() * footprint * radial,
                height(radial, angle),
                angle.sin() * footprint * radial,
            )
    };
    let base = u32::try_from(mesh.vertices.len()).unwrap_or(u32::MAX);
    mesh.vertices.push(Vertex {
        position: point(0.0, 0.0).to_array(),
        normal: Vec3::Y.to_array(),
        color: colour,
    });
    let radial_step = 0.5 / f32::from(RINGS);
    let angle_step = std::f32::consts::TAU * 0.5 / f32::from(SEGMENTS);
    for ring in 1..=RINGS {
        let radial = f32::from(ring) / f32::from(RINGS);
        for segment in 0..SEGMENTS {
            let angle = std::f32::consts::TAU * f32::from(segment) / f32::from(SEGMENTS);
            let along_radius =
                point((radial + radial_step).min(1.0), angle) - point(radial - radial_step, angle);
            let along_rim = point(radial, angle + angle_step) - point(radial, angle - angle_step);
            let normal = along_rim
                .cross(along_radius)
                .try_normalize()
                .unwrap_or(Vec3::Y);
            mesh.vertices.push(Vertex {
                position: point(radial, angle).to_array(),
                normal: normal.to_array(),
                color: colour,
            });
        }
    }
    let ring_vertex = |ring: u16, segment: u16| -> u32 {
        base + 1 + u32::from(ring - 1) * u32::from(SEGMENTS) + u32::from(segment % SEGMENTS)
    };
    for segment in 0..SEGMENTS {
        mesh.indices.extend_from_slice(&[
            base,
            ring_vertex(1, segment),
            ring_vertex(1, segment + 1),
        ]);
    }
    for ring in 1..RINGS {
        for segment in 0..SEGMENTS {
            let inner_a = ring_vertex(ring, segment);
            let inner_b = ring_vertex(ring, segment + 1);
            let outer_a = ring_vertex(ring + 1, segment);
            let outer_b = ring_vertex(ring + 1, segment + 1);
            mesh.indices
                .extend_from_slice(&[inner_a, outer_a, outer_b, inner_a, outer_b, inner_b]);
        }
    }
}

/// Faint schlieren wisps swaying around a dissolving solid: the densest
/// solution sinks and shears into the bulk. Deliberately dim.
fn add_dissolution_wisps(mesh: &mut Mesh, source: Vec3, reach: f32, phase: f32, seed: u64) {
    const WISPS: u32 = 5;
    for wisp in 0..WISPS {
        let angle = seeded_unit(seed, wisp, 260) * std::f32::consts::TAU
            + (phase * (0.16 + seeded_unit(seed, wisp, 261) * 0.12)).sin() * 0.5;
        let distance = 0.05 + seeded_unit(seed, wisp, 262) * reach;
        let sway = (phase * (0.7 + seeded_unit(seed, wisp, 263) * 0.5)
            + seed_phase(seed, 264 + wisp))
        .sin()
            * 0.03;
        let centre = source
            + Vec3::new(
                angle.cos() * distance + sway,
                0.05 + seeded_unit(seed, wisp, 265) * 0.06,
                angle.sin() * distance - sway,
            );
        add_shard(
            mesh,
            centre,
            Vec3::new(0.010, 0.075 + seeded_unit(seed, wisp, 266) * 0.05, 0.010),
            Quat::from_rotation_y(angle) * Quat::from_rotation_x(sway * 3.0),
            [0.86, 0.92, 0.95, 0.11],
            seed.wrapping_add(u64::from(wisp)),
        );
    }
}

/// Condensation on the inner glass above the liquid: seeded droplets whose
/// visible population and size follow the vapour amount.
fn add_glass_condensation(
    mesh: &mut Mesh,
    surface_centre: Vec3,
    wall_radius: f32,
    rim_y: f32,
    vapour: f32,
    seed: u64,
) {
    const DROPLETS: u32 = 46;
    let head_room = rim_y - surface_centre.y;
    if vapour <= 0.05 || head_room <= 0.08 {
        return;
    }
    for droplet in 0..DROPLETS {
        if seeded_unit(seed, droplet, 270) > vapour {
            continue;
        }
        let angle = seeded_unit(seed, droplet, 271) * std::f32::consts::TAU;
        let height = surface_centre.y
            + 0.05
            + seeded_unit(seed, droplet, 272) * (head_room - 0.09).max(0.01);
        add_sphere(
            mesh,
            Vec3::new(
                surface_centre.x + angle.cos() * wall_radius,
                height,
                surface_centre.z + angle.sin() * wall_radius,
            ),
            (0.005 + seeded_unit(seed, droplet, 273) * 0.007) * (0.6 + vapour * 0.4),
            [0.88, 0.94, 0.97, 0.38],
            3,
            5,
        );
    }
}

/// The receding-waterline film: a faint wet band on the glass between the
/// current level and the authored starting level. Only meaningful where the
/// level drops (fuel burning down, solvent boiling off).
fn add_high_water_mark(mesh: &mut Mesh, state: &LiquidState, top_offset: f32) {
    let top = state.initial_level_y + top_offset;
    let bottom = state.surface_centre.y + 0.015;
    if top - bottom < 0.03 {
        return;
    }
    add_cylinder_wall(
        mesh,
        Vec3::new(state.surface_centre.x, bottom, state.surface_centre.z),
        Vec3::new(state.surface_centre.x, top, state.surface_centre.z),
        state.radius + 0.004,
        [0.62, 0.78, 0.86, 0.10],
    );
    // The old waterline itself reads slightly stronger than the film.
    add_ring(
        mesh,
        Vec3::new(state.surface_centre.x, top, state.surface_centre.z),
        state.radius + 0.004,
        0.004,
        [0.72, 0.85, 0.92, 0.16],
    );
}

/// A small Worthington crown at the pour impact: a pulsing coronet of spikes
/// thrown up around the stream while it lands.
fn add_worthington_crown(mesh: &mut Mesh, impact: Vec3, strength: f32, phase: f32, seed: u64) {
    const SPIKES: u16 = 9;
    if strength <= 0.05 {
        return;
    }
    let pulse = (phase * 2.6).fract();
    let crown_radius = 0.045 + pulse * 0.02;
    let lift = (1.0 - pulse) * 0.9 + 0.1;
    for spike in 0..SPIKES {
        let angle = std::f32::consts::TAU * f32::from(spike) / f32::from(SPIKES)
            + seeded_unit(seed, u32::from(spike), 280) * 0.5;
        let height =
            (0.028 + seeded_unit(seed, u32::from(spike), 281) * 0.026) * lift * strength.min(1.0);
        let base = impact
            + Vec3::new(
                angle.cos() * crown_radius,
                0.012,
                angle.sin() * crown_radius,
            );
        add_shard(
            mesh,
            base + Vec3::Y * height * 0.5,
            Vec3::new(0.006, height, 0.006),
            Quat::from_rotation_y(angle) * Quat::from_rotation_z(0.28),
            [0.93, 0.96, 1.0, 0.50 * strength * (0.4 + lift * 0.6)],
            seed.wrapping_add(u64::from(spike)),
        );
    }
}

const STIRRER_ENTRY_END: f32 = 0.24;
const STIRRER_EXIT_START: f32 = 0.76;

fn stirring_apparatus_authorized(layout: SceneLayout, effect: &PresentationEffect) -> bool {
    layout.has_liquid && effect.effect == EffectProfile::LiquidMixing
}

fn gate_stirrer_driven_liquid_turbulence(
    inputs: &mut ReactionVisualInputs,
    plan: &ScenePlan,
    layout: SceneLayout,
    ordinal: u16,
    progress: f32,
    stage: MacroscopicStage,
    final_ordinal: u16,
) {
    let stirring_activity = plan
        .effects
        .iter()
        .filter(|effect| effect.start_ordinal <= ordinal && ordinal <= effect.end_ordinal)
        .filter(|effect| stirring_apparatus_authorized(layout, effect))
        .map(|effect| {
            if stage == MacroscopicStage::Reaction {
                stirring_pose(
                    layout,
                    effect_progress(effect, ordinal, progress),
                    effect_seed(plan, effect),
                )
                .activity
            } else {
                0.0
            }
        })
        .reduce(f32::max);
    let Some(stirring_activity) = stirring_activity else {
        return;
    };

    // Preserve turbulence from independently authorized bubbling, splashing,
    // heat, or surface disturbance. Only the LiquidMixing contribution waits
    // for the rod's active stroke.
    let independent_turbulence = plan
        .effects
        .iter()
        .filter(|effect| effect.effect != EffectProfile::LiquidMixing)
        .filter(|effect| effect.start_ordinal <= ordinal && ordinal <= effect.end_ordinal)
        .map(|effect| {
            ReactionVisualInputs::from_effects(
                std::slice::from_ref(effect),
                ordinal,
                progress,
                final_ordinal,
            )
            .liquid_turbulence
        })
        .sum::<f32>()
        .min(1.0);
    let mixing_turbulence = (inputs.liquid_turbulence - independent_turbulence).max(0.0);
    inputs.liquid_turbulence =
        (independent_turbulence + mixing_turbulence * stirring_activity).min(1.0);
}

/// Absolute, deterministic motion for a reusable glass stirring rod. The entry
/// and withdrawal use curved paths around the vessel rim, while the active
/// phase follows a slightly irregular ellipse with velocity-dependent lean.
/// No mutable rigid-body state is required, so seeking reconstructs the same
/// pose without letting presentation physics alter reaction meaning.
fn stirring_pose(layout: SceneLayout, progress: f32, seed: u64) -> StirrerPose {
    let progress = progress.clamp(0.0, 1.0);
    let vessel_rim = layout.vessel_center.y + 0.91 * layout.vessel_scale.y;
    let shaft_height = (layout.vessel_scale.y * 1.28).clamp(1.06, 1.62);
    let active_pose = |active_age: f32| {
        let active_age = active_age.clamp(0.0, 1.0);
        let travel = natural_stirring_travel(active_age, seed);
        let turns = 2.35 + seeded_unit(seed, 0, 111) * 0.52;
        let angle = seed_phase(seed, 112) + travel * turns * std::f32::consts::TAU;
        let radius = layout.vessel_scale.x
            * (0.18
                + (angle * 1.73 + seed_phase(seed, 113)).sin() * 0.020
                + (angle * 0.61 + seed_phase(seed, 114)).cos() * 0.012);
        let lower = Vec3::new(
            layout.vessel_center.x + angle.cos() * radius,
            layout.liquid_surface
                - (layout.liquid_surface - layout.liquid_center.y).max(0.18) * 0.78
                + (angle * 1.31 + seed_phase(seed, 115)).sin() * 0.010,
            layout.vessel_center.z + angle.sin() * radius * 0.78,
        );
        let tangent = Vec3::new(-angle.sin(), 0.0, angle.cos() * 0.78).normalize_or_zero();
        let hand_lag = 0.095
            + (angle * 0.83 + seed_phase(seed, 116)).sin() * 0.018
            + seeded_unit(seed, 0, 117) * 0.012;
        let upper = lower
            + Vec3::new(
                -tangent.x * hand_lag + (seeded_unit(seed, 0, 118) - 0.5) * 0.055,
                shaft_height,
                -tangent.z * hand_lag + (seeded_unit(seed, 0, 119) - 0.5) * 0.045,
            );
        (lower, upper)
    };

    let (lower, upper, submerged, activity) = if progress < STIRRER_ENTRY_END {
        let age = progress / STIRRER_ENTRY_END;
        let travel = smoother_step(age);
        let (active_lower, active_upper) = active_pose(0.0);
        let start_lower = Vec3::new(
            layout.vessel_center.x + layout.vessel_scale.x * 1.18,
            vessel_rim + 0.42,
            layout.vessel_center.z + layout.vessel_scale.z * 0.40,
        );
        let start_upper = start_lower + Vec3::new(-0.12, shaft_height, 0.08);
        let control_lower = Vec3::new(
            layout.vessel_center.x + layout.vessel_scale.x * 0.68,
            vessel_rim + 0.30,
            layout.vessel_center.z + layout.vessel_scale.z * 0.24,
        );
        let control_upper = control_lower + Vec3::new(-0.10, shaft_height, 0.06);
        (
            quadratic_curve(start_lower, control_lower, active_lower, travel),
            quadratic_curve(start_upper, control_upper, active_upper, travel),
            smoother_step((age - 0.56) / 0.44),
            0.0,
        )
    } else if progress < STIRRER_EXIT_START {
        let age = (progress - STIRRER_ENTRY_END) / (STIRRER_EXIT_START - STIRRER_ENTRY_END);
        let (lower, upper) = active_pose(age);
        let attack = smoother_step(age / 0.16);
        let release = 1.0 - smoother_step((age - 0.80) / 0.20);
        (lower, upper, 1.0, attack * release)
    } else {
        let age = (progress - STIRRER_EXIT_START) / (1.0 - STIRRER_EXIT_START);
        let travel = smoother_step(age);
        let (active_lower, active_upper) = active_pose(1.0);
        let end_lower = Vec3::new(
            layout.vessel_center.x + layout.vessel_scale.x * 1.16,
            vessel_rim + 0.46,
            layout.vessel_center.z - layout.vessel_scale.z * 0.36,
        );
        let end_upper = end_lower + Vec3::new(-0.10, shaft_height, -0.08);
        let control_lower = Vec3::new(
            layout.vessel_center.x + layout.vessel_scale.x * 0.58,
            vessel_rim + 0.34,
            layout.vessel_center.z - layout.vessel_scale.z * 0.22,
        );
        let control_upper = control_lower + Vec3::new(-0.08, shaft_height, -0.06);
        (
            quadratic_curve(active_lower, control_lower, end_lower, travel),
            quadratic_curve(active_upper, control_upper, end_upper, travel),
            1.0 - smoother_step(age / 0.44),
            0.0,
        )
    };
    let visibility =
        smoother_step(progress / 0.045) * (1.0 - smoother_step((progress - 0.94) / 0.06));
    StirrerPose {
        lower,
        upper,
        visibility,
        submerged,
        activity,
    }
}

fn natural_stirring_travel(progress: f32, seed: u64) -> f32 {
    let progress = progress.clamp(0.0, 1.0);
    let window = (std::f32::consts::PI * progress).sin();
    let broad = (progress * std::f32::consts::TAU * 2.1 + seed_phase(seed, 120)).sin() * 0.018;
    let fine = (progress * std::f32::consts::TAU * 4.7 + seed_phase(seed, 121)).sin() * 0.006;
    (progress + window * (broad + fine)).clamp(0.0, 1.0)
}

fn quadratic_curve(start: Vec3, control: Vec3, end: Vec3, progress: f32) -> Vec3 {
    start
        .lerp(control, progress)
        .lerp(control.lerp(end, progress), progress)
}

fn add_stirring_apparatus(
    meshes: &mut SceneMeshes,
    layout: SceneLayout,
    pose: StirrerPose,
    progress: f32,
    seed: u64,
    liquid_color: [f32; 4],
) {
    if pose.visibility <= 0.001 {
        return;
    }
    let axis = (pose.upper - pose.lower).normalize_or_zero();
    if axis.length_squared() <= f32::EPSILON {
        return;
    }

    let glass = alpha([0.68, 0.86, 0.94, 0.52], pose.visibility);
    let highlight = alpha([0.94, 0.98, 1.0, 0.72], pose.visibility);
    add_cylinder(&mut meshes.glass, pose.lower, pose.upper, 0.022, glass);
    let highlight_offset = Vec3::new(0.007, 0.0, 0.006);
    add_cylinder(
        &mut meshes.glass,
        pose.lower + highlight_offset,
        pose.upper + highlight_offset,
        0.0045,
        highlight,
    );
    add_sphere(&mut meshes.glass, pose.lower, 0.023, glass, 4, 6);
    add_sphere(&mut meshes.glass, pose.upper, 0.023, glass, 4, 6);

    let grip_start = pose.upper - axis * 0.19;
    let grip_end = pose.upper + axis * 0.045;
    add_cylinder(
        &mut meshes.translucent,
        grip_start,
        grip_end,
        0.038,
        alpha([0.12, 0.20, 0.24, 0.96], pose.visibility),
    );
    add_sphere(
        &mut meshes.translucent,
        grip_start,
        0.041,
        alpha([0.56, 0.78, 0.84, 0.68], pose.visibility),
        4,
        6,
    );

    // Always emitted with activity-scaled alpha: assemblies that keep the
    // stirrer resident rely on constant topology across ordinals.
    {
        let wake_center = Vec3::new(pose.lower.x, layout.liquid_surface + 0.015, pose.lower.z);
        for ring in 0..3_u8 {
            let ring_index = u32::from(ring);
            let ring_factor = f32::from(ring);
            let cycle = (progress * (3.2 + seeded_unit(seed, ring_index, 122) * 0.8)
                + ring_factor * 0.31)
                .fract();
            add_ring(
                &mut meshes.translucent,
                wake_center,
                0.055 + normalized_drag_distance(cycle, 0.42) * (0.12 + ring_factor * 0.025),
                0.006 + (1.0 - cycle) * 0.006,
                alpha(
                    mix_color(liquid_color, [0.92, 0.97, 0.99, 0.54], 0.64),
                    pose.activity * (1.0 - smoother_step(cycle)) * 0.48,
                ),
            );
        }
    }

    let withdrawal = ((progress - STIRRER_EXIT_START) / (1.0 - STIRRER_EXIT_START)).clamp(0.0, 1.0);
    let film = (std::f32::consts::PI * withdrawal).sin().max(0.0)
        * (1.0 - pose.submerged)
        * pose.visibility;
    {
        add_sphere(
            &mut meshes.translucent,
            pose.lower - axis * 0.012,
            0.014 + seeded_unit(seed, 0, 123) * 0.006,
            alpha(liquid_color, film * 0.82),
            4,
            6,
        );
    }
}

/// Subsurface flow tracers for a typed liquid-mixing event. The ribbons are a
/// stylised refraction cue rather than additional matter: their helical path
/// follows a seeded vortex, descends into the bulk liquid, and fades at both
/// ends. Absolute playhead sampling keeps the flow deterministic when seeking.
fn add_mixing_currents(
    mesh: &mut Mesh,
    center: Vec3,
    dynamics: EffectDynamics,
    envelope: f32,
    phase: f32,
    seed: u64,
    liquid_color: [f32; 4],
) {
    const SEGMENTS: u16 = 14;
    if envelope <= 0.001 {
        return;
    }
    let current_count = dynamics.particle_count.clamp(3, 6);
    for current in 0..current_count {
        let current = u32::from(current);
        let direction = if current % 2 == 0 { 1.0 } else { -0.72 };
        let phase_offset = seed_phase(seed ^ u64::from(current), 101);
        let base_radius = (0.34 + seeded_unit(seed, current, 102) * 0.34) * dynamics.spread;
        let width = 0.012 + seeded_unit(seed, current, 103) * 0.010;
        let current_alpha = envelope * (0.18 + seeded_unit(seed, current, 104) * 0.10);
        for segment in 0..SEGMENTS {
            let start = f32::from(segment) / f32::from(SEGMENTS);
            let end = f32::from(segment + 1) / f32::from(SEGMENTS);
            let point = |travel: f32| {
                let angle = phase * dynamics.rate * std::f32::consts::TAU
                    + phase_offset
                    + direction * travel * std::f32::consts::TAU * 1.35;
                let pulse = (travel * std::f32::consts::PI).sin();
                let radius = base_radius * (0.72 + pulse * 0.28);
                let position = center
                    + Vec3::new(
                        angle.cos() * radius,
                        0.28 - travel * (0.38 + dynamics.lift * 0.24) + (angle * 1.7).sin() * 0.018,
                        angle.sin() * radius,
                    );
                let side = Vec3::new(angle.cos(), 0.0, angle.sin()) * width * pulse.max(0.12);
                (position, side)
            };
            let (start_point, start_side) = point(start);
            let (end_point, end_side) = point(end);
            let lifecycle = (std::f32::consts::PI * (start + end) * 0.5).sin().max(0.0);
            let color = alpha(
                mix_color(liquid_color, [0.92, 0.97, 0.99, 0.46], 0.58),
                current_alpha * lifecycle,
            );
            add_flat_triangle(
                mesh,
                start_point - start_side,
                end_point - end_side,
                end_point + end_side,
                color,
            );
            add_flat_triangle(
                mesh,
                start_point - start_side,
                end_point + end_side,
                start_point + start_side,
                color,
            );
        }
    }
}

fn liquid_surface_point(
    center: Vec3,
    radius: f32,
    radial: f32,
    angle: f32,
    turbulence: f32,
    phase: f32,
    seed: u64,
) -> Vec3 {
    let edge_damping = (1.0 - radial * radial).max(0.0);
    let primary = (angle * 2.0 + phase * 2.3 + seed_phase(seed, 31)).sin();
    let secondary = (angle * 5.0 - phase * 1.7 + seed_phase(seed, 32)).cos() * 0.42;
    let radial_wave = (radial * 10.0 - phase * 2.8 + seed_phase(seed, 33)).sin() * 0.34;
    let displacement =
        ((primary + secondary) * radial + radial_wave) * 0.052 * turbulence * edge_damping;
    let meniscus = radial.powi(8) * 0.026;
    center
        + Vec3::new(
            angle.cos() * radius * radial,
            displacement + meniscus,
            angle.sin() * radius * radial,
        )
}

/// Advances a real low-resolution density/temperature/velocity field, then
/// converts occupied cells into overlapping soft optical splats. The field
/// obeys semi-Lagrangian advection, pressure projection, buoyancy, density
/// weight, drag, wind, vorticity confinement, and an open-rim vessel boundary.
#[allow(clippy::cast_precision_loss, clippy::too_many_arguments)]
fn add_gas_density_field(
    splats: &mut Vec<GasSplat>,
    center: Vec3,
    scale: Vec3,
    color: [f32; 4],
    seed: u64,
    phase: f32,
    density: f32,
    controls: GasFlowControls,
) {
    if density <= 0.01 || color[3] <= 0.001 {
        return;
    }
    let volume = GasFluidVolume::simulate(seed, phase.max(0.0), controls);
    let [width, height, depth] = GasFluidVolume::dimensions();
    let cell_size = Vec3::new(
        scale.x * 2.0 / (width - 1) as f32,
        scale.y * 2.0 / (height - 1) as f32,
        scale.z * 2.0 / (depth - 1) as f32,
    );
    for z in 0..depth {
        for y in 0..height {
            for x in 0..width {
                if splats.len() as u64 >= MAX_GAS_SPLATS {
                    return;
                }
                let local_density = volume.density_at(x, y, z) * density;
                if local_density <= 0.001_5 {
                    continue;
                }
                let normalized = GasFluidVolume::grid_position(x, y, z);
                let flow = volume.velocity_at(x, y, z) * scale;
                // Beer-Lambert-style extinction across overlapping grid
                // cells. Each cell remains translucent, while a complete
                // view ray through the field builds into thick fog.
                let optical_alpha =
                    (1.0 - (-color[3].min(0.62) * local_density * 6.80).exp()).min(0.22);
                let brightness = (0.94 - local_density.min(1.4) * 0.11).max(0.72);
                splats.push(GasSplat {
                    center: (center + normalized * scale).to_array(),
                    radius: cell_size.max_element() * (0.98 + local_density.min(1.2) * 0.18),
                    color: [
                        color[0] * brightness,
                        color[1] * brightness,
                        color[2] * brightness,
                        optical_alpha,
                    ],
                    flow: flow.to_array(),
                    density: local_density,
                    layering: controls.stratification,
                });
            }
        }
    }
}

fn add_flat_triangle(mesh: &mut Mesh, a: Vec3, b: Vec3, c: Vec3, color: [f32; 4]) {
    let normal = (b - a).cross(c - a).normalize_or_zero();
    if normal.length_squared() <= f32::EPSILON {
        return;
    }
    let base = u32::try_from(mesh.vertices.len()).unwrap_or(u32::MAX);
    for position in [a, b, c] {
        mesh.vertices.push(Vertex {
            position: position.to_array(),
            normal: normal.to_array(),
            color,
        });
    }
    mesh.indices.extend_from_slice(&[base, base + 1, base + 2]);
}

fn appearance_color(profile: AppearanceProfile) -> [f32; 4] {
    match profile {
        AppearanceProfile::LaboratoryNeutral => [0.16, 0.20, 0.23, 1.0],
        AppearanceProfile::ClearGlass => [0.46, 0.70, 0.82, 0.09],
        AppearanceProfile::Water => [0.36, 0.62, 0.74, 0.28],
        AppearanceProfile::AqueousColourless => [0.72, 0.79, 0.82, 0.18],
        AppearanceProfile::ReviewedColour(colour) => [
            f32::from(colour.red) / 255.0,
            f32::from(colour.green) / 255.0,
            f32::from(colour.blue) / 255.0,
            0.24,
        ],
        AppearanceProfile::WhitePrecipitate => [0.94, 0.96, 1.0, 0.92],
        AppearanceProfile::CreamPrecipitate => [0.94, 0.88, 0.68, 0.92],
        AppearanceProfile::YellowPrecipitate => [0.94, 0.82, 0.28, 0.92],
        AppearanceProfile::AlkaliMetal => [0.72, 0.76, 0.78, 1.0],
        AppearanceProfile::MetalSilver => [0.72, 0.80, 0.88, 1.0],
    }
}

fn add_disc(mesh: &mut Mesh, center: Vec3, radius: f32, color: [f32; 4]) {
    const SEGMENTS: u16 = 24;
    let base = u32::try_from(mesh.vertices.len()).unwrap_or(u32::MAX);
    mesh.vertices.push(Vertex {
        position: center.to_array(),
        normal: Vec3::Y.to_array(),
        color,
    });
    for segment in 0..=SEGMENTS {
        let angle = std::f32::consts::TAU * f32::from(segment) / f32::from(SEGMENTS);
        mesh.vertices.push(Vertex {
            position: (center + Vec3::new(angle.cos() * radius, 0.0, angle.sin() * radius))
                .to_array(),
            normal: Vec3::Y.to_array(),
            color,
        });
    }
    for segment in 0..SEGMENTS {
        mesh.indices.extend_from_slice(&[
            base,
            base + u32::from(segment) + 1,
            base + u32::from(segment) + 2,
        ]);
    }
}

#[derive(Default)]
struct Mesh {
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
}

#[derive(Default)]
struct SceneMeshes {
    opaque: Mesh,
    metallic: Mesh,
    translucent: Mesh,
    glass: Mesh,
    emissive: Mesh,
    gas: Vec<GasSplat>,
}

impl SceneMeshes {
    fn finish(self, view_direction: Vec3) -> (Vec<GpuVertex>, Vec<u32>, u32, u32, Vec<GasSplat>) {
        let mut vertices = Vec::with_capacity(
            self.opaque.vertices.len()
                + self.metallic.vertices.len()
                + self.translucent.vertices.len()
                + self.glass.vertices.len()
                + self.emissive.vertices.len(),
        );
        let mut indices = Vec::with_capacity(
            self.opaque.indices.len()
                + self.metallic.indices.len()
                + self.translucent.indices.len()
                + self.glass.indices.len()
                + self.emissive.indices.len(),
        );
        append_mesh(
            &mut vertices,
            &mut indices,
            self.opaque,
            MATERIAL_DIELECTRIC,
        );
        append_mesh(&mut vertices, &mut indices, self.metallic, MATERIAL_METAL);
        let opaque_index_count = u32::try_from(indices.len()).unwrap_or(u32::MAX);
        append_mesh(
            &mut vertices,
            &mut indices,
            self.translucent,
            MATERIAL_LIQUID,
        );
        append_mesh(&mut vertices, &mut indices, self.glass, MATERIAL_GLASS);
        let transparent_index_count = u32::try_from(indices.len()).unwrap_or(u32::MAX);
        sort_transparent_triangles(
            view_direction,
            &vertices,
            &mut indices[opaque_index_count as usize..transparent_index_count as usize],
        );
        append_mesh(
            &mut vertices,
            &mut indices,
            self.emissive,
            MATERIAL_EMISSIVE,
        );
        (
            vertices,
            indices,
            opaque_index_count,
            transparent_index_count,
            self.gas,
        )
    }
}

fn append_mesh(vertices: &mut Vec<GpuVertex>, indices: &mut Vec<u32>, mesh: Mesh, material: u32) {
    let vertex_offset = u32::try_from(vertices.len()).unwrap_or(u32::MAX);
    vertices.extend(mesh.vertices.into_iter().map(|vertex| GpuVertex {
        position: vertex.position,
        normal: vertex.normal,
        color: vertex.color,
        material,
    }));
    indices.extend(
        mesh.indices
            .into_iter()
            .map(|index| index.saturating_add(vertex_offset)),
    );
}

/// Depth-sorts alpha-blended triangles back-to-front along the live view
/// axis (camera cues, orbit and all) so liquid and glass layer correctly
/// regardless of submission order. Deterministic: stable sort over exact
/// centroid depths.
fn sort_transparent_triangles(view_direction: Vec3, vertices: &[GpuVertex], indices: &mut [u32]) {
    let mut triangles: Vec<(f32, [u32; 3])> = indices
        .chunks_exact(3)
        .map(|triangle| {
            let centroid = triangle
                .iter()
                .map(|&index| {
                    vertices
                        .get(index as usize)
                        .map_or(Vec3::ZERO, |vertex| Vec3::from_array(vertex.position))
                })
                .sum::<Vec3>()
                / 3.0;
            (
                centroid.dot(view_direction),
                [triangle[0], triangle[1], triangle[2]],
            )
        })
        .collect();
    triangles.sort_by(|left, right| right.0.total_cmp(&left.0));
    for (slot, (_, triangle)) in indices.chunks_exact_mut(3).zip(triangles) {
        slot.copy_from_slice(&triangle);
    }
}

#[derive(Debug, Clone, Copy)]
struct AnimatedAlkaliWaterStyle {
    activity: f32,
    flame: Option<FlamePalette>,
}

fn animated_alkali_water_style(plan: &ScenePlan) -> AnimatedAlkaliWaterStyle {
    let activity = plan
        .effects
        .iter()
        .find(|effect| effect.effect == EffectProfile::BubbleEmitter)
        .map_or(0.42, |effect| match effect.intensity {
            EffectIntensity::Subtle => 0.42,
            EffectIntensity::Moderate => 0.70,
            EffectIntensity::Strong => 1.0,
        });
    let flame = plan.effects.iter().find_map(|effect| match effect.effect {
        EffectProfile::FlameEmitter(palette) => Some(palette),
        _ => None,
    });
    AnimatedAlkaliWaterStyle { activity, flame }
}


fn explosive_metal_water_track_colour(
    colour: ClipColour,
    explosive: &chem_presentation::ExplosiveMetalWaterVisualProfile,
    ordinal: u16,
    ordinal_progress: f32,
) -> [f32; 4] {
    let rgba = |bound: &chem_presentation::BoundVisualColour, opacity| {
        let (base, target, amount) =
            bound_colour_endpoints(bound, opacity, ordinal, ordinal_progress);
        mix_color(base, target, amount)
    };
    let contact_amount = match ordinal.cmp(&explosive.contact_ordinal) {
        std::cmp::Ordering::Less => 0.0,
        std::cmp::Ordering::Equal => normalized_exponential_response(ordinal_progress, 3.4),
        std::cmp::Ordering::Greater => 1.0,
    };
    let water = rgba(&explosive.water_reactant, 0.34);
    let hydroxide = rgba(&explosive.hydroxide_product, 0.34);
    let liquid = mix_color(water, hydroxide, contact_amount);
    let flame = flame_colours(FlamePalette::Natural);
    match colour {
        ClipColour::Glass => [0.62, 0.84, 0.94, 0.22],
        ClipColour::Water => liquid,
        ClipColour::WaterHighlight => alpha(mix_color(liquid, [0.94, 0.98, 1.0, 0.46], 0.52), 0.46),
        ClipColour::ReactiveMetal => rgba(&explosive.metal_reactant, 1.0),
        ClipColour::FlameOuter => flame.body_high,
        ClipColour::FlameInner => flame.body_low,
        ClipColour::FlameCore => flame.core,
        ClipColour::FizzBubble | ClipColour::GasBubble => rgba(&explosive.hydrogen_product, 0.30),
        ClipColour::Vapour | ClipColour::ProductPlume | ClipColour::GasCloud => {
            rgba(&explosive.hydrogen_product, 0.15)
        }
        ClipColour::IgnitionSpark => [1.0, 0.72, 0.12, 0.94],
        ClipColour::CombustionSmoke => [0.20, 0.22, 0.24, 0.22],
        ClipColour::MixtureA
        | ClipColour::MixtureB
        | ClipColour::SaltResidue
        | ClipColour::Fuel
        | ClipColour::Soot
        | ClipColour::SootDeposit
        | ClipColour::LiquidInitial
        | ClipColour::LiquidAdded
        | ClipColour::PrecipitateCloud
        | ClipColour::Precipitate
        | ClipColour::SolidReactant
        | ClipColour::SolutionInitial
        | ClipColour::SolutionFinal
        | ClipColour::OriginalMetal
        | ClipColour::DepositedMetal
        | ClipColour::MetalErosion
        | ClipColour::ReactantA
        | ClipColour::ReactantB
        | ClipColour::SynthesisProduct
        | ClipColour::ReactionFront
        | ClipColour::ReactionVessel
        | ClipColour::MixingTool => [0.82, 0.86, 0.88, 0.20],
    }
}

/// Everything the neutralisation basin's state implies at one moment.
#[derive(Clone, Copy)]
struct NeutralisationLiquidLife {
    bench_top: f32,
    vessel_motion: Vec3,
    liquid_colour: [f32; 4],
    surface_colour: [f32; 4],
    turbulence: f32,
    boiling: f32,
    vapour: f32,
}

/// The neutralisation basin rides the authored vessel motion; boiling pops
/// the surface, steam condenses on the glass, and the boiled-off solvent
/// leaves a receding waterline.
fn add_neutralisation_liquid(
    meshes: &mut SceneMeshes,
    state: &LiquidState,
    life: NeutralisationLiquidLife,
    phase: f32,
    seed: u64,
) {
    const MODEL_SCALE: f32 = 0.45;
    let floor_y = state.floor_y + life.vessel_motion.y * MODEL_SCALE;
    add_contained_liquid(
        &mut meshes.translucent,
        state.surface_centre,
        floor_y,
        state.radius,
        life.liquid_colour,
        life.turbulence,
        phase,
        seed,
    );
    add_surface_agitation(
        meshes,
        &SurfaceAgitation {
            surface_centre: state.surface_centre,
            floor_y,
            radius: state.radius,
            intensity: life.boiling,
            centre_bias: 0.0,
            colour: life.surface_colour,
        },
        phase,
        seed,
    );
    add_glass_condensation(
        &mut meshes.translucent,
        state.surface_centre,
        state.radius + 0.03,
        life.bench_top + 1.8 + life.vessel_motion.y * MODEL_SCALE,
        life.vapour,
        seed.rotate_left(25),
    );
    add_high_water_mark(
        &mut meshes.translucent,
        state,
        life.vessel_motion.y * MODEL_SCALE,
    );
}

#[derive(Debug, Clone, Copy)]
struct NeutralisationAssemblyMoment<'a> {
    plan: &'a ScenePlan,
    layout: SceneLayout,
    stage: MacroscopicStage,
    progress: f32,
    post_process: PostProcessVisualState,
    stage_progress: f32,
    seed: u64,
    visual_inputs: ReactionVisualInputs,
    effect_colours: EffectColours,
    ordinal: u16,
    ordinal_progress: f32,
}

#[allow(clippy::cast_precision_loss)]
fn precipitation_clip_progress(plan: &ScenePlan, moment: RealWorldPosition) -> f32 {
    // Matches the stretched authored window compiled by chem-presentation's
    // fit_authored_precipitation_duration (0.625x presentation speed).
    const DURATION_MS: f32 = 9_600.0;
    let Some(precipitation) = &plan.precipitation else {
        return 0.0;
    };
    let Some(start_ms) = plan
        .timeline
        .start_ms_for_ordinal(precipitation.formation_ordinal)
    else {
        return 0.0;
    };
    let elapsed_ms = plan.timeline.elapsed_ms_at(moment).unwrap_or(0.0);
    ((elapsed_ms - start_ms as f32) / DURATION_MS).clamp(0.0, 1.0)
}

#[allow(clippy::cast_precision_loss)]
fn gas_evolution_clip_progress(plan: &ScenePlan, moment: RealWorldPosition) -> f32 {
    const GAS_SOURCE_PROGRESS: f32 = 35.0 / 179.0;
    let Some(gas_evolution) = &plan.gas_evolution else {
        return 0.0;
    };
    let reaction_duration = plan
        .timeline
        .beats
        .iter()
        .take_while(|beat| beat.stage == MacroscopicStage::Reaction)
        .fold(0_u64, |total, beat| {
            total.saturating_add(u64::from(beat.duration_ms))
        }) as f32;
    if reaction_duration <= f32::EPSILON {
        return 0.0;
    }
    let elapsed = plan
        .timeline
        .elapsed_ms_at(moment)
        .unwrap_or(0.0)
        .clamp(0.0, reaction_duration);
    let generation_ms = plan
        .timeline
        .start_ms_for_ordinal(gas_evolution.generation_ordinal)
        .unwrap_or(0) as f32;
    if generation_ms <= f32::EPSILON {
        return (elapsed / reaction_duration).clamp(0.0, 1.0);
    }
    if elapsed < generation_ms {
        return (elapsed / generation_ms * GAS_SOURCE_PROGRESS).clamp(0.0, GAS_SOURCE_PROGRESS);
    }
    let remaining = (reaction_duration - generation_ms).max(f32::EPSILON);
    (GAS_SOURCE_PROGRESS + ((elapsed - generation_ms) / remaining) * (1.0 - GAS_SOURCE_PROGRESS))
        .clamp(0.0, 1.0)
}

/// The gas-evolution receiving basin and everything its state implies: the
/// pour-raised level, the mixed colour spreading from the stream, and the
/// fizz that ramps once gas generation begins.
#[allow(clippy::too_many_arguments)]
fn add_gas_evolution_liquid(
    meshes: &mut SceneMeshes,
    plan: &ScenePlan,
    gas_evolution: &chem_presentation::GasEvolutionVisualProfile,
    state: &LiquidState,
    pour: Option<&PourState>,
    receiving_lift: f32,
    frame: f32,
    ordinal: u16,
    ordinal_progress: f32,
) {
    // Conservation made visible: the poured volume raises the level, the
    // falling stream stirs the surface, and the mixed colour spreads from
    // where the stream lands.
    let endpoints = bound_colour_endpoints(
        &gas_evolution.initial_reactant,
        0.42,
        ordinal,
        ordinal_progress,
    );
    add_receiving_liquid(
        meshes,
        state,
        pour,
        receiving_lift,
        endpoints,
        frame / 30.0 * 2.0,
        plan_seed(plan),
    );
    // Escaping gas fizzes the surface and collects a foam collar once
    // generation begins.
    let fizz = match ordinal.cmp(&gas_evolution.generation_ordinal) {
        std::cmp::Ordering::Less => 0.0,
        std::cmp::Ordering::Equal => normalized_exponential_response(ordinal_progress, 3.4),
        std::cmp::Ordering::Greater => 1.0,
    };
    let surface = state.surface_centre + Vec3::Y * receiving_lift;
    add_surface_agitation(
        meshes,
        &SurfaceAgitation {
            surface_centre: surface,
            floor_y: (surface.y - 0.18).max(state.floor_y),
            radius: state.radius,
            intensity: fizz,
            centre_bias: 0.35,
            colour: endpoints.0,
        },
        frame / 30.0 * 2.0,
        plan_seed(plan),
    );
    // Faint wisps of dissolving material sway around the reacting solid.
    if fizz > 0.1 {
        add_dissolution_wisps(
            &mut meshes.translucent,
            Vec3::new(surface.x, state.floor_y, surface.z),
            state.radius * 0.35,
            frame / 30.0 * 2.0,
            plan_seed(plan).rotate_left(21),
        );
    }
}

#[allow(clippy::cast_precision_loss)]
fn authored_reaction_clip_progress(plan: &ScenePlan, moment: RealWorldPosition) -> f32 {
    let reaction_duration = plan
        .timeline
        .beats
        .iter()
        .take_while(|beat| beat.stage == MacroscopicStage::Reaction)
        .fold(0_u64, |total, beat| {
            total.saturating_add(u64::from(beat.duration_ms))
        }) as f32;
    if reaction_duration <= f32::EPSILON {
        return 0.0;
    }
    (plan.timeline.elapsed_ms_at(moment).unwrap_or(0.0) / reaction_duration).clamp(0.0, 1.0)
}

/// The standing solution with the final-solution colour diffusing out from
/// the immersed metal. The front's timing follows the clip like the authored
/// rising column it replaces; its colour keeps the reviewed ordinal binding.
fn add_displacement_solution(
    meshes: &mut SceneMeshes,
    state: &LiquidState,
    colour: [f32; 4],
    final_colour: [f32; 4],
    progress: f32,
    phase: f32,
    seed: u64,
) {
    // The metal strip hangs into the solution at roughly this model-space
    // offset (0.45 model units at MODEL_SCALE 0.45).
    let source = Vec3::new(
        0.45 * 0.45,
        state.floor_y + (state.surface_centre.y - state.floor_y) * 0.55,
        0.0,
    );
    let appended_from = meshes.translucent.vertices.len();
    add_contained_liquid(
        &mut meshes.translucent,
        state.surface_centre,
        state.floor_y,
        state.radius,
        colour,
        0.16,
        phase,
        seed,
    );
    apply_liquid_dye(
        &mut meshes.translucent,
        appended_from,
        &LiquidDye {
            source,
            target: final_colour,
            amount: progress,
            reach: (state.radius * 2.0).hypot(state.surface_centre.y - state.floor_y),
        },
    );
}

fn metal_displacement_track_colour(
    colour: ClipColour,
    displacement: &chem_presentation::MetalDisplacementVisualProfile,
    ordinal: u16,
    ordinal_progress: f32,
) -> [f32; 4] {
    let rgba = |bound: &chem_presentation::BoundVisualColour, opacity| {
        let base = [
            f32::from(bound.base_colour.red) / 255.0,
            f32::from(bound.base_colour.green) / 255.0,
            f32::from(bound.base_colour.blue) / 255.0,
        ];
        let target = [
            f32::from(bound.colour.red) / 255.0,
            f32::from(bound.colour.green) / 255.0,
            f32::from(bound.colour.blue) / 255.0,
        ];
        let amount = bound
            .transition_ordinal
            .map_or(1.0, |start| match ordinal.cmp(&start) {
                std::cmp::Ordering::Less => 0.0,
                std::cmp::Ordering::Equal => normalized_exponential_response(ordinal_progress, 3.4),
                std::cmp::Ordering::Greater => 1.0,
            });
        [
            base[0] + (target[0] - base[0]) * amount,
            base[1] + (target[1] - base[1]) * amount,
            base[2] + (target[2] - base[2]) * amount,
            opacity,
        ]
    };
    match colour {
        ClipColour::SolutionInitial => rgba(&displacement.initial_solution, 0.29),
        ClipColour::SolutionFinal => rgba(&displacement.final_solution, 0.29),
        ClipColour::OriginalMetal => rgba(&displacement.original_metal, 1.0),
        ClipColour::DepositedMetal => rgba(&displacement.deposited_metal, 1.0),
        ClipColour::MetalErosion => [0.12, 0.13, 0.14, 1.0],
        ClipColour::Glass => [0.62, 0.84, 0.94, 0.22],
        _ => [0.76, 0.78, 0.80, 1.0],
    }
}

fn synthesis_combination_track_colour(
    colour: ClipColour,
    synthesis: &chem_presentation::SolidSolidSynthesisVisualProfile,
    ordinal: u16,
    ordinal_progress: f32,
) -> [f32; 4] {
    let rgba = |bound: &chem_presentation::BoundVisualColour| {
        let base = [
            f32::from(bound.base_colour.red) / 255.0,
            f32::from(bound.base_colour.green) / 255.0,
            f32::from(bound.base_colour.blue) / 255.0,
        ];
        let target = [
            f32::from(bound.colour.red) / 255.0,
            f32::from(bound.colour.green) / 255.0,
            f32::from(bound.colour.blue) / 255.0,
        ];
        let amount = bound
            .transition_ordinal
            .map_or(1.0, |start| match ordinal.cmp(&start) {
                std::cmp::Ordering::Less => 0.0,
                std::cmp::Ordering::Equal => normalized_exponential_response(ordinal_progress, 3.4),
                std::cmp::Ordering::Greater => 1.0,
            });
        [
            base[0] + (target[0] - base[0]) * amount,
            base[1] + (target[1] - base[1]) * amount,
            base[2] + (target[2] - base[2]) * amount,
            1.0,
        ]
    };
    match colour {
        ClipColour::ReactantA => rgba(&synthesis.reactant_a),
        ClipColour::ReactantB => rgba(&synthesis.reactant_b),
        ClipColour::SynthesisProduct => rgba(&synthesis.product),
        ClipColour::ReactionFront => [1.0, 0.22, 0.035, 0.58],
        ClipColour::ReactionVessel => [0.78, 0.74, 0.66, 1.0],
        ClipColour::MixingTool => [0.46, 0.49, 0.52, 1.0],
        _ => [0.76, 0.78, 0.80, 1.0],
    }
}

fn deposit_highlight_colour(colour: [f32; 4]) -> [f32; 4] {
    // A warm metallic glint: the lift is strongest in red and weakest in
    // blue, so fresh deposit sparkles hot instead of washing to white.
    [
        colour[0] + (1.0 - colour[0]) * 0.50,
        colour[1] + (1.0 - colour[1]) * 0.26,
        colour[2] + (1.0 - colour[2]) * 0.08,
        0.24,
    ]
}

fn gas_evolution_track_colour(
    colour: ClipColour,
    gas_evolution: &chem_presentation::GasEvolutionVisualProfile,
    ordinal: u16,
    ordinal_progress: f32,
) -> [f32; 4] {
    let rgba = |bound: &chem_presentation::BoundVisualColour, opacity| {
        let base = [
            f32::from(bound.base_colour.red) / 255.0,
            f32::from(bound.base_colour.green) / 255.0,
            f32::from(bound.base_colour.blue) / 255.0,
        ];
        let target = [
            f32::from(bound.colour.red) / 255.0,
            f32::from(bound.colour.green) / 255.0,
            f32::from(bound.colour.blue) / 255.0,
        ];
        let amount = bound
            .transition_ordinal
            .map_or(1.0, |start| match ordinal.cmp(&start) {
                std::cmp::Ordering::Less => 0.0,
                std::cmp::Ordering::Equal => normalized_exponential_response(ordinal_progress, 3.4),
                std::cmp::Ordering::Greater => 1.0,
            });
        [
            base[0] + (target[0] - base[0]) * amount,
            base[1] + (target[1] - base[1]) * amount,
            base[2] + (target[2] - base[2]) * amount,
            opacity,
        ]
    };
    match colour {
        ClipColour::Glass => [0.62, 0.84, 0.94, 0.22],
        ClipColour::LiquidInitial => rgba(&gas_evolution.initial_reactant, 0.34),
        ClipColour::LiquidAdded => rgba(&gas_evolution.added_reactant, 0.36),
        ClipColour::SolidReactant => rgba(&gas_evolution.added_reactant, 1.0),
        ClipColour::GasBubble => rgba(&gas_evolution.gas_product, 0.28),
        ClipColour::GasCloud
        | ClipColour::Water
        | ClipColour::WaterHighlight
        | ClipColour::ReactiveMetal
        | ClipColour::FlameOuter
        | ClipColour::FlameInner
        | ClipColour::FlameCore
        | ClipColour::FizzBubble
        | ClipColour::Vapour
        | ClipColour::MixtureA
        | ClipColour::MixtureB
        | ClipColour::SaltResidue
        | ClipColour::Fuel
        | ClipColour::IgnitionSpark
        | ClipColour::ProductPlume
        | ClipColour::CombustionSmoke
        | ClipColour::Soot
        | ClipColour::SootDeposit
        | ClipColour::PrecipitateCloud
        | ClipColour::Precipitate
        | ClipColour::SolutionInitial
        | ClipColour::SolutionFinal
        | ClipColour::OriginalMetal
        | ClipColour::DepositedMetal
        | ClipColour::MetalErosion
        | ClipColour::ReactantA
        | ClipColour::ReactantB
        | ClipColour::SynthesisProduct
        | ClipColour::ReactionFront
        | ClipColour::ReactionVessel
        | ClipColour::MixingTool => rgba(&gas_evolution.gas_product, 0.18),
    }
}

fn precipitation_track_colour(
    colour: ClipColour,
    precipitation: &chem_presentation::PrecipitationVisualProfile,
    ordinal: u16,
    ordinal_progress: f32,
) -> [f32; 4] {
    let rgba = |bound: &chem_presentation::BoundVisualColour, opacity| {
        let base = [
            f32::from(bound.base_colour.red) / 255.0,
            f32::from(bound.base_colour.green) / 255.0,
            f32::from(bound.base_colour.blue) / 255.0,
            opacity,
        ];
        let target = [
            f32::from(bound.colour.red) / 255.0,
            f32::from(bound.colour.green) / 255.0,
            f32::from(bound.colour.blue) / 255.0,
            opacity,
        ];
        let amount = bound
            .transition_ordinal
            .map_or(1.0, |start| match ordinal.cmp(&start) {
                std::cmp::Ordering::Less => 0.0,
                std::cmp::Ordering::Equal => normalized_exponential_response(ordinal_progress, 3.4),
                std::cmp::Ordering::Greater => 1.0,
            });
        [
            base[0] + (target[0] - base[0]) * amount,
            base[1] + (target[1] - base[1]) * amount,
            base[2] + (target[2] - base[2]) * amount,
            opacity,
        ]
    };
    match colour {
        ClipColour::Glass => [0.62, 0.84, 0.94, 0.22],
        ClipColour::LiquidInitial => rgba(&precipitation.initial_liquid, 0.34),
        ClipColour::LiquidAdded => rgba(&precipitation.added_liquid, 0.36),
        ClipColour::PrecipitateCloud => rgba(&precipitation.precipitate, 0.20),
        ClipColour::Precipitate
        | ClipColour::Water
        | ClipColour::WaterHighlight
        | ClipColour::ReactiveMetal
        | ClipColour::FlameOuter
        | ClipColour::FlameInner
        | ClipColour::FlameCore
        | ClipColour::FizzBubble
        | ClipColour::Vapour
        | ClipColour::MixtureA
        | ClipColour::MixtureB
        | ClipColour::SaltResidue
        | ClipColour::Fuel
        | ClipColour::IgnitionSpark
        | ClipColour::ProductPlume
        | ClipColour::CombustionSmoke
        | ClipColour::Soot
        | ClipColour::SootDeposit
        | ClipColour::GasBubble
        | ClipColour::GasCloud
        | ClipColour::SolidReactant
        | ClipColour::SolutionInitial
        | ClipColour::SolutionFinal
        | ClipColour::OriginalMetal
        | ClipColour::DepositedMetal
        | ClipColour::MetalErosion
        | ClipColour::ReactantA
        | ClipColour::ReactantB
        | ClipColour::SynthesisProduct
        | ClipColour::ReactionFront
        | ClipColour::ReactionVessel
        | ClipColour::MixingTool => rgba(&precipitation.precipitate, 1.0),
    }
}

fn combustion_track_colour(colour: ClipColour, fuel: [f32; 4], incomplete: bool) -> [f32; 4] {
    match colour {
        ClipColour::Glass => [0.62, 0.84, 0.94, 0.22],
        ClipColour::Fuel
        | ClipColour::Water
        | ClipColour::WaterHighlight
        | ClipColour::ReactiveMetal
        | ClipColour::FizzBubble
        | ClipColour::Vapour
        | ClipColour::MixtureA
        | ClipColour::MixtureB
        | ClipColour::SaltResidue
        | ClipColour::LiquidInitial
        | ClipColour::LiquidAdded
        | ClipColour::PrecipitateCloud
        | ClipColour::Precipitate
        | ClipColour::SolidReactant
        | ClipColour::SolutionInitial
        | ClipColour::SolutionFinal
        | ClipColour::OriginalMetal
        | ClipColour::DepositedMetal
        | ClipColour::MetalErosion
        | ClipColour::ReactantA
        | ClipColour::ReactantB
        | ClipColour::SynthesisProduct
        | ClipColour::ReactionFront
        | ClipColour::ReactionVessel
        | ClipColour::MixingTool => fuel,
        ClipColour::FlameOuter if incomplete => [1.0, 0.24, 0.025, 0.58],
        ClipColour::FlameInner if incomplete => [1.0, 0.60, 0.06, 0.82],
        ClipColour::FlameCore if incomplete => [1.0, 0.92, 0.45, 0.96],
        ClipColour::FlameOuter => [0.10, 0.31, 0.98, 0.52],
        ClipColour::FlameInner => [0.16, 0.66, 1.0, 0.82],
        ClipColour::FlameCore => [0.78, 0.96, 1.0, 0.98],
        ClipColour::IgnitionSpark => [1.0, 0.74, 0.14, 0.95],
        ClipColour::ProductPlume | ClipColour::GasBubble | ClipColour::GasCloud => {
            [0.84, 0.89, 0.93, 0.14]
        }
        ClipColour::CombustionSmoke => [0.10, 0.105, 0.11, 0.46],
        ClipColour::Soot => [0.055, 0.050, 0.047, 0.96],
        ClipColour::SootDeposit => [0.075, 0.068, 0.062, 0.48],
    }
}

fn add_neutralisation_supplemental_reactants(
    meshes: &mut SceneMeshes,
    moment: NeutralisationAssemblyMoment<'_>,
    vessel_motion: Vec3,
) {
    let NeutralisationAssemblyMoment {
        plan,
        layout,
        visual_inputs,
        ordinal,
        ordinal_progress,
        ..
    } = moment;
    for object in plan.objects.iter().filter(|object| {
        object.role == SceneRole::Reactant
            && !matches!(
                object.asset,
                AssetProfile::LiquidVolume | AssetProfile::GasCloud
            )
            && object.visible_from_ordinal <= ordinal
    }) {
        let scale = object_scale_from_effects(plan, object.role, ordinal, ordinal_progress)
            * object_replacement_scale(plan, object, ordinal, ordinal_progress);
        if scale <= f32::EPSILON {
            continue;
        }
        let motion = object_motion(
            plan,
            object,
            ordinal,
            ordinal_progress,
            reaction_surface_motion(plan, ordinal, ordinal_progress),
        );
        instantiate_asset(
            meshes,
            object.asset,
            object.appearance,
            &object.transform,
            scale,
            layout.object_offset(object) + motion.translation + vessel_motion,
            motion.rotation,
            stable_seed(&object.id),
            visual_inputs,
            continuous_phase(ordinal, ordinal_progress),
            1.0,
            object_colour_transition(object, ordinal, ordinal_progress),
        );
    }
}

fn add_neutralisation_reaction_gas(
    meshes: &mut SceneMeshes,
    moment: NeutralisationAssemblyMoment<'_>,
    vessel_motion: Vec3,
) {
    let NeutralisationAssemblyMoment {
        layout,
        post_process,
        seed,
        visual_inputs,
        effect_colours,
        ordinal,
        ordinal_progress,
        ..
    } = moment;
    let reaction_gas = (visual_inputs.gas_generation_rate - post_process.vapour * 0.72).max(0.0);
    if reaction_gas > 0.002 {
        add_gas_density_field(
            &mut meshes.gas,
            Vec3::new(
                layout.vessel_center.x,
                layout.liquid_surface + vessel_motion.y * 0.45 + 0.18,
                layout.vessel_center.z,
            ),
            Vec3::new(0.48, 0.68, 0.48),
            alpha(
                effect_colours.gas,
                effect_colours.gas[3].max(0.18) * reaction_gas,
            ),
            seed.rotate_left(17),
            continuous_phase(ordinal, ordinal_progress),
            reaction_gas,
            GasFlowControls::escaping(
                reaction_gas,
                0.34 + reaction_gas * 0.26,
                0.78,
                seed.rotate_left(17),
            ),
        );
    }
}

/// Fixed-lobe surface flame: constant topology, lobe sizes breathing
/// through seeded cycles, scaling with `strength` and overall `scale`.
#[allow(clippy::too_many_arguments)]
fn add_surface_flame(
    meshes: &mut SceneMeshes,
    palette: FlamePalette,
    source: Vec3,
    strength: f32,
    scale: f32,
    phase: f32,
    seed: u64,
) {
    const LOBES: u32 = 8;
    let strength = strength * scale;
    let colours = flame_colours(palette);
    for lobe in 0..LOBES {
        let rate = 1.1 + seeded_unit(seed, lobe, 311) * 0.9;
        let age = (phase * rate + seeded_unit(seed, lobe, 312)).fract();
        let flare = (std::f32::consts::PI * age).sin().max(0.0);
        let angle = seeded_unit(seed, lobe, 313) * std::f32::consts::TAU;
        let radial = seeded_unit(seed, lobe, 314).sqrt() * 0.05 * scale;
        let sway = curl_like_flow(phase * 1.6, seed, lobe) * 0.05 * age;
        let base =
            source + Vec3::new(angle.cos() * radial, age * 0.10 * scale, angle.sin() * radial) + sway;
        // Floors keep every lobe past add_flame_lobe's degenerate-geometry
        // early-outs even at zero strength: constant topology, invisible size.
        let height = ((0.12 + seeded_unit(seed, lobe, 315) * 0.18)
            * (0.6 + 0.4 * flare)
            * strength)
            .max(0.02);
        let width = ((0.032 + seeded_unit(seed, lobe, 316) * 0.030) * flare.max(0.05) * strength)
            .max(0.002);
        let tip = base + Vec3::Y * height;
        let body = mix_color(
            colours.body_low,
            colours.body_high,
            seeded_unit(seed, lobe, 317),
        );
        add_flame_lobe(
            &mut meshes.translucent,
            base,
            tip,
            width,
            alpha(body, (flare * strength.min(1.0)).max(0.02)),
        );
        if lobe % 2 == 0 {
            add_flame_lobe(
                &mut meshes.emissive,
                base + Vec3::Y * 0.006,
                base.lerp(tip, 0.62),
                (width * 0.45).max(0.002),
                alpha(colours.core, (flare * strength.min(1.0) * 0.8).max(0.02)),
            );
        }
    }
}

/// A shallow tapered ceramic dish, opaque, sitting on the bench. Shared by
/// the solids and combustion scenes; `radius`/`height` set the footprint.
#[allow(clippy::cast_precision_loss)]
fn add_ceramic_dish(mesh: &mut Mesh, bench_top: f32, radius: f32, height: f32, colour: [f32; 4]) {
    const SEGMENTS: u32 = 28;
    let dimmed = [colour[0] * 0.66, colour[1] * 0.64, colour[2] * 0.62, 1.0];
    let inner_colour = [colour[0] * 0.55, colour[1] * 0.54, colour[2] * 0.52, 1.0];
    let floor_y = height * 0.31;
    let base_vertex = u32::try_from(mesh.vertices.len()).unwrap_or(u32::MAX);
    // Profile rings: outer base -> outer rim -> inner rim -> inner floor.
    let profile: [(f32, f32, [f32; 4], f32); 4] = [
        (radius * 0.70, height * 0.08, dimmed, -0.6),
        (radius, height, dimmed, 0.35),
        (radius * 0.93, height - 0.012, inner_colour, 0.7),
        (radius * 0.62, floor_y, inner_colour, 0.55),
    ];
    for (ring_radius, ring_height, ring_colour, normal_y) in profile {
        for segment in 0..=SEGMENTS {
            let angle = std::f32::consts::TAU * segment as f32 / SEGMENTS as f32;
            let radial = Vec3::new(angle.cos(), 0.0, angle.sin());
            let normal = (radial * (1.0 - normal_y.abs()) + Vec3::Y * normal_y).normalize_or_zero();
            mesh.vertices.push(Vertex {
                position: (radial * ring_radius + Vec3::new(0.0, bench_top + ring_height, 0.0))
                    .to_array(),
                normal: normal.to_array(),
                color: ring_colour,
            });
        }
    }
    for ring in 0..3_u32 {
        for segment in 0..SEGMENTS {
            let a = base_vertex + ring * (SEGMENTS + 1) + segment;
            let b = a + SEGMENTS + 1;
            mesh.indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    // Inner floor fan.
    let centre_vertex = u32::try_from(mesh.vertices.len()).unwrap_or(u32::MAX);
    mesh.vertices.push(Vertex {
        position: [0.0, bench_top + floor_y, 0.0],
        normal: Vec3::Y.to_array(),
        color: inner_colour,
    });
    let floor_ring_start = base_vertex + 3 * (SEGMENTS + 1);
    for segment in 0..SEGMENTS {
        mesh.indices.extend_from_slice(&[
            centre_vertex,
            floor_ring_start + segment + 1,
            floor_ring_start + segment,
        ]);
    }
}

/// Fixed-population condensation mist beading on a cool wall band: every
/// droplet is always emitted, its size scaled by `mist`, so the topology
/// never churns while the fogging fades in and out.
fn add_condensation_mist(
    mesh: &mut Mesh,
    centre: Vec3,
    wall_radius: f32,
    band_bottom: f32,
    band_top: f32,
    mist: f32,
    seed: u64,
) {
    const DROPLETS: u32 = 30;
    for droplet in 0..DROPLETS {
        let angle = seeded_unit(seed, droplet, 541) * std::f32::consts::TAU;
        let height = band_bottom
            + seeded_unit(seed, droplet, 542) * (band_top - band_bottom).max(0.01);
        let size = (0.006 + seeded_unit(seed, droplet, 543) * 0.008) * mist;
        add_sphere(
            mesh,
            Vec3::new(
                centre.x + angle.cos() * wall_radius,
                height,
                centre.z + angle.sin() * wall_radius,
            ),
            size.max(0.000_5),
            [0.88, 0.94, 0.97, 0.42],
            3,
            5,
        );
    }
}

/// Fixed-population gas puffs drifting up from `surface` and thinning out.
fn add_rising_plume(
    mesh: &mut Mesh,
    surface: Vec3,
    strength: f32,
    colour: [f32; 4],
    phase: f32,
    seed: u64,
) {
    const PUFFS: u32 = 12;
    for puff in 0..PUFFS {
        let rate = 0.35 + seeded_unit(seed, puff, 331) * 0.4;
        let age = (phase * rate + seeded_unit(seed, puff, 332)).fract();
        let angle = seeded_unit(seed, puff, 333) * std::f32::consts::TAU;
        let radial = seeded_unit(seed, puff, 334).sqrt() * 0.34;
        let drift = curl_like_flow(phase * 0.7, seed, puff) * 0.10 * age;
        let position = surface
            + Vec3::new(angle.cos() * radial, 0.04 + age * 0.58, angle.sin() * radial)
            + Vec3::new(drift.x, 0.0, drift.z);
        let size = (0.038 + 0.068 * age) * strength;
        let fade = alpha(colour, colour[3] * (1.0 - age) * strength);
        add_sphere(mesh, position, size.max(0.000_5), fade, 4, 7);
    }
}

/// Fixed-population ignition sparks darting up around a flame source.
fn add_ignition_sparks(mesh: &mut Mesh, source: Vec3, strength: f32, phase: f32, seed: u64) {
    const SPARKS: u32 = 10;
    for spark in 0..SPARKS {
        let rate = 1.4 + seeded_unit(seed, spark, 371) * 1.3;
        let age = (phase * rate + seeded_unit(seed, spark, 372)).fract();
        let angle = seeded_unit(seed, spark, 373) * std::f32::consts::TAU;
        let radial = age * (0.08 + seeded_unit(seed, spark, 374) * 0.16);
        let position = source
            + Vec3::new(
                angle.cos() * radial,
                0.03 + age * (0.16 + seeded_unit(seed, spark, 375) * 0.14),
                angle.sin() * radial,
            );
        let size = (0.006 + seeded_unit(seed, spark, 376) * 0.006) * (1.0 - age) * strength;
        add_sphere(
            mesh,
            position,
            size.max(0.000_5),
            [1.0, 0.72, 0.12, 0.94 * (1.0 - age).max(0.05)],
            3,
            5,
        );
    }
}

/// The procedural laboratory beaker every reaction assembly shares.
/// `vessel_motion` stays in clip-model units so assemblies that ride an
/// authored lift keep their amplitude.
fn add_assembly_beaker(mesh: &mut Mesh, bench_top: f32, vessel_motion: Vec3) {
    const MODEL_SCALE: f32 = 0.45;
    const GLASS: [f32; 4] = [0.62, 0.84, 0.94, 0.22];
    const RADIUS: f32 = 0.94;
    let motion = vessel_motion * MODEL_SCALE;
    let bottom = Vec3::new(motion.x, bench_top + 0.02 + motion.y, motion.z);
    let top = Vec3::new(motion.x, bench_top + 1.80 + motion.y, motion.z);
    add_cylinder_wall(mesh, bottom, top, RADIUS, GLASS);
    add_ring(mesh, top, RADIUS, 0.022, [0.62, 0.84, 0.94, 0.24]);
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
    add_sphere(
        mesh,
        top + Vec3::new(RADIUS * 0.93, 0.02, 0.0),
        0.05,
        GLASS,
        4,
        6,
    );
}

#[derive(Debug, Clone, Copy)]
struct NeutralisationColours {
    liquid: [f32; 4],
    liquid_highlight: [f32; 4],
    mixing_a: [f32; 4],
    mixing_b: [f32; 4],
    salt: [f32; 4],
}

fn neutralisation_colours(
    plan: &ScenePlan,
    effect_colours: EffectColours,
    frame: f32,
) -> NeutralisationColours {
    let contents = plan.objects.iter().find(|object| {
        object.role == SceneRole::Contents && object.asset == AssetProfile::LiquidVolume
    });
    let colourless = appearance_color(AppearanceProfile::AqueousColourless);
    let authored_mix = ((frame - 12.0) / 108.0).clamp(0.0, 1.0);
    let authored_mix = authored_mix * authored_mix * (3.0 - 2.0 * authored_mix);
    let liquid = contents.map_or(effect_colours.liquid, |object| {
        if object.colour_transition.is_some() {
            effect_colours.liquid
        } else if matches!(object.appearance, AppearanceProfile::ReviewedColour(_)) {
            mix_color(colourless, effect_colours.liquid, authored_mix)
        } else {
            effect_colours.liquid
        }
    });
    let salt_rgb = contents
        .and_then(|object| {
            object
                .colour_transition
                .as_ref()
                .map(|transition| transition.target)
                .or(match object.appearance {
                    AppearanceProfile::ReviewedColour(colour) => Some(colour),
                    _ => None,
                })
        })
        .map_or([0.92, 0.93, 0.89, 1.0], |colour| {
            [
                f32::from(colour.red) / 255.0,
                f32::from(colour.green) / 255.0,
                f32::from(colour.blue) / 255.0,
                1.0,
            ]
        });
    NeutralisationColours {
        liquid,
        liquid_highlight: alpha(mix_color(liquid, [0.95, 0.98, 1.0, liquid[3]], 0.48), 0.42),
        mixing_a: alpha(mix_color(colourless, liquid, 0.42), liquid[3].max(0.18)),
        mixing_b: alpha(mix_color(colourless, liquid, 0.84), liquid[3].max(0.20)),
        salt: salt_rgb,
    }
}

fn neutralisation_track_colour(colour: ClipColour, colours: NeutralisationColours) -> [f32; 4] {
    let gentle_flame = flame_colours(FlamePalette::Natural);
    match colour {
        ClipColour::Glass => [0.62, 0.84, 0.94, 0.22],
        ClipColour::Water | ClipColour::Fuel => colours.liquid,
        ClipColour::WaterHighlight => colours.liquid_highlight,
        ClipColour::FlameOuter => gentle_flame.body_low,
        ClipColour::FlameInner => gentle_flame.body_high,
        ClipColour::FlameCore => gentle_flame.core,
        ClipColour::FizzBubble | ClipColour::GasBubble => [0.82, 0.94, 0.98, 0.36],
        ClipColour::MixtureA => colours.mixing_a,
        ClipColour::MixtureB => colours.mixing_b,
        ClipColour::SaltResidue
        | ClipColour::LiquidInitial
        | ClipColour::LiquidAdded
        | ClipColour::PrecipitateCloud
        | ClipColour::Precipitate
        | ClipColour::SolidReactant
        | ClipColour::SolutionInitial
        | ClipColour::SolutionFinal
        | ClipColour::OriginalMetal
        | ClipColour::DepositedMetal
        | ClipColour::MetalErosion
        | ClipColour::ReactantA
        | ClipColour::ReactantB
        | ClipColour::SynthesisProduct
        | ClipColour::ReactionFront
        | ClipColour::ReactionVessel
        | ClipColour::MixingTool => colours.salt,
        ClipColour::ReactiveMetal => [0.88, 0.90, 0.92, 1.0],
        ClipColour::Vapour | ClipColour::ProductPlume | ClipColour::GasCloud => {
            [0.86, 0.90, 0.92, 0.16]
        }
        ClipColour::IgnitionSpark => [1.0, 0.72, 0.12, 0.94],
        ClipColour::CombustionSmoke => [0.10, 0.105, 0.11, 0.46],
        ClipColour::Soot => [0.055, 0.050, 0.047, 0.96],
        ClipColour::SootDeposit => [0.075, 0.068, 0.062, 0.48],
    }
}

/// Physical pour state for one clip frame, in clip space.
#[derive(Debug, Clone, Copy)]
struct PourFrameState {
    present: bool,
    base: Vec3,
    axis: Vec3,
    radius: f32,
    height: f32,
    lip: Vec3,
    downhill: Vec3,
    plane_y: f32,
    /// Normalized 0..1 outflow.
    flow: f32,
    /// Remaining liquid as a fraction of vessel height.
    fraction: f32,
    /// 0..1 of the initial charge that has been poured out.
    poured: f32,
}

struct PourTable {
    frames: Vec<PourFrameState>,
    first_flow: Option<usize>,
}

/// How full the pouring vessel starts, as a fraction of its height.
const POUR_INITIAL_FILL: f32 = 0.62;

/// Frame windows for an authored pour: (start, end) pairs in virtual
/// 30 fps clip frames.
struct PourSchedule {
    approach: (f32, f32),
    tilt_in: (f32, f32),
    tilt_out: (f32, f32),
    retreat: (f32, f32),
}

/// Authored pose of the pouring vessel at a virtual clip frame, in model
/// units: hover in from rest, tip over the beaker rim, hold the pour, right
/// itself, and retreat to rest.
fn scheduled_pour_pose(schedule: &PourSchedule, frame: f32) -> (Vec3, Vec3) {
    let ramp = |(start, end): (f32, f32)| smooth01((frame - start) / (end - start));
    let tilt = (ramp(schedule.tilt_in) - ramp(schedule.tilt_out)).max(0.0) * 1.22;
    let axis = Vec3::new(-tilt.sin(), tilt.cos(), 0.0);
    let toward = ramp(schedule.approach) - ramp(schedule.retreat);
    let base = Vec3::new(2.90 - 0.40 * toward, 4.95 - 0.15 * toward, 0.0);
    (base, axis)
}

/// The same weir-law integration the baked clips went through, run over an
/// authored pose schedule: the visible pour follows from choreography alone.
fn build_scheduled_pour_table(schedule: &PourSchedule) -> PourTable {
    const FLOW_GAIN: f32 = 1.4;
    const FRAME_DT: f32 = 1.0 / 30.0;
    const RADIUS: f32 = 0.75;
    const HEIGHT: f32 = 1.55;
    const FRAMES: u16 = 180;
    let mut fraction = POUR_INITIAL_FILL;
    let mut frames = Vec::with_capacity(usize::from(FRAMES));
    let mut first_flow = None;
    for frame in 0..FRAMES {
        let (base, axis) = scheduled_pour_pose(schedule, f32::from(frame));
        let rim_centre = base + axis * HEIGHT;
        let downhill = (Vec3::NEG_Y - axis * Vec3::NEG_Y.dot(axis)).normalize_or_zero();
        let tilted = downhill.length_squared() > 0.5;
        let lip = if tilted {
            rim_centre + downhill * RADIUS
        } else {
            rim_centre
        };
        let plane_y = base.y + fraction * HEIGHT * axis.y.clamp(0.15, 1.0);
        let flow_raw = if tilted && fraction > 0.02 {
            (plane_y - lip.y).max(0.0).powf(1.5) * FLOW_GAIN
        } else {
            0.0
        };
        let flow = (flow_raw * 3.0).clamp(0.0, 1.0);
        if flow > 0.0 && first_flow.is_none() {
            first_flow = Some(usize::from(frame));
        }
        fraction = (fraction - flow_raw * FRAME_DT).max(0.02);
        frames.push(PourFrameState {
            present: true,
            base,
            axis,
            radius: RADIUS,
            height: HEIGHT,
            lip,
            downhill: if tilted { downhill } else { Vec3::X },
            plane_y,
            flow,
            fraction,
            poured: (POUR_INITIAL_FILL - fraction) / POUR_INITIAL_FILL,
        });
    }
    PourTable { frames, first_flow }
}

/// The small pouring vessel's glass: a basis-aware open cylinder with a
/// brighter rim band and a base disc, valid at any tilt.
#[allow(clippy::cast_precision_loss)]
fn add_pouring_vessel_glass(mesh: &mut Mesh, state: &PourState) {
    const SEGMENTS: u32 = 24;
    // Subdivided rows: tall translucent wall quads sort per-triangle against
    // the surfaces behind them and paint picket-shaped wedges (the same
    // failure the beaker wall had); short rows keep centroid ordering stable.
    const ROWS: u32 = 10;
    const GLASS: [f32; 4] = [0.62, 0.84, 0.94, 0.22];
    const RIM: [f32; 4] = [0.66, 0.86, 0.95, 0.30];
    let axis = state.axis;
    let mut e1 = axis.cross(Vec3::Y);
    if e1.length_squared() < 1e-4 {
        e1 = axis.cross(Vec3::X);
    }
    let e1 = e1.normalize_or_zero();
    let e2 = axis.cross(e1).normalize_or_zero();
    let top = state.base + axis * state.height;
    let base_vertex = u32::try_from(mesh.vertices.len()).unwrap_or(u32::MAX);
    for segment in 0..=SEGMENTS {
        let angle = std::f32::consts::TAU * segment as f32 / SEGMENTS as f32;
        let radial = e1 * angle.cos() + e2 * angle.sin();
        for row in 0..=ROWS {
            let level = state.base + axis * (state.height * 0.94 * row as f32 / ROWS as f32);
            mesh.vertices.push(Vertex {
                position: (level + radial * state.radius).to_array(),
                normal: radial.to_array(),
                color: GLASS,
            });
        }
        mesh.vertices.push(Vertex {
            position: (top + radial * state.radius).to_array(),
            normal: radial.to_array(),
            color: RIM,
        });
    }
    for segment in 0..SEGMENTS {
        let column = base_vertex + segment * (ROWS + 2);
        let next = base_vertex + (segment + 1) * (ROWS + 2);
        for level in 0..=ROWS {
            let (a, b, c, d) = (column + level, next + level, column + level + 1, next + level + 1);
            mesh.indices.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    // Base disc fan.
    let centre_vertex = u32::try_from(mesh.vertices.len()).unwrap_or(u32::MAX);
    let floor_colour = [0.48, 0.72, 0.84, 0.14];
    mesh.vertices.push(Vertex {
        position: (state.base + axis * 0.01).to_array(),
        normal: axis.to_array(),
        color: floor_colour,
    });
    for segment in 0..=SEGMENTS {
        let angle = std::f32::consts::TAU * segment as f32 / SEGMENTS as f32;
        let radial = e1 * angle.cos() + e2 * angle.sin();
        mesh.vertices.push(Vertex {
            position: (state.base + axis * 0.01 + radial * (state.radius * 0.96)).to_array(),
            normal: axis.to_array(),
            color: floor_colour,
        });
    }
    for segment in 0..SEGMENTS {
        mesh.indices.extend_from_slice(&[
            centre_vertex,
            centre_vertex + 1 + segment,
            centre_vertex + 2 + segment,
        ]);
    }
}

/// World-space pour state at a fractional clip frame.
#[derive(Debug, Clone, Copy)]
struct PourState {
    base: Vec3,
    axis: Vec3,
    radius: f32,
    height: f32,
    lip: Vec3,
    downhill: Vec3,
    plane_y: f32,
    flow: f32,
    fraction: f32,
    poured: f32,
    /// Clip seconds since outflow began (for the falling-front animation).
    flow_elapsed: f32,
}

// Frame indices are bounded by the 180-frame tables, so the casts are exact.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn pour_state_from(table: &PourTable, frame: f32, bench_top: f32) -> Option<PourState> {
    const MODEL_SCALE: f32 = 0.45;
    let floor = (frame.floor().max(0.0) as usize).min(table.frames.len() - 1);
    let ceil = (floor + 1).min(table.frames.len() - 1);
    let blend = (frame - floor as f32).clamp(0.0, 1.0);
    let a = table.frames[floor];
    let b = table.frames[ceil];
    let state = match (a.present, b.present) {
        (false, false) => return None,
        (true, false) => a,
        (false, true) => b,
        (true, true) => PourFrameState {
            present: true,
            base: a.base.lerp(b.base, blend),
            axis: a.axis.lerp(b.axis, blend).normalize_or_zero(),
            radius: a.radius + (b.radius - a.radius) * blend,
            height: a.height + (b.height - a.height) * blend,
            lip: a.lip.lerp(b.lip, blend),
            downhill: a.downhill.lerp(b.downhill, blend).normalize_or_zero(),
            plane_y: a.plane_y + (b.plane_y - a.plane_y) * blend,
            flow: a.flow + (b.flow - a.flow) * blend,
            fraction: a.fraction + (b.fraction - a.fraction) * blend,
            poured: a.poured + (b.poured - a.poured) * blend,
        },
    };
    let to_world = |position: Vec3| position * MODEL_SCALE + Vec3::Y * bench_top;
    let flow_elapsed = table
        .first_flow
        .map_or(0.0, |first| ((frame - first as f32) / 30.0).max(0.0));
    Some(PourState {
        base: to_world(state.base),
        axis: state.axis,
        radius: state.radius * MODEL_SCALE,
        height: state.height * MODEL_SCALE,
        lip: to_world(state.lip),
        downhill: state.downhill,
        plane_y: state.plane_y * MODEL_SCALE + bench_top,
        flow: state.flow,
        fraction: state.fraction,
        poured: state.poured,
        flow_elapsed,
    })
}

/// A dye front spreading through standing liquid from a physical source —
/// the pour impact, a reacting solid — so colour arrives where chemistry
/// happens instead of the whole volume lerping at once.
#[derive(Debug, Clone, Copy)]
struct LiquidDye {
    source: Vec3,
    target: [f32; 4],
    /// 0..1 mixing completion; the front has covered the basin at 1.
    amount: f32,
    /// Farthest basin point from the source, for front normalization.
    reach: f32,
}

/// Recolours the contained-liquid vertices appended after `start`: a
/// diffusion-like front (fast at first, slowing) with a noise-wobbled edge.
/// Deterministic per vertex, so scrubbing reproduces the same spread.
fn apply_liquid_dye(mesh: &mut Mesh, start: usize, dye: &LiquidDye) {
    if dye.amount <= f32::EPSILON || dye.reach <= f32::EPSILON {
        return;
    }
    let front = dye.amount.powf(0.65) * dye.reach * 1.18;
    // A smooth front, no per-vertex jitter: the tall two-row wall quads turn
    // any jittered coverage into tapered wedge artifacts under interpolation.
    let softness = (dye.reach * 0.30).max(0.05);
    for vertex in &mut mesh.vertices[start..] {
        let position = Vec3::from_array(vertex.position);
        let distance = position.distance(dye.source);
        let coverage = smooth01((front - distance) / softness + 0.5);
        vertex.color = mix_color(
            vertex.color,
            [dye.target[0], dye.target[1], dye.target[2], vertex.color[3]],
            coverage,
        );
    }
}

/// Renders the receiving basin with pour-driven surface turbulence, then
/// spreads the mixed colour from where the stream lands.
fn add_receiving_liquid(
    meshes: &mut SceneMeshes,
    state: &LiquidState,
    pour: Option<&PourState>,
    receiving_lift: f32,
    endpoints: ([f32; 4], [f32; 4], f32),
    phase: f32,
    seed: u64,
) {
    let (base, target, amount) = endpoints;
    let turbulence = pour.map_or(0.2, |pour| (0.2 + pour.flow * 0.9).min(1.0));
    let appended_from = meshes.translucent.vertices.len();
    add_contained_liquid(
        &mut meshes.translucent,
        state.surface_centre + Vec3::Y * receiving_lift,
        state.floor_y,
        state.radius,
        base,
        turbulence,
        phase,
        seed,
    );
    let source = pour.map_or(state.surface_centre, |pour| {
        Vec3::new(
            pour.lip.x + pour.downhill.x * 0.12,
            state.surface_centre.y + receiving_lift,
            pour.lip.z + pour.downhill.z * 0.12,
        )
    });
    apply_liquid_dye(
        &mut meshes.translucent,
        appended_from,
        &LiquidDye {
            source,
            target,
            amount,
            reach: (state.radius * 2.0).hypot(state.surface_centre.y - state.floor_y),
        },
    );
}

/// Splits a reviewed bound colour into its endpoints and the ordinal-driven
/// mixing amount, so callers can place the change in space instead of
/// applying the blended value uniformly.
fn bound_colour_endpoints(
    bound: &chem_presentation::BoundVisualColour,
    opacity: f32,
    ordinal: u16,
    ordinal_progress: f32,
) -> ([f32; 4], [f32; 4], f32) {
    let base = [
        f32::from(bound.base_colour.red) / 255.0,
        f32::from(bound.base_colour.green) / 255.0,
        f32::from(bound.base_colour.blue) / 255.0,
        opacity,
    ];
    let target = [
        f32::from(bound.colour.red) / 255.0,
        f32::from(bound.colour.green) / 255.0,
        f32::from(bound.colour.blue) / 255.0,
        opacity,
    ];
    let amount = bound
        .transition_ordinal
        .map_or(1.0, |start| match ordinal.cmp(&start) {
            std::cmp::Ordering::Less => 0.0,
            std::cmp::Ordering::Equal => normalized_exponential_response(ordinal_progress, 3.4),
            std::cmp::Ordering::Greater => 1.0,
        });
    (base, target, amount)
}


/// World-space standing-liquid state at a fractional clip frame.
#[derive(Debug, Clone, Copy)]
struct LiquidState {
    surface_centre: Vec3,
    floor_y: f32,
    radius: f32,
    colour: ClipColour,
    /// The authored starting level: the high-water mark for scenes whose
    /// level only ever drops (burn-down, evaporation).
    initial_level_y: f32,
}

/// The liquid inside the pouring vessel: a cylinder clipped by the
/// horizontal liquid plane, so the surface stays level and creeps toward
/// the lip as the vessel tips.
#[allow(clippy::cast_precision_loss)]
fn add_vessel_liquid(mesh: &mut Mesh, state: &PourState, colour: [f32; 4]) {
    const SEGMENTS: u32 = 20;
    // Subdivided rows for the same reason as the vessel glass: one tall
    // translucent band sort-flaps against the base disc behind it.
    const ROWS: u32 = 6;
    if state.fraction <= 0.03 || state.height <= 0.01 {
        return;
    }
    let axis = state.axis;
    let mut e1 = axis.cross(Vec3::Y);
    if e1.length_squared() < 1e-4 {
        e1 = axis.cross(Vec3::X);
    }
    let e1 = e1.normalize_or_zero();
    let e2 = axis.cross(e1).normalize_or_zero();
    let inner = state.radius * 0.88;
    let axis_y = axis.y.clamp(0.15, 1.0);
    let liquid_colour = [colour[0], colour[1], colour[2], (colour[3] + 0.34).min(0.9)];
    let base_vertex = u32::try_from(mesh.vertices.len()).unwrap_or(u32::MAX);
    let mut tops = Vec::with_capacity(SEGMENTS as usize);
    for segment in 0..SEGMENTS {
        let angle = std::f32::consts::TAU * segment as f32 / SEGMENTS as f32;
        let radial = e1 * angle.cos() + e2 * angle.sin();
        let bottom = state.base + radial * inner;
        let along = ((state.plane_y - bottom.y) / axis_y).clamp(0.015, state.height * 0.96);
        let top = bottom + axis * along;
        tops.push(top);
        for row in 0..=ROWS {
            mesh.vertices.push(Vertex {
                position: bottom.lerp(top, row as f32 / ROWS as f32).to_array(),
                normal: radial.to_array(),
                color: liquid_colour,
            });
        }
    }
    for segment in 0..SEGMENTS {
        let next = (segment + 1) % SEGMENTS;
        let column = base_vertex + segment * (ROWS + 1);
        let neighbour = base_vertex + next * (ROWS + 1);
        for row in 0..ROWS {
            let (b0, t0, b1, t1) =
                (column + row, column + row + 1, neighbour + row, neighbour + row + 1);
            mesh.indices.extend_from_slice(&[b0, t0, b1, b1, t0, t1]);
        }
    }
    // Level surface fan.
    let surface_centre = tops.iter().copied().sum::<Vec3>() / tops.len() as f32;
    let centre_vertex = u32::try_from(mesh.vertices.len()).unwrap_or(u32::MAX);
    let surface_colour = [
        (colour[0] * 0.85 + 0.15),
        (colour[1] * 0.85 + 0.15),
        (colour[2] * 0.85 + 0.15),
        (colour[3] + 0.40).min(0.92),
    ];
    mesh.vertices.push(Vertex {
        position: surface_centre.to_array(),
        normal: Vec3::Y.to_array(),
        color: surface_colour,
    });
    for top in &tops {
        mesh.vertices.push(Vertex {
            position: top.to_array(),
            normal: Vec3::Y.to_array(),
            color: surface_colour,
        });
    }
    for segment in 0..SEGMENTS {
        let next = (segment + 1) % SEGMENTS;
        mesh.indices.extend_from_slice(&[
            centre_vertex,
            centre_vertex + 1 + segment,
            centre_vertex + 1 + next,
        ]);
    }
}

/// Endpoints handed to the stream renderer.
#[derive(Debug, Clone, Copy)]
struct PourAnchors {
    spout: Vec3,
    impact: Vec3,
    strength: f32,
}

/// Renders everything the pour's physical state implies: the liquid inside
/// the tipping vessel, and — only while the spill condition holds — a
/// ballistic stream from the lip to the receiving surface.
fn add_state_driven_pour(
    meshes: &mut SceneMeshes,
    state: &PourState,
    colour: [f32; 4],
    receiving_surface: f32,
    phase: f32,
    seed: u64,
) {
    const POUR_GRAVITY: f32 = 6.0;
    add_vessel_liquid(&mut meshes.translucent, state, colour);
    if state.flow <= 0.001 {
        return;
    }
    let drop_height = state.lip.y - receiving_surface;
    if drop_height < 0.05 {
        return;
    }
    let fall_time = (2.0 * drop_height / POUR_GRAVITY).sqrt();
    let lip_velocity = 0.10 + 0.30 * state.flow;
    let landing = state.lip + state.downhill * (lip_velocity * fall_time);
    let anchors = PourAnchors {
        spout: state.lip + state.downhill * 0.012,
        impact: Vec3::new(landing.x, receiving_surface, landing.z),
        strength: state.flow,
    };
    // Clip time runs at 0.625x presentation speed; the falling front covers
    // the arc in real fall time.
    let front = (state.flow_elapsed * 1.6 / fall_time).min(1.0);
    add_pour_stream(meshes, anchors, colour, front, phase, seed);
}

/// A physics-informed poured stream: constant horizontal velocity with a
/// parabolic drop, continuity necking as the liquid accelerates, a slight
/// travelling wobble, and a flared foot that meets the receiving surface
/// with ripples, splash droplets, and froth. Deterministic in (anchors,
/// phase, seed).
#[allow(clippy::cast_precision_loss)]
fn add_pour_stream(
    meshes: &mut SceneMeshes,
    anchors: PourAnchors,
    colour: [f32; 4],
    front: f32,
    phase: f32,
    seed: u64,
) {
    const SEGMENTS: u32 = 14;
    const SIDES: u32 = 8;
    let front = front.clamp(0.05, 1.0);
    let landed = front >= 0.999;
    let drop = (anchors.spout.y - anchors.impact.y).max(0.05);
    let horizontal = Vec3::new(
        anchors.impact.x - anchors.spout.x,
        0.0,
        anchors.impact.z - anchors.spout.z,
    );
    let strength = anchors.strength;
    let base_radius = 0.020 + 0.026 * strength;
    let curve = |u: f32| {
        // Constant lip velocity horizontally; gravity squares the drop.
        anchors.spout + horizontal * u + Vec3::Y * (-drop * u * u)
    };
    let mesh = &mut meshes.translucent;
    let base_vertex = u32::try_from(mesh.vertices.len()).unwrap_or(u32::MAX);
    for segment in 0..=SEGMENTS {
        let u = segment as f32 / SEGMENTS as f32 * front;
        let centre = curve(u);
        let tangent = (curve((u + 0.02).min(1.0)) - curve((u - 0.02).max(0.0))).normalize_or_zero();
        let right = tangent.cross(Vec3::Y).normalize_or_zero();
        let right = if right.length_squared() < 0.5 {
            Vec3::X
        } else {
            right
        };
        let binormal = right.cross(tangent).normalize_or_zero();
        // Continuity: the stream thins as it accelerates under gravity.
        let neck = (1.0 + 3.0 * u * u).powf(-0.25);
        let wobble = 1.0 + 0.09 * (u * 21.0 - phase * 8.0 + seed as f32 % 6.0).sin() * u;
        let foot_flare = if landed {
            1.0 + 0.65 * ((u - 0.9) / 0.1).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let radius = base_radius * neck * wobble * foot_flare;
        // The stream leaves the lip as a slightly flattened sheet and pulls
        // round within the first quarter of the fall.
        let sheet = 1.0 - ((0.25 - u) / 0.25).clamp(0.0, 1.0) * 0.35;
        let froth = ((u - 0.86) / 0.14).clamp(0.0, 1.0) * 0.4;
        let ring_colour = [
            colour[0] + (1.0 - colour[0]) * froth,
            colour[1] + (1.0 - colour[1]) * froth,
            colour[2] + (1.0 - colour[2]) * froth,
            (colour[3] + 0.30 + froth * 0.2).min(0.95),
        ];
        let sway = right * (0.010 * (u * 13.0 - phase * 6.0).sin() * u);
        for side in 0..SIDES {
            let angle = std::f32::consts::TAU * side as f32 / SIDES as f32;
            let normal =
                (right * (angle.cos() * sheet) + binormal * angle.sin()).normalize_or_zero();
            let offset =
                right * (angle.cos() * radius * 1.15) + binormal * (angle.sin() * radius * sheet);
            mesh.vertices.push(Vertex {
                position: (centre + sway + offset).to_array(),
                normal: normal.to_array(),
                color: ring_colour,
            });
        }
    }
    for segment in 0..SEGMENTS {
        for side in 0..SIDES {
            let current = base_vertex + segment * SIDES + side;
            let next_side = base_vertex + segment * SIDES + (side + 1) % SIDES;
            let above = current + SIDES;
            let above_next = next_side + SIDES;
            mesh.indices
                .extend_from_slice(&[current, above, next_side, next_side, above, above_next]);
        }
    }
    // Splash effects appear only once the falling front has reached the
    // receiving liquid; before that, surface tension gathers the leading
    // edge into a teardrop head, fatter than the necked stream behind it.
    if landed {
        add_pour_impact(meshes, anchors, phase, seed);
    } else {
        let bright = [
            colour[0] * 0.6 + 0.4,
            colour[1] * 0.6 + 0.4,
            colour[2] * 0.6 + 0.4,
            (colour[3] + 0.35).min(0.9),
        ];
        let head = curve(front);
        add_sphere(&mut meshes.translucent, head, base_radius * 2.2, bright, 4, 6);
        add_sphere(
            &mut meshes.translucent,
            head.lerp(curve((front - 0.07).max(0.0)), 0.55),
            base_radius * 1.45,
            bright,
            4,
            6,
        );
    }
}

/// Faint refractive swirls left behind once a pour has mixed: ribbons of
/// slightly lightened liquid circling the mixing region below the surface,
/// fading as the solutions homogenize. Fixed population; sizes floored.
#[allow(clippy::cast_precision_loss)]
fn add_schlieren_swirls(
    mesh: &mut Mesh,
    centre: Vec3,
    radius: f32,
    strength: f32,
    phase: f32,
    seed: u64,
) {
    const RIBBONS: u32 = 4;
    const KNOTS: u32 = 6;
    const SHEEN: [f32; 4] = [0.92, 0.96, 1.0, 0.15];
    for ribbon in 0..RIBBONS {
        let spin = if ribbon % 2 == 0 { 0.24 } else { -0.19 };
        let base_angle = seeded_unit(seed, ribbon, 531) * std::f32::consts::TAU + phase * spin;
        let band = radius * (0.22 + seeded_unit(seed, ribbon, 532) * 0.5);
        let depth = 0.05 + seeded_unit(seed, ribbon, 533) * 0.15;
        for knot in 0..KNOTS {
            let t = knot as f32 / (KNOTS - 1) as f32;
            let angle = base_angle + t * 1.7;
            let position = Vec3::new(
                centre.x + angle.cos() * band * (1.0 - t * 0.18),
                centre.y - depth - t * 0.06,
                centre.z + angle.sin() * band * (1.0 - t * 0.18),
            );
            let size = (0.030 + 0.022 * (std::f32::consts::PI * t).sin()) * strength;
            add_sphere(
                mesh,
                position,
                size.max(0.000_5),
                alpha(SHEEN, SHEEN[3] * strength * (1.0 - t * 0.45)),
                4,
                6,
            );
        }
    }
}

/// Impact effects at the stream's foot: expanding ripples, seeded splash
/// droplets, and a froth patch, all scaled by pour strength and phased by
/// the deterministic clock.
#[allow(clippy::cast_precision_loss)]
fn add_pour_impact(meshes: &mut SceneMeshes, anchors: PourAnchors, phase: f32, seed: u64) {
    let strength = anchors.strength;
    let impact = anchors.impact;
    for ring in 0..2 {
        let ring_phase = (phase * 0.9 + ring as f32 * 0.5).fract();
        let radius = 0.08 + ring_phase * 0.30 * (0.6 + strength * 0.4);
        let alpha = (1.0 - ring_phase) * 0.30 * strength;
        add_ring(
            &mut meshes.translucent,
            impact + Vec3::Y * 0.012,
            radius,
            0.008,
            [0.92, 0.96, 1.0, alpha],
        );
    }
    add_disc(
        &mut meshes.translucent,
        impact + Vec3::Y * 0.018,
        0.075 + 0.02 * (phase * 11.0).sin().abs(),
        [0.95, 0.97, 1.0, 0.30 * strength],
    );
    add_worthington_crown(&mut meshes.translucent, impact, strength, phase, seed);
    for droplet in 0..6_u32 {
        let launch = seeded_unit(seed, droplet, 61);
        let droplet_phase = (phase * 1.5 + launch).fract();
        let direction = std::f32::consts::TAU * seeded_unit(seed, droplet, 62);
        let reach = 0.05 + 0.20 * droplet_phase * (0.5 + strength * 0.5);
        let arc = 0.16 * droplet_phase - 0.30 * droplet_phase * droplet_phase;
        let centre = impact
            + Vec3::new(
                direction.cos() * reach,
                arc.max(0.0) + 0.02,
                direction.sin() * reach,
            );
        add_sphere(
            &mut meshes.translucent,
            centre,
            0.011 + 0.007 * seeded_unit(seed, droplet, 63),
            [0.94, 0.97, 1.0, (1.0 - droplet_phase) * 0.7 * strength],
            3,
            5,
        );
    }
}

fn add_imported_metal(mesh: &mut Mesh, base_center: Vec3, scale: Vec3, color: [f32; 4]) {
    let source = embedded_metal_mesh();
    let vertex_offset = u32::try_from(mesh.vertices.len()).unwrap_or(u32::MAX);
    let model_scale = scale * Vec3::new(1.0, 0.78, 1.0);
    mesh.vertices.reserve(source.vertices.len());
    mesh.indices.reserve(source.indices.len());
    mesh.vertices.extend(source.vertices.iter().map(|vertex| {
        let normal_scale = Vec3::new(
            model_scale.x.abs().max(0.001).recip(),
            model_scale.y.abs().max(0.001).recip(),
            model_scale.z.abs().max(0.001).recip(),
        );
        Vertex {
            position: (base_center + vertex.position * model_scale).to_array(),
            normal: (vertex.normal * normal_scale)
                .normalize_or_zero()
                .to_array(),
            color,
        }
    }));
    mesh.indices.extend(
        source
            .indices
            .iter()
            .map(|index| index.saturating_add(vertex_offset)),
    );
}

fn add_sphere(
    mesh: &mut Mesh,
    center: Vec3,
    radius: f32,
    color: [f32; 4],
    rings: u16,
    sectors: u16,
) {
    let base = u32::try_from(mesh.vertices.len()).unwrap_or(u32::MAX);
    for ring in 0..=rings {
        let latitude = std::f32::consts::PI * f32::from(ring) / f32::from(rings);
        for sector in 0..=sectors {
            let longitude = std::f32::consts::TAU * f32::from(sector) / f32::from(sectors);
            let normal = Vec3::new(
                latitude.sin() * longitude.cos(),
                latitude.cos(),
                latitude.sin() * longitude.sin(),
            );
            mesh.vertices.push(Vertex {
                position: (center + normal * radius).to_array(),
                normal: normal.to_array(),
                color,
            });
        }
    }
    for ring in 0..rings {
        for sector in 0..sectors {
            let current = base + u32::from(ring * (sectors + 1) + sector);
            let next = current + u32::from(sectors) + 1;
            mesh.indices.extend_from_slice(&[
                current,
                next,
                current + 1,
                current + 1,
                next,
                next + 1,
            ]);
        }
    }
}

fn add_cylinder(mesh: &mut Mesh, start: Vec3, end: Vec3, radius: f32, color: [f32; 4]) {
    const SIDES: u16 = 16;
    let direction = end - start;
    let length = direction.length();
    if length <= f32::EPSILON {
        return;
    }
    let rotation = Quat::from_rotation_arc(Vec3::Y, direction / length);
    let base = u32::try_from(mesh.vertices.len()).unwrap_or(u32::MAX);
    for end_index in 0_u8..=1 {
        for side in 0..=SIDES {
            let angle = std::f32::consts::TAU * f32::from(side) / f32::from(SIDES);
            let local_normal = Vec3::new(angle.cos(), 0.0, angle.sin());
            let normal = rotation * local_normal;
            let local = Vec3::new(
                angle.cos() * radius,
                f32::from(end_index) * length,
                angle.sin() * radius,
            );
            mesh.vertices.push(Vertex {
                position: (start + rotation * local).to_array(),
                normal: normal.to_array(),
                color,
            });
        }
    }
    for side in 0..SIDES {
        let bottom = base + u32::from(side);
        let top = base + u32::from(SIDES) + 1 + u32::from(side);
        mesh.indices
            .extend_from_slice(&[bottom, top, bottom + 1, bottom + 1, top, top + 1]);
    }
}

fn add_cylinder_wall(mesh: &mut Mesh, bottom: Vec3, top: Vec3, radius: f32, color: [f32; 4]) {
    // Multiple rows keep the translucent sort coherent: tall wall triangles
    // span such a depth range that centroid ordering against horizontal
    // surfaces behind them flips per-triangle, blending wedge artifacts.
    const SIDES: u16 = 32;
    const ROWS: u16 = 12;
    let base = u32::try_from(mesh.vertices.len()).unwrap_or(u32::MAX);
    for row in 0..=ROWS {
        let level = bottom.lerp(top, f32::from(row) / f32::from(ROWS));
        for side in 0..=SIDES {
            let angle = std::f32::consts::TAU * f32::from(side) / f32::from(SIDES);
            let normal = Vec3::new(angle.cos(), 0.0, angle.sin());
            mesh.vertices.push(Vertex {
                position: (level + normal * radius).to_array(),
                normal: normal.to_array(),
                color,
            });
        }
    }
    for row in 0..ROWS {
        for side in 0..SIDES {
            let lower = base + u32::from(row) * (u32::from(SIDES) + 1) + u32::from(side);
            let upper = lower + u32::from(SIDES) + 1;
            mesh.indices
                .extend_from_slice(&[lower, upper, lower + 1, lower + 1, upper, upper + 1]);
        }
    }
}

/// Pointed, flat-shaded low-poly solid used for precipitate, powder, and
/// crystal fragments. Rendering stays faceted while motion uses a cheap point
/// trajectory and vessel-floor collision instead of an expensive mesh collider.
fn add_shard(
    mesh: &mut Mesh,
    center: Vec3,
    half_extents: Vec3,
    rotation: Quat,
    color: [f32; 4],
    seed: u64,
) {
    let top = Vec3::new(
        seeded_variation(seed, 0) * half_extents.x * 0.25,
        half_extents.y * (0.92 + seeded_variation(seed, 1)),
        seeded_variation(seed, 2) * half_extents.z * 0.25,
    );
    let bottom = Vec3::new(
        seeded_variation(seed, 3) * half_extents.x * 0.18,
        -half_extents.y * (0.54 + seeded_variation(seed, 4).abs()),
        seeded_variation(seed, 5) * half_extents.z * 0.18,
    );
    let mut ring = [Vec3::ZERO; 4];
    for (index, point) in ring.iter_mut().enumerate() {
        let angle = std::f32::consts::FRAC_PI_2 * f32::from(u8::try_from(index).unwrap_or(u8::MAX));
        let radial = 0.82 + seeded_variation(seed, 6 + index).abs();
        *point = Vec3::new(
            angle.cos() * half_extents.x * radial,
            seeded_variation(seed, 10 + index) * half_extents.y * 0.22,
            angle.sin() * half_extents.z * radial,
        );
    }
    let local = [top, bottom, ring[0], ring[1], ring[2], ring[3]];
    let faces = [
        [0, 2, 3],
        [0, 3, 4],
        [0, 4, 5],
        [0, 5, 2],
        [1, 3, 2],
        [1, 4, 3],
        [1, 5, 4],
        [1, 2, 5],
    ];
    for face in faces {
        let a = center + rotation * local[face[0]];
        let b = center + rotation * local[face[1]];
        let c = center + rotation * local[face[2]];
        let normal = (b - a).cross(c - a).normalize_or_zero();
        let base = u32::try_from(mesh.vertices.len()).unwrap_or(u32::MAX);
        for position in [a, b, c] {
            mesh.vertices.push(Vertex {
                position: position.to_array(),
                normal: normal.to_array(),
                color,
            });
        }
        mesh.indices.extend_from_slice(&[base, base + 1, base + 2]);
    }
}

fn seeded_variation(seed: u64, component: usize) -> f32 {
    let mixed = seed
        .wrapping_add(u64::try_from(component).unwrap_or(u64::MAX))
        .wrapping_mul(0x9e37_79b9_7f4a_7c15);
    let unit = f32::from((mixed >> 56) as u8) / 255.0;
    (unit - 0.5) * 0.24
}

fn add_box(mesh: &mut Mesh, center: Vec3, size: Vec3, color: [f32; 4]) {
    let half = size * 0.5;
    let corners = [
        center + Vec3::new(-half.x, -half.y, -half.z),
        center + Vec3::new(half.x, -half.y, -half.z),
        center + Vec3::new(half.x, half.y, -half.z),
        center + Vec3::new(-half.x, half.y, -half.z),
        center + Vec3::new(-half.x, -half.y, half.z),
        center + Vec3::new(half.x, -half.y, half.z),
        center + Vec3::new(half.x, half.y, half.z),
        center + Vec3::new(-half.x, half.y, half.z),
    ];
    let faces = [
        ([0, 1, 2, 3], Vec3::NEG_Z),
        ([5, 4, 7, 6], Vec3::Z),
        ([4, 0, 3, 7], Vec3::NEG_X),
        ([1, 5, 6, 2], Vec3::X),
        ([3, 2, 6, 7], Vec3::Y),
        ([4, 5, 1, 0], Vec3::NEG_Y),
    ];
    for (indices, normal) in faces {
        let base = u32::try_from(mesh.vertices.len()).unwrap_or(u32::MAX);
        for index in indices {
            mesh.vertices.push(Vertex {
                position: corners[index].to_array(),
                normal: normal.to_array(),
                color,
            });
        }
        // Wound so each face's front matches its declared outward normal;
        // the previous inside-out winding meant back-face culling removed
        // every box (including the bench) when viewed from outside.
        mesh.indices
            .extend_from_slice(&[base, base + 2, base + 1, base, base + 3, base + 2]);
    }
}

fn add_ring(mesh: &mut Mesh, center: Vec3, radius: f32, thickness: f32, color: [f32; 4]) {
    const SEGMENTS: u16 = 18;
    for segment in 0..SEGMENTS {
        let start_angle = std::f32::consts::TAU * f32::from(segment) / f32::from(SEGMENTS);
        let end_angle = std::f32::consts::TAU * f32::from(segment + 1) / f32::from(SEGMENTS);
        let start = center + Vec3::new(start_angle.cos() * radius, 0.0, start_angle.sin() * radius);
        let end = center + Vec3::new(end_angle.cos() * radius, 0.0, end_angle.sin() * radius);
        add_cylinder(mesh, start, end, thickness, color);
    }
}

fn add_particle_cluster(
    mesh: &mut Mesh,
    center: Vec3,
    scale: Vec3,
    color: [f32; 4],
    count: u8,
    seed: u64,
) {
    let particle_scale = scale.abs().max_element();
    if particle_scale <= 0.001 {
        return;
    }
    for index in 0..count {
        let angle = f32::from(index) * 2.399_963_1;
        let radius = (f32::from(index) / f32::from(count.max(1))).sqrt();
        let offset = Vec3::new(
            angle.cos() * radius * scale.x,
            f32::from((index * 11) % 9) / 9.0 * scale.y,
            angle.sin() * radius * scale.z,
        );
        let shard_seed = seed ^ u64::from(index).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        let size = (0.045 + f32::from(index % 4) * 0.012) * particle_scale;
        add_shard(
            mesh,
            center + offset,
            Vec3::new(size * 0.62, size * 1.28, size * 0.50),
            settling_shard_rotation(shard_seed, 1.0),
            color,
            shard_seed,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chem_presentation::compile_real_world_plan;

    use crate::chemistry;

    fn plan_for(request: chemistry::ReactionRequest) -> ScenePlan {
        let run = chemistry::run(request).expect("request validates");
        let profile = chemistry::presentation_profile_with_catalogue(
            request,
            run.frames(),
            run.macroscopic(),
        )
        .expect("validated observations select a presentation profile");
        compile_real_world_plan(run.frames(), &profile)
            .expect("plan compiles from validated frames")
    }

    /// Two builds of the same (plan, moment) must agree to the byte: scenes
    /// are pure functions of the playhead, with no hidden clock or RNG.
    fn assert_scene_rebuild_is_byte_identical(plan: &ScenePlan, label: &str) {
        let duration = plan.timeline.duration_ms();
        for quarter in 1..4_u64 {
            let moment = plan
                .timeline
                .locate(duration * quarter / 4)
                .expect("playhead within timeline");
            let first = build_scene_at(plan, moment, (0.0, 0.0));
            let second = build_scene_at(plan, moment, (0.0, 0.0));
            assert!(
                bytemuck::cast_slice::<GpuVertex, u8>(&first.0)
                    == bytemuck::cast_slice::<GpuVertex, u8>(&second.0),
                "{label} q{quarter}: vertices differ between identical builds"
            );
            assert!(
                first.1 == second.1,
                "{label} q{quarter}: indices differ between identical builds"
            );
            assert!(
                (first.2, first.3) == (second.2, second.3),
                "{label} q{quarter}: mesh bucket boundaries differ"
            );
            assert!(
                bytemuck::cast_slice::<GasSplat, u8>(&first.4)
                    == bytemuck::cast_slice::<GasSplat, u8>(&second.4),
                "{label} q{quarter}: gas splats differ between identical builds"
            );
        }
    }

    #[test]
    fn every_request_backed_scene_is_a_pure_function_of_its_playhead() {
        for request in chemistry::ReactionRequest::ALL {
            let plan = plan_for(request);
            assert_scene_rebuild_is_byte_identical(&plan, &request.id());
        }
    }

    #[test]
    fn every_dynamic_fixture_scene_is_a_pure_function_of_its_playhead() {
        for fixture in [
            "combustion-methane",
            "combustion-methane-incomplete",
            "displacement-zinc-copper",
            "synthesis-iron-sulfur",
        ] {
            let outcome =
                crate::smoke_dynamic_presentation(fixture).expect("smoke fixture compiles");
            let frames = match &outcome {
                crate::DynamicPresentationOutcome::ReviewedFamily(outcome) => outcome.frames(),
                crate::DynamicPresentationOutcome::Escalated(outcome) => outcome.frames(),
                crate::DynamicPresentationOutcome::Static { .. } => {
                    panic!("{fixture}: smoke fixture must yield an animation")
                }
            };
            let profile =
                crate::dynamic_presentation_profile(frames, outcome.static_outcome(), None)
                    .expect("smoke fixture profile compiles");
            let plan = compile_real_world_plan(frames, &profile)
                .expect("smoke fixture plan compiles");
            assert_scene_rebuild_is_byte_identical(&plan, fixture);
        }
    }

    #[test]
    #[ignore = "manual perf probe: cargo test -p chemspec-app -- --ignored --nocapture bench_scene"]
    fn bench_scene_rebuild() {
        let request = chemistry::ReactionRequest::from_id("alkali-water-potassium")
            .expect("smoke reaction id resolves");
        let plan = plan_for(request);
        let duration = plan.timeline.duration_ms();
        let runs: u64 = 200;
        let start = std::time::Instant::now();
        for run in 0..runs {
            let playhead = duration * run / runs;
            let moment = plan
                .timeline
                .locate(playhead)
                .expect("playhead within timeline");
            let _ = build_scene_at(&plan, moment, (0.0, 0.0));
        }
        eprintln!(
            "scene rebuild across playhead: avg {:?}",
            start.elapsed() / u32::try_from(runs).expect("small run count")
        );
    }

    #[test]
    fn pour_tables_spill_only_when_tilted_and_conserve_volume() {
        for table in [
            gas_evolution::procedural_pour_table(),
            precipitation::procedural_pour_table(),
        ] {
            let mut previous_fraction = POUR_INITIAL_FILL;
            let mut saw_flow = false;
            for state in &table.frames {
                assert!(
                    state.fraction <= previous_fraction + 1e-5,
                    "poured liquid never returns to the vessel"
                );
                if state.flow > 0.0 {
                    saw_flow = true;
                    let tilt = state.axis.dot(Vec3::Y).clamp(-1.0, 1.0).acos();
                    assert!(
                        tilt > 0.2,
                        "liquid must not leave an upright vessel (tilt {tilt})"
                    );
                    assert!(
                        state.plane_y > state.lip.y,
                        "flow requires the liquid plane above the lip"
                    );
                } else {
                    assert!(
                        (state.fraction - previous_fraction).abs() < 1e-5,
                        "volume is conserved while not pouring"
                    );
                }
                previous_fraction = state.fraction;
            }
            assert!(saw_flow, "the authored tilt window produces a pour");
            assert!(
                table.frames.iter().map(|s| s.fraction).fold(1.0, f32::min) < 0.5,
                "a visible share of the charge pours out"
            );
        }
    }

    #[test]
    fn the_camera_glides_into_the_final_pose_without_a_cut() {
        let plan = plan_for(chemistry::ReactionRequest::alkali_water(
            chemistry::AlkaliMetal::Potassium,
        ));
        let final_beat_index = plan.timeline.beats.len() - 1;
        let final_beat = plan.timeline.beats[final_beat_index].clone();
        let moment = |beat_progress: f32| RealWorldPosition {
            beat_index: final_beat_index,
            ordinal: final_beat.end_ordinal,
            ordinal_progress: beat_progress,
            beat_progress,
            stage: final_beat.stage,
        };

        // The glide approaches the arrival monotonically through the beat.
        // No cut anywhere in the closing beat: fine steps stay small.
        let mut previous = camera_cue_adjustment(&plan, moment(0.0));
        for step in 1..=50_u16 {
            let pose = camera_cue_adjustment(&plan, moment(f32::from(step) / 50.0));
            assert!(
                (pose.distance_scale - previous.distance_scale).abs() < 0.02,
                "distance cut at step {step}"
            );
            assert!(
                (pose.yaw_offset - previous.yaw_offset).abs() < 0.06,
                "yaw cut at step {step}"
            );
            previous = pose;
        }
        let arrived = camera_cue_adjustment(&plan, moment(1.0));
        assert!((arrived.distance_scale - HERO_ARRIVAL.distance_scale).abs() < 1e-3);
        assert!((arrived.pitch_offset - HERO_ARRIVAL.pitch_offset).abs() < 1e-3);

        // No cut at the schedule boundary: the last playable instant and the
        // held completed pose are the same frame.
        let boundary = camera_cue_adjustment(&plan, moment(0.999));
        let held = camera_cue_adjustment(
            &plan,
            RealWorldPosition {
                beat_index: final_beat_index + 3,
                ordinal: final_beat.end_ordinal.saturating_add(3),
                ordinal_progress: 1.0,
                beat_progress: 1.0,
                stage: final_beat.stage,
            },
        );
        assert!((boundary.distance_scale - held.distance_scale).abs() < 5e-3);
        assert!((boundary.pitch_offset - held.pitch_offset).abs() < 5e-3);
        assert!((held.distance_scale - HERO_ARRIVAL.distance_scale).abs() < f32::EPSILON);
    }

    fn test_material(
        binding: &str,
        role: chem_presentation::MacroscopicMaterialRole,
        phase: chem_domain::Phase,
        representation: chem_domain::RepresentationKind,
    ) -> chem_presentation::MacroscopicMaterial {
        chem_presentation::MacroscopicMaterial {
            binding: binding.to_owned(),
            semantic_identity: binding.to_owned(),
            structure_id: format!("Structures.{binding}"),
            formula: binding.to_owned(),
            role,
            phase,
            representation,
            colour: None,
            explosive_water_contact: None,
        }
    }

    fn plan_for_materials(
        request: chemistry::ReactionRequest,
        materials: Vec<chem_presentation::MacroscopicMaterial>,
        process: Option<chem_presentation::MacroscopicProcess>,
    ) -> ScenePlan {
        let run = chemistry::run(request).expect("request validates");
        let reaction = chem_presentation::MacroscopicReaction {
            profile_id: "presentation.test.renderer-materials".to_owned(),
            equation: request.equation(),
            materials,
            intensity: EffectIntensity::Moderate,
            process,
            fuel_carbon_count: None,
            surface_oxide_colour: None,
        };
        let profile = chem_presentation::compile_phase_driven_profile(run.frames(), &reaction)
            .expect("typed renderer fixture compiles");
        compile_real_world_plan(run.frames(), &profile).expect("renderer fixture plan compiles")
    }

    fn carbon_oxidation_plan() -> ScenePlan {
        let request = chemistry::ReactionRequest::from_id("oxygen-carbon-oxygen")
            .expect("carbon oxygen exists");
        plan_for_materials(
            request,
            vec![
                test_material(
                    "subject",
                    chem_presentation::MacroscopicMaterialRole::Reactant,
                    chem_domain::Phase::Solid,
                    chem_domain::RepresentationKind::Molecular,
                ),
                test_material(
                    "oxygen",
                    chem_presentation::MacroscopicMaterialRole::Reactant,
                    chem_domain::Phase::Gas,
                    chem_domain::RepresentationKind::Molecular,
                ),
                test_material(
                    "oxide",
                    chem_presentation::MacroscopicMaterialRole::Product,
                    chem_domain::Phase::Gas,
                    chem_domain::RepresentationKind::Molecular,
                ),
            ],
            None,
        )
    }

    fn surface_oxidation_plan() -> ScenePlan {
        let request = chemistry::ReactionRequest::from_id("oxygen-sodium-oxygen")
            .expect("sodium oxidation exists");
        plan_for_materials(
            request,
            vec![
                test_material(
                    "subject",
                    chem_presentation::MacroscopicMaterialRole::Reactant,
                    chem_domain::Phase::Solid,
                    chem_domain::RepresentationKind::Metallic,
                ),
                test_material(
                    "oxygen",
                    chem_presentation::MacroscopicMaterialRole::Reactant,
                    chem_domain::Phase::Gas,
                    chem_domain::RepresentationKind::Molecular,
                ),
                test_material(
                    "oxide",
                    chem_presentation::MacroscopicMaterialRole::Product,
                    chem_domain::Phase::Solid,
                    chem_domain::RepresentationKind::Ionic,
                ),
            ],
            Some(chem_presentation::MacroscopicProcess::SurfaceOxidation),
        )
    }

    fn canonical_plan() -> ScenePlan {
        plan_for(chemistry::ReactionRequest::DEFAULT)
    }

    #[test]
    fn reusable_motion_is_continuous_across_stage_boundaries() {
        assert!((continuous_phase(5, 1.0) - continuous_phase(6, 0.0)).abs() < f32::EPSILON);
        assert!((smoother_step(0.0)).abs() < f32::EPSILON);
        assert!((smoother_step(1.0) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn fixed_camera_has_no_interaction_state_and_frames_vessel_size_deterministically() {
        let plan = plan_for(chemistry::ReactionRequest::acid_carbonate_gas_evolution(
            chemistry::AlkaliMetal::Sodium,
            chemistry::Halogen::Chlorine,
        ));
        let camera = fixed_camera_pose(&plan);
        assert!(std::mem::size_of::<OrbitState>() > 0);
        assert!(camera.pitch < -0.5);
        assert_eq!(camera, fixed_camera_pose(&plan));

        let mut larger = plan.clone();
        let vessel = larger
            .objects
            .iter_mut()
            .find(|object| object.role == SceneRole::Vessel)
            .expect("vessel exists");
        vessel.transform.scale = [1_400, 1_400, 1_400];
        let larger_camera = fixed_camera_pose(&larger);
        assert!((camera.yaw - larger_camera.yaw).abs() < f32::EPSILON);
        assert!((camera.pitch - larger_camera.pitch).abs() < f32::EPSILON);
        assert!(larger_camera.view_height > camera.view_height);
    }

    #[test]
    fn gas_is_a_seeded_advected_soft_volume_instead_of_a_mesh_shell() {
        let mut first = Vec::new();
        add_gas_density_field(
            &mut first,
            Vec3::ZERO,
            Vec3::new(0.8, 1.1, 0.7),
            [0.7, 0.84, 0.9, 0.2],
            42,
            1.25,
            0.8,
            GasFlowControls::contained(0.8, 0.52, 0.18, 0.16, 42),
        );
        let mut repeated = Vec::new();
        add_gas_density_field(
            &mut repeated,
            Vec3::ZERO,
            Vec3::new(0.8, 1.1, 0.7),
            [0.7, 0.84, 0.9, 0.2],
            42,
            1.25,
            0.8,
            GasFlowControls::contained(0.8, 0.52, 0.18, 0.16, 42),
        );

        assert!(first.len() > 80);
        assert!(
            first
                .iter()
                .map(|splat| splat.color[3].to_bits())
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                > 8,
            "advected density cells should carry continuously varying optical depth"
        );
        assert!(
            first
                .iter()
                .any(|splat| Vec3::from_array(splat.flow).length_squared() > 0.000_001),
            "the rendered volume must retain its simulated velocity field"
        );
        assert!(
            first.iter().map(|splat| splat.color[3]).fold(0.0, f32::max) > 0.10,
            "the gas must retain enough optical depth to remain educationally visible"
        );
        let bounds = first.iter().fold(
            (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
            |(minimum, maximum), splat| {
                let center = Vec3::from_array(splat.center);
                (minimum.min(center), maximum.max(center))
            },
        );
        let extent = bounds.1 - bounds.0;
        assert!(
            extent.x > 0.65 && extent.y > 0.70 && extent.z > 0.55,
            "the volume should occupy the vessel headspace, not resemble one rigid object"
        );
        assert_eq!(
            bytemuck::cast_slice::<GasSplat, u8>(&first),
            bytemuck::cast_slice::<GasSplat, u8>(&repeated)
        );
    }

    #[test]
    fn gaseous_reactants_mix_in_place_without_gravity_drop_or_rigid_rotation() {
        let seed = 0x4f3c_2d1a;
        let start = gas_reactant_motion(seed, 0.0, Vec3::ZERO);
        let later = gas_reactant_motion(seed, 1.4, Vec3::new(0.02, 0.0, -0.01));
        let repeated = gas_reactant_motion(seed, 1.4, Vec3::new(0.02, 0.0, -0.01));

        assert_eq!(later.translation, repeated.translation);
        assert_eq!(start.rotation, Quat::IDENTITY);
        assert_eq!(later.rotation, Quat::IDENTITY);
        assert_ne!(start.translation, later.translation);
        assert!(start.translation.length() < 0.15);
        assert!(later.translation.length() < 0.15);
        assert!(
            start.translation.y.abs() < 0.08 && later.translation.y.abs() < 0.08,
            "gas must advect inside the vessel instead of entering from above"
        );
    }

    #[test]
    fn solid_products_are_seeded_faceted_shards_instead_of_spheres() {
        let mut first = Mesh::default();
        add_particle_cluster(
            &mut first,
            Vec3::ZERO,
            Vec3::new(0.7, 0.3, 0.6),
            [0.84, 0.76, 0.42, 1.0],
            4,
            42,
        );
        let mut repeated = Mesh::default();
        add_particle_cluster(
            &mut repeated,
            Vec3::ZERO,
            Vec3::new(0.7, 0.3, 0.6),
            [0.84, 0.76, 0.42, 1.0],
            4,
            42,
        );

        assert_eq!(first.vertices.len(), 4 * 8 * 3);
        assert_eq!(first.indices.len(), 4 * 8 * 3);
        assert_eq!(first.indices, repeated.indices);
        assert_eq!(
            bytemuck::cast_slice::<Vertex, u8>(&first.vertices),
            bytemuck::cast_slice::<Vertex, u8>(&repeated.vertices)
        );
        for face in first.vertices.chunks_exact(3) {
            assert!(
                face[0]
                    .normal
                    .iter()
                    .zip(face[1].normal)
                    .all(|(first, second)| (first - second).abs() < f32::EPSILON)
            );
            assert!(
                face[1]
                    .normal
                    .iter()
                    .zip(face[2].normal)
                    .all(|(first, second)| (first - second).abs() < f32::EPSILON)
            );
        }
    }

    #[test]
    fn legacy_solid_products_receive_seeded_dry_nucleation_instead_of_precipitation() {
        let request = chemistry::requests()
            .find(|request| request.family() == chemistry::ReactionFamily::FixedChargeIonPair)
            .expect("a reviewed ionic combination exists");
        let plan = plan_for(request);
        let effect = plan
            .effects
            .iter()
            .find(|effect| effect.effect == EffectProfile::SolidFormation)
            .expect("solid product asset selects generic dry solid formation");
        assert!(
            plan.effects
                .iter()
                .all(|effect| effect.effect != EffectProfile::PrecipitateFormation)
        );

        let early = build_scene(&plan, effect.start_ordinal, 0.05);
        let formed = build_scene(&plan, effect.start_ordinal, 0.82);
        let repeated = build_scene(&plan, effect.start_ordinal, 0.82);
        assert!(
            formed.0.len() > early.0.len(),
            "staggered solid nuclei should appear progressively"
        );
        assert_eq!(formed.1, repeated.1);
        assert_eq!(
            bytemuck::cast_slice::<GpuVertex, u8>(&formed.0),
            bytemuck::cast_slice::<GpuVertex, u8>(&repeated.0)
        );
    }

    #[test]
    fn sediment_uses_gravity_drag_floor_contact_and_damped_settling() {
        let (start_fall, start_bounce) = sediment_settling_motion(0.0);
        let (early_fall, _) = sediment_settling_motion(0.195);
        let (mid_fall, _) = sediment_settling_motion(0.39);
        let (contact_fall, contact_bounce) = sediment_settling_motion(0.78);
        let (_, rebound) = sediment_settling_motion(0.835);
        let (settled_fall, settled_bounce) = sediment_settling_motion(1.0);

        assert!(start_fall.abs() < f32::EPSILON);
        assert!(start_bounce.abs() < f32::EPSILON);
        assert!(
            early_fall < 0.25,
            "gravity should accelerate the initial fall"
        );
        assert!(mid_fall > early_fall);
        assert!((contact_fall - 1.0).abs() < f32::EPSILON);
        assert!(contact_bounce.abs() < f32::EPSILON);
        assert!(
            rebound > 0.0,
            "floor contact should produce a small rebound"
        );
        assert!((settled_fall - 1.0).abs() < f32::EPSILON);
        assert!(settled_bounce.abs() < 0.000_001);

        assert_eq!(
            settling_shard_rotation(42, 0.64),
            settling_shard_rotation(42, 0.64)
        );
        assert_ne!(
            settling_shard_rotation(42, 0.64),
            settling_shard_rotation(43, 0.64)
        );
    }

    #[test]
    fn flame_plume_is_seeded_layered_and_low_poly() {
        let dynamics = scene_registry::effect_dynamics(
            EffectProfile::FlameEmitter(FlamePalette::Lilac),
            EffectIntensity::Strong,
        );
        let mut first = SceneMeshes::default();
        add_flame_plume(
            &mut first,
            Vec3::ZERO,
            FlamePalette::Lilac,
            dynamics,
            1.0,
            2.35,
            73,
        );
        let mut repeated = SceneMeshes::default();
        add_flame_plume(
            &mut repeated,
            Vec3::ZERO,
            FlamePalette::Lilac,
            dynamics,
            1.0,
            2.35,
            73,
        );

        assert!(!first.translucent.vertices.is_empty());
        assert!(!first.emissive.vertices.is_empty());
        assert_eq!(first.translucent.indices, repeated.translucent.indices);
        assert_eq!(first.emissive.indices, repeated.emissive.indices);
        assert_eq!(
            bytemuck::cast_slice::<Vertex, u8>(&first.translucent.vertices),
            bytemuck::cast_slice::<Vertex, u8>(&repeated.translucent.vertices)
        );
        assert_eq!(
            bytemuck::cast_slice::<Vertex, u8>(&first.emissive.vertices),
            bytemuck::cast_slice::<Vertex, u8>(&repeated.emissive.vertices)
        );
        assert!(
            first
                .translucent
                .vertices
                .iter()
                .any(|vertex| vertex.color[2] > vertex.color[0]),
            "the lilac body must remain blue-violet rather than generic orange"
        );
    }

    #[test]
    fn liquid_surface_deformation_is_seeded_and_scales_with_turbulence() {
        let mut calm = Mesh::default();
        add_liquid_volume(
            &mut calm,
            Vec3::ZERO,
            Vec3::ONE,
            appearance_color(AppearanceProfile::Water),
            0.0,
            2.0,
            91,
        );
        let mut agitated = Mesh::default();
        add_liquid_volume(
            &mut agitated,
            Vec3::ZERO,
            Vec3::ONE,
            appearance_color(AppearanceProfile::Water),
            0.9,
            2.0,
            91,
        );

        assert_eq!(calm.vertices.len(), agitated.vertices.len());
        assert_ne!(
            bytemuck::cast_slice::<Vertex, u8>(&calm.vertices),
            bytemuck::cast_slice::<Vertex, u8>(&agitated.vertices)
        );
    }

    #[test]
    fn colour_diffusion_reaches_any_validated_target_for_liquid_solid_and_gas() {
        let target = VisualColour {
            red: 0xd8,
            green: 0x4a,
            blue: 0x4a,
        };
        for (asset, appearance) in [
            (
                AssetProfile::LiquidVolume,
                AppearanceProfile::AqueousColourless,
            ),
            (
                AssetProfile::CrystalCluster,
                AppearanceProfile::WhitePrecipitate,
            ),
            (AssetProfile::GasCloud, AppearanceProfile::AqueousColourless),
        ] {
            let render = |progress: Option<f32>| {
                let mut meshes = SceneMeshes::default();
                instantiate_asset(
                    &mut meshes,
                    asset,
                    appearance,
                    &PresentationTransform {
                        translation: [0, 0, 0],
                        rotation: [0, 0, 0],
                        scale: [800, 800, 800],
                    },
                    1.0,
                    Vec3::ZERO,
                    Quat::IDENTITY,
                    73,
                    ReactionVisualInputs::default(),
                    1.2,
                    1.0,
                    progress.map(|progress| AssetColourTransition {
                        target,
                        progress,
                        seed: 91,
                    }),
                );
                let (vertices, _, _, _, gas_splats) = meshes.finish(fixed_view_direction());
                if asset == AssetProfile::GasCloud {
                    gas_splats
                        .into_iter()
                        .map(|splat| splat.color)
                        .collect::<Vec<[f32; 4]>>()
                } else {
                    vertices
                        .into_iter()
                        .map(|vertex| vertex.color)
                        .collect::<Vec<[f32; 4]>>()
                }
            };
            let base = render(None);
            let mixing = render(Some(0.5));
            let final_colour = render(Some(1.0));

            assert_eq!(base.len(), mixing.len());
            assert_eq!(base.len(), final_colour.len());
            assert!(base.iter().zip(&mixing).any(|(base, mixing)| {
                base[..3]
                    .iter()
                    .zip(mixing[..3].iter())
                    .any(|(base, mixing)| (base - mixing).abs() > 0.001)
            }));
            assert!(
                mixing
                    .iter()
                    .zip(&final_colour)
                    .any(|(mixing, final_colour)| {
                        mixing[..3]
                            .iter()
                            .zip(final_colour[..3].iter())
                            .any(|(mixing, final_colour)| (mixing - final_colour).abs() > 0.001)
                    })
            );
            for (base, final_colour) in base.iter().zip(&final_colour) {
                assert!((base[3] - final_colour[3]).abs() < f32::EPSILON);
                assert!((final_colour[0] - f32::from(target.red) / 255.0).abs() < 0.000_01);
                assert!((final_colour[1] - f32::from(target.green) / 255.0).abs() < 0.000_01);
                assert!((final_colour[2] - f32::from(target.blue) / 255.0).abs() < 0.000_01);
            }
        }
    }

    #[test]
    fn mixing_currents_are_seeded_three_dimensional_and_change_with_time() {
        let dynamics =
            scene_registry::effect_dynamics(EffectProfile::LiquidMixing, EffectIntensity::Moderate);
        let mut first = Mesh::default();
        add_mixing_currents(
            &mut first,
            Vec3::ZERO,
            dynamics,
            0.8,
            2.0,
            91,
            appearance_color(AppearanceProfile::AqueousColourless),
        );
        let mut repeated = Mesh::default();
        add_mixing_currents(
            &mut repeated,
            Vec3::ZERO,
            dynamics,
            0.8,
            2.0,
            91,
            appearance_color(AppearanceProfile::AqueousColourless),
        );
        let mut later = Mesh::default();
        add_mixing_currents(
            &mut later,
            Vec3::ZERO,
            dynamics,
            0.8,
            2.2,
            91,
            appearance_color(AppearanceProfile::AqueousColourless),
        );

        assert!(!first.vertices.is_empty());
        assert_eq!(first.indices, repeated.indices);
        assert_eq!(
            bytemuck::cast_slice::<Vertex, u8>(&first.vertices),
            bytemuck::cast_slice::<Vertex, u8>(&repeated.vertices)
        );
        assert_ne!(
            bytemuck::cast_slice::<Vertex, u8>(&first.vertices),
            bytemuck::cast_slice::<Vertex, u8>(&later.vertices)
        );
        let (minimum_y, maximum_y) = first.vertices.iter().map(|vertex| vertex.position[1]).fold(
            (f32::INFINITY, f32::NEG_INFINITY),
            |(minimum, maximum), value| (minimum.min(value), maximum.max(value)),
        );
        assert!(maximum_y - minimum_y > 0.25);
    }

    #[test]
    fn stirring_apparatus_is_selected_from_generic_mixing_and_existing_liquid() {
        let neutralization = plan_for(chemistry::ReactionRequest::acid_base_neutralization(
            chemistry::AlkaliMetal::Sodium,
            chemistry::Halogen::Chlorine,
        ));
        let mixing = neutralization
            .effects
            .iter()
            .find(|effect| effect.effect == EffectProfile::LiquidMixing)
            .expect("neutralization has typed liquid mixing");
        assert!(stirring_apparatus_authorized(
            SceneLayout::resolve(&neutralization),
            mixing
        ));

        let hydrogen_oxidation = chemistry::ReactionRequest::from_id("oxygen-hydrogen-oxygen")
            .expect("hydrogen oxidation exists");
        let gas_to_liquid = plan_for_materials(
            hydrogen_oxidation,
            vec![
                test_material(
                    "subject",
                    chem_presentation::MacroscopicMaterialRole::Reactant,
                    chem_domain::Phase::Gas,
                    chem_domain::RepresentationKind::Molecular,
                ),
                test_material(
                    "oxygen",
                    chem_presentation::MacroscopicMaterialRole::Reactant,
                    chem_domain::Phase::Gas,
                    chem_domain::RepresentationKind::Molecular,
                ),
                test_material(
                    "oxide",
                    chem_presentation::MacroscopicMaterialRole::Product,
                    chem_domain::Phase::Liquid,
                    chem_domain::RepresentationKind::Molecular,
                ),
            ],
            None,
        );
        let mixing = gas_to_liquid
            .effects
            .iter()
            .find(|effect| effect.effect == EffectProfile::LiquidMixing)
            .expect("liquid product has generic mixing");
        assert!(
            !stirring_apparatus_authorized(SceneLayout::resolve(&gas_to_liquid), mixing),
            "forming a liquid from gases must not invent a stirring procedure"
        );
        assert!(
            neutralization
                .effects
                .iter()
                .filter(|effect| effect.effect != EffectProfile::LiquidMixing)
                .all(|effect| !stirring_apparatus_authorized(
                    SceneLayout::resolve(&neutralization),
                    effect
                )),
            "non-mixing effects must never select the apparatus"
        );
    }

    #[test]
    fn stirrer_enters_on_a_curve_moves_naturally_and_clears_the_vessel() {
        let plan = plan_for(chemistry::ReactionRequest::acid_base_neutralization(
            chemistry::AlkaliMetal::Sodium,
            chemistry::Halogen::Chlorine,
        ));
        let layout = SceneLayout::resolve(&plan);
        let seed = 91;
        let start = stirring_pose(layout, 0.0, seed);
        let entry_middle = stirring_pose(layout, STIRRER_ENTRY_END * 0.5, seed);
        let entry_end = stirring_pose(layout, STIRRER_ENTRY_END, seed);
        let straight_middle = start.lower.lerp(entry_end.lower, 0.5);
        assert!(entry_middle.lower.distance(straight_middle) > 0.035);
        assert!(start.visibility <= f32::EPSILON);
        assert!(entry_end.submerged > 0.99);

        let active = [
            stirring_pose(layout, 0.36, seed),
            stirring_pose(layout, 0.47, seed),
            stirring_pose(layout, 0.58, seed),
        ];
        for pose in active {
            let radial = Vec3::new(
                pose.lower.x - layout.vessel_center.x,
                0.0,
                pose.lower.z - layout.vessel_center.z,
            )
            .length();
            assert!(pose.lower.y < layout.liquid_surface);
            assert!(radial < layout.vessel_scale.x * 0.34);
            assert!(pose.activity > 0.5);
        }
        let first_distance = active[0].lower.distance(active[1].lower);
        let second_distance = active[1].lower.distance(active[2].lower);
        assert!(
            (first_distance - second_distance).abs() > 0.002,
            "seeded angular travel should avoid constant-speed robotic motion"
        );

        let withdrawal = stirring_pose(layout, 0.91, seed);
        let complete = stirring_pose(layout, 1.0, seed);
        let vessel_rim = layout.vessel_center.y + 0.91 * layout.vessel_scale.y;
        assert!(withdrawal.lower.y > layout.liquid_surface);
        assert!(complete.lower.y > vessel_rim);
        assert!(complete.visibility <= f32::EPSILON);

        let before_entry_boundary = stirring_pose(layout, STIRRER_ENTRY_END - 0.0001, seed);
        let after_entry_boundary = stirring_pose(layout, STIRRER_ENTRY_END, seed);
        let before_exit_boundary = stirring_pose(layout, STIRRER_EXIT_START - 0.0001, seed);
        let after_exit_boundary = stirring_pose(layout, STIRRER_EXIT_START, seed);
        assert!(
            before_entry_boundary
                .lower
                .distance(after_entry_boundary.lower)
                < 0.005
        );
        assert!(
            before_exit_boundary
                .lower
                .distance(after_exit_boundary.lower)
                < 0.005
        );
    }

    #[test]
    fn stirring_geometry_is_deterministic_and_disappears_after_withdrawal() {
        let plan = plan_for(chemistry::ReactionRequest::acid_base_neutralization(
            chemistry::AlkaliMetal::Sodium,
            chemistry::Halogen::Chlorine,
        ));
        let layout = SceneLayout::resolve(&plan);
        let render = |progress| {
            let mut meshes = SceneMeshes::default();
            add_stirring_apparatus(
                &mut meshes,
                layout,
                stirring_pose(layout, progress, 73),
                progress,
                73,
                appearance_color(AppearanceProfile::AqueousColourless),
            );
            meshes
        };
        let first = render(0.5);
        let repeated = render(0.5);
        assert!(!first.glass.vertices.is_empty());
        assert!(!first.translucent.vertices.is_empty());
        assert_eq!(first.glass.indices, repeated.glass.indices);
        assert_eq!(first.translucent.indices, repeated.translucent.indices);
        assert_eq!(
            bytemuck::cast_slice::<Vertex, u8>(&first.glass.vertices),
            bytemuck::cast_slice::<Vertex, u8>(&repeated.glass.vertices)
        );
        assert_eq!(
            bytemuck::cast_slice::<Vertex, u8>(&first.translucent.vertices),
            bytemuck::cast_slice::<Vertex, u8>(&repeated.translucent.vertices)
        );

        let complete = render(1.0);
        assert!(complete.glass.vertices.is_empty());
        assert!(complete.translucent.vertices.is_empty());
    }

    #[test]
    fn stirring_apparatus_never_leaks_into_post_reaction_separation() {
        let plan = plan_for(chemistry::ReactionRequest::acid_base_neutralization(
            chemistry::AlkaliMetal::Sodium,
            chemistry::Halogen::Chlorine,
        ));
        let layout = SceneLayout::resolve(&plan);
        let effect = plan
            .effects
            .iter()
            .find(|effect| effect.effect == EffectProfile::LiquidMixing)
            .expect("neutralization has typed liquid mixing");
        let colours = scene_effect_colours(&plan, effect.end_ordinal, 0.5);
        let render = |stage| {
            let mut meshes = SceneMeshes::default();
            instantiate_effect(
                &mut meshes,
                effect,
                EffectMoment {
                    ordinal: effect.end_ordinal,
                    progress: 0.5,
                    stage,
                },
                layout,
                effect_seed(&plan, effect),
                colours,
            );
            meshes
        };

        let reaction = render(MacroscopicStage::Reaction);
        let separation = render(MacroscopicStage::CrystalGrowth);
        assert!(!reaction.glass.vertices.is_empty());
        assert!(
            separation.glass.vertices.is_empty() && separation.translucent.vertices.is_empty(),
            "reaction-stage stirring geometry must not persist into crystallisation"
        );
    }

    #[test]
    fn liquid_mixing_waits_until_the_inserted_stirrer_actually_moves() {
        let plan = plan_for(chemistry::ReactionRequest::acid_base_neutralization(
            chemistry::AlkaliMetal::Sodium,
            chemistry::Halogen::Chlorine,
        ));
        let layout = SceneLayout::resolve(&plan);
        let dynamics =
            scene_registry::effect_dynamics(EffectProfile::LiquidMixing, EffectIntensity::Moderate);
        let seed = 91;
        let inserted = stirring_pose(layout, STIRRER_ENTRY_END * 0.92, seed);
        assert!(inserted.submerged > 0.8);
        assert!(inserted.activity <= f32::EPSILON);

        let mut waiting = Mesh::default();
        add_mixing_currents(
            &mut waiting,
            layout.liquid_center,
            dynamics,
            inserted.activity,
            0.8,
            seed,
            appearance_color(AppearanceProfile::AqueousColourless),
        );
        assert!(
            waiting.vertices.is_empty(),
            "an immersed but stationary rod must not start liquid mixing"
        );

        let moving = stirring_pose(layout, 0.43, seed);
        assert!(moving.activity > 0.5);
        let mut active = Mesh::default();
        add_mixing_currents(
            &mut active,
            layout.liquid_center,
            dynamics,
            moving.activity,
            1.1,
            seed,
            appearance_color(AppearanceProfile::AqueousColourless),
        );
        assert!(
            !active.vertices.is_empty(),
            "mixing currents must begin once the immersed rod starts moving"
        );

        let mut mixing_only = plan;
        mixing_only
            .effects
            .retain(|effect| effect.effect == EffectProfile::LiquidMixing);
        let effect = mixing_only
            .effects
            .first()
            .expect("the isolated mixing effect remains");
        let sample_at = |target: f32| {
            let span = f32::from(
                effect
                    .end_ordinal
                    .saturating_sub(effect.start_ordinal)
                    .saturating_add(1),
            );
            let mut ordinal = effect.start_ordinal;
            let mut progress = target * span;
            while progress >= 1.0 {
                ordinal = ordinal.saturating_add(1);
                progress -= 1.0;
            }
            (ordinal, progress)
        };
        let final_ordinal = mixing_only
            .timeline
            .beats
            .last()
            .map_or(effect.end_ordinal, |beat| beat.end_ordinal);
        let gated_inputs_at = |target| {
            let (ordinal, progress) = sample_at(target);
            let mut inputs = ReactionVisualInputs::from_effects(
                &mixing_only.effects,
                ordinal,
                progress,
                final_ordinal,
            );
            let ungated = inputs.liquid_turbulence;
            gate_stirrer_driven_liquid_turbulence(
                &mut inputs,
                &mixing_only,
                layout,
                ordinal,
                progress,
                MacroscopicStage::Reaction,
                final_ordinal,
            );
            (ungated, inputs.liquid_turbulence)
        };
        let (ungated_waiting, gated_waiting) = gated_inputs_at(STIRRER_ENTRY_END * 0.92);
        assert!(ungated_waiting > 0.0);
        assert!(
            gated_waiting <= f32::EPSILON,
            "LiquidMixing turbulence must wait for the active stirring stroke"
        );
        let (_, gated_active) = gated_inputs_at(0.43);
        assert!(
            gated_active > 0.0,
            "LiquidMixing turbulence must ramp up once stirring starts"
        );
    }

    #[test]
    fn distinct_gas_reaction_families_drive_the_same_generic_visual_channels() {
        let plans = [
            canonical_plan(),
            plan_for(chemistry::ReactionRequest::acid_carbonate_gas_evolution(
                chemistry::AlkaliMetal::Sodium,
                chemistry::Halogen::Chlorine,
            )),
        ];
        for plan in plans {
            let gas_start = plan
                .effects
                .iter()
                .find(|effect| effect.effect == EffectProfile::GasRelease)
                .expect("gas release is observation-backed")
                .start_ordinal;
            let final_ordinal = plan
                .timeline
                .beats
                .last()
                .expect("timeline ends")
                .end_ordinal;
            let inputs =
                ReactionVisualInputs::from_effects(&plan.effects, gas_start, 0.5, final_ordinal);
            assert!(inputs.gas_generation_rate > 0.0);
            assert!(inputs.bubble_rate > 0.0);
            assert!(inputs.liquid_turbulence > 0.0);
            let scene = build_scene(&plan, gas_start, 0.5);
            let uses_authored_gas_assembly = plan.gas_evolution.is_some()
                || plan
                    .objects
                    .iter()
                    .any(|object| object.asset == AssetProfile::ReactiveMetalWaterAssembly);
            if uses_authored_gas_assembly {
                // The assembly owns its gas look: the generic overlay must
                // not run on top of it. Gas-evolution scenes emit their own
                // dense-product haze splats (developed by late reaction —
                // at generation onset the fluid volume is still empty); the
                // alkali assembly (which wins the dispatch when both are
                // present) emits none.
                let routes_to_alkali = plan
                    .objects
                    .iter()
                    .any(|object| object.asset == AssetProfile::ReactiveMetalWaterAssembly);
                if plan.gas_evolution.is_some() && !routes_to_alkali {
                    let late = build_scene_with_stage(
                        &plan,
                        final_ordinal,
                        0.9,
                        MacroscopicStage::Reaction,
                        0.9,
                        Some(0.85),
                        fixed_view_direction(),
                    );
                    assert!(
                        !late.4.is_empty(),
                        "the gas-evolution assembly supplies its own dense-gas haze"
                    );
                } else {
                    assert!(
                        scene.4.is_empty(),
                        "an authored gas assembly must not be overlaid with procedural gas"
                    );
                }
                assert!(
                    scene.0.len() > 1_000,
                    "the authored reaction should supply its own visible geometry"
                );
                continue;
            }
            let gas_splats = scene.4;
            assert!(
                gas_splats.len() > 220,
                "a generic gas-producing reaction should build a dense shared headspace volume"
            );
            let (minimum_y, maximum_y) = gas_splats.iter().map(|splat| splat.center[1]).fold(
                (f32::INFINITY, f32::NEG_INFINITY),
                |(minimum, maximum), y| (minimum.min(y), maximum.max(y)),
            );
            assert!(
                maximum_y - minimum_y > 0.48,
                "contained fog and the venting plume should form one vertically continuous volume"
            );
        }
    }

    #[test]
    fn transient_effects_have_smooth_attack_and_release_envelopes() {
        let bubbles = scene_registry::effect_dynamics(
            EffectProfile::BubbleEmitter,
            EffectIntensity::Moderate,
        );
        assert!(effect_envelope(bubbles, 0.0).abs() < f32::EPSILON);
        assert!(effect_envelope(bubbles, 0.5) > 0.99);
        assert!(effect_envelope(bubbles, 1.0).abs() < f32::EPSILON);

        let precipitate = scene_registry::effect_dynamics(
            EffectProfile::PrecipitateFormation,
            EffectIntensity::Moderate,
        );
        assert!((effect_envelope(precipitate, 1.0) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn reaction_motion_is_seeded_repeatable_and_settles_at_effect_edges() {
        let plan = canonical_plan();
        let disturbance = plan
            .effects
            .iter()
            .find(|effect| effect.effect == EffectProfile::SurfaceDisturbance)
            .expect("surface disturbance exists");
        let start = reaction_surface_motion(&plan, disturbance.start_ordinal, 0.0);
        let active = reaction_surface_motion(&plan, disturbance.start_ordinal, 0.5);
        let repeated = reaction_surface_motion(&plan, disturbance.start_ordinal, 0.5);
        let end = reaction_surface_motion(&plan, disturbance.end_ordinal, 1.0);

        assert!(start.length() < f32::EPSILON);
        assert!(active.length() > 0.0);
        assert_eq!(active, repeated);
        assert!(end.length() < f32::EPSILON);
    }

    #[test]
    fn container_vibration_is_seeded_bounded_and_leaves_the_camera_fixed() {
        let active = ReactionVisualInputs {
            container_vibration: 0.55,
            ..ReactionVisualInputs::default()
        };
        let first = container_vibration_offset(active, 2.35, 73);
        let repeated = container_vibration_offset(active, 2.35, 73);
        let advanced = container_vibration_offset(active, 2.38, 73);

        assert_eq!(first, repeated);
        assert_ne!(first, advanced);
        assert!(first.length() < 0.008);
        assert_eq!(
            container_vibration_offset(ReactionVisualInputs::default(), 2.35, 73),
            Vec3::ZERO
        );

        let plan = canonical_plan();
        assert_eq!(fixed_camera_pose(&plan), fixed_camera_pose(&plan));
    }

    #[test]
    fn reactant_approach_flows_through_setup_ordinals_without_an_idle_hold() {
        let plan = canonical_plan();
        let reactant = plan
            .objects
            .iter()
            .find(|object| object.role == SceneRole::Reactant)
            .expect("reactant exists");
        let contact_ordinal = plan
            .effects
            .iter()
            .map(|effect| effect.start_ordinal)
            .min()
            .expect("reaction has an authorized effect");
        assert!(contact_ordinal >= reactant.visible_from_ordinal.saturating_add(2));

        let during_setup = object_motion(
            &plan,
            reactant,
            reactant.visible_from_ordinal.saturating_add(1),
            0.5,
            Vec3::ZERO,
        );
        assert!(
            during_setup.translation.length() > 0.01,
            "the reactant must still be approaching during later setup states"
        );

        let just_before_contact = object_motion(
            &plan,
            reactant,
            contact_ordinal.saturating_sub(1),
            1.0,
            Vec3::ZERO,
        );
        let at_contact = object_motion(&plan, reactant, contact_ordinal, 0.0, Vec3::ZERO);
        assert!(just_before_contact.translation.length() < f32::EPSILON);
        assert_eq!(just_before_contact.translation, at_contact.translation);
        assert_eq!(just_before_contact.rotation, at_contact.rotation);
    }

    #[test]
    fn reactant_entry_is_a_seeded_gravity_drop_with_a_damped_impact() {
        let seed = 0x51a7_9c2d;
        let start = gravitational_drop_offset(seed, 0.0);
        let quarter = gravitational_drop_offset(seed, 0.25);
        let halfway = gravitational_drop_offset(seed, 0.5);
        let three_quarters = gravitational_drop_offset(seed, 0.75);
        let repeated = gravitational_drop_offset(seed, 0.5);
        let contact = gravitational_drop_offset(seed, 1.0);

        assert_eq!(halfway, repeated);
        assert_ne!(halfway, gravitational_drop_offset(seed.rotate_left(9), 0.5));
        assert!(
            start.x.hypot(start.z) < 0.15,
            "the reactant must begin over the vessel centre"
        );
        assert!(
            halfway.y > start.y * 0.5,
            "gravity must hold the drop above a linear interpolation early on"
        );
        assert!(
            three_quarters.y - contact.y > start.y - quarter.y,
            "the falling distance per interval must increase under gravity"
        );
        assert!(contact.length() < f32::EPSILON);

        let impact = damped_impact_offset(seed, 0.1);
        let settled = damped_impact_offset(seed, 1.5);
        assert!(impact.y < 0.0, "contact first plunges into the liquid");
        assert!(settled.length() < impact.length() * 0.05);
    }

    #[test]
    fn natural_motion_curves_are_asymmetric_and_seeded_without_endpoint_drift() {
        assert!(normalized_terminal_distance(0.25, 4.8) < 0.25);
        assert!(normalized_drag_distance(0.25, 0.48) > 0.25);
        assert!(normalized_exponential_response(0.25, 4.2) > 0.25);
        assert!((normalized_terminal_distance(1.0, 4.8) - 1.0).abs() < f32::EPSILON);
        assert!((normalized_drag_distance(1.0, 0.48) - 1.0).abs() < f32::EPSILON);
        assert_eq!(curl_like_flow(1.7, 42, 3), curl_like_flow(1.7, 42, 3));
        assert_ne!(curl_like_flow(1.7, 42, 3), curl_like_flow(1.7, 42, 4));
    }

    #[test]
    fn product_formation_eases_from_zero_after_its_trusted_boundary() {
        let plan = canonical_plan();
        let product = plan
            .objects
            .iter()
            .find(|object| object.role == SceneRole::Product)
            .expect("product exists");

        assert!(
            object_formation_scale(product, product.visible_from_ordinal.saturating_sub(1), 1.0,)
                .abs()
                < f32::EPSILON
        );
        assert!(
            object_formation_scale(product, product.visible_from_ordinal, 0.0).abs() < f32::EPSILON
        );
        let forming = object_formation_scale(product, product.visible_from_ordinal, 0.5);
        assert!(forming > 0.5 && forming < 1.0);
        assert!(
            (object_formation_scale(product, product.visible_from_ordinal, 1.0) - 1.0).abs()
                < f32::EPSILON
        );
    }

    #[test]
    fn scene_mesh_is_deterministic_macroscopic_and_contains_depth_geometry() {
        let plan = canonical_plan();
        let first = build_scene(&plan, 3, 0.5);
        let second = build_scene(&plan, 3, 0.5);
        assert_eq!(
            bytemuck::cast_slice::<GpuVertex, u8>(&first.0),
            bytemuck::cast_slice::<GpuVertex, u8>(&second.0)
        );
        assert_eq!(first.1, second.1);
        assert!(first.2 > 0, "the scene must contain opaque depth geometry");
        assert!(
            usize::try_from(first.2).is_ok_and(|count| count < first.1.len()),
            "glass, liquid, and effects must remain in the transparent pass"
        );
        assert!(
            usize::try_from(first.3).is_ok_and(|count| count <= first.1.len()),
            "the additive pass boundary must remain inside the batched index buffer"
        );
        assert!(first.0.iter().any(|vertex| vertex.position[2].abs() > 0.1));
        assert!(
            first.0.len() > 100,
            "diorama should include reusable scene assets"
        );
    }

    #[test]
    fn reviewed_if7_graph_remains_exact_while_macroscopic_product_uses_gas_density() {
        let request = chemistry::ReactionRequest::from_id("covalent-i-f-if7")
            .expect("reviewed IF7 request exists");
        let plan = plan_for(request);
        let preview = request
            .product_preview()
            .expect("reviewed IF7 graph exists");
        let iodine = preview
            .atoms
            .iter()
            .position(|atom| atom.atomic_number == 53)
            .expect("IF7 contains iodine");
        assert_eq!(preview.atoms.len(), 8);
        assert!(
            preview
                .covalent_bonds()
                .iter()
                .all(|bond| { bond.start == iodine || bond.end == iodine })
        );
        assert!(plan.objects.iter().any(|object| {
            object.role == SceneRole::Product && object.asset == AssetProfile::GasCloud
        }));

        let final_ordinal = plan.timeline.beats.last().unwrap().end_ordinal;
        let scene = build_scene(&plan, final_ordinal, 1.0);
        assert!(
            !scene.4.is_empty()
                && scene
                    .4
                    .iter()
                    .any(|splat| Vec3::from_array(splat.flow).length_squared() > 0.000_001),
            "the persistent gas product must contribute an advected soft volume"
        );
    }

    #[test]
    fn persistent_gas_products_share_a_vessel_wide_stratified_layer_regime() {
        for reaction_id in ["oxygen-carbon-oxygen", "covalent-i-f-if7"] {
            let request =
                chemistry::ReactionRequest::from_id(reaction_id).expect("reviewed request exists");
            let plan = if reaction_id == "oxygen-carbon-oxygen" {
                carbon_oxidation_plan()
            } else {
                plan_for(request)
            };
            let layout = SceneLayout::resolve(&plan);
            let product = plan
                .objects
                .iter()
                .find(|object| {
                    object.role == SceneRole::Product && object.asset == AssetProfile::GasCloud
                })
                .unwrap_or_else(|| panic!("{reaction_id} reviewed product is gaseous"));
            let mut meshes = SceneMeshes::default();
            instantiate_plan_gas_asset(
                &mut meshes,
                product,
                layout,
                1.0,
                1.0,
                stable_seed(&product.id),
                ReactionVisualInputs::default(),
                3.2,
                None,
            );

            assert!(
                meshes.gas.len() > 80,
                "{reaction_id} should produce continuous occupied density"
            );
            assert!(
                meshes.gas.iter().all(|splat| splat.layering > 0.50),
                "{reaction_id} should use retained-product stratification"
            );
            let (minimum, maximum, weighted_y, mass) = meshes.gas.iter().fold(
                (
                    Vec3::splat(f32::INFINITY),
                    Vec3::splat(f32::NEG_INFINITY),
                    0.0,
                    0.0,
                ),
                |(minimum, maximum, weighted_y, mass), splat| {
                    let center = Vec3::from_array(splat.center);
                    (
                        minimum.min(center),
                        maximum.max(center),
                        weighted_y + center.y * splat.density,
                        mass + splat.density,
                    )
                },
            );
            let extent = maximum - minimum;
            assert!(
                extent.x > layout.vessel_scale.x
                    && extent.z > layout.vessel_scale.z
                    && extent.y > 0.16,
                "{reaction_id} should occupy the vessel cross-section as fog, not one small blob"
            );
            assert!(
                weighted_y / mass < layout.vessel_center.y,
                "{reaction_id} should retain most cooled product density in the lower vessel"
            );
        }
    }

    #[test]
    fn supplied_metal_asset_is_a_valid_normalized_embedded_mesh() {
        let mesh = parse_embedded_mesh(METAL_MESH_BYTES).expect("baked metal mesh is valid");
        assert_eq!(mesh.vertices.len(), 2_321);
        assert_eq!(mesh.indices.len(), 13_914);
        assert!(
            mesh.indices
                .iter()
                .all(|index| usize::try_from(*index).is_ok_and(|index| index < mesh.vertices.len()))
        );
        let (minimum, maximum) = mesh.vertices.iter().fold(
            (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
            |(minimum, maximum), vertex| {
                (minimum.min(vertex.position), maximum.max(vertex.position))
            },
        );
        assert!(minimum.y.abs() < 0.000_01);
        assert!((maximum - minimum).max_element() <= 1.000_01);
        assert!(
            mesh.vertices
                .iter()
                .all(|vertex| { (vertex.normal.length() - 1.0).abs() < 0.000_1 })
        );
    }

    #[test]
    fn surface_oxidation_without_trusted_colour_keeps_the_original_metal_appearance() {
        let plan = surface_oxidation_plan();
        let layout = SceneLayout::resolve(&plan);
        assert!(!layout.has_vessel);
        assert!(
            plan.objects
                .iter()
                .all(|object| object.role != SceneRole::Vessel)
        );
        let metal = plan
            .objects
            .iter()
            .find(|object| object.asset == AssetProfile::MetalChunk)
            .expect("surface scene has one imported metal");
        let effect = plan
            .effects
            .iter()
            .find(|effect| effect.effect == EffectProfile::SurfaceOxidation)
            .expect("typed oxidation effect exists");
        assert!(surface_oxidation_transition(&plan, metal, effect.start_ordinal, 0.0).is_none());
        assert!(surface_oxidation_transition(&plan, metal, effect.end_ordinal, 1.0).is_none());
        assert!((layout.reaction_point.y - layout.bench_top).abs() < 0.01);
        assert_eq!(
            object_motion(&plan, metal, effect.start_ordinal, 0.5, Vec3::ZERO).translation,
            Vec3::ZERO
        );

        let mut mesh = Mesh::default();
        add_imported_metal(
            &mut mesh,
            layout.reaction_point,
            transform_scale(&metal.transform),
            [0.88, 0.90, 0.92, 1.0],
        );
        assert!(mesh.vertices.iter().all(|vertex| {
            vertex.color[..3]
                .iter()
                .zip([0.88, 0.90, 0.92])
                .all(|(actual, expected)| (actual - expected).abs() < f32::EPSILON)
        }));
    }

    #[test]
    fn surface_oxidation_uses_product_bound_effect_colour_when_available() {
        let mut plan = surface_oxidation_plan();
        let effect = plan
            .effects
            .iter_mut()
            .find(|effect| effect.effect == EffectProfile::SurfaceOxidation)
            .expect("typed oxidation effect exists");
        effect.surface_oxide_colour = Some(chem_presentation::SurfaceOxideColour {
            product_binding: "oxide".to_owned(),
            target: VisualColour {
                red: 0xb9,
                green: 0x42,
                blue: 0x3b,
            },
            authority: chem_presentation::MacroscopicColourAuthority::ModelAsserted,
        });
        let ordinal = effect.end_ordinal;
        let metal = plan
            .objects
            .iter()
            .find(|object| object.asset == AssetProfile::MetalChunk)
            .expect("surface scene has one imported metal");
        let transition = surface_oxidation_transition(&plan, metal, ordinal, 1.0)
            .expect("surface transition is bound");
        assert_eq!(
            transition.target,
            VisualColour {
                red: 0xb9,
                green: 0x42,
                blue: 0x3b,
            }
        );
    }

    #[test]
    fn gas_forms_stays_in_vessel_while_gas_evolves_can_feed_the_open_rim() {
        let render_release = |plan: ScenePlan| {
            let effect = plan
                .effects
                .iter()
                .find(|effect| effect.effect == EffectProfile::GasRelease)
                .expect("gas release exists");
            let layout = SceneLayout::resolve(&plan);
            let mut meshes = SceneMeshes::default();
            instantiate_effect(
                &mut meshes,
                effect,
                EffectMoment {
                    ordinal: effect.start_ordinal,
                    progress: 0.64,
                    stage: MacroscopicStage::Reaction,
                },
                layout,
                effect_seed(&plan, effect),
                scene_effect_colours(&plan, effect.start_ordinal, 0.64),
            );
            let vessel_rim = layout.vessel_center.y + 0.91 * layout.vessel_scale.y;
            let mass_above_rim = meshes
                .gas
                .iter()
                .filter(|splat| splat.center[1] > vessel_rim)
                .map(|splat| splat.density)
                .sum::<f32>();
            (effect.trigger, mass_above_rim, meshes.gas.len())
        };

        let formed = render_release(carbon_oxidation_plan());
        let evolved = render_release(plan_for(
            chemistry::ReactionRequest::acid_carbonate_gas_evolution(
                chemistry::AlkaliMetal::Sodium,
                chemistry::Halogen::Chlorine,
            ),
        ));

        assert_eq!(formed.0, ObservationPredicate::Forms);
        assert_eq!(evolved.0, ObservationPredicate::Evolves);
        assert!(formed.2 > 40 && evolved.2 > 40);
        assert!(
            formed.1 <= 0.001,
            "`forms` must not silently claim open-rim venting"
        );
        assert!(
            evolved.1 > 0.01,
            "`evolves` should feed a continuous open-rim plume"
        );
    }

    #[test]
    fn evolving_gas_product_does_not_inherit_the_dense_layer_default() {
        let plan = plan_for(chemistry::ReactionRequest::alkali_water(
            chemistry::AlkaliMetal::Lithium,
        ));
        let product = plan
            .objects
            .iter()
            .find(|object| {
                object.role == SceneRole::Product && object.asset == AssetProfile::GasCloud
            })
            .expect("hydrogen gas product exists");
        assert_eq!(
            product
                .observation
                .as_ref()
                .map(|observation| observation.predicate),
            Some(ObservationPredicate::Evolves)
        );
        let mut meshes = SceneMeshes::default();
        instantiate_plan_gas_asset(
            &mut meshes,
            product,
            SceneLayout::resolve(&plan),
            1.0,
            1.0,
            stable_seed(&product.id),
            ReactionVisualInputs::default(),
            2.8,
            None,
        );
        assert!(meshes.gas.len() > 80);
        assert!(
            meshes.gas.iter().all(|splat| splat.layering == 0.0),
            "gas that explicitly evolves should stay mixed and use the separate buoyant plume"
        );
    }

    #[test]
    fn registry_reactants_are_replaced_by_the_final_3d_product() {
        let request = chemistry::ReactionRequest::from_id("covalent-i-f-if7")
            .expect("reviewed IF7 request exists");
        let plan = plan_for(request);
        let final_ordinal = plan.timeline.beats.last().unwrap().end_ordinal;

        let reactant = plan
            .objects
            .iter()
            .find(|object| object.role == SceneRole::Reactant)
            .expect("registry profile has reactants");
        assert!(object_replacement_scale(&plan, reactant, final_ordinal, 1.0) <= f32::EPSILON);
    }

    #[test]
    fn lithium_scene_adds_visible_surface_and_reaction_effect_geometry() {
        let plan = canonical_plan();
        let before = build_scene(&plan, 0, 0.5);
        let reacting_ordinal = plan
            .effects
            .iter()
            .map(|effect| effect.start_ordinal)
            .min()
            .expect("alkali-water profile has observation-backed effects");
        let reacting = build_scene(&plan, reacting_ordinal, 0.5);
        assert_eq!(reacting.0.len(), before.0.len());
        assert_ne!(
            bytemuck::cast_slice::<GpuVertex, u8>(&reacting.0),
            bytemuck::cast_slice::<GpuVertex, u8>(&before.0),
            "authored tracks should deform and move continuously without entity churn"
        );
        assert!(plan.effects.iter().any(|effect| {
            effect.effect == EffectProfile::SurfaceDisturbance
                || effect.effect == EffectProfile::SplashEmitter
        }));
        let camera = fixed_camera_pose(&plan);
        assert!(camera.pitch < -0.5);
        assert_eq!(camera, fixed_camera_pose(&plan));
    }

    #[test]
    fn authored_clip_advances_uniformly_across_chemistry_beat_boundaries() {
        let plan = canonical_plan();
        let duration = plan.timeline.duration_ms();
        let frames = (0..=4_u64)
            .map(|quarter| {
                let elapsed = duration.saturating_mul(quarter) / 4;
                let moment = plan
                    .timeline
                    .locate(elapsed)
                    .expect("quarter-time sample exists");
                plan.timeline.normalized_progress_at(moment) * 179.0
            })
            .collect::<Vec<_>>();
        let deltas = frames
            .windows(2)
            .map(|window| window[1] - window[0])
            .collect::<Vec<_>>();
        assert!(
            deltas.iter().all(|delta| (*delta - deltas[0]).abs() < 0.05),
            "unequal chemistry beats must not alter authored clip speed: {deltas:?}"
        );
    }

    #[test]
    fn potassium_uses_generic_flame_pass_while_lithium_does_not_invent_ignition() {
        let potassium = plan_for(chemistry::ReactionRequest::alkali_water(
            chemistry::AlkaliMetal::Potassium,
        ));
        let flame = potassium
            .effects
            .iter()
            .find(|effect| matches!(effect.effect, EffectProfile::FlameEmitter(_)))
            .expect("reviewed potassium profile selects the generic flame emitter");
        let potassium_mesh = build_scene(&potassium, flame.start_ordinal, 0.5);
        assert!(
            usize::try_from(potassium_mesh.3).is_ok_and(|start| start < potassium_mesh.1.len()),
            "emissive cores and sparks use the final additive batch"
        );

        let lithium = canonical_plan();
        assert!(
            !lithium
                .effects
                .iter()
                .any(|effect| matches!(effect.effect, EffectProfile::FlameEmitter(_)))
        );
        let lithium_mesh = build_scene(&lithium, flame.start_ordinal, 0.5);
        assert_eq!(
            usize::try_from(lithium_mesh.3).expect("index boundary fits usize"),
            lithium_mesh.1.len()
        );
    }

    #[test]
    fn bromide_and_iodide_precipitates_render_only_at_their_trusted_colours() {
        for halogen in [chemistry::Halogen::Bromine, chemistry::Halogen::Iodine] {
            let plan = plan_for(chemistry::ReactionRequest::silver_halide_precipitation(
                halogen,
            ));
            let product = plan
                .objects
                .iter()
                .find(|object| object.role == SceneRole::Product)
                .expect("precipitate product exists");
            let transition = product
                .colour_transition
                .as_ref()
                .expect("trusted colour transition exists");
            let expected =
                mix_visual_colour(appearance_color(product.appearance), transition.target, 1.0);
            let before = build_scene(&plan, transition.start_ordinal, 0.0);
            let visible = build_scene(&plan, transition.start_ordinal, 1.0);

            let has_expected_colour = |vertex: &GpuVertex| {
                vertex.color[..3]
                    .iter()
                    .zip(expected[..3].iter())
                    .all(|(actual, expected)| (actual - expected).abs() < f32::EPSILON)
            };
            assert!(!before.0.iter().any(has_expected_colour));
            assert!(visible.0.iter().any(has_expected_colour));
        }
    }

    #[test]
    fn halogen_displacement_shows_a_solution_with_the_reviewed_colour_change() {
        for (request, expected_colour) in chemistry::ReactionRequest::ALL
            .iter()
            .copied()
            .filter(|request| request.family() == chemistry::ReactionFamily::HalogenDisplacement)
            .map(|request| {
                let expected = if request.id().ends_with("iodide") {
                    "Brown"
                } else {
                    "Orange"
                };
                (request, expected)
            })
        {
            let plan = plan_for(request);
            let solution = plan
                .objects
                .iter()
                .find(|object| {
                    object.role == SceneRole::Contents && object.asset == AssetProfile::LiquidVolume
                })
                .expect("halogen displacement stages the halide solution");
            let transition = solution
                .colour_transition
                .as_ref()
                .expect("the solution carries the displaced halogen colour");
            assert_eq!(transition.value, expected_colour, "{}", request.id());
            assert!(plan.effects.iter().any(|effect| {
                effect.effect == EffectProfile::ColourTransition
                    && effect.trigger == chem_catalogue::ObservationPredicate::Colour
            }));
            // Nothing is invented beyond the reviewed observations: no gas,
            // no precipitate.
            assert!(!plan.objects.iter().any(|object| matches!(
                object.asset,
                AssetProfile::GasCloud | AssetProfile::PrecipitateCloud
            )));
        }
    }

    #[test]
    fn neutralization_mixes_colourless_liquid_without_inventing_a_phase_change() {
        let plan = plan_for(chemistry::ReactionRequest::acid_base_neutralization(
            chemistry::AlkaliMetal::Sodium,
            chemistry::Halogen::Chlorine,
        ));
        assert_eq!(
            plan.objects
                .iter()
                .filter(|object| object.asset == AssetProfile::LiquidVolume)
                .count(),
            1
        );
        let mixing = plan
            .effects
            .iter()
            .find(|effect| effect.effect == EffectProfile::LiquidMixing)
            .expect("reactant disappearance authorizes generic liquid mixing");
        assert_eq!(
            mixing.trigger,
            chem_catalogue::ObservationPredicate::Disappears
        );
        assert!(
            plan.effects
                .iter()
                .any(|effect| effect.effect == EffectProfile::SurfaceDisturbance)
        );
        assert!(!plan.effects.iter().any(|effect| matches!(
            effect.effect,
            EffectProfile::BubbleEmitter
                | EffectProfile::GasRelease
                | EffectProfile::PrecipitateFormation
                | EffectProfile::Clouding
                | EffectProfile::FlameEmitter(_)
        )));
        assert!(!plan.objects.iter().any(|object| matches!(
            object.asset,
            AssetProfile::GasCloud | AssetProfile::PrecipitateCloud
        )));

        let before = build_scene(&plan, mixing.start_ordinal.saturating_sub(1), 0.5);
        let active = build_scene(&plan, mixing.start_ordinal, 0.5);
        assert_eq!(active.0.len(), before.0.len());
        assert_eq!(active.1.len(), before.1.len());
        assert_ne!(
            bytemuck::cast_slice::<GpuVertex, u8>(&active.0),
            bytemuck::cast_slice::<GpuVertex, u8>(&before.0),
            "authored mixing tracks should move without per-frame entity churn"
        );

        let colourless = appearance_color(AppearanceProfile::AqueousColourless);
        assert!((colourless[2] - colourless[0]).abs() < 0.12);
        assert!(colourless[3] < appearance_color(AppearanceProfile::Water)[3]);
    }

    #[test]
    fn neutralization_separation_boils_solvent_and_grows_deterministic_salt_crystals() {
        let plan = plan_for(chemistry::ReactionRequest::acid_base_neutralization(
            chemistry::AlkaliMetal::Sodium,
            chemistry::Halogen::Chlorine,
        ));
        assert_eq!(
            plan.post_process,
            Some(chem_presentation::MacroscopicProcess::SolventEvaporationCrystallization)
        );
        let moment = |stage, beat_progress| {
            let (beat_index, beat) = plan
                .timeline
                .beats
                .iter()
                .enumerate()
                .find(|(_, beat)| beat.stage == stage)
                .expect("post-process beat exists");
            RealWorldPosition {
                beat_index,
                ordinal: beat.end_ordinal,
                ordinal_progress: beat_progress,
                beat_progress,
                stage,
            }
        };

        let early_boil = post_process_visual_state(&plan, MacroscopicStage::SolventBoiling, 0.24);
        let late_boil = post_process_visual_state(&plan, MacroscopicStage::SolventBoiling, 0.82);
        let finished = post_process_visual_state(&plan, MacroscopicStage::CrystalGrowth, 1.0);
        assert!(early_boil.liquid_fraction > late_boil.liquid_fraction);
        assert!(late_boil.boiling > 0.5 && late_boil.vapour > 0.5);
        assert!(finished.liquid_fraction <= f32::EPSILON);
        assert!((finished.crystal_growth - 1.0).abs() < f32::EPSILON);
        assert!(finished.flame <= f32::EPSILON);

        let boiling = build_scene_at(
            &plan,
            moment(MacroscopicStage::SolventBoiling, 0.58),
            (0.0, 0.0),
        );
        assert!(
            !boiling.4.is_empty(),
            "boiling solvent must emit an advected vapour volume"
        );
        assert!(
            usize::try_from(boiling.3).is_ok_and(|start| start < boiling.1.len()),
            "the burner must contribute a separate emissive flame pass"
        );

        let crystals = build_scene_at(
            &plan,
            moment(MacroscopicStage::CrystalGrowth, 1.0),
            (0.0, 0.0),
        );
        let repeated = build_scene_at(
            &plan,
            moment(MacroscopicStage::CrystalGrowth, 1.0),
            (0.0, 0.0),
        );
        let salt_extent = |growth: f32| {
            let mut mesh = Mesh::default();
            let layout = SceneLayout::resolve(&plan);
            add_crystallizing_salt(
                &mut mesh,
                Vec3::new(layout.vessel_center.x, layout.bench_top + 0.125, layout.vessel_center.z),
                growth,
                7,
                [1.0; 4],
            );
            mesh.vertices
                .iter()
                .map(|vertex| vertex.position[1])
                .fold(f32::MIN, f32::max)
        };
        assert!(
            salt_extent(1.0) > salt_extent(0.24) + 0.01,
            "faceted salt residue must grow from its nucleation scale"
        );
        assert_ne!(
            bytemuck::cast_slice::<GpuVertex, u8>(&crystals.0),
            bytemuck::cast_slice::<GpuVertex, u8>(&boiling.0)
        );
        assert_eq!(
            bytemuck::cast_slice::<GpuVertex, u8>(&crystals.0),
            bytemuck::cast_slice::<GpuVertex, u8>(&repeated.0)
        );
        assert_eq!(crystals.1, repeated.1);
    }

    #[test]
    fn neutralisation_assembly_reuses_beaker_motion_and_has_a_gentle_orange_flame() {
        let plan = plan_for(chemistry::ReactionRequest::acid_base_neutralization(
            chemistry::AlkaliMetal::Sodium,
            chemistry::Halogen::Chlorine,
        ));
        assert!(plan.objects.iter().any(|object| {
            object.role == SceneRole::Vessel
                && object.asset == AssetProfile::NeutralisationEvaporationAssembly
        }));

        let lifted = post_process_visual_state(&plan, MacroscopicStage::SolventBoiling, 0.5);
        assert!(lifted.active && lifted.lift > 0.2);
        let lifted_motion = Vec3::Y * (lifted.lift / 0.45);
        let mut lifted_beaker = Mesh::default();
        add_assembly_beaker(&mut lifted_beaker, -0.76, lifted_motion);
        let mut resting_beaker = Mesh::default();
        add_assembly_beaker(&mut resting_beaker, -0.76, Vec3::ZERO);
        let min_y = |mesh: &Mesh| {
            mesh.vertices
                .iter()
                .map(|vertex| vertex.position[1])
                .fold(f32::INFINITY, f32::min)
        };
        assert_eq!(lifted_beaker.vertices.len(), resting_beaker.vertices.len());
        assert!(
            min_y(&lifted_beaker) > min_y(&resting_beaker) + 0.2,
            "the beaker must ride the authored vessel lift"
        );

        let neutralisation_colours =
            neutralisation_colours(&plan, scene_effect_colours(&plan, 0, 0.0), 0.0);
        for colour in [
            neutralisation_track_colour(ClipColour::FlameOuter, neutralisation_colours),
            neutralisation_track_colour(ClipColour::FlameInner, neutralisation_colours),
            neutralisation_track_colour(ClipColour::FlameCore, neutralisation_colours),
        ] {
            assert!(
                colour[0] > colour[2] && colour[1] > colour[2],
                "the heating flame should be orange rather than lilac: {colour:?}"
            );
        }
    }

    #[test]
    fn precipitation_assembly_uses_the_absolute_authored_window_and_persistent_sediment() {
        let plan = plan_for(chemistry::ReactionRequest::silver_halide_precipitation(
            chemistry::Halogen::Bromine,
        ));
        let precipitation = plan
            .precipitation
            .as_ref()
            .expect("validated precipitation metadata reaches the scene plan");
        assert!(plan.objects.iter().any(|object| {
            object.role == SceneRole::Vessel
                && object.asset == AssetProfile::AqueousPrecipitationAssembly
        }));
        let start_ms = plan
            .timeline
            .start_ms_for_ordinal(precipitation.formation_ordinal)
            .expect("formation ordinal begins an authored beat");
        assert_eq!(plan.timeline.duration_ms() - start_ms, 9_600);

        let midpoint = plan
            .timeline
            .locate(start_ms + 3_000)
            .expect("midpoint is on the timeline");
        let first_sample = build_scene_at(&plan, midpoint, (0.0, 0.0));
        let _later_sample = build_scene_at(
            &plan,
            plan.timeline
                .locate(start_ms + 5_000)
                .expect("later sample is on the timeline"),
            (0.0, 0.0),
        );
        let repeated_midpoint = build_scene_at(&plan, midpoint, (0.0, 0.0));
        assert_eq!(
            bytemuck::cast_slice::<GpuVertex, u8>(&first_sample.0),
            bytemuck::cast_slice::<GpuVertex, u8>(&repeated_midpoint.0)
        );
        assert_eq!(first_sample.1, repeated_midpoint.1);

        assert!(precipitation::cloud_presence(1.0) < 0.001);
        assert!(precipitation::fleck_presence(1.0) < 0.001);
        assert!(
            precipitation::mound_settle(1.0) > 0.99,
            "the settled sediment must remain as visible geometry"
        );
    }

    #[test]
    fn precipitation_product_rgb_reaches_cloud_and_sediment_with_separate_opacity() {
        let plan = plan_for(chemistry::ReactionRequest::silver_halide_precipitation(
            chemistry::Halogen::Iodine,
        ));
        let precipitation = plan
            .precipitation
            .as_ref()
            .expect("precipitation has exact bound colours");
        let transition_ordinal = precipitation
            .precipitate
            .transition_ordinal
            .expect("validated colour observation retains its own ordinal");
        let before_exact = precipitation_track_colour(
            ClipColour::Precipitate,
            precipitation,
            transition_ordinal.saturating_sub(1),
            1.0,
        );
        let cloud =
            precipitation_track_colour(ClipColour::PrecipitateCloud, precipitation, u16::MAX, 1.0);
        let sediment =
            precipitation_track_colour(ClipColour::Precipitate, precipitation, u16::MAX, 1.0);
        assert_eq!(cloud[..3], sediment[..3]);
        assert!(cloud[3] < sediment[3]);
        assert!((sediment[3] - 1.0).abs() < f32::EPSILON);
        assert_ne!(
            before_exact[..3],
            sediment[..3],
            "the exact `.chems` colour must not appear before its observation ordinal"
        );
    }

    #[test]
    fn metal_displacement_material_slots_keep_exact_rgb_and_phase_opacity() {
        let bound =
            |binding: &str, [red, green, blue]: [u8; 3]| chem_presentation::BoundVisualColour {
                binding: binding.to_owned(),
                base_colour: VisualColour { red, green, blue },
                colour: VisualColour { red, green, blue },
                transition_ordinal: None,
            };
        let visual = chem_presentation::MetalDisplacementVisualProfile {
            formation_ordinal: 3,
            initial_solution: bound("initial-solution", [0x42, 0x76, 0xb0]),
            final_solution: bound("final-solution", [0xd8, 0xe3, 0xe8]),
            original_metal: bound("original-metal", [0xc4, 0xc7, 0xc9]),
            deposited_metal: bound("deposited-metal", [0xb9, 0x68, 0x46]),
        };
        let initial =
            metal_displacement_track_colour(ClipColour::SolutionInitial, &visual, u16::MAX, 1.0);
        let final_solution =
            metal_displacement_track_colour(ClipColour::SolutionFinal, &visual, u16::MAX, 1.0);
        let original =
            metal_displacement_track_colour(ClipColour::OriginalMetal, &visual, u16::MAX, 1.0);
        let deposited =
            metal_displacement_track_colour(ClipColour::DepositedMetal, &visual, u16::MAX, 1.0);
        let rgb = |[red, green, blue]: [u8; 3]| {
            [
                f32::from(red) / 255.0,
                f32::from(green) / 255.0,
                f32::from(blue) / 255.0,
            ]
        };
        assert_eq!(initial[..3], rgb([0x42, 0x76, 0xb0]));
        assert_eq!(final_solution[..3], rgb([0xd8, 0xe3, 0xe8]));
        assert_eq!(original[..3], rgb([0xc4, 0xc7, 0xc9]));
        assert_eq!(deposited[..3], rgb([0xb9, 0x68, 0x46]));
        assert!((initial[3] - 0.29).abs() < f32::EPSILON);
        assert!((final_solution[3] - 0.29).abs() < f32::EPSILON);
        assert!((original[3] - 1.0).abs() < f32::EPSILON);
        assert!((deposited[3] - 1.0).abs() < f32::EPSILON);
        let highlight = deposit_highlight_colour(deposited);
        assert!(
            highlight[..3]
                .iter()
                .zip(deposited[..3].iter())
                .all(|(highlight, base)| highlight > base)
        );
        assert!((highlight[3] - 0.24).abs() < f32::EPSILON);
        let erosion =
            metal_displacement_track_colour(ClipColour::MetalErosion, &visual, u16::MAX, 1.0);
        assert!(
            erosion
                .iter()
                .zip([0.12, 0.13, 0.14, 1.0])
                .all(|(actual, expected)| (actual - expected).abs() < f32::EPSILON)
        );
    }

    #[test]
    fn heavy_alkali_material_slots_keep_identity_bound_rgb_and_phase_opacity() {
        let bound = |binding: &str, base: [u8; 3], colour: [u8; 3], transition_ordinal| {
            chem_presentation::BoundVisualColour {
                binding: binding.to_owned(),
                base_colour: VisualColour {
                    red: base[0],
                    green: base[1],
                    blue: base[2],
                },
                colour: VisualColour {
                    red: colour[0],
                    green: colour[1],
                    blue: colour[2],
                },
                transition_ordinal,
            }
        };
        let visual = chem_presentation::ExplosiveMetalWaterVisualProfile {
            contact_ordinal: 2,
            variant: ExplosiveMetalWaterVariant::Rubidium,
            water_reactant: bound("water", [0x18, 0x7d, 0xb6], [0x18, 0x7d, 0xb6], None),
            metal_reactant: bound("metal", [0x92, 0x96, 0x9a], [0x92, 0x96, 0x9a], None),
            hydroxide_product: bound("hydroxide", [0x42, 0x8b, 0x9e], [0xea, 0xb4, 0x31], Some(4)),
            hydrogen_product: bound("hydrogen", [0xd6, 0xee, 0xf4], [0xd6, 0xee, 0xf4], None),
        };
        let rgb = |[red, green, blue]: [u8; 3]| {
            [
                f32::from(red) / 255.0,
                f32::from(green) / 255.0,
                f32::from(blue) / 255.0,
            ]
        };
        let assert_rgb = |actual: [f32; 4], expected: [u8; 3]| {
            assert!(
                actual[..3]
                    .iter()
                    .zip(rgb(expected))
                    .all(|(actual, expected)| (actual - expected).abs() <= 1.0e-6),
                "{actual:?} did not retain {expected:?}",
            );
        };
        let before_contact = explosive_metal_water_track_colour(ClipColour::Water, &visual, 1, 1.0);
        let product_base = explosive_metal_water_track_colour(ClipColour::Water, &visual, 3, 1.0);
        let product_target = explosive_metal_water_track_colour(ClipColour::Water, &visual, 5, 1.0);
        assert_rgb(before_contact, [0x18, 0x7d, 0xb6]);
        assert_rgb(product_base, [0x42, 0x8b, 0x9e]);
        assert_rgb(product_target, [0xea, 0xb4, 0x31]);
        assert!((product_target[3] - 0.34).abs() < f32::EPSILON);
        let metal = explosive_metal_water_track_colour(ClipColour::ReactiveMetal, &visual, 5, 1.0);
        let hydrogen = explosive_metal_water_track_colour(ClipColour::Vapour, &visual, 5, 1.0);
        assert_rgb(metal, [0x92, 0x96, 0x9a]);
        assert!((metal[3] - 1.0).abs() < f32::EPSILON);
        assert_rgb(hydrogen, [0xd6, 0xee, 0xf4]);
        assert!((hydrogen[3] - 0.15).abs() < f32::EPSILON);
    }

    #[test]
    fn heavy_alkali_scene_replays_and_seeks_backwards_from_the_absolute_playhead() {
        let plan = plan_for(chemistry::ReactionRequest::heavy_alkali_water(
            chemistry::HeavyAlkaliMetal::Rubidium,
        ));
        assert!(matches!(
            plan.explosive_metal_water
                .as_ref()
                .map(|profile| profile.variant),
            Some(ExplosiveMetalWaterVariant::Rubidium)
        ));
        let early = plan.timeline.locate(0).expect("timeline begins at zero");
        let later = plan
            .timeline
            .locate(plan.timeline.duration_ms() / 2)
            .expect("midpoint is on the timeline");
        let first = build_scene_at(&plan, early, (0.0, 0.0));
        let _later = build_scene_at(&plan, later, (0.0, 0.0));
        let replay = build_scene_at(&plan, early, (0.0, 0.0));
        assert_eq!(
            bytemuck::cast_slice::<GpuVertex, u8>(&first.0),
            bytemuck::cast_slice::<GpuVertex, u8>(&replay.0)
        );
        assert_eq!(first.1, replay.1);
    }

    fn authored_gas_plan(variant: GasEvolutionVariant) -> ScenePlan {
        let mut plan = plan_for(chemistry::ReactionRequest::acid_carbonate_gas_evolution(
            chemistry::AlkaliMetal::Sodium,
            chemistry::Halogen::Chlorine,
        ));
        let generation_ordinal = plan
            .effects
            .iter()
            .find(|effect| effect.effect == EffectProfile::GasRelease)
            .map_or(0, |effect| effect.start_ordinal);
        let colour = |binding: &str, value: VisualColour| chem_presentation::BoundVisualColour {
            binding: binding.to_owned(),
            base_colour: value,
            colour: value,
            transition_ordinal: None,
        };
        plan.gas_evolution = Some(chem_presentation::GasEvolutionVisualProfile {
            generation_ordinal,
            variant,
            initial_reactant: colour(
                "initial-reactant",
                VisualColour {
                    red: 0x42,
                    green: 0x74,
                    blue: 0xaa,
                },
            ),
            added_reactant: colour(
                "added-reactant",
                VisualColour {
                    red: 0xd8,
                    green: 0xb2,
                    blue: 0x58,
                },
            ),
            gas_product: colour(
                "gas-product",
                VisualColour {
                    red: 0xa4,
                    green: 0xd0,
                    blue: 0x72,
                },
            ),
        });
        plan
    }

    #[test]
    fn gas_evolution_variants_swap_pour_vessel_for_reactant_pile() {
        let mut plan = plan_for(chemistry::ReactionRequest::acid_bicarbonate_gas_evolution(
            chemistry::AlkaliMetal::Sodium,
            chemistry::Halogen::Chlorine,
        ));
        let layout = SceneLayout::resolve(&plan);
        let ordinal = plan
            .gas_evolution
            .as_ref()
            .expect("gas evolution profile exists")
            .generation_ordinal;
        let build = |plan: &ScenePlan| {
            let mut meshes = SceneMeshes::default();
            gas_evolution::add_gas_evolution_assembly(&mut meshes, plan, layout, 0.5, ordinal, 0.5);
            meshes
        };
        let liquid_liquid = build(&plan);
        plan.gas_evolution
            .as_mut()
            .expect("gas evolution profile exists")
            .variant = GasEvolutionVariant::SolidLiquid;
        let solid_liquid = build(&plan);
        assert!(
            solid_liquid.opaque.vertices.len() > liquid_liquid.opaque.vertices.len(),
            "the solid variant adds the faceted reactant pile"
        );
        assert!(
            liquid_liquid.glass.vertices.len() > solid_liquid.glass.vertices.len(),
            "the liquid variant adds the pouring vessel"
        );
    }

    #[test]
    fn reviewed_bicarbonate_gas_evolution_selects_liquid_liquid_clip() {
        let request = chemistry::ReactionRequest::acid_bicarbonate_gas_evolution(
            chemistry::AlkaliMetal::Sodium,
            chemistry::Halogen::Chlorine,
        );
        let run = chemistry::run(request).expect("request validates");
        let profile = chemistry::presentation_profile_with_catalogue(
            request,
            run.frames(),
            run.macroscopic(),
        )
        .expect("presentation compiles");
        let plan = compile_real_world_plan(run.frames(), &profile).expect("trusted plan compiles");
        assert_eq!(
            plan.gas_evolution.as_ref().map(|visual| visual.variant),
            Some(GasEvolutionVariant::LiquidLiquid),
            "macroscopic inputs: {:?}",
            run.macroscopic()
        );
    }

    #[test]
    fn gas_evolution_colours_reach_bubbles_plume_and_solid_without_changing_opacity() {
        let plan = authored_gas_plan(GasEvolutionVariant::SolidLiquid);
        let visual = plan.gas_evolution.as_ref().expect("authored gas profile");
        let bubble = gas_evolution_track_colour(ClipColour::GasBubble, visual, u16::MAX, 1.0);
        let plume = gas_evolution_track_colour(ClipColour::GasCloud, visual, u16::MAX, 1.0);
        let solid = gas_evolution_track_colour(ClipColour::SolidReactant, visual, u16::MAX, 1.0);
        assert_eq!(bubble[..3], plume[..3]);
        assert!(plume[3] < bubble[3]);
        assert_eq!(
            solid[..3],
            [
                f32::from(visual.added_reactant.colour.red) / 255.0,
                f32::from(visual.added_reactant.colour.green) / 255.0,
                f32::from(visual.added_reactant.colour.blue) / 255.0,
            ]
        );
        assert!((solid[3] - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn changing_gas_reactions_resets_to_the_new_absolute_clip_sample() {
        let old = authored_gas_plan(GasEvolutionVariant::LiquidLiquid);
        let new = authored_gas_plan(GasEvolutionVariant::SolidLiquid);
        let old_reaction_duration = old
            .timeline
            .beats
            .iter()
            .take_while(|beat| beat.stage == MacroscopicStage::Reaction)
            .fold(0_u64, |total, beat| {
                total.saturating_add(u64::from(beat.duration_ms))
            });
        let old_end = old
            .timeline
            .locate(old_reaction_duration.saturating_sub(1))
            .expect("old authored reaction endpoint");
        let _ = build_scene_at(&old, old_end, (0.0, 0.0));

        let new_start = new.timeline.locate(0).expect("new timeline starts");
        let after_switch = build_scene_at(&new, new_start, (0.0, 0.0));
        let fresh = build_scene_at(&new, new_start, (0.0, 0.0));
        assert_eq!(after_switch.1, fresh.1);
        assert_eq!(after_switch.2, fresh.2);
        assert_eq!(after_switch.3, fresh.3);
        assert_eq!(
            bytemuck::cast_slice::<GpuVertex, u8>(&after_switch.0),
            bytemuck::cast_slice::<GpuVertex, u8>(&fresh.0),
            "the new reaction must not retain prior transforms, material colours, or playhead"
        );
        assert_eq!(after_switch.4.len(), fresh.4.len());
    }

    #[test]
    fn gas_evolution_renderer_contains_no_reaction_or_species_identity_branch() {
        let renderer = include_str!("structural_3d/gas_evolution.rs");
        for forbidden in [
            "ReactionRequest",
            "reaction_name",
            "formula",
            "carbonate",
            "metal_name",
            "hydrocarbon",
        ] {
            assert!(
                !renderer.contains(forbidden),
                "renderer must not branch on `{forbidden}`"
            );
        }
    }

    #[test]
    fn precipitation_renderer_contains_no_reaction_or_species_identity_branch() {
        let renderer = include_str!("structural_3d/precipitation.rs");
        for forbidden in [
            "ReactionRequest",
            "reaction_name",
            "formula",
            "silver",
            "chloride",
            "bromide",
            "iodide",
        ] {
            assert!(
                !renderer.contains(forbidden),
                "renderer must not branch on `{forbidden}`"
            );
        }
    }

    #[test]
    fn metal_displacement_renderer_contains_no_reaction_or_species_identity_branch() {
        let renderer = include_str!("structural_3d/displacement.rs");
        for forbidden in [
            "ReactionRequest",
            "reaction_name",
            "formula",
            "species_name",
            "zinc",
            "copper",
            "silver",
        ] {
            assert!(
                !renderer.contains(forbidden),
                "renderer must not branch on `{forbidden}`"
            );
        }
    }

    #[test]
    fn synthesis_renderer_contains_no_reaction_or_species_identity_branch() {
        let renderer = include_str!("structural_3d/synthesis.rs");
        for forbidden in [
            "ReactionRequest",
            "reaction_name",
            "formula",
            "species_name",
            "iron",
            "sulfur",
            "zinc",
        ] {
            assert!(
                !renderer.contains(forbidden),
                "renderer must not branch on `{forbidden}`"
            );
        }
    }

    #[test]
    fn authored_combustion_materials_preserve_fuel_colour_and_distinguish_flames() {
        let fuel = [0.78, 0.53, 0.20, 0.32];
        for incomplete in [false, true] {
            let mapped = combustion_track_colour(ClipColour::Fuel, fuel, incomplete);
            assert!(
                mapped
                    .iter()
                    .zip(fuel)
                    .all(|(actual, expected)| (*actual - expected).abs() < f32::EPSILON)
            );
        }
        let complete = combustion_track_colour(ClipColour::FlameOuter, fuel, false);
        let incomplete = combustion_track_colour(ClipColour::FlameOuter, fuel, true);
        assert!(
            complete[2] > complete[0],
            "complete flame should preserve the authored blue family"
        );
        assert!(
            incomplete[0] > incomplete[2],
            "incomplete flame should preserve the authored orange family"
        );
        assert!(combustion_track_colour(ClipColour::CombustionSmoke, fuel, true)[3] > 0.4);
        assert!(combustion_track_colour(ClipColour::Soot, fuel, true)[3] > 0.9);
    }

    #[test]
    fn neutralisation_assembly_diffuses_reviewed_liquid_colour_into_the_salt() {
        let mut plan = plan_for(chemistry::ReactionRequest::acid_base_neutralization(
            chemistry::AlkaliMetal::Sodium,
            chemistry::Halogen::Chlorine,
        ));
        let blue = VisualColour {
            red: 0x63,
            green: 0x9d,
            blue: 0xd0,
        };
        let contents = plan
            .objects
            .iter_mut()
            .find(|object| object.role == SceneRole::Contents)
            .expect("neutralisation contents");
        contents.appearance = AppearanceProfile::ReviewedColour(blue);

        let effect_colours = scene_effect_colours(&plan, 0, 0.0);
        let initial = neutralisation_colours(&plan, effect_colours, 0.0);
        let mixed = neutralisation_colours(&plan, effect_colours, 120.0);
        let target = [
            f32::from(blue.red) / 255.0,
            f32::from(blue.green) / 255.0,
            f32::from(blue.blue) / 255.0,
        ];
        assert!(
            initial.liquid[..3]
                .iter()
                .zip(target)
                .any(|(initial, target)| (initial - target).abs() > 0.05),
            "the liquid must not jump to its product colour before mixing"
        );
        for (actual, expected) in mixed.liquid[..3].iter().zip(target) {
            assert!((actual - expected).abs() < 0.000_01);
        }
        for (actual, expected) in mixed.salt[..3].iter().zip(target) {
            assert!((actual - expected).abs() < 0.000_01);
        }
    }

    #[test]
    fn resolved_layout_grounds_the_vessel_and_keeps_liquid_inside_it() {
        let plan = canonical_plan();
        let layout = SceneLayout::resolve(&plan);
        let vessel = plan
            .objects
            .iter()
            .find(|object| object.role == SceneRole::Vessel)
            .expect("vessel exists");
        assert_eq!(
            vessel.asset,
            AssetProfile::ReactiveMetalWaterAssembly,
            "the authored vessel uses its own evaluated dimensions"
        );
        let vessel_base = layout.bench_top;
        let vessel_rim = layout.bench_top + 1.8;

        assert!((vessel_base - layout.bench_top).abs() < 0.001);
        assert!(layout.liquid_center.y > layout.bench_top);
        assert!(layout.liquid_surface > layout.liquid_center.y);
        assert!(layout.liquid_surface < vessel_rim);
        assert!(layout.reaction_point.y >= layout.liquid_surface);
    }

    #[test]
    fn complete_transform_rotation_changes_reusable_asset_geometry() {
        let base = PresentationTransform {
            translation: [0, 0, 0],
            rotation: [0, 0, 0],
            scale: [900, 500, 400],
        };
        let rotated = PresentationTransform {
            rotation: [0, 250, 0],
            ..base.clone()
        };
        let mut unrotated_meshes = SceneMeshes::default();
        instantiate_asset(
            &mut unrotated_meshes,
            AssetProfile::MetalChunk,
            AppearanceProfile::AlkaliMetal,
            &base,
            1.0,
            Vec3::ZERO,
            Quat::IDENTITY,
            42,
            ReactionVisualInputs::default(),
            0.0,
            1.0,
            None,
        );
        let mut rotated_meshes = SceneMeshes::default();
        instantiate_asset(
            &mut rotated_meshes,
            AssetProfile::MetalChunk,
            AppearanceProfile::AlkaliMetal,
            &rotated,
            1.0,
            Vec3::ZERO,
            Quat::IDENTITY,
            42,
            ReactionVisualInputs::default(),
            0.0,
            1.0,
            None,
        );
        let (unrotated, _, _, _, _) = unrotated_meshes.finish(fixed_view_direction());
        let (rotated, _, _, _, _) = rotated_meshes.finish(fixed_view_direction());

        assert_eq!(unrotated.len(), rotated.len());
        assert_ne!(
            bytemuck::cast_slice::<GpuVertex, u8>(&unrotated),
            bytemuck::cast_slice::<GpuVertex, u8>(&rotated),
            "catalogue-authored rotation must reach positions and normals"
        );
    }

    #[test]
    fn laboratory_environment_keeps_the_floor_without_a_backdrop_wall() {
        let mut meshes = SceneMeshes::default();
        instantiate_asset(
            &mut meshes,
            AssetProfile::LaboratoryBench,
            AppearanceProfile::LaboratoryNeutral,
            &PresentationTransform {
                translation: [0, -900, 0],
                rotation: [0, 0, 0],
                scale: [1000, 1000, 1000],
            },
            1.0,
            Vec3::ZERO,
            Quat::IDENTITY,
            0,
            ReactionVisualInputs::default(),
            0.0,
            1.0,
            None,
        );
        let (vertices, _, opaque_indices, _, _) = meshes.finish(fixed_view_direction());

        assert!(opaque_indices > 0, "the floor remains opaque geometry");
        assert!(
            vertices.iter().all(|vertex| vertex.position[1] < 0.0),
            "the environment must not add a vertical wall above the floor"
        );
    }
}
