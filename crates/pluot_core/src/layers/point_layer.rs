// Inspired by the DeckGL PointLayer
// Reference: https://deck.gl/docs/api-reference/layers/scatterplot-layer

use encase::{ShaderType, UniformBuffer};
use glam::{DMat4, DVec4, Mat4, Vec2, Vec4};
use serde::{Deserialize, Serialize};
use std::sync::{Arc};

use std::collections::HashMap;
use crate::picking::LayerPickingResult;
use crate::render_traits::{AspectRatioMode, AspectRatioAlignmentMode, ColorMode, DrawToRasterGpu, DrawToRasterCpu, DrawToSvg, MarginParams, OpacityMode, PickableLayer, PreparedLayer, SizeMode, UnitsMode, ViewParams};
use crate::viewport::{DataCoord, ScreenCoord};
use crate::shader_modules::{common, ShaderBuilder};
use crate::numeric_data::NumericData;
use crate::color_mode::{cpu_fill_color, prepare_color_mode, prepare_stroke_color, quantitative_domain};
use crate::scalar_mode::{
    cpu_fill_opacity, cpu_point_radius, cpu_stroke_opacity, cpu_stroke_width,
    prepare_fill_opacity_mode, prepare_size_mode, prepare_stroke_opacity_mode,
    prepare_stroke_width_mode,
};
use crate::render_types::{CpuContext, CpuRenderPass, PrepareResult, RenderResult};
use crate::render_types::GpuContext;
use crate::wgpu;
use crate::two::shapes::{TwoCircle, TwoColor, TwoElement, TwoGroup, TwoLine, TwoPath, TwoRectangle, TwoText};
use crate::two::svg::{update_svg, SvgContext};
use crate::positioning::{get_point_position, get_point_size};


#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum PointShapeMode {
    // 0: square (basically no-op in fragment shader)
    Square,
    // 1: circles (convert square to circle in fragment shader)
    Circle,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct PointLayerParams {
    pub layer_id: String,
    // If None, assume margin: 0 in all directions.
    pub bounds: Option<MarginParams>,
    pub data_unit_mode_x: UnitsMode,
    pub data_unit_mode_y: UnitsMode,

    pub point_radius_unit_mode_x: UnitsMode,
    pub point_radius_unit_mode_y: UnitsMode,
    pub point_shape_mode: PointShapeMode,
    pub model_matrix: Option<[f32; 16]>, // Column-major 4x4 matrix

    pub point_radius: Option<SizeMode>,

    // How to color each point. See [`ColorMode`]: modes carrying `NumericData`
    // (instanced/categorical/quantitative) supply one or more per-element value
    // arrays, which are uploaded to the GPU as textures at draw time.
    pub fill_color: Option<ColorMode>,

    // Note: Renamed from point_opacity.
    pub fill_opacity: Option<OpacityMode>,

    pub stroke_width_unit_mode: UnitsMode,

    pub stroke_color: Option<ColorMode>,
    pub stroke_opacity: Option<OpacityMode>,
    pub stroke_width: Option<SizeMode>,

    // Per-point X/Y coordinates. Each may be any supported numeric dtype
    // (8–64 bit int/uint, or 32/64-bit float), and X and Y may differ. The
    // data is uploaded to the GPU as a texture at its native width wherever
    // possible (see `NumericData::create_data_texture`).
    pub position_x: NumericData,
    pub position_y: NumericData,
}

impl Default for PointLayerParams {
    fn default() -> Self {
        Self {
            layer_id: "".to_string(),
            bounds: None,
            data_unit_mode_x: UnitsMode::Data,
            data_unit_mode_y: UnitsMode::Data,
            point_radius: Some(SizeMode::UniformSize(1.0)),
            point_radius_unit_mode_x: UnitsMode::Pixels,
            point_radius_unit_mode_y: UnitsMode::Pixels,
            point_shape_mode: PointShapeMode::Circle,
            model_matrix: None,
            fill_color: None,
            fill_opacity: None,
            stroke_width_unit_mode: UnitsMode::Pixels,
            stroke_color: None,
            stroke_opacity: None,
            stroke_width: None,
            position_x: NumericData::Float32(Arc::new(vec![])),
            position_y: NumericData::Float32(Arc::new(vec![])),
        }
    }
}

