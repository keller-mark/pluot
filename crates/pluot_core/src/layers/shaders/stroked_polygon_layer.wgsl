// StrokedPolygonLayer shader.
// Renders polygon outlines with miter joins at vertices, eliminating the gaps
// and overlaps that appear when adjacent rectangle quads meet at an angle.
//
// Approach: instanced rendering with 4 vertices (TriangleStrip quad) per edge.
// Each instance reads ring metadata from the `segments` buffer and looks up
// prev/src/dst/next via modular index arithmetic into the shared `points` buffer.
// Adjacent segments share the same miter corners at their common vertex, so quads
// tile seamlessly without gaps or overlaps.
//
// GPU buffers (2 storage buffers, no redundant data):
//   points:   flat interleaved [x0,y0, x1,y1, …] for all ring vertices
//   segments: per-edge [ring_start, ring_end, local_idx] metadata
//
// All geometry is computed in pixel space. Miter extension is clamped to
// MITER_LIMIT × half-width to avoid spikes at very sharp angles.

// The following functions are injected at compile time by the shader-module
// system (see `crate::shader_modules`). Their sources live in `wgsl_functions/`.
{{scale}}

{{translate}}

{{get_aspect_ratio_mat}}

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

// Per-edge ring metadata. ring_start and ring_end are absolute vertex indices
// into `points`; local_idx is the 0-based index of this edge's source vertex
// within its ring, so the source vertex is at points[ring_start + local_idx].
struct SegmentEntry {
    ring_start: u32,
    ring_end:   u32,
    local_idx:  u32,
};

