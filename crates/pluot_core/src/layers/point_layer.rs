// Inspired by the DeckGL PointLayer
// Reference: https://deck.gl/docs/api-reference/layers/scatterplot-layer

use encase::{ShaderType, UniformBuffer};
use glam::{Mat4, Vec2, Vec4};
use serde::{Deserialize, Serialize};
use std::sync::{Arc};

use crate::render_traits::{AspectRatioMode, DrawToRasterGpu, DrawToRasterCpu, DrawToSvg, MarginParams, PreparedLayer, UnitsMode, ViewParams};
use crate::render_types::{CpuContext, CpuRenderPass, PrepareResult, RenderResult};
use crate::render_types::GpuContext;
use crate::wgpu;
use crate::cache::{use_memo_vec_f32, use_memo_vec_i32};
use crate::two::shapes::{TwoCircle, TwoElement, TwoGroup, TwoLine, TwoPath, TwoRectangle, TwoText};
use crate::two::svg::{update_svg, SvgContext};
use crate::positioning::get_point_position;


#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum PointShapeMode {
    // 0: square (basically no-op in fragment shader)
    Square,
    // 1: circles (convert square to circle in fragment shader)
    Circle,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PointLayerParams {
    pub layer_id: String,
    // If None, assume margin: 0 in all directions.
    pub bounds: Option<MarginParams>,
    pub data_unit_mode: UnitsMode,
    pub point_radius: f32,
    pub point_radius_unit_mode: UnitsMode,
    pub point_shape_mode: PointShapeMode,

    pub position_x: Arc<Vec<f32>>, // TODO: generalize to other numeric dtypes?
    pub position_y: Arc<Vec<f32>>,
    // TODO: improve naming here
    pub labels_vec: Arc<Vec<i32>>,
}

// TODO: defaults for params?

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
        if (layer_params.point_radius_unit_mode == UnitsMode::Data && layer_params.data_unit_mode == UnitsMode::Pixels) {
            panic!("point_radius_unit_mode cannot be 'data' when data_unit_mode is 'pixels'");
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
    data_unit_mode: u32, // 0 = pixels, 1 = data units
    point_radius: f32,  // radius of each point
    point_radius_unit_mode: u32, // 0 = pixels, 1 = data units
    point_shape_mode: u32, // 0 = square, 1 = circle
    aspect_ratio_mode: u32, // 0 = ignore, 1 = contain, 2 = cover
    aspect_ratio_alignment_mode: u32, // 0 = center, 1 = start, 2 = end
    color: Vec4,         // rgba color for points
}

// We extract this function for reuse in derived scatterplot layers (e.g., ZarrPointLayer).
// TODO: is this the best way to share this logic?
// See https://www.youtube.com/watch?v=Phk0C-kLlho
// See https://github.com/linebender/xilem/blob/main/xilem_core/src/views/any_view.rs

// TODO: just pass view_params and layer_params here? But layer_params contains data too, which for some layers is not provided via constructor params...

