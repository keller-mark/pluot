//! Per-stratum violin statistics on the GPU, for both the filter-included
//! ("background") and the filter- and selection-included ("foreground")
//! values: counts, quartiles, non-outlier bounds and extents (via
//! [`compute_stratified_violin_stats`]), then bin counts (via
//! [`compute_stratified_violin_bins`]) once the bins are known.
//!
//! Matches the CPU summaries of `pluot_zarr::violin_data`, except that
//! quartiles are estimated by a [`PercentileSearch`] rather than by sorting.

use encase::{ShaderType, UniformBuffer};

use crate::emphasis_mode::prepare_emphasis_criteria;
use crate::numeric_data::NumericData;
use crate::render_traits::EmphasisCriteria;
use crate::render_types::GpuContext;
use crate::shader_modules::{common, ShaderBuilder};
use crate::wgpu;
use crate::wgpu::util::DeviceExt;

use super::histogram_percentile::PercentileSearch;
use super::reduce_stratified::{read_back_u32, StratifyBy};
use super::ForegroundBackground;
use crate::gpu_send::MaybeSendBoxFuture;

const WORKGROUP_SIZE: usize = 64;
const PERCENTILE_NUM_BINS: usize = 256;
/// Each pass after the first narrows each quartile's range by up to `PERCENTILE_NUM_BINS` times.
/// Three, since expression outliers can stretch the initial range far beyond the quartiles.
const PERCENTILE_NUM_PASSES: usize = 3;
const QUARTILE_PERCENTILES: [f32; 3] = [25.0, 50.0, 75.0];
const CRITERIA_FIRST_BINDING: u32 = 6;
/// Values beyond this many standard deviations from the mean are outliers (Chauvenet's criterion).
const OUTLIER_DEVIATIONS: f32 = 3.0;

/// The values to summarize: those of `values` that meet `filtering_criteria`, are not NaN, are
/// nonzero unless `keep_zeros`, and whose code in `stratify_by` is one of its `included_codes`.
pub struct StratifiedViolinInput<'a> {
    pub values: &'a NumericData,
    pub stratify_by: &'a StratifyBy,
    pub filtering_criteria: &'a [EmphasisCriteria],
    pub selection_criteria: &'a [EmphasisCriteria],
    pub keep_zeros: bool,
}

/// One stratum's values, summarized for drawing its violin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViolinStratumStats {
    pub num_values: u32,
    /// (first quartile, median, third quartile), or `None` when there are no values.
    pub quartiles: Option<(f32, f32, f32)>,
    /// The (inclusive) range of the non-outliers, which are within three standard deviations of
    /// the mean. Unbounded when there are fewer than two values.
    pub non_outlier_bounds: (f32, f32),
    /// The (min, max) of the non-outliers, or `None` when there are none.
    pub non_outlier_extent: Option<(f32, f32)>,
}

#[derive(ShaderType)]
struct ViolinUniforms {
    num_elements: u32,
    keep_zeros: u32,
    num_bins: u32,
    domain_min: f32,
    domain_max: f32,
}

fn order_key(value: f32) -> u32 {
    let bits = value.to_bits();
    bits ^ if bits & 0x8000_0000 != 0 { 0xFFFF_FFFF } else { 0x8000_0000 }
}

fn order_key_to_f32(key: u32) -> f32 {
    f32::from_bits(key ^ if key & 0x8000_0000 != 0 { 0x8000_0000 } else { 0xFFFF_FFFF })
}

fn split_sides<T: Clone>(per_slot: Vec<T>) -> ForegroundBackground<Vec<T>> {
    let background = per_slot.iter().step_by(2).cloned().collect();
    let foreground = per_slot.iter().skip(1).step_by(2).cloned().collect();
    ForegroundBackground { background, foreground }
}

fn join_sides<T: Copy>(sides: &ForegroundBackground<Vec<T>>) -> Vec<T> {
    sides.background.iter().zip(&sides.foreground).flat_map(|(&background, &foreground)| [background, foreground]).collect()
}

