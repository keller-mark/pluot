use crate::{render_types::GpuContext, wgpu};
use crate::wgpu::util::DeviceExt;
use std::num::NonZeroU64;
use std::sync::Arc;
use futures_intrusive::channel::shared::oneshot_channel;

use encase::{ShaderType, UniformBuffer};

// Reference: https://github.com/gfx-rs/wgpu/blob/trunk/examples/standalone/01_hello_compute/src/main.rs

// ── Mode enum ────────────────────────────────────────────────────────────────

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
    /// Output: (f32, f32) — (global minimum, global maximum).
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

// ── Uniform struct ────────────────────────────────────────────────────────────

/// Must match `ReduceUniforms` in shaders/reduce.wgsl exactly (field order
/// and types).  5 × 4 bytes = 20 bytes.
#[derive(ShaderType)]
struct ReduceUniforms {
    mode: u32,
    // TODO: can the shader just obtain num_elements from the length of the input array?
    num_elements: u32,
    num_bins: u32,
    data_min: f32,
    data_max: f32,
}

// ── Core dispatch function ────────────────────────────────────────────────────

/// Dispatches a GPU reduction and returns raw partial results as `Vec<f32>`.
///
/// **Return value layout**
/// - `Min / Max / Sum`:  one `f32` per workgroup; caller folds into final scalar.
/// - `Extent`:           two `f32` per workgroup — `[partial_min, partial_max]`
///                       interleaved; caller folds each separately.
/// - `Histogram`:        `num_bins` values whose bytes are actually `u32` bin
///                       counts; use `bytemuck::cast_slice` to recover `u32`s.
pub async fn compute_reduce(
    gpu_context: &GpuContext<'_>,
    input_arr: Arc<Vec<f32>>,
    mode: ReduceMode,
) -> Vec<f32> {
    let GpuContext { device, queue } = gpu_context;
    let is_histogram = mode.is_histogram();
    let workgroup_count = input_arr.len().div_ceil(64);

    let shader = device.create_shader_module(wgpu::include_wgsl!("shaders/reduce.wgsl"));

    // ── Uniforms ──────────────────────────────────────────────────────────────

    let (num_bins, data_min, data_max) = match &mode {
        ReduceMode::Histogram { num_bins, data_min, data_max } => (*num_bins, *data_min, *data_max),
        _ => (0, 0.0, 0.0),
    };

    let uniforms = ReduceUniforms {
        mode: mode.discriminant(),
        num_elements: input_arr.len() as u32,
        num_bins,
        data_min,
        data_max,
    };

    let mut buffer = UniformBuffer::new(Vec::<u8>::new());
    buffer.write(&uniforms).unwrap();
    let uniform_bytes = buffer.into_inner();

    let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("reduce_uniforms"),
        size: uniform_bytes.len() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&uniform_buffer, 0, &uniform_bytes);

    // ── Input buffer ──────────────────────────────────────────────────────────

    let input_data_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("reduce_input"),
        contents: bytemuck::cast_slice(&input_arr),
        usage: wgpu::BufferUsages::STORAGE,
    });

    // ── Output buffer ─────────────────────────────────────────────────────────
    //
    // Size depends on mode:
    //   Min / Max / Sum  → one f32  per workgroup (partial result)
    //   Extent           → two f32  per workgroup ([partial_min, partial_max])
    //   Histogram        → num_bins u32 values    (globally accumulated counts)
    //
    // WebGPU zero-initialises all newly created buffers, so the histogram
    // output starts at zero without an explicit clear pass.

    let output_size_bytes: u64 = if is_histogram {
        (num_bins as u64) * 4
    } else if matches!(mode, ReduceMode::Extent) {
        (workgroup_count as u64) * 2 * 4
    } else {
        (workgroup_count as u64) * 4
    };

    let output_data_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("reduce_output"),
        size: output_size_bytes,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });

    let download_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("reduce_download"),
        size: output_size_bytes,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    // ── Bind group layout ─────────────────────────────────────────────────────
    //
    // main_scalar    uses bindings 0 (uniform), 1 (input), 2 (output f32)
    // main_histogram uses bindings 0 (uniform), 1 (input), 3 (output atomic<u32>)

    let uniform_entry = wgpu::BindGroupLayoutEntry {
        binding: 0,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            min_binding_size: NonZeroU64::new(uniform_bytes.len() as u64),
            has_dynamic_offset: false,
        },
        count: None,
    };
    let input_entry = wgpu::BindGroupLayoutEntry {
        binding: 1,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            min_binding_size: Some(NonZeroU64::new(4).unwrap()),
            has_dynamic_offset: false,
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
                resource: input_data_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: if is_histogram { 3 } else { 2 },
                resource: output_data_buffer.as_entire_binding(),
            },
        ],
    });

    // ── Pipeline ──────────────────────────────────────────────────────────────

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

    // ── Dispatch ──────────────────────────────────────────────────────────────

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

    encoder.copy_buffer_to_buffer(&output_data_buffer, 0, &download_buffer, 0, output_size_bytes);

    queue.submit([encoder.finish()]);

    // ── Read back ─────────────────────────────────────────────────────────────

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

    let data = buffer_slice.get_mapped_range();
    bytemuck::allocation::pod_collect_to_vec(&data)
}

