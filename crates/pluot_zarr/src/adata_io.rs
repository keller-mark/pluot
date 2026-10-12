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
    if let Ok(group) = zarrs::group::Group::async_open(store.clone(), path).await {
        return parse_encoding(group.attributes(), path);
    }
    let array = zarrs::array::Array::async_open(store, path)
        .await
        .unwrap_or_else(|e| panic!("Failed to open AnnData element at \"{path}\": {e}"));
    parse_encoding(array.attributes(), path)
}

fn parse_encoding(attributes: &serde_json::Map<String, serde_json::Value>, path: &str) -> AnnDataEncoding {
    serde_json::from_value(serde_json::Value::Object(attributes.clone()))
        .unwrap_or_else(|e| panic!("Invalid AnnData encoding attributes at \"{path}\": {e}"))
}

/// The parts of an AnnData expression matrix (`X` or a `layers` entry) that live in `zarr.json`
/// documents rather than in chunks: the matrix's own AnnData encoding, plus the zarr metadata of
/// every array a column read opens.
///
/// Reading one gene's column opens exactly the same two to four arrays that reading any other
/// gene's column opens, so letting each read open them itself refetches the same handful of
/// `zarr.json` documents once per gene, per frame. Loading them once up front instead lets every
/// read rebuild its arrays in memory via [`zarrs::array::Array::new_with_metadata`], the same way
/// `OmeZarrBitmapLayer` reuses the array metadata its multiscale parent already loaded.
#[derive(Clone, Debug)]
pub struct ExprMatrixMetadata {
    /// The matrix itself: a dense array, or a `csr_matrix`/`csc_matrix` group.
    pub matrix_path: String,
    pub encoding: AnnDataEncoding,
    /// The matrix array itself when dense, else the sparse group's `data` sibling.
    values: zarrs::array::ArrayMetadata,
    /// The sparse group's `indptr` and `indices` siblings; `None` when the matrix is dense.
    indptr: Option<zarrs::array::ArrayMetadata>,
    indices: Option<zarrs::array::ArrayMetadata>,
}

impl ExprMatrixMetadata {
    fn values_array(&self, store: Arc<dyn AsyncReadableStorageTraits>) -> zarrs::array::Array<dyn AsyncReadableStorageTraits> {
        let path = match self.encoding {
            AnnDataEncoding::Array { .. } => self.matrix_path.clone(),
            _ => format!("{}/data", self.matrix_path),
        };
        array_with_metadata(store, &path, &self.values)
    }

    fn indptr_array(&self, store: Arc<dyn AsyncReadableStorageTraits>) -> zarrs::array::Array<dyn AsyncReadableStorageTraits> {
        self.sparse_array(store, "indptr", self.indptr.as_ref())
    }

    fn indices_array(&self, store: Arc<dyn AsyncReadableStorageTraits>) -> zarrs::array::Array<dyn AsyncReadableStorageTraits> {
        self.sparse_array(store, "indices", self.indices.as_ref())
    }

    fn sparse_array(&self, store: Arc<dyn AsyncReadableStorageTraits>, name: &str, metadata: Option<&zarrs::array::ArrayMetadata>) -> zarrs::array::Array<dyn AsyncReadableStorageTraits> {
        let metadata = metadata.unwrap_or_else(|| panic!("Expected a sparse AnnData matrix at \"{}\", which would have a \"{name}\" array", self.matrix_path));
        array_with_metadata(store, &format!("{}/{name}", self.matrix_path), metadata)
    }

    /// `n_obs`, from the logical `[n_obs, n_var]` shape in the sparse matrix's encoding attributes.
    fn sparse_n_obs(&self) -> usize {
        match &self.encoding {
            AnnDataEncoding::CsrMatrix { shape, .. } | AnnDataEncoding::CscMatrix { shape, .. } => shape[0] as usize,
            other => panic!("Expected a csr_matrix or csc_matrix encoding at \"{}\", got {other:?}", self.matrix_path),
        }
    }
}

fn array_with_metadata(store: Arc<dyn AsyncReadableStorageTraits>, path: &str, metadata: &zarrs::array::ArrayMetadata) -> zarrs::array::Array<dyn AsyncReadableStorageTraits> {
    zarrs::array::Array::new_with_metadata(store, path, metadata.clone())
        .unwrap_or_else(|e| panic!("Failed to rebuild zarr array at \"{path}\": {e}"))
}

/// Opens the zarr array at `path`, rebuilding it in memory when the caller already holds its
/// `metadata` (no `zarr.json` fetch) and fetching that document otherwise.
async fn open_array(store: Arc<dyn AsyncReadableStorageTraits>, path: &str, metadata: Option<&zarrs::array::ArrayMetadata>) -> zarrs::array::Array<dyn AsyncReadableStorageTraits> {
    match metadata {
        Some(metadata) => array_with_metadata(store, path, metadata),
        None => zarrs::array::Array::async_open(store, path)
            .await
            .unwrap_or_else(|e| panic!("Failed to open zarr array at \"{path}\": {e}")),
    }
}

