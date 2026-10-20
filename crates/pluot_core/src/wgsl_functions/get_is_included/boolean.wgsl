// EmphasisCriteria::Boolean — per-element boolean (or 0/1) mask, where
// nonzero items are included. Depends on `flat_texel_coord` being injected.
@group(0) @binding({{criteria_data_bidx}}) var {{criteria_data_var}}: texture_2d<{{criteria_data_dtype}}>;

fn {{criteria_fn_name}}(instance_index: u32) -> bool {
    let value = textureLoad(
        {{criteria_data_var}},
        flat_texel_coord(instance_index, textureDimensions({{criteria_data_var}}).x),
        0
    ).x;
    return f32(value) != 0.0;
}
