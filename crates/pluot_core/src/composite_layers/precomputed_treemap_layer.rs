// The precomputed treemap layer lays out leaves whose values were aggregated
// elsewhere (e.g., per-category counts), wrapping RectLayer and TextLayer.
// The rects are positioned in pixel units within the layer bounds, so they do
// not follow the camera.
use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::composite_layer::{base_draw_composite_layer, base_draw_composite_layer_svg, base_prepare_composite_layer};
use crate::d3::hierarchy::{HierarchyNode, Treemap, TreemapTile};
use crate::emphasis_mode::DEFAULT_BACKGROUND_COLOR;
use crate::layers::rect_layer::{RectLayer, RectLayerParams};
use crate::layers::text_layer::{TextAlignMode, TextBaselineMode, TextLayer, TextLayerParams};
use crate::numeric_data::NumericData;
use crate::picking::LayerPickingResult;
use crate::render_traits::{
    ColorMode, DrawToRasterCpu, DrawToRasterGpu, DrawToSvg, ExtentableLayer, MarginParams, OpacityMode, PickableLayer, PreparedAndDraw, PreparedLayer, UnitsMode, ViewParams
};
use crate::render_types::{CpuContext, CpuRenderPass, PrepareResult};
use crate::gpu_send::GpuRenderPass;
use crate::render_types::GpuContext;
use crate::two::svg::SvgContext;
use crate::viewport::{DataCoord, ScreenCoord};
use crate::wgpu;

const DEFAULT_PADDING: f32 = 1.0;
const DEFAULT_FILL_COLOR: (u8, u8, u8) = (76, 120, 168);
const DEFAULT_TEXT_SIZE: f32 = 10.0;
const DEFAULT_TEXT_COLOR: (u8, u8, u8) = (0, 0, 0);
const LABEL_INSET_PX: f32 = 3.0;
// The TextLayer does not clip, so a label line is only drawn when its width
// (estimated from this average glyph advance, as a fraction of the text size)
// fits within its rect.
const ESTIMATED_GLYPH_WIDTH_EM: f32 = 0.6;

/// Layer params struct for [`PrecomputedTreemapLayer`].
#[derive(Serialize, Deserialize, Debug, Clone, TS)]
#[serde(default)]
#[ts(optional_fields = nullable)]
pub struct PrecomputedTreemapLayerParams {
    pub layer_id: String,
    // If None, assume margin: 0 in all directions.
    pub bounds: Option<MarginParams>,

    /// One entry per leaf: its labels, from the outermost to the innermost level of the hierarchy.
    /// Leaves whose paths share a prefix are nested within the same parent rect.
    pub paths: Arc<Vec<Vec<String>>>,
    /// One value per leaf (e.g., a count), which determines the leaf's area.
    pub values: Arc<Vec<f32>>,
    /// One value per leaf, at most its `values` entry: the selected portion of the leaf (e.g., the
    /// count of selected items). When set, each leaf is drawn in `background_fill_color`, and its
    /// selected portion in `fill_color` in front, filling it from the bottom in proportion.
    /// `None` draws each whole leaf in `fill_color`.
    pub selected_values: Option<Arc<Vec<f32>>>,

    /// Defaults to `Binary`.
    pub tile: Option<TreemapTile>,
    /// The gap, in pixels, between siblings and between a parent and its children. Defaults to 1.
    pub padding: Option<f32>,

    /// How to color each leaf's rect. Modes carrying `NumericData` supply one value per leaf, in the order of `paths`.
    pub fill_color: Option<ColorMode>,
    pub fill_opacity: Option<OpacityMode>,
    /// Fill color of the unselected portion of each leaf, when there are `selected_values`.
    /// Defaults to light grey.
    pub background_fill_color: Option<(u8, u8, u8)>,

    /// Font size of the leaf labels, in pixels. Defaults to 10.
    pub text_size: Option<f32>,
    pub text_color: Option<(u8, u8, u8)>,
}

impl Default for PrecomputedTreemapLayerParams {
    fn default() -> Self {
        Self {
            layer_id: "".to_string(),
            bounds: None,
            paths: Arc::new(vec![]),
            values: Arc::new(vec![]),
            selected_values: None,
            tile: None,
            padding: None,
            fill_color: None,
            fill_opacity: None,
            background_fill_color: None,
            text_size: None,
            text_color: None,
        }
    }
}

