// Scalar counterpart of `channel_color_dispatch`: dispatches one of the
// per-channel scalar getters (fill opacity, stroke opacity or stroke width).
fn get_channel_{{stroke_or_fill_property}}(channel_index: u32, label_index: u32) -> f32 {
    switch (channel_index) {
        {{switch_cases}}
        default: { return 0.0; }
    }
}
