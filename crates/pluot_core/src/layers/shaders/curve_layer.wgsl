// CurveLayer shader.
// Inspired by the LineLayer shader (see shaders/line_layer.wgsl), which this borrows
// heavily from. The key difference: instead of a single straight segment per instance,
// each instance renders one straight sub-segment of a cubic Bezier curve.
//
// On the CPU side, the input SVG-like path commands are flattened into a list of cubic
// Bezier segments (every line/quadratic/arc is converted to one or more cubics). Each
// segment contributes `subdivisions` sub-segments. We instance over
// (num_segments * subdivisions), evaluate the Bezier at the two endpoints of the
// sub-segment in the vertex shader, then extrude a quad just like the LineLayer.
// This keeps the per-curve work on the GPU and scales to many/long curves.

fn scale(x: f32, y: f32, z: f32) -> mat4x4<f32> {
  return mat4x4<f32>(
    vec4<f32>(x, 0.0, 0.0, 0.0),
    vec4<f32>(0.0, y, 0.0, 0.0),
    vec4<f32>(0.0, 0.0, z, 0.0),
    vec4<f32>(0.0, 0.0, 0.0, 1.0)
  );
}

fn translate(x: f32, y: f32, z: f32) -> mat4x4<f32> {
  return mat4x4<f32>(
    vec4<f32>(1.0, 0.0, 0.0, 0.0),
    vec4<f32>(0.0, 1.0, 0.0, 0.0),
    vec4<f32>(0.0, 0.0, 1.0, 0.0),
    vec4<f32>(x, y, z, 1.0),
  );
}

fn get_aspect_ratio_mat(layer_aspect_ratio: f32, aspect_ratio_mode: u32, aspect_ratio_alignment_mode: u32) -> mat4x4<f32> {
    var x_scale_for_aspect_ratio_mode = 1.0;
    var y_scale_for_aspect_ratio_mode = 1.0;
    if (aspect_ratio_mode == 1u) {
        // fit/contain
        if (layer_aspect_ratio > 1.0) {
            x_scale_for_aspect_ratio_mode = 1.0 / layer_aspect_ratio;
        } else if(layer_aspect_ratio < 1.0) {
            y_scale_for_aspect_ratio_mode = layer_aspect_ratio;
        }
    } else if (aspect_ratio_mode == 2u) {
        // fill/cover
        if(layer_aspect_ratio > 1.0) {
            y_scale_for_aspect_ratio_mode = layer_aspect_ratio;
        } else if(layer_aspect_ratio < 1.0) {
            x_scale_for_aspect_ratio_mode = 1.0 / layer_aspect_ratio;
        }
    }

    var x_translation_for_aspect_ratio_alignment_mode = 0.0;
    var y_translation_for_aspect_ratio_alignment_mode = 0.0;
    if (aspect_ratio_alignment_mode == 1u) {
        // start
        x_translation_for_aspect_ratio_alignment_mode = x_scale_for_aspect_ratio_mode - 1.0;
        y_translation_for_aspect_ratio_alignment_mode = y_scale_for_aspect_ratio_mode - 1.0;
    } else if (aspect_ratio_alignment_mode == 2u) {
        // end
        x_translation_for_aspect_ratio_alignment_mode = 1.0 - x_scale_for_aspect_ratio_mode;
        y_translation_for_aspect_ratio_alignment_mode = 1.0 - y_scale_for_aspect_ratio_mode;
    }

    return translate(
        x_translation_for_aspect_ratio_alignment_mode,
        y_translation_for_aspect_ratio_alignment_mode,
        0.0
    ) * scale(
        x_scale_for_aspect_ratio_mode,
        y_scale_for_aspect_ratio_mode,
        1.0
    );
}

// Computes the final vertex position for a line quad. Identical to the LineLayer logic.
fn extrude_line(
    source_ndc: vec2<f32>,
    target_ndc: vec2<f32>,
    corner: vec2<f32>,
    line_width_ndc: f32,
    viewport_aspect_ratio: f32
) -> vec2<f32> {
    let p0 = source_ndc;
    let p1 = target_ndc;

    var dir = p1 - p0;
    dir.y /= viewport_aspect_ratio;
    // Guard against degenerate (zero-length) sub-segments, which can occur when a
    // Bezier control polygon collapses; normalize() of a zero vector is undefined.
    if (length(dir) < 1e-12) {
        dir = vec2<f32>(1.0, 0.0);
    } else {
        dir = normalize(dir);
    }

    let normal = vec2<f32>(-dir.y, dir.x);

    let extrusion = vec2<f32>(normal.x / viewport_aspect_ratio, normal.y) * line_width_ndc * 0.5;

    let base_point = mix(p0, p1, (corner.x + 1.0) / 2.0);
    return base_point + corner.y * extrusion;
}

// Evaluate a cubic Bezier curve at parameter t in [0, 1].
fn cubic_bezier(p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>, p3: vec2<f32>, t: f32) -> vec2<f32> {
    let mt = 1.0 - t;
    let mt2 = mt * mt;
    let t2 = t * t;
    return p0 * (mt2 * mt)
         + p1 * (3.0 * mt2 * t)
         + p2 * (3.0 * mt * t2)
         + p3 * (t2 * t);
}

