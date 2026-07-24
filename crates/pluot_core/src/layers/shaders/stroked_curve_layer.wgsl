// StrokedCurveLayer shader.
// Renders a pre-flattened polyline with round joins and round caps, eliminating
// the transparent gaps that appear when adjacent rectangle quads meet at an angle.
//
// Approach: 4-point window instancing (adapted from webgpu-instanced-lines).
// Each instance draws segment B->C plus half of the join geometry at both ends.
// The geometry is a triangle strip; even-indexed strip vertices are outer arc
// points, odd-indexed strip vertices are center points, and one special vertex
// per half is the inner miter corner that fills the concave side of the join.
//
// Constants (fixed at compile time):
//   JOIN_RESOLUTION = 8   -> 16 arc steps per join/cap
//   VERTS_PER_HALF  = 19  -> (16 + 3) vertices per strip half
//   VERTS_PER_INSTANCE = 38
//
// CPU side: all sub-paths are packed into a single flat points buffer. A
// companion segments buffer holds one entry per segment: [poly_start, poly_end,
// local_b], where local_b is the 0-based segment index within its polyline.
// Instance i draws the segment from points[poly_start+local_b] to
// points[poly_start+local_b+1].

// Shared projection helpers (identical to curve_layer.wgsl)

// The following functions are injected at compile time by the shader-module
// system (see `crate::shader_modules`). Their sources live in `wgsl_functions/`.
{{scale}}

{{translate}}

{{get_aspect_ratio_mat}}

// flat_texel_coord(idx, width): maps a flat element index to 2D texel coords.
// Used by the color module below to read per-element color value textures.
{{flat_texel_coord}}

struct StrokedCurveLayerUniforms {
    layer_size: vec2<f32>,          // (layer_width, layer_height) in pixels
    camera_view: mat4x4<f32>,
    data_unit_mode_x: u32,          // 0: px, 1: data, 2: normalized
    data_unit_mode_y: u32,          // 0: px, 1: data, 2: normalized
    stroke_width: f32,              // stroke width (pixels, data units, or normalized fraction, per stroke_width_unit_mode)
    stroke_width_unit_mode: u32,    // 0: px, 1: data coordinate system units, 2: normalized (0-1) units
    aspect_ratio_mode: u32,         // 0: ignore, 1: contain, 2: cover
    aspect_ratio_alignment_mode: u32,
    model_matrix: mat4x4<f32>,
    stroke_color_mode: u32,         // see ColorMode::shader_mode()
    stroke_color: vec4<f32>,        // rgba color used by the UniformRgb mode
    stroke_color_reverse: u32,      // 1 = reverse the quantitative colormap
    stroke_color_domain: vec2<f32>, // (min, max) normalization domain for quantitative mode
    stroke_opacity: f32,
}

// Per-segment metadata: indices into the flat points buffer.
// Stride = 12 bytes (3 × u32); matches the CPU-side Vec<u32> layout.
struct SegmentEntry {
    poly_start: u32,   // index of the first point of this polyline in `points`
    poly_end: u32,     // index of the last point (inclusive)
    local_b: u32,      // 0-based segment index within its polyline (B = poly_start + local_b)
}

@group(0) @binding(0) var<uniform> u: StrokedCurveLayerUniforms;
@group(0) @binding(1) var<storage, read> points: array<vec2<f32>>;
@group(0) @binding(2) var<storage, read> segments: array<SegmentEntry>;

// Color module: any per-element color value/palette texture bindings (from
// binding 3 onward) plus `fn get_stroke_color(instance_index: u32) -> vec3<f32>`.
// Assembled per color mode by `crate::color_mode::prepare_stroke_color`. A
// `StrokedCurveLayer` renders a single shape, so the shader always resolves
// element 0.
{{stroke_color_module}}

// Stroke width module: an optional value texture (instanced mode) plus
// `fn get_stroke_width(poly_index: u32) -> f32`. Assembled per size mode by
// `crate::scalar_mode::prepare_stroke_width_mode`.
{{stroke_width_module}}

