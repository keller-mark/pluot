use crate::wgpu;
use crate::log;

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use crate::zarr::AsyncZarritaStore;

// Note: this store cache is no longer needed, as the store does cacheing internally now.
static ZARR_STORES: OnceLock<Mutex<HashMap<String, Arc<AsyncZarritaStore>>>> = OnceLock::new();

//static BUFFER_CACHE: OnceLock<Mutex<HashMap<String, Vec<f32>>>> = OnceLock::new();

thread_local! {
    static GPU_CONTEXT: RefCell<Option<(wgpu::Device, wgpu::Queue)>> = const { RefCell::new(None) };
    static BUFFER_CACHE: RefCell<Option<HashMap<Vec<String>, Vec<f32>>>> = const { RefCell::new(None) };
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

pub async fn get_or_init_buffer(initializer: impl AsyncFnOnce() -> Vec<f32>, keys: &[String], cache_enabled: bool) -> Vec<f32> {
    // Initializer param
    // Reference: https://github.com/DioxusLabs/dioxus/blob/ec8f31dece5c75371177bf080bab46dff54ffd0e/packages/core/src/global_context.rs#L284

    if !cache_enabled {
        return initializer().await;
    }

    // This thread_local approach seems to work fine with futures::join!.
    // First, check if the buffer already exists
    let buffer_exists = BUFFER_CACHE.with(|map| {
        map.borrow()
            .as_ref()
            .and_then(|m| m.get(keys).cloned())
    });

    if let Some(buffer) = buffer_exists {
        //log("Buffer found in cache");
        return buffer;
    }

    // Buffer doesn't exist, so create it
    //log("Creating new buffer");
    let buffer = initializer().await;

    // Store it in the cache
    BUFFER_CACHE.with(|map| {
        let mut map_ref = map.borrow_mut();

        // Initialize the map if it doesn't exist
        if map_ref.is_none() {
            *map_ref = Some(HashMap::new());
        }

        // Insert the buffer
        map_ref.as_mut().unwrap().insert(keys.to_vec(), buffer.clone());
    });

    buffer
}
