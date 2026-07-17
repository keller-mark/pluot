use std::sync::Arc;
use pluot_core::compute::reduce::{
    reduce_min, reduce_max, reduce_sum, reduce_extent,
    reduce_histogram_with_known_extent, reduce_histogram_with_unknown_extent,
};

// All tests use the CPU fallback path (gpu_context = None).

// reduce_min

#[tokio::test]
async fn test_reduce_min_basic() {
    let input: Arc<Vec<f32>> = Arc::new(vec![3.0, 1.0, 4.0, 1.5, 9.0, 2.6]);
    let result = reduce_min(None, input).await;
    assert_eq!(result, 1.0);
}

#[tokio::test]
async fn test_reduce_min_single() {
    let input: Arc<Vec<f32>> = Arc::new(vec![42.0]);
    assert_eq!(reduce_min(None, input).await, 42.0);
}

#[tokio::test]
async fn test_reduce_min_empty() {
    let input = Arc::new(Vec::<f32>::new());
    assert_eq!(reduce_min(None, input).await, f32::INFINITY);
}

#[tokio::test]
async fn test_reduce_min_negative() {
    let input: Arc<Vec<f32>> = Arc::new(vec![-5.0, -1.0, -100.0, 0.0, 3.0]);
    assert_eq!(reduce_min(None, input).await, -100.0);
}

#[tokio::test]
async fn test_reduce_min_all_same() {
    let input: Arc<Vec<f32>> = Arc::new(vec![7.0; 128]);
    assert_eq!(reduce_min(None, input).await, 7.0);
}

// reduce_max

#[tokio::test]
async fn test_reduce_max_basic() {
    let input: Arc<Vec<f32>> = Arc::new(vec![3.0, 1.0, 4.0, 1.5, 9.0, 2.6]);
    assert_eq!(reduce_max(None, input).await, 9.0);
}

#[tokio::test]
async fn test_reduce_max_single() {
    let input: Arc<Vec<f32>> = Arc::new(vec![42.0]);
    assert_eq!(reduce_max(None, input).await, 42.0);
}

#[tokio::test]
async fn test_reduce_max_empty() {
    assert_eq!(reduce_max(None, Arc::new(Vec::<f32>::new())).await, f32::NEG_INFINITY);
}

#[tokio::test]
async fn test_reduce_max_negative() {
    let input: Arc<Vec<f32>> = Arc::new(vec![-5.0, -1.0, -100.0, -0.5]);
    assert_eq!(reduce_max(None, input).await, -0.5);
}

// reduce_sum

#[tokio::test]
async fn test_reduce_sum_basic() {
    let input: Arc<Vec<f32>> = Arc::new(vec![1.0, 2.0, 3.0, 4.0]);
    assert_eq!(reduce_sum(None, input).await, 10.0);
}

#[tokio::test]
async fn test_reduce_sum_single() {
    let input: Arc<Vec<f32>> = Arc::new(vec![5.5]);
    assert_eq!(reduce_sum(None, input).await, 5.5);
}

#[tokio::test]
async fn test_reduce_sum_empty() {
    assert_eq!(reduce_sum(None, Arc::new(Vec::<f32>::new())).await, 0.0);
}

#[tokio::test]
async fn test_reduce_sum_negative() {
    let input: Arc<Vec<f32>> = Arc::new(vec![-1.0, 2.0, -3.0, 4.0]);
    assert_eq!(reduce_sum(None, input).await, 2.0);
}

// reduce_extent

#[tokio::test]
async fn test_reduce_extent_basic() {
    let input: Arc<Vec<f32>> = Arc::new(vec![3.0, 1.0, 4.0, 1.5, 9.0, 2.6]);
    assert_eq!(reduce_extent(None, input).await, (1.0, 9.0));
}

#[tokio::test]
async fn test_reduce_extent_single() {
    let input: Arc<Vec<f32>> = Arc::new(vec![42.0]);
    assert_eq!(reduce_extent(None, input).await, (42.0, 42.0));
}

