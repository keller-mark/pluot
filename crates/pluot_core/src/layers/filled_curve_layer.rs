// PathCommand lives here so FilledCurveLayer owns the full curve-to-fill pipeline.
// CurveLayer re-exports PathCommand from this module for backward compatibility.

use earcut::Earcut;
use kurbo::{Arc as KurboArc, CubicBez, ParamCurve, Point as KurboPoint, QuadBez, SvgArc, Vec2 as KurboVec2};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::positioning::get_point_position;
use crate::render_traits::{
    DrawToRasterCpu, DrawToRasterGpu, DrawToSvg,
    MarginParams, PickableLayer, PreparedLayer, UnitsMode, ViewParams,
};
use crate::render_types::{CpuContext, CpuRenderPass, GpuContext, PrepareResult, RenderResult};
use crate::two::shapes::{TwoColor, TwoElement, TwoGroup, TwoPath};
use crate::two::svg::{update_svg, SvgContext};
use crate::wgpu;

use super::curve_and_polygon_utils::resolve_margins;
use super::triangulated_layer::{TriangulatedLayer, TriangulatedLayerParams};

// ── PathCommand ────────────────────────────────────────────────────────────────

/// A single drawing command, the post-parsed form of an SVG path segment.
/// All coordinates are absolute.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PathCommand {
    MoveTo { x: f32, y: f32 },
    LineTo { x: f32, y: f32 },
    CubicTo { x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32 },
    QuadraticTo { x1: f32, y1: f32, x: f32, y: f32 },
    ArcTo {
        rx: f32,
        ry: f32,
        #[serde(default)]
        x_axis_rotation: f32,
        large_arc: bool,
        sweep: bool,
        x: f32,
        y: f32,
    },
    Close,
}

// ── Curve helpers ──────────────────────────────────────────────────────────────

fn line_to_cubic(p0: KurboPoint, p1: KurboPoint) -> CubicBez {
    CubicBez::new(p0, p0, p1, p1)
}

pub(crate) fn commands_to_subpaths(commands: &[PathCommand]) -> Vec<Vec<CubicBez>> {
    let mut subpaths: Vec<Vec<CubicBez>> = Vec::new();
    let mut current: Vec<CubicBez> = Vec::new();
    let mut cursor = KurboPoint::ZERO;
    let mut subpath_start = KurboPoint::ZERO;

    for command in commands {
        match *command {
            PathCommand::MoveTo { x, y } => {
                if !current.is_empty() {
                    subpaths.push(std::mem::take(&mut current));
                }
                cursor = KurboPoint::new(x as f64, y as f64);
                subpath_start = cursor;
            }
            PathCommand::LineTo { x, y } => {
                let end = KurboPoint::new(x as f64, y as f64);
                current.push(line_to_cubic(cursor, end));
                cursor = end;
            }
            PathCommand::CubicTo { x1, y1, x2, y2, x, y } => {
                let end = KurboPoint::new(x as f64, y as f64);
                current.push(CubicBez::new(
                    cursor,
                    KurboPoint::new(x1 as f64, y1 as f64),
                    KurboPoint::new(x2 as f64, y2 as f64),
                    end,
                ));
                cursor = end;
            }
            PathCommand::QuadraticTo { x1, y1, x, y } => {
                let end = KurboPoint::new(x as f64, y as f64);
                let quad = QuadBez::new(cursor, KurboPoint::new(x1 as f64, y1 as f64), end);
                current.push(quad.raise());
                cursor = end;
            }
            PathCommand::ArcTo { rx, ry, x_axis_rotation, large_arc, sweep, x, y } => {
                let end = KurboPoint::new(x as f64, y as f64);
                let svg_arc = SvgArc {
                    from: cursor,
                    to: end,
                    radii: KurboVec2::new(rx as f64, ry as f64),
                    x_rotation: (x_axis_rotation as f64).to_radians(),
                    large_arc,
                    sweep,
                };
                match KurboArc::from_svg_arc(&svg_arc) {
                    Some(arc) => {
                        let tolerance = (svg_arc.radii.hypot() * 1e-3).max(1e-9);
                        let mut p0 = cursor;
                        arc.to_cubic_beziers(tolerance, |p1, p2, p3| {
                            current.push(CubicBez::new(p0, p1, p2, p3));
                            p0 = p3;
                        });
                    }
                    None => current.push(line_to_cubic(cursor, end)),
                }
                cursor = end;
            }
            PathCommand::Close => {
                if cursor != subpath_start {
                    current.push(line_to_cubic(cursor, subpath_start));
                }
                cursor = subpath_start;
            }
        }
    }
    if !current.is_empty() {
        subpaths.push(current);
    }
    subpaths
}

