use serde::{Deserialize, Serialize};
use ts_rs::TS;

use std::sync::Arc;
use crate::render_traits::{BrushableLayer, ExtentableLayer, ColorMode, DrawToRasterGpu, DrawToRasterCpu, DrawToSvg, PickableLayer, PreparedLayer, ViewParams, PreparedAndDraw, MarginParams, SizeMode, UnitsMode, FontWeight, FontStyle};
use crate::composite_layer::{base_draw_composite_layer, base_draw_composite_layer_svg, base_prepare_composite_layer};
use crate::two::svg::SvgContext;
use crate::layers::text_layer::{TextLayer, TextLayerParams, TextAlignMode, TextBaselineMode};
use crate::layers::line_layer::{LineLayer, LineLayerParams};
use crate::composite_layers::axis_linear_layer::AxisPosition;
use crate::viewport::get_bounds;
use crate::numeric_data::NumericData;
use crate::render_types::{CpuContext, CpuRenderPass, PrepareResult};
use crate::gpu_send::GpuRenderPass;
use crate::render_types::GpuContext;
use crate::wgpu;
use crate::d3::scale::{ScaleBand, Scaleable, Tickable};

const DEFAULT_TICK_SIZE: f64 = 6.0;
const DEFAULT_TICK_PADDING: f64 = 3.0;
const DEFAULT_FONT_SIZE: f64 = 12.0;
const DEFAULT_TITLE_PADDING: f64 = 3.0;
const DEFAULT_LINE_WIDTH: f32 = 1.0;

/// Layer params struct for [`AxisBandLayer`].
#[derive(Serialize, Deserialize, Debug, Clone, TS)]
#[serde(default)]
#[ts(optional_fields = nullable)]
pub struct AxisBandLayerParams {
    pub layer_id: String,
    pub position: AxisPosition,
    pub domain: Arc<Vec<String>>,

    /// The span of data coordinates the bands cover along this axis, e.g.
    /// `(0, n)` for `n` bands one data unit wide. When set, the bands follow
    /// the camera, and bands whose center is outside the visible range are not
    /// drawn. `None` (the default) spreads the bands across the layer in
    /// pixels, ignoring the camera.
    pub data_range: Option<(f64, f64)>,

    /// Text centered along the axis at the outer edge of the margin. `None` draws no title.
    pub title: Option<String>,
    /// When true, the ticks and tick labels are hidden while each band is
    /// narrower (in pixels) than the tick label font size, so that the labels
    /// never overlap. Defaults to false.
    pub hide_overlapping_ticks: Option<bool>,
}

impl Default for AxisBandLayerParams {
    fn default() -> Self {
        Self {
            layer_id: "".to_string(),
            position: AxisPosition::Bottom,
            domain: Arc::new(vec![]),
            data_range: None,
            title: None,
            hide_overlapping_ticks: None,
        }
    }
}

pub struct AxisBandLayer {
    view_params: ViewParams,
    layer_params: AxisBandLayerParams,
    sub_layer_instances: Vec<Box<dyn PreparedAndDraw>>,
}

impl AxisBandLayer {
    pub fn new(view_params: ViewParams, layer_params: AxisBandLayerParams) -> Self {
        Self {
            view_params,
            layer_params,
            sub_layer_instances: Vec::new(),
        }
    }

    fn shown_ticks(&self, scale: &ScaleBand) -> Vec<String> {
        let ticks_overlap = scale.bandwidth().abs() < DEFAULT_FONT_SIZE;
        if self.layer_params.hide_overlapping_ticks.unwrap_or(false) && ticks_overlap {
            Vec::new()
        } else {
            scale.ticks(None)
        }
    }