pub async fn base_draw_point_layer(
    gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass<'_>,
    view_params: &ViewParams,
    layer_params: &PointLayerParams,
) {
    let GpuContext { device, queue } = gpu_context;

    // This bytemuck::cast_slice does not clone,
    // it just reinterprets the same memory.
    let x_bytes = bytemuck::cast_slice(&layer_params.position_x);
    let y_bytes = bytemuck::cast_slice(&layer_params.position_y);

    // More efficient version that eliminates intermediate vectors and redundant operations
    let n = layer_params.labels_vec.len();

    // Convert to f32 and cast to bytes directly - no for loop needed
    //let labels_i32: Vec<i32> = data.labels_arr.iter().map(|&c| c as i32).collect();
    let labels_bytes: &[u8] = bytemuck::cast_slice(&layer_params.labels_vec);

    // TODO: can more of this be memoized/cached?
    // Which parts need to be re-executed every draw call? Which parts have high overhead?

    // Create separate buffers for X and Y coordinates
    let x_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("X Coordinates Storage Buffer"),
        size: x_bytes.len() as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&x_buffer, 0, &x_bytes);

    let y_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Y Coordinates Storage Buffer"),
        size: y_bytes.len() as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&y_buffer, 0, &y_bytes);

    let labels_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Class labels Storage Buffer"),
        size: labels_bytes.len() as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&labels_buffer, 0, &labels_bytes);

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
        data_unit_mode: match layer_params.data_unit_mode {
            UnitsMode::Pixels => 0,
            UnitsMode::Data => 1,
        },
        point_radius: layer_params.point_radius,
        point_radius_unit_mode: match layer_params.point_radius_unit_mode {
            UnitsMode::Pixels => 0,
            UnitsMode::Data => 1,
        },
        point_shape_mode: match layer_params.point_shape_mode {
            PointShapeMode::Square => 0,
            PointShapeMode::Circle => 1,
        },
        aspect_ratio_mode: match view_params.aspect_ratio_mode {
            AspectRatioMode::Ignore => 0,
            AspectRatioMode::Contain => 1,
            AspectRatioMode::Cover => 2,
        },
        aspect_ratio_alignment_mode: 0, // center. TODO
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
    let bind_group_layout = device
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("PointLayer BGL"),
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
                    // The X coordinates buffer.
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
                    // The Y coordinates buffer.
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
                    // The class labels coordinates buffer.
                    binding: 3,
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
    let bind_group = device
        .create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("PointLayer BG"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: x_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: y_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: labels_buffer.as_entire_binding(),
                },
            ],
        });

    let shader = device
        .create_shader_module(wgpu::include_wgsl!("shaders/point_layer.wgsl"));

    let render_pipeline_layout = device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Render Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
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
impl DrawToRasterGpu for PointLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        base_draw_point_layer(
            gpu_context, pass,
            &self.view_params,
            &self.layer_params,
        ).await;
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for PointLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

pub fn base_draw_point_layer_svg(
    view_params: &ViewParams,
    layer_params: &PointLayerParams,
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
        let x = layer_params.position_x[i];
        let y = layer_params.position_y[i];

        // Convert data coordinates to pixel coordinates within the layer area.
        let (px, py) = get_point_position(
            x,
            y,
            layer_w,
            layer_h,
            &camera_view,
            layer_params.data_unit_mode,
            view_params.aspect_ratio_mode,
            0, // TODO: pass enum value for aspect_ratio_alignment_mode
        );

        let point_radius = layer_params.point_radius;

        // Create a circle or square element based on point_shape_mode.
        svg_elements.push(match layer_params.point_shape_mode {
            PointShapeMode::Circle => TwoElement::Circle(TwoCircle {
                x: px as f64,
                y: (layer_h - py) as f64,
                radius: point_radius as f64,
                // TODO: more params
                ..Default::default()
            }),
            PointShapeMode::Square => TwoElement::Rectangle(TwoRectangle {
                x: (px - point_radius) as f64,
                y: ((layer_h - py) - point_radius) as f64,
                width: (point_radius * 2.0) as f64,
                height: (point_radius * 2.0) as f64,
                // TODO: more params
                ..Default::default()
            })
        });
    }

    // Insert rects into an SVG group with a transform and clipping to handle margins,
    // similar to the usage of scissor rect and viewport in the Canvas rendering.
    let layer_group_vec = vec![
        TwoElement::Group(TwoGroup {
            elements: svg_elements,
            translate: Some((margin_left, margin_top)),
            layer_id: Some(layer_params.layer_id.clone()),
            // TODO: check how clip_rect interacts with the translate
            clip_rect: Some((0.0, 0.0, layer_w as f64, layer_h as f64)),
            ..Default::default()
        })
    ];

    return layer_group_vec;
}


#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for PointLayer {
    async fn draw(&self, ctx: &mut SvgContext) {
        let svg_elements = base_draw_point_layer_svg(
            &self.view_params,
            &self.layer_params,
        );
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
