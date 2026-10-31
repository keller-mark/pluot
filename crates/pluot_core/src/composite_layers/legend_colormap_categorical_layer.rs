// A layer that renders a legend for a categorical colormap: a title, then one
// row per category with a color swatch and its label, from top to bottom.
// Rows that do not fit within the bounds are summarized by a final "+N more" row.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::composite_layer::{base_draw_composite_layer, base_draw_composite_layer_svg, base_prepare_composite_layer};
use crate::layers::heatmap_layer::DEFAULT_BACKGROUND_OPACITY;
use crate::layers::rect_layer::{RectLayer, RectLayerParams};
use crate::layers::text_layer::{TextAlignMode, TextBaselineMode, TextLayer, TextLayerParams};
use crate::numeric_data::NumericData;
use crate::picking::LayerPickingResult;
use crate::render_traits::{
    CategoricalColormap, CategoricalParams, ColorMode, ExtentableLayer, InstancedOpacityParams, MarginParams, OpacityMode, PickableLayer,
    PreparedAndDraw, PreparedLayer, UnitsMode, ViewParams,
};
use crate::render_types::GpuContext;
use crate::gpu_send::GpuRenderPass;
use crate::render_types::{CpuContext, CpuRenderPass, PrepareResult};
use crate::render_traits::{DrawToRasterCpu, DrawToRasterGpu, DrawToSvg};
use crate::two::svg::SvgContext;
use crate::viewport::{DataCoord, ScreenCoord};
use crate::wgpu;

const DEFAULT_TITLE_FONT_SIZE: f32 = 12.0;
const DEFAULT_TITLE_HEIGHT_PX: f32 = 16.0;
const DEFAULT_LABEL_FONT_SIZE: f32 = 10.0;
const ROW_HEIGHT_PX: f32 = 14.0;
const SWATCH_SIZE_PX: f32 = 10.0;
const SWATCH_LABEL_GAP_PX: f32 = 4.0;
// The TextLayer does not clip, so labels are truncated to the legend width
// using this estimate of the average glyph advance, as a fraction of the text size.
const ESTIMATED_GLYPH_WIDTH_EM: f32 = 0.6;

/// Layer params struct for [`LegendColormapCategoricalLayer`].
#[derive(Serialize, Deserialize, Debug, Clone, TS)]
#[serde(default)]
#[ts(optional_fields = nullable)]
pub struct LegendColormapCategoricalLayerParams {
    pub layer_id: String,
    /// Margins defining the legend's overall bounding box within the viewport.
    pub bounds: Option<MarginParams>,
    /// Title text rendered above the rows. Empty draws no title.
    pub title: String,
    /// One label per category code.
    pub categories: Arc<Vec<String>>,
    /// The codes of the categories to list, in order (e.g., only those of
    /// filter-included items). `None` lists every category in code order.
    pub codes: Option<Vec<u32>>,
    /// The codes of the selected categories, whose swatches are drawn at full
    /// opacity. The others are drawn at `background_opacity`. `None` selects every category.
    pub selected_codes: Option<Vec<u32>>,
    /// Indexed by category code, wrapping around its number of colors. Defaults to Tableau10.
    pub colormap: Option<CategoricalColormap>,
    /// Opacity of the swatches of unselected categories.
    pub background_opacity: Option<f32>,
}

impl Default for LegendColormapCategoricalLayerParams {
    fn default() -> Self {
        Self {
            layer_id: "".to_string(),
            bounds: None,
            title: "".to_string(),
            categories: Arc::new(vec![]),
            codes: None,
            selected_codes: None,
            colormap: None,
            background_opacity: None,
        }
    }
}

/// One listed category, with its row's vertical center in pixels (Y increasing upwards).
struct LegendRow {
    code: u32,
    center_y: f32,
}

pub struct LegendColormapCategoricalLayer {
    view_params: ViewParams,
    layer_params: LegendColormapCategoricalLayerParams,
    sub_layer_instances: Vec<Box<dyn PreparedAndDraw>>,
    rows: Vec<LegendRow>,
}

impl LegendColormapCategoricalLayer {
    pub fn new(view_params: ViewParams, layer_params: LegendColormapCategoricalLayerParams) -> Self {
        Self {
            view_params,
            layer_params,
            sub_layer_instances: Vec::new(),
            rows: Vec::new(),
        }
    }

