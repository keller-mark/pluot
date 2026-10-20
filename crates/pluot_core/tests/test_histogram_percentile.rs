use pluot_core::compute::histogram_percentile::{histogram_bin, PercentileSearch};

fn histogram(values: &[f32], (lo, hi): (f32, f32), num_bins: u32) -> Vec<u32> {
    let mut counts = vec![0u32; num_bins as usize];
    for &value in values {
        counts[histogram_bin(value, lo, hi, num_bins)] += 1;
    }
    counts
}

fn search(values: &[f32], percentile: f32, num_passes: usize) -> f32 {
    let extent = values.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), &v| (lo.min(v), hi.max(v)));
    let mut search = PercentileSearch::new(extent, percentile);
    for _ in 1..num_passes {
        search.narrow(&histogram(values, search.range(), 256));
    }
    search.estimate(&histogram(values, search.range(), 256))
}

/// 0 to 999, plus one outlier which stretches the extent a thousandfold.
fn with_outlier() -> Vec<f32> {
    (0..1000).map(|v| v as f32).chain([1.0e6]).collect()
}

#[test]
fn bins_clamp_out_of_range_values_into_the_edge_bins() {
    assert_eq!(histogram_bin(-5.0, 0.0, 10.0, 10), 0);
    assert_eq!(histogram_bin(10.0, 0.0, 10.0, 10), 9);
    assert_eq!(histogram_bin(25.0, 0.0, 10.0, 10), 9);
    assert_eq!(histogram_bin(3.0, 1.0, 1.0, 10), 0);
}

#[test]
fn a_single_pass_is_skewed_by_an_outlier() {
    assert!(search(&with_outlier(), 99.0, 1) > 3000.0);
}

#[test]
fn narrowing_locates_percentiles_despite_an_outlier() {
    let values = with_outlier();
    for (percentile, expected) in [(1.0, 10.0), (50.0, 500.0), (99.0, 991.0)] {
        let estimate = search(&values, percentile, 2);
        assert!((estimate - expected).abs() <= 2.0, "p{percentile}: {estimate}");
    }
}

#[test]
fn the_extreme_percentiles_are_the_extent() {
    let values = with_outlier();
    assert_eq!(search(&values, 0.0, 2), 0.0);
    assert_eq!(search(&values, 100.0, 2), 1.0e6);
}

#[test]
fn constant_values_have_constant_percentiles() {
    assert_eq!(search(&[4.0; 10], 90.0, 2), 4.0);
}
