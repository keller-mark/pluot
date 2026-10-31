use std::sync::Arc;

use pluot_core::compute::reduce_stratified::StratifyBy;
use pluot_core::compute::stratified_violin::StratifiedViolinInput;
use pluot_core::numeric_data::NumericData;
use pluot_core::render_traits::{CategoricalCriteriaParams, EmphasisCriteria, QuantitativeCriteriaParams};
use pluot_zarr::violin_data::{bin_violin, summarize_violin, violin_bin_edges, violin_bin_starts};

#[test]
fn summarizes_only_the_given_rows() {
    let values = [9.0, 1.0, 2.0, 3.0, 4.0];
    let stats = summarize_violin(&values, &[4, 3, 2, 1], true);
    assert_eq!(stats.quartiles, Some((1.75, 2.5, 3.25)));
    assert_eq!(stats.non_outlier_extent, Some((1.0, 4.0)));
    assert_eq!(stats.num_values, 4);
}

#[test]
fn zeros_and_nans_are_dropped_unless_zeros_are_kept() {
    let values = [0.0, 0.0, 2.0, 4.0, f32::NAN];
    assert_eq!(summarize_violin(&values, &[0, 1, 2, 3, 4], false).num_values, 2);
    assert_eq!(summarize_violin(&values, &[0, 1, 2, 3, 4], true).num_values, 4);
}

#[test]
fn values_beyond_three_standard_deviations_are_outliers() {
    let mut values = vec![1.0f32; 20];
    values.push(1000.0);
    let rows: Vec<u32> = (0..values.len() as u32).collect();
    let stats = summarize_violin(&values, &rows, true);
    assert_eq!(stats.non_outlier_extent, Some((1.0, 1.0)));
    assert_eq!(stats.num_values, 21);
    assert_eq!(bin_violin(&values, &rows, true, stats.non_outlier_bounds, (0.0, 1000.0), &[500.0]), vec![20, 0]);
}

#[test]
fn an_empty_group_has_no_quartiles() {
    let stats = summarize_violin(&[1.0], &[], true);
    assert_eq!((stats.num_values, stats.quartiles, stats.non_outlier_extent), (0, None, None));
}

#[test]
fn groups_share_bins_from_zero_to_the_largest_value() {
    let (domain, thresholds) = violin_bin_edges([(0.5, 1.5), (2.0, 2.0)], 2, None);
    assert_eq!(domain, (0.0, 2.0));
    assert_eq!(violin_bin_starts(domain, &thresholds), vec![0.0, 1.0, 2.0]);
    let values = [0.5, 1.5, 2.0];
    let unbounded = (f32::NEG_INFINITY, f32::INFINITY);
    assert_eq!(bin_violin(&values, &[0, 1], true, unbounded, domain, &thresholds), vec![1, 1, 0]);
    assert_eq!(bin_violin(&values, &[2], true, unbounded, domain, &thresholds), vec![0, 0, 1]);
}

#[test]
fn value_min_overrides_the_domain_minimum() {
    assert_eq!(violin_bin_edges([(1.0, 3.0)], 2, Some(1.0)).0, (1.0, 3.0));
}

/// Values with a long tail, zeros and NaNs, in three categories (and some missing), with
/// filtering and selection criteria on independent columns.
fn violin_input_columns(len: usize) -> (NumericData, StratifyBy, Vec<EmphasisCriteria>, Vec<EmphasisCriteria>) {
    let values: Vec<f32> = (0..len)
        .map(|i| match i % 97 {
            0 => f32::NAN,
            1..=20 => 0.0,
            96 => 500.0 + i as f32,
            _ => ((i as u64 * 2_654_435_761) % 1_000_003) as f32 / 100_000.0,
        })
        .collect();
    let codes = NumericData::Int8(Arc::new((0..len).map(|i| ((i * 31) % 4) as i8 - 1).collect()));
    let stratify_by = StratifyBy { codes, included_codes: vec![2, 0, 1, -7] };
    let filtering = vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
        codes: NumericData::Uint8(Arc::new((0..len).map(|i| (i % 5) as u8).collect())),
        included_codes: vec![0, 1, 2, 3],
    })];
    let selection = vec![EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
        values: NumericData::Float32(Arc::new((0..len).map(|i| (i % 11) as f32).collect())),
        min: Some(4.0),
        max: None,
        min_exclusive: None,
        max_exclusive: None,
    })];
    (NumericData::Float32(Arc::new(values)), stratify_by, filtering, selection)
}

#[cfg(not(feature = "lacks_gpu"))]
#[tokio::test]
async fn gpu_violins_match_cpu_violins() {
    use pluot_core::render_types::GpuContext;
    use pluot_zarr::violin_data::{load_violin_bins, load_violin_stats};

    let (device, queue) = pluot_core::cache::get_or_init_gpu_context().await.expect("GPU context");
    let gpu_context = GpuContext { device: &device, queue: &queue };
    let (values, stratify_by, filtering, selection) = violin_input_columns(50_000);
    for keep_zeros in [true, false] {
        let input = StratifiedViolinInput { values: &values, stratify_by: &stratify_by, filtering_criteria: &filtering, selection_criteria: &selection, keep_zeros };
        let cpu = load_violin_stats(None, &input, &[], false).await;
        let gpu = load_violin_stats(Some(&gpu_context), &input, &[], false).await;
        for (cpu, gpu) in cpu.background.iter().chain(&cpu.foreground).zip(gpu.background.iter().chain(&gpu.foreground)) {
            assert_eq!(gpu.num_values, cpu.num_values);
            let close = |a: f32, b: f32, tolerance: f32| a == b || (a - b).abs() <= tolerance * (1.0 + b.abs());
            let (gpu_bounds, cpu_bounds) = (gpu.non_outlier_bounds, cpu.non_outlier_bounds);
            assert!(close(gpu_bounds.0, cpu_bounds.0, 1e-3) && close(gpu_bounds.1, cpu_bounds.1, 1e-3), "{gpu_bounds:?} != {cpu_bounds:?}");
            assert_eq!(gpu.non_outlier_extent, cpu.non_outlier_extent);
            match (gpu.quartiles, cpu.quartiles) {
                (None, None) => {}
                (Some(g), Some(c)) => {
                    // Rank conventions differ by at most one value, whose neighbors are close in this near-continuous data.
                    for (g, c) in [(g.0, c.0), (g.1, c.1), (g.2, c.2)] {
                        assert!(close(g, c, 1e-2), "{g} != {c}");
                    }
                }
                (g, c) => panic!("{g:?} != {c:?}"),
            }
        }
        let extents = cpu.background.iter().chain(&cpu.foreground).filter_map(|stats| stats.non_outlier_extent);
        let (domain, thresholds) = violin_bin_edges(extents.collect::<Vec<_>>(), 16, None);
        let cpu_bins = load_violin_bins(None, &input, &cpu, domain, &thresholds, &[], false).await;
        let gpu_bins = load_violin_bins(Some(&gpu_context), &input, &cpu, domain, &thresholds, &[], false).await;
        assert_eq!(gpu_bins, cpu_bins);
        assert!(cpu.background.iter().zip(&cpu.foreground).any(|(bg, fg)| fg.num_values < bg.num_values), "the selection narrows the violins");
    }
}
