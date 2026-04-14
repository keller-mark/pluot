// Inspired by the DeckGL RectLayer.
// Reference: https://deck.gl/docs/api-reference/layers/line-layer

use encase::{ShaderType, UniformBuffer};
use glam::{Mat4, Vec2, Vec4};
use serde::{Deserialize, Serialize};
use std::sync::{Arc};

use crate::render_traits::{
    AspectRatioMode, AspectRatioAlignmentMode, DrawToRasterGpu, DrawToRasterCpu, DrawToSvg, MarginParams, PickableLayer, PreparedLayer, UnitsMode, ViewParams,
};
use crate::positioning::get_point_position;
use crate::render_types::{CpuContext, CpuRenderPass, PrepareResult, RenderResult};
use crate::render_types::GpuContext;
use crate::two::shapes::{
    TwoCircle, TwoElement, TwoGroup, TwoLine, TwoPath, TwoRectangle, TwoText,
};
use crate::two::svg::{update_svg, SvgContext};
use crate::wgpu;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RectLayerParams {
    pub layer_id: String,
    // If None, assume margin: 0 in all directions.
    pub bounds: Option<MarginParams>,
    pub data_unit_mode_x: UnitsMode,
    pub data_unit_mode_y: UnitsMode,

    // TODO: filled vs. stroked option

    pub stroke_width: f32,
    pub stroke_width_unit_mode: UnitsMode, // TODO: split into X and Y parts?

    // TODO(ref): pass in references instead of owned Vecs?
    // Would this cause issues when using serde to create layers based on JSON params?
    // TODO: improve naming here - should these be "source_x", "source_y", etc?
    pub position_x0: Arc<Vec<f32>>, // TODO: generalize to other numeric dtypes?
    pub position_y0: Arc<Vec<f32>>,
    // TODO: accept x/y/width/height instead?
    pub position_x1: Arc<Vec<f32>>,
    pub position_y1: Arc<Vec<f32>>,
    pub labels_vec: Arc<Vec<i32>>,
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
    stroke_width: f32,                // width of each line
    stroke_width_unit_mode: u32,      // 0 = pixels, 1 = data units
    aspect_ratio_mode: u32,           // 0 = ignore, 1 = contain, 2 = cover
    aspect_ratio_alignment_mode: u32, // 0 = center, 1 = start, 2 = end
    color: Vec4,                      // rgba color for points
}

// We extract this function for reuse in derived line layers (e.g., ZarrRectLayer).
// TODO: is this the best way to share this logic?
// TODO: just pass view_params and layer_params here? But layer_params contains data too, which for some layers is not provided via constructor params...
pub async fn base_draw_rect_layer(
    gpu_context: &GpuContext<'_>,
    pass: &mut wgpu::RenderPass<'_>,
    view_params: &ViewParams,
    layer_params: &RectLayerParams,
) {
    let GpuContext { device, queue } = gpu_context;
    // TODO: can more of this be memoized/cached? Which parts need to be re-executed every draw call?
    let position_x0_bytes = bytemuck::cast_slice(&layer_params.position_x0);
    let position_y0_bytes = bytemuck::cast_slice(&layer_params.position_y0);

    let position_x1_bytes = bytemuck::cast_slice(&layer_params.position_x1);
    let position_y1_bytes = bytemuck::cast_slice(&layer_params.position_y1);

    // More efficient version that eliminates intermediate vectors and redundant operations
    let n = layer_params.labels_vec.len();

    // Convert to f32 and cast to bytes directly - no for loop needed
    // let labels_i32: Vec<i32> = layer_params.labels_vec.iter().map(|&c| c as i32).collect();
    let labels_bytes: &[u8] = bytemuck::cast_slice(&layer_params.labels_vec);

    // Create separate buffers for X and Y coordinates
    let position_x0_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("x0 Coordinates Storage Buffer"),
        size: position_x0_bytes.len() as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&position_x0_buffer, 0, position_x0_bytes);

    let position_y0_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("y0 Coordinates Storage Buffer"),
        size: position_y0_bytes.len() as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&position_y0_buffer, 0, position_y0_bytes);

    let position_x1_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("x1 Coordinates Storage Buffer"),
        size: position_x1_bytes.len() as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&position_x1_buffer, 0, position_x1_bytes);

    let position_y1_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("y1 Coordinates Storage Buffer"),
        size: position_y1_bytes.len() as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&position_y1_buffer, 0, position_y1_bytes);

    let labels_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Class labels Storage Buffer"),
        size: labels_bytes.len() as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&labels_buffer, 0, labels_bytes);

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
        stroke_width: layer_params.stroke_width,
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
        color: Vec4::from_array([1.0, 0.0, 0.0, 1.0]),
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

    // Create bind group layout and bind group for positions + uniforms
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("RectLayer BGL"),
        entries: &[
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
                // The Source X coordinates buffer.
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
                // The Source Y coordinates buffer.
                binding: 2,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                // The Target X coordinates buffer.
                binding: 3,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                // The Target Y coordinates buffer.
                binding: 4,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                // The class labels coordinates buffer.
                binding: 5,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("RectLayer BG"),
        layout: &bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: position_x0_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: position_y0_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: position_x1_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: position_y1_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: labels_buffer.as_entire_binding(),
            },
        ],
    });

    let shader = device.create_shader_module(wgpu::include_wgsl!("shaders/rect_layer.wgsl"));

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
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                //blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                blend: Some(wgpu::BlendState {
                    color: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::SrcAlpha,
                        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                        operation: wgpu::BlendOperation::Add,
                    },
                    alpha: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::SrcAlpha,
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

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for RectLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        base_draw_rect_layer(
            gpu_context, pass,
            &self.view_params,
            &self.layer_params,
        )
        .await;
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for RectLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

pub fn base_draw_rect_layer_svg(
    view_params: &ViewParams,
    layer_params: &RectLayerParams,
) -> Vec<TwoElement> {
    // Iterate over the data points and create SVG elements.
    let n = layer_params.labels_vec.len();

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
    // End TODO

    let mut svg_elements: Vec<TwoElement> = Vec::with_capacity(n);
    for i in 0..n {
        let source_x = layer_params.position_x0[i];
        let source_y = layer_params.position_y0[i];
        let target_x = layer_params.position_x1[i];
        let target_y = layer_params.position_y1[i];

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
            None,
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
            None,
        );

        let rect_height = (target_y_px - source_y_px).abs();

        // Create a circle or square element based on point_shape_mode.
        svg_elements.push(TwoElement::Rectangle(TwoRectangle {
            x: source_x_px.min(target_x_px) as f64,
            y: ((layer_h - source_y_px.min(target_y_px)) - rect_height) as f64,
            width: (target_x_px - source_x_px).abs() as f64,
            height: rect_height as f64,
            linewidth: layer_params.stroke_width as f64,
            // TODO: more params
            ..Default::default()
        }));
    }

    // Insert rects into an SVG group with a transform and clipping to handle margins,
    // similar to the usage of scissor rect and viewport in the Canvas rendering.
    let layer_group_vec = vec![TwoElement::Group(TwoGroup {
        elements: svg_elements,
        translate: Some((margin_left, margin_top)),
        layer_id: Some(layer_params.layer_id.clone()),
        // TODO: check how clip_rect interacts with the translate
        clip_rect: Some((0.0, 0.0, layer_w as f64, layer_h as f64)),
        ..Default::default()
    })];

    return layer_group_vec;
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for RectLayer {
    async fn draw(&self, ctx: &mut SvgContext) {
        let svg_elements = base_draw_rect_layer_svg(
            &self.view_params,
            &self.layer_params,
        );
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
