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

@vertex
fn vertex(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.clip_position = camera.view_projection * vec4<f32>(input.position, 1.0);
    output.normal = normalize(input.normal);
    output.color = input.color;
    output.world_position = input.position;
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