/// Reads every `zarr.json` that reading columns of the expression matrix at `matrix_path` needs.
///
/// A sparse matrix is a group whose `data`, `indptr` and `indices` arrays are fetched together;
/// a dense one is an array, whose encoding attributes and zarr metadata both come from the single
/// document that opening it already fetched.
pub async fn read_expr_matrix_metadata(store: Arc<dyn AsyncReadableStorageTraits>, matrix_path: &str) -> ExprMatrixMetadata {
    let Ok(group) = zarrs::group::Group::async_open(store.clone(), matrix_path).await else {
        let array = zarrs::array::Array::async_open(store, matrix_path)
            .await
            .unwrap_or_else(|e| panic!("Failed to open AnnData expression matrix at \"{matrix_path}\": {e}"));
        return ExprMatrixMetadata {
            matrix_path: matrix_path.to_string(),
            encoding: parse_encoding(array.attributes(), matrix_path),
            values: array.metadata().clone(),
            indptr: None,
            indices: None,
        };
    };

    let (values, indptr, indices) = futures::join!(
        read_array_metadata(store.clone(), format!("{matrix_path}/data")),
        read_array_metadata(store.clone(), format!("{matrix_path}/indptr")),
        read_array_metadata(store.clone(), format!("{matrix_path}/indices")),
    );
    ExprMatrixMetadata {
        matrix_path: matrix_path.to_string(),
        encoding: parse_encoding(group.attributes(), matrix_path),
        values,
        indptr: Some(indptr),
        indices: Some(indices),
    }
}

