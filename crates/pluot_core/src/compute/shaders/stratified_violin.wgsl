// Per-stratum violin statistics, over the values that are filter-included,
// not NaN, and (unless `keep_zeros`) nonzero. Each value contributes to its
// stratum's background slot and, when it is also selection-included, to its
// foreground slot: slot = stratum * 2 + (0 for background, 1 for foreground).
// Each entry point is one pass; see `stratified_violin.rs` for how they chain.

const PERCENTILE_NUM_BINS: u32 = {{percentile_num_bins}}u;
const NUM_QUARTILES: u32 = 3u;

struct Uniforms {
    num_elements: u32,
    keep_zeros: u32,
    num_bins: u32,
    domain_min: f32,
    domain_max: f32,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var input: texture_2d<{{input_dtype}}>;
@group(0) @binding(2) var stratify: texture_2d<{{stratify_dtype}}>;
@group(0) @binding(3) var<storage, read_write> output: array<atomic<u32>>;
// Per-slot parameters of the pass (means, bounds, or ranges).
@group(0) @binding(4) var<storage, read> params: array<f32>;
// The bin thresholds within the domain, in ascending order.
@group(0) @binding(5) var<storage, read> thresholds: array<f32>;

{{flat_texel_coord}}

{{filtering_wgsl}}

{{selection_wgsl}}

// Tests the bits, since `value != value` may be folded away by fast-math shader compilers.
fn is_nan(value: f32) -> bool {
    return (bitcast<u32>(value) & 0x7FFFFFFFu) > 0x7F800000u;
}

fn load_input(flat_index: u32) -> f32 {
    return f32(textureLoad(input, flat_texel_coord(flat_index, textureDimensions(input).x), 0).x);
}

fn stratum_index(flat_index: u32) -> i32 {
    let code = i32(textureLoad(stratify, flat_texel_coord(flat_index, textureDimensions(stratify).x), 0).x);
    let strata = array<i32, {{num_strata}}>({{strata_codes}});
    for (var i = 0u; i < {{num_strata}}u; i = i + 1u) {
        if (strata[i] == code) {
            return i32(i);
        }
    }
    return -1;
}

// See reduce_stratified.wgsl.
fn order_key(f: f32) -> u32 {
    let bits = bitcast<u32>(f);
    let mask = select(0x80000000u, 0xFFFFFFFFu, (bits & 0x80000000u) != 0u);
    return bits ^ mask;
}

fn atomic_add_f32(idx: u32, value: f32) {
    var old_bits = atomicLoad(&output[idx]);
    loop {
        let result = atomicCompareExchangeWeak(&output[idx], old_bits, bitcast<u32>(bitcast<f32>(old_bits) + value));
        if (result.exchanged) {
            break;
        }
        old_bits = result.old_value;
    }
}

struct Counted {
    // -1 when the value is not counted.
    background_slot: i32,
    is_foreground: bool,
    value: f32,
};

fn element_index(workgroup_id: vec3<u32>, num_workgroups: vec3<u32>, local_index: u32) -> u32 {
    return (workgroup_id.x + workgroup_id.y * num_workgroups.x) * 64u + local_index;
}

fn counted(i: u32) -> Counted {
    var result = Counted(-1, false, 0.0);
    if (i >= uniforms.num_elements || !is_filtered_in(i)) {
        return result;
    }
    let value = load_input(i);
    if (is_nan(value) || (uniforms.keep_zeros == 0u && value == 0.0)) {
        return result;
    }
    let stratum = stratum_index(i);
    if (stratum < 0) {
        return result;
    }
    result.background_slot = stratum * 2;
    result.is_foreground = is_selected_in(i);
    result.value = value;
    return result;
}

// The slots that `c` contributes to: its background slot, then its foreground slot if any.
fn num_slots(c: Counted) -> u32 {
    return select(1u, 2u, c.is_foreground);
}

fn is_non_outlier(slot: u32, value: f32) -> bool {
    return params[slot * 2u] <= value && value <= params[slot * 2u + 1u];
}

// Per slot: [count, sum, order_key(min), order_key(max)].
@compute @workgroup_size(64)
fn moments(@builtin(workgroup_id) wid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>, @builtin(local_invocation_index) lid: u32) {
    let c = counted(element_index(wid, nwg, lid));
    if (c.background_slot < 0) {
        return;
    }
    for (var side = 0u; side < num_slots(c); side = side + 1u) {
        let base = (u32(c.background_slot) + side) * 4u;
        atomicAdd(&output[base], 1u);
        atomic_add_f32(base + 1u, c.value);
        atomicMin(&output[base + 2u], order_key(c.value));
        atomicMax(&output[base + 3u], order_key(c.value));
    }
}

// Per slot: the sum of squared differences from the slot's mean, `params[slot]`.
@compute @workgroup_size(64)
fn centered_squares(@builtin(workgroup_id) wid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>, @builtin(local_invocation_index) lid: u32) {
    let c = counted(element_index(wid, nwg, lid));
    if (c.background_slot < 0) {
        return;
    }
    for (var side = 0u; side < num_slots(c); side = side + 1u) {
        let slot = u32(c.background_slot) + side;
        let difference = c.value - params[slot];
        atomic_add_f32(slot, difference * difference);
    }
}

// Per slot: [order_key(min), order_key(max)] of the values within the slot's
// non-outlier bounds, `params[slot * 2..slot * 2 + 2]`.
@compute @workgroup_size(64)
fn non_outlier_extent(@builtin(workgroup_id) wid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>, @builtin(local_invocation_index) lid: u32) {
    let c = counted(element_index(wid, nwg, lid));
    if (c.background_slot < 0) {
        return;
    }
    for (var side = 0u; side < num_slots(c); side = side + 1u) {
        let slot = u32(c.background_slot) + side;
        if (is_non_outlier(slot, c.value)) {
            atomicMin(&output[slot * 2u], order_key(c.value));
            atomicMax(&output[slot * 2u + 1u], order_key(c.value));
        }
    }
}

// Per slot and quartile: a histogram of PERCENTILE_NUM_BINS bins spanning the
// range `params[r * 2..r * 2 + 2]` (r = slot * 3 + quartile), clamping values
// outside of it into the edge bins, as `PercentileSearch` expects.
@compute @workgroup_size(64)
fn quartile_histograms(@builtin(workgroup_id) wid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>, @builtin(local_invocation_index) lid: u32) {
    let c = counted(element_index(wid, nwg, lid));
    if (c.background_slot < 0) {
        return;
    }
    for (var side = 0u; side < num_slots(c); side = side + 1u) {
        let slot = u32(c.background_slot) + side;
        for (var quartile = 0u; quartile < NUM_QUARTILES; quartile = quartile + 1u) {
            let r = slot * NUM_QUARTILES + quartile;
            let lo = params[r * 2u];
            let range = params[r * 2u + 1u] - lo;
            var bin = 0u;
            if (range > 0.0) {
                bin = u32(clamp((c.value - lo) / range * f32(PERCENTILE_NUM_BINS), 0.0, f32(PERCENTILE_NUM_BINS - 1u)));
            }
            atomicAdd(&output[r * PERCENTILE_NUM_BINS + bin], 1u);
        }
    }
}

// Per slot: `num_bins` counts of the non-outliers within the domain (as
// `d3::array::bin` counts them), where bin `b` follows `b` thresholds.
@compute @workgroup_size(64)
fn violin_bins(@builtin(workgroup_id) wid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>, @builtin(local_invocation_index) lid: u32) {
    let c = counted(element_index(wid, nwg, lid));
    if (c.background_slot < 0 || c.value < uniforms.domain_min || c.value > uniforms.domain_max) {
        return;
    }
    var bin = 0u;
    for (var t = 0u; t + 1u < uniforms.num_bins; t = t + 1u) {
        if (thresholds[t] <= c.value) {
            bin = t + 1u;
        }
    }
    for (var side = 0u; side < num_slots(c); side = side + 1u) {
        let slot = u32(c.background_slot) + side;
        if (is_non_outlier(slot, c.value)) {
            atomicAdd(&output[slot * uniforms.num_bins + bin], 1u);
        }
    }
}
