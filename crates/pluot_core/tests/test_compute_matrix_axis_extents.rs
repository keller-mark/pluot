use std::sync::Arc;

use pluot_core::compute::matrix_axis_extents::{reduce_compressed_matrix_axis_extents, reduce_matrix_axis_extents, MatrixAxis};
use pluot_core::numeric_data::NumericData;
use pluot_core::sparse_matrix::{CompressedAxis, CompressedMatrix};

fn matrix() -> NumericData {
    NumericData::Float32(Arc::new(vec![
        3.0, -1.0, f32::NAN,
        0.5, 7.0, f32::NAN,
    ]))
}

#[tokio::test]
async fn cpu_extents_per_row_and_column_skip_nan() {
    let (mins, maxs) = reduce_matrix_axis_extents(None, &matrix(), 2, 3, MatrixAxis::Rows).await;
    assert_eq!((mins, maxs), (vec![-1.0, 0.5], vec![3.0, 7.0]));

    let (mins, maxs) = reduce_matrix_axis_extents(None, &matrix(), 2, 3, MatrixAxis::Cols).await;
    assert_eq!((mins, maxs), (vec![0.5, -1.0, f32::INFINITY], vec![3.0, 7.0, f32::NEG_INFINITY]));
}

#[tokio::test]
async fn cpu_extents_of_integer_matrix() {
    let matrix = NumericData::Uint16(Arc::new(vec![4, 9, 2, 1]));
    let (mins, maxs) = reduce_matrix_axis_extents(None, &matrix, 2, 2, MatrixAxis::Cols).await;
    assert_eq!((mins, maxs), (vec![2.0, 1.0], vec![4.0, 9.0]));
}

/// [[0, 5, NaN], [-2, 0, 0], [0, 0, 0]] as CSR, with every stored value nonzero but NaN.
fn compressed() -> CompressedMatrix {
    CompressedMatrix {
        indptr: NumericData::from(vec![0u32, 2, 3, 3]),
        indices: NumericData::from(vec![1i32, 2, 0]),
        data: NumericData::from(vec![5.0f32, f32::NAN, -2.0]),
        minor_subset: None,
    }
}

#[tokio::test]
async fn cpu_compressed_extents_count_implicit_zeros() {
    let rows = reduce_compressed_matrix_axis_extents(None, &compressed(), CompressedAxis::Rows, 3, 3, MatrixAxis::Rows).await;
    assert_eq!(rows, (vec![0.0, -2.0, 0.0], vec![5.0, 0.0, 0.0]));
    let cols = reduce_compressed_matrix_axis_extents(None, &compressed(), CompressedAxis::Rows, 3, 3, MatrixAxis::Cols).await;
    assert_eq!(cols, (vec![-2.0, 0.0, 0.0], vec![0.0, 5.0, 0.0]));
}

#[tokio::test]
async fn cpu_compressed_extents_follow_the_minor_subset() {
    let matrix = CompressedMatrix { minor_subset: Some(NumericData::from(vec![1u32, 0])), ..compressed() };
    let cols = reduce_compressed_matrix_axis_extents(None, &matrix, CompressedAxis::Rows, 3, 2, MatrixAxis::Cols).await;
    assert_eq!(cols, (vec![0.0, -2.0], vec![5.0, 0.0]));
}

#[cfg(not(feature = "lacks_gpu"))]
#[tokio::test]
async fn gpu_compressed_extents_match_cpu() {
    use pluot_core::render_types::GpuContext;

    let (device, queue) = pluot_core::cache::get_or_init_gpu_context().await.expect("GPU context");
    let gpu_context = GpuContext { device: &device, queue: &queue };
    let subset = CompressedMatrix { minor_subset: Some(NumericData::from(vec![2u32, 0])), ..compressed() };
    for (matrix, num_cols) in [(compressed(), 3), (subset, 2)] {
        for compressed_axis in [CompressedAxis::Rows, CompressedAxis::Cols] {
            let (num_rows, num_cols) = if compressed_axis == CompressedAxis::Rows { (3, num_cols) } else { (num_cols, 3) };
            for axis in [MatrixAxis::Rows, MatrixAxis::Cols] {
                let gpu = reduce_compressed_matrix_axis_extents(Some(&gpu_context), &matrix, compressed_axis, num_rows, num_cols, axis).await;
                let cpu = reduce_compressed_matrix_axis_extents(None, &matrix, compressed_axis, num_rows, num_cols, axis).await;
                assert_eq!(gpu, cpu, "{compressed_axis:?} {axis:?}");
            }
        }
    }
}

#[cfg(not(feature = "lacks_gpu"))]
#[tokio::test]
async fn gpu_extents_match_cpu() {
    use pluot_core::render_types::GpuContext;

    let (device, queue) = pluot_core::cache::get_or_init_gpu_context().await.expect("GPU context");
    let gpu_context = GpuContext { device: &device, queue: &queue };
    for axis in [MatrixAxis::Rows, MatrixAxis::Cols] {
        let gpu = reduce_matrix_axis_extents(Some(&gpu_context), &matrix(), 2, 3, axis).await;
        let cpu = reduce_matrix_axis_extents(None, &matrix(), 2, 3, axis).await;
        assert_eq!(gpu, cpu, "{axis:?}");
    }
}
