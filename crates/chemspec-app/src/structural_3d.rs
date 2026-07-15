//! Depth-tested low-poly rendering of reviewed macroscopic scene plans.
//!
//! The renderer resolves reusable assets and effects only. It does not inspect
//! structural atoms, bonds, source, or catalogue rules.

use bytemuck::{Pod, Zeroable};
use chem_presentation::{
    AppearanceProfile, AssetProfile, CameraBehaviour, EffectIntensity, EffectProfile,
    PresentationEffect, PresentationObject, PresentationTransform, SceneRole,
};
use chem_presentation::{RealWorldPosition, ScenePlan};
use glam::{EulerRot, Mat4, Quat, Vec3};
use iced::mouse;
use iced::widget::shader::{self, Action, Program};
use iced::{Rectangle, wgpu};

use crate::scene_registry::{self, AssetGeometry, EffectDynamics, EffectGeometry};

const MAX_VERTICES: u64 = 32_768;
const MAX_INDICES: u64 = 98_304;

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

#[derive(Debug)]
pub struct CameraState {
    yaw_offset: f32,
    pitch_offset: f32,
    zoom_offset: f32,
    dragging: bool,
    cursor: Option<iced::Point>,
}

impl Default for CameraState {
    fn default() -> Self {
        Self {
            yaw_offset: 0.0,
            pitch_offset: 0.0,
            zoom_offset: 0.0,
            dragging: false,
            cursor: None,
        }
    }
}

impl<Message> Program<Message> for Scene {
    type State = CameraState;
    type Primitive = ScenePrimitive;

