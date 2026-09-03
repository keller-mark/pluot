// StrokedCurveLayer accepts path commands as input.
// This layer is intended to be used as a sub-layer of CurveLayer.
// In the vector drawing case, rendering is performed by simply defining an SVG path element.
// In the raster drawing case, we use the approach from rreusser/webgpu-instanced-lines
// to render bezier curves and arcs via WebGPU. Note that this approach has overhead,
// and is therefore not currently meant to render a ton of individual curves.

use encase::{ShaderType, UniformBuffer};
use glam::{Mat4, Vec2, Vec4};
use kurbo::{CubicBez, ParamCurve};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use crate::picking::LayerPickingResult;
use crate::positioning::{get_point_position, get_point_size};
use crate::render_traits::{
    AspectRatioAlignmentMode, AspectRatioMode, ColorMode, DrawToRasterCpu, DrawToRasterGpu, DrawToSvg,
    BrushableLayer, EmphasisCriteria, MarginParams, OpacityMode, PickableLayer, PreparedLayer, SizeMode, UnitsMode, ViewParams,
};
use crate::render_types::{CpuContext, CpuRenderPass, GpuContext, PrepareResult};
use crate::color_mode::{cpu_fill_color, prepare_stroke_color, quantitative_domain};
use crate::scalar_mode::{cpu_stroke_opacity, cpu_stroke_width, prepare_stroke_opacity_mode, prepare_stroke_width_mode};
use crate::emphasis_mode::{background_color_vec4, cpu_is_included, prepare_emphasis_criteria, resolve_background_scalar, DEFAULT_BACKGROUND_COLOR};
use crate::shader_modules::{common, ShaderBuilder};
use crate::two::shapes::{TwoColor, TwoElement, TwoGroup, TwoPath};
use crate::two::svg::{update_svg, SvgContext};
use crate::viewport::{DataCoord, ScreenCoord};
use crate::wgpu;

use crate::curve_and_polygon_utils::{commands_to_subpaths, flatten_subpath, resolve_margins, PathCommand};
use crate::picking_geometry::{dist_sq_to_segment, unapply_model_matrix};

// Must match VERTS_PER_INSTANCE_F = 38 in stroked_curve_layer.wgsl.
// With JOIN_RESOLUTION=8: (8*2 + 3) * 2 = 38.
const VERTS_PER_INSTANCE: u32 = 38;


/// Layer params struct for [`StrokedCurveLayer`].
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct StrokedCurveLayerParams {
    pub layer_id: String,
    pub bounds: Option<MarginParams>,
    pub data_unit_mode_x: UnitsMode,
    pub data_unit_mode_y: UnitsMode,
    /// Whether `stroke_width` is measured in pixels or in data-coordinate units.
    pub stroke_width_unit_mode: UnitsMode,
    pub stroke_width: Option<SizeMode>,
    pub model_matrix: Option<[f32; 16]>,
    pub commands: Arc<Vec<PathCommand>>,
    pub subdivisions: u32,
    /// How to color the stroke. See [`ColorMode`]. `StrokedCurveLayer` renders a
    /// single shape, so modes carrying `NumericData` are expected to supply a
    /// single (length-1) value.
    pub stroke_color: Option<ColorMode>,
    pub stroke_opacity: Option<OpacityMode>,

    /// Criteria AND-ed together to determine whether the single shape is
    /// selected ("foreground") / filtered-in ("background"). `StrokedCurveLayer`
    /// renders a single shape, so modes carrying `NumericData` are expected to
    /// supply a single (length-1) value. An empty list means the shape is
    /// included.
    pub selection_criteria: Vec<EmphasisCriteria>,
    pub filtering_criteria: Vec<EmphasisCriteria>,

    /// Stroke color used when the shape is filter-included, but
    /// selection-excluded ("background"), in place of `stroke_color`.
    pub background_stroke_color: Option<(u8, u8, u8)>,

    /// These layer parameters allow modification of the "background" visual
    /// encoding. Rather than only supporting an alternative stroke color, a
    /// user may instead (or additionally) use opacity or stroke width. Each is
    /// only applied when its corresponding `enable_background_*` flag is set
    /// AND a value is provided here; otherwise the shape's normal stroke
    /// opacity/width is used unchanged (there is no universal "de-emphasized"
    /// default for these, unlike `background_stroke_color`, which falls back
    /// to `DEFAULT_BACKGROUND_COLOR`).
    pub background_stroke_opacity: Option<f32>,
    pub background_stroke_width: Option<f32>,

    // When true, a "background" shape has the stroke color specified via `background_stroke_color`.
    pub enable_background_stroke_color: bool,
    // When true, a "background" shape has the stroke opacity specified via `background_stroke_opacity`.
    pub enable_background_stroke_opacity: bool,
    // When true, a "background" shape has the stroke width specified via `background_stroke_width`.
    // Note: `StrokedCurveLayer` renders a single shape, so this applies uniformly
    // (see the `0u`-indexed `is_selected_in`/`get_stroke_width` calls in the shader).
    pub enable_background_stroke_width: bool,
}

