// ColorMode::InstancedRgbInterleaved — per-element RGB from one interleaved
// value texture: element `i` occupies flat indices 3*i, 3*i+1, 3*i+2. Values
// are on a 0-255 scale. Depends on `flat_texel_coord` being injected.
@group(0) @binding({{color_binding_0}}) var color_rgb: texture_2d<{{color_rgb_dtype}}>;

fn get_fill_color(instance_index: u32) -> vec3<f32> {
  let w = textureDimensions(color_rgb).x;
  let base = instance_index * 3u;
  let r = f32(textureLoad(color_rgb, flat_texel_coord(base, w), 0).x);
  let g = f32(textureLoad(color_rgb, flat_texel_coord(base + 1u, w), 0).x);
  let b = f32(textureLoad(color_rgb, flat_texel_coord(base + 2u, w), 0).x);
  return vec3<f32>(r, g, b) / 255.0;
}
