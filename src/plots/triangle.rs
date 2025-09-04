use std::borrow::Cow;

use crate::wgpu;

// use vello::{
//     kurbo::{Affine, Circle, Ellipse, Line, RoundedRect, Stroke},
//     peniko::{Blob, Brush, Color, Fill, Font},
//     AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene,
// };

use crate::two::shapes::{TwoCircle, TwoElement, TwoLine, TwoPath, TwoRectangle, TwoText};
use crate::utils::{PlotParams, RenderContext};

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

    let vs_module = context
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Vertex Shader"),
            source: wgpu::ShaderSource::Wgsl(vs_src.into()),
        });

    let fs_module = context
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Fragment Shader"),
            source: wgpu::ShaderSource::Wgsl(fs_src.into()),
        });

    let render_pipeline_layout =
        context
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[],
                push_constant_ranges: &[],
            });

    let render_pipeline = context
        .device
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
    // End render-specific things.

    // 1) Offscreen triangle target
    let tri_tex = context.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Triangle Offscreen Texture"),
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
    let tri_view = tri_tex.create_view(&wgpu::TextureViewDescriptor::default());

    {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &tri_view, // Render directly into output view. TODO: change this if rendering to offscreen texture, then rendering shapes/text, then overlaying.
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

    /*
    let vello_view = context
        .vello_tex
        .create_view(&wgpu::TextureViewDescriptor::default());
    // === Render with Vger into our texture ===
    crate::two::text_vger::with_vger_renderer(context.device, context.queue, |vger| {
        vger.begin(512.0, 512.0, 1.0);
        let cyan = vger.color_paint(vger::color::Color::CYAN);
        vger.fill_circle([100.0, 100.0], 20.0, cyan);

        vger.translate([32.0, 256.0]);
        vger.text(
            "Hello, world!",
            24,
            vger::color::Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            },
            None,
        );

        let desc = wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &vello_view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        };

        vger.encode(&desc);
    });
    */

    crate::two::canvas::render_shapes(
        context,
        encoder,
        &vec![
            TwoElement::Rectangle(TwoRectangle {
                x: 10.0,
                y: 20.0,
                width: 30.0,
                height: 40.0,
                opacity: 0.5,
                fill: Some("#FF0000".to_string()),
                stroke: Some("#00FFFF".to_string()),
                linewidth: 2.0,
                rotation: Some(std::f64::consts::PI / 4.0),
            }),
            TwoElement::Circle(TwoCircle {
                x: 50.0,
                y: 60.0,
                radius: 15.0,
                opacity: 1.0,
                fill: Some("#00FF00".to_string()),
                stroke: None,
                linewidth: 1.0,
            }),
            TwoElement::Line(TwoLine {
                x1: 70.0,
                y1: 800.0 - 80.0,
                x2: 90.0,
                y2: 800.0 - 100.0,
                opacity: 1.0,
                stroke: Some("#000000".to_string()),
                linewidth: 3.0,
            }),
        ],
    );

    //crate::two::text_fontdue::render_text(context, encoder);

    //println!("Rendered triangle");
    /*
    let vello_view = context
        .vello_tex
        .create_view(&wgpu::TextureViewDescriptor::default());

    // 2) Vello scene with text.
    let mut scene = vello::Scene::new();

    crate::plots::text_vello::add_text_to_scene(&mut scene);

    //println!("Added text to scene");

    // === 4) Render with Vello into our texture ===
    let params = vello::RenderParams {
        base_color: Color::from_rgba8(0, 0, 0, 0), // transparent
        width: context.params.width,
        height: context.params.height,
        antialiasing_method: AaConfig::Msaa16,
    };

    crate::plots::text_vello::with_vello_renderer(context.device, |vello_renderer| {
        vello_renderer
            .render_to_texture(context.device, context.queue, &scene, &vello_view, &params)
            .expect("vello render_to_texture");
    });

    //println!("Rendered vello scene");
    */

    crate::render::overlay_pass(context, encoder, &tri_tex);
}
