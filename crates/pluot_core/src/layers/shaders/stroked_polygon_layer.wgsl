// StrokedPolygonLayer shader.
// Renders polygon outlines with miter joins at vertices, eliminating the gaps
// and overlaps that appear when adjacent rectangle quads meet at an angle.
//
// Approach: instanced rendering with 4 vertices (TriangleStrip quad) per edge.
// For each edge (prev→src→dst→next), the two corners at src use the miter bisector
// of the prev→src→dst turn, and the two corners at dst use the miter bisector of
// the src→dst→next turn. Adjacent segments share the same miter corners at their
// shared vertex, so quads tile seamlessly without gaps or overlaps.
//
// All geometry is computed in pixel space to keep the aspect-ratio math simple.
// Miter extension is clamped to MITER_LIMIT × half-width to avoid spikes at
// very sharp angles.

fn scale_mat(x: f32, y: f32, z: f32) -> mat4x4<f32> {
    return mat4x4<f32>(
        vec4<f32>(x,   0.0, 0.0, 0.0),
        vec4<f32>(0.0, y,   0.0, 0.0),
        vec4<f32>(0.0, 0.0, z,   0.0),
        vec4<f32>(0.0, 0.0, 0.0, 1.0),
    );
}

fn translate_mat(x: f32, y: f32, z: f32) -> mat4x4<f32> {
    return mat4x4<f32>(
        vec4<f32>(1.0, 0.0, 0.0, 0.0),
        vec4<f32>(0.0, 1.0, 0.0, 0.0),
        vec4<f32>(0.0, 0.0, 1.0, 0.0),
        vec4<f32>(x,   y,   z,   1.0),
    );
}

fn get_aspect_ratio_mat(layer_aspect_ratio: f32, aspect_ratio_mode: u32, aspect_ratio_alignment_mode: u32) -> mat4x4<f32> {
    var sx = 1.0;
    var sy = 1.0;
    if (aspect_ratio_mode == 1u) {
        if (layer_aspect_ratio > 1.0) { sx = 1.0 / layer_aspect_ratio; }
        else if (layer_aspect_ratio < 1.0) { sy = layer_aspect_ratio; }
    } else if (aspect_ratio_mode == 2u) {
        if (layer_aspect_ratio > 1.0) { sy = layer_aspect_ratio; }
        else if (layer_aspect_ratio < 1.0) { sx = 1.0 / layer_aspect_ratio; }
    }
    var tx = 0.0;
    var ty = 0.0;
    if (aspect_ratio_alignment_mode == 1u) { tx = sx - 1.0; ty = sy - 1.0; }
    else if (aspect_ratio_alignment_mode == 2u) { tx = 1.0 - sx; ty = 1.0 - sy; }
    return translate_mat(tx, ty, 0.0) * scale_mat(sx, sy, 1.0);
}

struct StrokedPolygonUniforms {
    layer_size: vec2<f32>,
    camera_view: mat4x4<f32>,
    data_unit_mode_x: u32,
    data_unit_mode_y: u32,
    line_width: f32,
    line_width_unit_mode: u32,
    aspect_ratio_mode: u32,
    aspect_ratio_alignment_mode: u32,
    model_matrix: mat4x4<f32>,
    color: vec4<f32>,
};

