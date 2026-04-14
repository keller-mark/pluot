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
use pluot_core::render_traits::{DrawToRasterGpu, DrawToRasterCpu, DrawToSvg, PickableLayer, PreparedLayer, ViewParams, UnitsMode, MarginParams};
use pluot_core::layers::rect_layer::{RectLayerParams, base_draw_rect_layer, base_draw_rect_layer_svg};
use pluot_core::render_types::{CpuContext, CpuRenderPass, PrepareResult};
use pluot_core::render_types::GpuContext;
use pluot_core::d3::scale::{ScaleBand, Scaleable};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum BarOrientation {
    Vertical,
    Horizontal,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ZarrBarLayerParams {
    pub layer_id: String,
    pub bounds: Option<MarginParams>,

    pub stroke_width: f32,

    pub orientation: BarOrientation,

    // Data keys
    pub store_name: Option<String>,
    pub categorical_key: String,
    pub quantitative_key: String,

    // TODO: color key?
}

pub struct ZarrBarLayerData {
    pub categorical_arr: Arc<Vec<String>>,
    pub quantitative_arr: Arc<Vec<f32>>,
}

pub struct ZarrBarLayer {
    view_params: ViewParams,
    layer_params: ZarrBarLayerParams,
    store: Arc<AsyncZarritaStore>,
    store_name: String,
    data: Option<ZarrBarLayerData>,
    ready_to_draw: bool,
}

impl ZarrBarLayer {
    pub fn new(view_params: ViewParams, layer_params: ZarrBarLayerParams) -> Self {
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
            data: None,
            ready_to_draw: false,
        }
    }

    fn build_rect_params(&self) -> RectLayerParams {
        let data = self.data.as_ref().expect("Data was not prepared. Call prepare() first.");

        let bounds = &self.view_params.margins;
        let margin_top = bounds.as_ref().and_then(|m| m.margin_top).unwrap_or(0.0) as f64;
        let margin_right = bounds.as_ref().and_then(|m| m.margin_right).unwrap_or(0.0) as f64;
        let margin_bottom = bounds.as_ref().and_then(|m| m.margin_bottom).unwrap_or(0.0) as f64;
        let margin_left = bounds.as_ref().and_then(|m| m.margin_left).unwrap_or(0.0) as f64;

        let viewport_w = self.view_params.width as f64;
        let viewport_h = self.view_params.height as f64;

        let n = data.categorical_arr.len();
        let mut position_x0: Vec<f32> = Vec::with_capacity(n);
        let mut position_y0: Vec<f32> = Vec::with_capacity(n);
        let mut position_x1: Vec<f32> = Vec::with_capacity(n);
        let mut position_y1: Vec<f32> = Vec::with_capacity(n);
        let labels_vec: Vec<i32> = (0..n as i32).collect();

        let mut scale_band = ScaleBand::new();
        scale_band.set_domain(data.categorical_arr.as_ref().clone());

        match self.layer_params.orientation {
            BarOrientation::Vertical => {
                // Categorical on X (pixels), quantitative on Y (data units)
                scale_band.set_range((margin_left, viewport_w - margin_right));
                let bandwidth = scale_band.bandwidth();

                for i in 0..n {
                    let band_start = scale_band.scale(&data.categorical_arr[i]) as f32;
                    position_x0.push(band_start);
                    position_x1.push(band_start + bandwidth as f32);
                    position_y0.push(0.0);
                    position_y1.push(data.quantitative_arr[i]);
                }

                RectLayerParams {
                    layer_id: self.layer_params.layer_id.clone(),
                    bounds: self.layer_params.bounds.clone(),
                    data_unit_mode_x: UnitsMode::Pixels,
                    data_unit_mode_y: UnitsMode::Data,
                    stroke_width: self.layer_params.stroke_width,
                    stroke_width_unit_mode: UnitsMode::Pixels,
                    position_x0: Arc::new(position_x0),
                    position_y0: Arc::new(position_y0),
                    position_x1: Arc::new(position_x1),
                    position_y1: Arc::new(position_y1),
                    labels_vec: Arc::new(labels_vec),
                }
            }
            BarOrientation::Horizontal => {
                // Categorical on Y (pixels), quantitative on X (data units)
                scale_band.set_range((margin_bottom, viewport_h - margin_top));
                let bandwidth = scale_band.bandwidth();

                for i in 0..n {
                    let band_start = scale_band.scale(&data.categorical_arr[i]) as f32;
                    position_x0.push(0.0);
                    position_x1.push(data.quantitative_arr[i]);
                    position_y0.push(band_start);
                    position_y1.push(band_start + bandwidth as f32);
                }

                RectLayerParams {
                    layer_id: self.layer_params.layer_id.clone(),
                    bounds: self.layer_params.bounds.clone(),
                    data_unit_mode_x: UnitsMode::Data,
                    data_unit_mode_y: UnitsMode::Pixels,
                    stroke_width: self.layer_params.stroke_width,
                    stroke_width_unit_mode: UnitsMode::Pixels,
                    position_x0: Arc::new(position_x0),
                    position_y0: Arc::new(position_y0),
                    position_x1: Arc::new(position_x1),
                    position_y1: Arc::new(position_y1),
                    labels_vec: Arc::new(labels_vec),
                }
            }
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for ZarrBarLayer {
    async fn prepare(&mut self, _gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        let store = self.store.clone();

        let cat_future_deps = vec!["cat_bytes".to_string(), self.store_name.clone(), self.layer_params.layer_id.to_string()];
        let cat_future = use_memo_vec_string(async || {
            let array_path = &self.layer_params.categorical_key;
            let array = zarrs::array::Array::async_open(store.clone(), array_path).await.unwrap();
            let subset = array.subset_all();
            let values = array.async_retrieve_array_subset::<Vec<String>>(&subset).await?;
            Ok(values)
        }, &cat_future_deps, self.view_params.cache_enabled);

        let store2 = self.store.clone();
        let quant_future_deps = vec!["quant_bytes".to_string(), self.store_name.clone(), self.layer_params.layer_id.to_string()];
        let quant_future = use_memo_vec_f32(async || {
            let array_path = &self.layer_params.quantitative_key;
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

        self.data = Some(ZarrBarLayerData {
            categorical_arr: cat_arr,
            quantitative_arr: quant_arr,
        });

        self.ready_to_draw = true;
        return PrepareResult {
            bailed_early: false,
        };
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for ZarrBarLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        if !self.ready_to_draw {
            log("ZarrBarLayer was not ready to draw. Skipping draw call.");
            return;
        }

        let rect_params = self.build_rect_params();
        base_draw_rect_layer(gpu_context, pass, &self.view_params, &rect_params).await;
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for ZarrBarLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for ZarrBarLayer {
    async fn draw(&self, ctx: &mut SvgContext) {
        if !self.ready_to_draw {
            log("ZarrBarLayer was not ready to draw. Skipping draw call.");
            return;
        }

        let rect_params = self.build_rect_params();
        let svg_elements = base_draw_rect_layer_svg(&self.view_params, &rect_params);
        update_svg(ctx, &svg_elements);
    }
}

impl PickableLayer for ZarrBarLayer {}
