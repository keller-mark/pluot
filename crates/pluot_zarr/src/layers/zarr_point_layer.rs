use std::sync::Arc;
use pluot_core::layers::point_layer::PointLayer;
use pluot_core::viewport::DataCoord;
use pluot_core::viewport::ScreenCoord;
use serde::{Deserialize, Serialize};
use futures_time::future::FutureExt;
use futures_time::time::Duration;
use pluot_core::maybe_timeout;

use pluot_core::log;
use pluot_core::wgpu;
use pluot_core::zarr::AsyncZarritaStore;
use pluot_core::cache::{get_or_init_store, use_memo_vec_f32, use_memo_vec_i32};
use pluot_core::two::svg::{update_svg, SvgContext};
use pluot_core::render_traits::{DrawToRasterGpu, DrawToRasterCpu, DrawToSvg, PickableLayer, PreparedLayer, ViewParams, AspectRatioMode, UnitsMode, MarginParams};
use pluot_core::layers::point_layer::{PointShapeMode, PointLayerParams, base_draw_point_layer, base_draw_point_layer_svg};
use pluot_core::render_types::{CpuContext, CpuRenderPass, PrepareResult, RenderResult};
use pluot_core::render_types::GpuContext;
use pluot_core::LayerPickingResult;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ZarrPointLayerParams {
    pub layer_id: String,
    // If None, assume margin: 0 in all directions.
    pub bounds: Option<MarginParams>,
    pub data_unit_mode: UnitsMode,
    pub point_radius: f32,
    pub point_radius_unit_mode: UnitsMode,
    pub point_shape_mode: PointShapeMode,

    // Data keys
    pub store_name: Option<String>,
    pub x_key: String,
    pub y_key: String,
    pub color_key: Option<String>,
}

// TODO: defaults for params?

pub struct ZarrPointLayerData {
    pub x_arr: Arc<Vec<f32>>,
    pub y_arr: Arc<Vec<f32>>,
    pub labels_arr: Arc<Vec<i32>>,
}

pub struct ZarrPointLayer {
    view_params: ViewParams,
    layer_params: ZarrPointLayerParams,
    // TODO: do we want the store or just the store_name here?
    store: Arc<AsyncZarritaStore>,
    store_name: String,
    // Data will be None prior to runninng prepare().
    data: Option<ZarrPointLayerData>,

    ready_to_draw: bool,
}

