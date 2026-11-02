use std::any::Any;
use serde::Serialize;

use crate::wgpu;
use crate::wgpu::{Extent3d, TextureDescriptor, TextureFormat, TextureUsages};
use crate::render_types::GpuContext;
use crate::params::{GraphicsFormat, PlotParams, RenderParams, RenderBackend, ComputeBackend};
use crate::render_traits::{MarginParams, PickableLayer, ViewParams, get_layers, draw_layers_to_vector, draw_layers_to_raster};
use crate::cache::get_or_init_gpu_context;
use crate::zarr::StoreMap;
use crate::maybe::{MaybeSend, MaybeSync};

use futures_intrusive::channel::shared::oneshot_channel;

use crate::viewport::{DataCoord, ScreenCoord, unproject};

/// Layer-specific details about a picked or brushed element.
///
/// Each layer defines its own struct, which is serialized as-is
/// (e.g., to be rendered as a tooltip on the frontend).
pub trait ResultInfo: erased_serde::Serialize + Any + MaybeSend + MaybeSync {}

impl<T: Serialize + Any + MaybeSend + MaybeSync> ResultInfo for T {}

erased_serde::serialize_trait_object!(ResultInfo);

/// Serializable representation of picking results
/// from a single plotted layer.
#[derive(Serialize)]
pub struct LayerPickingResult {
    pub layer_id: String,
    pub info: Box<dyn ResultInfo>,
    // TODO: include a "picking result type" as part of the picking result:
    // Whether the picked item was a data point, axis element, axis label,
    // axis category, axis quantity, legend element, legend label,
    // legend category, legend quantity, title element, etc.
}

impl LayerPickingResult {
    pub fn new(layer_id: impl Into<String>, info: impl ResultInfo) -> Self {
        Self { layer_id: layer_id.into(), info: Box::new(info) }
    }

    /// The info, if it was produced by a layer whose info type is `T`.
    pub fn info_as<T: ResultInfo>(&self) -> Option<&T> {
        (&*self.info as &dyn Any).downcast_ref()
    }

    pub fn into_info<T: ResultInfo>(self) -> Option<T> {
        let info: Box<dyn Any> = self.info;
        info.downcast().ok().map(|info| *info)
    }
}

/// Serializable representation of picking results
/// from one or more plotted layers.
#[derive(Serialize)]
pub struct PickingResult {
    pub data_coord: Option<DataCoord>,
    pub screen_coord: ScreenCoord,
    pub layer_results: Vec<LayerPickingResult>,
}

/// Identify the data point(s) at (or nearby) the given plotted coordinate.
pub async fn pick(params: RenderParams, stores: Option<StoreMap>, screen_coord: ScreenCoord) -> PickingResult {
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

    let data_coord = unproject(&view_params, None, screen_coord);

    let layer_results: Vec<LayerPickingResult> = layers.iter_mut()
        .filter_map(|layer| layer.pick(screen_coord, data_coord))
        .collect();

    return PickingResult {
        data_coord,
        screen_coord,
        layer_results,
    };
}
