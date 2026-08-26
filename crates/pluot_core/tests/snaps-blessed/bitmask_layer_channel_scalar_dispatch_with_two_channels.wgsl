// Scalar counterpart of `channel_color_dispatch`: dispatches one of the
// per-channel scalar getters (fill opacity, stroke opacity or stroke width).
fn get_channel_stroke_opacity(channel_index: u32, label_index: u32) -> f32 {
    switch (channel_index) {
        case 0u: { return get_channel_stroke_opacity_0(label_index); }
        case 1u: { return get_channel_stroke_opacity_1(label_index); }
        default: { return 0.0; }
    }
}
