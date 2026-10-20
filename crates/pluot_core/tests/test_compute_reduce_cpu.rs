use std::sync::Arc;
use pluot_core::compute::reduce::{
    reduce_min, reduce_max, reduce_sum, reduce_count, reduce_mean, reduce_extent,
    reduce_histogram_with_known_extent, reduce_histogram_with_unknown_extent,
};
use pluot_core::numeric_data::NumericData;
use pluot_core::render_traits::{CategoricalCriteriaParams, EmphasisCriteria, QuantitativeCriteriaParams};

// All tests use the CPU fallback path (gpu_context = None).

// reduce_min

#[tokio::test]
async fn test_reduce_min_basic() {
    let input: Arc<Vec<f32>> = Arc::new(vec![3.0, 1.0, 4.0, 1.5, 9.0, 2.6]);
    let result = reduce_min(None, input, &[], &[]).await;
    assert_eq!(result.background, 1.0);
    assert_eq!(result.foreground, 1.0);
}

#[tokio::test]
async fn test_reduce_min_single() {
    let input: Arc<Vec<f32>> = Arc::new(vec![42.0]);
    assert_eq!(reduce_min(None, input, &[], &[]).await.background, 42.0);
}

#[tokio::test]
async fn test_reduce_min_empty() {
    let input = Arc::new(Vec::<f32>::new());
    assert_eq!(reduce_min(None, input, &[], &[]).await.background, f32::INFINITY);
}

#[tokio::test]
async fn test_reduce_min_negative() {
    let input: Arc<Vec<f32>> = Arc::new(vec![-5.0, -1.0, -100.0, 0.0, 3.0]);
    assert_eq!(reduce_min(None, input, &[], &[]).await.background, -100.0);
}

#[tokio::test]
async fn test_reduce_min_all_same() {
    let input: Arc<Vec<f32>> = Arc::new(vec![7.0; 128]);
    assert_eq!(reduce_min(None, input, &[], &[]).await.background, 7.0);
}

// reduce_max

#[tokio::test]
async fn test_reduce_max_basic() {
    let input: Arc<Vec<f32>> = Arc::new(vec![3.0, 1.0, 4.0, 1.5, 9.0, 2.6]);
    assert_eq!(reduce_max(None, input, &[], &[]).await.background, 9.0);
}

#[tokio::test]
async fn test_reduce_max_single() {
    let input: Arc<Vec<f32>> = Arc::new(vec![42.0]);
    assert_eq!(reduce_max(None, input, &[], &[]).await.background, 42.0);
}

#[tokio::test]
async fn test_reduce_max_empty() {
    assert_eq!(reduce_max(None, Arc::new(Vec::<f32>::new()), &[], &[]).await.background, f32::NEG_INFINITY);
}

#[tokio::test]
async fn test_reduce_max_negative() {
    let input: Arc<Vec<f32>> = Arc::new(vec![-5.0, -1.0, -100.0, -0.5]);
    assert_eq!(reduce_max(None, input, &[], &[]).await.background, -0.5);
}

// reduce_sum

#[tokio::test]
async fn test_reduce_sum_basic() {
    let input: Arc<Vec<f32>> = Arc::new(vec![1.0, 2.0, 3.0, 4.0]);
    assert_eq!(reduce_sum(None, input, &[], &[]).await.background, 10.0);
}

#[tokio::test]
async fn test_reduce_sum_single() {
    let input: Arc<Vec<f32>> = Arc::new(vec![5.5]);
    assert_eq!(reduce_sum(None, input, &[], &[]).await.background, 5.5);
}

#[tokio::test]
async fn test_reduce_sum_empty() {
    assert_eq!(reduce_sum(None, Arc::new(Vec::<f32>::new()), &[], &[]).await.background, 0.0);
}

#[tokio::test]
async fn test_reduce_sum_negative() {
    let input: Arc<Vec<f32>> = Arc::new(vec![-1.0, 2.0, -3.0, 4.0]);
    assert_eq!(reduce_sum(None, input, &[], &[]).await.background, 2.0);
}

// reduce_count

