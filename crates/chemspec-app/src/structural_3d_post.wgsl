// Post passes bracketing the HDR scene: the gradient backdrop drawn before
// geometry, the threshold + dual-filter bloom chain, and the final tonemapped
// composite into the application frame.

struct PanelStyle {
    top: vec4<f32>,
    bottom: vec4<f32>,
    border: vec4<f32>,
    // x: border width in uv units, y: vignette strength.
    params: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> panel: PanelStyle;

struct ScreenOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

fn fullscreen_corner(index: u32) -> vec2<f32> {
    return vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
}

// Drawn first into the HDR target with depth compare Always at the far plane:
// it paints the backdrop the lit scene composites over.
@vertex
fn panel_vertex(@builtin(vertex_index) index: u32) -> ScreenOutput {
    var output: ScreenOutput;
    let uv = fullscreen_corner(index);
    output.uv = uv;
    output.clip_position = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 1.0, 1.0);
    return output;
}

@fragment
fn panel_fragment(input: ScreenOutput) -> @location(0) vec4<f32> {
    let base = mix(panel.top, panel.bottom, input.uv.y);
    let centred = input.uv - vec2<f32>(0.5, 0.5);
    let vignette = clamp(1.0 - dot(centred, centred) * panel.params.y, 0.0, 1.0);
    var colour = base.rgb * vignette;
    let border = panel.params.x;
    if border > 0.0 {
        let edge = min(
            min(input.uv.x, 1.0 - input.uv.x),
            min(input.uv.y, 1.0 - input.uv.y),
        );
        if edge < border {
            colour = panel.border.rgb;
        }
    }
    return vec4<f32>(colour, 1.0);
}

// ---- Bloom chain -----------------------------------------------------------

struct BlitParams {
    // xy: source texel size. z: soft-knee threshold (>0 only on the first
    // downsample). w: intensity scale for the pass.
    texel: vec4<f32>,
};

@group(0) @binding(4)
var<uniform> blit_params: BlitParams;
@group(0) @binding(5)
var source_texture: texture_2d<f32>;
@group(0) @binding(6)
var source_sampler: sampler;

@vertex
fn blit_vertex(@builtin(vertex_index) index: u32) -> ScreenOutput {
    var output: ScreenOutput;
    let uv = fullscreen_corner(index);
    output.uv = uv;
    output.clip_position = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
    return output;
}

fn soft_threshold(colour: vec3<f32>, threshold: f32) -> vec3<f32> {
    let brightness = max(colour.r, max(colour.g, colour.b));
    let knee = threshold * 0.5;
    let soft = clamp(brightness - threshold + knee, 0.0, 2.0 * knee);
    let contribution = max(soft * soft / max(4.0 * knee, 1e-4), brightness - threshold)
        / max(brightness, 1e-4);
    return colour * max(contribution, 0.0);
}

@fragment
fn bloom_downsample(input: ScreenOutput) -> @location(0) vec4<f32> {
    let texel = blit_params.texel.xy;
    // 4-tap box on bilinear taps = 16 source pixels; enough at these sizes.
    var colour = textureSampleLevel(source_texture, source_sampler, input.uv + texel * vec2<f32>(-1.0, -1.0), 0.0).rgb;
    colour += textureSampleLevel(source_texture, source_sampler, input.uv + texel * vec2<f32>(1.0, -1.0), 0.0).rgb;
    colour += textureSampleLevel(source_texture, source_sampler, input.uv + texel * vec2<f32>(-1.0, 1.0), 0.0).rgb;
    colour += textureSampleLevel(source_texture, source_sampler, input.uv + texel * vec2<f32>(1.0, 1.0), 0.0).rgb;
    colour *= 0.25;
    if (blit_params.texel.z > 0.0) {
        colour = soft_threshold(colour, blit_params.texel.z);
    }
    return vec4<f32>(colour * blit_params.texel.w, 1.0);
}

