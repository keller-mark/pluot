mod plots;
mod utils;
mod zarr;

use wgpu::{TextureDescriptor, TextureUsages, TextureFormat, Extent3d};
use futures_intrusive::channel::shared::oneshot_channel;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock, Arc};

pub use crate::utils::RenderParams;
use crate::utils::RenderContext;
use crate::zarr::{AsyncZarritaStore};



// Note: this store cache is no longer needed, as the store does cacheing internally now.
static ZARR_STORES: OnceLock<Mutex<HashMap<String, Arc<AsyncZarritaStore>>>> = OnceLock::new();

thread_local! {
    static GPU_CONTEXT: RefCell<Option<(wgpu::Device, wgpu::Queue)>> = RefCell::new(None);
}

async fn init_gpu_context() -> (wgpu::Device, wgpu::Queue) {
    // Apparently this is expensive, so we try to cache it in the get_or_init_gpu_context function.
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
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
async fn get_or_init_gpu_context() -> (wgpu::Device, wgpu::Queue) {
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
async fn get_or_init_gpu_context() -> (wgpu::Device, wgpu::Queue) {
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

// == WASM Bindings ===
#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;
    use super::{render, RenderParams};

    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_namespace = console)]
        pub fn log(s: &str);

        // We need to define a `has` function, since the zarr_get_js function
        // may return undefined, and wasm_bindgen does not allow
        // Option<Uint8Array> as a return type annotation.
        #[wasm_bindgen(js_name = zarr_has)]
        async fn zarr_has_js(store_name: &str, key: &str) -> js_sys::Boolean;

        #[wasm_bindgen(js_name = zarr_get)]
        async fn zarr_get_js(store_name: &str, key: &str) -> js_sys::Uint8Array;

        #[wasm_bindgen(js_name = zarr_get_range_from_offset)]
        async fn zarr_get_range_from_offset_js(store_name: &str, key: &str, offset: u32, length: u32) -> js_sys::Uint8Array;

        #[wasm_bindgen(js_name = zarr_get_range_from_end)]
        async fn zarr_get_range_from_end_js(store_name: &str, key: &str, suffix_length: u32) -> js_sys::Uint8Array;
    }

    fn convert_to_bytes(u8arr: js_sys::Uint8Array) -> zarrs::storage::AsyncBytes {
        // Copy data from Uint8Array into a Rust Vec<u8>
        let mut vec = vec![0u8; u8arr.length() as usize];

        // TODO: can this be done without copying?
        // The issue is that the original Uint8Array is created via JS fetch() within zarrita fetchStore.

        u8arr.copy_to(&mut vec);

        // Convert Vec<u8> into Bytes
        zarrs::storage::Bytes::from(vec)
    }

    pub async fn zarr_has(store_name: &str, key: &str) -> bool {
        let has = zarr_has_js(store_name, key).await;
        has.is_truthy()
    }

    pub async fn zarr_get(store_name: &str, key: &str) -> zarrs::storage::AsyncBytes {
        let js_bytes = zarr_get_js(store_name, key).await;
        convert_to_bytes(js_bytes)
    }

    pub async fn zarr_get_range_from_offset(store_name: &str, key: &str, offset: u32, length: u32) -> zarrs::storage::AsyncBytes {
        let js_bytes = zarr_get_range_from_offset_js(store_name, key, offset, length).await;
        convert_to_bytes(js_bytes)
    }

    pub async fn zarr_get_range_from_end(store_name: &str, key: &str, suffix_length: u32) -> zarrs::storage::AsyncBytes {
        let js_bytes = zarr_get_range_from_end_js(store_name, key, suffix_length).await;
        convert_to_bytes(js_bytes)
    }


    #[wasm_bindgen]
    pub fn set_panic_hook() {
        // When the `console_error_panic_hook` feature is enabled, we can call the
        // `set_panic_hook` function at least once during initialization, and then
        // we will get better error messages if our code ever panics.
        //
        // For more details see
        // https://github.com/rustwasm/console_error_panic_hook#readme
        #[cfg(feature = "console_error_panic_hook")]
        console_error_panic_hook::set_once();
    }

    // This function should accept width and height as parameters,
    // and return a Uint8Array containing the rendered image data.
    #[wasm_bindgen]
    pub async fn render_wasm(params: JsValue) -> js_sys::Uint8Array {
        let params: RenderParams = serde_wasm_bindgen::from_value(params)
            .expect("Invalid parameters");

        let pixels = render(params).await;

        // Return a Uint8Array of RGBA bytes
        js_sys::Uint8Array::from(pixels.as_slice())
    }
}

