// Matrix values stored densely, row-major, wrapped into texture rows (see
// `flat_texel_coord`). `{{num_cols}}` is the WGSL expression holding the
// matrix's column count.
@group(0) @binding({{matrix_data_bidx}}) var matrix_data: texture_2d<{{matrix_data_dtype}}>;

fn get_matrix_value(row: u32, col: u32) -> f32 {
  let idx = row * {{num_cols}} + col;
  return f32(textureLoad(matrix_data, flat_texel_coord(idx, textureDimensions(matrix_data).x), 0).x);
}
