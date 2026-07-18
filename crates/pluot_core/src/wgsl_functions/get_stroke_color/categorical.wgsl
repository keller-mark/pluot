// ColorMode::Categorical / CategoricalCustom — per-element integer class labels
// indexed against a palette uploaded as a 1-row RGBA texture. The label wraps
// around (modulo) the palette length, handling negative labels. Depends on
// `flat_texel_coord` being injected.
@group(0) @binding({{color_binding_0}}) var color_labels: texture_2d<{{color_labels_dtype}}>;
@group(0) @binding({{color_binding_1}}) var color_palette: texture_2d<f32>;

fn get_stroke_color(instance_index: u32) -> vec3<f32> {
  let raw = i32(textureLoad(color_labels, flat_texel_coord(instance_index, textureDimensions(color_labels).x), 0).x);
  let n = i32(textureDimensions(color_palette).x);
  let idx = u32(((raw % n) + n) % n);
  return textureLoad(color_palette, vec2<u32>(idx, 0u), 0).rgb;
}
