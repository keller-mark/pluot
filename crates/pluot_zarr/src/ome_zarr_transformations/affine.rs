//! Dense affine transformations between coordinate systems, possibly of
//! differing dimensionality.

/// An affine transformation, stored as the upper `n_out x (n_in + 1)` block of
/// an `(n_out + 1) x (n_in + 1)` homogeneous matrix. This is the same layout as
/// the OME-Zarr `affine` transformation type: row-major, with the last column
/// holding the translation.
///
/// Points are column vectors whose components follow the axis order of the
/// coordinate system they belong to, so `output = linear * input + translation`.
///
/// Reference: <https://ngff.openmicroscopy.org/rfc/5/index.html>
#[derive(Debug, Clone, PartialEq)]
pub struct AffineMatrix {
    /// Number of input (source coordinate system) dimensions.
    n_in: usize,
    /// Number of output (target coordinate system) dimensions.
    n_out: usize,
    /// Row-major coefficients: `n_out` rows of `n_in + 1` values each.
    rows: Vec<f64>,
}

impl AffineMatrix {
    /// Construct from row-major coefficients, which must have
    /// `n_out * (n_in + 1)` elements.
    pub fn new(n_out: usize, n_in: usize, rows: Vec<f64>) -> Result<Self, String> {
        let expected = n_out * (n_in + 1);
        if rows.len() != expected {
            return Err(format!(
                "expected {expected} coefficients for a {n_out}x{} matrix, got {}",
                n_in + 1,
                rows.len(),
            ));
        }
        Ok(Self { n_in, n_out, rows })
    }

    /// The `n`-dimensional identity transformation.
    pub fn identity(n: usize) -> Self {
        let mut rows = vec![0.0; n * (n + 1)];
        for i in 0..n {
            rows[i * (n + 1) + i] = 1.0;
        }
        Self { n_in: n, n_out: n, rows }
    }

    /// A per-axis scaling, from the OME-Zarr `scale` transformation type.
    pub fn from_scale(scale: &[f64]) -> Self {
        let n = scale.len();
        let mut rows = vec![0.0; n * (n + 1)];
        for (i, &s) in scale.iter().enumerate() {
            rows[i * (n + 1) + i] = s;
        }
        Self { n_in: n, n_out: n, rows }
    }

    /// A per-axis offset, from the OME-Zarr `translation` transformation type.
    pub fn from_translation(translation: &[f64]) -> Self {
        let n = translation.len();
        let mut rows = vec![0.0; n * (n + 1)];
        for (i, &t) in translation.iter().enumerate() {
            rows[i * (n + 1) + i] = 1.0;
            rows[i * (n + 1) + n] = t;
        }
        Self { n_in: n, n_out: n, rows }
    }

    /// An axis permutation, from the OME-Zarr `mapAxis` transformation type:
    /// output component `i` takes its value from input component `map_axis[i]`.
    pub fn from_map_axis(map_axis: &[usize]) -> Result<Self, String> {
        let n = map_axis.len();
        let mut seen = vec![false; n];
        for &source in map_axis {
            if source >= n {
                return Err(format!("mapAxis index {source} is out of range for {n} axes"));
            }
            if seen[source] {
                return Err(format!("mapAxis index {source} appears more than once"));
            }
            seen[source] = true;
        }
        let mut rows = vec![0.0; n * (n + 1)];
        for (i, &source) in map_axis.iter().enumerate() {
            rows[i * (n + 1) + source] = 1.0;
        }
        Ok(Self { n_in: n, n_out: n, rows })
    }

    /// The OME-Zarr `affine` transformation type: `n_out` rows of `n_in + 1`
    /// values, the last value in each row being the translation.
    pub fn from_affine(affine: &[Vec<f64>]) -> Result<Self, String> {
        let n_out = affine.len();
        let n_cols = affine
            .first()
            .ok_or_else(|| "affine matrix must have at least one row".to_string())?
            .len();
        if n_cols < 2 {
            return Err(format!("affine rows must have at least 2 values, got {n_cols}"));
        }
        if affine.iter().any(|row| row.len() != n_cols) {
            return Err("affine rows must all have the same length".to_string());
        }
        Ok(Self {
            n_in: n_cols - 1,
            n_out,
            rows: affine.concat(),
        })
    }

    /// The OME-Zarr `rotation` transformation type: a square matrix with no
    /// translation component.
    pub fn from_rotation(rotation: &[Vec<f64>]) -> Result<Self, String> {
        let n = rotation.len();
        if n == 0 {
            return Err("rotation matrix must have at least one row".to_string());
        }
        if rotation.iter().any(|row| row.len() != n) {
            return Err(format!("rotation matrix must be square, expected {n} values per row"));
        }
        let mut rows = vec![0.0; n * (n + 1)];
        for (i, row) in rotation.iter().enumerate() {
            rows[i * (n + 1)..i * (n + 1) + n].copy_from_slice(row);
        }
        Ok(Self { n_in: n, n_out: n, rows })
    }

