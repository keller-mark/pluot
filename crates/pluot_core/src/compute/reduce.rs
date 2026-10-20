use crate::{render_types::GpuContext, wgpu};
use crate::emphasis_mode::{cpu_is_included, prepare_emphasis_criteria};
use crate::numeric_data::NumericData;
use crate::render_traits::EmphasisCriteria;
use crate::shader_modules::{common, ShaderBuilder, TextureDtype};
use std::num::NonZeroU64;
use futures::FutureExt;
use futures_intrusive::channel::shared::oneshot_channel;

use encase::{ShaderType, UniformBuffer};

use super::ForegroundBackground;

// Reference: https://github.com/gfx-rs/wgpu/blob/trunk/examples/standalone/01_hello_compute/src/main.rs

/// Binding index of the first filtering criteria value texture (see
/// `crate::emphasis_mode::prepare_emphasis_criteria`). Chosen to sit clear of
/// every binding used by either `main_scalar` (0, 1, 2) or `main_histogram`
/// (0, 1, 3). Selection criteria textures immediately follow the filtering
/// criteria textures (however many of those there are).
const CRITERIA_FIRST_BINDING: u32 = 4;

/// Specify the behavior of the reducer functionality.
// Discriminant values must stay in sync with the WGSL MODE_* constants in shaders/reduce.wgsl.
#[derive(Debug, Clone)]
pub enum ReduceMode {
    /// Output: one f32 (the global minimum).
    Min,
    /// Output: one f32 (the global maximum).
    Max,
    /// Output: one f32 (the sum of all elements).
    Sum,
    /// Output: (f32, f32) for (global minimum, global maximum).
    Extent,
    /// Output: Vec<u32> of length `num_bins`.
    ///
    /// Values are binned into `[data_min, data_max)`.  Values outside that
    /// range are clamped to the nearest edge bin.  `num_bins` must not exceed
    /// `MAX_HISTOGRAM_BINS` (256) defined in the shader.
    Histogram {
        num_bins: u32,
        data_min: f32,
        data_max: f32,
    },
    /// Output: one f32 (the number of included elements).
    Count,
    /// Output: one f32 (the mean of all included elements, or `NaN` if none
    /// are included).
    Mean,
}



impl ReduceMode {
    /// Discriminant matching the WGSL `MODE_*` constants (shared by
    /// `reduce.wgsl` and `reduce_stratified.wgsl`).
    pub fn discriminant(&self) -> u32 {
        match self {
            ReduceMode::Min => 0,
            ReduceMode::Max => 1,
            ReduceMode::Sum => 2,
            ReduceMode::Extent => 3,
            ReduceMode::Histogram { .. } => 4,
            ReduceMode::Count => 5,
            ReduceMode::Mean => 6,
        }
    }

    pub fn is_histogram(&self) -> bool {
        matches!(self, ReduceMode::Histogram { .. })
    }

    /// Whether this mode writes two f32 values per workgroup (like
    /// [`ReduceMode::Extent`]'s `[min, max]` pair) rather than one.
    fn is_dual_output(&self) -> bool {
        matches!(self, ReduceMode::Extent | ReduceMode::Mean)
    }
}

// Uniform struct

/// Must match `ReduceUniforms` in shaders/reduce.wgsl (and
/// shaders/reduce_stratified.wgsl, which reuses this same Rust struct)
/// exactly (field order and types).  6 x 4 bytes = 24 bytes.
#[derive(ShaderType)]
pub struct ReduceUniforms {
    pub mode: u32,
    /// Number of elements processed by the current dispatch (chunk length).
    pub num_elements: u32,
    pub num_bins: u32,
    pub data_min: f32,
    pub data_max: f32,
    /// Flat index of the current chunk's first element within the input texture.
    pub base_offset: u32,
}

// Core dispatch function

/// Maps `download_buffer` for reading, copies its contents into a `Vec<f32>`
/// (interpreting the raw bytes as `f32`), then unmaps it so the buffer can be
/// reused or dropped.
async fn read_back_f32(device: &wgpu::Device, download_buffer: &wgpu::Buffer) -> Vec<f32> {
    let buffer_slice = download_buffer.slice(..);

    #[cfg(target_arch = "wasm32")]
    {
        let (sender, receiver) = oneshot_channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |res| {
            if res.is_err() {
                panic!("Failed to map buffer for reading");
            }
            sender.send(res).ok();
        });
        let _ = device.poll(wgpu::PollType::Poll);
        receiver.receive().await.unwrap().unwrap();
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            if result.is_err() {
                panic!("Failed to map buffer for reading");
            }
        });
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
    }

    let data = buffer_slice.get_mapped_range().expect("MapRangeError");
    let result = bytemuck::allocation::pod_collect_to_vec(&data);
    drop(data);
    download_buffer.unmap();
    result
}

