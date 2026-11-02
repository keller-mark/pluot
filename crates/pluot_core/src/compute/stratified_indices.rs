//! Compacts the rows that meet some criteria into one ascending run of row
//! indices per stratum (e.g., per category), as a stable partition. On the
//! GPU, a key pass assigns each row its stratum, then either a prefix-sum
//! stream compaction ([`CompactionMethod::GpuScan`]) or a CPU pass over the
//! read-back keys ([`CompactionMethod::ReadBackKeys`]) groups the rows.

use encase::{ShaderType, UniformBuffer};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::emphasis_mode::{cpu_is_included, prepare_emphasis_criteria};
use crate::render_traits::EmphasisCriteria;
use crate::render_types::GpuContext;
use crate::shader_modules::{common, ShaderBuilder};
use crate::wgpu;
use crate::wgpu::util::DeviceExt;

use super::reduce_stratified::{read_back_u32, StratifyBy};
use crate::gpu_send::MaybeSendBoxFuture;

const BLOCK: usize = 256;
const NO_STRATUM: u32 = u32::MAX;
const CRITERIA_FIRST_BINDING: u32 = 5;
/// Bounds the size of the code-to-stratum lookup table.
const MAX_CODE_RANGE: i64 = 1 << 16;

/// How rows are compacted after a GPU pass assigns each one its stratum.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum CompactionMethod {
    /// Prefix-sum stream compaction on the GPU, reading back only the compacted rows.
    #[default]
    GpuScan,
    /// Reads back each row's stratum and compacts them in one CPU pass.
    ReadBackKeys,
}

/// Which rows to consider, in order.
#[derive(Clone, Copy, Debug)]
pub enum RowSource<'a> {
    /// Rows `0..len`.
    All(usize),
    /// The listed rows, which index the criteria and stratify codes.
    Rows(&'a [u32]),
}

impl RowSource<'_> {
    pub fn len(&self) -> usize {
        match self {
            RowSource::All(len) => *len,
            RowSource::Rows(rows) => rows.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn row(&self, i: usize) -> u32 {
        match self {
            RowSource::All(_) => i as u32,
            RowSource::Rows(rows) => rows[i],
        }
    }
}

/// Rows grouped into one run per stratum, each in the order of its [`RowSource`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StratifiedIndices {
    pub indices: Vec<u32>,
    /// `num_strata + 1` entries: stratum `s` is `indices[offsets[s]..offsets[s + 1]]`.
    pub offsets: Vec<u32>,
}

impl StratifiedIndices {
    pub fn stratum(&self, stratum: usize) -> &[u32] {
        &self.indices[self.offsets[stratum] as usize..self.offsets[stratum + 1] as usize]
    }

    pub fn num_strata(&self) -> usize {
        self.offsets.len().saturating_sub(1)
    }
}

/// Groups the `rows` that meet every one of `criteria` by the stratum of their
/// code in `stratify_by` (a single stratum when `None`), dropping rows whose
/// code is not one of its `included_codes`.
pub fn compute_stratified_indices<'a>(
    gpu_context: Option<&'a GpuContext<'a>>,
    rows: RowSource<'a>,
    stratify_by: Option<&'a StratifyBy>,
    criteria: &'a [EmphasisCriteria],
    method: CompactionMethod,
) -> MaybeSendBoxFuture<'a, StratifiedIndices> {
    Box::pin(async move {
        if let RowSource::All(len) = rows {
            for criterion in criteria {
                criterion.validate_len(len);
            }
        }
        let num_strata = stratify_by.map_or(1, |stratify_by| stratify_by.included_codes.len());
        if rows.is_empty() || num_strata == 0 {
            return StratifiedIndices { indices: Vec::new(), offsets: vec![0; num_strata + 1] };
        }
        let Some(gpu_context) = gpu_context else {
            let keys = cpu_keys(rows, stratify_by, criteria);
            return compact_keys_on_cpu(&keys, rows, num_strata);
        };
        let keys = GpuKeys::compute(gpu_context, rows, stratify_by, criteria).await;
        match method {
            CompactionMethod::GpuScan => keys.compact(gpu_context, num_strata).await,
            CompactionMethod::ReadBackKeys => {
                let keys = keys.read_back(gpu_context).await;
                compact_keys_on_cpu(&keys, rows, num_strata)
            }
        }
    })
}

