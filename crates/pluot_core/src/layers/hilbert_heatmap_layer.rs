// Lays out an ordered list of items along a generalized Hilbert ("gilbert")
// curve filling the layer bounds, coloring each cell by the value of the item
// it falls on. The curve keeps items that are close in the order close in 2D,
// so contiguous groups of items (e.g., cells sorted by cell type) form blobs.
//
// The grid is laid out in pixel units within the layer bounds, so it does not
// follow the camera. Each fragment computes its cell's position along the
// curve, the item at that position, and that item's color, so no per-item or
// per-pixel work happens on the CPU.
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use encase::{ShaderType, UniformBuffer};
use glam::Vec2;
use image::{codecs::png::PngEncoder, ExtendedColorType, ImageEncoder};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use std::sync::Arc;

use crate::color_mode::create_palette_texture;
use crate::emphasis_mode::{cpu_is_included, prepare_emphasis_criteria};
use crate::gilbert::{gilbert_xy2d, item_at_curve_position};
use crate::layers::heatmap_layer::{HeatmapColormap, DEFAULT_BACKGROUND_OPACITY};
use crate::numeric_data::{NumericData, NumericValue};
use crate::picking::LayerPickingResult;
use crate::render_traits::{
    DrawToRasterCpu, DrawToRasterGpu, DrawToSvg, EmphasisCriteria, ExtentableLayer, MarginParams, PickableLayer,
    PreparedLayer, ViewParams,
};
use crate::render_types::{CpuContext, CpuRenderPass, GpuContext, PrepareResult};
use crate::gpu_send::GpuRenderPass;
use crate::shader_modules::{colormaps as wgsl_colormaps, common, heatmap_cell_color, ShaderBuilder};
use crate::two::shapes::{TwoElement, TwoGroup, TwoImage, TwoImageRenderingStyle};
use crate::two::svg::{update_svg, SvgContext};
use crate::viewport::{DataCoord, ScreenCoord};
use crate::wgpu;

/// Layer params struct for [`HilbertHeatmapLayer`].
#[derive(Serialize, Deserialize, Debug, Clone, TS)]
#[serde(default)]
#[ts(optional_fields = nullable)]
pub struct HilbertHeatmapLayerParams {
    pub layer_id: String,
    pub bounds: Option<MarginParams>,

    /// Side length, in pixels, of each cell of the curve's grid, which fills the
    /// layer bounds. Defaults to 1.
    pub cell_size: Option<f32>,

    /// The items to lay out along the curve, in order, each as an index into
    /// `values` and the criteria. The items are spread evenly along the whole
    /// curve, so list only the filter-included items to fill it without gaps.
    pub ordered_indices: NumericData,
    /// One value per item: a scalar for a quantitative colormap, or a category
    /// code for a categorical one. NaNs and negative category codes are not drawn.
    pub values: NumericData,

    /// Defaults to viridis over (0, 1). Per-axis domains do not apply.
    pub colormap: Option<HeatmapColormap>,

    /// One entry per item. Filter-excluded items leave their cells empty.
    pub filtering_criteria: Vec<EmphasisCriteria>,
    /// One entry per item. Selection-excluded items are drawn at `background_opacity`.
    pub selection_criteria: Vec<EmphasisCriteria>,
    /// Opacity multiplier for cells whose item is not selected.
    pub background_opacity: Option<f32>,

    pub opacity: f32,
}

impl Default for HilbertHeatmapLayerParams {
    fn default() -> Self {
        Self {
            layer_id: "".to_string(),
            bounds: None,
            cell_size: None,
            ordered_indices: NumericData::Uint32(Arc::new(vec![])),
            values: NumericData::Float32(Arc::new(vec![])),
            colormap: None,
            filtering_criteria: vec![],
            selection_criteria: vec![],
            background_opacity: None,
            opacity: 1.0,
        }
    }
}

/// The grid of cells that the curve fills, within the layer bounds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HilbertGrid {
    pub margin_left: f32,
    pub margin_top: f32,
    pub margin_bottom: f32,
    pub layer_height: f32,
    pub cell_size: f32,
    pub width: u32,
    pub height: u32,
}

impl HilbertGrid {
    pub fn new(view_params: &ViewParams, bounds: Option<&MarginParams>, cell_size: Option<f32>) -> Self {
        let bounds = bounds.or(view_params.margins.as_ref());
        let margin = |get: fn(&MarginParams) -> Option<f32>| bounds.and_then(get).unwrap_or(0.0);
        let (margin_top, margin_right) = (margin(|m| m.margin_top), margin(|m| m.margin_right));
        let (margin_bottom, margin_left) = (margin(|m| m.margin_bottom), margin(|m| m.margin_left));
        let layer_width = (view_params.width as f32 - (margin_left + margin_right)).max(0.0);
        let layer_height = (view_params.height as f32 - (margin_top + margin_bottom)).max(0.0);
        let cell_size = cell_size.unwrap_or(1.0).max(f32::EPSILON);
        Self {
            margin_left,
            margin_top,
            margin_bottom,
            layer_height,
            cell_size,
            width: (layer_width / cell_size).floor() as u32,
            height: (layer_height / cell_size).floor() as u32,
        }
    }