/// Dispatches a GPU reduction over `input_view`, computing both the
/// filter-included ("background") and filter-*and*-selection-included
/// ("foreground") result in a single pass — every thread tests
/// `is_filtered_in(idx)`/`is_selected_in(idx)` once and feeds two parallel
/// accumulators, rather than the shader being dispatched twice with two
/// different criteria predicates. `filtering_criteria` and
/// `selection_criteria` are each AND-ed together independently, matching
/// [`crate::emphasis_mode::cpu_is_included`]'s semantics: an empty list means
/// every item passes.
///
/// `total` is the element count of the (already-uploaded) `input_view`.
///
/// **Large inputs**
///
/// The input is processed in chunks so that an arbitrarily large array can be
/// reduced even though a single dispatch is bounded by
/// `max_compute_workgroups_per_dimension` (~65,535 or ~4M elements) and a
/// single storage binding is bounded by `max_storage_buffer_binding_size`
/// (default 128 MiB or ~33M elements).  Each chunk is dispatched separately and
/// the partial results are combined as described below.
async fn dispatch_reduce(
    gpu_context: &GpuContext<'_>,
    input_view: &wgpu::TextureView,
    input_dtype: TextureDtype,
    total: usize,
    mode: &ReduceMode,
    filtering_criteria: &[EmphasisCriteria],
    selection_criteria: &[EmphasisCriteria],
) -> ForegroundBackground<Vec<f32>> {
    let GpuContext { device, queue } = gpu_context;
    let is_histogram = mode.is_histogram();
    let is_dual_output = mode.is_dual_output();

    // Two independent membership predicates, injected side by side: the
    // background gate (`is_filtered_in`, from `filtering_criteria` alone) and
    // the additional foreground narrowing (`is_selected_in`, from
    // `selection_criteria` alone, not pre-ANDed with filtering — the shader
    // ANDs it with `is_filtered_in` itself). Selection textures are bound
    // right after however many filtering textures there are.
    let filtering = prepare_emphasis_criteria(
        device, queue, filtering_criteria, "is_filtered_in", "filter_data", CRITERIA_FIRST_BINDING,
    );
    let selection_first_binding = CRITERIA_FIRST_BINDING + filtering.textures.len() as u32;
    let selection = prepare_emphasis_criteria(
        device, queue, selection_criteria, "is_selected_in", "select_data", selection_first_binding,
    );

    let shader_source = ShaderBuilder::new(include_str!("shaders/reduce.wgsl"))
        .inject_texture_sample_type("input", input_dtype)
        .inject_function("flat_texel_coord", common::FLAT_TEXEL_COORD)
        .inject_function("filtering_wgsl", &filtering.wgsl)
        .inject_function("selection_wgsl", &selection.wgsl)
        .build();
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("reduce.wgsl"),
        source: wgpu::ShaderSource::Wgsl(shader_source.into()),
    });

    let (num_bins, data_min, data_max) = match mode {
        ReduceMode::Histogram { num_bins, data_min, data_max } => (*num_bins, *data_min, *data_max),
        _ => (0, 0.0, 0.0),
    };

    // ── Chunk sizing ──────────────────────────────────────────────────────────
    //
    // The whole input lives in one texture, so the only per-dispatch limit is
    // the workgroup count: an element count above
    // `max_compute_workgroups_per_dimension * 64` would exceed a single
    // dispatch, so we process the input in chunks of that size, each reading
    // from the shared texture at its own `base_offset`.

    let limits = device.limits();
    let max_elems_by_dispatch =
        (limits.max_compute_workgroups_per_dimension as usize).saturating_mul(64);
    let chunk_elements = max_elems_by_dispatch.max(64);

    // ── Uniform layout (size is constant across chunks) ───────────────────────

    let uniform_size = {
        let mut buffer = UniformBuffer::new(Vec::<u8>::new());
        buffer
            .write(&ReduceUniforms {
                mode: mode.discriminant(),
                num_elements: 0,
                num_bins,
                data_min,
                data_max,
                base_offset: 0,
            })
            .unwrap();
        buffer.into_inner().len() as u64
    };

    // ── Bind group layout & pipeline (created once, reused per chunk) ─────────
    //
    // main_scalar    uses bindings 0 (uniform), 1 (input), 2 (output f32)
    // main_histogram uses bindings 0 (uniform), 1 (input), 3 (output atomic<u32>)
    // Both additionally use bindings CRITERIA_FIRST_BINDING.. for the
    // filtering criteria value textures, immediately followed by the
    // selection criteria value textures.

    let uniform_entry = wgpu::BindGroupLayoutEntry {
        binding: 0,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            min_binding_size: NonZeroU64::new(uniform_size),
            has_dynamic_offset: false,
        },
        count: None,
    };
    let input_entry = wgpu::BindGroupLayoutEntry {
        binding: 1,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Texture {
            sample_type: input_dtype.binding_sample_type(),
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    };
    let output_entry = wgpu::BindGroupLayoutEntry {
        // binding 2 for scalar modes, 3 for histogram
        binding: if is_histogram { 3 } else { 2 },
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: false },
            min_binding_size: Some(NonZeroU64::new(4).unwrap()),
            has_dynamic_offset: false,
        },
        count: None,
    };

    let mut bind_group_layout_entries = vec![uniform_entry, input_entry, output_entry];
    for (i, tex) in filtering.textures.iter().chain(selection.textures.iter()).enumerate() {
        bind_group_layout_entries.push(wgpu::BindGroupLayoutEntry {
            binding: CRITERIA_FIRST_BINDING + i as u32,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Texture {
                sample_type: tex.sample_type,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        });
    }

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &bind_group_layout_entries,
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });

    let entry_point = if is_histogram { "main_histogram" } else { "main_scalar" };

    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some(entry_point),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });

    // ── Histogram: a single output buffer accumulated across all chunks ───────
    //
    // The histogram shader accumulates into this buffer with global atomics, so
    // every chunk's dispatch adds into the same bins.  WebGPU zero-initialises
    // newly created buffers, so it starts at zero without an explicit clear.
    // Holds background bins at [0, num_bins) and foreground bins at
    // [num_bins, 2*num_bins).

    let hist_output = if is_histogram {
        Some(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("reduce_histogram_output"),
            size: (num_bins as u64) * 2 * 4,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        }))
    } else {
        None
    };

    // ── Process each chunk ────────────────────────────────────────────────────

    let mut scalar_partials: Vec<f32> = Vec::new();
    let mut offset = 0usize;

    while offset < total {
        let chunk_len = (total - offset).min(chunk_elements);
        let workgroup_count = chunk_len.div_ceil(64);

        // Uniforms for this chunk (num_elements is the chunk length; base_offset
        // locates the chunk's first element within the shared input texture).

        let mut uniform_buf = UniformBuffer::new(Vec::<u8>::new());
        uniform_buf
            .write(&ReduceUniforms {
                mode: mode.discriminant(),
                num_elements: chunk_len as u32,
                num_bins,
                data_min,
                data_max,
                base_offset: offset as u32,
            })
            .unwrap();
        let uniform_bytes = uniform_buf.into_inner();

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("reduce_uniforms"),
            size: uniform_bytes.len() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&uniform_buffer, 0, &uniform_bytes);

        // Output buffer: per-chunk for scalar modes, shared accumulator for
        // histogram. Every workgroup writes both a background and a
        // foreground partial:
        //
        //   Min / Max / Sum / Count  --> 2 f32 per workgroup: [bg, fg]
        //   Extent / Mean            --> 4 f32 per workgroup: [bg_a, bg_b, fg_a, fg_b]

        let scalar_output_bytes: u64 = if is_dual_output {
            (workgroup_count as u64) * 4 * 4
        } else {
            (workgroup_count as u64) * 2 * 4
        };

        let scalar_output_buffer = if is_histogram {
            None
        } else {
            Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("reduce_output"),
                size: scalar_output_bytes,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }))
        };

        let output_binding_resource = if is_histogram {
            hist_output.as_ref().unwrap().as_entire_binding()
        } else {
            scalar_output_buffer.as_ref().unwrap().as_entire_binding()
        };

        let mut bind_group_entries = vec![
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(input_view),
            },
            wgpu::BindGroupEntry {
                binding: if is_histogram { 3 } else { 2 },
                resource: output_binding_resource,
            },
        ];
        for (i, tex) in filtering.textures.iter().chain(selection.textures.iter()).enumerate() {
            bind_group_entries.push(wgpu::BindGroupEntry {
                binding: CRITERIA_FIRST_BINDING + i as u32,
                resource: wgpu::BindingResource::TextureView(&tex.view),
            });
        }

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &bind_group_layout,
            entries: &bind_group_entries,
        });

        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        {
            let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });
            compute_pass.set_pipeline(&pipeline);
            compute_pass.set_bind_group(0, &bind_group, &[]);
            compute_pass.dispatch_workgroups(workgroup_count as u32, 1, 1);
        }

        // For scalar modes, read this chunk's partials back immediately and
        // append them.  For histogram, the result accumulates in hist_output and
        // is read back once after all chunks complete.
        if let Some(scalar_output_buffer) = scalar_output_buffer.as_ref() {
            let download_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("reduce_download"),
                size: scalar_output_bytes,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            encoder.copy_buffer_to_buffer(
                scalar_output_buffer,
                0,
                &download_buffer,
                0,
                scalar_output_bytes,
            );
            queue.submit([encoder.finish()]);
            scalar_partials.extend(read_back_f32(device, &download_buffer).await);
        } else {
            queue.submit([encoder.finish()]);
        }

        offset += chunk_len;
    }

    // ── Read back the final result and split into background/foreground ──────
    //
    // Each raw group (2-wide for single-value modes, 4-wide for dual-value
    // modes) interleaves background then foreground; split it back into two
    // flat lists shaped exactly like a single (pre-fusion) dispatch's output,
    // so every `reduce_*` wrapper's fold logic is unaffected by this fusion.

    if is_histogram {
        let size = (num_bins as u64) * 2 * 4;
        let download_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("reduce_download"),
            size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        encoder.copy_buffer_to_buffer(hist_output.as_ref().unwrap(), 0, &download_buffer, 0, size);
        queue.submit([encoder.finish()]);
        let raw = read_back_f32(device, &download_buffer).await;
        let (background, foreground) = raw.split_at(num_bins as usize);
        ForegroundBackground { background: background.to_vec(), foreground: foreground.to_vec() }
    } else if is_dual_output {
        let mut background = Vec::with_capacity(scalar_partials.len() / 2);
        let mut foreground = Vec::with_capacity(scalar_partials.len() / 2);
        for group in scalar_partials.chunks(4) {
            background.extend_from_slice(&group[0..2]);
            foreground.extend_from_slice(&group[2..4]);
        }
        ForegroundBackground { background, foreground }
    } else {
        let mut background = Vec::with_capacity(scalar_partials.len() / 2);
        let mut foreground = Vec::with_capacity(scalar_partials.len() / 2);
        for group in scalar_partials.chunks(2) {
            background.push(group[0]);
            foreground.push(group[1]);
        }
        ForegroundBackground { background, foreground }
    }
}

