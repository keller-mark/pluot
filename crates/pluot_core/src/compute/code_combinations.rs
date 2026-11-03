//! Counts the rows sharing each combination of category codes across several categorical
//! columns (i.e., group sizes when grouping by every column), on the GPU.

use encase::{ShaderType, UniformBuffer};

use crate::emphasis_mode::prepare_emphasis_criteria;
use crate::numeric_data::NumericData;
use crate::render_traits::EmphasisCriteria;
use crate::render_types::GpuContext;
use crate::shader_modules::{common, ShaderBuilder};
use crate::wgpu;
use crate::wgpu::util::DeviceExt;

use super::reduce_stratified::read_back_u32;
use super::ForegroundBackground;
use crate::gpu_send::MaybeSendBoxFuture;

const WORKGROUP_SIZE: usize = 64;
const COLUMNS_FIRST_BINDING: u32 = 3;

#[derive(ShaderType)]
struct Uniforms {
    num_rows: u32,
}

/// The number of combinations of the `num_categories` of each column, or `None` when there
/// are too many to count in one buffer on this device.
pub fn num_code_combinations(gpu_context: &GpuContext<'_>, num_categories: &[usize]) -> Option<usize> {
    let max = gpu_context.device.limits().max_storage_buffer_binding_size as usize / 4;
    num_categories.iter().try_fold(1usize, |product, &n| product.checked_mul(n.max(1))).filter(|&product| product <= max)
}

