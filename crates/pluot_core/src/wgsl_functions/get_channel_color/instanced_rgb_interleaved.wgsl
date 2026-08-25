// BitmaskLayer per-channel ColorMode::InstancedRgbInterleaved — per-object
// RGB from one interleaved value texture, indexed by object id
// (`label_index`). Depends on `flat_texel_coord` being injected.
@group(0) @binding({{rgb_bidx}}) var channel_{{name}}_rgb_{{ch}}: texture_2d<{{rgb_dtype}}>;

fn get_channel_{{name}}_{{ch}}(label_index: u32) -> vec3<f32> {
  let w = textureDimensions(channel_{{name}}_rgb_{{ch}}).x;
  let base = label_index * 3u;
  let r = f32(textureLoad(channel_{{name}}_rgb_{{ch}}, flat_texel_coord(base, w), 0).x) / 255.0;
  let g = f32(textureLoad(channel_{{name}}_rgb_{{ch}}, flat_texel_coord(base + 1u, w), 0).x) / 255.0;
  let b = f32(textureLoad(channel_{{name}}_rgb_{{ch}}, flat_texel_coord(base + 2u, w), 0).x) / 255.0;
  return vec3<f32>(r, g, b);
}