/// Uploads `input` once and runs [`dispatch_reduce`], which computes both the
/// "background" and "foreground" result in a single dispatch.
async fn compute_reduce_fg_bg(
    gpu_context: &GpuContext<'_>,
    input: &NumericData,
    mode: ReduceMode,
    filtering_criteria: &[EmphasisCriteria],
    selection_criteria: &[EmphasisCriteria],
) -> ForegroundBackground<Vec<f32>> {
    let GpuContext { device, queue } = gpu_context;
    let (input_view, input_dtype) = input.create_data_texture(device, queue, "reduce_input");
    let total = input.len();

    dispatch_reduce(
        gpu_context, &input_view, input_dtype, total, &mode, filtering_criteria, selection_criteria,
    )
    .await
}

// CPU fallbacks
//
// Each reducer runs on the input at its native dtype and casts only the scalar
// *result* to f32 — the input array is never converted to f32 up front.
// `ScalarToF32` provides that single-value output cast for every supported
// element type, and `dispatch_cpu!` selects the matching `NumericData` arm.
//
// Every reducer additionally takes an `included: impl Fn(usize) -> bool`
// predicate so the same functions serve both the unfiltered case (predicate
// always `true`) and the filtered background/foreground passes (predicate
// backed by `cpu_is_included`).

