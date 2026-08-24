// Approximate object-boundary test: true if the point `px` (whose object id is
// `raw_label`) has a differently-labeled neighbor within `stroke_width` texels,
// in any of 8 directions. Used to render outline-only channels
// (`BitmaskChannelSettings::filled == false`). Injected once (not templated
// per-channel), regardless of channel count. Depends on `bitmask_sample`
// also being injected.
//
// `px` is the *continuous* position of the fragment in mask-texel space, not
// the integer texel it falls in, and `stroke_width` may be fractional. This is
// what keeps the outline's thickness independent of the mask's resolution:
// were the offsets applied to the containing texel instead, every fragment
// within a texel would answer identically and the band could only ever be a
// whole number of texels thick, so the same requested width would render
// differently for a coarse mask than for a fine one. Offsetting the continuous
// position instead lets the band's edge fall part-way through a texel, so its
// thickness is whatever `stroke_width` asks for -- down to the one-screen-pixel
// limit of the rasterizer, rather than the one-texel limit of the mask.
//
// Note the diagonal offsets reach `stroke_width * sqrt(2)`, so the band bulges
// somewhat at corners; this samples a square, not a disc.
fn bitmask_is_edge(
    channel_index: u32,
    px: vec2<f32>,
    raw_label: i32,
    img_w: u32,
    img_h: u32,
    stroke_width: f32,
) -> bool {
    let off = max(stroke_width, 0.0);
    let max_x = f32(img_w) - 1.0;
    let max_y = f32(img_h) - 1.0;
    let deltas = array<vec2<f32>, 8>(
        vec2<f32>(off, 0.0), vec2<f32>(-off, 0.0),
        vec2<f32>(0.0, off), vec2<f32>(0.0, -off),
        vec2<f32>(off, off), vec2<f32>(-off, off),
        vec2<f32>(off, -off), vec2<f32>(-off, -off),
    );
    for (var k = 0; k < 8; k = k + 1) {
        let n = px + deltas[k];
        let nx = u32(clamp(floor(n.x), 0.0, max_x));
        let ny = u32(clamp(floor(n.y), 0.0, max_y));
        let nval = bitmask_sample(channel_index, vec2<u32>(nx, ny));
        if (nval != raw_label) {
            return true;
        }
    }
    return false;
}
