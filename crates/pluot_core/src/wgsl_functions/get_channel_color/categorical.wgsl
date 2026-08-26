// BitmaskLayer per-channel ColorMode::Categorical / CategoricalCustom — an
// integer category index per object, indexed by object id (`label_index`)
// against a categorical palette uploaded as a 1-row RGBA texture.
// The category index wraps around (modulo) the palette length,
// handling negative values. Depends on `flat_texel_coord` being injected.
@group(0) @binding({{labels_bidx}}) var channel_{{stroke_or_fill_property}}_labels_{{c_idx}}: texture_2d<{{labels_dtype}}>;
@group(0) @binding({{palette_bidx}}) var channel_{{stroke_or_fill_property}}_palette_{{c_idx}}: texture_2d<f32>;

fn get_channel_{{stroke_or_fill_property}}_{{c_idx}}(label_index: u32) -> vec3<f32> {
  let raw = i32(textureLoad(channel_{{stroke_or_fill_property}}_labels_{{c_idx}}, flat_texel_coord(label_index, textureDimensions(channel_{{stroke_or_fill_property}}_labels_{{c_idx}}).x), 0).x);
  let n = i32(textureDimensions(channel_{{stroke_or_fill_property}}_palette_{{c_idx}}).x);
  let idx = u32(((raw % n) + n) % n);
  return textureLoad(channel_{{stroke_or_fill_property}}_palette_{{c_idx}}, vec2<u32>(idx, 0u), 0).rgb;
}
