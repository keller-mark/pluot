// BitmaskLayer per-channel SizeMode::UniformSize / OpacityMode::UniformOpacity
// (and None) — every object in this channel shares the static value from the
// uniform, whose field is named after the property being resolved. Templated
// per (channel, property) pair, hence the two-part function name.
fn get_channel_{{stroke_or_fill_property}}_{{c_idx}}(label_index: u32) -> f32 {
  return u.channels[{{c_idx}}].{{stroke_or_fill_property}};
}
