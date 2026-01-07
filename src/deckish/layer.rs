use crate::wgpu;
use crate::two::svg::{init_svg};
use svg::node::element::Group;
use crate::params::{RenderContext, RenderResult};

// Struct to store anything at the view level (i.e., not layer-specific)
pub struct ViewParams {
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
pub trait DrawToSvg {
    async fn draw(&self, group: &Group) -> Group;
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait DrawToCanvas {
    async fn draw(&self, device: wgpu::Device, queue: wgpu::Queue, encoder: &wgpu::CommandEncoder);
}


pub async fn render_svg(view_params: ViewParams, layers: Vec<Box<dyn DrawToSvg>>) -> Group {
    let (_, group) = init_svg(view_params.width as f64, view_params.height as f64);
    let mut group = group;
    for layer in &layers {
        // TODO: when/where to pass view_params to each layer?
        group = layer.draw(&group).await;
    }

    // TODO: also return RenderResult? How to aggregate results from multiple layers?

    group
}

pub async fn render_canvas(view_params: ViewParams, layers: Vec<Box<dyn DrawToCanvas>>, context: &mut RenderContext<'_>, encoder: &mut wgpu::CommandEncoder) {
    for layer in &layers {
        // TODO: when/where to pass view_params to each layer? during draw call? before draw call?
        // Should we instead assume the layer already has the necessary info from view_params?
        layer.draw(context.device.clone(), context.queue.clone(), encoder).await;
    }

    // TODO: return RenderResult? How to aggregate results from multiple layers?
}
