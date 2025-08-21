use std::convert::TryInto;
use std::borrow::Cow;

use crate::{utils::RenderContext, zarr::AsyncZarritaStore, log};

pub async fn render_triangle(context: &RenderContext<'_>, encoder: &mut wgpu::CommandEncoder) {
    let vs_src = r#"
        @vertex
        fn vs_main(@builtin(vertex_index) in_vertex_index: u32) -> @builtin(position) vec4<f32> {
            let x = f32(i32(in_vertex_index) - 1);
            let y = f32(i32(in_vertex_index & 1u) * 2 - 1);
            return vec4<f32>(x, y, 0.0, 1.0);
        }
    "#;

    let fs_src = r#"
        @fragment
        fn fs_main() -> @location(0) vec4<f32> {
            return vec4<f32>(1.0, 0.0, 0.0, 1.0);
        }
    "#;

    let vs_module = context.device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Vertex Shader"),
        source: wgpu::ShaderSource::Wgsl(vs_src.into()),
    });

    let fs_module = context.device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Fragment Shader"),
        source: wgpu::ShaderSource::Wgsl(fs_src.into()),
    });

    let render_pipeline_layout = context.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Render Pipeline Layout"),
        bind_group_layouts: &[],
        push_constant_ranges: &[],
    });

    let render_pipeline = context.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Render Pipeline"),
        layout: Some(&render_pipeline_layout),
        vertex: wgpu::VertexState {
            module: &vs_module,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &fs_module,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: context.texture_desc.format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    });
    // End render-specific things.

    {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &context.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::GREEN),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        render_pass.set_pipeline(&render_pipeline);
        render_pass.draw(0..3, 0..1);

        // End the renderpass.
        drop(render_pass);
    }
}

pub async fn render_scatterplot(context: &RenderContext<'_>, encoder: &mut wgpu::CommandEncoder) {
    // Get x and y data from the Zarr store.
    let store = context.store;
    let x_array_path = "/n_1000000/x_coords";
    let y_array_path = "/n_1000000/y_coords";
    let x_array = zarrs::array::Array::async_open(store.clone(), x_array_path).await.unwrap();
    let y_array = zarrs::array::Array::async_open(store.clone(), y_array_path).await.unwrap();

    // Print the Zarr.json metadata to the JS console.
    // log(&x_array.metadata().to_string_pretty());

    // Read the whole array
    let x_vec = x_array
        .async_retrieve_array_subset_ndarray::<f64>(&x_array.subset_all())
        .await
        .unwrap();
    let y_vec = y_array
        .async_retrieve_array_subset_ndarray::<f64>(&y_array.subset_all())
        .await
        .unwrap();

    // More efficient version that eliminates intermediate vectors and redundant operations
    let n = x_vec.len();
    assert_eq!(n, y_vec.len(), "x and y data must have the same length");

    // Convert to f32 and cast to bytes directly - no for loop needed
    let x_f32: Vec<f32> = x_vec.iter().map(|&x| x as f32).collect();
    let y_f32: Vec<f32> = y_vec.iter().map(|&y| y as f32).collect();

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

    // Create uniforms matching the WGSL layout
    // struct Uniforms {
    //   x_min, x_max, y_min, y_max : f32,
    //   point_size_px: f32, _pad0: f32,
    //   viewport_size: vec2<f32>,
    //   color: vec4<f32>
    // }
    let point_size_px: f32 = 4.0;
    let _pad0: f32 = 0.0;
    let viewport_w = context.params.width as f32;
    let viewport_h = context.params.height as f32;
    let color = [1.0_f32, 0.0, 0.0, 1.0];

    let mut uniform_bytes: Vec<u8> = Vec::with_capacity(12 * 4);

    let zoom = context.params.zoom.unwrap_or(0.0);
    let target_x = context.params.target_x.unwrap_or(0.0);
    let target_y = context.params.target_y.unwrap_or(0.0);
    // Adjust the axis limits based on the zoom level.
    let scale_factor = 2.0_f32.powf(-zoom);

    // TODO: account for aspect ratio.
    let x_min = target_x - (scale_factor / 2.0);
    let x_max = target_x + (scale_factor / 2.0);
    let y_min = target_y - (scale_factor / 2.0);
    let y_max = target_y + (scale_factor / 2.0);

    // Log the computed values for debugging.
    log(&format!("Zoom: {zoom}, x_min: {x_min}, x_max: {x_max}, y_min: {y_min}, y_max: {y_max}"));

    for f in [x_min, x_max, y_min, y_max, point_size_px, _pad0, viewport_w, viewport_h].iter() {
        uniform_bytes.extend_from_slice(&f.to_ne_bytes());
    }
    for c in color { uniform_bytes.extend_from_slice(&c.to_ne_bytes()); }

    let uniform_buffer = context.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Uniform Buffer"),
        size: uniform_bytes.len() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    context.queue.write_buffer(&uniform_buffer, 0, &uniform_bytes);

    // Create bind group layout and bind group for positions + uniforms
    let bind_group_layout = context.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Scatter BGL"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                // The X coordinates buffer.
                binding: 0,
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
            // The uniforms buffer.
            binding: 2,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        },
        ],
    });
    let bind_group = context.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Scatter BG"),
        layout: &bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: x_buffer.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: y_buffer.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 2, resource: uniform_buffer.as_entire_binding() },
        ],
    });

    let vs_module = context.device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Vertex Shader"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("shaders/scatterplot.vs.wgsl"))),
    });

    let fs_module = context.device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Fragment Shader"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("shaders/scatterplot.fs.wgsl"))),
    });

    let render_pipeline_layout = context.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Render Pipeline Layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    // TODO: Extract the shared render pipeline and render pass logic. There is a lot of duplication here.
    let render_pipeline = context.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Render Pipeline"),
        layout: Some(&render_pipeline_layout),
        vertex: wgpu::VertexState {
            module: &vs_module,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &fs_module,
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
        multiview: None,
        cache: None,
    });

    {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &context.view,
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
        });

        render_pass.set_pipeline(&render_pipeline);
        render_pass.set_bind_group(0, &bind_group, &[]);
        render_pass.draw(0..4, 0..(n as u32));

        // End the renderpass.
        drop(render_pass);
    }
}