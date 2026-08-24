// Loads the object id at pixel `px` for one channel of the shared,
// multi-channel `mask_data` texture (see `BitmaskLayer`), using
// `u.x_stride`/`u.y_stride`/`u.c_stride` to locate that channel's slice
// within the flat array -- mirrors `BitmapLayer`'s stride-based indexing.
// Injected once (not templated per-channel), regardless of channel count,
// since `channel_index` is an ordinary function parameter. Assumes
// `mask_data` and `flat_texel_coord` are already in scope.
fn bitmask_sample(channel_index: u32, px: vec2<u32>) -> i32 {
    let idx = px.y * u.y_stride + px.x * u.x_stride + channel_index * u.c_stride;
    let mask_tex_width = textureDimensions(mask_data).x;
    return i32(textureLoad(mask_data, flat_texel_coord(idx, mask_tex_width), 0).x);
}