pub struct PointLayer {
    view_params: ViewParams,
    layer_params: PointLayerParams,
}

impl PointLayer {
    pub fn new(
        view_params: ViewParams,
        layer_params: PointLayerParams,
    ) -> Self {
        // Error if point_radius_unit_mode is "data" when data_unit_mode is "pixels".
        if layer_params.point_radius_unit_mode_x != layer_params.point_radius_unit_mode_y {
            // TODO: support ellipses, potentially in a separate layer type.
            // See https://github.com/keller-mark/pluot-private/blob/main/point_layer.wgsl
            panic!("point_radius_unit_mode must be the same for X and Y axes. Please reach out if you need ellipse support");
        }
        // Validate the lengths of things.
        let n = layer_params.position_x.len();
        if let Some(fill_color) = &layer_params.fill_color {
            fill_color.validate_len(n);
        }
        if let Some(fill_opacity) = &layer_params.fill_opacity {
            fill_opacity.validate_len(n);
        }
        if let Some(point_radius) = &layer_params.point_radius {
            point_radius.validate_len(n);
        }
        if let Some(stroke_color) = &layer_params.stroke_color {
            stroke_color.validate_len(n);
        }
        if let Some(stroke_width) = &layer_params.stroke_width {
            stroke_width.validate_len(n);
        }
        if let Some(stroke_opacity) = &layer_params.stroke_opacity {
            stroke_opacity.validate_len(n);
        }
        for (name, len) in [
            ("position_y", layer_params.position_y.len()),
        ] {
            assert_eq!(
                len, n,
                "{name} has length {len} but position_x has length {n}",
            );
        }
        Self {
            view_params,
            layer_params,
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for PointLayer {
    async fn prepare(&mut self, _gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {

        // TODO: include the layer type in the memoization dependencies?
        // But what if we want multiple layers to be able to reuse the same cached data?
        // Then we should also avoid including the layer_id...

        // TODO: execute getters and cache the results.

        // For now, it is a no-op, since self.data is set in the constructor.
        return PrepareResult {
            bailed_early: false,
        };
    }
}

#[derive(ShaderType, Debug)]
struct PointLayerUniforms {
    layer_size: Vec2, // (layer_width, layer_height) in pixels
    camera_view: Mat4,   // mat4x4<f32>,
    data_unit_mode_x: u32, // 0 = pixels, 1 = data units, 2 = normalized
    data_unit_mode_y: u32, // 0 = pixels, 1 = data units, 2 = normalized
    point_radius: f32,  // radius of each point
    point_radius_unit_mode_x: u32, // 0 = pixels, 1 = data units, 2 = normalized
    point_radius_unit_mode_y: u32, // 0 = pixels, 1 = data units, 2 = normalized
    point_shape_mode: u32, // 0 = square, 1 = circle
    fill_opacity: f32,
    aspect_ratio_mode: u32, // 0 = ignore, 1 = contain, 2 = cover
    aspect_ratio_alignment_mode: u32, // 0 = center, 1 = start, 2 = end
    model_matrix: Mat4, // mat4x4<f32> for affine transformations of the image.
    fill_color_mode: u32,     // see ColorMode::shader_mode()
    fill_color: Vec4,         // rgba color used by the UniformRgb mode
    fill_color_reverse: u32,  // 1 = reverse the quantitative colormap
    fill_color_domain: Vec2,  // (min, max) normalization domain for quantitative mode
    stroke_width: f32,            // border width (UniformSize fallback)
    stroke_width_unit_mode: u32,  // 0 = pixels, 1 = data units, 2 = normalized
    stroke_color_mode: u32,       // see ColorMode::shader_mode()
    stroke_color: Vec4,           // rgba color used by the UniformRgb mode
    stroke_color_reverse: u32,    // 1 = reverse the quantitative colormap
    stroke_color_domain: Vec2,    // (min, max) normalization domain for quantitative mode
    stroke_opacity: f32,          // stroke opacity (UniformOpacity fallback)
}

// First bind-group binding index used for color-mode value/palette texture(s).
// Bindings 0-2 are the uniforms buffer and the X/Y position textures. The
// fill-color textures come first (from here), then the stroke-color, radius,
// stroke-width, fill-opacity and stroke-opacity textures, each present only when
// instanced.
const FILL_COLOR_BINDING_START: u32 = 3;

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for PointLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        let GpuContext { device, queue } = gpu_context;
        let Self { layer_params, view_params } = self;

        let n = layer_params.position_x.len();

        // Upload the X and Y coordinate arrays into single-channel 2D textures,
        // each at its native byte width wherever possible (8/16/32-bit are
        // zero-copy; only 64-bit dtypes are narrowed to 32 bits). X and Y are
        // uploaded independently so they may have different dtypes; the shader
        // reads each texel via its instance index and widens it to f32. See
        // `NumericData::create_data_texture`.
        let (x_texture_view, x_dtype) =
            layer_params.position_x.create_data_texture(device, queue, "X Coordinates Texture");
        let (y_texture_view, y_dtype) =
            layer_params.position_y.create_data_texture(device, queue, "Y Coordinates Texture");

        // Build the GPU-side resources for each fill/stroke mode. Modes that carry
        // per-element `NumericData` upload it as one or more textures and
        // contribute a WGSL getter function injected into the shader below. The
        // texture bindings are assigned sequentially: fill color first (from
        // FILL_COLOR_BINDING_START), then stroke color, radius, stroke width, fill
        // opacity and stroke opacity, each consuming binding slots only when
        // instanced.
        let fill_color = prepare_color_mode(device, queue, layer_params.fill_color.as_ref(), FILL_COLOR_BINDING_START);

        let stroke_color_binding_start = FILL_COLOR_BINDING_START + fill_color.textures.len() as u32;
        let stroke_color = prepare_stroke_color(device, queue, layer_params.stroke_color.as_ref(), stroke_color_binding_start);

        // Point radius (read in the vertex stage to expand the quad).
        let size_binding_start = stroke_color_binding_start + stroke_color.textures.len() as u32;
        let size = prepare_size_mode(device, queue, layer_params.point_radius.as_ref(), size_binding_start);

        // When `stroke_width` is None, treat it as a zero-width (no) border so no
        // stroke band is drawn. `prepare_stroke_width_mode` otherwise defaults a
        // missing width to 1px, which is not what we want here.
        let no_stroke_width = SizeMode::UniformSize(0.0);
        let stroke_width_mode = layer_params.stroke_width.as_ref().unwrap_or(&no_stroke_width);
        let stroke_width_binding_start = size_binding_start + size.texture.is_some() as u32;
        let width = prepare_stroke_width_mode(device, queue, Some(stroke_width_mode), stroke_width_binding_start);

        // Fill opacity (fragment stage) and stroke opacity (fragment stage).
        let fill_opacity_binding_start = stroke_width_binding_start + width.texture.is_some() as u32;
        let fill_opacity = prepare_fill_opacity_mode(device, queue, layer_params.fill_opacity.as_ref(), fill_opacity_binding_start);
        let stroke_opacity_binding_start = fill_opacity_binding_start + fill_opacity.texture.is_some() as u32;
        let stroke_opacity = prepare_stroke_opacity_mode(device, queue, layer_params.stroke_opacity.as_ref(), stroke_opacity_binding_start);

        // Note: WebGPU's shading language (WGSL) treats matrices as column-major.
        let camera_view = view_params.camera_view.unwrap_or([
            // Column 0
            1.0, 0.0, 0.0, 0.0, // Column 1
            0.0, 1.0, 0.0, 0.0, // Column 2
            0.0, 0.0, 1.0, 0.0, // Column 3
            0.0, 0.0, 0.0, 1.0,
        ]);

        // Use layer-specific bounds if not None, otherwise use the view's margins
        // (which may also be None).
        let bounds = if layer_params.bounds.is_none() {
            &view_params.margins
        } else {
            &layer_params.bounds
        };

        let margin_top = if let Some(margin_params) = &bounds {
            margin_params.margin_top.unwrap_or(0.0)
        } else { 0.0 } as f64;
        let margin_right = if let Some(margin_params) = &bounds {
            margin_params.margin_right.unwrap_or(0.0)
        } else { 0.0 } as f64;
        let margin_bottom = if let Some(margin_params) = &bounds {
            margin_params.margin_bottom.unwrap_or(0.0)
        } else { 0.0 } as f64;
        let margin_left = if let Some(margin_params) = &bounds {
            margin_params.margin_left.unwrap_or(0.0)
        } else { 0.0 } as f64;

        let viewport_w = view_params.width as f32;
        let viewport_h = view_params.height as f32;

        let layer_w = viewport_w - (margin_left + margin_right) as f32;
        let layer_h = viewport_h - (margin_top + margin_bottom) as f32;

        // Construct the uniform struct using Encase.
        let uniform_struct = PointLayerUniforms {
            layer_size: Vec2::new(layer_w, layer_h),
            camera_view: Mat4::from_cols_array(&camera_view),
            data_unit_mode_x: match layer_params.data_unit_mode_x {
                UnitsMode::Pixels => 0,
                UnitsMode::Data => 1,
                UnitsMode::Normalized => 2,
            },
            data_unit_mode_y: match layer_params.data_unit_mode_y {
                UnitsMode::Pixels => 0,
                UnitsMode::Data => 1,
                UnitsMode::Normalized => 2,
            },
            point_radius: size.static_value,
            point_radius_unit_mode_x: match layer_params.point_radius_unit_mode_x {
                UnitsMode::Pixels => 0,
                UnitsMode::Data => 1,
                UnitsMode::Normalized => 2,
            },
            point_radius_unit_mode_y: match layer_params.point_radius_unit_mode_y {
                UnitsMode::Pixels => 0,
                UnitsMode::Data => 1,
                UnitsMode::Normalized => 2,
            },
            point_shape_mode: match layer_params.point_shape_mode {
                PointShapeMode::Square => 0,
                PointShapeMode::Circle => 1,
            },
            fill_opacity: fill_opacity.static_value,
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
            model_matrix: Mat4::from_cols_array(&layer_params.model_matrix.unwrap_or([
                // Column 0
                1.0, 0.0, 0.0, 0.0, // Column 1
                0.0, 1.0, 0.0, 0.0, // Column 2
                0.0, 0.0, 1.0, 0.0, // Column 3
                0.0, 0.0, 0.0, 1.0,
            ])),
            fill_color_mode: fill_color.mode,
            fill_color: Vec4::from_array(fill_color.static_color),
            fill_color_reverse: fill_color.reverse,
            fill_color_domain: Vec2::from_array(fill_color.domain),
            stroke_width: width.static_value,
            stroke_width_unit_mode: match layer_params.stroke_width_unit_mode {
                UnitsMode::Pixels => 0,
                UnitsMode::Data => 1,
                UnitsMode::Normalized => 2,
            },
            stroke_color_mode: stroke_color.mode,
            stroke_color: Vec4::from_array(stroke_color.static_color),
            stroke_color_reverse: stroke_color.reverse,
            stroke_color_domain: Vec2::from_array(stroke_color.domain),
            stroke_opacity: stroke_opacity.static_value,
        };

        let mut buffer = UniformBuffer::new(Vec::<u8>::new());
        buffer.write(&uniform_struct).unwrap();
        let uniform_bytes = buffer.into_inner();

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Uniform Buffer"),
            size: uniform_bytes.len() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&uniform_buffer, 0, &uniform_bytes);


        // Create bind group layout and bind group for positions + uniforms.
        // Bindings 0-2 are fixed (uniforms + the two position textures); the
        // color-mode textures follow at binding `COLOR_BINDING_START` onward,
        // matching the WGSL declarations injected below.
        let mut bgl_entries = vec![
            wgpu::BindGroupLayoutEntry {
                // The uniforms buffer.
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
                // The X coordinates texture. Its sample type must match
                // the dtype-specific texture format chosen above.
                binding: 1,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Texture {
                    sample_type: x_dtype.binding_sample_type(),
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                // The Y coordinates texture. Its sample type must match
                // the dtype-specific texture format chosen above.
                binding: 2,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Texture {
                    sample_type: y_dtype.binding_sample_type(),
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
        ];
        // One fragment-visible texture per fill-color value / palette array,
        // followed by the same for the stroke color.
        for (i, tex) in fill_color.textures.iter().enumerate() {
            bgl_entries.push(wgpu::BindGroupLayoutEntry {
                binding: FILL_COLOR_BINDING_START + i as u32,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: tex.sample_type,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            });
        }
        for (i, tex) in stroke_color.textures.iter().enumerate() {
            bgl_entries.push(wgpu::BindGroupLayoutEntry {
                binding: stroke_color_binding_start + i as u32,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: tex.sample_type,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            });
        }
        // Instanced radius and stroke-width textures (read in the vertex stage to
        // expand / band the quad) and instanced fill-/stroke-opacity textures
        // (read in the fragment stage), each present only when the corresponding
        // mode is instanced.
        if let Some(tex) = &size.texture {
            bgl_entries.push(wgpu::BindGroupLayoutEntry {
                binding: size_binding_start,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Texture {
                    sample_type: tex.sample_type,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            });
        }
        if let Some(tex) = &width.texture {
            bgl_entries.push(wgpu::BindGroupLayoutEntry {
                binding: stroke_width_binding_start,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Texture {
                    sample_type: tex.sample_type,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            });
        }
        if let Some(tex) = &fill_opacity.texture {
            bgl_entries.push(wgpu::BindGroupLayoutEntry {
                binding: fill_opacity_binding_start,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: tex.sample_type,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            });
        }
        if let Some(tex) = &stroke_opacity.texture {
            bgl_entries.push(wgpu::BindGroupLayoutEntry {
                binding: stroke_opacity_binding_start,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: tex.sample_type,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            });
        }
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("PointLayer BGL"),
            entries: &bgl_entries,
        });

        let mut bg_entries = vec![
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&x_texture_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&y_texture_view),
            },
        ];
        for (i, tex) in fill_color.textures.iter().enumerate() {
            bg_entries.push(wgpu::BindGroupEntry {
                binding: FILL_COLOR_BINDING_START + i as u32,
                resource: wgpu::BindingResource::TextureView(&tex.view),
            });
        }
        for (i, tex) in stroke_color.textures.iter().enumerate() {
            bg_entries.push(wgpu::BindGroupEntry {
                binding: stroke_color_binding_start + i as u32,
                resource: wgpu::BindingResource::TextureView(&tex.view),
            });
        }
        if let Some(tex) = &size.texture {
            bg_entries.push(wgpu::BindGroupEntry {
                binding: size_binding_start,
                resource: wgpu::BindingResource::TextureView(&tex.view),
            });
        }
        if let Some(tex) = &width.texture {
            bg_entries.push(wgpu::BindGroupEntry {
                binding: stroke_width_binding_start,
                resource: wgpu::BindingResource::TextureView(&tex.view),
            });
        }
        if let Some(tex) = &fill_opacity.texture {
            bg_entries.push(wgpu::BindGroupEntry {
                binding: fill_opacity_binding_start,
                resource: wgpu::BindingResource::TextureView(&tex.view),
            });
        }
        if let Some(tex) = &stroke_opacity.texture {
            bg_entries.push(wgpu::BindGroupEntry {
                binding: stroke_opacity_binding_start,
                resource: wgpu::BindingResource::TextureView(&tex.view),
            });
        }
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("PointLayer BG"),
            layout: &bind_group_layout,
            entries: &bg_entries,
        });

        // Inject the shared WGSL functions at compile time (see `crate::shader_modules`).
        let shader_source = ShaderBuilder::new(include_str!("shaders/point_layer.wgsl"))
            .inject_function("scale", common::SCALE)
            .inject_function("translate", common::TRANSLATE)
            .inject_function("get_aspect_ratio_mat", common::GET_ASPECT_RATIO_MAT)
            .inject_texture_sample_type("x_coords", x_dtype)
            .inject_texture_sample_type("y_coords", y_dtype)
            // Color-mode specialization: the flat-index texel helper plus the
            // assembled color module (bindings + `get_fill_color`).
            .inject_function("flat_texel_coord", common::FLAT_TEXEL_COORD)
            .define("fill_color_module", &fill_color.wgsl)
            .define("stroke_color_module", &stroke_color.wgsl)
            // Size-, width- and opacity-mode specialization: each contributes its
            // `get_point_radius` / `get_stroke_width` / `get_fill_opacity` /
            // `get_stroke_opacity` function (plus a value texture binding when
            // instanced).
            .define("size_module", &size.wgsl)
            .define("stroke_width_module", &width.wgsl)
            .define("fill_opacity_module", &fill_opacity.wgsl)
            .define("stroke_opacity_module", &stroke_opacity.wgsl)
            .build();
        let shader = device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("point_layer.wgsl"),
                source: wgpu::ShaderSource::Wgsl(shader_source.into()),
            });

        let render_pipeline_layout = device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[Some(&bind_group_layout)],
                immediate_size: 0,
            });

        // TODO: Extract the shared render pipeline logic. There is a lot of duplication here.
        let render_pipeline = device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Render Pipeline"),
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

        // Can everything before pass.set_pipeline be cached? Probably not the queue.write calls...

        // Handle margins by adjusting viewport and scissor rect.
        // This allows us to avoid accounting for margins in the shaders, simplifying them.
        // (Shaders can simply assume the full viewport size is the plot area.)
        // Note: these settings will affect all subsequent draw calls in this render pass,
        // so ensure that other layers are setting their own viewport/scissor_rect appropriately.

        // Set viewport so that the (-1 to 1) NDC coordinates map to the desired plot area within the canvas.
        pass.set_viewport(
            margin_left as f32,
            margin_top as f32,
            viewport_w - (margin_left + margin_right) as f32,
            viewport_h - (margin_top + margin_bottom) as f32,
            0.0, // min_depth
            1.0, // max_depth
        );

        // Set scissor rect so that fragments rendered into the margins are clipped.
        // "Sets the scissor rectangle used during the rasterization stage. After transformation into viewport coordinates."
        // "The function of the scissor rectangle resembles set_viewport(), but it does not affect the coordinate system, only which fragments are discarded."
        pass.set_scissor_rect(
            margin_left as u32,
            margin_top as u32,
            (viewport_w - (margin_left + margin_right) as f32) as u32,
            (viewport_h - (margin_top + margin_bottom) as f32) as u32,
        );

        pass.set_pipeline(&render_pipeline);
        pass.set_bind_group(0, &bind_group, &[]);

        // TODO: Would it be more efficient to store the point X/Y/Size/Opacity/Color info in textures, as done by Regl-Scatterplot?
        // (As opposed to using instancing)
        // References:
        // - https://github.com/flekschas/regl-scatterplot/blob/90f0c951233b20bebd4fd1cb15ce1c4128ce9edf/src/point.vs#L43
        // - https://github.com/flekschas/regl-scatterplot/blob/90f0c951233b20bebd4fd1cb15ce1c4128ce9edf/src/index.js#L1938
        pass.draw(0..4, 0..(n as u32));
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for PointLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for PointLayer {
    async fn draw(&self, ctx: &mut SvgContext) {
        let Self { layer_params, view_params } = self;

        // Iterate over the data points and create SVG elements.
        let n = layer_params.position_x.len();

        // Quantitative normalization domains, computed once for the whole layer
        // (one for the fill color, one for the stroke color).
        let fill_quant_domain = match layer_params.fill_color.as_ref() {
            Some(ColorMode::Quantitative(params)) => quantitative_domain(params),
            _ => [0.0, 1.0],
        };
        let stroke_quant_domain = match layer_params.stroke_color.as_ref() {
            Some(ColorMode::Quantitative(params)) => quantitative_domain(params),
            _ => [0.0, 1.0],
        };

        // TODO: reduce code reuse here
        let camera_view = view_params.camera_view.unwrap_or([
            // Column 0
            1.0, 0.0, 0.0, 0.0, // Column 1
            0.0, 1.0, 0.0, 0.0, // Column 2
            0.0, 0.0, 1.0, 0.0, // Column 3
            0.0, 0.0, 0.0, 1.0,
        ]);

        // Use layer-specific bounds if not None, otherwise use the view's margins
        // (which may also be None).
        let bounds = if layer_params.bounds.is_none() {
            &view_params.margins
        } else {
            &layer_params.bounds
        };

        let margin_top = if let Some(margin_params) = &bounds {
            margin_params.margin_top.unwrap_or(0.0)
        } else { 0.0 } as f64;
        let margin_right = if let Some(margin_params) = &bounds {
            margin_params.margin_right.unwrap_or(0.0)
        } else { 0.0 } as f64;
        let margin_bottom = if let Some(margin_params) = &bounds {
            margin_params.margin_bottom.unwrap_or(0.0)
        } else { 0.0 } as f64;
        let margin_left = if let Some(margin_params) = &bounds {
            margin_params.margin_left.unwrap_or(0.0)
        } else { 0.0 } as f64;

        let viewport_w = view_params.width as f32;
        let viewport_h = view_params.height as f32;

        let layer_w = viewport_w - (margin_left + margin_right) as f32;
        let layer_h = viewport_h - (margin_top + margin_bottom) as f32;

        let model_matrix_raw: [f32; 16] = layer_params.model_matrix.unwrap_or([
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ]);
        // End TODO

        let mut svg_elements: Vec<TwoElement> = Vec::with_capacity(n);
        for i in 0..n {
            let x = layer_params.position_x.get_f32(i);
            let y = layer_params.position_y.get_f32(i);

            // Convert data coordinates to pixel coordinates within the layer area.
            let (px, py) = get_point_position(
                x,
                y,
                layer_w,
                layer_h,
                &camera_view,
                layer_params.data_unit_mode_x,
                layer_params.data_unit_mode_y,
                view_params.aspect_ratio_mode,
                view_params.aspect_ratio_alignment_mode,
                Some(&model_matrix_raw),
            );

            // Per-point radius / fill opacity (uniform or instanced), matching the
            // GPU size/opacity modes.
            let radius_value = cpu_point_radius(layer_params.point_radius.as_ref(), i);
            let fill_opacity = cpu_fill_opacity(layer_params.fill_opacity.as_ref(), i) as f64;

            let point_radius = if layer_params.point_radius_unit_mode_x == UnitsMode::Data {
                let (sx, sy) = get_point_size(
                    radius_value,
                    radius_value,
                    layer_w,
                    layer_h,
                    &camera_view,
                    layer_params.data_unit_mode_x,
                    layer_params.data_unit_mode_y,
                    view_params.aspect_ratio_mode,
                    view_params.aspect_ratio_alignment_mode,
                    Some(&model_matrix_raw),
                );
                // Note: sx and sy will currently always be the same unless ellipses are supported.
                (sx.abs() + sy.abs()) * 0.5
            } else if layer_params.point_radius_unit_mode_x == UnitsMode::Normalized {
                // Normalized mode: radius_value is a fraction (0 to 1) of the layer
                // size, independent of the camera. Height-relative, mirroring the
                // GPU shader's point_radius_px_normalized convention.
                radius_value * layer_h
            } else {
                radius_value
            };

            let fill = Some(TwoColor::Rgb(cpu_fill_color(
                layer_params.fill_color.as_ref(), i, fill_quant_domain,
            )));

            // Stroke: only drawn when a stroke width is configured. Uses the
            // stroke color mode, opacity and per-point width. The stroke is drawn
            // inward from `point_radius` (the point's outer bound stays fixed), so
            // the SVG stroke — which is centered on the path — is placed on a
            // boundary inset by half the stroke width.
            let (stroke, stroke_opacity, stroke_width_px) = if layer_params.stroke_width.is_some() {
                let width_value = cpu_stroke_width(layer_params.stroke_width.as_ref(), i);
                let width_px = if layer_params.stroke_width_unit_mode == UnitsMode::Data {
                    let (sx, sy) = get_point_size(
                        width_value,
                        width_value,
                        layer_w,
                        layer_h,
                        &camera_view,
                        layer_params.data_unit_mode_x,
                        layer_params.data_unit_mode_y,
                        view_params.aspect_ratio_mode,
                        view_params.aspect_ratio_alignment_mode,
                        Some(&model_matrix_raw),
                    );
                    (sx.abs() + sy.abs()) * 0.5
                } else if layer_params.stroke_width_unit_mode == UnitsMode::Normalized {
                    // Normalized mode: width_value is a fraction (0 to 1) of the
                    // layer size, independent of the camera. Height-relative,
                    // mirroring the GPU shader's stroke_width_px convention.
                    width_value * layer_h
                } else {
                    width_value
                };
                (
                    Some(TwoColor::Rgb(cpu_fill_color(
                        layer_params.stroke_color.as_ref(), i, stroke_quant_domain,
                    ))),
                    cpu_stroke_opacity(layer_params.stroke_opacity.as_ref(), i) as f64,
                    width_px as f64,
                )
            } else {
                (None, 1.0, 0.0)
            };

            // Create a circle or square element based on point_shape_mode.
            svg_elements.push(match layer_params.point_shape_mode {
                PointShapeMode::Circle => TwoElement::Circle(TwoCircle {
                    x: px as f64,
                    // Inset the drawn radius by half the stroke width so the
                    // centered SVG stroke's outer edge lands at `point_radius`.
                    radius: (point_radius as f64 - stroke_width_px / 2.0).max(0.0),
                    y: (layer_h - py) as f64,
                    fill,
                    stroke,
                    linewidth: stroke_width_px,
                    fill_opacity,
                    stroke_opacity,
                    ..Default::default()
                }),
                PointShapeMode::Square => TwoElement::Rectangle(TwoRectangle {
                    // Inset the drawn square by half the stroke width, so the
                    // centered SVG stroke stays within `point_radius` of center.
                    x: (px as f64 - point_radius as f64) + stroke_width_px / 2.0,
                    y: ((layer_h - py) as f64 - point_radius as f64) + stroke_width_px / 2.0,
                    width: (point_radius as f64 * 2.0 - stroke_width_px).max(0.0),
                    height: (point_radius as f64 * 2.0 - stroke_width_px).max(0.0),
                    fill,
                    stroke,
                    linewidth: stroke_width_px,
                    fill_opacity,
                    stroke_opacity,
                    ..Default::default()
                })
            });
        }

        // Insert rects into an SVG group with a transform and clipping to handle margins,
        // similar to the usage of scissor rect and viewport in the Canvas rendering.
        let svg_elements = vec![
            TwoElement::Group(TwoGroup {
                elements: svg_elements,
                translate: Some((margin_left, margin_top)),
                layer_id: Some(layer_params.layer_id.clone()),
                // TODO: check how clip_rect interacts with the translate
                clip_rect: Some((0.0, 0.0, layer_w as f64, layer_h as f64)),
                ..Default::default()
            })
        ];
        update_svg(ctx, &svg_elements);
    }
}

