// BitmaskLayer per-channel SizeMode::InstancedSize /
// OpacityMode::InstancedOpacity — one value per object, read from a value
// texture indexed by object id (`label_index`). Objects past the end of the
// array read the texture's zero padding, i.e. no stroke / full transparency.
// Depends on `flat_texel_coord` being injected.
@group(0) @binding({{values_bidx}}) var channel_{{name}}_values_{{ch}}: texture_2d<{{values_dtype}}>;

fn get_channel_{{name}}_{{ch}}(label_index: u32) -> f32 {
  return f32(textureLoad(channel_{{name}}_values_{{ch}}, flat_texel_coord(label_index, textureDimensions(channel_{{name}}_values_{{ch}}).x), 0).x);
}
