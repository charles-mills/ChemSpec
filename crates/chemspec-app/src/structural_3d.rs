//! Depth-tested low-poly rendering of reviewed macroscopic scene plans.
//!
//! Exact atoms and bonds remain available to the dedicated structural views.
//! This renderer consumes only trusted macroscopic assets and effects; it never
//! infers structure, parses source, or selects reaction rules.

use std::sync::OnceLock;

use bytemuck::{Pod, Zeroable};
use chem_catalogue::ObservationPredicate;
use chem_presentation::{
    AppearanceProfile, AssetProfile, EffectIntensity, EffectProfile, FlamePalette,
    GasEvolutionVariant, MacroscopicStage, PresentationColourTransition, PresentationEffect,
    PresentationObject, PresentationTransform, ReactionVisualInputs, SceneRole, VisualColour,
};
use chem_presentation::{RealWorldPosition, ScenePlan};
use glam::{EulerRot, Mat4, Quat, Vec3};
use iced::widget::shader::{self, Program};
use iced::{Rectangle, wgpu};

use crate::animated_clip::{AnimatedClip, ClipColour, ClipModule, ClipPass, ClipTrack, ClipVertex};
use crate::gas_fluid::{GasFlowControls, GasFluidVolume};
use crate::scene_registry::{self, AssetGeometry, EffectDynamics, EffectGeometry};

const MAX_VERTICES: u64 = 32_768;
const MAX_INDICES: u64 = 98_304;
const MAX_GAS_SPLATS: u64 = 4_096;

#[derive(Debug, Clone)]
pub struct Scene {
    plan: ScenePlan,
    moment: RealWorldPosition,
}

impl Scene {
    pub fn new(plan: &ScenePlan, moment: RealWorldPosition) -> Self {
        Self {
            plan: plan.clone(),
            moment,
        }
    }
}

/// Deliberately stateless: the macroscopic view has no orbit, pan, or zoom
/// interaction. Vessel-size framing is derived deterministically from the plan.
#[derive(Debug, Default)]
pub struct FixedCameraState;

impl<Message> Program<Message> for Scene {
    type State = FixedCameraState;
    type Primitive = ScenePrimitive;

    fn draw(
        &self,
        _state: &Self::State,
        _cursor: iced::mouse::Cursor,
        _bounds: Rectangle,
    ) -> Self::Primitive {
        let (vertices, indices, opaque_index_count, transparent_index_count, mut gas_splats) =
            build_scene_at(&self.plan, self.moment);
        let camera = fixed_camera_pose(&self.plan);
        let focus_target = SceneLayout::resolve(&self.plan).camera_target;
        let eye = focus_target
            + Quat::from_rotation_y(camera.yaw)
                * Quat::from_rotation_x(camera.pitch)
                * Vec3::new(0.0, 0.0, 8.0);
        let view_direction = (focus_target - eye).normalize_or_zero();
        gas_splats.sort_by(|left, right| {
            let left_depth = (Vec3::from_array(left.center) - eye).dot(view_direction);
            let right_depth = (Vec3::from_array(right.center) - eye).dot(view_direction);
            right_depth.total_cmp(&left_depth)
        });
        ScenePrimitive {
            vertices,
            indices,
            opaque_index_count,
            transparent_index_count,
            gas_splats,
            yaw: camera.yaw,
            pitch: camera.pitch,
            view_height: camera.view_height,
            focus_target,
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 3],
    normal: [f32; 3],
    color: [f32; 4],
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

const METAL_MESH_BYTES: &[u8] = include_bytes!("../assets/models/metal.mesh");
const EMBEDDED_MESH_MAGIC: &[u8; 8] = b"CMSHMESH";
const EMBEDDED_MESH_VERSION: u32 = 1;
static METAL_MESH: OnceLock<EmbeddedMesh> = OnceLock::new();
const ALKALI_WATER_CLIP_BYTES: &[u8] = include_bytes!("../assets/models/alkali_water.clip");
static ALKALI_WATER_CLIP: OnceLock<AnimatedClip> = OnceLock::new();
const NEUTRALISATION_CLIP_BYTES: &[u8] = include_bytes!("../assets/models/neutralisation.clip");
static NEUTRALISATION_CLIP: OnceLock<AnimatedClip> = OnceLock::new();
const COMPLETE_COMBUSTION_CLIP_BYTES: &[u8] =
    include_bytes!("../assets/models/complete_combustion.clip");
static COMPLETE_COMBUSTION_CLIP: OnceLock<AnimatedClip> = OnceLock::new();
const INCOMPLETE_COMBUSTION_CLIP_BYTES: &[u8] =
    include_bytes!("../assets/models/incomplete_combustion.clip");
static INCOMPLETE_COMBUSTION_CLIP: OnceLock<AnimatedClip> = OnceLock::new();
const PRECIPITATION_CLIP_BYTES: &[u8] = include_bytes!("../assets/models/precipitation.clip");
static PRECIPITATION_CLIP: OnceLock<AnimatedClip> = OnceLock::new();
const GAS_EVOLUTION_LIQUID_LIQUID_CLIP_BYTES: &[u8] =
    include_bytes!("../assets/models/gas_evolution_liquid_liquid.clip");
static GAS_EVOLUTION_LIQUID_LIQUID_CLIP: OnceLock<AnimatedClip> = OnceLock::new();
const GAS_EVOLUTION_SOLID_LIQUID_CLIP_BYTES: &[u8] =
    include_bytes!("../assets/models/gas_evolution_solid_liquid.clip");
static GAS_EVOLUTION_SOLID_LIQUID_CLIP: OnceLock<AnimatedClip> = OnceLock::new();
const METAL_DISPLACEMENT_CLIP_BYTES: &[u8] =
    include_bytes!("../assets/models/metal_displacement.clip");
static METAL_DISPLACEMENT_CLIP: OnceLock<AnimatedClip> = OnceLock::new();
const SYNTHESIS_COMBINATION_CLIP_BYTES: &[u8] =
    include_bytes!("../assets/models/synthesis_combination.clip");
static SYNTHESIS_COMBINATION_CLIP: OnceLock<AnimatedClip> = OnceLock::new();

fn embedded_metal_mesh() -> &'static EmbeddedMesh {
    METAL_MESH.get_or_init(|| {
        parse_embedded_mesh(METAL_MESH_BYTES)
            .unwrap_or_else(|error| panic!("embedded metal mesh is invalid: {error}"))
    })
}

fn alkali_water_clip() -> &'static AnimatedClip {
    ALKALI_WATER_CLIP.get_or_init(|| {
        AnimatedClip::parse(ALKALI_WATER_CLIP_BYTES)
            .unwrap_or_else(|error| panic!("embedded alkali-water clip is invalid: {error}"))
    })
}

fn neutralisation_clip() -> &'static AnimatedClip {
    NEUTRALISATION_CLIP.get_or_init(|| {
        AnimatedClip::parse(NEUTRALISATION_CLIP_BYTES)
            .unwrap_or_else(|error| panic!("embedded neutralisation clip is invalid: {error}"))
    })
}

fn complete_combustion_clip() -> &'static AnimatedClip {
    COMPLETE_COMBUSTION_CLIP.get_or_init(|| {
        AnimatedClip::parse(COMPLETE_COMBUSTION_CLIP_BYTES)
            .unwrap_or_else(|error| panic!("embedded complete-combustion clip is invalid: {error}"))
    })
}

fn incomplete_combustion_clip() -> &'static AnimatedClip {
    INCOMPLETE_COMBUSTION_CLIP.get_or_init(|| {
        AnimatedClip::parse(INCOMPLETE_COMBUSTION_CLIP_BYTES).unwrap_or_else(|error| {
            panic!("embedded incomplete-combustion clip is invalid: {error}")
        })
    })
}

