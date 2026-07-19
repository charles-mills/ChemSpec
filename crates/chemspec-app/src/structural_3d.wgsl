struct Camera {
    view_projection: mat4x4<f32>,
    key_direction: vec4<f32>,
    fill_direction: vec4<f32>,
    camera_position: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: Camera;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) world_position: vec3<f32>,
};

struct GasInput {
    @location(3) center: vec3<f32>,
    @location(4) radius: f32,
    @location(5) color: vec4<f32>,
    @location(6) flow: vec3<f32>,
    @location(7) density: f32,
    @location(8) layering: f32,
};

struct GasOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) world_position: vec3<f32>,
    @location(3) density: f32,
    @location(4) layering: f32,
};

@vertex
fn vertex(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.clip_position = camera.view_projection * vec4<f32>(input.position, 1.0);
    output.normal = normalize(input.normal);
    output.color = input.color;
    output.world_position = input.position;
    return output;
}

@vertex
fn gas_vertex(
    input: GasInput,
    @builtin(vertex_index) vertex_index: u32,
) -> GasOutput {
    let corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(1.0, -1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, 1.0),
    );
    let corner = corners[vertex_index];
    let view_direction = normalize(camera.camera_position.xyz - input.center);
    var screen_right = normalize(cross(vec3<f32>(0.0, 1.0, 0.0), view_direction));
    if (dot(screen_right, screen_right) < 0.01) {
        screen_right = vec3<f32>(1.0, 0.0, 0.0);
    }
    let screen_up = normalize(cross(view_direction, screen_right));
    let projected_flow = vec2<f32>(
        dot(input.flow, screen_right),
        dot(input.flow, screen_up)
    );
    let flow_speed = length(projected_flow);
    var long_axis = screen_up;
    if (flow_speed > 0.0001) {
        long_axis = normalize(
            screen_right * projected_flow.x + screen_up * projected_flow.y
        );
    }
    let short_axis = normalize(cross(view_direction, long_axis));
    let stretch = min(flow_speed * 0.52, 0.46);
    let advected_offset = (
        short_axis * corner.x * (1.0 - stretch * 0.28)
        + long_axis * corner.y * (1.0 + stretch)
    ) * input.radius;
    // Retained gas reads as overlapping sheets along a moving density
    // interface rather than as a collection of round cloud puffs.
    let layered_offset = (
        screen_right * corner.x * 1.18
        + screen_up * corner.y * 0.54
    ) * input.radius;
    let offset = mix(advected_offset, layered_offset, input.layering);
    let world_position = input.center + offset;
    var output: GasOutput;
    output.clip_position = camera.view_projection * vec4<f32>(world_position, 1.0);
    output.uv = corner;
    output.color = input.color;
    output.world_position = world_position;
    output.density = input.density;
    output.layering = input.layering;
    return output;
}

@fragment
fn fragment(input: VertexOutput) -> @location(0) vec4<f32> {
    let normal = normalize(input.normal);
    let key = max(dot(normal, normalize(-camera.key_direction.xyz)), 0.0);
    let fill = max(dot(normal, normalize(-camera.fill_direction.xyz)), 0.0);
    let view_direction = normalize(camera.camera_position.xyz - input.world_position);
    let half_direction = normalize(view_direction + normalize(-camera.key_direction.xyz));
    let facing = max(dot(normal, view_direction), 0.0);
    let rim = pow(1.0 - facing, 2.2);
    let specular = pow(max(dot(normal, half_direction), 0.0), 42.0);
    let hemisphere = normal.y * 0.5 + 0.5;
    let lighting = 0.48 + key * 0.46 + fill * 0.22 + hemisphere * 0.12;
    let translucent = 1.0 - step(0.98, input.color.a);
    let glass = 1.0 - step(0.30, input.color.a);
    let solid_specular = specular * (0.14 + 0.30 * (1.0 - translucent));
    let transmission = mix(
        input.color.rgb * lighting,
        vec3<f32>(0.50, 0.70, 0.80) * (0.28 + 0.54 * lighting),
        glass * 0.24
    );
    let polished = transmission
        + vec3<f32>(1.0, 0.96, 0.90) * solid_specular
        + vec3<f32>(0.12, 0.24, 0.32) * rim * (0.14 + glass * 0.64);
    let edge_alpha = input.color.a + rim * glass * 0.20;
    return vec4<f32>(min(polished, vec3<f32>(1.0)), clamp(edge_alpha, 0.0, 1.0));
}

