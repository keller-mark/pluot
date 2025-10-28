struct Uniforms {
    viewport_size: vec2<f32>,  // (width, height) in pixels
    plot_margin: vec4<f32>,    // (top, right, bottom, left) in pixels
    camera_view: mat4x4<f32>,  // Ignored for now
    bar_padding_px: f32,       // bar padding on left/right
    bar_size_px: f32,          // bar width in pixels
};

struct VSOut {
    @builtin(position) position: vec4<f32>,
};

struct FSOut {
    @location(0) color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var<storage, read> x_coords: array<f32>;
@group(0) @binding(2) var<storage, read> y_coords: array<f32>;

// 4 corners of a rectangle for triangle strip: bottom-left, bottom-right, top-left, top-right
const QUAD: array<vec2<f32>, 4> = array<vec2<f32>, 4>(
    vec2<f32>(0.0, 0.0),  // bottom-left
    vec2<f32>(1.0, 0.0),  // bottom-right
    vec2<f32>(0.0, 1.0),  // top-left
    vec2<f32>(1.0, 1.0)   // top-right
);

@vertex
fn vs_main(
    @builtin(instance_index) instance_index: u32,
    @builtin(vertex_index) vertex_index: u32
) -> VSOut {
    // X position is already in pixel space (from ScaleBand)
    let x_px = x_coords[instance_index];
    // Y value is the raw data value
    let y_value = y_coords[instance_index];

    // Convert margins from pixels to NDC
    let margin_ndc = u.plot_margin * (2.0 / vec4<f32>(u.viewport_size.yx, u.viewport_size.yx));
    let margin_top = margin_ndc.x;
    let margin_right = margin_ndc.y;
    let margin_bottom = margin_ndc.z;
    let margin_left = margin_ndc.w;

    // Define valid rendering region in NDC
    let valid_min = vec2<f32>(-1.0 + margin_left, -1.0 + margin_bottom);
    let valid_max = vec2<f32>(1.0 - margin_right, 1.0 - margin_top);

    // Get the quad corner: (0,0), (1,0), (0,1), or (1,1)
    let corner = QUAD[vertex_index & 3u];

    // Calculate bar rectangle in pixel space (Y-down coordinate system)
    let bar_left_px = x_px;
    let bar_right_px = x_px + u.bar_size_px;

    // The bottom of the bar is at the bottom margin.
    let bar_bottom_px = u.viewport_size.y - u.plot_margin.z;
    // The top of the bar is `y_value` pixels *below* the top margin.
    let bar_top_px = u.plot_margin.x + y_value;

    // Interpolate position based on corner
    let pos_px = vec2<f32>(
        mix(bar_left_px, bar_right_px, corner.x),
        mix(bar_bottom_px, bar_top_px, corner.y)
    );

    // Convert pixel position to NDC
    // NDC: x in [-1, 1], y in [-1, 1]
    // Pixel space: x in [0, width], y in [0, height] (Y is down)
    let ndc = (pos_px / u.viewport_size) * 2.0 - 1.0;
    // Flip Y coordinate (pixel Y increases down, NDC Y increases up)
    let ndc_flipped = vec2<f32>(ndc.x, -ndc.y);

    var out: VSOut;
    out.position = vec4<f32>(ndc_flipped, 0.0, 1.0);
    return out;
}

@fragment
fn fs_main(
    @builtin(position) frag_coord: vec4<f32>
) -> FSOut {
    var out: FSOut;
    // Black bars with full opacity
    out.color = vec4<f32>(0.0, 0.0, 1.0, 1.0);
    return out;
}
