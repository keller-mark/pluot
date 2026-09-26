use std::num::NonZeroU64;

use encase::{ShaderType, UniformBuffer};
use serde::{Deserialize, Serialize};

use crate::numeric_data::NumericData;
use crate::render_types::GpuContext;
use crate::shader_modules::{common, ShaderBuilder};
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
        Some(ctx) => gpu_axis_extents(ctx, matrix, num_rows, num_cols, axis, num_segments).await,
        None => dispatch_cpu!(matrix, |values| cpu_axis_extents(values, num_cols, axis, num_segments)),
    }
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

async fn gpu_axis_extents(
    gpu_context: &GpuContext<'_>,
    matrix: &NumericData,
    num_rows: usize,
    num_cols: usize,
    axis: MatrixAxis,
    num_segments: usize,
) -> (Vec<f32>, Vec<f32>) {
    let GpuContext { device, queue } = gpu_context;
    let (input_view, input_dtype) = matrix.create_data_texture(device, queue, "matrix_axis_extents_input");

    let shader_source = ShaderBuilder::new(include_str!("shaders/matrix_axis_extents.wgsl"))
        .inject_texture_sample_type("input", input_dtype)
        .inject_function("flat_texel_coord", common::FLAT_TEXEL_COORD)
        .build();
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("matrix_axis_extents.wgsl"),
        source: wgpu::ShaderSource::Wgsl(shader_source.into()),
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[
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
                ty: wgpu::BindingType::Texture {
                    sample_type: input_dtype.binding_sample_type(),
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: NonZeroU64::new(8),
                },
                count: None,
            },
        ],
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
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniform_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&input_view) },
                wgpu::BindGroupEntry { binding: 2, resource: output_buffer.as_entire_binding() },
            ],
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