// Stroke opacity module: an optional value texture (instanced mode) plus
// `fn get_stroke_opacity(poly_index: u32) -> f32`. Assembled per opacity mode by
// `crate::scalar_mode::prepare_stroke_opacity_mode`.
{{stroke_opacity_module}}

struct VSOut {
    @builtin(position) position: vec4<f32>,
}
struct FSOut {
    @location(0) color: vec4<f32>,
}

// Constants

// Number of arc steps per join half. Higher = smoother but more vertices.
const JOIN_RESOLUTION: u32 = 8u;
// MAX_RES = JOIN_RESOLUTION * 2; used in index arithmetic (must stay f32).
const MAX_RES_F: f32 = 16.0;
// Vertices per strip half = MAX_RES + 3.
const VERTS_PER_HALF_F: f32 = 19.0;
// Vertices per full instance = VERTS_PER_HALF * 2.
const VERTS_PER_INSTANCE_F: f32 = 38.0;
const PI: f32 = 3.141592653589793;

// Helpers

fn project_point(model_point: vec2<f32>, layer_aspect_ratio: f32) -> vec2<f32> {
    let point_pos_orig = u.model_matrix * vec4f(model_point.x, model_point.y, 0.0, 1.0);

    let lw = u.layer_size.x;
    let lh = u.layer_size.y;

    let ASPECT_RATIO_MAT = get_aspect_ratio_mat(layer_aspect_ratio, u.aspect_ratio_mode, u.aspect_ratio_alignment_mode);
    let NORM_TO_NDC = translate(-1.0, -1.0, 0.0) * scale(2.0, 2.0, 1.0);
    let NDC_TO_NORM = translate(0.5, 0.5, 0.0) * scale(0.5, 0.5, 1.0);

    // Pixel-mode points are in pixel coordinates and are converted to normalized
    // (0 to 1) coordinates within the layer by dividing by the layer size.
    // Normalized-mode points are already in (0 to 1) coordinates, so are used as-is.
    let pos_norm_px = vec2<f32>(
        select(point_pos_orig.x / lw, point_pos_orig.x, u.data_unit_mode_x == 2u),
        select(point_pos_orig.y / lh, point_pos_orig.y, u.data_unit_mode_y == 2u)
    );
    let pos_ndc_px = (NORM_TO_NDC * vec4f(pos_norm_px, 0.0, 1.0)).xy;

    if (u.data_unit_mode_x != 1u && u.data_unit_mode_y != 1u) {
        return pos_ndc_px;
    }

    let mvp = ASPECT_RATIO_MAT * u.camera_view;
    let t = NDC_TO_NORM * mvp * NORM_TO_NDC;
    let pos_norm_data = t * point_pos_orig;
    var pos_ndc_data = (NORM_TO_NDC * vec4f(pos_norm_data.xy, 0.0, 1.0)).xy;

    if (u.data_unit_mode_x != 1u) { pos_ndc_data.x = pos_ndc_px.x; }
    if (u.data_unit_mode_y != 1u) { pos_ndc_data.y = pos_ndc_px.y; }
    return pos_ndc_data;
}

// Convert NDC [-1,1]^2 -> pixel coordinates [0,layer_size].
fn ndc_to_px(ndc: vec2<f32>) -> vec2<f32> {
    return (ndc + vec2<f32>(1.0, 1.0)) * 0.5 * u.layer_size;
}

// Convert pixel coordinates back to NDC.
fn px_to_ndc(px: vec2<f32>) -> vec2<f32> {
    return px / u.layer_size * 2.0 - vec2<f32>(1.0, 1.0);
}

// Vertex shader

