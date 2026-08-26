// Async I/O helpers for reading AnnData-Zarr elements, built on the
// `AnnDataEncoding` types in `adata_metadata.rs`.

use std::sync::Arc;

use pluot_core::numeric_data::NumericData;
use zarrs::storage::AsyncReadableStorageTraits;

use crate::adata_metadata::AnnDataEncoding;
use crate::zarr_numeric_data::load_arr_as_numeric_data;

/// Upper bound on how much of a sparse matrix's `indptr`, `indices` or `data` array is read — and
/// so held in memory — at one time.
///
/// A CSR matrix's non-zeros for a single column are spread across every row, so extracting one
/// column has to traverse the whole matrix. Doing that in bounded pieces keeps peak memory
/// proportional to the `n_obs`-length column being produced rather than to the size of the matrix,
/// which for a large `X` is the difference between tens of megabytes and several gigabytes.
const MAX_BYTES_PER_READ: u64 = 36 << 20;

/// [`MAX_BYTES_PER_READ`] as a number of elements, at the 8-byte width of the widest dtype we
/// accept. Narrower dtypes therefore read below the byte budget rather than above it.
const MAX_ELEMENTS_PER_READ: u64 = MAX_BYTES_PER_READ / 8;


/// Reads the `attributes` of the zarr group or array at `path` and
/// deserializes them into an [`AnnDataEncoding`], regardless of whether the
/// element is a group (e.g. `dataframe`, `categorical`, `dict`) or an array
/// (e.g. `array`, `string-array`).
pub async fn read_encoding(store: Arc<dyn AsyncReadableStorageTraits>, path: &str) -> AnnDataEncoding {
    let attributes = if let Ok(group) = zarrs::group::Group::async_open(store.clone(), path).await {
        group.attributes().clone()
    } else {
        zarrs::array::Array::async_open(store, path)
            .await
            .unwrap_or_else(|e| panic!("Failed to open AnnData element at \"{path}\": {e}"))
            .attributes()
            .clone()
    };
    serde_json::from_value(serde_json::Value::Object(attributes))
        .unwrap_or_else(|e| panic!("Invalid AnnData encoding attributes at \"{path}\": {e}"))
}

/// Reads a whole `string-array`-encoded zarr array.
pub async fn read_string_array(store: Arc<dyn AsyncReadableStorageTraits>, path: &str) -> Result<Vec<String>, zarrs::array::ArrayError> {
    let array = zarrs::array::Array::async_open(store, path).await.unwrap();
    let subset = array.subset_all();
    array.async_retrieve_array_subset::<Vec<String>>(&subset).await
}

/// Reads the row labels of an AnnData dataframe (e.g. `obs` or `var`), i.e.
/// the column named by its `_index` attribute. Handles both `string-array`-
/// and `nullable-string-array`-encoded index columns; the `mask` sibling of
/// the latter is ignored, since a dataframe index is not expected to have
/// missing values.
/// TODO: also support integer and range-type indices
pub async fn read_dataframe_index(store: Arc<dyn AsyncReadableStorageTraits>, dataframe_path: &str) -> Result<Vec<String>, zarrs::array::ArrayError> {
    let index_column = match read_encoding(store.clone(), dataframe_path).await {
        AnnDataEncoding::DataFrame { index, .. } => index,
        other => panic!("Expected a dataframe encoding at \"{dataframe_path}\", got {other:?}"),
    };
    let index_path = format!("{dataframe_path}/{index_column}");
    let values_path = match read_encoding(store.clone(), &index_path).await {
        AnnDataEncoding::NullableStringArray { .. } => format!("{index_path}/values"),
        AnnDataEncoding::StringArray { .. } => index_path,
        other => panic!("Unsupported dataframe index encoding at \"{index_path}\": {other:?}"),
    };
    read_string_array(store, &values_path).await
}

