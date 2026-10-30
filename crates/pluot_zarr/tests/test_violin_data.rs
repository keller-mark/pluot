use pluot_zarr::violin_data::{bin_violins, summarize_violin, ViolinSummary};

#[test]
fn summarizes_only_the_given_rows() {
    let values = [9.0, 1.0, 2.0, 3.0, 4.0];
    let summary = summarize_violin(&values, &[4, 3, 2, 1], true);
    assert_eq!(summary.quartiles, Some((1.75, 2.5, 3.25)));
    assert_eq!(summary.non_outliers, vec![1.0, 2.0, 3.0, 4.0]);
    assert_eq!(summary.num_values, 4);
}

#[test]
fn zeros_are_dropped_unless_kept() {
    let values = [0.0, 0.0, 2.0, 4.0];
    assert_eq!(summarize_violin(&values, &[0, 1, 2, 3], false).non_outliers, vec![2.0, 4.0]);
    assert_eq!(summarize_violin(&values, &[0, 1, 2, 3], true).num_values, 4);
}

#[test]
fn values_beyond_three_standard_deviations_are_outliers() {
    let mut values = vec![1.0f32; 20];
    values.push(1000.0);
    let rows: Vec<u32> = (0..values.len() as u32).collect();
    let summary = summarize_violin(&values, &rows, true);
    assert_eq!(summary.non_outliers.len(), 20);
    assert_eq!(summary.num_values, 21);
}

#[test]
fn an_empty_group_has_no_quartiles() {
    let summary = summarize_violin(&[1.0], &[], true);
    assert_eq!(summary, ViolinSummary { quartiles: None, non_outliers: vec![], num_values: 0 });
}

#[test]
fn groups_share_bins_from_zero_to_the_largest_value() {
    let a = ViolinSummary { non_outliers: vec![0.5, 1.5], ..Default::default() };
    let b = ViolinSummary { non_outliers: vec![2.0], ..Default::default() };
    let bins = bin_violins(&[&a, &b], 2, None);
    assert_eq!(bins.value_domain, (0.0, 2.0));
    assert_eq!(bins.bin_starts, vec![0.0, 1.0, 2.0]);
    assert_eq!(bins.bin_counts, vec![vec![1, 1, 0], vec![0, 0, 1]]);
}

#[test]
fn value_min_overrides_the_domain_minimum() {
    let a = ViolinSummary { non_outliers: vec![1.0, 3.0], ..Default::default() };
    assert_eq!(bin_violins(&[&a], 2, Some(1.0)).value_domain, (1.0, 3.0));
}