/// One byte per row of `rows`: 1 if it meets every one of `criteria`, else 0.
pub fn compute_row_mask<'a>(gpu_context: Option<&'a GpuContext<'a>>, rows: RowSource<'a>, criteria: &'a [EmphasisCriteria]) -> MaybeSendBoxFuture<'a, Vec<u8>> {
    Box::pin(async move {
        if rows.is_empty() {
            return Vec::new();
        }
        let Some(gpu_context) = gpu_context else {
            return (0..rows.len()).map(|i| cpu_is_included(criteria, rows.row(i) as usize) as u8).collect();
        };
        GpuKeys::compute(gpu_context, rows, None, criteria).await.pack_mask(gpu_context).await
    })
}

/// The code-to-stratum lookup table of `stratify_by`, and the code of its first entry.
fn stratum_lookup(stratify_by: &StratifyBy) -> (Vec<u32>, i64) {
    let (min_code, max_code) = stratify_by.included_codes.iter().fold((i64::MAX, i64::MIN), |(lo, hi), &code| (lo.min(code), hi.max(code)));
    assert!(max_code - min_code < MAX_CODE_RANGE, "stratify codes must span fewer than {MAX_CODE_RANGE} values");
    let mut lookup = vec![NO_STRATUM; (max_code - min_code + 1) as usize];
    for (stratum, &code) in stratify_by.included_codes.iter().enumerate().rev() {
        lookup[(code - min_code) as usize] = stratum as u32;
    }
    (lookup, min_code)
}

fn cpu_keys(rows: RowSource<'_>, stratify_by: Option<&StratifyBy>, criteria: &[EmphasisCriteria]) -> Vec<u32> {
    let lookup = stratify_by.map(stratum_lookup);
    (0..rows.len())
        .map(|i| {
            let row = rows.row(i) as usize;
            if !cpu_is_included(criteria, row) {
                return NO_STRATUM;
            }
            match (stratify_by, &lookup) {
                (Some(stratify_by), Some((lookup, min_code))) => {
                    let code = stratify_by.codes.get_f64(row) as i64 - min_code;
                    if (0..lookup.len() as i64).contains(&code) { lookup[code as usize] } else { NO_STRATUM }
                }
                _ => 0,
            }
        })
        .collect()
}

/// A counting sort of `rows` by `keys`.
fn compact_keys_on_cpu(keys: &[u32], rows: RowSource<'_>, num_strata: usize) -> StratifiedIndices {
    let mut offsets = vec![0u32; num_strata + 1];
    for &key in keys {
        if key != NO_STRATUM {
            offsets[key as usize + 1] += 1;
        }
    }
    for stratum in 0..num_strata {
        offsets[stratum + 1] += offsets[stratum];
    }
    let mut next = offsets.clone();
    let mut indices = vec![0u32; offsets[num_strata] as usize];
    for (i, &key) in keys.iter().enumerate() {
        if key != NO_STRATUM {
            indices[next[key as usize] as usize] = rows.row(i);
            next[key as usize] += 1;
        }
    }
    StratifiedIndices { indices, offsets }
}

#[derive(ShaderType)]
struct KeysUniforms {
    num_elements: u32,
    has_rows: u32,
    min_code: i32,
    lookup_len: u32,
}

#[derive(ShaderType)]
struct CompactionUniforms {
    num_elements: u32,
    num_blocks: u32,
    num_strata: u32,
    has_rows: u32,
}

#[derive(ShaderType)]
struct ScanUniforms {
    len: u32,
    num_blocks: u32,
}

fn uniform_buffer(device: &wgpu::Device, uniforms: &(impl ShaderType + encase::internal::WriteInto)) -> wgpu::Buffer {
    let mut contents = UniformBuffer::new(Vec::<u8>::new());
    contents.write(uniforms).unwrap();
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("stratified_indices_uniforms"),
        contents: &contents.into_inner(),
        usage: wgpu::BufferUsages::UNIFORM,
    })
}

fn storage_buffer_init(device: &wgpu::Device, label: &str, contents: &[u32]) -> wgpu::Buffer {
    // Storage bindings must not be empty.
    let contents: &[u32] = if contents.is_empty() { &[0] } else { contents };
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::cast_slice(contents),
        usage: wgpu::BufferUsages::STORAGE,
    })
}

fn storage_buffer(device: &wgpu::Device, label: &str, num_words: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: (num_words.max(1) * 4) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn download_buffer(device: &wgpu::Device, num_words: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("stratified_indices_download"),
        size: (num_words.max(1) * 4) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    })
}

