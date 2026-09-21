// The precomputed histogram layer renders bars from per-bin quantities that were
// binned elsewhere, wrapping RectLayer and two AxisLinearLayers. The bars are
// positioned in data units along both axes, so they follow the camera.
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::composite_layer::{base_draw_composite_layer, base_draw_composite_layer_svg, base_prepare_composite_layer};
use crate::composite_layers::axis_linear_layer::{AxisLinearLayer, AxisLinearLayerParams, AxisPosition};
use crate::composite_layers::bar_plot_layer::BarOrientation;
use crate::d3::scale::ScaleLinear;
use crate::emphasis_mode::DEFAULT_BACKGROUND_COLOR;
use crate::extent::LayerExtentResult;
use crate::layers::rect_layer::{RectLayer, RectLayerParams};
use crate::numeric_data::NumericData;
use crate::render_traits::{
    BrushableLayer, ColorMode, DrawToRasterCpu, DrawToRasterGpu, DrawToSvg, ExtentableLayer, MarginParams, PickableLayer, PreparedAndDraw, PreparedLayer, UnitsMode, ViewParams
};
use crate::render_types::{CpuContext, CpuRenderPass, PrepareResult};
use crate::render_types::GpuContext;
use crate::two::svg::SvgContext;
use crate::viewport::get_bounds;
use crate::{log, wgpu};

const DEFAULT_BAR_MARGIN: f32 = 1.0;
const DEFAULT_FILL_COLOR: (u8, u8, u8) = (76, 120, 168);

/// Layer params struct for [`PrecomputedHistogramLayer`].
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct PrecomputedHistogramLayerParams {
    pub layer_id: String,
    // If None, assume margin: 0 in all directions.
    pub bounds: Option<MarginParams>,

    pub orientation: BarOrientation,

    /// The lower and upper edge of the binned domain, i.e. the extent of the
    /// values that were binned to produce the quantities. The bars are laid out
    /// along this domain: it positions them, and does not size them.
    pub bin_min: f32,
    pub bin_max: f32,
    /// The number of equal-width bins spanning `[bin_min, bin_max]`.
    pub num_bins: u32,

    /// The lower and upper end of the quantity domain, i.e. the extent of the
    /// per-bin quantities. The bars are sized along this domain: each one runs
    /// from `quantity_min` to its own quantity, and the layer reports
    /// `[quantity_min, quantity_max]` as its extent along this axis.
    pub quantity_min: f32,
    pub quantity_max: f32,

    /// The per-bin quantity of the "foreground" (selected) bars.
    pub quantity: Arc<Vec<f32>>,
    /// The per-bin quantity of the "background" (filter-included, but
    /// selection-excluded) bars, drawn behind the foreground bars. When None,
    /// only the foreground bars are drawn.
    pub background_quantity: Option<Arc<Vec<f32>>>,

    /// The gap between adjacent bars, in pixel units, split evenly across the
    /// two sides of each bar. The bars themselves are positioned in data units,
    /// so this gap is re-derived from the camera on every prepare.
    pub bar_margin: Option<f32>,

    pub fill_color: Option<(u8, u8, u8)>,
    pub background_fill_color: Option<(u8, u8, u8)>,
}

impl Default for PrecomputedHistogramLayerParams {
    fn default() -> Self {
        Self {
            layer_id: "".to_string(),
            bounds: None,
            orientation: BarOrientation::Vertical,
            bin_min: 0.0,
            bin_max: 1.0,
            num_bins: 50,
            quantity_min: 0.0,
            quantity_max: 1.0,
            quantity: Arc::new(vec![]),
            background_quantity: None,
            bar_margin: None,
            fill_color: None,
            background_fill_color: None,
        }
    }
}

pub struct PrecomputedHistogramLayer {
    view_params: ViewParams,
    layer_params: PrecomputedHistogramLayerParams,
    sub_layer_instances: Vec<Box<dyn PreparedAndDraw>>,
}

