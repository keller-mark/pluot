// The precomputed violin layer renders one violin per group from per-bin counts that were
// binned elsewhere, wrapping one CurveLayer per violin, a LineLayer for the quartile whiskers,
// and band and linear axes. Along the group axis, the violins are positioned in pixel units
// within the layer bounds, while along the value axis they are positioned in data units, so
// that the value axis follows the camera.
// Reference: https://github.com/vitessce/vitessce/blob/main/packages/view-types/statistical-plots/src/CellSetExpressionPlot.js
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::colormaps_categorical;
use crate::composite_layer::{base_draw_composite_layer, base_draw_composite_layer_svg, base_prepare_composite_layer};
use crate::composite_layers::axis_band_layer::{AxisBandLayer, AxisBandLayerParams};
use crate::composite_layers::axis_linear_layer::{AxisLinearLayer, AxisLinearLayerParams, AxisPosition};
use crate::composite_layers::bar_plot_layer::BarOrientation;
use crate::composite_layers::curve_layer::{CurveLayer, CurveLayerParams, PathCommand};
use crate::d3::scale::{ScaleBand, ScaleLinear, Scaleable};
use crate::d3::shape::{area, AreaPoint, CurveBasis};
use crate::emphasis_mode::DEFAULT_BACKGROUND_COLOR;
use crate::extent::LayerExtentResult;
use crate::layers::line_layer::{LineLayer, LineLayerParams};
use crate::numeric_data::NumericData;
use crate::picking::LayerPickingResult;
use crate::render_traits::{
    CategoricalColormap, ColorMode, DrawToRasterCpu, DrawToRasterGpu, DrawToSvg, ExtentableLayer, MarginParams, OpacityMode, PickableLayer, PreparedAndDraw, PreparedLayer, SizeMode, UnitsMode, ViewParams
};
use crate::render_types::{CpuContext, CpuRenderPass, PrepareResult};
use crate::gpu_send::GpuRenderPass;
use crate::render_types::GpuContext;
use crate::two::svg::SvgContext;
use crate::viewport::{DataCoord, ScreenCoord};
use crate::wgpu;

// Inner padding only, with half as much outer padding, keeps each band centered on the
// AxisBandLayer's (unpadded) tick for the same group.
const BAND_PADDING_INNER: f64 = 0.1;
const DEFAULT_FILL_OPACITY: f32 = 1.0;
const DEFAULT_WHISKER_COLOR: (u8, u8, u8) = (0, 0, 0);
const WHISKER_WIDTH_PX: f32 = 2.0;
const MEDIAN_HALF_WIDTH_PX: f64 = 4.0;

/// How violin widths are scaled relative to one another.
/// Reference: https://seaborn.pydata.org/generated/seaborn.violinplot.html (`density_norm`)
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, TS)]
pub enum ViolinDensityNorm {
    /// Every violin has the same area.
    Area,
    /// Each violin's peak width is proportional to its number of observations.
    Count,
    /// Every violin has the same peak width.
    Width,
}

/// Layer params struct for [`PrecomputedViolinLayer`].
#[derive(Serialize, Deserialize, Debug, Clone, TS)]
#[serde(default)]
#[ts(optional_fields = nullable)]
pub struct PrecomputedViolinLayerParams {
    pub layer_id: String,
    // If None, assume margin: 0 in all directions.
    pub bounds: Option<MarginParams>,

    /// `Vertical` (the default) lays the groups out along the X axis, with the values along the Y axis.
    /// `Horizontal` lays the groups out along the Y axis, from bottom to top, with the values along the X axis.
    pub orientation: Option<BarOrientation>,