async fn read_array_metadata(store: Arc<dyn AsyncReadableStorageTraits>, path: String) -> zarrs::array::ArrayMetadata {
    zarrs::array::Array::async_open(store, &path)
        .await
        .unwrap_or_else(|e| panic!("Failed to open AnnData expression array at \"{path}\": {e}"))
        .metadata()
        .clone()
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
pub async fn read_dense_column_numeric(store: Arc<dyn AsyncReadableStorageTraits>, array_path: &str, array_metadata: Option<&zarrs::array::ArrayMetadata>, col_index: u64) -> Result<NumericData, zarrs::array::ArrayError> {
    let array = open_array(store, array_path, array_metadata).await;
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
pub async fn read_dense_column_f32(store: Arc<dyn AsyncReadableStorageTraits>, array_path: &str, array_metadata: Option<&zarrs::array::ArrayMetadata>, col_index: u64) -> Result<Vec<f32>, zarrs::array::ArrayError> {
    let array = open_array(store, array_path, array_metadata).await;
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

/// Reads `array[start..stop]` of an integer-typed 1D zarr array (e.g. a sparse matrix's `indptr`
/// or `indices`) in its native signed/unsigned width. Takes an already-opened array because a
/// single column read calls this many times over the same two arrays.
async fn read_int_array_range(array: &zarrs::array::Array<dyn AsyncReadableStorageTraits>, start: u64, stop: u64) -> Result<IntArray, zarrs::array::ArrayError> {
    let array_path = array.path();
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
pub fn scatter_column<I, V>(row_indices: &[I], values: &[V], n_obs: usize) -> Vec<V>
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
pub fn find_column_entries<C>(col_indices: &[C], col_index: u64) -> Vec<usize>
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
pub fn rows_for_entries<P>(indptr: &[P], entries: &[u64]) -> Vec<usize>
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
    data_array: &zarrs::array::Array<dyn AsyncReadableStorageTraits>,
    indptr_array: &zarrs::array::Array<dyn AsyncReadableStorageTraits>,
    indices_array: &zarrs::array::Array<dyn AsyncReadableStorageTraits>,
    col_index: u64,
    n_obs: usize,
    budget: u64,
) -> Result<Vec<V>, zarrs::array::ArrayError>
where
    V: zarrs::array::ElementOwned + zarrs::storage::MaybeSend + zarrs::storage::MaybeSync + Copy + Default,
{
    let elements_per_read = budget as usize;

    let mut column = vec![V::default(); n_obs];
    for first_row in (0..n_obs).step_by(elements_per_read) {
        let rows_in_block = elements_per_read.min(n_obs - first_row);
        // One offset per row of the block, plus the terminating offset that closes its last row.
        let indptr_block = read_int_array_range(indptr_array, first_row as u64, (first_row + rows_in_block) as u64 + 1).await?;
        let (block_start, block_stop) = (indptr_block.offset_at(0), indptr_block.offset_at(rows_in_block));

        for span_start in (block_start..block_stop).step_by(elements_per_read) {
            let span_stop = (span_start + budget).min(block_stop);
            let indices_span = read_int_array_range(indices_array, span_start, span_stop).await?;
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
/// `layers` entry) in its native dtype, given the zero-based column index. `matrix` describes the
/// `csc_matrix`-encoded group, whose `indptr`, `indices` and `data` siblings this reads. Since a
/// CSC matrix's `indptr` is indexed by column, the requested column's non-zero entries live in one
/// contiguous range of `indices`/`data`, so only that range is read (unlike
/// [`read_csr_column_numeric`]). See [`read_dense_column_numeric`] for the dense equivalent.
pub async fn read_csc_column_numeric(store: Arc<dyn AsyncReadableStorageTraits>, matrix: &ExprMatrixMetadata, col_index: u64) -> Result<NumericData, zarrs::array::ArrayError> {
    let n_obs = matrix.sparse_n_obs();

    let indptr = read_int_array_range(&matrix.indptr_array(store.clone()), col_index, col_index + 2).await?;
    let (start, stop) = (indptr.offset_at(0), indptr.offset_at(1));

    // An all-zero column has no `indices`/`data` entries to read at all.
    let row_indices = if start != stop {
        Some(read_int_array_range(&matrix.indices_array(store.clone()), start, stop).await?)
    } else {
        None
    };
    let subset = zarrs::array::ArraySubset::new_with_ranges(&[start..stop]);

    let data_array = matrix.values_array(store);
    let data_path = data_array.path().to_string();

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
/// `layers` entry) in its native dtype, given the zero-based column index. `matrix` describes the
/// `csr_matrix`-encoded group, whose `indptr`, `indices` and `data` siblings this reads.
/// Unlike the CSC case, a CSR matrix's `indptr` is indexed by row rather than column, so
/// the requested column's non-zero entries are scattered across every row's range: there is no
/// contiguous slice of `indices`/`data` to target, and finding them means traversing the whole
/// matrix. [`read_csr_column_values`] does that traversal in fixed-size pieces, discarding each as
/// soon as the entries it contributes have been written, so peak memory stays proportional to the
/// `n_obs`-length output rather than to the size of the matrix.
pub async fn read_csr_column_numeric(store: Arc<dyn AsyncReadableStorageTraits>, matrix: &ExprMatrixMetadata, col_index: u64) -> Result<NumericData, zarrs::array::ArrayError> {
    let n_obs = matrix.sparse_n_obs();

    let data_array = matrix.values_array(store.clone());
    let indptr_array = matrix.indptr_array(store.clone());
    let indices_array = matrix.indices_array(store);
    let data_path = data_array.path().to_string();

    use zarrs::plugin::ZarrVersion;
    let dtype_name = data_array.data_type().name(ZarrVersion::V3).expect("Array data type must have a V3 name").to_string();

    macro_rules! extract {
        ($rust_ty:ty, $variant:ident) => {{
            let column = read_csr_column_values::<$rust_ty>(
                &data_array, &indptr_array, &indices_array, col_index, n_obs, MAX_ELEMENTS_PER_READ,
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

/// Reads one column (single gene, all rows) of an AnnData expression matrix — `X` or a `layers`
/// entry — as `f32`, given the zero-based column index.
///
/// AnnData stores such a matrix in any of three layouts, and `matrix` may describe any of them: a
/// dense `array`, or a `csr_matrix` or `csc_matrix` group. The layout is taken from the matrix's
/// own encoding metadata and dispatched to the matching reader, so callers that just want a gene's
/// values need not know (or branch on) how the matrix happens to be stored.
///
/// `matrix` is read once for the whole matrix (see [`read_expr_matrix_metadata`]) rather than per
/// column, so reading many genes fetches no `zarr.json` at all.
///
/// Values are converted to `f32` because that is what the GPU consumes; use
/// [`read_dense_column_numeric`], [`read_csr_column_numeric`] or [`read_csc_column_numeric`]
/// directly to keep a column in its stored dtype.
pub async fn read_matrix_column_f32(store: Arc<dyn AsyncReadableStorageTraits>, matrix: &ExprMatrixMetadata, col_index: u64) -> Result<Vec<f32>, zarrs::array::ArrayError> {
    match matrix.encoding {
        AnnDataEncoding::Array { .. } => read_dense_column_f32(store, &matrix.matrix_path, Some(&matrix.values), col_index).await,
        AnnDataEncoding::CsrMatrix { .. } => Ok(read_csr_column_numeric(store, matrix, col_index).await?.as_f32().into_owned()),
        AnnDataEncoding::CscMatrix { .. } => Ok(read_csc_column_numeric(store, matrix, col_index).await?.as_f32().into_owned()),
        ref other => panic!("Unsupported AnnData expression matrix encoding at \"{}\": {other:?}", matrix.matrix_path),
    }
}
