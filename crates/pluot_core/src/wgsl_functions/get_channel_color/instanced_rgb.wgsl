// BitmaskLayer per-channel ColorMode::InstancedRgb — per-object RGB from
// three parallel value textures, indexed by object id (`label_index`).
// Depends on `flat_texel_coord` being injected.
@group(0) @binding({{r_bidx}}) var channel_color_r_{{ch}}: texture_2d<{{r_dtype}}>;
@group(0) @binding({{g_bidx}}) var channel_color_g_{{ch}}: texture_2d<{{g_dtype}}>;
@group(0) @binding({{b_bidx}}) var channel_color_b_{{ch}}: texture_2d<{{b_dtype}}>;

fn get_channel_color_{{ch}}(label_index: u32) -> vec3<f32> {
  let r = f32(textureLoad(channel_color_r_{{ch}}, flat_texel_coord(label_index, textureDimensions(channel_color_r_{{ch}}).x), 0).x) / 255.0;
  let g = f32(textureLoad(channel_color_g_{{ch}}, flat_texel_coord(label_index, textureDimensions(channel_color_g_{{ch}}).x), 0).x) / 255.0;
  let b = f32(textureLoad(channel_color_b_{{ch}}, flat_texel_coord(label_index, textureDimensions(channel_color_b_{{ch}}).x), 0).x) / 255.0;
  return vec3<f32>(r, g, b);
}
