use std::sync::Arc;

use serde::{Deserialize, Serialize};
use svg::node::element::Group;

use crate::layers::composite_layer::{base_draw_composite_layer, base_draw_composite_layer_svg};
use crate::layer_traits::{
    DrawToCanvas, DrawToSvg, PreparedAndDraw, PreparedLayer,
    UnitsMode, ViewParams,
};
use crate::layers::rect_layer::{RectLayer, RectLayerParams};
use crate::layers::multiscale_utils::{
    self, ResolutionLevel, get_visible_tiles, select_resolution_level,
};
use crate::params::PrepareResult;
use crate::wgpu;


#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MultiscaleLayerParams {
    pub layer_id: String,
    /// Resolution levels ordered from highest resolution (finest, level 0)
    /// to lowest resolution (coarsest), per the OME-NGFF spec.
    /// Must contain at least one level.
    pub resolution_levels: Vec<ResolutionLevel>,
}

pub struct MultiscaleLayer {
    view_params: ViewParams,
    layer_params: MultiscaleLayerParams,
    sub_layer_instances: Vec<Box<dyn PreparedAndDraw>>,
}

impl MultiscaleLayer {
    pub fn new(view_params: ViewParams, layer_params: MultiscaleLayerParams) -> Self {
        assert!(
            !layer_params.resolution_levels.is_empty(),
            "MultiscaleLayer requires at least one resolution level"
        );
        Self {
            view_params,
            layer_params,
            sub_layer_instances: Vec::new(),
        }
    }

    /// Build RectLayer sublayers for each visible tile at the selected resolution level.
    fn build_sublayers(&self) -> Vec<Box<dyn PreparedAndDraw>> {
        let levels = &self.layer_params.resolution_levels;
        let level_idx = select_resolution_level(&self.view_params, levels);
        let level = &levels[level_idx];

        let tiles = get_visible_tiles(&self.view_params, level);

        let mut sublayers: Vec<Box<dyn PreparedAndDraw>> = Vec::new();

        let mut x0_vec: Vec<f32> = Vec::new();
        let mut y0_vec: Vec<f32> = Vec::new();
        let mut x1_vec: Vec<f32> = Vec::new();
        let mut y1_vec: Vec<f32> = Vec::new();
        let mut labels_vec: Vec<i32> = Vec::new();

        for tile in &tiles {
            let tile_pixels_w = (tile.tile_x_end - tile.tile_x_start) as f64;
            let tile_pixels_h = (tile.tile_y_end - tile.tile_y_start) as f64;
            let phys_x1 = tile.phys_x0 + tile_pixels_w * level.scale[1];
            let phys_y1 = tile.phys_y0 + tile_pixels_h * level.scale[0];

            x0_vec.push(tile.phys_x0 as f32);
            y0_vec.push(tile.phys_y0 as f32);
            x1_vec.push(phys_x1 as f32);
            y1_vec.push(phys_y1 as f32);

            // Checkerboard label for visual debugging.
            labels_vec.push(((tile.row + tile.col + level_idx as i32) % 2) as i32);
        }

        if !x0_vec.is_empty() {
            let rect_params = RectLayerParams {
                layer_id: format!(
                    "{}_tiles_level{}",
                    self.layer_params.layer_id, level_idx
                ),
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
impl PreparedLayer for MultiscaleLayer {
    async fn prepare(&mut self) -> PrepareResult {
        self.sub_layer_instances = self.build_sublayers();

        for sub_layer in self.sub_layer_instances.iter_mut() {
            sub_layer.prepare().await;
        }

        PrepareResult {
            bailed_early: false,
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToCanvas for MultiscaleLayer {
    async fn draw(&self, device: wgpu::Device, queue: wgpu::Queue, pass: &mut wgpu::RenderPass) {
        base_draw_composite_layer(&self.sub_layer_instances, device, queue, pass).await;
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for MultiscaleLayer {
    async fn draw(&self, group: &Group) -> Group {
        base_draw_composite_layer_svg(&self.sub_layer_instances, group).await
    }
}

inventory::submit! {
    crate::registry::LayerRegistration {
        layer_type_name: "MultiscaleLayer",
        create_layer: |value, view_params| {
            let params: MultiscaleLayerParams = serde_json::from_value(value).unwrap();
            Box::new(MultiscaleLayer::new(view_params.clone(), params))
        },
    }
}
