use crate::wgpu;
use crate::two::svg::{init_svg};
use svg::node::element::Group;
use crate::params::{RenderContext, RenderResult};

// Struct to store anything at the view level (i.e., not layer-specific)
pub struct ViewParams {
    pub view_id: String, // Just reuse the plot_id when there is a single view.
    pub width: u32,
    pub height: u32,

    // Device pixel ratio to support retina displays.
    // Default to 1.0 for standard displays.
    // Retina screens will have a value of 2.0 or higher.
    pub device_pixel_ratio: f32,

    pub camera_view: Option<[f32; 16]>,

    // Timeout in ms before bailing out of awaiting a data request.
    pub timeout: Option<u32>,

    // Allow disabling memoization/cacheing. Useful for testing/debugging.
    pub cache_enabled: bool,

    // Margins for plots that need them (e.g. scatterplot axes).
    pub margin_left: Option<f32>,
    pub margin_right: Option<f32>,
    pub margin_top: Option<f32>,
    pub margin_bottom: Option<f32>,
}

impl Default for ViewParams {
    fn default() -> Self {
        Self {
            view_id: "default_view".to_string(),
            width: 100,
            height: 100,
            device_pixel_ratio: 1.0,
            camera_view: None,
            timeout: None,
            cache_enabled: true,
            margin_left: None,
            margin_right: None,
            margin_top: None,
            margin_bottom: None,
        }
    }
}


#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait PreparedLayer {
    async fn prepare(&mut self);
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait DrawToSvg {
    async fn draw(&self, group: &Group) -> Group;
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait DrawToCanvas {
    async fn draw(&self, device: wgpu::Device, queue: wgpu::Queue, pass: &mut wgpu::RenderPass);
}

pub trait PreparedAndDrawToSvg: PreparedLayer + DrawToSvg {}
pub trait PreparedAndDrawToCanvas: PreparedLayer + DrawToCanvas {}


pub async fn render_svg(view_params: ViewParams, mut layers: Vec<Box<dyn PreparedAndDrawToSvg>>) -> Group {
    let (_, group) = init_svg(view_params.width as f64, view_params.height as f64);

    // TODO: use futures.join! here
    // TODO: use maybe_timeout here
    for layer in &mut layers {
        layer.prepare().await;
    }

    let mut group = group;
    for layer in &layers {
        // TODO: when/where to pass view_params to each layer?
        group = layer.draw(&group).await;
    }

    // TODO: also return RenderResult? How to aggregate results from multiple layers?

    group
}

pub async fn render_canvas(view_params: ViewParams, mut layers: Vec<Box<dyn PreparedAndDrawToCanvas>>, context: &mut RenderContext<'_>, encoder: &mut wgpu::CommandEncoder) {
    // TODO: use futures.join! here
    // TODO: use maybe_timeout here
    for layer in &mut layers {
        layer.prepare().await;
    }

    // Create the render pass
    // 1) Offscreen plot target
    let layered_tex = context.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Layered Offscreen Texture"),
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
    let layered_view = layered_tex.create_view(&wgpu::TextureViewDescriptor::default());

    {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Layered Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &layered_view,
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
            multiview_mask: None,
        });

        for layer in &layers {
            // TODO: when/where to pass view_params to each layer? during draw call? before draw call?
            // Should we instead assume the layer already has the necessary info from view_params?
            layer.draw(context.device.clone(), context.queue.clone(), &mut render_pass).await;
        }

        drop(render_pass);
    }

    // TODO: render directly to the context's out_tex, to avoid an extra render pass.
    crate::render::overlay_pass(context, encoder, &layered_tex);

    // TODO: return RenderResult? How to aggregate results from multiple layers?
}
