use std::sync::Arc;
use serde::{Deserialize, Serialize};
use pluot_core::{maybe_timeout, FutureExt, Duration, log};

use pluot_core::wgpu;
use pluot_core::cache::{use_memo_numeric_data, use_memo_vec_f32};
use pluot_core::emphasis_mode::DEFAULT_BACKGROUND_COLOR;
use pluot_core::zarr::is_timed_out_zarrs_error;
use zarrs::storage::AsyncReadableStorageTraits;
use pluot_core::two::svg::SvgContext;
use pluot_core::render_traits::{BrushableLayer, ColorMode, DrawToRasterCpu, DrawToRasterGpu, DrawToSvg, MarginParams, PickableLayer, PreparedAndDraw, PreparedLayer, UnitsMode, ViewParams, resolve_store_name};
use pluot_core::render_types::{CpuContext, CpuRenderPass, PrepareResult};
use pluot_core::render_types::GpuContext;
use pluot_core::composite_layer::{base_draw_composite_layer, base_draw_composite_layer_svg};
use pluot_core::compute::reduce::{reduce_extent, reduce_histogram_with_known_extent};
use pluot_core::composite_layers::bar_plot_layer::{BarOrientation, BarPlotLayer, BarPlotLayerParams};

use crate::zarr_numeric_data::load_arr_as_numeric_data;
use crate::zarr_emphasis_criteria::{resolve_zarr_emphasis_criteria, ZarrEmphasisCriteria};


/// Layer params struct for [`ZarrHistogramLayer`].
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

    // Criteria AND-ed together to determine the selected ("foreground") /
    // filtered-in ("background") set of data items used when computing the histogram.
    pub selection_criteria: Vec<ZarrEmphasisCriteria>,
    pub filtering_criteria: Vec<ZarrEmphasisCriteria>,

    pub background_fill_color: Option<(u8, u8, u8)>,
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
            selection_criteria: vec![],
            filtering_criteria: vec![],
            background_fill_color: None,
        }
    }
}

pub struct ZarrHistogramLayer {
    view_params: ViewParams,
    layer_params: ZarrHistogramLayerParams,
    store: Arc<dyn AsyncReadableStorageTraits>,
    store_name: String,

    // TODO: switch to `inner: Option<BarPlotLayer>`?
    sub_layer_instances: Vec<Box<dyn PreparedAndDraw>>,
}

