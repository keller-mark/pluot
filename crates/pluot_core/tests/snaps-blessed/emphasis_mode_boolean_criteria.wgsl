// EmphasisCriteria::Boolean — per-element boolean (or 0/1) mask, where
// nonzero items are included. Depends on `flat_texel_coord` being injected.
@group(0) @binding(5) var select_data_0: texture_2d<u32>;

fn is_selected_in_0(instance_index: u32) -> bool {
    let value = textureLoad(
        select_data_0,
        flat_texel_coord(instance_index, textureDimensions(select_data_0).x),
        0
    ).x;
    return f32(value) != 0.0;
}

fn is_selected_in(instance_index: u32) -> bool {
    return is_selected_in_0(instance_index);
}