// Rendered with additive blending into the next-larger level.
@fragment
fn bloom_upsample(input: ScreenOutput) -> @location(0) vec4<f32> {
    let texel = blit_params.texel.xy;
    var colour = textureSampleLevel(source_texture, source_sampler, input.uv + texel * vec2<f32>(-1.0, -1.0), 0.0).rgb;
    colour += textureSampleLevel(source_texture, source_sampler, input.uv + texel * vec2<f32>(1.0, -1.0), 0.0).rgb;
    colour += textureSampleLevel(source_texture, source_sampler, input.uv + texel * vec2<f32>(-1.0, 1.0), 0.0).rgb;
    colour += textureSampleLevel(source_texture, source_sampler, input.uv + texel * vec2<f32>(1.0, 1.0), 0.0).rgb;
    colour += textureSampleLevel(source_texture, source_sampler, input.uv + texel * vec2<f32>(-2.0, 0.0), 0.0).rgb;
    colour += textureSampleLevel(source_texture, source_sampler, input.uv + texel * vec2<f32>(2.0, 0.0), 0.0).rgb;
    colour += textureSampleLevel(source_texture, source_sampler, input.uv + texel * vec2<f32>(0.0, -2.0), 0.0).rgb;
    colour += textureSampleLevel(source_texture, source_sampler, input.uv + texel * vec2<f32>(0.0, 2.0), 0.0).rgb;
    colour += textureSampleLevel(source_texture, source_sampler, input.uv, 0.0).rgb * 4.0;
    colour /= 12.0;
    return vec4<f32>(colour * blit_params.texel.w, 1.0);
}

// ---- Ambient occlusion -----------------------------------------------------

// Reuses the blit bindings: params (texel.xy = aux texel, z = projection
// scale in pixels, w = world radius), source texture = resolved aux buffer.
@fragment
fn ssao_fragment(input: ScreenOutput) -> @location(0) vec4<f32> {
    let centre = textureSampleLevel(source_texture, source_sampler, input.uv, 0.0);
    let centre_distance = centre.w;
    if (centre_distance <= 0.01) {
        return vec4<f32>(1.0);
    }
    let normal = normalize(centre.xyz);
    // Screen-space sample radius shrinks with distance.
    let radius_px = clamp(blit_params.texel.z * blit_params.texel.w / centre_distance, 2.0, 28.0);
    var occlusion = 0.0;
    for (var tap = 0u; tap < 8u; tap += 1u) {
        let angle = f32(tap) * 2.399963;
        let reach = (f32(tap) + 1.0) / 8.0;
        let offset = vec2<f32>(cos(angle), sin(angle)) * reach * radius_px * blit_params.texel.xy;
        let sample = textureSampleLevel(source_texture, source_sampler, input.uv + offset, 0.0);
        if (sample.w <= 0.01) {
            continue;
        }
        let delta = centre_distance - sample.w;
        // Occluders sit closer to the camera; a range check rejects distant
        // silhouettes so edges do not halo.
        let ranged = 1.0 - smoothstep(0.10, 0.45, abs(delta));
        occlusion += step(0.015, delta) * ranged;
    }
    let strength = clamp(occlusion / 8.0, 0.0, 1.0);
    // Flat upward surfaces keep more ambient than crevices.
    let ao = 1.0 - strength * (0.55 + 0.20 * (1.0 - normal.y));
    return vec4<f32>(ao, ao, ao, 1.0);
}

// ---- Composite -------------------------------------------------------------

struct CompositeParams {
    inv_view_projection: mat4x4<f32>,
    light_view_projection: mat4x4<f32>,
    // x: exposure, y: bloom strength, z: 1 when the target surface is
    // non-sRGB and the shader must gamma-encode, w: focus-blur strength.
    values: vec4<f32>,
    // xy: heat-shimmer centre in uv, z: uv radius, w: shimmer strength.
    heat: vec4<f32>,
    // x: presentation seconds, y: flame exposure envelope, z: fog strength
    // (gas/vapour envelope), w: unused.
    clock: vec4<f32>,
    // xyz: camera position, w: bench-top height.
    ray: vec4<f32>,
};