impl Default for StrokedCurveLayerParams {
    fn default() -> Self {
        Self {
            layer_id: "".to_string(),
            bounds: None,
            data_unit_mode_x: UnitsMode::Data,
            data_unit_mode_y: UnitsMode::Data,
            stroke_width_unit_mode: UnitsMode::Pixels,
            stroke_width: Some(SizeMode::UniformSize(1.0)),
            model_matrix: None,
            commands: Arc::new(vec![]),
            subdivisions: 32,
            stroke_color: None,
            stroke_opacity: Some(OpacityMode::UniformOpacity(1.0)),
            selection_criteria: vec![],
            filtering_criteria: vec![],
            background_stroke_color: None,
            background_stroke_opacity: None,
            background_stroke_width: None,
            enable_background_stroke_color: true,
            enable_background_stroke_opacity: false,
            enable_background_stroke_width: false,
        }
    }
}

pub struct StrokedCurveLayer {
    view_params: ViewParams,
    layer_params: StrokedCurveLayerParams,
    subpaths: Vec<Vec<CubicBez>>,
    polylines: Vec<Vec<(f32, f32)>>,
}

impl StrokedCurveLayer {
    pub fn new(view_params: ViewParams, layer_params: StrokedCurveLayerParams) -> Self {
        // Validate the lengths of things. StrokedCurveLayer renders a single
        // shape (see `cpu_fill_color`/`cpu_stroke_width`/`cpu_stroke_opacity`
        // calls below, which always read index 0), so any instanced values
        // must have exactly one element.
        if let Some(stroke_color) = &layer_params.stroke_color {
            stroke_color.validate_len(1);
        }
        if let Some(stroke_width) = &layer_params.stroke_width {
            stroke_width.validate_len(1);
        }
        if let Some(stroke_opacity) = &layer_params.stroke_opacity {
            stroke_opacity.validate_len(1);
        }
        for selection_criteria in &layer_params.selection_criteria {
            selection_criteria.validate_len(1);
        }
        for filtering_criteria in &layer_params.filtering_criteria {
            filtering_criteria.validate_len(1);
        }

        // TODO: move this logic to the prepare() function?
        // TODO: only do these computations in the raster drawing case?
        let subpaths = commands_to_subpaths(&layer_params.commands);
        let subdivisions = layer_params.subdivisions.max(1);
        let polylines = subpaths.iter().map(|s| flatten_subpath(s, subdivisions)).collect();
        Self { view_params, layer_params, subpaths, polylines }
    }
}


#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for StrokedCurveLayer {
    async fn prepare(&mut self, _gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        PrepareResult { bailed_early: false }
    }
}

