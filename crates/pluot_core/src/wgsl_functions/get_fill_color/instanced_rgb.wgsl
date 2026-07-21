// ColorMode::InstancedRgb — per-element RGB from three parallel value textures.
// Values are on a 0-255 scale. Depends on `flat_texel_coord` being injected.
@group(0) @binding({{fill_color_r_bidx}}) var fill_color_r: texture_2d<{{fill_color_r_dtype}}>;
@group(0) @binding({{fill_color_g_bidx}}) var fill_color_g: texture_2d<{{fill_color_g_dtype}}>;
@group(0) @binding({{fill_color_b_bidx}}) var fill_color_b: texture_2d<{{fill_color_b_dtype}}>;

fn get_fill_color(instance_index: u32) -> vec3<f32> {
  let r = f32(textureLoad(fill_color_r, flat_texel_coord(instance_index, textureDimensions(fill_color_r).x), 0).x);
  let g = f32(textureLoad(fill_color_g, flat_texel_coord(instance_index, textureDimensions(fill_color_g).x), 0).x);
  let b = f32(textureLoad(fill_color_b, flat_texel_coord(instance_index, textureDimensions(fill_color_b).x), 0).x);
  return vec3<f32>(r, g, b) / 255.0;
}
