// A layer that renders a legend for a quantitative colormap.
//
// We internally render the colormap gradient using BitmapLayer.
// We interpolate 256 values between 0 and 1.
// References:
// - https://github.com/vitessce/vitessce/blob/9f8f37f16e9cb15b911156dcfa1de15050a95165/packages/legend/src/legend-utils.ts#L46
//
// In Horizontal orientation (the default), the gradient is rendered as a wide rectangle,
// with zero to one from left to right. This layer also renders a text layer with a title
// (positioned above the gradient), as well as a linear axis layer from 0.0 to 1.0 with
// 2 ticks (positioned below the gradient, with AxisPosition::Bottom).
//
// In Vertical orientation, the gradient is rendered as a tall rectangle, with zero to one
// from bottom to top. The title is positioned above the gradient, and the axis is
// positioned to the right of the gradient, with AxisPosition::Right.
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::colormaps_quantitative::sample;
use crate::composite_layer::{base_draw_composite_layer, base_draw_composite_layer_svg, base_prepare_composite_layer};
use crate::composite_layers::axis_linear_layer::{format_tick_value, AxisLinearLayer, AxisLinearLayerParams, AxisPosition};
use crate::d3::scale::ScaleLinear;
use crate::layers::bitmap_layer::{BitmapLayer, BitmapLayerParams, ChannelSettings, DimensionOrder};
use crate::layers::text_layer::{TextAlignMode, TextBaselineMode, TextLayer, TextLayerParams};
use crate::numeric_data::NumericData;
use crate::render_traits::{
    AspectRatioMode, MarginParams, PickableLayer, PreparedAndDraw, PreparedLayer, QuantitativeColormap, UnitsMode, ViewParams,
};
use crate::render_types::GpuContext;
use crate::render_types::{CpuContext, CpuRenderPass, PrepareResult};
use crate::render_traits::{DrawToRasterCpu, DrawToRasterGpu, DrawToSvg};
use crate::two::svg::SvgContext;
use crate::wgpu;

/// Number of gradient steps interpolated across the colormap's domain (0.0 to 1.0).
const GRADIENT_RESOLUTION: u32 = 256;

const DEFAULT_TITLE_FONT_SIZE: f32 = 12.0;
/// Vertical space reserved above the gradient bar for the title text.
const DEFAULT_TITLE_HEIGHT_PX: f32 = 16.0;
/// Thickness (in pixels) of the gradient color bar itself.
const DEFAULT_GRADIENT_HEIGHT_PX: f32 = 14.0;
/// Gap (in pixels) between the bottom of the gradient bar and the axis line.
const DEFAULT_GRADIENT_AXIS_GAP_PX: f32 = 2.0;

/// Determines whether legend should be vertical (left to right colormap)
/// or horizontal (bottom to top colormap).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum LegendOrientation {
    Vertical,
    Horizontal,
}

/// Layer params struct for [`LegendColormapQuantitativeLayer`].
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct LegendColormapQuantitativeLayerParams {
    pub layer_id: String,
    /// Margins defining the legend's overall bounding box within the viewport.
    pub bounds: Option<MarginParams>,
    /// Title text rendered above the gradient bar.
    pub title: String,
    /// The named quantitative colormap to render as a gradient.
    pub colormap: QuantitativeColormap,
    /// Determines whether the colormap should be reversed (by subtracting `1 - t` before
    /// executing the colormap function). By default, false.
    pub reverse: bool,
    /// A scale whose domain provides the data values the gradient's left and right ends
    /// represent, used only to label the axis ticks. When None, the ticks are labeled "0"
    /// and "1" (i.e. the colormap's own domain). This has no effect on the rendered
    /// gradient itself, which always spans the full colormap.
    pub scale: Option<ScaleLinear>,

    pub orientation: LegendOrientation,
}