    fn build_title_sublayer(&self, sublayer_bounds: MarginParams) -> Option<Box<dyn PreparedAndDraw>> {
        let title = self.layer_params.title.clone()?;
        let bounds = self.view_params.margins.as_ref();
        let margin = |f: fn(&MarginParams) -> Option<f32>| bounds.and_then(f).unwrap_or(0.0);
        let (width, height) = (self.view_params.width as f32, self.view_params.height as f32);
        let center_x = (margin(|m| m.margin_left) + width - margin(|m| m.margin_right)) / 2.0;
        let center_y = (margin(|m| m.margin_bottom) + height - margin(|m| m.margin_top)) / 2.0;
        let edge_offset = (DEFAULT_TITLE_PADDING + DEFAULT_FONT_SIZE / 2.0) as f32;
        let (x, y, rotation) = match self.layer_params.position {
            AxisPosition::Bottom => (center_x, edge_offset, 0.0),
            AxisPosition::Top => (center_x, height - edge_offset, 0.0),
            AxisPosition::Left => (edge_offset, center_y, -90.0),
            AxisPosition::Right => (width - edge_offset, center_y, 90.0),
        };
        Some(Box::new(TextLayer::new(
            self.view_params.clone(),
            TextLayerParams {
                layer_id: format!("{}_axis_band_layer_title_sublayer", self.layer_params.layer_id),
                bounds: Some(sublayer_bounds),
                data_unit_mode_x: UnitsMode::Pixels,
                data_unit_mode_y: UnitsMode::Pixels,
                text_size: DEFAULT_FONT_SIZE as f32,
                text_size_unit_mode: UnitsMode::Pixels,
                text_align_mode: TextAlignMode::Middle,
                text_baseline_mode: TextBaselineMode::Middle,
                text_rotation: Some(rotation),
                font_weight: FontWeight::Normal,
                font_style: FontStyle::Normal,
                position_x: NumericData::Float32(Arc::new(vec![x])),
                position_y: NumericData::Float32(Arc::new(vec![y])),
                text_vec: Arc::new(vec![title]),
                ..Default::default()
            },
        )))
    }

