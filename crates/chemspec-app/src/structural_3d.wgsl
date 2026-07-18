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

struct SurfaceLight {
    key: f32,
    fill: f32,
    hemisphere: f32,
    specular_base: f32,
    fresnel: f32,
};

fn surface_light(normal: vec3<f32>, world_position: vec3<f32>) -> SurfaceLight {
    let n = normalize(normal);
    let view_direction = normalize(camera.camera_position.xyz - world_position);
    let key_light = normalize(-camera.key_direction.xyz);
    let fill_light = normalize(-camera.fill_direction.xyz);
    let half_direction = normalize(view_direction + key_light);
    var light: SurfaceLight;
    light.key = max(dot(n, key_light), 0.0);
    light.fill = max(dot(n, fill_light), 0.0);
    light.hemisphere = n.y * 0.5 + 0.5;
    light.specular_base = max(dot(n, half_direction), 0.0);
    light.fresnel = pow(1.0 - max(dot(n, view_direction), 0.0), 5.0);
    return light;
}

fn aces(colour: vec3<f32>) -> vec3<f32> {
    let mapped = (colour * (2.51 * colour + vec3<f32>(0.03)))
        / (colour * (2.43 * colour + vec3<f32>(0.59)) + vec3<f32>(0.14));
    return clamp(mapped, vec3<f32>(0.0), vec3<f32>(1.0));
}

// Opaque scene geometry: bench, metal, precipitate, molecular models.
@fragment
fn fragment_solid(input: VertexOutput) -> @location(0) vec4<f32> {
    let light = surface_light(input.normal, input.world_position);
    let specular = pow(light.specular_base, 40.0);
    let lighting = 0.34 + light.key * 0.58 + light.fill * 0.20 + light.hemisphere * 0.14;
    let lit = input.color.rgb * lighting
        + vec3<f32>(1.0, 0.97, 0.92) * specular * 0.22
        + vec3<f32>(0.35, 0.45, 0.55) * light.fresnel * 0.06;
    return vec4<f32>(aces(lit), clamp(input.color.a, 0.0, 1.0));
}

// Liquids, gas shells, bubbles, and other translucent volumes.
@fragment
fn fragment_liquid(input: VertexOutput) -> @location(0) vec4<f32> {
    let light = surface_light(input.normal, input.world_position);
    let specular = pow(light.specular_base, 64.0);
    let lighting = 0.40 + light.key * 0.50 + light.fill * 0.20 + light.hemisphere * 0.16;
    let lit = input.color.rgb * lighting
        + vec3<f32>(1.0, 0.98, 0.94) * specular * 0.30
        + input.color.rgb * light.fresnel * 0.35;
    let alpha = clamp(input.color.a + light.fresnel * 0.12, 0.0, 1.0);
    return vec4<f32>(aces(lit), alpha);
}

// Thin laboratory glass: near-neutral transmission with a strong grazing
// (fresnel) reflection so silhouettes read while faces stay clear.
@fragment
fn fragment_glass(input: VertexOutput) -> @location(0) vec4<f32> {
    let light = surface_light(input.normal, input.world_position);
    let specular = pow(light.specular_base, 90.0);
    let reflectivity = 0.04 + 0.96 * light.fresnel;
    let tint = vec3<f32>(0.84, 0.90, 0.94);
    // The lathe vessel has an inner and an outer wall and each is drawn in
    // both glass passes, so a ray crosses up to four blended layers: keep
    // per-surface alpha low or the beaker reads as frosted.
    let transmission = input.color.rgb * tint
        * (0.20 + 0.46 * (light.key * 0.60 + light.fill * 0.25 + light.hemisphere * 0.35));
    let lit = transmission
        + vec3<f32>(0.90, 0.96, 1.0) * specular * 0.18
        + tint * reflectivity * 0.08;
    let alpha = clamp(input.color.a + light.fresnel * 0.18 + specular * 0.10, 0.0, 1.0);
    return vec4<f32>(aces(lit), alpha);
}

// Additive pass: flame cores and sparks are authored colours, not lit.
@fragment
fn emissive_fragment(input: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(input.color.rgb, clamp(input.color.a, 0.0, 1.0));
}