fn subpath_to_ring(subpath: &[CubicBez], subdivisions: u32) -> Vec<(f32, f32)> {
    let mut points: Vec<(f32, f32)> = Vec::new();
    if subpath.is_empty() {
        return points;
    }
    let push_unique = |points: &mut Vec<(f32, f32)>, p: (f32, f32)| {
        match points.last() {
            Some(last) if (last.0 - p.0).abs() < 1e-9 && (last.1 - p.1).abs() < 1e-9 => {}
            _ => points.push(p),
        }
    };
    let start = subpath[0].p0;
    push_unique(&mut points, (start.x as f32, start.y as f32));
    for seg in subpath {
        for step in 1..=subdivisions {
            let t = step as f64 / subdivisions as f64;
            let p = seg.eval(t);
            push_unique(&mut points, (p.x as f32, p.y as f32));
        }
    }
    if points.len() > 1 {
        let first = points[0];
        let last = *points.last().unwrap();
        if (first.0 - last.0).abs() < 1e-9 && (first.1 - last.1).abs() < 1e-9 {
            points.pop();
        }
    }
    points
}

fn ring_area_2x(points: &[(f32, f32)]) -> f32 {
    let n = points.len();
    let mut area = 0.0f32;
    for i in 0..n {
        let j = (i + 1) % n;
        area += points[i].0 * points[j].1 - points[j].0 * points[i].1;
    }
    area
}

fn compute_fill_vertices(subpaths: &[Vec<CubicBez>], subdivisions: u32) -> Vec<(f32, f32)> {
    let mut verts: Vec<(f32, f32)> = Vec::new();
    let mut ec: Earcut<f32> = Earcut::new();
    let mut indices: Vec<u32> = Vec::new();
    for subpath in subpaths {
        let ring = subpath_to_ring(subpath, subdivisions);
        if ring.len() < 3 || ring_area_2x(&ring).abs() <= 1e-12 {
            continue;
        }
        ec.earcut(ring.iter().map(|&(x, y)| [x, y]), &[] as &[u32], &mut indices);
        for &i in &indices {
            verts.push(ring[i as usize]);
        }
    }
    verts
}

// ── Params ─────────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct FilledCurveLayerParams {
    pub layer_id: String,
    pub bounds: Option<MarginParams>,
    pub data_unit_mode_x: UnitsMode,
    pub data_unit_mode_y: UnitsMode,
    pub model_matrix: Option<[f32; 16]>,
    pub commands: Arc<Vec<PathCommand>>,
    pub subdivisions: u32,
    /// RGBA fill color in [0, 1]. Defaults to opaque black.
    pub fill_color: [f32; 4],
    /// Additional opacity multiplier for the fill. Defaults to 1.
    pub fill_opacity: f32,
}

impl Default for FilledCurveLayerParams {
    fn default() -> Self {
        Self {
            layer_id: "".to_string(),
            bounds: None,
            data_unit_mode_x: UnitsMode::Data,
            data_unit_mode_y: UnitsMode::Data,
            model_matrix: None,
            commands: Arc::new(vec![]),
            subdivisions: 32,
            fill_color: [0.0, 0.0, 0.0, 1.0],
            fill_opacity: 1.0,
        }
    }
}

