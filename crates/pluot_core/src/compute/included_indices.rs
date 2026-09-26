use std::num::NonZeroU64;

use encase::{ShaderType, UniformBuffer};

use crate::emphasis_mode::{cpu_is_included, prepare_emphasis_criteria};
use crate::render_traits::EmphasisCriteria;
use crate::render_types::GpuContext;
use crate::shader_modules::{common, ShaderBuilder};
use crate::wgpu;

use super::reduce_stratified::read_back_u32;
use super::ForegroundBackground;

const FILTERED_IN: u32 = 1;
const SELECTED_IN: u32 = 2;
const CRITERIA_FIRST_BINDING: u32 = 2;

#[derive(ShaderType)]
struct IncludedIndicesUniforms {
    num_elements: u32,
    base_offset: u32,
}

/// Returns the ascending indices in `0..len` that meet `filtering_criteria`
/// (`background`), and the subset of those that also meet
/// `selection_criteria` (`foreground`). Each criteria list is AND-ed
/// together; an empty list includes everything.
///
/// Every criteria's per-element data must have length `len`.
pub async fn compute_included_indices(
    gpu_context: Option<&GpuContext<'_>>,
    len: usize,
    filtering_criteria: &[EmphasisCriteria],
    selection_criteria: &[EmphasisCriteria],
) -> ForegroundBackground<Vec<u32>> {
    for criteria in filtering_criteria.iter().chain(selection_criteria) {
        criteria.validate_len(len);
    }
    if filtering_criteria.is_empty() && selection_criteria.is_empty() {
        let all: Vec<u32> = (0..len as u32).collect();
        return ForegroundBackground { background: all.clone(), foreground: all };
    }

    let flags = match gpu_context {
        Some(ctx) => gpu_flags(ctx, len, filtering_criteria, selection_criteria).await,
        None => (0..len)
            .map(|i| {
                if !cpu_is_included(filtering_criteria, i) {
                    0
                } else if cpu_is_included(selection_criteria, i) {
                    FILTERED_IN | SELECTED_IN
                } else {
                    FILTERED_IN
                }
            })
            .collect(),
    };

    let indices_with = |flag: u32| {
        flags.iter().enumerate().filter(|(_, f)| **f & flag != 0).map(|(i, _)| i as u32).collect()
    };
    ForegroundBackground { background: indices_with(FILTERED_IN), foreground: indices_with(SELECTED_IN) }
}

async fn gpu_flags(
    gpu_context: &GpuContext<'_>,
    len: usize,
    filtering_criteria: &[EmphasisCriteria],
    selection_criteria: &[EmphasisCriteria],
) -> Vec<u32> {
    let GpuContext { device, queue } = gpu_context;

    let filtering = prepare_emphasis_criteria(
        device, queue, filtering_criteria, "is_filtered_in", "filter_data", CRITERIA_FIRST_BINDING,
    );
    let selection = prepare_emphasis_criteria(
        device, queue, selection_criteria, "is_selected_in", "select_data",
        CRITERIA_FIRST_BINDING + filtering.textures.len() as u32,
    );
    let criteria_textures: Vec<_> = filtering.textures.iter().chain(selection.textures.iter()).collect();

    let shader_source = ShaderBuilder::new(include_str!("shaders/included_indices.wgsl"))
        .inject_function("flat_texel_coord", common::FLAT_TEXEL_COORD)
        .inject_function("filtering_wgsl", &filtering.wgsl)
        .inject_function("selection_wgsl", &selection.wgsl)
        .build();
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("included_indices.wgsl"),
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
                min_binding_size: NonZeroU64::new(4),
            },
            count: None,
        },
    ];
    for (i, texture) in criteria_textures.iter().enumerate() {
        layout_entries.push(wgpu::BindGroupLayoutEntry {
            binding: CRITERIA_FIRST_BINDING + i as u32,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Texture {
                sample_type: texture.sample_type,
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
    let chunk_elements = (limits.max_compute_workgroups_per_dimension as usize * 64)
        .min(limits.max_storage_buffer_binding_size as usize / 4)
        .max(64);

    let mut flags = Vec::with_capacity(len);
    for base_offset in (0..len).step_by(chunk_elements) {
        let chunk_len = chunk_elements.min(len - base_offset);
        let output_size = (chunk_len * 4) as u64;

        let mut uniform_contents = UniformBuffer::new(Vec::<u8>::new());
        uniform_contents
            .write(&IncludedIndicesUniforms { num_elements: chunk_len as u32, base_offset: base_offset as u32 })
            .unwrap();
        let uniform_bytes = uniform_contents.into_inner();
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("included_indices_uniforms"),
            size: uniform_bytes.len() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&uniform_buffer, 0, &uniform_bytes);

        let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("included_indices_flags"),
            size: output_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let download_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("included_indices_download"),
            size: output_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut entries = vec![
            wgpu::BindGroupEntry { binding: 0, resource: uniform_buffer.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: output_buffer.as_entire_binding() },
        ];
        for (i, texture) in criteria_textures.iter().enumerate() {
            entries.push(wgpu::BindGroupEntry {
                binding: CRITERIA_FIRST_BINDING + i as u32,
                resource: wgpu::BindingResource::TextureView(&texture.view),
            });
        }
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &bind_group_layout,
            entries: &entries,
        });

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });
            compute_pass.set_pipeline(&pipeline);
            compute_pass.set_bind_group(0, &bind_group, &[]);
            compute_pass.dispatch_workgroups(chunk_len.div_ceil(64) as u32, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&output_buffer, 0, &download_buffer, 0, output_size);
        queue.submit([encoder.finish()]);
        flags.extend(read_back_u32(device, &download_buffer).await);
    }
    flags
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::numeric_data::NumericData;
    use crate::render_traits::{CategoricalCriteriaParams, QuantitativeCriteriaParams};

    fn criteria() -> (Vec<EmphasisCriteria>, Vec<EmphasisCriteria>) {
        let filtering = vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
            codes: NumericData::Int32(Arc::new(vec![0, 1, 1, 2, 1, 0])),
            included_codes: vec![1, 2],
        })];
        let selection = vec![EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
            values: NumericData::Float32(Arc::new(vec![0.0, 5.0, 1.0, 9.0, 7.0, 8.0])),
            min: Some(4.0),
            max: None,
            min_exclusive: None,
            max_exclusive: None,
        })];
        (filtering, selection)
    }

    #[tokio::test]
    async fn cpu_indices_are_filtered_then_selected() {
        let (filtering, selection) = criteria();
        let result = compute_included_indices(None, 6, &filtering, &selection).await;
        assert_eq!(result.background, vec![1, 2, 3, 4]);
        assert_eq!(result.foreground, vec![1, 3, 4]);
    }

    #[tokio::test]
    async fn no_criteria_includes_everything() {
        let result = compute_included_indices(None, 3, &[], &[]).await;
        assert_eq!(result.background, vec![0, 1, 2]);
        assert_eq!(result.foreground, vec![0, 1, 2]);
    }

    #[cfg(not(feature = "lacks_gpu"))]
    #[tokio::test]
    async fn gpu_indices_match_cpu() {
        let (device, queue) = crate::cache::get_or_init_gpu_context().await.expect("GPU context");
        let gpu_context = GpuContext { device: &device, queue: &queue };
        let (filtering, selection) = criteria();
        let gpu = compute_included_indices(Some(&gpu_context), 6, &filtering, &selection).await;
        let cpu = compute_included_indices(None, 6, &filtering, &selection).await;
        assert_eq!(gpu, cpu);
    }
}