/// Reads an AnnData `categorical` column (e.g. an `obs` groupby column) as
/// its `categories` labels and per-observation `codes` (the category index of
/// each row; `-1` denotes a missing value, per the AnnData spec).
/// TODO: also support non-string categories
pub async fn read_categorical_column(store: Arc<dyn AsyncReadableStorageTraits>, column_path: &str) -> Result<(Vec<String>, NumericData), zarrs::array::ArrayError> {
    let categories = read_string_array(store.clone(), &format!("{column_path}/categories")).await?;
    let codes = load_arr_as_numeric_data(store, &format!("{column_path}/codes")).await?;
    Ok((categories, codes))
}

/// Reads a numeric column of an AnnData dataframe (e.g., a column of `obs` or `var`).
pub async fn read_numeric_column(store: Arc<dyn AsyncReadableStorageTraits>, column_path: &str) -> Result<NumericData, zarrs::array::ArrayError> {
    let values = load_arr_as_numeric_data(store, column_path).await?;
    Ok(values)
}

/// Reads one column (all rows) of a dense 2D numeric zarr array (e.g. AnnData
/// `X` or a `layers` entry) in its native dtype, given the zero-based column
/// index. See [`load_arr_as_numeric_data`] for the whole-array equivalent.
pub async fn read_dense_column_numeric(store: Arc<dyn AsyncReadableStorageTraits>, array_path: &str, col_index: u64) -> Result<NumericData, zarrs::array::ArrayError> {
    let array = zarrs::array::Array::async_open(store, array_path).await.unwrap();
    let n_obs = array.shape()[0];
    let subset = zarrs::array::ArraySubset::new_with_ranges(&[0..n_obs, col_index..col_index + 1]);

    use zarrs::plugin::ZarrVersion;
    let dtype_name = array.data_type().name(ZarrVersion::V3).expect("Array data type must have a V3 name").to_string();

    macro_rules! load {
        ($rust_ty:ty, $variant:ident) => {{
            let data = array.async_retrieve_array_subset::<Vec<$rust_ty>>(&subset).await?;
            NumericData::$variant(Arc::new(data))
        }};
    }

    Ok(match dtype_name.as_str() {
        "uint8" => load!(u8, Uint8),
        "uint16" => load!(u16, Uint16),
        "uint32" => load!(u32, Uint32),
        "uint64" => load!(u64, Uint64),
        "int8" => load!(i8, Int8),
        "int16" => load!(i16, Int16),
        "int32" => load!(i32, Int32),
        "int64" => load!(i64, Int64),
        "float32" => load!(f32, Float32),
        "float64" => load!(f64, Float64),
        other => panic!("Unsupported dtype \"{other}\" for AnnData expression array \"{array_path}\""),
    })
}

/// Reads one column (all rows) of a dense 2D numeric zarr array (e.g. AnnData
/// `X` or a `layers` entry) as `f32`, given the zero-based column index.
pub async fn read_dense_column_f32(store: Arc<dyn AsyncReadableStorageTraits>, array_path: &str, col_index: u64) -> Result<Vec<f32>, zarrs::array::ArrayError> {
    let array = zarrs::array::Array::async_open(store, array_path).await.unwrap();
    let n_obs = array.shape()[0];
    let subset = zarrs::array::ArraySubset::new_with_ranges(&[0..n_obs, col_index..col_index + 1]);

    use zarrs::plugin::ZarrVersion;
    let dtype_name = array.data_type().name(ZarrVersion::V3).expect("Array data type must have a V3 name").to_string();

    Ok(match dtype_name.as_str() {
        "float32" => array.async_retrieve_array_subset::<Vec<f32>>(&subset).await?,
        "float64" => {
            let values = array.async_retrieve_array_subset::<Vec<f64>>(&subset).await?;
            values.iter().map(|&x| x as f32).collect()
        }
        other => panic!("Unsupported dtype \"{other}\" for AnnData expression array \"{array_path}\" (expected float32 or float64)"),
    })
}