impl ZarrHistogramLayer {
    pub fn new(view_params: ViewParams, layer_params: ZarrHistogramLayerParams) -> Self {
        let store_name = resolve_store_name(&layer_params.store_name, &view_params);

        let store = view_params.get_store(&store_name);
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

        // Criteria are resolved (zarr arrays loaded) inside the memoized
        // closure below, but the histogram result depends on their *values*,
        // so a serialized snapshot must be part of the outer cache key.
        let filtering_criteria_key = serde_json::to_string(&self.layer_params.filtering_criteria).unwrap_or_default();
        let selection_criteria_key = serde_json::to_string(&self.layer_params.selection_criteria).unwrap_or_default();

        let hist_future_deps = vec![
            "histogram_result".to_string(),
            self.store_name.clone(),
            self.layer_params.layer_id.clone(),
            num_bins.to_string(),
            filtering_criteria_key.clone(),
            selection_criteria_key,
            // TODO: data_min and data_max layer_params here?
        ];

        let quant_future_deps = vec!["histogram_input_arr".to_string(), self.store_name.clone(), self.layer_params.layer_id.clone(), self.layer_params.data_key.clone()];
        let extent_future_deps = vec!["histogram_input_extent".to_string(), self.store_name.clone(), self.layer_params.layer_id.clone(), self.layer_params.data_key.clone(), filtering_criteria_key];
        let filtering_criteria_future_deps = vec!["histogram_filter_criteria".to_string(), self.store_name.clone(), self.layer_params.layer_id.clone()];
        let selection_criteria_future_deps = vec!["histogram_select_criteria".to_string(), self.store_name.clone(), self.layer_params.layer_id.clone()];

        // Returns [data_min, data_max, bg_bin_0, ..., bg_bin_{num_bins-1}, fg_bin_0, ..., fg_bin_{num_bins-1}]
        let hist_future = use_memo_vec_f32(async || {
            // Nested caching: cache the raw data array in its native dtype
            // (any dtype supported by NumericData). The reducers below consume
            // NumericData directly, so the values are never cast to a single
            // dtype on the way in.
            let quant_arr = use_memo_numeric_data(async || {
                load_arr_as_numeric_data(store.clone(), &self.layer_params.data_key).await
            }, &quant_future_deps, self.view_params.cache_enabled && self.layer_params.cache_data)
                .await?;

            // Resolve filtering/selection criteria: each criterion's `codes_key`/
            // `values_key` zarr array is loaded (and independently memoized) into
            // an `EmphasisCriteria`.
            let filtering_criteria = resolve_zarr_emphasis_criteria(
                store.clone(),
                &self.layer_params.filtering_criteria,
                &filtering_criteria_future_deps,
                self.view_params.cache_enabled,
            ).await?;
            let selection_criteria = resolve_zarr_emphasis_criteria(
                store.clone(),
                &self.layer_params.selection_criteria,
                &selection_criteria_future_deps,
                self.view_params.cache_enabled,
            ).await?;

            // Nested caching: cache the extent.
            // Cloning a `NumericData` clones the inner `Arc<Vec<T>>`, not the data.
            // The extent is derived from the filter-included ("background") set
            // alone, so the background and foreground histograms share bin
            // boundaries and stay comparable.
            let quant_arr_for_extent = quant_arr.as_ref().clone();
            let filtering_criteria_for_extent = filtering_criteria.clone();
            let extent = use_memo_vec_f32(async || {
                let (lo, hi) = reduce_extent(gpu_context, quant_arr_for_extent, &filtering_criteria_for_extent, &[]).await.background;
                Ok::<Vec<f32>, std::convert::Infallible>(vec![lo, hi])
            }, &extent_future_deps, self.view_params.cache_enabled)
                .await
                .expect("Extent computation failed in ZarrHistogramLayer.prepare");

            let bin_counts = reduce_histogram_with_known_extent(
                gpu_context,
                quant_arr.as_ref().clone(),
                num_bins,
                extent[0],
                extent[1],
                &filtering_criteria,
                &selection_criteria,
            ).await;

            let mut result = vec![extent[0], extent[1]];
            result.extend(bin_counts.background.iter().map(|&c| c as f32));
            result.extend(bin_counts.foreground.iter().map(|&c| c as f32));
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
        let bins_start = 2;
        let bins_end = bins_start + num_bins as usize;
        let background_arr: Arc<Vec<f32>> = Arc::new(hist_data[bins_start..bins_end].to_vec());
        let foreground_arr: Arc<Vec<f32>> = Arc::new(hist_data[bins_end..bins_end + num_bins as usize].to_vec());

        let labels = Arc::new(Self::bin_labels(data_min, data_max, num_bins));

        // Render the filter-included ("background") bars first, so the
        // filter-and-selection-included ("foreground") bars drawn afterward
        // appear in front of them.
        let background_bar_layer = BarPlotLayer::new(
            self.view_params.clone(),
            BarPlotLayerParams {
                layer_id: format!("{}_bar_plot_sublayer_background", self.layer_params.layer_id),
                bounds: self.layer_params.bounds.clone(),
                data_unit_mode_for_identifier_dim: UnitsMode::Pixels,
                data_unit_mode_for_quantity_dim: UnitsMode::Data,
                orientation: self.layer_params.orientation.clone(),
                identifier: labels.clone(),
                quantity: background_arr,
                fill_color: Some(ColorMode::UniformRgb(
                    self.layer_params.background_fill_color.unwrap_or(DEFAULT_BACKGROUND_COLOR),
                )),
            },
        );

        let foreground_bar_layer = BarPlotLayer::new(
            self.view_params.clone(),
            BarPlotLayerParams {
                layer_id: format!("{}_bar_plot_sublayer_foreground", self.layer_params.layer_id),
                bounds: self.layer_params.bounds.clone(),
                data_unit_mode_for_identifier_dim: UnitsMode::Pixels,
                data_unit_mode_for_quantity_dim: UnitsMode::Data,
                orientation: self.layer_params.orientation.clone(),
                identifier: labels,
                quantity: foreground_arr,
                fill_color: Some(ColorMode::UniformRgb(
                    self.layer_params.fill_color.unwrap_or((76, 120, 168)),
                )),
            },
        );

        self.sub_layer_instances = vec![Box::new(background_bar_layer), Box::new(foreground_bar_layer)];

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

impl BrushableLayer for ZarrHistogramLayer {
    // TODO: implement a brush function which expects a RangeX brush mode and returns the min/max values of the range according to the x-axis linear scale's domain.
}

impl PickableLayer for ZarrHistogramLayer {}
