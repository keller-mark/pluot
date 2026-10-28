use std::num::NonZeroU64;

use encase::{ShaderType, UniformBuffer};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::numeric_data::NumericData;
use crate::render_types::GpuContext;
use crate::shader_modules::{common, matrix_value, ShaderBuilder, TextureDtype};
use crate::sparse_matrix::{CompressedAxis, CompressedMatrix};
use crate::wgpu;

use super::histogram_percentile::histogram_bin;
use super::reduce::ScalarToF32;
use super::reduce_stratified::read_back_u32;
use crate::gpu_send::MaybeSendBoxFuture;

/// An axis of a row-major matrix.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, TS)]
pub enum MatrixAxis {
    Rows,
    Cols,
}

#[derive(ShaderType)]
struct MatrixAxisExtentsUniforms {
    num_rows: u32,
    num_cols: u32,
    axis: u32,
    num_segments: u32,
    base_segment: u32,
    num_bins: u32,
}

/// What [`gpu_axis_reduce`] computes for each row or column.
enum AxisReduction<'a> {
    Extents,
    /// `num_bins` counts per segment, each binned within that segment's `(lo, hi)` range.
    Histograms { ranges: &'a [(f32, f32)], num_bins: u32 },
}

impl AxisReduction<'_> {
    fn words_per_segment(&self) -> usize {
        match self {
            AxisReduction::Extents => 2,
            AxisReduction::Histograms { num_bins, .. } => *num_bins as usize,
        }
    }
}

/// The `(mins, maxs)` of every row (or every column) of the row-major
/// `num_rows x num_cols` `matrix`, skipping NaN values. A row or column with
/// nothing but NaN values has an extent of `(f32::INFINITY, f32::NEG_INFINITY)`.
pub fn reduce_matrix_axis_extents<'a>(
    gpu_context: Option<&'a GpuContext<'_>>,
    matrix: &'a NumericData,
    num_rows: usize,
    num_cols: usize,
    axis: MatrixAxis,
) -> MaybeSendBoxFuture<'a, (Vec<f32>, Vec<f32>)> {
    Box::pin(async move {
        assert_eq!(matrix.len(), num_rows * num_cols, "matrix length must be num_rows * num_cols");
        let num_segments = if axis == MatrixAxis::Rows { num_rows } else { num_cols };
        if num_segments == 0 || matrix.is_empty() {
            return (vec![f32::INFINITY; num_segments], vec![f32::NEG_INFINITY; num_segments]);
        }
        match gpu_context {
            Some(ctx) => {
                let (view, dtype) = matrix.create_data_texture(ctx.device, ctx.queue, "matrix_axis_extents_input");
                let matrix_value_wgsl = dense_matrix_value_wgsl(dtype);
                let words = gpu_axis_reduce(ctx, &matrix_value_wgsl, &[(view, dtype)], num_rows, num_cols, axis, num_segments, AxisReduction::Extents).await;
                decode_gpu_extents(&words)
            }
            None => crate::with_numeric_slice!(matrix, |values| cpu_axis_extents(values, num_cols, axis, num_segments)),
        }
    })
}

fn dense_matrix_value_wgsl(dtype: TextureDtype) -> String {
    ShaderBuilder::new(matrix_value::DENSE)
        .define_bidx("matrix_data", MATRIX_VALUE_FIRST_BINDING)
        .inject_texture_sample_type("matrix_data", dtype)
        .define("num_cols", "uniforms.num_cols")
        .build()
}

/// Like [`reduce_matrix_axis_extents`], for a `num_rows x num_cols` sparse
/// `matrix` compressed along `compressed_axis`, whose implicit zeros count
/// towards the extents.
pub fn reduce_compressed_matrix_axis_extents<'a>(
    gpu_context: Option<&'a GpuContext<'_>>,
    matrix: &'a CompressedMatrix,
    compressed_axis: CompressedAxis,
    num_rows: usize,
    num_cols: usize,
    axis: MatrixAxis,
) -> MaybeSendBoxFuture<'a, (Vec<f32>, Vec<f32>)> {
    Box::pin(async move {
        matrix.validate(compressed_axis, num_rows, num_cols);
        let num_segments = if axis == MatrixAxis::Rows { num_rows } else { num_cols };
        if num_segments == 0 || num_rows * num_cols == 0 {
            return (vec![f32::INFINITY; num_segments], vec![f32::NEG_INFINITY; num_segments]);
        }
        match gpu_context {
            Some(ctx) => {
                let prepared = matrix.prepare_gpu(ctx.device, ctx.queue, compressed_axis, MATRIX_VALUE_FIRST_BINDING);
                let words = gpu_axis_reduce(ctx, &prepared.wgsl, &prepared.textures, num_rows, num_cols, axis, num_segments, AxisReduction::Extents).await;
                decode_gpu_extents(&words)
            }
            None => {
                let num_other = if axis == MatrixAxis::Rows { num_cols } else { num_rows };
                crate::with_numeric_slice!(&matrix.data, |values| cpu_compressed_axis_extents(matrix, values, compressed_axis, axis, num_segments, num_other))
            }
        }
    })
}

