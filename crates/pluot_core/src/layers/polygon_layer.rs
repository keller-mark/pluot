// PolygonLayer — renders a collection of polygons as stroked outlines, filled
// interiors, or both.
//
// Each polygon is a ring of model-space (x, y) vertices. Rendering is split
// across two internal sub-layers:
//
//   FilledPolygonLayer — triangulates each ring via earcut on the CPU and renders
//     the resulting triangles with FilledCurveLayer's shader.
//
//   StrokedPolygonLayer — closes each ring into a polyline (appending the first
//     point at the end) and renders with StrokedCurveLayer's round-join shader.
//
// SVG rendering emits one closed TwoPath per polygon.

use earcut::Earcut;
use glam::{Mat4, Vec4};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::positioning::get_point_position;
use crate::render_traits::{
    AspectRatioAlignmentMode, AspectRatioMode, DrawToRasterCpu, DrawToRasterGpu, DrawToSvg,
    MarginParams, PickableLayer, PreparedLayer, UnitsMode, ViewParams,
};
use crate::render_types::{CpuContext, CpuRenderPass, GpuContext, PrepareResult, RenderResult};
use crate::two::shapes::{TwoColor, TwoElement, TwoGroup, TwoPath};
use crate::two::svg::{update_svg, SvgContext};
use crate::wgpu;

use super::filled_curve_layer::FilledCurveLayer;
use super::filled_polygon_layer::FilledPolygonLayer;
use super::stroked_curve_layer::StrokedCurveLayer;
use super::stroked_polygon_layer::StrokedPolygonLayer;

// ── Params ─────────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct PolygonLayerParams {
    pub layer_id: String,
    /// If `None`, the view-level margins are used.
    pub bounds: Option<MarginParams>,
    pub data_unit_mode_x: UnitsMode,
    pub data_unit_mode_y: UnitsMode,
    pub model_matrix: Option<[f32; 16]>,

    /// One polygon per element; each polygon is a ring of (x, y) model-space vertices.
    /// Rings with fewer than 3 points are silently skipped.
    pub polygons: Arc<Vec<Vec<(f32, f32)>>>,

    /// Whether to stroke the polygon outlines. Defaults to `true`.
    pub stroked: bool,
    /// Whether to fill the polygon interiors. Defaults to `false`.
    pub filled: bool,

    /// RGBA stroke color in [0, 1]. Defaults to opaque black.
    pub stroke_color: [f32; 4],
    /// Stroke width in pixels. Defaults to 1.
    pub stroke_width: f32,
    /// Additional opacity multiplier for the stroke. Defaults to 1.
    pub stroke_opacity: f32,

    /// RGBA fill color in [0, 1]. Defaults to opaque black.
    pub fill_color: [f32; 4],
    /// Additional opacity multiplier for the fill. Defaults to 1.
    pub fill_opacity: f32,
}

