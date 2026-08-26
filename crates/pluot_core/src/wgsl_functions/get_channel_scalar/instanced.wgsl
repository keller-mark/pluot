// BitmaskLayer per-channel SizeMode::InstancedSize /
// OpacityMode::InstancedOpacity — one value per object, read from a value
// texture indexed by object id (`label_index`). Objects past the end of the
// array read the texture's zero padding, i.e. no stroke / full transparency.
// Depends on `flat_texel_coord` being injected.
@group(0) @binding({{values_bidx}}) var channel_{{stroke_or_fill_property}}_values_{{c_idx}}: texture_2d<{{values_dtype}}>;

fn get_channel_{{stroke_or_fill_property}}_{{c_idx}}(label_index: u32) -> f32 {
  return f32(textureLoad(channel_{{stroke_or_fill_property}}_values_{{c_idx}}, flat_texel_coord(label_index, textureDimensions(channel_{{stroke_or_fill_property}}_values_{{c_idx}}).x), 0).x);
}
