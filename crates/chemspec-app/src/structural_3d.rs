//! Depth-tested low-poly rendering of reviewed macroscopic scene plans and
//! exact catalogue-projected molecular identities.
//!
//! The renderer consumes already-reviewed atoms and bonds for presentation;
//! it never infers structure, parses source, or selects reaction rules.

use bytemuck::{Pod, Zeroable};
use chem_presentation::{
    AppearanceProfile, AssetProfile, EffectIntensity, EffectProfile, FlamePalette,
    PresentationColourTransition, PresentationEffect, PresentationObject, PresentationTransform,
    ReactionVisualInputs, SceneRole, VisualColour,
};
use chem_presentation::{RealWorldPosition, ScenePlan};
use glam::{EulerRot, Mat4, Quat, Vec3};
use iced::widget::shader::{self, Program};
use iced::{Rectangle, wgpu};

use crate::composition_catalogue::TrustedCompositionPreview;
use crate::scene_registry::{self, AssetGeometry, EffectDynamics, EffectGeometry};

const MAX_VERTICES: u64 = 65_536;
const MAX_INDICES: u64 = 196_608;
const MSAA_SAMPLES: u32 = 4;

/// Logical margin between the molecular inset and the widget edges.
pub const INSET_MARGIN: f32 = 14.0;

/// Logical side length of the square molecular inset for a widget size.
/// `main.rs` uses the same function to place the caption above the inset.
pub fn molecular_inset_side(width: f32, height: f32) -> f32 {
    (width.min(height) * 0.26).clamp(120.0, 230.0)
}

#[derive(Debug, Clone)]
pub struct Scene {
    plan: ScenePlan,
    moment: RealWorldPosition,
    reactant_previews: Vec<TrustedCompositionPreview>,
    product_preview: Option<TrustedCompositionPreview>,
}

impl Scene {
    pub fn new(
        plan: &ScenePlan,
        moment: RealWorldPosition,
        reactant_previews: &[TrustedCompositionPreview],
        product_preview: Option<&TrustedCompositionPreview>,
    ) -> Self {
        Self {
            plan: plan.clone(),
            moment,
            reactant_previews: reactant_previews.to_vec(),
            product_preview: product_preview.cloned(),
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
        let batches = build_scene(
            &self.plan,
            self.moment.ordinal,
            self.moment.ordinal_progress,
            &self.reactant_previews,
            self.product_preview.as_ref(),
        );
        let camera = fixed_camera_pose(&self.plan);
        let focus_target = SceneLayout::resolve(&self.plan).camera_target;
        ScenePrimitive {
            batches,
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
    batches: SceneBatches,
    yaw: f32,
    pitch: f32,
    view_height: f32,
    focus_target: Vec3,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct PanelUniform {
    top: [f32; 4],
    bottom: [f32; 4],
    border: [f32; 4],
    /// x: border width in uv units, y: vignette strength.
    params: [f32; 4],
}

#[derive(Debug, Clone, Copy, Default)]
#[allow(clippy::struct_field_names)]
struct SceneRanges {
    opaque_end: u32,
    translucent_end: u32,
    glass_end: u32,
    emissive_end: u32,
    inset_end: u32,
}

#[derive(Debug)]
pub struct ScenePipeline {
    opaque_pipeline: wgpu::RenderPipeline,
    translucent_pipeline: wgpu::RenderPipeline,
    glass_back_pipeline: wgpu::RenderPipeline,
    glass_front_pipeline: wgpu::RenderPipeline,
    additive_pipeline: wgpu::RenderPipeline,
    panel_pipeline: wgpu::RenderPipeline,
    blit_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    camera_buffer: wgpu::Buffer,
    camera_group: wgpu::BindGroup,
    inset_camera_group: wgpu::BindGroup,
    scene_panel_group: wgpu::BindGroup,
    inset_panel_group: wgpu::BindGroup,
    blit_layout: wgpu::BindGroupLayout,
    blit_sampler: wgpu::Sampler,
    format: wgpu::TextureFormat,
    offscreen: Option<OffscreenTargets>,
    ranges: SceneRanges,
    physical_bounds: [u32; 4],
    inset_viewport: Option<[f32; 4]>,
}

/// Widget-sized 4x multisampled scene target with its single-sample resolve
/// texture; the resolve is composited into the application frame by the blit
/// pass.
#[derive(Debug)]
struct OffscreenTargets {
    msaa_view: wgpu::TextureView,
    resolve_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
    blit_group: wgpu::BindGroup,
    size: [u32; 2],
}

/// Longest renderable prefix under the GPU budget: the whole scene when it
/// fits, otherwise every triangle whose vertices landed inside the vertex
/// budget. Batches append the macroscopic scene before the inset, so an
/// overflow sheds the inset and late effects instead of blanking the frame.
fn budget_prefix(vertex_len: usize, indices: &[u32]) -> (usize, usize) {
    let vertex_budget = usize::try_from(MAX_VERTICES).unwrap_or(usize::MAX);
    let index_budget = usize::try_from(MAX_INDICES).unwrap_or(usize::MAX);
    if vertex_len <= vertex_budget && indices.len() <= index_budget {
        return (vertex_len, indices.len());
    }
    let vertex_limit = vertex_len.min(vertex_budget);
    let index_limit = indices.len().min(index_budget);
    let mut cut = 0;
    while cut < index_limit && (indices[cut] as usize) < vertex_limit {
        cut += 1;
    }
    (vertex_limit, cut - cut % 3)
}

/// Fixed inset camera: the molecular layout is normalized to fit a unit-ish
/// sphere, so a static orthographic frame always contains it.
fn inset_camera_uniform() -> CameraUniform {
    let eye = Quat::from_rotation_y(0.60) * Quat::from_rotation_x(-0.38) * Vec3::new(0.0, 0.0, 6.0);
    let view = Mat4::look_at_rh(eye, Vec3::ZERO, Vec3::Y);
    let projection = Mat4::orthographic_rh(-1.12, 1.12, -1.12, 1.12, 0.1, 20.0);
    CameraUniform {
        view_projection: (projection * view).to_cols_array_2d(),
        key_direction: [-0.55, -0.88, -0.48, 0.0],
        fill_direction: [0.70, -0.45, 0.55, 0.0],
        camera_position: [eye.x, eye.y, eye.z, 1.0],
    }
}

const SCENE_PANEL: PanelUniform = PanelUniform {
    top: [0.080, 0.098, 0.120, 1.0],
    bottom: [0.028, 0.036, 0.047, 1.0],
    border: [0.0; 4],
    params: [0.0, 0.50, 0.0, 0.0],
};

const INSET_PANEL: PanelUniform = PanelUniform {
    top: [0.095, 0.114, 0.137, 1.0],
    bottom: [0.055, 0.067, 0.082, 1.0],
    border: [0.26, 0.36, 0.45, 1.0],
    params: [0.012, 0.30, 0.0, 0.0],
};

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
        let uniform_entry = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("chemspec structural 3d camera layout"),
            entries: &[uniform_entry(0)],
        });
        let panel_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("chemspec structural 3d panel layout"),
            entries: &[uniform_entry(0)],
        });
        let blit_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("chemspec structural 3d blit layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let uniform_buffer = |label: &'static str, size: u64| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let camera_size = std::mem::size_of::<CameraUniform>() as u64;
        let panel_size = std::mem::size_of::<PanelUniform>() as u64;
        let camera_buffer = uniform_buffer("chemspec structural 3d camera", camera_size);
        let inset_camera_buffer = uniform_buffer("chemspec structural 3d inset camera", camera_size);
        let scene_panel_buffer = uniform_buffer("chemspec structural 3d scene panel", panel_size);
        let inset_panel_buffer = uniform_buffer("chemspec structural 3d inset panel", panel_size);
        queue.write_buffer(
            &inset_camera_buffer,
            0,
            bytemuck::bytes_of(&inset_camera_uniform()),
        );
        queue.write_buffer(&scene_panel_buffer, 0, bytemuck::bytes_of(&SCENE_PANEL));
        queue.write_buffer(&inset_panel_buffer, 0, bytemuck::bytes_of(&INSET_PANEL));
        let single_group = |label: &'static str,
                            layout: &wgpu::BindGroupLayout,
                            buffer: &wgpu::Buffer| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(label),
                layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                }],
            })
        };
        let camera_group = single_group(
            "chemspec structural 3d camera group",
            &camera_layout,
            &camera_buffer,
        );
        let inset_camera_group = single_group(
            "chemspec structural 3d inset camera group",
            &camera_layout,
            &inset_camera_buffer,
        );
        let scene_panel_group = single_group(
            "chemspec structural 3d scene panel group",
            &panel_layout,
            &scene_panel_buffer,
        );
        let inset_panel_group = single_group(
            "chemspec structural 3d inset panel group",
            &panel_layout,
            &inset_panel_buffer,
        );
        let scene_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("chemspec structural 3d pipeline layout"),
                bind_group_layouts: &[&camera_layout],
                push_constant_ranges: &[],
            });
        let panel_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("chemspec structural 3d panel pipeline layout"),
                bind_group_layouts: &[&panel_layout],
                push_constant_ranges: &[],
            });
        let blit_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("chemspec structural 3d blit pipeline layout"),
            bind_group_layouts: &[&blit_layout],
            push_constant_ranges: &[],
        });
        let msaa_state = wgpu::MultisampleState {
            count: MSAA_SAMPLES,
            ..wgpu::MultisampleState::default()
        };
        let create_pipeline = |label: &'static str,
                               blend: Option<wgpu::BlendState>,
                               depth_write_enabled: bool,
                               cull_mode: Option<wgpu::Face>,
                               fragment_entry: &'static str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&scene_pipeline_layout),
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
                multisample: msaa_state,
                multiview: None,
                cache: None,
            })
        };
        let opaque_pipeline = create_pipeline(
            "chemspec structural 3d opaque pipeline",
            None,
            true,
            Some(wgpu::Face::Back),
            "fragment_solid",
        );
        let translucent_pipeline = create_pipeline(
            "chemspec structural 3d translucent pipeline",
            Some(wgpu::BlendState::ALPHA_BLENDING),
            false,
            None,
            "fragment_liquid",
        );
        let glass_back_pipeline = create_pipeline(
            "chemspec structural 3d glass back pipeline",
            Some(wgpu::BlendState::ALPHA_BLENDING),
            false,
            Some(wgpu::Face::Front),
            "fragment_glass",
        );
        let glass_front_pipeline = create_pipeline(
            "chemspec structural 3d glass front pipeline",
            Some(wgpu::BlendState::ALPHA_BLENDING),
            false,
            Some(wgpu::Face::Back),
            "fragment_glass",
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
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: msaa_state,
            multiview: None,
            cache: None,
        });
        let blit_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("chemspec structural 3d blit pipeline"),
            layout: Some(&blit_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &post_shader,
                entry_point: Some("blit_vertex"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &post_shader,
                entry_point: Some("blit_fragment"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let blit_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("chemspec structural 3d blit sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..wgpu::SamplerDescriptor::default()
        });
        Self {
            opaque_pipeline,
            translucent_pipeline,
            glass_back_pipeline,
            glass_front_pipeline,
            additive_pipeline,
            panel_pipeline,
            blit_pipeline,
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
            camera_buffer,
            camera_group,
            inset_camera_group,
            scene_panel_group,
            inset_panel_group,
            blit_layout,
            blit_sampler,
            format,
            offscreen: None,
            ranges: SceneRanges::default(),
            physical_bounds: [0; 4],
            inset_viewport: None,
        }
    }
}

