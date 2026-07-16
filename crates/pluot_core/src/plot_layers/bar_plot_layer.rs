// The bar plot layer is a wrapper around RectLayer, AxisLinearLayer, and AxisBandLayer.
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use crate::render_traits::{
    ColorMode, DrawToRasterCpu, DrawToRasterGpu, DrawToSvg, MarginParams, PickableLayer, PreparedAndDraw, PreparedLayer, UnitsMode, ViewParams
};
use std::collections::HashMap;
use crate::picking::LayerPickingResult;
use crate::viewport::{get_bounds, DataCoord, ScreenCoord};
use crate::layers::composite_layer::{base_draw_composite_layer, base_draw_composite_layer_svg};
use crate::two::svg::SvgContext;
use crate::layers::rect_layer::{RectLayer, RectLayerParams};
use crate::numeric_data::NumericData;
use crate::layers::axis_band_layer::{AxisBandLayer, AxisBandLayerParams};
use crate::layers::axis_linear_layer::{AxisLinearLayer, AxisLinearLayerParams, AxisPosition};
use crate::render_types::{CpuContext, CpuRenderPass, PrepareResult, RenderResult};
use crate::render_types::GpuContext;
use crate::{log, wgpu};
use crate::d3::scale::{ScaleBand, ScaleLinear, Scaleable};


const DEFAULT_BAR_MARGIN: f64 = 4.0;