    /// One label per violin, in order along the group axis.
    pub groups: Arc<Vec<String>>,
    /// The lower edge of each bin along the value axis, in data units, shared by every violin, in ascending order.
    pub bin_starts: Arc<Vec<f32>>,
    /// One entry per group, holding one count per bin. A violin's width at a bin is proportional
    /// to the bin's count, scaled according to `density_norm`.
    pub bin_counts: Arc<Vec<Vec<u32>>>,
    /// One entry per group, like `bin_counts`, counting a superset of its values (e.g., every
    /// filter-included value, whereas `bin_counts` counts only the selected ones). Drawn in
    /// `background_fill_color` behind the `bin_counts` violins, which are then scaled as the
    /// background violins are, so that each is nested within its background. `None` draws none.
    pub background_bin_counts: Option<Arc<Vec<Vec<u32>>>>,
    /// Defaults to `Width`, matching scanpy's `sc.pl.violin`.
    pub density_norm: Option<ViolinDensityNorm>,
    /// One (first quartile, median, third quartile) per group, drawn as a whisker. `None` draws no whiskers.
    pub quartiles: Option<Arc<Vec<Option<(f32, f32, f32)>>>>,

    /// The (min, max) extent of the value axis, reported to fit the camera.
    /// Defaults to the first and last of `bin_starts`.
    pub value_domain: Option<(f32, f32)>,

    /// One fill color per group. Defaults to Tableau10, by group index.
    pub fill_colors: Option<Arc<Vec<(u8, u8, u8)>>>,
    /// Defaults to 1.
    pub fill_opacity: Option<f32>,
    /// Fill color of the `background_bin_counts` violins. Defaults to light grey.
    pub background_fill_color: Option<(u8, u8, u8)>,
    pub whisker_color: Option<(u8, u8, u8)>,

    pub group_axis_title: Option<String>,
    pub value_axis_title: Option<String>,
}

impl Default for PrecomputedViolinLayerParams {
    fn default() -> Self {
        Self {
            layer_id: "".to_string(),
            bounds: None,
            orientation: None,
            groups: Arc::new(vec![]),
            bin_starts: Arc::new(vec![]),
            bin_counts: Arc::new(vec![]),
            background_bin_counts: None,
            density_norm: None,
            quartiles: None,
            value_domain: None,
            fill_colors: None,
            fill_opacity: None,
            background_fill_color: None,
            whisker_color: None,
            group_axis_title: None,
            value_axis_title: None,
        }
    }
}

pub struct PrecomputedViolinLayer {
    view_params: ViewParams,
    layer_params: PrecomputedViolinLayerParams,
    sub_layer_instances: Vec<Box<dyn PreparedAndDraw>>,
}

pub(crate) fn default_fill_color(group_index: usize) -> (u8, u8, u8) {
    let [r, g, b, _] = colormaps_categorical::sample(CategoricalColormap::Tableau10, group_index);
    ((r * 255.0).round() as u8, (g * 255.0).round() as u8, (b * 255.0).round() as u8)
}

/// A position along the group axis (in pixels within the layer bounds) and the value axis (in data units).
#[derive(Debug, Clone, Copy, PartialEq)]
struct GroupValuePoint {
    group_px: f64,
    value: f64,
}

impl GroupValuePoint {
    fn to_xy(self, orientation: &BarOrientation) -> (f64, f64) {
        match orientation {
            BarOrientation::Vertical => (self.group_px, self.value),
            BarOrientation::Horizontal => (self.value, self.group_px),
        }
    }
}

/// Positions of violins along the group axis.
pub struct ViolinScales {
    pub group_scale: ScaleBand,
    /// Maps a signed span (see [`normalized_spans`]) to an offset within a band.
    pub width_scale: ScaleLinear,
}

impl ViolinScales {
    pub fn new(p: &PrecomputedViolinLayerParams, group_axis_length_px: f32) -> Self {
        let mut group_scale = ScaleBand::new();
        group_scale.set_domain(p.groups.as_ref().clone());
        group_scale.set_range((0.0, group_axis_length_px as f64));
        group_scale.set_padding_inner(BAND_PADDING_INNER);
        group_scale.set_padding_outer(BAND_PADDING_INNER / 2.0);

        let mut width_scale = ScaleLinear::new();
        width_scale.set_domain((-1.0, 1.0));
        width_scale.set_range((0.0, group_scale.bandwidth()));

        Self { group_scale, width_scale }
    }

