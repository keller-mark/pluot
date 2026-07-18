// Inspired by the DeckGL LineLayer.
// Reference: https://deck.gl/docs/api-reference/layers/line-layer

use encase::{ShaderType, UniformBuffer};
use glam::{Mat4, Vec2, Vec4};
use serde::{Deserialize, Serialize};
use std::sync::{Arc};

use crate::render_traits::{ColorMode, DrawToRasterGpu, DrawToRasterCpu, DrawToSvg, PickableLayer, PreparedLayer, ViewParams, AspectRatioMode, AspectRatioAlignmentMode, UnitsMode, MarginParams};
use crate::render_types::{CpuContext, CpuRenderPass, PrepareResult, RenderResult};
use crate::render_types::GpuContext;
use crate::shader_modules::{common, ShaderBuilder};
use crate::color_mode::{cpu_fill_color, prepare_stroke_color, quantitative_domain};
use crate::numeric_data::NumericData;
use crate::wgpu;
use crate::two::shapes::{TwoCircle, TwoColor, TwoElement, TwoGroup, TwoLine, TwoPath, TwoRectangle, TwoText};
use crate::two::svg::{update_svg, SvgContext};
use crate::positioning::get_point_position;


#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct LineLayerParams {
    pub layer_id: String,
    // If None, assume margin: 0 in all directions.
    pub bounds: Option<MarginParams>,
    pub data_unit_mode_x: UnitsMode,
    pub data_unit_mode_y: UnitsMode,
    pub line_width: f32,
    pub line_width_unit_mode: UnitsMode,
    pub model_matrix: Option<[f32; 16]>, // Column-major 4x4 matrix

    // How to color each line. See [`ColorMode`]: modes carrying `NumericData`
    // (instanced/categorical/quantitative) supply one or more per-element value
    // arrays, which are uploaded to the GPU as textures at draw time. Named
    // `stroke_color` (lines are stroked, not filled); it drives the shared color
    // machinery via `prepare_stroke_color` / `get_stroke_color`.
    pub stroke_color: ColorMode,

    // Per-line source/target X/Y coordinates. Each may be any supported numeric
    // dtype (8-64 bit int/uint, or 32/64-bit float), and may differ across the
    // four arrays. The data is uploaded to the GPU as a texture at its native
    // width wherever possible (see `NumericData::create_data_texture`).
    pub source_position_x: NumericData,
    pub source_position_y: NumericData,
    pub target_position_x: NumericData,
    pub target_position_y: NumericData,
}

impl Default for LineLayerParams {
    fn default() -> Self {
        Self {
            layer_id: "".to_string(),
            bounds: None,
            data_unit_mode_x: UnitsMode::Data,
            data_unit_mode_y: UnitsMode::Data,
            line_width: 1.0,
            line_width_unit_mode: UnitsMode::Pixels,
            model_matrix: None,
            stroke_color: ColorMode::UniformRgb(None),
            source_position_x: NumericData::Float32(Arc::new(vec![])),
            source_position_y: NumericData::Float32(Arc::new(vec![])),
            target_position_x: NumericData::Float32(Arc::new(vec![])),
            target_position_y: NumericData::Float32(Arc::new(vec![])),
        }
    }
}


pub struct LineLayer {
    view_params: ViewParams,
    layer_params: LineLayerParams,
}

