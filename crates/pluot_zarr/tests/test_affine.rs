//! Unit tests for `AffineMatrix`: construction from each OME-Zarr
//! transformation type, composition, inversion, and application to points.

use pluot_zarr::ome_zarr_transformations::affine::AffineMatrix;

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
