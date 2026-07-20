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
use crate::color_mode::{cpu_fill_color, prepare_color_mode, quantitative_domain};
use crate::scalar_mode::{cpu_point_opacity, cpu_point_radius, prepare_opacity_mode, prepare_size_mode};
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

    // TODO: rename to fill_opacity.
    pub point_opacity: Option<OpacityMode>,


    // How to color each point. See [`ColorMode`]: modes carrying `NumericData`
    // (instanced/categorical/quantitative) supply one or more per-element value
    // arrays, which are uploaded to the GPU as textures at draw time.
    pub fill_color: Option<ColorMode>,

    // TODO: also support stroke_color, stroke_opacity, and stroke_width

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
            point_opacity: Some(OpacityMode::UniformOpacity(1.0)),
            fill_color: None,
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
        if let Some(point_radius) = &layer_params.point_radius {
            point_radius.validate_len(n);
        }
        if let Some(point_opacity) = &layer_params.point_opacity {
            point_opacity.validate_len(n);
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
    data_unit_mode_x: u32, // 0 = pixels, 1 = data units
    data_unit_mode_y: u32, // 0 = pixels, 1 = data units
    point_radius: f32,  // radius of each point
    point_radius_unit_mode_x: u32, // 0 = pixels, 1 = data units
    point_radius_unit_mode_y: u32, // 0 = pixels, 1 = data units
    point_shape_mode: u32, // 0 = square, 1 = circle
    point_opacity: f32,
    aspect_ratio_mode: u32, // 0 = ignore, 1 = contain, 2 = cover
    aspect_ratio_alignment_mode: u32, // 0 = center, 1 = start, 2 = end
    model_matrix: Mat4, // mat4x4<f32> for affine transformations of the image.
    fill_color_mode: u32,     // see ColorMode::shader_mode()
    fill_color: Vec4,         // rgba color used by the UniformRgb mode
    fill_color_reverse: u32,  // 1 = reverse the quantitative colormap
    fill_color_domain: Vec2,  // (min, max) normalization domain for quantitative mode
}

// First bind-group binding index used for color-mode value/palette texture(s).
// Bindings 0-2 are the uniforms buffer and the X/Y position textures.
const COLOR_BINDING_START: u32 = 3;

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

        // Build the GPU-side color resources for the configured color mode. Modes
        // that carry per-element `NumericData` upload it as one or more textures
        // (bound from COLOR_BINDING_START onward) and contribute the WGSL
        // `get_fill_color` function injected into the shader below.
        let color = prepare_color_mode(device, queue, layer_params.fill_color.as_ref(), COLOR_BINDING_START);

        // Build the GPU-side size and opacity resources. Like the color mode,
        // the instanced variants upload a per-element value texture; those
        // bindings follow the color textures. The size texture is read in the
        // vertex stage (radius), the opacity texture in the fragment stage.
        let size_binding_start = COLOR_BINDING_START + color.textures.len() as u32;
        let size = prepare_size_mode(device, queue, layer_params.point_radius.as_ref(), size_binding_start);
        let opacity_binding_start = size_binding_start + size.texture.is_some() as u32;
        let opacity = prepare_opacity_mode(device, queue, layer_params.point_opacity.as_ref(), opacity_binding_start);

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
            },
            data_unit_mode_y: match layer_params.data_unit_mode_y {
                UnitsMode::Pixels => 0,
                UnitsMode::Data => 1,
            },
            point_radius: size.static_value,
            point_radius_unit_mode_x: match layer_params.point_radius_unit_mode_x {
                UnitsMode::Pixels => 0,
                UnitsMode::Data => 1,
            },
            point_radius_unit_mode_y: match layer_params.point_radius_unit_mode_y {
                UnitsMode::Pixels => 0,
                UnitsMode::Data => 1,
            },
            point_shape_mode: match layer_params.point_shape_mode {
                PointShapeMode::Square => 0,
                PointShapeMode::Circle => 1,
            },
            point_opacity: opacity.static_value,
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
            fill_color_mode: color.mode,
            fill_color: Vec4::from_array(color.static_color),
            fill_color_reverse: color.reverse,
            fill_color_domain: Vec2::from_array(color.domain),
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
        // Instanced radius texture (read in the vertex stage) and instanced
        // opacity texture (read in the fragment stage), each present only when
        // the corresponding mode is instanced.
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
        for (i, tex) in color.textures.iter().enumerate() {
            bg_entries.push(wgpu::BindGroupEntry {
                binding: COLOR_BINDING_START + i as u32,
                resource: wgpu::BindingResource::TextureView(&tex.view),
            });
        }
        if let Some(tex) = &size.texture {
            bg_entries.push(wgpu::BindGroupEntry {
                binding: size_binding_start,
                resource: wgpu::BindingResource::TextureView(&tex.view),
            });
        }
        if let Some(tex) = &opacity.texture {
            bg_entries.push(wgpu::BindGroupEntry {
                binding: opacity_binding_start,
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
            .define("color_module", &color.wgsl)
            // Size- and opacity-mode specialization: each contributes its
            // `get_point_radius` / `get_point_opacity` function (plus a value
            // texture binding when instanced).
            .define("size_module", &size.wgsl)
            .define("opacity_module", &opacity.wgsl)
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

        // Quantitative normalization domain, computed once for the whole layer.
        let quant_domain = match layer_params.fill_color.as_ref() {
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

            // Per-point radius / opacity (uniform or instanced), matching the
            // GPU size/opacity modes.
            let radius_value = cpu_point_radius(layer_params.point_radius.as_ref(), i);
            let point_opacity = cpu_point_opacity(layer_params.point_opacity.as_ref(), i) as f64;

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
            } else {
                radius_value
            };

            let fill = Some(TwoColor::Rgb(cpu_fill_color(layer_params.fill_color.as_ref(), i, quant_domain)));

            // Create a circle or square element based on point_shape_mode.
            svg_elements.push(match layer_params.point_shape_mode {
                PointShapeMode::Circle => TwoElement::Circle(TwoCircle {
                    x: px as f64,
                    y: (layer_h - py) as f64,
                    radius: point_radius as f64,
                    fill,
                    opacity: point_opacity,
                    ..Default::default()
                }),
                PointShapeMode::Square => TwoElement::Rectangle(TwoRectangle {
                    x: (px - point_radius) as f64,
                    y: ((layer_h - py) - point_radius) as f64,
                    width: (point_radius * 2.0) as f64,
                    height: (point_radius * 2.0) as f64,
                    fill,
                    opacity: point_opacity,
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