/// One count per combination of codes, ordered by the codes of the first column, then the
/// second, and so on, where column `c`'s codes are in `0..num_categories[c]`. Rows with a code
/// outside of that range in any column (e.g., a missing value's -1) are not counted. Every
/// column of `codes_per_column` must have one code per row, and [`num_code_combinations`] must
/// be `Some`.
///
/// `background` counts the rows meeting `filtering_criteria`, and `foreground` those also meeting
/// `selection_criteria`, each list AND-ed together (an empty list includes every row).
pub fn count_code_combinations<'a>(
    gpu_context: &'a GpuContext<'a>,
    codes_per_column: &'a [&'a NumericData],
    num_categories: &'a [usize],
    filtering_criteria: &'a [EmphasisCriteria],
    selection_criteria: &'a [EmphasisCriteria],
) -> MaybeSendBoxFuture<'a, ForegroundBackground<Vec<u32>>> {
    Box::pin(async move {
        let GpuContext { device, queue } = gpu_context;
        let num_combinations = num_code_combinations(gpu_context, num_categories).expect("too many code combinations for one buffer");
        let num_rows = codes_per_column.iter().map(|codes| codes.len()).min().unwrap_or(0);
        for criteria in filtering_criteria.iter().chain(selection_criteria) {
            criteria.validate_len(num_rows);
        }

        let textures: Vec<_> = codes_per_column
            .iter()
            .enumerate()
            .map(|(column, codes)| codes.create_data_texture(device, queue, &format!("code_combinations_column_{column}")))
            .collect();
        let declarations: String = textures
            .iter()
            .enumerate()
            .map(|(column, (_, dtype))| {
                format!("@group(0) @binding({}) var column_{column}: texture_2d<{}>;\n", column as u32 + COLUMNS_FIRST_BINDING, dtype.sample_type().as_wgsl())
            })
            .collect();
        let filtering_first_binding = COLUMNS_FIRST_BINDING + textures.len() as u32;
        let filtering = prepare_emphasis_criteria(device, queue, filtering_criteria, "is_filtered_in", "filter_data", filtering_first_binding);
        let selection = prepare_emphasis_criteria(
            device, queue, selection_criteria, "is_selected_in", "select_data", filtering_first_binding + filtering.textures.len() as u32,
        );
        // Mixed-radix: the last column's code varies fastest.
        let accumulate: String = num_categories
            .iter()
            .enumerate()
            .map(|(column, &n)| {
                format!(
                    "    let code_{column} = i32(textureLoad(column_{column}, flat_texel_coord(row, textureDimensions(column_{column}).x), 0).x);
    if (code_{column} < 0 || code_{column} >= {n}) {{
        return;
    }}
    combination = combination * {n}u + u32(code_{column});
"
                )
            })
            .collect();
        let source = ShaderBuilder::new(
            "struct Uniforms { num_rows: u32 };
@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var<storage, read_write> background_counts: array<atomic<u32>>;
@group(0) @binding(2) var<storage, read_write> foreground_counts: array<atomic<u32>>;
{{declarations}}
{{flat_texel_coord}}
{{filtering_wgsl}}
{{selection_wgsl}}

@compute @workgroup_size(64)
fn main(@builtin(workgroup_id) wid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>, @builtin(local_invocation_index) lid: u32) {
    let row = (wid.x + wid.y * nwg.x) * 64u + lid;
    if (row >= uniforms.num_rows || !is_filtered_in(row)) {
        return;
    }
    var combination = 0u;
{{accumulate}}
    atomicAdd(&background_counts[combination], 1u);
    if (is_selected_in(row)) {
        atomicAdd(&foreground_counts[combination], 1u);
    }
}",
        )
        .define("declarations", &declarations)
        .inject_function("flat_texel_coord", common::FLAT_TEXEL_COORD)
        .inject_function("filtering_wgsl", &filtering.wgsl)
        .inject_function("selection_wgsl", &selection.wgsl)
        .define("accumulate", &accumulate)
        .build();
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("code_combinations"), source: wgpu::ShaderSource::Wgsl(source.into()) });

        let buffer_entry = |binding: u32, ty: wgpu::BufferBindingType| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer { ty, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        };
        let counts_type = wgpu::BufferBindingType::Storage { read_only: false };
        let mut layout_entries = vec![buffer_entry(0, wgpu::BufferBindingType::Uniform), buffer_entry(1, counts_type), buffer_entry(2, counts_type)];
        let texture_bindings: Vec<(&wgpu::TextureView, wgpu::TextureSampleType)> = textures
            .iter()
            .map(|(view, dtype)| (view, dtype.binding_sample_type()))
            .chain(filtering.textures.iter().chain(&selection.textures).map(|texture| (&texture.view, texture.sample_type)))
            .collect();
        for (offset, &(_, sample_type)) in texture_bindings.iter().enumerate() {
            layout_entries.push(wgpu::BindGroupLayoutEntry {
                binding: COLUMNS_FIRST_BINDING + offset as u32,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Texture { sample_type, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                count: None,
            });
        }
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: None, entries: &layout_entries });
        let pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&bind_group_layout)], immediate_size: 0 });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("code_combinations"),
            layout: Some(&pipeline_layout),
            module: &module,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        let mut uniforms = UniformBuffer::new(Vec::<u8>::new());
        uniforms.write(&Uniforms { num_rows: num_rows as u32 }).unwrap();
        let uniform_buffer =
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("code_combinations_uniforms"), contents: &uniforms.into_inner(), usage: wgpu::BufferUsages::UNIFORM });
        let size = (num_combinations * 4) as u64;
        let create_counts = |label: &str| {
            device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false })
        };
        let create_download = |label: &str| {
            device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false })
        };
        let (background_counts, foreground_counts) = (create_counts("code_combinations_background_counts"), create_counts("code_combinations_foreground_counts"));
        let mut entries = vec![
            wgpu::BindGroupEntry { binding: 0, resource: uniform_buffer.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: background_counts.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 2, resource: foreground_counts.as_entire_binding() },
        ];
        for (offset, &(view, _)) in texture_bindings.iter().enumerate() {
            entries.push(wgpu::BindGroupEntry { binding: COLUMNS_FIRST_BINDING + offset as u32, resource: wgpu::BindingResource::TextureView(view) });
        }
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &bind_group_layout, entries: &entries });

        let (background_download, foreground_download) =
            (create_download("code_combinations_background_download"), create_download("code_combinations_foreground_download"));
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        let num_workgroups = num_rows.div_ceil(WORKGROUP_SIZE);
        if num_workgroups > 0 {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: None, timestamp_writes: None });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            let x = num_workgroups.min(device.limits().max_compute_workgroups_per_dimension as usize);
            pass.dispatch_workgroups(x as u32, num_workgroups.div_ceil(x) as u32, 1);
        }
        encoder.copy_buffer_to_buffer(&background_counts, 0, &background_download, 0, size);
        encoder.copy_buffer_to_buffer(&foreground_counts, 0, &foreground_download, 0, size);
        queue.submit([encoder.finish()]);
        let background = read_back_u32(device, &background_download).await;
        let foreground = read_back_u32(device, &foreground_download).await;
        ForegroundBackground { background, foreground }
    })
}
