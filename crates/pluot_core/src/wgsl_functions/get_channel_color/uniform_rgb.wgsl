// BitmaskLayer per-channel ColorMode::UniformRgb (and None) — every object in
// this channel shares the static color from the uniform.
fn get_channel_color_{{ch}}(label_index: u32) -> vec3<f32> {
  return u.channels[{{ch}}].static_color.rgb;
}