/// Reads the logical `[n_obs, n_var]` shape of a CSR- or CSC-encoded sparse matrix from its
/// group's AnnData encoding attributes.
async fn read_sparse_matrix_shape(store: Arc<dyn AsyncReadableStorageTraits>, matrix_path: &str) -> Vec<u64> {
    match read_encoding(store, matrix_path).await {
        AnnDataEncoding::CsrMatrix { shape, .. } | AnnDataEncoding::CscMatrix { shape, .. } => shape,
        other => panic!("Expected a csr_matrix or csc_matrix encoding at \"{matrix_path}\", got {other:?}"),
    }
}

/// A 1D integer zarr array, held in the dtype it was stored in.
///
/// AnnData sparse matrices store `indptr` and `indices` as `int32` or `int64` (and, rarely, as
/// something narrower or unsigned). Normalizing those to a single `u64` representation on read
/// would allocate a widened copy of every element, and for `indices` — which has one entry per
/// non-zero of the whole matrix — that copy is the largest allocation in the read path. Keeping
/// the native dtype instead means bulk access goes through [`with_int_slice`], which
/// monomorphizes its body per dtype so elements are read at their stored width, and the only
/// values ever converted are the handful of scalars that feed zarr's `u64`-typed slicing API
/// (see [`IntArray::offset_at`]).
#[derive(Debug)]
enum IntArray {
    Uint8(Vec<u8>),
    Uint16(Vec<u16>),
    Uint32(Vec<u32>),
    Uint64(Vec<u64>),
    Int8(Vec<i8>),
    Int16(Vec<i16>),
    Int32(Vec<i32>),
    Int64(Vec<i64>),
}

/// Evaluates `$body` with `$slice` bound to an [`IntArray`]'s contents as a `&[T]` of its native
/// Rust integer type.
///
/// The body is expanded (and so monomorphized) once per dtype, which is what lets downstream code
/// stay generic over the stored dtype without either a widening pass over the array or a per-element
/// enum dispatch. Every arm must evaluate to the same type, so `$body` is typically a call to a
/// function generic over the element type.
macro_rules! with_int_slice {
    ($array:expr, |$slice:ident| $body:expr) => {
        match $array {
            IntArray::Uint8(v) => { let $slice: &[u8] = v; $body }
            IntArray::Uint16(v) => { let $slice: &[u16] = v; $body }
            IntArray::Uint32(v) => { let $slice: &[u32] = v; $body }
            IntArray::Uint64(v) => { let $slice: &[u64] = v; $body }
            IntArray::Int8(v) => { let $slice: &[i8] = v; $body }
            IntArray::Int16(v) => { let $slice: &[i16] = v; $body }
            IntArray::Int32(v) => { let $slice: &[i32] = v; $body }
            IntArray::Int64(v) => { let $slice: &[i64] = v; $body }
        }
    };
}

impl IntArray {
    /// The element at `idx` widened to `u64`.
    ///
    /// Reserved for the `O(1)` values that are consumed as *arguments* to zarr's `u64`-typed
    /// slicing API rather than as data — namely the `indptr` entries bounding a column's slice of
    /// `indices`/`data`. Element access over a whole array must go through [`with_int_slice`] so
    /// that it stays in the stored dtype.
    fn offset_at(&self, idx: usize) -> u64 {
        with_int_slice!(self, |values| u64::try_from(values[idx]).ok())
            .expect("Sparse matrix offsets (indptr) must be non-negative")
    }
}

