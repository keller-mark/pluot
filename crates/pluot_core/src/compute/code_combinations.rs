//! Counts the rows sharing each combination of category codes across several categorical
//! columns (i.e., group sizes when grouping by every column), on the GPU.

use encase::{ShaderType, UniformBuffer};

use crate::numeric_data::NumericData;
use crate::render_types::GpuContext;
use crate::shader_modules::{common, ShaderBuilder};
use crate::wgpu;
use crate::wgpu::util::DeviceExt;

use super::reduce_stratified::read_back_u32;
use crate::gpu_send::MaybeSendBoxFuture;

const WORKGROUP_SIZE: usize = 64;

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
pub fn count_code_combinations<'a>(
    gpu_context: &'a GpuContext<'a>,
    codes_per_column: &'a [&'a NumericData],
    num_categories: &'a [usize],
) -> MaybeSendBoxFuture<'a, Vec<u32>> {
    Box::pin(async move {
        let GpuContext { device, queue } = gpu_context;
        let num_combinations = num_code_combinations(gpu_context, num_categories).expect("too many code combinations for one buffer");
        let num_rows = codes_per_column.iter().map(|codes| codes.len()).min().unwrap_or(0);

        let textures: Vec<_> = codes_per_column
            .iter()
            .enumerate()
            .map(|(column, codes)| codes.create_data_texture(device, queue, &format!("code_combinations_column_{column}")))
            .collect();
        let declarations: String = textures
            .iter()
            .enumerate()
            .map(|(column, (_, dtype))| format!("@group(0) @binding({}) var column_{column}: texture_2d<{}>;\n", column + 2, dtype.sample_type().as_wgsl()))
            .collect();
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
@group(0) @binding(1) var<storage, read_write> counts: array<atomic<u32>>;
{{declarations}}
{{flat_texel_coord}}

@compute @workgroup_size(64)
fn main(@builtin(workgroup_id) wid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>, @builtin(local_invocation_index) lid: u32) {
    let row = (wid.x + wid.y * nwg.x) * 64u + lid;
    if (row >= uniforms.num_rows) {
        return;
    }
    var combination = 0u;
{{accumulate}}
    atomicAdd(&counts[combination], 1u);
}",
        )
        .define("declarations", &declarations)
        .inject_function("flat_texel_coord", common::FLAT_TEXEL_COORD)
        .define("accumulate", &accumulate)
        .build();
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("code_combinations"), source: wgpu::ShaderSource::Wgsl(source.into()) });

        let buffer_entry = |binding: u32, ty: wgpu::BufferBindingType| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer { ty, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        };
        let mut layout_entries = vec![buffer_entry(0, wgpu::BufferBindingType::Uniform), buffer_entry(1, wgpu::BufferBindingType::Storage { read_only: false })];
        for (column, (_, dtype)) in textures.iter().enumerate() {
            layout_entries.push(wgpu::BindGroupLayoutEntry {
                binding: column as u32 + 2,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Texture { sample_type: dtype.binding_sample_type(), view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
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
        let counts = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("code_combinations_counts"),
            size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let mut entries = vec![
            wgpu::BindGroupEntry { binding: 0, resource: uniform_buffer.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: counts.as_entire_binding() },
        ];
        for (column, (view, _)) in textures.iter().enumerate() {
            entries.push(wgpu::BindGroupEntry { binding: column as u32 + 2, resource: wgpu::BindingResource::TextureView(view) });
        }
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &bind_group_layout, entries: &entries });

        let download = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("code_combinations_download"),
            size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        let num_workgroups = num_rows.div_ceil(WORKGROUP_SIZE);
        if num_workgroups > 0 {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: None, timestamp_writes: None });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            let x = num_workgroups.min(device.limits().max_compute_workgroups_per_dimension as usize);
            pass.dispatch_workgroups(x as u32, num_workgroups.div_ceil(x) as u32, 1);
        }
        encoder.copy_buffer_to_buffer(&counts, 0, &download, 0, size);
        queue.submit([encoder.finish()]);
        read_back_u32(device, &download).await
    })
}
