// Computes the (min, max), or a histogram, of every row or every column of a
// matrix, skipping NaN values. The matrix is read through the injected `get_matrix_value`, so
// any storage layout (dense, CSR or CSC) works. One thread handles one whole
// row or column, which under-uses the GPU when there are few of them (e.g. per-column extents of a
// tall, narrow matrix), but needs no cross-thread reduction.
//
// ── Bindings ────────────────────────────────────────────────────────────────
//   @group(0) @binding(0)  uniforms : Uniforms            (uniform)
//   @group(0) @binding(1)  output   : array<u32>          (storage, read_write): the bits of [min, max], or num_bins counts, per row/column
//   @group(0) @binding(2)  ranges   : array<f32>          (storage, read): main_histogram only, [lo, hi] per row/column
//   @group(0) @binding(3+)                                (the matrix value textures `get_matrix_value` reads)
//
// A row or column with no non-NaN values is reported as (F32_MAX, -F32_MAX).
// Histogram binning matches reduce.wgsl's: out-of-range values are clamped into the edge bins.

const AXIS_COLS: u32 = 1u;
const F32_MAX: f32 = 3.40282347e38;

struct Uniforms {
    num_rows: u32,
    num_cols: u32,
    // 0 = one extent per row, 1 = one extent per column.
    axis: u32,
    // Number of rows/columns handled by this dispatch, starting at `base_segment`.
    num_segments: u32,
    base_segment: u32,
    num_bins: u32,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var<storage, read_write> output: array<u32>;
@group(0) @binding(2) var<storage, read> ranges: array<f32>;

{{flat_texel_coord}}

{{matrix_value}}

// `value != value` cannot be relied on, since shader compilers may assume values are never NaN.
fn is_nan(value: f32) -> bool {
    return (bitcast<u32>(value) & 0x7fffffffu) > 0x7f800000u;
}

fn segment_len() -> u32 {
    return select(uniforms.num_cols, uniforms.num_rows, uniforms.axis == AXIS_COLS);
}

fn segment_value(segment: u32, j: u32) -> f32 {
    if (uniforms.axis == AXIS_COLS) {
        return get_matrix_value(j, segment);
    }
    return get_matrix_value(segment, j);
}

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= uniforms.num_segments) {
        return;
    }
    let segment = uniforms.base_segment + gid.x;

    var lo = F32_MAX;
    var hi = -F32_MAX;
    for (var j = 0u; j < segment_len(); j = j + 1u) {
        let value = segment_value(segment, j);
        if (value == value) {
            lo = min(lo, value);
            hi = max(hi, value);
        }
    }
    output[2u * gid.x] = bitcast<u32>(lo);
    output[2u * gid.x + 1u] = bitcast<u32>(hi);
}

// Each thread owns its segment's bins, so no atomics are needed.
@compute @workgroup_size(64)
fn main_histogram(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= uniforms.num_segments) {
        return;
    }
    let segment = uniforms.base_segment + gid.x;
    let lo = ranges[2u * gid.x];
    let range = ranges[2u * gid.x + 1u] - lo;
    let first_bin = gid.x * uniforms.num_bins;

    for (var j = 0u; j < segment_len(); j = j + 1u) {
        let value = segment_value(segment, j);
        if (is_nan(value)) {
            continue;
        }
        var bin = 0u;
        if (range > 0.0) {
            bin = u32(clamp((value - lo) / range * f32(uniforms.num_bins), 0.0, f32(uniforms.num_bins) - 1.0));
        }
        output[first_bin + bin] = output[first_bin + bin] + 1u;
    }
}