// ── CPU fallbacks ─────────────────────────────────────────────────────────────

fn cpu_reduce_min(input: &[f32]) -> f32 {
    input.iter().copied().fold(f32::INFINITY, f32::min)
}

fn cpu_reduce_max(input: &[f32]) -> f32 {
    input.iter().copied().fold(f32::NEG_INFINITY, f32::max)
}

fn cpu_reduce_sum(input: &[f32]) -> f32 {
    input.iter().copied().sum()
}

fn cpu_reduce_extent(input: &[f32]) -> (f32, f32) {
    input.iter().copied().fold(
        (f32::INFINITY, f32::NEG_INFINITY),
        |(lo, hi), v| (f32::min(lo, v), f32::max(hi, v)),
    )
}

fn cpu_reduce_histogram(input: &[f32], num_bins: u32, data_min: f32, data_max: f32) -> Vec<u32> {
    let mut bins = vec![0u32; num_bins as usize];
    let range = data_max - data_min;
    for &v in input {
        let bin = if range <= 0.0 {
            0
        } else {
            let t = (v - data_min) / range;
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

/// Returns the minimum value in `input_arr`, or `f32::INFINITY` if empty.
pub async fn reduce_min(gpu_context: Option<&GpuContext<'_>>, input_arr: Arc<Vec<f32>>) -> f32 {
    match gpu_context {
        Some(ctx) => {
            let partials = compute_reduce(ctx, input_arr, ReduceMode::Min).await;
            partials.into_iter().fold(f32::INFINITY, f32::min)
        }
        None => cpu_reduce_min(&input_arr),
    }
}

/// Returns the maximum value in `input_arr`, or `f32::NEG_INFINITY` if empty.
pub async fn reduce_max(gpu_context: Option<&GpuContext<'_>>, input_arr: Arc<Vec<f32>>) -> f32 {
    match gpu_context {
        Some(ctx) => {
            let partials = compute_reduce(ctx, input_arr, ReduceMode::Max).await;
            partials.into_iter().fold(f32::NEG_INFINITY, f32::max)
        }
        None => cpu_reduce_max(&input_arr),
    }
}

/// Returns the sum of all values in `input_arr`, or `0.0` if empty.
pub async fn reduce_sum(gpu_context: Option<&GpuContext<'_>>, input_arr: Arc<Vec<f32>>) -> f32 {
    match gpu_context {
        Some(ctx) => {
            let partials = compute_reduce(ctx, input_arr, ReduceMode::Sum).await;
            partials.into_iter().sum()
        }
        None => cpu_reduce_sum(&input_arr),
    }
}

/// Returns `(min, max)` over `input_arr`, or `(f32::INFINITY, f32::NEG_INFINITY)` if empty.
pub async fn reduce_extent(
    gpu_context: Option<&GpuContext<'_>>,
    input_arr: Arc<Vec<f32>>,
) -> (f32, f32) {
    match gpu_context {
        Some(ctx) => {
            let partials = compute_reduce(ctx, input_arr, ReduceMode::Extent).await;
            let global_min = partials.iter().copied().step_by(2).fold(f32::INFINITY, f32::min);
            let global_max = partials.iter().copied().skip(1).step_by(2).fold(f32::NEG_INFINITY, f32::max);
            (global_min, global_max)
        }
        None => cpu_reduce_extent(&input_arr),
    }
}

/// Returns a histogram of `input_arr` as `num_bins` bin counts, using a
/// caller-provided data range.
///
/// Values are binned into `[data_min, data_max)`; out-of-range values are
/// clamped to the nearest edge bin.  `num_bins` must be ≤ 256.
pub async fn reduce_histogram_with_known_extent(
    gpu_context: Option<&GpuContext<'_>>,
    input_arr: Arc<Vec<f32>>,
    num_bins: u32,
    data_min: f32,
    data_max: f32,
) -> Vec<u32> {
    match gpu_context {
        Some(ctx) => {
            let mode = ReduceMode::Histogram { num_bins, data_min, data_max };
            let raw = compute_reduce(ctx, input_arr, mode).await;
            bytemuck::cast_slice::<f32, u32>(&raw).to_vec()
        }
        None => cpu_reduce_histogram(&input_arr, num_bins, data_min, data_max),
    }
}

/// Returns a histogram of `input_arr` as `num_bins` bin counts, automatically
/// deriving the data range via `reduce_extent`.
///
/// This performs two GPU dispatches (or two CPU passes) when the extent is
/// unknown: one for extent, one for the histogram.  `num_bins` must be ≤ 256.
pub async fn reduce_histogram_with_unknown_extent(
    gpu_context: Option<&GpuContext<'_>>,
    input_arr: Arc<Vec<f32>>,
    num_bins: u32,
) -> Vec<u32> {
    let (data_min, data_max) = reduce_extent(gpu_context, Arc::clone(&input_arr)).await;
    reduce_histogram_with_known_extent(gpu_context, input_arr, num_bins, data_min, data_max).await
}
