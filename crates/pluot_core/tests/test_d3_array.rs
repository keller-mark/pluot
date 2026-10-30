use pluot_core::d3::array::{bin, deviation, mean, quantile_sorted, Bin};

#[test]
fn quantile_sorted_interpolates_between_neighbors() {
    let values = [1.0f32, 2.0, 3.0, 4.0];
    assert_eq!(quantile_sorted(&values, 0.25), Some(1.75));
    assert_eq!(quantile_sorted(&values, 0.5), Some(2.5));
    assert_eq!(quantile_sorted(&values, 0.0), Some(1.0));
    assert_eq!(quantile_sorted(&values, 1.0), Some(4.0));
    assert_eq!(quantile_sorted(&[7.0f32], 0.5), Some(7.0));
    assert_eq!(quantile_sorted::<f32>(&[], 0.5), None);
}

#[test]
fn deviation_is_the_sample_standard_deviation() {
    assert_eq!(deviation(&[2.0f32, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0]), Some((32.0f64 / 7.0).sqrt()));
    assert_eq!(deviation(&[1.0f32]), None);
    assert_eq!(mean(&[1.0f32, 2.0, 6.0]), Some(3.0));
}

#[test]
fn bin_counts_values_between_thresholds_within_the_domain() {
    let bins = bin(&[0.0f32, 0.5, 1.0, 1.5, 2.0, 3.0], (0.0, 2.0), &[0.0, 1.0, 2.0]);
    // Thresholds at or below the domain minimum are dropped, but one at the maximum is kept.
    assert_eq!(bins, vec![
        Bin { x0: 0.0, x1: 1.0, count: 2 },
        Bin { x0: 1.0, x1: 2.0, count: 2 },
        Bin { x0: 2.0, x1: 2.0, count: 1 },
    ]);
}
