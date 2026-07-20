// Inspired by the DeckGL RectLayer.
// Reference: https://deck.gl/docs/api-reference/layers/line-layer

use encase::{ShaderType, UniformBuffer};
use glam::{Mat4, Vec2, Vec4};
use serde::{Deserialize, Serialize};
use std::sync::{Arc};

use crate::render_traits::{
    AspectRatioAlignmentMode, AspectRatioMode, ColorMode, DrawToRasterCpu, DrawToRasterGpu, DrawToSvg, MarginParams, PickableLayer, PreparedLayer, UnitsMode, ViewParams
};
use crate::positioning::get_point_position;
use crate::numeric_data::NumericData;
use crate::color_mode::{cpu_fill_color, prepare_color_mode, quantitative_domain};
use crate::render_types::{CpuContext, CpuRenderPass, PrepareResult, RenderResult};
use crate::shader_modules::{common, ShaderBuilder};
use crate::render_types::GpuContext;
use crate::two::shapes::{
    TwoCircle, TwoColor, TwoElement, TwoGroup, TwoLine, TwoPath, TwoRectangle, TwoText
};
use crate::two::svg::{update_svg, SvgContext};
use crate::wgpu;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct RectLayerParams {
    pub layer_id: String,
    // If None, assume margin: 0 in all directions.
    pub bounds: Option<MarginParams>,
    pub data_unit_mode_x: UnitsMode,
    pub data_unit_mode_y: UnitsMode,

    // If None, assume filled
    pub stroke_width: Option<f32>,
    pub stroke_width_unit_mode: UnitsMode, // TODO: split into X and Y parts?

    pub model_matrix: Option<[f32; 16]>, // Column-major 4x4 matrix

    // How to color each rect. See [`ColorMode`]: modes carrying `NumericData`
    // (instanced/categorical/quantitative) supply one or more per-element value
    // arrays, which are uploaded to the GPU as textures at draw time.
    pub fill_color: Option<ColorMode>,

    // TODO: improve naming here - should these be "source_x", "source_y", etc?
    // Each may be any supported numeric dtype (8-64 bit int/uint, or 32/64-bit
    // float), and may differ across the four arrays. The data is uploaded to
    // the GPU as a texture at its native width wherever possible (see
    // `NumericData::create_data_texture`).
    pub position_x0: NumericData,
    pub position_y0: NumericData,
    // TODO: accept x/y/width/height instead?
    pub position_x1: NumericData,
    pub position_y1: NumericData,
}

impl Default for RectLayerParams {
    fn default() -> Self {
        Self {
            layer_id: "".to_string(),
            bounds: None,
            data_unit_mode_x: UnitsMode::Data,
            data_unit_mode_y: UnitsMode::Data,
            stroke_width: None,
            stroke_width_unit_mode: UnitsMode::Pixels,
            model_matrix: None,
            fill_color: None,
            position_x0: NumericData::Float32(Arc::new(vec![])),
            position_y0: NumericData::Float32(Arc::new(vec![])),
            position_x1: NumericData::Float32(Arc::new(vec![])),
            position_y1: NumericData::Float32(Arc::new(vec![])),
        }
    }
}

// TODO: consider eliminating once we have a PolygonLayer?
// (or implementing using the eventual PolygonLayer internally)
pub struct RectLayer {
    view_params: ViewParams,
    layer_params: RectLayerParams,
}

