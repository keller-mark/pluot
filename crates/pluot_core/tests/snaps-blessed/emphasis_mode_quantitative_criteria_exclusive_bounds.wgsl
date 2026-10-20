// EmphasisCriteria::Quantitative with both bounds set (lower and upper). Each bound's comparison operator and test value are
// injected. Depends on `flat_texel_coord` being injected.
@group(0) @binding(7) var select_data_0: texture_2d<f32>;

fn is_selected_in_0(instance_index: u32) -> bool {
    let value = f32(textureLoad(
        select_data_0,
        flat_texel_coord(instance_index, textureDimensions(select_data_0).x),
        0
    ).x);
    return value > 1e0 && value < 2e0;
}

fn is_selected_in(instance_index: u32) -> bool {
    return is_selected_in_0(instance_index);
}
