// EmphasisCriteria::Quantitative with only one bound set — per-element scalar
// value tested against that bound alone, since the omitted bound means
// -infinity/+infinity and so needs no comparison at all. Both the operator and
// the value are injected, so this one template serves every one-sided case:
// `>=`/`>` against `min` when `max` is omitted, `<=`/`<` against `max` when
// `min` is omitted (e.g. a half-open histogram bin `[min, max)`). Depends on
// `flat_texel_coord` being injected.
@group(0) @binding(5) var filter_data_0: texture_2d<f32>;

fn is_filtered_in_0(instance_index: u32) -> bool {
    let value = f32(textureLoad(
        filter_data_0,
        flat_texel_coord(instance_index, textureDimensions(filter_data_0).x),
        0
    ).x);
    return value < 1e1;
}

fn is_filtered_in(instance_index: u32) -> bool {
    return is_filtered_in_0(instance_index);
}
