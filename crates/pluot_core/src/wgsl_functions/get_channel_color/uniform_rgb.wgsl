// BitmaskLayer per-channel ColorMode::UniformRgb (and None) — every object in
// this channel shares the static color from the uniform. Templated per
// (channel, fill/stroke) pair, hence the two-part function name.
fn get_channel_{{name}}_{{ch}}(label_index: u32) -> vec3<f32> {
  return u.channels[{{ch}}].{{name}}_static.rgb;
}
