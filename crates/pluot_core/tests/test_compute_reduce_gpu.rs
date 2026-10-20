//! GPU compute shader tests for the reduce module.
//!
//! Skipped when compiled with --features lacks_gpu (e.g. headless CI).
#![cfg(not(feature = "lacks_gpu"))]

use std::sync::Arc;
use pluot_core::cache::get_or_init_gpu_context;
use pluot_core::render_types::GpuContext;
use pluot_core::compute::reduce::{
    reduce_min, reduce_max, reduce_sum, reduce_count, reduce_mean, reduce_extent,
    reduce_histogram_with_known_extent, reduce_histogram_with_unknown_extent,
};
use pluot_core::numeric_data::NumericData;
use pluot_core::render_traits::{CategoricalCriteriaParams, EmphasisCriteria, QuantitativeCriteriaParams};

/// Helper: obtain a GpuContext for one test invocation.
async fn gpu_ctx() -> (pluot_core::wgpu::Device, pluot_core::wgpu::Queue) {
    get_or_init_gpu_context()
        .await
        .expect("No suitable GPU adapter found. Run with --features lacks_gpu to skip GPU tests")
}

// reduce_min (GPU)

#[tokio::test]
async fn test_gpu_reduce_min_basic() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    let input: Arc<Vec<f32>> = Arc::new(vec![3.0, 1.0, 4.0, 1.5, 9.0, 2.6]);
    assert_eq!(reduce_min(Some(&ctx), input, &[], &[]).await.background, 1.0);
}

#[tokio::test]
async fn test_gpu_reduce_min_negative() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    let input: Arc<Vec<f32>> = Arc::new(vec![-5.0, -1.0, -100.0, 0.0, 3.0]);
    assert_eq!(reduce_min(Some(&ctx), input, &[], &[]).await.background, -100.0);
}

#[tokio::test]
async fn test_gpu_reduce_min_large() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    // 1000 elements - spans multiple workgroups (64 threads each).
    let mut data: Vec<f32> = (0..1000).map(|i| i as f32).collect();
    data[537] = -42.0;
    let input = Arc::new(data);
    assert_eq!(reduce_min(Some(&ctx), input, &[], &[]).await.background, -42.0);
}

// reduce_max (GPU)

#[tokio::test]
async fn test_gpu_reduce_max_basic() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    let input: Arc<Vec<f32>> = Arc::new(vec![3.0, 1.0, 4.0, 1.5, 9.0, 2.6]);
    assert_eq!(reduce_max(Some(&ctx), input, &[], &[]).await.background, 9.0);
}

#[tokio::test]
async fn test_gpu_reduce_max_negative() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    let input: Arc<Vec<f32>> = Arc::new(vec![-5.0, -1.0, -100.0, -0.5]);
    assert_eq!(reduce_max(Some(&ctx), input, &[], &[]).await.background, -0.5);
}

#[tokio::test]
async fn test_gpu_reduce_max_large() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    let mut data: Vec<f32> = (0..1000).map(|i| -(i as f32)).collect();
    data[321] = 999.0;
    let input = Arc::new(data);
    assert_eq!(reduce_max(Some(&ctx), input, &[], &[]).await.background, 999.0);
}

// reduce_sum (GPU)

#[tokio::test]
async fn test_gpu_reduce_sum_basic() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    let input: Arc<Vec<f32>> = Arc::new(vec![1.0, 2.0, 3.0, 4.0]);
    assert_eq!(reduce_sum(Some(&ctx), input, &[], &[]).await.background, 10.0);
}

#[tokio::test]
async fn test_gpu_reduce_sum_large() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    // 256 ones --> sum should be 256.
    let input: Arc<Vec<f32>> = Arc::new(vec![1.0; 256]);
    assert_eq!(reduce_sum(Some(&ctx), input, &[], &[]).await.background, 256.0);
}

// reduce_count (GPU)

