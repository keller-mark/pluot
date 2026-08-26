// BitmaskLayer per-channel ColorMode::Categorical / CategoricalCustom — an
// integer category index per object, indexed by object id (`label_index`)
// against a categorical palette uploaded as a 1-row RGBA texture.
// The category index wraps around (modulo) the palette length,
// handling negative values. Depends on `flat_texel_coord` being injected.
@group(0) @binding(9) var channel_fill_color_labels_3: texture_2d<u32>;
@group(0) @binding(10) var channel_fill_color_palette_3: texture_2d<f32>;

fn get_channel_fill_color_3(label_index: u32) -> vec3<f32> {
  let raw = i32(textureLoad(channel_fill_color_labels_3, flat_texel_coord(label_index, textureDimensions(channel_fill_color_labels_3).x), 0).x);
  let n = i32(textureDimensions(channel_fill_color_palette_3).x);
  let idx = u32(((raw % n) + n) % n);
  return textureLoad(channel_fill_color_palette_3, vec2<u32>(idx, 0u), 0).rgb;
}
