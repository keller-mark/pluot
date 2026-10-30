use std::sync::Arc;

use pluot_core::numeric_data::NumericData;
use pluot_zarr::treemap_data::{count_obs_groups, ObsGroupCount};

#[test]
fn counts_each_combination_of_codes_skipping_missing_values() {
    let cell_type = NumericData::Int8(Arc::new(vec![0, 0, 1, 1, 1, -1]));
    let sample = NumericData::Int16(Arc::new(vec![0, 1, 1, 1, -1, 0]));
    let groups = count_obs_groups(&[&cell_type, &sample]);
    assert_eq!(groups, vec![
        ObsGroupCount { codes: vec![0, 0], count: 1 },
        ObsGroupCount { codes: vec![0, 1], count: 1 },
        ObsGroupCount { codes: vec![1, 1], count: 2 },
    ]);
}

#[test]
fn counts_a_single_column() {
    let cell_type = NumericData::Int8(Arc::new(vec![2, 0, 2]));
    let groups = count_obs_groups(&[&cell_type]);
    assert_eq!(groups, vec![
        ObsGroupCount { codes: vec![0], count: 1 },
        ObsGroupCount { codes: vec![2], count: 2 },
    ]);
}

#[test]
fn no_columns_yields_no_groups() {
    assert!(count_obs_groups(&[]).is_empty());
}
