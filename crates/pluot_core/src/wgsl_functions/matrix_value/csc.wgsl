// CSC matrix values: columns are the major axis.
fn get_matrix_value(row: u32, col: u32) -> f32 {
  return get_compressed_value(col, row);
}
