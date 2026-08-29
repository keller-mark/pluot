//! Stratified reducer: like [`super::reduce`], but computes one result per
//! category stratum in addition to the usual "background" (filter-included)
//! vs. "foreground" (filter-*and*-selection-included) split.
//!
//! Stratification is orthogonal to filtering/selection: `stratify_by`'s
//! category column need not be (and often isn't) the same column used by
//! `filtering_criteria`/`selection_criteria`.

use std::collections::HashMap;

use crate::emphasis_mode::{cpu_is_included, prepare_emphasis_criteria};
use crate::numeric_data::NumericData;
use crate::render_traits::{CategoricalCriteriaParams, EmphasisCriteria};
use crate::render_types::GpuContext;
use crate::shader_modules::{common, ShaderBuilder, TextureDtype};
use crate::wgpu;
use std::num::NonZeroU64;

use encase::{ShaderType, UniformBuffer};

use super::reduce::{reduce_extent, ReduceMode, ReduceUniforms};
use super::ForegroundBackground;

/// The category column to stratify by, together with the ordered set of
/// strata to compute a result for.
///
/// Reuses the categories+codes shape of [`CategoricalCriteriaParams`]:
/// `codes` is a category code per data item, and `included_codes` is the
/// ordered list of codes to produce one output slot per (output index `i`
/// corresponds to `included_codes[i]`). An item whose code is not in
/// `included_codes` contributes to no stratum. An empty `included_codes`
/// means zero strata (empty output), distinct from an unstratified reduction.
pub type StratifyBy = CategoricalCriteriaParams;

/// Binding index of the first filtering criteria value texture. Chosen to
/// sit clear of every binding `main_scalar` (0..3) or `main_histogram`
/// (0, 1, 2, 4) uses. Selection criteria textures immediately follow the
/// filtering criteria textures.
const CRITERIA_FIRST_BINDING: u32 = 5;

// ── Order-preserving float<->u32 bit-key mapping ──────────────────────────────
//
// Rust-side mirror of `order_key`/`order_key_to_f32` in
// reduce_stratified.wgsl — used both to seed the GPU output buffer's Min/Max
// identity elements before the first dispatch, and to decode the buffer back
// into floats after the last one. See that shader's comment for why this
// lets atomicMin/atomicMax combine floats.

fn order_key(value: f32) -> u32 {
    let bits = value.to_bits();
    let mask = if bits & 0x8000_0000 != 0 { 0xFFFF_FFFF } else { 0x8000_0000 };
    bits ^ mask
}

fn order_key_to_f32(key: u32) -> f32 {
    let mask = if key & 0x8000_0000 != 0 { 0x8000_0000 } else { 0xFFFF_FFFF };
    f32::from_bits(key ^ mask)
}

/// The (slot 0, slot 1) identity values each half (background, foreground) of
/// the GPU output buffer's 4 slots per stratum must be seeded with before the
/// first chunk dispatches, so that atomicMin/atomicMax/atomicAdd-based
/// accumulation starts from the correct identity element for `mode`. Unused
/// by [`ReduceMode::Histogram`] (which uses a separate buffer whose identity
/// is always 0, left to WebGPU's implicit zero-initialization of newly
/// created buffers).
fn scalar_output_identity(mode: &ReduceMode) -> (u32, u32) {
    match mode {
        ReduceMode::Min => (order_key(f32::INFINITY), 0),
        ReduceMode::Max => (order_key(f32::NEG_INFINITY), 0),
        ReduceMode::Sum | ReduceMode::Count | ReduceMode::Mean => (0, 0),
        ReduceMode::Extent => (order_key(f32::INFINITY), order_key(f32::NEG_INFINITY)),
        ReduceMode::Histogram { .. } => (0, 0),
    }
}