#[tokio::test]
async fn test_gpu_reduce_count_basic() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    let input: Arc<Vec<f32>> = Arc::new(vec![3.0, 1.0, 4.0, 1.5, 9.0]);
    assert_eq!(reduce_count(Some(&ctx), input, &[], &[]).await.background, 5.0);
}

#[tokio::test]
async fn test_gpu_reduce_count_filtering_and_selection() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    let input: Arc<Vec<f32>> = Arc::new(vec![1.0, 2.0, 3.0, 4.0, 5.0]);
    let filtering = vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
        codes: NumericData::Int32(Arc::new(vec![0, 1, 0, 1, 0])),
        included_codes: vec![0],
    })];
    let selection = vec![EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
        values: NumericData::Float32(Arc::new(vec![10.0, 20.0, 30.0, 40.0, 50.0])),
        min: Some(50.0),
        max: None,
        min_exclusive: None,
        max_exclusive: None,
    })];
    let result = reduce_count(Some(&ctx), input, &filtering, &selection).await;
    assert_eq!(result.background, 3.0);
    assert_eq!(result.foreground, 1.0);
}

#[tokio::test]
async fn test_gpu_reduce_count_quantitative_exclusive_bounds() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    let input: Arc<Vec<f32>> = Arc::new(vec![1.0, 2.0, 3.0, 4.0, 5.0]);
    let values = NumericData::Float32(Arc::new(vec![10.0, 20.0, 30.0, 40.0, 50.0]));

    // Half-open `[20, 40)`: keeps 20 and 30, drops 40 -- the case that makes
    // adjacent histogram bins partition their items rather than double-count
    // the shared edge.
    let filtering = vec![EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
        values: values.clone(),
        min: Some(20.0),
        max: Some(40.0),
        min_exclusive: None,
        max_exclusive: Some(true),
    })];
    // Selection is one-sided and exclusive: `> 20` keeps 30 of those two.
    let selection = vec![EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
        values,
        min: Some(20.0),
        max: None,
        min_exclusive: Some(true),
        max_exclusive: None,
    })];
    let result = reduce_count(Some(&ctx), input, &filtering, &selection).await;
    assert_eq!(result.background, 2.0);
    assert_eq!(result.foreground, 1.0);
}

#[tokio::test]
async fn test_gpu_reduce_count_unbounded_quantitative_criteria_includes_everything() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    let input: Arc<Vec<f32>> = Arc::new(vec![1.0, 2.0, 3.0, 4.0, 5.0]);

    // A quantitative criteria with neither bound set includes every item, and
    // so binds no value texture at all -- the categorical criteria AND-ed
    // after it must still land on the binding index it expects (indices 0, 2,
    // 4 --> 3 items).
    let filtering = vec![
        EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
            values: NumericData::Float32(Arc::new(vec![10.0, 20.0, 30.0, 40.0, 50.0])),
            min: None,
            max: None,
            min_exclusive: None,
            max_exclusive: None,
        }),
        EmphasisCriteria::Categorical(CategoricalCriteriaParams {
            codes: NumericData::Int32(Arc::new(vec![0, 1, 0, 1, 0])),
            included_codes: vec![0],
        }),
    ];
    let result = reduce_count(Some(&ctx), input, &filtering, &[]).await;
    assert_eq!(result.background, 3.0);
    // No selection criteria --> foreground equals background.
    assert_eq!(result.foreground, result.background);
}

// reduce_mean (GPU)

#[tokio::test]
async fn test_gpu_reduce_mean_basic() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    let input: Arc<Vec<f32>> = Arc::new(vec![1.0, 2.0, 3.0, 4.0]);
    assert_eq!(reduce_mean(Some(&ctx), input, &[], &[]).await.background, 2.5);
}