impl Default for PolygonLayerParams {
    fn default() -> Self {
        Self {
            layer_id: "".to_string(),
            bounds: None,
            data_unit_mode_x: UnitsMode::Data,
            data_unit_mode_y: UnitsMode::Data,
            model_matrix: None,
            polygons: Arc::new(vec![]),
            stroked: true,
            filled: false,
            stroke_color: [0.0, 0.0, 0.0, 1.0],
            stroke_width: 1.0,
            stroke_opacity: 1.0,
            fill_color: [0.0, 0.0, 0.0, 1.0],
            fill_opacity: 1.0,
        }
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────────

fn resolve_margins(params: &PolygonLayerParams, view: &ViewParams) -> (f64, f64, f64, f64) {
    let b = if params.bounds.is_none() { &view.margins } else { &params.bounds };
    let ml = b.as_ref().and_then(|m| m.margin_left).unwrap_or(0.0) as f64;
    let mt = b.as_ref().and_then(|m| m.margin_top).unwrap_or(0.0) as f64;
    let mr = b.as_ref().and_then(|m| m.margin_right).unwrap_or(0.0) as f64;
    let mb = b.as_ref().and_then(|m| m.margin_bottom).unwrap_or(0.0) as f64;
    (ml, mt, mr, mb)
}

/// Triangulate a polygon ring using earcut, appending flat (x, y) triangle
/// vertices to `out`. Rings with fewer than 3 vertices are skipped.
fn triangulate_ring(ring: &[(f32, f32)], ec: &mut Earcut<f32>, indices: &mut Vec<u32>, out: &mut Vec<(f32, f32)>) {
    if ring.len() < 3 {
        return;
    }
    ec.earcut(ring.iter().map(|&(x, y)| [x, y]), &[] as &[u32], indices);
    for &i in indices.iter() {
        out.push(ring[i as usize]);
    }
}

/// Build a closed polyline from a polygon ring.
/// The ring is closed by appending the first point if the last point differs from it.
fn ring_to_closed_polyline(ring: &[(f32, f32)]) -> Vec<(f32, f32)> {
    if ring.len() < 2 {
        return vec![];
    }
    let mut pts = ring.to_vec();
    if pts.first() != pts.last() {
        pts.push(pts[0]);
    }
    pts
}

// ── Layer ──────────────────────────────────────────────────────────────────────

pub struct PolygonLayer {
    view_params: ViewParams,
    layer_params: PolygonLayerParams,
    stroked: Option<StrokedPolygonLayer>,
    filled: Option<FilledPolygonLayer>,
}

impl PolygonLayer {
    pub fn new(view_params: ViewParams, layer_params: PolygonLayerParams) -> Self {
        let camera_view = view_params.camera_view.unwrap_or([
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ]);

        let (margin_left, margin_top, margin_right, margin_bottom) =
            resolve_margins(&layer_params, &view_params);

        let data_unit_mode_x = match layer_params.data_unit_mode_x {
            UnitsMode::Pixels => 0,
            UnitsMode::Data => 1,
        };
        let data_unit_mode_y = match layer_params.data_unit_mode_y {
            UnitsMode::Pixels => 0,
            UnitsMode::Data => 1,
        };
        let aspect_ratio_mode = match view_params.aspect_ratio_mode {
            AspectRatioMode::Ignore => 0,
            AspectRatioMode::Contain => 1,
            AspectRatioMode::Cover => 2,
        };
        let aspect_ratio_alignment_mode = match view_params.aspect_ratio_alignment_mode {
            AspectRatioAlignmentMode::Center => 0,
            AspectRatioAlignmentMode::Start => 1,
            AspectRatioAlignmentMode::End => 2,
        };
        let model_matrix = Mat4::from_cols_array(&layer_params.model_matrix.unwrap_or([
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ]));

        let filled = if layer_params.filled {
            let mut ec = Earcut::new();
            let mut indices = Vec::new();
            let mut fill_vertices: Vec<(f32, f32)> = Vec::new();
            for ring in layer_params.polygons.iter() {
                triangulate_ring(ring, &mut ec, &mut indices, &mut fill_vertices);
            }
            if fill_vertices.is_empty() {
                None
            } else {
                let [r, g, b, a] = layer_params.fill_color;
                Some(FilledPolygonLayer(FilledCurveLayer {
                    view_params: view_params.clone(),
                    fill_color: Vec4::new(r, g, b, a * layer_params.fill_opacity),
                    fill_vertices,
                    data_unit_mode_x,
                    data_unit_mode_y,
                    aspect_ratio_mode,
                    aspect_ratio_alignment_mode,
                    model_matrix,
                    margin_left,
                    margin_top,
                    margin_right,
                    margin_bottom,
                    camera_view,
                }))
            }
        } else {
            None
        };

        let stroked = if layer_params.stroked && !layer_params.polygons.is_empty() {
            let polylines: Vec<Vec<(f32, f32)>> = layer_params.polygons.iter()
                .map(|ring| ring_to_closed_polyline(ring))
                .filter(|pts| pts.len() >= 2)
                .collect();
            if polylines.is_empty() {
                None
            } else {
                let [r, g, b, a] = layer_params.stroke_color;
                Some(StrokedPolygonLayer(StrokedCurveLayer {
                    view_params: view_params.clone(),
                    stroke_color: Vec4::new(r, g, b, a * layer_params.stroke_opacity),
                    stroke_width: layer_params.stroke_width,
                    polylines,
                    data_unit_mode_x,
                    data_unit_mode_y,
                    aspect_ratio_mode,
                    aspect_ratio_alignment_mode,
                    model_matrix,
                    margin_left,
                    margin_top,
                    margin_right,
                    margin_bottom,
                    camera_view,
                }))
            }
        } else {
            None
        };

        Self { view_params, layer_params, stroked, filled }
    }
}

// ── Trait impls ────────────────────────────────────────────────────────────────

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for PolygonLayer {
    async fn prepare(&mut self, gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        if let Some(f) = &mut self.filled  { f.prepare(gpu_context).await; }
        if let Some(s) = &mut self.stroked { s.prepare(gpu_context).await; }
        PrepareResult { bailed_early: false }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for PolygonLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        // Fill first so the stroke renders on top.
        if let Some(f) = &self.filled  { f.draw(gpu_context, pass).await; }
        if let Some(s) = &self.stroked { s.draw(gpu_context, pass).await; }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for PolygonLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for PolygonLayer {
    async fn draw(&self, ctx: &mut SvgContext) {
        let Self { layer_params, view_params, .. } = self;

        let camera_view = view_params.camera_view.unwrap_or([
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ]);

        let (margin_left, margin_top, margin_right, margin_bottom) =
            resolve_margins(layer_params, view_params);

        let viewport_w = view_params.width as f32;
        let viewport_h = view_params.height as f32;
        let layer_w = viewport_w - (margin_left + margin_right) as f32;
        let layer_h = viewport_h - (margin_top + margin_bottom) as f32;

        let to_px = |x: f32, y: f32| -> (f64, f64) {
            let (px, py) = get_point_position(
                x, y,
                layer_w, layer_h,
                &camera_view,
                layer_params.data_unit_mode_x.clone(),
                layer_params.data_unit_mode_y.clone(),
                view_params.aspect_ratio_mode.clone(),
                view_params.aspect_ratio_alignment_mode.clone(),
                layer_params.model_matrix.as_ref().map(|m| m.as_slice()),
            );
            (px as f64, (layer_h - py) as f64)
        };

        let to_rgba = |[r, g, b, a]: [f32; 4], opacity: f32| -> TwoColor {
            TwoColor::Rgba((
                (r * 255.0).round().clamp(0.0, 255.0) as u8,
                (g * 255.0).round().clamp(0.0, 255.0) as u8,
                (b * 255.0).round().clamp(0.0, 255.0) as u8,
                (a * opacity * 255.0).round().clamp(0.0, 255.0) as u8,
            ))
        };

        let stroke = if layer_params.stroked {
            Some(to_rgba(layer_params.stroke_color, layer_params.stroke_opacity))
        } else {
            None
        };
        let fill = if layer_params.filled {
            Some(to_rgba(layer_params.fill_color, layer_params.fill_opacity))
        } else {
            None
        };

        let mut svg_elements: Vec<TwoElement> = Vec::with_capacity(layer_params.polygons.len());
        for ring in layer_params.polygons.iter() {
            if ring.len() < 3 {
                continue;
            }
            // Project ring to pixel space and close it (append first point) so
            // TwoPath emits a fully closed path outline.
            let mut points: Vec<(f64, f64)> = ring.iter().map(|&(x, y)| to_px(x, y)).collect();
            points.push(points[0]);
            svg_elements.push(TwoElement::Path(TwoPath {
                points,
                stroke: stroke.clone(),
                fill: fill.clone(),
                linewidth: layer_params.stroke_width as f64,
                opacity: 1.0,
            }));
        }

        let svg_elements = vec![TwoElement::Group(TwoGroup {
            elements: svg_elements,
            translate: Some((margin_left, margin_top)),
            layer_id: Some(layer_params.layer_id.clone()),
            clip_rect: Some((0.0, 0.0, layer_w as f64, layer_h as f64)),
            ..Default::default()
        })];

        update_svg(ctx, &svg_elements);
    }
}

inventory::submit! {
    crate::registry::LayerRegistration {
        layer_type_name: "PolygonLayer",
        create_layer: |value, view_params| {
            let params: PolygonLayerParams = serde_json::from_value(value).unwrap();
            Box::new(PolygonLayer::new(view_params.clone(), params))
        },
    }
}

impl PickableLayer for PolygonLayer {}
