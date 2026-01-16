// Inspired by the DeckGL LineLayer.
// Reference: https://deck.gl/docs/api-reference/layers/line-layer

// The line layer should support rendering lines between pairs of points,
// with customizable color and width.

// In the initial implementation, color and width will be uniform for all lines (i.e., set globally for the layer).
use encase::{ShaderType, UniformBuffer};
use glam::{Mat4, Vec2, Vec4};

use crate::layers::core::{DrawToCanvas, PreparedLayer, ViewParams, AspectRatioMode, UnitsMode, MarginParams};
use crate::wgpu;
use crate::cache::{use_memo_vec_f32, use_memo_vec_i32};


struct LineLayerData {
    // Lines are from source (x,y) to target (x,y).
    source_x_arr: Vec<f32>,
    source_y_arr: Vec<f32>,
    target_x_arr: Vec<f32>,
    target_y_arr: Vec<f32>,
    labels_arr: Vec<i32>,
}

pub struct LineLayer {
    view_params: ViewParams,
    layer_id: String,
    // If None, assume margin: 0 in all directions.
    bounds: Option<MarginParams>,

    data_unit_mode: UnitsMode,
    line_width: f32,
    line_width_unit_mode: UnitsMode,
    // Data will be None prior to runninng prepare().
    data: Option<LineLayerData>,
}

impl LineLayer {
    pub fn new(
        view_params: ViewParams,
        bounds: Option<MarginParams>,
        layer_id: String,
        data_unit_mode: UnitsMode,
        line_width: f32,
        line_width_unit_mode: UnitsMode,
        // TODO(ref): pass in references instead of owned Vecs?
        source_x_vec: Vec<f32>,
        source_y_vec: Vec<f32>,
        target_x_vec: Vec<f32>,
        target_y_vec: Vec<f32>,
        labels_vec: Vec<i32>,
    ) -> Self {
        Self {
            view_params,
            bounds,
            layer_id,
            data_unit_mode,
            line_width,
            line_width_unit_mode,
            data: Some(LineLayerData {
                source_x_arr: source_x_vec,
                source_y_arr: source_y_vec,
                target_x_arr: target_x_vec,
                target_y_arr: target_y_vec,
                labels_arr: labels_vec,
            }),
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for LineLayer {
    async fn prepare(&mut self) {

        // TODO: include the layer type in the memoization dependencies?
        // But what if we want multiple layers to be able to reuse the same cached data?
        // Then we should also avoid including the layer_id...

        // TODO: execute getters and cache the results.

        // For now, it is a no-op, since self.data is set in the constructor.
    }
}

#[derive(ShaderType, Debug)]
struct LineLayerUniforms {
    layer_size: Vec2, // (layer_width, layer_height) in pixels
    camera_view: Mat4,   // mat4x4<f32>,
    data_unit_mode: u32, // 0 = pixels, 1 = data units
    line_width: f32,  // width of each line
    line_width_unit_mode: u32, // 0 = pixels, 1 = data units
    aspect_ratio_mode: u32, // 0 = ignore, 1 = contain, 2 = cover
    aspect_ratio_alignment_mode: u32, // 0 = center, 1 = start, 2 = end
    color: Vec4,         // rgba color for points
}

// We extract this function for reuse in derived scatterplot layers (e.g., ZarrScatterplotLayer).
// TODO: is this the best way to share this logic?
pub async fn draw_line_layer(
    device: wgpu::Device, queue: wgpu::Queue, pass: &mut wgpu::RenderPass<'_>,
    data: &LineLayerData,
    view_params: &ViewParams,
    bounds: &Option<MarginParams>,
    data_unit_mode: &UnitsMode,
    line_width: f32,
    line_width_unit_mode: &UnitsMode,
) {
    // TODO: can more of this be memoized/cached? Which parts need to be re-executed every draw call?
    let source_x_bytes = bytemuck::cast_slice(&data.source_x_arr);
    let source_y_bytes = bytemuck::cast_slice(&data.source_y_arr);

    let target_x_bytes = bytemuck::cast_slice(&data.target_x_arr);
    let target_y_bytes = bytemuck::cast_slice(&data.target_y_arr);

    // More efficient version that eliminates intermediate vectors and redundant operations
    let n = data.labels_arr.len();

    // Convert to f32 and cast to bytes directly - no for loop needed
    let labels_i32: Vec<i32> = data.labels_arr.iter().map(|&c| c as i32).collect();
    let labels_bytes: &[u8] = bytemuck::cast_slice(&labels_i32);


    // Create separate buffers for X and Y coordinates
    let source_x_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Source X Coordinates Storage Buffer"),
        size: source_x_bytes.len() as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&source_x_buffer, 0, &source_x_bytes);

    let source_y_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Source Y Coordinates Storage Buffer"),
        size: source_y_bytes.len() as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&source_y_buffer, 0, &source_y_bytes);

    let target_x_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Target X Coordinates Storage Buffer"),
        size: target_x_bytes.len() as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&target_x_buffer, 0, &target_x_bytes);

    let target_y_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Target Y Coordinates Storage Buffer"),
        size: target_y_bytes.len() as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&target_y_buffer, 0, &target_y_bytes);

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
        data_unit_mode: match data_unit_mode {
            UnitsMode::Pixels => 0,
            UnitsMode::Data => 1,
        },
        line_width,
        line_width_unit_mode: match line_width_unit_mode {
            UnitsMode::Pixels => 0,
            UnitsMode::Data => 1,
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
            label: Some("LineLayer BGL"),
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
    let bind_group = device
        .create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("LineLayer BG"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: source_x_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: source_y_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: target_x_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: target_y_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: labels_buffer.as_entire_binding(),
                },
            ],
        });

    let shader = device
        .create_shader_module(wgpu::include_wgsl!("line_layer.wgsl"));

    let render_pipeline_layout = device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("LineLayer PLD"),
            bind_group_layouts: &[&bind_group_layout],
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
impl DrawToCanvas for LineLayer {
    async fn draw(&self, device: wgpu::Device, queue: wgpu::Queue, pass: &mut wgpu::RenderPass) {
        let data = self.data.as_ref().expect("Data was not prepared. Call prepare() first.");
        draw_line_layer(
            device, queue, pass,
            data,
            &self.view_params,
            &self.bounds,
            &self.data_unit_mode,
            self.line_width,
            &self.line_width_unit_mode,
        ).await;
    }
}

// TODO: Implement DrawToSvg.
