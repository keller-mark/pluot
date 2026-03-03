use crate::picking::LayerPickingResult;
use crate::viewport::{DataCoord, ScreenCoord};
use crate::wgpu;
use crate::two::svg::{init_svg, SvgContext};
use crate::render_types::{CpuContext, CpuRenderPass, GpuContext, PrepareResult, RenderResult};
use crate::maybe::{MaybeSend, MaybeSync};
use crate::params::LayerParams;
use crate::registry::get_layer_from_registry;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
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

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum UnitsMode {
    // 0: pixels (e.g., for fixed pixel-unit sizes).
    Pixels,
    // 1: data units (e.g., for physical sizes).
    Data,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MarginParams {
    pub margin_left: Option<f32>,
    pub margin_right: Option<f32>,
    pub margin_top: Option<f32>,
    pub margin_bottom: Option<f32>,
}

// Struct to store anything at the view level (i.e., not layer-specific)
#[derive(Clone, Debug, Serialize, Deserialize)]
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
    pub margins: Option<MarginParams>,

    pub store_name: Option<String>,

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
            margins: None,
            store_name: None,
        }
    }
}


#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait PreparedLayer {
    async fn prepare(&mut self, gpu_context: Option<&GpuContext<'_>>) -> PrepareResult;
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait DrawToSvg {
    async fn draw(&self, ctx: &mut SvgContext);
}


#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait DrawToRasterGpu: MaybeSend + MaybeSync {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass);
}

// Stub trait for CPU-based raster rendering (software rasterizer).
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait DrawToRasterCpu: MaybeSend + MaybeSync {
    async fn draw(&self, cpu_context: &CpuContext<'_>, pass: &mut CpuRenderPass);
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait PickableLayer {
    // TODO: should this be async?
    fn pick(&self, screen_coord: ScreenCoord, data_coord: Option<DataCoord>) -> Option<LayerPickingResult> {
        // Default implementation: not pickable, return empty result.
        None
    }
}


// Stub trait for CPU-based compute operations.
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait ComputeCpu: MaybeSend + MaybeSync {
    // TODO: what should this return?
    async fn compute(&self, cpu_context: &CpuContext<'_>);
}

// Stub trait for GPU-based compute operations via wgpu compute shaders.
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait ComputeGpu: MaybeSend + MaybeSync {
    // TODO: what should this return?
    async fn compute(&self, gpu_context: &GpuContext<'_>);
}

pub trait PreparedAndDrawToSvg: PreparedLayer + DrawToSvg + MaybeSend + MaybeSync {}
impl<T: PreparedLayer + DrawToSvg + MaybeSend + MaybeSync> PreparedAndDrawToSvg for T {}

pub trait PreparedAndDrawToRasterGpu: PreparedLayer + DrawToRasterGpu + MaybeSend + MaybeSync {}
impl<T: PreparedLayer + DrawToRasterGpu + MaybeSend + MaybeSync> PreparedAndDrawToRasterGpu for T {}

pub trait PreparedAndDrawToRasterCpu: PreparedLayer + DrawToRasterCpu + MaybeSend + MaybeSync {}
impl<T: PreparedLayer + DrawToRasterCpu + MaybeSend + MaybeSync> PreparedAndDrawToRasterCpu for T {}

// Trait for layers that can prepare and render to all output formats.
pub trait PreparedAndDraw: PreparedLayer + DrawToSvg + DrawToRasterGpu + DrawToRasterCpu + PickableLayer + MaybeSend + MaybeSync {}
impl<T: PreparedLayer + DrawToSvg + DrawToRasterGpu + DrawToRasterCpu + PickableLayer + MaybeSend + MaybeSync> PreparedAndDraw for T {}



pub fn get_layer(layer_params: &LayerParams, view_params: &ViewParams) -> Box<dyn PreparedAndDraw> {
    get_layer_from_registry(&layer_params.layer_type, layer_params.layer_params.clone(), view_params)
}


pub fn get_layers(layers: &[LayerParams], view_params: &ViewParams) -> Vec<Box<dyn PreparedAndDraw>> {
    layers.iter().map(|layer_params| {
        get_layer(&layer_params, &view_params)
    }).collect()
}

pub async fn draw_layers_to_vector(
    view_params: &ViewParams,
    layers: &mut Vec<Box<dyn PreparedAndDraw>>,
    _gpu_context: Option<&GpuContext<'_>>,
) -> (SvgContext, RenderResult) {
    let mut ctx = init_svg(view_params.width as f64, view_params.height as f64);

    for layer in layers.iter_mut() {
        DrawToSvg::draw(layer.as_ref(), &mut ctx).await;
    }

    let bailed_early = false; // TODO: aggregate from prepare_results when timeout support is added.
    (ctx, RenderResult { bailed_early })
}

pub async fn draw_layers_to_raster(
    view_params: &ViewParams,
    layers: &mut Vec<Box<dyn PreparedAndDraw>>,
    gpu_context: &GpuContext<'_>,
    encoder: &mut wgpu::CommandEncoder,
    out_tex: &wgpu::Texture,
) -> RenderResult {
    // For pyo3 usage, we need to use iterator types that are Send to avoid the following error
    // when iterating over vectors of layers:
    // "has type `std::slice::Iter<'_, Box<dyn PreparedAndDrawToCanvas>>` which is not `Send`"
    let layer_refs: Vec<_> = layers.iter_mut().collect();

    let out_view = out_tex.create_view(&wgpu::TextureViewDescriptor::default());

    {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Layered Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &out_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    // TODO: make background color configurable.
                    load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        for layer in layer_refs {
            // TODO: when/where to pass view_params to each layer? during draw call? before draw call?
            // Should we instead assume the layer already has the necessary info from view_params?
            DrawToRasterGpu::draw(layer.as_ref(), &gpu_context, &mut render_pass).await;
        }

        drop(render_pass);
    }

    let bailed_early = false; // TODO: aggregate from prepare_results when timeout support is added.
    RenderResult { bailed_early }
}
