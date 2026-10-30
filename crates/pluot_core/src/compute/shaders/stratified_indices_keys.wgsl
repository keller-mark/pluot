// Writes one key per row: the index of the row's stratum, or NO_STRATUM when
// the row does not meet the criteria or its code is not one of the strata.
//
// ── Bindings ────────────────────────────────────────────────────────────────
//   @group(0) @binding(0)    uniforms       : Uniforms      (uniform)
//   @group(0) @binding(1)    keys           : array<u32>    (storage, read_write)
//   @group(0) @binding(2)    rows           : array<u32>    (storage, read)
//   @group(0) @binding(3)    stratum_lookup : array<u32>    (storage, read)
//   @group(0) @binding(4)    stratify       : texture_2d    (only when stratified)
//   @group(0) @binding(5..)  criteria value textures (see
//                            `crate::emphasis_mode::prepare_emphasis_criteria`).

const NO_STRATUM: u32 = 0xFFFFFFFFu;
const BLOCK: u32 = 256u;

struct Uniforms {
    num_elements: u32,
    // Nonzero when element `i` refers to row `rows[i]`, rather than to row `i`.
    has_rows: u32,
    // The code that `stratum_lookup[0]` is the stratum of.
    min_code: i32,
    lookup_len: u32,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var<storage, read_write> keys: array<u32>;
@group(0) @binding(2) var<storage, read> rows: array<u32>;
@group(0) @binding(3) var<storage, read> stratum_lookup: array<u32>;

{{flat_texel_coord}}

{{criteria_wgsl}}

{{stratum_of}}

@compute @workgroup_size(256)
fn main(
    @builtin(workgroup_id) workgroup_id: vec3<u32>,
    @builtin(num_workgroups) num_workgroups: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    let i = (workgroup_id.x + workgroup_id.y * num_workgroups.x) * BLOCK + local_index;
    if (i >= uniforms.num_elements) {
        return;
    }
    var row = i;
    if (uniforms.has_rows != 0u) {
        row = rows[i];
    }
    var key = NO_STRATUM;
    if (is_included(row)) {
        key = stratum_of(row);
    }
    keys[i] = key;
}
