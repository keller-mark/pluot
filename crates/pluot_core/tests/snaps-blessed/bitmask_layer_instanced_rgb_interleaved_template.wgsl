// BitmaskLayer per-channel ColorMode::InstancedRgbInterleaved — per-object
// RGB from one interleaved value texture, indexed by object id
// (`label_index`). Depends on `flat_texel_coord` being injected.
@group(0) @binding(8) var channel_fill_color_rgb_2: texture_2d<u32>;

fn get_channel_fill_color_2(label_index: u32) -> vec3<f32> {
  let w = textureDimensions(channel_fill_color_rgb_2).x;
  let base = label_index * 3u;
  let r = f32(textureLoad(channel_fill_color_rgb_2, flat_texel_coord(base, w), 0).x) / 255.0;
  let g = f32(textureLoad(channel_fill_color_rgb_2, flat_texel_coord(base + 1u, w), 0).x) / 255.0;
  let b = f32(textureLoad(channel_fill_color_rgb_2, flat_texel_coord(base + 2u, w), 0).x) / 255.0;
  return vec3<f32>(r, g, b);
}
