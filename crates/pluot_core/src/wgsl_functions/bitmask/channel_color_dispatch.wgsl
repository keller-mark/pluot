// Dispatches one of the per-channel color getters (the channel's fill color or
// its stroke color) to the function matching `channel_index`. Each channel may
// use a different `ColorMode` (and therefore a different generated function/set
// of texture bindings -- WGSL has no runtime function-pointer indirection), so
// this switch is generated once per draw call, sized to the actual channel
// count (see `crate::layers::bitmask_layer::draw`). Depends on the matching
// per-channel functions (see `get_channel_color`) also being injected.
// Template: the switch's case list below is substituted in with one case per
// channel, returning that channel's generated getter.
fn get_channel_{{name}}(channel_index: u32, label_index: u32) -> vec3<f32> {
    switch (channel_index) {
        {{switch_cases}}
        default: { return vec3<f32>(0.0, 0.0, 0.0); }
    }
}