    /// Number of input dimensions.
    pub fn n_in(&self) -> usize {
        self.n_in
    }

    /// Number of output dimensions.
    pub fn n_out(&self) -> usize {
        self.n_out
    }

    /// Coefficient at `row` (an output component) and `col`, where
    /// `col == n_in()` is the translation column.
    pub fn get(&self, row: usize, col: usize) -> f64 {
        self.rows[row * (self.n_in + 1) + col]
    }

    /// Compose two transformations: apply `self` first, then `next`.
    pub fn then(&self, next: &Self) -> Result<Self, String> {
        if next.n_in != self.n_out {
            return Err(format!(
                "cannot compose a transformation with {} output dimensions \
                 with one expecting {} input dimensions",
                self.n_out, next.n_in,
            ));
        }
        let (n_in, n_out) = (self.n_in, next.n_out);
        let mut rows = vec![0.0; n_out * (n_in + 1)];
        for r in 0..n_out {
            for c in 0..n_in {
                rows[r * (n_in + 1) + c] = (0..self.n_out)
                    .map(|k| next.get(r, k) * self.get(k, c))
                    .sum();
            }
            // The composed translation also picks up `next`'s linear part
            // applied to `self`'s translation.
            rows[r * (n_in + 1) + n_in] = next.get(r, next.n_in)
                + (0..self.n_out)
                    .map(|k| next.get(r, k) * self.get(k, self.n_in))
                    .sum::<f64>();
        }
        Ok(Self { n_in, n_out, rows })
    }

    /// Invert the transformation, which requires it to be square with a
    /// non-singular linear part.
    pub fn inverse(&self) -> Result<Self, String> {
        if self.n_in != self.n_out {
            return Err(format!(
                "cannot invert a transformation from {} to {} dimensions",
                self.n_in, self.n_out,
            ));
        }
        let n = self.n_in;
        // Gauss-Jordan elimination with partial pivoting, reducing the linear
        // part to the identity while applying the same operations to `inv`.
        let mut linear: Vec<f64> = (0..n)
            .flat_map(|r| (0..n).map(move |c| (r, c)))
            .map(|(r, c)| self.get(r, c))
            .collect();
        let mut inv = vec![0.0; n * n];
        for i in 0..n {
            inv[i * n + i] = 1.0;
        }
        let magnitude = linear.iter().fold(0.0_f64, |acc, v| acc.max(v.abs()));
        let tolerance = 1e-12 * magnitude.max(1.0);

        for col in 0..n {
            let pivot = (col..n)
                .max_by(|&a, &b| {
                    linear[a * n + col]
                        .abs()
                        .total_cmp(&linear[b * n + col].abs())
                })
                .unwrap();
            if linear[pivot * n + col].abs() <= tolerance {
                return Err("the linear part of the transformation is singular".to_string());
            }
            for c in 0..n {
                linear.swap(col * n + c, pivot * n + c);
                inv.swap(col * n + c, pivot * n + c);
            }
            let divisor = linear[col * n + col];
            for c in 0..n {
                linear[col * n + c] /= divisor;
                inv[col * n + c] /= divisor;
            }
            for r in 0..n {
                if r == col {
                    continue;
                }
                let factor = linear[r * n + col];
                if factor == 0.0 {
                    continue;
                }
                for c in 0..n {
                    linear[r * n + c] -= factor * linear[col * n + c];
                    inv[r * n + c] -= factor * inv[col * n + c];
                }
            }
        }

        // The inverse translation is `-linear^-1 * translation`.
        let mut rows = vec![0.0; n * (n + 1)];
        for r in 0..n {
            rows[r * (n + 1)..r * (n + 1) + n].copy_from_slice(&inv[r * n..r * n + n]);
            rows[r * (n + 1) + n] = -(0..n).map(|k| inv[r * n + k] * self.get(k, n)).sum::<f64>();
        }
        Ok(Self { n_in: n, n_out: n, rows })
    }

