struct FSOut {
    @location(0) color: vec4<f32>,
};

@fragment
fn fs_main(
    @location(0) color_in: vec4<f32>,
    @location(1) quad_pos: vec2<f32>,
) -> FSOut {
    // Anti-aliased circle using SDF and smoothstep
    let r = 1.0;
    let dist = length(quad_pos);
    let sdf = r - dist;           // positive inside, negative outside
    let aa = fwidth(sdf);         // edge width in pixels

    // Early discard far outside edge to save fill-rate
    if (sdf < -aa) {
        discard;
    }

    let alpha = smoothstep(0.0, aa, sdf);

    var out: FSOut;
    // Output premultiplied alpha to work with PREMULTIPLIED_ALPHA blending
    out.color = vec4<f32>(color_in.rgb, color_in.a * alpha);
    return out;
}