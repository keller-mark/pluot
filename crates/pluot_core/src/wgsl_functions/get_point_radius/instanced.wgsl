// SizeMode::InstancedSize — per-point radius read from a single-channel value
// texture. Values are in the same units as the uniform radius. Depends on
// `flat_texel_coord` being injected.
@group(0) @binding({{point_radius_values_bidx}}) var point_radius_values: texture_2d<{{point_radius_values_dtype}}>;

fn get_point_radius(instance_index: u32) -> f32 {
  return f32(textureLoad(point_radius_values, flat_texel_coord(instance_index, textureDimensions(point_radius_values).x), 0).x);
}
