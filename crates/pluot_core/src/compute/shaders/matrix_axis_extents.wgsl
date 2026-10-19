// Computes the (min, max) of every row or every column of a matrix, skipping
// NaN values. The matrix is read through the injected `get_matrix_value`, so
// any storage layout (dense, CSR or CSC) works. One thread handles one whole
// row or column, which under-uses the GPU when there are few of them (e.g. per-column extents of a
// tall, narrow matrix), but needs no cross-thread reduction.
//
// ── Bindings ────────────────────────────────────────────────────────────────
//   @group(0) @binding(0)  uniforms : Uniforms            (uniform)
//   @group(0) @binding(1)  output   : array<f32>          (storage, read_write): [min, max] per row/column
//   @group(0) @binding(2+)                                (the matrix value textures `get_matrix_value` reads)
//
// A row or column with no non-NaN values is reported as (F32_MAX, -F32_MAX).

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
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var<storage, read_write> output: array<f32>;

{{flat_texel_coord}}

{{matrix_value}}

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= uniforms.num_segments) {
        return;
    }
    let segment = uniforms.base_segment + gid.x;
    let per_column = uniforms.axis == AXIS_COLS;
    let len = select(uniforms.num_cols, uniforms.num_rows, per_column);

    var lo = F32_MAX;
    var hi = -F32_MAX;
    for (var j = 0u; j < len; j = j + 1u) {
        var value: f32;
        if (per_column) {
            value = get_matrix_value(j, segment);
        } else {
            value = get_matrix_value(segment, j);
        }
        if (value == value) {
            lo = min(lo, value);
            hi = max(hi, value);
        }
    }
    output[2u * gid.x] = lo;
    output[2u * gid.x + 1u] = hi;
}
