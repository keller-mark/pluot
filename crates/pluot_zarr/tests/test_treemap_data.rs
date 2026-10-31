use std::sync::Arc;

use pluot_core::numeric_data::NumericData;
use pluot_core::render_traits::{CategoricalCriteriaParams, EmphasisCriteria, QuantitativeCriteriaParams};
use pluot_zarr::treemap_data::{count_obs_groups, ObsGroupCount};

fn included_codes(codes: &NumericData, included_codes: &[i64]) -> EmphasisCriteria {
    EmphasisCriteria::Categorical(CategoricalCriteriaParams { codes: codes.clone(), included_codes: included_codes.to_vec() })
}

fn at_least(values: &NumericData, min: f32) -> EmphasisCriteria {
    EmphasisCriteria::Quantitative(QuantitativeCriteriaParams { values: values.clone(), min: Some(min), max: None, min_exclusive: None, max_exclusive: None })
}

#[test]
fn counts_each_combination_of_codes_skipping_missing_values() {
    let cell_type = NumericData::Int8(Arc::new(vec![0, 0, 1, 1, 1, -1]));
    let sample = NumericData::Int16(Arc::new(vec![0, 1, 1, 1, -1, 0]));
    let groups = count_obs_groups(&[&cell_type, &sample], &[], &[]);
    assert_eq!(groups, vec![
        ObsGroupCount { codes: vec![0, 0], count: 1, selected_count: 1 },
        ObsGroupCount { codes: vec![0, 1], count: 1, selected_count: 1 },
        ObsGroupCount { codes: vec![1, 1], count: 2, selected_count: 2 },
    ]);
}

#[test]
fn counts_a_single_column() {
    let cell_type = NumericData::Int8(Arc::new(vec![2, 0, 2]));
    let groups = count_obs_groups(&[&cell_type], &[], &[]);
    assert_eq!(groups, vec![
        ObsGroupCount { codes: vec![0], count: 1, selected_count: 1 },
        ObsGroupCount { codes: vec![2], count: 2, selected_count: 2 },
    ]);
}

#[test]
fn counts_the_filtered_rows_and_the_selected_ones_among_them() {
    let cell_type = NumericData::Int8(Arc::new(vec![0, 0, 1, 1, 2, 2]));
    let batch = NumericData::Uint8(Arc::new(vec![0, 1, 0, 1, 0, 1]));
    let expression = NumericData::Float32(Arc::new(vec![5.0, 0.0, 0.0, 0.0, 9.0, 3.0]));
    let groups = count_obs_groups(&[&cell_type], &[included_codes(&batch, &[0, 1]), included_codes(&cell_type, &[0, 1])], &[at_least(&expression, 1.0)]);
    assert_eq!(groups, vec![
        ObsGroupCount { codes: vec![0], count: 2, selected_count: 1 },
        ObsGroupCount { codes: vec![1], count: 2, selected_count: 0 },
    ]);
}

#[test]
fn no_columns_yields_no_groups() {
    assert!(count_obs_groups(&[], &[], &[]).is_empty());
}

#[cfg(not(feature = "lacks_gpu"))]
#[tokio::test]
async fn gpu_counts_match_cpu_counts() {
    use pluot_core::render_types::GpuContext;
    use pluot_zarr::treemap_data::count_obs_groups_on_gpu;

    let (device, queue) = pluot_core::cache::get_or_init_gpu_context().await.expect("GPU context");
    let gpu_context = GpuContext { device: &device, queue: &queue };
    let len = 100_000;
    let cell_type = NumericData::Int8(Arc::new((0..len).map(|i| ((i * 7) % 12) as i8 - 1).collect()));
    let sample = NumericData::Uint16(Arc::new((0..len).map(|i| ((i * 13) % 5) as u16).collect()));
    let expression = NumericData::Float32(Arc::new((0..len).map(|i| ((i * 31) % 17) as f32).collect()));
    let columns = [&cell_type, &sample];
    assert_eq!(count_obs_groups_on_gpu(&gpu_context, &columns, &[11, 5], &[], &[]).await, count_obs_groups(&columns, &[], &[]));
    let filtering = [included_codes(&sample, &[0, 2, 3])];
    let selection = [at_least(&expression, 8.0)];
    assert_eq!(
        count_obs_groups_on_gpu(&gpu_context, &columns, &[11, 5], &filtering, &selection).await,
        count_obs_groups(&columns, &filtering, &selection),
    );
    assert!(count_obs_groups_on_gpu(&gpu_context, &[], &[], &[], &[]).await.is_empty());
}
