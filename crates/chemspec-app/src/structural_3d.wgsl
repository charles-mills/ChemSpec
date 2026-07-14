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
    let rim = pow(1.0 - max(dot(normal, view_direction), 0.0), 2.0);
    let hemisphere = normal.y * 0.5 + 0.5;
    let lighting = 0.56 + key * 0.46 + fill * 0.24 + hemisphere * 0.12 + rim * 0.18;
    let polished = input.color.rgb * lighting + vec3<f32>(0.025, 0.04, 0.055) * rim;
    return vec4<f32>(polished, input.color.a);
}
