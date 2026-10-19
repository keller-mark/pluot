// The heatmap layer should support either a quantitative or a categorical colormap.
// The color mapping functionality should be similar to the bitmap (for quantitative colormaps) and bitmask (for categorical colormaps) layers.
//
// There are multiple cases for how the X/Y axis categories may be selected and/or filtered:
// - None: consider all elements along this axis
// - Explicit identifiers: consider these specified entries along this axis
//   - Technically, the identifier columns are axis-aligned row/columns with the same number of elements (as below). We need to load this identifier column in order to identify the row/column indices within the matrix corresponding to the specified identifiers.
// - Axis-aligned row/column with the same number of elements (e.g., obs column of anndata object to filter the list of cells i.e. rows of the X matrix)
//   - Boolean mask: consider the True entries along this axis
//   - Categorical with list of categories
//   - Quantitative with min/max range
//
// Given an axis-aligned row/column and filtering/selection criteria, we can perform a mapping from the criteria to the list of included matrix row or column indices on either the GPU (via compute shader) or CPU. This will be similar to the reducer operations, except rather than the output being a summary or distribution, it will be a list of integer indices.
// Then, we will use this list of indices in order to load the matrix data (via loading subsets of the zarr array corresponding to the specified row/column indices - or all data along a given axis if no filtering criteria was provided).

// We will upload the heatmap matrix as a texture to the shader.
// Since the (filtered) matrix may be very long or wide, a single row or column of the matrix may need to wrap onto multiple rows of a texture. We will need a function to look up the texture index corresponding to the data item of interest.
// We will simply consider the 2D texture with shape texture_shape to be a 1D array with flat_texture_length, and we will also consider the 2D matrix with shape matrix_shape to be a 1D array with flat_matrix_length, and we will just wrap the flat matrix into the texture as needed. We will render multiple textures corresponding to multiple heatmap layers as needed when the matrix is larger than a single texture.

// Which operations should be performed in the base layer versus the zarr layer?
// - The base layer will be given the filtered matrix returned from the zarr array subset loading operation. The base layer will transform the matrix into the texture for rendering.
// - The base layer will NOT perform filtering or accept filtering criteria; the parent zarr layer will provide the pre-filtered matrix.
// - The base layer will be given an optional boolean mask of selection criteria for the rows/columns, with the row/column count of mask entries matching the filtered matrix row/column count. The base layer will accept a background_opacity parameter to use when coloring the selection-excluded items.
// - The zarr layer will load the axis-aligned rows/columns, use these to compute the filter-included matrix row/column indices, load the filter-included matrix subset array, and compute the filter-included selection boolean mask. The zarr layer will pass these things to the base layer to render.

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use encase::{ShaderType, UniformBuffer};
use glam::{DMat4, DVec4, Mat4, Vec2};
use image::{codecs::png::PngEncoder, ExtendedColorType, ImageEncoder};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use crate::color_mode::create_palette_texture;
use crate::colormaps_categorical;
use crate::colormaps_quantitative;
use crate::compute::matrix_axis_extents::MatrixAxis;
use crate::emphasis_mode::{cpu_is_included, prepare_emphasis_criteria};
use crate::numeric_data::{upload_data_texture, NumericData};
use crate::picking::LayerPickingResult;
use crate::positioning::{get_point_position, get_point_size};
use crate::render_traits::{
    AspectRatioAlignmentMode, AspectRatioMode, BooleanCriteriaParams, BrushableLayer, CategoricalColormap,
    DrawToRasterCpu, DrawToRasterGpu, DrawToSvg, EmphasisCriteria, ExtentableLayer, MarginParams, PickableLayer,
    PreparedLayer, QuantitativeColormap, UnitsMode, ViewParams,
};
use crate::render_types::{CpuContext, CpuRenderPass, GpuContext, PrepareResult};
use crate::shader_modules::{colormaps as wgsl_colormaps, common, heatmap_cell_color, matrix_value, ShaderBuilder, TextureDtype};
use crate::two::shapes::{TwoElement, TwoGroup, TwoImage, TwoImageRenderingStyle};
use crate::two::svg::{update_svg, SvgContext};
use crate::viewport::{DataCoord, ScreenCoord};
use crate::wgpu;

/// Opacity multiplier for selection-excluded cells when
/// [`HeatmapLayerParams::background_opacity`] is `None`.
pub const DEFAULT_BACKGROUND_OPACITY: f32 = 0.2;

const IDENTITY_MATRIX: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0,
    0.0, 1.0, 0.0, 0.0,
    0.0, 0.0, 1.0, 0.0,
    0.0, 0.0, 0.0, 1.0,
];

