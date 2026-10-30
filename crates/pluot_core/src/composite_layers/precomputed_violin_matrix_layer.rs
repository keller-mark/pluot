// The precomputed violin matrix layer renders a grid of violins, one per (group, var) cell, from
// per-bin counts that were binned elsewhere, wrapping multi-curve CurveLayers for the violins,
// two band axes, and a colormap legend. Both categorical axes, and the value axis within each cell, are
// positioned in pixel units within the layer bounds, so the camera does not affect the plot.
// Reference: https://scanpy.readthedocs.io/en/stable/generated/scanpy.pl.stacked_violin.html
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::composite_layer::{base_draw_composite_layer, base_draw_composite_layer_svg, base_prepare_composite_layer};
use crate::composite_layers::axis_band_layer::{AxisBandLayer, AxisBandLayerParams};
use crate::composite_layers::axis_linear_layer::AxisPosition;
use crate::composite_layers::curve_layer::{CurveLayer, CurveLayerParams, PathCommand};
use crate::composite_layers::legend_colormap_quantitative_layer::{LegendColormapQuantitativeLayer, LegendColormapQuantitativeLayerParams, LegendOrientation};
use crate::composite_layers::precomputed_violin_layer::{default_fill_color, normalized_spans, ViolinDensityNorm};
use crate::d3::scale::{ScaleBand, ScaleLinear, Scaleable};
use crate::d3::shape::{area, AreaPoint, CurveBasis};
use crate::numeric_data::NumericData;
use crate::picking::LayerPickingResult;
use crate::render_traits::{
    ColorMode, DrawToRasterCpu, DrawToRasterGpu, DrawToSvg, ExtentableLayer, MarginParams, OpacityMode, PickableLayer, PreparedAndDraw, PreparedLayer, QuantitativeColormap, QuantitativeParams, SizeMode, UnitsMode, ViewParams
};
use crate::render_types::{CpuContext, CpuRenderPass, PrepareResult};
use crate::gpu_send::GpuRenderPass;
use crate::render_types::GpuContext;
use crate::two::svg::SvgContext;
use crate::viewport::{DataCoord, ScreenCoord};
use crate::wgpu;

// Inner padding only, with half as much outer padding, keeps each column centered on the
// AxisBandLayer's (unpadded) tick, and makes every column's step `layer length / n`.
const COLUMN_PADDING_INNER: f64 = 0.1;
// The fraction of each row's height left empty above and below its violins.
const ROW_PADDING: f64 = 0.05;
const DEFAULT_FILL_OPACITY: f32 = 1.0;
const DEFAULT_STROKE_COLOR: (u8, u8, u8) = (0, 0, 0);
const DEFAULT_STROKE_WIDTH_PX: f32 = 1.0;
// TODO: remove this padding once AxisLinearLayerParams.fit_outer_ticks is implemented.
const LEGEND_PADDING_HORIZONTAL: f32 = 5.0;

/// Layer params struct for [`PrecomputedViolinMatrixLayer`].
#[derive(Serialize, Deserialize, Debug, Clone, TS)]
#[serde(default)]
#[ts(optional_fields = nullable)]
pub struct PrecomputedViolinMatrixLayerParams {
    pub layer_id: String,
    // If None, assume margin: 0 in all directions.
    pub bounds: Option<MarginParams>,

    /// By default, `var_names` are laid out along the X axis and `groups` along the Y axis.
    /// When true, `groups` are laid out along the X axis and `var_names` along the Y axis.
    /// Either way, each violin's values run along the Y axis within its row. Defaults to false.
    pub swap_axes: Option<bool>,

    /// One label per var (e.g., gene), in order along the var axis.
    pub var_names: Arc<Vec<String>>,
    /// One label per group (e.g., cell type), in order along the group axis.
    pub groups: Arc<Vec<String>>,
    /// The lower edge of each bin along the value axis, in data units, shared by every violin, in ascending order.
    pub bin_starts: Arc<Vec<f32>>,
    /// One entry per cell, holding one count per bin. The cell of group `g` and var `v` is at
    /// index `g * var_names.len() + v`. A violin's width at a bin is proportional to the bin's
    /// count, scaled according to `density_norm`.
    pub bin_counts: Arc<Vec<Vec<u32>>>,
    /// Defaults to `Width`, matching scanpy's `sc.pl.stacked_violin`.
    pub density_norm: Option<ViolinDensityNorm>,

