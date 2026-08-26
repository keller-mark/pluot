// BitmaskLayer per-channel ColorMode::UniformRgb (and None) — every object in
// this channel shares the static color from the uniform. Templated per
// (channel, fill/stroke) pair, hence the two-part function name.
fn get_channel_fill_color_0(label_index: u32) -> vec3<f32> {
  return u.channels[0].fill_color_static.rgb;
}