impl RectLayer {
    pub fn new(view_params: ViewParams, layer_params: RectLayerParams) -> Self {
        // Error if line_width_unit_mode is "data" when data_unit_mode is "pixels".
        if layer_params.stroke_width_unit_mode == UnitsMode::Data && (layer_params.data_unit_mode_x == UnitsMode::Pixels || layer_params.data_unit_mode_y == UnitsMode::Pixels) {
            panic!("line_width_unit_mode cannot be 'data' when data_unit_mode is 'pixels'");
        }
        // Validate the lengths of things.
        let n = layer_params.position_x0.len();
        if let Some(fill_color) = &layer_params.fill_color {
            fill_color.validate_len(n);
        }
        for (name, len) in [
            ("position_y0", layer_params.position_y0.len()),
            ("position_x1", layer_params.position_x1.len()),
            ("position_y1", layer_params.position_y1.len()),
        ] {
            assert_eq!(
                len, n,
                "{name} has length {len} but position_x0 has length {n}",
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
impl PreparedLayer for RectLayer {
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
struct RectLayerUniforms {
    layer_size: Vec2,                 // (layer_width, layer_height) in pixels
    camera_view: Mat4,                // mat4x4<f32>,
    data_unit_mode_x: u32,            // 0 = pixels, 1 = data units
    data_unit_mode_y: u32,            // 0 = pixels, 1 = data units
    filled: u32,                      // 0: false, 1: true
    stroke_width: f32,                // width of each line
    stroke_width_unit_mode: u32,      // 0 = pixels, 1 = data units
    aspect_ratio_mode: u32,           // 0 = ignore, 1 = contain, 2 = cover
    aspect_ratio_alignment_mode: u32, // 0 = center, 1 = start, 2 = end
    model_matrix: Mat4, // mat4x4<f32> for affine transformations of the image.
    fill_color_mode: u32,             // see ColorMode::shader_mode()
    fill_color: Vec4,                 // rgba color used by the UniformRgb mode
    fill_color_reverse: u32,          // 1 = reverse the quantitative colormap
    fill_color_domain: Vec2,          // (min, max) normalization domain for quantitative mode
}

// First bind-group binding index used for color-mode value/palette texture(s).
// Bindings 0-4 are the uniforms buffer and the four position textures.
const COLOR_BINDING_START: u32 = 5;

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for RectLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        let GpuContext { device, queue } = gpu_context;
        let Self { layer_params, view_params } = self;

        // Upload the four corner coordinate arrays into single-channel 2D
        // textures, each at its native byte width wherever possible (8/16/32-bit
        // are zero-copy; only 64-bit dtypes are narrowed to 32 bits). Each array is
        // uploaded independently so they may have different dtypes; the shader
        // reads each texel via its instance index and widens it to f32. See
        // `NumericData::create_data_texture`.
        let (position_x0_texture_view, position_x0_dtype) =
            layer_params.position_x0.create_data_texture(device, queue, "x0 Coordinates Texture");
        let (position_y0_texture_view, position_y0_dtype) =
            layer_params.position_y0.create_data_texture(device, queue, "y0 Coordinates Texture");
        let (position_x1_texture_view, position_x1_dtype) =
            layer_params.position_x1.create_data_texture(device, queue, "x1 Coordinates Texture");
        let (position_y1_texture_view, position_y1_dtype) =
            layer_params.position_y1.create_data_texture(device, queue, "y1 Coordinates Texture");

        // Number of rects to draw: one instance per element of the position arrays.
        let n = layer_params.position_x0.len();

        // Build the GPU-side color resources for the configured color mode. Modes
        // that carry per-element `NumericData` upload it as one or more textures
        // (bound from COLOR_BINDING_START onward) and contribute the WGSL
        // `get_fill_color` function injected into the shader below.
        let color = prepare_color_mode(device, queue, layer_params.fill_color.as_ref(), COLOR_BINDING_START);

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
        let uniform_struct = RectLayerUniforms {
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
            filled: if layer_params.stroke_width.is_none() { 1 } else { 0 },
            stroke_width: layer_params.stroke_width.unwrap_or(0.0),
            stroke_width_unit_mode: match layer_params.stroke_width_unit_mode {
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
                // The x0 coordinates texture. Its sample type must match
                // the dtype-specific texture format chosen above.
                binding: 1,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Texture {
                    sample_type: position_x0_dtype.binding_sample_type(),
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                // The y0 coordinates texture. Its sample type must match
                // the dtype-specific texture format chosen above.
                binding: 2,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Texture {
                    sample_type: position_y0_dtype.binding_sample_type(),
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                // The x1 coordinates texture. Its sample type must match
                // the dtype-specific texture format chosen above.
                binding: 3,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Texture {
                    sample_type: position_x1_dtype.binding_sample_type(),
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                // The y1 coordinates texture. Its sample type must match
                // the dtype-specific texture format chosen above.
                binding: 4,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Texture {
                    sample_type: position_y1_dtype.binding_sample_type(),
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
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("RectLayer BGL"),
            entries: &bgl_entries,
        });

        let mut bg_entries = vec![
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&position_x0_texture_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&position_y0_texture_view),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(&position_x1_texture_view),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(&position_y1_texture_view),
            },
        ];
        for (i, tex) in color.textures.iter().enumerate() {
            bg_entries.push(wgpu::BindGroupEntry {
                binding: COLOR_BINDING_START + i as u32,
                resource: wgpu::BindingResource::TextureView(&tex.view),
            });
        }
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("RectLayer BG"),
            layout: &bind_group_layout,
            entries: &bg_entries,
        });

        // Inject the shared WGSL functions at compile time (see `crate::shader_modules`).
        let shader_source = ShaderBuilder::new(include_str!("shaders/rect_layer.wgsl"))
            .inject_function("scale", common::SCALE)
            .inject_function("translate", common::TRANSLATE)
            .inject_function("get_aspect_ratio_mat", common::GET_ASPECT_RATIO_MAT)
            .inject_texture_sample_type("position_x0_coords", position_x0_dtype)
            .inject_texture_sample_type("position_y0_coords", position_y0_dtype)
            .inject_texture_sample_type("position_x1_coords", position_x1_dtype)
            .inject_texture_sample_type("position_y1_coords", position_y1_dtype)
            // Color-mode specialization: the flat-index texel helper plus the
            // assembled color module (bindings + `get_fill_color`).
            .inject_function("flat_texel_coord", common::FLAT_TEXEL_COORD)
            .define("color_module", &color.wgsl)
            .build();
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rect_layer.wgsl"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("RectLayer PLD"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        // TODO: Extract the shared render pipeline logic. There is a lot of duplication here.
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("RectLayer RPD"),
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
impl DrawToRasterCpu for RectLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for RectLayer {
    async fn draw(&self, ctx: &mut SvgContext) {
        let Self { layer_params, view_params } = self;

        // Iterate over the data points and create SVG elements.
        let n = layer_params.position_x0.len();

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
            let source_x = layer_params.position_x0.get_f32(i);
            let source_y = layer_params.position_y0.get_f32(i);
            let target_x = layer_params.position_x1.get_f32(i);
            let target_y = layer_params.position_y1.get_f32(i);

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

            let rect_height = (target_y_px - source_y_px).abs();

            let color = TwoColor::Rgb(cpu_fill_color(layer_params.fill_color.as_ref(), i, quant_domain));

            svg_elements.push(TwoElement::Rectangle(TwoRectangle {
                x: source_x_px.min(target_x_px) as f64,
                y: ((layer_h - source_y_px.min(target_y_px)) - rect_height) as f64,
                width: (target_x_px - source_x_px).abs() as f64,
                height: rect_height as f64,
                fill: if layer_params.stroke_width.is_none() {
                    Some(color.clone())
                } else { None },
                stroke: if layer_params.stroke_width.is_some() {
                    Some(color)
                } else { None },
                linewidth: layer_params.stroke_width.unwrap_or(0.0) as f64,
                ..Default::default()
            }));
        }

        // Insert rects into an SVG group with a transform and clipping to handle margins,
        // similar to the usage of scissor rect and viewport in the Canvas rendering.
        let svg_elements = vec![TwoElement::Group(TwoGroup {
            elements: svg_elements,
            translate: Some((margin_left, margin_top)),
            layer_id: Some(layer_params.layer_id.clone()),
            // TODO: check how clip_rect interacts with the translate
            clip_rect: Some((0.0, 0.0, layer_w as f64, layer_h as f64)),
            ..Default::default()
        })];

        update_svg(ctx, &svg_elements);
    }
}

inventory::submit! {
    crate::registry::LayerRegistration {
        layer_type_name: "RectLayer",
        create_layer: |value, view_params| {
            let params: RectLayerParams = serde_json::from_value(value).unwrap();
            Box::new(RectLayer::new(view_params.clone(), params))
        },
    }
}

impl PickableLayer for RectLayer {}