/// The values, codes and criteria of one [`StratifiedViolinInput`], uploaded once and shared by its passes.
struct ViolinPasses<'a> {
    gpu_context: &'a GpuContext<'a>,
    module: wgpu::ShaderModule,
    bind_group_layout: wgpu::BindGroupLayout,
    textures: Vec<wgpu::TextureView>,
    num_elements: usize,
    num_slots: usize,
    keep_zeros: bool,
}

impl<'a> ViolinPasses<'a> {
    fn new(gpu_context: &'a GpuContext<'a>, input: &StratifiedViolinInput<'_>) -> Self {
        let GpuContext { device, queue } = gpu_context;
        let (input_view, input_dtype) = input.values.create_data_texture(device, queue, "stratified_violin_input");
        let (stratify_view, stratify_dtype) = input.stratify_by.codes.create_data_texture(device, queue, "stratified_violin_codes");
        let filtering = prepare_emphasis_criteria(device, queue, input.filtering_criteria, "is_filtered_in", "filter_data", CRITERIA_FIRST_BINDING);
        let selection = prepare_emphasis_criteria(
            device, queue, input.selection_criteria, "is_selected_in", "select_data",
            CRITERIA_FIRST_BINDING + filtering.textures.len() as u32,
        );
        let strata_codes = input.stratify_by.included_codes.iter().map(i64::to_string).collect::<Vec<_>>().join(", ");
        let source = ShaderBuilder::new(include_str!("shaders/stratified_violin.wgsl"))
            .inject_texture_sample_type("input", input_dtype)
            .inject_texture_sample_type("stratify", stratify_dtype)
            .inject_function("flat_texel_coord", common::FLAT_TEXEL_COORD)
            .inject_function("filtering_wgsl", &filtering.wgsl)
            .inject_function("selection_wgsl", &selection.wgsl)
            .define_u32("percentile_num_bins", PERCENTILE_NUM_BINS as u32)
            .define_u32("num_strata", input.stratify_by.included_codes.len() as u32)
            .define("strata_codes", &strata_codes)
            .build();
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("stratified_violin.wgsl"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
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
        let criteria_textures: Vec<_> = filtering.textures.into_iter().chain(selection.textures).collect();
        let mut entries = vec![
            buffer_entry(0, wgpu::BufferBindingType::Uniform),
            texture_entry(1, input_dtype.binding_sample_type()),
            texture_entry(2, stratify_dtype.binding_sample_type()),
            buffer_entry(3, wgpu::BufferBindingType::Storage { read_only: false }),
            buffer_entry(4, wgpu::BufferBindingType::Storage { read_only: true }),
            buffer_entry(5, wgpu::BufferBindingType::Storage { read_only: true }),
        ];
        for (i, texture) in criteria_textures.iter().enumerate() {
            entries.push(texture_entry(CRITERIA_FIRST_BINDING + i as u32, texture.sample_type));
        }
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: None, entries: &entries });
        let mut textures = vec![input_view, stratify_view];
        textures.extend(criteria_textures.into_iter().map(|texture| texture.view));

