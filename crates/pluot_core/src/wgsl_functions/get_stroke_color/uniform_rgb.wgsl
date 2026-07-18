// ColorMode::UniformRgb — every element shares the static color from the uniform.
fn get_stroke_color(instance_index: u32) -> vec3<f32> {
  return u.stroke_color.rgb;
}
