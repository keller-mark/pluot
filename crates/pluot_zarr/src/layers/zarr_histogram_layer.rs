use std::sync::Arc;
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use pluot_core::{maybe_timeout, FutureExt, Duration, log, BrushParams, LayerBrushingResult};

use pluot_core::wgpu;
use pluot_core::cache::use_memo_vec_f32;
use pluot_core::emphasis_mode::DEFAULT_BACKGROUND_COLOR;
use pluot_core::params::BrushMode;
use pluot_core::zarr::is_timed_out_zarrs_error;
use zarrs::storage::AsyncReadableStorageTraits;
use pluot_core::two::svg::SvgContext;
use pluot_core::render_traits::{BrushableLayer, ColorMode, DrawToRasterCpu, DrawToRasterGpu, DrawToSvg, MarginParams, PickableLayer, PreparedAndDraw, PreparedLayer, UnitsMode, ViewParams, resolve_store_name};
use pluot_core::render_types::{CpuContext, CpuRenderPass, PrepareResult};
use pluot_core::render_types::GpuContext;
use pluot_core::composite_layer::{base_draw_composite_layer, base_draw_composite_layer_svg};
use pluot_core::compute::reduce::{reduce_extent, reduce_histogram_with_known_extent};
use pluot_core::composite_layers::bar_plot_layer::{BarOrientation, BarPlotLayer, BarPlotLayerParams};
use pluot_core::composite_layers::axis_linear_layer::{AxisLinearLayer, AxisLinearLayerParams, AxisPosition};
use pluot_core::d3::scale::ScaleLinear;
use pluot_core::viewport::DataVertices;