    /// The (min, max) of the value axis within each row, shared by every violin.
    /// Defaults to the first and last of `bin_starts`.
    pub value_domain: Option<(f32, f32)>,

    /// One value per cell, in the same order as `bin_counts`, mapped through `cmap` to color each
    /// violin's fill (e.g., its median). `None` (for every cell, or for one cell) fills with `fill_color`.
    pub fill_values: Option<Arc<Vec<Option<f32>>>>,
    /// Defaults to the (min, max) of the finite `fill_values`.
    pub fill_value_domain: Option<(f32, f32)>,
    /// Defaults to `Blues`, matching scanpy's `sc.pl.stacked_violin`.
    pub cmap: Option<QuantitativeColormap>,
    /// Title of the colormap legend, which is drawn within the right margin when `fill_values` is set.
    pub fill_value_title: Option<String>,
    /// Used for cells without a fill value. Defaults to the first Tableau10 color.
    pub fill_color: Option<(u8, u8, u8)>,
    /// Defaults to 1.
    pub fill_opacity: Option<f32>,
    /// Defaults to black.
    pub stroke_color: Option<(u8, u8, u8)>,
    /// In pixels. Defaults to 1. Zero draws no outlines.
    pub stroke_width: Option<f32>,

    pub var_axis_title: Option<String>,
    pub group_axis_title: Option<String>,
}

impl Default for PrecomputedViolinMatrixLayerParams {
    fn default() -> Self {
        Self {
            layer_id: "".to_string(),
            bounds: None,
            swap_axes: None,
            var_names: Arc::new(vec![]),
            groups: Arc::new(vec![]),
            bin_starts: Arc::new(vec![]),
            bin_counts: Arc::new(vec![]),
            density_norm: None,
            value_domain: None,
            fill_values: None,
            fill_value_domain: None,
            cmap: None,
            fill_value_title: None,
            fill_color: None,
            fill_opacity: None,
            stroke_color: None,
            stroke_width: None,
            var_axis_title: None,
            group_axis_title: None,
        }
    }
}

pub struct PrecomputedViolinMatrixLayer {
    view_params: ViewParams,
    layer_params: PrecomputedViolinMatrixLayerParams,
    sub_layer_instances: Vec<Box<dyn PreparedAndDraw>>,
}

/// The cell of `group_index` and `var_index` within `bin_counts` and `fill_values`.
pub fn violin_matrix_cell_index(group_index: usize, var_index: usize, num_vars: usize) -> usize {
    group_index * num_vars + var_index
}

/// The outlines of several violins, as the `commands` and `curve_offsets` of a multi-curve [`CurveLayer`].
struct ViolinOutlines {
    commands: Vec<PathCommand>,
    curve_offsets: Vec<u32>,
    /// One per violin.
    fill_values: Vec<f32>,
}

impl ViolinOutlines {
    fn new() -> Self {
        Self { commands: Vec::new(), curve_offsets: vec![0], fill_values: Vec::new() }
    }

    fn push(&mut self, outline: Vec<PathCommand>, fill_value: f32) {
        self.commands.extend(outline);
        self.curve_offsets.push(self.commands.len() as u32);
        self.fill_values.push(fill_value);
    }

    fn is_empty(&self) -> bool {
        self.fill_values.is_empty()
    }
}

/// Pixel positions (within the layer bounds) of the matrix's columns, rows, and each row's values.
struct ViolinMatrixScales {
    column_scale: ScaleBand,
    row_scale: ScaleBand,
    /// Maps a signed span (see [`normalized_spans`]) to an offset within a column.
    width_scale: ScaleLinear,
    /// Maps a value to an offset within a row.
    value_scale: ScaleLinear,
    swap_axes: bool,
}

