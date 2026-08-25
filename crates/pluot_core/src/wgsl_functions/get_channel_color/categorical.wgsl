// BitmaskLayer per-channel ColorMode::Categorical / CategoricalCustom — an
// integer "set color" index per object, indexed by object id (`label_index`)
// against a palette uploaded as a 1-row RGBA texture. The index wraps around
// (modulo) the palette length, handling negative values. Depends on
// `flat_texel_coord` being injected.
@group(0) @binding({{labels_bidx}}) var channel_{{name}}_labels_{{ch}}: texture_2d<{{labels_dtype}}>;
@group(0) @binding({{palette_bidx}}) var channel_{{name}}_palette_{{ch}}: texture_2d<f32>;

fn get_channel_{{name}}_{{ch}}(label_index: u32) -> vec3<f32> {
  let raw = i32(textureLoad(channel_{{name}}_labels_{{ch}}, flat_texel_coord(label_index, textureDimensions(channel_{{name}}_labels_{{ch}}).x), 0).x);
  let n = i32(textureDimensions(channel_{{name}}_palette_{{ch}}).x);
  let idx = u32(((raw % n) + n) % n);
  return textureLoad(channel_{{name}}_palette_{{ch}}, vec2<u32>(idx, 0u), 0).rgb;
}
