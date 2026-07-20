// Reduction compute shader.
//
// Reference: https://github.com/wgmath/wgmath/blob/main/crates/wgebra/src/linalg/reduce.wgsl
//
// Two entry points are provided so that the output buffer type can differ:
//
//   main_scalar: Min (0), Max (1), Sum (2), Extent (3)
//   main_histogram: Histogram (4)
//
// The input array is uploaded once as a single-channel (red-only) 2D texture:
// flat element `idx` lives at texel `(idx % width, idx / width)`. Its sampled
// type (`f32`/`u32`/`i32`) is injected at runtime by the shader-module system
// (see `crate::shader_modules`), so the array is read at its native dtype and
// widened to f32 in the shader — no CPU-side cast. A single texture serves every
// dispatch chunk; `uniforms.base_offset` selects each chunk's element range.
//
// ── Bindings for main_scalar ────────────────────────────────────────────────
//   @group(0) @binding(0)  uniforms      : ReduceUniforms         (uniform)
//   @group(0) @binding(1)  input         : texture_2d<dtype>
//   @group(0) @binding(2)  output        : array<f32>             (storage, read_write)
//
//   Output layout:
//     Min / Max / Sum  -->  one f32 per workgroup (partial result); the caller
//                         reduces the workgroup_count partial values to one.
//     Extent           -->  two f32 per workgroup: [partial_min, partial_max]
//
// ── Bindings for main_histogram ─────────────────────────────────────────────
//   @group(0) @binding(0)  uniforms      : ReduceUniforms        (uniform)
//   @group(0) @binding(1)  input         : texture_2d<dtype>
//   @group(0) @binding(3)  output_hist   : array<atomic<u32>>    (storage, read_write)
//
//   output_hist must be zero-initialised by the caller before dispatch.
//   Size: uniforms.num_bins  (must be <= MAX_HISTOGRAM_BINS = 256).

// Constants

const WORKGROUP_SIZE: u32       = 64u;
const MAX_HISTOGRAM_BINS: u32   = 256u;

// Mode constants. Must match the Rust-side ReduceMode discriminants.
const MODE_MIN:       u32 = 0u;
const MODE_MAX:       u32 = 1u;
const MODE_SUM:       u32 = 2u;
const MODE_EXTENT:    u32 = 3u;
const MODE_HISTOGRAM: u32 = 4u;

// Uniforms

struct ReduceUniforms {
    // Which reduction to perform (see MODE_* constants above).
    mode: u32,
    // Number of elements processed by THIS dispatch (the current chunk length).
    num_elements: u32,
    // Histogram: number of bins. Must be <= MAX_HISTOGRAM_BINS.
    num_bins: u32,
    // Histogram: minimum value of the data range (inclusive).
    data_min: f32,
    // Histogram: maximum value of the data range (exclusive).
    data_max: f32,
    // Flat index in the input texture of this chunk's first element. Added to
    // the per-dispatch global id to locate each element in the shared texture.
    base_offset: u32,
}

@group(0) @binding(0) var<uniform>             uniforms:     ReduceUniforms;
@group(0) @binding(1) var                      input:        texture_2d<{{input_dtype}}>;
@group(0) @binding(2) var<storage, read_write> output:       array<f32>;
@group(0) @binding(3) var<storage, read_write> output_hist:  array<atomic<u32>>;

// flat_texel_coord(idx, width): maps a flat element index to 2D texel coords.
{{flat_texel_coord}}

// ── Workgroup-shared memory ───────────────────────────────────────────────────

// Tree-reduction accumulators for scalar modes.
// shared_a: primary  (min / max / sum / extent-min)
// shared_b: secondary (extent-max only)
var<workgroup> shared_a: array<f32, 64>;
var<workgroup> shared_b: array<f32, 64>;

// Per-workgroup histogram bins; zero-initialised by the WebGPU runtime
// (workgroup address space is defined in spec 6.3.1).
var<workgroup> local_hist: array<atomic<u32>, 256>;

// Helpers

// Maximum finite f32 (identity for min-reduction).
fn pos_inf() -> f32 { return 0x1.fffffep+127f; }

// Minimum finite f32 (identity for max-reduction).
fn neg_inf() -> f32 { return -0x1.fffffep+127f; }

// Read input element `flat_index`, mapping the flat index into the 2D texture
// the array was reshaped into on upload (idx % width, idx / width). `f32(...)`
// is a no-op when the injected sampled type is already f32, and widens u32/i32
// texels to f32 otherwise.
fn load_input(flat_index: u32) -> f32 {
    let tex_width = textureDimensions(input).x;
    let coords = flat_texel_coord(flat_index, tex_width);
    return f32(textureLoad(input, coords, 0).x);
}

// ── Entry point: main_scalar ──────────────────────────────────────────────────
//
// Each workgroup of 64 threads reduces a contiguous 64-element tile of the
// input to a single partial result via a parallel binary-tree reduction in
// workgroup-shared memory.  Thread 0 then writes the partial result to
// output[workgroup_id] (or output[workgroup_id * 2 .. +1] for Extent).
//
// The caller is responsible for a second reduction pass over the
// workgroup_count partial results to obtain the final scalar answer.