    pub fn len(&self) -> u32 {
        self.width * self.height
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The cell under `screen_coord` (whose Y increases upwards), counting rows from the top.
    pub fn cell_at(&self, screen_coord: ScreenCoord) -> Option<(u32, u32)> {
        let x = (screen_coord.x - self.margin_left) / self.cell_size;
        let y = (self.layer_height - (screen_coord.y - self.margin_bottom)) / self.cell_size;
        if x < 0.0 || y < 0.0 || x >= self.width as f32 || y >= self.height as f32 {
            return None;
        }
        Some((x.floor() as u32, y.floor() as u32))
    }

    /// The position along the curve of `cell`, and the position in the order of
    /// the item drawn there, given the number of items.
    pub fn curve_position_and_ordinal(&self, cell: (u32, u32), num_items: usize) -> (u32, usize) {
        let position = gilbert_xy2d(cell.0, cell.1, self.width, self.height);
        (position, item_at_curve_position(position, num_items as u32, self.len()) as usize)
    }
}

fn index_at(indices: &NumericData, i: usize) -> usize {
    match indices.value(i) {
        NumericValue::Uint(index) => index as usize,
        NumericValue::Int(index) => index as usize,
        NumericValue::Float32(index) => index as usize,
        NumericValue::Float64(index) => index as usize,
    }
}

pub struct HilbertHeatmapLayer {
    view_params: ViewParams,
    layer_params: HilbertHeatmapLayerParams,
}

impl HilbertHeatmapLayer {
    pub fn new(view_params: ViewParams, layer_params: HilbertHeatmapLayerParams) -> Self {
        let num_items = layer_params.values.len();
        for criteria in layer_params.filtering_criteria.iter().chain(&layer_params.selection_criteria) {
            criteria.validate_len(num_items);
        }
        Self { view_params, layer_params }
    }

    pub fn grid(&self) -> HilbertGrid {
        HilbertGrid::new(&self.view_params, self.layer_params.bounds.as_ref(), self.layer_params.cell_size)
    }

    fn colormap(&self) -> HeatmapColormap {
        match self.layer_params.colormap.clone().unwrap_or_default() {
            HeatmapColormap::Quantitative(mut params) => {
                params.axis_domains = None;
                HeatmapColormap::Quantitative(params)
            }
            other => other,
        }
    }

    fn is_categorical(&self) -> bool {
        !matches!(self.layer_params.colormap, None | Some(HeatmapColormap::Quantitative(_)))
    }

    /// The curve position, ordinal and index of the item drawn at `cell`, unless the cell is empty.
    fn item_at(&self, grid: &HilbertGrid, cell: (u32, u32), is_categorical: bool) -> Option<(u32, usize, usize)> {
        let p = &self.layer_params;
        let (position, ordinal) = grid.curve_position_and_ordinal(cell, p.ordered_indices.len());
        let index = index_at(&p.ordered_indices, ordinal);
        if !cpu_is_included(&p.filtering_criteria, index) {
            return None;
        }
        let value = p.values.get_f32(index);
        if value.is_nan() || (is_categorical && value < 0.0) {
            return None;
        }
        Some((position, ordinal, index))
    }