fn bind_group(device: &wgpu::Device, pipeline: &wgpu::ComputePipeline, entries: &[(u32, &wgpu::Buffer)]) -> wgpu::BindGroup {
    let entries: Vec<wgpu::BindGroupEntry> =
        entries.iter().map(|&(binding, buffer)| wgpu::BindGroupEntry { binding, resource: buffer.as_entire_binding() }).collect();
    device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &pipeline.get_bind_group_layout(0), entries: &entries })
}

/// Dispatches `num_blocks` workgroups, spilling into the second dimension past the per-dimension limit.
fn dispatch_blocks(device: &wgpu::Device, pass: &mut wgpu::ComputePass<'_>, num_blocks: usize) {
    if num_blocks == 0 {
        return;
    }
    let max_per_dimension = device.limits().max_compute_workgroups_per_dimension as usize;
    let x = num_blocks.min(max_per_dimension);
    pass.dispatch_workgroups(x as u32, num_blocks.div_ceil(x) as u32, 1);
}

fn run_pass(device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, pipeline: &wgpu::ComputePipeline, bind_group: &wgpu::BindGroup, num_blocks: usize) {
    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: None, timestamp_writes: None });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, bind_group, &[]);
    dispatch_blocks(device, &mut pass, num_blocks);
}

fn auto_layout_pipeline(device: &wgpu::Device, module: &wgpu::ShaderModule, entry_point: &str) -> wgpu::ComputePipeline {
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some(entry_point),
        layout: None,
        module,
        entry_point: Some(entry_point),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    })
}

/// Encodes an in-place exclusive prefix sum of the first `len` words of `data`.
fn encode_exclusive_scan(device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, module: &wgpu::ShaderModule, data: &wgpu::Buffer, len: usize) {
    let num_blocks = len.div_ceil(BLOCK);
    let block_sums = storage_buffer(device, "exclusive_scan_block_sums", num_blocks);
    let uniforms = uniform_buffer(device, &ScanUniforms { len: len as u32, num_blocks: num_blocks as u32 });

    let scan_blocks = auto_layout_pipeline(device, module, "scan_blocks");
    let scan_bind_group = bind_group(device, &scan_blocks, &[(0, &uniforms), (1, data), (2, &block_sums)]);
    run_pass(device, encoder, &scan_blocks, &scan_bind_group, num_blocks);
    if num_blocks > 1 {
        encode_exclusive_scan(device, encoder, module, &block_sums, num_blocks);
        let add_block_offsets = auto_layout_pipeline(device, module, "add_block_offsets");
        let add_bind_group = bind_group(device, &add_block_offsets, &[(0, &uniforms), (1, data), (2, &block_sums)]);
        run_pass(device, encoder, &add_block_offsets, &add_bind_group, num_blocks);
    }
}

/// Each row's stratum (or [`NO_STRATUM`]), computed on the GPU and left there.
struct GpuKeys {
    keys: wgpu::Buffer,
    rows: wgpu::Buffer,
    has_rows: bool,
    len: usize,
}