    /// (left, right, top, bottom) of the legend, in pixels with Y increasing upwards.
    fn legend_box(&self) -> (f32, f32, f32, f32) {
        let bounds = self.layer_params.bounds.as_ref();
        let margin = |get: fn(&MarginParams) -> Option<f32>| bounds.and_then(get).unwrap_or(0.0);
        (
            margin(|m| m.margin_left),
            self.view_params.width as f32 - margin(|m| m.margin_right),
            self.view_params.height as f32 - margin(|m| m.margin_top),
            margin(|m| m.margin_bottom),
        )
    }

    fn listed_codes(&self) -> Vec<u32> {
        let p = &self.layer_params;
        match &p.codes {
            Some(codes) => codes.iter().copied().filter(|&code| (code as usize) < p.categories.len()).collect(),
            None => (0..p.categories.len() as u32).collect(),
        }
    }

    /// The rows that fit within the legend, plus the number of listed categories
    /// that do not and the vertical center of the row summarizing them.
    fn layout_rows(&self) -> (Vec<LegendRow>, Option<(usize, f32)>) {
        let (_, _, top, bottom) = self.legend_box();
        let rows_top = if self.layer_params.title.is_empty() { top } else { top - DEFAULT_TITLE_HEIGHT_PX };
        let row_center_y = |row: usize| rows_top - (row as f32 + 0.5) * ROW_HEIGHT_PX;
        let max_rows = ((rows_top - bottom) / ROW_HEIGHT_PX).floor().max(0.0) as usize;
        let codes = self.listed_codes();
        let num_shown = if codes.len() <= max_rows { codes.len() } else { max_rows.saturating_sub(1) };
        let rows = codes.iter().take(num_shown).enumerate().map(|(row, &code)| LegendRow { code, center_y: row_center_y(row) }).collect();
        let num_hidden = codes.len() - num_shown;
        (rows, (num_hidden > 0 && max_rows > 0).then(|| (num_hidden, row_center_y(num_shown))))
    }

    fn truncated_label(&self, label: &str) -> String {
        let (left, right, _, _) = self.legend_box();
        let max_chars = ((right - left - SWATCH_SIZE_PX - SWATCH_LABEL_GAP_PX) / (ESTIMATED_GLYPH_WIDTH_EM * DEFAULT_LABEL_FONT_SIZE)).floor().max(1.0) as usize;
        if label.chars().count() <= max_chars {
            return label.to_string();
        }
        let mut truncated: String = label.chars().take(max_chars.saturating_sub(1)).collect();
        truncated.push('…');
        truncated
    }

