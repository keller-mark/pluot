// ColorMode::Quantitative — per-element scalar values normalized into 0-1 using
// the (min, max) domain uniform, then mapped through a continuous colormap. The
// colormap function's source is injected at `{{colormap_fn_source}}` and called
// by name at `{{colormap_fn_name}}`. Depends on `flat_texel_coord` being injected.
{{colormap_fn_source}}

@group(0) @binding({{stroke_color_values_bidx}}) var stroke_color_values: texture_2d<{{stroke_color_values_dtype}}>;

fn get_stroke_color(instance_index: u32) -> vec3<f32> {
  var x = f32(textureLoad(stroke_color_values, flat_texel_coord(instance_index, textureDimensions(stroke_color_values).x), 0).x);
  let lo = u.stroke_color_domain.x;
  let hi = u.stroke_color_domain.y;
  x = clamp((x - lo) / max(hi - lo, 1e-20), 0.0, 1.0);
  if (u.stroke_color_reverse == 1u) {
    x = 1.0 - x;
  }
  return {{colormap_fn_name}}(x).rgb;
}
