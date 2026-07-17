// ColorMode::Quantitative — per-element scalar values normalized into 0-1 using
// the (min, max) domain uniform, then mapped through a continuous colormap. The
// colormap function's source is injected at `{{colormap_fn_source}}` and called
// by name at `{{colormap_fn_name}}`. Depends on `flat_texel_coord` being injected.
{{colormap_fn_source}}

@group(0) @binding({{color_binding_0}}) var color_values: texture_2d<{{color_values_dtype}}>;

fn get_fill_color(instance_index: u32) -> vec3<f32> {
  var x = f32(textureLoad(color_values, flat_texel_coord(instance_index, textureDimensions(color_values).x), 0).x);
  let lo = u.fill_color_domain.x;
  let hi = u.fill_color_domain.y;
  x = clamp((x - lo) / max(hi - lo, 1e-20), 0.0, 1.0);
  if (u.fill_color_reverse == 1u) {
    x = 1.0 - x;
  }
  return {{colormap_fn_name}}(x).rgb;
}