/// Reads `array_path[start..stop]` of an integer-typed 1D zarr array (e.g. a sparse matrix's
/// `indptr` or `indices`) in its native signed/unsigned width.
async fn read_int_array_range(store: Arc<dyn AsyncReadableStorageTraits>, array_path: &str, start: u64, stop: u64) -> Result<IntArray, zarrs::array::ArrayError> {
    let array = zarrs::array::Array::async_open(store, array_path).await.unwrap();
    let subset = zarrs::array::ArraySubset::new_with_ranges(&[start..stop]);

    use zarrs::plugin::ZarrVersion;
    let dtype_name = array.data_type().name(ZarrVersion::V3).expect("Array data type must have a V3 name").to_string();

    macro_rules! load {
        ($rust_ty:ty, $variant:ident) => {{
            IntArray::$variant(array.async_retrieve_array_subset::<Vec<$rust_ty>>(&subset).await?)
        }};
    }

    Ok(match dtype_name.as_str() {
        "uint8" => load!(u8, Uint8),
        "uint16" => load!(u16, Uint16),
        "uint32" => load!(u32, Uint32),
        "uint64" => load!(u64, Uint64),
        "int8" => load!(i8, Int8),
        "int16" => load!(i16, Int16),
        "int32" => load!(i32, Int32),
        "int64" => load!(i64, Int64),
        other => panic!("Unsupported dtype \"{other}\" for sparse matrix index array \"{array_path}\" (expected an integer type)"),
    })
}

/// Scatters the non-zero `values` of a single sparse column into a dense `n_obs`-length column,
/// placing each value at the row named by the matching entry of `row_indices`.
///
/// Generic over the index dtype `I` and the value dtype `V` so that both slices are read at their
/// stored width. A row index still has to become a `usize` to index `dense` — that is what a row
/// index *is* — but since this function is monomorphized per dtype, the conversion is a compile-time
/// -known widening rather than a dynamic dispatch, and no converted copy of `row_indices` is built.
fn scatter_column<I, V>(row_indices: &[I], values: &[V], n_obs: usize) -> Vec<V>
where
    I: Copy + TryInto<usize>,
    V: Copy + Default,
{
    let mut dense = vec![V::default(); n_obs];
    for (&row, &value) in row_indices.iter().zip(values.iter()) {
        let row = row.try_into().ok().expect("Sparse matrix row index must be non-negative");
        dense[row] = value;
    }
    dense
}

/// Returns the ascending positions, within a sparse matrix's `indices` array, of every entry
/// referring to column `col_index`.
///
/// `col_index` is narrowed into the `indices` dtype once, up front, so the scan compares
/// natively-typed values instead of widening every element of the array. A `col_index` too large
/// for that dtype cannot match any entry, so the narrowing failing simply means "no matches".
fn find_column_entries<C>(col_indices: &[C], col_index: u64) -> Vec<usize>
where
    C: Copy + Eq + TryFrom<u64>,
{
    let Ok(target) = C::try_from(col_index) else {
        return Vec::new();
    };
    col_indices
        .iter()
        .enumerate()
        .filter(|(_, &value)| value == target)
        .map(|(entry, _)| entry)
        .collect()
}

/// Maps ascending positions within a CSR matrix's `indices`/`data` arrays to the row each entry
/// belongs to, given a block of the matrix's `indptr`. The returned rows are non-decreasing and
/// numbered relative to the first row of that block.
///
/// Each position is converted into the `indptr` dtype once, rather than the `indptr` elements being
/// widened: `entries` holds at most one position per row of the requested column, so this touches
/// far fewer values than converting `indptr` itself would.
fn rows_for_entries<P>(indptr: &[P], entries: &[u64]) -> Vec<usize>
where
    P: Copy + Ord + TryFrom<u64>,
{
    entries
        .iter()
        .map(|&entry| {
            let Ok(entry) = P::try_from(entry) else {
                panic!("Sparse matrix entry position does not fit in the dtype of its indptr array");
            };
            // `indptr` is non-decreasing and row `r` owns the entry positions `indptr[r]..indptr[r + 1]`,
            // so the row containing `entry` is the last one whose start offset is `<= entry`.
            indptr
                .partition_point(|&row_start| row_start <= entry)
                .checked_sub(1)
                .expect("Sparse matrix entry position must lie at or after the block's first indptr offset")
        })
        .collect()
}

