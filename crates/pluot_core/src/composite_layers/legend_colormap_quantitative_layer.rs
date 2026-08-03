// A layer that renders a legend for a quantitative colormap.
//
// We internally render the colormap gradient using BitmapLayer.
// We interpolate 256 values between 0 and 1.
// References:
// - https://github.com/vitessce/vitessce/blob/9f8f37f16e9cb15b911156dcfa1de15050a95165/packages/legend/src/legend-utils.ts#L46
//
// The gradient is rendered as a wide rectangle, with zero to one from left to right.
// This layer also renders a text layer with a title (positioned above the gradient),
// as well as a linear axis layer from 0.0 to 1.0 with 2 ticks (positioned below the gradient, with AxisPosition::Bottom).
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::colormaps_quantitative::sample;
use crate::composite_layer::{base_draw_composite_layer, base_draw_composite_layer_svg, base_prepare_composite_layer};
use crate::composite_layers::axis_linear_layer::{format_tick_value, AxisLinearLayer, AxisLinearLayerParams, AxisPosition};
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
    /// The (min, max) data values the gradient's left and right ends represent, used only
    /// to label the axis ticks. When None, the ticks are labeled "0" and "1" (i.e. the
    /// colormap's own domain). This has no effect on the rendered gradient itself, which
    /// always spans the full colormap.
    pub domain: Option<(f32, f32)>,
}

impl Default for LegendColormapQuantitativeLayerParams {
    fn default() -> Self {
        Self {
            layer_id: "".to_string(),
            bounds: None,
            title: "".to_string(),
            colormap: QuantitativeColormap::Viridis,
            reverse: false,
            domain: None,
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
    fn build_gradient_bitmap_params(&self, x0: f32, x1: f32, y0: f32, y1: f32) -> BitmapLayerParams {
        let n = GRADIENT_RESOLUTION as usize;
        let mut data = vec![0u8; 3 * n];
        for i in 0..n {
            let mut t = i as f32 / (n - 1) as f32;
            if self.layer_params.reverse {
                t = 1.0 - t;
            }
            let rgba = sample(self.layer_params.colormap, t);
            data[i] = (rgba[0] * 255.0).round() as u8;
            data[n + i] = (rgba[1] * 255.0).round() as u8;
            data[2 * n + i] = (rgba[2] * 255.0).round() as u8;
        }

        let img_w = n as f32;
        let img_h = 1.0_f32;
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
            // The image is stored at its native 256x1 pixel size; this model_matrix
            // scales and translates it (in the layer's pixel space) to exactly span
            // [x0, x1] x [y0, y1], regardless of the source image's pixel dimensions.
            model_matrix: Some([
                w / img_w, 0.0, 0.0, 0.0,
                0.0, h / img_h, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                x0, y0, 0.0, 1.0,
            ]),
            dimension_order: DimensionOrder::CYX,
            shape: vec![3, 1, n as u32],
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

        // Stack, from top to bottom: title text, gradient bar, axis (line + tick labels).
        let title_y = legend_top;
        let gradient_top = title_y - DEFAULT_TITLE_HEIGHT_PX as f64;
        let gradient_bottom = gradient_top - DEFAULT_GRADIENT_HEIGHT_PX as f64;
        let axis_y = gradient_bottom - DEFAULT_GRADIENT_AXIS_GAP_PX as f64;

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

        sublayers.push(Box::new(BitmapLayer::new(
            self.view_params.clone(),
            self.build_gradient_bitmap_params(
                legend_left as f32,
                legend_right as f32,
                gradient_bottom as f32,
                gradient_top as f32,
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
        let tick_labels = self.layer_params.domain.map(|(lo, hi)| {
            vec![format_tick_value(lo as f64), format_tick_value(hi as f64)]
        });
        sublayers.push(Box::new(AxisLinearLayer::new(
            axis_view_params,
            AxisLinearLayerParams {
                layer_id: format!("{}_legend_colormap_quantitative_layer_axis_sublayer", self.layer_params.layer_id),
                position: AxisPosition::Bottom,
                tick_values: Some(vec![0.0, 1.0]),
                tick_labels,
                // TODO: also ensure that the ticks are not hanging over by ensuring the lower tick uses the bottom text-alignment-baseline and the upper tick uses the top text-alignment-baseline
            },
        )));

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
