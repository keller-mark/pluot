// HeatmapLayer categorical colormap: the cell value is an integer category
// code indexed against a palette uploaded as a 1-row RGBA texture. Codes wrap
// around (modulo) the palette length, handling negative values.
@group(0) @binding({{palette_bidx}}) var palette: texture_2d<f32>;

fn get_cell_color(value: f32, row: u32, col: u32) -> vec3<f32> {
  let n = i32(textureDimensions(palette).x);
  let idx = u32(((i32(value) % n) + n) % n);
  return textureLoad(palette, vec2<u32>(idx, 0u), 0).rgb;
}
