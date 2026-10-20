use std::sync::Arc;

use pluot_core::numeric_data::NumericData;
use pluot_core::sparse_matrix::{CompressedAxis, CompressedMatrix};
use pluot_zarr::heatmap_data::{
    axis_criteria_key, gather_compressed_columns, gather_rows_from_columns, resolve_identifier_indices, selection_mask, AxisCriteria,
};
use pluot_zarr::zarr_emphasis_criteria::{ZarrEmphasisCriteria, ZarrQuantitativeCriteriaParams};

#[test]
fn identifiers_resolve_in_order_skipping_unknown_and_repeats() {
    let labels: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
    let requested: Vec<String> = ["c", "x", "a", "c"].iter().map(|s| s.to_string()).collect();
    assert_eq!(resolve_identifier_indices(&requested, &labels), vec![2, 0]);
}

#[test]
fn gathers_rows_from_columns_in_row_major_order() {
    let columns = vec![Arc::new(NumericData::from(vec![1u16, 2, 3])), Arc::new(NumericData::from(vec![4u16, 5, 6]))];
    let NumericData::Uint16(values) = gather_rows_from_columns(&columns, &[2, 0]) else {
        panic!("expected the columns' dtype");
    };
    assert_eq!(*values, vec![3, 6, 1, 4]);
}

#[test]
fn gathers_compressed_columns_keeping_only_requested_rows() {
    let column = |rows: Vec<i64>, values: Vec<u16>| {
        Arc::new(CompressedMatrix {
            indptr: NumericData::from(vec![0u32, rows.len() as u32]),
            indices: NumericData::from(rows),
            data: NumericData::from(values),
            minor_subset: None,
        })
    };
    let columns = vec![column(vec![0, 2, 3], vec![1, 2, 3]), column(vec![1], vec![4])];
    let matrix = gather_compressed_columns(&columns, &[3, 0], 4);
    matrix.validate(CompressedAxis::Cols, 2, 2);
    let NumericData::Uint16(data) = &matrix.data else {
        panic!("expected the columns' dtype");
    };
    assert_eq!(**data, vec![1, 3]);
    assert_eq!(matrix.value_at(CompressedAxis::Cols, 0, 0), 3.0);
    assert_eq!(matrix.value_at(CompressedAxis::Cols, 1, 0), 1.0);
    assert_eq!(matrix.value_at(CompressedAxis::Cols, 0, 1), 0.0);
}

#[test]
fn selection_mask_marks_selected_included_indices() {
    assert_eq!(selection_mask(&[5, 1, 3], &[3, 5, 9]), vec![1, 0, 1]);
}

#[test]
fn criteria_keys_distinguish_thresholds_and_are_length_prefixed() {
    let quantitative = |max| {
        AxisCriteria::Criteria(vec![ZarrEmphasisCriteria::Quantitative(ZarrQuantitativeCriteriaParams {
            values_key: "/obs/n_genes".to_string(),
            min: None,
            max: Some(max),
            min_exclusive: None,
            max_exclusive: None,
        })])
    };
    let narrow = axis_criteria_key(Some(&quantitative(10.0)));
    assert_ne!(narrow, axis_criteria_key(Some(&quantitative(20.0))));
    assert_eq!(narrow[0], (narrow.len() - 1).to_string());
    assert_eq!(axis_criteria_key(None), vec!["1".to_string(), "all".to_string()]);
}