#[tokio::test]
async fn test_reduce_count_basic() {
    let input: Arc<Vec<f32>> = Arc::new(vec![3.0, 1.0, 4.0, 1.5, 9.0]);
    assert_eq!(reduce_count(None, input, &[], &[]).await.background, 5.0);
}

#[tokio::test]
async fn test_reduce_count_empty() {
    assert_eq!(reduce_count(None, Arc::new(Vec::<f32>::new()), &[], &[]).await.background, 0.0);
}

#[tokio::test]
async fn test_reduce_count_filtering_and_selection() {
    // Filtering keeps category 0 (indices 0, 2, 4 --> 3 items). Selection
    // further narrows (orthogonal quantitative column) to just index 4.
    let input: Arc<Vec<f32>> = Arc::new(vec![1.0, 2.0, 3.0, 4.0, 5.0]);
    let filtering = vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
        codes: category_codes(),
        included_codes: vec![0],
    })];
    let selection = vec![EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
        values: NumericData::Float32(Arc::new(vec![10.0, 20.0, 30.0, 40.0, 50.0])),
        min: Some(50.0),
        max: None,
        min_exclusive: None,
        max_exclusive: None,
    })];
    let result = reduce_count(None, input, &filtering, &selection).await;
    assert_eq!(result.background, 3.0);
    assert_eq!(result.foreground, 1.0);
}

// reduce_mean

#[tokio::test]
async fn test_reduce_mean_basic() {
    let input: Arc<Vec<f32>> = Arc::new(vec![1.0, 2.0, 3.0, 4.0]);
    assert_eq!(reduce_mean(None, input, &[], &[]).await.background, 2.5);
}

#[tokio::test]
async fn test_reduce_mean_empty_is_nan() {
    assert!(reduce_mean(None, Arc::new(Vec::<f32>::new()), &[], &[]).await.background.is_nan());
}

#[tokio::test]
async fn test_reduce_mean_filtering_only() {
    // Values [1, 2, 3, 4, 5]; filtering keeps category 0 (values 1, 3, 5).
    let input: Arc<Vec<f32>> = Arc::new(vec![1.0, 2.0, 3.0, 4.0, 5.0]);
    let filtering = vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
        codes: category_codes(),
        included_codes: vec![0],
    })];
    let result = reduce_mean(None, input, &filtering, &[]).await;
    assert_eq!(result.background, 3.0);
    assert_eq!(result.foreground, 3.0);
}

// reduce_extent

#[tokio::test]
async fn test_reduce_extent_basic() {
    let input: Arc<Vec<f32>> = Arc::new(vec![3.0, 1.0, 4.0, 1.5, 9.0, 2.6]);
    assert_eq!(reduce_extent(None, input, &[], &[]).await.background, (1.0, 9.0));
}

#[tokio::test]
async fn test_reduce_extent_single() {
    let input: Arc<Vec<f32>> = Arc::new(vec![42.0]);
    assert_eq!(reduce_extent(None, input, &[], &[]).await.background, (42.0, 42.0));
}

#[tokio::test]
async fn test_reduce_extent_empty() {
    assert_eq!(
        reduce_extent(None, Arc::new(Vec::<f32>::new()), &[], &[]).await.background,
        (f32::INFINITY, f32::NEG_INFINITY),
    );
}

#[tokio::test]
async fn test_reduce_extent_negative() {
    let input: Arc<Vec<f32>> = Arc::new(vec![-10.0, 5.0, 0.0, -3.0, 7.0]);
    assert_eq!(reduce_extent(None, input, &[], &[]).await.background, (-10.0, 7.0));
}

// ── reduce_histogram_with_known_extent ───────────────────────────────────────

#[tokio::test]
async fn test_histogram_known_extent_uniform() {
    // 10 values in [0, 10), 2 bins --> 5 per bin.
    let input: Arc<Vec<f32>> = Arc::new(vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]);
    let bins = reduce_histogram_with_known_extent(None, input, 2, 0.0, 10.0, &[], &[]).await.background;
    assert_eq!(bins, vec![5, 5]);
}