/// Builds the dense `n_obs`-length column `col_index` of the CSR matrix at `matrix_path`, in the
/// `data` array's own dtype `V`.
///
/// The matrix is traversed in two nested fixed-size steps, so no array is ever held whole: `indptr`
/// is read one block of `budget` rows at a time, and each block's slice of `indices` is then read
/// `budget` positions at a time. Block boundaries need not line up with row boundaries — every
/// position is resolved to its row independently, by looking it up in the `indptr` block it came
/// from — and a span's `data` is fetched only once its `indices` have shown that the span holds
/// something for this column, which for a single gene skips nearly every span.
async fn read_csr_column_values<V>(
    store: Arc<dyn AsyncReadableStorageTraits>,
    matrix_path: &str,
    data_array: &zarrs::array::Array<dyn AsyncReadableStorageTraits>,
    col_index: u64,
    n_obs: usize,
    budget: u64,
) -> Result<Vec<V>, zarrs::array::ArrayError>
where
    V: zarrs::array::ElementOwned + zarrs::storage::MaybeSend + zarrs::storage::MaybeSync + Copy + Default,
{
    let indptr_path = format!("{matrix_path}/indptr");
    let indices_path = format!("{matrix_path}/indices");
    let elements_per_read = budget as usize;

    let mut column = vec![V::default(); n_obs];
    for first_row in (0..n_obs).step_by(elements_per_read) {
        let rows_in_block = elements_per_read.min(n_obs - first_row);
        // One offset per row of the block, plus the terminating offset that closes its last row.
        let indptr_block = read_int_array_range(store.clone(), &indptr_path, first_row as u64, (first_row + rows_in_block) as u64 + 1).await?;
        let (block_start, block_stop) = (indptr_block.offset_at(0), indptr_block.offset_at(rows_in_block));

        for span_start in (block_start..block_stop).step_by(elements_per_read) {
            let span_stop = (span_start + budget).min(block_stop);
            let indices_span = read_int_array_range(store.clone(), &indices_path, span_start, span_stop).await?;
            let matches = with_int_slice!(&indices_span, |indices| find_column_entries(indices, col_index));
            if matches.is_empty() {
                continue;
            }

            // `find_column_entries` reports positions within the span it was given; shift those to
            // absolute positions in `indices`/`data` to resolve each one against `indptr`.
            let positions: Vec<u64> = matches.iter().map(|&position| span_start + position as u64).collect();
            let rows = with_int_slice!(&indptr_block, |offsets| rows_for_entries(offsets, &positions));

            let subset = zarrs::array::ArraySubset::new_with_ranges(&[span_start..span_stop]);
            let values = data_array.async_retrieve_array_subset::<Vec<V>>(&subset).await?;
            for (&position, &row) in matches.iter().zip(rows.iter()) {
                column[first_row + row] = values[position];
            }
        }
    }
    Ok(column)
}

