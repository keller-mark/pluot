// Map a flat element index into 2D texel coordinates for a single-channel data
// texture whose flat array was reshaped into rows of `width` texels: element
// `idx` lives at texel `(idx % width, idx / width)`. See
// `NumericData::create_data_texture`.
fn flat_texel_coord(idx: u32, width: u32) -> vec2<u32> {
  return vec2<u32>(idx % width, idx / width);
}