impl PrecomputedHistogramLayer {
    pub fn new(view_params: ViewParams, layer_params: PrecomputedHistogramLayerParams) -> Self {
        // TODO: validate that the length of the foreground/background vectors matches num_bins.
        //
        Self {
            view_params,
            layer_params,
            sub_layer_instances: Vec::new(),
        }
    }

    /// The width of one pixel, in data units, along the binned value axis: X for
    /// a vertical histogram, Y for a horizontal one.
    fn data_units_per_pixel_along_value_axis(&self) -> f32 {
        let margins = &self.view_params.margins;
        let margin_top = margins.as_ref().and_then(|m| m.margin_top).unwrap_or(0.0) as f64;
        let margin_right = margins.as_ref().and_then(|m| m.margin_right).unwrap_or(0.0) as f64;
        let margin_bottom = margins.as_ref().and_then(|m| m.margin_bottom).unwrap_or(0.0) as f64;
        let margin_left = margins.as_ref().and_then(|m| m.margin_left).unwrap_or(0.0) as f64;

        let viewport_w = self.view_params.width as f64;
        let viewport_h = self.view_params.height as f64;

        let bounds = get_bounds(&self.view_params);
        let mut scale = ScaleLinear::new();
        match self.layer_params.orientation {
            BarOrientation::Vertical => {
                scale.set_domain((bounds.x_min as f64, bounds.x_max as f64));
                scale.set_range((margin_left, viewport_w - margin_right));
            }
            BarOrientation::Horizontal => {
                scale.set_domain((bounds.y_min as f64, bounds.y_max as f64));
                scale.set_range((margin_bottom, viewport_h - margin_top));
            }
        }

        (scale.invert(1.0) - scale.invert(0.0)) as f32
    }

    /// The lower and upper edge of each bar along the binned value axis, in data
    /// units, inset by half of the bar margin on either side.
    fn bar_edges(&self) -> (Vec<f32>, Vec<f32>) {
        let p = &self.layer_params;
        let bin_width = (p.bin_max - p.bin_min) / p.num_bins as f32;

        let margin = p.bar_margin.unwrap_or(DEFAULT_BAR_MARGIN) * self.data_units_per_pixel_along_value_axis();
        // Bars narrower than the margin would otherwise be inverted.
        let inset = (margin / 2.0).min(bin_width / 2.0);

        (0..p.num_bins)
            .map(|bin_index| {
                let bin_start = p.bin_min + bin_width * bin_index as f32;
                (bin_start + inset, bin_start + bin_width - inset)
            })
            .unzip()
    }

    fn build_rect_layer(
        &self,
        layer_id: String,
        quantity: &[f32],
        fill_color: (u8, u8, u8),
        bar_lo: &[f32],
        bar_hi: &[f32],
    ) -> RectLayer {
        let n = quantity.len().min(bar_lo.len());
        let baseline = vec![self.layer_params.quantity_min; n];

        // The bars span the bin along the value axis, and run from the bottom of
        // the quantity domain to their own quantity along the other axis.
        let (position_x0, position_y0, position_x1, position_y1) = match self.layer_params.orientation {
            BarOrientation::Vertical => (
                bar_lo[..n].to_vec(),
                baseline,
                bar_hi[..n].to_vec(),
                quantity[..n].to_vec(),
            ),
            BarOrientation::Horizontal => (
                baseline,
                bar_lo[..n].to_vec(),
                quantity[..n].to_vec(),
                bar_hi[..n].to_vec(),
            ),
        };

        RectLayer::new(
            self.view_params.clone(),
            RectLayerParams {
                layer_id,
                bounds: self.layer_params.bounds.clone(),
                data_unit_mode_x: UnitsMode::Data,
                data_unit_mode_y: UnitsMode::Data,
                fill_color: Some(ColorMode::UniformRgb(fill_color)),
                position_x0: NumericData::Float32(Arc::new(position_x0)),
                position_y0: NumericData::Float32(Arc::new(position_y0)),
                position_x1: NumericData::Float32(Arc::new(position_x1)),
                position_y1: NumericData::Float32(Arc::new(position_y1)),
                ..Default::default()
            },
        )
    }