impl ZarrPointLayer {
    pub fn new(
        view_params: ViewParams,
        layer_params: ZarrPointLayerParams,
    ) -> Self {
        // Error if point_radius_unit_mode is "data" when data_unit_mode is "pixels".
        if (layer_params.point_radius_unit_mode == UnitsMode::Data && layer_params.data_unit_mode == UnitsMode::Pixels) {
            panic!("point_radius_unit_mode cannot be 'data' when data_unit_mode is 'pixels'");
        }
        // If store_name is None, use the store name from view_params.
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
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for ZarrPointLayer {
    async fn prepare(&mut self, _gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        let store = self.store.clone();

        // TODO: include the layer type in the memoization dependencies?
        // But what if we want multiple layers to be able to reuse the same cached data?
        // Then we should also avoid including the layer_id...
        let l_i32_future_deps = vec!["l_bytes".to_string(), self.store_name.clone(), self.layer_params.layer_id.to_string()];
        let l_i32_future = use_memo_vec_i32(async || {
            let labels_array_path = &self.layer_params.color_key.as_ref().expect("Color key");
            let labels_array_future = zarrs::array::Array::async_open(store.clone(), labels_array_path);
            let labels_array = labels_array_future.await.unwrap();
            let labels_subset = labels_array.subset_all();
            //let labels_result = labels_array.async_retrieve_array_subset_ndarray::<i64>(&labels_subset).await;
            let labels_result = labels_array.async_retrieve_array_subset::<Vec<i64>>(&labels_subset).await;

            let labels_vec = labels_result.unwrap();
            // More efficient version that eliminates intermediate vectors and redundant operations
            // Convert to f32 and cast to bytes directly - no for loop needed
            let labels_i32: Vec<i32> = labels_vec.iter().map(|&c| c as i32).collect();
            labels_i32
        }, &l_i32_future_deps, self.view_params.cache_enabled);

        // TODO: improve the keys / memoization dependencies to at least include the plot_id and store_name.
        let x_f32_future_deps = vec!["x_bytes".to_string(), self.store_name.clone(), self.layer_params.layer_id.to_string()];
        let x_f32_future = use_memo_vec_f32(async || {
            let x_array_path = &self.layer_params.x_key.as_ref();
            let x_array_future = zarrs::array::Array::async_open(store.clone(), x_array_path);
            let x_array = x_array_future.await.unwrap();
            let x_subset = x_array.subset_all();
            //let x_result = x_array.async_retrieve_array_subset_ndarray::<f64>(&x_subset).await;
            let x_result = x_array.async_retrieve_array_subset::<Vec<f64>>(&x_subset).await;

            let position_x = x_result.unwrap();
            let x_f32_inner: Vec<f32> = position_x.iter().map(|&x| x as f32).collect();
            x_f32_inner
        }, &x_f32_future_deps, self.view_params.cache_enabled);

        let y_f32_future_deps = vec!["y_bytes".to_string(), self.store_name.clone(), self.layer_params.layer_id.to_string()];
        let y_f32_future = use_memo_vec_f32(async || {
            let y_array_path = &self.layer_params.y_key.as_ref();
            let y_array_future = zarrs::array::Array::async_open(store.clone(), y_array_path);
            let y_array = y_array_future.await.unwrap();
            let y_subset = y_array.subset_all();
            //let y_result = y_array.async_retrieve_array_subset_ndarray::<f64>(&y_subset).await;
            let y_result = y_array.async_retrieve_array_subset::<Vec<f64>>(&y_subset).await;

            let position_y = y_result.unwrap();
            let y_f32_inner: Vec<f32> = position_y.iter().map(|&y| y as f32).collect();
            y_f32_inner
        }, &y_f32_future_deps, self.view_params.cache_enabled);

        // Await in parallel: Use futures::join, similar to Promise.all in JS.
        //let (x_f32, y_f32, l_i32) = futures::join!(x_f32_future, y_f32_future, l_i32_future);

        let futures_try_join_result = futures::try_join!(
            maybe_timeout!(x_f32_future, self.view_params.timeout),
            maybe_timeout!(y_f32_future, self.view_params.timeout),
            maybe_timeout!(l_i32_future, self.view_params.timeout),
        );

        // TODO: load image data as vec of individual chunks (rather than requesting the full slice)
        // to allow for progressive rendering of large images as the chunks load.
        // We want to render the chunks that have loaded prior to the timeout (if there was a timeout specified).
        // First convert the requested slice to the chunk keys?

        let (x_f32, y_f32, l_i32) = match futures_try_join_result {
            Ok((x_f32_result, y_f32_result, l_i32_result)) => {
                // log("All futures succeeded within the timeout.");
                (x_f32_result, y_f32_result, l_i32_result)
            }
            Err(_) => {
                // TODO: still render something in this case
                // log("One or more futures timed out or failed");
                return PrepareResult { bailed_early: true };
            }
        };


        self.data = Some(ZarrPointLayerData {
            x_arr: x_f32,
            y_arr: y_f32,
            labels_arr: l_i32,
        });

        self.ready_to_draw = true;
        return PrepareResult {
            bailed_early: false,
        };
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for ZarrPointLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        if !self.ready_to_draw {
            log("ZarrPointLayer was not ready to draw. Skipping draw call.");
            return;
        }
        let data = self.data.as_ref().expect("Data was not prepared. Call prepare() first.");

        base_draw_point_layer(
            gpu_context, pass,
            &self.view_params,
            &PointLayerParams {
                layer_id: self.layer_params.layer_id.clone(),
                bounds: self.layer_params.bounds.clone(),
                data_unit_mode: self.layer_params.data_unit_mode.clone(),
                point_radius: self.layer_params.point_radius,
                point_radius_unit_mode: self.layer_params.point_radius_unit_mode.clone(),
                point_shape_mode: self.layer_params.point_shape_mode.clone(),
                position_x: data.x_arr.clone(),
                position_y: data.y_arr.clone(),
                labels_vec: data.labels_arr.clone(),
            },
        ).await;
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for ZarrPointLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}


#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for ZarrPointLayer {
    async fn draw(&self, ctx: &mut SvgContext) {
        if !self.ready_to_draw {
            log("ZarrPointLayer was not ready to draw. Skipping draw call.");
            return;
        }
        let data = self.data.as_ref().expect("Data was not prepared. Call prepare() first.");

        let svg_elements = base_draw_point_layer_svg(
            &self.view_params,
            &PointLayerParams {
                layer_id: self.layer_params.layer_id.clone(),
                bounds: self.layer_params.bounds.clone(),
                data_unit_mode: self.layer_params.data_unit_mode.clone(),
                point_radius: self.layer_params.point_radius,
                point_radius_unit_mode: self.layer_params.point_radius_unit_mode.clone(),
                point_shape_mode: self.layer_params.point_shape_mode.clone(),
                position_x: data.x_arr.clone(),
                position_y: data.y_arr.clone(),
                labels_vec: data.labels_arr.clone(),
            },
        );

        update_svg(ctx, &svg_elements);
    }
}

impl PickableLayer for ZarrPointLayer {
    fn pick(&self, _screen_coord: ScreenCoord, data_coord: Option<DataCoord>) -> Option<LayerPickingResult> {
        let DataCoord::TwoD { x: cx, y: cy } = data_coord? else {
            return None;
        };

        let data = self.data.as_ref().expect("Data was not prepared. Call prepare() first.");

        let layer = PointLayer::new(
            self.view_params.clone(),
            PointLayerParams {
                layer_id: self.layer_params.layer_id.clone(),
                bounds: self.layer_params.bounds.clone(),
                data_unit_mode: self.layer_params.data_unit_mode.clone(),
                point_radius: self.layer_params.point_radius,
                point_radius_unit_mode: self.layer_params.point_radius_unit_mode.clone(),
                point_shape_mode: self.layer_params.point_shape_mode.clone(),
                position_x: data.x_arr.clone(),
                position_y: data.y_arr.clone(),
                labels_vec: data.labels_arr.clone(),
            },
        );

        layer.pick(_screen_coord, data_coord)
    }
}
