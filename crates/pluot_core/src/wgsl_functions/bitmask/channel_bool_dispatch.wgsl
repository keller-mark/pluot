// Boolean counterpart of `channel_scalar_dispatch`: dispatches one of the
// per-channel filtering/selection membership getters (`is_filtered_in` /
// `is_selected_in`). Defaults to `true` (included) for an out-of-range
// channel index, matching the "None" EmphasisCriteria semantics.
fn get_channel_{{stroke_or_fill_property}}(channel_index: u32, label_index: u32) -> bool {
    switch (channel_index) {
        {{switch_cases}}
        default: { return true; }
    }
}
