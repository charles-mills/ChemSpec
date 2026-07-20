// HDR forward pass for the macroscopic scene.
//
// Geometry carries a per-vertex material id assigned by bucket at upload:
// 0 dielectric, 1 liquid/translucent, 2 glass, 3 emissive, 4 metal.
// Output is linear HDR (unclamped); bloom + tonemapping happen in
// structural_3d_post.wgsl. Lighting is a shadow-free key/fill directional rig
// with GGX specular, hemisphere ambient, and a procedural studio environment
// for fresnel reflections. The transparent pass renders
// after the opaque scene has been resolved, so glass and liquid refract a
// real background image.

struct Camera {
    view_projection: mat4x4<f32>,
    key_direction: vec4<f32>,
    fill_direction: vec4<f32>,
    camera_position: vec4<f32>,
    // x: presentation seconds, yz: HDR target pixel size, w: unused.
    params: vec4<f32>,
    // xy: vessel centre in world xz, z: footprint radius, w: intensity.
    caustic: vec4<f32>,
    // rgb: liquid tint for caustics, w: bench-top world height.
    caustic_tint: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: Camera;
// The scene mirrored across the bench plane, resolved half-res; bench
// pixels sample it for exact planar reflections. In the reflection pass
// itself this binds a dummy texture (bench pixels are discarded there).
@group(0) @binding(3)
var reflection_texture: texture_2d<f32>;
@group(0) @binding(4)
var reflection_sampler: sampler;

// Bound only for the transparent pass: the resolved opaque scene.
@group(1) @binding(0)
var background_texture: texture_2d<f32>;
@group(1) @binding(1)
var background_sampler: sampler;

const MATERIAL_DIELECTRIC: u32 = 0u;
const MATERIAL_LIQUID: u32 = 1u;
const MATERIAL_GLASS: u32 = 2u;
const MATERIAL_EMISSIVE: u32 = 3u;
const MATERIAL_METAL: u32 = 4u;

const KEY_COLOUR: vec3<f32> = vec3<f32>(1.04, 0.98, 0.90);
const KEY_INTENSITY: f32 = 2.35;
const FILL_COLOUR: vec3<f32> = vec3<f32>(0.56, 0.66, 0.80);
const FILL_INTENSITY: f32 = 0.62;
const SKY_AMBIENT: vec3<f32> = vec3<f32>(0.30, 0.36, 0.46);
const GROUND_AMBIENT: vec3<f32> = vec3<f32>(0.20, 0.17, 0.14);
const EMISSIVE_BOOST: f32 = 2.3;
// Backlight separating silhouettes from the dark backdrop.
const KICK_DIRECTION: vec3<f32> = vec3<f32>(0.60, -0.42, -0.68);
const KICK_COLOUR: vec3<f32> = vec3<f32>(0.62, 0.78, 1.0);

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(9) material: u32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) world_position: vec3<f32>,
    @location(3) @interpolate(flat) material: u32,
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
    output.material = input.material;
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

// Procedural "studio" environment: warm ground bounce, cool sky, and a bright
// horizon band that gives glass and metal a believable reflection without any
// texture assets.
fn environment_radiance(direction: vec3<f32>) -> vec3<f32> {
    let elevation = clamp(direction.y, -1.0, 1.0);
    let sky = mix(
        vec3<f32>(0.34, 0.40, 0.50),
        vec3<f32>(0.16, 0.20, 0.28),
        clamp(elevation * 1.4, 0.0, 1.0),
    );
    let ground = vec3<f32>(0.24, 0.20, 0.16);
    var radiance = mix(ground, sky, smoothstep(-0.25, 0.35, elevation));
    let horizon = exp(-abs(elevation) * 5.2);
    radiance += vec3<f32>(0.55, 0.56, 0.58) * horizon * 0.45;
    // A soft overhead strip-light so upward-facing curvature picks up a sheen.
    let strip = smoothstep(0.55, 0.92, elevation);
    radiance += vec3<f32>(0.90, 0.88, 0.84) * strip * 0.55;
    return radiance;
}

