// BitmaskLayer per-channel SizeMode::UniformSize / OpacityMode::UniformOpacity
// (and None) — every object in this channel shares the static value from the
// uniform, whose field is named after the property being resolved. Templated
// per (channel, property) pair, hence the two-part function name.
fn get_channel_stroke_width_0(label_index: u32) -> f32 {
  return u.channels[0].stroke_width;
}
