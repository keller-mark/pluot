// Post-render un-premultiply screen-quad pass
// We can alternatively do this on the CPU via a for-loop over the viewport pixels,
// but this becomes slow for large screen / plot sizes.

use crate::wgpu;
use crate::render_types::GpuContext;

/// Encodes a full-screen pass that converts the premultiplied-alpha output of
/// the layered render pass back into straight (un-premultiplied) alpha.
///
/// Blending the layers over a transparent clear color stores premultiplied
/// bytes (R*A, G*A, B*A, A). Consumers such as `putImageData` on the JS side
/// expect straight alpha, so without this step the browser compositor applies
/// alpha a second time. This runs the divide on the GPU instead of looping over
/// every pixel on the CPU after readback.
///
/// `src` must be created with [`wgpu::TextureUsages::TEXTURE_BINDING`] and `dst`
/// with [`wgpu::TextureUsages::RENDER_ATTACHMENT`]; the two must be distinct
/// (a texture can't be sampled and rendered to in the same pass). `dst` holds
/// the straight-alpha result after the encoded commands execute.
pub fn unpremultiply(
    gpu_context: &GpuContext<'_>,
    encoder: &mut wgpu::CommandEncoder,
    src: &wgpu::Texture,
    dst: &wgpu::Texture,
) {
    let shader = gpu_context.device
        .create_shader_module(wgpu::include_wgsl!("layers/shaders/unpremultiply.wgsl"));

    let bind_group_layout = gpu_context.device
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Un-premultiply Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    // textureLoad does no filtering, so non-filterable is fine.
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });

    let src_view = src.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = gpu_context.device
        .create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Un-premultiply Bind Group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&src_view),
            }],
        });

    let pipeline_layout = gpu_context.device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Un-premultiply Pipeline Layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

    let pipeline = gpu_context.device
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Un-premultiply Pipeline"),
            layout: Some(&pipeline_layout),
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
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    // No blending: write the un-premultiplied value verbatim.
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            cache: None,
            multiview_mask: None,
        });

    let dst_view = dst.create_view(&wgpu::TextureViewDescriptor::default());
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("Un-premultiply Pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &dst_view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_pipeline(&pipeline);
    pass.set_bind_group(0, &bind_group, &[]);
    pass.draw(0..3, 0..1);
}
