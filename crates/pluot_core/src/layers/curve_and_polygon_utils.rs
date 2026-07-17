// Shared utility functions for polygon and curve layers.

use earcut::Earcut;
use kurbo::{Arc as KurboArc, CubicBez, ParamCurve, Point as KurboPoint, QuadBez, SvgArc, Vec2 as KurboVec2};
use serde::{Deserialize, Serialize};

use crate::numeric_data::NumericData;
use crate::render_traits::MarginParams;

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

/// Triangulate a collection of curve sub-paths into a flat, interleaved list of
/// triangle-vertex coordinates `[x0, y0, x1, y1, …]` (3 consecutive vertices per
/// triangle), ready to hand to a `TriangulatedLayer`.
pub(crate) fn compute_fill_vertices(subpaths: &[Vec<CubicBez>], subdivisions: u32) -> Vec<f32> {
    let mut verts: Vec<f32> = Vec::new();
    let mut ec: Earcut<f32> = Earcut::new();
    let mut indices: Vec<u32> = Vec::new();
    for subpath in subpaths {
        let ring = subpath_to_ring(subpath, subdivisions);
        if ring.len() < 3 || ring_area_2x(&ring).abs() <= 1e-12 {
            continue;
        }
        ec.earcut(ring.iter().map(|&(x, y)| [x, y]), &[] as &[u32], &mut indices);
        for &i in &indices {
            let (x, y) = ring[i as usize];
            verts.push(x);
            verts.push(y);
        }
    }
    verts
}

/// Triangulate a collection of polygon rings into a flat, interleaved list of
/// triangle-vertex coordinates `[x0, y0, x1, y1, …]` (3 consecutive vertices per
/// triangle). Rings with fewer than 3 points are skipped.
pub(crate) fn triangulate_polygon_rings(rings: &[Vec<(f32, f32)>]) -> Vec<f32> {
    let mut ec: Earcut<f32> = Earcut::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut verts: Vec<f32> = Vec::new();
    for ring in rings {
        if ring.len() < 3 {
            continue;
        }
        ec.earcut(ring.iter().map(|&(x, y)| [x, y]), &[] as &[u32], &mut indices);
        for &i in &indices {
            let (x, y) = ring[i as usize];
            verts.push(x);
            verts.push(y);
        }
    }
    verts
}

/// Resolve margins from layer bounds (preferred) or view-level margins.
pub(crate) fn resolve_margins(
    bounds: &Option<MarginParams>,
    view_margins: &Option<MarginParams>,
) -> (f64, f64, f64, f64) {
    let b = if bounds.is_none() { view_margins } else { bounds };
    let ml = b.as_ref().and_then(|m| m.margin_left).unwrap_or(0.0) as f64;
    let mt = b.as_ref().and_then(|m| m.margin_top).unwrap_or(0.0) as f64;
    let mr = b.as_ref().and_then(|m| m.margin_right).unwrap_or(0.0) as f64;
    let mb = b.as_ref().and_then(|m| m.margin_bottom).unwrap_or(0.0) as f64;
    (ml, mt, mr, mb)
}

/// Flatten a sub-path into a polyline of model-space (x, y) points.
/// Consecutive duplicates are removed to avoid zero-length shader segments.
pub(crate) fn flatten_subpath(subpath: &[CubicBez], subdivisions: u32) -> Vec<(f32, f32)> {
    let mut pts: Vec<(f32, f32)> = Vec::new();
    if subpath.is_empty() {
        return pts;
    }
    let push = |pts: &mut Vec<(f32, f32)>, p: (f32, f32)| {
        if let Some(&last) = pts.last() {
            if (last.0 - p.0).abs() < 1e-9 && (last.1 - p.1).abs() < 1e-9 {
                return;
            }
        }
        pts.push(p);
    };
    let p0 = subpath[0].p0;
    push(&mut pts, (p0.x as f32, p0.y as f32));
    for seg in subpath {
        for step in 1..=subdivisions {
            let t = step as f64 / subdivisions as f64;
            let p = seg.eval(t);
            push(&mut pts, (p.x as f32, p.y as f32));
        }
    }
    pts
}

