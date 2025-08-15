use std::convert::TryInto;
use std::borrow::Cow;

use crate::with_vello_renderer;
use crate::{utils::RenderContext, zarr_get_js};

use skrifa::MetadataProvider;
use vello::wgpu;
use vello::{
    peniko::{Blob, Brush, Color, Fill, Font},
    AaConfig, AaSupport, Renderer, RendererOptions, RenderParams, Scene,
};

const FONT_BYTES: &[u8] = include_bytes!("fonts/Inter-Bold.ttf").as_slice();

pub fn overlay_pass(context: &mut RenderContext<'_>, encoder: &mut wgpu::CommandEncoder, background_tex: &wgpu::Texture, background_view: &wgpu::TextureView) {
    // 3) Composition pass: sample tri_tex then text_tex and draw to swapchain
    let overlay_vs = r#"
        struct VsOut { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> };
        @vertex
        fn vs_main(@builtin(vertex_index) i: u32) -> VsOut {
            var pos = array<vec2<f32>, 3>(
                vec2<f32>(-1.0, -3.0),
                vec2<f32>(-1.0,  1.0),
                vec2<f32>( 3.0,  1.0)
            );
            let p = pos[i];
            var o: VsOut;
            o.pos = vec4<f32>(p, 0.0, 1.0);
            let uv = 0.5 * (p + vec2<f32>(1.0, 1.0));
            // Flip Y so uv.y=0 is top, uv.y=1 is bottom.
            o.uv = vec2<f32>(uv.x, 1.0 - uv.y);
            return o;
        }
    "#;
    let overlay_fs = r#"
        @group(0) @binding(0) var tex0: texture_2d<f32>;
        @group(0) @binding(1) var samp0: sampler;
        struct FsIn { @location(0) uv: vec2<f32> };
        @fragment
        fn fs_main(in: FsIn) -> @location(0) vec4<f32> {
            return textureSample(tex0, samp0, in.uv);
            // UV debug: red=u, green=v
            // return vec4<f32>(in.uv, 0.0, 1.0);
        }
    "#;

    let overlay_vs_module = context.device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Overlay VS"),
        source: wgpu::ShaderSource::Wgsl(overlay_vs.into()),
    });
    let overlay_fs_module = context.device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Overlay FS"),
        source: wgpu::ShaderSource::Wgsl(overlay_fs.into()),
    });

    let overlay_bgl = context.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Overlay BGL"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let overlay_pl = context.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Overlay PL"),
        bind_group_layouts: &[&overlay_bgl],
        push_constant_ranges: &[],
    });
    let overlay_pipeline = context.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Overlay Pipeline"),
        layout: Some(&overlay_pl),
        vertex: wgpu::VertexState {
            module: &overlay_vs_module,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &overlay_fs_module,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: context.texture_desc.format,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    });

    let overlay_sampler = context.device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Overlay Sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::FilterMode::Nearest,
        ..Default::default()
    });

    let bg_background = context.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("BG background (pre-vello)"),
        layout: &overlay_bgl,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&background_view) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&overlay_sampler) },
        ],
    });
    let bg_foreground = context.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("BG foreground (vello scene)"),
        layout: &overlay_bgl,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&context.vello_view) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&overlay_sampler) },
        ],
    });

    {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Composite Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &context.view,
                resolve_target: None,
                ops: wgpu::Operations {
                    // Pick your final background color:
                    load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        render_pass.set_pipeline(&overlay_pipeline);

        // Draw triangles texture first
        render_pass.set_bind_group(0, &bg_background, &[]);
        render_pass.draw(0..3, 0..1);

        // Then draw text texture on top
        render_pass.set_bind_group(0, &bg_foreground, &[]);
        render_pass.draw(0..3, 0..1);
    }
}

pub async fn render_triangle(context: &mut RenderContext<'_>, encoder: &mut wgpu::CommandEncoder) {
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
    // 1) Offscreen triangle target
    let tri_tex = context.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Triangle Offscreen Texture"),
        size: wgpu::Extent3d { width: context.width, height: context.height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: context.texture_desc.format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let tri_view = tri_tex.create_view(&wgpu::TextureViewDescriptor::default());

    {
        // Render triangle into tri_tex with transparent background.
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Triangle Offscreen Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &tri_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        render_pass.set_pipeline(&render_pipeline);
        render_pass.draw(0..3, 0..1);
    }

    // 2) Vello scene with text.
    // Load a font from bytes (you can replace this with any TTF/OTF you own).
    let font_bytes: Cow<'static, [u8]> = Cow::from(FONT_BYTES);
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
    let pen_y = line_height;
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
    context.vello_scene
        .draw_glyphs(&peniko_font)
        .font_size(px_size)
        .hint(true)
        .brush(&Brush::Solid(Color::from_rgb8(240, 0, 245)))
        .draw(Fill::NonZero, glyphs.into_iter());

    // === 4) Render with Vello into our texture ===
    let params = RenderParams {
        base_color: Color::from_rgba8(0, 0, 0, 0), // transparent
        width: context.width,
        height: context.height,
        antialiasing_method: AaConfig::Msaa16,
    };
    with_vello_renderer(context.device, |vello_renderer| {
        vello_renderer
            .render_to_texture(context.device, context.queue, &context.vello_scene, &context.vello_view, &params)
            .expect("vello render_to_texture");
    });

    overlay_pass(context, encoder, &tri_tex, &tri_view);
}






pub async fn render_scatterplot(context: &mut RenderContext<'_>, encoder: &mut wgpu::CommandEncoder) {
    // Get x and y data from the global map
    let xs = zarr_get_js(&context.store_name, "x").await.to_vec();
    let ys = zarr_get_js(&context.store_name, "y").await.to_vec();
   
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
    let positions_buffer = context.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Positions Storage Buffer"),
        size: positions_bytes.len() as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
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
    let bind_group = context.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Scatter BG"),
        layout: &bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: positions_buffer.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: uniform_buffer.as_entire_binding() },
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

    // 1) Offscreen scatterplot target
    let scatter_tex = context.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("scatterplot Offscreen Texture"),
        size: wgpu::Extent3d { width: context.width, height: context.height, depth_or_array_layers: 1 },
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
                // depth_slice: None,
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

    }

    // 2) Vello scene with text.
    // Load a font from bytes (you can replace this with any TTF/OTF you own).
    let font_bytes: Cow<'static, [u8]> = Cow::from(FONT_BYTES);
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
    let pen_y = line_height;
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
    context.vello_scene
        .draw_glyphs(&peniko_font)
        .font_size(px_size)
        .hint(true)
        .brush(&Brush::Solid(Color::from_rgb8(240, 0, 245)))
        .draw(Fill::NonZero, glyphs.into_iter());

    // === 4) Render with Vello into our texture ===
    let params = RenderParams {
        base_color: Color::from_rgba8(0, 0, 0, 0), // transparent
        width: context.width,
        height: context.height,
        antialiasing_method: AaConfig::Msaa16,
    };
    with_vello_renderer(context.device, |vello_renderer| {
        vello_renderer
            .render_to_texture(context.device, context.queue, &context.vello_scene, &context.vello_view, &params)
            .expect("vello render_to_texture");
    });

    overlay_pass(context, encoder, &scatter_tex, &scatter_view);
}