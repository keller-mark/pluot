use std::sync::Arc;

use serde::{Deserialize, Serialize};
use svg::node::element::Group;

use crate::layers::core::{
    AspectRatioMode, DrawToCanvas, DrawToSvg, MarginParams, PreparedAndDraw, PreparedLayer,
    UnitsMode, ViewParams,
};
use crate::layers::composite_layer::{base_draw_composite_layer, base_draw_composite_layer_svg};
use crate::layers::rect_layer::{RectLayer, RectLayerParams};
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

    /// Extract zoom and translation from the camera_view matrix.
    fn get_view_transform(&self) -> (f32, f32, f32) {
        let camera_view = self.view_params.camera_view.unwrap_or([
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ]);
        let zoom = camera_view[0];
        let translate_x = camera_view[12];
        let translate_y = camera_view[13];
        (zoom, translate_x, translate_y)
    }

    /// Calculate the visible data range based on camera view, in the (0, 1) data coordinate space.
    fn get_visible_range(&self) -> (f64, f64, f64, f64) {
        let (zoom, translate_x, translate_y) = self.get_view_transform();

        let aspect_ratio_mode = self.view_params.aspect_ratio_mode;

        let bounds = &self.view_params.margins;

        let margin_top = bounds.as_ref().and_then(|m| m.margin_top).unwrap_or(0.0) as f64;
        let margin_right = bounds.as_ref().and_then(|m| m.margin_right).unwrap_or(0.0) as f64;
        let margin_bottom = bounds.as_ref().and_then(|m| m.margin_bottom).unwrap_or(0.0) as f64;
        let margin_left = bounds.as_ref().and_then(|m| m.margin_left).unwrap_or(0.0) as f64;

        let viewport_w = self.view_params.width as f32;
        let viewport_h = self.view_params.height as f32;

        let layer_w = viewport_w - (margin_left + margin_right) as f32;
        let layer_h = viewport_h - (margin_top + margin_bottom) as f32;

        let layer_aspect_ratio = layer_w / layer_h;

        let mut x_scale_for_aspect_ratio_mode = 1.0_f32;
        let mut y_scale_for_aspect_ratio_mode = 1.0_f32;
        match aspect_ratio_mode {
            AspectRatioMode::Ignore => {}
            AspectRatioMode::Contain => {
                if layer_aspect_ratio > 1.0 {
                    x_scale_for_aspect_ratio_mode = layer_aspect_ratio;
                } else if layer_aspect_ratio < 1.0 {
                    y_scale_for_aspect_ratio_mode = layer_aspect_ratio;
                }
            }
            AspectRatioMode::Cover => {
                if layer_aspect_ratio > 1.0 {
                    y_scale_for_aspect_ratio_mode = 1.0 / layer_aspect_ratio;
                } else if layer_aspect_ratio < 1.0 {
                    x_scale_for_aspect_ratio_mode = 1.0 / layer_aspect_ratio;
                }
            }
        }

        let x_adjustment = x_scale_for_aspect_ratio_mode - 1.0;
        let y_adjustment = y_scale_for_aspect_ratio_mode - 1.0;

        let min_x = (((-translate_x - 1.0 - x_adjustment) / zoom) + 1.0) / 2.0;
        let max_x = (((-translate_x + 1.0 + x_adjustment) / zoom) + 1.0) / 2.0;
        let min_y = (((-translate_y - 1.0 - y_adjustment) / zoom) + 1.0) / 2.0;
        let max_y = (((-translate_y + 1.0 + y_adjustment) / zoom) + 1.0) / 2.0;

        (min_x as f64, max_x as f64, min_y as f64, max_y as f64)
    }

    /// Build RectLayer sublayers for each visible tile.
    fn build_sublayers(&self) -> Vec<Box<dyn PreparedAndDraw>> {
        let tile_size = self.layer_params.tile_size;
        let (min_x, max_x, min_y, max_y) = self.get_visible_range();

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
                labels_vec.push(((row + col) % 2) as i32);
            }
        }

        if !x0_vec.is_empty() {
            let rect_params = RectLayerParams {
                layer_id: format!("{}_tiles", self.layer_params.layer_id),
                bounds: self.view_params.margins.clone(),
                data_unit_mode: UnitsMode::Data,
                stroke_width: 1.0,
                stroke_width_unit_mode: UnitsMode::Pixels,
                position_x0: Arc::new(x0_vec),
                position_y0: Arc::new(y0_vec),
                position_x1: Arc::new(x1_vec),
                position_y1: Arc::new(y1_vec),
                labels_vec: Arc::new(labels_vec),
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
    async fn prepare(&mut self) {
        self.sub_layer_instances = self.build_sublayers();

        for sub_layer in self.sub_layer_instances.iter_mut() {
            sub_layer.prepare().await;
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToCanvas for TileLayer {
    async fn draw(&self, device: wgpu::Device, queue: wgpu::Queue, pass: &mut wgpu::RenderPass) {
        base_draw_composite_layer(&self.sub_layer_instances, device, queue, pass).await;
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for TileLayer {
    async fn draw(&self, group: &Group) -> Group {
        base_draw_composite_layer_svg(&self.sub_layer_instances, group).await
    }
}
