use crate::wgpu;
use crate::log;

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use crate::zarr::AsyncZarritaStore;

// Note: this store cache is no longer needed, as the store does cacheing internally now.
static ZARR_STORES: OnceLock<Mutex<HashMap<String, Arc<AsyncZarritaStore>>>> = OnceLock::new();

/// Cached internal data for TextLayer rendering.
/// Contains the font atlas bitmap and per-glyph instance data.
#[derive(Clone)]
pub struct CachedInternalTextLayerData {
    pub atlas_data: Vec<u8>,
    pub all_instance_data: Vec<f32>,
    pub atlas_width: usize,
    pub atlas_height: usize,
}

thread_local! {
    static GPU_CONTEXT: RefCell<Option<(wgpu::Device, wgpu::Queue)>> = const { RefCell::new(None) };
    // TODO: How to generalize the USE_MEMO_CACHE___ to support other numeric dtypes?
    // Would it be better (or possible) to cache wgpu::Buffer objects (or their [u8] byte parameters)?
    // Can entire Layer Data objects be cached? Maybe via Enums like our PlotParams enums?
    static USE_MEMO_CACHE_VEC_F32: RefCell<Option<HashMap<Vec<String>, Arc<Vec<f32>>>>> = const { RefCell::new(None) };
    static USE_MEMO_CACHE_VEC_I32: RefCell<Option<HashMap<Vec<String>, Arc<Vec<i32>>>>> = const { RefCell::new(None) };
    static USE_MEMO_CACHE_INTERNAL_TEXT_LAYER_DATA: RefCell<Option<HashMap<Vec<String>, Arc<CachedInternalTextLayerData>>>> = const { RefCell::new(None) };
}

async fn init_gpu_context() -> (wgpu::Device, wgpu::Queue) {
    // Apparently this is expensive, so we try to cache it in the get_or_init_gpu_context function.
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    // We can try to enable WebGL fallback here, but it is not working,
    // even when we add wgpu as a direct dependency with the "webgl" feature enabled.
    // References:
    // - https://github.com/gfx-rs/wgpu/issues/6166#issuecomment-2327015218
    // - https://github.com/emilk/egui/blob/a9e92525c01e90417b431af9a4ea9db4d3dd6179/crates/egui-wgpu/src/setup.rs#L160
    /*
    let instance = wgpu::util::new_instance_with_webgpu_detection(
        &wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        },
    ).await;
    */

    // WebGL2 fallback requires specifying compatible_surface, but this would tie us closer to web stuff
    // which we probably don't want.
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

// TODO: Should we also implement a non-async variant of this cacheing/memoization function?
// Is there a downside to always using async, i.e., even if the `initializer` function never .awaits anything?
pub async fn use_memo_vec_f32(initializer: impl AsyncFnOnce() -> Vec<f32>, keys: &[String], cache_enabled: bool) -> Arc<Vec<f32>> {
    // Initializer param
    // Reference: https://github.com/DioxusLabs/dioxus/blob/ec8f31dece5c75371177bf080bab46dff54ffd0e/packages/core/src/global_context.rs#L284

    if !cache_enabled {
        return Arc::new(initializer().await);
    }

    // This thread_local approach seems to work fine with futures::join!.
    // First, check if the buffer already exists
    let buffer_exists = USE_MEMO_CACHE_VEC_F32.with(|map| {
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
    let buffer = Arc::new(initializer().await);

    // Store it in the cache
    USE_MEMO_CACHE_VEC_F32.with(|map| {
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

// TODO: is there a better way to define a generic use_memo function that works for multiple types (e.g., Vec<f32>, Vec<i32>, etc.)?
// We want to balance type safety with code duplication.
// I.e., we may want to avoid using Box<dyn Any> or similar approaches that lose type information,
// since we don't want the downstream calling code to be doing a bunch of type casting/checking.
// Maybe a macro could help here? Or enums, one enum per layer.data struct type?
pub async fn use_memo_vec_i32(initializer: impl AsyncFnOnce() -> Vec<i32>, keys: &[String], cache_enabled: bool) -> Arc<Vec<i32>> {
    // Initializer param
    // Reference: https://github.com/DioxusLabs/dioxus/blob/ec8f31dece5c75371177bf080bab46dff54ffd0e/packages/core/src/global_context.rs#L284

    if !cache_enabled {
        return Arc::new(initializer().await);
    }

    // This thread_local approach seems to work fine with futures::join!.
    // First, check if the buffer already exists
    let buffer_exists = USE_MEMO_CACHE_VEC_I32.with(|map| {
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
    let buffer = Arc::new(initializer().await);

    // Store it in the cache
    USE_MEMO_CACHE_VEC_I32.with(|map| {
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

pub async fn use_memo_internal_text_layer_data(
    initializer: impl AsyncFnOnce() -> CachedInternalTextLayerData,
    keys: &[String],
    cache_enabled: bool
) -> Arc<CachedInternalTextLayerData> {
    if !cache_enabled {
        return Arc::new(initializer().await);
    }

    // First, check if the data already exists in cache
    let data_exists = USE_MEMO_CACHE_INTERNAL_TEXT_LAYER_DATA.with(|map| {
        map.borrow()
            .as_ref()
            .and_then(|m| m.get(keys).cloned())
    });

    if let Some(data) = data_exists {
        return data;
    }

    // Data doesn't exist, so create it
    let data = Arc::new(initializer().await);

    // Store it in the cache
    USE_MEMO_CACHE_INTERNAL_TEXT_LAYER_DATA.with(|map| {
        let mut map_ref = map.borrow_mut();

        // Initialize the map if it doesn't exist
        if map_ref.is_none() {
            *map_ref = Some(HashMap::new());
        }

        // Insert the data
        map_ref.as_mut().unwrap().insert(keys.to_vec(), data.clone());
    });

    data
}

// TODO: Every render, try to clear things from the use_memo cache hash maps.
// See egui FrameCache approach: clear any variables that were not used in the previous frame
// (corresponding to the same plot ID and format (i.e., raster/vector)).