fn precipitation_clip() -> &'static AnimatedClip {
    PRECIPITATION_CLIP.get_or_init(|| {
        AnimatedClip::parse(PRECIPITATION_CLIP_BYTES)
            .unwrap_or_else(|error| panic!("embedded precipitation clip is invalid: {error}"))
    })
}

fn gas_evolution_clip(variant: GasEvolutionVariant) -> &'static AnimatedClip {
    match variant {
        GasEvolutionVariant::LiquidLiquid => GAS_EVOLUTION_LIQUID_LIQUID_CLIP.get_or_init(|| {
            AnimatedClip::parse(GAS_EVOLUTION_LIQUID_LIQUID_CLIP_BYTES).unwrap_or_else(|error| {
                panic!("embedded liquid-liquid gas-evolution clip is invalid: {error}")
            })
        }),
        GasEvolutionVariant::SolidLiquid => GAS_EVOLUTION_SOLID_LIQUID_CLIP.get_or_init(|| {
            AnimatedClip::parse(GAS_EVOLUTION_SOLID_LIQUID_CLIP_BYTES).unwrap_or_else(|error| {
                panic!("embedded solid-liquid gas-evolution clip is invalid: {error}")
            })
        }),
    }
}

fn metal_displacement_clip() -> &'static AnimatedClip {
    METAL_DISPLACEMENT_CLIP.get_or_init(|| {
        AnimatedClip::parse(METAL_DISPLACEMENT_CLIP_BYTES)
            .unwrap_or_else(|error| panic!("embedded metal-displacement clip is invalid: {error}"))
    })
}