/// Maps `download_buffer` for reading, copies its contents into a `Vec<u32>`,
/// then unmaps it. See `reduce.rs`'s `read_back_f32` (same shape, `u32`
/// instead of `f32` since every value this module reads back — bit-packed
/// floats, order-keys, or plain counts — is stored as a raw `u32`).
async fn read_back_u32(device: &wgpu::Device, download_buffer: &wgpu::Buffer) -> Vec<u32> {
    let buffer_slice = download_buffer.slice(..);

    #[cfg(target_arch = "wasm32")]
    {
        use futures::FutureExt;
        use futures_intrusive::channel::shared::oneshot_channel;
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

/// Dispatches the single-pass stratified reduction over `input_view`,
/// scattering into per-stratum background/foreground accumulators (or
/// per-stratum-per-side histograms) in one pass. `filtering_criteria` and
/// `selection_criteria` are each AND-ed together independently (empty means
/// every element passes), exactly like `reduce.rs`'s `dispatch_reduce`.
///
/// `total` is the element count of the (already-uploaded) `input_view` and
/// `stratify_view`, which must have the same length.
///
/// **Large inputs**: as in `reduce.rs`'s `dispatch_reduce`, the input is
/// processed in chunks so an arbitrarily large array can be reduced despite
/// per-dispatch and per-binding size limits. Every chunk here scatters
/// straight into the same persistent output buffer via atomics (no per-chunk
/// buffer or CPU-side fold needed) — one dispatch handles every stratum *and*
/// both background/foreground at once, so chunking is the only reason for
/// more than one dispatch.
///
/// **Return value layout**: `Min`/`Max`/`Sum`/`Count`/`Extent`/`Mean` return
/// 4 `u32` per stratum — `[bg_a, bg_b, fg_a, fg_b]` (see
/// `reduce_stratified.wgsl`'s header for what each slot holds per mode);
/// `Histogram` returns `2 * num_bins` counts per stratum — background bins
/// then foreground bins.
#[allow(clippy::too_many_arguments)]
async fn dispatch_reduce_stratified(
    gpu_context: &GpuContext<'_>,
    input_view: &wgpu::TextureView,
    input_dtype: TextureDtype,
    stratify_view: &wgpu::TextureView,
    stratify_dtype: TextureDtype,
    total: usize,
    mode: &ReduceMode,
    stratify_by: &StratifyBy,
    filtering_criteria: &[EmphasisCriteria],
    selection_criteria: &[EmphasisCriteria],
) -> Vec<u32> {
    let GpuContext { device, queue } = gpu_context;
    let is_histogram = mode.is_histogram();
    let num_strata = stratify_by.included_codes.len();

    // Two independent membership predicates, injected side by side — see
    // `reduce.rs`'s `dispatch_reduce` for the identical mechanism.
    let filtering = prepare_emphasis_criteria(
        device, queue, filtering_criteria, "is_filtered_in", "filter_data", CRITERIA_FIRST_BINDING,
    );
    let selection_first_binding = CRITERIA_FIRST_BINDING + filtering.textures.len() as u32;
    let selection = prepare_emphasis_criteria(
        device, queue, selection_criteria, "is_selected_in", "select_data", selection_first_binding,
    );

    // The requested strata's codes are baked into the shader at build time
    // (small, known ahead of time), exactly like the categorical criteria
    // membership test's `included_codes`.
    let strata_codes =
        stratify_by.included_codes.iter().map(i64::to_string).collect::<Vec<_>>().join(", ");
    let shader_source = ShaderBuilder::new(include_str!("shaders/reduce_stratified.wgsl"))
        .inject_texture_sample_type("input", input_dtype)
        .inject_texture_sample_type("stratify", stratify_dtype)
        .inject_function("flat_texel_coord", common::FLAT_TEXEL_COORD)
        .inject_function("filtering_wgsl", &filtering.wgsl)
        .inject_function("selection_wgsl", &selection.wgsl)
        .define_u32("num_strata", num_strata as u32)
        .define("strata_codes", &strata_codes)
        .build();
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("reduce_stratified.wgsl"),
        source: wgpu::ShaderSource::Wgsl(shader_source.into()),
    });

    let (num_bins, data_min, data_max) = match mode {
        ReduceMode::Histogram { num_bins, data_min, data_max } => (*num_bins, *data_min, *data_max),
        _ => (0, 0.0, 0.0),
    };

    let limits = device.limits();
    let max_elems_by_dispatch = (limits.max_compute_workgroups_per_dimension as usize).saturating_mul(64);
    let chunk_elements = max_elems_by_dispatch.max(64);

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

    // ── Bind group layout & pipeline ──────────────────────────────────────────
    //
    // main_scalar    uses bindings 0 (uniform), 1 (input), 2 (stratify), 3 (output)
    // main_histogram uses bindings 0 (uniform), 1 (input), 2 (stratify), 4 (output_hist)
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
    let stratify_entry = wgpu::BindGroupLayoutEntry {
        binding: 2,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Texture {
            sample_type: stratify_dtype.binding_sample_type(),
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    };
    let output_binding = if is_histogram { 4 } else { 3 };
    let output_size: u64 = if is_histogram {
        (num_strata as u64) * (num_bins as u64) * 2 * 4
    } else {
        (num_strata as u64) * 4 * 4
    };
    let output_entry = wgpu::BindGroupLayoutEntry {
        binding: output_binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: false },
            min_binding_size: Some(NonZeroU64::new(4).unwrap()),
            has_dynamic_offset: false,
        },
        count: None,
    };

    let mut bind_group_layout_entries = vec![uniform_entry, input_entry, stratify_entry, output_entry];
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

    // ── Persistent output buffer ───────────────────────────────────────────────
    //
    // Seeded with each mode's identity element(s) (duplicated into both the
    // background and foreground half of each stratum's slots), then
    // scattered into by every chunk's dispatch via atomics; read back exactly
    // once after the last chunk.

    let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("reduce_stratified_output"),
        size: output_size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    if !is_histogram {
        let (id_a, id_b) = scalar_output_identity(mode);
        let init: Vec<u32> = (0..num_strata).flat_map(|_| [id_a, id_b, id_a, id_b]).collect();
        queue.write_buffer(&output_buffer, 0, bytemuck::cast_slice(&init));
    }
    // output_hist's identity is always 0, so (like reduce.rs's hist_output) it
    // relies on WebGPU's implicit zero-initialization of new buffers.

    let mut offset = 0usize;
    while offset < total {
        let chunk_len = (total - offset).min(chunk_elements);
        let workgroup_count = chunk_len.div_ceil(64);

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
            label: Some("reduce_stratified_uniforms"),
            size: uniform_bytes.len() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&uniform_buffer, 0, &uniform_bytes);

        let mut bind_group_entries = vec![
            wgpu::BindGroupEntry { binding: 0, resource: uniform_buffer.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(input_view) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(stratify_view) },
            wgpu::BindGroupEntry { binding: output_binding, resource: output_buffer.as_entire_binding() },
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

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });
            compute_pass.set_pipeline(&pipeline);
            compute_pass.set_bind_group(0, &bind_group, &[]);
            compute_pass.dispatch_workgroups(workgroup_count as u32, 1, 1);
        }
        queue.submit([encoder.finish()]);

        offset += chunk_len;
    }

    let download_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("reduce_stratified_download"),
        size: output_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    encoder.copy_buffer_to_buffer(&output_buffer, 0, &download_buffer, 0, output_size);
    queue.submit([encoder.finish()]);
    read_back_u32(device, &download_buffer).await
}