/// Casts one scalar of a supported numeric dtype to f32. Applied only to
/// reduction outputs — never to convert the input array.
trait ScalarToF32: Copy {
    fn scalar_to_f32(self) -> f32;
}
macro_rules! impl_scalar_to_f32 {
    ($($t:ty),*) => { $(impl ScalarToF32 for $t {
        fn scalar_to_f32(self) -> f32 { self as f32 }
    })* };
}
impl_scalar_to_f32!(u8, u16, u32, u64, i8, i16, i32, i64, f32, f64);

/// Runs `$body` — which may reference the bound native slice `$v` — on whichever
/// dtype the `NumericData` holds, so the CPU reducers stay dtype-generic without
/// converting the input to f32.
macro_rules! dispatch_cpu {
    ($input:expr, |$v:ident| $body:expr) => {
        match $input {
            NumericData::Uint8($v) => $body,
            NumericData::Uint16($v) => $body,
            NumericData::Uint32($v) => $body,
            NumericData::Uint64($v) => $body,
            NumericData::Int8($v) => $body,
            NumericData::Int16($v) => $body,
            NumericData::Int32($v) => $body,
            NumericData::Int64($v) => $body,
            NumericData::Float32($v) => $body,
            NumericData::Float64($v) => $body,
        }
    };
}

