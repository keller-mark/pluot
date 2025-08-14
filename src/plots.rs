use crate::utils::RenderContext;

use wasm_bindgen::prelude::*;
use wgpu::{TextureDescriptor, TextureUsages, TextureFormat, Extent3d};
use futures_intrusive::channel::shared::oneshot_channel;

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};


pub async fn render_triangle(render_context: &RenderContext<'_>) {

    let device: &wgpu::Device = &render_context.device;
    let texture_desc_format: &wgpu::TextureFormat = &render_context.texture_desc_format;
    let view: &wgpu::TextureView = &render_context.view;
    let queue: &wgpu::Queue = &render_context.queue;
    let encoder: &wgpu::CommandEncoder = &mut render_context.encoder;
    let width: u32 = render_context.width;
    let height: u32 = render_context.height;

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

    let vs_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Vertex Shader"),
        source: wgpu::ShaderSource::Wgsl(vs_src.into()),
    });

    let fs_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Fragment Shader"),
        source: wgpu::ShaderSource::Wgsl(fs_src.into()),
    });

    let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Render Pipeline Layout"),
        bind_group_layouts: &[],
        push_constant_ranges: &[],
    });

    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
                format: *texture_desc_format,
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
                view: &view,
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

pub async fn render_scatterplot(render_context: &RenderContext<'_>) {

    let device: &wgpu::Device = &render_context.device;
    let texture_desc_format: &wgpu::TextureFormat = &render_context.texture_desc_format;
    let view: &wgpu::TextureView = &render_context.view;
    let queue: &wgpu::Queue = &render_context.queue;
    let global_map: &HashMap<String, Vec<i32>> = &render_context.global_map;
    let encoder: &wgpu::CommandEncoder = &mut render_context.encoder;
    let width: u32 = render_context.width;
    let height: u32 = render_context.height;

    // Get x and y data from the global map
    let (xs, ys) = {
        let xs = global_map.get("x").expect("No 'x' data registered").into_iter()
            .map(|&v| v as f32).collect::<Vec<f32>>();
        let ys = global_map.get("y").expect("No 'y' data registered").into_iter()
            .map(|&v| v as f32).collect::<Vec<f32>>();
        (xs, ys)
    };
    let n = xs.len();
    assert_eq!(n, ys.len(), "x and y data must have the same length");

    // Pack positions into a contiguous vec2<f32> array for a storage buffer
    let mut positions_bytes: Vec<u8> = Vec::with_capacity(n * 2 * 4);
    let (mut x_min, mut x_max) = (f32::INFINITY, f32::NEG_INFINITY);
    let (mut y_min, mut y_max) = (f32::INFINITY, f32::NEG_INFINITY);
    for i in 0..n {
        let x = xs[i];
        let y = ys[i];
        x_min = x_min.min(x); x_max = x_max.max(x);
        y_min = y_min.min(y); y_max = y_max.max(y);
        positions_bytes.extend_from_slice(&x.to_ne_bytes());
        positions_bytes.extend_from_slice(&y.to_ne_bytes());
    }
    let positions_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Positions Storage Buffer"),
        size: positions_bytes.len() as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&positions_buffer, 0, &positions_bytes);

    // Create uniforms matching the WGSL layout
    // struct Uniforms {
    //   x_min, x_max, y_min, y_max : f32,
    //   point_size_px: f32, _pad0: f32,
    //   viewport_size: vec2<f32>,
    //   color: vec4<f32>
    // }
    let point_size_px: f32 = 4.0;
    let _pad0: f32 = 0.0;
    let viewport_w = width as f32;
    let viewport_h = height as f32;
    let color = [1.0_f32, 1.0, 1.0, 1.0];

    let mut uniform_bytes: Vec<u8> = Vec::with_capacity(12 * 4);
    for f in [x_min, x_max, y_min, y_max, point_size_px, _pad0, viewport_w, viewport_h].iter() {
        uniform_bytes.extend_from_slice(&f.to_ne_bytes());
    }
    for c in color { uniform_bytes.extend_from_slice(&c.to_ne_bytes()); }

    let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Uniform Buffer"),
        size: uniform_bytes.len() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&uniform_buffer, 0, &uniform_bytes);

    // Create bind group layout and bind group for positions + uniforms
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Scatter BGL"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
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
                binding: 1,
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
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Scatter BG"),
        layout: &bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: positions_buffer.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: uniform_buffer.as_entire_binding() },
        ],
    });

    let vs_src = r#"
        struct Uniforms {
            x_min: f32,
            x_max: f32,
            y_min: f32,
            y_max: f32,
            point_size_px: f32,   // diameter in pixels
            _pad0: f32,
            viewport_size: vec2<f32>, // (width, height) in pixels
            color: vec4<f32>,     // rgba color for points
        };

        struct VSOut {
            @builtin(position) position: vec4<f32>,
            @location(0) color: vec4<f32>,
        };

        @group(0) @binding(0)
        var<storage, read> positions: array<vec2<f32>>;

        @group(0) @binding(1)
        var<uniform> u: Uniforms;

        // 4 corners of a unit quad for triangle strip: (-1,-1), (1,-1), (-1,1), (1,1)
        const QUAD: array<vec2<f32>, 4> = array<vec2<f32>, 4>(
            vec2<f32>(-1.0, -1.0),
            vec2<f32>( 1.0, -1.0),
            vec2<f32>(-1.0,  1.0),
            vec2<f32>( 1.0,  1.0)
        );

        // Map a data value v from [min,max] to NDC [-1,1]
        fn to_ndc(v: f32, minv: f32, maxv: f32) -> f32 {
            let t = (v - minv) / max(1e-12, (maxv - minv));
            return t * 2.0 - 1.0;
        }

        @vertex
        fn vs_main(
            @builtin(instance_index) instance: u32,
            @builtin(vertex_index) vid: u32
        ) -> VSOut {
            // Center of this point in data space
            let p = positions[instance];
            // Center in clip/NDC space (y increases up)
            let center_ndc = vec2<f32>(
                to_ndc(p.x, u.x_min, u.x_max),
                to_ndc(p.y, u.y_min, u.y_max)
            );

            // Convert desired pixel radius to NDC
            let radius_px = 0.5 * u.point_size_px;
            // pixels -> NDC: ndc_per_px = 2 / viewport
            let ndc_per_px = 2.0 / u.viewport_size;
            let radius_ndc = vec2<f32>(radius_px * ndc_per_px.x, radius_px * ndc_per_px.y);

            // Pick corner of quad and place around center
            let corner = QUAD[vid & 3u]; // vid % 4
            let offset_ndc = vec2<f32>(corner.x * radius_ndc.x, corner.y * radius_ndc.y);

            var out: VSOut;
            out.position = vec4<f32>(center_ndc + offset_ndc, 0.0, 1.0);
            out.color = u.color;
            return out;
        }

    "#;
    
    let fs_src = r#"
        struct FSOut {
            @location(0) color: vec4<f32>,
        };

        @fragment
        fn fs_main(@location(0) color_in: vec4<f32>) -> FSOut {
            var out: FSOut;
            out.color = color_in;
            return out;
        }
    "#;

    let vs_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Vertex Shader"),
        source: wgpu::ShaderSource::Wgsl(vs_src.into()),
    });

    let fs_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Fragment Shader"),
        source: wgpu::ShaderSource::Wgsl(fs_src.into()),
    });

    let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Render Pipeline Layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    // TODO: Extract the shared render pipeline and render pass logic. There is a lot of duplication here.
    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
                format: *texture_desc_format,
                blend: Some(wgpu::BlendState::REPLACE),
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
                view: &view,
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
        render_pass.set_bind_group(0, &bind_group, &[]);
        render_pass.draw(0..4, 0..(n as u32));

        // End the renderpass.
        drop(render_pass);
    }
}