/// Reads one column (single gene, all rows) of a CSC-sparse AnnData matrix (e.g. `X` or a
/// `layers` entry) in its native dtype, given the zero-based column index. `matrix_path` is the
/// path to the `csc_matrix`-encoded group, which has sibling `indptr`, `indices`, and `data`
/// arrays. Since a CSC matrix's `indptr` is indexed by column, the requested column's non-zero
/// entries live in one contiguous range of `indices`/`data`, so only that range is read (unlike
/// [`read_csr_column_numeric`]). See [`read_dense_column_numeric`] for the dense equivalent.
pub async fn read_csc_column_numeric(store: Arc<dyn AsyncReadableStorageTraits>, matrix_path: &str, col_index: u64) -> Result<NumericData, zarrs::array::ArrayError> {
    let shape = read_sparse_matrix_shape(store.clone(), matrix_path).await;
    let n_obs = shape[0] as usize;

    let indptr = read_int_array_range(store.clone(), &format!("{matrix_path}/indptr"), col_index, col_index + 2).await?;
    let (start, stop) = (indptr.offset_at(0), indptr.offset_at(1));

    // An all-zero column has no `indices`/`data` entries to read at all.
    let row_indices = if start != stop {
        Some(read_int_array_range(store.clone(), &format!("{matrix_path}/indices"), start, stop).await?)
    } else {
        None
    };
    let subset = zarrs::array::ArraySubset::new_with_ranges(&[start..stop]);

    let data_path = format!("{matrix_path}/data");
    let data_array = zarrs::array::Array::async_open(store.clone(), &data_path).await.unwrap();

    use zarrs::plugin::ZarrVersion;
    let dtype_name = data_array.data_type().name(ZarrVersion::V3).expect("Array data type must have a V3 name").to_string();

    macro_rules! densify {
        ($rust_ty:ty, $variant:ident) => {{
            let dense = match &row_indices {
                Some(row_indices) => {
                    let values = data_array.async_retrieve_array_subset::<Vec<$rust_ty>>(&subset).await?;
                    with_int_slice!(row_indices, |rows| scatter_column(rows, &values, n_obs))
                }
                None => vec![<$rust_ty>::default(); n_obs],
            };
            NumericData::$variant(Arc::new(dense))
        }};
    }

    Ok(match dtype_name.as_str() {
        "uint8" => densify!(u8, Uint8),
        "uint16" => densify!(u16, Uint16),
        "uint32" => densify!(u32, Uint32),
        "uint64" => densify!(u64, Uint64),
        "int8" => densify!(i8, Int8),
        "int16" => densify!(i16, Int16),
        "int32" => densify!(i32, Int32),
        "int64" => densify!(i64, Int64),
        "float32" => densify!(f32, Float32),
        "float64" => densify!(f64, Float64),
        other => panic!("Unsupported dtype \"{other}\" for AnnData sparse expression matrix \"{data_path}\""),
    })
}

