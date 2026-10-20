//! Reads dense, CSR and CSC encodings of the same matrix from an in-memory store.
#![cfg(not(target_arch = "wasm32"))]

use std::sync::Arc;

use serde_json::json;
use zarrs::array::{data_type, ArrayBuilder};
use zarrs::group::GroupBuilder;
use zarrs::storage::storage_adapter::sync_to_async::{SyncToAsyncSpawnBlocking, SyncToAsyncStorageAdapter};
use zarrs::storage::store::MemoryStore;
use zarrs::storage::AsyncReadableStorageTraits;

use pluot_core::numeric_data::NumericData;
use pluot_core::sparse_matrix::{CompressedAxis, CompressedMatrix};
use pluot_zarr::adata_io::{
    read_compressed_column, read_compressed_subset, read_compressed_subset_with_budget, read_dense_subset_numeric, read_matrix_layout,
    MatrixLayout,
};

struct TokioSpawnBlocking;

impl SyncToAsyncSpawnBlocking for TokioSpawnBlocking {
    async fn spawn_blocking<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        tokio::task::spawn_blocking(f).await.unwrap()
    }
}

const DENSE: [[i32; 4]; 3] = [[0, 5, 0, 7], [0, 0, 0, 0], [3, 0, 8, 0]];

fn write_array(store: &Arc<MemoryStore>, path: &str, shape: Vec<u64>, chunk_shape: Vec<u64>, values: &[i32], attributes: serde_json::Value) {
    let serde_json::Value::Object(attributes) = attributes else { unreachable!() };
    let array = ArrayBuilder::new(shape, chunk_shape, data_type::int32(), 0i32)
        .attributes(attributes)
        .build(store.clone(), path)
        .unwrap();
    array.store_metadata().unwrap();
    array.store_array_subset(&array.subset_all(), values).unwrap();
}

fn write_sparse(store: &Arc<MemoryStore>, path: &str, encoding: &str, indptr: &[i32], indices: &[i32], data: &[i32]) {
    let serde_json::Value::Object(attributes) = json!({"encoding-type": encoding, "encoding-version": "0.1.0", "shape": [3, 4]}) else {
        unreachable!()
    };
    GroupBuilder::new().attributes(attributes).build(store.clone(), path).unwrap().store_metadata().unwrap();
    for (name, values) in [("indptr", indptr), ("indices", indices), ("data", data)] {
        write_array(store, &format!("{path}/{name}"), vec![values.len() as u64], vec![2], values, json!({}));
    }
}

fn fixture() -> Arc<dyn AsyncReadableStorageTraits> {
    let store = Arc::new(MemoryStore::new());
    let dense: Vec<i32> = DENSE.iter().flatten().copied().collect();
    write_array(&store, "/dense", vec![3, 4], vec![2, 3], &dense, json!({"encoding-type": "array", "encoding-version": "0.2.0"}));
    write_sparse(&store, "/csr", "csr_matrix", &[0, 2, 2, 4], &[1, 3, 0, 2], &[5, 7, 3, 8]);
    write_sparse(&store, "/csc", "csc_matrix", &[0, 1, 2, 3, 4], &[2, 0, 2, 0], &[3, 5, 8, 7]);
    Arc::new(SyncToAsyncStorageAdapter::new(store, TokioSpawnBlocking))
}

/// The row-major `rows x cols` values of `matrix`, read back through its lookup.
fn densify(matrix: &CompressedMatrix, axis: CompressedAxis, rows: &[u32], cols: &[u32]) -> NumericData {
    matrix.validate(axis, rows.len(), cols.len());
    let values = (0..rows.len()).flat_map(|row| (0..cols.len()).map(move |col| (row, col)));
    NumericData::from(values.map(|(row, col)| matrix.value_at(axis, row, col) as i32).collect::<Vec<_>>())
}

fn expected(rows: &[u32], cols: &[u32]) -> Vec<i32> {
    rows.iter().flat_map(|&r| cols.iter().map(move |&c| DENSE[r as usize][c as usize])).collect()
}

const CASES: [(&[u32], &[u32]); 4] = [
    (&[0, 1, 2], &[0, 1, 2, 3]),
    (&[2, 0], &[3, 1, 2]),
    (&[1], &[0, 3]),
    (&[], &[1]),
];

#[tokio::test]
async fn compressed_columns_hold_only_the_stored_entries() {
    let store = fixture();
    for (path, axis) in [("/csr", CompressedAxis::Rows), ("/csc", CompressedAxis::Cols)] {
        for col in 0..4u32 {
            let column = read_compressed_column(store.clone(), path, axis, col as u64).await.unwrap();
            let stored = DENSE.iter().filter(|row| row[col as usize] != 0).count();
            assert_eq!(column.data.len(), stored, "{path} col {col}");
            assert_eq!(densify(&column, CompressedAxis::Cols, &[0, 1, 2], &[0]).as_f32().to_vec(), expected(&[0, 1, 2], &[col]).iter().map(|&v| v as f32).collect::<Vec<_>>());
        }
    }
}

#[tokio::test]
async fn subsets_match_the_dense_matrix_for_every_encoding() {
    let store = fixture();
    for path in ["/dense", "/csr", "/csc"] {
        for (rows, cols) in CASES {
            let values = match read_matrix_layout(store.clone(), path).await {
                MatrixLayout::Dense => read_dense_subset_numeric(store.clone(), path, rows, cols).await.unwrap(),
                MatrixLayout::Compressed(axis) => densify(&read_compressed_subset(store.clone(), path, axis, rows, cols).await.unwrap(), axis, rows, cols),
            };
            let NumericData::Int32(values) = values else {
                panic!("expected the stored int32 dtype");
            };
            assert_eq!(*values, expected(rows, cols), "{path} rows {rows:?} cols {cols:?}");
        }
    }
}

#[tokio::test]
async fn compressed_subsets_match_at_every_budget() {
    let store = fixture();
    for (path, axis) in [("/csr", CompressedAxis::Rows), ("/csc", CompressedAxis::Cols)] {
        for budget in 1..=5 {
            for (rows, cols) in CASES {
                let matrix = read_compressed_subset_with_budget(store.clone(), path, axis, rows, cols, budget).await.unwrap();
                let NumericData::Int32(values) = densify(&matrix, axis, rows, cols) else {
                    panic!("expected the stored int32 dtype");
                };
                assert_eq!(*values, expected(rows, cols), "{path} budget {budget} rows {rows:?} cols {cols:?}");
            }
        }
    }
}