// TODO: should this be a more general/common "PlotOrientation" enum?
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum BarOrientation {
    Vertical,
    Horizontal,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BarPlotLayerParams {
    pub layer_id: String,
    // If None, assume margin: 0 in all directions.
    pub bounds: Option<MarginParams>,
    pub data_unit_mode_for_identifier_dim: UnitsMode,
    pub data_unit_mode_for_quantity_dim: UnitsMode,

    pub orientation: BarOrientation,

    // The Vec of bar identifiers.
    pub identifier: Arc<Vec<String>>, // TODO: generalize to other dtypes?
    // The Vec of quantities used to compute bar heights/lengths.
    pub quantity: Arc<Vec<f32>>, // TODO: generalize to other numeric dtypes?

    // TODO: color of bars
    pub fill_color: Option<(u8, u8, u8)>,
    pub fill_color_mode: ColorMode,
    // TODO: stacked bars (here or own layer?)
    // TODO: grouped bars (here or own layer?)
    // TODO: configurable bar margin
    // TODO: configurable axis layer positioning?

}

pub struct BarPlotLayer {
    view_params: ViewParams,
    layer_params: BarPlotLayerParams,
    sub_layer_instances: Vec<Box<dyn PreparedAndDraw>>,
}

impl BarPlotLayer {
    pub fn new(view_params: ViewParams, layer_params: BarPlotLayerParams) -> Self {
        Self {
            view_params,
            layer_params,
            sub_layer_instances: Vec::new(),
        }
    }

    /// Construct the RectLayer, AxisLinearLayer, and AxisBandLayer.
    fn build_sublayers(&self) -> Vec<Box<dyn PreparedAndDraw>> {
        let b = get_bounds(&self.view_params);
        let (min_x, max_x, min_y, max_y) = (b.x_min as f64, b.x_max as f64, b.y_min as f64, b.y_max as f64);

        let bounds = &self.view_params.margins;
        let margin_top = bounds.as_ref().and_then(|m| m.margin_top).unwrap_or(0.0) as f64;
        let margin_right = bounds.as_ref().and_then(|m| m.margin_right).unwrap_or(0.0) as f64;
        let margin_bottom = bounds.as_ref().and_then(|m| m.margin_bottom).unwrap_or(0.0) as f64;
        let margin_left = bounds.as_ref().and_then(|m| m.margin_left).unwrap_or(0.0) as f64;

        let viewport_w = self.view_params.width as f64;
        let viewport_h = self.view_params.height as f64;

        let layer_w = viewport_w - margin_left - margin_right;
        let layer_h = viewport_h - margin_bottom - margin_top;

        let n = self.layer_params.identifier.len();
        let mut position_x0: Vec<f32> = Vec::with_capacity(n);
        let mut position_y0: Vec<f32> = Vec::with_capacity(n);
        let mut position_x1: Vec<f32> = Vec::with_capacity(n);
        let mut position_y1: Vec<f32> = Vec::with_capacity(n);
        let labels_vec: Vec<i32> = (0..n as i32).collect();

        let mut scale_band = ScaleBand::new();
        scale_band.set_domain(self.layer_params.identifier.as_ref().clone());

        match self.layer_params.orientation {
            BarOrientation::Vertical => {
                // Categorical on X (pixels), quantitative on Y (data units)
                scale_band.set_range((0.0, layer_w));
                let bandwidth = scale_band.bandwidth();

                for i in 0..n {
                    let band_start = scale_band.scale(&self.layer_params.identifier[i]) as f32;
                    position_x0.push(band_start + (DEFAULT_BAR_MARGIN / 2.0) as f32);
                    position_x1.push(band_start + bandwidth as f32 - (DEFAULT_BAR_MARGIN / 2.0) as f32);
                    position_y0.push(0.0);
                    position_y1.push(self.layer_params.quantity[i]);
                }

                return vec![
                    Box::new(RectLayer::new(
                        self.view_params.clone(),
                        RectLayerParams {
                            layer_id: self.layer_params.layer_id.clone(),
                            bounds: self.layer_params.bounds.clone(),
                            data_unit_mode_x: UnitsMode::Pixels,
                            data_unit_mode_y: UnitsMode::Data,
                            stroke_width: None,
                            stroke_width_unit_mode: UnitsMode::Pixels,
                            fill_color: self.layer_params.fill_color,
                            fill_color_mode: self.layer_params.fill_color_mode,
                            model_matrix: None,
                            position_x0: NumericData::Float32(Arc::new(position_x0)),
                            position_y0: NumericData::Float32(Arc::new(position_y0)),
                            position_x1: NumericData::Float32(Arc::new(position_x1)),
                            position_y1: NumericData::Float32(Arc::new(position_y1)),
                            labels_vec: Arc::new(labels_vec),
                        }
                    )),
                    Box::new(AxisLinearLayer::new(
                        self.view_params.clone(),
                        AxisLinearLayerParams {
                            layer_id: format!("{}_bar_plot_layer_quantitative_axis_sublayer", self.layer_params.layer_id),
                            position: AxisPosition::Left,
                        }
                    )),
                    Box::new(AxisBandLayer::new(
                        self.view_params.clone(),
                        AxisBandLayerParams {
                            layer_id: format!("{}_bar_plot_layer_categorical_axis_sublayer", self.layer_params.layer_id),
                            position: AxisPosition::Bottom,
                            domain: self.layer_params.identifier.clone()
                        }
                    ))
                ];
            }
            BarOrientation::Horizontal => {
                // Categorical on Y (pixels), quantitative on X (data units)
                scale_band.set_range((0.0, layer_h));
                let bandwidth = scale_band.bandwidth();

                for i in 0..n {
                    let band_start = scale_band.scale(&self.layer_params.identifier[i]) as f32;
                    position_x0.push(0.0);
                    position_x1.push(self.layer_params.quantity[i]);
                    position_y0.push(band_start + (DEFAULT_BAR_MARGIN / 2.0) as f32);
                    position_y1.push(band_start + bandwidth as f32 - (DEFAULT_BAR_MARGIN / 2.0) as f32);
                }

                return vec![
                    Box::new(RectLayer::new(
                        self.view_params.clone(),
                        RectLayerParams {
                            layer_id: self.layer_params.layer_id.clone(),
                            bounds: self.layer_params.bounds.clone(),
                            data_unit_mode_x: UnitsMode::Data,
                            data_unit_mode_y: UnitsMode::Pixels,
                            stroke_width: None,
                            stroke_width_unit_mode: UnitsMode::Pixels,
                            fill_color: self.layer_params.fill_color,
                            fill_color_mode: self.layer_params.fill_color_mode,
                            model_matrix: None,
                            position_x0: NumericData::Float32(Arc::new(position_x0)),
                            position_y0: NumericData::Float32(Arc::new(position_y0)),
                            position_x1: NumericData::Float32(Arc::new(position_x1)),
                            position_y1: NumericData::Float32(Arc::new(position_y1)),
                            labels_vec: Arc::new(labels_vec),
                        }
                    )),
                    Box::new(AxisLinearLayer::new(
                        self.view_params.clone(),
                        AxisLinearLayerParams {
                            layer_id: format!("{}_bar_plot_layer_quantitative_axis_sublayer", self.layer_params.layer_id),
                            position: AxisPosition::Bottom,
                        }
                    )),
                    Box::new(AxisBandLayer::new(
                        self.view_params.clone(),
                        AxisBandLayerParams {
                            layer_id: format!("{}_bar_plot_layer_categorical_axis_sublayer", self.layer_params.layer_id),
                            position: AxisPosition::Left,
                            domain: self.layer_params.identifier.clone()
                        }
                    ))
                ];
            }
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for BarPlotLayer {
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
impl DrawToRasterGpu for BarPlotLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        base_draw_composite_layer(&self.sub_layer_instances, gpu_context, pass).await;
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for BarPlotLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for BarPlotLayer {
    async fn draw(&self, ctx: &mut SvgContext) {
        base_draw_composite_layer_svg(&self.sub_layer_instances, ctx).await
    }
}

inventory::submit! {
    crate::registry::LayerRegistration {
        layer_type_name: "BarPlotLayer",
        create_layer: |value, view_params| {
            let params: BarPlotLayerParams = serde_json::from_value(value).unwrap();
            Box::new(BarPlotLayer::new(view_params.clone(), params))
        },
    }
}

impl PickableLayer for BarPlotLayer {
    fn pick(&self, screen_coord: ScreenCoord, data_coord: Option<DataCoord>) -> Option<LayerPickingResult> {
        let DataCoord::TwoD { x: data_coord_x, y: data_coord_y } = data_coord? else {
            return None;
        };

        let n = self.layer_params.identifier.len();
        if n == 0 {
            return None;
        }

        // TODO: implement a ScaleBand.invert and ScaleLinear.invert method?

        // Subtract margins from screen_coord.
        // TODO: subtract margins in the upstream pick() function so it does not have to be done here?
        let bounds = &self.view_params.margins;
        let margin_top = bounds.as_ref().and_then(|m| m.margin_top).unwrap_or(0.0) as f64;
        let margin_right = bounds.as_ref().and_then(|m| m.margin_right).unwrap_or(0.0) as f64;
        let margin_bottom = bounds.as_ref().and_then(|m| m.margin_bottom).unwrap_or(0.0) as f64;
        let margin_left = bounds.as_ref().and_then(|m| m.margin_left).unwrap_or(0.0) as f64;

        let viewport_w = self.view_params.width as f64;
        let viewport_h = self.view_params.height as f64;

        let layer_w = viewport_w - margin_left - margin_right;
        let layer_h = viewport_h - margin_bottom - margin_top;

        let layer_x = screen_coord.x - margin_left as f32;
        let layer_y = screen_coord.y - margin_bottom as f32;

        // Use the bandwidth to identify the bar of interest.
        // TODO: also account for the bar height/width and padding.

        let mut bar_idx = 0usize;

        match self.layer_params.orientation {
            BarOrientation::Vertical => {
                // Use X coord if orientation is Vertical.
                match self.layer_params.data_unit_mode_for_identifier_dim {
                    UnitsMode::Data => {
                        // Use data_coord.x if data_unit_mode_for_identifier_dim is Data.
                        todo!("Not yet implemented");
                    }
                    UnitsMode::Pixels => {
                        // Use screen_coord.x (layer_x) if Pixels.
                        let bandwidth = layer_w / n as f64;
                        bar_idx = (layer_x / bandwidth as f32).floor() as usize;
                    }
                };
            }
            BarOrientation::Horizontal => {
                // Use Y coord if orientation is Horizontal.
                match self.layer_params.data_unit_mode_for_identifier_dim {
                    UnitsMode::Data => {
                        // Use data_coord.y if data_unit_mode_for_identifier_dim is Data.
                        todo!("Not yet implemented");
                    }
                    UnitsMode::Pixels => {
                        // Use screen_coord.y (layer_y) if Pixels.
                        todo!("Not yet implemented");
                    }
                };
            }
        };

        log(&format!("bar_idx is {}.", bar_idx));

        let mut info = HashMap::new();
        info.insert("index".to_string(), bar_idx.to_string());
        info.insert("identifier".to_string(), self.layer_params.identifier[bar_idx].to_string());
        info.insert("quantity".to_string(), self.layer_params.quantity[bar_idx].to_string());

        Some(LayerPickingResult {
            layer_id: self.layer_params.layer_id.clone(),
            info,
        })
    }
}