/// Uploads `input`/`stratify_by.codes` once and runs
/// [`dispatch_reduce_stratified`], which computes every stratum's background
/// *and* foreground result in a single dispatch.
async fn compute_reduce_stratified(
    gpu_context: &GpuContext<'_>,
    input: &NumericData,
    stratify_by: &StratifyBy,
    mode: ReduceMode,
    filtering_criteria: &[EmphasisCriteria],
    selection_criteria: &[EmphasisCriteria],
) -> Vec<u32> {
    if stratify_by.included_codes.is_empty() {
        // Nothing to compute (and a zero-length array is not valid WGSL), so
        // skip touching the GPU entirely.
        return Vec::new();
    }

    let GpuContext { device, queue } = gpu_context;
    let (input_view, input_dtype) = input.create_data_texture(device, queue, "reduce_stratified_input");
    let (stratify_view, stratify_dtype) =
        stratify_by.codes.create_data_texture(device, queue, "reduce_stratified_codes");
    let total = input.len();

    dispatch_reduce_stratified(
        gpu_context, &input_view, input_dtype, &stratify_view, stratify_dtype, total, &mode, stratify_by,
        filtering_criteria, selection_criteria,
    )
    .await
}

// ── CPU fallback ───────────────────────────────────────────────────────────────

/// Runs one O(n) pass per background/foreground side over `input`, folding
/// every filter-*and*-stratum-included element's value into its stratum's
/// accumulator (starting from `identity`, combined via `accumulate`) —
/// covering every stratum in one scan rather than looping over strata and
/// re-scanning the input once per stratum.
fn cpu_stratified_fold<T: Clone>(
    input: &NumericData,
    stratify_by: &StratifyBy,
    filtering_criteria: &[EmphasisCriteria],
    selection_criteria: &[EmphasisCriteria],
    identity: T,
    mut accumulate: impl FnMut(&mut T, f32),
) -> ForegroundBackground<Vec<T>> {
    let code_to_stratum: HashMap<i64, usize> =
        stratify_by.included_codes.iter().enumerate().map(|(i, &code)| (code, i)).collect();
    let num_strata = stratify_by.included_codes.len();

    let mut fold_pass = |included: &dyn Fn(usize) -> bool| -> Vec<T> {
        let mut acc = vec![identity.clone(); num_strata];
        for i in 0..input.len() {
            if !included(i) {
                continue;
            }
            let code = stratify_by.codes.get_f32(i) as i64;
            if let Some(&s) = code_to_stratum.get(&code) {
                accumulate(&mut acc[s], input.get_f32(i));
            }
        }
        acc
    };

    let is_background = |i: usize| cpu_is_included(filtering_criteria, i);
    let background = fold_pass(&is_background);
    let foreground = if selection_criteria.is_empty() {
        background.clone()
    } else {
        let is_foreground = |i: usize| is_background(i) && cpu_is_included(selection_criteria, i);
        fold_pass(&is_foreground)
    };
    ForegroundBackground { background, foreground }
}

