use crate::{render_types::GpuContext, wgpu};
use crate::numeric_data::NumericData;
use crate::shader_modules::ShaderBuilder;
use std::num::NonZeroU64;
use futures::FutureExt;
use futures_intrusive::channel::shared::oneshot_channel;

use encase::{ShaderType, UniformBuffer};

// Reference: https://github.com/gfx-rs/wgpu/blob/trunk/examples/standalone/01_hello_compute/src/main.rs

// Mode enum

/// Discriminant values must stay in sync with the WGSL MODE_* constants in
/// shaders/reduce.wgsl.
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
}



impl ReduceMode {
    fn discriminant(&self) -> u32 {
        match self {
            ReduceMode::Min => 0,
            ReduceMode::Max => 1,
            ReduceMode::Sum => 2,
            ReduceMode::Extent => 3,
            ReduceMode::Histogram { .. } => 4,
        }
    }

    fn is_histogram(&self) -> bool {
        matches!(self, ReduceMode::Histogram { .. })
    }
}

// Uniform struct

/// Must match `ReduceUniforms` in shaders/reduce.wgsl exactly (field order
/// and types).  6 x 4 bytes = 24 bytes.
#[derive(ShaderType)]
struct ReduceUniforms {
    mode: u32,
    // Number of elements processed by the current dispatch (chunk length).
    num_elements: u32,
    num_bins: u32,
    data_min: f32,
    data_max: f32,
    // Flat index of the current chunk's first element within the input texture.
    base_offset: u32,
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

/// Dispatches a GPU reduction and returns raw partial results as `Vec<f32>`.
///
/// **Large inputs**
///
/// The input is processed in chunks so that an arbitrarily large array can be
/// reduced even though a single dispatch is bounded by
/// `max_compute_workgroups_per_dimension` (~65,535 or ~4M elements) and a
/// single storage binding is bounded by `max_storage_buffer_binding_size`
/// (default 128 MiB or ~33M elements).  Each chunk is dispatched separately and
/// the partial results are combined as described below.
///
/// **Return value layout**
/// - `Min / Max / Sum`:  one `f32` per workgroup, concatenated across all
///                       chunks; caller folds them into the final scalar.
/// - `Extent`:           two `f32` per workgroup: `[partial_min, partial_max]`
///                       interleaved, concatenated across chunks; caller folds
///                       each component separately.
/// - `Histogram`:        `num_bins` values whose bytes are actually `u32` bin
///                       counts, accumulated across all chunks via the shader's
///                       global atomics; use `bytemuck::cast_slice` to recover
///                       the `u32`s.
pub async fn compute_reduce(
    gpu_context: &GpuContext<'_>,
    input: &NumericData,
    mode: ReduceMode,
) -> Vec<f32> {
    let GpuContext { device, queue } = gpu_context;
    let is_histogram = mode.is_histogram();
    let is_extent = matches!(mode, ReduceMode::Extent);

    // Upload the input once as a single-channel 2D texture in its native dtype
    // (8/16/32-bit are zero-copy; only 64-bit is narrowed). The shader reads
    // each element via `f32(textureLoad(...))`, so there is no CPU-side cast.
    let (input_view, input_dtype) = input.create_data_texture(device, queue, "reduce_input");

    // Assemble the shader with the texture's sampled type injected at runtime.
    let shader_source = ShaderBuilder::new(include_str!("shaders/reduce.wgsl"))
        .inject_texture_sample_type("input", input_dtype)
        .build();
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("reduce.wgsl"),
        source: wgpu::ShaderSource::Wgsl(shader_source.into()),
    });

    let (num_bins, data_min, data_max) = match &mode {
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

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[uniform_entry, input_entry, output_entry],
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

    let hist_output = if is_histogram {
        Some(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("reduce_histogram_output"),
            size: (num_bins as u64) * 4,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        }))
    } else {
        None
    };

    // ── Process each chunk ────────────────────────────────────────────────────

    let total = input.len();
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
        // histogram.
        //
        //   Min / Max / Sum  --> one f32 per workgroup (partial result)
        //   Extent           --> two f32 per workgroup ([partial_min, partial_max])

        let scalar_output_bytes: u64 = if is_extent {
            (workgroup_count as u64) * 2 * 4
        } else {
            (workgroup_count as u64) * 4
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

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&input_view),
                },
                wgpu::BindGroupEntry {
                    binding: if is_histogram { 3 } else { 2 },
                    resource: output_binding_resource,
                },
            ],
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

    // ── Read back the final result ────────────────────────────────────────────

    if let Some(hist_output) = hist_output {
        let size = (num_bins as u64) * 4;
        let download_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("reduce_download"),
            size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        encoder.copy_buffer_to_buffer(&hist_output, 0, &download_buffer, 0, size);
        queue.submit([encoder.finish()]);
        read_back_f32(device, &download_buffer).await
    } else {
        scalar_partials
    }
}

// CPU fallbacks
//
// Each reducer runs on the input at its native dtype and casts only the scalar
// *result* to f32 — the input array is never converted to f32 up front.
// `ScalarToF32` provides that single-value output cast for every supported
// element type, and `dispatch_cpu!` selects the matching `NumericData` arm.

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

fn cpu_reduce_min<T: ScalarToF32 + PartialOrd>(input: &[T]) -> f32 {
    input
        .iter()
        .copied()
        .reduce(|a, b| if b < a { b } else { a })
        .map_or(f32::INFINITY, ScalarToF32::scalar_to_f32)
}

