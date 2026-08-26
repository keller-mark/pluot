// BitmaskLayer per-channel ColorMode::Quantitative — a per-object scalar
// feature value, indexed by object id (`label_index`), normalized into 0-1
// using the channel's (min, max) domain, then mapped through a continuous
// colormap. The colormap function's source and name are injected by
// ShaderBuilder as placeholders below (not spelled out here, to avoid the
// literal placeholder text itself being matched and substituted). Depends on
// `flat_texel_coord` being injected.
@group(0) @binding(11) var channel_stroke_color_values_4: texture_2d<f32>;

fn get_channel_stroke_color_4(label_index: u32) -> vec3<f32> {
  var x = f32(textureLoad(channel_stroke_color_values_4, flat_texel_coord(label_index, textureDimensions(channel_stroke_color_values_4).x), 0).x);
  let lo = u.channels[4].stroke_color_domain.x;
  let hi = u.channels[4].stroke_color_domain.y;
  x = clamp((x - lo) / max(hi - lo, 1e-20), 0.0, 1.0);
  if (u.channels[4].stroke_color_reverse == 1u) {
    x = 1.0 - x;
  }
  return viridis(x).rgb;
}
