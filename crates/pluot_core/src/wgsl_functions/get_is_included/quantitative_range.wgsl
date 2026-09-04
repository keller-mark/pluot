// EmphasisCriteria::Quantitative with both bounds set — per-element scalar
// value tested against a [min, max] range. Each bound's comparison operator is
// injected (`>=` or `>` for the lower bound, `<=` or `<` for the upper), so an
// exclusive bound needs no separate template. See
// `quantitative_one_sided.wgsl` for the min-only/max-only variant, which omits
// the comparison against the unbounded side entirely. Depends on
// `flat_texel_coord` being injected.
@group(0) @binding({{criteria_data_bidx}}) var {{criteria_data_var}}: texture_2d<{{criteria_data_dtype}}>;

fn {{criteria_fn_name}}(instance_index: u32) -> bool {
    let value = f32(textureLoad(
        {{criteria_data_var}},
        flat_texel_coord(instance_index, textureDimensions({{criteria_data_var}}).x),
        0
    ).x);
    return value {{criteria_min_op}} {{criteria_min_value}} && value {{criteria_max_op}} {{criteria_max_value}};
}
