// HeatmapLayer quantitative colormap: the cell value is normalized into 0-1
// against the layer's (min, max) domain, then mapped through the continuous
// colormap whose source and name are injected alongside this snippet.
fn get_cell_color(value: f32) -> vec3<f32> {
  var x = clamp((value - u.domain.x) / max(u.domain.y - u.domain.x, 1e-20), 0.0, 1.0);
  if (u.reverse == 1u) {
    x = 1.0 - x;
  }
  return {{colormap_fn_name}}(x).rgb;
}
