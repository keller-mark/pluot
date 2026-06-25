// A CurveLayer that renders SVG-like vector paths (lines, cubic/quadratic Bezier
// curves and elliptical arcs) as stroked and/or filled curves.
//
// GPU rendering is split across two free functions:
//
//   draw_stroked_curve_gpu — round joins & round caps (webgpu-instanced-lines approach).
//   draw_fill_gpu          — triangulated fill interior (earcut).
//
// SVG rendering walks `subpaths` and emits one TwoPath polyline per sub-path.

use glam::{Mat4, Vec4};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use earcut::Earcut;
use kurbo::{Arc as KurboArc, CubicBez, ParamCurve, Point as KurboPoint, QuadBez, SvgArc, Vec2 as KurboVec2};

use crate::render_traits::{DrawToRasterGpu, DrawToRasterCpu, DrawToSvg, PickableLayer, PreparedLayer, ViewParams, AspectRatioMode, AspectRatioAlignmentMode, UnitsMode, MarginParams};
use crate::render_types::{CpuContext, CpuRenderPass, PrepareResult, RenderResult};
use crate::render_types::GpuContext;
use crate::wgpu;
use crate::two::shapes::{TwoColor, TwoElement, TwoGroup, TwoPath};
use crate::two::svg::{update_svg, SvgContext};

use super::stroked_curve_layer::{flatten_subpath, draw_stroked_curve_gpu};
use super::filled_curve_layer::draw_fill_gpu;

/// A single drawing command, the post-parsed form of an SVG path segment.
/// All coordinates are absolute (relative SVG commands should be resolved before
/// being passed in). This mirrors the subset of SVG path commands we support.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PathCommand {
    /// Start a new sub-path at (x, y). Corresponds to "M".
    MoveTo { x: f32, y: f32 },
    /// Straight line from the current point to (x, y). Corresponds to "L".
    LineTo { x: f32, y: f32 },
    /// Cubic Bezier with control points (x1, y1), (x2, y2) and endpoint (x, y).
    /// Corresponds to "C".
    CubicTo { x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32 },
    /// Quadratic Bezier with control point (x1, y1) and endpoint (x, y).
    /// Corresponds to "Q".
    QuadraticTo { x1: f32, y1: f32, x: f32, y: f32 },
    /// Elliptical arc to endpoint (x, y). Corresponds to "A".
    ArcTo {
        rx: f32,
        ry: f32,
        /// Rotation of the ellipse's x-axis, in degrees.
        #[serde(default)]
        x_axis_rotation: f32,
        large_arc: bool,
        sweep: bool,
        x: f32,
        y: f32,
    },
    /// Close the current sub-path with a straight line back to its start.
    /// Corresponds to "Z".
    Close,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct CurveLayerParams {
    pub layer_id: String,
    // If None, assume margin: 0 in all directions.
    pub bounds: Option<MarginParams>,
    pub data_unit_mode_x: UnitsMode,
    pub data_unit_mode_y: UnitsMode,
    pub stroke_width: f32,
    pub stroke_width_unit_mode: UnitsMode,
    pub model_matrix: Option<[f32; 16]>, // Column-major 4x4 matrix

    /// The path to draw, as a sequence of absolute drawing commands.
    pub commands: Arc<Vec<PathCommand>>,
    /// Number of straight sub-segments used to approximate each cubic Bezier
    /// segment. Higher values produce smoother curves at the cost of more
    /// instanced draws.
    pub subdivisions: u32,

    /// Whether to stroke the path outline. Defaults to `true`.
    pub stroked: bool,
    /// Whether to fill the (closed) path interior. Defaults to `false`.
    pub filled: bool,
    /// RGBA stroke color in [0, 1]. Defaults to opaque black.
    pub stroke_color: [f32; 4],
    /// RGBA fill color in [0, 1]. Defaults to opaque black.
    pub fill_color: [f32; 4],
    /// Additional opacity multiplier for the stroke, in [0, 1].
    pub stroke_opacity: f32,
    /// Additional opacity multiplier for the fill, in [0, 1].
    pub fill_opacity: f32,
}

impl Default for CurveLayerParams {
    fn default() -> Self {
        Self {
            layer_id: "".to_string(),
            bounds: None,
            data_unit_mode_x: UnitsMode::Data,
            data_unit_mode_y: UnitsMode::Data,
            stroke_width: 1.0,
            stroke_width_unit_mode: UnitsMode::Pixels,
            model_matrix: None,
            commands: Arc::new(vec![]),
            subdivisions: 32,
            stroked: true,
            filled: false,
            stroke_color: [0.0, 0.0, 0.0, 1.0],
            fill_color: [0.0, 0.0, 0.0, 1.0],
            stroke_opacity: 1.0,
            fill_opacity: 1.0,
        }
    }
}

fn line_to_cubic(p0: KurboPoint, p1: KurboPoint) -> CubicBez {
    CubicBez::new(p0, p0, p1, p1)
}