    pub fn band_start(&self, group: &String) -> f64 {
        self.group_scale.scale(group)
    }

    pub fn band_center(&self, group: &String) -> f64 {
        self.band_start(group) + self.group_scale.bandwidth() / 2.0
    }

    pub fn violin_outline(&self, group: &String, bin_starts: &[f32], spans: &[f64], orientation: &BarOrientation) -> Vec<PathCommand> {
        let band_start = self.band_start(group);
        let points: Vec<AreaPoint> = bin_starts.iter().zip(spans)
            .map(|(&bin_start, &span)| {
                let value = bin_start as f64;
                let lower = GroupValuePoint { group_px: band_start + self.width_scale.scale(&-span), value };
                let upper = GroupValuePoint { group_px: band_start + self.width_scale.scale(&span), value };
                let ((x0, y0), (x1, y1)) = (lower.to_xy(orientation), upper.to_xy(orientation));
                AreaPoint { x0, y0, x1, y1 }
            })
            .collect();
        area(&mut CurveBasis::default(), &points)
    }
}

/// Each violin's width at each bin, as a fraction of the band's width, following seaborn's `density_norm`.
/// Reference: https://github.com/mwaskom/seaborn/blob/v0.13.2/seaborn/categorical.py#L1007-L1016
pub fn normalized_spans(bin_counts: &[Vec<u32>], density_norm: ViolinDensityNorm) -> Vec<Vec<f64>> {
    scaled_spans(bin_counts, &span_scales(bin_counts, density_norm))
}

/// The factor by which each violin's counts are multiplied to get its [`normalized_spans`].
fn span_scales(bin_counts: &[Vec<u32>], density_norm: ViolinDensityNorm) -> Vec<f64> {
    let peak = |counts: &[u32]| counts.iter().copied().max().unwrap_or(0) as f64;
    let total = |counts: &[u32]| counts.iter().map(|&count| count as f64).sum::<f64>();
    let max_peak_density = bin_counts.iter()
        .filter(|counts| total(counts) > 0.0)
        .map(|counts| peak(counts) / total(counts))
        .fold(0.0, f64::max);
    let max_total = bin_counts.iter().map(|counts| total(counts)).fold(0.0, f64::max);
    bin_counts.iter()
        .map(|counts| {
            let (peak, total) = (peak(counts), total(counts));
            if peak == 0.0 {
                return 0.0;
            }
            match density_norm {
                ViolinDensityNorm::Area => 1.0 / (total * max_peak_density),
                ViolinDensityNorm::Count => total / (peak * max_total),
                ViolinDensityNorm::Width => 1.0 / peak,
            }
        })
        .collect()
}

fn scaled_spans(bin_counts: &[Vec<u32>], scales: &[f64]) -> Vec<Vec<f64>> {
    bin_counts.iter().zip(scales)
        .map(|(counts, &scale)| counts.iter().map(|&count| count as f64 * scale).collect())
        .collect()
}

/// The [`normalized_spans`] of `bin_counts` and, if any, of `background_bin_counts`. When there
/// is a background, both are scaled as the background is, so each violin nests within its background.
pub fn foreground_and_background_spans(
    bin_counts: &[Vec<u32>],
    background_bin_counts: Option<&[Vec<u32>]>,
    density_norm: ViolinDensityNorm,
) -> (Vec<Vec<f64>>, Option<Vec<Vec<f64>>>) {
    match background_bin_counts {
        None => (normalized_spans(bin_counts, density_norm), None),
        Some(background_bin_counts) => {
            let scales = span_scales(background_bin_counts, density_norm);
            (scaled_spans(bin_counts, &scales), Some(scaled_spans(background_bin_counts, &scales)))
        }
    }
}

