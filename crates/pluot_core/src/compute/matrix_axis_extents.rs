use std::num::NonZeroU64;

use encase::{ShaderType, UniformBuffer};
use serde::{Deserialize, Serialize};

use crate::numeric_data::NumericData;
use crate::render_types::GpuContext;
use crate::shader_modules::{common, matrix_value, ShaderBuilder, TextureDtype};
use crate::sparse_matrix::{CompressedAxis, CompressedMatrix};
use crate::wgpu;

use super::reduce::{dispatch_cpu, read_back_f32, ScalarToF32};

/// An axis of a row-major matrix.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
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
}

/// The `(mins, maxs)` of every row (or every column) of the row-major
/// `num_rows x num_cols` `matrix`, skipping NaN values. A row or column with
/// nothing but NaN values has an extent of `(f32::INFINITY, f32::NEG_INFINITY)`.
pub async fn reduce_matrix_axis_extents(
    gpu_context: Option<&GpuContext<'_>>,
    matrix: &NumericData,
    num_rows: usize,
    num_cols: usize,
    axis: MatrixAxis,
) -> (Vec<f32>, Vec<f32>) {
    assert_eq!(matrix.len(), num_rows * num_cols, "matrix length must be num_rows * num_cols");
    let num_segments = if axis == MatrixAxis::Rows { num_rows } else { num_cols };
    if num_segments == 0 || matrix.is_empty() {
        return (vec![f32::INFINITY; num_segments], vec![f32::NEG_INFINITY; num_segments]);
    }
    match gpu_context {
        Some(ctx) => {
            let (view, dtype) = matrix.create_data_texture(ctx.device, ctx.queue, "matrix_axis_extents_input");
            let matrix_value_wgsl = ShaderBuilder::new(matrix_value::DENSE)
                .define_bidx("matrix_data", MATRIX_VALUE_FIRST_BINDING)
                .inject_texture_sample_type("matrix_data", dtype)
                .define("num_cols", "uniforms.num_cols")
                .build();
            gpu_axis_extents(ctx, &matrix_value_wgsl, &[(view, dtype)], num_rows, num_cols, axis, num_segments).await
        }
        None => dispatch_cpu!(matrix, |values| cpu_axis_extents(values, num_cols, axis, num_segments)),
    }
}

