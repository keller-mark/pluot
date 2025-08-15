mod set_panic_hook;
mod plots;
mod utils;

use wasm_bindgen::prelude::*;

use std::cell::RefCell;

use vello::wgpu;
use vello::wgpu::{TextureDescriptor, TextureUsages, TextureFormat, Extent3d};
use vello::{
    peniko::{Blob, Brush, Color, Fill, Font},
    AaConfig, AaSupport, Renderer, RendererOptions, RenderParams, Scene,
};
use futures_intrusive::channel::shared::oneshot_channel;

use crate::utils::RenderContext;

thread_local! {
    static GPU_CONTEXT: RefCell<Option<(wgpu::Device, wgpu::Queue)>> = RefCell::new(None);
    static VELLO_RENDERER: RefCell<Option<Renderer>> = RefCell::new(None);
}



async fn get_or_init_gpu_context() -> (wgpu::Device, wgpu::Queue) {
    // Check if already initialized
    let existing = GPU_CONTEXT.with(|ctx| ctx.borrow().clone());
    if let Some(context) = existing {
        return context;
    }
    
    // Initialize GPU context
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions::default())
        .await
        .expect("No suitable GPU adapters found on the system!");
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor::default(), None)
        .await
        .expect("Failed to create device");
    
    // Store the context
    GPU_CONTEXT.with(|ctx| {
        *ctx.borrow_mut() = Some((device.clone(), queue.clone()));
    });
    
    (device, queue)
}

fn with_vello_renderer<F, R>(device: &wgpu::Device, f: F) -> R
where
    F: FnOnce(&mut Renderer) -> R,
{
    VELLO_RENDERER.with(|renderer| {
        // Check if already initialized
        if renderer.borrow().is_none() {
            let vello_renderer = Renderer::new(
                device,
                RendererOptions {
                    use_cpu: false,
                    antialiasing_support: AaSupport::all(),
                    num_init_threads: std::num::NonZeroUsize::new(1),
                    pipeline_cache: None,
                },
            ).expect("create vello renderer");
            
            *renderer.borrow_mut() = Some(vello_renderer);
        }
        
        f(renderer.borrow_mut().as_mut().unwrap())
    })
}

#[wasm_bindgen]
extern "C" {
    fn alert(s: &str);

    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);

    #[wasm_bindgen(js_name = zarr_get)]
    pub async fn zarr_get_js(store_name: &str, key: &str) -> js_sys::Int32Array;
}

// This function should accept width and height as parameters,
// and return a Uint8Array containing the rendered image data.
#[wasm_bindgen]
pub async fn render(width: u32, height: u32, plot_type: &str, store_name: &str) -> js_sys::Uint8Array {
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
        label: Some("Render Texture"),
        // Size of the texture. All components must be greater than zero.
        // For a regular 1D/2D texture, the unused sizes will be 1.
        // For 2DArray textures, Z is the number of 2D textures in that array.
        size: Extent3d { width, height, depth_or_array_layers: 1 },
        // Mip count of texture. For a texture with no extra mips, this must be 1.
        mip_level_count: 1,
        // Sample count of texture. If this is not 1, texture must have [BindingType::Texture::multisampled] set to true.
        sample_count: 1,
        // Dimensions of the texture.
        dimension: wgpu::TextureDimension::D2,
        // Format of the texture.
        format: TextureFormat::Rgba8Unorm,
        // Allowed usages of the texture. If used in other ways, the operation will panic.
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC | TextureUsages::STORAGE_BINDING,
        // Specifies what view formats will be allowed when calling Texture::create_view on this texture.
        // View formats of the same format as the texture are always allowed.
        // Note: currently, only the srgb-ness is allowed to change. (ex: Rgba8Unorm texture + Rgba8UnormSrgb view)
        view_formats: &[],
    };
    let texture = device.create_texture(&texture_desc);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    // Create vello scene and texture.

    let vello_tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Vello Text Overlay Texture"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        // Important: Use a non-sRGB UNORM format for Vello offscreen rendering.
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::STORAGE_BINDING,
        view_formats: &[],
    });
    let vello_view = vello_tex.create_view(&wgpu::TextureViewDescriptor::default());

    let mut vello_scene = Scene::new();


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

    let mut context = RenderContext {
        store_name: store_name.to_string(),
        device: &device,
        texture_desc: &texture_desc,
        view: &view,
        queue: &queue,
        width,
        height,
        vello_tex: &vello_tex,
        vello_view: &vello_view,
        vello_scene: &mut vello_scene,
    };

    match plot_type {
        "triangle" => {
            plots::render_triangle(&mut context, &mut encoder).await;
        },
        "scatterplot" => {
            plots::render_scatterplot(&mut context, &mut encoder).await;
        },
        _ => panic!("Unsupported plot type"),
    }

    // Copy the texture to the output buffer.
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &output_buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                // Must be 256-byte aligned on WebGPU
                bytes_per_row: Some(padded_bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        texture_desc.size,
    );

    let command_buffer = encoder.finish();
    queue.submit([command_buffer]);

    // Map and await completion without blocking the browser thread
    let buffer_slice = output_buffer.slice(..);
    let (sender, receiver) = oneshot_channel();
    buffer_slice.map_async(wgpu::MapMode::Read, move |res| {
        sender.send(res).ok();
    });
    let _ = device.poll(wgpu::Maintain::Wait);
    receiver.receive().await.unwrap().unwrap();

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

    // Return a Uint8Array of RGBA bytes
    js_sys::Uint8Array::from(pixels.as_slice())
}

#[wasm_bindgen]
pub fn greet() {
    alert("Hello, pluot!");
}
