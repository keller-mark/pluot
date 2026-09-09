// The extent analog of brushing.rs / picking.rs.
use serde::{Serialize, Deserialize};

use crate::wgpu;
use crate::render_types::GpuContext;
use crate::params::{PlotParams, RenderParams, RenderBackend, ComputeBackend};
use crate::render_traits::{MarginParams, ViewParams, get_layers};
use crate::cache::get_or_init_gpu_context;
use crate::zarr::StoreMap;

/// Serializable representation of the data extent
/// reported by a single plotted layer.
#[derive(Serialize, Deserialize)]
pub struct LayerExtentResult {
    pub layer_id: String,
    pub x_min: f32,
    pub x_max: f32,
    pub y_min: f32,
    pub y_max: f32,
    // Only present for layers plotted in a 3D coordinate system.
    pub z_min: Option<f32>,
    pub z_max: Option<f32>,
}

/// Serializable representation of the data extents
/// reported by one or more plotted layers.
#[derive(Serialize, Deserialize)]
pub struct ExtentResult {
    pub layer_results: Vec<LayerExtentResult>,
}

/// Determine the data extent (min/max bounds along x, y, and z) of each layer.
pub async fn extent(params: RenderParams, stores: Option<StoreMap>) -> ExtentResult {
    // TODO: the stuff up to layer.prepare is duplicated from render(). Refactor to avoid duplication.
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
        stores: params.stores.clone(),
        // Thread the concrete store objects down so layer constructors read from
        // them directly instead of the global store registry.
        store_objects: stores,
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

    // The layer extent is derived from state populated during `prepare`, so
    // layers must be prepared before querying it, even though nothing else
    // about the returned extents depends on the prepare results themselves.
    let _prepare_results = futures::future::join_all(prepare_futures).await;

    let layer_results: Vec<LayerExtentResult> = layers.iter_mut()
        .filter_map(|layer| layer.extent())
        .collect();

    return ExtentResult {
        layer_results,
    };
}
