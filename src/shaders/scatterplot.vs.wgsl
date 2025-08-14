struct Uniforms {
    x_min: f32,
    x_max: f32,
    y_min: f32,
    y_max: f32,
    point_size_px: f32,   // diameter in pixels
    _pad0: f32,
    viewport_size: vec2<f32>, // (width, height) in pixels
    color: vec4<f32>,     // rgba color for points
};

struct VSOut {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) quad_pos: vec2<f32>,
};

@group(0) @binding(0)
var<storage, read> positions: array<vec2<f32>>;

@group(0) @binding(1)
var<uniform> u: Uniforms;

// 4 corners of a unit quad for triangle strip: (-1,-1), (1,-1), (-1,1), (1,1)
const QUAD: array<vec2<f32>, 4> = array<vec2<f32>, 4>(
    vec2<f32>(-1.0, -1.0),
    vec2<f32>( 1.0, -1.0),
    vec2<f32>(-1.0,  1.0),
    vec2<f32>( 1.0,  1.0)
);

// Map a data value v from [min,max] to NDC [-1,1]
fn to_ndc(v: f32, minv: f32, maxv: f32) -> f32 {
    let t = (v - minv) / max(1e-12, (maxv - minv));
    return t * 2.0 - 1.0;
}

@vertex
fn vs_main(
    @builtin(instance_index) instance: u32,
    @builtin(vertex_index) vid: u32
) -> VSOut {
    // Center of this point in data space
    let p = positions[instance];
    // Center in clip/NDC space (y increases up)
    let center_ndc = vec2<f32>(
        to_ndc(p.x, u.x_min, u.x_max),
        to_ndc(p.y, u.y_min, u.y_max)
    );

    // Convert desired pixel radius to NDC
    let radius_px = 0.5 * u.point_size_px;
    // pixels -> NDC: ndc_per_px = 2 / viewport
    let ndc_per_px = 2.0 / u.viewport_size;
    let radius_ndc = vec2<f32>(radius_px * ndc_per_px.x, radius_px * ndc_per_px.y);

    // Pick corner of quad and place around center
    let corner = QUAD[vid & 3u]; // vid % 4
    let offset_ndc = vec2<f32>(corner.x * radius_ndc.x, corner.y * radius_ndc.y);

    var out: VSOut;
    out.position = vec4<f32>(center_ndc + offset_ndc, 0.0, 1.0);
    out.color = u.color;
    out.quad_pos = corner; // pass unit quad position for circular masking
    return out;
}