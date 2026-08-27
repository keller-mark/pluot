// Boolean counterpart of `channel_scalar_dispatch`/`channel_color_dispatch`:
// dispatches one of the per-channel filtering/selection predicates
// (is_filtered_in or is_selected_in). Defaults to true (include) for a
// channel index out of range, matching the empty-criteria "everything
// included" semantics. Depends on the getter functions (see
// `crate::emphasis_mode::prepare_emphasis_criteria`) also being injected.
fn get_channel_{{stroke_or_fill_property}}(channel_index: u32, label_index: u32) -> bool {
    switch (channel_index) {
        {{switch_cases}}
        default: { return true; }
    }
}