fn cpu_reduce_max<T: ScalarToF32 + PartialOrd>(input: &[T]) -> f32 {
    input
        .iter()
        .copied()
        .reduce(|a, b| if b > a { b } else { a })
        .map_or(f32::NEG_INFINITY, ScalarToF32::scalar_to_f32)
}

fn cpu_reduce_sum<T: ScalarToF32 + std::iter::Sum>(input: &[T]) -> f32 {
    // Accumulate in the native dtype; cast only the final sum.
    input.iter().copied().sum::<T>().scalar_to_f32()
}

fn cpu_reduce_extent<T: ScalarToF32 + PartialOrd>(input: &[T]) -> (f32, f32) {
    match input.split_first() {
        None => (f32::INFINITY, f32::NEG_INFINITY),
        Some((&first, rest)) => {
            let (mut lo, mut hi) = (first, first);
            for &v in rest {
                if v < lo { lo = v; }
                if v > hi { hi = v; }
            }
            (lo.scalar_to_f32(), hi.scalar_to_f32())
        }
    }
}

fn cpu_reduce_histogram<T: ScalarToF32>(input: &[T], num_bins: u32, data_min: f32, data_max: f32) -> Vec<u32> {
    let mut bins = vec![0u32; num_bins as usize];
    let range = data_max - data_min;
    for &v in input {
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
// When a GpuContext is provided, the GPU path is used (compute_reduce +
// CPU-side fold of partial workgroup results).  When None, a naive CPU
// fallback runs instead.

/// Returns the minimum value in `input`, or `f32::INFINITY` if empty.
///
/// Accepts anything convertible into [`NumericData`] (e.g. an
/// `Arc<Vec<f32>>`), so any supported dtype is reduced without a CPU-side cast.
pub async fn reduce_min(gpu_context: Option<&GpuContext<'_>>, input: impl Into<NumericData>) -> f32 {
    let input = input.into();
    match gpu_context {
        Some(ctx) => {
            let partials = compute_reduce(ctx, &input, ReduceMode::Min).await;
            partials.into_iter().fold(f32::INFINITY, f32::min)
        }
        None => dispatch_cpu!(&input, |v| cpu_reduce_min(v)),
    }
}

/// Returns the maximum value in `input`, or `f32::NEG_INFINITY` if empty.
pub async fn reduce_max(gpu_context: Option<&GpuContext<'_>>, input: impl Into<NumericData>) -> f32 {
    let input = input.into();
    match gpu_context {
        Some(ctx) => {
            let partials = compute_reduce(ctx, &input, ReduceMode::Max).await;
            partials.into_iter().fold(f32::NEG_INFINITY, f32::max)
        }
        None => dispatch_cpu!(&input, |v| cpu_reduce_max(v)),
    }
}

/// Returns the sum of all values in `input`, or `0.0` if empty.
pub async fn reduce_sum(gpu_context: Option<&GpuContext<'_>>, input: impl Into<NumericData>) -> f32 {
    let input = input.into();
    match gpu_context {
        Some(ctx) => {
            let partials = compute_reduce(ctx, &input, ReduceMode::Sum).await;
            partials.into_iter().sum()
        }
        None => dispatch_cpu!(&input, |v| cpu_reduce_sum(v)),
    }
}

/// Returns `(min, max)` over `input`, or `(f32::INFINITY, f32::NEG_INFINITY)` if empty.
pub async fn reduce_extent(
    gpu_context: Option<&GpuContext<'_>>,
    input: impl Into<NumericData>,
) -> (f32, f32) {
    let input = input.into();
    match gpu_context {
        Some(ctx) => {
            let partials = compute_reduce(ctx, &input, ReduceMode::Extent).await;
            let global_min = partials.iter().copied().step_by(2).fold(f32::INFINITY, f32::min);
            let global_max = partials.iter().copied().skip(1).step_by(2).fold(f32::NEG_INFINITY, f32::max);
            (global_min, global_max)
        }
        None => dispatch_cpu!(&input, |v| cpu_reduce_extent(v)),
    }
}

/// Returns a histogram of `input_arr` as `num_bins` bin counts, using a
/// caller-provided data range.
///
/// Values are binned into `[data_min, data_max)`; out-of-range values are
/// clamped to the nearest edge bin.  `num_bins` must be ≤ 256.
pub async fn reduce_histogram_with_known_extent(
    gpu_context: Option<&GpuContext<'_>>,
    input: impl Into<NumericData>,
    num_bins: u32,
    data_min: f32,
    data_max: f32,
) -> Vec<u32> {
    let input = input.into();
    match gpu_context {
        Some(ctx) => {
            let mode = ReduceMode::Histogram { num_bins, data_min, data_max };
            let raw = compute_reduce(ctx, &input, mode).await;
            bytemuck::cast_slice::<f32, u32>(&raw).to_vec()
        }
        None => dispatch_cpu!(&input, |v| cpu_reduce_histogram(v, num_bins, data_min, data_max)),
    }
}

/// Returns a histogram of `input_arr` as `num_bins` bin counts, automatically
/// deriving the data range via `reduce_extent`.
///
/// This performs two GPU dispatches (or two CPU passes) when the extent is
/// unknown: one for extent, one for the histogram.  `num_bins` must be ≤ 256.
pub async fn reduce_histogram_with_unknown_extent(
    gpu_context: Option<&GpuContext<'_>>,
    input: impl Into<NumericData>,
    num_bins: u32,
) -> Vec<u32> {
    let input = input.into();
    let (data_min, data_max) = reduce_extent(gpu_context, input.clone()).await;
    reduce_histogram_with_known_extent(gpu_context, input, num_bins, data_min, data_max).await
}