    fn update(
        &self,
        state: &mut Self::State,
        event: &iced::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<Message>> {
        match event {
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if cursor.is_over(bounds) =>
            {
                state.dragging = true;
                state.cursor = cursor.position();
                Some(Action::capture())
            }
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if state.dragging =>
            {
                state.dragging = false;
                state.cursor = None;
                Some(Action::capture())
            }
            iced::Event::Mouse(mouse::Event::CursorMoved { position }) if state.dragging => {
                if let Some(previous) = state.cursor {
                    state.yaw_offset += (position.x - previous.x) * 0.009;
                    state.pitch_offset =
                        (state.pitch_offset + (position.y - previous.y) * 0.009).clamp(-0.7, 0.7);
                }
                state.cursor = Some(*position);
                Some(Action::request_redraw().and_capture())
            }
            iced::Event::Mouse(mouse::Event::WheelScrolled { delta }) if cursor.is_over(bounds) => {
                let amount = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => *y * 0.35,
                    mouse::ScrollDelta::Pixels { y, .. } => *y * 0.008,
                };
                state.zoom_offset = (state.zoom_offset - amount).clamp(-2.0, 2.0);
                Some(Action::request_redraw().and_capture())
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        state: &Self::State,
        _cursor: mouse::Cursor,
        _bounds: Rectangle,
    ) -> Self::Primitive {
        let (vertices, indices, opaque_index_count) = build_scene(
            &self.plan,
            self.moment.ordinal,
            self.moment.ordinal_progress,
        );
        let (yaw, pitch, zoom) = camera_pose(&self.plan, self.moment);
        let focus_target = SceneLayout::resolve(&self.plan).camera_target;
        ScenePrimitive {
            vertices,
            indices,
            opaque_index_count,
            yaw: yaw + state.yaw_offset,
            pitch: pitch + state.pitch_offset,
            zoom: zoom + state.zoom_offset,
            focus_target,
        }
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.dragging {
            mouse::Interaction::Grabbing
        } else if cursor.is_over(bounds) {
            mouse::Interaction::Grab
        } else {
            mouse::Interaction::default()
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
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
    opaque_index_count: u32,
    yaw: f32,
    pitch: f32,
    zoom: f32,
    focus_target: Vec3,
}

#[derive(Debug)]
pub struct ScenePipeline {
    opaque_pipeline: wgpu::RenderPipeline,
    transparent_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    depth: Option<DepthTarget>,
    opaque_index_count: u32,
    index_count: u32,
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
                               cull_mode: Option<wgpu::Face>| {
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
                    entry_point: Some("fragment"),
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
        );
        let transparent_pipeline = create_pipeline(
            "chemspec structural 3d transparent pipeline",
            Some(wgpu::BlendState::ALPHA_BLENDING),
            false,
            None,
        );
        Self {
            opaque_pipeline,
            transparent_pipeline,
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
            uniform_buffer,
            bind_group,
            depth: None,
            opaque_index_count: 0,
            index_count: 0,
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
            pipeline.index_count = 0;
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
        pipeline.index_count = u32::try_from(self.indices.len()).unwrap_or(u32::MAX);
        pipeline.opaque_index_count = self.opaque_index_count.min(pipeline.index_count);

        let aspect = width as f32 / height.max(1) as f32;
        let reaction_target = self.focus_target;
        let pitch = self.pitch.clamp(-1.18, -0.22);
        let eye = reaction_target
            + Quat::from_rotation_y(self.yaw)
                * Quat::from_rotation_x(pitch)
                * Vec3::new(0.0, 0.0, self.zoom);
        let view = Mat4::look_at_rh(eye, reaction_target, Vec3::Y);
        let projection = Mat4::perspective_rh(0.66, aspect, 0.1, 50.0);
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
        if pipeline.opaque_index_count < pipeline.index_count {
            pass.set_pipeline(&pipeline.transparent_pipeline);
            pass.draw_indexed(pipeline.opaque_index_count..pipeline.index_count, 0, 0..1);
        }
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
}

#[derive(Debug, Clone, Copy)]
struct ObjectMotion {
    translation: Vec3,
    rotation: Quat,
}

impl Default for ObjectMotion {
    fn default() -> Self {
        Self {
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
        }
    }
}

fn build_scene(plan: &ScenePlan, ordinal: u16, progress: f32) -> (Vec<Vertex>, Vec<u32>, u32) {
    let mut meshes = SceneMeshes::default();
    let layout = SceneLayout::resolve(plan);
    let reaction_motion = reaction_surface_motion(plan, ordinal, progress);
    let animated_layout = layout.with_reaction_motion(reaction_motion);
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
    );
    for object in &plan.objects {
        if object.visible_from_ordinal <= ordinal {
            let shrink = object_scale_from_effects(plan, object.role, ordinal, progress);
            let motion = object_motion(plan, object, ordinal, progress, reaction_motion);
            instantiate_asset(
                &mut meshes,
                object.asset,
                object.appearance,
                &object.transform,
                shrink,
                layout.object_offset(object) + motion.translation,
                motion.rotation,
                stable_seed(&object.id),
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
            );
        }
    }
    meshes.finish()
}

fn camera_pose(plan: &ScenePlan, moment: RealWorldPosition) -> (f32, f32, f32) {
    let behaviour = plan
        .timeline
        .beats
        .get(moment.beat_index)
        .map_or(CameraBehaviour::WideEstablishingShot, |beat| {
            beat.camera.behaviour
        });
    let pose = scene_registry::camera_pose(behaviour);
    let next_behaviour = plan
        .timeline
        .beats
        .get(moment.beat_index + 1)
        .map_or(behaviour, |beat| beat.camera.behaviour);
    let next = scene_registry::camera_pose(next_behaviour);
    let progress = moment.beat_progress.clamp(0.0, 1.0);
    let eased = smoother_step(progress);
    let phase = continuous_phase(moment.ordinal, moment.ordinal_progress);
    let energy = active_camera_energy(plan, moment.ordinal, moment.ordinal_progress);
    let beat_breath = (std::f32::consts::PI * progress).sin();
    let yaw_drift = (phase * 0.37 + seed_phase(plan_seed(plan), 1)).sin()
        * (0.004 + energy * 0.014)
        * beat_breath;
    let pitch_drift = (phase * 0.29 + seed_phase(plan_seed(plan), 2)).cos()
        * (0.002 + energy * 0.008)
        * beat_breath;
    let focus_breath = (phase * 0.22 + seed_phase(plan_seed(plan), 3)).sin()
        * (0.018 + energy * 0.055)
        * beat_breath;
    (
        pose.yaw + (next.yaw - pose.yaw) * eased + yaw_drift,
        pose.pitch + (next.pitch - pose.pitch) * eased + pitch_drift,
        pose.zoom + (next.zoom - pose.zoom) * eased + focus_breath,
    )
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
            let extent = smoother_step((elapsed / span).clamp(0.0, 1.0));
            if grows {
                0.12 + extent * 0.88
            } else {
                (1.0 - 0.76 * extent).max(0.20)
            }
        })
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
    let local_progress = if ordinal == object.visible_from_ordinal {
        progress.clamp(0.0, 1.0)
    } else {
        1.0
    };
    let introduction = if ordinal == object.visible_from_ordinal {
        let arrival = ease_out_back(local_progress);
        let remainder = 1.0 - arrival;
        let settle =
            (local_progress * std::f32::consts::PI * 3.0).sin() * (1.0 - local_progress).powi(2);
        Vec3::new(
            -0.48 * remainder,
            0.78 * remainder + settle * 0.055,
            0.22 * remainder,
        )
    } else {
        Vec3::ZERO
    };
    let phase = continuous_phase(ordinal, progress);
    let activity = reaction_motion.length().min(1.0);
    let roll = (phase * 0.91 + seed_phase(seed, 5)).sin() * 0.045 * activity;
    let pitch = (phase * 0.67 + seed_phase(seed, 6)).cos() * 0.025 * activity;
    let arrival_yaw = (1.0 - smoother_step(local_progress)) * 0.24;
    ObjectMotion {
        translation: introduction + reaction_motion,
        rotation: Quat::from_euler(EulerRot::XYZ, pitch, arrival_yaw, roll),
    }
}

fn smoother_step(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * value * (value * (value * 6.0 - 15.0) + 10.0)
}

fn ease_out_back(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    let shifted = value - 1.0;
    let overshoot = 1.18;
    shifted * shifted * ((overshoot + 1.0) * shifted + overshoot) + 1.0
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
        EffectProfile::SplashEmitter => 0x8538_ec85_5c19_1b69,
        EffectProfile::ObjectShrinkage => 0xda94_2042_e4dd_58b5,
        EffectProfile::PrecipitateFormation => 0xa409_3822_299f_31d0,
        EffectProfile::Clouding => 0x082e_fa98_ec4e_6c89,
        EffectProfile::ColourTransition => 0x4528_21e6_38d0_1377,
        EffectProfile::HeatDistortion => 0xbe54_66cf_34e9_0c6c,
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
        smoother_step(progress / dynamics.fade_in)
    };
    let release = if dynamics.fade_out <= f32::EPSILON {
        1.0
    } else {
        1.0 - smoother_step((progress - (1.0 - dynamics.fade_out)) / dynamics.fade_out)
    };
    attack * release
}

fn active_camera_energy(plan: &ScenePlan, ordinal: u16, progress: f32) -> f32 {
    plan.effects
        .iter()
        .filter(|effect| effect.start_ordinal <= ordinal && ordinal <= effect.end_ordinal)
        .map(|effect| {
            let dynamics = scene_registry::effect_dynamics(effect.effect, effect.intensity);
            dynamics.camera_energy
                * effect_envelope(dynamics, effect_progress(effect, ordinal, progress))
        })
        .sum::<f32>()
        .min(0.65)
}

fn reaction_surface_motion(plan: &ScenePlan, ordinal: u16, progress: f32) -> Vec3 {
    plan.effects
        .iter()
        .filter(|effect| {
            effect.effect == EffectProfile::SurfaceDisturbance
                && effect.start_ordinal <= ordinal
                && ordinal <= effect.end_ordinal
        })
        .fold(Vec3::ZERO, |motion, effect| {
            let dynamics = scene_registry::effect_dynamics(effect.effect, effect.intensity);
            let envelope = effect_envelope(dynamics, effect_progress(effect, ordinal, progress));
            let phase = continuous_phase(ordinal, progress) * dynamics.rate * std::f32::consts::TAU;
            let seed = effect_seed(plan, effect);
            let x = ((phase + seed_phase(seed, 11)).sin()
                + (phase * 2.37 + seed_phase(seed, 12)).sin() * 0.34)
                * dynamics.spread
                * 0.27;
            let z = ((phase * 0.83 + seed_phase(seed, 13)).cos()
                + (phase * 1.91 + seed_phase(seed, 14)).sin() * 0.29)
                * dynamics.spread
                * 0.21;
            let y =
                (0.5 + 0.5 * (phase * 1.43 + seed_phase(seed, 15)).sin()) * dynamics.lift * 0.055;
            motion + Vec3::new(x, y, z) * envelope
        })
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
            add_disc(
                &mut meshes.translucent,
                position + Vec3::new(0.0, 0.148, 0.0),
                1.30,
                [0.01, 0.02, 0.025, 0.22],
            );
        }
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
            let bottom = position + Vec3::new(0.0, -0.52 * scale.y, 0.0);
            let top = position + Vec3::new(0.0, 0.54 * scale.y, 0.0);
            add_cylinder(&mut meshes.translucent, bottom, top, 0.82 * scale.x, color);
            add_disc(
                &mut meshes.translucent,
                top,
                0.82 * scale.x,
                [0.32, 0.62, 0.76, 0.48],
            );
            add_ring(
                &mut meshes.translucent,
                top + Vec3::new(0.0, 0.008, 0.0),
                0.79 * scale.x,
                0.014,
                [0.58, 0.82, 0.92, 0.46],
            );
            add_disc(&mut meshes.translucent, bottom, 0.82 * scale.x, color);
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
        AssetGeometry::ParticleCluster => {
            add_particle_cluster(&mut meshes.opaque, position, scale, color, 18);
        }
        AssetGeometry::GasCluster => {
            add_particle_cluster(&mut meshes.translucent, position, scale, color, 12);
        }
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

#[allow(clippy::similar_names, clippy::too_many_lines)]
fn instantiate_effect(
    meshes: &mut SceneMeshes,
    effect: &PresentationEffect,
    ordinal: u16,
    progress: f32,
    layout: SceneLayout,
    seed: u64,
) {
    let dynamics = scene_registry::effect_dynamics(effect.effect, effect.intensity);
    let effect_progress = effect_progress(effect, ordinal, progress);
    let envelope = effect_envelope(dynamics, effect_progress);
    let phase = continuous_phase(ordinal, progress);
    let count = dynamics.particle_count;
    let surface_point = layout.reaction_point;
    match scene_registry::effect_geometry(effect.effect) {
        EffectGeometry::ParticleCloud => {
            for index in 0..count {
                let index = u32::from(index);
                let birth = seeded_unit(seed, index, 1) * 0.72;
                let born = smoother_step((effect_progress - birth) / 0.24);
                if born <= f32::EPSILON {
                    continue;
                }
                let angle = seeded_unit(seed, index, 2) * std::f32::consts::TAU;
                let radius = seeded_unit(seed, index, 3).sqrt() * dynamics.spread;
                let target = layout.liquid_center
                    + Vec3::new(
                        angle.cos() * radius,
                        -0.34 + seeded_unit(seed, index, 4) * 0.26,
                        angle.sin() * radius,
                    );
                let drift = Vec3::new(
                    (phase * 0.43 + seed_phase(seed ^ u64::from(index), 5)).sin(),
                    0.0,
                    (phase * 0.37 + seed_phase(seed ^ u64::from(index), 6)).cos(),
                ) * dynamics.turbulence
                    * 0.08
                    * born;
                let point = surface_point.lerp(target, born) + drift;
                add_sphere(
                    &mut meshes.translucent,
                    point,
                    (0.025 + seeded_unit(seed, index, 7) * 0.035) * born,
                    alpha([0.94, 0.96, 1.0, 0.82], envelope * born),
                    5,
                    7,
                );
            }
        }
        EffectGeometry::RisingBubbles => {
            for index in 0..count {
                let index = u32::from(index);
                let speed = 0.76 + seeded_unit(seed, index, 1) * 0.62;
                let cycle = (phase * dynamics.rate * speed + seeded_unit(seed, index, 2)).fract();
                let lifecycle = (std::f32::consts::PI * cycle).sin().sqrt() * envelope;
                let angle = seeded_unit(seed, index, 3) * std::f32::consts::TAU;
                let radial = seeded_unit(seed, index, 4).sqrt() * dynamics.spread;
                let wobble = (phase * (1.1 + speed) + seed_phase(seed ^ u64::from(index), 5)).sin()
                    * dynamics.turbulence
                    * 0.16;
                let point = surface_point
                    + Vec3::new(
                        angle.cos() * radial + wobble,
                        -0.42 + smoother_step(cycle) * (0.46 + dynamics.lift),
                        angle.sin() * radial - wobble * 0.7,
                    );
                add_sphere(
                    &mut meshes.translucent,
                    point,
                    0.025 + seeded_unit(seed, index, 6) * 0.045,
                    alpha([0.58, 0.80, 0.90, 0.28], lifecycle),
                    5,
                    7,
                );
            }
        }
        EffectGeometry::EscapingGas => {
            for index in 0..count {
                let index = u32::from(index);
                let speed = 0.72 + seeded_unit(seed, index, 1) * 0.56;
                let cycle = (phase * dynamics.rate * speed + seeded_unit(seed, index, 2)).fract();
                let angle = seeded_unit(seed, index, 3) * std::f32::consts::TAU
                    + phase * dynamics.turbulence * 0.23;
                let spread = dynamics.spread * (0.14 + smoother_step(cycle) * 0.62);
                let curl = (phase * 0.61 + seed_phase(seed ^ u64::from(index), 4)).sin()
                    * dynamics.turbulence
                    * cycle;
                let point = surface_point
                    + Vec3::new(
                        angle.cos() * spread + curl * 0.16,
                        0.04 + smoother_step(cycle) * dynamics.lift,
                        angle.sin() * spread - curl * 0.11,
                    );
                add_sphere(
                    &mut meshes.translucent,
                    point,
                    0.018 + seeded_unit(seed, index, 5) * 0.025,
                    alpha(
                        [0.72, 0.86, 0.92, 0.14],
                        envelope * (1.0 - smoother_step(cycle)),
                    ),
                    4,
                    6,
                );
            }
        }
        EffectGeometry::SurfaceRipples => {
            for ring in 0..count.min(7) {
                let ring = u32::from(ring);
                let cycle = (phase * dynamics.rate + seeded_unit(seed, ring, 1)).fract();
                let ring_alpha = envelope * (1.0 - smoother_step(cycle)).powi(2);
                add_ring(
                    &mut meshes.translucent,
                    surface_point + Vec3::new(0.0, 0.012, 0.0),
                    0.10 + smoother_step(cycle) * dynamics.spread,
                    0.008 + (1.0 - cycle) * 0.008,
                    alpha([0.50, 0.77, 0.90, 0.42], ring_alpha),
                );
            }
        }
        EffectGeometry::SplashDroplets => {
            for index in 0..count {
                let index = u32::from(index);
                let speed = 0.82 + seeded_unit(seed, index, 1) * 0.48;
                let cycle = (phase * dynamics.rate * speed + seeded_unit(seed, index, 2)).fract();
                let angle = seeded_unit(seed, index, 3) * std::f32::consts::TAU;
                let distance = smoother_step(cycle)
                    * dynamics.spread
                    * (0.44 + seeded_unit(seed, index, 4) * 0.56);
                let arc = (std::f32::consts::PI * cycle).sin()
                    * dynamics.lift
                    * (0.56 + seeded_unit(seed, index, 5) * 0.44);
                let lifecycle = (std::f32::consts::PI * cycle).sin().sqrt() * envelope;
                let point = surface_point
                    + Vec3::new(angle.cos() * distance, 0.02 + arc, angle.sin() * distance);
                add_sphere(
                    &mut meshes.translucent,
                    point,
                    0.018 + seeded_unit(seed, index, 6) * 0.025,
                    alpha([0.42, 0.72, 0.88, 0.62], lifecycle),
                    4,
                    6,
                );
            }
        }
        EffectGeometry::PresentationOnly => {}
    }
}

fn alpha(mut color: [f32; 4], factor: f32) -> [f32; 4] {
    color[3] *= factor.clamp(0.0, 1.0);
    color
}

fn appearance_color(profile: AppearanceProfile) -> [f32; 4] {
    match profile {
        AppearanceProfile::LaboratoryNeutral => [0.16, 0.20, 0.23, 1.0],
        AppearanceProfile::ClearGlass => [0.46, 0.70, 0.82, 0.09],
        AppearanceProfile::Water | AppearanceProfile::AqueousColourless => [0.36, 0.62, 0.74, 0.28],
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
}

impl SceneMeshes {
    fn finish(self) -> (Vec<Vertex>, Vec<u32>, u32) {
        let mut vertices = Vec::with_capacity(
            self.opaque.vertices.len()
                + self.translucent.vertices.len()
                + self.glass.vertices.len(),
        );
        let mut indices = Vec::with_capacity(
            self.opaque.indices.len() + self.translucent.indices.len() + self.glass.indices.len(),
        );
        append_mesh(&mut vertices, &mut indices, self.opaque);
        let opaque_index_count = u32::try_from(indices.len()).unwrap_or(u32::MAX);
        append_mesh(&mut vertices, &mut indices, self.translucent);
        append_mesh(&mut vertices, &mut indices, self.glass);
        (vertices, indices, opaque_index_count)
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

fn add_particle_cluster(mesh: &mut Mesh, center: Vec3, scale: Vec3, color: [f32; 4], count: u8) {
    for index in 0..count {
        let angle = f32::from(index) * 2.399_963_1;
        let radius = (f32::from(index) / f32::from(count.max(1))).sqrt();
        let offset = Vec3::new(
            angle.cos() * radius * scale.x,
            f32::from((index * 11) % 9) / 9.0 * scale.y,
            angle.sin() * radius * scale.z,
        );
        add_sphere(
            mesh,
            center + offset,
            0.045 + f32::from(index % 4) * 0.012,
            color,
            5,
            7,
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
        assert!(first.0.iter().any(|vertex| vertex.position[2].abs() > 0.1));
        assert!(
            first.0.len() > 100,
            "diorama should include reusable scene assets"
        );
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
        assert!(reacting.0.len() > before.0.len());
        assert!(plan.effects.iter().any(|effect| {
            effect.effect == EffectProfile::SurfaceDisturbance
                || effect.effect == EffectProfile::SplashEmitter
        }));
        let moment = plan.timeline.locate(0).expect("timeline begins");
        let (_, pitch, _) = camera_pose(&plan, moment);
        assert!(pitch < -0.5);
    }

    #[test]
    fn bromide_and_iodide_precipitates_render_only_at_their_trusted_colours() {
        for (halogen, appearance) in [
            (
                chemistry::Halogen::Bromine,
                AppearanceProfile::CreamPrecipitate,
            ),
            (
                chemistry::Halogen::Iodine,
                AppearanceProfile::YellowPrecipitate,
            ),
        ] {
            let plan = plan_for(chemistry::ReactionRequest::silver_halide_precipitation(
                halogen,
            ));
            let product = plan
                .objects
                .iter()
                .find(|object| object.role == SceneRole::Product)
                .expect("precipitate product exists");
            let expected = appearance_color(appearance);
            let before = build_scene(&plan, product.visible_from_ordinal.saturating_sub(1), 0.5);
            let visible = build_scene(&plan, product.visible_from_ordinal, 0.5);

            let has_expected_colour = |vertex: &Vertex| {
                vertex
                    .color
                    .iter()
                    .zip(expected)
                    .all(|(actual, expected)| (actual - expected).abs() < f32::EPSILON)
            };
            assert!(!before.0.iter().any(has_expected_colour));
            assert!(visible.0.iter().any(has_expected_colour));
        }
    }

    #[test]
    fn effect_free_families_render_one_liquid_volume() {
        for (request, expected_liquids) in [
            (
                chemistry::ReactionRequest::acid_base_neutralization(
                    chemistry::AlkaliMetal::Sodium,
                    chemistry::Halogen::Chlorine,
                ),
                1,
            ),
            (
                chemistry::ReactionRequest::ALL
                    .iter()
                    .copied()
                    .find(|request| {
                        request.family() == chemistry::ReactionFamily::HalogenDisplacement
                    })
                    .expect("a supported halogen displacement exists"),
                0,
            ),
        ] {
            let plan = plan_for(request);
            assert_eq!(
                plan.objects
                    .iter()
                    .filter(|object| object.asset == AssetProfile::LiquidVolume)
                    .count(),
                expected_liquids
            );
            assert!(plan.effects.is_empty());
            let start = build_scene(&plan, 0, 0.5);
            let end = build_scene(
                &plan,
                plan.timeline
                    .beats
                    .last()
                    .expect("timeline has a final beat")
                    .end_ordinal,
                0.5,
            );
            assert_eq!(start.0.len(), end.0.len());
            assert_eq!(start.1.len(), end.1.len());
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
        );
        let (unrotated, _, _) = unrotated_meshes.finish();
        let (rotated, _, _) = rotated_meshes.finish();

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
        );
        let (vertices, _, opaque_indices) = meshes.finish();

        assert!(opaque_indices > 0, "the floor remains opaque geometry");
        assert!(
            vertices.iter().all(|vertex| vertex.position[1] < 0.0),
            "the environment must not add a vertical wall above the floor"
        );
    }
}
