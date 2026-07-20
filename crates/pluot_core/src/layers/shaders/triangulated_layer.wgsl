// TriangulatedLayer shader.
// Projects pre-triangulated fill geometry (flat list of model-space vertices,
// 3 per triangle) through the same projection pipeline as the stroke and shades
// each triangle with the fill color.

// The following functions are injected at compile time by the shader-module
// system (see `crate::shader_modules`). Their sources live in `wgsl_functions/`.
{{scale}}

{{translate}}

{{get_aspect_ratio_mat}}

// flat_texel_coord(idx, width): maps a flat element index to 2D texel coords.
// Used by the color module below to read per-element color value textures.
{{flat_texel_coord}}

struct TriangulatedLayerUniforms {
    layer_size: vec2<f32>,
    camera_view: mat4x4<f32>,
    data_unit_mode_x: u32,
    data_unit_mode_y: u32,
    aspect_ratio_mode: u32,
    aspect_ratio_alignment_mode: u32,
    model_matrix: mat4x4<f32>,
    fill_color_mode: u32, // see ColorMode::shader_mode()
    fill_color: vec4<f32>, // rgba color used by the UniformRgb mode
    fill_color_reverse: u32, // 1 = reverse the quantitative colormap
    fill_color_domain: vec2<f32>, // (min, max) normalization domain for quantitative mode
    fill_opacity: f32,
}

@group(0) @binding(0) var<uniform> u: TriangulatedLayerUniforms;
// The interleaved vertex coordinates [x0, y0, x1, y1, …] are uploaded as a
// single-channel (red-only) 2D texture holding the flat array reshaped into rows:
// flat element `idx` lives at texel `(idx % width, idx / width)`. The data is NOT
// reordered on the CPU, so the shader recomputes the 2D texel coords. The texture's
// sampled type is injected at runtime by the shader-module system (see
// `crate::shader_modules`) so that 8/16/32-bit data lives on the GPU at native
// width: `f32` for floating-point data, `u32` for unsigned, `i32` for signed.
@group(0) @binding(1) var vertices: texture_2d<{{vertices_dtype}}>;
// Per-vertex index into the fill_color color-mode arrays (see
// `TriangulatedLayerParams::vertex_color_index`), uploaded the same way.
@group(0) @binding(2) var vertex_color_index: texture_2d<{{vertex_color_index_dtype}}>;

// Load the flat coordinate value at index `idx`, widening it to f32.
// `f32(...)` is a no-op when the injected sampled type is already f32.
fn load_coord(idx: u32) -> f32 {
    let w = textureDimensions(vertices).x;
    return f32(textureLoad(vertices, flat_texel_coord(idx, w), 0).x);
}

// Load the color-mode element index for vertex `idx`, widening it to u32.
fn load_color_index(idx: u32) -> u32 {
    let w = textureDimensions(vertex_color_index).x;
    return u32(textureLoad(vertex_color_index, flat_texel_coord(idx, w), 0).x);
}

// Color module: any per-element color value/palette texture bindings (from
// binding 3 onward) plus `fn get_fill_color(color_index: u32) -> vec3<f32>`.
// Assembled per color mode by `crate::color_mode::prepare_color_mode`.
{{color_module}}

// Fill opacity module: an optional per-element opacity value texture (instanced
// mode) plus `fn get_fill_opacity(color_index: u32) -> f32`. Assembled per
// opacity mode by `crate::scalar_mode::prepare_fill_opacity_mode`.
{{fill_opacity_module}}

struct VSOut {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) color_index: u32,
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
    // Two interleaved coordinate values per vertex: x at 2*i, y at 2*i + 1.
    let model_point = vec2<f32>(
        load_coord(2u * vertex_index),
        load_coord(2u * vertex_index + 1u),
    );
    let layer_aspect_ratio = u.layer_size.x / u.layer_size.y;
    let pos_ndc = project_point(model_point, layer_aspect_ratio);

    var out: VSOut;
    out.position = vec4f(pos_ndc.x, pos_ndc.y, 0.0, 1.0);
    out.color_index = load_color_index(vertex_index);
    return out;
}

@fragment
fn fs_main(
    @builtin(position) frag_coord: vec4<f32>,
    @location(0) @interpolate(flat) color_index: u32,
) -> FSOut {
    // The color module's get_fill_color resolves the per-element color for the
    // active color mode (static, instanced RGB, categorical or quantitative).
    let out_color = get_fill_color(color_index);
    let fill_opacity = get_fill_opacity(color_index);

    var out: FSOut;
    out.color = vec4<f32>(out_color, fill_opacity);
    return out;
}
