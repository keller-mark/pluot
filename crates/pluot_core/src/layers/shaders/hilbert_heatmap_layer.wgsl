{{flat_texel_coord}}

{{gilbert_xy2d}}

struct Uniforms {
    // Top-left corner of the grid, in framebuffer pixels.
    grid_origin: vec2<f32>,
    cell_size: f32,
    grid_width: u32,
    grid_height: u32,
    num_items: u32,
    is_categorical: u32,
    reverse: u32,
    domain: vec2<f32>,
    opacity: f32,
    background_opacity: f32,
};

@group(0) @binding(0) var<uniform> u: Uniforms;

@group(0) @binding({{ordered_indices_bidx}}) var ordered_indices: texture_2d<{{ordered_indices_dtype}}>;
@group(0) @binding({{item_values_bidx}}) var item_values: texture_2d<{{item_values_dtype}}>;

{{colormap_fn_source}}

{{cell_domain}}

{{cell_color}}

{{filtering}}

{{selection}}

const QUAD: array<vec2<f32>, 4> = array<vec2<f32>, 4>(
    vec2<f32>(-1.0, -1.0),
    vec2<f32>(1.0, -1.0),
    vec2<f32>(-1.0, 1.0),
    vec2<f32>(1.0, 1.0)
);

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4<f32> {
    return vec4<f32>(QUAD[vertex_index], 0.0, 1.0);
}

fn is_nan(value: f32) -> bool {
    return (bitcast<u32>(value) & 0x7FFFFFFFu) > 0x7F800000u;
}

@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let cell_f = floor((position.xy - u.grid_origin) / u.cell_size);
    if (any(cell_f < vec2<f32>(0.0)) || cell_f.x >= f32(u.grid_width) || cell_f.y >= f32(u.grid_height)) {
        discard;
    }
    let curve_len = u.grid_width * u.grid_height;
    let curve_position = gilbert_xy2d(vec2<u32>(cell_f), u.grid_width, u.grid_height);
    let ordinal = mul_div_floor(curve_position, u.num_items, curve_len);
    let item = u32(textureLoad(ordered_indices, flat_texel_coord(ordinal, textureDimensions(ordered_indices).x), 0).x);
    if (!is_filtered_in(item)) {
        discard;
    }

    let value = f32(textureLoad(item_values, flat_texel_coord(item, textureDimensions(item_values).x), 0).x);
    // Negative category codes are missing values, per the AnnData spec.
    if (is_nan(value) || (u.is_categorical == 1u && value < 0.0)) {
        discard;
    }

    let alpha = u.opacity * select(u.background_opacity, 1.0, is_selected_in(item));
    return vec4<f32>(get_cell_color(value, item, 0u), alpha);
}