// === Python Bindings ===
#[cfg(all(not(target_arch = "wasm32"), feature = "python"))]
mod python {
    use pyo3::prelude::*;
    use pyo3::wrap_pyfunction;
    use pyo3::types::{PyBytes, PyDict, PyAny};
    use pyo3::ToPyObject;

    use serde_pyobject::from_pyobject;
    use super::{render, RenderParams};

    pub fn log(s: &str) {
        println!("{}", s);
    }

    pub async fn zarr_has(store_name: &str, key: &str) -> bool {
        // Acquire the Python GIL. This must be done for all Python interactions.
        let result = Python::with_gil(|py| {
            let main_module = py.import_bound("__main__").unwrap();

            let has_func = main_module.getattr("zarr_has").unwrap();
            let result: bool = has_func.call1((store_name, key)).unwrap().extract().unwrap();
            result
        });

        result
    }

    pub async fn zarr_get(store_name: &str, key: &str) -> zarrs::storage::AsyncBytes {
        // Acquire the Python GIL. This must be done for all Python interactions.
        let result = Python::with_gil(|py| {

            let main_module = py.import_bound("__main__").unwrap();

            let get_func = main_module.getattr("zarr_get").unwrap();
            let result: Vec<u8> = get_func.call1((store_name, key)).unwrap().extract().unwrap();
            zarrs::storage::Bytes::from(result)
        });
        result
    }

    pub async fn zarr_get_range_from_offset(store_name: &str, key: &str, offset: u32, length: u32) -> zarrs::storage::AsyncBytes {
        // Acquire the Python GIL. This must be done for all Python interactions.
        let result = Python::with_gil(|py| {
            let main_module = py.import_bound("__main__").unwrap();

            let get_func = main_module.getattr("zarr_get_range_from_offset").unwrap();
            let result: Vec<u8> = get_func.call1((store_name, key, offset, length)).unwrap().extract().unwrap();
            zarrs::storage::Bytes::from(result)
        });
        result
    }

    pub async fn zarr_get_range_from_end(store_name: &str, key: &str, suffix_length: u32) -> zarrs::storage::AsyncBytes {
        // Acquire the Python GIL. This must be done for all Python interactions.
        let result = Python::with_gil(|py| {
            let main_module = py.import_bound("__main__").unwrap();

            let get_func = main_module.getattr("zarr_get_range_from_end").unwrap();
            let result: Vec<u8> = get_func.call1((store_name, key, suffix_length)).unwrap().extract().unwrap();
            zarrs::storage::Bytes::from(result)
        });
        result
    }



    /*
    #[pyfunction]
    #[pyo3(signature = (**kwds))]
    pub async fn render_py<'a>(kwds: Option<&Bound<'_, PyDict>>) -> PyResult<Py<PyBytes>> {
        // 1. Create the parameters struct from the direct inputs.

        /*let params = Python::with_gil(|py| {
            from_pyobject(kwds.unwrap().into_bound(py)).unwrap()
        });*/
        let params: RenderParams = from_pyobject(kwds.unwrap().clone()).unwrap();

        // 2. Await the core async rendering logic.
        let pixels = render(params).await;

        // 3. Return the pixel data. PyO3 automatically converts a Vec<u8>
        //    into a Python `bytes` object. The `PyResult` handles errors.
       
        Python::with_gil(|py| {
            Ok(PyBytes::new(py, &pixels).into_py(py))
        })
        
