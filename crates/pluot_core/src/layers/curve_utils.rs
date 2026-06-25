use kurbo::{CubicBez, ParamCurve};

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
