// WGSL shaders: instanced quad in screen space sampling R8 atlas

struct VsOut {
    @builtin(position) pos : vec4<f32>,
    @location(0) uv : vec2<f32>,
};

struct Ubo {
    viewport : vec2<f32>,
    color    : vec4<f32>,
};

@group(0) @binding(0) var glyph_tex : texture_2d<f32>;
@group(0) @binding(1) var glyph_sampler : sampler;
@group(0) @binding(2) var<uniform> u : Ubo;

// Per-instance attributes:
// @location(0): rect_px = vec4(x, y, w, h)
// @location(1): uv_rect = vec4(u0, v0, u1, v1)
@vertex
fn vs_main(
    @location(0) rect_px: vec4<f32>,
    @location(1) uv_rect: vec4<f32>,
    @builtin(vertex_index) vid: u32
) -> VsOut {
    // Corner in [0,1]^2 from vertex_index 0..3 (triangle strip)
    let cx = f32(vid & 1u);
    let cy = f32((vid >> 1u) & 1u);
    let corner = vec2<f32>(cx, cy);

    // Note: `rect_px` indicates where to render the glyph on the screen.
    // Meanwhile, `uv_rect` indicates where to sample the glyph in the texture atlas.

    // Pixel position
    let px = rect_px.xy + corner * rect_px.zw;

    // NDC transform (PositiveYDown -> NDC)
    let ndc = vec2<f32>(
        (px.x / u.viewport.x) * 2.0 - 1.0,
        1.0 - (px.y / u.viewport.y) * 2.0
    );

    // UV from rect
    let uv = uv_rect.xy + corner * (uv_rect.zw - uv_rect.xy);

    var out : VsOut;
    out.pos = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = uv;
    return out;
}

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let a = textureSample(glyph_tex, glyph_sampler, uv).r;
    // Premultiply for blending
    let rgb = u.color.rgb * a;
    return vec4<f32>(rgb, a);
}