#[derive(ShaderType, Debug)]
struct StrokedCurveLayerUniforms {
    layer_size: Vec2,                 // (layer_width, layer_height) in pixels
    camera_view: Mat4,                // mat4x4<f32>,
    data_unit_mode_x: u32,            // 0 = pixels, 1 = data units, 2 = normalized
    data_unit_mode_y: u32,            // 0 = pixels, 1 = data units, 2 = normalized
    stroke_width: f32,
    stroke_width_unit_mode: u32,      // 0 = pixels, 1 = data units, 2 = normalized
    aspect_ratio_mode: u32,           // 0 = ignore, 1 = contain, 2 = cover
    aspect_ratio_alignment_mode: u32, // 0 = center, 1 = start, 2 = end
    model_matrix: Mat4,
    stroke_color_mode: u32,      // see ColorMode::shader_mode()
    stroke_color: Vec4,          // rgba color used by the UniformRgb mode
    stroke_color_reverse: u32,   // 1 = reverse the quantitative colormap
    stroke_color_domain: Vec2,   // (min, max) normalization domain for quantitative mode
    stroke_opacity: f32,
    background_stroke_color: Vec4, // rgba stroke color used when filter-included, but selection-excluded ("background")
    background_stroke_opacity: f32, // stroke opacity used for "background" when enable_background_stroke_opacity is set
    background_stroke_width: f32,   // stroke width used for "background" when enable_background_stroke_width is set
    // Each flag is pre-resolved on the CPU to also require the corresponding
    // background_* param to be non-default, so the shader only checks `== 1u`.
    enable_background_stroke_color: u32,
    enable_background_stroke_opacity: u32,
    enable_background_stroke_width: u32,

    // TODO: define a stroke_linecap parameter, with either None or Round options,
    // and add support for this configurable property in both the Raster and SVG drawing cases.
}