#[tokio::test]
async fn test_reduce_extent_empty() {
    assert_eq!(
        reduce_extent(None, Arc::new(Vec::<f32>::new())).await,
        (f32::INFINITY, f32::NEG_INFINITY),
    );
}

#[tokio::test]
async fn test_reduce_extent_negative() {
    let input: Arc<Vec<f32>> = Arc::new(vec![-10.0, 5.0, 0.0, -3.0, 7.0]);
    assert_eq!(reduce_extent(None, input).await, (-10.0, 7.0));
}

// ── reduce_histogram_with_known_extent ───────────────────────────────────────

#[tokio::test]
async fn test_histogram_known_extent_uniform() {
    // 10 values in [0, 10), 2 bins --> 5 per bin.
    let input: Arc<Vec<f32>> = Arc::new(vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]);
    let bins = reduce_histogram_with_known_extent(None, input, 2, 0.0, 10.0).await;
    assert_eq!(bins, vec![5, 5]);
}

#[tokio::test]
async fn test_histogram_known_extent_single_bin() {
    let input: Arc<Vec<f32>> = Arc::new(vec![1.0, 2.0, 3.0]);
    let bins = reduce_histogram_with_known_extent(None, input, 1, 0.0, 10.0).await;
    assert_eq!(bins, vec![3]);
}

#[tokio::test]
async fn test_histogram_known_extent_out_of_range_clamped() {
    // Values outside [0, 10) should be clamped to edge bins.
    let input: Arc<Vec<f32>> = Arc::new(vec![-5.0, 0.0, 5.0, 10.0, 15.0]);
    let bins = reduce_histogram_with_known_extent(None, input, 2, 0.0, 10.0).await;
    // bin 0: -5.0 (clamped), 0.0; bin 1: 5.0, 10.0 (clamped), 15.0 (clamped)
    assert_eq!(bins, vec![2, 3]);
}

#[tokio::test]
async fn test_histogram_known_extent_empty() {
    let bins = reduce_histogram_with_known_extent(None, Arc::new(Vec::<f32>::new()), 4, 0.0, 10.0).await;
    assert_eq!(bins, vec![0, 0, 0, 0]);
}

#[tokio::test]
async fn test_histogram_known_extent_zero_range() {
    // When data_min == data_max, all values land in bin 0.
    let input: Arc<Vec<f32>> = Arc::new(vec![5.0, 5.0, 5.0]);
    let bins = reduce_histogram_with_known_extent(None, input, 4, 5.0, 5.0).await;
    assert_eq!(bins, vec![3, 0, 0, 0]);
}

#[tokio::test]
async fn test_histogram_known_extent_many_bins() {
    // One value per integer, 10 bins across [0, 10).
    let input: Arc<Vec<f32>> = Arc::new(vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]);
    let bins = reduce_histogram_with_known_extent(None, input, 10, 0.0, 10.0).await;
    assert_eq!(bins, vec![1; 10]);
}

// ── reduce_histogram_with_unknown_extent ─────────────────────────────────────

#[tokio::test]
async fn test_histogram_unknown_extent_basic() {
    let input: Arc<Vec<f32>> = Arc::new(vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]);
    let bins = reduce_histogram_with_unknown_extent(None, input, 2).await;
    assert_eq!(bins.len(), 2);
    assert_eq!(bins.iter().sum::<u32>(), 10);
}

#[tokio::test]
async fn test_histogram_unknown_extent_single_value() {
    // All identical --> extent is (v, v), zero range --> all in bin 0.
    let input: Arc<Vec<f32>> = Arc::new(vec![3.0, 3.0, 3.0]);
    let bins = reduce_histogram_with_unknown_extent(None, input, 4).await;
    assert_eq!(bins, vec![3, 0, 0, 0]);
}

#[tokio::test]
async fn test_histogram_unknown_extent_preserves_total() {
    let input: Arc<Vec<f32>> = Arc::new(vec![1.0, 2.0, 3.0, 4.0, 5.0]);
    let bins = reduce_histogram_with_unknown_extent(None, input, 5).await;
    assert_eq!(bins.iter().sum::<u32>(), 5);
}