/// Parameters for [`HeatmapColormap::Quantitative`].
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HeatmapQuantitativeColormapParams {
    pub colormap: QuantitativeColormap,
    #[serde(default)]
    pub reverse: bool,
    /// (min, max) normalization domain, defaulting to (0.0, 1.0). Never derived
    /// from the matrix values, since that would cost a CPU pass over them.
    #[serde(default)]
    pub domain: Option<(f32, f32)>,
    /// One (min, max) domain per row or per column, taking precedence over `domain`.
    #[serde(default)]
    pub axis_domains: Option<HeatmapAxisDomains>,
}

/// One (min, max) normalization domain per row or per column of the matrix.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HeatmapAxisDomains {
    pub axis: MatrixAxis,
    /// One lower bound per entry along `axis`.
    pub min: NumericData,
    /// One upper bound per entry along `axis`.
    pub max: NumericData,
}

/// How to map each matrix value to a color.
///
/// Serialized as an adjacently-tagged enum, e.g.
/// `{"colormap_mode": "Categorical", "colormap_params": "Tableau10"}`.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "colormap_mode", content = "colormap_params")]
pub enum HeatmapColormap {
    /// Values are scalars, normalized against a domain.
    Quantitative(HeatmapQuantitativeColormapParams),
    /// Values are integer category codes into a named palette.
    Categorical(CategoricalColormap),
    /// Values are integer category codes into a list of (r, g, b) colors.
    CategoricalCustom(Vec<(u8, u8, u8)>),
}

impl Default for HeatmapColormap {
    fn default() -> Self {
        HeatmapColormap::Quantitative(HeatmapQuantitativeColormapParams {
            colormap: QuantitativeColormap::Viridis,
            reverse: false,
            domain: None,
            axis_domains: None,
        })
    }
}

impl HeatmapColormap {
    fn palette(&self) -> Option<Vec<[f32; 4]>> {
        match self {
            HeatmapColormap::Quantitative(_) => None,
            HeatmapColormap::Categorical(colormap) => Some(colormaps_categorical::palette(*colormap).to_vec()),
            HeatmapColormap::CategoricalCustom(colors) => Some(
                colors.iter().map(|(r, g, b)| [*r as f32 / 255.0, *g as f32 / 255.0, *b as f32 / 255.0, 1.0]).collect(),
            ),
        }
    }

    fn quantitative_domain_and_reverse(&self) -> ((f32, f32), bool) {
        match self {
            HeatmapColormap::Quantitative(params) => (params.domain.unwrap_or((0.0, 1.0)), params.reverse),
            _ => ((0.0, 1.0), false),
        }
    }

    fn axis_domains(&self) -> Option<&HeatmapAxisDomains> {
        match self {
            HeatmapColormap::Quantitative(params) => params.axis_domains.as_ref(),
            _ => None,
        }
    }

    /// CPU mirror of the `get_cell_color` WGSL snippets.
    pub fn cpu_color(&self, value: f32, row: u32, col: u32) -> [u8; 3] {
        let to_u8 = |c: f32| (c * 255.0).round().clamp(0.0, 255.0) as u8;
        let rgba = match self {
            HeatmapColormap::Quantitative(params) => {
                let ((mut lo, mut hi), reverse) = self.quantitative_domain_and_reverse();
                if let Some(domains) = &params.axis_domains {
                    let i = if domains.axis == MatrixAxis::Rows { row } else { col } as usize;
                    (lo, hi) = (domains.min.get_f32(i), domains.max.get_f32(i));
                }
                let x = ((value - lo) / (hi - lo).max(1e-20)).clamp(0.0, 1.0);
                colormaps_quantitative::sample(params.colormap, if reverse { 1.0 - x } else { x })
            }
            _ => {
                let palette = self.palette().unwrap_or_default();
                if palette.is_empty() {
                    return [0, 0, 0];
                }
                palette[(value as i64).rem_euclid(palette.len() as i64) as usize]
            }
        };
        [to_u8(rgba[0]), to_u8(rgba[1]), to_u8(rgba[2])]
    }
}

