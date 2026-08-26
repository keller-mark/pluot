// Approximate object-boundary test: true if the point `px` (whose object id is
// `raw_label`) has a differently-labeled neighbor within `stroke_width` texels,
// in any of 8 directions. Used to render outline-only channels
// (`BitmaskChannelSettings::filled == false`). Injected once (not templated
// per-channel), regardless of channel count. Depends on `bitmask_sample`
// also being injected.
//
// `px` is the *continuous* position of the fragment in mask-texel space, not
// the integer texel it falls in, and `stroke_width` may be fractional.
// This keeps the outline's thickness independent of the mask's resolution.
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