/// A leaf's rect within the layer bounds, in pixels, with Y increasing downwards (as in D3).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LeafRect {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl LeafRect {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        self.x1 > self.x0 && self.y1 > self.y0 && x >= self.x0 && x <= self.x1 && y >= self.y0 && y <= self.y1
    }
}

pub struct PrecomputedTreemapLayer {
    view_params: ViewParams,
    layer_params: PrecomputedTreemapLayerParams,
    sub_layer_instances: Vec<Box<dyn PreparedAndDraw>>,
    // One per leaf, in the order of `paths`.
    leaf_rects: Vec<LeafRect>,
}

/// Groups leaves by their labels level by level, so that each distinct label prefix becomes one parent node.
/// Each leaf node's data is its index into `paths`; parent nodes have no data.
pub fn build_hierarchy(paths: &[Vec<String>]) -> HierarchyNode<Option<usize>> {
    HierarchyNode::new(None, build_children(paths, (0..paths.len()).collect(), 0))
}

fn build_children(paths: &[Vec<String>], indices: Vec<usize>, depth: usize) -> Vec<HierarchyNode<Option<usize>>> {
    let mut children = Vec::new();
    let mut group_indices: Vec<Vec<usize>> = Vec::new();
    let mut group_by_label: HashMap<&str, usize> = HashMap::new();
    for index in indices {
        match paths[index].get(depth) {
            None => children.push(HierarchyNode::new(Some(index), vec![])),
            Some(label) => {
                let group = *group_by_label.entry(label.as_str()).or_insert_with(|| {
                    group_indices.push(Vec::new());
                    group_indices.len() - 1
                });
                group_indices[group].push(index);
            }
        }
    }
    for indices in group_indices {
        // A label that ends exactly one path is that path's leaf, rather than a parent wrapping it,
        // so that the leaf is not inset by an extra level of padding.
        match indices[..] {
            [index] if paths[index].len() == depth + 1 => children.push(HierarchyNode::new(Some(index), vec![])),
            _ => children.push(HierarchyNode::new(None, build_children(paths, indices, depth + 1))),
        }
    }
    children
}

/// Returns one rect per entry of `paths`.
pub fn layout_leaves(paths: &[Vec<String>], values: &[f32], size: (f32, f32), tile: TreemapTile, padding: f32) -> Vec<LeafRect> {
    let mut root = build_hierarchy(paths);
    root.sum(&|leaf_index| leaf_index.and_then(|i| values.get(i)).map_or(0.0, |&v| v.max(0.0) as f64))
        .sort_by(&|a, b| b.value.total_cmp(&a.value));

    let mut treemap = Treemap::new();
    treemap
        .set_tile(tile)
        .set_size((size.0 as f64, size.1 as f64))
        .set_padding(padding as f64)
        .set_round(true);
    treemap.layout(&mut root);

    let mut rects = vec![LeafRect { x0: 0.0, y0: 0.0, x1: 0.0, y1: 0.0 }; paths.len()];
    root.each_before(&mut |node| {
        if let Some(index) = node.data {
            rects[index] = LeafRect { x0: node.x0 as f32, y0: node.y0 as f32, x1: node.x1 as f32, y1: node.y1 as f32 };
        }
    });
    rects
}

pub fn format_value(value: f32) -> String {
    if value.fract() != 0.0 || value.abs() >= 1e15 {
        return value.to_string();
    }
    let digits = (value.abs() as u64).to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3 + 1);
    if value < 0.0 {
        grouped.push('-');
    }
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}

/// The lines of a leaf's label, from top to bottom: its labels from innermost to outermost, then
/// its value, preceded by its selected value if any.
pub fn label_lines(path: &[String], value: f32, selected_value: Option<f32>) -> Vec<String> {
    let value_line = match selected_value {
        Some(selected_value) => format!("{} / {}", format_value(selected_value), format_value(value)),
        None => format_value(value),
    };
    path.iter().rev().cloned().chain(std::iter::once(value_line)).collect()
}

