use std::cell::RefCell;
use std::collections::HashMap;

use crate::wgpu;
use crate::wgpu::util::DeviceExt;

use fontdue::layout::{CoordinateSystem, Layout, LayoutSettings, TextStyle};
use fontdue::{Font, FontSettings};

use crate::params::RenderContext;
use crate::two::shapes::{TwoText, TwoTextAlign, TwoTextBaseline};

const FONT_BYTES: &[u8] = include_bytes!("fonts/Inter-Bold.ttf").as_slice();

// Cached font atlas data
#[derive(Clone)]
struct FontAtlasCache {
    font: Font,
    atlas_texture: Option<wgpu::Texture>,
    glyph_cache: HashMap<(char, u32), (fontdue::Metrics, Vec<u8>)>, // char + font_size -> metrics + bitmap
}

thread_local! {
    static FONT_ATLAS: RefCell<Option<FontAtlasCache>> = RefCell::new(None);
}

fn get_or_init_font_atlas() -> FontAtlasCache {
    FONT_ATLAS.with(|atlas| {
        let mut atlas_ref = atlas.borrow_mut();
        if let Some(ref cached_atlas) = *atlas_ref {
            cached_atlas.clone()
        } else {
            let font = Font::from_bytes(FONT_BYTES, FontSettings::default()).expect("load font");
            let cache = FontAtlasCache {
                font,
                atlas_texture: None,
                glyph_cache: HashMap::new(),
            };
            *atlas_ref = Some(cache.clone());
            cache
        }
    })
}

// Text measurement functions
fn measure_text_width(font: &Font, text: &str, font_size: f32) -> f32 {
    let mut layout = Layout::new(CoordinateSystem::PositiveYDown);
    layout.reset(&LayoutSettings {
        max_width: None,
        max_height: None,
        ..LayoutSettings::default()
    });
    layout.append(&[font], &TextStyle::new(text, font_size, 0));

    let glyphs = layout.glyphs();
    if glyphs.is_empty() {
        return 0.0;
    }

    // Calculate the total width by finding the rightmost point
    let mut max_x = 0.0f32;
    for glyph in glyphs {
        let right_edge = glyph.x + glyph.width as f32;
        max_x = max_x.max(right_edge);
    }
    max_x
}

fn calculate_text_position(
    text_element: &TwoText,
    text_width: f32,
    translate: Option<(f64, f64)>,
) -> (f32, f32) {
    let x = match text_element.align {
        TwoTextAlign::Start => text_element.x as f32,
        TwoTextAlign::Middle => text_element.x as f32 - text_width / 2.0,
        TwoTextAlign::End => text_element.x as f32 - text_width,
    };

    // For baseline, we'll use the provided y coordinate as-is for now
    // More sophisticated baseline handling could be added later
    let y = match text_element.baseline {
        TwoTextBaseline::Top => text_element.y as f32,
        TwoTextBaseline::Middle => text_element.y as f32,
        TwoTextBaseline::Alphabetic => text_element.y as f32,
        TwoTextBaseline::Bottom => text_element.y as f32,
    };

    // Account for translation if provided.
    if let Some((tx, ty)) = translate {
        (x + tx as f32, y + ty as f32)
    } else {
        (x, y)
    }
}

// TODO: operate the opposite way. ensure that all color fields of TwoElements are [r, g, b[, a]] tuples,
// and only translate them to strings as-needed (e.g., for SVG rendering, using "rgb()" or "rgba()").
fn parse_color(color_str: &str) -> [f32; 4] {
    if color_str.starts_with('#') && color_str.len() == 7 {
        let r = u8::from_str_radix(&color_str[1..3], 16).unwrap_or(0) as f32 / 255.0;
        let g = u8::from_str_radix(&color_str[3..5], 16).unwrap_or(0) as f32 / 255.0;
        let b = u8::from_str_radix(&color_str[5..7], 16).unwrap_or(0) as f32 / 255.0;
        [r, g, b, 1.0]
    } else {
        // Default to black for unparseable colors
        [0.0, 0.0, 0.0, 1.0]
    }
}

