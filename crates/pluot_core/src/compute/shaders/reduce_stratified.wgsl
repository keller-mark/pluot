// Stratified reduction compute shader.
//
// Unlike reduce.wgsl's workgroup-local tree reduction (which assumes every
// thread in a workgroup contributes to the same single output), stratified
// reduction scatters each element's contribution to a different output slot
// depending on its category (chunked only for
// dispatch-size limits, exactly like reduce.wgsl).
//
// Like reduce.wgsl, this also computes BOTH the "background" (filter-
// included) and "foreground" (filter-*and*-selection-included) result in
// that same single dispatch.

// Mode constants. Must match the Rust-side ReduceMode discriminants (see reduce.rs).
const MODE_MIN:       u32 = 0u;
const MODE_MAX:       u32 = 1u;
const MODE_SUM:       u32 = 2u;
const MODE_EXTENT:    u32 = 3u;
const MODE_HISTOGRAM: u32 = 4u;
const MODE_COUNT:     u32 = 5u;
const MODE_MEAN:      u32 = 6u;

// Uniforms (identical layout to reduce.wgsl's ReduceUniforms — the Rust side
// reuses that same struct for both shaders).

struct ReduceUniforms {
    mode: u32,
    // Number of elements processed by THIS dispatch (the current chunk length).
    num_elements: u32,
    // Histogram: number of bins per stratum.
    num_bins: u32,
    // Histogram: minimum value of the data range (inclusive).
    data_min: f32,
    // Histogram: maximum value of the data range (exclusive).
    data_max: f32,
    // Flat index in the input/stratify textures of this chunk's first element.
    base_offset: u32,
}

@group(0) @binding(0) var<uniform>             uniforms:    ReduceUniforms;
@group(0) @binding(1) var                      input:       texture_2d<{{input_dtype}}>;
@group(0) @binding(2) var                      stratify:    texture_2d<{{stratify_dtype}}>;
@group(0) @binding(3) var<storage, read_write> output:      array<atomic<u32>>;
@group(0) @binding(4) var<storage, read_write> output_hist: array<atomic<u32>>;

// flat_texel_coord(idx, width): maps a flat element index to 2D texel coords.
{{flat_texel_coord}}

// Filtering membership test (background gate). See reduce.wgsl for details —
// identical mechanism, reused as-is.
{{filtering_wgsl}}

// Selection membership test (additionally narrows filter-included elements to
// foreground); ANDed with `is_filtered_in` by the entry points below.
{{selection_wgsl}}

// ── Helpers ────────────────────────────────────────────────────────────────

fn load_input(flat_index: u32) -> f32 {
    let tex_width = textureDimensions(input).x;
    let coords = flat_texel_coord(flat_index, tex_width);
    return f32(textureLoad(input, coords, 0).x);
}

fn load_stratify_code(flat_index: u32) -> i32 {
    let tex_width = textureDimensions(stratify).x;
    let coords = flat_texel_coord(flat_index, tex_width);
    return i32(textureLoad(stratify, coords, 0).x);
}

// Baked-in linear search over the requested strata's category codes (small
// and known at shader-build time), analogous to the categorical criteria
// membership test in `crate::emphasis_mode`. Returns -1 if `code` isn't one
// of the requested strata.
fn stratum_index(code: i32) -> i32 {
    let strata = array<i32, {{num_strata}}>({{strata_codes}});
    for (var i: u32 = 0u; i < {{num_strata}}u; i = i + 1u) {
        if (strata[i] == code) {
            return i32(i);
        }
    }
    return -1;
}

// Order-preserving float<->u32 bit-key mapping, so atomicMin/atomicMax (which
// only operate on integers) can combine floats: flipping the sign bit of a
// positive float's bits pushes it above every negative float's (fully
// flipped) bits, preserving float ordering in the unsigned domain. A standard
// technique for GPU atomic float min/max.
fn order_key(f: f32) -> u32 {
    let bits = bitcast<u32>(f);
    let mask = select(0x80000000u, 0xFFFFFFFFu, (bits & 0x80000000u) != 0u);
    return bits ^ mask;
}

