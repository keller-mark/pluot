use std::borrow::Cow;

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
use crate::d3::scale::{LinearRangeable, ScaleLinear, Tickable};
use crate::two::shapes::{
    TwoCircle, TwoElement, TwoGroup, TwoLine, TwoPath, TwoRectangle, TwoText,
};

#[derive(ShaderType, Debug)]
struct Scatterplot3dUniforms {
    camera_view: Mat4,   // mat4x4<f32>,
    point_size_px: f32,  // diameter in pixels
    viewport_size: Vec2, // (width, height) in pixels
    color: Vec4,         // rgba color for points
}

pub async fn render_scatterplot_3d(
    context: &mut RenderContext<'_>,
    encoder: &mut wgpu::CommandEncoder,
) -> RenderResult {
    // Get x and y data from the Zarr store.
    let store = context.store;

    let PlotParams::Scatterplot3d(scatterplot_params) = &context.params.plot_params else {
        panic!("Expected scatterplot params");
    };

    let x_array_path = &scatterplot_params.x_key.as_ref();
    let y_array_path = &scatterplot_params.y_key.as_ref();
    let z_array_path = &scatterplot_params.z_key.as_ref();
    let labels_array_path = scatterplot_params.color_key.as_ref().expect("Color key");

    let x_array_future = zarrs::array::Array::async_open(store.clone(), x_array_path);
    let y_array_future = zarrs::array::Array::async_open(store.clone(), y_array_path);
    let z_array_future = zarrs::array::Array::async_open(store.clone(), z_array_path);
    let labels_array_future = zarrs::array::Array::async_open(store.clone(), labels_array_path);

    // Wait for all futures to complete
    let arr_open_results = futures::join!(
        x_array_future,
        y_array_future,
        z_array_future,
        labels_array_future
    );

    let x_array = arr_open_results.0.unwrap();
    let y_array = arr_open_results.1.unwrap();
    let z_array = arr_open_results.2.unwrap();
    let labels_array = arr_open_results.3.unwrap();

    let x_subset = x_array.subset_all();
    let y_subset = y_array.subset_all();
    let z_subset = z_array.subset_all();
    let labels_subset = labels_array.subset_all();

    // Use futures::join! to run the async retrievals in parallel, similar to Promise.all in JS.
    let (x_result, y_result, z_result, labels_result) = futures::join!(
        x_array.async_retrieve_array_subset_ndarray::<f64>(&x_subset),
        y_array.async_retrieve_array_subset_ndarray::<f64>(&y_subset),
        z_array.async_retrieve_array_subset_ndarray::<f64>(&z_subset),
        labels_array.async_retrieve_array_subset_ndarray::<i64>(&labels_subset),
    );

    // Print the Zarr.json metadata to the JS console.
    // log(&x_array.metadata().to_string_pretty());

    // Read the whole array
    let x_vec = x_result.unwrap();
    let y_vec = y_result.unwrap();
    let z_vec = z_result.unwrap();
    let labels_vec = labels_result.unwrap();

    // More efficient version that eliminates intermediate vectors and redundant operations
    let n = x_vec.len();
    assert_eq!(n, y_vec.len(), "x and y data must have the same length");

    // Convert to f32 and cast to bytes directly - no for loop needed
    let x_f32: Vec<f32> = x_vec.iter().map(|&x| x as f32).collect();
    let y_f32: Vec<f32> = y_vec.iter().map(|&y| y as f32).collect();
    let z_f32: Vec<f32> = z_vec.iter().map(|&z| z as f32).collect();
    let labels_i32: Vec<i32> = labels_vec.iter().map(|&c| c as i32).collect();

    let x_bytes = bytemuck::cast_slice(&x_f32);
    let y_bytes = bytemuck::cast_slice(&y_f32);
    let z_bytes = bytemuck::cast_slice(&z_f32);
    let labels_bytes: &[u8] = bytemuck::cast_slice(&labels_i32);

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

    let z_buffer = context.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Z Coordinates Storage Buffer"),
        size: z_bytes.len() as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    context.queue.write_buffer(&z_buffer, 0, &z_bytes);

    let labels_buffer = context.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Class labels Storage Buffer"),
        size: labels_bytes.len() as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    context.queue.write_buffer(&labels_buffer, 0, &labels_bytes);

    // Create uniforms matching the WGSL layout
    // struct Uniforms {
    //   camera_view: mat4x4<f32>,
    //   point_size_px: f32,
    //   _pad0: f32,
    //   viewport_size: vec2<f32>,
    //   color: vec4<f32>
    // }

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

    // Convert zoom level to scale factor
    // scale_factor of 0 means zoom = 1.0 (no zoom)
    // scale_factor of 1 means zoom = 0.5 (zoomed out to half)
    // scale_factor of 2 means zoom = 0.25 (zoomed out to a quarter)
    // scale_factor of 3 means zoom = 0.125 (zoomed out to an eighth)

    // scale_factor of -1 means zoom = 2.0 (zoomed in to double)
    // scale_factor of -2 means zoom = 4.0 (zoomed in to quadruple)
    // scale_factor of -3 means zoom = 8.0 (zoomed in to octuple)
    let scale_factor = (1.0 / zoom).log2();

    // X translation interpretation:
    // A translate_x value of 1.0 means a point at x=-1.0 (left edge of viewport/screen-quad) is now at the center of the viewport.
    // A translate_x value of 2.0 means a point at x=-1.0 is now at the right edge of the viewport.
    // A translate_x value of -1.0 means a point at x=1.0 (right edge of viewport/screen-quad) is now at the center of the viewport.

    // Zoom interpretation:
    // A zoom value of 0.5 means that points are scaled down by half, so a point at x=-1.0 is now at x=-0.5, and a point at x=1.0 is now at x=0.5.
    // A zoom value of 0.25 means that points are scaled down by a quarter, so a point at x=-1.0 is now at x=-0.25, and a point at x=1.0 is now at x=0.25.

    // Zoom and translation combined interpretation:
    // A translate_x value of 0.5 when zoom = 0.5 means a point at x=-1.0 is now at the center of the viewport, and a point at x=1.0 is now at the right of the viewport.
    // When zoom = 0.5 AND translate_x = 0.5 AND translate_y = 0.5, all four screen-quad [-1 to 1] corner points are in the top right quadrant of the viewport.
    // When zoom = 0.5 AND translate_x = -0.5 AND translate_y = -0.5, all four screen-quad [-1 to 1] corner points are in the bottom left quadrant of the viewport.

    let x_range = 2.0 / zoom; // The range of x values visible in the viewport
    let y_range = 2.0 / zoom; // The range of y values visible in the viewport

    let min_x = (-translate_x - 1.0) / zoom; // translation of (x=-1)
    let max_x = (-translate_x + 1.0) / zoom; // translation of (x=1)
    let min_y = (-translate_y - 1.0) / zoom; // translation of (y=-1)
    let max_y = (-translate_y + 1.0) / zoom; // translation of (y=1)

    let point_size_px: f32 = scatterplot_params.point_radius.unwrap_or(5.0);
    let viewport_w = context.params.width as f32;
    let viewport_h = context.params.height as f32;

    // Construct the uniform struct using Encase.
    let uniform_struct = Scatterplot3dUniforms {
        camera_view: Mat4::from_cols_array(&camera_view),
        point_size_px,
        viewport_size: Vec2::new(viewport_w, viewport_h),
        color: Vec4::from_array([1.0, 0.0, 0.0, 1.0]),
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
                    wgpu::BindGroupLayoutEntry {
                        // The Z coordinates buffer.
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
                        // The class labels coordinates buffer.
                        binding: 4,
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
    let bind_group = context
        .device
        .create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Scatter BG"),
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
                    resource: z_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: labels_buffer.as_entire_binding(),
                },
            ],
        });

    let shader = context
        .device
        .create_shader_module(wgpu::include_wgsl!("shaders/scatterplot_3d.wgsl"));

    let render_pipeline_layout =
        context
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[&bind_group_layout],
                immediate_size: 0,
            });

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
    let scatter_tex = context.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("scatterplot Offscreen Texture"),
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
    let scatter_view = scatter_tex.create_view(&wgpu::TextureViewDescriptor::default());

    {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &scatter_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    // Set a white background for the scatterplot.
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

    // TODO: modify the axis range to depend on viewport width/height rather than hard-coding.
    // But for 3D, this 2D axis doesnt make sense anyway.

    // Construct the X-axis:
    let mut x_scale = ScaleLinear::new();
    x_scale.set_domain((min_x as f64, max_x as f64));
    x_scale.set_range((0.0, 800.0));
    let x_axis = Axis::new(AxisOrientation::Bottom);
    let x_axis_elements = x_axis.generate_elements(&x_scale);

    let x_axis_group = TwoElement::Group(TwoGroup {
        elements: x_axis_elements,
        translate: Some((0.0, 750.0)),
        ..Default::default()
    });

    // Construct the Y-axis:
    let mut y_scale = ScaleLinear::new();
    y_scale.set_domain((min_y as f64, max_y as f64));
    y_scale.set_range((800.0, 0.0)); // Inverted range
    let y_axis = Axis::new(AxisOrientation::Left);
    let y_axis_elements = y_axis.generate_elements(&y_scale);

    let y_axis_group = TwoElement::Group(TwoGroup {
        elements: y_axis_elements,
        translate: Some((40.0, 0.0)),
        ..Default::default()
    });

    let axis_elements = vec![x_axis_group, y_axis_group];

    // Render the X and Y axes:
    crate::two::canvas::render_shapes(context, encoder, &axis_elements);

    crate::render::overlay_pass(context, encoder, &scatter_tex);

    RenderResult {
        bailed_early: false,
    }
}
