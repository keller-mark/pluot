// OpacityMode::InstancedOpacity — per-polygon stroke opacity (0-1) read from a
// single-channel value texture. Depends on `flat_texel_coord` being injected.
@group(0) @binding({{stroke_opacity_values_bidx}}) var stroke_opacity_values: texture_2d<{{stroke_opacity_values_dtype}}>;

fn get_stroke_opacity(poly_index: u32) -> f32 {
  return f32(textureLoad(stroke_opacity_values, flat_texel_coord(poly_index, textureDimensions(stroke_opacity_values).x), 0).x);
}
