// EmphasisCriteria::Quantitative — per-element scalar value tested against an
// inclusive [min, max] range. An omitted bound is baked in as +/- f32::MAX,
// matching the -infinity/+infinity semantics. Depends on `flat_texel_coord`
// being injected.
@group(0) @binding({{criteria_data_bidx}}) var {{criteria_data_var}}: texture_2d<{{criteria_data_dtype}}>;

fn {{criteria_fn_name}}(instance_index: u32) -> bool {
    let value = f32(textureLoad(
        {{criteria_data_var}},
        flat_texel_coord(instance_index, textureDimensions({{criteria_data_var}}).x),
        0
    ).x);
    return value >= {{criteria_min_value}} && value <= {{criteria_max_value}};
}