impl ViolinMatrixScales {
    fn new(p: &PrecomputedViolinMatrixLayerParams, layer_w: f32, layer_h: f32) -> Self {
        let swap_axes = p.swap_axes.unwrap_or(false);
        let (column_labels, row_labels) = if swap_axes { (&p.groups, &p.var_names) } else { (&p.var_names, &p.groups) };

        let mut column_scale = ScaleBand::new();
        column_scale.set_domain(column_labels.as_ref().clone());
        column_scale.set_range((0.0, layer_w as f64));
        column_scale.set_padding_inner(COLUMN_PADDING_INNER);
        column_scale.set_padding_outer(COLUMN_PADDING_INNER / 2.0);

        let mut row_scale = ScaleBand::new();
        row_scale.set_domain(row_labels.as_ref().clone());
        row_scale.set_range((0.0, layer_h as f64));

        let mut width_scale = ScaleLinear::new();
        width_scale.set_domain((-1.0, 1.0));
        width_scale.set_range((0.0, column_scale.bandwidth()));

        let (value_min, value_max) = resolve_value_domain(p);
        // A zero-length domain would place every value at NaN.
        let value_max = if value_max > value_min { value_max } else { value_min + 1.0 };
        let row_height = row_scale.bandwidth();
        let mut value_scale = ScaleLinear::new();
        value_scale.set_domain((value_min as f64, value_max as f64));
        value_scale.set_range((row_height * ROW_PADDING, row_height * (1.0 - ROW_PADDING)));

        Self { column_scale, row_scale, width_scale, value_scale, swap_axes }
    }

    /// Returns the (column start, row start) of the cell, in pixels.
    fn cell_origin(&self, group: &String, var_name: &String) -> (f64, f64) {
        let (column_label, row_label) = if self.swap_axes { (group, var_name) } else { (var_name, group) };
        (self.column_scale.scale(column_label), self.row_scale.scale(row_label))
    }

    fn violin_outline(&self, cell_origin: (f64, f64), bin_starts: &[f32], spans: &[f64]) -> Vec<PathCommand> {
        let (column_start, row_start) = cell_origin;
        let points: Vec<AreaPoint> = bin_starts.iter().zip(spans)
            .map(|(&bin_start, &span)| {
                let y = row_start + self.value_scale.scale(&(bin_start as f64));
                AreaPoint {
                    x0: column_start + self.width_scale.scale(&-span),
                    y0: y,
                    x1: column_start + self.width_scale.scale(&span),
                    y1: y,
                }
            })
            .collect();
        area(&mut CurveBasis::default(), &points)
    }

    /// Returns the index of the band at `px` along a band axis of `length` pixels with `num_bands` bands.
    fn band_index_at(px: f64, length: f64, num_bands: usize) -> Option<usize> {
        if num_bands == 0 || px < 0.0 || px > length {
            return None;
        }
        Some(((px / length * num_bands as f64) as usize).min(num_bands - 1))
    }
}

fn resolve_value_domain(p: &PrecomputedViolinMatrixLayerParams) -> (f32, f32) {
    p.value_domain.unwrap_or_else(|| {
        let first = p.bin_starts.first().copied().unwrap_or(0.0);
        let last = p.bin_starts.last().copied().unwrap_or(1.0);
        (first, last)
    })
}

fn resolve_fill_value_domain(p: &PrecomputedViolinMatrixLayerParams, fill_values: &[Option<f32>]) -> (f32, f32) {
    let (min, max) = p.fill_value_domain.unwrap_or_else(|| {
        fill_values.iter().copied()
            .flatten()
            .filter(|value| value.is_finite())
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), v| (lo.min(v), hi.max(v)))
    });
    if !min.is_finite() || !max.is_finite() {
        (0.0, 1.0)
    } else if max > min {
        (min, max)
    } else {
        // A zero-length domain cannot be normalized against in the colormap shader.
        (min, min + 1.0)
    }
}

impl PrecomputedViolinMatrixLayer {
    pub fn new(view_params: ViewParams, layer_params: PrecomputedViolinMatrixLayerParams) -> Self {
        Self {
            view_params,
            layer_params,
            sub_layer_instances: Vec::new(),
        }
    }

    /// Returns (margin_top, margin_right, margin_bottom, margin_left), in pixels.
    fn margins(&self) -> (f32, f32, f32, f32) {
        let bounds = if self.layer_params.bounds.is_none() { &self.view_params.margins } else { &self.layer_params.bounds };
        let margin = |get: fn(&MarginParams) -> Option<f32>| bounds.as_ref().and_then(get).unwrap_or(0.0);
        (margin(|m| m.margin_top), margin(|m| m.margin_right), margin(|m| m.margin_bottom), margin(|m| m.margin_left))
    }

    /// Returns (layer_width, layer_height), in pixels.
    fn layer_size(&self) -> (f32, f32) {
        let (margin_top, margin_right, margin_bottom, margin_left) = self.margins();
        let layer_w = (self.view_params.width as f32 - (margin_left + margin_right)).max(1.0);
        let layer_h = (self.view_params.height as f32 - (margin_top + margin_bottom)).max(1.0);
        (layer_w, layer_h)
    }

