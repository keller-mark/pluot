// OpacityMode::InstancedOpacity — per-point opacity (0-1) read from a
// single-channel value texture. Depends on `flat_texel_coord` being injected.
@group(0) @binding({{point_opacity_values_bidx}}) var point_opacity_values: texture_2d<{{point_opacity_values_dtype}}>;

fn get_point_opacity(instance_index: u32) -> f32 {
  return f32(textureLoad(point_opacity_values, flat_texel_coord(instance_index, textureDimensions(point_opacity_values).x), 0).x);
}
