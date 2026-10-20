// EmphasisCriteria::Quantitative with both bounds set (lower and upper). Each bound's comparison operator and test value are
// injected. Depends on `flat_texel_coord` being injected.
@group(0) @binding({{criteria_data_bidx}}) var {{criteria_data_var}}: texture_2d<{{criteria_data_dtype}}>;

fn {{criteria_fn_name}}(instance_index: u32) -> bool {
    let value = f32(textureLoad(
        {{criteria_data_var}},
        flat_texel_coord(instance_index, textureDimensions({{criteria_data_var}}).x),
        0
    ).x);
    return value {{criteria_min_op}} {{criteria_min_value}} && value {{criteria_max_op}} {{criteria_max_value}};
}
