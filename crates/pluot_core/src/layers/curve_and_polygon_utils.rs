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

/// Extract per-edge segment arrays with neighbor context for miter join computation.
/// Returns parallel arrays (prev_x, prev_y, src_x, src_y, dst_x, dst_y, next_x, next_y)
/// where prev is the vertex before src and next is the vertex after dst (wrapping around
/// closed rings). Rings with fewer than 2 points are skipped.
#[allow(clippy::type_complexity)]
pub(crate) fn polygon_segments_with_neighbors(
    rings: &[Vec<(f32, f32)>],
) -> (Vec<f32>, Vec<f32>, Vec<f32>, Vec<f32>, Vec<f32>, Vec<f32>, Vec<f32>, Vec<f32>) {
    let mut prev_x: Vec<f32> = vec![];
    let mut prev_y: Vec<f32> = vec![];
    let mut src_x: Vec<f32> = vec![];
    let mut src_y: Vec<f32> = vec![];
    let mut dst_x: Vec<f32> = vec![];
    let mut dst_y: Vec<f32> = vec![];
    let mut next_x: Vec<f32> = vec![];
    let mut next_y: Vec<f32> = vec![];
    for ring in rings {
        if ring.len() < 2 {
            continue;
        }
        let n = ring.len();
        for i in 0..n {
            let prev = ring[(i + n - 1) % n];
            let src  = ring[i];
            let dst  = ring[(i + 1) % n];
            let next = ring[(i + 2) % n];
            prev_x.push(prev.0); prev_y.push(prev.1);
            src_x.push(src.0);   src_y.push(src.1);
            dst_x.push(dst.0);   dst_y.push(dst.1);
            next_x.push(next.0); next_y.push(next.1);
        }
    }
    (prev_x, prev_y, src_x, src_y, dst_x, dst_y, next_x, next_y)
}
