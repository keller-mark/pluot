// BitmaskLayer per-channel ColorMode::InstancedRgb — per-object RGB from
// three parallel value textures, indexed by object id (`label_index`).
// Depends on `flat_texel_coord` being injected.
@group(0) @binding(5) var channel_stroke_color_r_1: texture_2d<u32>;
@group(0) @binding(6) var channel_stroke_color_g_1: texture_2d<u32>;
@group(0) @binding(7) var channel_stroke_color_b_1: texture_2d<u32>;

fn get_channel_stroke_color_1(label_index: u32) -> vec3<f32> {
  let r = f32(textureLoad(channel_stroke_color_r_1, flat_texel_coord(label_index, textureDimensions(channel_stroke_color_r_1).x), 0).x) / 255.0;
  let g = f32(textureLoad(channel_stroke_color_g_1, flat_texel_coord(label_index, textureDimensions(channel_stroke_color_g_1).x), 0).x) / 255.0;
  let b = f32(textureLoad(channel_stroke_color_b_1, flat_texel_coord(label_index, textureDimensions(channel_stroke_color_b_1).x), 0).x) / 255.0;
  return vec3<f32>(r, g, b);
}
