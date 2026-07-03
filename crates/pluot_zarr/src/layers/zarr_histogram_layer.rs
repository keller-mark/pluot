use std::sync::Arc;
use serde::{Deserialize, Serialize};
use pluot_core::{maybe_timeout, FutureExt, Duration, log};

use pluot_core::wgpu;
use pluot_core::zarr::AsyncZarritaStore;
use pluot_core::cache::{get_or_init_store, use_memo_vec_f32};
use pluot_core::zarr::is_timed_out_zarrs_error;
use pluot_core::two::svg::SvgContext;
use pluot_core::render_traits::{ColorMode, DrawToRasterCpu, DrawToRasterGpu, DrawToSvg, MarginParams, PickableLayer, PreparedAndDraw, PreparedLayer, UnitsMode, ViewParams};
use pluot_core::render_types::{CpuContext, CpuRenderPass, PrepareResult};
use pluot_core::render_types::GpuContext;
use pluot_core::layers::composite_layer::{base_draw_composite_layer, base_draw_composite_layer_svg};
use pluot_core::compute::reduce::{reduce_extent, reduce_histogram_with_known_extent};
use pluot_core::plot_layers::bar_plot_layer::{BarOrientation, BarPlotLayer, BarPlotLayerParams};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
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

    pub fill_color: Option<(u8, u8, u8)>,
}

impl Default for ZarrHistogramLayerParams {
    fn default() -> Self {
        Self {
            layer_id: "".to_string(),
            bounds: None,
            orientation: BarOrientation::Vertical,
            store_name: None,
            data_key: "".to_string(),
            num_bins: 50,
            cache_data: true,
            fill_color: None,
        }
    }
}

pub struct ZarrHistogramLayer {
    view_params: ViewParams,
    layer_params: ZarrHistogramLayerParams,
    store: Arc<AsyncZarritaStore>,
    store_name: String,

    // TODO: switch to `inner: Option<BarPlotLayer>`?
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

        let store = get_or_init_store(&store_name, view_params.wait_for_store_gets, view_params.wait_for_store_pushes);
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

        // Returns [data_min, data_max, bin_count_0, ..., bin_count_{num_bins-1}]
        let hist_future = use_memo_vec_f32(async || {
            // Nested caching: cache the raw data array.
            let quant_arr = use_memo_vec_f32(async || {
                let array_path = &self.layer_params.data_key;
                let array = zarrs::array::Array::async_open(store.clone(), array_path).await.unwrap();
                let subset = array.subset_all();
                // TODO: generalize to support alternative dtypes
                let arr_raw = array.async_retrieve_array_subset::<Vec<f64>>(&subset).await?;
                let arr_inner: Vec<f32> = arr_raw.iter().map(|&x| x as f32).collect();
                Ok::<Vec<f32>, zarrs::array::ArrayError>(arr_inner)

            }, &quant_future_deps, self.view_params.cache_enabled && self.layer_params.cache_data)
                .await?;

            // Nested caching: cache the extent.
            let quant_arr_for_extent = quant_arr.clone();
            let extent = use_memo_vec_f32(async || {
                let (lo, hi) = reduce_extent(gpu_context, quant_arr_for_extent).await;
                Ok::<Vec<f32>, std::convert::Infallible>(vec![lo, hi])
            }, &extent_future_deps, self.view_params.cache_enabled)
                .await
                .expect("Extent computation failed in ZarrHistogramLayer.prepare");

            let bin_counts = reduce_histogram_with_known_extent(
                gpu_context,
                quant_arr,
                num_bins,
                extent[0],
                extent[1],
            ).await;

            let mut result = vec![extent[0], extent[1]];
            result.extend(bin_counts.iter().map(|&c| c as f32));
            Ok(result)
        }, &hist_future_deps, self.view_params.cache_enabled);

        let future_result = maybe_timeout!(hist_future, self.view_params.timeout).await;

        let hist_data = match future_result {
            Ok(Ok(hist_result)) => hist_result,
            Ok(Err(e)) => {
                // Zarrs error from async_retrieve_array_subset.
                if is_timed_out_zarrs_error(&e) {
                    return PrepareResult { bailed_early: true };
                } else {
                    panic!("Zarrs error during ZarrHistogramLayer prepare: {:?}", e);
                }
            }
            Err(e) => {
                log(&format!("Other error during ZarrHistogramLayer prepare: {:?}", e));
                // Wall-clock timeout from maybe_timeout!
                return PrepareResult { bailed_early: true };
            }
        };

        let data_min = hist_data[0];
        let data_max = hist_data[1];
        let hist_arr: Arc<Vec<f32>> = Arc::new(hist_data[2..].to_vec());

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
                fill_color_mode: ColorMode::Static,
                fill_color: match self.layer_params.fill_color {
                    Some(color) => Some(color),
                    None => Some((76, 120, 168)),
                },
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