    fn build_sublayers(&self) -> Vec<Box<dyn PreparedAndDraw>> {
        let bounds = &self.view_params.margins;

        let margin_top = bounds.as_ref().and_then(|m| m.margin_top).unwrap_or(0.0) as f64;
        let margin_right = bounds.as_ref().and_then(|m| m.margin_right).unwrap_or(0.0) as f64;
        let margin_bottom = bounds.as_ref().and_then(|m| m.margin_bottom).unwrap_or(0.0) as f64;
        let margin_left = bounds.as_ref().and_then(|m| m.margin_left).unwrap_or(0.0) as f64;

        let viewport_w = self.view_params.width as f64;
        let viewport_h = self.view_params.height as f64;

        let mut line_source_positions: Vec<[f32; 2]> = Vec::new();
        let mut line_target_positions: Vec<[f32; 2]> = Vec::new();
        let mut text_positions: Vec<[f32; 2]> = Vec::new();
        let mut text_strings: Vec<String> = Vec::new();

        let mut scale = ScaleBand::new();
        scale.set_domain(self.layer_params.domain.to_vec());

        let horizontal_px = (margin_left, viewport_w - margin_right);
        let vertical_px = (margin_bottom, viewport_h - margin_top);
        let visible_bounds = get_bounds(&self.view_params);
        // The pixel span the bands cover: the whole layer, or the projection of
        // `data_range` given the visible data bounds.
        let band_range_px = |layer_px: (f64, f64), visible: (f32, f32)| match self.layer_params.data_range {
            None => layer_px,
            Some((start, end)) => {
                let to_px = |value: f64| {
                    layer_px.0 + (value - visible.0 as f64) / (visible.1 - visible.0) as f64 * (layer_px.1 - layer_px.0)
                };
                (to_px(start), to_px(end))
            }
        };
        let is_visible = |px: f64, layer_px: (f64, f64)| px >= layer_px.0 && px <= layer_px.1;

        match self.layer_params.position {
            AxisPosition::Bottom => {
                scale.set_range(band_range_px(horizontal_px, (visible_bounds.x_min, visible_bounds.x_max)));
                let ticks = self.shown_ticks(&scale);
                let bandwidth = scale.bandwidth();
                let axis_y = margin_bottom;

                // Main axis line
                line_source_positions.push([margin_left as f32, axis_y as f32]);
                line_target_positions.push([(viewport_w - margin_right) as f32, axis_y as f32]);

                for tick in &ticks {
                    let x_px = scale.scale(tick) + bandwidth / 2.0;
                    if !is_visible(x_px, horizontal_px) {
                        continue;
                    }
                    let x = x_px as f32;
                    let y = axis_y as f32;

                    // Tick line
                    line_source_positions.push([x, y]);
                    line_target_positions.push([x, y - DEFAULT_TICK_SIZE as f32]);

                    // Label position
                    text_positions.push([x, y - (DEFAULT_TICK_SIZE + DEFAULT_TICK_PADDING) as f32]);
                    text_strings.push(tick.clone());
                }
            }
            AxisPosition::Top => {
                scale.set_range(band_range_px(horizontal_px, (visible_bounds.x_min, visible_bounds.x_max)));
                let ticks = self.shown_ticks(&scale);
                let bandwidth = scale.bandwidth();
                let axis_y = viewport_h - margin_top;

                // Main axis line
                line_source_positions.push([margin_left as f32, axis_y as f32]);
                line_target_positions.push([(viewport_w - margin_right) as f32, axis_y as f32]);

                for tick in &ticks {
                    let x_px = scale.scale(tick) + bandwidth / 2.0;
                    if !is_visible(x_px, horizontal_px) {
                        continue;
                    }
                    let x = x_px as f32;
                    let y = axis_y as f32;

                    // Tick line (upward)
                    line_source_positions.push([x, y]);
                    line_target_positions.push([x, y + DEFAULT_TICK_SIZE as f32]);

                    // Label position
                    text_positions.push([x, y + (DEFAULT_TICK_SIZE + DEFAULT_TICK_PADDING) as f32]);
                    text_strings.push(tick.clone());
                }
            }
            AxisPosition::Left => {
                scale.set_range(band_range_px(vertical_px, (visible_bounds.y_min, visible_bounds.y_max)));
                let ticks = self.shown_ticks(&scale);
                let bandwidth = scale.bandwidth();
                let axis_x = margin_left;

                // Main axis line
                line_source_positions.push([axis_x as f32, margin_bottom as f32]);
                line_target_positions.push([axis_x as f32, (viewport_h - margin_top) as f32]);

                for tick in &ticks {
                    let y_px = scale.scale(tick) + bandwidth / 2.0;
                    if !is_visible(y_px, vertical_px) {
                        continue;
                    }
                    let x = axis_x as f32;
                    let y = y_px as f32;

                    // Tick line (leftward)
                    line_source_positions.push([x, y]);
                    line_target_positions.push([x - DEFAULT_TICK_SIZE as f32, y]);

                    // Label position
                    text_positions.push([x - (DEFAULT_TICK_SIZE + DEFAULT_TICK_PADDING) as f32, y]);
                    text_strings.push(tick.clone());
                }
            }
            AxisPosition::Right => {
                scale.set_range(band_range_px(vertical_px, (visible_bounds.y_min, visible_bounds.y_max)));
                let ticks = self.shown_ticks(&scale);
                let bandwidth = scale.bandwidth();
                let axis_x = viewport_w - margin_right;

                // Main axis line
                line_source_positions.push([axis_x as f32, margin_bottom as f32]);
                line_target_positions.push([axis_x as f32, (viewport_h - margin_top) as f32]);

                for tick in &ticks {
                    let y_px = scale.scale(tick) + bandwidth / 2.0;
                    if !is_visible(y_px, vertical_px) {
                        continue;
                    }
                    let x = axis_x as f32;
                    let y = y_px as f32;

                    // Tick line (rightward)
                    line_source_positions.push([x, y]);
                    line_target_positions.push([x + DEFAULT_TICK_SIZE as f32, y]);

                    // Label position
                    text_positions.push([x + (DEFAULT_TICK_SIZE + DEFAULT_TICK_PADDING) as f32, y]);
                    text_strings.push(tick.clone());
                }
            }
        }

        // Text alignment/baseline based on position
        let (text_align_mode, text_baseline_mode, text_rotation) = match self.layer_params.position {
            AxisPosition::Bottom => (TextAlignMode::Start, TextBaselineMode::Middle, 90.0),
            AxisPosition::Top => (TextAlignMode::Start, TextBaselineMode::Middle, -90.0),
            AxisPosition::Left => (TextAlignMode::End, TextBaselineMode::Middle, 0.0),
            AxisPosition::Right => (TextAlignMode::Start, TextBaselineMode::Middle, 0.0),
        };

        let mut sublayers: Vec<Box<dyn PreparedAndDraw>> = Vec::new();

        // Zero margins for sublayers since this layer handles positioning itself
        let sublayer_bounds = MarginParams {
            margin_top: Some(0.0_f32),
            margin_right: Some(0.0_f32),
            margin_bottom: Some(0.0_f32),
            margin_left: Some(0.0_f32),
        };

        let line_source_position_x = line_source_positions.iter().map(|pos| pos[0]).collect();
        let line_source_position_y = line_source_positions.iter().map(|pos| pos[1]).collect();
        let line_target_position_x = line_target_positions.iter().map(|pos| pos[0]).collect();
        let line_target_position_y = line_target_positions.iter().map(|pos| pos[1]).collect();
        // LineLayer for axis line and ticks
        let line_params = LineLayerParams {
            layer_id: format!("{}_axis_band_layer_line_sublayer", self.layer_params.layer_id),
            bounds: Some(sublayer_bounds.clone()),
            data_unit_mode_x: UnitsMode::Pixels,
            data_unit_mode_y: UnitsMode::Pixels,
            stroke_width: Some(SizeMode::UniformSize(DEFAULT_LINE_WIDTH)),
            stroke_width_unit_mode: UnitsMode::Pixels,
            stroke_opacity: None,
            model_matrix: None,
            stroke_color: None,
            source_position_x: NumericData::Float32(Arc::new(line_source_position_x)),
            source_position_y: NumericData::Float32(Arc::new(line_source_position_y)),
            target_position_x: NumericData::Float32(Arc::new(line_target_position_x)),
            target_position_y: NumericData::Float32(Arc::new(line_target_position_y)),
            ..Default::default()
        };
        sublayers.push(Box::new(LineLayer::new(
            self.view_params.clone(),
            line_params,
        )));

        sublayers.extend(self.build_title_sublayer(sublayer_bounds.clone()));

        // TextLayer for tick labels
        if !text_strings.is_empty() {
            let text_position_x = text_positions.iter().map(|pos| pos[0]).collect();
            let text_position_y = text_positions.iter().map(|pos| pos[1]).collect();

            let text_params = TextLayerParams {
                layer_id: format!("{}_axis_band_layer_text_sublayer", self.layer_params.layer_id),
                bounds: Some(sublayer_bounds),
                data_unit_mode_x: UnitsMode::Pixels,
                data_unit_mode_y: UnitsMode::Pixels,
                text_size: DEFAULT_FONT_SIZE as f32,
                text_size_unit_mode: UnitsMode::Pixels,
                text_align_mode,
                text_baseline_mode,
                text_rotation: Some(text_rotation as f32),
                model_matrix: None,
                font_family: None,
                font_weight: FontWeight::Normal,
                font_style: FontStyle::Normal,
                fill_color: None,
                position_x: NumericData::Float32(Arc::new(text_position_x)),
                position_y: NumericData::Float32(Arc::new(text_position_y)),
                text_vec: Arc::new(text_strings),
                ..Default::default()
            };
            sublayers.push(Box::new(TextLayer::new(
                self.view_params.clone(),
                text_params,
            )));
        }

        sublayers
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for AxisBandLayer {
    async fn prepare(&mut self, gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        self.sub_layer_instances = self.build_sublayers();
        return base_prepare_composite_layer(&mut self.sub_layer_instances, gpu_context).await;
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for AxisBandLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut GpuRenderPass<'_>) {
        base_draw_composite_layer(&self.sub_layer_instances, gpu_context, pass).await;
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for AxisBandLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for AxisBandLayer {
    async fn draw(&self, ctx: &mut SvgContext) {
        base_draw_composite_layer_svg(&self.sub_layer_instances, ctx).await
    }
}

impl BrushableLayer for AxisBandLayer {}

impl ExtentableLayer for AxisBandLayer {}

impl PickableLayer for AxisBandLayer {}

inventory::submit! {
    crate::registry::LayerRegistration {
        layer_type_name: "AxisBandLayer",
        create_layer: |value, view_params| {
            let params: AxisBandLayerParams = serde_json::from_value(value).unwrap();
            Box::new(AxisBandLayer::new(view_params.clone(), params))
        },
    }
}
