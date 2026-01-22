use std::sync::Arc;
use encase::{ShaderType, UniformBuffer};
use glam::{Mat4, Vec2, Vec4};
use serde::{Deserialize, Serialize};

use crate::layers::core::{DrawToCanvas, DrawToSvg, PreparedLayer, ViewParams, AspectRatioMode, UnitsMode, MarginParams};
use crate::layers::scatterplot_layer::{PointShapeMode, ScatterplotLayerData, base_draw_scatterplot_layer, base_draw_scatterplot_layer_svg};
use crate::wgpu;
use crate::zarr::AsyncZarritaStore;
use crate::cache::{get_or_init_store, use_memo_vec_f32, use_memo_vec_i32};
use crate::two::svg::update_svg;
use svg::node::element::Group;
use crate::log;




#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ZarrScatterplotLayerParams {
    pub layer_id: String,
    // If None, assume margin: 0 in all directions.
    pub bounds: Option<MarginParams>,
    pub data_unit_mode: UnitsMode,
    pub point_radius: f32,
    pub point_radius_unit_mode: UnitsMode,
    pub point_shape_mode: PointShapeMode,

    // Data keys
    pub store_name: String,
    pub x_key: String,
    pub y_key: String,
    pub color_key: Option<String>,
}

// TODO: defaults for params?


pub struct ZarrScatterplotLayer {
    view_params: ViewParams,
    layer_params: ZarrScatterplotLayerParams,
    // TODO: do we want the store or just the store_name here?
    store: Arc<AsyncZarritaStore>,
    // Data will be None prior to runninng prepare().
    data: Option<ScatterplotLayerData>,
}

impl ZarrScatterplotLayer {
    pub fn new(
        view_params: ViewParams,
        layer_params: ZarrScatterplotLayerParams,
    ) -> Self {
        // Error if point_radius_unit_mode is "data" when data_unit_mode is "pixels".
        if (layer_params.point_radius_unit_mode == UnitsMode::Data && layer_params.data_unit_mode == UnitsMode::Pixels) {
            panic!("point_radius_unit_mode cannot be 'data' when data_unit_mode is 'pixels'");
        }
        let store = get_or_init_store(&layer_params.store_name);
        Self {
            view_params,
            layer_params,
            store,
            data: None,
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for ZarrScatterplotLayer {
    async fn prepare(&mut self) {
        let store = self.store.clone();

        // TODO: include the layer type in the memoization dependencies?
        // But what if we want multiple layers to be able to reuse the same cached data?
        // Then we should also avoid including the layer_id...
        let l_i32_future_deps = vec!["l_bytes".to_string(), self.layer_params.store_name.to_string(), self.layer_params.layer_id.to_string()];
        let l_i32_future = use_memo_vec_i32(async || {
            let labels_array_path = &self.layer_params.color_key.as_ref().expect("Color key");
            let labels_array_future = zarrs::array::Array::async_open(store.clone(), labels_array_path);
            let labels_array = labels_array_future.await.unwrap();
            let labels_subset = labels_array.subset_all();
            let labels_result = labels_array.async_retrieve_array_subset::<Vec<i64>>(&labels_subset).await;

            let labels_vec = labels_result.unwrap();
            // More efficient version that eliminates intermediate vectors and redundant operations
            // Convert to f32 and cast to bytes directly - no for loop needed
            let labels_i32: Vec<i32> = labels_vec.iter().map(|&c| c as i32).collect();
            labels_i32
        }, &l_i32_future_deps, self.view_params.cache_enabled);

        // TODO: improve the keys / memoization dependencies to at least include the plot_id and store_name.
        let x_f32_future_deps = vec!["x_bytes".to_string(), self.layer_params.store_name.to_string(), self.layer_params.layer_id.to_string()];
        let x_f32_future = use_memo_vec_f32(async || {
            let x_array_path = &self.layer_params.x_key.as_ref();
            let x_array_future = zarrs::array::Array::async_open(store.clone(), x_array_path);
            let x_array = x_array_future.await.unwrap();
            let x_subset = x_array.subset_all();
            let x_result = x_array.async_retrieve_array_subset::<Vec<f64>>(&x_subset).await;

            let x_vec = x_result.unwrap();
            let x_f32_inner: Vec<f32> = x_vec.iter().map(|&x| x as f32).collect();
            x_f32_inner
        }, &x_f32_future_deps, self.view_params.cache_enabled);

        let y_f32_future_deps = vec!["y_bytes".to_string(), self.layer_params.store_name.to_string(), self.layer_params.layer_id.to_string()];
        let y_f32_future = use_memo_vec_f32(async || {
            let y_array_path = &self.layer_params.y_key.as_ref();
            let y_array_future = zarrs::array::Array::async_open(store.clone(), y_array_path);
            let y_array = y_array_future.await.unwrap();
            let y_subset = y_array.subset_all();
            let y_result = y_array.async_retrieve_array_subset::<Vec<f64>>(&y_subset).await;

            let y_vec = y_result.unwrap();
            let y_f32_inner: Vec<f32> = y_vec.iter().map(|&y| y as f32).collect();
            y_f32_inner
        }, &y_f32_future_deps, self.view_params.cache_enabled);

        // Await in parallel: Use futures::join, similar to Promise.all in JS.
        let (x_f32, y_f32, l_i32) = futures::join!(x_f32_future, y_f32_future, l_i32_future);

        self.data = Some(ScatterplotLayerData {
            x_arr: x_f32,
            y_arr: y_f32,
            labels_arr: l_i32,
        });
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToCanvas for ZarrScatterplotLayer {
    async fn draw(&self, device: wgpu::Device, queue: wgpu::Queue, pass: &mut wgpu::RenderPass) {
        let data = self.data.as_ref().expect("Data was not prepared. Call prepare() first.");
        base_draw_scatterplot_layer(
            device, queue, pass,
            data,
            &self.view_params,
            &self.layer_params.bounds,
            &self.layer_params.data_unit_mode,
            self.layer_params.point_radius,
            &self.layer_params.point_radius_unit_mode,
            &self.layer_params.point_shape_mode,
        ).await;
    }
}


#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for ZarrScatterplotLayer {
    async fn draw(&self, group: &Group) -> Group {
        let data = self.data.as_ref().expect("Data was not prepared. Call prepare() first.");

        let view_params = &self.view_params;
        let bounds = &self.layer_params.bounds;

        let svg_elements = base_draw_scatterplot_layer_svg(
            data,
            view_params,
            bounds,
            &self.layer_params.data_unit_mode,
            self.layer_params.point_radius,
            &self.layer_params.point_radius_unit_mode,
            &self.layer_params.point_shape_mode,
            &self.layer_params.layer_id,
        );

        // TODO: use an SVG group with a transform and clipping to handle margins,
        // similar to the usage of scissor rect and viewport in the Canvas rendering.
        
        // TODO: refactor to avoid the cloning here?
        let updated_group = update_svg(group.clone(), &svg_elements);

        log("Done drawing ZarrScatterplotLayer to SVG.");

        return updated_group.clone();
        
    }
}