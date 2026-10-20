// EmphasisCriteria::Categorical with an explicit empty `included_codes`: no
// item is included. Texture-free, since there is nothing to look up.
fn is_filtered_in_0(instance_index: u32) -> bool {
    return false;
}

fn is_filtered_in(instance_index: u32) -> bool {
    return is_filtered_in_0(instance_index);
}