#[tokio::test]
async fn test_gpu_reduce_mean_large() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    // 1000 elements spans multiple workgroups/chunks; mean of 0..1000 is 499.5.
    let input: Arc<Vec<f32>> = Arc::new((0..1000).map(|i| i as f32).collect());
    assert_eq!(reduce_mean(Some(&ctx), input, &[], &[]).await.background, 499.5);
}

// reduce_extent (GPU)

#[tokio::test]
async fn test_gpu_reduce_extent_basic() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    let input: Arc<Vec<f32>> = Arc::new(vec![3.0, 1.0, 4.0, 1.5, 9.0, 2.6]);
    assert_eq!(reduce_extent(Some(&ctx), input, &[], &[]).await.background, (1.0, 9.0));
}

#[tokio::test]
async fn test_gpu_reduce_extent_negative() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    let input: Arc<Vec<f32>> = Arc::new(vec![-10.0, 5.0, 0.0, -3.0, 7.0]);
    assert_eq!(reduce_extent(Some(&ctx), input, &[], &[]).await.background, (-10.0, 7.0));
}

#[tokio::test]
async fn test_gpu_reduce_extent_large() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    let mut data: Vec<f32> = (0..500).map(|i| i as f32).collect();
    data[123] = -99.0;
    data[456] = 9999.0;
    let input = Arc::new(data);
    assert_eq!(reduce_extent(Some(&ctx), input, &[], &[]).await.background, (-99.0, 9999.0));
}

// ── reduce_histogram_with_known_extent (GPU) ─────────────────────────────────

#[tokio::test]
async fn test_gpu_histogram_known_extent_uniform() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    let input: Arc<Vec<f32>> = Arc::new(vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]);
    let bins = reduce_histogram_with_known_extent(Some(&ctx), input, 2, 0.0, 10.0, &[], &[]).await.background;
    assert_eq!(bins, vec![5, 5]);
}

#[tokio::test]
async fn test_gpu_histogram_known_extent_out_of_range_clamped() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    let input: Arc<Vec<f32>> = Arc::new(vec![-5.0, 0.0, 5.0, 10.0, 15.0]);
    let bins = reduce_histogram_with_known_extent(Some(&ctx), input, 2, 0.0, 10.0, &[], &[]).await.background;
    assert_eq!(bins, vec![2, 3]);
}

#[tokio::test]
async fn test_gpu_histogram_known_extent_many_bins() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    let input: Arc<Vec<f32>> = Arc::new(vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]);
    let bins = reduce_histogram_with_known_extent(Some(&ctx), input, 10, 0.0, 10.0, &[], &[]).await.background;
    assert_eq!(bins, vec![1; 10]);
}

#[tokio::test]
async fn test_gpu_histogram_known_extent_zero_range() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    let input: Arc<Vec<f32>> = Arc::new(vec![5.0, 5.0, 5.0]);
    let bins = reduce_histogram_with_known_extent(Some(&ctx), input, 4, 5.0, 5.0, &[], &[]).await.background;
    assert_eq!(bins, vec![3, 0, 0, 0]);
}

#[tokio::test]
async fn test_gpu_histogram_known_extent_large() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    // 1000 values in [0, 1000), 10 bins --> 100 per bin.
    let input: Arc<Vec<f32>> = Arc::new((0..1000).map(|i| i as f32).collect());
    let bins = reduce_histogram_with_known_extent(Some(&ctx), input, 10, 0.0, 1000.0, &[], &[]).await.background;
    assert_eq!(bins.iter().sum::<u32>(), 1000);
    assert_eq!(bins, vec![100; 10]);
}

// ── reduce_histogram_with_unknown_extent (GPU) ───────────────────────────────

#[tokio::test]
async fn test_gpu_histogram_unknown_extent_basic() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    let input: Arc<Vec<f32>> = Arc::new(vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]);
    let bins = reduce_histogram_with_unknown_extent(Some(&ctx), input, 2, &[], &[]).await.background;
    assert_eq!(bins.len(), 2);
    assert_eq!(bins.iter().sum::<u32>(), 10);
}

