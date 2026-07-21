// SizeMode::InstancedSize — per-element stroke width read from a single-channel
// value texture. Values are in the same units as the uniform width. Depends on
// `flat_texel_coord` being injected.
@group(0) @binding({{stroke_width_values_bidx}}) var stroke_width_values: texture_2d<{{stroke_width_values_dtype}}>;

fn get_stroke_width(poly_index: u32) -> f32 {
  return f32(textureLoad(stroke_width_values, flat_texel_coord(poly_index, textureDimensions(stroke_width_values).x), 0).x);
}
