// EmphasisCriteria::Quantitative with both bounds set — per-element scalar
// value tested against a [min, max] range. Each bound's comparison operator is
// injected (`>=` or `>` for the lower bound, `<=` or `<` for the upper), so an
// exclusive bound needs no separate template. See
// `quantitative_one_sided.wgsl` for the min-only/max-only variant, which omits
// the comparison against the unbounded side entirely. Depends on
// `flat_texel_coord` being injected.
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
