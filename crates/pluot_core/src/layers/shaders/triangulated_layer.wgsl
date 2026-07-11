// TriangulatedLayer shader.
// Projects pre-triangulated fill geometry (flat list of model-space vertices,
// 3 per triangle) through the same projection pipeline as the stroke and shades
// each triangle with the fill color.

// The following functions are injected at compile time by the shader-module
// system (see `crate::shader_modules`). Their sources live in `wgsl_functions/`.
{{scale}}

{{translate}}

{{get_aspect_ratio_mat}}

struct TriangulatedLayerUniforms {
    layer_size: vec2<f32>,
    camera_view: mat4x4<f32>,
    data_unit_mode_x: u32,
    data_unit_mode_y: u32,
    aspect_ratio_mode: u32,
    aspect_ratio_alignment_mode: u32,
    model_matrix: mat4x4<f32>,
    fill_color: vec4<f32>,
}

@group(0) @binding(0) var<uniform> u: TriangulatedLayerUniforms;
@group(0) @binding(1) var<storage, read> vertices: array<vec2<f32>>;

struct VSOut {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
}

struct FSOut {
    @location(0) color: vec4<f32>,
}

fn project_point(model_point: vec2<f32>, layer_aspect_ratio: f32) -> vec2<f32> {
    let point_pos_orig = u.model_matrix * vec4f(model_point.x, model_point.y, 0.0, 1.0);

    let layer_width_px = u.layer_size.x;
    let layer_height_px = u.layer_size.y;

    let ASPECT_RATIO_MAT = get_aspect_ratio_mat(layer_aspect_ratio, u.aspect_ratio_mode, u.aspect_ratio_alignment_mode);
    let NORM_TO_NDC_MAT = translate(-1.0, -1.0, 0.0) * scale(2.0, 2.0, 1.0);
    let NDC_TO_NORM_MAT = translate(0.5, 0.5, 0.0) * scale(0.5, 0.5, 1.0);

    let point_pos_norm_px = vec2<f32>(point_pos_orig.x / layer_width_px, point_pos_orig.y / layer_height_px);
    let pos_ndc_px = (NORM_TO_NDC_MAT * vec4f(point_pos_norm_px.xy, 0.0, 1.0)).xy;

    if (u.data_unit_mode_x == 0u && u.data_unit_mode_y == 0u) {
        return pos_ndc_px;
    }

    let model_view_projection = ASPECT_RATIO_MAT * u.camera_view;
    let transform_mat = (NDC_TO_NORM_MAT * model_view_projection * NORM_TO_NDC_MAT);
    let pos_norm = transform_mat * point_pos_orig;
    var pos_ndc_data = (NORM_TO_NDC_MAT * vec4f(pos_norm.xy, 0.0, 1.0)).xy;

    if (u.data_unit_mode_x == 0u) {
        pos_ndc_data.x = pos_ndc_px.x;
    }
    if (u.data_unit_mode_y == 0u) {
        pos_ndc_data.y = pos_ndc_px.y;
    }
    return pos_ndc_data;
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VSOut {
    let model_point = vertices[vertex_index];
    let layer_aspect_ratio = u.layer_size.x / u.layer_size.y;
    let pos_ndc = project_point(model_point, layer_aspect_ratio);

    var out: VSOut;
    out.position = vec4f(pos_ndc.x, pos_ndc.y, 0.0, 1.0);
    out.color = u.fill_color;
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