// ── Public wrapper functions ──────────────────────────────────────────────────

/// Per-stratum count of included elements (as `f32`) — see [`super::reduce::reduce_count`].
pub async fn reduce_stratified_count(
    gpu_context: Option<&GpuContext<'_>>,
    input: impl Into<NumericData>,
    stratify_by: &StratifyBy,
    filtering_criteria: &[EmphasisCriteria],
    selection_criteria: &[EmphasisCriteria],
) -> ForegroundBackground<Vec<f32>> {
    let input = input.into();
    match gpu_context {
        Some(ctx) => {
            let raw = compute_reduce_stratified(
                ctx, &input, stratify_by, ReduceMode::Count, filtering_criteria, selection_criteria,
            )
            .await;
            let background = raw.chunks(4).map(|c| c[0] as f32).collect();
            let foreground = raw.chunks(4).map(|c| c[2] as f32).collect();
            ForegroundBackground { background, foreground }
        }
        None => cpu_stratified_fold(
            &input, stratify_by, filtering_criteria, selection_criteria,
            0.0f32,
            |acc, _v| *acc += 1.0,
        ),
    }
}

/// Per-stratum minimum — see [`super::reduce::reduce_min`].
pub async fn reduce_stratified_min(
    gpu_context: Option<&GpuContext<'_>>,
    input: impl Into<NumericData>,
    stratify_by: &StratifyBy,
    filtering_criteria: &[EmphasisCriteria],
    selection_criteria: &[EmphasisCriteria],
) -> ForegroundBackground<Vec<f32>> {
    let input = input.into();
    match gpu_context {
        Some(ctx) => {
            let raw = compute_reduce_stratified(
                ctx, &input, stratify_by, ReduceMode::Min, filtering_criteria, selection_criteria,
            )
            .await;
            let background = raw.chunks(4).map(|c| order_key_to_f32(c[0])).collect();
            let foreground = raw.chunks(4).map(|c| order_key_to_f32(c[2])).collect();
            ForegroundBackground { background, foreground }
        }
        None => cpu_stratified_fold(
            &input, stratify_by, filtering_criteria, selection_criteria,
            f32::INFINITY,
            |acc, v| if v < *acc { *acc = v },
        ),
    }
}