/// Calls `visit(segment, entry)` for each stored entry within the matrix (or its minor subset).
fn for_each_compressed_entry(matrix: &CompressedMatrix, compressed_axis: CompressedAxis, axis: MatrixAxis, mut visit: impl FnMut(usize, usize)) {
    let segments_are_major = (axis == MatrixAxis::Rows) == (compressed_axis == CompressedAxis::Rows);
    let minor_positions: Option<Vec<Option<usize>>> = matrix.minor_subset.as_ref().map(|subset| {
        let max_index = (0..subset.len()).map(|position| subset.get_f64(position) as usize).max().unwrap_or(0);
        let mut positions = vec![None; max_index + 1];
        for position in 0..subset.len() {
            positions[subset.get_f64(position) as usize] = Some(position);
        }
        positions
    });
    for major in 0..matrix.num_major() {
        for entry in matrix.indptr.get_f64(major) as usize..matrix.indptr.get_f64(major + 1) as usize {
            let minor_index = matrix.indices.get_f64(entry) as usize;
            let minor_position = match &minor_positions {
                Some(positions) => match positions.get(minor_index).copied().flatten() {
                    Some(position) => position,
                    None => continue,
                },
                None => minor_index,
            };
            visit(if segments_are_major { major } else { minor_position }, entry);
        }
    }
}

/// Visits each stored entry once, so unlike the GPU path, whose threads each
/// binary search every cell of their row or column, this is O(entries).
fn cpu_compressed_axis_extents<T: ScalarToF32 + PartialOrd>(
    matrix: &CompressedMatrix,
    values: &[T],
    compressed_axis: CompressedAxis,
    axis: MatrixAxis,
    num_segments: usize,
    num_other: usize,
) -> (Vec<f32>, Vec<f32>) {
    let mut extents: Vec<Option<(f32, f32)>> = vec![None; num_segments];
    let mut num_stored = vec![0usize; num_segments];
    for_each_compressed_entry(matrix, compressed_axis, axis, |segment, entry| {
        num_stored[segment] += 1;
        let value = values[entry];
        #[allow(clippy::eq_op)]
        if value != value {
            return;
        }
        let value = value.scalar_to_f32();
        let extent = extents[segment].get_or_insert((value, value));
        extent.0 = extent.0.min(value);
        extent.1 = extent.1.max(value);
    });
    extents
        .into_iter()
        .zip(num_stored)
        .map(|(extent, num_stored)| {
            let extent = if num_stored < num_other {
                Some(extent.map_or((0.0, 0.0), |(lo, hi)| (lo.min(0.0), hi.max(0.0))))
            } else {
                extent
            };
            extent.unwrap_or((f32::INFINITY, f32::NEG_INFINITY))
        })
        .unzip()
}

fn cpu_axis_extents<T: ScalarToF32 + PartialOrd>(values: &[T], num_cols: usize, axis: MatrixAxis, num_segments: usize) -> (Vec<f32>, Vec<f32>) {
    let mut extents: Vec<Option<(T, T)>> = vec![None; num_segments];
    for (idx, &value) in values.iter().enumerate() {
        #[allow(clippy::eq_op)]
        if value != value {
            continue;
        }
        let segment = if axis == MatrixAxis::Rows { idx / num_cols } else { idx % num_cols };
        let extent = extents[segment].get_or_insert((value, value));
        if value < extent.0 {
            extent.0 = value;
        }
        if value > extent.1 {
            extent.1 = value;
        }
    }
    extents
        .into_iter()
        .map(|extent| extent.map_or((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi)| (lo.scalar_to_f32(), hi.scalar_to_f32())))
        .unzip()
}