struct VSOut {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

struct FSOut {
    @location(0) color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: StrokedPolygonUniforms;
@group(0) @binding(1) var<storage, read> src_x_coords:  array<f32>;
@group(0) @binding(2) var<storage, read> src_y_coords:  array<f32>;
@group(0) @binding(3) var<storage, read> dst_x_coords:  array<f32>;
@group(0) @binding(4) var<storage, read> dst_y_coords:  array<f32>;
@group(0) @binding(5) var<storage, read> prev_x_coords: array<f32>;
@group(0) @binding(6) var<storage, read> prev_y_coords: array<f32>;
@group(0) @binding(7) var<storage, read> next_x_coords: array<f32>;
@group(0) @binding(8) var<storage, read> next_y_coords: array<f32>;

// corner.x: -1 = source end, +1 = target end
// corner.y: -1 = one side,   +1 = other side
const QUAD: array<vec2<f32>, 4> = array<vec2<f32>, 4>(
    vec2<f32>(-1.0, -1.0),
    vec2<f32>( 1.0, -1.0),
    vec2<f32>(-1.0,  1.0),
    vec2<f32>( 1.0,  1.0),
);

const MITER_LIMIT: f32 = 4.0;

// Project a model-space point to pixel space, handling data/pixel unit modes.
// Pixel space: (0,0) = top-left of the layer viewport, (layer_w, layer_h) = bottom-right.
fn project_to_px(x: f32, y: f32) -> vec2<f32> {
    let layer_w = u.layer_size.x;
    let layer_h = u.layer_size.y;
    let aspect = layer_w / layer_h;

    let orig = u.model_matrix * vec4f(x, y, 0.0, 1.0);

    let NORM_TO_NDC = translate_mat(-1.0, -1.0, 0.0) * scale_mat(2.0, 2.0, 1.0);
    let NDC_TO_NORM = translate_mat( 0.5,  0.5, 0.0) * scale_mat(0.5, 0.5, 1.0);

    // Pixel-mode NDC (data treated as raw pixel coordinates)
    let norm_px = vec2<f32>(orig.x / layer_w, orig.y / layer_h);
    let ndc_px  = (NORM_TO_NDC * vec4f(norm_px, 0.0, 1.0)).xy;

    var ndc: vec2<f32>;
    if (u.data_unit_mode_x == 0u && u.data_unit_mode_y == 0u) {
        ndc = ndc_px;
    } else {
        let ASPECT_RATIO_MAT = get_aspect_ratio_mat(aspect, u.aspect_ratio_mode, u.aspect_ratio_alignment_mode);
        let mvp = ASPECT_RATIO_MAT * u.camera_view;
        let transform = NDC_TO_NORM * mvp * NORM_TO_NDC;
        let transformed = transform * orig;
        var ndc_data = (NORM_TO_NDC * vec4f(transformed.xy, 0.0, 1.0)).xy;

        if (u.data_unit_mode_x == 0u) { ndc_data.x = ndc_px.x; }
        if (u.data_unit_mode_y == 0u) { ndc_data.y = ndc_px.y; }
        ndc = ndc_data;
    }

    // NDC [-1,1] → pixel [0, layer_size]
    return (ndc + vec2<f32>(1.0, 1.0)) * 0.5 * vec2<f32>(layer_w, layer_h);
}

// Compute the miter-join corner position at `curr_px`.
// `prev_px` and `next_px` are the neighboring polygon vertices (in pixel space).
// `side` is -1.0 or +1.0 to select which side of the path.
// `half_width` is the stroke half-width in pixels.
// Returns the miter offset position in pixel space.
fn miter_corner_px(
    prev_px: vec2<f32>,
    curr_px: vec2<f32>,
    next_px: vec2<f32>,
    side: f32,
    half_width: f32,
) -> vec2<f32> {
    let delta_a = curr_px - prev_px;
    let len_a   = length(delta_a);
    let delta_b = next_px - curr_px;
    let len_b   = length(delta_b);

    // Both neighbors degenerate: can't compute a direction, return the vertex.
    if (len_a < 0.5 && len_b < 0.5) {
        return curr_px;
    }

    // Only one neighbor is valid: fall back to simple perpendicular extrusion.
    if (len_a < 0.5) {
        let d = delta_b / len_b;
        return curr_px + side * vec2<f32>(-d.y, d.x) * half_width;
    }
    if (len_b < 0.5) {
        let d = delta_a / len_a;
        return curr_px + side * vec2<f32>(-d.y, d.x) * half_width;
    }

    // Both neighbors valid: compute miter bisector.
    let dir_a  = delta_a / len_a;
    let dir_b  = delta_b / len_b;
    let perp_a = vec2<f32>(-dir_a.y, dir_a.x);
    let perp_b = vec2<f32>(-dir_b.y, dir_b.x);

    let miter_sum = perp_a + perp_b;
    let miter_len = length(miter_sum);

    // Near-180° hairpin (antiparallel segments): miter is degenerate, use perp_b.
    if (miter_len < 1e-6) {
        return curr_px + side * perp_b * half_width;
    }

    let miter_dir = miter_sum / miter_len;

    // sin of the half-angle between the two edge directions.
    // miter_scale = 1/sin(half_angle); clamped to MITER_LIMIT to prevent spikes.
    let sin_half    = dot(miter_dir, perp_b);
    let miter_scale = min(1.0 / max(sin_half, 1.0 / MITER_LIMIT), MITER_LIMIT);

    return curr_px + side * miter_dir * half_width * miter_scale;
}

@vertex
fn vs_main(
    @builtin(instance_index) instance_index: u32,
    @builtin(vertex_index)   vertex_index:   u32,
) -> VSOut {
    let corner = QUAD[vertex_index & 3u];

    let prev_px = project_to_px(prev_x_coords[instance_index], prev_y_coords[instance_index]);
    let src_px  = project_to_px(src_x_coords[instance_index],  src_y_coords[instance_index]);
    let dst_px  = project_to_px(dst_x_coords[instance_index],  dst_y_coords[instance_index]);
    let next_px = project_to_px(next_x_coords[instance_index], next_y_coords[instance_index]);

    let half_width = u.line_width * 0.5;
    let side = corner.y;

    var pos_px: vec2<f32>;
    if (corner.x < 0.0) {
        // Source end: miter using prev→src→dst
        pos_px = miter_corner_px(prev_px, src_px, dst_px, side, half_width);
    } else {
        // Target end: miter using src→dst→next
        pos_px = miter_corner_px(src_px, dst_px, next_px, side, half_width);
    }

    // Pixel [0, layer_size] → NDC [-1, 1]
    let pos_ndc = pos_px / u.layer_size * 2.0 - vec2<f32>(1.0, 1.0);

    var out: VSOut;
    out.position = vec4f(pos_ndc, 0.0, 1.0);
    out.color = u.color;
    return out;
}

@fragment
fn fs_main(
    @builtin(position) frag_coord: vec4<f32>,
    @location(0) color_in: vec4<f32>,
) -> FSOut {
    var out: FSOut;
    out.color = color_in;
    return out;
}
