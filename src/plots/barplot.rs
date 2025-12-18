use std::borrow::Cow;

use crate::log;
use crate::wgpu;
use encase::{ShaderType, UniformBuffer};
use glam::{Mat4, Vec2, Vec4};
/*
use vello::{
    peniko::{Blob, Brush, Color, Fill, Font},
    kurbo::{Affine, Circle, Ellipse, Line, RoundedRect, Stroke},
    AaConfig, AaSupport, Renderer, RendererOptions, RenderParams, Scene,
};
*/
use crate::params::{PlotParams, RenderContext, RenderResult};

use crate::d3::axis::{Axis, AxisOrientation};
use crate::d3::scale::{LinearRangeable, ScaleBand, ScaleLinear, Scaleable, Tickable};
use crate::two::shapes::{
    TwoCircle, TwoElement, TwoGroup, TwoLine, TwoPath, TwoRectangle, TwoText,
};

#[derive(ShaderType, Debug)]
struct BarPlotUniforms {
    viewport_size: Vec2, // (width, height) in pixels
    plot_margin: Vec4,   // (top, right, bottom, left) in pixels
    camera_view: Mat4,   // mat4x4<f32>,
    bar_padding_px: f32, // bar plot padding on left/right in pixels
    bar_size_px: f32,    // bar width in pixels
}

