// BitmaskLayer per-channel ColorMode::InstancedRgb — per-object RGB from
// three parallel value textures, indexed by object id (`label_index`).
// Depends on `flat_texel_coord` being injected.
@group(0) @binding({{r_bidx}}) var channel_{{stroke_or_fill_property}}_r_{{c_idx}}: texture_2d<{{r_dtype}}>;
@group(0) @binding({{g_bidx}}) var channel_{{stroke_or_fill_property}}_g_{{c_idx}}: texture_2d<{{g_dtype}}>;
@group(0) @binding({{b_bidx}}) var channel_{{stroke_or_fill_property}}_b_{{c_idx}}: texture_2d<{{b_dtype}}>;

fn get_channel_{{stroke_or_fill_property}}_{{c_idx}}(label_index: u32) -> vec3<f32> {
  let r = f32(textureLoad(channel_{{stroke_or_fill_property}}_r_{{c_idx}}, flat_texel_coord(label_index, textureDimensions(channel_{{stroke_or_fill_property}}_r_{{c_idx}}).x), 0).x) / 255.0;
  let g = f32(textureLoad(channel_{{stroke_or_fill_property}}_g_{{c_idx}}, flat_texel_coord(label_index, textureDimensions(channel_{{stroke_or_fill_property}}_g_{{c_idx}}).x), 0).x) / 255.0;
  let b = f32(textureLoad(channel_{{stroke_or_fill_property}}_b_{{c_idx}}, flat_texel_coord(label_index, textureDimensions(channel_{{stroke_or_fill_property}}_b_{{c_idx}}).x), 0).x) / 255.0;
  return vec3<f32>(r, g, b);
}