@group(0) @binding(7)
var<uniform> composite_params: CompositeParams;
@group(0) @binding(8)
var scene_texture: texture_2d<f32>;
@group(0) @binding(9)
var bloom_texture: texture_2d<f32>;
@group(0) @binding(10)
var composite_sampler: sampler;
@group(0) @binding(11)
var blur_texture: texture_2d<f32>;
@group(0) @binding(12)
var ao_texture: texture_2d<f32>;
@group(0) @binding(13)
var aux_texture: texture_2d<f32>;
@group(0) @binding(14)
var composite_shadow_map: texture_depth_2d;
@group(0) @binding(15)
var composite_shadow_sampler: sampler_comparison;

// One shadow visibility tap along the volumetric march.
fn shaft_visibility(world: vec3<f32>) -> f32 {
    let clip = composite_params.light_view_projection * vec4<f32>(world, 1.0);
    let ndc = clip.xyz / max(clip.w, 1e-4);
    let uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 || ndc.z <= 0.0 || ndc.z >= 1.0) {
        return 1.0;
    }
    return textureSampleCompareLevel(composite_shadow_map, composite_shadow_sampler, uv, ndc.z);
}

// Khronos PBR Neutral: preserves saturated educational colours far better
// than filmic curves while still compressing HDR highlights.
// The shoulder starts well below the spec's 0.76: the key + fill push
// diffuse whites past 1.0, and with the spec knee everything from 1.5-3.0
// landed in 0.94-0.98 — flat white. The earlier, longer shoulder keeps
// gradation in bright liquids and glassware.
fn tonemap_pbr_neutral(colour: vec3<f32>) -> vec3<f32> {
    let start_compression = 0.50;
    let desaturation = 0.08;
    let x = min(colour.r, min(colour.g, colour.b));
    var offset = 0.04;
    if (x < 0.08) {
        offset = x - 6.25 * x * x;
    }
    var mapped = colour - offset;
    let peak = max(mapped.r, max(mapped.g, mapped.b));
    if (peak < start_compression) {
        return mapped;
    }
    let d = 1.0 - start_compression;
    let new_peak = 1.0 - d * d / (peak + d - start_compression);
    mapped *= new_peak / peak;
    let g = 1.0 - 1.0 / (desaturation * (peak - new_peak) + 1.0);
    return mix(mapped, vec3<f32>(new_peak), g);
}

fn dither_hash(position: vec2<f32>) -> f32 {
    return fract(sin(dot(position, vec2<f32>(12.9898, 78.233))) * 43758.5453);
}

