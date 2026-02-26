use crate::wgpu;
use crate::wgpu::{Extent3d, TextureDescriptor, TextureFormat, TextureUsages};
use crate::two::svg::init_svg;
use svg::node::element::Group;
use crate::render_types::{GpuContext, PrepareResult, RenderResult};
use crate::maybe::{MaybeSend, MaybeSync};
use crate::params::{GraphicsFormat, PlotParams, RenderParams, LayerParams};
use crate::registry::get_layer_from_registry;
use crate::cache::get_or_init_gpu_context;
use serde::{Deserialize, Serialize};

use futures_intrusive::channel::shared::oneshot_channel;

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
    async fn prepare(&mut self, gpu_context: Option<&mut GpuContext<'_>>) -> PrepareResult;
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait DrawToSvg {
    // TODO: take an SVG container struct instead, see comments in two/svg.rs
    async fn draw(&self, group: &Group) -> Group;
}


#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait DrawToRasterGpu: MaybeSend + MaybeSync {
    async fn draw(&self, gpu_context: &mut GpuContext<'_>, pass: &mut wgpu::RenderPass);
}

// Stub trait for CPU-based raster rendering (software rasterizer).
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait DrawToRasterCpu: MaybeSend + MaybeSync {
    // TODO: create stub structs for CpuContext and CpuRenderPass in render_types.rs
    async fn draw(&self, cpu_context: &mut CpuContext<'_>, pass: &mut CpuRenderPass);
}


// Stub trait for CPU-based compute operations.
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait ComputeCpu: MaybeSend + MaybeSync {
    // TODO: what should this return?
    async fn compute(&self, cpu_context: &mut CpuContext<'_>);
}

// Stub trait for GPU-based compute operations via wgpu compute shaders.
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait ComputeGpu: MaybeSend + MaybeSync {
    // TODO: what should this return?
    async fn compute(&self, gpu_context: &mut GpuContext<'_>);
}

pub trait PreparedAndDrawToSvg: PreparedLayer + DrawToSvg + MaybeSend + MaybeSync {}
impl<T: PreparedLayer + DrawToSvg + MaybeSend + MaybeSync> PreparedAndDrawToSvg for T {}

pub trait PreparedAndDrawToCanvas: PreparedLayer + DrawToCanvas + MaybeSend + MaybeSync {}
impl<T: PreparedLayer + DrawToCanvas + MaybeSend + MaybeSync> PreparedAndDrawToCanvas for T {}

// Trait for layers that can render to both SVG and Canvas

// TODO: create a trait that represents all prepare and draw variants.
pub trait PreparedAndDraw: PreparedAndDrawToCanvas + PreparedAndDrawToSvg {}
impl<T: PreparedAndDrawToCanvas + PreparedAndDrawToSvg> PreparedAndDraw for T {}


pub fn get_layer(layer_params: &LayerParams, view_params: &ViewParams) -> Box<dyn PreparedAndDraw> {
    get_layer_from_registry(&layer_params.layer_type, layer_params.layer_params.clone(), view_params)
}


pub fn get_layers(layers: &[LayerParams], view_params: &ViewParams) -> Vec<Box<dyn PreparedAndDraw>> {
    layers.iter().map(|layer_params| {
        get_layer(&layer_params, &view_params)
    }).collect()
}

// gpu_context is None when using the CPU compute backend.
// Layers prepare sequentially because gpu_context holds an exclusive &mut reference
// that cannot be shared across concurrent futures.
pub async fn prepare_layers(
    layers: &mut Vec<Box<dyn PreparedAndDraw>>,
    gpu_context: Option<&mut GpuContext<'_>>,
) -> Vec<PrepareResult> {
    // TODO: if gpu_context is None, and RenderParams.compute_backend is GPU, panic.
    let mut results = Vec::with_capacity(layers.len());
    for layer in layers.iter_mut() {
        // Pass None per-layer for now; GPU compute support will need a per-layer context strategy.
        let result = layer.prepare(gpu_context).await;
        results.push(result);
    }
    results
}

pub async fn draw_layers_to_vector(
    view_params: &ViewParams,
    layers: &mut Vec<Box<dyn PreparedAndDraw>>,
    // TODO: pass Option<gpu_context>
) -> (Group, RenderResult) {
    // TODO: if gpu_context is None, and RenderParams.compute_backend is GPU, panic.

    // TODO: Use a match statement to select the GPU/CPU rendering paths.


    let (_, mut group) = init_svg(view_params.width as f64, view_params.height as f64);

    for layer in layers.iter_mut() {
        group = DrawToSvg::draw(layer.as_ref(), &group).await;
    }

    let bailed_early = false; // TODO: aggregate from prepare_results when timeout support is added.
    (group, RenderResult { bailed_early })
}

pub async fn draw_layers_to_raster(
    view_params: &ViewParams,
    layers: &mut Vec<Box<dyn PreparedAndDraw>>,
    // TODO: pass Option<gpu_context>
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    encoder: &mut wgpu::CommandEncoder,
    out_tex: &wgpu::Texture,
) -> RenderResult {
    // TODO: if gpu_context is None, and RenderParams.render_backend is GPU, panic.

    // TODO: Use a match statement to select the GPU/CPU rendering paths.

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

        for layer in layers.iter_mut() {
            DrawToCanvas::draw(layer.as_ref(), device.clone(), queue.clone(), &mut render_pass).await;
        }

        drop(render_pass);
    }

    let bailed_early = false; // TODO: aggregate from prepare_results when timeout support is added.
    RenderResult { bailed_early }
}

// TODO: move this function back into render.rs
pub async fn render(params: RenderParams) -> Vec<u8> {
    let width = params.width;
    let height = params.height;

    let view_params = ViewParams {
        view_id: params.plot_id.clone(),
        width,
        height,
        margins: Some(MarginParams {
            margin_top: Some(params.margin_top.unwrap_or(0.0)),
            margin_right: Some(params.margin_right.unwrap_or(0.0)),
            margin_bottom: Some(params.margin_bottom.unwrap_or(0.0)),
            margin_left: Some(params.margin_left.unwrap_or(0.0)),
        }),
        device_pixel_ratio: params.device_pixel_ratio,
        camera_view: params.camera_view,
        timeout: params.timeout,
        cache_enabled: params.cache_enabled,
        aspect_ratio_mode: params.aspect_ratio_mode,
        store_name: Some(params.store_name.clone()),
    };

    #[allow(irrefutable_let_patterns)]
    let PlotParams::LayeredPlot(plot_params) = &params.plot_params else {
        panic!("Expected layered plot params");
    };

    let mut layers = get_layers(&plot_params.layers, &view_params);

    match params.format {
        GraphicsFormat::Vector => {
            prepare_layers(&mut layers, None).await;

            let (group, _render_result) = draw_layers_to_vector(&view_params, &mut layers).await;

            let svg_string = group.to_string();

            if !params.svg_compression_enabled {
                return svg_string.as_bytes().to_vec();
            }
            return lz_str::compress_to_uint8_array(&svg_string);
        }
        GraphicsFormat::Raster => {
            let (device, queue) = get_or_init_gpu_context().await;

            let texture_desc = TextureDescriptor {
                label: Some("Final Render Texture"),
                size: Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: TextureFormat::Rgba8UnormSrgb,
                usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
                view_formats: &[],
            };
            let texture = device.create_texture(&texture_desc);

            let bytes_per_pixel: u32 = 4;
            let unpadded_bytes_per_row = width * bytes_per_pixel;
            let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
            let padded_bytes_per_row = ((unpadded_bytes_per_row + align - 1) / align) * align;
            let output_buffer_size = (padded_bytes_per_row as u64) * (height as u64);

            let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Output Buffer"),
                size: output_buffer_size,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });

            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

            let mut gpu_context = GpuContext { device: &device, queue: &queue };
            prepare_layers(&mut layers, Some(&mut gpu_context)).await;

            let render_result = draw_layers_to_raster(
                &view_params,
                &mut layers,
                &device,
                &queue,
                &mut encoder,
                &texture,
            ).await;

            encoder.copy_texture_to_buffer(
                texture.as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &output_buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(padded_bytes_per_row),
                        rows_per_image: None,
                    },
                },
                texture_desc.size,
            );

            queue.submit(Some(encoder.finish()));

            let buffer_slice = output_buffer.slice(..);

            #[cfg(target_arch = "wasm32")]
            {
                let (sender, receiver) = oneshot_channel();
                buffer_slice.map_async(wgpu::MapMode::Read, move |res| {
                    if res.is_err() {
                        panic!("Failed to map texture for reading");
                    }
                    sender.send(res).ok();
                });

                let _ = device.poll(wgpu::PollType::Poll);
                receiver.receive().await.unwrap().unwrap();
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
                    if result.is_err() {
                        panic!("Failed to map texture for reading");
                    }
                });
                let _ = device.poll(wgpu::PollType::wait_indefinitely());
            }

            // TODO: add back comments from render.rs

            let data = buffer_slice.get_mapped_range();

            let num_extra_bytes = 1;
            let mut pixels = vec![0u8; (unpadded_bytes_per_row * height + num_extra_bytes) as usize];

            for y in 0..height {
                let src_start = (y as usize) * (padded_bytes_per_row as usize);
                let src_end = src_start + (unpadded_bytes_per_row as usize);
                let dst_start = (y as usize) * (unpadded_bytes_per_row as usize);
                let dst_end = dst_start + (unpadded_bytes_per_row as usize);
                pixels[dst_start..dst_end].copy_from_slice(&data[src_start..src_end]);
            }

            // Final byte encodes the RenderResult for the caller.
            pixels[(unpadded_bytes_per_row * height) as usize] = match render_result.bailed_early {
                false => 0,
                true => 1,
            };

            drop(data);
            output_buffer.unmap();

            pixels
        }
    }
}