inventory::submit! {
    crate::registry::LayerRegistration {
        layer_type_name: "PointLayer",
        create_layer: |value, view_params| {
            let params: PointLayerParams = serde_json::from_value(value).unwrap();
            Box::new(PointLayer::new(view_params.clone(), params))
        },
    }
}

impl PickableLayer for PointLayer {
    fn pick(&self, _screen_coord: ScreenCoord, data_coord: Option<DataCoord>) -> Option<LayerPickingResult> {
        let DataCoord::TwoD { x: cx, y: cy } = data_coord? else {
            return None;
        };

        let n = self.layer_params.position_x.len();
        if n == 0 {
            return None;
        }

        // Map the world coordinate into the point's model space by inverting
        // the model_matrix; the vertex shader computes
        // world = model_matrix * vec4(position_x, position_y, 0, 1).
        let m = self.layer_params.model_matrix.unwrap_or([
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ]);
        let mut m64 = [0.0f64; 16];
        for (i, v) in m.iter().enumerate() {
            m64[i] = *v as f64;
        }
        let mat = DMat4::from_cols_array(&m64);
        if mat.determinant() == 0.0 {
            return None;
        }
        let p = mat.inverse() * DVec4::new(cx as f64, cy as f64, 0.0, 1.0);
        let (cx, cy) = (p.x as f32, p.y as f32);

        // For now, this is a very naive picking implementation that just iterates over the points to find the closest match.
        // In the future, we will use multiple render targets to perform GPU-accelerated picking
        // Reference: https://github.com/keller-mark/pluot/issues/140

        let mut min_dist_sq = f32::MAX;
        let mut closest_idx = 0usize;

        for i in 0..n {
            let dx = self.layer_params.position_x.get_f32(i) - cx;
            let dy = self.layer_params.position_y.get_f32(i) - cy;
            let dist_sq = dx * dx + dy * dy;
            if dist_sq < min_dist_sq {
                min_dist_sq = dist_sq;
                closest_idx = i;
            }
        }

        let mut info = HashMap::new();
        info.insert("index".to_string(), closest_idx.to_string());
        // Format in the coordinate's native dtype (integers without a decimal point).
        info.insert("x".to_string(), self.layer_params.position_x.format_element(closest_idx));
        info.insert("y".to_string(), self.layer_params.position_y.format_element(closest_idx));

        Some(LayerPickingResult {
            layer_id: self.layer_params.layer_id.clone(),
            info,
        })
    }
}
