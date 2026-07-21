// OpacityMode::InstancedOpacity — per-polygon fill opacity (0-1) read from a
// single-channel value texture. `color_index` is the per-vertex polygon index
// (see `TriangulatedLayerParams::vertex_color_index`). Depends on
// `flat_texel_coord` being injected.
@group(0) @binding({{fill_opacity_values_bidx}}) var fill_opacity_values: texture_2d<{{fill_opacity_values_dtype}}>;

fn get_fill_opacity(color_index: u32) -> f32 {
  return f32(textureLoad(fill_opacity_values, flat_texel_coord(color_index, textureDimensions(fill_opacity_values).x), 0).x);
}
