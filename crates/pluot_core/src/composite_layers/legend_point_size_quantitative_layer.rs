// A layer that renders a legend for the point sizes of a point layer.
//
// Internally, this layer renders a single PointLayer instance containing `num_steps`
// points with linearly increasing radii (from `radius_range.0` to `radius_range.1`),
// plus a TextLayer with the corresponding data values (linearly interpolated across
// `domain`) labeling each point. It also renders a legend title text element above the
// legend items.
//
// This layer is used to render a legend for the point sizes of the AdataZarrDotPlotLayer,
// whose dots are sized by `fraction_expressing * max_dot_radius` -- a linear map from a
// [0, 1] domain to a [0, max_dot_radius] pixel range -- the same kind of mapping this
// legend's `scale` (domain -> radius) describes.
//
// In Horizontal orientation (the default), the points are laid out in a row, in
// increasing size from left to right, all vertically centered on a shared line. The
// title is positioned above the row, and each point's value label is centered
// underneath it.
//
// In Vertical orientation, the points are laid out in a column, in increasing size
// from bottom to top (mirroring the bottom-to-top domain convention used by
// LegendColormapQuantitativeLayer's Vertical orientation), all horizontally centered on
// a shared line. The title is positioned above the column, and each point's value label
// is placed to the right of it, vertically centered.
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::composite_layer::{base_draw_composite_layer, base_draw_composite_layer_svg, base_prepare_composite_layer};
use crate::composite_layers::axis_linear_layer::format_tick_value;
use crate::composite_layers::legend_colormap_quantitative_layer::LegendOrientation;
use crate::d3::scale::{ScaleLinear, Scaleable};
use crate::layers::point_layer::{PointLayer, PointLayerParams, PointShapeMode};
use crate::layers::text_layer::{TextAlignMode, TextBaselineMode, TextLayer, TextLayerParams};
use crate::numeric_data::NumericData;
use crate::render_traits::{
    ColorMode, InstancedSizeParams, MarginParams, PickableLayer, PreparedAndDraw, PreparedLayer, SizeMode, UnitsMode, ViewParams,
};
use crate::render_types::GpuContext;
use crate::render_types::{CpuContext, CpuRenderPass, PrepareResult};
use crate::render_traits::{DrawToRasterCpu, DrawToRasterGpu, DrawToSvg};
use crate::two::svg::SvgContext;
use crate::wgpu;

const DEFAULT_TITLE_FONT_SIZE: f32 = 12.0;
/// Vertical space reserved above the points for the title text.
const DEFAULT_TITLE_HEIGHT_PX: f32 = 16.0;
/// Font size of each point's value label.
const DEFAULT_LABEL_FONT_SIZE: f32 = 12.0;
/// Gap (in pixels) between a point's edge and its value label.
const DEFAULT_LABEL_GAP_PX: f32 = 4.0;
/// Gap (in pixels) between the edges of adjacent points.
const DEFAULT_ITEM_GAP_PX: f32 = 10.0;
/// Fallback fill color for the example points (mid-gray).
const DEFAULT_POINT_COLOR: (u8, u8, u8) = (120, 120, 120);

/// Layer params struct for [`LegendPointSizeQuantitativeLayer`].
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct LegendPointSizeQuantitativeLayerParams {
    pub layer_id: String,
    /// Margins defining the legend's overall bounding box within the viewport.
    pub bounds: Option<MarginParams>,
    /// Title text rendered above the legend points.
    pub title: String,
    /// The scale mapping data values (domain) to point radii in pixels (range). Should
    /// match the linear value-to-radius mapping used by the point layer this legend
    /// documents (e.g. `fraction_expressing * max_dot_radius`).
    pub scale: ScaleLinear,
    /// Number of example points to render, evenly spaced (by linear interpolation) across
    /// `scale`'s domain. Must be at least 2, so both ends of the domain are represented.
    pub num_steps: usize,
    /// Fill color shared by every example point.
    pub point_color: (u8, u8, u8),
    /// Whether to render the example points as circles or squares.
    pub point_shape_mode: PointShapeMode,

    pub orientation: LegendOrientation,
}

impl Default for LegendPointSizeQuantitativeLayerParams {
    fn default() -> Self {
        let mut scale = ScaleLinear::new();
        scale.set_range((2.0, 10.0));
        Self {
            layer_id: "".to_string(),
            bounds: None,
            title: "".to_string(),
            scale,
            num_steps: 4,
            point_color: DEFAULT_POINT_COLOR,
            point_shape_mode: PointShapeMode::Circle,
            orientation: LegendOrientation::Horizontal,
        }
    }
}