impl LineLayer {
    pub fn new(
        view_params: ViewParams,
        layer_params: LineLayerParams,
    ) -> Self {
        // Error if line_width_unit_mode is "data" when data_unit_mode is "pixels".
        if layer_params.line_width_unit_mode == UnitsMode::Data && (layer_params.data_unit_mode_x == UnitsMode::Pixels || layer_params.data_unit_mode_y == UnitsMode::Pixels) {
            panic!("line_width_unit_mode cannot be 'data' when data_unit_mode is 'pixels'");
        }
        // TODO: validate the length of the colorMode values when instanced
        Self {
            view_params,
            layer_params,
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for LineLayer {
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
struct LineLayerUniforms {
    layer_size: Vec2, // (layer_width, layer_height) in pixels
    camera_view: Mat4,   // mat4x4<f32>,
    data_unit_mode_x: u32, // 0 = pixels, 1 = data units
    data_unit_mode_y: u32, // 0 = pixels, 1 = data units
    line_width: f32,  // width of each line
    line_width_unit_mode: u32, // 0 = pixels, 1 = data units
    aspect_ratio_mode: u32, // 0 = ignore, 1 = contain, 2 = cover
    aspect_ratio_alignment_mode: u32, // 0 = center, 1 = start, 2 = end
    model_matrix: Mat4, // mat4x4<f32> for affine transformations of the image.
    stroke_color_mode: u32,      // see ColorMode::shader_mode()
    stroke_color: Vec4,          // rgba color used by the UniformRgb mode
    stroke_color_reverse: u32,   // 1 = reverse the quantitative colormap
    stroke_color_domain: Vec2,   // (min, max) normalization domain for quantitative mode
}

// First bind-group binding index used for color-mode value/palette texture(s).
// Bindings 0-4 are the uniforms buffer and the four position textures.
const COLOR_BINDING_START: u32 = 5;

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for LineLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        let GpuContext { device, queue } = gpu_context;
        let Self { layer_params, view_params } = self;

        // Upload the source/target X and Y coordinate arrays into single-channel
        // 2D textures, each at its native byte width wherever possible (8/16/32-bit
        // are zero-copy; only 64-bit dtypes are narrowed to 32 bits). Each array is
        // uploaded independently so they may have different dtypes; the shader
        // reads each texel via its instance index and widens it to f32. See
        // `NumericData::create_data_texture`.
        let (source_x_texture_view, source_x_dtype) =
            layer_params.source_position_x.create_data_texture(device, queue, "Source X Coordinates Texture");
        let (source_y_texture_view, source_y_dtype) =
            layer_params.source_position_y.create_data_texture(device, queue, "Source Y Coordinates Texture");
        let (target_x_texture_view, target_x_dtype) =
            layer_params.target_position_x.create_data_texture(device, queue, "Target X Coordinates Texture");
        let (target_y_texture_view, target_y_dtype) =
            layer_params.target_position_y.create_data_texture(device, queue, "Target Y Coordinates Texture");

        // Number of lines to draw: one instance per element of the position arrays.
        let n = layer_params.source_position_x.len();

        // Build the GPU-side color resources for the configured color mode. Modes
        // that carry per-element `NumericData` upload it as one or more textures
        // (bound from COLOR_BINDING_START onward) and contribute the WGSL
        // `get_stroke_color` function injected into the shader below.
        let color = prepare_stroke_color(device, queue, &layer_params.stroke_color, COLOR_BINDING_START);

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
        let uniform_struct = LineLayerUniforms {
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
            line_width: layer_params.line_width,
            line_width_unit_mode: match layer_params.line_width_unit_mode {
                UnitsMode::Pixels => 0,
                UnitsMode::Data => 1,
            },
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
            stroke_color_mode: color.mode,
            stroke_color: Vec4::from_array(color.static_color),
            stroke_color_reverse: color.reverse,
            stroke_color_domain: Vec2::from_array(color.domain),
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
        // Bindings 0-4 are fixed (uniforms + the four position textures); the
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
                // The Source X coordinates texture. Its sample type must match
                // the dtype-specific texture format chosen above.
                binding: 1,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Texture {
                    sample_type: source_x_dtype.binding_sample_type(),
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                // The Source Y coordinates texture. Its sample type must match
                // the dtype-specific texture format chosen above.
                binding: 2,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Texture {
                    sample_type: source_y_dtype.binding_sample_type(),
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                // The Target X coordinates texture. Its sample type must match
                // the dtype-specific texture format chosen above.
                binding: 3,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Texture {
                    sample_type: target_x_dtype.binding_sample_type(),
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                // The Target Y coordinates texture. Its sample type must match
                // the dtype-specific texture format chosen above.
                binding: 4,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Texture {
                    sample_type: target_y_dtype.binding_sample_type(),
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
        let bind_group_layout = device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("LineLayer BGL"),
                entries: &bgl_entries,
            });

        let mut bg_entries = vec![
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&source_x_texture_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&source_y_texture_view),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(&target_x_texture_view),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(&target_y_texture_view),
            },
        ];
        for (i, tex) in color.textures.iter().enumerate() {
            bg_entries.push(wgpu::BindGroupEntry {
                binding: COLOR_BINDING_START + i as u32,
                resource: wgpu::BindingResource::TextureView(&tex.view),
            });
        }
        let bind_group = device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("LineLayer BG"),
                layout: &bind_group_layout,
                entries: &bg_entries,
            });

        // Inject the shared WGSL functions at compile time (see `crate::shader_modules`).
        let shader_source = ShaderBuilder::new(include_str!("shaders/line_layer.wgsl"))
            .inject_function("scale", common::SCALE)
            .inject_function("translate", common::TRANSLATE)
            .inject_function("get_aspect_ratio_mat", common::GET_ASPECT_RATIO_MAT)
            .inject_texture_sample_type("source_x_coords", source_x_dtype)
            .inject_texture_sample_type("source_y_coords", source_y_dtype)
            .inject_texture_sample_type("target_x_coords", target_x_dtype)
            .inject_texture_sample_type("target_y_coords", target_y_dtype)
            // Color-mode specialization: the flat-index texel helper plus the
            // assembled color module (bindings + `get_stroke_color`).
            .inject_function("flat_texel_coord", common::FLAT_TEXEL_COORD)
            .define("color_module", &color.wgsl)
            .build();
        let shader = device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("line_layer.wgsl"),
                source: wgpu::ShaderSource::Wgsl(shader_source.into()),
            });

