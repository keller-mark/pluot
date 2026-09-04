// EmphasisCriteria::Categorical — per-element category code (categories+codes
// format) tested against an inline array of included codes, baked in at shader
// build time since the inclusion list is small and known on the CPU ahead of
// time. Depends on `flat_texel_coord` being injected.
@group(0) @binding(5) var filter_data_0: texture_2d<i32>;

fn is_filtered_in_0(instance_index: u32) -> bool {
    let code = i32(textureLoad(
        filter_data_0,
        flat_texel_coord(instance_index, textureDimensions(filter_data_0).x),
        0
    ).x);
    let included = array<i32, 3>(0, 1, 2);
    for (var i: u32 = 0u; i < 3u; i = i + 1u) {
        if (included[i] == code) {
            return true;
        }
    }
    return false;
}

// EmphasisCriteria::Quantitative with only one bound set. Both the operator and
// the value are injected. Depends on `flat_texel_coord` being injected.
@group(0) @binding(6) var filter_data_1: texture_2d<f32>;

fn is_filtered_in_1(instance_index: u32) -> bool {
    let value = f32(textureLoad(
        filter_data_1,
        flat_texel_coord(instance_index, textureDimensions(filter_data_1).x),
        0
    ).x);
    return value >= 1e1;
}

fn is_filtered_in(instance_index: u32) -> bool {
    return is_filtered_in_0(instance_index) && is_filtered_in_1(instance_index);
}