fn ggx_specular(
    normal: vec3<f32>,
    view_direction: vec3<f32>,
    light_direction: vec3<f32>,
    roughness: f32,
    f0: vec3<f32>,
) -> vec3<f32> {
    let half_direction = normalize(view_direction + light_direction);
    let n_dot_l = max(dot(normal, light_direction), 0.0);
    let n_dot_v = max(dot(normal, view_direction), 1e-4);
    let n_dot_h = max(dot(normal, half_direction), 0.0);
    let v_dot_h = max(dot(view_direction, half_direction), 0.0);
    let alpha = roughness * roughness;
    let alpha_squared = alpha * alpha;
    let denom = n_dot_h * n_dot_h * (alpha_squared - 1.0) + 1.0;
    let distribution = alpha_squared / max(3.14159265 * denom * denom, 1e-5);
    let k = alpha * 0.5;
    let geometry = (n_dot_l / (n_dot_l * (1.0 - k) + k))
        * (n_dot_v / (n_dot_v * (1.0 - k) + k));
    let fresnel = f0 + (vec3<f32>(1.0) - f0) * pow(1.0 - v_dot_h, 5.0);
    return distribution * geometry * fresnel * n_dot_l / max(4.0 * n_dot_v * n_dot_l, 1e-4);
}

struct MaterialParams {
    roughness: f32,
    metalness: f32,
    f0: vec3<f32>,
    env_strength: f32,
};

fn material_params(material: u32, albedo: vec3<f32>) -> MaterialParams {
    var params: MaterialParams;
    switch material {
        case 4u: {
            params.roughness = 0.34;
            params.metalness = 1.0;
            params.f0 = albedo;
            params.env_strength = 0.85;
        }
        case 2u: {
            // Roughness masks the i8-quantized normals in authored clip
            // beakers that a mirror finish would band visibly.
            params.roughness = 0.12;
            params.metalness = 0.0;
            params.f0 = vec3<f32>(0.04);
            params.env_strength = 0.85;
        }
        case 1u: {
            params.roughness = 0.10;
            params.metalness = 0.0;
            params.f0 = vec3<f32>(0.02);
            params.env_strength = 0.35;
        }
        default: {
            params.roughness = 0.55;
            params.metalness = 0.0;
            params.f0 = vec3<f32>(0.04);
            params.env_strength = 0.18;
        }
    }
    return params;
}

// Two drifting noise layers multiplied into a filament web: the familiar
// dancing light pattern under a lit vessel of liquid. Purely presentational
// and driven by the deterministic playhead clock.
fn caustic_pattern(position: vec2<f32>, time: f32) -> f32 {
    let a = gas_noise(vec3<f32>(position * 3.1, time * 0.35));
    let b = gas_noise(vec3<f32>(position * 4.3 + vec2<f32>(7.7, 2.9), 1.7 + time * 0.28));
    return pow(clamp(a * b * 2.9, 0.0, 1.0), 3.0);
}

