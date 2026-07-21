// ColorMode::Categorical / CategoricalCustom — per-element integer class labels
// indexed against a palette uploaded as a 1-row RGBA texture. The label wraps
// around (modulo) the palette length, handling negative labels. Depends on
// `flat_texel_coord` being injected.
@group(0) @binding({{stroke_color_labels_bidx}}) var stroke_color_labels: texture_2d<{{stroke_color_labels_dtype}}>;
@group(0) @binding({{stroke_color_palette_bidx}}) var stroke_color_palette: texture_2d<f32>;

fn get_stroke_color(instance_index: u32) -> vec3<f32> {
  let raw = i32(textureLoad(stroke_color_labels, flat_texel_coord(instance_index, textureDimensions(stroke_color_labels).x), 0).x);
  let n = i32(textureDimensions(stroke_color_palette).x);
  let idx = u32(((raw % n) + n) % n);
  return textureLoad(stroke_color_palette, vec2<u32>(idx, 0u), 0).rgb;
}
