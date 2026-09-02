// The brushing analog of picking.rs
use std::collections::HashMap;
use serde::{Serialize, Deserialize};

use crate::{UnitsMode, wgpu};
use crate::wgpu::{Extent3d, TextureDescriptor, TextureFormat, TextureUsages};
use crate::render_types::GpuContext;
use crate::params::{GraphicsFormat, PlotParams, RenderParams, RenderBackend, ComputeBackend, BrushMode};
use crate::render_traits::{MarginParams, BrushableLayer, ViewParams, get_layers, draw_layers_to_vector, draw_layers_to_raster};
use crate::cache::get_or_init_gpu_context;
use crate::zarr::StoreMap;

use futures_intrusive::channel::shared::oneshot_channel;

use crate::viewport::{DataCoord, DataVertices, ScreenCoord, ScreenVertices, unproject};

/// Serializable representation of brushing results
/// from a single plotted layer.
#[derive(Serialize, Deserialize)]
pub struct LayerBrushingResult {
    pub layer_id: String,
    pub info: HashMap<String, String>, // Any top-level info about the brushed region.
    pub element_info: HashMap<String, Vec<String>>, // Per-element info about the brushed elements (e.g., index in data array, value, etc.)
    // TODO: include "snapped vertices" as part of the picking result if relevant.
}

/// Serializable representation of brushing results
/// from one or more plotted layers.
#[derive(Serialize, Deserialize)]
pub struct BrushingResult {
    pub layer_results: Vec<LayerBrushingResult>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct BrushParams {
    // The brushed polygon expressed in screen vertices, regardless of the unitsMode below.
    // Depending on the unitsModes below, we can always convert screen-space (pixels units) to normalized or data units,
    // given the RenderParams (width/height/camera_view).
    pub screen_vertices: ScreenVertices,
    pub brush_units_mode_x: UnitsMode,
    pub brush_units_mode_y: UnitsMode,
    pub brush_mode: BrushMode,
}

/// Identify the data point(s) within the brushed region (rect or polygon).
pub async fn brush(params: RenderParams, stores: Option<StoreMap>, brush_params: BrushParams) -> BrushingResult {
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

    // Collect all PrepareResult values and update bailed_early if any of them bailed early,
    // aggregating the prepare results from all layers.
    // TODO: use maybe_timeout! here? or only within individual prepare functions?
    let prepare_results = futures::future::join_all(prepare_futures).await;
    // let prepare_bailed_early = prepare_results.iter().any(|r| r.bailed_early);


    // Convert the brushed screen vertices to data ("world") coordinates, one-by-one.
    // If any vertex falls outside the plotted region (e.g., within the margins),
    // unproject returns None for that vertex, and we treat the whole conversion as
    // unavailable rather than silently dropping vertices and distorting the brushed shape.
    let data_vertices: Option<DataVertices> = brush_params.screen_vertices.iter()
        .map(|&screen_coord| unproject(&view_params, None, screen_coord))
        .collect();

    let layer_results: Vec<LayerBrushingResult> = layers.iter_mut()
        .filter_map(|layer| layer.brush(brush_params.clone(), data_vertices.clone()))
        .collect();

    return BrushingResult {
        layer_results,
    };
}
