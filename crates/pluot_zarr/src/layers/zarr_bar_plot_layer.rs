use std::sync::Arc;
use serde::{Deserialize, Serialize};
use futures_time::future::FutureExt;
use futures_time::time::Duration;
use pluot_core::maybe_timeout;

use pluot_core::log;
use pluot_core::wgpu;
use pluot_core::zarr::AsyncZarritaStore;
use pluot_core::cache::{get_or_init_store, use_memo_vec_f32, use_memo_vec_string};
use pluot_core::zarr::is_timed_out_zarrs_error;
use pluot_core::two::svg::{update_svg, SvgContext};
use pluot_core::render_traits::{DrawToRasterGpu, DrawToRasterCpu, DrawToSvg, PickableLayer, PreparedLayer, PreparedAndDraw, ViewParams, UnitsMode, MarginParams};
use pluot_core::layers::rect_layer::{RectLayerParams, base_draw_rect_layer, base_draw_rect_layer_svg};
use pluot_core::render_types::{CpuContext, CpuRenderPass, PrepareResult};
use pluot_core::render_types::GpuContext;
use pluot_core::d3::scale::{ScaleBand, Scaleable};
use pluot_core::plot_layers::bar_plot_layer::{BarOrientation, BarPlotLayer, BarPlotLayerParams};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ZarrBarPlotLayerParams {
    pub layer_id: String,
    pub bounds: Option<MarginParams>,
    pub orientation: BarOrientation,

    // Data keys
    pub store_name: Option<String>,
    pub identifier_key: String,
    pub quantity_key: String,

    // TODO: see TODOs in bar_plot_layer.rs
}

pub struct ZarrBarPlotLayer {
    view_params: ViewParams,
    layer_params: ZarrBarPlotLayerParams,
    store: Arc<AsyncZarritaStore>,
    store_name: String,

    /// The inner BarPlotLayer, constructed during `prepare()`.
    inner: Option<BarPlotLayer>,
}

impl ZarrBarPlotLayer {
    pub fn new(view_params: ViewParams, layer_params: ZarrBarPlotLayerParams) -> Self {
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
            inner: None,
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for ZarrBarPlotLayer {
    async fn prepare(&mut self, gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        let store = self.store.clone();

        let cat_future_deps = vec!["cat_bytes".to_string(), self.store_name.clone(), self.layer_params.layer_id.to_string()];
        let cat_future = use_memo_vec_string(async || {
            let array_path = &self.layer_params.identifier_key;
            let array = zarrs::array::Array::async_open(store.clone(), array_path).await.unwrap();
            let subset = array.subset_all();
            let values = array.async_retrieve_array_subset::<Vec<String>>(&subset).await?;
            Ok(values)
        }, &cat_future_deps, self.view_params.cache_enabled);

        let store2 = self.store.clone();
        let quant_future_deps = vec!["quant_bytes".to_string(), self.store_name.clone(), self.layer_params.layer_id.to_string()];
        let quant_future = use_memo_vec_f32(async || {
            let array_path = &self.layer_params.quantity_key;
            let array = zarrs::array::Array::async_open(store2.clone(), array_path).await.unwrap();
            let subset = array.subset_all();
            let values = array.async_retrieve_array_subset::<Vec<i64>>(&subset).await?;
            let f32_values: Vec<f32> = values.iter().map(|&v| v as f32).collect();
            Ok(f32_values)
        }, &quant_future_deps, self.view_params.cache_enabled);

        let futures_try_join_result = futures::try_join!(
            maybe_timeout!(cat_future, self.view_params.timeout),
            maybe_timeout!(quant_future, self.view_params.timeout),
        );

        let (cat_arr, quant_arr) = match futures_try_join_result {
            Ok((cat_result, quant_result)) => {
                match (cat_result, quant_result) {
                    (Ok(c), Ok(q)) => (c, q),
                    (Err(e), _) | (_, Err(e)) => {
                        if is_timed_out_zarrs_error(&e) {
                            return PrepareResult { bailed_early: true };
                        } else {
                            panic!("Zarrs error during ZarrBarLayer prepare: {:?}", e);
                        }
                    }
                }
            }
            Err(_) => {
                return PrepareResult { bailed_early: true };
            }
        };

        let mut sublayer = BarPlotLayer::new(
            self.view_params.clone(),
            BarPlotLayerParams {
                layer_id: format!("{}_bar_plot_sublayer", self.layer_params.layer_id),
                bounds: self.layer_params.bounds.clone(),
                orientation: self.layer_params.orientation.clone(),
                data_unit_mode_for_identifier_dim: UnitsMode::Pixels,
                data_unit_mode_for_quantity_dim: UnitsMode::Data,
                identifier: cat_arr,
                quantity: quant_arr,
            }
        );
        sublayer.prepare(gpu_context).await;
        self.inner = Some(sublayer);

        return PrepareResult {
            bailed_early: false,
        };
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for ZarrBarPlotLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        if let Some(inner) = &self.inner {
            DrawToRasterGpu::draw(inner, gpu_context, pass).await;
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for ZarrBarPlotLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for ZarrBarPlotLayer {
    async fn draw(&self, ctx: &mut SvgContext) {
        if let Some(inner) = &self.inner {
            DrawToSvg::draw(inner, ctx).await
        }
    }
}

impl PickableLayer for ZarrBarPlotLayer {}