// First bind-group binding index used for color-mode value/palette texture(s).
// Bindings 0-2 are the uniforms buffer, the points buffer, and the segments buffer.
const COLOR_BINDING_START: u32 = 3;

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for StrokedCurveLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        let Self { view_params, layer_params, polylines, .. } = self;

        let has_work = polylines.iter().any(|p| p.len() >= 2);
        if !has_work {
            return;
        }

        let (margin_left, margin_top, margin_right, margin_bottom) =
            resolve_margins(&layer_params.bounds, &view_params.margins);

        let camera_view = view_params.camera_view.unwrap_or([
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ]);

        let layer_w = view_params.width as f32 - (margin_left + margin_right) as f32;
        let layer_h = view_params.height as f32 - (margin_top + margin_bottom) as f32;

        let data_unit_mode_x = match layer_params.data_unit_mode_x { UnitsMode::Pixels => 0, UnitsMode::Data => 1, UnitsMode::Normalized => 2 };
        let data_unit_mode_y = match layer_params.data_unit_mode_y { UnitsMode::Pixels => 0, UnitsMode::Data => 1, UnitsMode::Normalized => 2 };
        let aspect_ratio_mode = match view_params.aspect_ratio_mode { AspectRatioMode::Ignore => 0, AspectRatioMode::Contain => 1, AspectRatioMode::Cover => 2 };
        let aspect_ratio_alignment_mode = match view_params.aspect_ratio_alignment_mode { AspectRatioAlignmentMode::Center => 0, AspectRatioAlignmentMode::Start => 1, AspectRatioAlignmentMode::End => 2 };
        let model_matrix = Mat4::from_cols_array(&layer_params.model_matrix.unwrap_or([
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ]));

        let GpuContext { device, queue } = gpu_context;

        let mut pts_data: Vec<f32> = Vec::new();
        let mut seg_data: Vec<u32> = Vec::new();
        let mut total_segments: u32 = 0;

        for polyline in polylines {
            if polyline.len() < 2 {
                continue;
            }
            let poly_start = (pts_data.len() / 2) as u32;
            for (x, y) in polyline {
                pts_data.push(*x);
                pts_data.push(*y);
            }
            let poly_end = poly_start + polyline.len() as u32 - 1;
            for local_b in 0..(polyline.len() as u32 - 1) {
                seg_data.push(poly_start);
                seg_data.push(poly_end);
                seg_data.push(local_b);
                total_segments += 1;
            }
        }

        // Build the GPU-side color resources for the configured color mode. Modes
        // that carry per-element `NumericData` upload it as one or more textures
        // (bound from COLOR_BINDING_START onward) and contribute the WGSL
        // `get_stroke_color` function injected into the shader below.
        let color = prepare_stroke_color(device, queue, layer_params.stroke_color.as_ref(), COLOR_BINDING_START);

        // Build the GPU-side width and opacity resources. Like the color mode,
        // the instanced variants upload a value texture; those bindings follow
        // the color textures. `CurveLayer` renders a single shape, so the shader
        // always resolves element 0. The width texture is read in the vertex
        // stage, the opacity texture in the fragment stage.
        let width_binding_start = COLOR_BINDING_START + color.textures.len() as u32;
        let width = prepare_stroke_width_mode(device, queue, layer_params.stroke_width.as_ref(), width_binding_start);
        let opacity_binding_start = width_binding_start + width.texture.is_some() as u32;
        let opacity = prepare_stroke_opacity_mode(device, queue, layer_params.stroke_opacity.as_ref(), opacity_binding_start);

        // Filtering and selection criteria (fragment stage only: the shape is
        // discarded when filter-excluded, and re-colored with
        // `background_stroke_color` when selection-excluded; see fs_main).
        // `StrokedCurveLayer` renders a single shape, so the shader always
        // resolves element 0.
        let filter_binding_start = opacity_binding_start + opacity.texture.is_some() as u32;
        let filtering = prepare_emphasis_criteria(
            device, queue, &layer_params.filtering_criteria, "is_filtered_in", "filter_data", filter_binding_start,
        );
        let select_binding_start = filter_binding_start + filtering.textures.len() as u32;
        let selection = prepare_emphasis_criteria(
            device, queue, &layer_params.selection_criteria, "is_selected_in", "select_data", select_binding_start,
        );

        let mut bgl_entries = vec![
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ];
        // One fragment-visible texture per color value / palette array.
        for (i, tex) in color.textures.iter().enumerate() {
            bgl_entries.push(wgpu::BindGroupLayoutEntry {
                binding: COLOR_BINDING_START + i as u32,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: tex.sample_type,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            });
        }
        // Instanced width texture (read in the vertex stage) and instanced
        // opacity texture (read in the fragment stage), each present only when
        // the corresponding mode is instanced.
        if let Some(tex) = &width.texture {
            bgl_entries.push(wgpu::BindGroupLayoutEntry {
                binding: width_binding_start,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Texture {
                    sample_type: tex.sample_type,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            });
        }
        if let Some(tex) = &opacity.texture {
            bgl_entries.push(wgpu::BindGroupLayoutEntry {
                binding: opacity_binding_start,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: tex.sample_type,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            });
        }
        // Filtering/selection codes or values textures (fragment stage only;
        // see `is_filtered_in`/`is_selected_in` in the shader), one per
        // criteria that carries per-element data.
        for (i, tex) in filtering.textures.iter().enumerate() {
            bgl_entries.push(wgpu::BindGroupLayoutEntry {
                binding: filter_binding_start + i as u32,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: tex.sample_type,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            });
        }
        for (i, tex) in selection.textures.iter().enumerate() {
            bgl_entries.push(wgpu::BindGroupLayoutEntry {
                binding: select_binding_start + i as u32,
                // Visible to both stages: the fragment stage uses `is_selected_in`
                // to re-color/re-opacify the background shape, while the vertex
                // stage also needs it to resolve `background_stroke_width` (which
                // affects vertex-stage geometry).
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: tex.sample_type,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            });
        }
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("StrokedCurve BGL"),
            entries: &bgl_entries,
        });

        // Inject the shared WGSL functions at compile time (see `crate::shader_modules`).
        let shader_source = ShaderBuilder::new(include_str!("shaders/stroked_curve_layer.wgsl"))
            .inject_function("scale", common::SCALE)
            .inject_function("translate", common::TRANSLATE)
            .inject_function("get_aspect_ratio_mat", common::GET_ASPECT_RATIO_MAT)
            // Color-mode specialization: the flat-index texel helper plus the
            // assembled color module (bindings + `get_stroke_color`).
            .inject_function("flat_texel_coord", common::FLAT_TEXEL_COORD)
            .define("stroke_color_module", &color.wgsl)
            // Width- and opacity-mode specialization: each contributes its
            // `get_stroke_width` / `get_stroke_opacity` function (plus a value
            // texture binding when instanced).
            .define("stroke_width_module", &width.wgsl)
            .define("stroke_opacity_module", &opacity.wgsl)
            // Filtering/selection specialization: each contributes its
            // `is_filtered_in` / `is_selected_in` predicate (plus a codes/values
            // texture binding when the criteria carries per-element data).
            .define("filtering_module", &filtering.wgsl)
            .define("selection_module", &selection.wgsl)
            .build();
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("stroked_curve_layer.wgsl"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("StrokedCurve PLD"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("StrokedCurve RPD"),
            layout: Some(&pipeline_layout),
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

        let uniform_struct = StrokedCurveLayerUniforms {
            layer_size: Vec2::new(layer_w, layer_h),
            camera_view: Mat4::from_cols_array(&camera_view),
            data_unit_mode_x,
            data_unit_mode_y,
            stroke_width: width.static_value,
            stroke_width_unit_mode: match layer_params.stroke_width_unit_mode {
                UnitsMode::Pixels => 0,
                UnitsMode::Data => 1,
                UnitsMode::Normalized => 2,
            },
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            model_matrix,
            stroke_color_mode: color.mode,
            stroke_color: Vec4::from_array(color.static_color),
            stroke_color_reverse: color.reverse,
            stroke_color_domain: Vec2::from_array(color.domain),
            stroke_opacity: opacity.static_value,
            background_stroke_color: background_color_vec4(layer_params.background_stroke_color),
            background_stroke_opacity: layer_params.background_stroke_opacity.unwrap_or(0.0),
            background_stroke_width: layer_params.background_stroke_width.unwrap_or(0.0),
            enable_background_stroke_color: layer_params.enable_background_stroke_color as u32,
            enable_background_stroke_opacity: (layer_params.enable_background_stroke_opacity
                && layer_params.background_stroke_opacity.is_some()) as u32,
            enable_background_stroke_width: (layer_params.enable_background_stroke_width
                && layer_params.background_stroke_width.is_some()) as u32,
        };
        let mut ub = UniformBuffer::new(Vec::<u8>::new());
        ub.write(&uniform_struct).unwrap();
        let uniform_bytes = ub.into_inner();

        let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("StrokedCurve Uniform"),
            size: uniform_bytes.len() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&uniform_buf, 0, &uniform_bytes);

        let pts_bytes: &[u8] = bytemuck::cast_slice(&pts_data);
        let pts_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("StrokedCurve Points"),
            size: pts_bytes.len() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&pts_buf, 0, pts_bytes);

        let seg_bytes: &[u8] = bytemuck::cast_slice(&seg_data);
        let seg_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("StrokedCurve Segments"),
            size: seg_bytes.len() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&seg_buf, 0, seg_bytes);

        let mut bg_entries = vec![
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: pts_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: seg_buf.as_entire_binding(),
            },
        ];
        for (i, tex) in color.textures.iter().enumerate() {
            bg_entries.push(wgpu::BindGroupEntry {
                binding: COLOR_BINDING_START + i as u32,
                resource: wgpu::BindingResource::TextureView(&tex.view),
            });
        }
        if let Some(tex) = &width.texture {
            bg_entries.push(wgpu::BindGroupEntry {
                binding: width_binding_start,
                resource: wgpu::BindingResource::TextureView(&tex.view),
            });
        }
        if let Some(tex) = &opacity.texture {
            bg_entries.push(wgpu::BindGroupEntry {
                binding: opacity_binding_start,
                resource: wgpu::BindingResource::TextureView(&tex.view),
            });
        }
        for (i, tex) in filtering.textures.iter().enumerate() {
            bg_entries.push(wgpu::BindGroupEntry {
                binding: filter_binding_start + i as u32,
                resource: wgpu::BindingResource::TextureView(&tex.view),
            });
        }
        for (i, tex) in selection.textures.iter().enumerate() {
            bg_entries.push(wgpu::BindGroupEntry {
                binding: select_binding_start + i as u32,
                resource: wgpu::BindingResource::TextureView(&tex.view),
            });
        }
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("StrokedCurve BG"),
            layout: &bgl,
            entries: &bg_entries,
        });

        pass.set_viewport(margin_left as f32, margin_top as f32, layer_w, layer_h, 0.0, 1.0);
        pass.set_scissor_rect(margin_left as u32, margin_top as u32, layer_w as u32, layer_h as u32);

        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..VERTS_PER_INSTANCE, 0..total_segments);
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for StrokedCurveLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for StrokedCurveLayer {
    async fn draw(&self, ctx: &mut SvgContext) {
        let Self { layer_params, view_params, subpaths, .. } = self;

        let camera_view = view_params.camera_view.unwrap_or([
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ]);

        let (margin_left, margin_top, margin_right, margin_bottom) =
            resolve_margins(&layer_params.bounds, &view_params.margins);

        let layer_w = view_params.width as f32 - (margin_left + margin_right) as f32;
        let layer_h = view_params.height as f32 - (margin_top + margin_bottom) as f32;
        let subdivisions = layer_params.subdivisions.max(1) as f64;

        let to_px = |x: f32, y: f32| -> (f64, f64) {
            let (px, py) = get_point_position(
                x, y,
                layer_w, layer_h,
                &camera_view,
                layer_params.data_unit_mode_x.clone(),
                layer_params.data_unit_mode_y.clone(),
                view_params.aspect_ratio_mode.clone(),
                view_params.aspect_ratio_alignment_mode.clone(),
                layer_params.model_matrix.as_ref().map(|m| m.as_slice()),
            );
            (px as f64, (layer_h - py) as f64)
        };

        // Filter-excluded shapes are not rendered at all.
        let is_filtered_in = cpu_is_included(&layer_params.filtering_criteria, 0);
        // Filter-included but selection-excluded ("background") shapes still
        // render, but de-emphasized with `background_stroke_color` in place of
        // the configured stroke color.
        let is_selected = cpu_is_included(&layer_params.selection_criteria, 0);

        // A single shape uses one color, resolved from element 0.
        let quant_domain = match layer_params.stroke_color.as_ref() {
            Some(ColorMode::Quantitative(params)) => quantitative_domain(params),
            _ => [0.0, 1.0],
        };
        let stroke = TwoColor::Rgb(if is_selected || !layer_params.enable_background_stroke_color {
            cpu_fill_color(layer_params.stroke_color.as_ref(), 0, quant_domain)
        } else {
            layer_params.background_stroke_color.unwrap_or(DEFAULT_BACKGROUND_COLOR)
        });

        // A single shape uses one width / opacity, resolved from element 0.
        // The background overrides (when enabled) replace these before the
        // width's unit conversion below.
        let width_value = resolve_background_scalar(
            is_selected,
            layer_params.enable_background_stroke_width,
            layer_params.background_stroke_width,
            cpu_stroke_width(layer_params.stroke_width.as_ref(), 0),
        );
        let stroke_opacity = resolve_background_scalar(
            is_selected,
            layer_params.enable_background_stroke_opacity,
            layer_params.background_stroke_opacity,
            cpu_stroke_opacity(layer_params.stroke_opacity.as_ref(), 0),
        ) as f64;

        // Stroke width in pixels. In pixel mode it is used directly; in data mode
        // it is transformed through the same pipeline as positions (with w=0, so
        // translations cancel out), mirroring the GPU shader. Stroke width is
        // measured relative to the Y axis, so use the Y screen extent. In
        // normalized mode it is a fraction (0 to 1) of the layer size, also
        // treated as height-relative.
        let stroke_width_px = if layer_params.stroke_width_unit_mode == UnitsMode::Data {
            let (_sx, sy) = get_point_size(
                width_value,
                width_value,
                layer_w,
                layer_h,
                &camera_view,
                layer_params.data_unit_mode_x.clone(),
                layer_params.data_unit_mode_y.clone(),
                view_params.aspect_ratio_mode.clone(),
                view_params.aspect_ratio_alignment_mode.clone(),
                layer_params.model_matrix.as_ref().map(|m| m.as_slice()),
            );
            sy.abs()
        } else if layer_params.stroke_width_unit_mode == UnitsMode::Normalized {
            width_value * layer_h
        } else {
            width_value
        };

        let mut svg_elements: Vec<TwoElement> = Vec::with_capacity(subpaths.len());
        for subpath in subpaths {
            if !is_filtered_in || subpath.is_empty() {
                continue;
            }
            let mut d = String::new();
            let first = subpath[0].p0;
            let (fx, fy) = to_px(first.x as f32, first.y as f32);
            d.push_str(&format!("M {} {}", fx, fy));
            for seg in subpath {
                for step in 1..=(subdivisions as u32) {
                    let t = step as f64 / subdivisions;
                    let p = seg.eval(t);
                    let (px, py) = to_px(p.x as f32, p.y as f32);
                    d.push_str(&format!(" L {} {}", px, py));
                }
            }
            svg_elements.push(TwoElement::Path(TwoPath {
                d,
                stroke: Some(stroke.clone()),
                fill: None,
                linewidth: stroke_width_px as f64,
                opacity: 1.0,
                fill_opacity: 1.0,
                stroke_opacity,
                stroke_linejoin: Some("round".to_string()),
                stroke_linecap: Some("round".to_string()),
            }));
        }

        let svg_elements = vec![TwoElement::Group(TwoGroup {
            elements: svg_elements,
            translate: Some((margin_left, margin_top)),
            layer_id: Some(layer_params.layer_id.clone()),
            clip_rect: Some((0.0, 0.0, layer_w as f64, layer_h as f64)),
            ..Default::default()
        })];

        update_svg(ctx, &svg_elements);
    }
}