fn cpu_reduce_min<T: ScalarToF32 + PartialOrd>(input: &[T], included: impl Fn(usize) -> bool) -> f32 {
    input
        .iter()
        .enumerate()
        .filter_map(|(i, &v)| included(i).then_some(v))
        .reduce(|a, b| if b < a { b } else { a })
        .map_or(f32::INFINITY, ScalarToF32::scalar_to_f32)
}

fn cpu_reduce_max<T: ScalarToF32 + PartialOrd>(input: &[T], included: impl Fn(usize) -> bool) -> f32 {
    input
        .iter()
        .enumerate()
        .filter_map(|(i, &v)| included(i).then_some(v))
        .reduce(|a, b| if b > a { b } else { a })
        .map_or(f32::NEG_INFINITY, ScalarToF32::scalar_to_f32)
}

fn cpu_reduce_sum<T: ScalarToF32 + std::iter::Sum>(input: &[T], included: impl Fn(usize) -> bool) -> f32 {
    // Accumulate in the native dtype; cast only the final sum.
    input
        .iter()
        .enumerate()
        .filter_map(|(i, &v)| included(i).then_some(v))
        .sum::<T>()
        .scalar_to_f32()
}

fn cpu_reduce_count<T>(input: &[T], included: impl Fn(usize) -> bool) -> f32 {
    (0..input.len()).filter(|&i| included(i)).count() as f32
}

fn cpu_reduce_mean<T: ScalarToF32>(input: &[T], included: impl Fn(usize) -> bool) -> f32 {
    let mut sum = 0.0f32;
    let mut count = 0u32;
    for (i, &v) in input.iter().enumerate() {
        if included(i) {
            sum += v.scalar_to_f32();
            count += 1;
        }
    }
    // Matches the GPU path's 0.0 / 0.0 (NaN) when nothing is included.
    sum / count as f32
}

fn cpu_reduce_extent<T: ScalarToF32 + PartialOrd>(input: &[T], included: impl Fn(usize) -> bool) -> (f32, f32) {
    let mut included_values = input.iter().enumerate().filter_map(|(i, &v)| included(i).then_some(v));
    match included_values.next() {
        None => (f32::INFINITY, f32::NEG_INFINITY),
        Some(first) => {
            let (mut lo, mut hi) = (first, first);
            for v in included_values {
                if v < lo { lo = v; }
                if v > hi { hi = v; }
            }
            (lo.scalar_to_f32(), hi.scalar_to_f32())
        }
    }
}

fn cpu_reduce_histogram<T: ScalarToF32>(
    input: &[T], num_bins: u32, data_min: f32, data_max: f32, included: impl Fn(usize) -> bool,
) -> Vec<u32> {
    let mut bins = vec![0u32; num_bins as usize];
    let range = data_max - data_min;
    for (i, &v) in input.iter().enumerate() {
        if !included(i) {
            continue;
        }
        let bin = if range <= 0.0 {
            0
        } else {
            // Bin edges are given in f32, so binning is inherently an f32
            // comparison; convert one scalar at a time (no up-front input cast).
            let t = (v.scalar_to_f32() - data_min) / range;
            (t * num_bins as f32).clamp(0.0, (num_bins - 1) as f32) as u32
        };
        bins[bin as usize] += 1;
    }
    bins
}