        let render_pipeline_layout = device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("LineLayer PLD"),
                bind_group_layouts: &[Some(&bind_group_layout)],
                immediate_size: 0,
            });

        // TODO: Extract the shared render pipeline logic. There is a lot of duplication here.
        let render_pipeline = device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("LineLayer RPD"),
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
                        //blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
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
impl DrawToRasterCpu for LineLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for LineLayer {
    async fn draw(&self, ctx: &mut SvgContext) {
        let Self { layer_params, view_params } = self;

        // Iterate over the data points and create SVG elements.
        let n = layer_params.source_position_x.len();

        // Quantitative normalization domain, computed once for the whole layer.
        let quant_domain = match &layer_params.stroke_color {
            ColorMode::Quantitative(params) => quantitative_domain(params),
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
            let source_x = layer_params.source_position_x.get_f32(i);
            let source_y = layer_params.source_position_y.get_f32(i);
            let target_x = layer_params.target_position_x.get_f32(i);
            let target_y = layer_params.target_position_y.get_f32(i);

            // Convert data coordinates to pixel coordinates within the layer area.
            let (source_x_px, source_y_px) = get_point_position(
                source_x,
                source_y,
                layer_w,
                layer_h,
                &camera_view,
                layer_params.data_unit_mode_x,
                layer_params.data_unit_mode_y,
                view_params.aspect_ratio_mode,
                view_params.aspect_ratio_alignment_mode,
                Some(&model_matrix_raw),
            );
            let (target_x_px, target_y_px) = get_point_position(
                target_x,
                target_y,
                layer_w,
                layer_h,
                &camera_view,
                layer_params.data_unit_mode_x,
                layer_params.data_unit_mode_y,
                view_params.aspect_ratio_mode,
                view_params.aspect_ratio_alignment_mode,
                Some(&model_matrix_raw),
            );

            let color = TwoColor::Rgb(cpu_fill_color(&layer_params.stroke_color, i, quant_domain));

            svg_elements.push(TwoElement::Line(TwoLine {
                x1: source_x_px as f64,
                y1: (layer_h - source_y_px) as f64,
                x2: target_x_px as f64,
                y2: (layer_h - target_y_px) as f64,
                stroke: Some(color),
                linewidth: layer_params.line_width as f64,
                // TODO: more params
                ..Default::default()
            }));
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
        layer_type_name: "LineLayer",
        create_layer: |value, view_params| {
            let params: LineLayerParams = serde_json::from_value(value).unwrap();
            Box::new(LineLayer::new(view_params.clone(), params))
        },
    }
}

impl PickableLayer for LineLayer {}