pub struct LegendPointSizeQuantitativeLayer {
    view_params: ViewParams,
    layer_params: LegendPointSizeQuantitativeLayerParams,
    sub_layer_instances: Vec<Box<dyn PreparedAndDraw>>,
}

impl LegendPointSizeQuantitativeLayer {
    pub fn new(view_params: ViewParams, layer_params: LegendPointSizeQuantitativeLayerParams) -> Self {
        assert!(
            layer_params.num_steps >= 2,
            "LegendPointSizeQuantitativeLayer num_steps must be >= 2 (got {}), so both ends of the domain are represented",
            layer_params.num_steps,
        );
        Self {
            view_params,
            layer_params,
            sub_layer_instances: Vec::new(),
        }
    }

    /// The `num_steps` (value, radius) pairs to render: `num_steps` values evenly spaced
    /// across `scale`'s domain, each mapped to its radius via `scale`.
    fn steps(&self) -> Vec<(f32, f32)> {
        let (d0, d1) = self.layer_params.scale.get_domain();
        let n = self.layer_params.num_steps;
        (0..n)
            .map(|i| {
                let t = i as f64 / (n - 1) as f64;
                let value = d0 + (d1 - d0) * t;
                let radius = self.layer_params.scale.scale(&value);
                (value as f32, radius as f32)
            })
            .collect()
    }