@fragment
fn emissive_fragment(input: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(input.color.rgb, clamp(input.color.a, 0.0, 1.0));
}

fn gas_hash(position: vec3<f32>) -> f32 {
    var value = fract(position * 0.1031);
    value += dot(value, value.yzx + vec3<f32>(33.33));
    return fract((value.x + value.y) * value.z);
}

fn gas_noise(position: vec3<f32>) -> f32 {
    let cell = floor(position);
    let local = fract(position);
    let blend = local * local * (vec3<f32>(3.0) - 2.0 * local);
    let x00 = mix(gas_hash(cell), gas_hash(cell + vec3<f32>(1.0, 0.0, 0.0)), blend.x);
    let x10 = mix(
        gas_hash(cell + vec3<f32>(0.0, 1.0, 0.0)),
        gas_hash(cell + vec3<f32>(1.0, 1.0, 0.0)),
        blend.x
    );
    let x01 = mix(
        gas_hash(cell + vec3<f32>(0.0, 0.0, 1.0)),
        gas_hash(cell + vec3<f32>(1.0, 0.0, 1.0)),
        blend.x
    );
    let x11 = mix(
        gas_hash(cell + vec3<f32>(0.0, 1.0, 1.0)),
        gas_hash(cell + vec3<f32>(1.0, 1.0, 1.0)),
        blend.x
    );
    return mix(mix(x00, x10, blend.y), mix(x01, x11, blend.y), blend.z);
}

fn gas_fbm(position: vec3<f32>) -> f32 {
    return gas_noise(position) * 0.58
        + gas_noise(position * 2.03 + vec3<f32>(7.1, 3.4, 5.7)) * 0.29
        + gas_noise(position * 4.11 + vec3<f32>(2.8, 8.3, 1.6)) * 0.13;
}

@fragment
fn gas_fragment(input: GasOutput) -> @location(0) vec4<f32> {
    let radius_squared = dot(input.uv, input.uv);
    let flow_warp = input.world_position
        + vec3<f32>(input.uv.x, input.uv.y, input.density) * 0.19;
    let boundary_noise = gas_fbm(flow_warp * 3.6);
    let fine_density = gas_fbm(flow_warp * 7.4 + vec3<f32>(11.0, 5.0, 17.0));
    let irregular_radius = radius_squared
        + (boundary_noise - 0.5) * mix(0.30, 0.18, input.layering);
    if (irregular_radius >= 1.04) {
        discard;
    }
    let gaussian = exp(-max(irregular_radius, 0.0) * 2.48);
    let soft_edge = 1.0 - smoothstep(0.34, 1.04, irregular_radius);
    let optical_depth = gaussian
        * soft_edge
        * mix(0.74 + fine_density * 0.52, 0.88 + fine_density * 0.34, input.layering);
    let view_direction = normalize(camera.camera_position.xyz - input.world_position);
    let forward_scatter = pow(max(dot(view_direction, normalize(-camera.key_direction.xyz)), 0.0), 3.0);
    let self_transmittance = exp(-min(input.density, 1.8) * 0.72);
    let illumination = 0.46 + self_transmittance * 0.34 + forward_scatter * 0.20;
    let in_scatter = vec3<f32>(0.16, 0.20, 0.22)
        * (0.18 + forward_scatter * 0.18)
        * gaussian;
    let color = min(input.color.rgb * illumination + in_scatter, vec3<f32>(1.0));
    let extinction = input.color.a
        * optical_depth
        * (1.08 + min(input.density, 1.4) * 0.18);
    return vec4<f32>(color, clamp(extinction, 0.0, 0.92));
}