    fn build_sublayers(&self) -> Vec<Box<dyn PreparedAndDraw>> {
        let p = &self.layer_params;
        let (bar_lo, bar_hi) = self.bar_edges();

        let mut sublayers: Vec<Box<dyn PreparedAndDraw>> = Vec::new();

        // The background bars are drawn first, so that the foreground bars
        // appear in front of them.
        if let Some(background_quantity) = &p.background_quantity {
            sublayers.push(Box::new(self.build_rect_layer(
                format!("{}_precomputed_histogram_rect_sublayer_background", p.layer_id),
                background_quantity,
                p.background_fill_color.unwrap_or(DEFAULT_BACKGROUND_COLOR),
                &bar_lo,
                &bar_hi,
            )));
        }
        sublayers.push(Box::new(self.build_rect_layer(
            format!("{}_precomputed_histogram_rect_sublayer_foreground", p.layer_id),
            &p.quantity,
            p.fill_color.unwrap_or(DEFAULT_FILL_COLOR),
            &bar_lo,
            &bar_hi,
        )));

        let (value_axis_position, quantity_axis_position) = match p.orientation {
            BarOrientation::Vertical => (AxisPosition::Bottom, AxisPosition::Left),
            BarOrientation::Horizontal => (AxisPosition::Left, AxisPosition::Bottom),
        };

        // Both axes follow the camera rather than being pinned to the binned
        // domain, since the bars are positioned in data units.
        sublayers.push(Box::new(AxisLinearLayer::new(
            self.view_params.clone(),
            AxisLinearLayerParams {
                layer_id: format!("{}_precomputed_histogram_value_axis_sublayer", p.layer_id),
                position: value_axis_position,
                ..Default::default()
            },
        )));
        sublayers.push(Box::new(AxisLinearLayer::new(
            self.view_params.clone(),
            AxisLinearLayerParams {
                layer_id: format!("{}_precomputed_histogram_quantity_axis_sublayer", p.layer_id),
                position: quantity_axis_position,
                ..Default::default()
            },
        )));

        sublayers
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for PrecomputedHistogramLayer {
    async fn prepare(&mut self, gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        self.sub_layer_instances = self.build_sublayers();

        base_prepare_composite_layer(&mut self.sub_layer_instances, gpu_context).await
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for PrecomputedHistogramLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        base_draw_composite_layer(&self.sub_layer_instances, gpu_context, pass).await;
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for PrecomputedHistogramLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for PrecomputedHistogramLayer {
    async fn draw(&self, ctx: &mut SvgContext) {
        base_draw_composite_layer_svg(&self.sub_layer_instances, ctx).await
    }
}

inventory::submit! {
    crate::registry::LayerRegistration {
        layer_type_name: "PrecomputedHistogramLayer",
        create_layer: |value, view_params| {
            let params: PrecomputedHistogramLayerParams = serde_json::from_value(value).unwrap();
            Box::new(PrecomputedHistogramLayer::new(view_params.clone(), params))
        },
    }
}

impl BrushableLayer for PrecomputedHistogramLayer {}

impl PickableLayer for PrecomputedHistogramLayer {}

impl ExtentableLayer for PrecomputedHistogramLayer {
    fn extent(&self) -> Option<LayerExtentResult> {
        let p = &self.layer_params;

        let (x, y) = match p.orientation {
            BarOrientation::Vertical => ((p.bin_min, p.bin_max), (p.quantity_min, p.quantity_max)),
            BarOrientation::Horizontal => ((p.quantity_min, p.quantity_max), (p.bin_min, p.bin_max)),
        };

        Some(LayerExtentResult {
            layer_id: p.layer_id.clone(),
            x,
            y,
            z: None,
        })
    }
}