#[tokio::test]
async fn test_gpu_histogram_unknown_extent_single_value() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    let input: Arc<Vec<f32>> = Arc::new(vec![3.0, 3.0, 3.0]);
    let bins = reduce_histogram_with_unknown_extent(Some(&ctx), input, 4, &[], &[]).await.background;
    assert_eq!(bins, vec![3, 0, 0, 0]);
}

#[tokio::test]
async fn test_gpu_histogram_unknown_extent_preserves_total() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    let input: Arc<Vec<f32>> = Arc::new(vec![1.0, 2.0, 3.0, 4.0, 5.0]);
    let bins = reduce_histogram_with_unknown_extent(Some(&ctx), input, 5, &[], &[]).await.background;
    assert_eq!(bins.iter().sum::<u32>(), 5);
}

// ── Filtering/selection criteria (background/foreground) ─────────────────────
//
// `background` is the filter-included set,
// `foreground` is the filter-*and*-selection-included
// subset.

#[tokio::test]
async fn test_gpu_reduce_sum_filtering_and_selection() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };

    // Values [1, 2, 3, 4, 5]; filtering keeps category 0 (indices 0, 2, 4 -->
    // values 1, 3, 5, sum 9). Selection further narrows (on an orthogonal
    // quantitative column) to just index 4 (value 5).
    let input: Arc<Vec<f32>> = Arc::new(vec![1.0, 2.0, 3.0, 4.0, 5.0]);
    let filtering = vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
        codes: NumericData::Int32(Arc::new(vec![0, 1, 0, 1, 0])),
        included_codes: vec![0],
    })];
    let selection = vec![EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
        values: NumericData::Float32(Arc::new(vec![10.0, 20.0, 30.0, 40.0, 50.0])),
        min: Some(50.0),
        max: None,
        min_exclusive: None,
        max_exclusive: None,
    })];

    let result = reduce_sum(Some(&ctx), input, &filtering, &selection).await;
    assert_eq!(result.background, 9.0);
    assert_eq!(result.foreground, 5.0);
}

#[tokio::test]
async fn test_gpu_reduce_extent_no_selection_matches_background() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };

    let input: Arc<Vec<f32>> = Arc::new(vec![10.0, -5.0, 8.0, 3.0, 1.0]);
    let filtering = vec![EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
        values: NumericData::Float32(Arc::new(vec![0.0, 1.0, 0.0, 0.0, 0.0])),
        min: None,
        max: Some(0.5),
        min_exclusive: None,
        max_exclusive: None,
    })];

    let result = reduce_extent(Some(&ctx), input, &filtering, &[]).await;
    // Background excludes index 1 (-5.0) --> {10.0, 8.0, 3.0, 1.0}.
    assert_eq!(result.background, (1.0, 10.0));
    // No selection criteria --> foreground equals background.
    assert_eq!(result.foreground, result.background);
}

#[tokio::test]
async fn test_gpu_histogram_background_foreground_share_bin_edges() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };

    // Background: all 10 values [0..10). Foreground: only even-indexed values
    // [0, 2, 4, 6, 8]. Bin edges come from the background range, so both
    // histograms share [0, 10) rather than the foreground deriving its own,
    // narrower range.
    let input: Arc<Vec<f32>> = Arc::new(vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]);
    let selection = vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
        codes: NumericData::Int32(Arc::new(vec![0, 1, 0, 1, 0, 1, 0, 1, 0, 1])),
        included_codes: vec![0],
    })];

    let result = reduce_histogram_with_unknown_extent(Some(&ctx), input, 2, &[], &selection).await;
    assert_eq!(result.background, vec![5, 5]);
    // Foreground values are 0, 2, 4, 6, 8 --> bin 0: {0, 2, 4}, bin 1: {6, 8}.
    assert_eq!(result.foreground, vec![3, 2]);
}
