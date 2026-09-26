// HeatmapLayer quantitative colormap: the cell value is normalized into 0-1
// against the cell's (min, max) domain (see `get_cell_domain`), then mapped
// through the continuous colormap whose source and name are injected
// alongside this snippet.
fn get_cell_color(value: f32, row: u32, col: u32) -> vec3<f32> {
  let domain = get_cell_domain(row, col);
  var x = clamp((value - domain.x) / max(domain.y - domain.x, 1e-20), 0.0, 1.0);
  if (u.reverse == 1u) {
    x = 1.0 - x;
  }
  return {{colormap_fn_name}}(x).rgb;
}