impl GpuKeys {
    fn compute<'a>(
        gpu_context: &'a GpuContext<'_>,
        rows: RowSource<'a>,
        stratify_by: Option<&'a StratifyBy>,
        criteria: &'a [EmphasisCriteria],
    ) -> MaybeSendBoxFuture<'a, Self> {
        Box::pin(async move {
            let GpuContext { device, queue } = gpu_context;
            let len = rows.len();
            assert!(
                (len * 4) as u64 <= device.limits().max_storage_buffer_binding_size,
                "{len} rows exceed the maximum storage buffer binding size",
            );
            let prepared = prepare_emphasis_criteria(device, queue, criteria, "is_included", "criteria_data", CRITERIA_FIRST_BINDING);

            let (lookup, min_code) = stratify_by.map(stratum_lookup).unwrap_or_default();
            let stratify_texture = stratify_by.map(|stratify_by| stratify_by.codes.create_data_texture(device, queue, "stratified_indices_codes"));
            let stratum_of = match &stratify_texture {
                Some((_, dtype)) => ShaderBuilder::new(
                    "@group(0) @binding(4) var stratify: texture_2d<{{stratify_dtype}}>;
fn stratum_of(row: u32) -> u32 {
    let code = i32(textureLoad(stratify, flat_texel_coord(row, textureDimensions(stratify).x), 0).x) - uniforms.min_code;
    if (code < 0 || u32(code) >= uniforms.lookup_len) {
        return NO_STRATUM;
    }
    return stratum_lookup[code];
}",
                )
                .inject_texture_sample_type("stratify", *dtype)
                .build(),
                None => "fn stratum_of(row: u32) -> u32 { return 0u; }".to_string(),
            };
            let shader_source = ShaderBuilder::new(include_str!("shaders/stratified_indices_keys.wgsl"))
                .inject_function("flat_texel_coord", common::FLAT_TEXEL_COORD)
                .inject_function("criteria_wgsl", &prepared.wgsl)
                .inject_function("stratum_of", &stratum_of)
                .build();
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("stratified_indices_keys.wgsl"),
                source: wgpu::ShaderSource::Wgsl(shader_source.into()),
            });

            let buffer_entry = |binding: u32, ty: wgpu::BufferBindingType| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer { ty, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            };
            let texture_entry = |binding: u32, sample_type: wgpu::TextureSampleType| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Texture { sample_type, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                count: None,
            };
            let mut layout_entries = vec![
                buffer_entry(0, wgpu::BufferBindingType::Uniform),
                buffer_entry(1, wgpu::BufferBindingType::Storage { read_only: false }),
                buffer_entry(2, wgpu::BufferBindingType::Storage { read_only: true }),
                buffer_entry(3, wgpu::BufferBindingType::Storage { read_only: true }),
            ];
            if let Some((_, dtype)) = &stratify_texture {
                layout_entries.push(texture_entry(4, dtype.binding_sample_type()));
            }
            for (i, texture) in prepared.textures.iter().enumerate() {
                layout_entries.push(texture_entry(CRITERIA_FIRST_BINDING + i as u32, texture.sample_type));
            }
            let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: None, entries: &layout_entries });
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[Some(&bind_group_layout)],
                immediate_size: 0,
            });
            let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("stratified_indices_keys"),
                layout: Some(&pipeline_layout),
                module: &module,
                entry_point: Some("main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            });

            let uniforms = uniform_buffer(device, &KeysUniforms {
                num_elements: len as u32,
                has_rows: matches!(rows, RowSource::Rows(_)) as u32,
                min_code: min_code as i32,
                lookup_len: lookup.len() as u32,
            });
            let keys = storage_buffer(device, "stratified_indices_keys", len);
            let rows_buffer = storage_buffer_init(device, "stratified_indices_rows", match rows {
                RowSource::All(_) => &[],
                RowSource::Rows(rows) => rows,
            });
            let lookup_buffer = storage_buffer_init(device, "stratified_indices_lookup", &lookup);
            let mut entries = vec![
                wgpu::BindGroupEntry { binding: 0, resource: uniforms.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: keys.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: rows_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: lookup_buffer.as_entire_binding() },
            ];
            if let Some((view, _)) = &stratify_texture {
                entries.push(wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(view) });
            }
            for (i, texture) in prepared.textures.iter().enumerate() {
                entries.push(wgpu::BindGroupEntry {
                    binding: CRITERIA_FIRST_BINDING + i as u32,
                    resource: wgpu::BindingResource::TextureView(&texture.view),
                });
            }
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &bind_group_layout, entries: &entries });

            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
            run_pass(device, &mut encoder, &pipeline, &bind_group, len.div_ceil(BLOCK));
            queue.submit([encoder.finish()]);

            Self { keys, rows: rows_buffer, has_rows: matches!(rows, RowSource::Rows(_)), len }
        })
    }

    fn compaction_module(device: &wgpu::Device, num_strata: usize) -> wgpu::ShaderModule {
        let source = ShaderBuilder::new(include_str!("shaders/stream_compaction.wgsl")).define_u32("max_strata", num_strata.max(1) as u32).build();
        device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("stream_compaction.wgsl"), source: wgpu::ShaderSource::Wgsl(source.into()) })
    }

    fn compaction_uniforms(&self, device: &wgpu::Device, num_strata: usize) -> wgpu::Buffer {
        uniform_buffer(device, &CompactionUniforms {
            num_elements: self.len as u32,
            num_blocks: self.len.div_ceil(BLOCK) as u32,
            num_strata: num_strata as u32,
            has_rows: self.has_rows as u32,
        })
    }

    fn read_back<'a>(self, gpu_context: &'a GpuContext<'_>) -> MaybeSendBoxFuture<'a, Vec<u32>> {
        Box::pin(async move {
            let GpuContext { device, queue } = gpu_context;
            let download = download_buffer(device, self.len);
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
            encoder.copy_buffer_to_buffer(&self.keys, 0, &download, 0, (self.len * 4) as u64);
            queue.submit([encoder.finish()]);
            read_back_u32(device, &download).await
        })
    }

    fn compact<'a>(self, gpu_context: &'a GpuContext<'_>, num_strata: usize) -> MaybeSendBoxFuture<'a, StratifiedIndices> {
        Box::pin(async move {
            let GpuContext { device, queue } = gpu_context;
            let max_strata = device.limits().max_compute_workgroup_storage_size as usize / 4 - BLOCK;
            assert!(num_strata <= max_strata, "at most {max_strata} strata can be compacted at once, not {num_strata}");
            let num_blocks = self.len.div_ceil(BLOCK);
            let module = Self::compaction_module(device, num_strata);
            let scan_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("exclusive_scan.wgsl"),
                source: wgpu::ShaderSource::Wgsl(include_str!("shaders/exclusive_scan.wgsl").into()),
            });
            let uniforms = self.compaction_uniforms(device, num_strata);
            // One count per (stratum, block), then a zero whose exclusive prefix sum is the total.
            let num_offsets = num_strata * num_blocks + 1;
            let block_offsets = storage_buffer(device, "stream_compaction_block_offsets", num_offsets);
            let compacted = storage_buffer(device, "stream_compaction_compacted", self.len);

            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
            encoder.clear_buffer(&block_offsets, 0, None);
            let count_blocks = auto_layout_pipeline(device, &module, "count_blocks");
            let count_bind_group = bind_group(device, &count_blocks, &[(0, &uniforms), (1, &self.keys), (2, &block_offsets)]);
            run_pass(device, &mut encoder, &count_blocks, &count_bind_group, num_blocks);
            encode_exclusive_scan(device, &mut encoder, &scan_module, &block_offsets, num_offsets);
            let scatter = auto_layout_pipeline(device, &module, "scatter");
            let scatter_bind_group =
                bind_group(device, &scatter, &[(0, &uniforms), (1, &self.keys), (2, &block_offsets), (3, &self.rows), (4, &compacted)]);
            run_pass(device, &mut encoder, &scatter, &scatter_bind_group, num_blocks);

            let offsets_download = download_buffer(device, num_strata + 1);
            for stratum in 0..=num_strata {
                let source_offset = (stratum * num_blocks * 4) as u64;
                encoder.copy_buffer_to_buffer(&block_offsets, source_offset, &offsets_download, (stratum * 4) as u64, 4);
            }
            queue.submit([encoder.finish()]);
            let offsets = read_back_u32(device, &offsets_download).await;

            let total = offsets[num_strata] as usize;
            let indices = if total == 0 {
                Vec::new()
            } else {
                let indices_download = download_buffer(device, total);
                let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
                encoder.copy_buffer_to_buffer(&compacted, 0, &indices_download, 0, (total * 4) as u64);
                queue.submit([encoder.finish()]);
                read_back_u32(device, &indices_download).await
            };
            StratifiedIndices { indices, offsets }
        })
    }

    fn pack_mask<'a>(self, gpu_context: &'a GpuContext<'_>) -> MaybeSendBoxFuture<'a, Vec<u8>> {
        Box::pin(async move {
            let GpuContext { device, queue } = gpu_context;
            let num_words = self.len.div_ceil(4);
            let module = Self::compaction_module(device, 1);
            let uniforms = self.compaction_uniforms(device, 1);
            let mask_words = storage_buffer(device, "stream_compaction_mask", num_words);
            let pipeline = auto_layout_pipeline(device, &module, "pack_mask");
            let bind_group = bind_group(device, &pipeline, &[(0, &uniforms), (1, &self.keys), (5, &mask_words)]);

            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
            run_pass(device, &mut encoder, &pipeline, &bind_group, num_words.div_ceil(BLOCK));
            let download = download_buffer(device, num_words);
            encoder.copy_buffer_to_buffer(&mask_words, 0, &download, 0, (num_words * 4) as u64);
            queue.submit([encoder.finish()]);
            let mut mask: Vec<u8> = bytemuck::allocation::pod_collect_to_vec(&read_back_u32(device, &download).await);
            mask.truncate(self.len);
            mask
        })
    }
}

