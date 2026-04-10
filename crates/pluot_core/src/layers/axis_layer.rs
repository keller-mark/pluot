use serde::{Deserialize, Serialize};

use std::sync::Arc;
use crate::render_traits::{DrawToRasterGpu, DrawToRasterCpu, DrawToSvg, PickableLayer, PreparedLayer, ViewParams, PreparedAndDraw, MarginParams, UnitsMode};
use crate::viewport::get_bounds;
use crate::layers::composite_layer::{base_draw_composite_layer, base_draw_composite_layer_svg, base_prepare_composite_layer};
use crate::two::svg::SvgContext;
use crate::layers::text_layer::{TextLayer, TextLayerParams, TextAlignMode, TextBaselineMode};
use crate::layers::line_layer::{LineLayer, LineLayerParams};
use crate::render_types::{CpuContext, CpuRenderPass, PrepareResult, RenderResult};
use crate::render_types::GpuContext;
use crate::wgpu;
use crate::d3::scale::{LinearRangeable, ScaleLinear, Tickable, Scaleable};

// TODO: make these configurable via AxisLinearLayerParams
const DEFAULT_TICK_COUNT: usize = 10;
const DEFAULT_TICK_SIZE: f64 = 6.0;
const DEFAULT_TICK_PADDING: f64 = 3.0;
const DEFAULT_FONT_SIZE: f64 = 12.0;
const DEFAULT_LINE_WIDTH: f32 = 1.0;
const DEFAULT_AXIS_COLOR: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
const DEFAULT_LABEL_COLOR: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum AxisPosition {
    Top,
    Right,
    Bottom,
    Left,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AxisLinearLayerParams {
    pub layer_id: String,
    pub position: AxisPosition,
}

pub struct AxisLinearLayer {
    view_params: ViewParams,
    layer_params: AxisLinearLayerParams,
    sub_layer_instances: Vec<Box<dyn PreparedAndDraw>>,
}

impl AxisLinearLayer {
    pub fn new(view_params: ViewParams, layer_params: AxisLinearLayerParams) -> Self {
        Self {
            view_params,
            layer_params,
            sub_layer_instances: Vec::new(),
        }
    }

    /// Build the sublayers (line layer for axis/ticks, text layer for labels)
    fn build_sublayers(&self) -> Vec<Box<dyn PreparedAndDraw>> {

        let bounds = &self.view_params.margins;

        let margin_top = if let Some(margin_params) = &bounds {
            margin_params.margin_top.unwrap_or(0.0)
        } else { 0.0 } as f64;
        let margin_right = if let Some(margin_params) = &bounds {
            margin_params.margin_right.unwrap_or(0.0)
        } else { 0.0 } as f64;
        let margin_bottom = if let Some(margin_params) = &bounds {
            margin_params.margin_bottom.unwrap_or(0.0)
        } else { 0.0 } as f64;
        let margin_left = if let Some(margin_params) = &bounds {
            margin_params.margin_left.unwrap_or(0.0)
        } else { 0.0 } as f64;

        let viewport_w = self.view_params.width as f64;
        let viewport_h = self.view_params.height as f64;


        let b = get_bounds(&self.view_params);
        let (min_x, max_x, min_y, max_y) = (b.x_min as f64, b.x_max as f64, b.y_min as f64, b.y_max as f64);

        let mut line_source_positions: Vec<[f32; 2]> = Vec::new();
        let mut line_target_positions: Vec<[f32; 2]> = Vec::new();
        let mut text_positions: Vec<[f32; 2]> = Vec::new();
        let mut text_strings: Vec<String> = Vec::new();

        match self.layer_params.position {
            AxisPosition::Bottom => {
                let mut scale = ScaleLinear::new();
                scale.set_domain((min_x, max_x));
                scale.set_range((margin_left, viewport_w - margin_right));

                let ticks = scale.ticks(None);
                // The pixel-based coordinate system has y=0 at the bottom.
                let axis_y = margin_bottom;

                // Main axis line
                line_source_positions.push([margin_left as f32, axis_y as f32]);
                line_target_positions.push([(viewport_w - margin_right) as f32, axis_y as f32]);

                // Tick marks and labels
                for tick in &ticks {
                    let x = scale.scale(tick) as f32;
                    let y = axis_y as f32;

                    // Tick line
                    line_source_positions.push([x, y]);
                    line_target_positions.push([x, y - DEFAULT_TICK_SIZE as f32]);

                    // Label position
                    text_positions.push([x, y - (DEFAULT_TICK_SIZE + DEFAULT_TICK_PADDING) as f32]);
                    text_strings.push(format_tick_value(*tick));
                }
            }
            AxisPosition::Top => {
                let mut scale = ScaleLinear::new();
                scale.set_domain((min_x, max_x));
                scale.set_range((margin_left, viewport_w - margin_right));

                let ticks = scale.ticks(None);
                let axis_y = viewport_h - margin_top;

                // Main axis line
                line_source_positions.push([margin_left as f32, axis_y as f32]);
                line_target_positions.push([(viewport_w - margin_right) as f32, axis_y as f32]);

                // Tick marks and labels
                for tick in &ticks {
                    let x = scale.scale(tick) as f32;
                    let y = axis_y as f32;

                    // Tick line (upward)
                    line_source_positions.push([x, y]);
                    line_target_positions.push([x, y + DEFAULT_TICK_SIZE as f32]);

                    // Label position
                    text_positions.push([x, y + (DEFAULT_TICK_SIZE + DEFAULT_TICK_PADDING) as f32]);
                    text_strings.push(format_tick_value(*tick));
                }
            }
            AxisPosition::Left => {
                let mut scale = ScaleLinear::new();
                scale.set_domain((min_y, max_y));
                scale.set_range((margin_bottom, viewport_h - margin_top)); // TODO: verify lack of inversion here

                let ticks = scale.ticks(None);
                let axis_x = margin_left;

                // Main axis line
                line_source_positions.push([axis_x as f32, margin_bottom as f32]);
                line_target_positions.push([axis_x as f32, (viewport_h - margin_top) as f32]);

                // Tick marks and labels
                for tick in &ticks {
                    let x = axis_x as f32;
                    let y = scale.scale(tick) as f32;

                    // Tick line (leftward)
                    line_source_positions.push([x, y]);
                    line_target_positions.push([x - DEFAULT_TICK_SIZE as f32, y]);

                    // Label position
                    text_positions.push([x - (DEFAULT_TICK_SIZE + DEFAULT_TICK_PADDING) as f32, y]);
                    text_strings.push(format_tick_value(*tick));
                }
            }
            AxisPosition::Right => {
                let mut scale = ScaleLinear::new();
                scale.set_domain((min_y, max_y));
                scale.set_range((margin_bottom, viewport_h - margin_top)); // TODO: verify lack of inversion here

                let ticks = scale.ticks(None);
                let axis_x = viewport_w - margin_right;

                // Main axis line
                line_source_positions.push([axis_x as f32, margin_bottom as f32]);
                line_target_positions.push([axis_x as f32, (viewport_h - margin_top) as f32]);

                // Tick marks and labels
                for tick in &ticks {
                    let x = axis_x as f32;
                    let y = scale.scale(tick) as f32;

                    // Tick line (rightward)
                    line_source_positions.push([x, y]);
                    line_target_positions.push([x + DEFAULT_TICK_SIZE as f32, y]);

                    // Label position
                    text_positions.push([x + (DEFAULT_TICK_SIZE + DEFAULT_TICK_PADDING) as f32, y]);
                    text_strings.push(format_tick_value(*tick));
                }
            }
        }

        // Determine text anchor and baseline based on position.
        // TODO: rotation here for top/bottom axes.
        let (text_align_mode, text_baseline_mode, text_rotation) = match self.layer_params.position {
            AxisPosition::Bottom => (TextAlignMode::Start, TextBaselineMode::Middle, 90.0),
            AxisPosition::Top => (TextAlignMode::Start, TextBaselineMode::Middle, -90.0),
            AxisPosition::Left => (TextAlignMode::End, TextBaselineMode::Middle, 0.0),
            AxisPosition::Right => (TextAlignMode::Start, TextBaselineMode::Middle, 0.0),
        };

        let mut sublayers: Vec<Box<dyn PreparedAndDraw>> = Vec::new();

        // We use zero margins for the sublayers,
        // as the AxisLinearLayer itself is responsible for positioning,
        // and it will need to render things into the margins.
        let bounds = MarginParams {
            margin_top: Some(0.0_f32),
            margin_right: Some(0.0_f32),
            margin_bottom: Some(0.0_f32),
            margin_left: Some(0.0_f32),
        };

        let line_source_position_x = line_source_positions.iter().map(|pos| pos[0]).collect();
        let line_source_position_y = line_source_positions.iter().map(|pos| pos[1]).collect();

        let line_target_position_x = line_target_positions.iter().map(|pos| pos[0]).collect();
        let line_target_position_y = line_target_positions.iter().map(|pos| pos[1]).collect();

        let line_labels_vec: Vec<i32> = line_source_positions.iter().map(|_| 0_i32).collect();

        // Create LineLayer for axis line and ticks
        let line_params = LineLayerParams {
            layer_id: format!("{}_axis_layer_line_sublayer", self.layer_params.layer_id),
            bounds: Some(bounds.clone()),
            data_unit_mode: UnitsMode::Pixels,
            line_width: DEFAULT_LINE_WIDTH,
            line_width_unit_mode: UnitsMode::Pixels,
            source_position_x: Arc::new(line_source_position_x),
            source_position_y: Arc::new(line_source_position_y),
            target_position_x: Arc::new(line_target_position_x),
            target_position_y: Arc::new(line_target_position_y),
            labels_vec: line_labels_vec, // TODO: make this optional in LineLayerParams
        };
        sublayers.push(Box::new(LineLayer::new(
            self.view_params.clone(),
            line_params,
        )));

        // Create TextLayer for tick labels
        if !text_strings.is_empty() {
            let text_position_x = text_positions.iter().map(|pos| pos[0]).collect();
            let text_position_y = text_positions.iter().map(|pos| pos[1]).collect();

            let text_params = TextLayerParams {
                layer_id: format!("{}_axis_layer_text_sublayer", self.layer_params.layer_id),
                bounds: Some(bounds),
                data_unit_mode: UnitsMode::Pixels,
                text_size: DEFAULT_FONT_SIZE as f32,
                text_size_unit_mode: UnitsMode::Pixels,
                text_align_mode,
                text_baseline_mode,
                text_rotation: Some(text_rotation as f32),

                position_x: Arc::new(text_position_x),
                position_y: Arc::new(text_position_y),
                text_vec: Arc::new(text_strings),
            };
            sublayers.push(Box::new(TextLayer::new(
                self.view_params.clone(),
                text_params,
            )));
        }

        sublayers
    }
}

// Format a tick value for display
// TODO: do we already have this in the axis.rs module?
fn format_tick_value(value: f64) -> String {
    if value.abs() < 1e-10 {
        "0".to_string()
    } else if value.abs() >= 1e6 || (value.abs() < 1e-3 && value != 0.0) {
        format!("{:.2e}", value)
    } else if value.fract().abs() < 1e-10 {
        format!("{:.0}", value)
    } else {
        format!("{:.2}", value)
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for AxisLinearLayer {
    async fn prepare(&mut self, gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        // Build sublayers based on current view params
        self.sub_layer_instances = self.build_sublayers();

        // Prepare all sublayers
        return base_prepare_composite_layer(&mut self.sub_layer_instances, gpu_context).await;
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for AxisLinearLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        base_draw_composite_layer(&self.sub_layer_instances, gpu_context, pass).await;
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for AxisLinearLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for AxisLinearLayer {
    async fn draw(&self, ctx: &mut SvgContext) {
        base_draw_composite_layer_svg(&self.sub_layer_instances, ctx).await
    }
}

inventory::submit! {
    crate::registry::LayerRegistration {
        layer_type_name: "AxisLinearLayer",
        create_layer: |value, view_params| {
            let params: AxisLinearLayerParams = serde_json::from_value(value).unwrap();
            Box::new(AxisLinearLayer::new(view_params.clone(), params))
        },
    }
}

impl PickableLayer for AxisLinearLayer {}
