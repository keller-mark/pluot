use std::convert::TryInto;
use std::borrow::Cow;

use crate::{utils::RenderContext, zarr_get_js};
use wgpu::util::DeviceExt;

use fontdue::{Font, FontSettings};
use fontdue::layout::{Layout, LayoutSettings, CoordinateSystem, TextStyle};


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

    // Font rendering
    // 1) Rasterize "Hello world" with fontdue and pack into a single-row atlas.
    // Provide a font file in your repo at assets/Roboto-Regular.ttf (or change the path).
    let font_data: &[u8] = include_bytes!("fonts/Inter-Bold.ttf");
    let font = Font::from_bytes(font_data, FontSettings::default()).expect("load font");

    let px: f32 = 64.0; // font size in pixels
    let mut layout = Layout::new(CoordinateSystem::PositiveYDown);
    layout.reset(&LayoutSettings {
        max_width: None,
        max_height: None,
        ..LayoutSettings::default()
    });
    layout.append(&[&font], &TextStyle::new("Hello world", px, 0));

    let glyphs = layout.glyphs();
    if glyphs.is_empty() {
        return;
    }

    // Rasterize each glyph and measure atlas size (row pack).
    let mut atlas_width: usize = 0;
    let mut atlas_height: usize = 0;
    let mut rasters: Vec<(fontdue::Metrics, Vec<u8>)> = Vec::with_capacity(glyphs.len());
    for g in glyphs {
        let (metrics, bitmap) = font.rasterize_config(g.key);
        atlas_width += metrics.width.max(1);
        atlas_height = atlas_height.max(metrics.height.max(1));
        rasters.push((metrics, bitmap));
    }
    if atlas_width == 0 || atlas_height == 0 {
        return;
    }

    // Build the atlas RGBA (actually single channel) row.
    // We keep it R8Unorm and sample .r in the shader.
    let mut atlas: Vec<u8> = vec![0u8; atlas_width * atlas_height];
    let mut x_cursor: usize = 0;

    // Per-glyph instance data: [x, y, w, h, u0, v0, u1, v1]
    let mut instance_data: Vec<f32> = Vec::with_capacity(glyphs.len() * 8);

    for (g, (m, bmp)) in glyphs.iter().zip(rasters.into_iter()) {
        // Actual bitmap dimensions
        let gw = m.width.max(0) as usize;
        let gh = m.height.max(0) as usize;

        // Atlas pack dimensions (pad zero-size glyphs to avoid degenerate packing)
        let gw_pad = gw.max(1);
        let gh_pad = gh.max(1);

        // Copy bitmap into atlas only if it has pixels
        if gw > 0 && gh > 0 {
            for row in 0..gh {
                let src = &bmp[row * gw..row * gw + gw];
                let dst = &mut atlas[row * atlas_width + x_cursor..row * atlas_width + x_cursor + gw];
                dst.copy_from_slice(src);
            }

            // Compute screen-space rect in pixels (top-left)
            let x_px = g.x as f32;
            let y_px = (g.y + m.ymin as f32).round();
            let w_px = gw as f32;
            let h_px = gh as f32;

            // Place with a small margin from top-left
            let origin = glam::vec2(10.0, 10.0);
            let rect_x = origin.x + x_px;
            let rect_y = origin.y + y_px;

            // UV rectangle (normalized) uses padded pack width/height
            let u0 = (x_cursor as f32) / (atlas_width as f32);
            let v0 = 0.0;
            let u1 = ((x_cursor + gw_pad) as f32) / (atlas_width as f32);
            let v1 = (gh_pad as f32) / (atlas_height as f32);

            instance_data.extend_from_slice(&[rect_x, rect_y, w_px, h_px, u0, v0, u1, v1]);
        }

        // Advance pack cursor by padded width
        x_cursor += gw_pad;
    }

    // 2) Upload atlas as a single-channel R8Unorm texture.
    let atlas_tex = context.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Text Atlas"),
        size: wgpu::Extent3d {
            width: atlas_width as u32,
            height: atlas_height as u32,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    context.queue.write_texture(
        atlas_tex.as_image_copy(),
        &atlas,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(atlas_width as u32),
            rows_per_image: Some(atlas_height as u32),
        },
        wgpu::Extent3d {
            width: atlas_width as u32,
            height: atlas_height as u32,
            depth_or_array_layers: 1,
        },
    );
    let atlas_view = atlas_tex.create_view(&wgpu::TextureViewDescriptor::default());
    let atlas_sampler = context.device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Text Sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::FilterMode::Nearest,
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        ..Default::default()
    });

    // 3) Create instance buffer
    let instance_buffer = context.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Text Instances"),
        contents: bytemuck::cast_slice(&instance_data),
        usage: wgpu::BufferUsages::VERTEX,
    });

    // 4) Uniforms: viewport size and text color (premultiplied in shader)
    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Uniforms {
        viewport: [f32; 2],
        // Pad to 16-byte alignment before vec4; total struct size = 32 bytes.
        _pad: [f32; 2],
        color: [f32; 4],
    }
    let uniforms = Uniforms {
        viewport: [context.width as f32, context.height as f32],
        _pad: [0.0, 0.0],
        color: [0.0, 0.0, 0.0, 1.0], // black text
    };
    let uniform_buffer = context.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Text Uniforms"),
        contents: bytemuck::bytes_of(&uniforms),
        usage: wgpu::BufferUsages::UNIFORM,
    });

    // 5) Bind group layout: texture + sampler + uniforms
    let bgl = context.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Text BGL"),
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
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });
    let bind_group: wgpu::BindGroup = context.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Text BG"),
        layout: &bgl,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&atlas_view) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&atlas_sampler) },
            wgpu::BindGroupEntry { binding: 2, resource: uniform_buffer.as_entire_binding() },
        ],
    });

    // 6) WGSL shaders: instanced quad in screen space sampling R8 atlas
    let vs_src = r#"
        struct VsOut {
            @builtin(position) pos : vec4<f32>,
            @location(0) uv : vec2<f32>,
        };

        struct Ubo {
            viewport : vec2<f32>,
            color    : vec4<f32>,
        };
        @group(0) @binding(2) var<uniform> u : Ubo;

        // Per-instance attributes:
        // @location(0): rect_px = vec4(x, y, w, h)
        // @location(1): uv_rect = vec4(u0, v0, u1, v1)
        @vertex
        fn vs_main(
            @location(0) rect_px : vec4<f32>,
            @location(1) uv_rect : vec4<f32>,
            @builtin(vertex_index) vid : u32
        ) -> VsOut {
            // Corner in [0,1]^2 from vertex_index 0..3 (triangle strip)
            let cx = f32(vid & 1u);
            let cy = f32((vid >> 1u) & 1u);
            let corner = vec2<f32>(cx, cy);

            // Pixel position
            let px = rect_px.xy + corner * rect_px.zw;

            // NDC transform (PositiveYDown -> NDC)
            let ndc = vec2<f32>(
                (px.x / u.viewport.x) * 2.0 - 1.0,
                1.0 - (px.y / u.viewport.y) * 2.0
            );

            // UV from rect
            let uv = uv_rect.xy + corner * (uv_rect.zw - uv_rect.xy);

            var out : VsOut;
            out.pos = vec4<f32>(ndc, 0.0, 1.0);
            out.uv = uv;
            return out;
        }
    "#;

    let fs_src = r#"
        struct Ubo {
            viewport : vec2<f32>,
            color    : vec4<f32>,
        };
        @group(0) @binding(0) var glyph_tex : texture_2d<f32>;
        @group(0) @binding(1) var glyph_sampler : sampler;
        @group(0) @binding(2) var<uniform> u : Ubo;

        @fragment
        fn fs_main(@location(0) uv : vec2<f32>) -> @location(0) vec4<f32> {
            let a = textureSample(glyph_tex, glyph_sampler, uv).r;
            // Premultiply for blending
            let rgb = u.color.rgb * a;
            return vec4<f32>(rgb, a);
        }
    "#;

    let vs_module = context.device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Text VS"),
        source: wgpu::ShaderSource::Wgsl(vs_src.into()),
    });
    let fs_module = context.device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Text FS"),
        source: wgpu::ShaderSource::Wgsl(fs_src.into()),
    });

    // 7) Pipeline (instanced quad with per-instance vertex buffer)
    let pipeline_layout = context.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Text Pipeline Layout"),
        bind_group_layouts: &[&bgl],
        push_constant_ranges: &[],
    });

    // Vertex buffer layout: two vec4<f32> per instance
    let vertex_buffers = [wgpu::VertexBufferLayout {
        array_stride: (8 * std::mem::size_of::<f32>()) as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &[
            wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x4,
            },
            wgpu::VertexAttribute {
                offset: (4 * std::mem::size_of::<f32>()) as u64,
                shader_location: 1,
                format: wgpu::VertexFormat::Float32x4,
            },
        ],
    }];

    let pipeline = context.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Text Pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &vs_module,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &vertex_buffers,
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
            strip_index_format: None,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    });

    // Number of emitted instances (skip zero-sized glyphs)
    let instance_count: u32 = (instance_data.len() / 8) as u32;

    // 8) Render
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Text Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &context.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.set_vertex_buffer(0, instance_buffer.slice(..));
        // 4 vertices (triangle strip) per instance
        pass.draw(0..4, 0..instance_count);
    }
}






pub async fn render_scatterplot(context: &RenderContext<'_>, encoder: &mut wgpu::CommandEncoder) {
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