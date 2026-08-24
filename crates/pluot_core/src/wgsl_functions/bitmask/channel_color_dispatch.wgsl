// Dispatches to the correct per-channel `get_channel_color_N`. Each channel
// may use a different `ColorMode` (and therefore a different generated
// function/set of texture bindings -- WGSL has no runtime function-pointer
// indirection), so this switch is generated once per draw call, sized to the
// actual channel count (see `crate::layers::bitmask_layer::draw`). Depends on
// the per-channel `get_channel_color_N` functions (see `get_channel_color`)
// also being injected. Template: the switch's case list below is substituted
// in with one "case N: return get_channel_color_N(label_index);" per channel.
fn get_channel_color(channel_index: u32, label_index: u32) -> vec3<f32> {
    switch (channel_index) {
        {{switch_cases}}
        default: { return vec3<f32>(0.0, 0.0, 0.0); }
    }
}