    fn scales(&self) -> ViolinMatrixScales {
        let (layer_w, layer_h) = self.layer_size();
        ViolinMatrixScales::new(&self.layer_params, layer_w, layer_h)
    }

    fn build_curve_layer(&self, name: &str, outlines: ViolinOutlines, fill_color: ColorMode) -> Box<dyn PreparedAndDraw> {
        let p = &self.layer_params;
        let stroke_width = p.stroke_width.unwrap_or(DEFAULT_STROKE_WIDTH_PX);
        Box::new(CurveLayer::new(
            self.view_params.clone(),
            CurveLayerParams {
                layer_id: format!("{}_precomputed_violin_matrix_{name}_curve_sublayer", p.layer_id),
                bounds: p.bounds.clone(),
                data_unit_mode_x: UnitsMode::Pixels,
                data_unit_mode_y: UnitsMode::Pixels,
                commands: Arc::new(outlines.commands),
                curve_offsets: Some(Arc::new(outlines.curve_offsets)),
                stroked: stroke_width > 0.0,
                filled: true,
                stroke_color: Some(ColorMode::UniformRgb(p.stroke_color.unwrap_or(DEFAULT_STROKE_COLOR))),
                stroke_width: Some(SizeMode::UniformSize(stroke_width)),
                fill_color: Some(fill_color),
                fill_opacity: Some(OpacityMode::UniformOpacity(p.fill_opacity.unwrap_or(DEFAULT_FILL_OPACITY))),
                ..Default::default()
            },
        ))
    }

    /// One curve layer for the violins colored by their fill values, and another for those without one.
    fn build_violin_layers(&self, scales: &ViolinMatrixScales) -> Vec<Box<dyn PreparedAndDraw>> {
        let p = &self.layer_params;
        let spans = normalized_spans(&p.bin_counts, p.density_norm.unwrap_or(ViolinDensityNorm::Width));
        let mut colormapped = ViolinOutlines::new();
        let mut uniform = ViolinOutlines::new();
        for (group_index, group) in p.groups.iter().enumerate() {
            for (var_index, var_name) in p.var_names.iter().enumerate() {
                let cell_index = violin_matrix_cell_index(group_index, var_index, p.var_names.len());
                let Some(cell_spans) = spans.get(cell_index).filter(|s| s.iter().any(|&span| span > 0.0)) else {
                    continue;
                };
                let outline = scales.violin_outline(scales.cell_origin(group, var_name), &p.bin_starts, cell_spans);
                match p.fill_values.as_ref().and_then(|values| values.get(cell_index).copied().flatten()) {
                    Some(fill_value) => colormapped.push(outline, fill_value),
                    None => uniform.push(outline, 0.0),
                }
            }
        }

        let mut layers = Vec::new();
        if !colormapped.is_empty() {
            let fill_value_domain = resolve_fill_value_domain(p, p.fill_values.as_deref().map_or(&[], Vec::as_slice));
            let fill_color = ColorMode::Quantitative(QuantitativeParams {
                values: NumericData::Float32(Arc::new(std::mem::take(&mut colormapped.fill_values))),
                colormap: p.cmap.unwrap_or(QuantitativeColormap::Blues),
                reverse: false,
                domain: Some(fill_value_domain),
            });
            layers.push(self.build_curve_layer("colormapped", colormapped, fill_color));
        }
        if !uniform.is_empty() {
            let fill_color = ColorMode::UniformRgb(p.fill_color.unwrap_or_else(|| default_fill_color(0)));
            layers.push(self.build_curve_layer("uniform", uniform, fill_color));
        }
        layers
    }

    fn build_axis_layers(&self) -> Vec<Box<dyn PreparedAndDraw>> {
        let p = &self.layer_params;
        let var_axis = (Arc::clone(&p.var_names), p.var_axis_title.clone());
        let group_axis = (Arc::clone(&p.groups), p.group_axis_title.clone());
        let (x_axis, y_axis) = if p.swap_axes.unwrap_or(false) { (group_axis, var_axis) } else { (var_axis, group_axis) };
        [(AxisPosition::Bottom, x_axis, "x"), (AxisPosition::Left, y_axis, "y")].into_iter()
            .map(|(position, (domain, title), axis_name)| Box::new(AxisBandLayer::new(
                self.view_params.clone(),
                AxisBandLayerParams {
                    layer_id: format!("{}_precomputed_violin_matrix_{axis_name}_axis_sublayer", p.layer_id),
                    position,
                    domain,
                    title,
                    ..Default::default()
                },
            )) as Box<dyn PreparedAndDraw>)
            .collect()
    }

