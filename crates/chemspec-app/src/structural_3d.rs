//! Depth-tested low-poly rendering of reviewed macroscopic scene plans.
//!
//! The renderer resolves reusable assets and effects only. It does not inspect
//! structural atoms, bonds, source, or catalogue rules.

use bytemuck::{Pod, Zeroable};
use chem_presentation::{
    AppearanceProfile, AssetProfile, CameraBehaviour, EffectIntensity, EffectProfile,
    PresentationTransform, ScenePlan, SceneRole,
};
use glam::{Mat4, Quat, Vec3};
use iced::mouse;
use iced::widget::shader::{self, Action, Program};
use iced::{Rectangle, wgpu};

use crate::scene_registry::{self, AssetGeometry, EffectGeometry};

const MAX_VERTICES: u64 = 32_768;
const MAX_INDICES: u64 = 98_304;

#[derive(Debug, Clone)]
pub struct Scene {
    plan: ScenePlan,
    ordinal: u16,
    progress: f32,
}

impl Scene {
    pub fn new(plan: &ScenePlan, ordinal: u16, progress: f32) -> Self {
        Self {
            plan: plan.clone(),
            ordinal,
            progress: progress.clamp(0.0, 1.0),
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
        let (vertices, indices) = build_scene(&self.plan, self.ordinal, self.progress);
        let (yaw, pitch, zoom) = camera_pose(&self.plan, self.ordinal, self.progress);
        ScenePrimitive {
            vertices,
            indices,
            yaw: yaw + state.yaw_offset,
            pitch: pitch + state.pitch_offset,
            zoom: zoom + state.zoom_offset,
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
    yaw: f32,
    pitch: f32,
    zoom: f32,
}

#[derive(Debug)]
pub struct ScenePipeline {
    render_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    depth: Option<DepthTarget>,
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
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("chemspec structural 3d pipeline"),
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
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: Some(wgpu::Face::Back),
                ..wgpu::PrimitiveState::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        Self {
            render_pipeline,
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

        let aspect = width as f32 / height.max(1) as f32;
        let reaction_target = Vec3::new(0.0, 0.56, 0.0);
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
        pass.set_pipeline(&pipeline.render_pipeline);
        pass.set_bind_group(0, &pipeline.bind_group, &[]);
        pass.set_vertex_buffer(0, pipeline.vertex_buffer.slice(..));
        pass.set_index_buffer(pipeline.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..pipeline.index_count, 0, 0..1);
    }
}

fn build_scene(plan: &ScenePlan, ordinal: u16, progress: f32) -> (Vec<Vertex>, Vec<u32>) {
    let mut mesh = Mesh::default();
    instantiate_asset(
        &mut mesh,
        plan.environment,
        AppearanceProfile::LaboratoryNeutral,
        &PresentationTransform {
            translation: [0, -900, 0],
            rotation: [0, 0, 0],
            scale: [1000, 1000, 1000],
        },
        ordinal,
        1.0,
        Vec3::ZERO,
    );
    for object in &plan.objects {
        if object.visible_from_ordinal <= ordinal {
            let shrink = object_scale_from_effects(plan, object.role, ordinal, progress);
            let motion = object_motion(plan, object.role, ordinal, progress);
            instantiate_asset(
                &mut mesh,
                object.asset,
                object.appearance,
                &object.transform,
                ordinal,
                shrink,
                motion,
            );
        }
    }
    for effect in &plan.effects {
        if effect.start_ordinal <= ordinal && ordinal <= effect.end_ordinal {
            instantiate_effect(
                &mut mesh,
                effect.effect,
                effect.intensity,
                ordinal,
                progress,
            );
        }
    }
    (mesh.vertices, mesh.indices)
}

fn camera_pose(plan: &ScenePlan, ordinal: u16, progress: f32) -> (f32, f32, f32) {
    let behaviour = plan
        .camera
        .iter()
        .find(|cue| cue.start_ordinal <= ordinal && ordinal <= cue.end_ordinal)
        .map_or(CameraBehaviour::WideEstablishingShot, |cue| cue.behaviour);
    let pose = scene_registry::camera_pose(behaviour);
    let next_behaviour = plan
        .camera
        .iter()
        .find(|cue| cue.start_ordinal > ordinal)
        .map_or(behaviour, |cue| cue.behaviour);
    let next = scene_registry::camera_pose(next_behaviour);
    let eased = progress * progress * (3.0 - 2.0 * progress);
    (
        pose.yaw + (next.yaw - pose.yaw) * eased,
        pose.pitch + (next.pitch - pose.pitch) * eased,
        pose.zoom + (next.zoom - pose.zoom) * eased,
    )
}

fn object_scale_from_effects(
    plan: &ScenePlan,
    role: SceneRole,
    ordinal: u16,
    progress: f32,
) -> f32 {
    if role != SceneRole::Reactant {
        return 1.0;
    }
    plan.effects
        .iter()
        .find(|effect| {
            effect.effect == EffectProfile::ObjectShrinkage
                && effect.start_ordinal <= ordinal
                && ordinal <= effect.end_ordinal
        })
        .map_or(1.0, |effect| {
            let span = f32::from(
                effect
                    .end_ordinal
                    .saturating_sub(effect.start_ordinal)
                    .max(1),
            );
            let elapsed = f32::from(ordinal.saturating_sub(effect.start_ordinal)) + progress;
            (1.0 - 0.72 * (elapsed / span).clamp(0.0, 1.0)).max(0.24)
        })
}

fn object_motion(plan: &ScenePlan, role: SceneRole, ordinal: u16, progress: f32) -> Vec3 {
    if role != SceneRole::Reactant {
        return Vec3::ZERO;
    }
    let introduction = if ordinal == 0 {
        Vec3::new(-0.42 * (1.0 - progress), 0.72 * (1.0 - progress), 0.18)
    } else {
        Vec3::ZERO
    };
    let disturbed = plan.effects.iter().any(|effect| {
        effect.effect == EffectProfile::SurfaceDisturbance
            && effect.start_ordinal <= ordinal
            && ordinal <= effect.end_ordinal
    });
    if !disturbed {
        return introduction;
    }
    let phase = (f32::from(ordinal) + progress) * 2.2;
    introduction
        + Vec3::new(
            phase.sin() * 0.34,
            phase.mul_add(1.7, 0.0).sin().abs() * 0.035,
            (phase * 0.73).cos() * 0.24,
        )
}

fn instantiate_asset(
    mesh: &mut Mesh,
    asset: AssetProfile,
    appearance: AppearanceProfile,
    transform: &PresentationTransform,
    ordinal: u16,
    scale_multiplier: f32,
    position_offset: Vec3,
) {
    let position = Vec3::new(
        f32::from(transform.translation[0]) / 1_000.0,
        f32::from(transform.translation[1]) / 1_000.0,
        f32::from(transform.translation[2]) / 1_000.0,
    ) + position_offset;
    let scale = Vec3::new(
        f32::from(transform.scale[0]) / 1_000.0,
        f32::from(transform.scale[1]) / 1_000.0,
        f32::from(transform.scale[2]) / 1_000.0,
    ) * scale_multiplier;
    let color = appearance_color(appearance);
    match scene_registry::asset_geometry(asset) {
        AssetGeometry::Bench => {
            add_box(mesh, position, Vec3::new(7.2, 0.28, 5.4) * scale, color);
        }
        AssetGeometry::CylindricalVessel => {
            let bottom = position + Vec3::new(0.0, -0.55 * scale.y, 0.0);
            let top = position + Vec3::new(0.0, 0.95 * scale.y, 0.0);
            let radius = 0.92 * scale.x;
            add_ring(mesh, top, radius, 0.028, [0.78, 0.94, 1.0, 0.78]);
            add_ring(mesh, bottom, radius * 0.96, 0.018, [0.68, 0.88, 1.0, 0.45]);
            for segment in 0_u8..12 {
                let angle = std::f32::consts::TAU * f32::from(segment) / 12.0;
                let edge = Vec3::new(angle.cos() * radius, 0.0, angle.sin() * radius);
                add_cylinder(mesh, bottom + edge * 0.96, top + edge, 0.010, color);
            }
        }
        AssetGeometry::LiquidCylinder => {
            let bottom = position + Vec3::new(0.0, -0.52 * scale.y, 0.0);
            let top = position + Vec3::new(0.0, 0.54 * scale.y, 0.0);
            add_cylinder(mesh, bottom, top, 0.82 * scale.x, color);
            add_disc(mesh, top, 0.82 * scale.x, [0.34, 0.76, 0.96, 0.76]);
            add_disc(mesh, bottom, 0.82 * scale.x, color);
        }
        AssetGeometry::LowPolyChunk => {
            let variation = 1.0 + f32::from(ordinal % 3) * 0.035;
            add_box(
                mesh,
                position,
                Vec3::new(0.7, 0.18, 0.42) * scale * variation,
                color,
            );
        }
        AssetGeometry::ParticleCluster => add_particle_cluster(mesh, position, scale, color, 18),
        AssetGeometry::GasCluster => add_particle_cluster(mesh, position, scale, color, 12),
    }
}

fn instantiate_effect(
    mesh: &mut Mesh,
    effect: EffectProfile,
    intensity: EffectIntensity,
    ordinal: u16,
    progress: f32,
) {
    let count = match intensity {
        EffectIntensity::Subtle => 6,
        EffectIntensity::Moderate => 12,
        EffectIntensity::Strong => 20,
    };
    let phase = f32::from(ordinal) + progress;
    let reaction_point = Vec3::new(
        (phase * 2.2).sin() * 0.34,
        0.66,
        (phase * 1.61).cos() * 0.24,
    );
    match scene_registry::effect_geometry(effect) {
        EffectGeometry::ParticleCloud => add_particle_cluster(
            mesh,
            Vec3::new(0.0, -0.45, 0.0),
            Vec3::new(0.85, 0.32, 0.85),
            [0.94, 0.96, 1.0, 0.82],
            count,
        ),
        EffectGeometry::RisingBubbles => {
            for index in 0..count {
                let lane = index % 4;
                let layer = index / 4;
                let cycle = (f32::from(layer) * 0.23 + progress * 0.9).fract();
                let point = reaction_point
                    + Vec3::new(
                        f32::from(lane) * 0.12 - 0.18,
                        -0.38 + cycle * 1.18,
                        f32::from((index * 7) % 5) * 0.09 - 0.18,
                    );
                add_sphere(
                    mesh,
                    point,
                    0.055 + f32::from(index % 3) * 0.018,
                    [0.72, 0.91, 1.0, 0.62],
                    5,
                    7,
                );
            }
        }
        EffectGeometry::SurfaceRipples => {
            for ring in 0_u8..3 {
                let cycle = (progress + f32::from(ring) * 0.31).fract();
                add_ring(
                    mesh,
                    reaction_point + Vec3::new(0.0, 0.012, 0.0),
                    0.14 + cycle * 0.58,
                    0.012,
                    [0.68, 0.91, 1.0, (1.0 - cycle) * 0.62],
                );
            }
        }
        EffectGeometry::SplashDroplets => {
            for index in 0..count.min(12) {
                let angle = f32::from(index) * 2.399_963_1;
                let cycle = (progress * 1.45 + f32::from(index) * 0.083).fract();
                let spread = 0.12 + cycle * 0.34;
                let point = reaction_point
                    + Vec3::new(
                        angle.cos() * spread,
                        cycle * (1.0 - cycle) * 1.15,
                        angle.sin() * spread,
                    );
                add_sphere(mesh, point, 0.025, [0.52, 0.84, 1.0, 0.82], 4, 6);
            }
        }
        EffectGeometry::PresentationOnly => {}
    }
}

fn appearance_color(profile: AppearanceProfile) -> [f32; 4] {
    match profile {
        AppearanceProfile::LaboratoryNeutral => [0.30, 0.38, 0.44, 1.0],
        AppearanceProfile::ClearGlass => [0.68, 0.88, 1.0, 0.32],
        AppearanceProfile::Water | AppearanceProfile::AqueousColourless => [0.28, 0.68, 0.94, 0.62],
        AppearanceProfile::WhitePrecipitate => [0.94, 0.96, 1.0, 0.92],
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

    fn canonical_plan() -> ScenePlan {
        let run = chemistry::run(chemistry::Experience::DEFAULT).expect("canonical run validates");
        let last = u16::try_from(
            run.frames()
                .frames()
                .last()
                .expect("frames exist")
                .ordinal(),
        )
        .expect("ordinal fits presentation range");
        compile_real_world_plan(
            run.frames(),
            &chemistry::presentation_profile(chemistry::Experience::DEFAULT, last),
        )
        .expect("plan compiles from trusted frames")
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
        let reacting = build_scene(&plan, 6, 0.5);
        assert!(reacting.0.len() > before.0.len());
        assert!(plan.effects.iter().any(|effect| {
            effect.effect == EffectProfile::SurfaceDisturbance
                || effect.effect == EffectProfile::SplashEmitter
        }));
        let (_, pitch, _) = camera_pose(&plan, 0, 0.0);
        assert!(pitch < -0.5);
    }
}
