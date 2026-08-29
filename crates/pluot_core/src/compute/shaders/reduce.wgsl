// Reduction compute shader.
//
// Reference: https://github.com/wgmath/wgmath/blob/main/crates/wgebra/src/linalg/reduce.wgsl
//
// Computes BOTH the "background" (filter-included) and "foreground"
// (filter-*and*-selection-included) reduction in a single dispatch: every
// thread tests `is_filtered_in`/`is_selected_in` once and feeds two parallel
// accumulators (background, foreground) through the same tree reduction,
// rather than the whole shader being dispatched twice with two different
// criteria predicates.
//
// Two entry points are provided so that the output buffer type can differ:
//
//   main_scalar: Min (0), Max (1), Sum (2), Extent (3), Count (5), Mean (6)
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
//   Output layout, per workgroup — background partial(s) then foreground
//   partial(s):
//     Min / Max / Sum / Count -->  2 f32: [bg, fg]; the caller reduces the
//                         workgroup_count (bg, fg) pairs to one each.
//     Extent           -->  4 f32: [bg_min, bg_max, fg_min, fg_max]
//     Mean             -->  4 f32: [bg_sum, bg_count, fg_sum, fg_count];
//                         the caller sums each component separately, then divides.
//
// ── Bindings for main_histogram ─────────────────────────────────────────────
//   @group(0) @binding(0)  uniforms      : ReduceUniforms        (uniform)
//   @group(0) @binding(1)  input         : texture_2d<dtype>
//   @group(0) @binding(3)  output_hist   : array<atomic<u32>>    (storage, read_write)
//
//   output_hist must be zero-initialised by the caller before dispatch.
//   Size: 2 * uniforms.num_bins (must be <= MAX_HISTOGRAM_BINS = 256) — the
//   first half holds background bin counts, the second half foreground.
//
// ── Filtering/selection criteria bindings ───────────────────────────────────
//   @group(0) @binding(4..)             filtering criteria value textures,
//                                       assembled by `is_filtered_in` (see
//                                       `crate::emphasis_mode::prepare_emphasis_criteria`).
//   @group(0) @binding(4+n_filter..)    selection criteria value textures,
//                                       assembled likewise by `is_selected_in`.
//   `is_filtered_in` gates both background and foreground; `is_selected_in`
//   (tested only once `is_filtered_in` already holds) narrows to foreground.

// Constants

const WORKGROUP_SIZE: u32       = 64u;
const MAX_HISTOGRAM_BINS: u32   = 256u;

// Mode constants. Must match the Rust-side ReduceMode discriminants.
const MODE_MIN:       u32 = 0u;
const MODE_MAX:       u32 = 1u;
const MODE_SUM:       u32 = 2u;
const MODE_EXTENT:    u32 = 3u;
const MODE_HISTOGRAM: u32 = 4u;
const MODE_COUNT:     u32 = 5u;
const MODE_MEAN:      u32 = 6u;

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

// Filtering membership test (background gate): ANDs together whichever
// filtering criteria this dispatch was built with (see
// `crate::emphasis_mode::prepare_emphasis_criteria`). An empty criteria list
// compiles to an always-true predicate with no texture bindings.
{{filtering_wgsl}}

// Selection membership test (additionally narrows filter-included elements to
// foreground): same mechanism, built from the selection criteria alone (not
// pre-ANDed with filtering — the entry points below AND it with
// `is_filtered_in` themselves).
{{selection_wgsl}}

// ── Workgroup-shared memory ───────────────────────────────────────────────────

// Tree-reduction accumulators for scalar modes, doubled for background/foreground.
// *_a: primary  (min / max / sum / extent-min / mean-sum)
// *_b: secondary (extent-max / mean-count only)
var<workgroup> shared_bg_a: array<f32, 64>;
var<workgroup> shared_bg_b: array<f32, 64>;
var<workgroup> shared_fg_a: array<f32, 64>;
var<workgroup> shared_fg_b: array<f32, 64>;

// Per-workgroup histogram bins, doubled for background/foreground;
// zero-initialised by the WebGPU runtime (workgroup address space is defined
// in spec 6.3.1).
var<workgroup> local_hist_bg: array<atomic<u32>, 256>;
var<workgroup> local_hist_fg: array<atomic<u32>, 256>;

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

// Per-mode identity/load/combine for the primary (*_a) and secondary (*_b)
// accumulator, shared by the background and foreground reduction so the mode
// dispatch logic below isn't duplicated per side.

fn identity_a(mode: u32) -> f32 {
    if (mode == MODE_MIN || mode == MODE_EXTENT) { return pos_inf(); }
    if (mode == MODE_MAX) { return neg_inf(); }
    return 0.0; // SUM, COUNT, MEAN
}

fn identity_b(mode: u32) -> f32 {
    if (mode == MODE_EXTENT) { return neg_inf(); }
    return 0.0; // MEAN's count (unused by MIN/MAX/SUM/COUNT)
}

fn load_a(mode: u32, v: f32) -> f32 {
    if (mode == MODE_COUNT) { return 1.0; }
    return v; // MIN/MAX/SUM/EXTENT/MEAN all seed slot a with the raw value
}

fn load_b(mode: u32, v: f32) -> f32 {
    if (mode == MODE_EXTENT) { return v; }
    if (mode == MODE_MEAN) { return 1.0; }
    return 0.0; // unused
}

