// Shared utility functions for polygon and curve layers.

use kurbo::{CubicBez, ParamCurve};

use crate::render_traits::MarginParams;

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

/// Build compact GPU-ready data for stroked polygon rendering with miter joins.
///
/// Returns:
/// - `points`: all ring vertices concatenated as flat `[x, y, x, y, …]` f32 values.
/// - `segments`: one `[ring_start, ring_end, local_idx]` u32 triple per edge, where
///   `ring_start`/`ring_end` are absolute indices into `points` (in vertex units, not
///   byte units) and `local_idx` is the 0-based index of the edge's source vertex
///   within its ring. The shader uses these to look up prev/src/dst/next with
///   correct wrap-around via modular arithmetic, without any redundant storage.
///
/// Rings with fewer than 2 points are skipped.
pub(crate) fn polygon_gpu_data(
    rings: &[Vec<(f32, f32)>],
) -> (Vec<f32>, Vec<[u32; 3]>) {
    let mut points: Vec<f32> = Vec::new();
    let mut segments: Vec<[u32; 3]> = Vec::new();

    for ring in rings {
        if ring.len() < 2 {
            continue;
        }
        let ring_start = (points.len() / 2) as u32;
        for &(x, y) in ring {
            points.push(x);
            points.push(y);
        }
        let ring_end = (points.len() / 2 - 1) as u32;
        for local_idx in 0..(ring.len() as u32) {
            segments.push([ring_start, ring_end, local_idx]);
        }
    }

    (points, segments)
}