@vertex
fn vs_main(
    @builtin(instance_index) instance_index: u32,
    @builtin(vertex_index) vertex_index: u32,
) -> VSOut {
    var out: VSOut;

    let seg = segments[instance_index];
    let poly_start = i32(seg.poly_start);
    let poly_end = i32(seg.poly_end);

    // 4-point window: A (prev) -> B (start) -> C (end) -> D (next)
    // B = poly_start + local_b; indices are absolute into the flat points buffer.
    let A_idx = i32(seg.local_b) - 1 + poly_start;
    let B_idx = i32(seg.local_b)     + poly_start;
    let C_idx = i32(seg.local_b) + 1 + poly_start;
    let D_idx = i32(seg.local_b) + 2 + poly_start;

    let aOutOfBounds = A_idx < poly_start;
    let dOutOfBounds = D_idx > poly_end;

    let aspect = u.layer_size.x / u.layer_size.y;

    // Fetch and project all four points (clamp OOB indices; validity tracked separately).
    var pA = ndc_to_px(project_point(points[u32(clamp(A_idx, poly_start, poly_end))], aspect));
    var pB = ndc_to_px(project_point(points[u32(clamp(B_idx, poly_start, poly_end))], aspect));
    var pC = ndc_to_px(project_point(points[u32(clamp(C_idx, poly_start, poly_end))], aspect));
    var pD = ndc_to_px(project_point(points[u32(clamp(D_idx, poly_start, poly_end))], aspect));

    // Stroke width (uniform or instanced; a single shape always resolves
    // element 0), resolved to pixels here.
    let stroke_width = get_stroke_width(0u);
    var stroke_width_px = stroke_width;
    if (u.stroke_width_unit_mode == 1u) {
        // Data-coordinate width: transform the width delta through the same
        // pipeline as positions, but with w=0 so translations cancel out (it is
        // a size, not a position). Stroke width is height-relative, so use the Y
        // component of the transformed delta and scale it back to pixels.
        let lw = u.layer_size.x;
        let lh = u.layer_size.y;
        let NORM_TO_NDC = translate(-1.0, -1.0, 0.0) * scale(2.0, 2.0, 1.0);
        let NDC_TO_NORM = translate(0.5, 0.5, 0.0) * scale(0.5, 0.5, 1.0);
        let ASPECT_RATIO_MAT = get_aspect_ratio_mat(aspect, u.aspect_ratio_mode, u.aspect_ratio_alignment_mode);
        let mvp = ASPECT_RATIO_MAT * u.camera_view;
        let width_orig = u.model_matrix * vec4f(stroke_width, stroke_width, 0.0, 0.0);
        let width_norm = (NDC_TO_NORM * mvp * NORM_TO_NDC) * width_orig;
        stroke_width_px = abs(width_norm.y) * lh;
    } else if (u.stroke_width_unit_mode == 2u) {
        // Normalized-mode width: stroke_width is a fraction (0 to 1) of the
        // layer height, independent of the camera.
        stroke_width_px = stroke_width * u.layer_size.y;
    }

    let half_width = stroke_width_px * 0.5;

    var aInvalid = aOutOfBounds;
    var dInvalid = dOutOfBounds;

    // Determine which half of the triangle strip this vertex belongs to.
    // The strip is symmetric: the C-side is computed by swapping B and C (mirror).
    let idx = f32(vertex_index);
    let mirror = idx >= VERTS_PER_HALF_F;
    let mirrorSign = select(1.0, -1.0, mirror);

    if (mirror) {
        let tp = pC; pC = pB; pB = tp;
        let td = pD; pD = pA; pA = td;
        let ti = dInvalid; dInvalid = aInvalid; aInvalid = ti;
    }

    // Handle line endpoints: caps (aInvalid) and tangent extrapolation (dInvalid).
    // For caps: reflect A across B so the A->B tangent opposes B->C, producing a
    // 180-degree arc (semicircle). For dInvalid: extrapolate D to get a tangent.
    let isCap = aInvalid; // we always insert round caps

    if (aInvalid) {
        pA = pC; // reflect -> tAB = -tBC -> 180-degree cap geometry
    }
    if (dInvalid) {
        pD = 2.0 * pC - pB; // extrapolate D for consistent tangent at C
    }

    // Tangent and normal vectors (all in pixel space).
    var tBC = pC - pB;
    let lBC = length(tBC);
    if (lBC > 1e-6) { tBC = tBC / lBC; }
    let nBC = vec2<f32>(-tBC.y, tBC.x);

    var tAB = pB - pA;
    let lAB = length(tAB);
    if (lAB > 1e-6) { tAB = tAB / lAB; }
    let nAB = vec2<f32>(-tAB.y, tAB.x);

    let cosB = clamp(dot(tAB, tBC), -1.0, 1.0);

    // Turn direction at B: positive = CCW (outer join on the left).
    var dirB = -dot(tBC, nAB); // 2D cross product
    let bCollinear = abs(dirB) < 1e-4;
    let bIsHairpin = bCollinear && cosB < 0.0;
    dirB = select(sign(dirB), -mirrorSign, bCollinear);

    // Miter bisector vector (points toward the outer join corner).
    var miter = select(0.5 * (nAB + nBC) * dirB, -tBC, bIsHairpin);

    // Map vertex_index -> join fan index i.
    // Even i -> outer arc vertex; odd i -> center vertex; i==MAX_RES+1 -> inner miter.
    var i = select(idx, VERTS_PER_INSTANCE_F - idx, mirror);
    i = i + select(0.0, -1.0, dirB < 0.0);
    i = i - select(0.0, 1.0, mirror);
    i = max(0.0, i);

    // Build local coordinate basis for vertex offset.
    //   xBasis: tangent direction (for miter extension along segment)
    //   yBasis: outward normal direction (for line width)
    var xBasis = tBC;
    var yBasis = nBC * dirB;
    var xy = vec2<f32>(0.0, 0.0); // offset in (xBasis, yBasis) space

    if (i == MAX_RES_F + 1.0) {
        // Inner miter corner: fills the concave side of the join.
        // m = tan(half turning angle); clamped so it doesn't exceed segment lengths.
        let cross_val = tAB.x * tBC.y - tAB.y * tBC.x;
        let m = select(cross_val / (1.0 + cosB), 0.0, cosB <= -0.9999);
        let max_ext = select(min(lBC, lAB) / max(half_width, 1e-6), 1e9, half_width < 1e-6);
        xy = vec2<f32>(min(abs(m), max_ext), -1.0);
    } else {
        // Join / cap arc geometry.
        // Switch to miter-aligned basis: yBasis along the bisector, xBasis perpendicular.
        let m2 = dot(miter, miter);
        let lm = sqrt(m2);
        if (lm > 1e-6) {
            yBasis = miter / lm;
            xBasis = dirB * vec2<f32>(yBasis.y, -yBasis.x);
        }

        // miterLimit² = 4² = 16; if miter vector is very short, fall back to bevel.
        let isBevel = 1.0 > 16.0 * m2;

        if (i % 2.0 == 0.0) {
            // Outer arc vertex: sweep from one edge normal to the other.
            // t (in range [0,1]) parameterizes the arc; capMult doubles the sweep for caps.
            // theta = angle in the miter-aligned frame.
            let t = clamp(i, 0.0, MAX_RES_F) / MAX_RES_F;
            let capMult = select(1.0, 2.0, isCap);
            let theta = -0.5 * (acos(cosB) * t - PI) * capMult;
            xy = vec2<f32>(cos(theta), sin(theta));
        }
        // Odd vertex: center of the fan (xy stays at (0,0)).
        // (For non-round joins there would be a bevel offset, but we always use round.)
    }

    // Apply offset in pixel space, convert back to NDC.
    let dP = xBasis * xy.x + yBasis * xy.y;
    let pos_px = pB + half_width * dP;
    let pos_ndc = px_to_ndc(pos_px);

    out.position = vec4<f32>(pos_ndc.x, pos_ndc.y, 0.0, 1.0);
    return out;
}

@fragment
fn fs_main(
    @builtin(position) frag_coord: vec4<f32>,
) -> FSOut {
    // A single shape shares one color / opacity, so this always resolves element 0.
    let out_color = get_stroke_color(0u);
    let stroke_opacity = get_stroke_opacity(0u);

    var out: FSOut;
    out.color = vec4<f32>(out_color, stroke_opacity);
    return out;
}
