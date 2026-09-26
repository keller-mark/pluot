{{scale}}

{{translate}}

{{get_aspect_ratio_mat}}

{{flat_texel_coord}}

struct Uniforms {
    layer_size: vec2<f32>,
    camera_view: mat4x4<f32>,
    data_unit_mode_x: u32, // 0: pixel units, 1: data units, 2: normalized (0-1) units
    data_unit_mode_y: u32,
    aspect_ratio_mode: u32,
    aspect_ratio_alignment_mode: u32,

    model_matrix: mat4x4<f32>,

    // Bottom-left corner and size of this block's quad, in cell units (pre-model_matrix).
    quad_origin: vec2<f32>,
    quad_size: vec2<f32>,

    num_cols: u32,
    block_first_row: u32,
    block_num_rows: u32,
    swap_axes: u32,

    domain: vec2<f32>,
    reverse: u32,
    opacity: f32,
    background_opacity: f32,
};

struct VSOut {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coord: vec2<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
// This block's rows of the row-major matrix, wrapped into texture rows (see `flat_texel_coord`).
@group(0) @binding(1) var matrix_data: texture_2d<{{matrix_data_dtype}}>;

{{colormap_fn_source}}

{{cell_color}}

{{row_selection}}

{{col_selection}}

const QUAD: array<vec2<f32>, 4> = array<vec2<f32>, 4>(
    vec2<f32>(0.0, 0.0),
    vec2<f32>(1.0, 0.0),
    vec2<f32>(0.0, 1.0),
    vec2<f32>(1.0, 1.0)
);

// Y is flipped so that tex_coord.y is 0 at the top of the quad, where the first row is drawn.
const TEX_COORDS: array<vec2<f32>, 4> = array<vec2<f32>, 4>(
    vec2<f32>(0.0, 1.0),
    vec2<f32>(1.0, 1.0),
    vec2<f32>(0.0, 0.0),
    vec2<f32>(1.0, 0.0)
);

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VSOut {
    let uv = TEX_COORDS[vertex_index];
    let vertex_pos = u.model_matrix * vec4f(u.quad_origin + QUAD[vertex_index] * u.quad_size, 0.0, 1.0);

    let layer_width_px = u.layer_size.x;
    let layer_height_px = u.layer_size.y;
    let aspect_ratio_mat = get_aspect_ratio_mat(
        layer_width_px / layer_height_px,
        u.aspect_ratio_mode,
        u.aspect_ratio_alignment_mode
    );
    let norm_to_ndc = translate(-1.0, -1.0, 0.0) * scale(2.0, 2.0, 1.0);
    let ndc_to_norm = translate(0.5, 0.5, 0.0) * scale(0.5, 0.5, 1.0);

    // Same positioning semantics as bitmap_layer.wgsl.
    let non_data_pos_norm = vec2<f32>(
        select(vertex_pos.x / layer_width_px, vertex_pos.x, u.data_unit_mode_x == 2u),
        select(vertex_pos.y / layer_height_px, vertex_pos.y, u.data_unit_mode_y == 2u)
    );
    let non_data_pos_ndc = norm_to_ndc * vec4f(non_data_pos_norm, 0.0, 1.0);

    let data_pos_norm = (ndc_to_norm * aspect_ratio_mat * u.camera_view * norm_to_ndc) * vertex_pos;
    var position = norm_to_ndc * vec4f(data_pos_norm.xy, 0.0, 1.0);

    if (u.data_unit_mode_x != 1u) {
        position.x = non_data_pos_ndc.x;
    }
    if (u.data_unit_mode_y != 1u) {
        position.y = non_data_pos_ndc.y;
    }

    var out: VSOut;
    out.position = position;
    out.tex_coord = uv;
    return out;
}

@fragment
fn fs_main(in: VSOut) -> @location(0) vec4<f32> {
    let along_rows = select(in.tex_coord.y, in.tex_coord.x, u.swap_axes == 1u);
    let along_cols = select(in.tex_coord.x, in.tex_coord.y, u.swap_axes == 1u);
    let local_row = min(u32(floor(along_rows * f32(u.block_num_rows))), u.block_num_rows - 1u);
    let col = min(u32(floor(along_cols * f32(u.num_cols))), u.num_cols - 1u);

    let flat_idx = local_row * u.num_cols + col;
    let value = f32(textureLoad(matrix_data, flat_texel_coord(flat_idx, textureDimensions(matrix_data).x), 0).x);
    if (value != value) {
        discard;
    }

    let row = u.block_first_row + local_row;
    let is_selected = is_row_selected(row) && is_col_selected(col);
    let alpha = u.opacity * select(u.background_opacity, 1.0, is_selected);
    return vec4<f32>(get_cell_color(value), alpha);
}
