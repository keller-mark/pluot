// Approximate object-boundary test: true if pixel `px` (whose object id is
// `raw_label`) has a differently-labeled neighbor within `stroke_width`
// texels, in any of 8 directions. Used to render outline-only channels
// (`BitmaskChannelSettings::filled == false`). Injected once (not templated
// per-channel), regardless of channel count. Depends on `bitmask_sample`
// also being injected.
fn bitmask_is_edge(
    channel_index: u32,
    px: vec2<u32>,
    raw_label: i32,
    img_w: u32,
    img_h: u32,
    stroke_width: f32,
) -> bool {
    let off = i32(max(stroke_width, 1.0));
    let x0 = i32(px.x);
    let y0 = i32(px.y);
    let w = i32(img_w);
    let h = i32(img_h);
    let deltas = array<vec2<i32>, 8>(
        vec2<i32>(off, 0), vec2<i32>(-off, 0),
        vec2<i32>(0, off), vec2<i32>(0, -off),
        vec2<i32>(off, off), vec2<i32>(-off, off),
        vec2<i32>(off, -off), vec2<i32>(-off, -off),
    );
    for (var k = 0; k < 8; k = k + 1) {
        let nx = clamp(x0 + deltas[k].x, 0, w - 1);
        let ny = clamp(y0 + deltas[k].y, 0, h - 1);
        let nval = bitmask_sample(channel_index, vec2<u32>(u32(nx), u32(ny)));
        if (nval != raw_label) {
            return true;
        }
    }
    return false;
}