fn combine_a(mode: u32, a: f32, b: f32) -> f32 {
    if (mode == MODE_MIN || mode == MODE_EXTENT) { return min(a, b); }
    if (mode == MODE_MAX) { return max(a, b); }
    return a + b; // SUM, COUNT, MEAN
}

fn combine_b(mode: u32, a: f32, b: f32) -> f32 {
    if (mode == MODE_EXTENT) { return max(a, b); }
    return a + b; // MEAN's count (unused by MIN/MAX/SUM/COUNT)
}

// ── Entry point: main_scalar ──────────────────────────────────────────────────
//
// Each workgroup of 64 threads reduces a contiguous 64-element tile of the
// input to one background and one foreground partial result via two parallel
// binary-tree reductions in workgroup-shared memory. Thread 0 then writes
// both partials for this workgroup (see the binding table above for layout).
//
// The caller is responsible for a second reduction pass over the
// workgroup_count partial results to obtain the final background/foreground
// scalars.

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
    let flat_index = uniforms.base_offset + gid;
    let in_bounds = gid < uniforms.num_elements;
    let is_bg = in_bounds && is_filtered_in(flat_index);
    let is_fg = is_bg && is_selected_in(flat_index);

    // ── Load into shared memory with identity values for excluded lanes ──

    var v = 0.0;
    if (is_bg) {
        v = load_input(flat_index);
        shared_bg_a[lid] = load_a(mode, v);
        shared_bg_b[lid] = load_b(mode, v);
    } else {
        shared_bg_a[lid] = identity_a(mode);
        shared_bg_b[lid] = identity_b(mode);
    }
    if (is_fg) {
        shared_fg_a[lid] = load_a(mode, v);
        shared_fg_b[lid] = load_b(mode, v);
    } else {
        shared_fg_a[lid] = identity_a(mode);
        shared_fg_b[lid] = identity_b(mode);
    }
    workgroupBarrier();

    // ── Parallel binary-tree reduction (background and foreground together) ──
    //
    // Each step halves the active set.  stride is uniform across all invocations,
    // so workgroupBarrier() is reached in uniform control flow every iteration.

    var stride = WORKGROUP_SIZE / 2u; // 32
    while stride > 0u {
        if lid < stride {
            shared_bg_a[lid] = combine_a(mode, shared_bg_a[lid], shared_bg_a[lid + stride]);
            shared_bg_b[lid] = combine_b(mode, shared_bg_b[lid], shared_bg_b[lid + stride]);
            shared_fg_a[lid] = combine_a(mode, shared_fg_a[lid], shared_fg_a[lid + stride]);
            shared_fg_b[lid] = combine_b(mode, shared_fg_b[lid], shared_fg_b[lid + stride]);
        }
        workgroupBarrier();
        stride /= 2u;
    }

    // ── Thread 0 writes both partial results for this workgroup ───────────────

    if lid == 0u {
        if mode == MODE_EXTENT || mode == MODE_MEAN {
            output[wid * 4u]      = shared_bg_a[0u];
            output[wid * 4u + 1u] = shared_bg_b[0u];
            output[wid * 4u + 2u] = shared_fg_a[0u];
            output[wid * 4u + 3u] = shared_fg_b[0u];
        } else {
            output[wid * 2u]      = shared_bg_a[0u];
            output[wid * 2u + 1u] = shared_fg_a[0u];
        }
    }
}

// ── Entry point: main_histogram ───────────────────────────────────────────────
//
// Each thread increments the appropriate bin of a workgroup-local histogram
// stored in shared memory (avoiding contention on global atomics for large
// workloads) — once for background, and again for foreground when selected.
// After all threads have voted, each thread flushes a slice of the local
// histograms to the global output via atomicAdd, so contributions from every
// workgroup are correctly accumulated.
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
    let flat_index = uniforms.base_offset + gid;

    // local_hist_bg/local_hist_fg are zero-initialised (workgroup address space).

    // ── Accumulate into workgroup-local histograms ────────────────────────────
    // Filter-excluded elements skip both; selection-excluded elements still
    // count toward background but not foreground.

    if gid < uniforms.num_elements && is_filtered_in(flat_index) {
        let val = load_input(flat_index);
        var bin: u32;
        if data_range <= 0.0 {
            bin = 0u;
        } else {
            // Normalised position in [0, 1); clamp to keep within valid bin range.
            let t = (val - uniforms.data_min) / data_range;
            bin = u32(clamp(t * f32(num_bins), 0.0, f32(num_bins) - 1.0));
        }
        atomicAdd(&local_hist_bg[bin], 1u);
        if (is_selected_in(flat_index)) {
            atomicAdd(&local_hist_fg[bin], 1u);
        }
    }
    workgroupBarrier();

    // ── Flush workgroup-local counts to global output ─────────────────────────
    //
    // Distributes bin ownership across threads: thread `lid` handles bins
    // lid, lid+64, lid+128, … up to num_bins. Background bins occupy
    // [0, num_bins); foreground bins occupy [num_bins, 2*num_bins).

    for (var b = lid; b < num_bins; b += WORKGROUP_SIZE) {
        let count_bg = atomicLoad(&local_hist_bg[b]);
        if count_bg > 0u {
            atomicAdd(&output_hist[b], count_bg);
        }
        let count_fg = atomicLoad(&local_hist_fg[b]);
        if count_fg > 0u {
            atomicAdd(&output_hist[num_bins + b], count_fg);
        }
    }
}