    /// Build the sublayers: title text, one PointLayer instance per step, and one
    /// value label per step.
    fn build_sublayers(&self) -> Vec<Box<dyn PreparedAndDraw>> {
        let bounds = &self.layer_params.bounds;
        let margin_top = bounds.as_ref().and_then(|m| m.margin_top).unwrap_or(0.0);
        let margin_left = bounds.as_ref().and_then(|m| m.margin_left).unwrap_or(0.0);

        let viewport_h = self.view_params.height as f32;

        let legend_left = margin_left;
        let legend_top = viewport_h - margin_top;

        // TODO: use ScaleLinear and ScaleBand to clean up all the positioning arithmetic

        let title_y = legend_top;
        let has_title = !self.layer_params.title.is_empty();
        let items_top = if has_title { title_y - DEFAULT_TITLE_HEIGHT_PX } else { title_y };

        let zero_bounds = MarginParams {
            margin_top: Some(0.0),
            margin_right: Some(0.0),
            margin_bottom: Some(0.0),
            margin_left: Some(0.0),
        };

        let mut sublayers: Vec<Box<dyn PreparedAndDraw>> = Vec::new();

        if has_title {
            sublayers.push(Box::new(TextLayer::new(
                self.view_params.clone(),
                TextLayerParams {
                    layer_id: format!("{}_legend_point_size_quantitative_layer_title_sublayer", self.layer_params.layer_id),
                    bounds: Some(zero_bounds.clone()),
                    data_unit_mode_x: UnitsMode::Pixels,
                    data_unit_mode_y: UnitsMode::Pixels,
                    text_size: DEFAULT_TITLE_FONT_SIZE,
                    text_size_unit_mode: UnitsMode::Pixels,
                    text_align_mode: TextAlignMode::Start,
                    text_baseline_mode: TextBaselineMode::Top,
                    position_x: NumericData::Float32(Arc::new(vec![legend_left])),
                    position_y: NumericData::Float32(Arc::new(vec![title_y])),
                    text_vec: Arc::new(vec![self.layer_params.title.clone()]),
                    ..Default::default()
                },
            )));
        }

        let steps = self.steps();
        let max_radius = steps.iter().fold(0.0_f32, |acc, (_, r)| acc.max(*r));

        let mut point_position_x: Vec<f32> = Vec::with_capacity(steps.len());
        let mut point_position_y: Vec<f32> = Vec::with_capacity(steps.len());
        let mut point_radii: Vec<f32> = Vec::with_capacity(steps.len());
        let mut label_position_x: Vec<f32> = Vec::with_capacity(steps.len());
        let mut label_position_y: Vec<f32> = Vec::with_capacity(steps.len());
        let mut label_strings: Vec<String> = Vec::with_capacity(steps.len());

        let (label_align_mode, label_baseline_mode) = match self.layer_params.orientation {
            LegendOrientation::Horizontal => (TextAlignMode::Middle, TextBaselineMode::Top),
            LegendOrientation::Vertical => (TextAlignMode::Start, TextBaselineMode::Middle),
        };

        match self.layer_params.orientation {
            LegendOrientation::Horizontal => {
                // Points laid out left to right, all vertically centered on a shared
                // line positioned so the largest point's top edge touches `items_top`.
                let slot_width = 2.0 * max_radius + DEFAULT_ITEM_GAP_PX;
                let center_y = items_top - max_radius;

                for (i, (value, radius)) in steps.iter().enumerate() {
                    let center_x = legend_left + max_radius + i as f32 * slot_width;
                    point_position_x.push(center_x);
                    point_position_y.push(center_y);
                    point_radii.push(*radius);

                    label_position_x.push(center_x);
                    label_position_y.push(center_y - max_radius - DEFAULT_LABEL_GAP_PX);
                    label_strings.push(format_tick_value(*value as f64));
                }
            }
            LegendOrientation::Vertical => {
                // Points laid out bottom to top (domain-min at the bottom), all
                // horizontally centered on a shared line positioned so the largest
                // point's top edge touches `items_top`.
                let slot_height = 2.0 * max_radius + DEFAULT_ITEM_GAP_PX;
                let center_x = legend_left + max_radius;
                let top_center_y = items_top - max_radius;
                let n = steps.len();

                for (i, (value, radius)) in steps.iter().enumerate() {
                    let center_y = top_center_y - (n - 1 - i) as f32 * slot_height;
                    point_position_x.push(center_x);
                    point_position_y.push(center_y);
                    point_radii.push(*radius);

                    label_position_x.push(center_x + max_radius + DEFAULT_LABEL_GAP_PX);
                    label_position_y.push(center_y);
                    label_strings.push(format_tick_value(*value as f64));
                }
            }
        }

        sublayers.push(Box::new(PointLayer::new(
            self.view_params.clone(),
            PointLayerParams {
                layer_id: format!("{}_legend_point_size_quantitative_layer_points_sublayer", self.layer_params.layer_id),
                bounds: Some(zero_bounds.clone()),
                data_unit_mode_x: UnitsMode::Pixels,
                data_unit_mode_y: UnitsMode::Pixels,
                point_radius_unit_mode_x: UnitsMode::Pixels,
                point_radius_unit_mode_y: UnitsMode::Pixels,
                point_shape_mode: self.layer_params.point_shape_mode,
                point_radius: Some(SizeMode::InstancedSize(InstancedSizeParams {
                    values: NumericData::Float32(Arc::new(point_radii)),
                })),
                fill_color: Some(ColorMode::UniformRgb(self.layer_params.point_color)),
                position_x: NumericData::Float32(Arc::new(point_position_x)),
                position_y: NumericData::Float32(Arc::new(point_position_y)),
                ..Default::default()
            },
        )));

        sublayers.push(Box::new(TextLayer::new(
            self.view_params.clone(),
            TextLayerParams {
                layer_id: format!("{}_legend_point_size_quantitative_layer_labels_sublayer", self.layer_params.layer_id),
                bounds: Some(zero_bounds),
                data_unit_mode_x: UnitsMode::Pixels,
                data_unit_mode_y: UnitsMode::Pixels,
                text_size: DEFAULT_LABEL_FONT_SIZE,
                text_size_unit_mode: UnitsMode::Pixels,
                text_align_mode: label_align_mode,
                text_baseline_mode: label_baseline_mode,
                position_x: NumericData::Float32(Arc::new(label_position_x)),
                position_y: NumericData::Float32(Arc::new(label_position_y)),
                text_vec: Arc::new(label_strings),
                ..Default::default()
            },
        )));

        sublayers
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for LegendPointSizeQuantitativeLayer {
    async fn prepare(&mut self, gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        self.sub_layer_instances = self.build_sublayers();
        base_prepare_composite_layer(&mut self.sub_layer_instances, gpu_context).await
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for LegendPointSizeQuantitativeLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        base_draw_composite_layer(&self.sub_layer_instances, gpu_context, pass).await;
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for LegendPointSizeQuantitativeLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for LegendPointSizeQuantitativeLayer {
    async fn draw(&self, ctx: &mut SvgContext) {
        base_draw_composite_layer_svg(&self.sub_layer_instances, ctx).await
    }
}

inventory::submit! {
    crate::registry::LayerRegistration {
        layer_type_name: "LegendPointSizeQuantitativeLayer",
        create_layer: |value, view_params| {
            let params: LegendPointSizeQuantitativeLayerParams = serde_json::from_value(value).unwrap();
            Box::new(LegendPointSizeQuantitativeLayer::new(view_params.clone(), params))
        },
    }
}

impl PickableLayer for LegendPointSizeQuantitativeLayer {}