impl Default for LegendColormapQuantitativeLayerParams {
    fn default() -> Self {
        Self {
            layer_id: "".to_string(),
            bounds: None,
            title: "".to_string(),
            colormap: QuantitativeColormap::Viridis,
            reverse: false,
            scale: None,
            orientation: LegendOrientation::Horizontal,
        }
    }
}

pub struct LegendColormapQuantitativeLayer {
    view_params: ViewParams,
    layer_params: LegendColormapQuantitativeLayerParams,
    sub_layer_instances: Vec<Box<dyn PreparedAndDraw>>,
}

impl LegendColormapQuantitativeLayer {
    pub fn new(view_params: ViewParams, layer_params: LegendColormapQuantitativeLayerParams) -> Self {
        Self {
            view_params,
            layer_params,
            sub_layer_instances: Vec::new(),
        }
    }

    /// Build a CYX (RGB) gradient image spanning the colormap's domain (0.0 to 1.0),
    /// positioned (in pixel coordinates) to span [x0, x1] horizontally and [y0, y1] vertically.
    ///
    /// When `vertical` is false, the gradient varies from `x0` (t=0.0) to `x1` (t=1.0)
    /// along a single row. When `vertical` is true, it varies from `y0` (t=0.0) to
    /// `y1` (t=1.0) along a single column, matching the bottom-to-top convention used
    /// by the AxisPosition::Right axis (tick 0.0 at the bottom, tick 1.0 at the top).
    fn build_gradient_bitmap_params(&self, x0: f32, x1: f32, y0: f32, y1: f32, vertical: bool) -> BitmapLayerParams {
        let n = GRADIENT_RESOLUTION as usize;
        let mut data = vec![0u8; 3 * n];
        for i in 0..n {
            let physical_t = i as f32 / (n - 1) as f32;
            let color_t = if self.layer_params.reverse { 1.0 - physical_t } else { physical_t };
            let rgba = sample(self.layer_params.colormap, color_t);
            // The underlying image array is stored top-to-bottom (see BitmapLayer),
            // so for a vertical bar we reverse the slot order to put t=0.0 at the
            // bottom (last row) and t=1.0 at the top (first row).
            let slot = if vertical { n - 1 - i } else { i };
            data[slot] = (rgba[0] * 255.0).round() as u8;
            data[n + slot] = (rgba[1] * 255.0).round() as u8;
            data[2 * n + slot] = (rgba[2] * 255.0).round() as u8;
        }

        let (img_w, img_h) = if vertical { (1.0_f32, n as f32) } else { (n as f32, 1.0_f32) };
        let shape = if vertical { vec![3, n as u32, 1] } else { vec![3, 1, n as u32] };
        let w = x1 - x0;
        let h = y1 - y0;

        BitmapLayerParams {
            layer_id: format!("{}_legend_colormap_quantitative_layer_gradient_sublayer", self.layer_params.layer_id),
            bounds: Some(MarginParams {
                margin_top: Some(0.0),
                margin_right: Some(0.0),
                margin_bottom: Some(0.0),
                margin_left: Some(0.0),
            }),
            data_unit_mode_x: UnitsMode::Pixels,
            data_unit_mode_y: UnitsMode::Pixels,
            pixel_offset: None,
            // The image is stored at its native pixel size; this model_matrix
            // scales and translates it (in the layer's pixel space) to exactly span
            // [x0, x1] x [y0, y1], regardless of the source image's pixel dimensions.
            model_matrix: Some([
                w / img_w, 0.0, 0.0, 0.0,
                0.0, h / img_h, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                x0, y0, 0.0, 1.0,
            ]),
            dimension_order: DimensionOrder::CYX,
            shape,
            channel_settings: vec![
                ChannelSettings { window: (0.0, 255.0), color: (1.0, 0.0, 0.0) },
                ChannelSettings { window: (0.0, 255.0), color: (0.0, 1.0, 0.0) },
                ChannelSettings { window: (0.0, 255.0), color: (0.0, 0.0, 1.0) },
            ],
            opacity: 1.0,
            data: NumericData::Uint8(Arc::new(data)),
        }
    }