use crate::zarr_numeric_data::load_arr_as_numeric_data_memoized;
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

    // The linear scale backing the rendered value axis, mapping the binned
    // value domain (data_min, data_max) to the axis's pixel range. Kept
    // around so that a brush selection along this axis can be resolved back
    // to a data value range.
    value_scale: Option<ScaleLinear>,
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
            value_scale: None,
        }
    }

    /// The [`AxisPosition`] at which the value axis (the axis spanning the
    /// binned value domain) is rendered, given the histogram's orientation.
    fn value_axis_position(orientation: &BarOrientation) -> AxisPosition {
        match orientation {
            BarOrientation::Vertical => AxisPosition::Bottom,
            BarOrientation::Horizontal => AxisPosition::Left,
        }
    }

    /// Build the linear scale mapping the binned value domain to the pixel
    /// range of the value axis, matching the range that [`AxisLinearLayer`]
    /// itself would compute for this position.
    fn build_value_scale(view_params: &ViewParams, orientation: &BarOrientation, domain: (f64, f64)) -> ScaleLinear {
        let margins = &view_params.margins;
        let margin_top = margins.as_ref().and_then(|m| m.margin_top).unwrap_or(0.0) as f64;
        let margin_right = margins.as_ref().and_then(|m| m.margin_right).unwrap_or(0.0) as f64;
        let margin_bottom = margins.as_ref().and_then(|m| m.margin_bottom).unwrap_or(0.0) as f64;
        let margin_left = margins.as_ref().and_then(|m| m.margin_left).unwrap_or(0.0) as f64;

        let viewport_w = view_params.width as f64;
        let viewport_h = view_params.height as f64;

        let mut scale = ScaleLinear::new();
        scale.set_domain(domain);
        match orientation {
            BarOrientation::Vertical => scale.set_range((margin_left, viewport_w - margin_right)),
            BarOrientation::Horizontal => scale.set_range((margin_bottom, viewport_h - margin_top)),
        }
        scale
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

    /// Unwraps one `maybe_timeout!`-wrapped memo result from `prepare`, returning
    /// `None` when the layer should bail early — either the wall-clock timeout
    /// elapsed (the outer `Err`) or the store timed out mid-fetch. Any other
    /// zarrs error is a genuine failure rather than a partial-render signal.
    fn unwrap_or_bail<T, E: std::fmt::Debug>(
        result: Result<Result<T, zarrs::array::ArrayError>, E>,
        label: &str,
    ) -> Option<T> {
        match result {
            Ok(Ok(value)) => Some(value),
            Ok(Err(e)) => {
                // Zarrs error from async_retrieve_array_subset.
                if is_timed_out_zarrs_error(&e) {
                    None
                } else {
                    panic!("Zarrs error during ZarrHistogramLayer prepare ({label}): {e:?}");
                }
            }
            Err(e) => {
                // Wall-clock timeout from maybe_timeout!
                log(&format!("Other error during ZarrHistogramLayer prepare ({label}): {e:?}"));
                None
            }
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for ZarrHistogramLayer {
    async fn prepare(&mut self, gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        let store = self.store.clone();
        let num_bins = self.layer_params.num_bins;

        // TODO: do not convert the vec to string using serde_json; extract the properties individually instead.
        let filtering_criteria_key = serde_json::to_string(&self.layer_params.filtering_criteria).unwrap_or_default();
        let selection_criteria_key = serde_json::to_string(&self.layer_params.selection_criteria).unwrap_or_default();

        // The extent and the "background" bin counts are derived from the
        // filter-included set alone, so their keys deliberately omit the
        // selection criteria.
        let background_future_deps = vec![
            "histogram_background".to_string(),
            self.store_name.clone(),
            self.layer_params.layer_id.clone(),
            self.layer_params.data_key.clone(),
            num_bins.to_string(),
            filtering_criteria_key.clone(),
        ];
        let foreground_future_deps = vec![
            "histogram_foreground".to_string(),
            self.store_name.clone(),
            self.layer_params.layer_id.clone(),
            self.layer_params.data_key.clone(),
            num_bins.to_string(),
            filtering_criteria_key.clone(),
            selection_criteria_key,
        ];

        let extent_future_deps = vec!["histogram_input_extent".to_string(), self.store_name.clone(), self.layer_params.data_key.clone(), filtering_criteria_key];

        let quant_cache_enabled = self.view_params.cache_enabled && self.layer_params.cache_data;

        // Returns [data_min, data_max, bg_bin_0, ..., bg_bin_{num_bins-1}]
        let background_future = use_memo_vec_f32(async || {
            // Nested caching: cache the raw data array in its native dtype
            // (any dtype supported by NumericData). The reducers below consume
            // NumericData directly, so the values are never cast to a single
            // dtype on the way in.
            let quant_arr = load_arr_as_numeric_data_memoized(
                store.clone(),
                &self.store_name,
                &self.layer_params.data_key,
                quant_cache_enabled,
            ).await?;

            // Resolve the filtering criteria: each criterion's `codes_key` /
            // `values_key` zarr array is loaded (and independently memoized,
            // keyed only by store name and array path) into an
            // `EmphasisCriteria`.
            let filtering_criteria = resolve_zarr_emphasis_criteria(
                store.clone(),
                &self.layer_params.filtering_criteria,
                &self.store_name,
                self.view_params.cache_enabled,
            ).await?;

            // Nested caching: cache the extent.
            // Cloning a `NumericData` clones the inner `Arc<Vec<T>>`, not the data.
            // The extent is derived from the filter-included ("background") set
            // alone, so the background and foreground histograms share bin boundaries.
            let quant_arr_for_extent = quant_arr.as_ref().clone();
            let filtering_criteria_for_extent = filtering_criteria.clone();

            let extent = use_memo_vec_f32(async || {
                let (lo, hi) = reduce_extent(gpu_context, quant_arr_for_extent, &filtering_criteria_for_extent, &[]).await.background;
                Ok::<Vec<f32>, std::convert::Infallible>(vec![lo, hi])
            }, &extent_future_deps, self.view_params.cache_enabled)
                .await
                .expect("Extent computation failed in ZarrHistogramLayer.prepare");

            // Passing no selection criteria makes the reducer's foreground pass
            // an alias of its background pass (a single GPU dispatch either way,
            // and a clone rather than a second scan on the CPU path). The real
            // foreground counts come from `foreground_future` below.
            let bin_counts = reduce_histogram_with_known_extent(
                gpu_context,
                quant_arr.as_ref().clone(),
                num_bins,
                extent[0],
                extent[1],
                &filtering_criteria,
                &[],
            ).await;

            let mut result = vec![extent[0], extent[1]];
            result.extend(bin_counts.background.iter().map(|&c| c as f32));
            Ok(result)
        }, &background_future_deps, self.view_params.cache_enabled);

        let background_data = match Self::unwrap_or_bail(
            maybe_timeout!(background_future, self.view_params.timeout).await,
            "background histogram",
        ) {
            Some(data) => data,
            None => return PrepareResult { bailed_early: true },
        };

        let data_min = background_data[0];
        let data_max = background_data[1];
        let background_arr: Arc<Vec<f32>> = Arc::new(background_data[2..].to_vec());

        let value_scale = Self::build_value_scale(&self.view_params, &self.layer_params.orientation, (data_min as f64, data_max as f64));
        self.value_scale = Some(value_scale);

        // The foreground ("selected") bin counts get their own memo.
        let foreground_future = use_memo_vec_f32(async || {
            let quant_arr = load_arr_as_numeric_data_memoized(
                store.clone(),
                &self.store_name,
                &self.layer_params.data_key,
                quant_cache_enabled,
            ).await?;

            // Both lists are needed here: the foreground is the filter-included
            // *and* selection-included subset. The filtering arrays were already
            // loaded by the background memo, and the selection arrays by the
            // previous brush position, so on a brush move these are cache hits
            // even though the thresholds have changed.
            let filtering_criteria = resolve_zarr_emphasis_criteria(
                store.clone(),
                &self.layer_params.filtering_criteria,
                &self.store_name,
                self.view_params.cache_enabled,
            ).await?;

            let selection_criteria = resolve_zarr_emphasis_criteria(
                store.clone(),
                &self.layer_params.selection_criteria,
                &self.store_name,
                self.view_params.cache_enabled,
            ).await?;

            let bin_counts = reduce_histogram_with_known_extent(
                gpu_context,
                quant_arr.as_ref().clone(),
                num_bins,
                data_min,
                data_max,
                &filtering_criteria,
                &selection_criteria,
            ).await;

            Ok(bin_counts.foreground.iter().map(|&c| c as f32).collect())
        }, &foreground_future_deps, self.view_params.cache_enabled);

        let foreground_arr = Self::unwrap_or_bail(
            maybe_timeout!(foreground_future, self.view_params.timeout).await,
            "foreground histogram",
        );

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
                // The value axis is rendered by ZarrHistogramLayer itself (see below),
                // using the real continuous value domain rather than per-bin labels.
                render_categorical_axis: Some(false),
            },
        );

        let foreground_bar_layer = foreground_arr.map(|foreground_arr| {
            BarPlotLayer::new(
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
                    render_categorical_axis: Some(false),
                },
            )
        });
        let bailed_early = foreground_bar_layer.is_none();

        let value_axis_layer = AxisLinearLayer::new(
            self.view_params.clone(),
            AxisLinearLayerParams {
                layer_id: format!("{}_value_axis_sublayer", self.layer_params.layer_id),
                position: Self::value_axis_position(&self.layer_params.orientation),
                domain: Some((data_min as f64, data_max as f64)),
                ..Default::default()
            },
        );

        let mut sub_layer_instances: Vec<Box<dyn PreparedAndDraw>> = vec![Box::new(background_bar_layer)];
        if let Some(foreground_bar_layer) = foreground_bar_layer {
            sub_layer_instances.push(Box::new(foreground_bar_layer));
        }
        sub_layer_instances.push(Box::new(value_axis_layer));
        self.sub_layer_instances = sub_layer_instances;

        for sub_layer in self.sub_layer_instances.iter_mut() {
            sub_layer.prepare(gpu_context).await;
        }

        PrepareResult { bailed_early }
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
    fn brush(&self, brush_params: BrushParams, _data_vertices: Option<DataVertices>) -> Option<LayerBrushingResult> {
        // The value axis runs along X for a vertical histogram, and along Y
        // for a horizontal one, so only a brush of the matching mode applies.
        let expected_brush_mode = match self.layer_params.orientation {
            BarOrientation::Vertical => BrushMode::RangeX,
            BarOrientation::Horizontal => BrushMode::RangeY,
        };
        if brush_params.brush_mode != expected_brush_mode {
            return None;
        }

        let value_scale = self.value_scale.as_ref()?;

        let screen_positions: Vec<f32> = match self.layer_params.orientation {
            BarOrientation::Vertical => brush_params.screen_vertices.iter().map(|v| v.x).collect(),
            BarOrientation::Horizontal => brush_params.screen_vertices.iter().map(|v| v.y).collect(),
        };

        let px_min = screen_positions.iter().cloned().fold(f32::INFINITY, f32::min);
        let px_max = screen_positions.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        if !px_min.is_finite() || !px_max.is_finite() {
            return None;
        }

        // The scale's range may be reversed relative to the raw screen
        // coordinates, so invert both ends and re-sort rather than assume order.
        let value_a = value_scale.invert(px_min as f64);
        let value_b = value_scale.invert(px_max as f64);
        let (value_min, value_max) = if value_a <= value_b { (value_a, value_b) } else { (value_b, value_a) };

        let mut info = HashMap::new();
        info.insert("min".to_string(), value_min.to_string());
        info.insert("max".to_string(), value_max.to_string());

        Some(LayerBrushingResult {
            layer_id: self.layer_params.layer_id.clone(),
            info,
            element_info: HashMap::new(),
        })
    }
}

impl PickableLayer for ZarrHistogramLayer {}
