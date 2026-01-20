use crate::wgpu;
use crate::wgpu::{Extent3d, TextureDescriptor, TextureFormat, TextureUsages};
use crate::two::svg::init_svg;
use crate::layers::core::{render_svg, render_canvas, ViewParams};
/*use vello::{
    peniko::{Blob, Brush, Color, Fill, Font},
    AaConfig, AaSupport, Renderer, RendererOptions, Scene,
};*/
use crate::log;

use futures::FutureExt;
use futures_intrusive::channel::shared::oneshot_channel;

use crate::params::{GraphicsFormat, RenderContext};
pub use crate::params::{PlotParams, RenderParams, RenderResult};
use crate::plots;
use crate::cache::{get_or_init_gpu_context, get_or_init_store};


pub fn overlay_pass(
    context: &mut RenderContext<'_>,
    encoder: &mut wgpu::CommandEncoder,
    background_tex: &wgpu::Texture,
) {
    let vello_view = context
        .vello_tex
        .create_view(&wgpu::TextureViewDescriptor::default());
    let background_view = background_tex.create_view(&wgpu::TextureViewDescriptor::default());

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

    let overlay_vs_module = context
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Overlay VS"),
            source: wgpu::ShaderSource::Wgsl(overlay_vs.into()),
        });
    let overlay_fs_module = context
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Overlay FS"),
            source: wgpu::ShaderSource::Wgsl(overlay_fs.into()),
        });

    let overlay_bgl = context
        .device
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
    let overlay_pl = context
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Overlay PL"),
            bind_group_layouts: &[&overlay_bgl],
            immediate_size: 0,
        });
    let overlay_pipeline = context
        .device
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
            cache: None,
            multiview_mask: None,
        });

    let overlay_sampler = context.device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Overlay Sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        ..Default::default()
    });

    let bg_background = context
        .device
        .create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("BG background (pre-vello)"),
            layout: &overlay_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&background_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&overlay_sampler),
                },
            ],
        });
    let bg_foreground = context
        .device
        .create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("BG foreground (vello scene)"),
            layout: &overlay_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&vello_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&overlay_sampler),
                },
            ],
        });

    let out_view = context
        .out_tex
        .create_view(&wgpu::TextureViewDescriptor::default());

    {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Composite Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &out_view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    // Pick your final background color:
                    load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
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
