//! Guards the shaders against implicit-derivative sampling.
//!
//! `textureSample`, `textureSampleCompare`, `textureSampleBias`, and the
//! `dpdx`/`dpdy`/`fwidth` builtins are invalid in non-uniform control flow.
//! Native wgpu (naga) tolerates that, but the browser's WGSL compiler (Tint)
//! rejects the whole shader module — the web demo then renders a black frame
//! with no JS-visible error. Every render target here has a single mip, so
//! the explicit-LOD forms (`textureSampleLevel(..., 0.0)`,
//! `textureSampleCompareLevel`) are drop-in identical and always legal.
//! If a future shader genuinely needs implicit LOD, hoist the sample out of
//! non-uniform control flow and relax this test for that one call site.

const SCENE_SHADER: &str = include_str!("../src/structural_3d.wgsl");
const POST_SHADER: &str = include_str!("../src/structural_3d_post.wgsl");
const SHADERS: [(&str, &str); 2] = [
    ("structural_3d.wgsl", SCENE_SHADER),
    ("structural_3d_post.wgsl", POST_SHADER),
];

const BANNED: [&str; 6] = [
    "textureSample(",
    "textureSampleCompare(",
    "textureSampleBias(",
    "dpdx",
    "dpdy",
    "fwidth",
];

#[test]
fn shaders_avoid_implicit_derivative_sampling() {
    for (name, source) in SHADERS {
        for banned in BANNED {
            for (index, line) in source.lines().enumerate() {
                assert!(
                    !line.contains(banned),
                    "{name}:{}: `{banned}` breaks the WebGPU demo in \
                     non-uniform control flow; use the explicit-LOD form \
                     (textureSampleLevel / textureSampleCompareLevel)",
                    index + 1,
                );
            }
        }
    }
}

#[test]
fn structural_scene_is_shadow_free_but_keeps_reflections() {
    let renderer = include_str!("../src/structural_3d.rs");

    assert!(!SCENE_SHADER.contains("shadow_map"));
    assert!(!POST_SHADER.contains("shadow_map"));
    assert!(!POST_SHADER.contains("ssao"));
    assert!(!renderer.contains("shadow pass"));
    assert!(!renderer.contains("shadow_pipeline"));
    assert!(!renderer.contains("ao pass"));

    assert!(SCENE_SHADER.contains("textureSampleLevel(reflection_texture"));
    assert!(renderer.contains("reflection pass"));
    assert!(renderer.contains("reflected opaque pipeline"));
}
