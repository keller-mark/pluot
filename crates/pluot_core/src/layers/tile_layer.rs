// TODO: remove this layer definition?
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use crate::render_traits::{
    CategoricalColormap, CategoricalParams, ColorMode, DrawToRasterGpu, DrawToRasterCpu, DrawToSvg,
    MarginParams, PickableLayer, PreparedAndDraw, PreparedLayer, UnitsMode, ViewParams,
};
use crate::viewport::get_bounds;
use crate::layers::composite_layer::{base_draw_composite_layer, base_draw_composite_layer_svg};
use crate::two::svg::SvgContext;
use crate::layers::rect_layer::{RectLayer, RectLayerParams};
use crate::numeric_data::NumericData;
use crate::render_types::{CpuContext, CpuRenderPass, PrepareResult, RenderResult};
use crate::render_types::GpuContext;
use crate::wgpu;


#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TileLayerParams {
    pub layer_id: String,
    /// The size of each tile in data units.
    pub tile_size: f64,


}


// TODO: rename to "DebugTileLayer" or similar,
// and move its core functionality into plain functions that can be composed into other "tiled" layers.
pub struct TileLayer {
    view_params: ViewParams,
    layer_params: TileLayerParams,
    sub_layer_instances: Vec<Box<dyn PreparedAndDraw>>,
}

impl TileLayer {
    pub fn new(view_params: ViewParams, layer_params: TileLayerParams) -> Self {
        Self {
            view_params,
            layer_params,
            sub_layer_instances: Vec::new(),
        }
    }

    /// Build RectLayer sublayers for each visible tile.
    fn build_sublayers(&self) -> Vec<Box<dyn PreparedAndDraw>> {
        let tile_size = self.layer_params.tile_size;
        let b = get_bounds(&self.view_params);
        let (min_x, max_x, min_y, max_y) = (b.x_min as f64, b.x_max as f64, b.y_min as f64, b.y_max as f64);

        // Determine the range of tile indices that overlap the visible area.
        // Tile (col, row) covers data range [col*tile_size, (col+1)*tile_size) x [row*tile_size, (row+1)*tile_size).
        let tile_col_start = (min_x / tile_size).floor() as i32;
        let tile_col_end = (max_x / tile_size).ceil() as i32;
        let tile_row_start = (min_y / tile_size).floor() as i32;
        let tile_row_end = (max_y / tile_size).ceil() as i32;

        let mut sublayers: Vec<Box<dyn PreparedAndDraw>> = Vec::new();

        // Use zero margins for sublayers since the TileLayer manages positioning
        // via the camera_view / viewport already.
        let bounds = MarginParams {
            margin_top: Some(0.0),
            margin_right: Some(0.0),
            margin_bottom: Some(0.0),
            margin_left: Some(0.0),
        };

        // Collect all visible tile rects into a single RectLayer for efficiency.
        let mut x0_vec: Vec<f32> = Vec::new();
        let mut y0_vec: Vec<f32> = Vec::new();
        let mut x1_vec: Vec<f32> = Vec::new();
        let mut y1_vec: Vec<f32> = Vec::new();
        let mut labels_vec: Vec<i32> = Vec::new();

        for row in tile_row_start..tile_row_end {
            for col in tile_col_start..tile_col_end {
                let x0 = col as f64 * tile_size;
                let y0 = row as f64 * tile_size;
                let x1 = x0 + tile_size;
                let y1 = y0 + tile_size;

                x0_vec.push(x0 as f32);
                y0_vec.push(y0 as f32);
                x1_vec.push(x1 as f32);
                y1_vec.push(y1 as f32);
                // Use a label derived from tile coordinates for potential coloring/debugging.
                labels_vec.push((((row + col) % 2)));
            }
        }

        if !x0_vec.is_empty() {
            let rect_params = RectLayerParams {
                layer_id: format!("{}_tiles", self.layer_params.layer_id),
                bounds: self.view_params.margins.clone(),
                data_unit_mode_x: UnitsMode::Data,
                data_unit_mode_y: UnitsMode::Data,
                stroke_width: Some(1.0),
                stroke_width_unit_mode: UnitsMode::Pixels,
                fill_color: ColorMode::Categorical(CategoricalParams {
                    values: NumericData::Int32(Arc::new(labels_vec)),
                    colormap: CategoricalColormap::Tableau10,
                }),
                model_matrix: None,
                position_x0: NumericData::Float32(Arc::new(x0_vec)),
                position_y0: NumericData::Float32(Arc::new(y0_vec)),
                position_x1: NumericData::Float32(Arc::new(x1_vec)),
                position_y1: NumericData::Float32(Arc::new(y1_vec)),
            };
            sublayers.push(Box::new(RectLayer::new(
                self.view_params.clone(),
                rect_params,
            )));
        }

        sublayers
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for TileLayer {
    async fn prepare(&mut self, gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        self.sub_layer_instances = self.build_sublayers();

        for sub_layer in self.sub_layer_instances.iter_mut() {
            sub_layer.prepare(gpu_context).await;
        }

        return PrepareResult {
            bailed_early: false,
        };
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for TileLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        base_draw_composite_layer(&self.sub_layer_instances, gpu_context, pass).await;
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for TileLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for TileLayer {
    async fn draw(&self, ctx: &mut SvgContext) {
        base_draw_composite_layer_svg(&self.sub_layer_instances, ctx).await
    }
}

inventory::submit! {
    crate::registry::LayerRegistration {
        layer_type_name: "TileLayer",
        create_layer: |value, view_params| {
            let params: TileLayerParams = serde_json::from_value(value).unwrap();
            Box::new(TileLayer::new(view_params.clone(), params))
        },
    }
}

impl PickableLayer for TileLayer {}