/// Per-stratum maximum — see [`super::reduce::reduce_max`].
pub async fn reduce_stratified_max(
    gpu_context: Option<&GpuContext<'_>>,
    input: impl Into<NumericData>,
    stratify_by: &StratifyBy,
    filtering_criteria: &[EmphasisCriteria],
    selection_criteria: &[EmphasisCriteria],
) -> ForegroundBackground<Vec<f32>> {
    let input = input.into();
    match gpu_context {
        Some(ctx) => {
            let raw = compute_reduce_stratified(
                ctx, &input, stratify_by, ReduceMode::Max, filtering_criteria, selection_criteria,
            )
            .await;
            let background = raw.chunks(4).map(|c| order_key_to_f32(c[0])).collect();
            let foreground = raw.chunks(4).map(|c| order_key_to_f32(c[2])).collect();
            ForegroundBackground { background, foreground }
        }
        None => cpu_stratified_fold(
            &input, stratify_by, filtering_criteria, selection_criteria,
            f32::NEG_INFINITY,
            |acc, v| if v > *acc { *acc = v },
        ),
    }
}

/// Per-stratum sum — see [`super::reduce::reduce_sum`].
pub async fn reduce_stratified_sum(
    gpu_context: Option<&GpuContext<'_>>,
    input: impl Into<NumericData>,
    stratify_by: &StratifyBy,
    filtering_criteria: &[EmphasisCriteria],
    selection_criteria: &[EmphasisCriteria],
) -> ForegroundBackground<Vec<f32>> {
    let input = input.into();
    match gpu_context {
        Some(ctx) => {
            let raw = compute_reduce_stratified(
                ctx, &input, stratify_by, ReduceMode::Sum, filtering_criteria, selection_criteria,
            )
            .await;
            let background = raw.chunks(4).map(|c| f32::from_bits(c[0])).collect();
            let foreground = raw.chunks(4).map(|c| f32::from_bits(c[2])).collect();
            ForegroundBackground { background, foreground }
        }
        None => cpu_stratified_fold(
            &input, stratify_by, filtering_criteria, selection_criteria,
            0.0f32,
            |acc, v| *acc += v,
        ),
    }
}

/// Per-stratum mean — see [`super::reduce::reduce_mean`].
pub async fn reduce_stratified_mean(
    gpu_context: Option<&GpuContext<'_>>,
    input: impl Into<NumericData>,
    stratify_by: &StratifyBy,
    filtering_criteria: &[EmphasisCriteria],
    selection_criteria: &[EmphasisCriteria],
) -> ForegroundBackground<Vec<f32>> {
    let input = input.into();
    match gpu_context {
        Some(ctx) => {
            let raw = compute_reduce_stratified(
                ctx, &input, stratify_by, ReduceMode::Mean, filtering_criteria, selection_criteria,
            )
            .await;
            let background = raw.chunks(4).map(|c| f32::from_bits(c[0]) / (c[1] as f32)).collect();
            let foreground = raw.chunks(4).map(|c| f32::from_bits(c[2]) / (c[3] as f32)).collect();
            ForegroundBackground { background, foreground }
        }
        None => {
            let raw = cpu_stratified_fold(
                &input, stratify_by, filtering_criteria, selection_criteria,
                (0.0f32, 0.0f32),
                |acc, v| {
                    acc.0 += v;
                    acc.1 += 1.0;
                },
            );
            let decode = |acc: Vec<(f32, f32)>| acc.into_iter().map(|(sum, count)| sum / count).collect();
            ForegroundBackground { background: decode(raw.background), foreground: decode(raw.foreground) }
        }
    }
}