// ── Public wrapper functions ──────────────────────────────────────────────────
//
// When a GpuContext is provided, the GPU path is used (compute_reduce_fg_bg +
// CPU-side fold of partial workgroup results).  When None, a naive CPU
// fallback runs instead.
//
// Every reducer accepts `filtering_criteria` and `selection_criteria` (each
// AND-ed together, empty meaning "every item included") and always returns both
// components: `background` is computed over the filter-included set,
// `foreground` over the filter-*and*-selection-included subset. Passing `&[]`
// for both makes `background` and `foreground` identical, equal to the
// unfiltered reduction over the whole input.

/// Returns the minimum value in `input` (or `f32::INFINITY` if empty/nothing
/// included), for both the filter-included ("background") and
/// filter-and-selection-included ("foreground") subsets.
///
/// Accepts anything convertible into [`NumericData`] (e.g. an
/// `Arc<Vec<f32>>`), so any supported dtype is reduced without a CPU-side cast.
pub async fn reduce_min(
    gpu_context: Option<&GpuContext<'_>>,
    input: impl Into<NumericData>,
    filtering_criteria: &[EmphasisCriteria],
    selection_criteria: &[EmphasisCriteria],
) -> ForegroundBackground<f32> {
    let input = input.into();
    match gpu_context {
        Some(ctx) => {
            let partials =
                compute_reduce_fg_bg(ctx, &input, ReduceMode::Min, filtering_criteria, selection_criteria).await;
            ForegroundBackground {
                background: partials.background.into_iter().fold(f32::INFINITY, f32::min),
                foreground: partials.foreground.into_iter().fold(f32::INFINITY, f32::min),
            }
        }
        None => {
            let is_background = |i: usize| cpu_is_included(filtering_criteria, i);
            let background = dispatch_cpu!(&input, |v| cpu_reduce_min(v, is_background));
            let foreground = if selection_criteria.is_empty() {
                background
            } else {
                let is_foreground = |i: usize| is_background(i) && cpu_is_included(selection_criteria, i);
                dispatch_cpu!(&input, |v| cpu_reduce_min(v, is_foreground))
            };
            ForegroundBackground { background, foreground }
        }
    }
}

/// Returns the maximum value in `input` (or `f32::NEG_INFINITY` if
/// empty/nothing included), for both the "background" and "foreground"
/// subsets — see [`reduce_min`].
pub async fn reduce_max(
    gpu_context: Option<&GpuContext<'_>>,
    input: impl Into<NumericData>,
    filtering_criteria: &[EmphasisCriteria],
    selection_criteria: &[EmphasisCriteria],
) -> ForegroundBackground<f32> {
    let input = input.into();
    match gpu_context {
        Some(ctx) => {
            let partials =
                compute_reduce_fg_bg(ctx, &input, ReduceMode::Max, filtering_criteria, selection_criteria).await;
            ForegroundBackground {
                background: partials.background.into_iter().fold(f32::NEG_INFINITY, f32::max),
                foreground: partials.foreground.into_iter().fold(f32::NEG_INFINITY, f32::max),
            }
        }
        None => {
            let is_background = |i: usize| cpu_is_included(filtering_criteria, i);
            let background = dispatch_cpu!(&input, |v| cpu_reduce_max(v, is_background));
            let foreground = if selection_criteria.is_empty() {
                background
            } else {
                let is_foreground = |i: usize| is_background(i) && cpu_is_included(selection_criteria, i);
                dispatch_cpu!(&input, |v| cpu_reduce_max(v, is_foreground))
            };
            ForegroundBackground { background, foreground }
        }
    }
}