impl shader::Primitive for ScenePrimitive {
    type Pipeline = ScenePipeline;

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss,
        clippy::too_many_lines
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
        if pipeline
            .offscreen
            .as_ref()
            .is_none_or(|offscreen| offscreen.size != [width, height])
        {
            let colour_target = |label: &'static str, samples: u32, usage| {
                device
                    .create_texture(&wgpu::TextureDescriptor {
                        label: Some(label),
                        size: wgpu::Extent3d {
                            width,
                            height,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: samples,
                        dimension: wgpu::TextureDimension::D2,
                        format: pipeline.format,
                        usage,
                        view_formats: &[],
                    })
                    .create_view(&wgpu::TextureViewDescriptor::default())
            };
            let msaa_view = colour_target(
                "chemspec structural 3d msaa colour",
                MSAA_SAMPLES,
                wgpu::TextureUsages::RENDER_ATTACHMENT,
            );
            let resolve_view = colour_target(
                "chemspec structural 3d resolve colour",
                1,
                wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            );
            let depth_view = device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("chemspec structural 3d depth"),
                    size: wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: MSAA_SAMPLES,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Depth32Float,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                })
                .create_view(&wgpu::TextureViewDescriptor::default());
            let blit_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("chemspec structural 3d blit group"),
                layout: &pipeline.blit_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&resolve_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&pipeline.blit_sampler),
                    },
                ],
            });
            pipeline.offscreen = Some(OffscreenTargets {
                msaa_view,
                resolve_view,
                depth_view,
                blit_group,
                size: [width, height],
            });
        }
        let batches = &self.batches;
        let (vertex_count, prefix_len) = budget_prefix(batches.vertices.len(), &batches.indices);
        queue.write_buffer(
            &pipeline.vertex_buffer,
            0,
            bytemuck::cast_slice(&batches.vertices[..vertex_count]),
        );
        queue.write_buffer(
            &pipeline.index_buffer,
            0,
            bytemuck::cast_slice(&batches.indices[..prefix_len]),
        );
        let index_count = u32::try_from(prefix_len).unwrap_or(u32::MAX);
        let opaque_end = batches.opaque_end.min(index_count);
        let translucent_end = batches.translucent_end.clamp(opaque_end, index_count);
        let glass_end = batches.glass_end.clamp(translucent_end, index_count);
        let emissive_end = batches.emissive_end.clamp(glass_end, index_count);
        let inset_end = batches.inset_end.clamp(emissive_end, index_count);
        pipeline.ranges = SceneRanges {
            opaque_end,
            translucent_end,
            glass_end,
            emissive_end,
            inset_end,
        };
        // The compact layout hides the caption, so hide the inset with it.
        pipeline.inset_viewport = (inset_end > emissive_end
            && bounds.width >= crate::theme::breakpoint::MOBILE)
            .then(|| {
                let side = molecular_inset_side(bounds.width, bounds.height) * scale;
                let margin = INSET_MARGIN * scale;
                let x = width as f32 - side - margin;
                let y = height as f32 - side - margin;
                (x >= 0.0 && y >= 0.0).then_some([x, y, side, side])
            })
            .flatten();

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
        queue.write_buffer(&pipeline.camera_buffer, 0, bytemuck::bytes_of(&uniform));
    }

    #[allow(clippy::cast_precision_loss, clippy::too_many_lines)]
    fn render(
        &self,
        pipeline: &Self::Pipeline,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        let Some(offscreen) = &pipeline.offscreen else {
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
        let ranges = pipeline.ranges;
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("chemspec structural 3d scene pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &offscreen.msaa_view,
                    depth_slice: None,
                    resolve_target: Some(&offscreen.resolve_view),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Discard,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &offscreen.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_vertex_buffer(0, pipeline.vertex_buffer.slice(..));
            pass.set_index_buffer(pipeline.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.set_pipeline(&pipeline.panel_pipeline);
            pass.set_bind_group(0, &pipeline.scene_panel_group, &[]);
            pass.draw(0..3, 0..1);
            pass.set_bind_group(0, &pipeline.camera_group, &[]);
            if ranges.opaque_end > 0 {
                pass.set_pipeline(&pipeline.opaque_pipeline);
                pass.draw_indexed(0..ranges.opaque_end, 0, 0..1);
            }
            // Back faces of the glass first, then interior volumes, then the
            // glass front: the beaker reads as an enclosing shell around its
            // contents instead of a single unsorted transparent batch.
            if ranges.translucent_end < ranges.glass_end {
                pass.set_pipeline(&pipeline.glass_back_pipeline);
                pass.draw_indexed(ranges.translucent_end..ranges.glass_end, 0, 0..1);
            }
            if ranges.opaque_end < ranges.translucent_end {
                pass.set_pipeline(&pipeline.translucent_pipeline);
                pass.draw_indexed(ranges.opaque_end..ranges.translucent_end, 0, 0..1);
            }
            if ranges.translucent_end < ranges.glass_end {
                pass.set_pipeline(&pipeline.glass_front_pipeline);
                pass.draw_indexed(ranges.translucent_end..ranges.glass_end, 0, 0..1);
            }
            if ranges.glass_end < ranges.emissive_end {
                pass.set_pipeline(&pipeline.additive_pipeline);
                pass.draw_indexed(ranges.glass_end..ranges.emissive_end, 0, 0..1);
            }
            if let Some([inset_x, inset_y, inset_width, inset_height]) = pipeline.inset_viewport
                && ranges.emissive_end < ranges.inset_end
            {
                pass.set_viewport(inset_x, inset_y, inset_width, inset_height, 0.0, 1.0);
                pass.set_pipeline(&pipeline.panel_pipeline);
                pass.set_bind_group(0, &pipeline.inset_panel_group, &[]);
                pass.draw(0..3, 0..1);
                pass.set_pipeline(&pipeline.opaque_pipeline);
                pass.set_bind_group(0, &pipeline.inset_camera_group, &[]);
                pass.draw_indexed(ranges.emissive_end..ranges.inset_end, 0, 0..1);
            }
        }
        let mut composite = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
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
        composite.set_viewport(x as f32, y as f32, width as f32, height as f32, 0.0, 1.0);
        composite.set_scissor_rect(scissor_x, scissor_y, scissor_width, scissor_height);
        composite.set_pipeline(&pipeline.blit_pipeline);
        composite.set_bind_group(0, &offscreen.blit_group, &[]);
        composite.draw(0..3, 0..1);
    }
}

#[derive(Debug, Clone, Copy)]
struct SceneLayout {
    bench_top: f32,
    vessel_center: Vec3,
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
        let vessel_scale = vessel.map_or(Vec3::ONE, |object| transform_scale(&object.transform));
        let vessel_source = vessel.map_or(Vec3::ZERO, |object| {
            transform_translation(&object.transform)
        });
        let vessel_center = Vec3::new(
            vessel_source.x,
            bench_top + 0.55 * vessel_scale.y,
            vessel_source.z,
        );
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
        let reaction_point = Vec3::new(vessel_center.x, liquid_surface + 0.065, vessel_center.z);
        let precipitation = plan.effects.iter().any(|effect| {
            matches!(
                effect.effect,
                EffectProfile::PrecipitateFormation | EffectProfile::Clouding
            )
        });
        let camera_target = Vec3::new(
            vessel_center.x,
            if precipitation {
                liquid_center.y
            } else {
                liquid_surface
            },
            vessel_center.z,
        );
        Self {
            bench_top,
            vessel_center,
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
}

#[derive(Debug, Clone, Copy)]
struct ObjectMotion {
    translation: Vec3,
    rotation: Quat,
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

impl Default for ObjectMotion {
    fn default() -> Self {
        Self {
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
        }
    }
}

fn build_scene(
    plan: &ScenePlan,
    ordinal: u16,
    progress: f32,
    reactant_previews: &[TrustedCompositionPreview],
    product_preview: Option<&TrustedCompositionPreview>,
) -> SceneBatches {
    let mut meshes = SceneMeshes::default();
    let layout = SceneLayout::resolve(plan);
    let final_ordinal = plan
        .timeline
        .beats
        .last()
        .map_or(ordinal, |beat| beat.end_ordinal);
    let visual_inputs =
        ReactionVisualInputs::from_effects(&plan.effects, ordinal, progress, final_ordinal);
    let phase = continuous_phase(ordinal, progress);
    let reaction_motion = reaction_surface_motion(plan, ordinal, progress);
    let vibration = container_vibration_offset(visual_inputs, phase, plan_seed(plan));
    let effect_colours = scene_effect_colours(plan, ordinal, progress);
    let animated_layout = layout
        .with_vessel_motion(vibration)
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
        None,
    );
    for object in &plan.objects {
        if object.visible_from_ordinal <= ordinal {
            // Consumption/replacement shrink composes with the reviewed
            // formation grow-in: both live in [0, 1].
            let scale = object_scale_from_effects(plan, object.role, ordinal, progress)
                * object_replacement_scale(plan, object, ordinal, progress)
                * object_formation_scale(object, ordinal, progress);
            let motion = object_motion(plan, object, ordinal, progress, reaction_motion);
            let object_vibration = if object.role == SceneRole::Environment {
                Vec3::ZERO
            } else {
                vibration
            };
            // A completed consumption removes the reactant from the scene.
            if scale <= f32::EPSILON {
                continue;
            }
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
                object_colour_transition(object, ordinal, progress),
            );
        }
    }
    for effect in &plan.effects {
        if effect.start_ordinal <= ordinal && ordinal <= effect.end_ordinal {
            instantiate_effect(
                &mut meshes,
                effect,
                ordinal,
                progress,
                animated_layout,
                effect_seed(plan, effect),
                effect_colours,
            );
        }
    }
    // The molecular model lives in its own corner inset, never inside the
    // macroscopic scene: the beaker shows what an observer would see, the
    // inset shows the sub-microscopic story at its own scale.
    if let Some(preview) =
        active_molecular_preview(plan, ordinal, reactant_previews, product_preview)
    {
        let identity = PresentationTransform {
            translation: [0, 0, 0],
            rotation: [0, 0, 0],
            scale: [1000, 1000, 1000],
        };
        let spin = Quat::from_rotation_y(phase * 0.45);
        instantiate_molecule(&mut meshes.inset, preview, &identity, 1.0, Vec3::ZERO, spin);
    }
    meshes.finish()
}

/// The preview shown in the molecular inset: the product once any product
/// object has reached its trusted visibility ordinal, otherwise the first
/// reactant with a reviewed structure.
// ponytail: single reactant only; cycle through reactants if that ever reads
// as an omission.
pub fn active_molecular_preview<'preview>(
    plan: &ScenePlan,
    ordinal: u16,
    reactant_previews: &'preview [TrustedCompositionPreview],
    product_preview: Option<&'preview TrustedCompositionPreview>,
) -> Option<&'preview TrustedCompositionPreview> {
    let product_visible = plan.objects.iter().any(|object| {
        object.role == SceneRole::Product && object.visible_from_ordinal <= ordinal
    });
    if product_visible && product_preview.is_some() {
        return product_preview;
    }
    reactant_previews.first()
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
    let profile = match role {
        SceneRole::Reactant => Some((EffectProfile::ObjectShrinkage, false)),
        SceneRole::Product => Some((EffectProfile::PrecipitateFormation, true)),
        _ => None,
    };
    let Some((profile, grows)) = profile else {
        return 1.0;
    };
    plan.effects
        .iter()
        .find(|effect| effect.effect == profile && effect.start_ordinal <= ordinal)
        .map_or(1.0, |effect| {
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
        })
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
    let has_formation_effect = plan
        .effects
        .iter()
        .any(|effect| effect.effect == EffectProfile::PrecipitateFormation);

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
    let seed = stable_seed(&object.id) ^ plan_seed(plan);
    let arrival_progress = reactant_arrival_progress(plan, object, ordinal, progress);
    let introduction = gravitational_drop_offset(seed, arrival_progress);
    let contact_age = (continuous_phase(ordinal, progress)
        - f32::from(reactant_contact_ordinal(plan, object)))
    .max(0.0);
    let impact = damped_impact_offset(seed, contact_age);
    let phase = continuous_phase(ordinal, progress);
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
        EffectProfile::BubbleEmitter => 0x9e37_79b9_7f4a_7c15,
        EffectProfile::GasRelease => 0xd1b5_4a32_d192_ed03,
        EffectProfile::SurfaceDisturbance => 0x94d0_49bb_1331_11eb,
        EffectProfile::LiquidMixing => 0x3f84_d5b5_b547_0917,
        EffectProfile::SplashEmitter => 0x8538_ec85_5c19_1b69,
        EffectProfile::ObjectShrinkage => 0xda94_2042_e4dd_58b5,
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
                EffectProfile::SurfaceDisturbance | EffectProfile::LiquidMixing
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
    colour_transition: Option<AssetColourTransition>,
) {
    let position = transform_translation(transform) + position_offset;
    let scale = transform_scale(transform) * scale_multiplier;
    let rotation = rotation_offset * transform_rotation(transform);
    let color = appearance_color(appearance);
    let opaque_start = meshes.opaque.vertices.len();
    let translucent_start = meshes.translucent.vertices.len();
    let glass_start = meshes.glass.vertices.len();
    match scene_registry::asset_geometry(asset) {
        AssetGeometry::Bench => {
            add_box(
                &mut meshes.opaque,
                position,
                Vec3::new(20.0, 0.28, 10.0) * scale,
                color,
            );
        }
        AssetGeometry::CylindricalVessel => {
            let bottom = position.y - 0.55 * scale.y;
            let top = position.y + 0.95 * scale.y;
            let radius = 0.92 * scale.x;
            let thickness = (0.035 * scale.x).max(0.022);
            add_lathe(
                &mut meshes.glass,
                Vec3::new(position.x, 0.0, position.z),
                &beaker_profile(bottom, top, radius, thickness),
                color,
            );
            // Soft contact shadow grounding the vessel on the bench.
            add_soft_disc(
                &mut meshes.translucent,
                Vec3::new(position.x, bottom + 0.004, position.z),
                radius * 1.45,
                [0.0, 0.005, 0.01, 0.38],
            );
        }
        AssetGeometry::LiquidCylinder => {
            add_liquid_volume(
                &mut meshes.translucent,
                position,
                scale,
                color,
                visual_inputs.liquid_turbulence,
                phase,
                variation_seed,
            );
        }
        AssetGeometry::LowPolyChunk => {
            let variation = 0.96 + f32::from((variation_seed % 9) as u8) * 0.01;
            add_irregular_chunk(
                &mut meshes.opaque,
                position,
                Vec3::new(0.52, 0.18, 0.36) * scale * variation,
                color,
                variation_seed,
            );
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
            add_gas_volume(
                &mut meshes.translucent,
                position,
                scale,
                color,
                variation_seed,
                phase,
                0.58 + visual_inputs.gas_generation_rate * 0.42,
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
    }
    rotate_mesh_vertices(&mut meshes.opaque, opaque_start, position, rotation);
    rotate_mesh_vertices(
        &mut meshes.translucent,
        translucent_start,
        position,
        rotation,
    );
    rotate_mesh_vertices(&mut meshes.glass, glass_start, position, rotation);
}

/// Classic ball-and-stick proportion: ball radius as a fraction of the van
/// der Waals radius, leaving the bonds visible between atoms.
const BALL_RADIUS_FRACTION: f32 = 0.30;

/// Renders the exact reviewed structural preview as a compact ball-and-stick
/// model. This replaces generic particle clusters whenever the active request
/// exposes an unambiguous catalogue graph.
fn instantiate_molecule(
    mesh: &mut Mesh,
    preview: &TrustedCompositionPreview,
    transform: &PresentationTransform,
    scale_multiplier: f32,
    position_offset: Vec3,
    rotation_offset: Quat,
) {
    let center = transform_translation(transform) + position_offset;
    let scale = transform_scale(transform) * scale_multiplier;
    let uniform_scale = (scale.x + scale.y + scale.z) / 3.0;
    let rotation = rotation_offset * transform_rotation(transform);
    let layout = molecular_layout(preview);
    let world_unit = layout.units_per_angstrom * uniform_scale;
    let start = mesh.vertices.len();

    for bond in preview.covalent_bonds() {
        let (Some(left), Some(right)) = (
            layout.positions.get(bond.start),
            layout.positions.get(bond.end),
        ) else {
            continue;
        };
        add_molecular_bond(
            mesh,
            center + *left * uniform_scale,
            center + *right * uniform_scale,
            bond.order,
            world_unit,
            molecular_atom_color(preview.atoms[bond.start].atomic_number),
            molecular_atom_color(preview.atoms[bond.end].atomic_number),
        );
    }
    for link in preview.ionic_links() {
        let (Some(left), Some(right)) = (
            layout.positions.get(link.start),
            layout.positions.get(link.end),
        ) else {
            continue;
        };
        let ionic = [0.30, 0.78, 0.96, 1.0];
        add_molecular_bond(
            mesh,
            center + *left * uniform_scale,
            center + *right * uniform_scale,
            1,
            world_unit * 0.6,
            ionic,
            ionic,
        );
    }
    for (atom, position) in preview.atoms.iter().zip(&layout.positions) {
        add_sphere(
            mesh,
            center + *position * uniform_scale,
            vdw_radius_angstrom(atom.atomic_number) * BALL_RADIUS_FRACTION * world_unit,
            molecular_atom_color(atom.atomic_number),
            8,
            12,
        );
    }
    rotate_mesh_vertices(mesh, start, center, rotation);
}

/// Each bond is split at its midpoint and half-coloured by the atom it
/// touches — the standard ball-and-stick convention.
fn add_molecular_bond(
    mesh: &mut Mesh,
    start: Vec3,
    end: Vec3,
    order: u8,
    world_unit: f32,
    start_color: [f32; 4],
    end_color: [f32; 4],
) {
    let direction = (end - start).normalize_or_zero();
    let perpendicular = if direction.cross(Vec3::Y).length_squared() > 0.01 {
        direction.cross(Vec3::Y).normalize()
    } else {
        direction.cross(Vec3::X).normalize_or_zero()
    };
    // Parallel-line spacing for double and triple bonds, in ångströms.
    let offsets: &[f32] = match order {
        1 => &[0.0],
        2 => &[-0.17, 0.17],
        _ => &[-0.28, 0.0, 0.28],
    };
    let radius = 0.13 * world_unit;
    let midpoint = (start + end) * 0.5;
    for offset in offsets {
        let offset = perpendicular * *offset * world_unit;
        add_cylinder(mesh, start + offset, midpoint + offset, radius, start_color);
        add_cylinder(mesh, midpoint + offset, end + offset, radius, end_color);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MolecularLink {
    Covalent,
    Ionic,
}

struct MolecularLayout {
    /// Scene-unit position per preview atom.
    positions: Vec<Vec3>,
    /// Scene units per ångström after fitting the model to the vessel frame.
    units_per_angstrom: f32,
}

/// Embeds the reviewed bond graph in 3D with VSEPR electron-domain angles and
/// covalent-radius bond lengths. Lone pairs come straight from the preview's
/// non-bonding electron counts, so bent water, pyramidal ammonia, and
/// pentagonal-bipyramidal IF7 all fall out of the same rule.
#[allow(clippy::cast_precision_loss, clippy::too_many_lines)]
fn molecular_layout(preview: &TrustedCompositionPreview) -> MolecularLayout {
    const BASE_UNITS_PER_ANGSTROM: f32 = 0.50;
    const MIN_HALF_EXTENT: f32 = 0.34;
    const MAX_HALF_EXTENT: f32 = 0.88;
    let atom_count = preview.atoms.len();
    if atom_count == 0 {
        return MolecularLayout {
            positions: Vec::new(),
            units_per_angstrom: BASE_UNITS_PER_ANGSTROM,
        };
    }

    // Neighbour lists as (other, bond order, kind), covalent entries first so
    // real bonds claim VSEPR slots before ionic associations take the
    // remaining lone-pair directions.
    let mut neighbours: Vec<Vec<(usize, u8, MolecularLink)>> = vec![Vec::new(); atom_count];
    let links = preview
        .covalent_bonds()
        .iter()
        .map(|bond| (bond.start, bond.end, bond.order, MolecularLink::Covalent))
        .chain(
            preview
                .ionic_links()
                .iter()
                .map(|link| (link.start, link.end, 1, MolecularLink::Ionic)),
        );
    for (left, right, order, kind) in links {
        if left < atom_count
            && right < atom_count
            && left != right
            && !neighbours[left].iter().any(|(other, _, _)| *other == right)
        {
            neighbours[left].push((right, order, kind));
            neighbours[right].push((left, order, kind));
        }
    }
    for list in &mut neighbours {
        list.sort_by_key(|(other, _, kind)| (*kind == MolecularLink::Ionic, *other));
    }
    let covalent_degree = |atom: usize| {
        neighbours[atom]
            .iter()
            .filter(|(_, _, kind)| *kind == MolecularLink::Covalent)
            .count()
    };

    // Positions are laid out in ångströms, then normalized to scene units.
    let mut positions = vec![Vec3::ZERO; atom_count];
    let mut placed = vec![false; atom_count];
    let mut parent_map: Vec<Option<usize>> = vec![None; atom_count];
    let mut cursor_x = f32::NEG_INFINITY;
    for seed in 0..atom_count {
        if placed[seed] {
            continue;
        }
        let mut in_component = vec![false; atom_count];
        in_component[seed] = true;
        let mut stack = vec![seed];
        let mut component = Vec::new();
        let mut edge_ends = 0_usize;
        while let Some(atom) = stack.pop() {
            component.push(atom);
            for (other, _, _) in &neighbours[atom] {
                edge_ends += 1;
                if !in_component[*other] {
                    in_component[*other] = true;
                    stack.push(*other);
                }
            }
        }
        let mut queue = std::collections::VecDeque::new();
        if edge_ends / 2 >= component.len() && component.len() > 2 {
            if let Some(ring) = simple_ring(&component, &neighbours) {
                // ponytail: flat regular polygon even for saturated rings;
                // chair/crown conformers can replace this if the visual
                // ever matters.
                let mut edge_sum = 0.0_f32;
                for (slot, atom) in ring.iter().copied().enumerate() {
                    let next = ring[(slot + 1) % ring.len()];
                    let (_, order, kind) = neighbours[atom]
                        .iter()
                        .copied()
                        .find(|(other, _, _)| *other == next)
                        .expect("consecutive ring atoms are bonded");
                    edge_sum += bond_length_angstrom(
                        preview.atoms[atom].atomic_number,
                        preview.atoms[next].atomic_number,
                        order,
                        kind,
                    );
                }
                let edge = edge_sum / ring.len() as f32;
                let ring_radius = edge / (2.0 * (std::f32::consts::PI / ring.len() as f32).sin());
                for (slot, atom) in ring.iter().copied().enumerate() {
                    let angle = std::f32::consts::TAU * slot as f32 / ring.len() as f32;
                    positions[atom] =
                        Vec3::new(angle.cos() * ring_radius, 0.0, angle.sin() * ring_radius);
                    placed[atom] = true;
                }
                // Substituents take the directions the ring leaves free:
                // in-plane radial for trigonal ring atoms, half-tetrahedral
                // tilts above/below the plane (alternating around the ring)
                // for tetrahedral ones. Their subtrees continue through the
                // shared BFS below.
                for (slot, atom) in ring.iter().copied().enumerate() {
                    let radial = positions[atom].normalize_or_zero();
                    let lone_pairs = usize::from(preview.atoms[atom].non_bonding_electrons / 2);
                    let steric = (covalent_degree(atom) + lone_pairs).max(neighbours[atom].len());
                    let mut branch = 0_usize;
                    for (other, order, kind) in &neighbours[atom] {
                        if placed[*other] {
                            continue;
                        }
                        let direction = if steric <= 3 {
                            radial
                        } else {
                            let side = if (slot + branch).is_multiple_of(2) { 1.0 } else { -1.0 };
                            (radial / 3.0_f32.sqrt() + Vec3::Y * side * (2.0 / 3.0_f32).sqrt())
                                .normalize()
                        };
                        branch += 1;
                        let length = bond_length_angstrom(
                            preview.atoms[atom].atomic_number,
                            preview.atoms[*other].atomic_number,
                            *order,
                            *kind,
                        );
                        positions[*other] = positions[atom] + direction * length;
                        placed[*other] = true;
                        parent_map[*other] = Some(atom);
                        queue.push_back(*other);
                    }
                }
            } else {
                // ponytail: fused rings and cages (P4) still fall back to a
                // regular polygon with true edge lengths; replace with a
                // multi-ring embedder if fused systems ever enter the
                // catalogue.
                let mut length_sum = 0.0_f32;
                let mut length_count = 0_u32;
                for atom in &component {
                    for (other, order, kind) in &neighbours[*atom] {
                        length_sum += bond_length_angstrom(
                            preview.atoms[*atom].atomic_number,
                            preview.atoms[*other].atomic_number,
                            *order,
                            *kind,
                        );
                        length_count += 1;
                    }
                }
                let edge = length_sum / length_count.max(1) as f32;
                let polygon_radius =
                    edge / (2.0 * (std::f32::consts::PI / component.len() as f32).sin());
                for (slot, atom) in component.iter().copied().enumerate() {
                    let angle = std::f32::consts::TAU * slot as f32 / component.len() as f32;
                    positions[atom] =
                        Vec3::new(angle.cos() * polygon_radius, 0.0, angle.sin() * polygon_radius);
                    placed[atom] = true;
                }
            }
        } else {
            let root = component
                .iter()
                .copied()
                .max_by_key(|atom| (covalent_degree(*atom), std::cmp::Reverse(*atom)))
                .unwrap_or(seed);
            placed[root] = true;
            queue.push_back(root);
        }
        while let Some(atom) = queue.pop_front() {
            let lone_pairs = usize::from(preview.atoms[atom].non_bonding_electrons / 2);
            let slots = (covalent_degree(atom) + lone_pairs).max(neighbours[atom].len());
            let directions = vsepr_directions(slots);
            let mut oriented = directions;
            let mut next_slot = 0_usize;
            if let Some(parent) = parent_map[atom] {
                let to_parent = (positions[parent] - positions[atom]).normalize_or_zero();
                let mut orientation = Quat::from_rotation_arc(oriented[0], to_parent);
                // Torsion: point the next branch away from the
                // grandparent for staggered, zigzag chains.
                if let (Some(grandparent), true) = (parent_map[parent], oriented.len() > 1) {
                    let reference = (positions[grandparent] - positions[parent])
                        .reject_from_normalized(to_parent);
                    let candidate =
                        (orientation * oriented[1]).reject_from_normalized(to_parent);
                    if reference.length_squared() > 1e-6 && candidate.length_squared() > 1e-6 {
                        let from = candidate.normalize();
                        let to = (-reference).normalize();
                        let angle = from.cross(to).dot(to_parent).atan2(from.dot(to));
                        orientation = Quat::from_axis_angle(to_parent, angle) * orientation;
                    }
                }
                for direction in &mut oriented {
                    *direction = orientation * *direction;
                }
                next_slot = 1;
            }
            for (other, order, kind) in &neighbours[atom] {
                if placed[*other] {
                    continue;
                }
                let direction = oriented.get(next_slot).copied().unwrap_or(-oriented[0]);
                next_slot += 1;
                let length = bond_length_angstrom(
                    preview.atoms[atom].atomic_number,
                    preview.atoms[*other].atomic_number,
                    *order,
                    *kind,
                );
                positions[*other] = positions[atom] + direction * length;
                placed[*other] = true;
                parent_map[*other] = Some(atom);
                queue.push_back(*other);
            }
        }
        // Disconnected fragments sit side by side instead of overlapping.
        let (min_x, max_x) = component.iter().fold(
            (f32::INFINITY, f32::NEG_INFINITY),
            |(minimum, maximum), atom| {
                (minimum.min(positions[*atom].x), maximum.max(positions[*atom].x))
            },
        );
        let shift = if cursor_x.is_finite() {
            cursor_x + 1.2 - min_x
        } else {
            0.0
        };
        for atom in &component {
            positions[*atom].x += shift;
        }
        cursor_x = max_x + shift;
    }

    let centroid = positions.iter().copied().sum::<Vec3>() / atom_count as f32;
    let mut extent = 0.0_f32;
    for (atom, position) in preview.atoms.iter().zip(&mut positions) {
        *position -= centroid;
        extent = extent.max(
            position.length() + vdw_radius_angstrom(atom.atomic_number) * BALL_RADIUS_FRACTION,
        );
    }
    let mut units_per_angstrom = BASE_UNITS_PER_ANGSTROM;
    if extent * units_per_angstrom > MAX_HALF_EXTENT {
        units_per_angstrom = MAX_HALF_EXTENT / extent;
    } else if extent * units_per_angstrom < MIN_HALF_EXTENT && extent > f32::EPSILON {
        units_per_angstrom = MIN_HALF_EXTENT / extent;
    }
    for position in &mut positions {
        *position *= units_per_angstrom;
    }
    MolecularLayout {
        positions,
        units_per_angstrom,
    }
}

/// Orders a component's cycle atoms when they form exactly one simple ring:
/// leaves prune away until only the 2-core remains, and that core must be a
/// single cycle (every core atom keeps exactly two core neighbours). Fused
/// rings, spiro joints, and cages return `None`.
fn simple_ring(
    component: &[usize],
    neighbours: &[Vec<(usize, u8, MolecularLink)>],
) -> Option<Vec<usize>> {
    let mut in_core = vec![false; neighbours.len()];
    let mut degree = vec![0_usize; neighbours.len()];
    for atom in component {
        in_core[*atom] = true;
        degree[*atom] = neighbours[*atom].len();
    }
    let mut leaves: Vec<usize> = component
        .iter()
        .copied()
        .filter(|atom| degree[*atom] <= 1)
        .collect();
    while let Some(atom) = leaves.pop() {
        in_core[atom] = false;
        for (other, _, _) in &neighbours[atom] {
            if in_core[*other] {
                degree[*other] -= 1;
                if degree[*other] == 1 {
                    leaves.push(*other);
                }
            }
        }
    }
    let core: Vec<usize> = component
        .iter()
        .copied()
        .filter(|atom| in_core[*atom])
        .collect();
    if core.len() < 3 || core.iter().any(|atom| degree[*atom] != 2) {
        return None;
    }
    let mut ring = vec![core[0]];
    let mut previous = usize::MAX;
    loop {
        let current = *ring.last().expect("ring starts non-empty");
        let next = neighbours[current]
            .iter()
            .map(|(other, _, _)| *other)
            .find(|other| in_core[*other] && *other != previous)?;
        if next == ring[0] {
            break;
        }
        previous = current;
        ring.push(next);
    }
    (ring.len() == core.len()).then_some(ring)
}

/// Ideal electron-domain directions per steric number. Axial slots come first
/// for bipyramids so bonds fill axial positions before equatorial ones,
/// leaving lone pairs equatorial exactly as VSEPR prescribes.
#[allow(clippy::cast_precision_loss)]
fn vsepr_directions(count: usize) -> Vec<Vec3> {
    let equatorial = |slots: usize| {
        (0..slots).map(move |slot| {
            let angle = std::f32::consts::TAU * slot as f32 / slots as f32;
            Vec3::new(angle.cos(), 0.0, angle.sin())
        })
    };
    match count {
        0 | 1 => vec![Vec3::X],
        2 => vec![Vec3::X, -Vec3::X],
        3 => equatorial(3).collect(),
        4 => {
            let corner = 1.0 / 3.0_f32.sqrt();
            vec![
                Vec3::new(corner, corner, corner),
                Vec3::new(corner, -corner, -corner),
                Vec3::new(-corner, corner, -corner),
                Vec3::new(-corner, -corner, corner),
            ]
        }
        5 => [Vec3::Y, -Vec3::Y].into_iter().chain(equatorial(3)).collect(),
        6 => vec![Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z],
        7 => [Vec3::Y, -Vec3::Y].into_iter().chain(equatorial(5)).collect(),
        count => {
            let golden_angle = std::f32::consts::PI * (3.0 - 5.0_f32.sqrt());
            (0..count)
                .map(|slot| {
                    let y = 1.0 - 2.0 * (slot as f32 + 0.5) / count as f32;
                    let radius = (1.0 - y * y).sqrt();
                    let angle = golden_angle * slot as f32;
                    Vec3::new(angle.cos() * radius, y, angle.sin() * radius)
                })
                .collect()
        }
    }
}

fn bond_length_angstrom(left: u8, right: u8, order: u8, kind: MolecularLink) -> f32 {
    let sum = covalent_radius_angstrom(left) + covalent_radius_angstrom(right);
    match kind {
        MolecularLink::Ionic => sum * 1.25,
        MolecularLink::Covalent => {
            sum * match order {
                0 | 1 => 1.0,
                2 => 0.87,
                _ => 0.78,
            }
        }
    }
}

/// Covalent radii in ångströms (Cordero et al., 2008); elements outside the
/// reviewed reaction scope fall back to a period-based estimate.
fn covalent_radius_angstrom(atomic_number: u8) -> f32 {
    match atomic_number {
        1 => 0.31,
        2 => 0.28,
        3 => 1.28,
        4 => 0.96,
        5 => 0.84,
        6 => 0.76,
        7 => 0.71,
        8 => 0.66,
        9 => 0.57,
        10 => 0.58,
        11 => 1.66,
        12 => 1.41,
        13 => 1.21,
        14 => 1.11,
        15 => 1.07,
        16 => 1.05,
        17 => 1.02,
        18 => 1.06,
        19 => 2.03,
        20 => 1.76,
        25 | 50 | 53 => 1.39,
        26 | 29 => 1.32,
        30 => 1.22,
        35 => 1.20,
        47 => 1.45,
        56 => 2.15,
        82 => 1.46,
        other => crate::elements::by_atomic_number(other)
            .map_or(1.20, |element| 0.15 + 0.28 * f32::from(element.period)),
    }
}

/// Van der Waals radii in ångströms (Bondi/Alvarez); the covalent radius plus
/// a constant approximates elements without a tabulated value.
fn vdw_radius_angstrom(atomic_number: u8) -> f32 {
    match atomic_number {
        1 => 1.10,
        2 => 1.40,
        3 => 1.81,
        6 => 1.70,
        7 => 1.55,
        8 => 1.52,
        9 => 1.47,
        10 => 1.54,
        11 => 2.27,
        12 => 1.73,
        13 => 1.84,
        14 => 2.10,
        15 | 16 => 1.80,
        17 => 1.75,
        18 => 1.88,
        19 => 2.75,
        20 => 2.31,
        26 => 2.05,
        29 => 1.96,
        30 => 2.01,
        35 => 1.85,
        47 => 2.11,
        53 => 1.98,
        other => covalent_radius_angstrom(other) + 0.75,
    }
}

/// Standard CPK colours (Jmol convention) so models read the same as any
/// chemistry textbook; unlisted elements use the conventional fallback pink.
fn molecular_atom_color(atomic_number: u8) -> [f32; 4] {
    let rgb: [u8; 3] = match atomic_number {
        1 => [255, 255, 255],
        2 => [217, 255, 255],
        3 => [204, 128, 255],
        4 => [194, 255, 0],
        5 => [255, 181, 181],
        6 => [144, 144, 144],
        7 => [48, 80, 248],
        8 => [255, 13, 13],
        9 => [144, 224, 80],
        10 => [179, 227, 245],
        11 => [171, 92, 242],
        12 => [138, 255, 0],
        13 => [191, 166, 166],
        14 => [240, 200, 160],
        15 => [255, 128, 0],
        16 => [255, 255, 48],
        17 => [31, 240, 31],
        18 => [128, 209, 227],
        19 => [143, 64, 212],
        20 => [61, 255, 0],
        25 => [156, 122, 199],
        26 => [224, 102, 51],
        29 => [200, 128, 51],
        30 => [125, 128, 176],
        35 => [166, 41, 41],
        47 => [192, 192, 192],
        50 => [102, 128, 128],
        53 => [148, 0, 148],
        56 => [0, 201, 0],
        82 => [87, 89, 97],
        _ => [255, 20, 147],
    };
    [
        f32::from(rgb[0]) / 255.0,
        f32::from(rgb[1]) / 255.0,
        f32::from(rgb[2]) / 255.0,
        1.0,
    ]
}

#[allow(clippy::similar_names, clippy::too_many_lines)]
fn instantiate_effect(
    meshes: &mut SceneMeshes,
    effect: &PresentationEffect,
    ordinal: u16,
    progress: f32,
    layout: SceneLayout,
    seed: u64,
    colours: EffectColours,
) {
    let dynamics = scene_registry::effect_dynamics(effect.effect, effect.intensity);
    let effect_progress = effect_progress(effect, ordinal, progress);
    let envelope = effect_envelope(dynamics, effect_progress);
    let phase = continuous_phase(ordinal, progress);
    let count = dynamics.particle_count;
    let surface_point = layout.reaction_point;
    match scene_registry::effect_geometry(effect.effect) {
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
            let rise = normalized_terminal_distance(effect_progress, 3.6);
            let drift = curl_like_flow(phase * 0.44, seed, 0) * dynamics.turbulence * 0.12;
            let center =
                surface_point + drift + Vec3::new(0.0, 0.16 + rise * dynamics.lift * 0.72, 0.0);
            let cloud_scale = Vec3::new(
                0.30 + dynamics.spread * (0.58 + rise * 0.34),
                0.32 + dynamics.lift * (0.50 + rise * 0.42),
                0.30 + dynamics.spread * (0.52 + rise * 0.30),
            );
            add_gas_volume(
                &mut meshes.translucent,
                center,
                cloud_scale,
                alpha(colours.gas, envelope),
                seed,
                phase * dynamics.rate,
                envelope,
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
            add_mixing_currents(
                &mut meshes.translucent,
                layout.liquid_center,
                dynamics,
                envelope,
                phase,
                seed,
                colours.liquid,
            );
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
    // Additive light pool: the flame visibly illuminates the surface it
    // burns on, with a subtle seeded flicker.
    let flicker = 0.86 + 0.14 * (phase * 7.3 + seed_phase(seed, 11)).sin();
    add_soft_disc(
        &mut meshes.emissive,
        source + Vec3::new(0.0, 0.012, 0.0),
        dynamics.spread * (1.1 + 0.3 * flicker),
        alpha(colours.core, 0.24 * envelope * flicker),
    );
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

/// Builds one connected, irregular low-poly gas shell. Invisible procedural
/// parcels are represented by the lobes of this surface; the normal view never
/// exposes those parcels as molecular beads.
fn add_gas_volume(
    mesh: &mut Mesh,
    center: Vec3,
    scale: Vec3,
    color: [f32; 4],
    seed: u64,
    phase: f32,
    density: f32,
) {
    const RINGS: u16 = 8;
    const SECTORS: u16 = 14;
    if density <= 0.01 || color[3] <= 0.001 {
        return;
    }
    for ring in 0..RINGS {
        let latitude_a = std::f32::consts::PI * f32::from(ring) / f32::from(RINGS);
        let latitude_b = std::f32::consts::PI * f32::from(ring + 1) / f32::from(RINGS);
        for sector in 0..SECTORS {
            let longitude_a = std::f32::consts::TAU * f32::from(sector) / f32::from(SECTORS);
            let longitude_b = std::f32::consts::TAU * f32::from(sector + 1) / f32::from(SECTORS);
            let a = gas_surface_point(center, scale, latitude_a, longitude_a, seed, phase, density);
            let b = gas_surface_point(center, scale, latitude_b, longitude_a, seed, phase, density);
            let c = gas_surface_point(center, scale, latitude_b, longitude_b, seed, phase, density);
            let d = gas_surface_point(center, scale, latitude_a, longitude_b, seed, phase, density);
            add_flat_triangle(mesh, a, b, c, color);
            add_flat_triangle(mesh, a, c, d, color);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn gas_surface_point(
    center: Vec3,
    scale: Vec3,
    latitude: f32,
    longitude: f32,
    seed: u64,
    phase: f32,
    density: f32,
) -> Vec3 {
    let direction = Vec3::new(
        latitude.sin() * longitude.cos(),
        latitude.cos(),
        latitude.sin() * longitude.sin(),
    );
    let lobe_a =
        (longitude * 3.0 + phase * 0.83 + seed_phase(seed, 41)).sin() * latitude.sin().powi(2);
    let lobe_b = (latitude * 4.0 - phase * 0.57 + seed_phase(seed, 42)).cos();
    let lobe_c = (longitude * 5.0 + latitude * 2.0 + seed_phase(seed, 43)).sin();
    let radius = (0.78 + lobe_a * 0.15 + lobe_b * 0.09 + lobe_c * 0.06)
        * (0.72 + density.clamp(0.0, 1.0) * 0.28);
    let curl = Vec3::new(
        (latitude * 2.0 + phase * 0.47 + seed_phase(seed, 44)).sin(),
        (longitude * 2.0 - phase * 0.31 + seed_phase(seed, 45)).cos() * 0.35,
        (latitude + longitude + phase * 0.39 + seed_phase(seed, 46)).sin(),
    ) * 0.055
        * density;
    center + direction * scale * radius + curl
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
        // Warm neutral bench: value/hue separation from the cool glass and
        // backdrop so the coloured chemistry stays the hero of the frame.
        AppearanceProfile::LaboratoryNeutral => [0.21, 0.185, 0.165, 1.0],
        AppearanceProfile::ClearGlass => [0.60, 0.70, 0.76, 0.05],
        AppearanceProfile::Water => [0.32, 0.60, 0.75, 0.30],
        AppearanceProfile::AqueousColourless => [0.72, 0.79, 0.82, 0.18],
        AppearanceProfile::WhitePrecipitate => [0.94, 0.96, 1.0, 0.92],
        AppearanceProfile::CreamPrecipitate => [0.94, 0.88, 0.68, 0.92],
        AppearanceProfile::YellowPrecipitate => [0.94, 0.82, 0.28, 0.92],
        AppearanceProfile::AlkaliMetal => [0.72, 0.76, 0.78, 1.0],
        AppearanceProfile::MetalSilver => [0.72, 0.80, 0.88, 1.0],
    }
}

/// Revolves a 2D `(radius, y)` profile around the vertical axis through
/// `center` with smoothed normals, for vessel bodies with real wall
/// thickness, rolled lips, and floors.
#[allow(clippy::cast_possible_truncation)]
fn add_lathe(mesh: &mut Mesh, center: Vec3, profile: &[(f32, f32)], color: [f32; 4]) {
    const SEGMENTS: u16 = 28;
    if profile.len() < 2 {
        return;
    }
    let mut profile_normals = vec![glam::Vec2::ZERO; profile.len()];
    for index in 0..profile.len() - 1 {
        let delta_radius = profile[index + 1].0 - profile[index].0;
        let delta_y = profile[index + 1].1 - profile[index].1;
        let normal = glam::Vec2::new(delta_y, -delta_radius).normalize_or_zero();
        profile_normals[index] += normal;
        profile_normals[index + 1] += normal;
    }
    for normal in &mut profile_normals {
        *normal = normal.normalize_or_zero();
    }
    let base = u32::try_from(mesh.vertices.len()).unwrap_or(u32::MAX);
    for ((radius, y), normal) in profile.iter().zip(&profile_normals) {
        for segment in 0..=SEGMENTS {
            let angle = std::f32::consts::TAU * f32::from(segment) / f32::from(SEGMENTS);
            let direction = Vec3::new(angle.cos(), 0.0, angle.sin());
            mesh.vertices.push(Vertex {
                position: (center + direction * *radius + Vec3::Y * *y).to_array(),
                normal: (direction * normal.x + Vec3::Y * normal.y).to_array(),
                color,
            });
        }
    }
    let stride = u32::from(SEGMENTS) + 1;
    for ring in 0..profile.len() as u32 - 1 {
        for segment in 0..u32::from(SEGMENTS) {
            let current = base + ring * stride + segment;
            let next = current + stride;
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

/// Beaker cross-section from the outer floor up the outer wall, over a rolled
/// lip, and back down the inner wall to the inner floor.
fn beaker_profile(bottom: f32, top: f32, radius: f32, thickness: f32) -> Vec<(f32, f32)> {
    let lip = radius + thickness * 0.9;
    let inner = radius - thickness;
    vec![
        (radius * 0.06, bottom),
        (radius * 0.90, bottom),
        (radius, bottom + 0.05),
        (radius, top - 0.06),
        (lip, top - 0.012),
        (lip - thickness * 0.4, top + 0.016),
        (inner, top - 0.035),
        (inner, bottom + thickness + 0.012),
        (radius * 0.06, bottom + thickness),
    ]
}

/// Radially faded disc: full colour at the centre, transparent at the rim.
/// Used as a cheap soft contact shadow.
fn add_soft_disc(mesh: &mut Mesh, center: Vec3, radius: f32, color: [f32; 4]) {
    const SEGMENTS: u16 = 24;
    let base = u32::try_from(mesh.vertices.len()).unwrap_or(u32::MAX);
    mesh.vertices.push(Vertex {
        position: center.to_array(),
        normal: Vec3::Y.to_array(),
        color,
    });
    let edge_color = [color[0], color[1], color[2], 0.0];
    for segment in 0..=SEGMENTS {
        let angle = std::f32::consts::TAU * f32::from(segment) / f32::from(SEGMENTS);
        mesh.vertices.push(Vertex {
            position: (center + Vec3::new(angle.cos() * radius, 0.0, angle.sin() * radius))
                .to_array(),
            normal: Vec3::Y.to_array(),
            color: edge_color,
        });
    }
    for segment in 0..u32::from(SEGMENTS) {
        mesh.indices
            .extend_from_slice(&[base, base + 2 + segment, base + 1 + segment]);
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

#[derive(Debug, Default)]
struct SceneBatches {
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
    opaque_end: u32,
    translucent_end: u32,
    glass_end: u32,
    emissive_end: u32,
    inset_end: u32,
}

#[derive(Default)]
struct SceneMeshes {
    opaque: Mesh,
    translucent: Mesh,
    glass: Mesh,
    emissive: Mesh,
    inset: Mesh,
}

impl SceneMeshes {
    fn finish(self) -> SceneBatches {
        let mut vertices = Vec::with_capacity(
            self.opaque.vertices.len()
                + self.translucent.vertices.len()
                + self.glass.vertices.len()
                + self.emissive.vertices.len()
                + self.inset.vertices.len(),
        );
        let mut indices = Vec::with_capacity(
            self.opaque.indices.len()
                + self.translucent.indices.len()
                + self.glass.indices.len()
                + self.emissive.indices.len()
                + self.inset.indices.len(),
        );
        let boundary = |indices: &Vec<u32>| u32::try_from(indices.len()).unwrap_or(u32::MAX);
        append_mesh(&mut vertices, &mut indices, self.opaque);
        let opaque_end = boundary(&indices);
        append_mesh(&mut vertices, &mut indices, self.translucent);
        let translucent_end = boundary(&indices);
        append_mesh(&mut vertices, &mut indices, self.glass);
        let glass_end = boundary(&indices);
        append_mesh(&mut vertices, &mut indices, self.emissive);
        let emissive_end = boundary(&indices);
        append_mesh(&mut vertices, &mut indices, self.inset);
        let inset_end = boundary(&indices);
        SceneBatches {
            vertices,
            indices,
            opaque_end,
            translucent_end,
            glass_end,
            emissive_end,
            inset_end,
        }
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

fn add_irregular_chunk(mesh: &mut Mesh, center: Vec3, size: Vec3, color: [f32; 4], seed: u64) {
    let half = size * 0.5;
    let mut corners = [Vec3::ZERO; 8];
    for (index, corner) in corners.iter_mut().enumerate() {
        let sign = Vec3::new(
            if index & 1 == 0 { -1.0 } else { 1.0 },
            if index & 2 == 0 { -1.0 } else { 1.0 },
            if index & 4 == 0 { -1.0 } else { 1.0 },
        );
        let jitter = Vec3::new(
            seeded_variation(seed, index * 3),
            seeded_variation(seed, index * 3 + 1) * 0.45,
            seeded_variation(seed, index * 3 + 2),
        );
        *corner = center + sign * half * (Vec3::ONE + jitter);
    }
    let faces = [
        [0, 1, 3, 2],
        [5, 4, 6, 7],
        [4, 0, 2, 6],
        [1, 5, 7, 3],
        [2, 3, 7, 6],
        [4, 5, 1, 0],
    ];
    for indices in faces {
        let base = u32::try_from(mesh.vertices.len()).unwrap_or(u32::MAX);
        let normal = (corners[indices[1]] - corners[indices[0]])
            .cross(corners[indices[2]] - corners[indices[0]])
            .normalize_or_zero();
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
        let profile = chemistry::presentation_profile(request, run.frames())
            .expect("trusted observations select a presentation profile");
        compile_real_world_plan(run.frames(), &profile).expect("plan compiles from trusted frames")
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
        let plan = canonical_plan();
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
    fn gas_is_one_seeded_low_poly_volume_instead_of_a_particle_cluster() {
        let mut first = Mesh::default();
        add_gas_volume(
            &mut first,
            Vec3::ZERO,
            Vec3::new(0.8, 1.1, 0.7),
            [0.7, 0.84, 0.9, 0.2],
            42,
            1.25,
            0.8,
        );
        let mut repeated = Mesh::default();
        add_gas_volume(
            &mut repeated,
            Vec3::ZERO,
            Vec3::new(0.8, 1.1, 0.7),
            [0.7, 0.84, 0.9, 0.2],
            42,
            1.25,
            0.8,
        );

        assert!(first.vertices.len() > 300);
        assert_eq!(first.indices, repeated.indices);
        assert_eq!(
            bytemuck::cast_slice::<Vertex, u8>(&first.vertices),
            bytemuck::cast_slice::<Vertex, u8>(&repeated.vertices)
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
                    progress.map(|progress| AssetColourTransition {
                        target,
                        progress,
                        seed: 91,
                    }),
                );
                meshes.finish().vertices
            };
            let base = render(None);
            let mixing = render(Some(0.5));
            let final_colour = render(Some(1.0));

            assert_eq!(base.len(), mixing.len());
            assert_eq!(base.len(), final_colour.len());
            assert!(base.iter().zip(&mixing).any(|(base, mixing)| {
                base.color[..3]
                    .iter()
                    .zip(mixing.color[..3].iter())
                    .any(|(base, mixing)| (base - mixing).abs() > 0.001)
            }));
            assert!(
                mixing
                    .iter()
                    .zip(&final_colour)
                    .any(|(mixing, final_colour)| {
                        mixing.color[..3]
                            .iter()
                            .zip(final_colour.color[..3].iter())
                            .any(|(mixing, final_colour)| (mixing - final_colour).abs() > 0.001)
                    })
            );
            for (base, final_colour) in base.iter().zip(&final_colour) {
                assert!((base.color[3] - final_colour.color[3]).abs() < f32::EPSILON);
                assert!((final_colour.color[0] - f32::from(target.red) / 255.0).abs() < 0.000_01);
                assert!((final_colour.color[1] - f32::from(target.green) / 255.0).abs() < 0.000_01);
                assert!((final_colour.color[2] - f32::from(target.blue) / 255.0).abs() < 0.000_01);
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
        let first = build_scene(&plan, 3, 0.5, &[], None);
        let second = build_scene(&plan, 3, 0.5, &[], None);
        assert_eq!(
            bytemuck::cast_slice::<Vertex, u8>(&first.vertices),
            bytemuck::cast_slice::<Vertex, u8>(&second.vertices)
        );
        assert_eq!(first.indices, second.indices);
        assert!(first.opaque_end > 0, "the scene must contain opaque depth geometry");
        assert!(
            usize::try_from(first.glass_end).is_ok_and(|count| count <= first.indices.len())
                && first.glass_end > first.opaque_end,
            "glass, liquid, and effects must remain in the transparent pass"
        );
        assert!(
            usize::try_from(first.emissive_end).is_ok_and(|count| count <= first.indices.len()),
            "the additive pass boundary must remain inside the batched index buffer"
        );
        assert!(first.vertices.iter().any(|vertex| vertex.position[2].abs() > 0.1));
        assert!(
            first.vertices.len() > 100,
            "diorama should include reusable scene assets"
        );
    }

    #[test]
    fn reviewed_if7_graph_renders_distinct_atoms_and_bonds_in_3d() {
        let request = chemistry::ReactionRequest::from_id("covalent-i-f-if7")
            .expect("reviewed IF7 request exists");
        let plan = plan_for(request);
        let preview = request
            .product_preview()
            .expect("reviewed IF7 graph exists");
        let layout = molecular_layout(&preview);
        let iodine = preview
            .atoms
            .iter()
            .position(|atom| atom.atomic_number == 53)
            .expect("IF7 contains iodine");
        assert_eq!(layout.positions.len(), 8);
        assert!(
            preview
                .covalent_bonds()
                .iter()
                .all(|bond| { bond.start == iodine || bond.end == iodine })
        );

        let final_ordinal = plan.timeline.beats.last().unwrap().end_ordinal;
        let scene = build_scene(
            &plan,
            final_ordinal,
            1.0,
            &request.reactant_previews(),
            Some(&preview),
        );
        for color in [molecular_atom_color(9), molecular_atom_color(53)] {
            assert!(scene.vertices.iter().any(|vertex| {
                vertex
                    .color
                    .iter()
                    .zip(color)
                    .all(|(actual, expected)| (actual - expected).abs() < f32::EPSILON)
            }));
        }
    }

    #[test]
    fn vsepr_layout_bends_water_and_keeps_carbon_dioxide_linear() {
        let angle_at = |layout: &MolecularLayout, apex: usize, left: usize, right: usize| {
            let first = (layout.positions[left] - layout.positions[apex]).normalize();
            let second = (layout.positions[right] - layout.positions[apex]).normalize();
            first.dot(second).clamp(-1.0, 1.0).acos().to_degrees()
        };

        let water = crate::composition_catalogue::trusted_preview([8, 1, 1])
            .expect("water preview resolves");
        let oxygen = water
            .atoms
            .iter()
            .position(|atom| atom.atomic_number == 8)
            .expect("water contains oxygen");
        let hydrogens: Vec<usize> = water
            .atoms
            .iter()
            .enumerate()
            .filter_map(|(index, atom)| (atom.atomic_number == 1).then_some(index))
            .collect();
        let layout = molecular_layout(&water);
        let bend = angle_at(&layout, oxygen, hydrogens[0], hydrogens[1]);
        assert!(
            (100.0..116.0).contains(&bend),
            "water must bend near the tetrahedral angle, got {bend}"
        );
        let bond_lengths: Vec<f32> = hydrogens
            .iter()
            .map(|hydrogen| (layout.positions[*hydrogen] - layout.positions[oxygen]).length())
            .collect();
        assert!((bond_lengths[0] - bond_lengths[1]).abs() < 0.001);

        let carbon_dioxide = crate::composition_catalogue::trusted_preview([6, 8, 8])
            .expect("carbon dioxide preview resolves");
        let carbon = carbon_dioxide
            .atoms
            .iter()
            .position(|atom| atom.atomic_number == 6)
            .expect("CO2 contains carbon");
        let oxygens: Vec<usize> = carbon_dioxide
            .atoms
            .iter()
            .enumerate()
            .filter_map(|(index, atom)| (atom.atomic_number == 8).then_some(index))
            .collect();
        let layout = molecular_layout(&carbon_dioxide);
        let spread = angle_at(&layout, carbon, oxygens[0], oxygens[1]);
        assert!(spread > 175.0, "CO2 must stay linear, got {spread}");
    }

    #[test]
    fn benzene_lays_out_as_a_planar_hexagon_with_radial_hydrogens() {
        let benzene =
            crate::composition_catalogue::trusted_preview([6, 6, 6, 6, 6, 6, 1, 1, 1, 1, 1, 1])
                .expect("benzene preview resolves");
        let carbons: Vec<usize> = benzene
            .atoms
            .iter()
            .enumerate()
            .filter_map(|(index, atom)| (atom.atomic_number == 6).then_some(index))
            .collect();
        let hydrogens: Vec<usize> = benzene
            .atoms
            .iter()
            .enumerate()
            .filter_map(|(index, atom)| (atom.atomic_number == 1).then_some(index))
            .collect();
        assert_eq!(carbons.len(), 6);
        assert_eq!(hydrogens.len(), 6);

        let layout = molecular_layout(&benzene);
        // The whole molecule is coplanar: sp2 ring carbons keep their
        // hydrogens in the ring plane.
        let plane_y = layout.positions[carbons[0]].y;
        for position in &layout.positions {
            assert!((position.y - plane_y).abs() < 0.001);
        }
        // Regular hexagon: equal radii from the ring centre and equal
        // bonded C-C distances.
        let ring_center = carbons
            .iter()
            .map(|carbon| layout.positions[*carbon])
            .sum::<Vec3>()
            / 6.0;
        let radii: Vec<f32> = carbons
            .iter()
            .map(|carbon| (layout.positions[*carbon] - ring_center).length())
            .collect();
        for radius in &radii {
            assert!((radius - radii[0]).abs() < 0.001);
        }
        let ring_bonds: Vec<f32> = benzene
            .covalent_bonds()
            .iter()
            .filter(|bond| carbons.contains(&bond.start) && carbons.contains(&bond.end))
            .map(|bond| (layout.positions[bond.start] - layout.positions[bond.end]).length())
            .collect();
        assert_eq!(ring_bonds.len(), 6, "the Kekulé ring closes with six C-C bonds");
        for length in &ring_bonds {
            assert!((length - ring_bonds[0]).abs() < 0.001);
        }
        // Each hydrogen bonds to exactly one carbon and points radially
        // outward from it.
        for hydrogen in &hydrogens {
            let bonds: Vec<_> = benzene
                .covalent_bonds()
                .iter()
                .filter(|bond| bond.start == *hydrogen || bond.end == *hydrogen)
                .collect();
            assert_eq!(bonds.len(), 1);
            let carbon = if bonds[0].start == *hydrogen {
                bonds[0].end
            } else {
                bonds[0].start
            };
            assert!(carbons.contains(&carbon));
            let outward =
                (layout.positions[*hydrogen] - layout.positions[carbon]).normalize();
            let radial = (layout.positions[carbon] - ring_center).normalize();
            assert!(
                outward.dot(radial) > 0.999,
                "hydrogen must extend the ring radius, got dot {}",
                outward.dot(radial)
            );
        }
    }

    #[test]
    fn sulfur_crown_keeps_all_eight_atoms_evenly_on_one_ring() {
        let sulfur = crate::composition_catalogue::trusted_preview([16; 8])
            .expect("S8 preview resolves");
        assert_eq!(sulfur.atoms.len(), 8);
        let layout = molecular_layout(&sulfur);
        let center = layout.positions.iter().copied().sum::<Vec3>() / 8.0;
        let radii: Vec<f32> = layout
            .positions
            .iter()
            .map(|position| (*position - center).length())
            .collect();
        for radius in &radii {
            assert!((radius - radii[0]).abs() < 0.001, "octagon atoms share one radius");
        }
        let bond_lengths: Vec<f32> = sulfur
            .covalent_bonds()
            .iter()
            .map(|bond| (layout.positions[bond.start] - layout.positions[bond.end]).length())
            .collect();
        assert_eq!(bond_lengths.len(), 8, "the S8 ring closes with eight bonds");
        for length in &bond_lengths {
            assert!((length - bond_lengths[0]).abs() < 0.001);
        }
        // Bonded neighbours sit on adjacent polygon slots: every bond spans
        // one octagon edge, never a chord across the ring.
        let edge = 2.0 * radii[0] * (std::f32::consts::PI / 8.0).sin();
        for length in &bond_lengths {
            assert!((length - edge).abs() < 0.001);
        }
    }

    #[test]
    fn budget_overflow_sheds_the_tail_instead_of_blanking_the_scene() {
        let in_budget = budget_prefix(1_000, &[0, 1, 2, 3, 4, 5]);
        assert_eq!(in_budget, (1_000, 6));

        let vertex_budget = usize::try_from(MAX_VERTICES).expect("budget fits usize");
        let over = vertex_budget + 10;
        let overflow_vertex = u32::try_from(vertex_budget).expect("budget fits u32");
        // Two triangles inside the budget, then one referencing overflowing
        // vertices: the prefix keeps the first two and stays triangle-aligned.
        let indices = [0, 1, 2, 3, 4, 5, overflow_vertex, 6, 7];
        let (vertices, prefix) = budget_prefix(over, &indices);
        assert_eq!(vertices, vertex_budget);
        assert_eq!(prefix, 6, "the cut must land on a whole triangle");
    }

    #[test]
    fn molecular_model_renders_only_in_the_inset_batch() {
        let request = chemistry::ReactionRequest::from_id("covalent-i-f-if7")
            .expect("reviewed IF7 request exists");
        let plan = plan_for(request);
        let final_ordinal = plan.timeline.beats.last().unwrap().end_ordinal;
        let preview = request.product_preview().expect("IF7 preview exists");

        let without_previews = build_scene(&plan, final_ordinal, 1.0, &[], None);
        assert_eq!(
            without_previews.emissive_end, without_previews.inset_end,
            "no reviewed preview means no inset geometry"
        );

        let with_preview = build_scene(&plan, final_ordinal, 1.0, &[], Some(&preview));
        assert!(
            with_preview.inset_end > with_preview.emissive_end,
            "the reviewed product preview populates the inset batch"
        );
        // The macroscopic batches are identical with and without the preview:
        // molecules never appear inside the observable scene.
        assert_eq!(with_preview.emissive_end, without_previews.emissive_end);
        assert!(
            active_molecular_preview(&plan, 0, &[], Some(&preview)).is_none(),
            "the product model stays hidden before its visibility ordinal"
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
        let before = build_scene(&plan, 0, 0.5, &[], None);
        let reacting_ordinal = plan
            .effects
            .iter()
            .map(|effect| effect.start_ordinal)
            .min()
            .expect("alkali-water profile has observation-backed effects");
        let reacting = build_scene(&plan, reacting_ordinal, 0.5, &[], None);
        assert!(reacting.vertices.len() > before.vertices.len());
        assert!(plan.effects.iter().any(|effect| {
            effect.effect == EffectProfile::SurfaceDisturbance
                || effect.effect == EffectProfile::SplashEmitter
        }));
        let camera = fixed_camera_pose(&plan);
        assert!(camera.pitch < -0.5);
        assert_eq!(camera, fixed_camera_pose(&plan));
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
        let potassium_mesh = build_scene(&potassium, flame.start_ordinal, 0.5, &[], None);
        assert!(
            potassium_mesh.glass_end < potassium_mesh.emissive_end,
            "emissive cores and sparks use the final additive batch"
        );

        let lithium = canonical_plan();
        assert!(
            !lithium
                .effects
                .iter()
                .any(|effect| matches!(effect.effect, EffectProfile::FlameEmitter(_)))
        );
        let lithium_mesh = build_scene(&lithium, flame.start_ordinal, 0.5, &[], None);
        assert_eq!(lithium_mesh.glass_end, lithium_mesh.emissive_end);
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
            let before = build_scene(&plan, transition.start_ordinal, 0.0, &[], None);
            let visible = build_scene(&plan, transition.start_ordinal, 1.0, &[], None);

            let has_expected_colour = |vertex: &Vertex| {
                vertex
                    .color
                    .iter()
                    .zip(expected)
                    .all(|(actual, expected)| (actual - expected).abs() < f32::EPSILON)
            };
            assert!(!before.vertices.iter().any(has_expected_colour));
            assert!(visible.vertices.iter().any(has_expected_colour));
        }
    }

    #[test]
    fn effect_free_halogen_displacement_does_not_invent_liquid_or_phase_effects() {
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
        assert!(plan.effects.is_empty());
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

        let before = build_scene(
            &plan,
            mixing.start_ordinal.saturating_sub(1),
            0.5,
            &[],
            None,
        );
        let active = build_scene(&plan, mixing.start_ordinal, 0.5, &[], None);
        assert!(active.vertices.len() > before.vertices.len());
        assert!(active.indices.len() > before.indices.len());

        let colourless = appearance_color(AppearanceProfile::AqueousColourless);
        assert!((colourless[2] - colourless[0]).abs() < 0.12);
        assert!(colourless[3] < appearance_color(AppearanceProfile::Water)[3]);
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
        let vessel_scale = transform_scale(&vessel.transform);
        let vessel_base = layout.vessel_center.y - 0.55 * vessel_scale.y;
        let vessel_rim = layout.vessel_center.y + 0.95 * vessel_scale.y;

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
            None,
        );
        let unrotated = unrotated_meshes.finish().vertices;
        let rotated = rotated_meshes.finish().vertices;

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
            None,
        );
        let batches = meshes.finish();

        assert!(batches.opaque_end > 0, "the floor remains opaque geometry");
        assert!(
            batches.vertices.iter().all(|vertex| vertex.position[1] < 0.0),
            "the environment must not add a vertical wall above the floor"
        );
    }
}
