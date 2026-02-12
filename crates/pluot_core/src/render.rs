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

// This function should accept width and height as parameters,
// and return a Uint8Array containing the rendered image data.
pub async fn render(params: RenderParams) -> Vec<u8> {
    let width = params.width;
    let height = params.height;
    let store_name = &params.store_name;

    // The Instance is the context for all other wgpu objects.
    // This is the first thing you create when using wgpu.
    // Its primary use is to create Adapters and Surfaces.
    // Does not have to be kept alive.

    // The InstanceDescriptor has fields for which backends wgpu will choose during instantiation,
    // and which DX12 shader compiler wgpu will use.
    let (device, queue) = get_or_init_gpu_context().await;

    let compute_result = crate::compute_example::compute_example_with_memo(device.clone(), queue.clone()).await;
    log(&format!("Compute shader with memo result: {}", compute_result));

    // Create a texture to render to.
    // TODO: move this rendering setup logic inside render_canvas, after the layer.prepare() functions,
    // to allow the prepare functions to do GPGPU / compute shader pipelines prior to the render pass(es).
    let texture_desc = TextureDescriptor {
        // Debug label of the texture. This will show up in graphics debuggers for easy identification.
        label: Some("Final Render Texture"),
        // Size of the texture. All components must be greater than zero.
        // For a regular 1D/2D texture, the unused sizes will be 1.
        // For 2DArray textures, Z is the number of 2D textures in that array.
        size: Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        // Mip count of texture. For a texture with no extra mips, this must be 1.
        mip_level_count: 1,
        // Sample count of texture. If this is not 1, texture must have [BindingType::Texture::multisampled] set to true.
        sample_count: 1,
        // Dimensions of the texture.
        dimension: wgpu::TextureDimension::D2,
        // Format of the texture.
        format: TextureFormat::Rgba8UnormSrgb,
        // Allowed usages of the texture. If used in other ways, the operation will panic.
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
        // Specifies what view formats will be allowed when calling Texture::create_view on this texture.
        // View formats of the same format as the texture are always allowed.
        // Note: currently, only the srgb-ness is allowed to change. (ex: Rgba8Unorm texture + Rgba8UnormSrgb view)
        view_formats: &[],
    };
    let texture = device.create_texture(&texture_desc);
    //let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    // Create vello scene and texture.
    let vello_tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Shape/Text Overlay Texture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,

        // For Vello:
        // Important: Use a non-sRGB UNORM format for Vello offscreen rendering.
        // Note: Vello requires TextureUsages::STORAGE_BINDING, which requires Rgba8Unorm (incompatible with Rgba8UnormSrgb format)
        /*format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::COPY_SRC,*/
        // For VGER:
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    //let mut vello_scene = vello::Scene::new();

    // Create a buffer to store the output (RGBA8)
    let bytes_per_pixel: u32 = 4;
    let unpadded_bytes_per_row = width * bytes_per_pixel;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT; // 256
    let padded_bytes_per_row = ((unpadded_bytes_per_row + align - 1) / align) * align;
    let output_buffer_size = (padded_bytes_per_row as u64) * (height as u64);

    let output_buffer_desc = wgpu::BufferDescriptor {
        label: Some("Output Buffer"),
        size: output_buffer_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    };
    let output_buffer = device.create_buffer(&output_buffer_desc);

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Render Encoder"),
    });

    let store = get_or_init_store(store_name);

    let view_params = ViewParams {
        view_id: params.plot_id.clone(),
        width: params.width,
        height: params.height,
        margins: None,
        device_pixel_ratio: params.device_pixel_ratio,
        camera_view: params.camera_view,
        timeout: params.timeout,
        cache_enabled: params.cache_enabled,
        aspect_ratio_mode: params.aspect_ratio_mode,
        store_name: Some(store_name.clone()),
    };

    let (_, mut group) = init_svg(width as f64, height as f64);

    // TODO: pass view_params via context?
    // TODO: pass encoder via context?
    let mut context = RenderContext {
        store: &store,
        device: &device,
        texture_desc: &texture_desc,
        out_tex: &texture,
        queue: &queue,
        params: &params,

        vello_tex: &vello_tex,
        //vello_scene: &mut vello_scene,
        out_group: &mut group,
    };

    // All render functions will return layers here.
    // Plot type-specific rendering logic.
    let plot_layers = match params.plot_params {
        /*
        PlotParams::Triangle => plots::triangle::render_triangle(&mut context, &mut encoder).await,
        PlotParams::Scatterplot(_) => {
            plots::scatterplot::render_scatterplot(&mut context, &mut encoder).await
        }
        PlotParams::Scatterplot3d(_) => {
            plots::scatterplot_3d::render_scatterplot_3d(&mut context, &mut encoder).await
        }
        PlotParams::Bioimage(_) => {
            plots::bioimage::render_bioimage(&mut context, &mut encoder).await
        }
        PlotParams::BarPlot(_) => plots::barplot::render_barplot(&mut context, &mut encoder).await,
        */
        PlotParams::LayeredPlot(_) => {
            plots::layered_plot::render_layered_plot(&mut context, &mut encoder)
        }
        _ => panic!("Unsupported plot type"),
    };

    // Then, we call the render_svg or render_canvas function from layers/core.rs
    // to obtain a RenderResult.
    let render_result = match params.format {
        GraphicsFormat::Raster => render_canvas(
                view_params,
                plot_layers,
                &mut context,
                &mut encoder,
            )
            .await,
        GraphicsFormat::Vector => render_svg(
                view_params,
                plot_layers,
                &mut context,
            )
            .await
    };



    // Finally, we handle the output based on the format.


    if params.format == GraphicsFormat::Vector {
        // Return the SVG string as bytes.
        let svg_string = context.out_group.to_string();

        // If compression is not enabled, return the SVG string bytes.
        if !params.svg_compression_enabled {
            return svg_string.as_bytes().to_vec();
        }

        // If compression is enabled, use lz-string before returning the Uint8Array.
        return lz_str::compress_to_uint8_array(&svg_string);
    }





    // Copy the texture to the output buffer.
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &output_buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                // Must be 256-byte aligned on WebGPU
                bytes_per_row: Some(padded_bytes_per_row),
                //rows_per_image: Some(height),
                rows_per_image: None,
            },
        },
        texture_desc.size,
    );

    // TODO: vello runs queue.submit internally, for the previous encoder passed to the render_triangle function:
    queue.submit(Some(encoder.finish()));

    // Map and await completion without blocking the browser thread
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
        // See https://github.com/lapce/floem/blob/5f4709b9c4806f0a21b3450bd9795c3472f8fc2e/vello/src/lib.rs#L595
        //println!("Starting map_async");
        // See https://github.com/gfx-rs/wgpu/blob/c488bbe60447d28736c26c82a32cd87794b3bf1d/examples/features/src/framework.rs#L598
        // Reference: https://github.com/milos-agathon/forge3d/blob/e4ded1b8067b3b15afd46c4a3f6cf34e9fa7a0a0/src/CLAUDE.md?plain=1#L92
        //let (tx, rx) = std::sync::mpsc::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            if result.is_err() {
                panic!("Failed to map texture for reading");
            }
            //let _ = tx.send(result);
        });

        let _ = device.poll(wgpu::PollType::wait_indefinitely());

        //rx.recv().unwrap().unwrap();
        //let mapped = slice.get_mapped_range();

        // TODO: When called from python,
        // does this device.poll need to be called outside of the pyo3_async_runtimes::tokio::future_into_py block?
        // TODO: Do we need to use pyo3's py.detach within py.attach()?
        // This seems to hang when called from a jupyter notebook (although only when vello is used to render something)
        // TODO: look more closely into what Vello is doing.
        // What is the Vello .render_to_texture function doing internally that is causing this to block?
        // TODO: Maybe it needs surface_texture.present() to be called first?
        // See https://github.com/yutannihilation/vellogd-r/blob/17e7380fcf9883ea50cf01a9bfd1cfb4dbcb32ee/src/rust/vellogd-shared/src/winit_app/mod.rs#L649
        //
        // See info from vello about using a Blitter
        // Reference: https://github.com/linebender/vello/blob/54e2a47abd0a9b1ad8b172bbaffed97d1c2248d6/CHANGELOG.md?plain=1#L64C2-L73C35
        //
        // Also see this usage of Vello which mentions deadlocking:
        // Reference https://github.com/DioxusLabs/blitz/blob/dbca61c417f6289640a2ca20a2d87473ccc473ee/packages/anyrender_vello/src/wgpu_context.rs#L314
        //
        // Also see this dicsussion about deadlocking from pyo3-log
        // Reference: https://docs.rs/pyo3-log/latest/pyo3_log/#interaction-with-python-gil
        /*loop {
            let poll_result = device.poll(wgpu::PollType::Poll);
            if (poll_result.is_err()) {
                break;
            }
            if (poll_result.unwrap() == wgpu::PollStatus::QueueEmpty) {
                break;
            }
            println!("Still polling");
        }*/
        //println!("buffer_slice.map_async");

        // TODO: See how Graphite uses Vello
        // Reference: https://github.com/GraphiteEditor/Graphite/blob/1b91198b28ddb5648c51a3754f2aecf75c32eafe/desktop/src/render/state.rs#L239
    }

    // Read and depad rows into a tightly packed RGBA buffer
    let data = buffer_slice.get_mapped_range();


    let NUM_EXTRA_BYTES = 1;

    let mut pixels = vec![0u8; (unpadded_bytes_per_row * height + NUM_EXTRA_BYTES) as usize];

    for y in 0..height {
        let src_start = (y as usize) * (padded_bytes_per_row as usize);
        let src_end = src_start + (unpadded_bytes_per_row as usize);
        let dst_start = (y as usize) * (unpadded_bytes_per_row as usize);
        let dst_end = dst_start + (unpadded_bytes_per_row as usize);
        pixels[dst_start..dst_end].copy_from_slice(&data[src_start..src_end]);
    }

    // Add final byte to provide the RenderResult values to the caller.
    pixels[(unpadded_bytes_per_row * height) as usize] = match render_result.bailed_early {
        false => 0,
        true => 1,
    };

    drop(data);
    output_buffer.unmap();

    pixels
}
