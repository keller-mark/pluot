// EmphasisCriteria::Categorical — per-element category code (categories+codes
// format) tested against an inline array of included codes, baked in at shader
// build time since the inclusion list is small and known on the CPU ahead of
// time. Depends on `flat_texel_coord` being injected.
@group(0) @binding({{criteria_data_bidx}}) var {{criteria_data_var}}: texture_2d<{{criteria_data_dtype}}>;

fn {{criteria_fn_name}}(instance_index: u32) -> bool {
    let code = i32(textureLoad(
        {{criteria_data_var}},
        flat_texel_coord(instance_index, textureDimensions({{criteria_data_var}}).x),
        0
    ).x);
    let included = array<i32, {{criteria_included_len}}>({{criteria_included_codes}});
    for (var i: u32 = 0u; i < {{criteria_included_len}}u; i = i + 1u) {
        if (included[i] == code) {
            return true;
        }
    }
    return false;
}
