use std::borrow::Cow;

use crate::d3::axis::{Axis, AxisOrientation};
use crate::d3::scale::{Scale, ScaleLinear};
use crate::wgpu;

// use vello::{
//     kurbo::{Affine, Circle, Ellipse, Line, RoundedRect, Stroke},
//     peniko::{Blob, Brush, Color, Fill, Font},
//     AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene,
// };

use crate::params::{PlotParams, RenderContext, RenderResult};
use crate::two::shapes::{
    TwoCircle, TwoElement, TwoGroup, TwoLine, TwoPath, TwoRectangle, TwoText,
};

pub async fn render_triangle(
    context: &mut RenderContext<'_>,
    encoder: &mut wgpu::CommandEncoder,
) -> RenderResult {
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

    // Render the X axis:
    let mut x_scale = ScaleLinear::new();
    x_scale.set_domain((0.0, 100.0));
    x_scale.set_range((20.0, 780.0));
    let x_axis = Axis::new(AxisOrientation::Bottom);
    let x_axis_elements = x_axis.generate_elements(&x_scale);

    let x_axis_group = vec![TwoElement::Group(TwoGroup {
        elements: x_axis_elements,
        translate: Some((0.0, 750.0)),
        ..Default::default()
    })];

    // Render the axis:
    crate::two::canvas::render_shapes(context, encoder, &x_axis_group);

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

    RenderResult {
        bailed_early: false,
    }
}
