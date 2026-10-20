//! GPU compute shader tests for the reduce_stratified module.
//!
//! Skipped when compiled with --features lacks_gpu (e.g. headless CI).
#![cfg(not(feature = "lacks_gpu"))]

use std::sync::Arc;
use pluot_core::cache::get_or_init_gpu_context;
use pluot_core::render_types::GpuContext;
use pluot_core::compute::reduce_stratified::{
    reduce_stratified_count, reduce_stratified_min, reduce_stratified_max, reduce_stratified_sum,
    reduce_stratified_mean, reduce_stratified_extent, reduce_stratified_histogram_with_known_extent,
    StratifyBy,
};
use pluot_core::numeric_data::NumericData;
use pluot_core::render_traits::{EmphasisCriteria, QuantitativeCriteriaParams};

/// Helper: obtain a GpuContext for one test invocation.
async fn gpu_ctx() -> (pluot_core::wgpu::Device, pluot_core::wgpu::Queue) {
    get_or_init_gpu_context()
        .await
        .expect("No suitable GPU adapter found. Run with --features lacks_gpu to skip GPU tests")
}

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
async fn test_gpu_stratified_count_and_sum() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    let stratify_by = stratify_by_abc();

    let count = reduce_stratified_count(Some(&ctx), values(), &stratify_by, &[], &[]).await;
    assert_eq!(count.background, vec![3.0, 2.0, 1.0]);

    let sum = reduce_stratified_sum(Some(&ctx), values(), &stratify_by, &[], &[]).await;
    assert_eq!(sum.background, vec![90.0, 80.0, 40.0]);
}

#[tokio::test]
async fn test_gpu_stratified_min_max_mean() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    let stratify_by = stratify_by_abc();

    let min_result = reduce_stratified_min(Some(&ctx), values(), &stratify_by, &[], &[]).await;
    let max_result = reduce_stratified_max(Some(&ctx), values(), &stratify_by, &[], &[]).await;
    let mean_result = reduce_stratified_mean(Some(&ctx), values(), &stratify_by, &[], &[]).await;

    assert_eq!(min_result.background, vec![10.0, 20.0, 40.0]);
    assert_eq!(max_result.background, vec![50.0, 60.0, 40.0]);
    assert_eq!(mean_result.background, vec![30.0, 40.0, 40.0]);
}

#[tokio::test]
async fn test_gpu_stratified_extent() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    let result = reduce_stratified_extent(Some(&ctx), values(), &stratify_by_abc(), &[], &[]).await;
    assert_eq!(result.background, vec![(10.0, 50.0), (20.0, 60.0), (40.0, 40.0)]);
}

#[tokio::test]
async fn test_gpu_stratified_histogram_with_known_extent() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    let result = reduce_stratified_histogram_with_known_extent(
        Some(&ctx), values(), &stratify_by_abc(), 2, 0.0, 60.0, &[], &[],
    )
    .await;
    assert_eq!(result.background, vec![vec![1, 2], vec![1, 1], vec![0, 1]]);
}

#[tokio::test]
async fn test_gpu_stratified_selection_narrows_foreground_only() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };

    // Selection (orthogonal quantitative column) keeps only indices 2 and 5.
    let selection = vec![EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
        values: NumericData::Float32(Arc::new(vec![0.0, 0.0, 100.0, 0.0, 0.0, 100.0])),
        min: Some(50.0),
        max: None,
        min_exclusive: None,
        max_exclusive: None,
    })];
    let result =
        reduce_stratified_sum(Some(&ctx), values(), &stratify_by_abc(), &[], &selection).await;
    assert_eq!(result.background, vec![90.0, 80.0, 40.0]);
    assert_eq!(result.foreground, vec![30.0, 60.0, 0.0]);
}
