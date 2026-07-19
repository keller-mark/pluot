// SizeMode::InstancedSize — per-line width read from a single-channel value
// texture. Values are in the same units as the uniform width. Depends on
// `flat_texel_coord` being injected.
@group(0) @binding({{line_width_values_bidx}}) var line_width_values: texture_2d<{{line_width_values_dtype}}>;

fn get_line_width(instance_index: u32) -> f32 {
  return f32(textureLoad(line_width_values, flat_texel_coord(instance_index, textureDimensions(line_width_values).x), 0).x);
}