/// Returns the sum of all values in `input` (or `0.0` if empty/nothing
/// included), for both the "background" and "foreground" subsets — see
/// [`reduce_min`].
pub async fn reduce_sum(
    gpu_context: Option<&GpuContext<'_>>,
    input: impl Into<NumericData>,
    filtering_criteria: &[EmphasisCriteria],
    selection_criteria: &[EmphasisCriteria],
) -> ForegroundBackground<f32> {
    let input = input.into();
    match gpu_context {
        Some(ctx) => {
            let partials =
                compute_reduce_fg_bg(ctx, &input, ReduceMode::Sum, filtering_criteria, selection_criteria).await;
            ForegroundBackground {
                background: partials.background.into_iter().sum(),
                foreground: partials.foreground.into_iter().sum(),
            }
        }
        None => {
            let is_background = |i: usize| cpu_is_included(filtering_criteria, i);
            let background = dispatch_cpu!(&input, |v| cpu_reduce_sum(v, is_background));
            let foreground = if selection_criteria.is_empty() {
                background
            } else {
                let is_foreground = |i: usize| is_background(i) && cpu_is_included(selection_criteria, i);
                dispatch_cpu!(&input, |v| cpu_reduce_sum(v, is_foreground))
            };
            ForegroundBackground { background, foreground }
        }
    }
}

/// Returns the number of included elements in `input` (as an `f32`), for both
/// the "background" and "foreground" subsets — see [`reduce_min`].
pub async fn reduce_count(
    gpu_context: Option<&GpuContext<'_>>,
    input: impl Into<NumericData>,
    filtering_criteria: &[EmphasisCriteria],
    selection_criteria: &[EmphasisCriteria],
) -> ForegroundBackground<f32> {
    let input = input.into();
    match gpu_context {
        Some(ctx) => {
            let partials =
                compute_reduce_fg_bg(ctx, &input, ReduceMode::Count, filtering_criteria, selection_criteria).await;
            ForegroundBackground {
                background: partials.background.into_iter().sum(),
                foreground: partials.foreground.into_iter().sum(),
            }
        }
        None => {
            let is_background = |i: usize| cpu_is_included(filtering_criteria, i);
            let background = dispatch_cpu!(&input, |v| cpu_reduce_count(v, is_background));
            let foreground = if selection_criteria.is_empty() {
                background
            } else {
                let is_foreground = |i: usize| is_background(i) && cpu_is_included(selection_criteria, i);
                dispatch_cpu!(&input, |v| cpu_reduce_count(v, is_foreground))
            };
            ForegroundBackground { background, foreground }
        }
    }
}

/// Returns the mean of the included elements in `input` (or `NaN` if none are
/// included), for both the "background" and "foreground" subsets — see
/// [`reduce_min`].
pub async fn reduce_mean(
    gpu_context: Option<&GpuContext<'_>>,
    input: impl Into<NumericData>,
    filtering_criteria: &[EmphasisCriteria],
    selection_criteria: &[EmphasisCriteria],
) -> ForegroundBackground<f32> {
    let input = input.into();
    let fold_mean = |partials: Vec<f32>| {
        let sum: f32 = partials.iter().copied().step_by(2).sum();
        let count: f32 = partials.iter().copied().skip(1).step_by(2).sum();
        sum / count
    };
    match gpu_context {
        Some(ctx) => {
            let partials =
                compute_reduce_fg_bg(ctx, &input, ReduceMode::Mean, filtering_criteria, selection_criteria).await;
            ForegroundBackground {
                background: fold_mean(partials.background),
                foreground: fold_mean(partials.foreground),
            }
        }
        None => {
            let is_background = |i: usize| cpu_is_included(filtering_criteria, i);
            let background = dispatch_cpu!(&input, |v| cpu_reduce_mean(v, is_background));
            let foreground = if selection_criteria.is_empty() {
                background
            } else {
                let is_foreground = |i: usize| is_background(i) && cpu_is_included(selection_criteria, i);
                dispatch_cpu!(&input, |v| cpu_reduce_mean(v, is_foreground))
            };
            ForegroundBackground { background, foreground }
        }
    }
}