fn shade_surface(input: VertexOutput) -> vec4<f32> {
    let normal = normalize(input.normal);
    let view_direction = normalize(camera.camera_position.xyz - input.world_position);
    let key = normalize(-camera.key_direction.xyz);
    let fill = normalize(-camera.fill_direction.xyz);
    // Authored colours are sRGB-tuned; lighting happens in linear space.
    var albedo = pow(max(input.color.rgb, vec3<f32>(0.0)), vec3<f32>(2.2));
    let n_dot_v = max(dot(normal, view_direction), 1e-4);
    if (input.material == MATERIAL_LIQUID) {
        // Beer-Lambert-style deepening: grazing rays cross more solution, so
        // silhouettes saturate while the surface stays bright.
        let grazing = clamp(1.0 / max(n_dot_v, 0.25) - 1.0, 0.0, 2.2);
        albedo = pow(albedo, vec3<f32>(1.0 + grazing * 0.8));
    }
    var params = material_params(input.material, albedo);
    // Procedural lab-bench finish: brushed streaks, roughness variation, and
    // a faint worktop grid — bench pixels only, no assets.
    let bench_surface = input.material == MATERIAL_DIELECTRIC
        && normal.y > 0.9
        && abs(input.world_position.y - camera.caustic_tint.w) < 0.045;
    // The mirrored pass must not render the mirror itself — and the bench
    // is a solid box, so every face at or below its top plane is excluded,
    // not just the upward-facing one.
    if (camera.params.w > 0.5
        && input.material == MATERIAL_DIELECTRIC
        && input.world_position.y <= camera.caustic_tint.w + 0.005)
    {
        discard;
    }
    var brush = 0.0;
    if (bench_surface) {
        let planar = input.world_position.xz;
        brush = gas_noise(vec3<f32>(planar.x * 1.7, planar.y * 26.0, 3.0));
        let patina = gas_noise(vec3<f32>(planar * 0.55, 9.0));
        albedo *= 0.92 + brush * 0.10 + patina * 0.06;
        params.roughness = 0.40 + brush * 0.24;
        params.env_strength = 0.30;
        let cell = abs(fract(planar * 0.55) - vec2<f32>(0.5)) * 2.0;
        let grid = smoothstep(0.965, 0.995, max(cell.x, cell.y));
        albedo *= 1.0 - grid * 0.05;
    }

    let key_radiance = KEY_COLOUR * KEY_INTENSITY;
    let fill_radiance = FILL_COLOUR * FILL_INTENSITY;

    let n_dot_key = max(dot(normal, key), 0.0);
    let n_dot_fill = max(dot(normal, fill), 0.0);
    let hemisphere = mix(GROUND_AMBIENT, SKY_AMBIENT, normal.y * 0.5 + 0.5);

    let diffuse_albedo = albedo * (1.0 - params.metalness);
    var radiance = diffuse_albedo * (
        key_radiance * n_dot_key
        + fill_radiance * n_dot_fill
        + hemisphere
    );

    radiance += key_radiance * ggx_specular(normal, view_direction, key, params.roughness, params.f0);
    radiance += fill_radiance * ggx_specular(normal, view_direction, fill, max(params.roughness, 0.12), params.f0);

    let fresnel = params.f0
        + (max(vec3<f32>(1.0 - params.roughness), params.f0) - params.f0)
            * pow(1.0 - n_dot_v, 5.0);
    let reflection = reflect(-view_direction, normal);
    let env = environment_radiance(reflection) * fresnel * params.env_strength;
    var tint = vec3<f32>(1.0);
    if (input.material == MATERIAL_METAL) {
        tint = albedo;
    }
    radiance += env * tint;

    // Exact planar reflection on the bench: the mirrored scene sampled at
    // this pixel, smeared along the brushed finish, strongest at grazing
    // angles.
    if (bench_surface && camera.params.w < 0.5) {
        let uv = input.clip_position.xy / max(camera.params.yz, vec2<f32>(1.0));
        // Low-frequency wobble, biased vertical: a brushed smear rather
        // than a jagged edge.
        let wobble_noise = gas_noise(vec3<f32>(input.world_position.xz * 1.1, 21.0)) - 0.5;
        let smear = vec2<f32>(wobble_noise * 0.006, (brush - 0.5) * 0.014);
        let mirrored =
            textureSampleLevel(reflection_texture, reflection_sampler, clamp(uv + smear, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0)
                .rgb;
        let grazing = pow(1.0 - n_dot_v, 1.6);
        radiance += mirrored * (0.06 + 0.30 * grazing);
    }

    // Rim kicker: strong on glass, restrained on matte surfaces.
    let kick = normalize(-KICK_DIRECTION);
    var kick_strength = 0.10;
    if (input.material == MATERIAL_GLASS) {
        kick_strength = 0.45;
    }
    if (input.material == MATERIAL_LIQUID) {
        kick_strength = 0.26;
    }
    if (input.material == MATERIAL_METAL) {
        kick_strength = 0.30;
    }
    radiance += KICK_COLOUR
        * pow(1.0 - n_dot_v, 2.6)
        * max(dot(normal, kick), 0.0)
        * kick_strength;

    // Caustics: light focused through the liquid dances on the bench inside
    // and around the vessel footprint.
    if (input.material == MATERIAL_DIELECTRIC
        && camera.caustic.w > 0.001
        && normal.y > 0.85)
    {
        let bench_height = camera.caustic_tint.w;
        let height_band = 1.0
            - smoothstep(0.18, 0.34, abs(input.world_position.y - bench_height));
        let planar = distance(input.world_position.xz, camera.caustic.xy);
        let footprint = 1.0 - smoothstep(camera.caustic.z * 0.50, camera.caustic.z * 1.18, planar);
        if (height_band * footprint > 0.001) {
            let web = caustic_pattern(input.world_position.xz, camera.params.x);
            radiance += camera.caustic_tint.rgb
                * web
                * footprint
                * height_band
                * camera.caustic.w
                * 0.62;
        }
    }

    // In the mirror pass, a semi-matte bench reflects an object's base
    // crisply while its upper reaches dissolve; fade radiance with height.
    if (camera.params.w > 0.5) {
        let height = max(input.world_position.y - camera.caustic_tint.w, 0.0);
        radiance *= exp(-height * 1.4);
    }

    var alpha = input.color.a;
    // Authored alpha is the choreography's only hide mechanism (props fade
    // in/out via vertex alpha), so the fresnel presence terms must scale
    // with it — otherwise faded-out geometry keeps its edge glow.
    let authored = smoothstep(0.0, 0.12, input.color.a);
    if (input.material == MATERIAL_GLASS) {
        // Glass keeps its authored transmission tint but gains fresnel-driven
        // edge presence so silhouettes read against any backdrop.
        let edge = pow(1.0 - n_dot_v, 2.0);
        alpha = clamp(alpha + (edge * 0.28 + length(fresnel) * 0.10) * authored, 0.0, 1.0);
    }
    if (input.material == MATERIAL_LIQUID) {
        let edge = pow(1.0 - n_dot_v, 3.0);
        alpha = clamp(alpha + edge * 0.12 * authored, 0.0, 1.0);
    }
    return vec4<f32>(radiance, alpha);
}

@fragment
fn fragment(input: VertexOutput) -> @location(0) vec4<f32> {
    return shade_surface(input);
}

struct GbufferOutput {
    @location(0) colour: vec4<f32>,
    @location(1) aux: vec4<f32>,
};

// Opaque-pass variant that also writes the world normal and camera distance
// used to bound the volumetric march.
@fragment
fn fragment_gbuffer(input: VertexOutput) -> GbufferOutput {
    var output: GbufferOutput;
    output.colour = shade_surface(input);
    let distance_to_camera = length(camera.camera_position.xyz - input.world_position);
    output.aux = vec4<f32>(normalize(input.normal), distance_to_camera);
    return output;
}

// Transparent pass: same shading plus screen-space refraction of the resolved
// opaque scene. The refracted term is weighted into the alpha blend rather
// than replacing it, so stacked transparent surfaces still layer.
// ponytail: background excludes other transparent surfaces, so liquid seen
// through the front wall relies on the ordinary blend; true multi-layer
// refraction needs OIT or per-layer resolves.
@fragment
fn fragment_transparent(input: VertexOutput) -> @location(0) vec4<f32> {
    var shaded = shade_surface(input);
    let normal = normalize(input.normal);
    let view_direction = normalize(camera.camera_position.xyz - input.world_position);
    var screen_right = normalize(cross(vec3<f32>(0.0, 1.0, 0.0), view_direction));
    if (dot(screen_right, screen_right) < 0.01) {
        screen_right = vec3<f32>(1.0, 0.0, 0.0);
    }
    let screen_up = normalize(cross(view_direction, screen_right));
    var strength = 0.0;
    if (input.material == MATERIAL_GLASS) {
        strength = 0.055;
    }
    if (input.material == MATERIAL_LIQUID) {
        strength = 0.022;
    }
    if (strength > 0.0) {
        let uv = input.clip_position.xy / max(camera.params.yz, vec2<f32>(1.0));
        let offset = vec2<f32>(dot(normal, screen_right), -dot(normal, screen_up)) * strength;
        let sample_uv = clamp(uv + offset, vec2<f32>(0.001), vec2<f32>(0.999));
        let background =
            textureSampleLevel(background_texture, background_sampler, sample_uv, 0.0).rgb;
        let tint = mix(vec3<f32>(1.0), pow(max(input.color.rgb, vec3<f32>(0.0)), vec3<f32>(2.2)), 0.45);
        // Same gate as the fresnel terms: refraction weight grows as authored
        // alpha shrinks, which without the gate makes faded-out props MORE
        // visible the harder the choreography tries to hide them.
        let transmission_weight = (1.0 - shaded.a)
            * select(0.30, 0.62, input.material == MATERIAL_GLASS)
            * smoothstep(0.0, 0.12, input.color.a);
        shaded = vec4<f32>(
            shaded.rgb + background * tint * transmission_weight,
            clamp(shaded.a + transmission_weight * 0.35, 0.0, 1.0),
        );
    }
    return shaded;
}

@fragment
fn emissive_fragment(input: VertexOutput) -> @location(0) vec4<f32> {
    // Unclamped HDR so flame cores and sparks drive bloom in the post chain.
    let linear = pow(max(input.color.rgb, vec3<f32>(0.0)), vec3<f32>(2.2));
    return vec4<f32>(linear * EMISSIVE_BOOST, clamp(input.color.a, 0.0, 1.0));
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
    let key = normalize(-camera.key_direction.xyz);
    let forward_scatter = pow(max(dot(view_direction, key), 0.0), 3.0);
    let self_transmittance = exp(-min(input.density, 1.8) * 0.72);
    let illumination = 0.52 + self_transmittance * 0.40 + forward_scatter * 0.34;
    let in_scatter = vec3<f32>(0.16, 0.20, 0.22)
        * (0.18 + forward_scatter * 0.22)
        * gaussian;
    let gas_albedo = pow(max(input.color.rgb, vec3<f32>(0.0)), vec3<f32>(2.2));
    let color = gas_albedo * illumination + in_scatter;
    let extinction = input.color.a
        * optical_depth
        * (1.08 + min(input.density, 1.4) * 0.18);
    return vec4<f32>(color, clamp(extinction, 0.0, 0.92));
}