    /// The colormap legend for `fill_values`, rendered within the plot's right margin.
    fn build_legend_layer(&self) -> Option<LegendColormapQuantitativeLayer> {
        let p = &self.layer_params;
        let fill_values = p.fill_values.as_ref()?;
        let (fill_min, fill_max) = resolve_fill_value_domain(p, fill_values);
        let mut color_scale = ScaleLinear::new();
        color_scale.set_domain((fill_min as f64, fill_max as f64));
        let (margin_top, margin_right, _, _) = self.margins();
        Some(LegendColormapQuantitativeLayer::new(
            self.view_params.clone(),
            LegendColormapQuantitativeLayerParams {
                layer_id: format!("{}_precomputed_violin_matrix_legend_sublayer", p.layer_id),
                bounds: Some(MarginParams {
                    margin_left: Some(self.view_params.width as f32 - margin_right + LEGEND_PADDING_HORIZONTAL),
                    margin_right: Some(LEGEND_PADDING_HORIZONTAL),
                    margin_top: Some(margin_top),
                    margin_bottom: Some(0.0),
                }),
                title: p.fill_value_title.clone().unwrap_or_default(),
                colormap: p.cmap.unwrap_or(QuantitativeColormap::Blues),
                reverse: false,
                scale: Some(color_scale),
                orientation: LegendOrientation::Horizontal,
            },
        ))
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for PrecomputedViolinMatrixLayer {
    async fn prepare(&mut self, gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        let scales = self.scales();
        let mut sublayers = self.build_violin_layers(&scales);
        sublayers.extend(self.build_axis_layers());
        if let Some(legend_layer) = self.build_legend_layer() {
            sublayers.push(Box::new(legend_layer));
        }
        self.sub_layer_instances = sublayers;

        base_prepare_composite_layer(&mut self.sub_layer_instances, gpu_context).await
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for PrecomputedViolinMatrixLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut GpuRenderPass<'_>) {
        base_draw_composite_layer(&self.sub_layer_instances, gpu_context, pass).await;
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for PrecomputedViolinMatrixLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for PrecomputedViolinMatrixLayer {
    async fn draw(&self, ctx: &mut SvgContext) {
        base_draw_composite_layer_svg(&self.sub_layer_instances, ctx).await
    }
}

inventory::submit! {
    crate::registry::LayerRegistration {
        layer_type_name: "PrecomputedViolinMatrixLayer",
        create_layer: |value, view_params| {
            let params: PrecomputedViolinMatrixLayerParams = serde_json::from_value(value).unwrap();
            Box::new(PrecomputedViolinMatrixLayer::new(view_params.clone(), params))
        },
    }
}

impl ExtentableLayer for PrecomputedViolinMatrixLayer {}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
pub struct PrecomputedViolinMatrixPickingInfo {
    /// Index into `bin_counts` (see [`violin_matrix_cell_index`]).
    pub index: usize,
    pub var_name: String,
    pub group: String,
}

impl PickableLayer for PrecomputedViolinMatrixLayer {
    // Picks the cell containing the cursor, rather than only the violin's filled area,
    // since narrow violins would otherwise be hard to hover.
    fn pick(&self, screen_coord: ScreenCoord, _data_coord: Option<DataCoord>) -> Option<LayerPickingResult> {
        let p = &self.layer_params;
        let (_, _, margin_bottom, margin_left) = self.margins();
        let (layer_w, layer_h) = self.layer_size();
        let (num_columns, num_rows) = if p.swap_axes.unwrap_or(false) {
            (p.groups.len(), p.var_names.len())
        } else {
            (p.var_names.len(), p.groups.len())
        };
        let column_index = ViolinMatrixScales::band_index_at((screen_coord.x - margin_left) as f64, layer_w as f64, num_columns)?;
        let row_index = ViolinMatrixScales::band_index_at((screen_coord.y - margin_bottom) as f64, layer_h as f64, num_rows)?;
        let (group_index, var_index) = if p.swap_axes.unwrap_or(false) { (column_index, row_index) } else { (row_index, column_index) };
        Some(LayerPickingResult::new(&p.layer_id, PrecomputedViolinMatrixPickingInfo {
            index: violin_matrix_cell_index(group_index, var_index, p.var_names.len()),
            var_name: p.var_names[var_index].clone(),
            group: p.groups[group_index].clone(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(names: &[&str]) -> Arc<Vec<String>> {
        Arc::new(names.iter().map(|name| name.to_string()).collect())
    }

    fn params(var_names: &[&str], groups: &[&str]) -> PrecomputedViolinMatrixLayerParams {
        PrecomputedViolinMatrixLayerParams {
            var_names: labels(var_names),
            groups: labels(groups),
            bin_starts: Arc::new(vec![0.0, 1.0, 2.0]),
            ..Default::default()
        }
    }

    fn y_coords(outline: &[PathCommand]) -> Vec<f32> {
        outline.iter()
            .filter_map(|command| match *command {
                PathCommand::MoveTo { y, .. } | PathCommand::LineTo { y, .. } | PathCommand::CubicTo { y, .. } => Some(y),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn var_names_are_columns_unless_axes_are_swapped() {
        let p = params(&["g1", "g2"], &["a", "b", "c", "d"]);
        let scales = ViolinMatrixScales::new(&p, 200.0, 400.0);
        assert_eq!(scales.cell_origin(&"c".to_string(), &"g2".to_string()).1, 200.0);
        assert!((scales.cell_origin(&"c".to_string(), &"g2".to_string()).0 - 105.0).abs() < 1e-9);

        let swapped = ViolinMatrixScales::new(&PrecomputedViolinMatrixLayerParams { swap_axes: Some(true), ..p }, 400.0, 200.0);
        assert_eq!(swapped.cell_origin(&"c".to_string(), &"g2".to_string()).1, 100.0);
        assert!((swapped.cell_origin(&"c".to_string(), &"g2".to_string()).0 - 205.0).abs() < 1e-9);
    }

    #[test]
    fn violins_span_the_padded_row_along_the_value_axis() {
        let p = params(&["g1"], &["a", "b"]);
        let scales = ViolinMatrixScales::new(&p, 100.0, 200.0);
        let origin = scales.cell_origin(&"b".to_string(), &"g1".to_string());
        let outline = scales.violin_outline(origin, &p.bin_starts, &[0.5, 1.0, 0.25]);
        let ys = y_coords(&outline);
        assert_eq!(ys.iter().copied().fold(f32::MAX, f32::min), 105.0);
        assert_eq!(ys.iter().copied().fold(f32::MIN, f32::max), 195.0);
    }

    #[test]
    fn band_index_at_divides_the_axis_into_equal_steps() {
        assert_eq!(ViolinMatrixScales::band_index_at(0.0, 300.0, 3), Some(0));
        assert_eq!(ViolinMatrixScales::band_index_at(150.0, 300.0, 3), Some(1));
        assert_eq!(ViolinMatrixScales::band_index_at(300.0, 300.0, 3), Some(2));
        assert_eq!(ViolinMatrixScales::band_index_at(-1.0, 300.0, 3), None);
        assert_eq!(ViolinMatrixScales::band_index_at(10.0, 300.0, 0), None);
    }

    #[test]
    fn fill_value_domain_ignores_missing_and_non_finite_values() {
        let p = params(&[], &[]);
        assert_eq!(resolve_fill_value_domain(&p, &[None, Some(f32::NAN), Some(2.0), Some(5.0)]), (2.0, 5.0));
        assert_eq!(resolve_fill_value_domain(&p, &[None]), (0.0, 1.0));
        assert_eq!(resolve_fill_value_domain(&p, &[Some(3.0), Some(3.0)]), (3.0, 4.0));
    }

    #[test]
    fn picks_the_cell_under_the_cursor() {
        let view_params = ViewParams { width: 300, height: 200, ..Default::default() };
        let layer = PrecomputedViolinMatrixLayer::new(view_params, params(&["g1", "g2", "g3"], &["a", "b"]));
        let result = layer.pick(ScreenCoord { x: 250.0, y: 150.0 }, None).unwrap();
        let info: PrecomputedViolinMatrixPickingInfo = result.into_info().unwrap();
        assert_eq!(info, PrecomputedViolinMatrixPickingInfo { index: 5, var_name: "g3".to_string(), group: "b".to_string() });
    }
}