/// Returns `(min, max)` over `input` (or `(f32::INFINITY, f32::NEG_INFINITY)`
/// if empty/nothing included), for both the "background" and "foreground"
/// subsets — see [`reduce_min`].
pub async fn reduce_extent(
    gpu_context: Option<&GpuContext<'_>>,
    input: impl Into<NumericData>,
    filtering_criteria: &[EmphasisCriteria],
    selection_criteria: &[EmphasisCriteria],
) -> ForegroundBackground<(f32, f32)> {
    let input = input.into();
    let fold_extent = |partials: Vec<f32>| {
        let global_min = partials.iter().copied().step_by(2).fold(f32::INFINITY, f32::min);
        let global_max = partials.iter().copied().skip(1).step_by(2).fold(f32::NEG_INFINITY, f32::max);
        (global_min, global_max)
    };
    match gpu_context {
        Some(ctx) => {
            let partials =
                compute_reduce_fg_bg(ctx, &input, ReduceMode::Extent, filtering_criteria, selection_criteria).await;
            ForegroundBackground {
                background: fold_extent(partials.background),
                foreground: fold_extent(partials.foreground),
            }
        }
        None => {
            let is_background = |i: usize| cpu_is_included(filtering_criteria, i);
            let background = dispatch_cpu!(&input, |v| cpu_reduce_extent(v, is_background));
            let foreground = if selection_criteria.is_empty() {
                background
            } else {
                let is_foreground = |i: usize| is_background(i) && cpu_is_included(selection_criteria, i);
                dispatch_cpu!(&input, |v| cpu_reduce_extent(v, is_foreground))
            };
            ForegroundBackground { background, foreground }
        }
    }
}

/// Returns a histogram of `input` as `num_bins` bin counts, using a
/// caller-provided data range, for both the "background" and "foreground"
/// subsets — see [`reduce_min`].
///
/// Values are binned into `[data_min, data_max)`; out-of-range values are
/// clamped to the nearest edge bin.  `num_bins` must be ≤ 256.
pub async fn reduce_histogram_with_known_extent(
    gpu_context: Option<&GpuContext<'_>>,
    input: impl Into<NumericData>,
    num_bins: u32,
    data_min: f32,
    data_max: f32,
    filtering_criteria: &[EmphasisCriteria],
    selection_criteria: &[EmphasisCriteria],
) -> ForegroundBackground<Vec<u32>> {
    let input = input.into();
    let mode = ReduceMode::Histogram { num_bins, data_min, data_max };
    match gpu_context {
        Some(ctx) => {
            let partials = compute_reduce_fg_bg(ctx, &input, mode, filtering_criteria, selection_criteria).await;
            ForegroundBackground {
                background: bytemuck::cast_slice::<f32, u32>(&partials.background).to_vec(),
                foreground: bytemuck::cast_slice::<f32, u32>(&partials.foreground).to_vec(),
            }
        }
        None => {
            let is_background = |i: usize| cpu_is_included(filtering_criteria, i);
            let background =
                dispatch_cpu!(&input, |v| cpu_reduce_histogram(v, num_bins, data_min, data_max, is_background));
            let foreground = if selection_criteria.is_empty() {
                background.clone()
            } else {
                let is_foreground = |i: usize| is_background(i) && cpu_is_included(selection_criteria, i);
                dispatch_cpu!(&input, |v| cpu_reduce_histogram(v, num_bins, data_min, data_max, is_foreground))
            };
            ForegroundBackground { background, foreground }
        }
    }
}

/// Returns a histogram of `input` as `num_bins` bin counts, automatically
/// deriving the data range via `reduce_extent`, for both the "background" and
/// "foreground" subsets — see [`reduce_min`].
///
/// The bin edges are derived from the background (filter-included) set alone
/// — via `reduce_extent(.., filtering_criteria, &[])` — so the background and
/// foreground histograms share the same bin boundaries and stay comparable;
/// deriving the foreground's range from the (generally narrower) selected
/// subset would make the two histograms' bins incomparable.
///
/// This performs two GPU dispatches (or two CPU passes) per pass when the
/// extent is unknown: one for extent, one for the histogram.  `num_bins` must
/// be ≤ 256.
pub async fn reduce_histogram_with_unknown_extent(
    gpu_context: Option<&GpuContext<'_>>,
    input: impl Into<NumericData>,
    num_bins: u32,
    filtering_criteria: &[EmphasisCriteria],
    selection_criteria: &[EmphasisCriteria],
) -> ForegroundBackground<Vec<u32>> {
    let input = input.into();
    let extent = reduce_extent(gpu_context, input.clone(), filtering_criteria, &[]).await;
    reduce_histogram_with_known_extent(
        gpu_context,
        input,
        num_bins,
        extent.background.0,
        extent.background.1,
        filtering_criteria,
        selection_criteria,
    )
    .await
}