        //Ok(PyBytes::new(py, &pixels))
    }
    */

    /*
    #[pyfunction]
    #[pyo3(signature = (**kwds))]
    pub async fn render_py(kwds: Option<PyObject>) -> Vec<u8> {

        let params: RenderParams = Python::with_gil(|py| {
            if let Some(dict) = kwds {
                from_pyobject::<RenderParams, _>(dict.into_bound(py))
            } else {
                Ok(RenderParams::default())
            }
        }).unwrap();

        let pixels = render(params).await;
        pixels
    }
    */

    /*
    #[pyfunction]
    #[pyo3(signature = (**kwds))]
    pub fn render_py(py: Python, kwds: Option<PyObject>) -> PyResult<Bound<PyAny>> {
        let params: RenderParams = Python::with_gil(|py| {
            if let Some(dict) = kwds {
                from_pyobject::<RenderParams, _>(dict.into_bound(py))
            } else {
                Ok(RenderParams::default())
            }
        }).unwrap();

        pyo3_async_runtimes::tokio::future_into_py(py, async {
            let pixels = render(params).await;
            Ok(pixels)
        })
    }
    */
    #[pyfunction]
    #[pyo3(signature = (**kwds))]
    pub fn render_py(py: Python, kwds: Option<PyObject>) -> PyResult<Bound<PyAny>> {
        // Use the py parameter directly instead of Python::with_gil
        let params: RenderParams = if let Some(dict) = kwds {
            from_pyobject::<RenderParams, _>(dict.into_bound(py)).unwrap()
        } else {
            RenderParams::default()
        };

        println!("Plot type: {:?}", params.plot_type);

        pyo3_async_runtimes::tokio::future_into_py(py, async {
            let pixels = render(params).await;
            Ok(pixels)
        })
    }


    
    // This function creates the Python module.
    #[pymodule]
    fn pluot_py_wrapper(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
        m.add_function(wrap_pyfunction!(render_py, m)?)?;
        Ok(())
    }
}

// === Rust-only Bindings ===
#[cfg(all(not(target_arch = "wasm32"), not(feature = "python")))]
mod plain_rust {
    use core::panic;

    use super::{render, RenderParams};

    pub fn log(s: &str) {
        println!("{}", s);
    }

    pub async fn zarr_has(store_name: &str, key: &str) -> bool {
        panic!("zarr_has is not implemented in plain Rust mode.");
    }

    pub async fn zarr_get(store_name: &str, key: &str) -> zarrs::storage::AsyncBytes {
        panic!("zarr_get is not implemented in plain Rust mode.");
    }

    pub async fn zarr_get_range_from_offset(store_name: &str, key: &str, offset: u32, length: u32) -> zarrs::storage::AsyncBytes {
        panic!("zarr_get_range_from_offset is not implemented in plain Rust mode.");
    }

    pub async fn zarr_get_range_from_end(store_name: &str, key: &str, suffix_length: u32) -> zarrs::storage::AsyncBytes {
        panic!("zarr_get_range_from_end is not implemented in plain Rust mode.");
    }
}

// Unified exports.
#[cfg(target_arch = "wasm32")]
pub use wasm::{log, zarr_has, zarr_get, zarr_get_range_from_offset, zarr_get_range_from_end};

#[cfg(all(not(target_arch = "wasm32"), feature = "python"))]
pub use python::{log, zarr_has, zarr_get, zarr_get_range_from_offset, zarr_get_range_from_end};

#[cfg(all(not(target_arch = "wasm32"), not(feature = "python")))]
pub use plain_rust::{log, zarr_has, zarr_get, zarr_get_range_from_offset, zarr_get_range_from_end};

// This function should accept width and height as parameters,
// and return a Uint8Array containing the rendered image data.
pub async fn render(params: RenderParams) -> Vec<u8> {
    let width = params.width;
    let height = params.height;
    let plot_type = &params.plot_type;
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
        format: TextureFormat::Rgba8UnormSrgb,
        // Allowed usages of the texture. If used in other ways, the operation will panic.
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
        // Specifies what view formats will be allowed when calling Texture::create_view on this texture.
        // View formats of the same format as the texture are always allowed.
        // Note: currently, only the srgb-ness is allowed to change. (ex: Rgba8Unorm texture + Rgba8UnormSrgb view)
        view_formats: &[],
    };
    let texture = device.create_texture(&texture_desc);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

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
        view: &view,
        queue: &queue,
        params: &params,
    };

    // Plot type-specific rendering logic.
    match plot_type.as_str() {
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

    #[cfg(target_arch = "wasm32")]
    {
        let _ = device.poll(wgpu::PollType::Poll);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = device.poll(wgpu::PollType::Wait);
    }
        
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

    pixels
}