/// Reads one column (single gene, all rows) of a CSR-sparse AnnData matrix (e.g. `X` or a
/// `layers` entry) in its native dtype, given the zero-based column index. `matrix_path` is the
/// path to the `csr_matrix`-encoded group, which has sibling `indptr`, `indices`, and `data`
/// arrays. Unlike the CSC case, a CSR matrix's `indptr` is indexed by row rather than column, so
/// the requested column's non-zero entries are scattered across every row's range: there is no
/// contiguous slice of `indices`/`data` to target, and finding them means traversing the whole
/// matrix. [`read_csr_column_values`] does that traversal in fixed-size pieces, discarding each as
/// soon as the entries it contributes have been written, so peak memory stays proportional to the
/// `n_obs`-length output rather than to the size of the matrix.
pub async fn read_csr_column_numeric(store: Arc<dyn AsyncReadableStorageTraits>, matrix_path: &str, col_index: u64) -> Result<NumericData, zarrs::array::ArrayError> {
    let shape = read_sparse_matrix_shape(store.clone(), matrix_path).await;
    let n_obs = shape[0] as usize;

    let data_path = format!("{matrix_path}/data");
    let data_array = zarrs::array::Array::async_open(store.clone(), &data_path).await.unwrap();

    use zarrs::plugin::ZarrVersion;
    let dtype_name = data_array.data_type().name(ZarrVersion::V3).expect("Array data type must have a V3 name").to_string();

    macro_rules! extract {
        ($rust_ty:ty, $variant:ident) => {{
            let column = read_csr_column_values::<$rust_ty>(
                store.clone(), matrix_path, &data_array, col_index, n_obs, MAX_ELEMENTS_PER_READ,
            ).await?;
            NumericData::$variant(Arc::new(column))
        }};
    }

    Ok(match dtype_name.as_str() {
        "uint8" => extract!(u8, Uint8),
        "uint16" => extract!(u16, Uint16),
        "uint32" => extract!(u32, Uint32),
        "uint64" => extract!(u64, Uint64),
        "int8" => extract!(i8, Int8),
        "int16" => extract!(i16, Int16),
        "int32" => extract!(i32, Int32),
        "int64" => extract!(i64, Int64),
        "float32" => extract!(f32, Float32),
        "float64" => extract!(f64, Float64),
        other => panic!("Unsupported dtype \"{other}\" for AnnData sparse expression matrix \"{data_path}\""),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // The 3x4 matrix used throughout these tests:
    //   row 0: [0, 5, 0, 7]
    //   row 1: [0, 0, 0, 0]
    //   row 2: [3, 0, 8, 0]
    // CSR: indptr [0, 2, 2, 4], indices [1, 3, 0, 2], data [5, 7, 3, 8]
    // CSC: indptr [0, 1, 2, 3, 4], indices [2, 0, 2, 0], data [3, 5, 8, 7]
    const DENSE: [[i32; 4]; 3] = [[0, 5, 0, 7], [0, 0, 0, 0], [3, 0, 8, 0]];

    /// Mirrors [`read_csr_column_values`] over already-read arrays, traversing them in the same
    /// nested fixed-size steps so that the block/span boundary arithmetic is exercised. `budget`
    /// stands in for [`MAX_ELEMENTS_PER_READ`]; values well below the matrix size force the
    /// multi-block path that a realistic budget would only reach on a very large matrix.
    fn csr_column<P, C>(indptr: &[P], indices: &[C], data: &[i32], col_index: u64, n_obs: usize, budget: u64) -> Vec<i32>
    where
        P: Copy + Ord + TryFrom<u64> + TryInto<u64>,
        C: Copy + Eq + TryFrom<u64>,
    {
        let offset = |row: usize| -> u64 { indptr[row].try_into().ok().unwrap() };
        let per_read = budget as usize;

        let mut column = vec![0i32; n_obs];
        for first_row in (0..n_obs).step_by(per_read) {
            let rows_in_block = per_read.min(n_obs - first_row);
            let indptr_block = &indptr[first_row..=first_row + rows_in_block];
            let (block_start, block_stop) = (offset(first_row), offset(first_row + rows_in_block));

            for span_start in (block_start..block_stop).step_by(per_read) {
                let span_stop = (span_start + budget).min(block_stop);
                let span = &indices[span_start as usize..span_stop as usize];
                let matches = find_column_entries(span, col_index);
                if matches.is_empty() {
                    continue;
                }
                let positions: Vec<u64> = matches.iter().map(|&p| span_start + p as u64).collect();
                let rows = rows_for_entries(indptr_block, &positions);
                for (&position, &row) in matches.iter().zip(rows.iter()) {
                    column[first_row + row] = data[span_start as usize + position];
                }
            }
        }
        column
    }

    #[test]
    fn csr_extraction_matches_the_dense_matrix_at_every_budget() {
        let indptr: [i32; 4] = [0, 2, 2, 4];
        let indices: [i32; 4] = [1, 3, 0, 2];
        let data: [i32; 4] = [5, 7, 3, 8];

        // A budget of 1 reads a single row and a single entry at a time — every block and span
        // boundary the traversal can produce; 8 exceeds the matrix, so it reads everything at once.
        for budget in 1..=8u64 {
            for col in 0..4u64 {
                let expected: Vec<i32> = DENSE.iter().map(|row| row[col as usize]).collect();
                assert_eq!(csr_column(&indptr, &indices, &data, col, 3, budget), expected, "column {col}, budget {budget}");
            }
        }
    }

    #[test]
    fn csr_extraction_is_dtype_agnostic() {
        // A `uint8` indices array alongside an `int64` indptr: the two are read and used at their
        // own widths, and neither is widened to match the other.
        let indptr: [i64; 4] = [0, 2, 2, 4];
        let indices: [u8; 4] = [1, 3, 0, 2];
        let data: [i32; 4] = [5, 7, 3, 8];

        assert_eq!(csr_column(&indptr, &indices, &data, 1, 3, 2), vec![5, 0, 0]);
        // A column index too large for the indices dtype simply matches nothing.
        assert_eq!(csr_column(&indptr, &indices, &data, 300, 3, 2), vec![0, 0, 0]);
    }

    #[test]
    fn csr_extraction_handles_empty_rows_and_matrices() {
        // Row 1 is empty (indptr[1] == indptr[2]), so it stays zero in every column.
        let indptr: [i32; 4] = [0, 2, 2, 4];
        let indices: [i32; 4] = [1, 3, 0, 2];
        let data: [i32; 4] = [5, 7, 3, 8];
        for budget in 1..=8u64 {
            assert_eq!(csr_column(&indptr, &indices, &data, 0, 3, budget)[1], 0);
        }

        // A matrix with no non-zeros at all reads no spans whatsoever.
        assert_eq!(csr_column(&[0i32, 0, 0, 0], &[] as &[i32], &[], 2, 3, 2), vec![0, 0, 0]);
    }

    #[test]
    fn csr_extraction_handles_a_row_larger_than_the_budget() {
        // A single row holding more entries than one read allows: its entries span several reads,
        // and each of those still has to resolve back to that same row.
        //   row 0: [0, 0, 0], row 1: [1, 2, 3], row 2: [0, 0, 0]
        let indptr: [i32; 4] = [0, 0, 3, 3];
        let indices: [i32; 3] = [0, 1, 2];
        let data: [i32; 3] = [1, 2, 3];

        for budget in 1..=4u64 {
            assert_eq!(csr_column(&indptr, &indices, &data, 0, 3, budget), vec![0, 1, 0], "budget {budget}");
            assert_eq!(csr_column(&indptr, &indices, &data, 2, 3, budget), vec![0, 3, 0], "budget {budget}");
        }
    }

    #[test]
    fn find_column_entries_returns_ascending_positions() {
        let indices: [i32; 6] = [2, 0, 2, 1, 2, 0];
        assert_eq!(find_column_entries(&indices, 2), vec![0, 2, 4]);
        assert_eq!(find_column_entries(&indices, 0), vec![1, 5]);
        assert_eq!(find_column_entries(&indices, 3), Vec::<usize>::new());
    }

    #[test]
    fn rows_for_entries_assigns_each_entry_to_its_row() {
        // Rows 1 and 3 are empty; row 0 owns entries 0..2, row 2 owns 2..3, row 4 owns 3..5.
        let indptr: [i32; 6] = [0, 2, 2, 3, 3, 5];
        assert_eq!(rows_for_entries(&indptr, &[0, 1, 2, 3, 4]), vec![0, 0, 2, 4, 4]);
    }

    #[test]
    fn rows_for_entries_numbers_rows_relative_to_its_indptr_block() {
        // The tail of the same indptr, as a block covering rows 2..4: absolute positions resolve to
        // rows 0 and 2 of the block, which the caller then shifts back by the block's first row.
        let block: [i32; 3] = [2, 3, 3];
        assert_eq!(rows_for_entries(&block, &[2]), vec![0]);
        let block: [i32; 2] = [3, 5];
        assert_eq!(rows_for_entries(&block, &[3, 4]), vec![0, 0]);
    }

    #[test]
    fn scatter_column_places_values_at_their_row_indices() {
        // The CSC form of column 0 of `DENSE`: a single non-zero at row 2.
        assert_eq!(scatter_column(&[2i32], &[3i32], 3), vec![0, 0, 3]);
        // Rows may be listed in any order, and an all-zero column scatters nothing.
        assert_eq!(scatter_column(&[2u16, 0], &[8.0f32, 5.0], 3), vec![5.0, 0.0, 8.0]);
        assert_eq!(scatter_column(&[] as &[i64], &[] as &[u8], 3), vec![0, 0, 0]);
    }
}
