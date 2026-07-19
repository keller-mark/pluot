use std::sync::Arc;
use serde::{Deserialize, Serialize};

use pluot_core::{maybe_timeout, FutureExt, Duration};
use pluot_core::viewport::{DataCoord, ScreenCoord};

use pluot_core::log;
use pluot_core::numeric_data::NumericData;
use pluot_core::wgpu;
use pluot_core::zarr::AsyncZarritaStore;
use pluot_core::cache::{get_or_init_store, use_memo_vec_f32, use_memo_vec_i32};
use pluot_core::zarr::is_timed_out_zarrs_error;
use pluot_core::two::svg::SvgContext;
use pluot_core::render_traits::{CategoricalColormap, CategoricalParams, ColorMode, DrawToRasterGpu, DrawToRasterCpu, DrawToSvg, PickableLayer, PreparedLayer, ViewParams, MarginParams};
use pluot_core::layers::point_layer::PointShapeMode;
use pluot_core::layers::point_3d_layer::{Point3dLayer, Point3dLayerParams};
use pluot_core::render_types::{CpuContext, CpuRenderPass, PrepareResult};
use pluot_core::render_types::GpuContext;
use pluot_core::LayerPickingResult;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct ZarrPoint3dLayerParams {
    pub layer_id: String,
    pub bounds: Option<MarginParams>,
    pub point_radius: f32,
    pub point_shape_mode: PointShapeMode,

    // Data keys
    pub store_name: Option<String>,
    pub x_key: String,
    pub y_key: String,
    pub z_key: String,
    pub color_key: Option<String>,
}

impl Default for ZarrPoint3dLayerParams {
    fn default() -> Self {
        Self {
            layer_id: "".to_string(),
            bounds: None,
            point_radius: 1.0,
            point_shape_mode: PointShapeMode::Circle,
            store_name: None,
            x_key: "".to_string(),
            y_key: "".to_string(),
            z_key: "".to_string(),
            color_key: None,
        }
    }
}

pub struct ZarrPoint3dLayerData {
    pub x_arr: Arc<Vec<f32>>,
    pub y_arr: Arc<Vec<f32>>,
    pub z_arr: Arc<Vec<f32>>,
    pub labels_arr: Arc<Vec<i32>>,
}

pub struct ZarrPoint3dLayer {
    view_params: ViewParams,
    layer_params: ZarrPoint3dLayerParams,
    store: Arc<AsyncZarritaStore>,
    store_name: String,

    inner: Option<Point3dLayer>,
}