struct VSOut {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

struct FSOut {
    @location(0) color: vec4<f32>,
};

@group(0) @binding(0) var<uniform>       u:        StrokedPolygonUniforms;
// The interleaved vertex coordinates [x0, y0, x1, y1, …] are uploaded as a
// single-channel (red-only) 2D texture holding the flat array reshaped into rows:
// flat element `idx` lives at texel `(idx % width, idx / width)`. The data is NOT
// reordered on the CPU, so the shader recomputes the 2D texel coords. The texture's
// sampled type is injected at runtime by the shader-module system (see
// `crate::shader_modules`) so that 8/16/32-bit data lives on the GPU at native
// width: `f32` for floating-point data, `u32` for unsigned, `i32` for signed.
@group(0) @binding(1) var points: texture_2d<{{points_dtype}}>;
@group(0) @binding(2) var<storage, read> segments: array<SegmentEntry>;

// Load the vertex at index `idx` from the interleaved coordinate texture:
// its x is at flat index 2*idx and its y at 2*idx + 1. `f32(...)` is a no-op
// when the injected sampled type is already f32, and widens u32/i32 otherwise.
fn load_point(idx: u32) -> vec2<f32> {
    let w = textureDimensions(points).x;
    let xi = 2u * idx;
    let yi = xi + 1u;
    let x = f32(textureLoad(points, vec2<u32>(xi % w, xi / w), 0).x);
    let y = f32(textureLoad(points, vec2<u32>(yi % w, yi / w), 0).x);
    return vec2<f32>(x, y);
}

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
// Pixel space: (0,0) = top-left of the layer viewport, layer_size = bottom-right.
fn project_to_px(pt: vec2<f32>) -> vec2<f32> {
    let layer_w = u.layer_size.x;
    let layer_h = u.layer_size.y;
    let aspect  = layer_w / layer_h;

    let orig = u.model_matrix * vec4f(pt.x, pt.y, 0.0, 1.0);

    let NORM_TO_NDC = translate(-1.0, -1.0, 0.0) * scale(2.0, 2.0, 1.0);
    let NDC_TO_NORM = translate( 0.5,  0.5, 0.0) * scale(0.5, 0.5, 1.0);

    let norm_px = vec2<f32>(orig.x / layer_w, orig.y / layer_h);
    let ndc_px  = (NORM_TO_NDC * vec4f(norm_px, 0.0, 1.0)).xy;

    var ndc: vec2<f32>;
    if (u.data_unit_mode_x == 0u && u.data_unit_mode_y == 0u) {
        ndc = ndc_px;
    } else {
        let ASPECT_RATIO_MAT = get_aspect_ratio_mat(aspect, u.aspect_ratio_mode, u.aspect_ratio_alignment_mode);
        let mvp       = ASPECT_RATIO_MAT * u.camera_view;
        let transform = NDC_TO_NORM * mvp * NORM_TO_NDC;
        var ndc_data  = (NORM_TO_NDC * vec4f((transform * orig).xy, 0.0, 1.0)).xy;

        if (u.data_unit_mode_x == 0u) { ndc_data.x = ndc_px.x; }
        if (u.data_unit_mode_y == 0u) { ndc_data.y = ndc_px.y; }
        ndc = ndc_data;
    }

    return (ndc + vec2<f32>(1.0, 1.0)) * 0.5 * vec2<f32>(layer_w, layer_h);
}

// Compute the miter-join corner at `curr_px` given pixel-space neighbors.
// `side` is -1.0 or +1.0; `half_width` is stroke half-width in pixels.
fn miter_corner_px(
    prev_px: vec2<f32>,
    curr_px: vec2<f32>,
    next_px: vec2<f32>,
    side:       f32,
    half_width: f32,
) -> vec2<f32> {
    let delta_a = curr_px - prev_px;
    let len_a   = length(delta_a);
    let delta_b = next_px - curr_px;
    let len_b   = length(delta_b);

    if (len_a < 0.5 && len_b < 0.5) { return curr_px; }

    if (len_a < 0.5) {
        let d = delta_b / len_b;
        return curr_px + side * vec2<f32>(-d.y, d.x) * half_width;
    }
    if (len_b < 0.5) {
        let d = delta_a / len_a;
        return curr_px + side * vec2<f32>(-d.y, d.x) * half_width;
    }

    let dir_a  = delta_a / len_a;
    let dir_b  = delta_b / len_b;
    let perp_a = vec2<f32>(-dir_a.y, dir_a.x);
    let perp_b = vec2<f32>(-dir_b.y, dir_b.x);

    let miter_sum = perp_a + perp_b;
    let miter_len = length(miter_sum);

    // Near-180-degree hairpin: miter is degenerate, fall back to perp_b.
    if (miter_len < 1e-6) {
        return curr_px + side * perp_b * half_width;
    }

    let miter_dir   = miter_sum / miter_len;
    let sin_half    = dot(miter_dir, perp_b);
    let miter_scale = min(1.0 / max(sin_half, 1.0 / MITER_LIMIT), MITER_LIMIT);
    return curr_px + side * miter_dir * half_width * miter_scale;
}

@vertex
fn vs_main(
    @builtin(instance_index) instance_index: u32,
    @builtin(vertex_index)   vertex_index:   u32,
) -> VSOut {
    let seg        = segments[instance_index];
    let ring_start = seg.ring_start;
    let ring_size  = seg.ring_end - ring_start + 1u;
    let li         = seg.local_idx;

    // Look up the four neighboring vertices with ring-wrap via modular arithmetic.
    let prev_pt = load_point(ring_start + (li + ring_size - 1u) % ring_size);
    let src_pt  = load_point(ring_start + li);
    let dst_pt  = load_point(ring_start + (li + 1u) % ring_size);
    let next_pt = load_point(ring_start + (li + 2u) % ring_size);

    let prev_px = project_to_px(prev_pt);
    let src_px  = project_to_px(src_pt);
    let dst_px  = project_to_px(dst_pt);
    let next_px = project_to_px(next_pt);

    let half_width = u.line_width * 0.5;
    let corner     = QUAD[vertex_index & 3u];
    let side       = corner.y;

    var pos_px: vec2<f32>;
    if (corner.x < 0.0) {
        // Source end: miter using prev->src->dst
        pos_px = miter_corner_px(prev_px, src_px, dst_px, side, half_width);
    } else {
        // Target end: miter using src->dst->next
        pos_px = miter_corner_px(src_px, dst_px, next_px, side, half_width);
    }

    let pos_ndc = pos_px / u.layer_size * 2.0 - vec2<f32>(1.0, 1.0);

    var out: VSOut;
    out.position = vec4f(pos_ndc, 0.0, 1.0);
    out.color    = u.color;
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
