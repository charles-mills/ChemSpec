// Fullscreen-triangle passes that bracket the lit scene: the gradient
// backdrop / inset panel drawn before geometry, and the final blit that
// composites the antialiased offscreen scene into the application frame.

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

// Drawn with depth compare Always + depth write, at the far plane: it both
// paints the backdrop and resets depth for the viewport it covers, which is
// what lets the molecular inset re-use the main pass depth buffer.
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

@group(0) @binding(1)
var blit_texture: texture_2d<f32>;
@group(0) @binding(2)
var blit_sampler: sampler;

@vertex
fn blit_vertex(@builtin(vertex_index) index: u32) -> ScreenOutput {
    var output: ScreenOutput;
    let uv = fullscreen_corner(index);
    output.uv = uv;
    output.clip_position = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
    return output;
}

@fragment
fn blit_fragment(input: ScreenOutput) -> @location(0) vec4<f32> {
    return textureSample(blit_texture, blit_sampler, input.uv);
}