pub fn render_text(
    context: &mut RenderContext<'_>,
    encoder: &mut wgpu::CommandEncoder,
    text_elements: &[TwoText],
    translate: Option<(f64, f64)>,
) {
    // Configurable padding around each glyph to prevent texture bleeding
    const PADDING: usize = 1;

    if text_elements.is_empty() {
        return;
    }

    // Get cached font
    let font_atlas = get_or_init_font_atlas();

    // Build a comprehensive layout with all text elements to create the atlas
    let mut layout = Layout::new(CoordinateSystem::PositiveYDown);
    layout.reset(&LayoutSettings {
        max_width: None,
        max_height: None,
        ..LayoutSettings::default()
    });

    // Append all text from all elements to ensure we have all glyphs in the atlas
    for text_element in text_elements {
        layout.append(
            &[&font_atlas.font],
            &TextStyle::new(&text_element.text, text_element.fontsize as f32, 0),
        );
    }

    let glyphs = layout.glyphs();
    if glyphs.is_empty() {
        return;
    }

    // Rasterize each glyph and measure atlas size (row pack)
    let mut atlas_width: usize = 0;
    let mut atlas_height: usize = 0;
    let mut rasters: Vec<(fontdue::Metrics, Vec<u8>)> = Vec::with_capacity(glyphs.len());

    for g in glyphs {
        let (metrics, bitmap) = font_atlas.font.rasterize_config(g.key);
        // Add padding around each glyph: PADDING + glyph_width + PADDING
        atlas_width += 2 * PADDING + metrics.width.max(1);
        atlas_height = atlas_height.max(2 * PADDING + metrics.height.max(1));
        rasters.push((metrics, bitmap));
    }

    if atlas_width == 0 || atlas_height == 0 {
        return;
    }

    // Build the atlas RGBA (actually single channel) row - initialize with zeros for padding
    let mut atlas: Vec<u8> = vec![0u8; atlas_width * atlas_height];
    let mut x_cursor: usize = PADDING; // Start with padding offset

    // Now process each text element individually to generate instance data
    let mut all_instance_data: Vec<f32> = Vec::new();
    let mut total_instances = 0u32;

    for text_element in text_elements {
        // Measure text width for alignment
        let text_width = measure_text_width(
            &font_atlas.font,
            &text_element.text,
            text_element.fontsize as f32,
        );
        let (base_x, base_y) = calculate_text_position(text_element, text_width, translate);

        // Create a separate layout for this text element
        let mut element_layout = Layout::new(CoordinateSystem::PositiveYDown);
        element_layout.reset(&LayoutSettings {
            max_width: None,
            max_height: None,
            ..LayoutSettings::default()
        });
        element_layout.append(
            &[&font_atlas.font],
            &TextStyle::new(&text_element.text, text_element.fontsize as f32, 0),
        );

        let element_glyphs = element_layout.glyphs();

        // Track our position in the atlas for this text element
        let mut element_cursor = x_cursor;

        for (i, g) in element_glyphs.iter().enumerate() {
            let (m, bmp) = &rasters[total_instances as usize + i];

            // Actual bitmap dimensions
            let gw = m.width.max(0) as usize;
            let gh = m.height.max(0) as usize;

            // Copy bitmap into atlas with padding offset
            if gw > 0 && gh > 0 {
                for row in 0..gh {
                    let src = &bmp[row * gw..row * gw + gw];
                    // Offset destination by PADDING pixels vertically and horizontally
                    let dst_row = PADDING + row;
                    let dst_start = dst_row * atlas_width + element_cursor;
                    let dst_end = dst_start + gw;
                    let dst = &mut atlas[dst_start..dst_end];
                    dst.copy_from_slice(src);
                }
            }

            // Compute screen-space rect in pixels
            let x_px = base_x + g.x as f32;
            let y_px = base_y + g.y as f32;
            let w_px = gw as f32;
            let h_px = gh as f32;

            // UV rectangle (normalized) - exclude padding from sampled area
            let u0 = (element_cursor as f32) / (atlas_width as f32);
            let v0 = (PADDING as f32) / (atlas_height as f32);
            let u1 = ((element_cursor + gw) as f32) / (atlas_width as f32);
            let v1 = ((PADDING + gh) as f32) / (atlas_height as f32);

            if gw > 0 && gh > 0 {
                all_instance_data.extend_from_slice(&[x_px, y_px, w_px, h_px, u0, v0, u1, v1]);
            }

            // Advance cursor by glyph width + padding for next glyph
            element_cursor += gw + 2 * PADDING;
        }

        x_cursor = element_cursor;
        total_instances += element_glyphs.len() as u32;
    }

    // 2) Upload atlas as a single-channel R8Unorm texture
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
    let instance_buffer = context
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Text Instances"),
            contents: bytemuck::cast_slice(&all_instance_data),
            usage: wgpu::BufferUsages::VERTEX,
        });

    // 4) Uniforms: viewport size and text color (we'll use the first text element's color for now)
    // TODO: update this to allow for a color per text element.
    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Uniforms {
        viewport: [f32; 2],
        // Pad to 16-byte alignment before vec4; total struct size = 32 bytes.
        _pad: [f32; 2],
        color: [f32; 4],
    }

    // Use the first text element's color, or default to black
    let color = if !text_elements.is_empty() {
        parse_color(&text_elements[0].fill)
    } else {
        [0.0, 0.0, 0.0, 1.0]
    };

    let uniforms = Uniforms {
        viewport: [context.params.width as f32, context.params.height as f32],
        _pad: [0.0, 0.0],
        color,
    };

    let uniform_buffer = context
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Text Uniforms"),
            contents: bytemuck::bytes_of(&uniforms),
            usage: wgpu::BufferUsages::UNIFORM,
        });

    // 5) Bind group layout: texture + sampler + uniforms
    let bgl = context
        .device
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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

    let bind_group = context
        .device
        .create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Text BG"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&atlas_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: uniform_buffer.as_entire_binding(),
                },
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

    let vs_module = context
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Text VS"),
            source: wgpu::ShaderSource::Wgsl(vs_src.into()),
        });
    let fs_module = context
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Text FS"),
            source: wgpu::ShaderSource::Wgsl(fs_src.into()),
        });

    // 7) Pipeline (instanced quad with per-instance vertex buffer)
    let pipeline_layout = context
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
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

    let pipeline = context
        .device
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
    let instance_count: u32 = (all_instance_data.len() / 8) as u32;

    let vello_view = context
        .vello_tex
        .create_view(&wgpu::TextureViewDescriptor::default());

    // 8) Render
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Text Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &vello_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    // Use Load so we blend over existing Vello output instead of clearing it.
                    load: wgpu::LoadOp::Load,
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