#[tokio::test]
async fn test_histogram_known_extent_single_bin() {
    let input: Arc<Vec<f32>> = Arc::new(vec![1.0, 2.0, 3.0]);
    let bins = reduce_histogram_with_known_extent(None, input, 1, 0.0, 10.0, &[], &[]).await.background;
    assert_eq!(bins, vec![3]);
}

#[tokio::test]
async fn test_histogram_known_extent_out_of_range_clamped() {
    // Values outside [0, 10) should be clamped to edge bins.
    let input: Arc<Vec<f32>> = Arc::new(vec![-5.0, 0.0, 5.0, 10.0, 15.0]);
    let bins = reduce_histogram_with_known_extent(None, input, 2, 0.0, 10.0, &[], &[]).await.background;
    // bin 0: -5.0 (clamped), 0.0; bin 1: 5.0, 10.0 (clamped), 15.0 (clamped)
    assert_eq!(bins, vec![2, 3]);
}

#[tokio::test]
async fn test_histogram_known_extent_empty() {
    let bins =
        reduce_histogram_with_known_extent(None, Arc::new(Vec::<f32>::new()), 4, 0.0, 10.0, &[], &[]).await.background;
    assert_eq!(bins, vec![0, 0, 0, 0]);
}

#[tokio::test]
async fn test_histogram_known_extent_zero_range() {
    // When data_min == data_max, all values land in bin 0.
    let input: Arc<Vec<f32>> = Arc::new(vec![5.0, 5.0, 5.0]);
    let bins = reduce_histogram_with_known_extent(None, input, 4, 5.0, 5.0, &[], &[]).await.background;
    assert_eq!(bins, vec![3, 0, 0, 0]);
}

#[tokio::test]
async fn test_histogram_known_extent_many_bins() {
    // One value per integer, 10 bins across [0, 10).
    let input: Arc<Vec<f32>> = Arc::new(vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]);
    let bins = reduce_histogram_with_known_extent(None, input, 10, 0.0, 10.0, &[], &[]).await.background;
    assert_eq!(bins, vec![1; 10]);
}

// ── reduce_histogram_with_unknown_extent ─────────────────────────────────────

#[tokio::test]
async fn test_histogram_unknown_extent_basic() {
    let input: Arc<Vec<f32>> = Arc::new(vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]);
    let bins = reduce_histogram_with_unknown_extent(None, input, 2, &[], &[]).await.background;
    assert_eq!(bins.len(), 2);
    assert_eq!(bins.iter().sum::<u32>(), 10);
}

#[tokio::test]
async fn test_histogram_unknown_extent_single_value() {
    // All identical --> extent is (v, v), zero range --> all in bin 0.
    let input: Arc<Vec<f32>> = Arc::new(vec![3.0, 3.0, 3.0]);
    let bins = reduce_histogram_with_unknown_extent(None, input, 4, &[], &[]).await.background;
    assert_eq!(bins, vec![3, 0, 0, 0]);
}

#[tokio::test]
async fn test_histogram_unknown_extent_preserves_total() {
    let input: Arc<Vec<f32>> = Arc::new(vec![1.0, 2.0, 3.0, 4.0, 5.0]);
    let bins = reduce_histogram_with_unknown_extent(None, input, 5, &[], &[]).await.background;
    assert_eq!(bins.iter().sum::<u32>(), 5);
}

// ── Filtering/selection criteria (background/foreground) ─────────────────────
//
// `background` is the filter-included set,
// `foreground` is the filter-*and*-selection-included
// subset. Selection criteria may rely on a column entirely orthogonal to the
// filtering criteria's column.

fn category_codes() -> NumericData {
    NumericData::Int32(Arc::new(vec![0, 1, 0, 1, 0]))
}

#[tokio::test]
async fn test_reduce_sum_filtering_only_excludes_from_background_and_foreground() {
    // Values [1, 2, 3, 4, 5]; filtering keeps only category 0 (indices 0, 2, 4
    // --> values 1, 3, 5). No selection criteria, so foreground == background.
    let input: Arc<Vec<f32>> = Arc::new(vec![1.0, 2.0, 3.0, 4.0, 5.0]);
    let filtering = vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
        codes: category_codes(),
        included_codes: vec![0],
    })];
    let result = reduce_sum(None, input, &filtering, &[]).await;
    assert_eq!(result.background, 9.0);
    assert_eq!(result.foreground, 9.0);
}

