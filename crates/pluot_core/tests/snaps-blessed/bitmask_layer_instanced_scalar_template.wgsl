// BitmaskLayer per-channel SizeMode::InstancedSize /
// OpacityMode::InstancedOpacity — one value per object, read from a value
// texture indexed by object id (`label_index`). Objects past the end of the
// array read the texture's zero padding, i.e. no stroke / full transparency.
// Depends on `flat_texel_coord` being injected.
@group(0) @binding(12) var channel_fill_opacity_values_1: texture_2d<f32>;

fn get_channel_fill_opacity_1(label_index: u32) -> f32 {
  return f32(textureLoad(channel_fill_opacity_values_1, flat_texel_coord(label_index, textureDimensions(channel_fill_opacity_values_1).x), 0).x);
}