/// `num_bins` counts per row (or per column) of the row-major
/// `num_rows x num_cols` `matrix`, binning each row's (or column's) values
/// within its own `ranges` entry and skipping NaN values. Values outside of
/// a range are clamped into its edge bins, as in
/// [`super::reduce::reduce_histogram_with_known_extent`].
pub fn reduce_matrix_axis_histograms<'a>(
    gpu_context: Option<&'a GpuContext<'_>>,
    matrix: &'a NumericData,
    num_rows: usize,
    num_cols: usize,
    axis: MatrixAxis,
    ranges: &'a [(f32, f32)],
    num_bins: u32,
) -> MaybeSendBoxFuture<'a, Vec<u32>> {
    Box::pin(async move {
        assert_eq!(matrix.len(), num_rows * num_cols, "matrix length must be num_rows * num_cols");
        let num_segments = if axis == MatrixAxis::Rows { num_rows } else { num_cols };
        assert_eq!(ranges.len(), num_segments, "one range per segment is required");
        if num_segments == 0 || matrix.is_empty() || num_bins == 0 {
            return vec![0; num_segments * num_bins as usize];
        }
        match gpu_context {
            Some(ctx) => {
                let (view, dtype) = matrix.create_data_texture(ctx.device, ctx.queue, "matrix_axis_histograms_input");
                let matrix_value_wgsl = dense_matrix_value_wgsl(dtype);
                let reduction = AxisReduction::Histograms { ranges, num_bins };
                gpu_axis_reduce(ctx, &matrix_value_wgsl, &[(view, dtype)], num_rows, num_cols, axis, num_segments, reduction).await
            }
            None => crate::with_numeric_slice!(matrix, |values| {
                let mut counts = vec![0u32; num_segments * num_bins as usize];
                for (idx, &value) in values.iter().enumerate() {
                    let value = value.scalar_to_f32();
                    if value.is_nan() {
                        continue;
                    }
                    let segment = if axis == MatrixAxis::Rows { idx / num_cols } else { idx % num_cols };
                    let (lo, hi) = ranges[segment];
                    counts[segment * num_bins as usize + histogram_bin(value, lo, hi, num_bins)] += 1;
                }
                counts
            }),
        }
    })
}

/// Like [`reduce_matrix_axis_histograms`], for a `num_rows x num_cols` sparse
/// `matrix` compressed along `compressed_axis`, whose implicit zeros are counted.
pub fn reduce_compressed_matrix_axis_histograms<'a>(
    gpu_context: Option<&'a GpuContext<'_>>,
    matrix: &'a CompressedMatrix,
    compressed_axis: CompressedAxis,
    num_rows: usize,
    num_cols: usize,
    axis: MatrixAxis,
    ranges: &'a [(f32, f32)],
    num_bins: u32,
) -> MaybeSendBoxFuture<'a, Vec<u32>> {
    Box::pin(async move {
        matrix.validate(compressed_axis, num_rows, num_cols);
        let num_segments = if axis == MatrixAxis::Rows { num_rows } else { num_cols };
        assert_eq!(ranges.len(), num_segments, "one range per segment is required");
        if num_segments == 0 || num_rows * num_cols == 0 || num_bins == 0 {
            return vec![0; num_segments * num_bins as usize];
        }
        match gpu_context {
            Some(ctx) => {
                let prepared = matrix.prepare_gpu(ctx.device, ctx.queue, compressed_axis, MATRIX_VALUE_FIRST_BINDING);
                let reduction = AxisReduction::Histograms { ranges, num_bins };
                gpu_axis_reduce(ctx, &prepared.wgsl, &prepared.textures, num_rows, num_cols, axis, num_segments, reduction).await
            }
            None => {
                let num_other = if axis == MatrixAxis::Rows { num_cols } else { num_rows };
                let mut counts = vec![0u32; num_segments * num_bins as usize];
                let mut num_stored = vec![0usize; num_segments];
                for_each_compressed_entry(matrix, compressed_axis, axis, |segment, entry| {
                    num_stored[segment] += 1;
                    let value = matrix.data.get_f32(entry);
                    if !value.is_nan() {
                        let (lo, hi) = ranges[segment];
                        counts[segment * num_bins as usize + histogram_bin(value, lo, hi, num_bins)] += 1;
                    }
                });
                for (segment, &(lo, hi)) in ranges.iter().enumerate() {
                    counts[segment * num_bins as usize + histogram_bin(0.0, lo, hi, num_bins)] += (num_other - num_stored[segment]) as u32;
                }
                counts
            }
        }
    })
}

/// Where the matrix value textures are bound, after the uniforms, output and ranges buffers.
const MATRIX_VALUE_FIRST_BINDING: u32 = 3;

fn decode_gpu_extents(words: &[u32]) -> (Vec<f32>, Vec<f32>) {
    words
        .chunks(2)
        .map(|pair| {
            let (lo, hi) = (f32::from_bits(pair[0]), f32::from_bits(pair[1]));
            if lo > hi { (f32::INFINITY, f32::NEG_INFINITY) } else { (lo, hi) }
        })
        .unzip()
}

fn storage_entry(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: NonZeroU64::new(4),
        },
        count: None,
    }
}