fn resolve_value_domain(p: &PrecomputedViolinLayerParams) -> (f32, f32) {
    p.value_domain.unwrap_or_else(|| {
        let first = p.bin_starts.first().copied().unwrap_or(0.0);
        let last = p.bin_starts.last().copied().unwrap_or(1.0);
        (first, last)
    })
}

impl PrecomputedViolinLayer {
    pub fn new(view_params: ViewParams, layer_params: PrecomputedViolinLayerParams) -> Self {
        Self {
            view_params,
            layer_params,
            sub_layer_instances: Vec::new(),
        }
    }

    fn orientation(&self) -> BarOrientation {
        self.layer_params.orientation.clone().unwrap_or(BarOrientation::Vertical)
    }

    /// Returns (data_unit_mode_x, data_unit_mode_y).
    fn unit_modes(&self) -> (UnitsMode, UnitsMode) {
        match self.orientation() {
            BarOrientation::Vertical => (UnitsMode::Pixels, UnitsMode::Data),
            BarOrientation::Horizontal => (UnitsMode::Data, UnitsMode::Pixels),
        }
    }

    /// Returns (margin_left, margin_bottom, layer_width, layer_height), in pixels.
    fn layer_box(&self) -> (f32, f32, f32, f32) {
        let bounds = if self.layer_params.bounds.is_none() { &self.view_params.margins } else { &self.layer_params.bounds };
        let margin = |get: fn(&MarginParams) -> Option<f32>| bounds.as_ref().and_then(get).unwrap_or(0.0);
        let (margin_top, margin_right) = (margin(|m| m.margin_top), margin(|m| m.margin_right));
        let (margin_bottom, margin_left) = (margin(|m| m.margin_bottom), margin(|m| m.margin_left));
        let layer_w = (self.view_params.width as f32 - (margin_left + margin_right)).max(1.0);
        let layer_h = (self.view_params.height as f32 - (margin_top + margin_bottom)).max(1.0);
        (margin_left, margin_bottom, layer_w, layer_h)
    }

    fn scales(&self) -> ViolinScales {
        let (_, _, layer_w, layer_h) = self.layer_box();
        let group_axis_length_px = match self.orientation() {
            BarOrientation::Vertical => layer_w,
            BarOrientation::Horizontal => layer_h,
        };
        ViolinScales::new(&self.layer_params, group_axis_length_px)
    }

    fn build_violin_layers(&self, scales: &ViolinScales) -> Vec<Box<dyn PreparedAndDraw>> {
        let p = &self.layer_params;
        let orientation = self.orientation();
        let (data_unit_mode_x, data_unit_mode_y) = self.unit_modes();
        let (spans, background_spans) = foreground_and_background_spans(
            &p.bin_counts,
            p.background_bin_counts.as_deref().map(Vec::as_slice),
            p.density_norm.unwrap_or(ViolinDensityNorm::Width),
        );
        let fill_opacity = p.fill_opacity.unwrap_or(DEFAULT_FILL_OPACITY);
        let violin_layer = |name: &str, group_index: usize, spans: &[f64], fill_color: (u8, u8, u8)| {
            Box::new(CurveLayer::new(
                self.view_params.clone(),
                CurveLayerParams {
                    layer_id: format!("{}_precomputed_violin_{name}_sublayer_{group_index}", p.layer_id),
                    bounds: p.bounds.clone(),
                    data_unit_mode_x,
                    data_unit_mode_y,
                    commands: Arc::new(scales.violin_outline(&p.groups[group_index], &p.bin_starts, spans, &orientation)),
                    stroked: false,
                    filled: true,
                    fill_color: Some(ColorMode::UniformRgb(fill_color)),
                    fill_opacity: Some(OpacityMode::UniformOpacity(fill_opacity)),
                    ..Default::default()
                },
            )) as Box<dyn PreparedAndDraw>
        };
        let background_fill_color = p.background_fill_color.unwrap_or(DEFAULT_BACKGROUND_COLOR);
        let background_layers = background_spans.iter().flatten().enumerate().take(p.groups.len())
            .map(|(group_index, spans)| violin_layer("background_curve", group_index, spans, background_fill_color));
        let foreground_layers = spans.iter().enumerate().take(p.groups.len())
            .map(|(group_index, spans)| {
                let fill_color = p.fill_colors.as_ref()
                    .and_then(|colors| colors.get(group_index).copied())
                    .unwrap_or_else(|| default_fill_color(group_index));
                violin_layer("curve", group_index, spans, fill_color)
            });
        background_layers.chain(foreground_layers).collect()
    }

