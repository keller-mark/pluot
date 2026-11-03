#![cfg(all(test, not(target_arch = "wasm32"), not(feature = "lacks_gpu")))]

use pluot_core::numeric_data::NumericData;
use pluot_core::render_traits::{BooleanCriteriaParams, EmphasisCriteria};
use pluot_core::render_types::GpuContext;
use pluot_core::layers::hilbert_heatmap_layer::threshold_opacity;
use pluot_zarr::hilbert_heatmap_data::var_opacity_thresholds;
use pluot_zarr::layers::adata_zarr_hilbert_heatmap_layer::PercentileOpacity;

fn included_unless(excluded: impl Fn(usize) -> bool, len: usize) -> Vec<EmphasisCriteria> {
    vec![EmphasisCriteria::Boolean(BooleanCriteriaParams { mask: NumericData::from((0..len).map(|i| !excluded(i) as u8).collect::<Vec<u8>>()) })]
}

async fn with_cpu_and_gpu<F: AsyncFn(Option<&GpuContext<'_>>)>(check: F) {
    check(None).await;
    let (device, queue) = pluot_core::cache::get_or_init_gpu_context().await.expect("GPU context");
    check(Some(&GpuContext { device: &device, queue: &queue })).await;
}

async fn check_tied_values_share_the_percentile_of_the_values_below_them(gpu_context: Option<&GpuContext<'_>>) {
    let values = vec![0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 100.0];
    let filtering = included_unless(|obs| obs == 9, values.len());
    let thresholds = [PercentileOpacity { percentile: 40.0, opacity: 0.4 }, PercentileOpacity { percentile: 10.0, opacity: 0.1 }];
    let thresholds = var_opacity_thresholds(gpu_context, &NumericData::from(values.clone()), &filtering, &thresholds).await;
    let opacities: Vec<f32> = values.iter().map(|&value| threshold_opacity(value, &thresholds)).collect();
    // The zeros have no included values below them, and the 1 has 4 of 9 (44%).
    assert_eq!(opacities, vec![0.0, 0.0, 0.0, 0.0, 0.4, 0.4, 0.4, 0.4, 0.4, 0.4]);
}

#[tokio::test]
async fn tied_values_share_the_percentile_of_the_values_below_them() {
    with_cpu_and_gpu(check_tied_values_share_the_percentile_of_the_values_below_them).await;
}

async fn check_thresholds_approximate_the_exact_percentiles(gpu_context: Option<&GpuContext<'_>>) {
    const LEN: usize = 100_000;
    let values: Vec<f32> = (0..LEN).map(|i| ((i * 7919) % LEN) as f32).collect();
    let filtering = included_unless(|obs| values[obs] >= 50_000.0, LEN);
    let thresholds = [0.0, 25.0, 50.0, 90.0].map(|percentile| PercentileOpacity { percentile, opacity: percentile / 100.0 });
    let thresholds = var_opacity_thresholds(gpu_context, &NumericData::from(values), &filtering, &thresholds).await;
    assert_eq!(thresholds[0].value, f32::MIN);
    for (threshold, exact) in thresholds[1..].iter().zip([12_500.0, 25_000.0, 45_000.0]) {
        assert!((threshold.value - exact).abs() <= 1.0, "{} vs {exact}", threshold.value);
    }
}

#[tokio::test]
async fn thresholds_approximate_the_exact_percentiles() {
    with_cpu_and_gpu(check_thresholds_approximate_the_exact_percentiles).await;
}