/// Extract per-edge segment arrays from a collection of polygon rings.
/// Returns parallel arrays (src_x, src_y, dst_x, dst_y); rings with fewer
/// than 2 points are skipped.
pub(crate) fn polygon_edges_from_rings(
    rings: &[Vec<(f32, f32)>],
) -> (Vec<f32>, Vec<f32>, Vec<f32>, Vec<f32>) {
    let mut src_x = vec![];
    let mut src_y = vec![];
    let mut dst_x = vec![];
    let mut dst_y = vec![];
    for ring in rings {
        if ring.len() < 2 {
            continue;
        }
        let n = ring.len();
        for i in 0..n {
            let j = (i + 1) % n;
            src_x.push(ring[i].0);
            src_y.push(ring[i].1);
            dst_x.push(ring[j].0);
            dst_y.push(ring[j].1);
        }
    }
    (src_x, src_y, dst_x, dst_y)
}

/// The flat interleaved polygon representation shared by `PolygonLayer` and its
/// `StrokedPolygonLayer`/`FilledPolygonLayer` sub-layers:
/// - `coords`: all polygon vertices concatenated as a flat interleaved
///   `[x0, y0, x1, y1, …]` array.
/// - `offsets`: an Arrow-style offset array of `num_polygons + 1` entries, in
///   vertex units. Polygon `p` occupies vertex indices `offsets[p]..offsets[p+1]`,
///   i.e. its coordinates live at `coords[2*offsets[p] .. 2*offsets[p+1]]`.
///
/// Reconstruct per-polygon rings of `(x, y)` vertices from that representation.
/// Coordinates and offsets are read regardless of their source dtype. Used by the
/// SVG paths and by the fill triangulation.
pub(crate) fn polygon_rings_from_flat(
    coords: &NumericData,
    offsets: &NumericData,
) -> Vec<Vec<(f32, f32)>> {
    let mut rings: Vec<Vec<(f32, f32)>> = Vec::new();
    let num_offsets = offsets.len();
    if num_offsets < 2 {
        return rings;
    }
    for p in 0..(num_offsets - 1) {
        let start = offsets.get_f64(p) as usize;
        let end = offsets.get_f64(p + 1) as usize;
        let mut ring = Vec::with_capacity(end.saturating_sub(start));
        for v in start..end {
            ring.push((coords.get_f32(2 * v), coords.get_f32(2 * v + 1)));
        }
        rings.push(ring);
    }
    rings
}

/// Build per-edge segment metadata for stroked polygon rendering with miter joins,
/// directly from the flat vertex `offsets` (see [`polygon_rings_from_flat`]).
///
/// Returns one `[ring_start, ring_end, local_idx]` u32 triple per edge, where
/// `ring_start`/`ring_end` are absolute vertex indices into the flat coordinate
/// array and `local_idx` is the 0-based index of the edge's source vertex within
/// its ring. The shader uses these to look up prev/src/dst/next with correct
/// wrap-around via modular arithmetic. Rings with fewer than 2 vertices are skipped.
pub(crate) fn polygon_segments_from_offsets(offsets: &NumericData) -> Vec<[u32; 3]> {
    let mut segments: Vec<[u32; 3]> = Vec::new();
    let num_offsets = offsets.len();
    if num_offsets < 2 {
        return segments;
    }
    for p in 0..(num_offsets - 1) {
        let ring_start = offsets.get_f64(p) as u32;
        let ring_end_excl = offsets.get_f64(p + 1) as u32;
        let ring_size = ring_end_excl - ring_start;
        if ring_size < 2 {
            continue;
        }
        let ring_end = ring_end_excl - 1;
        for local_idx in 0..ring_size {
            segments.push([ring_start, ring_end, local_idx]);
        }
    }
    segments
}