impl ZarrPoint3dLayer {
    pub fn new(
        view_params: ViewParams,
        layer_params: ZarrPoint3dLayerParams,
    ) -> Self {
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
impl PreparedLayer for ZarrPoint3dLayer {
    async fn prepare(&mut self, gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        let store = self.store.clone();

        let l_i32_future_deps = vec!["l_bytes".to_string(), self.store_name.clone(), self.layer_params.layer_id.to_string()];
        let l_i32_future = use_memo_vec_i32(async || {
            let labels_array_path = &self.layer_params.color_key.as_ref().expect("Color key");
            let labels_array_future = zarrs::array::Array::async_open(store.clone(), labels_array_path);
            let labels_array = labels_array_future.await.unwrap();
            let labels_subset = labels_array.subset_all();
            let labels_vec = labels_array.async_retrieve_array_subset::<Vec<i64>>(&labels_subset).await?;
            let labels_i32: Vec<i32> = labels_vec.iter().map(|&c| c as i32).collect();
            Ok(labels_i32)
        }, &l_i32_future_deps, self.view_params.cache_enabled);

        let x_f32_future_deps = vec!["x_bytes".to_string(), self.store_name.clone(), self.layer_params.layer_id.to_string()];
        let x_f32_future = use_memo_vec_f32(async || {
            let x_array_path = &self.layer_params.x_key.as_ref();
            let x_array_future = zarrs::array::Array::async_open(store.clone(), x_array_path);
            let x_array = x_array_future.await.unwrap();
            let x_subset = x_array.subset_all();
            let position_x = x_array.async_retrieve_array_subset::<Vec<f64>>(&x_subset).await?;
            let x_f32_inner: Vec<f32> = position_x.iter().map(|&x| x as f32).collect();
            Ok(x_f32_inner)
        }, &x_f32_future_deps, self.view_params.cache_enabled);

        let y_f32_future_deps = vec!["y_bytes".to_string(), self.store_name.clone(), self.layer_params.layer_id.to_string()];
        let y_f32_future = use_memo_vec_f32(async || {
            let y_array_path = &self.layer_params.y_key.as_ref();
            let y_array_future = zarrs::array::Array::async_open(store.clone(), y_array_path);
            let y_array = y_array_future.await.unwrap();
            let y_subset = y_array.subset_all();
            let position_y = y_array.async_retrieve_array_subset::<Vec<f64>>(&y_subset).await?;
            let y_f32_inner: Vec<f32> = position_y.iter().map(|&y| y as f32).collect();
            Ok(y_f32_inner)
        }, &y_f32_future_deps, self.view_params.cache_enabled);

        let z_f32_future_deps = vec!["z_bytes".to_string(), self.store_name.clone(), self.layer_params.layer_id.to_string()];
        let z_f32_future = use_memo_vec_f32(async || {
            let z_array_path = &self.layer_params.z_key.as_ref();
            let z_array_future = zarrs::array::Array::async_open(store.clone(), z_array_path);
            let z_array = z_array_future.await.unwrap();
            let z_subset = z_array.subset_all();
            let position_z = z_array.async_retrieve_array_subset::<Vec<f64>>(&z_subset).await?;
            let z_f32_inner: Vec<f32> = position_z.iter().map(|&z| z as f32).collect();
            Ok(z_f32_inner)
        }, &z_f32_future_deps, self.view_params.cache_enabled);

        let futures_try_join_result = futures::try_join!(
            maybe_timeout!(x_f32_future, self.view_params.timeout),
            maybe_timeout!(y_f32_future, self.view_params.timeout),
            maybe_timeout!(z_f32_future, self.view_params.timeout),
            maybe_timeout!(l_i32_future, self.view_params.timeout),
        );

        let (x_f32, y_f32, z_f32, l_i32) = match futures_try_join_result {
            Ok((x_f32_result, y_f32_result, z_f32_result, l_i32_result)) => {
                match (x_f32_result, y_f32_result, z_f32_result, l_i32_result) {
                    (Ok(x), Ok(y), Ok(z), Ok(l)) => (x, y, z, l),
                    (Err(e), _, _, _) | (_, Err(e), _, _) | (_, _, Err(e), _) | (_, _, _, Err(e)) => {
                        if is_timed_out_zarrs_error(&e) {
                            return PrepareResult { bailed_early: true };
                        } else {
                            panic!("Zarrs error during ZarrPoint3dLayer prepare: {:?}", e);
                        }
                    }
                }
            }
            Err(_) => {
                return PrepareResult { bailed_early: true };
            }
        };

        let mut sublayer = Point3dLayer::new(
            self.view_params.clone(),
            Point3dLayerParams {
                layer_id: self.layer_params.layer_id.clone(),
                bounds: self.layer_params.bounds.clone(),
                point_radius: self.layer_params.point_radius,
                point_shape_mode: self.layer_params.point_shape_mode,
                fill_color: Some(ColorMode::Categorical(CategoricalParams {
                    codes: NumericData::Int32(l_i32.clone()),
                    colormap: CategoricalColormap::Tableau10,
                })),
                position_x: NumericData::Float32(x_f32.clone()),
                position_y: NumericData::Float32(y_f32.clone()),
                position_z: NumericData::Float32(z_f32.clone()),
                labels_vec: l_i32.clone(),
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
impl DrawToRasterGpu for ZarrPoint3dLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        if let Some(inner) = &self.inner {
            DrawToRasterGpu::draw(inner, gpu_context, pass).await;
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for ZarrPoint3dLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for ZarrPoint3dLayer {
    async fn draw(&self, _ctx: &mut SvgContext) {
        // SVG rendering not supported for 3D layers.
    }
}

impl PickableLayer for ZarrPoint3dLayer {
    fn pick(&self, screen_coord: ScreenCoord, data_coord: Option<DataCoord>) -> Option<LayerPickingResult> {

        let DataCoord::ThreeD { x: _, y: _, z: _ } = data_coord? else {
            return None;
        };

        if let Some(inner) = &self.inner {
            return PickableLayer::pick(inner, screen_coord, data_coord);
        }
        return None;
    }
}