impl BrushableLayer for StrokedCurveLayer {}

impl PickableLayer for StrokedCurveLayer {
    fn pick(&self, _screen_coord: ScreenCoord, data_coord: Option<DataCoord>) -> Option<LayerPickingResult> {
        let DataCoord::TwoD { x: wx, y: wy } = data_coord? else {
            return None;
        };

        // Pixel/normalized-units positioning places the curve relative to
        // the layer bounds rather than in data space, so a data-space
        // distance test does not apply.
        if self.layer_params.data_unit_mode_x != UnitsMode::Data
            || self.layer_params.data_unit_mode_y != UnitsMode::Data
        {
            return None;
        }

        // Filter-excluded shapes are ignored in picking.
        if !cpu_is_included(&self.layer_params.filtering_criteria, 0) {
            return None;
        }

        // Map the world coordinate into model space by inverting the
        // model_matrix; the vertex shader computes
        // world = model_matrix * vec4(position, 0, 1).
        let (cx, cy) = unapply_model_matrix(self.layer_params.model_matrix, wx, wy)?;

        // Naive picking: iterate over every segment of every sub-path,
        // keeping the one closest to the picked point (no distance
        // threshold), mirroring PointLayer's nearest-neighbor approach.
        let mut min_dist_sq = f32::MAX;
        let mut closest: Option<(usize, usize)> = None;
        for (subpath_idx, points) in self.polylines.iter().enumerate() {
            for i in 0..points.len().saturating_sub(1) {
                let (ax, ay) = points[i];
                let (bx, by) = points[i + 1];
                let dist_sq = dist_sq_to_segment(cx, cy, ax, ay, bx, by);
                if dist_sq < min_dist_sq {
                    min_dist_sq = dist_sq;
                    closest = Some((subpath_idx, i));
                }
            }
        }

        let (subpath_idx, segment_idx) = closest?;
        let mut info = HashMap::new();
        info.insert("subpath_index".to_string(), subpath_idx.to_string());
        info.insert("segment_index".to_string(), segment_idx.to_string());

        Some(LayerPickingResult {
            layer_id: self.layer_params.layer_id.clone(),
            info,
        })
    }
}
