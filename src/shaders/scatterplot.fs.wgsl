
struct FSOut {
    @location(0) color: vec4<f32>,
};

@fragment
fn fs_main(@location(0) color_in: vec4<f32>) -> FSOut {
    var out: FSOut;
    out.color = color_in;
    return out;
}