    /// Apply the transformation to a single point.
    pub fn apply(&self, point: &[f64]) -> Result<Vec<f64>, String> {
        if point.len() != self.n_in {
            return Err(format!(
                "expected a {}-dimensional point, got {} dimensions",
                self.n_in,
                point.len(),
            ));
        }
        Ok((0..self.n_out)
            .map(|r| {
                self.get(r, self.n_in)
                    + (0..self.n_in).map(|c| self.get(r, c) * point[c]).sum::<f64>()
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Assert that two point lists agree to within the tolerances used by the
    /// OME-Zarr transformations conformance suite.
    fn assert_close(actual: &[f64], expected: &[f64]) {
        assert_eq!(actual.len(), expected.len(), "{actual:?} vs {expected:?}");
        for (a, e) in actual.iter().zip(expected) {
            assert!(
                (a - e).abs() <= 1e-6 + 1e-3 * e.abs(),
                "{actual:?} vs {expected:?}",
            );
        }
    }

    #[test]
    fn test_identity() {
        let m = AffineMatrix::identity(2);
        assert_close(&m.apply(&[1.0, 2.0]).unwrap(), &[1.0, 2.0]);
    }

    #[test]
    fn test_scale_and_translation() {
        let scale = AffineMatrix::from_scale(&[10.0, 20.0]);
        assert_close(&scale.apply(&[1.0, 2.0]).unwrap(), &[10.0, 40.0]);

        let translation = AffineMatrix::from_translation(&[10.0, 20.0]);
        assert_close(&translation.apply(&[1.0, 2.0]).unwrap(), &[11.0, 22.0]);
    }

    #[test]
    fn test_map_axis_swaps_components() {
        let m = AffineMatrix::from_map_axis(&[1, 0]).unwrap();
        assert_close(&m.apply(&[1.0, 2.0]).unwrap(), &[2.0, 1.0]);
    }

    #[test]
    fn test_map_axis_rejects_non_permutations() {
        assert!(AffineMatrix::from_map_axis(&[0, 0]).is_err());
        assert!(AffineMatrix::from_map_axis(&[0, 2]).is_err());
    }

    #[test]
    fn test_affine_scale_and_translate() {
        // From the conformance suite's `affine` case.
        let m = AffineMatrix::from_affine(&[
            vec![2.0, 0.0, 0.0, 20.0],
            vec![0.0, 3.0, 0.0, 60.0],
            vec![0.0, 0.0, 4.0, 120.0],
        ])
        .unwrap();
        assert_eq!((m.n_in(), m.n_out()), (3, 3));
        assert_close(&m.apply(&[-1.0, -1.0, -1.0]).unwrap(), &[18.0, 57.0, 116.0]);
        assert_close(&m.apply(&[2.0, 2.0, 2.0]).unwrap(), &[24.0, 66.0, 128.0]);
    }

    #[test]
    fn test_affine_up_projection() {
        // From the conformance suite's `affine_upProjection` case: 2D in, 3D out.
        let m = AffineMatrix::from_affine(&[
            vec![2.0, 0.0, 0.0],
            vec![0.0, 3.0, 0.0],
            vec![1.0, 1.0, 0.0],
        ])
        .unwrap();
        assert_eq!((m.n_in(), m.n_out()), (2, 3));
        assert_close(&m.apply(&[1.0, 2.0]).unwrap(), &[2.0, 6.0, 3.0]);
        // A projection is not square, so it cannot be inverted.
        assert!(m.inverse().is_err());
    }

    #[test]
    fn test_rotation() {
        let c = std::f64::consts::FRAC_1_SQRT_2;
        let m = AffineMatrix::from_rotation(&[vec![c, -c], vec![c, c]]).unwrap();
        assert_close(&m.apply(&[-1.0, -1.0]).unwrap(), &[0.0, -1.414_213_56]);
        assert_close(&m.apply(&[2.0, 2.0]).unwrap(), &[0.0, 2.828_427_12]);
    }

    #[test]
    fn test_then_applies_self_before_next() {
        let scale = AffineMatrix::from_scale(&[2.0, 2.0]);
        let translation = AffineMatrix::from_translation(&[1.0, 1.0]);
        // Scale first, then translate.
        assert_close(&scale.then(&translation).unwrap().apply(&[1.0, 1.0]).unwrap(), &[3.0, 3.0]);
        // Translate first, then scale.
        assert_close(&translation.then(&scale).unwrap().apply(&[1.0, 1.0]).unwrap(), &[4.0, 4.0]);
    }

    #[test]
    fn test_then_rejects_dimension_mismatch() {
        let two_d = AffineMatrix::identity(2);
        let three_d = AffineMatrix::identity(3);
        assert!(two_d.then(&three_d).is_err());
    }

    #[test]
    fn test_inverse_round_trips() {
        let m = AffineMatrix::from_affine(&[
            vec![2.0, 0.0, 0.0, 20.0],
            vec![0.0, 3.0, 0.0, 60.0],
            vec![0.0, 0.0, 4.0, 120.0],
        ])
        .unwrap();
        let inverse = m.inverse().unwrap();
        assert_close(&inverse.apply(&[18.0, 57.0, 116.0]).unwrap(), &[-1.0, -1.0, -1.0]);
        assert_close(&inverse.apply(&[24.0, 66.0, 128.0]).unwrap(), &[2.0, 2.0, 2.0]);
    }

    #[test]
    fn test_inverse_rejects_singular_linear_part() {
        assert!(AffineMatrix::from_scale(&[1.0, 0.0]).inverse().is_err());
    }

    #[test]
    fn test_inverse_of_rotation() {
        let c = std::f64::consts::FRAC_1_SQRT_2;
        let m = AffineMatrix::from_rotation(&[vec![c, -c], vec![c, c]]).unwrap();
        let inverse = m.inverse().unwrap();
        assert_close(&inverse.apply(&[0.0, -1.414_213_56]).unwrap(), &[-1.0, -1.0]);
    }

    #[test]
    fn test_apply_rejects_wrong_dimensionality() {
        assert!(AffineMatrix::identity(2).apply(&[1.0, 2.0, 3.0]).is_err());
    }
}
