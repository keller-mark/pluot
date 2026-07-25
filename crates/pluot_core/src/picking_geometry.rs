// Shared naive geometry helpers used by CPU-side (non-GPU-accelerated) picking
// implementations across layer types. See PointLayer::pick for the pattern
// these grew out of: https://github.com/keller-mark/pluot/issues/140

use glam::{DMat4, DVec4};

/// Invert `model_matrix` (identity if `None`) and apply it to a world-space
/// (post-`unproject`) point, returning the equivalent model-space point.
/// Returns `None` if the matrix is singular. Mirrors the world-to-model-space
/// inversion used by `PointLayer`/`BitmapLayer` picking.
pub(crate) fn unapply_model_matrix(model_matrix: Option<[f32; 16]>, x: f32, y: f32) -> Option<(f32, f32)> {
    let m = model_matrix.unwrap_or([
        1.0, 0.0, 0.0, 0.0,
        0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        0.0, 0.0, 0.0, 1.0,
    ]);
    let mut m64 = [0.0f64; 16];
    for (i, v) in m.iter().enumerate() {
        m64[i] = *v as f64;
    }
    let mat = DMat4::from_cols_array(&m64);
    if mat.determinant() == 0.0 {
        return None;
    }
    let p = mat.inverse() * DVec4::new(x as f64, y as f64, 0.0, 1.0);
    Some((p.x as f32, p.y as f32))
}

/// Naive point-in-polygon test using the even-odd (ray-casting) rule.
/// `ring` need not repeat its first point at the end.
pub(crate) fn point_in_polygon(x: f32, y: f32, ring: &[(f32, f32)]) -> bool {
    let n = ring.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = ring[i];
        let (xj, yj) = ring[j];
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Squared distance from point `(px, py)` to the segment `(ax, ay)-(bx, by)`.
pub(crate) fn dist_sq_to_segment(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let dx = bx - ax;
    let dy = by - ay;
    let len_sq = dx * dx + dy * dy;
    let t = if len_sq > 0.0 {
        (((px - ax) * dx + (py - ay) * dy) / len_sq).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let cx = ax + t * dx;
    let cy = ay + t * dy;
    let ddx = px - cx;
    let ddy = py - cy;
    ddx * ddx + ddy * ddy
}