fn synthesis_combination_clip() -> &'static AnimatedClip {
    SYNTHESIS_COMBINATION_CLIP.get_or_init(|| {
        AnimatedClip::parse(SYNTHESIS_COMBINATION_CLIP_BYTES).unwrap_or_else(|error| {
            panic!("embedded synthesis-combination clip is invalid: {error}")
        })
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
}

#[derive(Debug)]
pub struct ScenePrimitive {
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
    opaque_index_count: u32,
    transparent_index_count: u32,
    gas_splats: Vec<GasSplat>,
    yaw: f32,
    pitch: f32,
    view_height: f32,
    focus_target: Vec3,
}

#[derive(Debug)]
pub struct ScenePipeline {
    opaque_pipeline: wgpu::RenderPipeline,
    transparent_pipeline: wgpu::RenderPipeline,
    additive_pipeline: wgpu::RenderPipeline,
    gas_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    gas_buffer: wgpu::Buffer,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    depth: Option<DepthTarget>,
    opaque_index_count: u32,
    transparent_index_count: u32,
    index_count: u32,
    gas_count: u32,
    physical_bounds: [u32; 4],
}

#[derive(Debug)]
struct DepthTarget {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    size: [u32; 2],
}

impl shader::Pipeline for ScenePipeline {
    #[allow(clippy::too_many_lines)]
    fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("chemspec structural 3d shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("structural_3d.wgsl").into()),
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("chemspec structural 3d camera layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("chemspec structural 3d camera"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("chemspec structural 3d camera group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("chemspec structural 3d pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });
        let create_pipeline = |label: &'static str,
                               blend: Option<wgpu::BlendState>,
                               depth_write_enabled: bool,
                               cull_mode: Option<wgpu::Face>,
                               fragment_entry: &'static str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vertex"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x4],
                    }],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fragment_entry),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
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
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            })
        };
        let opaque_pipeline = create_pipeline(
            "chemspec structural 3d opaque pipeline",
            None,
            true,
            Some(wgpu::Face::Back),
            "fragment",
        );
        let transparent_pipeline = create_pipeline(
            "chemspec structural 3d transparent pipeline",
            Some(wgpu::BlendState::ALPHA_BLENDING),
            false,
            None,
            "fragment",
        );
        let additive_pipeline = create_pipeline(
            "chemspec structural 3d additive flame pipeline",
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
            layout: Some(&layout),
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
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
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
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        Self {
            opaque_pipeline,
            transparent_pipeline,
            additive_pipeline,
            gas_pipeline,
            vertex_buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("chemspec structural 3d vertices"),
                size: MAX_VERTICES * std::mem::size_of::<Vertex>() as u64,
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
            bind_group,
            depth: None,
            opaque_index_count: 0,
            transparent_index_count: 0,
            index_count: 0,
            gas_count: 0,
            physical_bounds: [0; 4],
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
        let viewport_size = viewport.physical_size();
        let depth_size = [viewport_size.width.max(1), viewport_size.height.max(1)];
        if pipeline
            .depth
            .as_ref()
            .is_none_or(|depth| depth.size != depth_size)
        {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("chemspec structural 3d depth"),
                size: wgpu::Extent3d {
                    width: depth_size[0],
                    height: depth_size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth32Float,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            pipeline.depth = Some(DepthTarget {
                _texture: texture,
                view,
                size: depth_size,
            });
        }
        if self.vertices.len() as u64 > MAX_VERTICES || self.indices.len() as u64 > MAX_INDICES {
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

        let aspect = width as f32 / height.max(1) as f32;
        let reaction_target = self.focus_target;
        let pitch = self.pitch.clamp(-1.18, -0.22);
        let eye = reaction_target
            + Quat::from_rotation_y(self.yaw)
                * Quat::from_rotation_x(pitch)
                * Vec3::new(0.0, 0.0, 8.0);
        let view = Mat4::look_at_rh(eye, reaction_target, Vec3::Y);
        let half_height = self.view_height * 0.5;
        let half_width = half_height * aspect;
        let projection = Mat4::orthographic_rh(
            -half_width,
            half_width,
            -half_height,
            half_height,
            0.1,
            50.0,
        );
        let uniform = CameraUniform {
            view_projection: (projection * view).to_cols_array_2d(),
            key_direction: [-0.55, -0.88, -0.48, 0.0],
            fill_direction: [0.70, -0.45, 0.55, 0.0],
            camera_position: [eye.x, eye.y, eye.z, 1.0],
        };
        queue.write_buffer(&pipeline.uniform_buffer, 0, bytemuck::bytes_of(&uniform));
    }

    #[allow(clippy::cast_precision_loss)]
    fn render(
        &self,
        pipeline: &Self::Pipeline,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        let Some(depth) = &pipeline.depth else { return };
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
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("chemspec structural 3d render pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth.view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_viewport(x as f32, y as f32, width as f32, height as f32, 0.0, 1.0);
        pass.set_scissor_rect(scissor_x, scissor_y, scissor_width, scissor_height);
        pass.set_pipeline(&pipeline.opaque_pipeline);
        pass.set_bind_group(0, &pipeline.bind_group, &[]);
        pass.set_vertex_buffer(0, pipeline.vertex_buffer.slice(..));
        pass.set_index_buffer(pipeline.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..pipeline.opaque_index_count, 0, 0..1);
        if pipeline.gas_count > 0 {
            pass.set_pipeline(&pipeline.gas_pipeline);
            pass.set_vertex_buffer(0, pipeline.gas_buffer.slice(..));
            pass.draw(0..6, 0..pipeline.gas_count);
            pass.set_vertex_buffer(0, pipeline.vertex_buffer.slice(..));
        }
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
            )
        }) {
            let vessel_center = Vec3::new(0.0, bench_top + 0.90, 0.0);
            let liquid_center = Vec3::new(0.0, bench_top + 0.81, 0.0);
            let liquid_surface = bench_top + 1.543;
            return Self {
                bench_top,
                has_vessel: true,
                vessel_center,
                vessel_scale: Vec3::new(0.99, 0.90, 0.99),
                has_liquid: true,
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
) -> (Vec<Vertex>, Vec<u32>, u32, u32, Vec<GasSplat>) {
    build_scene_with_stage(
        plan,
        ordinal,
        progress,
        MacroscopicStage::Reaction,
        progress,
        None,
    )
}

fn build_scene_at(
    plan: &ScenePlan,
    moment: RealWorldPosition,
) -> (Vec<Vertex>, Vec<u32>, u32, u32, Vec<GasSplat>) {
    let authored_clip_progress =
        if moment.stage == MacroscopicStage::Reaction && plan.gas_evolution.is_some() {
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
) -> (Vec<Vertex>, Vec<u32>, u32, u32, Vec<GasSplat>) {
    let mut meshes = SceneMeshes::default();
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
        object.role == SceneRole::Vessel && object.asset == AssetProfile::ReactiveMetalWaterAssembly
    }) {
        add_animated_alkali_water_assembly(
            &mut meshes,
            plan,
            layout,
            authored_clip_progress.unwrap_or(visual_inputs.reaction_progress),
        );
        return meshes.finish();
    }
    if stage == MacroscopicStage::Reaction && plan.gas_evolution.is_some() {
        add_animated_gas_evolution_assembly(
            &mut meshes,
            plan,
            layout,
            authored_clip_progress.unwrap_or(visual_inputs.reaction_progress),
            ordinal,
            progress,
        );
        return meshes.finish();
    }
    if stage == MacroscopicStage::Reaction && plan.metal_displacement.is_some() {
        add_animated_metal_displacement_assembly(
            &mut meshes,
            plan,
            layout,
            authored_clip_progress.unwrap_or(visual_inputs.reaction_progress),
            ordinal,
            progress,
        );
        return meshes.finish();
    }
    if stage == MacroscopicStage::Reaction && plan.solid_solid_synthesis.is_some() {
        add_animated_synthesis_combination_assembly(
            &mut meshes,
            plan,
            layout,
            authored_clip_progress.unwrap_or(visual_inputs.reaction_progress),
            ordinal,
            progress,
        );
        return meshes.finish();
    }
    if plan.objects.iter().any(|object| {
        object.role == SceneRole::Vessel
            && object.asset == AssetProfile::NeutralisationEvaporationAssembly
    }) {
        add_animated_neutralisation_assembly(
            &mut meshes,
            NeutralisationAssemblyMoment {
                plan,
                layout,
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
        return meshes.finish();
    }
    if let Some(assembly) = plan.objects.iter().find(|object| {
        object.role == SceneRole::Vessel
            && matches!(
                object.asset,
                AssetProfile::CompleteCombustionAssembly
                    | AssetProfile::IncompleteCombustionAssembly
            )
    }) {
        add_animated_combustion_assembly(
            &mut meshes,
            assembly,
            layout,
            authored_clip_progress.unwrap_or(visual_inputs.reaction_progress),
        );
        return meshes.finish();
    }
    if plan.objects.iter().any(|object| {
        object.role == SceneRole::Vessel
            && object.asset == AssetProfile::AqueousPrecipitationAssembly
    }) {
        add_animated_precipitation_assembly(
            &mut meshes,
            plan,
            layout,
            authored_clip_progress.unwrap_or(visual_inputs.reaction_progress),
            ordinal,
            progress,
        );
        return meshes.finish();
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
    meshes.finish()
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
                // Liquid colour enters at the reaction region and diffuses
                // radially and vertically instead of recolouring all at once.
                (offset.x.hypot(offset.z) * 0.28 + offset.y.abs() * 0.10 + noise * 0.08)
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
            | AssetProfile::NeutralisationEvaporationAssembly
            | AssetProfile::CompleteCombustionAssembly
            | AssetProfile::IncompleteCombustionAssembly
            | AssetProfile::AqueousPrecipitationAssembly
            | AssetProfile::MetalDisplacementAssembly
            | AssetProfile::SolidSolidSynthesisAssembly
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
    let translucent_start = meshes.translucent.vertices.len();
    let glass_start = meshes.glass.vertices.len();
    let gas_start = meshes.gas.len();
    match scene_registry::asset_geometry(asset) {
        AssetGeometry::Bench => {
            add_box(
                &mut meshes.opaque,
                position,
                Vec3::new(20.0, 0.28, 10.0) * scale,
                color,
            );
            add_disc(
                &mut meshes.translucent,
                position + Vec3::new(0.0, 0.148, 0.0),
                1.30,
                [0.01, 0.02, 0.025, 0.22],
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
            add_imported_metal(&mut meshes.opaque, position, scale, color);
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
            &mut meshes.translucent,
            translucent_start,
            asset,
            position,
            transition,
        );
        apply_gas_colour_transition(&mut meshes.gas, gas_start, position, transition);
    }
    rotate_mesh_vertices(&mut meshes.opaque, opaque_start, position, rotation);
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
        add_crystallizing_salt(
            &mut meshes.opaque,
            layout,
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
    let centre = Vec3::new(
        layout.vessel_center.x,
        layout.bench_top,
        layout.vessel_center.z,
    );
    let vessel_bottom = layout.bench_top + state.lift;
    let support_y = vessel_bottom - 0.035;
    let metal = [0.20, 0.24, 0.28, 1.0];
    let burner = [0.12, 0.17, 0.22, 1.0];
    add_cylinder(
        &mut meshes.opaque,
        centre + Vec3::Y * 0.018,
        centre + Vec3::Y * (0.105 * reveal),
        0.13 * reveal,
        burner,
    );
    add_ring(
        &mut meshes.opaque,
        Vec3::new(centre.x, support_y, centre.z),
        0.57 * reveal,
        0.022,
        metal,
    );
    for leg in 0..3_u8 {
        let angle = std::f32::consts::TAU * f32::from(leg) / 3.0 + seed_phase(seed, 151) * 0.04;
        let foot = centre + Vec3::new(angle.cos() * 0.48, 0.025, angle.sin() * 0.48);
        let top = Vec3::new(
            centre.x + angle.cos() * 0.50 * reveal,
            support_y,
            centre.z + angle.sin() * 0.50 * reveal,
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
    layout: SceneLayout,
    progress: f32,
    seed: u64,
    colour: [f32; 4],
) {
    let progress = progress.clamp(0.0, 1.0);
    let floor = Vec3::new(
        layout.vessel_center.x,
        layout.bench_top + stateful_vessel_floor_offset(layout) + 0.035,
        layout.vessel_center.z,
    );
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
        let axis = Vec3::new(
            seeded_unit(seed, index, 175) - 0.5,
            0.65 + seeded_unit(seed, index, 176) * 0.35,
            seeded_unit(seed, index, 177) - 0.5,
        )
        .normalize_or_zero();
        let rotation =
            Quat::from_axis_angle(axis, seeded_unit(seed, index, 178) * std::f32::consts::TAU);
        add_shard(
            mesh,
            point,
            Vec3::new(size * 0.86, size * 0.82, size * 0.78),
            rotation,
            crystal_colour,
            shard_seed,
        );
    }
}

fn stateful_vessel_floor_offset(layout: SceneLayout) -> f32 {
    (layout.vessel_center.y - (layout.bench_top + 0.605)).max(0.0)
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
    const RINGS: u16 = 6;
    const SEGMENTS: u16 = 24;
    let radius = 0.82 * scale.x;
    let bottom = center + Vec3::new(0.0, -0.52 * scale.y, 0.0);
    let surface = center + Vec3::new(0.0, 0.54 * scale.y, 0.0);
    let surface_color = mix_color(color, [0.86, 0.94, 0.97, 0.54], 0.46);
    let rim_color = mix_color(color, [0.92, 0.97, 0.99, 0.62], 0.68);
    add_cylinder(mesh, bottom, surface, radius, color);
    add_disc(mesh, bottom, radius, color);

    for ring in 0..RINGS {
        let inner_radius = f32::from(ring) / f32::from(RINGS);
        let outer_radius = f32::from(ring + 1) / f32::from(RINGS);
        for segment in 0..SEGMENTS {
            let angle_a = std::f32::consts::TAU * f32::from(segment) / f32::from(SEGMENTS);
            let angle_b = std::f32::consts::TAU * f32::from(segment + 1) / f32::from(SEGMENTS);
            let inner_a = liquid_surface_point(
                surface,
                radius,
                inner_radius,
                angle_a,
                turbulence,
                phase,
                seed,
            );
            let inner_b = liquid_surface_point(
                surface,
                radius,
                inner_radius,
                angle_b,
                turbulence,
                phase,
                seed,
            );
            let outer_a = liquid_surface_point(
                surface,
                radius,
                outer_radius,
                angle_a,
                turbulence,
                phase,
                seed,
            );
            let outer_b = liquid_surface_point(
                surface,
                radius,
                outer_radius,
                angle_b,
                turbulence,
                phase,
                seed,
            );
            if ring == 0 {
                add_flat_triangle(mesh, inner_a, outer_a, outer_b, surface_color);
            } else {
                add_flat_triangle(mesh, inner_a, outer_a, outer_b, surface_color);
                add_flat_triangle(mesh, inner_a, outer_b, inner_b, surface_color);
            }
        }
    }
    add_ring(
        mesh,
        surface + Vec3::new(0.0, 0.018 + turbulence * 0.006, 0.0),
        radius * 0.965,
        0.014,
        rim_color,
    );
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

    if pose.activity > 0.001 {
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
    if film > 0.001 {
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
    translucent: Mesh,
    glass: Mesh,
    emissive: Mesh,
    gas: Vec<GasSplat>,
}

impl SceneMeshes {
    fn finish(self) -> (Vec<Vertex>, Vec<u32>, u32, u32, Vec<GasSplat>) {
        let mut vertices = Vec::with_capacity(
            self.opaque.vertices.len()
                + self.translucent.vertices.len()
                + self.glass.vertices.len()
                + self.emissive.vertices.len(),
        );
        let mut indices = Vec::with_capacity(
            self.opaque.indices.len()
                + self.translucent.indices.len()
                + self.glass.indices.len()
                + self.emissive.indices.len(),
        );
        append_mesh(&mut vertices, &mut indices, self.opaque);
        let opaque_index_count = u32::try_from(indices.len()).unwrap_or(u32::MAX);
        append_mesh(&mut vertices, &mut indices, self.translucent);
        append_mesh(&mut vertices, &mut indices, self.glass);
        let transparent_index_count = u32::try_from(indices.len()).unwrap_or(u32::MAX);
        append_mesh(&mut vertices, &mut indices, self.emissive);
        (
            vertices,
            indices,
            opaque_index_count,
            transparent_index_count,
            self.gas,
        )
    }
}

fn append_mesh(vertices: &mut Vec<Vertex>, indices: &mut Vec<u32>, mesh: Mesh) {
    let vertex_offset = u32::try_from(vertices.len()).unwrap_or(u32::MAX);
    vertices.extend(mesh.vertices);
    indices.extend(
        mesh.indices
            .into_iter()
            .map(|index| index.saturating_add(vertex_offset)),
    );
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

fn add_animated_alkali_water_assembly(
    meshes: &mut SceneMeshes,
    plan: &ScenePlan,
    layout: SceneLayout,
    progress: f32,
) {
    let clip = alkali_water_clip();
    debug_assert_eq!(clip.frames_per_second, 30);
    let frame = clip.frame_at_progress(progress);
    let style = animated_alkali_water_style(plan);
    let seed = plan_seed(plan);
    for (track_index, track) in clip.tracks.iter().enumerate() {
        if !animated_track_enabled(track, track_index, style, seed) {
            continue;
        }
        let colour = animated_track_colour(track.colour, style);
        let destination = match (track.pass, track.colour) {
            (_, ClipColour::Glass) => &mut meshes.glass,
            (ClipPass::Opaque, _) => &mut meshes.opaque,
            (ClipPass::Translucent, _) => &mut meshes.translucent,
            (ClipPass::Emissive, _) => &mut meshes.emissive,
        };
        append_animated_track(
            destination,
            clip,
            track,
            frame,
            layout.bench_top,
            style.activity,
            colour,
        );
    }
}

#[derive(Debug, Clone, Copy)]
struct NeutralisationAssemblyMoment<'a> {
    plan: &'a ScenePlan,
    layout: SceneLayout,
    progress: f32,
    post_process: PostProcessVisualState,
    stage_progress: f32,
    seed: u64,
    visual_inputs: ReactionVisualInputs,
    effect_colours: EffectColours,
    ordinal: u16,
    ordinal_progress: f32,
}

fn add_animated_neutralisation_assembly(
    meshes: &mut SceneMeshes,
    moment: NeutralisationAssemblyMoment<'_>,
) {
    let NeutralisationAssemblyMoment {
        plan,
        layout,
        progress,
        post_process,
        stage_progress,
        seed,
        effect_colours,
        ..
    } = moment;
    let clip = neutralisation_clip();
    debug_assert_eq!(clip.frames_per_second, 30);
    let frame = clip.frame_at_progress(progress);
    let vessel_motion = neutralisation_vessel_motion(clip, frame);
    let colours = neutralisation_colours(plan, effect_colours, frame);
    append_shared_beaker(
        &mut meshes.glass,
        alkali_water_clip(),
        layout.bench_top,
        vessel_motion,
    );
    for track in &clip.tracks {
        if track.module == ClipModule::VesselAnchor {
            continue;
        }
        let colour = neutralisation_track_colour(track.colour, colours);
        let destination = match (track.pass, track.colour) {
            (_, ClipColour::Glass) => &mut meshes.glass,
            (ClipPass::Opaque, _) => &mut meshes.opaque,
            (ClipPass::Translucent, _) => &mut meshes.translucent,
            (ClipPass::Emissive, _) => &mut meshes.emissive,
        };
        append_animated_track(
            destination,
            clip,
            track,
            frame,
            layout.bench_top,
            1.0,
            colour,
        );
    }
    add_neutralisation_supplemental_reactants(meshes, moment, vessel_motion);
    add_neutralisation_reaction_gas(meshes, moment, vessel_motion);
    if post_process.vapour > 0.002 {
        let centre = Vec3::new(
            layout.vessel_center.x,
            layout.liquid_surface + vessel_motion.y * 0.45 + 0.30,
            layout.vessel_center.z,
        );
        add_gas_density_field(
            &mut meshes.gas,
            centre,
            Vec3::new(0.46, 0.74, 0.46),
            [0.88, 0.92, 0.93, 0.34 * post_process.vapour],
            seed.rotate_left(23),
            stage_progress * 4.2,
            post_process.vapour,
            GasFlowControls::escaping(
                post_process.vapour,
                0.48 + post_process.boiling * 0.34,
                0.92,
                seed.rotate_left(23),
            ),
        );
    }
}

fn add_animated_combustion_assembly(
    meshes: &mut SceneMeshes,
    assembly: &PresentationObject,
    layout: SceneLayout,
    progress: f32,
) {
    let incomplete = assembly.asset == AssetProfile::IncompleteCombustionAssembly;
    let clip = if incomplete {
        incomplete_combustion_clip()
    } else {
        complete_combustion_clip()
    };
    debug_assert_eq!(clip.frames_per_second, 30);
    let frame = clip.frame_at_progress(progress);
    append_shared_beaker(
        &mut meshes.glass,
        alkali_water_clip(),
        layout.bench_top,
        Vec3::ZERO,
    );
    let mut fuel = appearance_color(assembly.appearance);
    fuel[3] = 0.32;
    for track in &clip.tracks {
        let colour = combustion_track_colour(track.colour, fuel, incomplete);
        let destination = match (track.pass, track.colour) {
            (_, ClipColour::Glass) => &mut meshes.glass,
            (ClipPass::Opaque, _) => &mut meshes.opaque,
            (ClipPass::Translucent, _) => &mut meshes.translucent,
            (ClipPass::Emissive, _) => &mut meshes.emissive,
        };
        append_animated_track(
            destination,
            clip,
            track,
            frame,
            layout.bench_top,
            1.0,
            colour,
        );
    }
}

#[allow(clippy::cast_precision_loss)]
fn precipitation_clip_progress(plan: &ScenePlan, moment: RealWorldPosition) -> f32 {
    const DURATION_MS: f32 = 6_000.0;
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

fn add_animated_precipitation_assembly(
    meshes: &mut SceneMeshes,
    plan: &ScenePlan,
    layout: SceneLayout,
    progress: f32,
    ordinal: u16,
    ordinal_progress: f32,
) {
    let precipitation = plan
        .precipitation
        .as_ref()
        .expect("validated precipitation assembly has material bindings");
    let clip = precipitation_clip();
    debug_assert_eq!(clip.frame_count, 180);
    debug_assert_eq!(clip.frames_per_second, 30);
    let frame = clip.frame_at_progress(progress);
    append_shared_beaker(
        &mut meshes.glass,
        alkali_water_clip(),
        layout.bench_top,
        Vec3::ZERO,
    );
    for track in &clip.tracks {
        let colour =
            precipitation_track_colour(track.colour, precipitation, ordinal, ordinal_progress);
        let destination = match (track.pass, track.colour) {
            (_, ClipColour::Glass) => &mut meshes.glass,
            (ClipPass::Opaque, _) => &mut meshes.opaque,
            (ClipPass::Translucent, _) => &mut meshes.translucent,
            (ClipPass::Emissive, _) => &mut meshes.emissive,
        };
        append_animated_track(
            destination,
            clip,
            track,
            frame,
            layout.bench_top,
            1.0,
            colour,
        );
    }
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

fn add_animated_gas_evolution_assembly(
    meshes: &mut SceneMeshes,
    plan: &ScenePlan,
    layout: SceneLayout,
    progress: f32,
    ordinal: u16,
    ordinal_progress: f32,
) {
    let gas_evolution = plan
        .gas_evolution
        .as_ref()
        .expect("validated gas-evolution assembly has material bindings");
    let clip = gas_evolution_clip(gas_evolution.variant);
    debug_assert_eq!(clip.frame_count, 180);
    debug_assert_eq!(clip.frames_per_second, 30);
    let frame = clip.frame_at_progress(progress);
    append_shared_beaker(
        &mut meshes.glass,
        alkali_water_clip(),
        layout.bench_top,
        Vec3::ZERO,
    );
    for track in &clip.tracks {
        let colour =
            gas_evolution_track_colour(track.colour, gas_evolution, ordinal, ordinal_progress);
        let destination = match (track.pass, track.colour) {
            (_, ClipColour::Glass) => &mut meshes.glass,
            (ClipPass::Opaque, _) => &mut meshes.opaque,
            (ClipPass::Translucent, _) => &mut meshes.translucent,
            (ClipPass::Emissive, _) => &mut meshes.emissive,
        };
        append_animated_track(
            destination,
            clip,
            track,
            frame,
            layout.bench_top,
            1.0,
            colour,
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

fn add_animated_metal_displacement_assembly(
    meshes: &mut SceneMeshes,
    plan: &ScenePlan,
    layout: SceneLayout,
    progress: f32,
    ordinal: u16,
    ordinal_progress: f32,
) {
    let displacement = plan
        .metal_displacement
        .as_ref()
        .expect("validated metal-displacement assembly has material bindings");
    let clip = metal_displacement_clip();
    debug_assert_eq!(clip.frame_count, 180);
    debug_assert_eq!(clip.frames_per_second, 30);
    let frame = clip.frame_at_progress(progress);
    append_shared_beaker(
        &mut meshes.glass,
        alkali_water_clip(),
        layout.bench_top,
        Vec3::ZERO,
    );
    for track in &clip.tracks {
        if !metal_displacement_track_visible(track.module, frame) {
            continue;
        }
        let colour =
            metal_displacement_track_colour(track.colour, displacement, ordinal, ordinal_progress);
        let destination = match (track.pass, track.colour) {
            (_, ClipColour::Glass) => &mut meshes.glass,
            (ClipPass::Opaque, _) => &mut meshes.opaque,
            (ClipPass::Translucent, _) => &mut meshes.translucent,
            (ClipPass::Emissive, _) => &mut meshes.emissive,
        };
        let deposited = matches!(
            track.module,
            ClipModule::MetalDeposit | ClipModule::MetalFlakes
        );
        if deposited {
            append_animated_track_adjusted(
                destination,
                clip,
                track,
                frame,
                layout.bench_top,
                1.0,
                colour,
                1.16,
                0.012,
            );
            append_animated_track_adjusted(
                &mut meshes.emissive,
                clip,
                track,
                frame,
                layout.bench_top,
                1.0,
                deposit_highlight_colour(colour),
                1.22,
                0.026,
            );
        } else {
            append_animated_track(
                destination,
                clip,
                track,
                frame,
                layout.bench_top,
                1.0,
                colour,
            );
        }
    }
}

fn metal_displacement_track_visible(module: ClipModule, frame: f32) -> bool {
    const DEPOSIT_START_FRAME: f32 = 53.0;
    const FLAKE_START_FRAME: f32 = 103.0;
    match module {
        ClipModule::MetalDeposit => frame >= DEPOSIT_START_FRAME,
        ClipModule::MetalFlakes => frame >= FLAKE_START_FRAME,
        _ => true,
    }
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

fn add_animated_synthesis_combination_assembly(
    meshes: &mut SceneMeshes,
    plan: &ScenePlan,
    layout: SceneLayout,
    progress: f32,
    ordinal: u16,
    ordinal_progress: f32,
) {
    let synthesis = plan
        .solid_solid_synthesis
        .as_ref()
        .expect("validated solid-solid synthesis assembly has material bindings");
    let clip = synthesis_combination_clip();
    debug_assert_eq!(clip.frame_count, 180);
    debug_assert_eq!(clip.frames_per_second, 30);
    let frame = clip.frame_at_progress(progress);
    for track in &clip.tracks {
        if track.module == ClipModule::SynthesisReactionFront && !synthesis.show_reaction_front {
            continue;
        }
        let colour =
            synthesis_combination_track_colour(track.colour, synthesis, ordinal, ordinal_progress);
        let destination = match track.pass {
            ClipPass::Opaque => &mut meshes.opaque,
            ClipPass::Translucent => &mut meshes.translucent,
            ClipPass::Emissive => &mut meshes.emissive,
        };
        append_animated_track(
            destination,
            clip,
            track,
            frame,
            layout.bench_top,
            1.0,
            colour,
        );
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
    [
        colour[0] + (1.0 - colour[0]) * 0.28,
        colour[1] + (1.0 - colour[1]) * 0.28,
        colour[2] + (1.0 - colour[2]) * 0.28,
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

fn neutralisation_vessel_motion(clip: &AnimatedClip, frame: f32) -> Vec3 {
    let anchor = clip
        .tracks
        .iter()
        .find(|track| track.module == ClipModule::VesselAnchor)
        .expect("validated neutralisation clip has a vessel anchor");
    clip.sample(anchor, 0, frame).position - clip.sample(anchor, 0, 0.0).position
}

fn append_shared_beaker(
    mesh: &mut Mesh,
    shared_clip: &AnimatedClip,
    bench_top: f32,
    vessel_motion: Vec3,
) {
    const MODEL_SCALE: f32 = 0.45;
    for track in shared_clip
        .tracks
        .iter()
        .filter(|track| track.module == ClipModule::Beaker)
    {
        let vertex_offset = u32::try_from(mesh.vertices.len()).unwrap_or(u32::MAX);
        mesh.vertices.reserve(track.vertex_count);
        mesh.indices.reserve(track.indices.len());
        for vertex_index in 0..track.vertex_count {
            let vertex = shared_clip.sample(track, vertex_index, 0.0);
            mesh.vertices.push(Vertex {
                position: ((vertex.position + vessel_motion) * MODEL_SCALE + Vec3::Y * bench_top)
                    .to_array(),
                normal: vertex.normal.to_array(),
                color: [0.62, 0.84, 0.94, 0.22],
            });
        }
        mesh.indices.extend(
            track
                .indices
                .iter()
                .map(|index| index.saturating_add(vertex_offset)),
        );
    }
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

fn animated_track_enabled(
    track: &ClipTrack,
    track_index: usize,
    style: AnimatedAlkaliWaterStyle,
    seed: u64,
) -> bool {
    if track.module == ClipModule::Flame {
        return style.flame.is_some();
    }
    let retention = match track.module {
        ClipModule::Bubbles => style.activity,
        ClipModule::Splashes => ((style.activity - 0.18) / 0.82).clamp(0.18, 1.0),
        ClipModule::Vapour => (style.activity * 0.86).clamp(0.28, 1.0),
        ClipModule::Water if track.colour == ClipColour::WaterHighlight => {
            (0.34 + style.activity * 0.66).clamp(0.0, 1.0)
        }
        ClipModule::Beaker | ClipModule::Water | ClipModule::Metal => 1.0,
        ClipModule::Mixing
        | ClipModule::Salt
        | ClipModule::Stirrer
        | ClipModule::VesselAnchor
        | ClipModule::Sparks
        | ClipModule::Plume
        | ClipModule::Soot
        | ClipModule::PrecipitateCloud
        | ClipModule::FallingPrecipitate
        | ClipModule::PouringVessel
        | ClipModule::Sediment
        | ClipModule::SurfaceBursts
        | ClipModule::SolidReactant
        | ClipModule::InitialSolution
        | ClipModule::FinalSolution
        | ClipModule::OriginalMetal
        | ClipModule::MetalErosion
        | ClipModule::MetalDeposit
        | ClipModule::MetalFlakes
        | ClipModule::SynthesisReactantA
        | ClipModule::SynthesisReactantB
        | ClipModule::SynthesisProduct
        | ClipModule::SynthesisReactionFront
        | ClipModule::SynthesisVessel
        | ClipModule::SynthesisMixingTool => 0.0,
        ClipModule::Flame => unreachable!("flame handled above"),
    };
    let index = u32::try_from(track_index).unwrap_or(u32::MAX);
    seeded_unit(seed, index, 211) <= retention
}

fn animated_track_colour(colour: ClipColour, style: AnimatedAlkaliWaterStyle) -> [f32; 4] {
    let flame = flame_colours(style.flame.unwrap_or(FlamePalette::Natural));
    match colour {
        ClipColour::Glass => [0.62, 0.84, 0.94, 0.22],
        ClipColour::Water => [0.34, 0.64, 0.80, 0.34],
        ClipColour::WaterHighlight => [0.72, 0.90, 0.98, 0.46],
        ClipColour::ReactiveMetal => [0.88, 0.90, 0.92, 1.0],
        ClipColour::FlameOuter => flame.body_high,
        ClipColour::FlameInner => flame.body_low,
        ClipColour::FlameCore => flame.core,
        ClipColour::FizzBubble | ClipColour::GasBubble => [0.80, 0.94, 1.0, 0.30 * style.activity],
        ClipColour::Vapour => [0.84, 0.88, 0.92, 0.13 + style.activity * 0.06],
        ClipColour::MixtureA
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
        | ClipColour::MixingTool => [0.82, 0.86, 0.88, 0.20],
        ClipColour::Fuel => [0.88, 0.82, 0.54, 0.30],
        ClipColour::IgnitionSpark => [1.0, 0.72, 0.12, 0.94],
        ClipColour::ProductPlume | ClipColour::GasCloud => [0.86, 0.90, 0.92, 0.16],
        ClipColour::CombustionSmoke => [0.10, 0.105, 0.11, 0.46],
        ClipColour::Soot => [0.055, 0.050, 0.047, 0.96],
        ClipColour::SootDeposit => [0.075, 0.068, 0.062, 0.48],
    }
}

fn append_animated_track(
    mesh: &mut Mesh,
    clip: &AnimatedClip,
    track: &ClipTrack,
    frame: f32,
    bench_top: f32,
    activity: f32,
    colour: [f32; 4],
) {
    append_animated_track_adjusted(
        mesh, clip, track, frame, bench_top, activity, colour, 1.0, 0.0,
    );
}

#[allow(clippy::too_many_arguments, clippy::cast_precision_loss)]
fn append_animated_track_adjusted(
    mesh: &mut Mesh,
    clip: &AnimatedClip,
    track: &ClipTrack,
    frame: f32,
    bench_top: f32,
    activity: f32,
    colour: [f32; 4],
    local_scale: f32,
    normal_offset: f32,
) {
    const MODEL_SCALE: f32 = 0.45;
    let centre = if (local_scale - 1.0).abs() <= f32::EPSILON {
        Vec3::ZERO
    } else {
        (0..track.vertex_count).fold(Vec3::ZERO, |sum, vertex_index| {
            sum + clip.sample(track, vertex_index, frame).position
        }) / track.vertex_count.max(1) as f32
    };
    let vertex_offset = u32::try_from(mesh.vertices.len()).unwrap_or(u32::MAX);
    mesh.vertices.reserve(track.vertex_count);
    mesh.indices.reserve(track.indices.len());
    for vertex_index in 0..track.vertex_count {
        let current = clip.sample(track, vertex_index, frame);
        let initial = if matches!(track.module, ClipModule::Water | ClipModule::Metal) {
            clip.sample(track, vertex_index, 0.0)
        } else {
            current
        };
        let ClipVertex {
            mut position,
            normal,
        } = current;
        match track.module {
            ClipModule::Water => {
                position = initial.position.lerp(position, 0.28 + activity * 0.72);
            }
            ClipModule::Metal => {
                position.x = initial.position.x + (position.x - initial.position.x) * activity;
                position.z = initial.position.z + (position.z - initial.position.z) * activity;
            }
            ClipModule::Beaker
            | ClipModule::Flame
            | ClipModule::Bubbles
            | ClipModule::Splashes
            | ClipModule::Vapour
            | ClipModule::Mixing
            | ClipModule::Salt
            | ClipModule::Stirrer
            | ClipModule::Sparks
            | ClipModule::Plume
            | ClipModule::Soot
            | ClipModule::PrecipitateCloud
            | ClipModule::FallingPrecipitate
            | ClipModule::PouringVessel
            | ClipModule::Sediment
            | ClipModule::SurfaceBursts
            | ClipModule::SolidReactant
            | ClipModule::InitialSolution
            | ClipModule::FinalSolution
            | ClipModule::OriginalMetal
            | ClipModule::MetalErosion
            | ClipModule::MetalDeposit
            | ClipModule::MetalFlakes
            | ClipModule::SynthesisReactantA
            | ClipModule::SynthesisReactantB
            | ClipModule::SynthesisProduct
            | ClipModule::SynthesisReactionFront
            | ClipModule::SynthesisVessel
            | ClipModule::SynthesisMixingTool => {}
            ClipModule::VesselAnchor => {
                unreachable!("anchor tracks are not renderable geometry");
            }
        }
        if (local_scale - 1.0).abs() > f32::EPSILON {
            position = centre + (position - centre) * local_scale;
        }
        position += normal * normal_offset;
        mesh.vertices.push(Vertex {
            position: (position * MODEL_SCALE + Vec3::Y * bench_top).to_array(),
            normal: normal.to_array(),
            color: colour,
        });
    }
    mesh.indices.extend(
        track
            .indices
            .iter()
            .map(|index| index.saturating_add(vertex_offset)),
    );
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
    const SIDES: u16 = 32;
    let base = u32::try_from(mesh.vertices.len()).unwrap_or(u32::MAX);
    for level in [bottom, top] {
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
    for side in 0..SIDES {
        let lower = base + u32::from(side);
        let upper = base + u32::from(SIDES) + 1 + u32::from(side);
        mesh.indices
            .extend_from_slice(&[lower, upper, lower + 1, lower + 1, upper, upper + 1]);
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
        mesh.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
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
        assert_eq!(std::mem::size_of::<FixedCameraState>(), 0);
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
            bytemuck::cast_slice::<Vertex, u8>(&formed.0),
            bytemuck::cast_slice::<Vertex, u8>(&repeated.0)
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
                let (vertices, _, _, _, gas_splats) = meshes.finish();
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
                assert!(
                    scene.4.is_empty(),
                    "an authored gas assembly must not be overlaid with procedural gas"
                );
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
            bytemuck::cast_slice::<Vertex, u8>(&first.0),
            bytemuck::cast_slice::<Vertex, u8>(&second.0)
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
            bytemuck::cast_slice::<Vertex, u8>(&reacting.0),
            bytemuck::cast_slice::<Vertex, u8>(&before.0),
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
        let clip = alkali_water_clip();
        let frames = (0..=4_u64)
            .map(|quarter| {
                let elapsed = duration.saturating_mul(quarter) / 4;
                let moment = plan
                    .timeline
                    .locate(elapsed)
                    .expect("quarter-time sample exists");
                clip.frame_at_progress(plan.timeline.normalized_progress_at(moment))
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

            let has_expected_colour = |vertex: &Vertex| {
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
    fn phase_unknown_halogen_displacement_gets_progress_motion_without_inventing_phase() {
        let request = chemistry::ReactionRequest::ALL
            .iter()
            .copied()
            .find(|request| request.family() == chemistry::ReactionFamily::HalogenDisplacement)
            .expect("a supported halogen displacement exists");
        let plan = plan_for(request);
        assert!(
            !plan
                .objects
                .iter()
                .any(|object| object.asset == AssetProfile::LiquidVolume)
        );
        assert!(plan.effects.iter().any(|effect| {
            effect.effect == EffectProfile::ReactionActivity
                && effect.trigger == chem_catalogue::ObservationPredicate::Forms
        }));
        assert!(plan.effects.iter().all(|effect| {
            !matches!(
                effect.effect,
                EffectProfile::GasRelease
                    | EffectProfile::PrecipitateFormation
                    | EffectProfile::LiquidMixing
            )
        }));
        assert!(!plan.objects.iter().any(|object| matches!(
            object.asset,
            AssetProfile::GasCloud | AssetProfile::PrecipitateCloud
        )));
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
            bytemuck::cast_slice::<Vertex, u8>(&active.0),
            bytemuck::cast_slice::<Vertex, u8>(&before.0),
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

        let boiling = build_scene_at(&plan, moment(MacroscopicStage::SolventBoiling, 0.58));
        assert!(
            !boiling.4.is_empty(),
            "boiling solvent must emit an advected vapour volume"
        );
        assert!(
            usize::try_from(boiling.3).is_ok_and(|start| start < boiling.1.len()),
            "the burner must contribute a separate emissive flame pass"
        );

        let crystals = build_scene_at(&plan, moment(MacroscopicStage::CrystalGrowth, 1.0));
        let repeated = build_scene_at(&plan, moment(MacroscopicStage::CrystalGrowth, 1.0));
        let clip = neutralisation_clip();
        let salt = clip
            .tracks
            .iter()
            .find(|track| track.module == ClipModule::Salt)
            .expect("authored clip contains salt residue");
        let early_size = clip
            .sample(salt, 0, 170.0)
            .position
            .distance(clip.sample(salt, 1, 170.0).position);
        let final_size = clip
            .sample(salt, 0, 231.0)
            .position
            .distance(clip.sample(salt, 1, 231.0).position);
        assert!(
            final_size > early_size * 4.0,
            "faceted salt residue must grow from the authored nucleation scale"
        );
        assert_ne!(
            bytemuck::cast_slice::<Vertex, u8>(&crystals.0),
            bytemuck::cast_slice::<Vertex, u8>(&boiling.0)
        );
        assert_eq!(
            bytemuck::cast_slice::<Vertex, u8>(&crystals.0),
            bytemuck::cast_slice::<Vertex, u8>(&repeated.0)
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

        let clip = neutralisation_clip();
        let lifted_motion = neutralisation_vessel_motion(clip, 139.0);
        assert!(lifted_motion.y > 0.5);
        let mut shared_beaker = Mesh::default();
        append_shared_beaker(
            &mut shared_beaker,
            alkali_water_clip(),
            -0.76,
            lifted_motion,
        );
        let expected_vertices = alkali_water_clip()
            .tracks
            .iter()
            .filter(|track| track.module == ClipModule::Beaker)
            .map(|track| track.vertex_count)
            .sum::<usize>();
        assert_eq!(shared_beaker.vertices.len(), expected_vertices);

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
        let potassium_flame = animated_track_colour(
            ClipColour::FlameOuter,
            AnimatedAlkaliWaterStyle {
                activity: 1.0,
                flame: Some(FlamePalette::Lilac),
            },
        );
        let expected_lilac = flame_colours(FlamePalette::Lilac).body_high;
        assert!(
            potassium_flame
                .iter()
                .zip(expected_lilac)
                .all(|(actual, expected)| (actual - expected).abs() <= f32::EPSILON),
            "neutralisation styling must not recolour potassium ignition"
        );
    }

    #[test]
    fn precipitation_assembly_uses_absolute_six_second_playback_and_persistent_sediment() {
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
        assert_eq!(plan.timeline.duration_ms() - start_ms, 6_000);

        let midpoint = plan
            .timeline
            .locate(start_ms + 3_000)
            .expect("midpoint is on the timeline");
        let first_sample = build_scene_at(&plan, midpoint);
        let _later_sample = build_scene_at(
            &plan,
            plan.timeline
                .locate(start_ms + 5_000)
                .expect("later sample is on the timeline"),
        );
        let repeated_midpoint = build_scene_at(&plan, midpoint);
        assert_eq!(
            bytemuck::cast_slice::<Vertex, u8>(&first_sample.0),
            bytemuck::cast_slice::<Vertex, u8>(&repeated_midpoint.0)
        );
        assert_eq!(first_sample.1, repeated_midpoint.1);

        let clip = precipitation_clip();
        let surface_area = |module| {
            clip.tracks
                .iter()
                .filter(|track| track.module == module)
                .flat_map(|track| {
                    track.indices.chunks_exact(3).map(move |triangle| {
                        let a = clip
                            .sample(
                                track,
                                usize::try_from(triangle[0]).expect("vertex index"),
                                179.0,
                            )
                            .position;
                        let b = clip
                            .sample(
                                track,
                                usize::try_from(triangle[1]).expect("vertex index"),
                                179.0,
                            )
                            .position;
                        let c = clip
                            .sample(
                                track,
                                usize::try_from(triangle[2]).expect("vertex index"),
                                179.0,
                            )
                            .position;
                        (b - a).cross(c - a).length() * 0.5
                    })
                })
                .sum::<f32>()
        };
        let cloud_area = surface_area(ClipModule::PrecipitateCloud);
        let fragments_area = surface_area(ClipModule::FallingPrecipitate);
        let sediment_area = surface_area(ClipModule::Sediment);
        assert!(cloud_area < 0.001);
        assert!(fragments_area < 0.001);
        assert!(
            sediment_area > 1.0,
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
    fn synthesis_combination_clip_is_complete_deterministic_and_colour_bound() {
        let clip = synthesis_combination_clip();
        assert_eq!(clip.frame_count, 180);
        assert_eq!(clip.frames_per_second, 30);
        assert_eq!(clip.tracks.len(), 29);
        for module in [
            ClipModule::SynthesisReactantA,
            ClipModule::SynthesisReactantB,
            ClipModule::SynthesisProduct,
            ClipModule::SynthesisReactionFront,
            ClipModule::SynthesisVessel,
            ClipModule::SynthesisMixingTool,
        ] {
            assert!(clip.tracks.iter().any(|track| track.module == module));
        }
        let track = clip.tracks.first().expect("clip has tracks");
        let first = clip.sample(track, 0, 91.375);
        let repeated = clip.sample(track, 0, 91.375);
        assert_eq!(first.position, repeated.position);
        assert_eq!(first.normal, repeated.normal);

        let bound =
            |binding: &str, [red, green, blue]: [u8; 3]| chem_presentation::BoundVisualColour {
                binding: binding.to_owned(),
                base_colour: VisualColour { red, green, blue },
                colour: VisualColour { red, green, blue },
                transition_ordinal: None,
            };
        let visual = chem_presentation::SolidSolidSynthesisVisualProfile {
            formation_ordinal: 3,
            reactant_a: bound("a", [0x80, 0x84, 0x88]),
            reactant_b: bound("b", [0xe4, 0xc1, 0x32]),
            product: bound("product", [0x35, 0x38, 0x3b]),
            show_reaction_front: true,
        };
        assert_eq!(
            synthesis_combination_track_colour(ClipColour::ReactantA, &visual, u16::MAX, 1.0)[..3],
            [
                f32::from(0x80_u8) / 255.0,
                f32::from(0x84_u8) / 255.0,
                f32::from(0x88_u8) / 255.0
            ]
        );
        assert_eq!(
            synthesis_combination_track_colour(
                ClipColour::SynthesisProduct,
                &visual,
                u16::MAX,
                1.0,
            )[..3],
            [
                f32::from(0x35_u8) / 255.0,
                f32::from(0x38_u8) / 255.0,
                f32::from(0x3b_u8) / 255.0
            ]
        );
    }

    #[test]
    fn deposit_readability_layer_expands_the_authored_silhouette_deterministically() {
        let clip = metal_displacement_clip();
        let track = clip
            .tracks
            .iter()
            .find(|track| track.module == ClipModule::MetalDeposit)
            .expect("deposit track exists");
        let frame = clip.frame_at_progress(1.0);
        let mut authored = Mesh::default();
        append_animated_track(
            &mut authored,
            clip,
            track,
            frame,
            0.0,
            1.0,
            [0.72, 0.42, 0.28, 1.0],
        );
        let mut emphasized = Mesh::default();
        append_animated_track_adjusted(
            &mut emphasized,
            clip,
            track,
            frame,
            0.0,
            1.0,
            [0.72, 0.42, 0.28, 1.0],
            1.16,
            0.012,
        );
        let extent = |mesh: &Mesh| {
            let (minimum, maximum) = mesh.vertices.iter().fold(
                (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
                |(minimum, maximum), vertex| {
                    let position = Vec3::from_array(vertex.position);
                    (minimum.min(position), maximum.max(position))
                },
            );
            maximum - minimum
        };
        let authored_extent = extent(&authored);
        let emphasized_extent = extent(&emphasized);
        assert!(emphasized_extent.length() > authored_extent.length() * 1.10);

        let mut replay = Mesh::default();
        append_animated_track_adjusted(
            &mut replay,
            clip,
            track,
            frame,
            0.0,
            1.0,
            [0.72, 0.42, 0.28, 1.0],
            1.16,
            0.012,
        );
        assert_eq!(
            bytemuck::cast_slice::<Vertex, u8>(&emphasized.vertices),
            bytemuck::cast_slice::<Vertex, u8>(&replay.vertices)
        );
        assert_eq!(emphasized.indices, replay.indices);
    }

    #[test]
    fn deposit_and_flake_tracks_stay_hidden_until_their_authored_start_frames() {
        assert!(!metal_displacement_track_visible(
            ClipModule::MetalDeposit,
            52.999
        ));
        assert!(metal_displacement_track_visible(
            ClipModule::MetalDeposit,
            53.0
        ));
        assert!(!metal_displacement_track_visible(
            ClipModule::MetalFlakes,
            102.999
        ));
        assert!(metal_displacement_track_visible(
            ClipModule::MetalFlakes,
            103.0
        ));
        assert!(metal_displacement_track_visible(
            ClipModule::OriginalMetal,
            0.0
        ));
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
        let _ = build_scene_at(&old, old_end);

        let new_start = new.timeline.locate(0).expect("new timeline starts");
        let after_switch = build_scene_at(&new, new_start);
        let fresh = build_scene_at(&new, new_start);
        assert_eq!(after_switch.1, fresh.1);
        assert_eq!(after_switch.2, fresh.2);
        assert_eq!(after_switch.3, fresh.3);
        assert_eq!(
            bytemuck::cast_slice::<Vertex, u8>(&after_switch.0),
            bytemuck::cast_slice::<Vertex, u8>(&fresh.0),
            "the new reaction must not retain prior transforms, material colours, or playhead"
        );
        assert_eq!(after_switch.4.len(), fresh.4.len());
    }

    #[test]
    fn gas_evolution_renderer_contains_no_reaction_or_species_identity_branch() {
        let source = include_str!("structural_3d.rs");
        let start = source
            .find("fn add_animated_gas_evolution_assembly")
            .expect("gas-evolution renderer exists");
        let end = source[start..]
            .find("fn precipitation_track_colour")
            .map(|offset| start + offset)
            .expect("renderer function boundary exists");
        let renderer = &source[start..end];
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
        let source = include_str!("structural_3d.rs");
        let start = source
            .find("fn add_animated_precipitation_assembly")
            .expect("precipitation renderer exists");
        let end = source[start..]
            .find("fn combustion_track_colour")
            .map(|offset| start + offset)
            .expect("renderer function boundary exists");
        let renderer = &source[start..end];
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
        let source = include_str!("structural_3d.rs");
        let start = source
            .find("fn add_animated_metal_displacement_assembly")
            .expect("metal-displacement renderer exists");
        let end = source[start..]
            .find("fn gas_evolution_track_colour")
            .map(|offset| start + offset)
            .expect("renderer function boundary exists");
        let renderer = &source[start..end];
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
        let source = include_str!("structural_3d.rs");
        let start = source
            .find("fn add_animated_synthesis_combination_assembly")
            .expect("synthesis renderer exists");
        let end = source[start..]
            .find("fn deposit_highlight_colour")
            .map(|offset| start + offset)
            .expect("renderer function boundary exists");
        let renderer = &source[start..end];
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
        let (unrotated, _, _, _, _) = unrotated_meshes.finish();
        let (rotated, _, _, _, _) = rotated_meshes.finish();

        assert_eq!(unrotated.len(), rotated.len());
        assert_ne!(
            bytemuck::cast_slice::<Vertex, u8>(&unrotated),
            bytemuck::cast_slice::<Vertex, u8>(&rotated),
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
        let (vertices, _, opaque_indices, _, _) = meshes.finish();

        assert!(opaque_indices > 0, "the floor remains opaque geometry");
        assert!(
            vertices.iter().all(|vertex| vertex.position[1] < 0.0),
            "the environment must not add a vertical wall above the floor"
        );
    }
}