/// Returns `reduction.words_per_segment()` words per segment: the bits of
/// each `[min, max]` pair for extents, or the bin counts for histograms.
#[allow(clippy::too_many_arguments)]
fn gpu_axis_reduce<'a>(
    gpu_context: &'a GpuContext<'_>,
    matrix_value_wgsl: &'a str,
    matrix_textures: &'a [(wgpu::TextureView, TextureDtype)],
    num_rows: usize,
    num_cols: usize,
    axis: MatrixAxis,
    num_segments: usize,
    reduction: AxisReduction<'a>,
) -> MaybeSendBoxFuture<'a, Vec<u32>> {
    Box::pin(async move {
        let GpuContext { device, queue } = gpu_context;
        let words_per_segment = reduction.words_per_segment();
        let (entry_point, num_bins) = match &reduction {
            AxisReduction::Extents => ("main", 0),
            AxisReduction::Histograms { num_bins, .. } => ("main_histogram", *num_bins),
        };

        let shader_source = ShaderBuilder::new(include_str!("shaders/matrix_axis_extents.wgsl"))
            .inject_function("flat_texel_coord", common::FLAT_TEXEL_COORD)
            .inject_function("matrix_value", matrix_value_wgsl)
            .build();
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("matrix_axis_extents.wgsl"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        let mut layout_entries = vec![
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            storage_entry(1, false),
        ];
        if matches!(reduction, AxisReduction::Histograms { .. }) {
            layout_entries.push(storage_entry(2, true));
        }
        for (i, (_, dtype)) in matrix_textures.iter().enumerate() {
            layout_entries.push(wgpu::BindGroupLayoutEntry {
                binding: MATRIX_VALUE_FIRST_BINDING + i as u32,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Texture {
                    sample_type: dtype.binding_sample_type(),
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            });
        }
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &layout_entries,
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some(entry_point),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        let limits = device.limits();
        let chunk_segments = (limits.max_compute_workgroups_per_dimension as usize * 64)
            .min(limits.max_storage_buffer_binding_size as usize / (words_per_segment * 4))
            .max(1);

        let mut words = Vec::with_capacity(num_segments * words_per_segment);
        for base_segment in (0..num_segments).step_by(chunk_segments) {
            let chunk_len = chunk_segments.min(num_segments - base_segment);
            let output_size = (chunk_len * words_per_segment * 4) as u64;

            let mut uniform_contents = UniformBuffer::new(Vec::<u8>::new());
            uniform_contents
                .write(&MatrixAxisExtentsUniforms {
                    num_rows: num_rows as u32,
                    num_cols: num_cols as u32,
                    axis: (axis == MatrixAxis::Cols) as u32,
                    num_segments: chunk_len as u32,
                    base_segment: base_segment as u32,
                    num_bins,
                })
                .unwrap();
            let uniform_bytes = uniform_contents.into_inner();
            let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("matrix_axis_extents_uniforms"),
                size: uniform_bytes.len() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            queue.write_buffer(&uniform_buffer, 0, &uniform_bytes);

            let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("matrix_axis_extents_output"),
                size: output_size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
            let download_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("matrix_axis_extents_download"),
                size: output_size,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            let ranges_buffer = match &reduction {
                AxisReduction::Extents => None,
                AxisReduction::Histograms { ranges, .. } => {
                    let chunk_ranges: Vec<f32> = ranges[base_segment..base_segment + chunk_len].iter().flat_map(|&(lo, hi)| [lo, hi]).collect();
                    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("matrix_axis_histograms_ranges"),
                        size: (chunk_ranges.len() * 4) as u64,
                        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
                    queue.write_buffer(&buffer, 0, bytemuck::cast_slice(&chunk_ranges));
                    Some(buffer)
                }
            };
            let mut entries = vec![
                wgpu::BindGroupEntry { binding: 0, resource: uniform_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: output_buffer.as_entire_binding() },
            ];
            if let Some(ranges_buffer) = &ranges_buffer {
                entries.push(wgpu::BindGroupEntry { binding: 2, resource: ranges_buffer.as_entire_binding() });
            }
            for (i, (view, _)) in matrix_textures.iter().enumerate() {
                entries.push(wgpu::BindGroupEntry {
                    binding: MATRIX_VALUE_FIRST_BINDING + i as u32,
                    resource: wgpu::BindingResource::TextureView(view),
                });
            }
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &bind_group_layout,
                entries: &entries,
            });

            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
            {
                let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: None, timestamp_writes: None });
                compute_pass.set_pipeline(&pipeline);
                compute_pass.set_bind_group(0, &bind_group, &[]);
                compute_pass.dispatch_workgroups(chunk_len.div_ceil(64) as u32, 1, 1);
            }
            encoder.copy_buffer_to_buffer(&output_buffer, 0, &download_buffer, 0, output_size);
            queue.submit([encoder.finish()]);

            words.extend(read_back_u32(device, &download_buffer).await);
        }
        words
    })
}