/// Layer params struct for [`HeatmapLayer`].
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct HeatmapLayerParams {
    pub layer_id: String,
    pub bounds: Option<MarginParams>,
    pub data_unit_mode_x: UnitsMode,
    pub data_unit_mode_y: UnitsMode,

    /// (x, y) translation in cell units, applied before `model_matrix`.
    pub cell_offset: Option<(f32, f32)>,
    /// Column-major 4x4 matrix applied to cell-unit positions, where each cell is 1x1.
    pub model_matrix: Option<[f32; 16]>,

    pub num_rows: u32,
    pub num_cols: u32,
    /// Row-major matrix values, of length `num_rows * num_cols`.
    pub data: NumericData,

    /// When false, the first row is drawn at the top and the first column at
    /// the left. When true, the matrix is drawn transposed: the first row at
    /// the left and the first column at the top.
    pub swap_axes: bool,

    pub colormap: Option<HeatmapColormap>,

    /// One entry per row (1 = selected, 0 = not). `None` selects every row.
    pub row_selection: Option<NumericData>,
    /// One entry per column (1 = selected, 0 = not). `None` selects every column.
    pub col_selection: Option<NumericData>,
    /// Opacity multiplier for cells whose row or column is not selected.
    pub background_opacity: Option<f32>,

    pub opacity: f32,
}

impl Default for HeatmapLayerParams {
    fn default() -> Self {
        Self {
            layer_id: "".to_string(),
            bounds: None,
            data_unit_mode_x: UnitsMode::Data,
            data_unit_mode_y: UnitsMode::Data,
            cell_offset: None,
            model_matrix: None,
            num_rows: 0,
            num_cols: 0,
            data: NumericData::Float32(Arc::new(vec![])),
            swap_axes: false,
            colormap: None,
            row_selection: None,
            col_selection: None,
            background_opacity: None,
            opacity: 1.0,
        }
    }
}

impl HeatmapLayerParams {
    pub(crate) fn common(&self) -> HeatmapCommon<'_> {
        HeatmapCommon {
            layer_id: &self.layer_id,
            bounds: self.bounds.as_ref(),
            data_unit_mode_x: self.data_unit_mode_x,
            data_unit_mode_y: self.data_unit_mode_y,
            cell_offset: self.cell_offset,
            model_matrix: self.model_matrix,
            num_rows: self.num_rows,
            num_cols: self.num_cols,
            swap_axes: self.swap_axes,
            colormap: self.colormap.as_ref(),
            row_selection: self.row_selection.as_ref(),
            col_selection: self.col_selection.as_ref(),
            background_opacity: self.background_opacity,
            opacity: self.opacity,
        }
    }
}

/// The params shared by every heatmap layer, whatever the storage layout of
/// its matrix values. See [`HeatmapLayerParams`] for what each one means.
pub(crate) struct HeatmapCommon<'a> {
    pub layer_id: &'a str,
    pub bounds: Option<&'a MarginParams>,
    pub data_unit_mode_x: UnitsMode,
    pub data_unit_mode_y: UnitsMode,
    pub cell_offset: Option<(f32, f32)>,
    pub model_matrix: Option<[f32; 16]>,
    pub num_rows: u32,
    pub num_cols: u32,
    pub swap_axes: bool,
    pub colormap: Option<&'a HeatmapColormap>,
    pub row_selection: Option<&'a NumericData>,
    pub col_selection: Option<&'a NumericData>,
    pub background_opacity: Option<f32>,
    pub opacity: f32,
}

/// The matrix values that one draw call of [`HeatmapCommon::draw_gpu`]
/// reads, covering rows `first_row..first_row + num_rows`.
pub(crate) struct HeatmapValueBlock {
    pub first_row: u32,
    pub num_rows: u32,
    /// Bound consecutively from [`MATRIX_VALUE_FIRST_BINDING`].
    pub textures: Vec<(wgpu::TextureView, TextureDtype)>,
}

