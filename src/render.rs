use crate::wgpu;
use crate::wgpu::{Extent3d, TextureDescriptor, TextureFormat, TextureUsages};
/*use vello::{
    peniko::{Blob, Brush, Color, Fill, Font},
    AaConfig, AaSupport, Renderer, RendererOptions, Scene,
};*/

use futures::FutureExt;
use futures_intrusive::channel::shared::oneshot_channel;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use crate::params::RenderContext;
pub use crate::params::{PlotParams, RenderParams};
use crate::plots;
use crate::zarr::AsyncZarritaStore;

// Note: this store cache is no longer needed, as the store does cacheing internally now.
static ZARR_STORES: OnceLock<Mutex<HashMap<String, Arc<AsyncZarritaStore>>>> = OnceLock::new();

thread_local! {
    static GPU_CONTEXT: RefCell<Option<(wgpu::Device, wgpu::Queue)>> = RefCell::new(None);
}

async fn init_gpu_context() -> (wgpu::Device, wgpu::Queue) {
    // Apparently this is expensive, so we try to cache it in the get_or_init_gpu_context function.
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    // We can try to enable WebGL fallback here, but it is not working,
    // even when we add wgpu as a direct dependency with the "webgl" feature enabled.
    /*
    let instance = wgpu::util::new_instance_with_webgpu_detection(
        &wgpu::InstanceDescriptor::default(),
    ).await;
    */
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions::default())
        .await
        .expect("No suitable GPU adapters found on the system!");
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor::default())
        .await
        .expect("Failed to create device");
    (device, queue)
}

#[cfg(target_arch = "wasm32")]
pub async fn get_or_init_gpu_context() -> (wgpu::Device, wgpu::Queue) {
    // Check if already initialized
    let existing = GPU_CONTEXT.with(|ctx| ctx.borrow().clone());
    if let Some(context) = existing {
        return context;
    }

    // Initialize GPU context
    let (device, queue) = init_gpu_context().await;

    // Store the context
    GPU_CONTEXT.with(|ctx| {
        *ctx.borrow_mut() = Some((device.clone(), queue.clone()));
    });

    (device, queue)
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn get_or_init_gpu_context() -> (wgpu::Device, wgpu::Queue) {
    // The tokio::test will fail if we rely on thread_local to cache the GPU context.
    // So we just create a new context each time for now.

    // TODO: cache in a way that is compatible with tokio::test.
    let (device, queue) = init_gpu_context().await;
    (device, queue)
}

pub fn get_or_init_store(name: &str) -> Arc<AsyncZarritaStore> {
    let map_mutex = ZARR_STORES.get_or_init(|| Mutex::new(HashMap::new()));
    let map = map_mutex.lock().unwrap();

    if let Some(store) = map.get(name) {
        store.clone()
    } else {
        drop(map);
        let mut map = map_mutex.lock().unwrap();
        map.entry(name.to_string())
            .or_insert_with(|| Arc::new(AsyncZarritaStore::new(name.to_string())))
            .clone()
    }
}

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

    let mut context = RenderContext {
        store: &store,
        device: &device,
        texture_desc: &texture_desc,
        out_tex: &texture,
        queue: &queue,
        params: &params,

        vello_tex: &vello_tex,
        //vello_scene: &mut vello_scene,
    };

    // Plot type-specific rendering logic.
    match params.plot_params {
        PlotParams::Triangle => {
            plots::triangle::render_triangle(&mut context, &mut encoder).await;
        }
        PlotParams::Scatterplot(_) => {
            plots::scatterplot::render_scatterplot(&mut context, &mut encoder).await;
        }
        PlotParams::Bioimage(_) => {
            plots::bioimage::render_bioimage(&mut context, &mut encoder).await;
        }
        _ => panic!("Unsupported plot type"),
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

        let _ = device.poll(wgpu::PollType::Wait);

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
    }

    // Read and depad rows into a tightly packed RGBA buffer
    let data = buffer_slice.get_mapped_range();

    let mut pixels = vec![0u8; (unpadded_bytes_per_row * height) as usize];
    for y in 0..height {
        let src_start = (y as usize) * (padded_bytes_per_row as usize);
        let src_end = src_start + (unpadded_bytes_per_row as usize);
        let dst_start = (y as usize) * (unpadded_bytes_per_row as usize);
        let dst_end = dst_start + (unpadded_bytes_per_row as usize);
        pixels[dst_start..dst_end].copy_from_slice(&data[src_start..src_end]);
    }
    drop(data);
    output_buffer.unmap();

    pixels
}

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
            push_constant_ranges: &[],
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
            multiview: None,
            cache: None,
        });

    let overlay_sampler = context.device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Overlay Sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::FilterMode::Nearest,
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
