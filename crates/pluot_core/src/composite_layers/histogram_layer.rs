// The histogram layer wraps a PrecomputedHistogramLayer. It runs the histogram
// reducer in prepare() to convert raw f32 data into bin counts, then delegates
// rendering to a PrecomputedHistogramLayer built from those counts.
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use crate::render_traits::{
    BrushableLayer, ExtentableLayer, DrawToRasterCpu, DrawToRasterGpu, DrawToSvg, MarginParams, PickableLayer, PreparedAndDraw, PreparedLayer, ViewParams
};
use crate::render_types::{CpuContext, CpuRenderPass, PrepareResult};
use crate::render_types::GpuContext;
use crate::two::svg::SvgContext;
use crate::wgpu;
use crate::composite_layer::{base_draw_composite_layer, base_draw_composite_layer_svg};
use crate::cache::use_memo_vec_f32;
use crate::compute::reduce::{reduce_extent, reduce_histogram_with_known_extent};

use super::bar_plot_layer::BarOrientation;
use super::precomputed_histogram_layer::{PrecomputedHistogramLayer, PrecomputedHistogramLayerParams};


/// Layer params struct for [`HistogramLayer`].
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

    pub fill_color: Option<(u8, u8, u8)>,
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
                    let (lo, hi) = reduce_extent(gpu_context, extent_data, &[], &[]).await.background;
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
                &[],
                &[],
            )
            .await
            .background;
            let quantity: Vec<f32> = bin_counts.iter().map(|&c| c as f32).collect();
            Ok::<Vec<f32>, std::convert::Infallible>(quantity)
        }, &cache_deps, self.view_params.cache_enabled)
        .await
        .unwrap();

        let max_count = quantity.iter().cloned().fold(0.0f32, f32::max);

        let histogram_layer = PrecomputedHistogramLayer::new(
            self.view_params.clone(),
            PrecomputedHistogramLayerParams {
                layer_id: format!("{}_precomputed_histogram_sublayer", self.layer_params.layer_id),
                bounds: self.layer_params.bounds.clone(),
                orientation: self.layer_params.orientation.clone(),
                bin_min: data_min,
                bin_max: data_max,
                num_bins,
                quantity_min: 0.0,
                quantity_max: max_count,
                quantity,
                fill_color: self.layer_params.fill_color,
                ..Default::default()
            },
        );

        self.sub_layer_instances = vec![Box::new(histogram_layer)];

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

impl BrushableLayer for HistogramLayer {}

impl ExtentableLayer for HistogramLayer {}

impl PickableLayer for HistogramLayer {}
