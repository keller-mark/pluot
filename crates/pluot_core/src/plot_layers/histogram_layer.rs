// The histogram layer wraps a BarPlotLayer. It runs the histogram reducer in
// prepare() to convert raw f32 data into bin counts, then delegates rendering
// to a BarPlotLayer built from those counts.
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use crate::render_traits::{
    DrawToRasterGpu, DrawToRasterCpu, DrawToSvg, MarginParams, PickableLayer, PreparedAndDraw,
    PreparedLayer, UnitsMode, ViewParams,
};
use crate::render_types::{CpuContext, CpuRenderPass, PrepareResult};
use crate::render_types::GpuContext;
use crate::two::svg::SvgContext;
use crate::wgpu;
use crate::layers::composite_layer::{base_draw_composite_layer, base_draw_composite_layer_svg};
use crate::cache::use_memo_vec_f32;
use crate::compute::reduce::{reduce_extent, reduce_histogram_with_known_extent};

use super::bar_plot_layer::{BarOrientation, BarPlotLayer, BarPlotLayerParams};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HistogramLayerParams {
    pub layer_id: String,
    pub bounds: Option<MarginParams>,
    pub orientation: BarOrientation,
    /// The raw f32 values to be binned.
    pub data: Arc<Vec<f32>>,
    /// Number of histogram bins (must be <= 256).
    pub num_bins: u32,
    /// Optional pre-computed extent. When None, extent is derived from the data.
    pub data_min: Option<f32>,
    pub data_max: Option<f32>,
}

pub struct HistogramLayer {
    view_params: ViewParams,
    layer_params: HistogramLayerParams,
    sub_layer_instances: Vec<Box<dyn PreparedAndDraw>>,
}

impl HistogramLayer {
    pub fn new(view_params: ViewParams, layer_params: HistogramLayerParams) -> Self {
        Self {
            view_params,
            layer_params,
            sub_layer_instances: Vec::new(),
        }
    }

    /// Generate human-readable bin-edge labels, e.g. "0.00–10.00".
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
impl PreparedLayer for HistogramLayer {
    async fn prepare(&mut self, gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        let p = &self.layer_params;
        let num_bins = p.num_bins;

        // Resolve the data extent (use provided values or compute from data, cached).
        let (data_min, data_max) = match (p.data_min, p.data_max) {
            (Some(lo), Some(hi)) => (lo, hi),
            _ => {
                let extent_data = Arc::clone(&p.data);
                let extent_deps = vec![
                    "extent".to_string(),
                    p.layer_id.clone(),
                ];
                let extent = use_memo_vec_f32(async || {
                    let (lo, hi) = reduce_extent(gpu_context, extent_data).await;
                    Ok::<Vec<f32>, std::convert::Infallible>(vec![lo, hi])
                }, &extent_deps, self.view_params.cache_enabled)
                .await
                .unwrap();
                (extent[0], extent[1])
            }
        };

        // Compute histogram bin counts (cached via use_memo_vec_f32).
        let data = Arc::clone(&p.data);
        let cache_deps = vec![
            "histogram".to_string(),
            p.layer_id.clone(),
            num_bins.to_string(),
            data_min.to_string(),
            data_max.to_string(),
        ];
        let quantity = use_memo_vec_f32(async || {
            let bin_counts = reduce_histogram_with_known_extent(
                gpu_context,
                data,
                num_bins,
                data_min,
                data_max,
            )
            .await;
            let quantity: Vec<f32> = bin_counts.iter().map(|&c| c as f32).collect();
            Ok::<Vec<f32>, std::convert::Infallible>(quantity)
        }, &cache_deps, self.view_params.cache_enabled)
        .await
        .unwrap();

        let labels = Self::bin_labels(data_min, data_max, num_bins);

        // Build a BarPlotLayer from the computed histogram data.
        let bar_layer = BarPlotLayer::new(
            self.view_params.clone(),
            BarPlotLayerParams {
                layer_id: self.layer_params.layer_id.clone(),
                bounds: self.layer_params.bounds.clone(),
                data_unit_mode_for_identifier_dim: UnitsMode::Pixels,
                data_unit_mode_for_quantity_dim: UnitsMode::Data,
                orientation: self.layer_params.orientation.clone(),
                identifier: Arc::new(labels),
                quantity,
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
impl DrawToRasterGpu for HistogramLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        base_draw_composite_layer(&self.sub_layer_instances, gpu_context, pass).await;
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for HistogramLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for HistogramLayer {
    async fn draw(&self, ctx: &mut SvgContext) {
        base_draw_composite_layer_svg(&self.sub_layer_instances, ctx).await
    }
}

inventory::submit! {
    crate::registry::LayerRegistration {
        layer_type_name: "HistogramLayer",
        create_layer: |value, view_params| {
            let params: HistogramLayerParams = serde_json::from_value(value).unwrap();
            Box::new(HistogramLayer::new(view_params.clone(), params))
        },
    }
}

impl PickableLayer for HistogramLayer {}
