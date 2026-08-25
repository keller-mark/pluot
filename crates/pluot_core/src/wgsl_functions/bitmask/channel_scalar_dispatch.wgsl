// Scalar counterpart of `channel_color_dispatch`: dispatches one of the
// per-channel scalar getters (fill opacity, stroke opacity or stroke width) to
// the function matching `channel_index`. Generated once per draw call per
// property, sized to the actual channel count, because each channel resolves
// the property through its own `SizeMode`/`OpacityMode` (see
// `crate::layers::bitmask_layer::draw`). Depends on the matching per-channel
// functions (see `get_channel_scalar`) also being injected. Template: the
// switch's case list below is substituted in with one case per channel,
// returning that channel's generated getter.
fn get_channel_{{name}}(channel_index: u32, label_index: u32) -> f32 {
    switch (channel_index) {
        {{switch_cases}}
        default: { return 0.0; }
    }
}