impl HeatmapCommon<'_> {
    /// Width and height of the whole matrix, in cell units.
    fn display_size(&self) -> (u32, u32) {
        if self.swap_axes { (self.num_rows, self.num_cols) } else { (self.num_cols, self.num_rows) }
    }

    /// Bottom-left corner and size, in cell units, of the quad covering rows
    /// `first_row..first_row + num_rows`.
    fn row_block_quad(&self, first_row: u32, num_rows: u32) -> (Vec2, Vec2) {
        let (offset_x, offset_y) = self.cell_offset.unwrap_or((0.0, 0.0));
        if self.swap_axes {
            (Vec2::new(offset_x + first_row as f32, offset_y), Vec2::new(num_rows as f32, self.num_cols as f32))
        } else {
            let rows_below = (self.num_rows - first_row - num_rows) as f32;
            (Vec2::new(offset_x, offset_y + rows_below), Vec2::new(self.num_cols as f32, num_rows as f32))
        }
    }

    /// The (row, col) drawn at display cell `(x, y)`, with `y` counted from the top.
    fn cell_at_display(&self, x: u32, y_from_top: u32) -> (u32, u32) {
        if self.swap_axes { (x, y_from_top) } else { (y_from_top, x) }
    }

    fn row_selection_criteria(&self) -> Vec<EmphasisCriteria> {
        mask_as_criteria(self.row_selection)
    }

    fn col_selection_criteria(&self) -> Vec<EmphasisCriteria> {
        mask_as_criteria(self.col_selection)
    }

    /// Panics if a selection mask or per-axis domain does not match the matrix shape.
    pub(crate) fn validate(&self) {
        for criteria in self.row_selection_criteria() {
            criteria.validate_len(self.num_rows as usize);
        }
        for criteria in self.col_selection_criteria() {
            criteria.validate_len(self.num_cols as usize);
        }
        if let Some(domains) = self.colormap.and_then(HeatmapColormap::axis_domains) {
            let expected_len = if domains.axis == MatrixAxis::Rows { self.num_rows } else { self.num_cols } as usize;
            assert!(
                domains.min.len() == expected_len && domains.max.len() == expected_len,
                "Heatmap {:?} domains have lengths ({}, {}) but the matrix has {expected_len} {:?}",
                domains.axis, domains.min.len(), domains.max.len(), domains.axis,
            );
        }
    }

    /// (margin_left, margin_top, layer_w, layer_h)
    fn layer_rect(&self, view_params: &ViewParams) -> (f32, f32, f32, f32) {
        let bounds = self.bounds.or(view_params.margins.as_ref());
        let margin = |f: fn(&MarginParams) -> Option<f32>| bounds.and_then(f).unwrap_or(0.0);
        let (left, right) = (margin(|m| m.margin_left), margin(|m| m.margin_right));
        let (top, bottom) = (margin(|m| m.margin_top), margin(|m| m.margin_bottom));
        (
            left,
            top,
            view_params.width as f32 - (left + right),
            view_params.height as f32 - (top + bottom),
        )
    }

    /// Draws one quad per block, where `matrix_value_wgsl` defines
    /// `get_matrix_value(local_row, col)` over each block's textures.
    pub(crate) fn draw_gpu(
        &self,
        view_params: &ViewParams,
        gpu_context: &GpuContext<'_>,
        pass: &mut wgpu::RenderPass,
        matrix_value_wgsl: &str,
        blocks: &[HeatmapValueBlock],
    ) {
        let GpuContext { device, queue } = gpu_context;
        let Some(first_block) = blocks.first() else {
            return;
        };

        let colormap = self.colormap.cloned().unwrap_or_default();
        let ((domain_lo, domain_hi), reverse) = colormap.quantitative_domain_and_reverse();

        let palette_view = colormap.palette().map(|colors| create_palette_texture(device, queue, &colors));
        let domain_textures = colormap.axis_domains().map(|domains| {
            (
                domains.min.create_data_texture(device, queue, "Heatmap Domain Min Texture"),
                domains.max.create_data_texture(device, queue, "Heatmap Domain Max Texture"),
            )
        });

        let row_selection = prepare_emphasis_criteria(
            device, queue, &self.row_selection_criteria(), "is_row_selected", "row_selection", FIRST_SELECTION_BINDING,
        );
        let col_selection = prepare_emphasis_criteria(
            device, queue, &self.col_selection_criteria(), "is_col_selected", "col_selection",
            FIRST_SELECTION_BINDING + row_selection.textures.len() as u32,
        );

        let (cell_color_wgsl, colormap_fn_source, colormap_fn_name) = match &colormap {
            HeatmapColormap::Quantitative(params) => {
                let (source, name) = wgsl_colormaps::wgsl_source_and_name(params.colormap);
                (heatmap_cell_color::QUANTITATIVE, source, name)
            }
            _ => (heatmap_cell_color::CATEGORICAL, "", ""),
        };
        let cell_domain_wgsl = match (&colormap, colormap.axis_domains(), &domain_textures) {
            (HeatmapColormap::Quantitative(_), Some(domains), Some(((_, min_dtype), (_, max_dtype)))) => {
                ShaderBuilder::new(heatmap_cell_color::DOMAIN_PER_AXIS)
                    .define_bidx("domain_min", DOMAIN_MIN_BINDING)
                    .define_bidx("domain_max", DOMAIN_MAX_BINDING)
                    .inject_texture_sample_type("domain_min", *min_dtype)
                    .inject_texture_sample_type("domain_max", *max_dtype)
                    .define("domain_index", if domains.axis == MatrixAxis::Rows { "row" } else { "col" })
                    .build()
            }
            (HeatmapColormap::Quantitative(_), _, _) => heatmap_cell_color::DOMAIN_UNIFORM.to_string(),
            _ => String::new(),
        };
        let shader_source = ShaderBuilder::new(include_str!("shaders/heatmap_layer.wgsl"))
            .inject_function("scale", common::SCALE)
            .inject_function("translate", common::TRANSLATE)
            .inject_function("get_aspect_ratio_mat", common::GET_ASPECT_RATIO_MAT)
            .inject_function("flat_texel_coord", common::FLAT_TEXEL_COORD)
            .inject_function("matrix_value", matrix_value_wgsl)
            .inject_function("colormap_fn_source", colormap_fn_source)
            .inject_function("cell_domain", &cell_domain_wgsl)
            .inject_function("cell_color", cell_color_wgsl)
            .define("colormap_fn_name", colormap_fn_name)
            .define_bidx("palette", PALETTE_BINDING)
            .inject_function("row_selection", &row_selection.wgsl)
            .inject_function("col_selection", &col_selection.wgsl)
            .build();
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("heatmap_layer.wgsl"),
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
        let mut layout_entries = vec![wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }];
        for (i, (_, dtype)) in first_block.textures.iter().enumerate() {
            layout_entries.push(texture_entry(MATRIX_VALUE_FIRST_BINDING + i as u32, dtype.binding_sample_type()));
        }
        if palette_view.is_some() {
            layout_entries.push(texture_entry(PALETTE_BINDING, wgpu::TextureSampleType::Float { filterable: false }));
        }
        if let Some(((_, min_dtype), (_, max_dtype))) = &domain_textures {
            layout_entries.push(texture_entry(DOMAIN_MIN_BINDING, min_dtype.binding_sample_type()));
            layout_entries.push(texture_entry(DOMAIN_MAX_BINDING, max_dtype.binding_sample_type()));
        }
        let selection_textures: Vec<_> = row_selection.textures.iter().chain(col_selection.textures.iter()).collect();
        for (i, texture) in selection_textures.iter().enumerate() {
            layout_entries.push(texture_entry(FIRST_SELECTION_BINDING + i as u32, texture.sample_type));
        }
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Heatmap BGL"),
            entries: &layout_entries,
        });

        let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Heatmap Pipeline Layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Heatmap Render Pipeline"),
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

        let (margin_left, margin_top, layer_w, layer_h) = self.layer_rect(view_params);
        pass.set_viewport(margin_left, margin_top, layer_w, layer_h, 0.0, 1.0);
        pass.set_scissor_rect(margin_left as u32, margin_top as u32, layer_w as u32, layer_h as u32);
        pass.set_pipeline(&render_pipeline);

        let camera_view = view_params.camera_view.unwrap_or(IDENTITY_MATRIX);
        for block in blocks {
            let (quad_origin, quad_size) = self.row_block_quad(block.first_row, block.num_rows);

            let uniforms = HeatmapLayerUniforms {
                layer_size: Vec2::new(layer_w, layer_h),
                camera_view: Mat4::from_cols_array(&camera_view),
                data_unit_mode_x: units_mode_u32(self.data_unit_mode_x),
                data_unit_mode_y: units_mode_u32(self.data_unit_mode_y),
                aspect_ratio_mode: match view_params.aspect_ratio_mode {
                    AspectRatioMode::Ignore => 0,
                    AspectRatioMode::Contain => 1,
                    AspectRatioMode::Cover => 2,
                },
                aspect_ratio_alignment_mode: match view_params.aspect_ratio_alignment_mode {
                    AspectRatioAlignmentMode::Center => 0,
                    AspectRatioAlignmentMode::Start => 1,
                    AspectRatioAlignmentMode::End => 2,
                },
                model_matrix: Mat4::from_cols_array(&self.model_matrix.unwrap_or(IDENTITY_MATRIX)),
                quad_origin,
                quad_size,
                num_cols: self.num_cols,
                block_first_row: block.first_row,
                block_num_rows: block.num_rows,
                swap_axes: self.swap_axes as u32,
                domain: Vec2::new(domain_lo, domain_hi),
                reverse: reverse as u32,
                opacity: self.opacity,
                background_opacity: self.background_opacity.unwrap_or(DEFAULT_BACKGROUND_OPACITY),
            };
            let mut uniform_buffer_contents = UniformBuffer::new(Vec::<u8>::new());
            uniform_buffer_contents.write(&uniforms).unwrap();
            let uniform_bytes = uniform_buffer_contents.into_inner();
            let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Heatmap Uniforms"),
                size: uniform_bytes.len() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            queue.write_buffer(&uniform_buffer, 0, &uniform_bytes);

            let mut entries = vec![wgpu::BindGroupEntry { binding: 0, resource: uniform_buffer.as_entire_binding() }];
            for (i, (view, _)) in block.textures.iter().enumerate() {
                entries.push(wgpu::BindGroupEntry {
                    binding: MATRIX_VALUE_FIRST_BINDING + i as u32,
                    resource: wgpu::BindingResource::TextureView(view),
                });
            }
            if let Some(palette_view) = &palette_view {
                entries.push(wgpu::BindGroupEntry { binding: PALETTE_BINDING, resource: wgpu::BindingResource::TextureView(palette_view) });
            }
            if let Some(((min_view, _), (max_view, _))) = &domain_textures {
                entries.push(wgpu::BindGroupEntry { binding: DOMAIN_MIN_BINDING, resource: wgpu::BindingResource::TextureView(min_view) });
                entries.push(wgpu::BindGroupEntry { binding: DOMAIN_MAX_BINDING, resource: wgpu::BindingResource::TextureView(max_view) });
            }
            for (i, texture) in selection_textures.iter().enumerate() {
                entries.push(wgpu::BindGroupEntry {
                    binding: FIRST_SELECTION_BINDING + i as u32,
                    resource: wgpu::BindingResource::TextureView(&texture.view),
                });
            }
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Heatmap BG"),
                layout: &bind_group_layout,
                entries: &entries,
            });

            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..4, 0..1);
        }
    }

    /// Draws the matrix as one pixelated image, reading each cell's value
    /// (NaN for none) from `value_at(row, col)`.
    pub(crate) async fn draw_svg(&self, view_params: &ViewParams, ctx: &mut SvgContext, value_at: impl Fn(u32, u32) -> f32) {
        let (img_w, img_h) = self.display_size();
        if img_w == 0 || img_h == 0 {
            return;
        }

        let colormap = self.colormap.cloned().unwrap_or_default();
        let row_selection = self.row_selection_criteria();
        let col_selection = self.col_selection_criteria();
        let background_alpha = self.background_opacity.unwrap_or(DEFAULT_BACKGROUND_OPACITY);

        let mut rgba = vec![0u8; (img_w * img_h * 4) as usize];
        for y in 0..img_h {
            for x in 0..img_w {
                let (row, col) = self.cell_at_display(x, y);
                let value = value_at(row, col);
                if value.is_nan() {
                    continue;
                }
                let is_selected = cpu_is_included(&row_selection, row as usize) && cpu_is_included(&col_selection, col as usize);
                let alpha = if is_selected { 1.0 } else { background_alpha };
                let pixel = ((y * img_w + x) * 4) as usize;
                rgba[pixel..pixel + 3].copy_from_slice(&colormap.cpu_color(value, row, col));
                rgba[pixel + 3] = (alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        }

        let mut png = Vec::new();
        PngEncoder::new(&mut png)
            .write_image(&rgba, img_w, img_h, ExtendedColorType::Rgba8)
            .expect("PNG encode");
        let href = format!("data:image/png;base64,{}", BASE64_STANDARD.encode(&png));

        let camera_view = view_params.camera_view.unwrap_or(IDENTITY_MATRIX);
        let model_matrix = self.model_matrix.unwrap_or(IDENTITY_MATRIX);
        let (margin_left, margin_top, layer_w, layer_h) = self.layer_rect(view_params);
        let (offset_x, offset_y) = self.cell_offset.unwrap_or((0.0, 0.0));
        let (px, py) = get_point_position(
            offset_x, offset_y, layer_w, layer_h, &camera_view,
            self.data_unit_mode_x, self.data_unit_mode_y,
            view_params.aspect_ratio_mode, view_params.aspect_ratio_alignment_mode,
            Some(&model_matrix),
        );
        let (sw, sh) = get_point_size(
            img_w as f32, img_h as f32, layer_w, layer_h, &camera_view,
            self.data_unit_mode_x, self.data_unit_mode_y,
            view_params.aspect_ratio_mode, view_params.aspect_ratio_alignment_mode,
            Some(&model_matrix),
        );

        let image = TwoElement::Group(TwoGroup {
            elements: vec![TwoElement::Image(TwoImage {
                x: 0.0,
                y: 0.0,
                width: img_w as f64,
                height: img_h as f64,
                href,
                opacity: self.opacity as f64,
                image_rendering_style: Some(TwoImageRenderingStyle::Pixelated),
            })],
            translate: Some((px as f64, (layer_h - py - sh) as f64)),
            scale: Some((sw as f64 / img_w as f64, sh as f64 / img_h as f64)),
            ..Default::default()
        });

        update_svg(ctx, &[TwoElement::Group(TwoGroup {
            elements: vec![image],
            translate: Some((margin_left as f64, margin_top as f64)),
            layer_id: Some(self.layer_id.to_string()),
            clip_rect: Some((0.0, 0.0, layer_w as f64, layer_h as f64)),
            ..Default::default()
        })]);
    }

    /// The (row, col) of the cell at `data_coord`, if any.
    pub(crate) fn pick_cell(&self, data_coord: Option<DataCoord>) -> Option<(u32, u32)> {
        let DataCoord::TwoD { x, y } = data_coord? else {
            return None;
        };
        if self.data_unit_mode_x != UnitsMode::Data || self.data_unit_mode_y != UnitsMode::Data {
            return None;
        }

        let model_matrix = DMat4::from_cols_array(&self.model_matrix.unwrap_or(IDENTITY_MATRIX).map(|v| v as f64));
        if model_matrix.determinant() == 0.0 {
            return None;
        }
        let local = model_matrix.inverse() * DVec4::new(x as f64, y as f64, 0.0, 1.0);
        let (offset_x, offset_y) = self.cell_offset.unwrap_or((0.0, 0.0));
        let (cell_x, cell_y) = (local.x - offset_x as f64, local.y - offset_y as f64);
        let (img_w, img_h) = self.display_size();
        if cell_x < 0.0 || cell_x >= img_w as f64 || cell_y <= 0.0 || cell_y > img_h as f64 {
            return None;
        }
        Some(self.cell_at_display(cell_x.floor() as u32, (img_h as f64 - cell_y).floor() as u32))
    }

    /// The picking result for the cell at (`row`, `col`) holding `value`.
    pub(crate) fn picking_result(&self, row: u32, col: u32, value: String) -> LayerPickingResult {
        let mut info = HashMap::new();
        info.insert("row".to_string(), row.to_string());
        info.insert("col".to_string(), col.to_string());
        info.insert("value".to_string(), value);
        LayerPickingResult { layer_id: self.layer_id.to_string(), info }
    }
}

fn mask_as_criteria(mask: Option<&NumericData>) -> Vec<EmphasisCriteria> {
    mask.map(|mask| {
        EmphasisCriteria::Boolean(BooleanCriteriaParams { mask: mask.clone() })
    })
    .into_iter()
    .collect()
}

/// How many whole matrix rows fit in one texture of at most `max_texels` texels.
fn rows_per_texture(num_cols: u32, max_texels: u64) -> u32 {
    (max_texels / num_cols.max(1) as u64).clamp(1, u32::MAX as u64) as u32
}

fn units_mode_u32(mode: UnitsMode) -> u32 {
    match mode {
        UnitsMode::Pixels => 0,
        UnitsMode::Data => 1,
        UnitsMode::Normalized => 2,
    }
}

pub struct HeatmapLayer {
    view_params: ViewParams,
    layer_params: HeatmapLayerParams,
}

impl HeatmapLayer {
    pub fn new(view_params: ViewParams, layer_params: HeatmapLayerParams) -> Self {
        let expected_len = layer_params.num_rows as usize * layer_params.num_cols as usize;
        assert_eq!(
            layer_params.data.len(), expected_len,
            "HeatmapLayer data has length {} but num_rows * num_cols is {expected_len}",
            layer_params.data.len(),
        );
        layer_params.common().validate();
        Self { view_params, layer_params }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for HeatmapLayer {
    async fn prepare(&mut self, _gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        PrepareResult { bailed_early: false }
    }
}

#[derive(ShaderType, Debug)]
struct HeatmapLayerUniforms {
    layer_size: Vec2,
    camera_view: Mat4,
    data_unit_mode_x: u32,
    data_unit_mode_y: u32,
    aspect_ratio_mode: u32,
    aspect_ratio_alignment_mode: u32,
    model_matrix: Mat4,
    quad_origin: Vec2,
    quad_size: Vec2,
    num_cols: u32,
    block_first_row: u32,
    block_num_rows: u32,
    swap_axes: u32,
    domain: Vec2,
    reverse: u32,
    opacity: f32,
    background_opacity: f32,
}

/// The first of the (up to four, for a sparse matrix) bindings of a heatmap's matrix value textures.
pub(crate) const MATRIX_VALUE_FIRST_BINDING: u32 = 1;
const PALETTE_BINDING: u32 = 5;
const DOMAIN_MIN_BINDING: u32 = 6;
const DOMAIN_MAX_BINDING: u32 = 7;
const FIRST_SELECTION_BINDING: u32 = 8;

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for HeatmapLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        let GpuContext { device, queue } = gpu_context;
        let layer_params = &self.layer_params;
        if layer_params.num_rows == 0 || layer_params.num_cols == 0 {
            return;
        }

        let (data_bytes, data_dtype) = layer_params.data.as_texture_data();
        let max_dim = device.limits().max_texture_dimension_2d as u64;
        let block_rows = rows_per_texture(layer_params.num_cols, max_dim * max_dim);
        let bytes_per_row = layer_params.num_cols as usize * data_dtype.bytes_per_texel() as usize;
        let blocks: Vec<HeatmapValueBlock> = (0..layer_params.num_rows)
            .step_by(block_rows as usize)
            .map(|first_row| {
                let num_rows = block_rows.min(layer_params.num_rows - first_row);
                let block_bytes = &data_bytes[first_row as usize * bytes_per_row..(first_row + num_rows) as usize * bytes_per_row];
                let view = upload_data_texture(device, queue, block_bytes, data_dtype, "Heatmap Matrix Texture");
                HeatmapValueBlock { first_row, num_rows, textures: vec![(view, data_dtype)] }
            })
            .collect();

        let matrix_value_wgsl = ShaderBuilder::new(matrix_value::DENSE)
            .define_bidx("matrix_data", MATRIX_VALUE_FIRST_BINDING)
            .inject_texture_sample_type("matrix_data", data_dtype)
            .define("num_cols", "u.num_cols")
            .build();
        layer_params.common().draw_gpu(&self.view_params, gpu_context, pass, &matrix_value_wgsl, &blocks);
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for HeatmapLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for HeatmapLayer {
    async fn draw(&self, ctx: &mut SvgContext) {
        let layer_params = &self.layer_params;
        let value_at = |row: u32, col: u32| layer_params.data.get_f32((row * layer_params.num_cols + col) as usize);
        layer_params.common().draw_svg(&self.view_params, ctx, value_at).await;
    }
}

inventory::submit! {
    crate::registry::LayerRegistration {
        layer_type_name: "HeatmapLayer",
        create_layer: |value, view_params| {
            let params: HeatmapLayerParams = serde_json::from_value(value).unwrap();
            Box::new(HeatmapLayer::new(view_params.clone(), params))
        },
    }
}

impl BrushableLayer for HeatmapLayer {}

impl ExtentableLayer for HeatmapLayer {}

impl PickableLayer for HeatmapLayer {
    /// Returns the picked cell's "row", "col" and "value".
    fn pick(&self, _screen_coord: ScreenCoord, data_coord: Option<DataCoord>) -> Option<LayerPickingResult> {
        let common = self.layer_params.common();
        let (row, col) = common.pick_cell(data_coord)?;
        let value = self.layer_params.data.format_element((row * self.layer_params.num_cols + col) as usize);
        Some(common.picking_result(row, col, value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params_2x3(swap_axes: bool) -> HeatmapLayerParams {
        HeatmapLayerParams {
            num_rows: 2,
            num_cols: 3,
            data: NumericData::Float32(Arc::new(vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0])),
            swap_axes,
            ..Default::default()
        }
    }

    #[test]
    fn row_blocks_stack_top_to_bottom() {
        let params = HeatmapLayerParams { num_rows: 5, ..params_2x3(false) };
        assert_eq!(params.common().row_block_quad(0, 2), (Vec2::new(0.0, 3.0), Vec2::new(3.0, 2.0)));
        assert_eq!(params.common().row_block_quad(2, 3), (Vec2::new(0.0, 0.0), Vec2::new(3.0, 3.0)));
    }

    #[test]
    fn swapped_row_blocks_run_left_to_right() {
        let params = HeatmapLayerParams { num_rows: 5, ..params_2x3(true) };
        assert_eq!(params.common().row_block_quad(2, 3), (Vec2::new(2.0, 0.0), Vec2::new(3.0, 3.0)));
    }

    #[test]
    fn rows_per_texture_keeps_rows_whole() {
        assert_eq!(rows_per_texture(3, 10), 3);
        assert_eq!(rows_per_texture(20, 10), 1);
    }
}
