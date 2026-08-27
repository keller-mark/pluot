// EmphasisCriteria::Quantitative — per-element scalar value tested against an
// inclusive [min, max] range. An omitted bound is baked in as +/- f32::MAX,
// matching the -infinity/+infinity semantics. Depends on `flat_texel_coord`
// being injected.
@group(0) @binding(5) var filter_data_0: texture_2d<f32>;

fn is_filtered_in_0(instance_index: u32) -> bool {
    let value = f32(textureLoad(
        filter_data_0,
        flat_texel_coord(instance_index, textureDimensions(filter_data_0).x),
        0
    ).x);
    return value >= 2e0 && value <= 3.4028235e38;
}

fn is_filtered_in(instance_index: u32) -> bool {
    return is_filtered_in_0(instance_index);
}
