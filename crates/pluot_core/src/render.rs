use crate::wgpu;
use crate::wgpu::{Extent3d, TextureDescriptor, TextureFormat, TextureUsages};
use crate::render_types::GpuContext;
use crate::params::{GraphicsFormat, PlotParams, RenderParams, RenderBackend, ComputeBackend};
use crate::render_traits::{MarginParams, ViewParams, get_layers, draw_layers_to_vector, draw_layers_to_raster};
use crate::cache::get_or_init_gpu_context;
use crate::render_post::unpremultiply;

use futures_intrusive::channel::shared::oneshot_channel;

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
        wait_for_store_gets: params.wait_for_store_gets,
        cache_enabled: params.cache_enabled,
        aspect_ratio_mode: params.aspect_ratio_mode,
        aspect_ratio_alignment_mode: params.aspect_ratio_alignment_mode,
        store_name: Some(params.store_name.clone()),
    };

    #[allow(irrefutable_let_patterns)]
    let PlotParams::LayeredPlot(plot_params) = &params.plot_params else {
        panic!("Expected layered plot params");
    };

    let mut layers = get_layers(&plot_params.layers, &view_params);

    let owned_gpu_context: Option<(wgpu::Device, wgpu::Queue)>;
    if params.render_backend == Some(RenderBackend::Gpu) || params.compute_backend == Some(ComputeBackend::Gpu) {
        // GPU explicitly requested: panic if GPU support is unavailable.
        owned_gpu_context = Some(
            get_or_init_gpu_context().await
                .expect("No suitable GPU adapters found on the system!")
        );
    } else if params.render_backend.is_none() || params.compute_backend.is_none() {
        // Backend not specified: try GPU, then fall back to CPU gracefully without panicking.
        owned_gpu_context = get_or_init_gpu_context().await;
    } else {
        owned_gpu_context = None;
    }

    let gpu_context = owned_gpu_context.as_ref().map(|(device, queue)| GpuContext { device, queue });

    // Collect references first to avoid Send issues with the iterator
    let prepare_futures: Vec<_> = layers.iter_mut().map(|layer| layer.prepare(gpu_context.as_ref())).collect();

    // Collect all PrepareResult values and update bailed_early if any of them bailed early,
    // aggregating the prepare results from all layers.
    // TODO: use maybe_timeout! here? or only within individual prepare functions?
    let prepare_results = futures::future::join_all(prepare_futures).await;
    let prepare_bailed_early = prepare_results.iter().any(|r| r.bailed_early);

    match params.format {
        GraphicsFormat::Vector => {
            let (ctx, _render_result) = draw_layers_to_vector(&view_params, &mut layers, gpu_context.as_ref()).await;

            // Return the SVG string as bytes.
            let svg_string = ctx.to_svg_string(params.svg_include_document);

            // If compression is not enabled, return the SVG string bytes.
            if !params.svg_compression_enabled {
                return svg_string.as_bytes().to_vec();
            }
            // If compression is enabled, use lz-string before returning the Uint8Array.
            return lz_str::compress_to_uint8_array(&svg_string);
        }
        GraphicsFormat::Raster => {
            // TODO: allow for CPU raster rendering if GPU isn't available or if compute_backend is CPU.

            let gpu_context = gpu_context.expect("GPU context should be available for raster rendering");


            // Create a texture to render to.
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
                format: TextureFormat::Rgba8Unorm,
                // TEXTURE_BINDING so the un-premultiply pass below can sample it.
                usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC | TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            };
            let texture = gpu_context.device.create_texture(&texture_desc);

            // Second texture that receives the straight-alpha result of the
            // un-premultiply post-process pass. This is what gets copied out;
            // we can't read and write `texture` in the same pass.
            let resolved_texture = gpu_context.device.create_texture(&TextureDescriptor {
                label: Some("Un-premultiplied Render Texture"),
                size: texture_desc.size,
                mip_level_count: texture_desc.mip_level_count,
                sample_count: texture_desc.sample_count,
                dimension: texture_desc.dimension,
                format: texture_desc.format,
                usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
                view_formats: &[],
            });

            // Create a buffer to store the output (RGBA8)
            let bytes_per_pixel: u32 = 4;
            let unpadded_bytes_per_row = width * bytes_per_pixel;
            let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT; // 256
            let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(align) * align;
            let output_buffer_size = (padded_bytes_per_row as u64) * (height as u64);

            let output_buffer = gpu_context.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Output Buffer"),
                size: output_buffer_size,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });

            let mut encoder = gpu_context.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

            let render_result = draw_layers_to_raster(
                &view_params,
                &mut layers,
                &gpu_context,
                &mut encoder,
                &texture,
            ).await;

            // Un-premultiply on the GPU: a full-screen pass that samples the
            // premultiplied `texture` and writes straight alpha into
            // `resolved_texture`.
            unpremultiply(&gpu_context, &mut encoder, &texture, &resolved_texture);

            // Copy the un-premultiplied texture to the output buffer.
            encoder.copy_texture_to_buffer(
                resolved_texture.as_image_copy(),
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

            gpu_context.queue.submit(Some(encoder.finish()));

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

                let _ = gpu_context.device.poll(wgpu::PollType::Poll);
                receiver.receive().await.unwrap().unwrap();
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
                    if result.is_err() {
                        panic!("Failed to map texture for reading");
                    }
                });
                let _ = gpu_context.device.poll(wgpu::PollType::wait_indefinitely());
            }

            // Read and depad rows into a tightly packed RGBA buffer
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

            // De-premultiplication now happens on the GPU (see the un-premultiply
            // pass above), so the depadded buffer already holds straight alpha.

            let mut bailed_early = prepare_bailed_early;
            bailed_early = bailed_early || render_result.bailed_early;

            // Add final byte to provide the RenderResult values to the caller.
            pixels[(unpadded_bytes_per_row * height) as usize] = match bailed_early {
                false => 0,
                true => 1,
            };

            drop(data);
            output_buffer.unmap();

            pixels
        }
    }
}