    fn is_drawable(&self) -> bool {
        !self.layer_params.ordered_indices.is_empty() && !self.grid().is_empty()
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for HilbertHeatmapLayer {
    async fn prepare(&mut self, _gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        PrepareResult { bailed_early: false }
    }
}

#[derive(ShaderType, Debug)]
struct HilbertHeatmapLayerUniforms {
    grid_origin: Vec2,
    cell_size: f32,
    grid_width: u32,
    grid_height: u32,
    num_items: u32,
    is_categorical: u32,
    reverse: u32,
    domain: Vec2,
    opacity: f32,
    background_opacity: f32,
}

const ORDERED_INDICES_BINDING: u32 = 1;
const ITEM_VALUES_BINDING: u32 = 2;
const PALETTE_BINDING: u32 = 3;
const FIRST_CRITERIA_BINDING: u32 = 4;

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for HilbertHeatmapLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut GpuRenderPass<'_>) {
        if !self.is_drawable() {
            return;
        }
        let GpuContext { device, queue } = gpu_context;
        let p = &self.layer_params;
        let grid = self.grid();
        let colormap = self.colormap();
        let ((domain_lo, domain_hi), reverse) = colormap.quantitative_domain_and_reverse();

        let (ordered_indices_view, ordered_indices_dtype) = p.ordered_indices.create_data_texture(device, queue, "Hilbert Heatmap Ordered Indices Texture");
        let (values_view, values_dtype) = p.values.create_data_texture(device, queue, "Hilbert Heatmap Values Texture");
        let palette_view = colormap.palette().map(|colors| create_palette_texture(device, queue, &colors));
        let filtering = prepare_emphasis_criteria(device, queue, &p.filtering_criteria, "is_filtered_in", "filter_data", FIRST_CRITERIA_BINDING);
        let selection = prepare_emphasis_criteria(
            device, queue, &p.selection_criteria, "is_selected_in", "select_data",
            FIRST_CRITERIA_BINDING + filtering.textures.len() as u32,
        );

        let (cell_color_wgsl, cell_domain_wgsl, colormap_fn_source, colormap_fn_name) = match &colormap {
            HeatmapColormap::Quantitative(params) => {
                let (source, name) = wgsl_colormaps::wgsl_source_and_name(params.colormap);
                (heatmap_cell_color::QUANTITATIVE, heatmap_cell_color::DOMAIN_UNIFORM, source, name)
            }
            _ => (heatmap_cell_color::CATEGORICAL, "", "", ""),
        };
        let shader_source = ShaderBuilder::new(include_str!("shaders/hilbert_heatmap_layer.wgsl"))
            .inject_function("flat_texel_coord", common::FLAT_TEXEL_COORD)
            .inject_function("gilbert_xy2d", common::GILBERT_XY2D)
            .define_bidx("ordered_indices", ORDERED_INDICES_BINDING)
            .inject_texture_sample_type("ordered_indices", ordered_indices_dtype)
            .define_bidx("item_values", ITEM_VALUES_BINDING)
            .inject_texture_sample_type("item_values", values_dtype)
            .inject_function("colormap_fn_source", colormap_fn_source)
            .inject_function("cell_domain", cell_domain_wgsl)
            .inject_function("cell_color", cell_color_wgsl)
            .define("colormap_fn_name", colormap_fn_name)
            .define_bidx("palette", PALETTE_BINDING)
            .inject_function("filtering", &filtering.wgsl)
            .inject_function("selection", &selection.wgsl)
            .build();
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("hilbert_heatmap_layer.wgsl"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        let texture_entry = |binding: u32, sample_type: wgpu::TextureSampleType| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let mut layout_entries = vec![
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            texture_entry(ORDERED_INDICES_BINDING, ordered_indices_dtype.binding_sample_type()),
            texture_entry(ITEM_VALUES_BINDING, values_dtype.binding_sample_type()),
        ];
        if palette_view.is_some() {
            layout_entries.push(texture_entry(PALETTE_BINDING, wgpu::TextureSampleType::Float { filterable: false }));
        }
        let criteria_textures: Vec<_> = filtering.textures.iter().chain(selection.textures.iter()).collect();
        for (i, texture) in criteria_textures.iter().enumerate() {
            layout_entries.push(texture_entry(FIRST_CRITERIA_BINDING + i as u32, texture.sample_type));
        }
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Hilbert Heatmap BGL"),
            entries: &layout_entries,
        });
        let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Hilbert Heatmap Pipeline Layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Hilbert Heatmap Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::SrcAlpha,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            cache: None,
            multiview_mask: None,
        });

        let uniforms = HilbertHeatmapLayerUniforms {
            grid_origin: Vec2::new(grid.margin_left, grid.margin_top),
            cell_size: grid.cell_size,
            grid_width: grid.width,
            grid_height: grid.height,
            num_items: p.ordered_indices.len() as u32,
            is_categorical: self.is_categorical() as u32,
            reverse: reverse as u32,
            domain: Vec2::new(domain_lo, domain_hi),
            opacity: p.opacity,
            background_opacity: p.background_opacity.unwrap_or(DEFAULT_BACKGROUND_OPACITY),
        };
        let mut uniform_buffer_contents = UniformBuffer::new(Vec::<u8>::new());
        uniform_buffer_contents.write(&uniforms).unwrap();
        let uniform_bytes = uniform_buffer_contents.into_inner();
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Hilbert Heatmap Uniforms"),
            size: uniform_bytes.len() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&uniform_buffer, 0, &uniform_bytes);

        let mut entries = vec![
            wgpu::BindGroupEntry { binding: 0, resource: uniform_buffer.as_entire_binding() },
            wgpu::BindGroupEntry { binding: ORDERED_INDICES_BINDING, resource: wgpu::BindingResource::TextureView(&ordered_indices_view) },
            wgpu::BindGroupEntry { binding: ITEM_VALUES_BINDING, resource: wgpu::BindingResource::TextureView(&values_view) },
        ];
        if let Some(palette_view) = &palette_view {
            entries.push(wgpu::BindGroupEntry { binding: PALETTE_BINDING, resource: wgpu::BindingResource::TextureView(palette_view) });
        }
        for (i, texture) in criteria_textures.iter().enumerate() {
            entries.push(wgpu::BindGroupEntry {
                binding: FIRST_CRITERIA_BINDING + i as u32,
                resource: wgpu::BindingResource::TextureView(&texture.view),
            });
        }
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Hilbert Heatmap BG"),
            layout: &bind_group_layout,
            entries: &entries,
        });

        let grid_width_px = grid.width as f32 * grid.cell_size;
        let grid_height_px = grid.height as f32 * grid.cell_size;
        pass.set_viewport(grid.margin_left, grid.margin_top, grid_width_px, grid_height_px, 0.0, 1.0);
        pass.set_scissor_rect(grid.margin_left as u32, grid.margin_top as u32, grid_width_px.ceil() as u32, grid_height_px.ceil() as u32);
        pass.set_pipeline(&render_pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..4, 0..1);
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for HilbertHeatmapLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for HilbertHeatmapLayer {
    async fn draw(&self, ctx: &mut SvgContext) {
        if !self.is_drawable() {
            return;
        }
        let p = &self.layer_params;
        let grid = self.grid();
        let colormap = self.colormap();
        let background_alpha = p.background_opacity.unwrap_or(DEFAULT_BACKGROUND_OPACITY);
        let is_categorical = self.is_categorical();

        let mut rgba = vec![0u8; grid.len() as usize * 4];
        for y in 0..grid.height {
            for x in 0..grid.width {
                let Some((_, _, item)) = self.item_at(&grid, (x, y), is_categorical) else { continue };
                let alpha = p.opacity * if cpu_is_included(&p.selection_criteria, item) { 1.0 } else { background_alpha };
                let pixel = ((y * grid.width + x) * 4) as usize;
                rgba[pixel..pixel + 3].copy_from_slice(&colormap.cpu_color(p.values.get_f32(item), item as u32, 0));
                rgba[pixel + 3] = (alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        }

        let mut png = Vec::new();
        PngEncoder::new(&mut png)
            .write_image(&rgba, grid.width, grid.height, ExtendedColorType::Rgba8)
            .expect("PNG encode");
        let width = (grid.width as f32 * grid.cell_size) as f64;
        let height = (grid.height as f32 * grid.cell_size) as f64;
        update_svg(ctx, &[TwoElement::Group(TwoGroup {
            elements: vec![TwoElement::Image(TwoImage {
                x: 0.0,
                y: 0.0,
                width,
                height,
                href: format!("data:image/png;base64,{}", BASE64_STANDARD.encode(&png)),
                opacity: 1.0,
                image_rendering_style: Some(TwoImageRenderingStyle::Pixelated),
            })],
            translate: Some((grid.margin_left as f64, grid.margin_top as f64)),
            layer_id: Some(p.layer_id.clone()),
            clip_rect: Some((0.0, 0.0, width, height)),
            ..Default::default()
        })]);
    }
}

inventory::submit! {
    crate::registry::LayerRegistration {
        layer_type_name: "HilbertHeatmapLayer",
        create_layer: |value, view_params| {
            let params: HilbertHeatmapLayerParams = serde_json::from_value(value).unwrap();
            Box::new(HilbertHeatmapLayer::new(view_params.clone(), params))
        },
    }
}

impl ExtentableLayer for HilbertHeatmapLayer {}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
pub struct HilbertHeatmapPickingInfo {
    /// The picked cell's position along the curve.
    pub curve_position: u32,
    /// The picked item's position in `ordered_indices`.
    pub ordinal: usize,
    /// The picked item, as an index into `values`.
    pub index: usize,
    pub value: NumericValue,
    pub is_selected: bool,
}

impl PickableLayer for HilbertHeatmapLayer {
    fn pick(&self, screen_coord: ScreenCoord, _data_coord: Option<DataCoord>) -> Option<LayerPickingResult> {
        if !self.is_drawable() {
            return None;
        }
        let grid = self.grid();
        let (curve_position, ordinal, index) = self.item_at(&grid, grid.cell_at(screen_coord)?, self.is_categorical())?;
        let p = &self.layer_params;
        Some(LayerPickingResult::new(&p.layer_id, HilbertHeatmapPickingInfo {
            curve_position,
            ordinal,
            index,
            value: p.values.value(index),
            is_selected: cpu_is_included(&p.selection_criteria, index),
        }))
    }
}
