use std::sync::Arc;
use serde::{Deserialize, Serialize};
use futures_time::future::FutureExt;
use futures_time::time::Duration;
use pluot_core::maybe_timeout;

use pluot_core::wgpu;
use pluot_core::zarr::AsyncZarritaStore;
use pluot_core::cache::{get_or_init_store, use_memo_vec_f32};
use pluot_core::zarr::is_timed_out_zarrs_error;
use pluot_core::two::svg::SvgContext;
use pluot_core::render_traits::{DrawToRasterGpu, DrawToRasterCpu, DrawToSvg, MarginParams, PickableLayer, PreparedAndDraw, PreparedLayer, UnitsMode, ViewParams};
use pluot_core::render_types::{CpuContext, CpuRenderPass, PrepareResult};
use pluot_core::render_types::GpuContext;
use pluot_core::layers::composite_layer::{base_draw_composite_layer, base_draw_composite_layer_svg};
use pluot_core::compute::reduce::{reduce_extent, reduce_histogram_with_known_extent};
use pluot_core::plot_layers::bar_plot_layer::{BarOrientation, BarPlotLayer, BarPlotLayerParams};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ZarrHistogramLayerParams {
    pub layer_id: String,
    pub bounds: Option<MarginParams>,
    pub orientation: BarOrientation,

    // Data keys
    pub store_name: Option<String>,
    pub data_key: String,

    /// Number of histogram bins (must be <= 256).
    pub num_bins: u32,

    // Whether to cache the full data array once loaded.
    // If false, will only cache the histogram result, throwing away the full array.
    // (E.g., will need to re-load the full array if num_bins changes).
    pub cache_data: bool,
}

pub struct ZarrHistogramLayer {
    view_params: ViewParams,
    layer_params: ZarrHistogramLayerParams,
    store: Arc<AsyncZarritaStore>,
    store_name: String,
    sub_layer_instances: Vec<Box<dyn PreparedAndDraw>>,
}

impl ZarrHistogramLayer {
    pub fn new(view_params: ViewParams, layer_params: ZarrHistogramLayerParams) -> Self {
        let store_name = match &layer_params.store_name {
            Some(layer_store_name) => layer_store_name.clone(),
            None => {
                match &view_params.store_name {
                    Some(view_store_name) => view_store_name.clone(),
                    None => panic!("store_name must be specified either in layer_params or view_params for Zarr-based layers."),
                }
            }
        };

        let store = get_or_init_store(&store_name, view_params.wait_for_store_gets);
        Self {
            view_params,
            layer_params,
            store,
            store_name,
            sub_layer_instances: Vec::new(),
        }
    }

    fn bin_labels(data_min: f32, data_max: f32, num_bins: u32) -> Vec<String> {
        let step = (data_max - data_min) / num_bins as f32;
        (0..num_bins)
            .map(|i| {
                let lo = data_min + step * i as f32;
                let hi = lo + step;
                format!("{lo:.2}\u{2013}{hi:.2}")
            })
            .collect()
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for ZarrHistogramLayer {
    async fn prepare(&mut self, gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        let store = self.store.clone();
        let num_bins = self.layer_params.num_bins;

        let hist_future_deps = vec![
            "histogram_result".to_string(),
            self.store_name.clone(),
            self.layer_params.layer_id.clone(),
            num_bins.to_string(),
            // TODO: data_min and data_max layer_params here?
        ];

        let quant_future_deps = vec!["histogram_input_arr".to_string(), self.store_name.clone(), self.layer_params.layer_id.clone(), self.layer_params.data_key.clone()];
        let extent_future_deps = vec!["histogram_input_extent".to_string(), self.store_name.clone(), self.layer_params.layer_id.clone(), self.layer_params.data_key.clone()];

        let hist_future = use_memo_vec_f32(async || {
            // Nested cacheing.
            let quant_arr = use_memo_vec_f32(async || {
                let array_path = &self.layer_params.data_key;
                let array = zarrs::array::Array::async_open(store.clone(), array_path).await.unwrap();
                let subset = array.subset_all();
                let arr_raw = array.async_retrieve_array_subset::<Vec<f64>>(&subset).await?;
                let arr_inner: Vec<f32> = arr_raw.iter().map(|&x| x as f32).collect();
                Ok(arr_inner)
            }, &quant_future_deps, self.layer_params.cache_data).await;

            if quant_arr.is_err() {
                return quant_arr;
            }

            let quant_arr_inner = quant_arr.unwrap();

            // Compute distribution of quant_arr.
            let extent = use_memo_vec_f32(async || {
                let (lo, hi) = reduce_extent(gpu_context, quant_arr_inner).await;
                Ok(vec![lo, hi])
            }, &extent_future_deps, self.view_params.cache_enabled).await;

            if extent.is_err() {
                return extent;
            }

            let extent_inner = extent.unwrap();

            let bin_counts = reduce_histogram_with_known_extent(
                gpu_context,
                quant_arr_inner,
                num_bins,
                extent_inner[0],
                extent_inner[1],
            )
            .await;
            let bin_counts_inner: Vec<f32> = bin_counts.iter().map(|&c| c as f32).collect();
            Ok::<Arc<Vec<f32>>, std::convert::Infallible>(Arc::new(bin_counts_inner))

            // TODO: need to return BOTH the bin_counts_inner AND the extent_inner arr values from the use_memo.
        }, &hist_future_deps, self.view_params.cache_enabled);

        let future_result = maybe_timeout!(hist_future, self.view_params.timeout).await;

        let (hist_arr) = match future_result {
            Ok(hist_result) => hist_result,
            Err(e) => {
                if is_timed_out_zarrs_error(&e) {
                    return PrepareResult { bailed_early: true };
                } else {
                    panic!("Zarrs error during ZarrBarLayer prepare: {:?}", e);
                }
            }
        };

        // TODO: somehow obtain both the hist_arr and extent_result here.

        let data_min = extent_result[0];
        let data_max = extent_result[1];

        let labels = Self::bin_labels(data_min, data_max, num_bins);

        let bar_layer = BarPlotLayer::new(
            self.view_params.clone(),
            BarPlotLayerParams {
                layer_id: format!("{}_bar_plot_sublayer", self.layer_params.layer_id),
                bounds: self.layer_params.bounds.clone(),
                data_unit_mode_for_identifier_dim: UnitsMode::Pixels,
                data_unit_mode_for_quantity_dim: UnitsMode::Data,
                orientation: self.layer_params.orientation.clone(),
                identifier: Arc::new(labels),
                quantity: hist_arr,
            },
        );

        self.sub_layer_instances = vec![Box::new(bar_layer)];

        for sub_layer in self.sub_layer_instances.iter_mut() {
            sub_layer.prepare(gpu_context).await;
        }

        PrepareResult { bailed_early: false }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for ZarrHistogramLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        base_draw_composite_layer(&self.sub_layer_instances, gpu_context, pass).await;
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for ZarrHistogramLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for ZarrHistogramLayer {
    async fn draw(&self, ctx: &mut SvgContext) {
        base_draw_composite_layer_svg(&self.sub_layer_instances, ctx).await
    }
}

impl PickableLayer for ZarrHistogramLayer {}
