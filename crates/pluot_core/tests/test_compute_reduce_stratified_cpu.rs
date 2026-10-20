use std::sync::Arc;
use pluot_core::compute::reduce_stratified::{
    reduce_stratified_count, reduce_stratified_min, reduce_stratified_max, reduce_stratified_sum,
    reduce_stratified_mean, reduce_stratified_extent, reduce_stratified_histogram_with_known_extent,
    reduce_stratified_histogram_with_unknown_extent, StratifyBy,
};
use pluot_core::numeric_data::NumericData;
use pluot_core::render_traits::{EmphasisCriteria, QuantitativeCriteriaParams};

// All tests use the CPU fallback path (gpu_context = None).
//
// Fixture: 6 items, 3 categories: {A: 0, 2, 4}, {B: 1, 5}, {C: 3}.
// Values:                            10,  30, 50    20, 60      40

fn category_column() -> NumericData {
    NumericData::Int32(Arc::new(vec![0, 1, 0, 2, 0, 1]))
}

fn values() -> Arc<Vec<f32>> {
    Arc::new(vec![10.0, 20.0, 30.0, 40.0, 50.0, 60.0])
}

fn stratify_by_abc() -> StratifyBy {
    StratifyBy { codes: category_column(), included_codes: vec![0, 1, 2] }
}

#[tokio::test]
async fn test_stratified_count_basic() {
    let result = reduce_stratified_count(None, values(), &stratify_by_abc(), &[], &[]).await;
    assert_eq!(result.background, vec![3.0, 2.0, 1.0]);
    assert_eq!(result.foreground, result.background);
}

#[tokio::test]
async fn test_stratified_sum_basic() {
    let result = reduce_stratified_sum(None, values(), &stratify_by_abc(), &[], &[]).await;
    // A: 10+30+50=90, B: 20+60=80, C: 40
    assert_eq!(result.background, vec![90.0, 80.0, 40.0]);
}

#[tokio::test]
async fn test_stratified_min_max_basic() {
    let stratify_by = stratify_by_abc();
    let min_result = reduce_stratified_min(None, values(), &stratify_by, &[], &[]).await;
    let max_result = reduce_stratified_max(None, values(), &stratify_by, &[], &[]).await;
    assert_eq!(min_result.background, vec![10.0, 20.0, 40.0]);
    assert_eq!(max_result.background, vec![50.0, 60.0, 40.0]);
}

#[tokio::test]
async fn test_stratified_mean_basic() {
    let result = reduce_stratified_mean(None, values(), &stratify_by_abc(), &[], &[]).await;
    assert_eq!(result.background, vec![30.0, 40.0, 40.0]);
}

#[tokio::test]
async fn test_stratified_extent_basic() {
    let result = reduce_stratified_extent(None, values(), &stratify_by_abc(), &[], &[]).await;
    assert_eq!(result.background, vec![(10.0, 50.0), (20.0, 60.0), (40.0, 40.0)]);
}

#[tokio::test]
async fn test_stratified_empty_included_codes_yields_no_strata() {
    let stratify_by = StratifyBy { codes: category_column(), included_codes: vec![] };
    let result = reduce_stratified_sum(None, values(), &stratify_by, &[], &[]).await;
    assert_eq!(result.background, Vec::<f32>::new());
    assert_eq!(result.foreground, Vec::<f32>::new());
}

#[tokio::test]
async fn test_stratified_code_not_present_yields_zero_count() {
    // Code 99 is not present in the category column at all.
    let stratify_by = StratifyBy { codes: category_column(), included_codes: vec![0, 99] };
    let result = reduce_stratified_count(None, values(), &stratify_by, &[], &[]).await;
    assert_eq!(result.background, vec![3.0, 0.0]);
}

#[tokio::test]
async fn test_stratified_output_order_matches_included_codes() {
    // Requesting strata in a different order permutes the output accordingly.
    let stratify_by = StratifyBy { codes: category_column(), included_codes: vec![2, 0, 1] };
    let result = reduce_stratified_sum(None, values(), &stratify_by, &[], &[]).await;
    assert_eq!(result.background, vec![40.0, 90.0, 80.0]);
}

// ── Selection narrows foreground independently per stratum ───────────────────

#[tokio::test]
async fn test_stratified_selection_narrows_foreground_only() {
    // Selection (orthogonal quantitative column) keeps only items with
    // value >= 100 in an unrelated column: indices 2 (item value 30) and 5
    // (item value 60), i.e. one item from stratum A and one from stratum B.
    let selection = vec![EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
        values: NumericData::Float32(Arc::new(vec![0.0, 0.0, 100.0, 0.0, 0.0, 100.0])),
        min: Some(50.0),
        max: None,
        min_exclusive: None,
        max_exclusive: None,
    })];
    let result = reduce_stratified_sum(None, values(), &stratify_by_abc(), &[], &selection).await;
    assert_eq!(result.background, vec![90.0, 80.0, 40.0]);
    // A: only index 2 (30.0). B: only index 5 (60.0). C: nothing selected --> 0.0.
    assert_eq!(result.foreground, vec![30.0, 60.0, 0.0]);
}

#[tokio::test]
async fn test_stratified_filtering_excludes_before_stratifying() {
    // Filter out index 4 (a stratum-A item worth 50.0) on an orthogonal column.
    let filtering = vec![EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
        values: NumericData::Float32(Arc::new(vec![0.0, 0.0, 0.0, 0.0, 1.0, 0.0])),
        min: None,
        max: Some(0.5),
        min_exclusive: None,
        max_exclusive: None,
    })];
    let result = reduce_stratified_sum(None, values(), &stratify_by_abc(), &filtering, &[]).await;
    // A: 10 + 30 (50 filtered out) = 40, B: 80, C: 40.
    assert_eq!(result.background, vec![40.0, 80.0, 40.0]);
}

// ── Histogram ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_stratified_histogram_with_known_extent() {
    let result = reduce_stratified_histogram_with_known_extent(
        None, values(), &stratify_by_abc(), 2, 0.0, 60.0, &[], &[],
    )
    .await;
    // Bins over [0, 60) in 2 bins: [0, 30) and [30, 60).
    // A: {10, 30, 50} --> bin0: 10; bin1: 30, 50 --> [1, 2]
    // B: {20, 60} --> bin0: 20; bin1: 60(clamped) --> [1, 1]
    // C: {40} --> bin1: 40 --> [0, 1]
    assert_eq!(result.background, vec![vec![1, 2], vec![1, 1], vec![0, 1]]);
}

#[tokio::test]
async fn test_stratified_histogram_with_unknown_extent_shares_bin_edges_across_strata() {
    // Extent is derived from the whole (non-stratified) filter-included set:
    // [10, 60]. All 3 strata's histograms must share these edges.
    let result =
        reduce_stratified_histogram_with_unknown_extent(None, values(), &stratify_by_abc(), 2, &[], &[]).await;
    // Edges: [10, 35), [35, 60]. A: {10, 30, 50} --> [2, 1]. B: {20, 60} --> [1, 1]. C: {40} --> [0, 1].
    assert_eq!(result.background, vec![vec![2, 1], vec![1, 1], vec![0, 1]]);
    let totals: Vec<u32> = result.background.iter().map(|bins| bins.iter().sum()).collect();
    assert_eq!(totals, vec![3, 2, 1]);
}