        Self {
            gpu_context,
            module,
            bind_group_layout,
            textures,
            num_elements: input.values.len(),
            num_slots: input.stratify_by.included_codes.len() * 2,
            keep_zeros: input.keep_zeros,
        }
    }

    /// Runs `entry_point` over every value, accumulating into an output seeded with `output_init`.
    fn run<'b>(
        &'b self,
        entry_point: &'b str,
        output_init: Vec<u32>,
        params: &'b [f32],
        thresholds: &'b [f32],
        num_bins: usize,
        domain: (f32, f32),
    ) -> MaybeSendBoxFuture<'b, Vec<u32>> {
        Box::pin(async move {
            let GpuContext { device, queue } = self.gpu_context;
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[Some(&self.bind_group_layout)],
                immediate_size: 0,
            });
            let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry_point),
                layout: Some(&pipeline_layout),
                module: &self.module,
                entry_point: Some(entry_point),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            });

            let mut uniforms = UniformBuffer::new(Vec::<u8>::new());
            uniforms
                .write(&ViolinUniforms {
                    num_elements: self.num_elements as u32,
                    keep_zeros: self.keep_zeros as u32,
                    num_bins: num_bins as u32,
                    domain_min: domain.0,
                    domain_max: domain.1,
                })
                .unwrap();
            let buffer = |label: &str, contents: &[u8], usage: wgpu::BufferUsages| {
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some(label), contents, usage })
            };
            // Storage bindings must not be empty.
            let non_empty = |values: &'b [f32]| -> &'b [f32] { if values.is_empty() { &[0.0] } else { values } };
            let uniform_buffer = buffer("stratified_violin_uniforms", &uniforms.into_inner(), wgpu::BufferUsages::UNIFORM);
            let output_size = (output_init.len() * 4) as u64;
            let output = buffer("stratified_violin_output", bytemuck::cast_slice(&output_init), wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC);
            let params = buffer("stratified_violin_params", bytemuck::cast_slice(non_empty(params)), wgpu::BufferUsages::STORAGE);
            let thresholds = buffer("stratified_violin_thresholds", bytemuck::cast_slice(non_empty(thresholds)), wgpu::BufferUsages::STORAGE);

            let mut entries = vec![
                wgpu::BindGroupEntry { binding: 0, resource: uniform_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&self.textures[0]) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&self.textures[1]) },
                wgpu::BindGroupEntry { binding: 3, resource: output.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 4, resource: params.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 5, resource: thresholds.as_entire_binding() },
            ];
            for (i, view) in self.textures[2..].iter().enumerate() {
                entries.push(wgpu::BindGroupEntry { binding: CRITERIA_FIRST_BINDING + i as u32, resource: wgpu::BindingResource::TextureView(view) });
            }
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &self.bind_group_layout, entries: &entries });

            let num_workgroups = self.num_elements.div_ceil(WORKGROUP_SIZE);
            let max_per_dimension = device.limits().max_compute_workgroups_per_dimension as usize;
            let download = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("stratified_violin_download"),
                size: output_size,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
            if num_workgroups > 0 {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: None, timestamp_writes: None });
                pass.set_pipeline(&pipeline);
                pass.set_bind_group(0, &bind_group, &[]);
                let x = num_workgroups.min(max_per_dimension);
                pass.dispatch_workgroups(x as u32, num_workgroups.div_ceil(x) as u32, 1);
            }
            encoder.copy_buffer_to_buffer(&output, 0, &download, 0, output_size);
            queue.submit([encoder.finish()]);
            read_back_u32(device, &download).await
        })
    }

    fn run_with_params<'b>(&'b self, entry_point: &'b str, output_init: Vec<u32>, params: &'b [f32]) -> MaybeSendBoxFuture<'b, Vec<u32>> {
        self.run(entry_point, output_init, params, &[], 0, (0.0, 0.0))
    }
}