fn order_key_to_f32(key: u32) -> f32 {
    let mask = select(0xFFFFFFFFu, 0x80000000u, (key & 0x80000000u) != 0u);
    return bitcast<f32>(key ^ mask);
}

// Atomically adds `value` into `output[idx]`, interpreting its bits as an f32.
// WGSL atomics only operate on u32/i32, so float accumulation needs a
// compare-exchange retry loop rather than a native atomic add.
fn atomic_add_f32(idx: u32, value: f32) {
    var old_bits = atomicLoad(&output[idx]);
    loop {
        let new_bits = bitcast<u32>(bitcast<f32>(old_bits) + value);
        let result = atomicCompareExchangeWeak(&output[idx], old_bits, new_bits);
        if (result.exchanged) {
            break;
        }
        old_bits = result.old_value;
    }
}

// ── Entry point: main_scalar ──────────────────────────────────────────────────

@compute @workgroup_size(64, 1, 1)
fn main_scalar(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let gid = global_id.x;
    if (gid >= uniforms.num_elements) {
        return;
    }
    let flat_index = uniforms.base_offset + gid;
    if (!is_filtered_in(flat_index)) {
        return;
    }
    let stratum = stratum_index(load_stratify_code(flat_index));
    if (stratum < 0) {
        return;
    }
    let s = u32(stratum);
    let is_fg = is_selected_in(flat_index);
    let mode = uniforms.mode;
    let v = load_input(flat_index);
    let base = s * 4u; // [bg_a, bg_b, fg_a, fg_b]

    if (mode == MODE_MIN) {
        atomicMin(&output[base], order_key(v));
        if (is_fg) { atomicMin(&output[base + 2u], order_key(v)); }
    } else if (mode == MODE_MAX) {
        atomicMax(&output[base], order_key(v));
        if (is_fg) { atomicMax(&output[base + 2u], order_key(v)); }
    } else if (mode == MODE_SUM) {
        atomic_add_f32(base, v);
        if (is_fg) { atomic_add_f32(base + 2u, v); }
    } else if (mode == MODE_COUNT) {
        atomicAdd(&output[base], 1u);
        if (is_fg) { atomicAdd(&output[base + 2u], 1u); }
    } else if (mode == MODE_MEAN) {
        atomic_add_f32(base, v);
        atomicAdd(&output[base + 1u], 1u);
        if (is_fg) {
            atomic_add_f32(base + 2u, v);
            atomicAdd(&output[base + 3u], 1u);
        }
    } else { // MODE_EXTENT
        atomicMin(&output[base], order_key(v));
        atomicMax(&output[base + 1u], order_key(v));
        if (is_fg) {
            atomicMin(&output[base + 2u], order_key(v));
            atomicMax(&output[base + 3u], order_key(v));
        }
    }
}

// ── Entry point: main_histogram ───────────────────────────────────────────────

@compute @workgroup_size(64, 1, 1)
fn main_histogram(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let gid = global_id.x;
    if (gid >= uniforms.num_elements) {
        return;
    }
    let flat_index = uniforms.base_offset + gid;
    if (!is_filtered_in(flat_index)) {
        return;
    }
    let stratum = stratum_index(load_stratify_code(flat_index));
    if (stratum < 0) {
        return;
    }
    let s = u32(stratum);
    let val = load_input(flat_index);
    let data_range = uniforms.data_max - uniforms.data_min;

    var bin: u32;
    if (data_range <= 0.0) {
        bin = 0u;
    } else {
        let t = (val - uniforms.data_min) / data_range;
        bin = u32(clamp(t * f32(uniforms.num_bins), 0.0, f32(uniforms.num_bins) - 1.0));
    }
    let base = s * uniforms.num_bins * 2u;
    atomicAdd(&output_hist[base + bin], 1u);
    if (is_selected_in(flat_index)) {
        atomicAdd(&output_hist[base + uniforms.num_bins + bin], 1u);
    }
}