/// Per-stratum `(min, max)` — see [`super::reduce::reduce_extent`].
pub async fn reduce_stratified_extent(
    gpu_context: Option<&GpuContext<'_>>,
    input: impl Into<NumericData>,
    stratify_by: &StratifyBy,
    filtering_criteria: &[EmphasisCriteria],
    selection_criteria: &[EmphasisCriteria],
) -> ForegroundBackground<Vec<(f32, f32)>> {
    let input = input.into();
    match gpu_context {
        Some(ctx) => {
            let raw = compute_reduce_stratified(
                ctx, &input, stratify_by, ReduceMode::Extent, filtering_criteria, selection_criteria,
            )
            .await;
            let background =
                raw.chunks(4).map(|c| (order_key_to_f32(c[0]), order_key_to_f32(c[1]))).collect();
            let foreground =
                raw.chunks(4).map(|c| (order_key_to_f32(c[2]), order_key_to_f32(c[3]))).collect();
            ForegroundBackground { background, foreground }
        }
        None => cpu_stratified_fold(
            &input, stratify_by, filtering_criteria, selection_criteria,
            (f32::INFINITY, f32::NEG_INFINITY),
            |acc, v| {
                if v < acc.0 { acc.0 = v; }
                if v > acc.1 { acc.1 = v; }
            },
        ),
    }
}

/// Per-stratum histogram (`num_bins` bin counts each) using a caller-provided
/// data range — see [`super::reduce::reduce_histogram_with_known_extent`].
pub async fn reduce_stratified_histogram_with_known_extent(
    gpu_context: Option<&GpuContext<'_>>,
    input: impl Into<NumericData>,
    stratify_by: &StratifyBy,
    num_bins: u32,
    data_min: f32,
    data_max: f32,
    filtering_criteria: &[EmphasisCriteria],
    selection_criteria: &[EmphasisCriteria],
) -> ForegroundBackground<Vec<Vec<u32>>> {
    let input = input.into();
    let mode = ReduceMode::Histogram { num_bins, data_min, data_max };
    match gpu_context {
        Some(ctx) => {
            let raw =
                compute_reduce_stratified(ctx, &input, stratify_by, mode, filtering_criteria, selection_criteria)
                    .await;
            let stride = num_bins as usize * 2;
            let background = raw.chunks(stride).map(|c| c[..num_bins as usize].to_vec()).collect();
            let foreground = raw.chunks(stride).map(|c| c[num_bins as usize..].to_vec()).collect();
            ForegroundBackground { background, foreground }
        }
        None => {
            let range = data_max - data_min;
            cpu_stratified_fold(
                &input, stratify_by, filtering_criteria, selection_criteria,
                vec![0u32; num_bins as usize],
                move |acc, v| {
                    let bin = if range <= 0.0 {
                        0
                    } else {
                        let t = (v - data_min) / range;
                        (t * num_bins as f32).clamp(0.0, (num_bins - 1) as f32) as usize
                    };
                    acc[bin] += 1;
                },
            )
        }
    }
}

/// Per-stratum histogram, automatically deriving the data range from the
/// *unstratified* filter-included set — see
/// [`super::reduce::reduce_histogram_with_unknown_extent`].
///
/// The bin edges are derived once, from `reduce_extent(.., filtering_criteria,
/// &[])` over the whole (non-stratified) input, so every stratum's histogram —
/// and the overall background/foreground pair — share the same bin
/// boundaries and stay comparable across strata. Deriving each stratum's
/// range from its own (generally narrower) subset would make the per-stratum
/// histograms incomparable, the same reasoning
/// [`super::reduce::reduce_histogram_with_unknown_extent`] applies to its
/// own background/foreground pair.
pub async fn reduce_stratified_histogram_with_unknown_extent(
    gpu_context: Option<&GpuContext<'_>>,
    input: impl Into<NumericData>,
    stratify_by: &StratifyBy,
    num_bins: u32,
    filtering_criteria: &[EmphasisCriteria],
    selection_criteria: &[EmphasisCriteria],
) -> ForegroundBackground<Vec<Vec<u32>>> {
    let input = input.into();
    let extent = reduce_extent(gpu_context, input.clone(), filtering_criteria, &[]).await;
    reduce_stratified_histogram_with_known_extent(
        gpu_context,
        input,
        stratify_by,
        num_bins,
        extent.background.0,
        extent.background.1,
        filtering_criteria,
        selection_criteria,
    )
    .await
}
