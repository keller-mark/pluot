use std::sync::Arc;

use pluot_core::compute::included_indices::{compute_included_indices, compute_included_indices_with};
use pluot_core::compute::stratified_indices::CompactionMethod;
use pluot_core::numeric_data::NumericData;
use pluot_core::render_traits::{CategoricalCriteriaParams, EmphasisCriteria, QuantitativeCriteriaParams};

fn criteria() -> (Vec<EmphasisCriteria>, Vec<EmphasisCriteria>) {
    let filtering = vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
        codes: NumericData::Int32(Arc::new(vec![0, 1, 1, 2, 1, 0])),
        included_codes: vec![1, 2],
    })];
    let selection = vec![EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
        values: NumericData::Float32(Arc::new(vec![0.0, 5.0, 1.0, 9.0, 7.0, 8.0])),
        min: Some(4.0),
        max: None,
        min_exclusive: None,
        max_exclusive: None,
    })];
    (filtering, selection)
}

#[tokio::test]
async fn cpu_indices_are_filtered_then_selected() {
    let (filtering, selection) = criteria();
    let result = compute_included_indices(None, 6, &filtering, &selection).await;
    assert_eq!(result.background, vec![1, 2, 3, 4]);
    assert_eq!(result.foreground, vec![1, 3, 4]);
}

#[tokio::test]
async fn no_criteria_includes_everything() {
    let result = compute_included_indices(None, 3, &[], &[]).await;
    assert_eq!(result.background, vec![0, 1, 2]);
    assert_eq!(result.foreground, vec![0, 1, 2]);
}

#[cfg(not(feature = "lacks_gpu"))]
#[tokio::test]
async fn gpu_indices_match_cpu() {
    use pluot_core::render_types::GpuContext;

    let (device, queue) = pluot_core::cache::get_or_init_gpu_context().await.expect("GPU context");
    let gpu_context = GpuContext { device: &device, queue: &queue };
    let (filtering, selection) = criteria();
    let cpu = compute_included_indices(None, 6, &filtering, &selection).await;
    for method in [CompactionMethod::GpuScan, CompactionMethod::ReadBackKeys] {
        let gpu = compute_included_indices_with(Some(&gpu_context), 6, &filtering, &selection, method).await;
        assert_eq!(gpu, cpu, "{method:?}");
    }
}