/// Like [`reduce_matrix_axis_extents`], for a `num_rows x num_cols` sparse
/// `matrix` compressed along `compressed_axis`, whose implicit zeros count
/// towards the extents.
pub async fn reduce_compressed_matrix_axis_extents(
    gpu_context: Option<&GpuContext<'_>>,
    matrix: &CompressedMatrix,
    compressed_axis: CompressedAxis,
    num_rows: usize,
    num_cols: usize,
    axis: MatrixAxis,
) -> (Vec<f32>, Vec<f32>) {
    matrix.validate(compressed_axis, num_rows, num_cols);
    let num_segments = if axis == MatrixAxis::Rows { num_rows } else { num_cols };
    if num_segments == 0 || num_rows * num_cols == 0 {
        return (vec![f32::INFINITY; num_segments], vec![f32::NEG_INFINITY; num_segments]);
    }
    match gpu_context {
        Some(ctx) => {
            let prepared = matrix.prepare_gpu(ctx.device, ctx.queue, compressed_axis, MATRIX_VALUE_FIRST_BINDING);
            gpu_axis_extents(ctx, &prepared.wgsl, &prepared.textures, num_rows, num_cols, axis, num_segments).await
        }
        None => {
            let num_other = if axis == MatrixAxis::Rows { num_cols } else { num_rows };
            dispatch_cpu!(&matrix.data, |values| cpu_compressed_axis_extents(matrix, values, compressed_axis, axis, num_segments, num_other))
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
    let segments_are_major = (axis == MatrixAxis::Rows) == (compressed_axis == CompressedAxis::Rows);
    let minor_positions: Option<Vec<Option<usize>>> = matrix.minor_subset.as_ref().map(|subset| {
        let max_index = (0..subset.len()).map(|position| subset.get_f64(position) as usize).max().unwrap_or(0);
        let mut positions = vec![None; max_index + 1];
        for position in 0..subset.len() {
            positions[subset.get_f64(position) as usize] = Some(position);
        }
        positions
    });

    let mut extents: Vec<Option<(f32, f32)>> = vec![None; num_segments];
    let mut num_stored = vec![0usize; num_segments];
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
            let segment = if segments_are_major { major } else { minor_position };
            num_stored[segment] += 1;
            let value = values[entry];
            #[allow(clippy::eq_op)]
            if value != value {
                continue;
            }
            let value = value.scalar_to_f32();
            let extent = extents[segment].get_or_insert((value, value));
            extent.0 = extent.0.min(value);
            extent.1 = extent.1.max(value);
        }
    }
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

/// Where the matrix value textures are bound, after the uniforms and output buffer.
const MATRIX_VALUE_FIRST_BINDING: u32 = 2;

async fn gpu_axis_extents(
    gpu_context: &GpuContext<'_>,
    matrix_value_wgsl: &str,
    matrix_textures: &[(wgpu::TextureView, TextureDtype)],
    num_rows: usize,
    num_cols: usize,
    axis: MatrixAxis,
    num_segments: usize,
) -> (Vec<f32>, Vec<f32>) {
    let GpuContext { device, queue } = gpu_context;

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
        wgpu::BindGroupLayoutEntry {
            binding: 1,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: false },
                has_dynamic_offset: false,
                min_binding_size: NonZeroU64::new(8),
            },
            count: None,
        },
    ];
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
        entry_point: Some("main"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });

    let limits = device.limits();
    let chunk_segments = (limits.max_compute_workgroups_per_dimension as usize * 64)
        .min(limits.max_storage_buffer_binding_size as usize / 8)
        .max(64);

    let mut mins = Vec::with_capacity(num_segments);
    let mut maxs = Vec::with_capacity(num_segments);
    for base_segment in (0..num_segments).step_by(chunk_segments) {
        let chunk_len = chunk_segments.min(num_segments - base_segment);
        let output_size = (chunk_len * 8) as u64;

        let mut uniform_contents = UniformBuffer::new(Vec::<u8>::new());
        uniform_contents
            .write(&MatrixAxisExtentsUniforms {
                num_rows: num_rows as u32,
                num_cols: num_cols as u32,
                axis: (axis == MatrixAxis::Cols) as u32,
                num_segments: chunk_len as u32,
                base_segment: base_segment as u32,
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
        let mut entries = vec![
            wgpu::BindGroupEntry { binding: 0, resource: uniform_buffer.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: output_buffer.as_entire_binding() },
        ];
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

        for pair in read_back_f32(device, &download_buffer).await.chunks(2) {
            let is_empty = pair[0] > pair[1];
            mins.push(if is_empty { f32::INFINITY } else { pair[0] });
            maxs.push(if is_empty { f32::NEG_INFINITY } else { pair[1] });
        }
    }
    (mins, maxs)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn matrix() -> NumericData {
        NumericData::Float32(Arc::new(vec![
            3.0, -1.0, f32::NAN,
            0.5, 7.0, f32::NAN,
        ]))
    }

    #[tokio::test]
    async fn cpu_extents_per_row_and_column_skip_nan() {
        let (mins, maxs) = reduce_matrix_axis_extents(None, &matrix(), 2, 3, MatrixAxis::Rows).await;
        assert_eq!((mins, maxs), (vec![-1.0, 0.5], vec![3.0, 7.0]));

        let (mins, maxs) = reduce_matrix_axis_extents(None, &matrix(), 2, 3, MatrixAxis::Cols).await;
        assert_eq!((mins, maxs), (vec![0.5, -1.0, f32::INFINITY], vec![3.0, 7.0, f32::NEG_INFINITY]));
    }

    #[tokio::test]
    async fn cpu_extents_of_integer_matrix() {
        let matrix = NumericData::Uint16(Arc::new(vec![4, 9, 2, 1]));
        let (mins, maxs) = reduce_matrix_axis_extents(None, &matrix, 2, 2, MatrixAxis::Cols).await;
        assert_eq!((mins, maxs), (vec![2.0, 1.0], vec![4.0, 9.0]));
    }

    /// [[0, 5, NaN], [-2, 0, 0], [0, 0, 0]] as CSR, with every stored value nonzero but NaN.
    fn compressed() -> CompressedMatrix {
        CompressedMatrix {
            indptr: NumericData::from(vec![0u32, 2, 3, 3]),
            indices: NumericData::from(vec![1i32, 2, 0]),
            data: NumericData::from(vec![5.0f32, f32::NAN, -2.0]),
            minor_subset: None,
        }
    }

    #[tokio::test]
    async fn cpu_compressed_extents_count_implicit_zeros() {
        let rows = reduce_compressed_matrix_axis_extents(None, &compressed(), CompressedAxis::Rows, 3, 3, MatrixAxis::Rows).await;
        assert_eq!(rows, (vec![0.0, -2.0, 0.0], vec![5.0, 0.0, 0.0]));
        let cols = reduce_compressed_matrix_axis_extents(None, &compressed(), CompressedAxis::Rows, 3, 3, MatrixAxis::Cols).await;
        assert_eq!(cols, (vec![-2.0, 0.0, 0.0], vec![0.0, 5.0, 0.0]));
    }

    #[tokio::test]
    async fn cpu_compressed_extents_follow_the_minor_subset() {
        let matrix = CompressedMatrix { minor_subset: Some(NumericData::from(vec![1u32, 0])), ..compressed() };
        let cols = reduce_compressed_matrix_axis_extents(None, &matrix, CompressedAxis::Rows, 3, 2, MatrixAxis::Cols).await;
        assert_eq!(cols, (vec![0.0, -2.0], vec![5.0, 0.0]));
    }

    #[cfg(not(feature = "lacks_gpu"))]
    #[tokio::test]
    async fn gpu_compressed_extents_match_cpu() {
        let (device, queue) = crate::cache::get_or_init_gpu_context().await.expect("GPU context");
        let gpu_context = GpuContext { device: &device, queue: &queue };
        let subset = CompressedMatrix { minor_subset: Some(NumericData::from(vec![2u32, 0])), ..compressed() };
        for (matrix, num_cols) in [(compressed(), 3), (subset, 2)] {
            for compressed_axis in [CompressedAxis::Rows, CompressedAxis::Cols] {
                let (num_rows, num_cols) = if compressed_axis == CompressedAxis::Rows { (3, num_cols) } else { (num_cols, 3) };
                for axis in [MatrixAxis::Rows, MatrixAxis::Cols] {
                    let gpu = reduce_compressed_matrix_axis_extents(Some(&gpu_context), &matrix, compressed_axis, num_rows, num_cols, axis).await;
                    let cpu = reduce_compressed_matrix_axis_extents(None, &matrix, compressed_axis, num_rows, num_cols, axis).await;
                    assert_eq!(gpu, cpu, "{compressed_axis:?} {axis:?}");
                }
            }
        }
    }

    #[cfg(not(feature = "lacks_gpu"))]
    #[tokio::test]
    async fn gpu_extents_match_cpu() {
        let (device, queue) = crate::cache::get_or_init_gpu_context().await.expect("GPU context");
        let gpu_context = GpuContext { device: &device, queue: &queue };
        for axis in [MatrixAxis::Rows, MatrixAxis::Cols] {
            let gpu = reduce_matrix_axis_extents(Some(&gpu_context), &matrix(), 2, 3, axis).await;
            let cpu = reduce_matrix_axis_extents(None, &matrix(), 2, 3, axis).await;
            assert_eq!(gpu, cpu, "{axis:?}");
        }
    }
}
