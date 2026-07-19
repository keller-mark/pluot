// OpacityMode::InstancedOpacity — per-line opacity (0-1) read from a
// single-channel value texture. Depends on `flat_texel_coord` being injected.
@group(0) @binding({{line_opacity_values_bidx}}) var line_opacity_values: texture_2d<{{line_opacity_values_dtype}}>;

fn get_line_opacity(instance_index: u32) -> f32 {
  return f32(textureLoad(line_opacity_values, flat_texel_coord(instance_index, textureDimensions(line_opacity_values).x), 0).x);
}