    /// A line along the value axis spanning each group's interquartile range, crossed by a short line at its median.
    fn build_whisker_layer(&self, scales: &ViolinScales) -> Option<LineLayer> {
        let p = &self.layer_params;
        let quartiles = p.quartiles.as_ref()?;
        let orientation = self.orientation();
        let (data_unit_mode_x, data_unit_mode_y) = self.unit_modes();
        let mut source_x = Vec::new();
        let mut source_y = Vec::new();
        let mut target_x = Vec::new();
        let mut target_y = Vec::new();
        for (group, &(q1, median, q3)) in p.groups.iter().zip(quartiles.iter()).filter_map(|(group, q)| Some((group, q.as_ref()?))) {
            let center = scales.band_center(group);
            let segments = [
                (GroupValuePoint { group_px: center, value: q1 as f64 }, GroupValuePoint { group_px: center, value: q3 as f64 }),
                (
                    GroupValuePoint { group_px: center - MEDIAN_HALF_WIDTH_PX, value: median as f64 },
                    GroupValuePoint { group_px: center + MEDIAN_HALF_WIDTH_PX, value: median as f64 },
                ),
            ];
            for (source, target) in segments {
                let ((sx, sy), (tx, ty)) = (source.to_xy(&orientation), target.to_xy(&orientation));
                source_x.push(sx as f32);
                source_y.push(sy as f32);
                target_x.push(tx as f32);
                target_y.push(ty as f32);
            }
        }
        if source_x.is_empty() {
            return None;
        }

        Some(LineLayer::new(
            self.view_params.clone(),
            LineLayerParams {
                layer_id: format!("{}_precomputed_violin_whisker_sublayer", p.layer_id),
                bounds: p.bounds.clone(),
                data_unit_mode_x,
                data_unit_mode_y,
                stroke_width_unit_mode: UnitsMode::Pixels,
                stroke_color: Some(ColorMode::UniformRgb(p.whisker_color.unwrap_or(DEFAULT_WHISKER_COLOR))),
                stroke_width: Some(SizeMode::UniformSize(WHISKER_WIDTH_PX)),
                source_position_x: NumericData::Float32(Arc::new(source_x)),
                source_position_y: NumericData::Float32(Arc::new(source_y)),
                target_position_x: NumericData::Float32(Arc::new(target_x)),
                target_position_y: NumericData::Float32(Arc::new(target_y)),
                ..Default::default()
            },
        ))
    }