    fn build_sublayers(&self, rows: &[LegendRow], hidden: Option<(usize, f32)>) -> Vec<Box<dyn PreparedAndDraw>> {
        let p = &self.layer_params;
        let (left, _, top, _) = self.legend_box();
        let layer_id = |name: &str| format!("{}_legend_colormap_categorical_layer_{name}_sublayer", p.layer_id);
        let zero_bounds = MarginParams {
            margin_top: Some(0.0),
            margin_right: Some(0.0),
            margin_bottom: Some(0.0),
            margin_left: Some(0.0),
        };
        let text_layer = |name: &str, align: TextAlignMode, baseline: TextBaselineMode, size: f32, xs: Vec<f32>, ys: Vec<f32>, texts: Vec<String>| {
            TextLayer::new(
                self.view_params.clone(),
                TextLayerParams {
                    layer_id: layer_id(name),
                    bounds: Some(zero_bounds.clone()),
                    data_unit_mode_x: UnitsMode::Pixels,
                    data_unit_mode_y: UnitsMode::Pixels,
                    text_size: size,
                    text_size_unit_mode: UnitsMode::Pixels,
                    text_align_mode: align,
                    text_baseline_mode: baseline,
                    position_x: NumericData::Float32(Arc::new(xs)),
                    position_y: NumericData::Float32(Arc::new(ys)),
                    text_vec: Arc::new(texts),
                    ..Default::default()
                },
            )
        };

        let mut sublayers: Vec<Box<dyn PreparedAndDraw>> = Vec::new();
        if !p.title.is_empty() {
            sublayers.push(Box::new(text_layer(
                "title", TextAlignMode::Start, TextBaselineMode::Top, DEFAULT_TITLE_FONT_SIZE, vec![left], vec![top], vec![p.title.clone()],
            )));
        }
        if !rows.is_empty() {
            let swatch_opacities: Vec<f32> = rows
                .iter()
                .map(|row| match &p.selected_codes {
                    Some(selected_codes) if !selected_codes.contains(&row.code) => p.background_opacity.unwrap_or(DEFAULT_BACKGROUND_OPACITY),
                    _ => 1.0,
                })
                .collect();
            let half_swatch = SWATCH_SIZE_PX / 2.0;
            sublayers.push(Box::new(RectLayer::new(
                self.view_params.clone(),
                RectLayerParams {
                    layer_id: layer_id("swatch"),
                    bounds: Some(zero_bounds.clone()),
                    data_unit_mode_x: UnitsMode::Pixels,
                    data_unit_mode_y: UnitsMode::Pixels,
                    fill_color: Some(ColorMode::Categorical(CategoricalParams {
                        codes: NumericData::from(rows.iter().map(|row| row.code).collect::<Vec<u32>>()),
                        colormap: p.colormap.unwrap_or(CategoricalColormap::Tableau10),
                    })),
                    fill_opacity: Some(OpacityMode::InstancedOpacity(InstancedOpacityParams { values: NumericData::from(swatch_opacities) })),
                    position_x0: NumericData::from(vec![left; rows.len()]),
                    position_x1: NumericData::from(vec![left + SWATCH_SIZE_PX; rows.len()]),
                    position_y0: NumericData::from(rows.iter().map(|row| row.center_y - half_swatch).collect::<Vec<f32>>()),
                    position_y1: NumericData::from(rows.iter().map(|row| row.center_y + half_swatch).collect::<Vec<f32>>()),
                    ..Default::default()
                },
            )));
        }

        let label_x = left + SWATCH_SIZE_PX + SWATCH_LABEL_GAP_PX;
        let mut label_ys: Vec<f32> = rows.iter().map(|row| row.center_y).collect();
        let mut labels: Vec<String> = rows.iter().map(|row| self.truncated_label(&p.categories[row.code as usize])).collect();
        if let Some((num_hidden, center_y)) = hidden {
            label_ys.push(center_y);
            labels.push(format!("+{num_hidden} more"));
        }
        if !labels.is_empty() {
            sublayers.push(Box::new(text_layer(
                "labels", TextAlignMode::Start, TextBaselineMode::Middle, DEFAULT_LABEL_FONT_SIZE, vec![label_x; labels.len()], label_ys, labels,
            )));
        }
        sublayers
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for LegendColormapCategoricalLayer {
    async fn prepare(&mut self, gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        let (rows, hidden) = self.layout_rows();
        self.sub_layer_instances = self.build_sublayers(&rows, hidden);
        self.rows = rows;
        base_prepare_composite_layer(&mut self.sub_layer_instances, gpu_context).await
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for LegendColormapCategoricalLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut GpuRenderPass<'_>) {
        base_draw_composite_layer(&self.sub_layer_instances, gpu_context, pass).await;
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for LegendColormapCategoricalLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for LegendColormapCategoricalLayer {
    async fn draw(&self, ctx: &mut SvgContext) {
        base_draw_composite_layer_svg(&self.sub_layer_instances, ctx).await
    }
}

inventory::submit! {
    crate::registry::LayerRegistration {
        layer_type_name: "LegendColormapCategoricalLayer",
        create_layer: |value, view_params| {
            let params: LegendColormapCategoricalLayerParams = serde_json::from_value(value).unwrap();
            Box::new(LegendColormapCategoricalLayer::new(view_params.clone(), params))
        },
    }
}

impl ExtentableLayer for LegendColormapCategoricalLayer {}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
pub struct LegendColormapCategoricalPickingInfo {
    pub code: u32,
    pub category: String,
}

impl PickableLayer for LegendColormapCategoricalLayer {
    /// The category of the row under `screen_coord`, anywhere across the legend's width.
    fn pick(&self, screen_coord: ScreenCoord, _data_coord: Option<DataCoord>) -> Option<LayerPickingResult> {
        let (left, right, _, _) = self.legend_box();
        if screen_coord.x < left || screen_coord.x > right {
            return None;
        }
        let row = self.rows.iter().find(|row| (screen_coord.y - row.center_y).abs() <= ROW_HEIGHT_PX / 2.0)?;
        Some(LayerPickingResult::new(&self.layer_params.layer_id, LegendColormapCategoricalPickingInfo {
            code: row.code,
            category: self.layer_params.categories[row.code as usize].clone(),
        }))
    }
}