@compute @workgroup_size(64, 1, 1)
fn main_scalar(
    @builtin(global_invocation_id) global_id:    vec3<u32>,
    @builtin(local_invocation_id)  local_id:     vec3<u32>,
    @builtin(workgroup_id)         workgroup_id: vec3<u32>,
) {
    let lid  = local_id.x;
    let gid  = global_id.x;
    let wid  = workgroup_id.x;
    let mode = uniforms.mode;
    let in_bounds = gid < uniforms.num_elements;

    // ── Load into shared memory with identity values for out-of-bounds lanes ──

    if in_bounds {
        let v = load_input(uniforms.base_offset + gid);
        if mode == MODE_EXTENT {
            shared_a[lid] = v; // min accumulator
            shared_b[lid] = v; // max accumulator
        } else {
            shared_a[lid] = v;
        }
    } else {
        if mode == MODE_MIN {
            shared_a[lid] = pos_inf();
        } else if mode == MODE_MAX {
            shared_a[lid] = neg_inf();
        } else if mode == MODE_SUM {
            shared_a[lid] = 0.0;
        } else { // MODE_EXTENT
            shared_a[lid] = pos_inf();
            shared_b[lid] = neg_inf();
        }
    }
    workgroupBarrier();

    // ── Parallel binary-tree reduction ───────────────────────────────────────
    //
    // Each step halves the active set.  stride is uniform across all invocations,
    // so workgroupBarrier() is reached in uniform control flow every iteration.

    var stride = WORKGROUP_SIZE / 2u; // 32
    while stride > 0u {
        if lid < stride {
            if mode == MODE_MIN {
                shared_a[lid] = min(shared_a[lid], shared_a[lid + stride]);
            } else if mode == MODE_MAX {
                shared_a[lid] = max(shared_a[lid], shared_a[lid + stride]);
            } else if mode == MODE_SUM {
                shared_a[lid] = shared_a[lid] + shared_a[lid + stride];
            } else { // MODE_EXTENT: simultaneous min and max
                shared_a[lid] = min(shared_a[lid], shared_a[lid + stride]);
                shared_b[lid] = max(shared_b[lid], shared_b[lid + stride]);
            }
        }
        workgroupBarrier();
        stride /= 2u;
    }

    // ── Thread 0 writes the partial result for this workgroup ─────────────────

    if lid == 0u {
        if mode == MODE_EXTENT {
            output[wid * 2u]      = shared_a[0u]; // partial min
            output[wid * 2u + 1u] = shared_b[0u]; // partial max
        } else {
            output[wid] = shared_a[0u];
        }
    }
}

// ── Entry point: main_histogram ───────────────────────────────────────────────
//
// Each thread increments the appropriate bin of a workgroup-local histogram
// stored in shared memory (avoiding contention on global atomics for large
// workloads).  After all threads have voted, each thread flushes a slice of
// the local histogram to the global output via atomicAdd, so contributions
// from every workgroup are correctly accumulated.
//
// The global output_hist buffer must be zero-initialised by the caller before
// the first dispatch (a single fill pass or buffer creation with zeroed data).
//
// Bin assignment: bin = floor((value - data_min) / (data_max - data_min) * num_bins)
// Values outside [data_min, data_max) are clamped to the nearest edge bin.

@compute @workgroup_size(64, 1, 1)
fn main_histogram(
    @builtin(global_invocation_id) global_id: vec3<u32>,
    @builtin(local_invocation_id)  local_id:  vec3<u32>,
) {
    let lid       = local_id.x;
    let gid       = global_id.x;
    let num_bins  = uniforms.num_bins;
    let data_range = uniforms.data_max - uniforms.data_min;

    // local_hist is zero-initialised (workgroup address space).

    // ── Accumulate into workgroup-local histogram ─────────────────────────────

    if gid < uniforms.num_elements {
        let val = load_input(uniforms.base_offset + gid);
        var bin: u32;
        if data_range <= 0.0 {
            bin = 0u;
        } else {
            // Normalised position in [0, 1); clamp to keep within valid bin range.
            let t = (val - uniforms.data_min) / data_range;
            bin = u32(clamp(t * f32(num_bins), 0.0, f32(num_bins) - 1.0));
        }
        atomicAdd(&local_hist[bin], 1u);
    }
    workgroupBarrier();

    // ── Flush workgroup-local counts to global output ─────────────────────────
    //
    // Distributes bin ownership across threads: thread `lid` handles bins
    // lid, lid+64, lid+128, … up to num_bins.

    for (var b = lid; b < num_bins; b += WORKGROUP_SIZE) {
        let count = atomicLoad(&local_hist[b]);
        if count > 0u {
            atomicAdd(&output_hist[b], count);
        }
    }
}
