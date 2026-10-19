use pluot_core::numeric_data::NumericData;
use pluot_core::sparse_matrix::{CompressedAxis, CompressedMatrix};

/// [[0, 5, 0, 7], [0, 0, 0, 0], [3, 0, 8, 0]] as CSR.
fn csr() -> CompressedMatrix {
    CompressedMatrix {
        indptr: NumericData::from(vec![0u32, 2, 2, 4]),
        indices: NumericData::from(vec![1i32, 3, 0, 2]),
        data: NumericData::from(vec![5i32, 7, 3, 8]),
        minor_subset: None,
    }
}

#[test]
fn values_include_implicit_zeros() {
    let matrix = csr();
    matrix.validate(CompressedAxis::Rows, 3, 4);
    let dense: Vec<f32> = (0..3).flat_map(|row| (0..4).map(move |col| (row, col))).map(|(row, col)| matrix.value_at(CompressedAxis::Rows, row, col)).collect();
    assert_eq!(dense, vec![0.0, 5.0, 0.0, 7.0, 0.0, 0.0, 0.0, 0.0, 3.0, 0.0, 8.0, 0.0]);
}

#[test]
fn minor_subset_reorders_the_minor_axis() {
    let matrix = CompressedMatrix { minor_subset: Some(NumericData::from(vec![3u32, 1])), ..csr() };
    matrix.validate(CompressedAxis::Rows, 3, 2);
    assert_eq!(matrix.value_at(CompressedAxis::Rows, 0, 0), 7.0);
    assert_eq!(matrix.value_at(CompressedAxis::Rows, 0, 1), 5.0);
    assert_eq!(matrix.format_value_at(CompressedAxis::Rows, 2, 0), "0");
}

#[test]
fn csc_reads_columns_as_the_major_axis() {
    let matrix = csr();
    assert_eq!(matrix.value_at(CompressedAxis::Cols, 3, 0), 7.0);
}
