use std::convert::TryInto;
use std::borrow::Cow;

use crate::{utils::RenderContext, zarr_get_js};

use skrifa::MetadataProvider;
use vello::{
    peniko::{Blob, Brush, Color, Fill, Font},
    AaConfig, AaSupport, Renderer, RendererOptions, RenderParams, Scene,
};

pub async fn render_triangle(context: &RenderContext<'_>, encoder: &mut vello::wgpu::CommandEncoder) {
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

    let vs_module = context.device.create_shader_module(vello::wgpu::ShaderModuleDescriptor {
        label: Some("Vertex Shader"),
        source: vello::wgpu::ShaderSource::Wgsl(vs_src.into()),
    });

    let fs_module = context.device.create_shader_module(vello::wgpu::ShaderModuleDescriptor {
        label: Some("Fragment Shader"),
        source: vello::wgpu::ShaderSource::Wgsl(fs_src.into()),
    });

    let render_pipeline_layout = context.device.create_pipeline_layout(&vello::wgpu::PipelineLayoutDescriptor {
        label: Some("Render Pipeline Layout"),
        bind_group_layouts: &[],
        push_constant_ranges: &[],
    });

    let render_pipeline = context.device.create_render_pipeline(&vello::wgpu::RenderPipelineDescriptor {
        label: Some("Render Pipeline"),
        layout: Some(&render_pipeline_layout),
        vertex: vello::wgpu::VertexState {
            module: &vs_module,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(vello::wgpu::FragmentState {
            module: &fs_module,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(vello::wgpu::ColorTargetState {
                format: context.texture_desc.format,
                blend: Some(vello::wgpu::BlendState::REPLACE),
                write_mask: vello::wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: vello::wgpu::PrimitiveState {
            topology: vello::wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: vello::wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    });
    // End render-specific things.

    // Start text rendering things.
    // === 2) Vello renderer ===
    let mut vello_renderer = Renderer::new(
        context.device,
        RendererOptions {
            use_cpu: false,
            antialiasing_support: AaSupport::all(),
            num_init_threads: std::num::NonZeroUsize::new(1),
            pipeline_cache: None,
        },
    )
    .expect("create vello renderer");

    // === 3) Build a scene with text ===
    let mut scene = Scene::new();

    // Load a font from bytes (you can replace this with any TTF/OTF you own).
    // Put "DejaVuSans.ttf" next to this file.
    let font_bytes: Cow<'static, [u8]> = Cow::from(include_bytes!("fonts/Inter-Bold.ttf").as_slice());
    let blob = Blob::new(std::sync::Arc::new(font_bytes));
    let peniko_font = Font::new(blob, 0);

    // TODO: explore using Parley https://github.com/linebender/parley

    // Reference: https://github.com/linebender/vello/blob/main/examples/scenes/src/simple_text.rs
    // Build a simple “Hello, world” glyph run using Skrifa:
    let font_ref = skrifa::FontRef::new(peniko_font.data.as_ref()).expect("parse font");

    // choose a pixel size and compute scale factor from design units to px
    let px_size: f32 = 64.0;

    // map chars -> glyph ids, accumulate x advances
    let cmap = font_ref.charmap();
    let axes = font_ref.axes();
    let font_size = skrifa::instance::Size::new(px_size);
    let variations: &[(&str, f32)] = &[];
    let var_loc = axes.location(variations.iter().copied());
    let metrics = font_ref.metrics(font_size, &var_loc);
    let line_height = metrics.ascent - metrics.descent + metrics.leading;
    let glyph_metrics = font_ref.glyph_metrics(font_size, &var_loc);

    let text = "Hello, world!";
    let mut pen_x = 0_f32;
    let mut pen_y = line_height;
    let mut glyphs = Vec::with_capacity(text.len());

    for ch in text.chars() {
        if let Some(gid) = cmap.map(ch) {
            // advance in *pixels*
            let adv: f32 = glyph_metrics
                .advance_width(gid)
                .unwrap_or(0.0); // in px because we passed Size::new(px_size)
            glyphs.push(vello::Glyph {
                id: gid.to_u32(),
                x: pen_x,
                y: pen_y,
            });
            pen_x += adv;
        }
    }

    // Draw the glyph run: white fill
    scene
        .draw_glyphs(&peniko_font)
        .font_size(px_size)
        .hint(true)
        .brush(&Brush::Solid(Color::from_rgb8(240, 0, 245)))
        .draw(Fill::NonZero, glyphs.into_iter());

    // === 4) Render with Vello into our texture ===
    let params = RenderParams {
        base_color: Color::BLACK, // clear color (behind scene)
        width: context.width,
        height: context.height,
        antialiasing_method: AaConfig::Msaa16,
    };

    // End text rendering things.

    {
        let mut render_pass = encoder.begin_render_pass(&vello::wgpu::RenderPassDescriptor {
            label: Some("Render Pass"),
            color_attachments: &[Some(vello::wgpu::RenderPassColorAttachment {
                view: &context.view,
                // depth_slice: None,
                resolve_target: None,
                ops: vello::wgpu::Operations {
                    load: vello::wgpu::LoadOp::Clear(vello::wgpu::Color::GREEN),
                    store: vello::wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        render_pass.set_pipeline(&render_pipeline);
        render_pass.draw(0..3, 0..1);

        vello_renderer
            .render_to_texture(context.device, context.queue, &scene, context.view, &params)
            .expect("vello render_to_texture");

        // End the renderpass.
        drop(render_pass);
    }
}






pub async fn render_scatterplot(context: &RenderContext<'_>, encoder: &mut vello::wgpu::CommandEncoder) {
    // Get x and y data from the global map
    let xs = zarr_get_js(&context.store_name, "x").to_vec();
    let ys = zarr_get_js(&context.store_name, "y").to_vec();
   
    let n = xs.len().try_into().unwrap();
    assert_eq!(n, ys.len(), "x and y data must have the same length");

    // Pack positions into a contiguous vec2<f32> array for a storage buffer
    let mut positions_bytes: Vec<u8> = Vec::with_capacity(n * 2 * 4);
    let (mut x_min, mut x_max) = (f32::INFINITY, f32::NEG_INFINITY);
    let (mut y_min, mut y_max) = (f32::INFINITY, f32::NEG_INFINITY);
    for i in 0..n {
        let x = xs[i] as f32;
        let y = ys[i] as f32;
        x_min = x_min.min(x); x_max = x_max.max(x);
        y_min = y_min.min(y); y_max = y_max.max(y);
        positions_bytes.extend_from_slice(&x.to_ne_bytes());
        positions_bytes.extend_from_slice(&y.to_ne_bytes());
    }
    let positions_buffer = context.device.create_buffer(&vello::wgpu::BufferDescriptor {
        label: Some("Positions Storage Buffer"),
        size: positions_bytes.len() as u64,
        usage: vello::wgpu::BufferUsages::STORAGE | vello::wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    context.queue.write_buffer(&positions_buffer, 0, &positions_bytes);

    // Create uniforms matching the WGSL layout
    // struct Uniforms {
    //   x_min, x_max, y_min, y_max : f32,
    //   point_size_px: f32, _pad0: f32,
    //   viewport_size: vec2<f32>,
    //   color: vec4<f32>
    // }
    let point_size_px: f32 = 4.0;
    let _pad0: f32 = 0.0;
    let viewport_w = context.width as f32;
    let viewport_h = context.height as f32;
    let color = [1.0_f32, 0.0, 0.0, 1.0];

    let mut uniform_bytes: Vec<u8> = Vec::with_capacity(12 * 4);
    for f in [x_min, x_max, y_min, y_max, point_size_px, _pad0, viewport_w, viewport_h].iter() {
        uniform_bytes.extend_from_slice(&f.to_ne_bytes());
    }
    for c in color { uniform_bytes.extend_from_slice(&c.to_ne_bytes()); }

    let uniform_buffer = context.device.create_buffer(&vello::wgpu::BufferDescriptor {
        label: Some("Uniform Buffer"),
        size: uniform_bytes.len() as u64,
        usage: vello::wgpu::BufferUsages::UNIFORM | vello::wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    context.queue.write_buffer(&uniform_buffer, 0, &uniform_bytes);

    // Create bind group layout and bind group for positions + uniforms
    let bind_group_layout = context.device.create_bind_group_layout(&vello::wgpu::BindGroupLayoutDescriptor {
        label: Some("Scatter BGL"),
        entries: &[
            vello::wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: vello::wgpu::ShaderStages::VERTEX,
                ty: vello::wgpu::BindingType::Buffer {
                    ty: vello::wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            vello::wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: vello::wgpu::ShaderStages::VERTEX,
                ty: vello::wgpu::BindingType::Buffer {
                    ty: vello::wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });
    let bind_group = context.device.create_bind_group(&vello::wgpu::BindGroupDescriptor {
        label: Some("Scatter BG"),
        layout: &bind_group_layout,
        entries: &[
            vello::wgpu::BindGroupEntry { binding: 0, resource: positions_buffer.as_entire_binding() },
            vello::wgpu::BindGroupEntry { binding: 1, resource: uniform_buffer.as_entire_binding() },
        ],
    });

    let vs_module = context.device.create_shader_module(vello::wgpu::ShaderModuleDescriptor {
        label: Some("Vertex Shader"),
        source: vello::wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("shaders/scatterplot.vs.wgsl"))),
    });

    let fs_module = context.device.create_shader_module(vello::wgpu::ShaderModuleDescriptor {
        label: Some("Fragment Shader"),
        source: vello::wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("shaders/scatterplot.fs.wgsl"))),
    });

    let render_pipeline_layout = context.device.create_pipeline_layout(&vello::wgpu::PipelineLayoutDescriptor {
        label: Some("Render Pipeline Layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    // TODO: Extract the shared render pipeline and render pass logic. There is a lot of duplication here.
    let render_pipeline = context.device.create_render_pipeline(&vello::wgpu::RenderPipelineDescriptor {
        label: Some("Render Pipeline"),
        layout: Some(&render_pipeline_layout),
        vertex: vello::wgpu::VertexState {
            module: &vs_module,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(vello::wgpu::FragmentState {
            module: &fs_module,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(vello::wgpu::ColorTargetState {
                format: context.texture_desc.format,
                blend: Some(vello::wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: vello::wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: vello::wgpu::PrimitiveState {
            topology: vello::wgpu::PrimitiveTopology::TriangleStrip,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: vello::wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    });

    {
        let mut render_pass = encoder.begin_render_pass(&vello::wgpu::RenderPassDescriptor {
            label: Some("Render Pass"),
            color_attachments: &[Some(vello::wgpu::RenderPassColorAttachment {
                view: &context.view,
                // depth_slice: None,
                resolve_target: None,
                ops: vello::wgpu::Operations {
                    // Set a white background for the scatterplot.
                    // TODO: make this configurable.
                    load: vello::wgpu::LoadOp::Clear(vello::wgpu::Color::WHITE),
                    store: vello::wgpu::StoreOp::Store,
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