fn commands_to_subpaths(commands: &[PathCommand]) -> Vec<Vec<CubicBez>> {
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

fn resolve_margins(layer_params: &CurveLayerParams, view_params: &ViewParams) -> (f64, f64, f64, f64) {
    let bounds = if layer_params.bounds.is_none() {
        &view_params.margins
    } else {
        &layer_params.bounds
    };
    let ml = bounds.as_ref().and_then(|m| m.margin_left).unwrap_or(0.0) as f64;
    let mt = bounds.as_ref().and_then(|m| m.margin_top).unwrap_or(0.0) as f64;
    let mr = bounds.as_ref().and_then(|m| m.margin_right).unwrap_or(0.0) as f64;
    let mb = bounds.as_ref().and_then(|m| m.margin_bottom).unwrap_or(0.0) as f64;
    (ml, mt, mr, mb)
}

pub struct CurveLayer {
    view_params: ViewParams,
    layer_params: CurveLayerParams,
    /// Cubic Bezier segments per sub-path (used by SVG rendering).
    subpaths: Vec<Vec<CubicBez>>,
    /// Pre-triangulated fill vertices (empty when fill is disabled).
    fill_vertices: Vec<(f32, f32)>,
    /// Pre-flattened polylines per sub-path (empty when stroke is disabled).
    polylines: Vec<Vec<(f32, f32)>>,
    /// Pre-multiplied stroke/fill colors (opacity baked in).
    stroke_color: Vec4,
    fill_color: Vec4,
}

impl CurveLayer {
    pub fn new(view_params: ViewParams, layer_params: CurveLayerParams) -> Self {
        if layer_params.stroke_width_unit_mode == UnitsMode::Data
            && (layer_params.data_unit_mode_x == UnitsMode::Pixels
                || layer_params.data_unit_mode_y == UnitsMode::Pixels)
        {
            panic!("stroke_width_unit_mode cannot be 'data' when data_unit_mode is 'pixels'");
        }

        let subpaths = commands_to_subpaths(&layer_params.commands);
        let subdivisions = layer_params.subdivisions.max(1);
        let has_segments = subpaths.iter().any(|s| !s.is_empty());

        let polylines = if layer_params.stroked && has_segments {
            subpaths.iter().map(|s| flatten_subpath(s, subdivisions)).collect()
        } else {
            vec![]
        };

        let fill_vertices = if layer_params.filled {
            compute_fill_vertices(&subpaths, subdivisions)
        } else {
            vec![]
        };

        let [r, g, b, a] = layer_params.stroke_color;
        let stroke_color = Vec4::new(r, g, b, a * layer_params.stroke_opacity);
        let [r, g, b, a] = layer_params.fill_color;
        let fill_color = Vec4::new(r, g, b, a * layer_params.fill_opacity);

        Self { view_params, layer_params, subpaths, fill_vertices, polylines, stroke_color, fill_color }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for CurveLayer {
    async fn prepare(&mut self, _gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        PrepareResult { bailed_early: false }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for CurveLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        let Self { view_params, layer_params, fill_vertices, polylines, stroke_color, fill_color, .. } = self;

        let (margin_left, margin_top, margin_right, margin_bottom) =
            resolve_margins(layer_params, view_params);

        let camera_view = view_params.camera_view.unwrap_or([
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ]);

        let layer_w = view_params.width as f32 - (margin_left + margin_right) as f32;
        let layer_h = view_params.height as f32 - (margin_top + margin_bottom) as f32;

        let data_unit_mode_x = match layer_params.data_unit_mode_x { UnitsMode::Pixels => 0, UnitsMode::Data => 1 };
        let data_unit_mode_y = match layer_params.data_unit_mode_y { UnitsMode::Pixels => 0, UnitsMode::Data => 1 };
        let aspect_ratio_mode = match view_params.aspect_ratio_mode { AspectRatioMode::Ignore => 0, AspectRatioMode::Contain => 1, AspectRatioMode::Cover => 2 };
        let aspect_ratio_alignment_mode = match view_params.aspect_ratio_alignment_mode { AspectRatioAlignmentMode::Center => 0, AspectRatioAlignmentMode::Start => 1, AspectRatioAlignmentMode::End => 2 };
        let model_matrix = Mat4::from_cols_array(&layer_params.model_matrix.unwrap_or([
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ]));

        // Fill first so stroke renders on top.
        draw_fill_gpu(
            gpu_context, pass, fill_vertices, *fill_color,
            layer_w, layer_h, &camera_view,
            data_unit_mode_x, data_unit_mode_y,
            aspect_ratio_mode, aspect_ratio_alignment_mode,
            model_matrix, margin_left, margin_top, margin_right, margin_bottom,
        );
        draw_stroked_curve_gpu(
            gpu_context, pass, polylines, *stroke_color, layer_params.stroke_width,
            layer_w, layer_h, &camera_view,
            data_unit_mode_x, data_unit_mode_y,
            aspect_ratio_mode, aspect_ratio_alignment_mode,
            model_matrix, margin_left, margin_top, margin_right, margin_bottom,
        );
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for CurveLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for CurveLayer {
    async fn draw(&self, ctx: &mut SvgContext) {
        let Self { layer_params, view_params, subpaths, .. } = self;

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

        let subdivisions = layer_params.subdivisions.max(1) as f64;

        use crate::positioning::get_point_position;

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
        layer_type_name: "CurveLayer",
        create_layer: |value, view_params| {
            let params: CurveLayerParams = serde_json::from_value(value).unwrap();
            Box::new(CurveLayer::new(view_params.clone(), params))
        },
    }
}

impl PickableLayer for CurveLayer {}
