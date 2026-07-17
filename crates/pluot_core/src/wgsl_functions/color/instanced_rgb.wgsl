// ColorMode::InstancedRgb — per-element RGB from three parallel value textures.
// Values are on a 0-255 scale. Depends on `flat_texel_coord` being injected.
@group(0) @binding({{color_binding_0}}) var color_r: texture_2d<{{color_r_dtype}}>;
@group(0) @binding({{color_binding_1}}) var color_g: texture_2d<{{color_g_dtype}}>;
@group(0) @binding({{color_binding_2}}) var color_b: texture_2d<{{color_b_dtype}}>;

fn get_fill_color(instance_index: u32) -> vec3<f32> {
  let r = f32(textureLoad(color_r, flat_texel_coord(instance_index, textureDimensions(color_r).x), 0).x);
  let g = f32(textureLoad(color_g, flat_texel_coord(instance_index, textureDimensions(color_g).x), 0).x);
  let b = f32(textureLoad(color_b, flat_texel_coord(instance_index, textureDimensions(color_b).x), 0).x);
  return vec3<f32>(r, g, b) / 255.0;
}