/// Counts, quartiles, and non-outlier bounds and extents of each stratum's values.
pub fn compute_stratified_violin_stats<'a>(
    gpu_context: &'a GpuContext<'a>,
    input: &'a StratifiedViolinInput<'a>,
) -> MaybeSendBoxFuture<'a, ForegroundBackground<Vec<ViolinStratumStats>>> {
    Box::pin(async move {
        if input.stratify_by.included_codes.is_empty() {
            return ForegroundBackground { background: Vec::new(), foreground: Vec::new() };
        }
        let passes = ViolinPasses::new(gpu_context, input);
        let num_slots = passes.num_slots;

        let moments_init = (0..num_slots).flat_map(|_| [0, 0f32.to_bits(), order_key(f32::INFINITY), order_key(f32::NEG_INFINITY)]).collect();
        let moments = passes.run_with_params("moments", moments_init, &[]).await;
        let counts: Vec<u32> = moments.chunks(4).map(|slot| slot[0]).collect();
        let means: Vec<f32> = moments.chunks(4).map(|slot| f32::from_bits(slot[1]) / slot[0].max(1) as f32).collect();
        let extents: Vec<(f32, f32)> = moments.chunks(4).map(|slot| (order_key_to_f32(slot[2]), order_key_to_f32(slot[3]))).collect();

        let squares = passes.run_with_params("centered_squares", vec![0f32.to_bits(); num_slots], &means).await;
        let bounds: Vec<(f32, f32)> = (0..num_slots)
            .map(|slot| {
                if counts[slot] < 2 {
                    return (f32::NEG_INFINITY, f32::INFINITY);
                }
                let deviation = (f32::from_bits(squares[slot]) / (counts[slot] - 1) as f32).sqrt();
                (means[slot] - OUTLIER_DEVIATIONS * deviation, means[slot] + OUTLIER_DEVIATIONS * deviation)
            })
            .collect();
        let bounds_params: Vec<f32> = bounds.iter().flat_map(|&(lo, hi)| [lo, hi]).collect();
        let extent_init = (0..num_slots).flat_map(|_| [order_key(f32::INFINITY), order_key(f32::NEG_INFINITY)]).collect();
        let non_outlier_extents = passes.run_with_params("non_outlier_extent", extent_init, &bounds_params).await;

        let mut searches: Vec<PercentileSearch> =
            extents.iter().flat_map(|&extent| QUARTILE_PERCENTILES.map(|percentile| PercentileSearch::new(extent, percentile))).collect();
        let histograms = async |searches: &[PercentileSearch]| {
            let ranges: Vec<f32> = searches.iter().flat_map(|search| <[f32; 2]>::from(search.range())).collect();
            passes.run_with_params("quartile_histograms", vec![0; searches.len() * PERCENTILE_NUM_BINS], &ranges).await
        };
        for _ in 1..PERCENTILE_NUM_PASSES {
            let counts = histograms(&searches).await;
            searches.iter_mut().zip(counts.chunks(PERCENTILE_NUM_BINS)).for_each(|(search, counts)| search.narrow(counts));
        }
        let counts_per_search = histograms(&searches).await;
        let quartile_estimates: Vec<f32> =
            searches.iter().zip(counts_per_search.chunks(PERCENTILE_NUM_BINS)).map(|(search, counts)| search.estimate(counts)).collect();

        let stats = (0..num_slots)
            .map(|slot| {
                let (min_key, max_key) = (non_outlier_extents[slot * 2], non_outlier_extents[slot * 2 + 1]);
                let q = &quartile_estimates[slot * 3..slot * 3 + 3];
                ViolinStratumStats {
                    num_values: counts[slot],
                    quartiles: (counts[slot] > 0).then_some((q[0], q[1], q[2])),
                    non_outlier_bounds: bounds[slot],
                    non_outlier_extent: (min_key <= max_key).then(|| (order_key_to_f32(min_key), order_key_to_f32(max_key))),
                }
            })
            .collect();
        split_sides(stats)
    })
}

/// Counts each stratum's non-outliers (per `non_outlier_bounds`) within `domain` into the bins
/// delimited by `thresholds`, which must be ascending and within `domain`. Each stratum has
/// `thresholds.len() + 1` bins, as `d3::array::bin` makes, the last of which includes `domain.1`.
pub fn compute_stratified_violin_bins<'a>(
    gpu_context: &'a GpuContext<'a>,
    input: &'a StratifiedViolinInput<'a>,
    non_outlier_bounds: &'a ForegroundBackground<Vec<(f32, f32)>>,
    domain: (f32, f32),
    thresholds: &'a [f32],
) -> MaybeSendBoxFuture<'a, ForegroundBackground<Vec<Vec<u32>>>> {
    Box::pin(async move {
        if input.stratify_by.included_codes.is_empty() {
            return ForegroundBackground { background: Vec::new(), foreground: Vec::new() };
        }
        let passes = ViolinPasses::new(gpu_context, input);
        let num_bins = thresholds.len() + 1;
        let bounds_params: Vec<f32> = join_sides(non_outlier_bounds).iter().flat_map(|&(lo, hi)| [lo, hi]).collect();
        let counts = passes.run("violin_bins", vec![0; passes.num_slots * num_bins], &bounds_params, thresholds, num_bins, domain).await;
        split_sides(counts.chunks(num_bins).map(<[u32]>::to_vec).collect())
    })
}