/// The bottom part of `rect` whose share of its area is `selected_value / value`.
pub fn selected_rect(rect: &LeafRect, value: f32, selected_value: f32) -> LeafRect {
    let fraction = if value > 0.0 { (selected_value / value).clamp(0.0, 1.0) } else { 0.0 };
    LeafRect { y0: rect.y1 - (rect.y1 - rect.y0) * fraction, ..*rect }
}

impl PrecomputedTreemapLayer {
    pub fn new(view_params: ViewParams, layer_params: PrecomputedTreemapLayerParams) -> Self {
        Self {
            view_params,
            layer_params,
            sub_layer_instances: Vec::new(),
            leaf_rects: Vec::new(),
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

    fn selected_value(&self, index: usize) -> Option<f32> {
        self.layer_params.selected_values.as_ref().map(|selected_values| selected_values.get(index).copied().unwrap_or(0.0))
    }

    fn build_rect_layers(&self, layer_h: f32) -> Vec<RectLayer> {
        let p = &self.layer_params;
        let fill_color = p.fill_color.clone().unwrap_or(ColorMode::UniformRgb(DEFAULT_FILL_COLOR));
        if p.selected_values.is_none() {
            return vec![self.build_rect_layer("rect", &self.leaf_rects, fill_color, layer_h)];
        }
        let selected_rects: Vec<LeafRect> = self
            .leaf_rects
            .iter()
            .enumerate()
            .map(|(index, rect)| selected_rect(rect, p.values.get(index).copied().unwrap_or(0.0), self.selected_value(index).unwrap_or(0.0)))
            .collect();
        let background_fill_color = ColorMode::UniformRgb(p.background_fill_color.unwrap_or(DEFAULT_BACKGROUND_COLOR));
        vec![
            self.build_rect_layer("background_rect", &self.leaf_rects, background_fill_color, layer_h),
            self.build_rect_layer("rect", &selected_rects, fill_color, layer_h),
        ]
    }

    fn build_rect_layer(&self, name: &str, rects: &[LeafRect], fill_color: ColorMode, layer_h: f32) -> RectLayer {
        let p = &self.layer_params;
        // Sublayers use pixel units with Y increasing upwards, unlike the D3-style layout.
        let position_x0 = rects.iter().map(|r| r.x0).collect();
        let position_y0 = rects.iter().map(|r| layer_h - r.y1).collect();
        let position_x1 = rects.iter().map(|r| r.x1).collect();
        let position_y1 = rects.iter().map(|r| layer_h - r.y0).collect();

        RectLayer::new(
            self.view_params.clone(),
            RectLayerParams {
                layer_id: format!("{}_precomputed_treemap_{name}_sublayer", p.layer_id),
                bounds: p.bounds.clone(),
                data_unit_mode_x: UnitsMode::Pixels,
                data_unit_mode_y: UnitsMode::Pixels,
                fill_color: Some(fill_color),
                fill_opacity: p.fill_opacity.clone(),
                position_x0: NumericData::Float32(Arc::new(position_x0)),
                position_y0: NumericData::Float32(Arc::new(position_y0)),
                position_x1: NumericData::Float32(Arc::new(position_x1)),
                position_y1: NumericData::Float32(Arc::new(position_y1)),
                ..Default::default()
            },
        )
    }

    fn build_text_layer(&self, layer_h: f32) -> Option<TextLayer> {
        let p = &self.layer_params;
        let text_size = p.text_size.unwrap_or(DEFAULT_TEXT_SIZE);

        let mut position_x = Vec::new();
        let mut position_y = Vec::new();
        let mut text_vec = Vec::new();
        for (index, rect) in self.leaf_rects.iter().enumerate() {
            let value = p.values.get(index).copied().unwrap_or(0.0);
            let lines = label_lines(&p.paths[index], value, self.selected_value(index));
            let max_line_width = rect.x1 - rect.x0 - 2.0 * LABEL_INSET_PX;
            let last_line = lines.len() - 1;
            for (line_index, line) in lines.into_iter().enumerate() {
                // Matches the line spacing of Vitessce's treemap, which sets the value apart from the labels.
                let extra_gap = if line_index == last_line { 0.3 } else { 0.0 };
                let baseline_y = rect.y0 + (1.1 + 0.9 * line_index as f32 + extra_gap) * text_size;
                if baseline_y > rect.y1 - LABEL_INSET_PX {
                    break;
                }
                if line.chars().count() as f32 * ESTIMATED_GLYPH_WIDTH_EM * text_size > max_line_width {
                    continue;
                }
                position_x.push(rect.x0 + LABEL_INSET_PX);
                position_y.push(layer_h - baseline_y);
                text_vec.push(line);
            }
        }
        if text_vec.is_empty() {
            return None;
        }

        Some(TextLayer::new(
            self.view_params.clone(),
            TextLayerParams {
                layer_id: format!("{}_precomputed_treemap_text_sublayer", p.layer_id),
                bounds: p.bounds.clone(),
                data_unit_mode_x: UnitsMode::Pixels,
                data_unit_mode_y: UnitsMode::Pixels,
                text_size,
                text_size_unit_mode: UnitsMode::Pixels,
                text_align_mode: TextAlignMode::Start,
                text_baseline_mode: TextBaselineMode::Alphabetic,
                fill_color: Some(ColorMode::UniformRgb(p.text_color.unwrap_or(DEFAULT_TEXT_COLOR))),
                position_x: NumericData::Float32(Arc::new(position_x)),
                position_y: NumericData::Float32(Arc::new(position_y)),
                text_vec: Arc::new(text_vec),
                ..Default::default()
            },
        ))
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for PrecomputedTreemapLayer {
    async fn prepare(&mut self, gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        let (_, _, layer_w, layer_h) = self.layer_box();
        let p = &self.layer_params;
        self.leaf_rects = layout_leaves(
            &p.paths,
            &p.values,
            (layer_w, layer_h),
            p.tile.unwrap_or_default(),
            p.padding.unwrap_or(DEFAULT_PADDING),
        );

        let mut sublayers: Vec<Box<dyn PreparedAndDraw>> =
            self.build_rect_layers(layer_h).into_iter().map(|layer| Box::new(layer) as Box<dyn PreparedAndDraw>).collect();
        if let Some(text_layer) = self.build_text_layer(layer_h) {
            sublayers.push(Box::new(text_layer));
        }
        self.sub_layer_instances = sublayers;

        base_prepare_composite_layer(&mut self.sub_layer_instances, gpu_context).await
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for PrecomputedTreemapLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut GpuRenderPass<'_>) {
        base_draw_composite_layer(&self.sub_layer_instances, gpu_context, pass).await;
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for PrecomputedTreemapLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for PrecomputedTreemapLayer {
    async fn draw(&self, ctx: &mut SvgContext) {
        base_draw_composite_layer_svg(&self.sub_layer_instances, ctx).await
    }
}

inventory::submit! {
    crate::registry::LayerRegistration {
        layer_type_name: "PrecomputedTreemapLayer",
        create_layer: |value, view_params| {
            let params: PrecomputedTreemapLayerParams = serde_json::from_value(value).unwrap();
            Box::new(PrecomputedTreemapLayer::new(view_params.clone(), params))
        },
    }
}

impl ExtentableLayer for PrecomputedTreemapLayer {}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
pub struct PrecomputedTreemapPickingInfo {
    /// Index into `paths` and `values`.
    pub index: usize,
    pub path: Vec<String>,
    pub value: f32,
    /// The leaf's `selected_values` entry, if any.
    pub selected_value: Option<f32>,
}

impl PickableLayer for PrecomputedTreemapLayer {
    // RectLayer does not support picking in pixel units, so the leaf rects are hit-tested here instead.
    fn pick(&self, screen_coord: ScreenCoord, _data_coord: Option<DataCoord>) -> Option<LayerPickingResult> {
        let (margin_left, margin_bottom, _, layer_h) = self.layer_box();
        let x = screen_coord.x - margin_left;
        let y = layer_h - (screen_coord.y - margin_bottom);
        let index = self.leaf_rects.iter().position(|rect| rect.contains(x, y))?;
        let p = &self.layer_params;
        Some(LayerPickingResult::new(&p.layer_id, PrecomputedTreemapPickingInfo {
            index,
            path: p.paths[index].clone(),
            value: p.values.get(index).copied().unwrap_or(0.0),
            selected_value: self.selected_value(index),
        }))
    }
}