#[tokio::test]
async fn test_reduce_sum_selection_narrows_foreground_only() {
    // Same filtering as above (category 0 --> values 1, 3, 5, sum 9), plus a
    // selection criteria on an orthogonal quantitative column that further
    // narrows to just value 5 (index 4).
    let input: Arc<Vec<f32>> = Arc::new(vec![1.0, 2.0, 3.0, 4.0, 5.0]);
    let filtering = vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
        codes: category_codes(),
        included_codes: vec![0],
    })];
    let selection = vec![EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
        values: NumericData::Float32(Arc::new(vec![10.0, 20.0, 30.0, 40.0, 50.0])),
        min: Some(50.0),
        max: None,
        min_exclusive: None,
        max_exclusive: None,
    })];
    let result = reduce_sum(None, input, &filtering, &selection).await;
    assert_eq!(result.background, 9.0);
    assert_eq!(result.foreground, 5.0);
}

#[tokio::test]
async fn test_reduce_min_max_background_foreground_diverge() {
    let input: Arc<Vec<f32>> = Arc::new(vec![10.0, -5.0, 8.0, 3.0, 1.0]);
    // Filter out index 1 (-5.0), so background excludes it entirely.
    let filtering = vec![EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
        values: NumericData::Float32(Arc::new(vec![0.0, 1.0, 0.0, 0.0, 0.0])),
        min: None,
        max: Some(0.5),
        min_exclusive: None,
        max_exclusive: None,
    })];
    // Further select only index 3 (value 3.0) as the foreground.
    let selection = vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
        codes: NumericData::Int32(Arc::new(vec![0, 0, 0, 1, 0])),
        included_codes: vec![1],
    })];
    let min_result = reduce_min(None, Arc::clone(&input), &filtering, &selection).await;
    let max_result = reduce_max(None, input, &filtering, &selection).await;
    // Background is {10.0, 8.0, 3.0, 1.0} (index 1 filtered out).
    assert_eq!(min_result.background, 1.0);
    assert_eq!(max_result.background, 10.0);
    // Foreground is just {3.0} (index 3).
    assert_eq!(min_result.foreground, 3.0);
    assert_eq!(max_result.foreground, 3.0);
}

#[tokio::test]
async fn test_reduce_extent_empty_included_codes_excludes_everything() {
    // An explicit empty `included_codes` means nothing is included, distinct
    // from an empty `filtering_criteria` list (which includes everything).
    let input: Arc<Vec<f32>> = Arc::new(vec![1.0, 2.0, 3.0]);
    let filtering = vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
        codes: NumericData::Int32(Arc::new(vec![0, 0, 0])),
        included_codes: vec![],
    })];
    let result = reduce_extent(None, input, &filtering, &[]).await;
    assert_eq!(result.background, (f32::INFINITY, f32::NEG_INFINITY));
    assert_eq!(result.foreground, (f32::INFINITY, f32::NEG_INFINITY));
}

#[tokio::test]
async fn test_histogram_background_foreground_share_bin_edges() {
    // Background (filter-included): all 10 values [0..10). Foreground
    // (additionally selected): only the even-indexed values [0, 2, 4, 6, 8].
    // Bin edges are derived from the background range, so both histograms
    // share the same [0, 10) edges rather than the foreground deriving a
    // narrower range from its own subset.
    let input: Arc<Vec<f32>> = Arc::new(vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]);
    let selection = vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
        codes: NumericData::Int32(Arc::new(vec![0, 1, 0, 1, 0, 1, 0, 1, 0, 1])),
        included_codes: vec![0],
    })];
    let result = reduce_histogram_with_unknown_extent(None, input, 2, &[], &selection).await;
    assert_eq!(result.background, vec![5, 5]);
    // Foreground values are 0, 2, 4, 6, 8 --> bin 0: {0, 2, 4}, bin 1: {6, 8}.
    assert_eq!(result.foreground, vec![3, 2]);
}
