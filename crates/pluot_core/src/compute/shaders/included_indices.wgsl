// Writes one flag word per element: FILTERED_IN when the element meets the
// filtering criteria, plus SELECTED_IN when it also meets the selection
// criteria. The caller compacts the flags into ascending index lists.
//
// ── Bindings ────────────────────────────────────────────────────────────────
//   @group(0) @binding(0)             uniforms : Uniforms            (uniform)
//   @group(0) @binding(1)             flags    : array<u32>          (storage, read_write)
//   @group(0) @binding(2..)           filtering criteria value textures, then
//                                     selection criteria value textures (see
//                                     `crate::emphasis_mode::prepare_emphasis_criteria`).

const FILTERED_IN: u32 = 1u;
const SELECTED_IN: u32 = 2u;

struct Uniforms {
    // Number of elements processed by this dispatch (the current chunk length).
    num_elements: u32,
    // Flat index of the current chunk's first element.
    base_offset: u32,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var<storage, read_write> flags: array<u32>;

{{flat_texel_coord}}

{{filtering_wgsl}}

{{selection_wgsl}}

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= uniforms.num_elements) {
        return;
    }
    let idx = uniforms.base_offset + gid.x;
    var flag = 0u;
    if (is_filtered_in(idx)) {
        flag = FILTERED_IN;
        if (is_selected_in(idx)) {
            flag = flag | SELECTED_IN;
        }
    }
    flags[gid.x] = flag;
}