fn shimmer_hash(position: vec2<f32>) -> f32 {
    return fract(sin(dot(position, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn shimmer_noise(position: vec2<f32>) -> f32 {
    let cell = floor(position);
    let local = fract(position);
    let blend = local * local * (vec2<f32>(3.0) - 2.0 * local);
    let x0 = mix(shimmer_hash(cell), shimmer_hash(cell + vec2<f32>(1.0, 0.0)), blend.x);
    let x1 = mix(
        shimmer_hash(cell + vec2<f32>(0.0, 1.0)),
        shimmer_hash(cell + vec2<f32>(1.0, 1.0)),
        blend.x
    );
    return mix(x0, x1, blend.y);
}

@fragment
fn composite_fragment(input: ScreenOutput) -> @location(0) vec4<f32> {
    var uv = input.uv;
    // Heat shimmer: refractive wobble rising through the hot column.
    let heat = composite_params.heat;
    if (heat.w > 0.001) {
        let falloff = 1.0 - smoothstep(heat.z * 0.35, heat.z, distance(uv, heat.xy));
        if (falloff > 0.001) {
            let time = composite_params.clock.x;
            let wave = vec2<f32>(
                shimmer_noise(uv * 34.0 + vec2<f32>(0.0, time * 2.6)) - 0.5,
                shimmer_noise(uv * 29.0 + vec2<f32>(11.3, time * 3.1)) - 0.5,
            );
            uv += wave * falloff * heat.w * 0.014;
        }
    }
    let scene = textureSampleLevel(scene_texture, composite_sampler, uv, 0.0).rgb;
    let bloom = textureSampleLevel(bloom_texture, composite_sampler, uv, 0.0).rgb;
    // A camera stopping down against flame glare: the exposure dips with the
    // flame envelope and recovers as it fades. Deliberately restrained.
    let exposure = composite_params.values.x / (1.0 + composite_params.clock.y * 0.26);
    let bloom_strength = composite_params.values.y;
    // Ambient occlusion darkens the scene term only, never the bloom.
    let ao = textureSampleLevel(ao_texture, composite_sampler, uv, 0.0).r;
    var colour = scene * ao;

    // Volumetric key-light shafts: a short march through height fog, carved
    // by the shadow map, appearing only while gas or vapour is active.
    let fog_strength = composite_params.clock.z;
    if (fog_strength > 0.01) {
        let near_world = composite_params.inv_view_projection
            * vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.1, 1.0);
        let far_world = composite_params.inv_view_projection
            * vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.9, 1.0);
        let origin = composite_params.ray.xyz;
        let direction = normalize(far_world.xyz / far_world.w - near_world.xyz / near_world.w);
        let pixel_distance = textureSampleLevel(aux_texture, composite_sampler, uv, 0.0).w;
        let march_end = clamp(pixel_distance, 1.0, 14.0);
        let bench_top = composite_params.ray.w;
        var accumulated = 0.0;
        let jitter = shimmer_hash(input.clip_position.xy) * 0.9;
        for (var step_index = 0u; step_index < 10u; step_index += 1u) {
            let t = (f32(step_index) + jitter) / 10.0 * march_end;
            let world = origin + direction * t;
            let height = world.y - bench_top;
            // A thin haze layer hugging the bench, fading by two units up.
            let density = clamp(1.0 - height * 0.5, 0.0, 1.0) * step(0.0, height);
            accumulated += shaft_visibility(world) * density;
        }
        let shafts = accumulated / 10.0 * fog_strength * 0.30;
        colour += vec3<f32>(0.55, 0.63, 0.72) * shafts;
    }

    // Close-up focus: tilt-shift softening away from frame centre, with a
    // six-tap disc kernel whose highlight weighting rounds bright glints
    // into bokeh discs instead of smearing them.
    let focus = composite_params.values.w;
    if (focus > 0.001) {
        let centred = uv - vec2<f32>(0.5, 0.46);
        let edge = smoothstep(0.12, 0.55, dot(centred, centred) * 2.6);
        let amount = edge * focus;
        let radius = amount * 0.011;
        var accum = vec3<f32>(0.0);
        var weight_sum = 0.0;
        for (var tap = 0u; tap < 6u; tap += 1u) {
            let angle = f32(tap) * 1.0471976;
            let offset = vec2<f32>(cos(angle), sin(angle)) * radius;
            let tap_colour = textureSampleLevel(blur_texture, composite_sampler, uv + offset, 0.0).rgb;
            let brightness = dot(tap_colour, vec3<f32>(0.3333));
            let weight = 1.0 + brightness * brightness * 3.0;
            accum += tap_colour * weight;
            weight_sum += weight;
        }
        colour = mix(colour, accum / weight_sum, amount);
    }
    colour = (colour + bloom * bloom_strength) * exposure;
    colour = tonemap_pbr_neutral(max(colour, vec3<f32>(0.0)));
    // End-card: the closing glide draws a soft vignette that frames the
    // arrival like a title card, then holds it.
    let endcard = composite_params.clock.w;
    if (endcard > 0.001) {
        let framed = input.uv - vec2<f32>(0.5, 0.47);
        let vignette = 1.0 - smoothstep(0.32, 0.95, dot(framed, framed) * 2.2) * 0.34 * endcard;
        colour *= vignette;
    }
    if (composite_params.values.z > 0.5) {
        colour = pow(colour, vec3<f32>(1.0 / 2.2));
    }
    // Photographic grain at the edge of perception, heavier in the shadows,
    // plus dither to break gradient banding before 8-bit quantization.
    let luminance = dot(colour, vec3<f32>(0.299, 0.587, 0.114));
    let grain_seed = input.clip_position.xy
        + vec2<f32>(composite_params.clock.x * 61.7, composite_params.clock.x * 39.1);
    let grain = (shimmer_hash(grain_seed) - 0.5) * 0.009 * (1.0 - clamp(luminance, 0.0, 1.0));
    let dither = (dither_hash(input.clip_position.xy) - 0.5) / 255.0;
    return vec4<f32>(clamp(colour + grain + dither, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
