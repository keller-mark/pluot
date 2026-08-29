// EmphasisCriteria::Categorical with an explicit empty `included_codes`: no
// item is included. Texture-free, since there is nothing to look up.
fn {{criteria_fn_name}}(instance_index: u32) -> bool {
    return false;
}
