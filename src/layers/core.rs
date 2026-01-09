use crate::wgpu;
use crate::two::svg::{init_svg};
use svg::node::element::Group;
use crate::params::{RenderContext, RenderResult};

#[derive(Clone, Debug)]
pub enum AspectRatioMode {
    /*
     - 0: ignore / squeeze: For example,  a 200 x 100 canvas would show values from -1 to 1 in x and y. The -1 to 1 square would be stretched in the X direction since the canvas is wider than it is tall.

     - 1: fit (contain): For example, a 200 x 100 canvas would range from -1 to 1 in the Y direction, and from -1-extra to 1+extra in the X direction. The -1 to 1 square would keep its square aspect ratio and would be fully visible inside the rectangle (with no part of this square clipped). The pixels would be centered.

     - 2: fill (cover): For example, a 200 x 100 canvas would range from -1 to 1 in the X direction, and from -1+extra to 1-extra in the Y direction. The -1 to 1 square would keep its square aspect ratio but would be clipped in the Y direction (at the top and bottom) so that the entire canvas is filled/covered. The pixels would be centered.
     */
     Ignore,
     Contain,
     Cover,
}

#[derive(Clone, Debug)]
pub enum UnitsMode {
    // 0: pixels (e.g., for fixed pixel-unit sizes).
    Pixels,
    // 1: data units (e.g., for physical sizes).
    Data,
}

// Struct to store anything at the view level (i.e., not layer-specific)
#[derive(Clone, Debug)]
pub struct ViewParams {
    pub view_id: String, // Just reuse the plot_id when there is a single view.
    pub width: u32,
    pub height: u32,

    pub aspect_ratio_mode: AspectRatioMode,

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

    // Note: Views should have margins, but these should be translated to "bounds" for layers.
    // This is because we may want to render certain layers in the margins
    // (e.g., text/line layers for axes/titles/etc).
}

impl Default for ViewParams {
    fn default() -> Self {
        Self {
            view_id: "default_view".to_string(),
            width: 100,
            height: 100,
            aspect_ratio_mode: AspectRatioMode::Contain,
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
impl<T: PreparedLayer + DrawToSvg> PreparedAndDrawToSvg for T {}

pub trait PreparedAndDrawToCanvas: PreparedLayer + DrawToCanvas {}
impl<T: PreparedLayer + DrawToCanvas> PreparedAndDrawToCanvas for T {}



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

pub async fn render_canvas(view_params: ViewParams, mut layers: Vec<Box<dyn PreparedAndDrawToCanvas>>, context: &mut RenderContext<'_>, encoder: &mut wgpu::CommandEncoder) -> RenderResult {
    // TODO: use futures.join! here
    // TODO: use maybe_timeout here
    for layer in &mut layers {
        layer.prepare().await;
    }

    // Create the render pass
    let out_view = context
        .out_tex
        .create_view(&wgpu::TextureViewDescriptor::default());

    {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Layered Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                // Render directly to the context's out_tex, to avoid an extra render pass.
                view: &out_view,
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

    // TODO: return RenderResult? How to aggregate results from multiple layers?

    RenderResult {
        bailed_early: false,
    }
}
