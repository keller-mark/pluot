use std::sync::Arc;

use pluot_core::compute::reduce_stratified::StratifyBy;
use pluot_core::compute::stratified_indices::{compute_row_mask, compute_stratified_indices, CompactionMethod, RowSource, StratifiedIndices};
use pluot_core::numeric_data::NumericData;
use pluot_core::render_traits::{EmphasisCriteria, QuantitativeCriteriaParams};

fn codes(len: usize) -> NumericData {
    NumericData::Int8(Arc::new((0..len).map(|i| ((i * 7919) % 5) as i8 - 1).collect()))
}

fn values_at_least(len: usize, min: f32) -> Vec<EmphasisCriteria> {
    vec![EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
        values: NumericData::Float32(Arc::new((0..len).map(|i| ((i * 104729) % 1000) as f32).collect())),
        min: Some(min),
        max: None,
        min_exclusive: None,
        max_exclusive: None,
    })]
}

#[tokio::test]
async fn cpu_strata_are_stable_runs_in_stratum_order() {
    let stratify_by = StratifyBy { codes: NumericData::Int32(Arc::new(vec![1, -1, 0, 1, 2, 0, 1])), included_codes: vec![1, 0, -1] };
    let rows = [6, 0, 2, 5, 3, 1];
    let result = compute_stratified_indices(None, RowSource::Rows(&rows), Some(&stratify_by), &[], CompactionMethod::GpuScan).await;
    assert_eq!(result, StratifiedIndices { indices: vec![6, 0, 3, 2, 5, 1], offsets: vec![0, 3, 5, 6] });
    assert_eq!(result.stratum(1), &[2, 5]);
}

#[cfg(not(feature = "lacks_gpu"))]
mod gpu {
    use super::*;
    use pluot_core::render_types::GpuContext;

    async fn assert_methods_match_cpu(rows: RowSource<'_>, stratify_by: Option<&StratifyBy>, criteria: &[EmphasisCriteria]) {
        let (device, queue) = pluot_core::cache::get_or_init_gpu_context().await.expect("GPU context");
        let gpu_context = GpuContext { device: &device, queue: &queue };
        let cpu = compute_stratified_indices(None, rows, stratify_by, criteria, CompactionMethod::GpuScan).await;
        for method in [CompactionMethod::GpuScan, CompactionMethod::ReadBackKeys] {
            let gpu = compute_stratified_indices(Some(&gpu_context), rows, stratify_by, criteria, method).await;
            assert_eq!(gpu, cpu, "{method:?}");
        }
    }

    #[tokio::test]
    async fn gpu_strata_match_cpu_across_scan_levels() {
        for len in [1, 255, 256, 257, 70_000, 300_000] {
            let stratify_by = StratifyBy { codes: codes(len), included_codes: vec![3, -1, 0, 1] };
            assert_methods_match_cpu(RowSource::All(len), Some(&stratify_by), &values_at_least(len, 250.0)).await;
        }
    }

    #[tokio::test]
    async fn gpu_compaction_without_strata_matches_cpu() {
        let len = 100_000;
        assert_methods_match_cpu(RowSource::All(len), None, &values_at_least(len, 500.0)).await;
        assert_methods_match_cpu(RowSource::All(len), None, &[]).await;
    }

    #[tokio::test]
    async fn gpu_listed_rows_keep_their_order_within_strata() {
        let len = 5_000;
        let rows: Vec<u32> = (0..len as u32).rev().step_by(3).collect();
        let stratify_by = StratifyBy { codes: codes(len), included_codes: vec![0, 1, 2, 3, -1] };
        assert_methods_match_cpu(RowSource::Rows(&rows), Some(&stratify_by), &values_at_least(len, 100.0)).await;
    }

    #[tokio::test]
    async fn gpu_row_mask_matches_cpu() {
        let (device, queue) = pluot_core::cache::get_or_init_gpu_context().await.expect("GPU context");
        let gpu_context = GpuContext { device: &device, queue: &queue };
        let len = 10_001;
        let rows: Vec<u32> = (0..len as u32).rev().collect();
        let criteria = values_at_least(len, 300.0);
        for rows in [RowSource::All(len), RowSource::Rows(&rows)] {
            let gpu = compute_row_mask(Some(&gpu_context), rows, &criteria).await;
            let cpu = compute_row_mask(None, rows, &criteria).await;
            assert_eq!(gpu, cpu);
        }
    }
}

/// Compares the compaction methods' wall-clock times; run with `--ignored --nocapture`.
#[cfg(not(feature = "lacks_gpu"))]
#[ignore]
#[tokio::test]
async fn benchmark_compaction_methods() {
    use pluot_core::render_types::GpuContext;
    use std::time::Instant;

    let (device, queue) = pluot_core::cache::get_or_init_gpu_context().await.expect("GPU context");
    let gpu_context = GpuContext { device: &device, queue: &queue };
    const REPEATS: u32 = 5;
    for len in [100_000, 1_000_000, 10_000_000] {
        let criteria = values_at_least(len, 250.0);
        for num_strata in [1, 10, 100] {
            let stratify_by = StratifyBy {
                codes: NumericData::Int32(Arc::new((0..len).map(|i| ((i * 7919) % num_strata) as i32).collect())),
                included_codes: (0..num_strata as i64).collect(),
            };
            let stratify_by = (num_strata > 1).then_some(&stratify_by);
            let mut timings = Vec::new();
            for method in [CompactionMethod::GpuScan, CompactionMethod::ReadBackKeys] {
                compute_stratified_indices(Some(&gpu_context), RowSource::All(len), stratify_by, &criteria, method).await;
                let start = Instant::now();
                for _ in 0..REPEATS {
                    compute_stratified_indices(Some(&gpu_context), RowSource::All(len), stratify_by, &criteria, method).await;
                }
                timings.push(format!("{method:?} {:>8.2?}", start.elapsed() / REPEATS));
            }
            println!("BENCHMARK len={len:>8} strata={num_strata:>3}: {}", timings.join("  "));
        }
    }
}
