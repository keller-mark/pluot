// EmphasisCriteria::Quantitative with only one bound set — per-element scalar
// value tested against that bound alone, since the omitted bound means
// -infinity/+infinity and so needs no comparison at all. Both the operator and
// the value are injected, so this one template serves every one-sided case:
// `>=`/`>` against `min` when `max` is omitted, `<=`/`<` against `max` when
// `min` is omitted (e.g. a half-open histogram bin `[min, max)`). Depends on
// `flat_texel_coord` being injected.
@group(0) @binding({{criteria_data_bidx}}) var {{criteria_data_var}}: texture_2d<{{criteria_data_dtype}}>;

fn {{criteria_fn_name}}(instance_index: u32) -> bool {
    let value = f32(textureLoad(
        {{criteria_data_var}},
        flat_texel_coord(instance_index, textureDimensions({{criteria_data_var}}).x),
        0
    ).x);
    return value {{criteria_op}} {{criteria_value}};
}
