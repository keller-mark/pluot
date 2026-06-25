// StrokedPolygonLayer shader.
// Geometry: identical to the LineLayer (4-vertex quad per segment, TriangleStrip),
// which is efficient for straight polygon edges that do not need bezier subdivision.
// Color: solid from the uniform (no per-instance label / categorical colormap).

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

fn extrude_line(
    source_ndc: vec2<f32>,
    target_ndc: vec2<f32>,
    corner: vec2<f32>,
    line_width_ndc: f32,
    viewport_aspect_ratio: f32,
) -> vec2<f32> {
    var dir = target_ndc - source_ndc;
    dir.y /= viewport_aspect_ratio;
    dir = normalize(dir);
    let normal = vec2<f32>(-dir.y, dir.x);
    let extrusion = vec2<f32>(normal.x / viewport_aspect_ratio, normal.y) * line_width_ndc * 0.5;
    let base_point = mix(source_ndc, target_ndc, (corner.x + 1.0) / 2.0);
    return base_point + corner.y * extrusion;
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
@group(0) @binding(1) var<storage, read> source_x_coords: array<f32>;
@group(0) @binding(2) var<storage, read> source_y_coords: array<f32>;
@group(0) @binding(3) var<storage, read> target_x_coords: array<f32>;
@group(0) @binding(4) var<storage, read> target_y_coords: array<f32>;

const QUAD: array<vec2<f32>, 4> = array<vec2<f32>, 4>(
    vec2<f32>(-1.0, -1.0),
    vec2<f32>( 1.0, -1.0),
    vec2<f32>(-1.0,  1.0),
    vec2<f32>( 1.0,  1.0),
);

@vertex
fn vs_main(
    @builtin(instance_index) instance_index: u32,
    @builtin(vertex_index) vertex_index: u32,
) -> VSOut {
    let source_orig = u.model_matrix * vec4f(source_x_coords[instance_index], source_y_coords[instance_index], 0.0, 1.0);
    let target_orig = u.model_matrix * vec4f(target_x_coords[instance_index], target_y_coords[instance_index], 0.0, 1.0);

    let corner = QUAD[vertex_index & 3u];

    let layer_w = u.layer_size.x;
    let layer_h = u.layer_size.y;
    let layer_aspect_ratio = layer_w / layer_h;

    let ASPECT_RATIO_MAT = get_aspect_ratio_mat(layer_aspect_ratio, u.aspect_ratio_mode, u.aspect_ratio_alignment_mode);
    let NORM_TO_NDC = translate_mat(-1.0, -1.0, 0.0) * scale_mat(2.0, 2.0, 1.0);
    let NDC_TO_NORM = translate_mat( 0.5,  0.5, 0.0) * scale_mat(0.5, 0.5, 1.0);

    var src_px = vec2<f32>(0.0);
    var dst_px = vec2<f32>(0.0);
    var src_data = vec2<f32>(0.0);
    var dst_data = vec2<f32>(0.0);

    if (u.data_unit_mode_x == 0u || u.data_unit_mode_y == 0u) {
        let src_norm = vec2<f32>(source_orig.x / layer_w, source_orig.y / layer_h);
        let dst_norm = vec2<f32>(target_orig.x / layer_w, target_orig.y / layer_h);
        src_px = (NORM_TO_NDC * vec4f(src_norm, 0.0, 1.0)).xy;
        dst_px = (NORM_TO_NDC * vec4f(dst_norm, 0.0, 1.0)).xy;

        if (u.data_unit_mode_x == 0u && u.data_unit_mode_y == 0u) {
            let line_width_ndc = u.line_width / layer_h * 2.0;
            let pos_ndc = extrude_line(src_px, dst_px, corner, line_width_ndc, layer_aspect_ratio);
            var out: VSOut;
            out.position = vec4f(pos_ndc, 0.0, 1.0);
            out.color = u.color;
            return out;
        }
    }

    let mvp = ASPECT_RATIO_MAT * u.camera_view;
    let transform = NDC_TO_NORM * mvp * NORM_TO_NDC;

    let src_t = transform * source_orig;
    let dst_t = transform * target_orig;
    src_data = (NORM_TO_NDC * vec4f(src_t.xy, 0.0, 1.0)).xy;
    dst_data = (NORM_TO_NDC * vec4f(dst_t.xy, 0.0, 1.0)).xy;

    if (u.data_unit_mode_x == 0u) { src_data.x = src_px.x; dst_data.x = dst_px.x; }
    if (u.data_unit_mode_y == 0u) { src_data.y = src_px.y; dst_data.y = dst_px.y; }

    let line_width_ndc = u.line_width / layer_h * 2.0;
    let pos_ndc = extrude_line(src_data, dst_data, corner, line_width_ndc, layer_aspect_ratio);

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
