// Computes the (min, max) of every row or every column of a row-major matrix,
// skipping NaN values. One thread handles one whole row or column, which
// under-uses the GPU when there are few of them (e.g. per-column extents of a
// tall, narrow matrix), but needs no cross-thread reduction.
//
// ── Bindings ────────────────────────────────────────────────────────────────
//   @group(0) @binding(0)  uniforms : Uniforms            (uniform)
//   @group(0) @binding(1)  input    : texture_2d<dtype>   (flat row-major matrix, see `flat_texel_coord`)
//   @group(0) @binding(2)  output   : array<f32>          (storage, read_write): [min, max] per row/column
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
@group(0) @binding(1) var input: texture_2d<{{input_dtype}}>;
@group(0) @binding(2) var<storage, read_write> output: array<f32>;

{{flat_texel_coord}}

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= uniforms.num_segments) {
        return;
    }
    let segment = uniforms.base_segment + gid.x;
    let per_column = uniforms.axis == AXIS_COLS;
    let len = select(uniforms.num_cols, uniforms.num_rows, per_column);
    let width = textureDimensions(input).x;

    var lo = F32_MAX;
    var hi = -F32_MAX;
    for (var j = 0u; j < len; j = j + 1u) {
        let idx = select(segment * uniforms.num_cols + j, j * uniforms.num_cols + segment, per_column);
        let value = f32(textureLoad(input, flat_texel_coord(idx, width), 0).x);
        if (value == value) {
            lo = min(lo, value);
            hi = max(hi, value);
        }
    }
    output[2u * gid.x] = lo;
    output[2u * gid.x + 1u] = hi;
}