pub async fn render_barplot(
    context: &mut RenderContext<'_>,
    encoder: &mut wgpu::CommandEncoder,
) -> RenderResult {
    // Get x and y data from the Zarr store.
    let store = context.store;
    let height = context.params.height as f64;
    let width = context.params.width as f64;

    let margin_top = context.params.margin_top.unwrap_or(0.0) as f64;
    let margin_right = context.params.margin_right.unwrap_or(0.0) as f64;
    let margin_bottom = context.params.margin_bottom.unwrap_or(0.0) as f64;
    let margin_left = context.params.margin_left.unwrap_or(0.0) as f64;

    let PlotParams::BarPlot(barplot_params) = &context.params.plot_params else {
        panic!("Expected bar plot params");
    };

    let x_array_path = &barplot_params.x_key.as_ref();
    let y_array_path = &barplot_params.y_key.as_ref();

    let x_array_future = zarrs::array::Array::async_open(store.clone(), x_array_path);
    let y_array_future = zarrs::array::Array::async_open(store.clone(), y_array_path);

    // Wait for all futures to complete
    let arr_open_results = futures::join!(x_array_future, y_array_future);

    let x_array = arr_open_results.0.unwrap();
    let y_array = arr_open_results.1.unwrap();

    let x_subset = x_array.subset_all();
    let y_subset = y_array.subset_all();

    // Use futures::join! to run the async retrievals in parallel, similar to Promise.all in JS.
    let (x_result, y_result) = futures::join!(
        x_array.async_retrieve_array_subset_ndarray::<String>(&x_subset),
        y_array.async_retrieve_array_subset_ndarray::<i64>(&y_subset),
    );

    // Print the Zarr.json metadata to the JS console.
    // log(&x_array.metadata().to_string_pretty());

    // Read the whole array
    let x_vec = x_result.unwrap();
    let y_vec = y_result.unwrap();

    // TODO: how best to obtain a Vec of strings?
    // See alternative at https://github.com/zarrs/zarrs/blob/b1a7a19fd249eca1edce493081aed669e6fd2463/zarrs/examples/array_write_read_string.rs#L98
    let x_str_vec = x_vec.iter().map(|s| s.to_string()).collect::<Vec<String>>();

    log(&x_str_vec[0]);

    // More efficient version that eliminates intermediate vectors and redundant operations
    let n = x_vec.len();
    assert_eq!(n, y_vec.len(), "x and y data must have the same length");

    // Create uniforms matching the WGSL layout

    // Note: WebGPU's shading language (WGSL) treats matrices as column-major.
    let camera_view = context.params.camera_view.unwrap_or([
        // Column 0
        1.0, 0.0, 0.0, 0.0, // Column 1
        0.0, 1.0, 0.0, 0.0, // Column 2
        0.0, 0.0, 1.0, 0.0, // Column 3
        0.0, 0.0, 0.0, 1.0,
    ]);

    let zoom = camera_view[0]; // Assuming uniform scaling in x/y, take the first element (x scaling).
    let translate_x = camera_view[12];
    let translate_y = camera_view[13];

    let min_x = (-translate_x - 1.0) / zoom; // translation of (x=-1)
    let max_x = (-translate_x + 1.0) / zoom; // translation of (x=1)
    let min_y = (-translate_y - 1.0) / zoom; // translation of (y=-1)
    let max_y = (-translate_y + 1.0) / zoom; // translation of (y=1)

    let viewport_w = context.params.width as f32;
    let viewport_h = context.params.height as f32;

    // Create a scale for the categorical/string X values.
    let mut x_scale = ScaleBand::new();
    // TODO: set padding in pixel units rather than as a fraction.
    x_scale.set_padding_outer(0.1);
    x_scale.set_padding_inner(0.1);
    x_scale.set_domain(x_str_vec.clone());
    x_scale.set_range((margin_left, width - margin_right));

    // Create a linear scale for the Y values.
    let mut y_scale = ScaleLinear::new();
    y_scale.set_domain((0.0 as f64, 100.0 as f64));
    y_scale.set_range((height - margin_bottom, margin_top)); // Inverted range

    // Convert to f32 and cast to bytes directly - no for loop needed
    let x_f32: Vec<f32> = x_str_vec.iter().map(|x| x_scale.scale(x) as f32).collect();
    let y_f32: Vec<f32> = y_vec
        .iter()
        .map(|&y| y_scale.scale(&(y as f64)) as f32)
        .collect();

    let x_bytes = bytemuck::cast_slice(&x_f32);
    let y_bytes = bytemuck::cast_slice(&y_f32);

    // Create separate buffers for X and Y coordinates
    let x_buffer = context.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("X Coordinates Storage Buffer"),
        size: x_bytes.len() as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    context.queue.write_buffer(&x_buffer, 0, &x_bytes);

    let y_buffer = context.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Y Coordinates Storage Buffer"),
        size: y_bytes.len() as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    context.queue.write_buffer(&y_buffer, 0, &y_bytes);

    // Construct the uniform struct using Encase.
    let uniform_struct = BarPlotUniforms {
        camera_view: Mat4::from_cols_array(&camera_view),
        plot_margin: Vec4::from_array([
            // top, right, bottom, left
            margin_top as f32,
            margin_right as f32,
            margin_bottom as f32,
            margin_left as f32,
        ]),
        viewport_size: Vec2::new(viewport_w, viewport_h),
        bar_padding_px: x_scale.get_padding_outer() as f32,
        bar_size_px: x_scale.bandwidth() as f32,
    };

    let mut buffer = UniformBuffer::new(Vec::<u8>::new());
    buffer.write(&uniform_struct).unwrap();
    let uniform_bytes = buffer.into_inner();

    // TODO: use create_buffer_init instead?
    let uniform_buffer = context.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Uniform Buffer"),
        size: uniform_bytes.len() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    context
        .queue
        .write_buffer(&uniform_buffer, 0, &uniform_bytes);

    // Create bind group layout and bind group for positions + uniforms
    let bind_group_layout =
        context
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Scatter BGL"),
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
                ],
            });
    let bind_group = context
        .device
        .create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Barplot BG"),
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
            ],
        });

    let shader = context
        .device
        .create_shader_module(wgpu::include_wgsl!("shaders/barplot.wgsl"));

    let render_pipeline_layout =
        context
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[&bind_group_layout],
                immediate_size: 0,
            });

    // TODO: Extract the shared render pipeline and render pass logic. There is a lot of duplication here.
    let render_pipeline = context
        .device
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
                    format: context.texture_desc.format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
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

    // 1) Offscreen scatterplot target
    let barplot_tex = context.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Barplot Offscreen Texture"),
        size: wgpu::Extent3d {
            width: context.params.width,
            height: context.params.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: context.texture_desc.format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let barplot_view = barplot_tex.create_view(&wgpu::TextureViewDescriptor::default());

    {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &barplot_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    // Set a white background for the bar plot.
                    // TODO: make this configurable.
                    load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        render_pass.set_pipeline(&render_pipeline);
        render_pass.set_bind_group(0, &bind_group, &[]);

        // TODO: Would it be more efficient to store the point X/Y/Size/Opacity/Color info in textures, as done by Regl-Scatterplot?
        // (As opposed to using instancing)
        // References:
        // - https://github.com/flekschas/regl-scatterplot/blob/90f0c951233b20bebd4fd1cb15ce1c4128ce9edf/src/point.vs#L43
        // - https://github.com/flekschas/regl-scatterplot/blob/90f0c951233b20bebd4fd1cb15ce1c4128ce9edf/src/index.js#L1938
        render_pass.draw(0..4, 0..(n as u32));

        // End the renderpass.
        drop(render_pass);
    }

    // Construct the X-axis:
    let x_axis = Axis::new(AxisOrientation::Bottom);
    let x_axis_elements = x_axis.generate_elements(&x_scale);

    let x_axis_group = TwoElement::Group(TwoGroup {
        elements: x_axis_elements,
        translate: Some((0.0, height - margin_bottom)),
        ..Default::default()
    });

    // Construct the Y-axis:
    let y_axis = Axis::new(AxisOrientation::Left);
    let y_axis_elements = y_axis.generate_elements(&y_scale);

    let y_axis_group = TwoElement::Group(TwoGroup {
        elements: y_axis_elements,
        translate: Some((margin_left, 0.0)),
        ..Default::default()
    });

    let axis_elements = vec![x_axis_group, y_axis_group];

    // Render the X and Y axes:
    crate::two::canvas::render_shapes(context, encoder, &axis_elements);

    crate::render::overlay_pass(context, encoder, &barplot_tex);

    RenderResult {
        bailed_early: false,
    }
}
