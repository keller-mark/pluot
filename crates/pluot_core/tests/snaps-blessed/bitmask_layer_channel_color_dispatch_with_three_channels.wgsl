// The switch case list below is substituted in with one case per
// channel, which calls that channel's generated color getter.
// Depends on the getter functions (see `get_channel_color`) also being injected.
fn get_channel_stroke_color(channel_index: u32, label_index: u32) -> vec3<f32> {
    switch (channel_index) {
        case 0u: { return get_channel_stroke_color_0(label_index); }
        case 1u: { return get_channel_stroke_color_1(label_index); }
        case 2u: { return get_channel_stroke_color_2(label_index); }
        default: { return vec3<f32>(0.0, 0.0, 0.0); }
    }
}