    fn build_axis_layers(&self) -> Vec<Box<dyn PreparedAndDraw>> {
        let p = &self.layer_params;
        let (group_axis_position, value_axis_position) = match self.orientation() {
            BarOrientation::Vertical => (AxisPosition::Bottom, AxisPosition::Left),
            BarOrientation::Horizontal => (AxisPosition::Left, AxisPosition::Bottom),
        };
        vec![
            Box::new(AxisBandLayer::new(
                self.view_params.clone(),
                AxisBandLayerParams {
                    layer_id: format!("{}_precomputed_violin_group_axis_sublayer", p.layer_id),
                    position: group_axis_position,
                    domain: Arc::clone(&p.groups),
                    title: p.group_axis_title.clone(),
                    ..Default::default()
                },
            )),
            Box::new(AxisLinearLayer::new(
                self.view_params.clone(),
                AxisLinearLayerParams {
                    layer_id: format!("{}_precomputed_violin_value_axis_sublayer", p.layer_id),
                    position: value_axis_position,
                    title: p.value_axis_title.clone(),
                    ..Default::default()
                },
            )),
        ]
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for PrecomputedViolinLayer {
    async fn prepare(&mut self, gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        let scales = self.scales();
        let mut sublayers = self.build_violin_layers(&scales);
        if let Some(whisker_layer) = self.build_whisker_layer(&scales) {
            sublayers.push(Box::new(whisker_layer));
        }
        sublayers.extend(self.build_axis_layers());
        self.sub_layer_instances = sublayers;

        base_prepare_composite_layer(&mut self.sub_layer_instances, gpu_context).await
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for PrecomputedViolinLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut GpuRenderPass<'_>) {
        base_draw_composite_layer(&self.sub_layer_instances, gpu_context, pass).await;
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for PrecomputedViolinLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for PrecomputedViolinLayer {
    async fn draw(&self, ctx: &mut SvgContext) {
        base_draw_composite_layer_svg(&self.sub_layer_instances, ctx).await
    }
}

inventory::submit! {
    crate::registry::LayerRegistration {
        layer_type_name: "PrecomputedViolinLayer",
        create_layer: |value, view_params| {
            let params: PrecomputedViolinLayerParams = serde_json::from_value(value).unwrap();
            Box::new(PrecomputedViolinLayer::new(view_params.clone(), params))
        },
    }
}

impl ExtentableLayer for PrecomputedViolinLayer {
    fn extent(&self) -> Option<LayerExtentResult> {
        let value_extent = resolve_value_domain(&self.layer_params);
        // The group axis is positioned in pixels, so the camera does not affect it, and any valid extent will do.
        let group_extent = (0.0, 1.0);
        let (x, y) = match self.orientation() {
            BarOrientation::Vertical => (group_extent, value_extent),
            BarOrientation::Horizontal => (value_extent, group_extent),
        };
        Some(LayerExtentResult {
            layer_id: self.layer_params.layer_id.clone(),
            x,
            y,
            z: None,
        })
    }
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
pub struct PrecomputedViolinPickingInfo {
    /// Index into `groups`.
    pub index: usize,
    pub group: String,
    pub quartiles: Option<(f32, f32, f32)>,
}

impl PickableLayer for PrecomputedViolinLayer {
    // Picks the group whose band contains the cursor, rather than only the violin's filled area,
    // since narrow violins would otherwise be hard to hover.
    fn pick(&self, screen_coord: ScreenCoord, _data_coord: Option<DataCoord>) -> Option<LayerPickingResult> {
        let (margin_left, margin_bottom, layer_w, layer_h) = self.layer_box();
        let x = screen_coord.x - margin_left;
        let y = screen_coord.y - margin_bottom;
        if x < 0.0 || x > layer_w || y < 0.0 || y > layer_h {
            return None;
        }
        let group_px = match self.orientation() {
            BarOrientation::Vertical => x,
            BarOrientation::Horizontal => y,
        } as f64;
        let scales = self.scales();
        let bandwidth = scales.group_scale.bandwidth();
        let p = &self.layer_params;
        let index = p.groups.iter().position(|group| {
            let band_start = scales.band_start(group);
            group_px >= band_start && group_px <= band_start + bandwidth
        })?;
        Some(LayerPickingResult::new(&p.layer_id, PrecomputedViolinPickingInfo {
            index,
            group: p.groups[index].clone(),
            quartiles: p.quartiles.as_ref().and_then(|quartiles| quartiles.get(index).copied().flatten()),
        }))
    }
}