// ── Layer ──────────────────────────────────────────────────────────────────────

pub struct FilledCurveLayer {
    view_params: ViewParams,
    layer_params: FilledCurveLayerParams,
    subpaths: Vec<Vec<CubicBez>>,
    fill_vertices: Arc<Vec<(f32, f32)>>,
}

impl FilledCurveLayer {
    pub fn new(view_params: ViewParams, layer_params: FilledCurveLayerParams) -> Self {
        let subpaths = commands_to_subpaths(&layer_params.commands);
        let subdivisions = layer_params.subdivisions.max(1);
        let fill_vertices = Arc::new(compute_fill_vertices(&subpaths, subdivisions));
        Self { view_params, layer_params, subpaths, fill_vertices }
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────────


// ── Trait impls ────────────────────────────────────────────────────────────────

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for FilledCurveLayer {
    async fn prepare(&mut self, _gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        PrepareResult { bailed_early: false }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for FilledCurveLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        if self.fill_vertices.is_empty() {
            return;
        }
        let triangulated = TriangulatedLayer::new(
            self.view_params.clone(),
            TriangulatedLayerParams {
                layer_id: self.layer_params.layer_id.clone(),
                bounds: self.layer_params.bounds.clone(),
                data_unit_mode_x: self.layer_params.data_unit_mode_x.clone(),
                data_unit_mode_y: self.layer_params.data_unit_mode_y.clone(),
                model_matrix: self.layer_params.model_matrix,
                vertices: Arc::clone(&self.fill_vertices),
                fill_color: self.layer_params.fill_color,
                fill_opacity: self.layer_params.fill_opacity,
            },
        );
        DrawToRasterGpu::draw(&triangulated, gpu_context, pass).await;
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for FilledCurveLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for FilledCurveLayer {
    async fn draw(&self, ctx: &mut SvgContext) {
        let Self { layer_params, view_params, subpaths, .. } = self;

        let camera_view = view_params.camera_view.unwrap_or([
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ]);

        let (margin_left, margin_top, margin_right, margin_bottom) =
            resolve_margins(&layer_params.bounds, &view_params.margins);

        let viewport_w = view_params.width as f32;
        let viewport_h = view_params.height as f32;
        let layer_w = viewport_w - (margin_left + margin_right) as f32;
        let layer_h = viewport_h - (margin_top + margin_bottom) as f32;

        let subdivisions = layer_params.subdivisions.max(1) as f64;

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

        let [r, g, b, a] = layer_params.fill_color;
        let fill = TwoColor::Rgba((
            (r * 255.0).round().clamp(0.0, 255.0) as u8,
            (g * 255.0).round().clamp(0.0, 255.0) as u8,
            (b * 255.0).round().clamp(0.0, 255.0) as u8,
            (a * layer_params.fill_opacity * 255.0).round().clamp(0.0, 255.0) as u8,
        ));

        let mut svg_elements: Vec<TwoElement> = Vec::with_capacity(subpaths.len());
        for subpath in subpaths {
            if subpath.is_empty() {
                continue;
            }
            let mut points: Vec<(f64, f64)> = Vec::new();
            let first = subpath[0].p0;
            points.push(to_px(first.x as f32, first.y as f32));
            for seg in subpath {
                for step in 1..=(subdivisions as u32) {
                    let t = step as f64 / subdivisions;
                    let p = seg.eval(t);
                    points.push(to_px(p.x as f32, p.y as f32));
                }
            }
            svg_elements.push(TwoElement::Path(TwoPath {
                points,
                stroke: None,
                fill: Some(fill.clone()),
                linewidth: 0.0,
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
        layer_type_name: "FilledCurveLayer",
        create_layer: |value, view_params| {
            let params: FilledCurveLayerParams = serde_json::from_value(value).unwrap();
            Box::new(FilledCurveLayer::new(view_params.clone(), params))
        },
    }
}

impl PickableLayer for FilledCurveLayer {}