struct CurveLayerUniforms {
    layer_size: vec2<f32>, // (layer_width, layer_height) in pixels
    camera_view: mat4x4<f32>,
    data_unit_mode_x: u32, // 0: px units, 1: data coordinate system units
    data_unit_mode_y: u32, // 0: px units, 1: data coordinate system units
    line_width: f32,
    line_width_unit_mode: u32, // 0: px units, 1: data coordinate system units // TODO: use this
    aspect_ratio_mode: u32, // 0: ignore/squeeze, 1: fit/contain, 2: fill/cover.
    aspect_ratio_alignment_mode: u32, // 0: center, 1: start, 2: end
    subdivisions: u32, // number of straight sub-segments per cubic Bezier segment
    model_matrix: mat4x4<f32>,
    color: vec4<f32>, // rgba stroke color for the curve
};

struct VSOut {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

struct FSOut {
    @location(0) color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: CurveLayerUniforms;
// Flat list of cubic control points: 4 consecutive vec2 per Bezier segment
// (p0, p1, p2, p3, then the next segment's p0, ...).
@group(0) @binding(1) var<storage, read> control_points: array<vec2<f32>>;

// 4 corners of a unit quad for triangle strip: (-1,-1), (1,-1), (-1,1), (1,1)
const QUAD: array<vec2<f32>, 4> = array<vec2<f32>, 4>(
    vec2<f32>(-1.0, -1.0),
    vec2<f32>( 1.0, -1.0),
    vec2<f32>(-1.0,  1.0),
    vec2<f32>( 1.0,  1.0)
);

// Transform a single model-space point through the same pipeline as the LineLayer,
// returning its position in NDC (-1..1) space. Supports per-axis pixel/data unit mixing.
fn project_point(model_point: vec2<f32>, layer_aspect_ratio: f32) -> vec2<f32> {
    let point_pos_orig = u.model_matrix * vec4f(model_point.x, model_point.y, 0.0, 1.0);

    let layer_width_px = u.layer_size.x;
    let layer_height_px = u.layer_size.y;

    let ASPECT_RATIO_MAT = get_aspect_ratio_mat(
        layer_aspect_ratio,
        u.aspect_ratio_mode,
        u.aspect_ratio_alignment_mode
    );

    let NORM_TO_NDC_MAT = translate(-1.0, -1.0, 0.0) * scale(2.0, 2.0, 1.0);
    let NDC_TO_NORM_MAT = translate(0.5, 0.5, 0.0) * scale(0.5, 0.5, 1.0);

    // NDC position assuming pixel-unit positioning (camera / aspect-ratio ignored).
    let point_pos_norm_px = vec2<f32>(
        point_pos_orig.x / layer_width_px,
        point_pos_orig.y / layer_height_px
    );
    let pos_ndc_px = (NORM_TO_NDC_MAT * vec4f(point_pos_norm_px.xy, 0.0, 1.0)).xy;

    if (u.data_unit_mode_x == 0u && u.data_unit_mode_y == 0u) {
        return pos_ndc_px;
    }

    // NDC position assuming data-unit positioning (camera + aspect ratio applied).
    let model_view_projection = ASPECT_RATIO_MAT * u.camera_view;
    let transform_mat = (NDC_TO_NORM_MAT * model_view_projection * NORM_TO_NDC_MAT);
    let pos_norm = transform_mat * point_pos_orig;
    var pos_ndc_data = (NORM_TO_NDC_MAT * vec4f(pos_norm.xy, 0.0, 1.0)).xy;

    // Mix pixel/data positioning per axis.
    if (u.data_unit_mode_x == 0u) {
        pos_ndc_data.x = pos_ndc_px.x;
    }
    if (u.data_unit_mode_y == 0u) {
        pos_ndc_data.y = pos_ndc_px.y;
    }
    return pos_ndc_data;
}

@vertex
fn vs_main(
    @builtin(instance_index) instance_index: u32,
    @builtin(vertex_index) vertex_index: u32
) -> VSOut {
    // Decompose the instance index into (segment, sub-segment).
    let segment_index = instance_index / u.subdivisions;
    let sub_index = instance_index % u.subdivisions;

    // Fetch this segment's 4 cubic control points (in model/data space).
    let base = segment_index * 4u;
    let p0 = control_points[base + 0u];
    let p1 = control_points[base + 1u];
    let p2 = control_points[base + 2u];
    let p3 = control_points[base + 3u];

    // Parametric endpoints of this sub-segment.
    let t0 = f32(sub_index) / f32(u.subdivisions);
    let t1 = f32(sub_index + 1u) / f32(u.subdivisions);

    // Evaluate the curve, producing a straight segment to extrude.
    let source_model = cubic_bezier(p0, p1, p2, p3, t0);
    let target_model = cubic_bezier(p0, p1, p2, p3, t1);

    let layer_aspect_ratio = u.layer_size.x / u.layer_size.y;

    let source_ndc = project_point(source_model, layer_aspect_ratio);
    let target_ndc = project_point(target_model, layer_aspect_ratio);

    let corner = QUAD[vertex_index & 3u];

    // TODO: Handle line_width_unit_mode == 1 (data coordinates).
    let line_width_ndc = u.line_width / u.layer_size.y * 2.0;

    let point_pos_ndc = extrude_line(
        source_ndc,
        target_ndc,
        corner,
        line_width_ndc,
        layer_aspect_ratio
    );

    var out: VSOut;
    out.position = vec4f(point_pos_ndc.x, point_pos_ndc.y, 0.0, 1.0);
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
