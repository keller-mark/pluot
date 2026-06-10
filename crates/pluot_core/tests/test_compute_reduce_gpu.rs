//! GPU compute shader tests for the reduce module.
//!
//! Skipped when compiled with --features lacks_gpu (e.g. headless CI).
#![cfg(not(feature = "lacks_gpu"))]

use std::sync::Arc;
use pluot_core::cache::get_or_init_gpu_context;
use pluot_core::render_types::GpuContext;
use pluot_core::compute::reduce::{
    reduce_min, reduce_max, reduce_sum, reduce_extent,
    reduce_histogram_with_known_extent, reduce_histogram_with_unknown_extent,
};

/// Helper: obtain a GpuContext for one test invocation.
async fn gpu_ctx() -> (pluot_core::wgpu::Device, pluot_core::wgpu::Queue) {
    get_or_init_gpu_context()
        .await
        .expect("No suitable GPU adapter found — run with --features lacks_gpu to skip GPU tests")
}

// reduce_min (GPU)

#[tokio::test]
async fn test_gpu_reduce_min_basic() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    let input = Arc::new(vec![3.0, 1.0, 4.0, 1.5, 9.0, 2.6]);
    assert_eq!(reduce_min(Some(&ctx), input).await, 1.0);
}

#[tokio::test]
async fn test_gpu_reduce_min_negative() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    let input = Arc::new(vec![-5.0, -1.0, -100.0, 0.0, 3.0]);
    assert_eq!(reduce_min(Some(&ctx), input).await, -100.0);
}

#[tokio::test]
async fn test_gpu_reduce_min_large() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    // 1000 elements — spans multiple workgroups (64 threads each).
    let mut data: Vec<f32> = (0..1000).map(|i| i as f32).collect();
    data[537] = -42.0;
    let input = Arc::new(data);
    assert_eq!(reduce_min(Some(&ctx), input).await, -42.0);
}

// reduce_max (GPU)

#[tokio::test]
async fn test_gpu_reduce_max_basic() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    let input = Arc::new(vec![3.0, 1.0, 4.0, 1.5, 9.0, 2.6]);
    assert_eq!(reduce_max(Some(&ctx), input).await, 9.0);
}

#[tokio::test]
async fn test_gpu_reduce_max_negative() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    let input = Arc::new(vec![-5.0, -1.0, -100.0, -0.5]);
    assert_eq!(reduce_max(Some(&ctx), input).await, -0.5);
}

#[tokio::test]
async fn test_gpu_reduce_max_large() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    let mut data: Vec<f32> = (0..1000).map(|i| -(i as f32)).collect();
    data[321] = 999.0;
    let input = Arc::new(data);
    assert_eq!(reduce_max(Some(&ctx), input).await, 999.0);
}

// reduce_sum (GPU)

#[tokio::test]
async fn test_gpu_reduce_sum_basic() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    let input = Arc::new(vec![1.0, 2.0, 3.0, 4.0]);
    assert_eq!(reduce_sum(Some(&ctx), input).await, 10.0);
}

#[tokio::test]
async fn test_gpu_reduce_sum_large() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    // 256 ones --> sum should be 256.
    let input = Arc::new(vec![1.0; 256]);
    assert_eq!(reduce_sum(Some(&ctx), input).await, 256.0);
}

// reduce_extent (GPU)

#[tokio::test]
async fn test_gpu_reduce_extent_basic() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    let input = Arc::new(vec![3.0, 1.0, 4.0, 1.5, 9.0, 2.6]);
    assert_eq!(reduce_extent(Some(&ctx), input).await, (1.0, 9.0));
}

#[tokio::test]
async fn test_gpu_reduce_extent_negative() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    let input = Arc::new(vec![-10.0, 5.0, 0.0, -3.0, 7.0]);
    assert_eq!(reduce_extent(Some(&ctx), input).await, (-10.0, 7.0));
}

#[tokio::test]
async fn test_gpu_reduce_extent_large() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    let mut data: Vec<f32> = (0..500).map(|i| i as f32).collect();
    data[123] = -99.0;
    data[456] = 9999.0;
    let input = Arc::new(data);
    assert_eq!(reduce_extent(Some(&ctx), input).await, (-99.0, 9999.0));
}

// ── reduce_histogram_with_known_extent (GPU) ─────────────────────────────────

#[tokio::test]
async fn test_gpu_histogram_known_extent_uniform() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    let input = Arc::new(vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]);
    let bins = reduce_histogram_with_known_extent(Some(&ctx), input, 2, 0.0, 10.0).await;
    assert_eq!(bins, vec![5, 5]);
}

#[tokio::test]
async fn test_gpu_histogram_known_extent_out_of_range_clamped() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    let input = Arc::new(vec![-5.0, 0.0, 5.0, 10.0, 15.0]);
    let bins = reduce_histogram_with_known_extent(Some(&ctx), input, 2, 0.0, 10.0).await;
    assert_eq!(bins, vec![2, 3]);
}

#[tokio::test]
async fn test_gpu_histogram_known_extent_many_bins() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    let input = Arc::new(vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]);
    let bins = reduce_histogram_with_known_extent(Some(&ctx), input, 10, 0.0, 10.0).await;
    assert_eq!(bins, vec![1; 10]);
}

#[tokio::test]
async fn test_gpu_histogram_known_extent_zero_range() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    let input = Arc::new(vec![5.0, 5.0, 5.0]);
    let bins = reduce_histogram_with_known_extent(Some(&ctx), input, 4, 5.0, 5.0).await;
    assert_eq!(bins, vec![3, 0, 0, 0]);
}

#[tokio::test]
async fn test_gpu_histogram_known_extent_large() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    // 1000 values in [0, 1000), 10 bins --> 100 per bin.
    let input = Arc::new((0..1000).map(|i| i as f32).collect());
    let bins = reduce_histogram_with_known_extent(Some(&ctx), input, 10, 0.0, 1000.0).await;
    assert_eq!(bins.iter().sum::<u32>(), 1000);
    assert_eq!(bins, vec![100; 10]);
}

// ── reduce_histogram_with_unknown_extent (GPU) ───────────────────────────────

#[tokio::test]
async fn test_gpu_histogram_unknown_extent_basic() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    let input = Arc::new(vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]);
    let bins = reduce_histogram_with_unknown_extent(Some(&ctx), input, 2).await;
    assert_eq!(bins.len(), 2);
    assert_eq!(bins.iter().sum::<u32>(), 10);
}

#[tokio::test]
async fn test_gpu_histogram_unknown_extent_single_value() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    let input = Arc::new(vec![3.0, 3.0, 3.0]);
    let bins = reduce_histogram_with_unknown_extent(Some(&ctx), input, 4).await;
    assert_eq!(bins, vec![3, 0, 0, 0]);
}

#[tokio::test]
async fn test_gpu_histogram_unknown_extent_preserves_total() {
    let (device, queue) = gpu_ctx().await;
    let ctx = GpuContext { device: &device, queue: &queue };
    let input = Arc::new(vec![1.0, 2.0, 3.0, 4.0, 5.0]);
    let bins = reduce_histogram_with_unknown_extent(Some(&ctx), input, 5).await;
    assert_eq!(bins.iter().sum::<u32>(), 5);
}