    /// Build the sublayers: title text, gradient bitmap, and a 0.0-to-1.0 linear axis.
    fn build_sublayers(&self) -> Vec<Box<dyn PreparedAndDraw>> {
        let bounds = &self.layer_params.bounds;
        let margin_top = bounds.as_ref().and_then(|m| m.margin_top).unwrap_or(0.0) as f64;
        let margin_right = bounds.as_ref().and_then(|m| m.margin_right).unwrap_or(0.0) as f64;
        let margin_bottom = bounds.as_ref().and_then(|m| m.margin_bottom).unwrap_or(0.0) as f64;
        let margin_left = bounds.as_ref().and_then(|m| m.margin_left).unwrap_or(0.0) as f64;

        let viewport_w = self.view_params.width as f64;
        let viewport_h = self.view_params.height as f64;

        let legend_left = margin_left;
        let legend_right = viewport_w - margin_right;
        let legend_top = viewport_h - margin_top;
        let legend_bottom = margin_bottom;

        // TODO: use ScaleLinear and ScaleBand to clean up all the positioning arithmetic

        let title_y = legend_top;

        let zero_bounds = MarginParams {
            margin_top: Some(0.0),
            margin_right: Some(0.0),
            margin_bottom: Some(0.0),
            margin_left: Some(0.0),
        };

        let mut sublayers: Vec<Box<dyn PreparedAndDraw>> = Vec::new();

        if !self.layer_params.title.is_empty() {
            sublayers.push(Box::new(TextLayer::new(
                self.view_params.clone(),
                TextLayerParams {
                    layer_id: format!("{}_legend_colormap_quantitative_layer_title_sublayer", self.layer_params.layer_id),
                    bounds: Some(zero_bounds.clone()),
                    data_unit_mode_x: UnitsMode::Pixels,
                    data_unit_mode_y: UnitsMode::Pixels,
                    text_size: DEFAULT_TITLE_FONT_SIZE,
                    text_size_unit_mode: UnitsMode::Pixels,
                    text_align_mode: TextAlignMode::Start,
                    text_baseline_mode: TextBaselineMode::Top,
                    position_x: NumericData::Float32(Arc::new(vec![legend_left as f32])),
                    position_y: NumericData::Float32(Arc::new(vec![title_y as f32])),
                    text_vec: Arc::new(vec![self.layer_params.title.clone()]),
                    ..Default::default()
                },
            )));
        }

        let tick_labels = self.layer_params.scale.as_ref().map(|scale| {
            let (lo, hi) = scale.get_domain();
            vec![format_tick_value(lo), format_tick_value(hi)]
        });

        match self.layer_params.orientation {
            LegendOrientation::Horizontal => {
                // Stack, from top to bottom: title text, gradient bar, axis (line + tick labels).
                let gradient_top = title_y - DEFAULT_TITLE_HEIGHT_PX as f64;
                let gradient_bottom = gradient_top - DEFAULT_GRADIENT_HEIGHT_PX as f64;
                let axis_y = gradient_bottom - DEFAULT_GRADIENT_AXIS_GAP_PX as f64;

                sublayers.push(Box::new(BitmapLayer::new(
                    self.view_params.clone(),
                    self.build_gradient_bitmap_params(
                        legend_left as f32,
                        legend_right as f32,
                        gradient_bottom as f32,
                        gradient_top as f32,
                        false,
                    ),
                )));

                // A fresh identity-camera, aspect-ratio-ignoring ViewParams makes the axis's
                // visible domain come out to exactly (0.0, 1.0) (see `viewport::get_bounds`),
                // matching the gradient's domain regardless of the parent view's camera state.
                let axis_view_params = ViewParams {
                    camera_view: None,
                    aspect_ratio_mode: AspectRatioMode::Ignore,
                    margins: Some(MarginParams {
                        margin_left: Some(legend_left as f32),
                        margin_right: Some((viewport_w - legend_right) as f32),
                        margin_top: Some((viewport_h - gradient_top) as f32),
                        margin_bottom: Some(axis_y as f32),
                    }),
                    ..self.view_params.clone()
                };
                sublayers.push(Box::new(AxisLinearLayer::new(
                    axis_view_params,
                    AxisLinearLayerParams {
                        layer_id: format!("{}_legend_colormap_quantitative_layer_axis_sublayer", self.layer_params.layer_id),
                        position: AxisPosition::Bottom,
                        tick_values: Some(vec![0.0, 1.0]),
                        tick_labels,
                        // TODO: also ensure that the ticks are not hanging over by ensuring the lower tick uses the bottom text-alignment-baseline and the upper tick uses the top text-alignment-baseline
                        // See fit_outer_ticks and implement this logic in AxisLinearLayer
                        ..Default::default()
                    },
                )));
            }
            LegendOrientation::Vertical => {
                // Stack, from left to right: gradient bar (title above it), axis (line + tick labels).
                let gradient_top = title_y - DEFAULT_TITLE_HEIGHT_PX as f64;
                let gradient_bottom = legend_bottom;
                let gradient_left = legend_left;
                let gradient_right = gradient_left + DEFAULT_GRADIENT_HEIGHT_PX as f64;
                let axis_x = gradient_right + DEFAULT_GRADIENT_AXIS_GAP_PX as f64;

                sublayers.push(Box::new(BitmapLayer::new(
                    self.view_params.clone(),
                    self.build_gradient_bitmap_params(
                        gradient_left as f32,
                        gradient_right as f32,
                        gradient_bottom as f32,
                        gradient_top as f32,
                        true,
                    ),
                )));

                // A fresh identity-camera, aspect-ratio-ignoring ViewParams makes the axis's
                // visible domain come out to exactly (0.0, 1.0) (see `viewport::get_bounds`),
                // matching the gradient's domain regardless of the parent view's camera state.
                let axis_view_params = ViewParams {
                    camera_view: None,
                    aspect_ratio_mode: AspectRatioMode::Ignore,
                    margins: Some(MarginParams {
                        margin_left: None,
                        margin_right: Some((viewport_w - axis_x) as f32),
                        margin_top: Some((viewport_h - gradient_top) as f32),
                        margin_bottom: Some(gradient_bottom as f32),
                    }),
                    ..self.view_params.clone()
                };
                sublayers.push(Box::new(AxisLinearLayer::new(
                    axis_view_params,
                    AxisLinearLayerParams {
                        layer_id: format!("{}_legend_colormap_quantitative_layer_axis_sublayer", self.layer_params.layer_id),
                        position: AxisPosition::Right,
                        tick_values: Some(vec![0.0, 1.0]),
                        tick_labels,
                        ..Default::default()
                    },
                )));
            }
        }

        sublayers
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for LegendColormapQuantitativeLayer {
    async fn prepare(&mut self, gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        self.sub_layer_instances = self.build_sublayers();
        base_prepare_composite_layer(&mut self.sub_layer_instances, gpu_context).await
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for LegendColormapQuantitativeLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        base_draw_composite_layer(&self.sub_layer_instances, gpu_context, pass).await;
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for LegendColormapQuantitativeLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for LegendColormapQuantitativeLayer {
    async fn draw(&self, ctx: &mut SvgContext) {
        base_draw_composite_layer_svg(&self.sub_layer_instances, ctx).await
    }
}

inventory::submit! {
    crate::registry::LayerRegistration {
        layer_type_name: "LegendColormapQuantitativeLayer",
        create_layer: |value, view_params| {
            let params: LegendColormapQuantitativeLayerParams = serde_json::from_value(value).unwrap();
            Box::new(LegendColormapQuantitativeLayer::new(view_params.clone(), params))
        },
    }
}

impl PickableLayer for LegendColormapQuantitativeLayer {}
