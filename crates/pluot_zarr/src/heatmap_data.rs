// Loads the filtered submatrix of an AnnData expression matrix, plus the per-axis selection
// masks, for `AdataZarrHeatmapLayer`.
//
// Each axis's filter-included indices, its selected indices, its selection mask, each row block
// of the submatrix and each block's extent are memoized independently, so that e.g. dragging a
// selection brush re-uses the loaded matrix blocks.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use zarrs::array::ArrayError;
use zarrs::storage::AsyncReadableStorageTraits;

use pluot_core::cache::{use_memo_compressed_matrix, use_memo_numeric_data, use_memo_vec_f32, use_memo_vec_string};
use pluot_core::compute::included_indices::compute_included_indices;
use pluot_core::compute::histogram_percentile::{histogram_bin, PercentileSearch};
use pluot_core::compute::matrix_axis_extents::{
    reduce_compressed_matrix_axis_extents, reduce_compressed_matrix_axis_histograms, reduce_matrix_axis_extents, reduce_matrix_axis_histograms,
    MatrixAxis as HeatmapAxis,
};
use pluot_core::compute::reduce::{reduce_extent, reduce_histogram_with_known_extent};
use pluot_core::log;
use pluot_core::numeric_data::{NumericData, NumericElement};
use pluot_core::render_types::GpuContext;
use pluot_core::sparse_matrix::{CompressedAxis, CompressedMatrix};
use pluot_core::{with_int_slice, with_numeric_slice};

use crate::adata_io::{
    read_compressed_column, read_compressed_subset, read_dataframe_column_strings, read_dense_column_numeric,
    read_dense_subset_numeric, read_matrix_layout, read_matrix_shape, MatrixLayout,
};
use crate::zarr_emphasis_criteria::{resolve_zarr_emphasis_criteria, ZarrEmphasisCriteria};

/// Which entries along one axis of the matrix (obs rows or var columns) are
/// included by a filter, or emphasized by a selection.
///
/// Serialized as an adjacently-tagged enum, e.g.
/// `{"axis_criteria_mode": "Identifiers", "axis_criteria_params": {"identifiers": ["CD3E"]}}`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "axis_criteria_mode", content = "axis_criteria_params")]
pub enum AxisCriteria {
    /// The entries whose identifier is listed, in the listed order. Unknown
    /// identifiers are skipped and repeated ones are included once.
    Identifiers(IdentifierCriteriaParams),
    /// The entries meeting every criteria (AND-ed together), in stored order.
    /// Each criteria's array must have one element per entry along the axis.
    Criteria(Vec<ZarrEmphasisCriteria>),
}

/// Parameters for [`AxisCriteria::Identifiers`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IdentifierCriteriaParams {
    /// The dataframe column holding the identifiers. `None` means its index.
    #[serde(default)]
    pub column: Option<String>,
    pub identifiers: Vec<String>,
}

/// One axis of the matrix, identified by its AnnData dataframe.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MatrixAxis {
    Obs,
    Var,
}

impl MatrixAxis {
    pub fn dataframe_path(self) -> &'static str {
        match self {
            MatrixAxis::Obs => "/obs",
            MatrixAxis::Var => "/var",
        }
    }

    fn shape_dim(self) -> usize {
        match self {
            MatrixAxis::Obs => 0,
            MatrixAxis::Var => 1,
        }
    }
}

/// Cache key parts identifying `criteria`, prefixed with their count so that
/// several of these can be concatenated into one key unambiguously.
pub fn axis_criteria_key(criteria: Option<&AxisCriteria>) -> Vec<String> {
    let parts: Vec<String> = match criteria {
        None => vec!["all".to_string()],
        Some(AxisCriteria::Identifiers(params)) => {
            let mut parts = vec!["identifiers".to_string(), params.column.clone().unwrap_or_else(|| "index".to_string())];
            parts.extend(params.identifiers.iter().cloned());
            parts
        }
        Some(AxisCriteria::Criteria(criteria)) => {
            let mut parts = vec!["criteria".to_string()];
            for criterion in criteria {
                match criterion {
                    ZarrEmphasisCriteria::Categorical(params) => {
                        parts.push("categorical".to_string());
                        parts.push(params.codes_key.clone());
                        parts.push(params.included_codes.iter().map(i64::to_string).collect::<Vec<_>>().join(","));
                    }
                    ZarrEmphasisCriteria::Quantitative(params) => {
                        parts.push("quantitative".to_string());
                        parts.push(params.values_key.clone());
                        parts.push(format!("{:?}", params.min));
                        parts.push(format!("{:?}", params.max));
                        parts.push(format!("{:?}", params.min_exclusive));
                        parts.push(format!("{:?}", params.max_exclusive));
                    }
                    ZarrEmphasisCriteria::Boolean(params) => {
                        parts.push("boolean".to_string());
                        parts.push(params.mask_key.clone());
                    }
                }
            }
            parts
        }
    };
    std::iter::once(parts.len().to_string()).chain(parts).collect()
}

/// The indices held by a [`load_axis_indices`] result.
pub fn indices_of(data: &NumericData) -> &[u32] {
    match data {
        NumericData::Uint32(values) => values,
        _ => unreachable!("index arrays are memoized as Uint32"),
    }
}

/// Loads (and caches) the `[n_obs, n_var]` shape of the matrix at `matrix_path`.
pub async fn load_matrix_shape(store: Arc<dyn AsyncReadableStorageTraits>, store_name: &str, matrix_path: &str, cache_enabled: bool) -> Result<[usize; 2], ArrayError> {
    let keys = vec!["adata_matrix_shape".to_string(), store_name.to_string(), matrix_path.to_string()];
    let shape = use_memo_numeric_data(
        async || Ok::<_, ArrayError>(NumericData::from(read_matrix_shape(store, matrix_path).await)),
        &keys,
        cache_enabled,
    )
    .await?;
    let NumericData::Uint64(shape) = shape.as_ref() else {
        unreachable!("matrix shapes are memoized as Uint64");
    };
    Ok([shape[0] as usize, shape[1] as usize])
}

/// Loads (and caches) how the matrix at `matrix_path` is stored.
pub async fn load_matrix_layout(store: Arc<dyn AsyncReadableStorageTraits>, store_name: &str, matrix_path: &str, cache_enabled: bool) -> MatrixLayout {
    let keys = vec!["adata_matrix_layout".to_string(), store_name.to_string(), matrix_path.to_string()];
    let layout = use_memo_vec_string(
        async || {
            let layout = match read_matrix_layout(store, matrix_path).await {
                MatrixLayout::Dense => "dense",
                MatrixLayout::Compressed(CompressedAxis::Rows) => "csr",
                MatrixLayout::Compressed(CompressedAxis::Cols) => "csc",
            };
            Ok::<_, std::convert::Infallible>(vec![layout.to_string()])
        },
        &keys,
        cache_enabled,
    )
    .await
    .expect("infallible");
    match layout[0].as_str() {
        "csr" => MatrixLayout::Compressed(CompressedAxis::Rows),
        "csc" => MatrixLayout::Compressed(CompressedAxis::Cols),
        _ => MatrixLayout::Dense,
    }
}

/// Loads (and caches) a string column of the `axis` dataframe: its index when `column` is `None`.
pub async fn load_axis_labels(store: Arc<dyn AsyncReadableStorageTraits>, store_name: &str, axis: MatrixAxis, column: Option<&str>, cache_enabled: bool) -> Result<Arc<Vec<String>>, ArrayError> {
    let keys = vec![
        "adata_dataframe_labels".to_string(),
        store_name.to_string(),
        axis.dataframe_path().to_string(),
        column.unwrap_or("index").to_string(),
    ];
    use_memo_vec_string(async || read_dataframe_column_strings(store, axis.dataframe_path(), column).await, &keys, cache_enabled).await
}

/// The position in `labels` of each of `identifiers`, in order, skipping (and
/// logging) unknown ones and repeats.
pub fn resolve_identifier_indices(identifiers: &[String], labels: &[String]) -> Vec<u32> {
    let mut lookup: HashMap<&str, u32> = HashMap::with_capacity(labels.len());
    for (index, label) in labels.iter().enumerate() {
        lookup.entry(label.as_str()).or_insert(index as u32);
    }
    let mut seen = std::collections::HashSet::new();
    let mut indices = Vec::with_capacity(identifiers.len());
    for identifier in identifiers {
        match lookup.get(identifier.as_str()) {
            Some(&index) if seen.insert(index) => indices.push(index),
            Some(_) => {}
            None => log(&format!("heatmap_data: identifier \"{identifier}\" not found; skipping")),
        }
    }
    indices
}

/// Loads (and caches) the indices along `axis` included by `criteria`: every
/// index, in order, when `criteria` is `None`.
pub async fn load_axis_indices(
    gpu_context: Option<&GpuContext<'_>>,
    store: Arc<dyn AsyncReadableStorageTraits>,
    store_name: &str,
    matrix_path: &str,
    axis: MatrixAxis,
    criteria: Option<&AxisCriteria>,
    cache_enabled: bool,
) -> Result<Arc<NumericData>, ArrayError> {
    let mut keys = vec![
        "adata_axis_indices".to_string(),
        store_name.to_string(),
        matrix_path.to_string(),
        axis.dataframe_path().to_string(),
    ];
    keys.extend(axis_criteria_key(criteria));

    use_memo_numeric_data(
        async || {
            let axis_len = load_matrix_shape(store.clone(), store_name, matrix_path, cache_enabled).await?[axis.shape_dim()];
            let indices: Vec<u32> = match criteria {
                None => (0..axis_len as u32).collect(),
                Some(AxisCriteria::Identifiers(params)) => {
                    let labels = load_axis_labels(store.clone(), store_name, axis, params.column.as_deref(), cache_enabled).await?;
                    resolve_identifier_indices(&params.identifiers, &labels)
                }
                Some(AxisCriteria::Criteria(criteria)) => {
                    let resolved = resolve_zarr_emphasis_criteria(store.clone(), criteria, store_name, cache_enabled).await?;
                    compute_included_indices(gpu_context, axis_len, &resolved, &[]).await.background
                }
            };
            Ok(NumericData::from(indices))
        },
        &keys,
        cache_enabled,
    )
    .await
}

/// One entry per `included` index: 1 if it is among `selected`, else 0.
pub fn selection_mask(included: &[u32], selected: &[u32]) -> Vec<u8> {
    let mut selected = selected.to_vec();
    selected.sort_unstable();
    included.iter().map(|index| selected.binary_search(index).is_ok() as u8).collect()
}

/// Loads (and caches) the selection mask over the `filtering`-included indices
/// along `axis`, or `None` when there is no `selection` (i.e. everything is selected).
pub async fn load_axis_selection_mask(
    gpu_context: Option<&GpuContext<'_>>,
    store: Arc<dyn AsyncReadableStorageTraits>,
    store_name: &str,
    matrix_path: &str,
    axis: MatrixAxis,
    filtering: Option<&AxisCriteria>,
    selection: Option<&AxisCriteria>,
    cache_enabled: bool,
) -> Result<Option<Arc<NumericData>>, ArrayError> {
    let Some(selection) = selection else {
        return Ok(None);
    };
    let mut keys = vec![
        "adata_axis_selection_mask".to_string(),
        store_name.to_string(),
        matrix_path.to_string(),
        axis.dataframe_path().to_string(),
    ];
    keys.extend(axis_criteria_key(filtering));
    keys.extend(axis_criteria_key(Some(selection)));

    let mask = use_memo_numeric_data(
        async || {
            let (included, selected) = futures::try_join!(
                load_axis_indices(gpu_context, store.clone(), store_name, matrix_path, axis, filtering, cache_enabled),
                load_axis_indices(gpu_context, store.clone(), store_name, matrix_path, axis, Some(selection), cache_enabled),
            )?;
            Ok::<_, ArrayError>(NumericData::from(selection_mask(indices_of(&included), indices_of(&selected))))
        },
        &keys,
        cache_enabled,
    )
    .await?;
    Ok(Some(mask))
}

/// How the loaded matrix values are cached.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum MatrixCacheMode {
    /// Cache each row block of the filtered submatrix as a whole. Loads only
    /// the filter-included cells, but a change to either axis's filter
    /// reloads every block.
    #[default]
    RowBlocks,
    /// Cache each var's whole column (every obs) independently, and assemble
    /// the row blocks from them. Changing the obs filter, or adding a var,
    /// only loads columns not already cached. Reading a column of a CSR
    /// matrix traverses the whole matrix.
    VarColumns,
}

/// How the quantitative colormap's domain is derived from the loaded values.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum HeatmapNormalization {
    /// One (min, max) over every loaded cell.
    #[default]
    Matrix,
    /// One (min, max) per obs, over that obs's included vars.
    PerObs,
    /// One (min, max) per var, over that var's included obs.
    PerVar,
}

/// Loads (and caches) one var's whole column of a dense matrix, in its
/// stored dtype. Keyed independently of any filtering.
pub async fn load_var_column(
    store: Arc<dyn AsyncReadableStorageTraits>,
    store_name: &str,
    matrix_path: &str,
    var_index: u32,
    cache_enabled: bool,
) -> Result<Arc<NumericData>, ArrayError> {
    let keys = vec!["adata_var_column".to_string(), store_name.to_string(), matrix_path.to_string(), var_index.to_string()];
    use_memo_numeric_data(
        async || read_dense_column_numeric(store, matrix_path, var_index as u64).await,
        &keys,
        cache_enabled,
    )
    .await
}

/// Loads (and caches) the stored entries of one var's whole column of a
/// sparse matrix, as a single-column CSC matrix. Keyed independently of any filtering.
pub async fn load_compressed_var_column(
    store: Arc<dyn AsyncReadableStorageTraits>,
    store_name: &str,
    matrix_path: &str,
    compressed_axis: CompressedAxis,
    var_index: u32,
    cache_enabled: bool,
) -> Result<Arc<CompressedMatrix>, ArrayError> {
    let keys = vec!["adata_compressed_var_column".to_string(), store_name.to_string(), matrix_path.to_string(), var_index.to_string()];
    use_memo_compressed_matrix(
        async || read_compressed_column(store, matrix_path, compressed_axis, var_index as u64).await,
        &keys,
        cache_enabled,
    )
    .await
}

fn gather_rows_as<T: NumericElement>(first: &[T], rest: &[Arc<NumericData>], rows: &[u32]) -> NumericData
where
    NumericData: From<Vec<T>>,
{
    let columns: Vec<&[T]> = std::iter::once(first)
        .chain(rest.iter().map(|column| T::slice_of(column).expect("matrix columns must share one dtype")))
        .collect();
    let mut out = Vec::with_capacity(rows.len() * columns.len());
    for &row in rows {
        out.extend(columns.iter().map(|column| column[row as usize]));
    }
    NumericData::from(out)
}

/// The row-major `rows x columns.len()` matrix gathered from whole columns,
/// in their stored dtype (which every column must share).
pub fn gather_rows_from_columns(columns: &[Arc<NumericData>], rows: &[u32]) -> NumericData {
    let Some(first) = columns.first() else {
        return NumericData::Float32(Arc::new(vec![]));
    };
    with_numeric_slice!(first.as_ref(), |first| gather_rows_as(first, &columns[1..], rows))
}

/// `(entry, row)` for each entry of `row_indices` whose row is requested.
fn requested_entries<I: Copy + TryInto<usize>>(row_indices: &[I], is_row_requested: &[bool]) -> Vec<(usize, u32)> {
    row_indices
        .iter()
        .enumerate()
        .filter_map(|(entry, &row)| {
            let row: usize = row.try_into().ok().expect("Sparse matrix row indices must be non-negative");
            is_row_requested[row].then_some((entry, row as u32))
        })
        .collect()
}

/// The values at the `kept` entries of each column, which must share the dtype of `first`.
fn gather_entries_as<T: NumericElement>(first: &[T], rest: &[Arc<CompressedMatrix>], kept: &[Vec<(usize, u32)>]) -> NumericData
where
    NumericData: From<Vec<T>>,
{
    let columns = std::iter::once(first).chain(rest.iter().map(|column| T::slice_of(&column.data).expect("matrix columns must share one dtype")));
    let mut out = Vec::with_capacity(kept.iter().map(Vec::len).sum());
    for (values, column_kept) in columns.zip(kept) {
        out.extend(column_kept.iter().map(|&(entry, _)| values[entry]));
    }
    NumericData::from(out)
}

/// The CSC matrix of `rows` (its `minor_subset`) and single-column `columns`,
/// holding only the entries of requested rows, in their stored dtype (which
/// every column must share). `rows` must not contain duplicates.
pub fn gather_compressed_columns(columns: &[Arc<CompressedMatrix>], rows: &[u32], n_obs: usize) -> CompressedMatrix {
    let mut is_row_requested = vec![false; n_obs];
    for &row in rows {
        is_row_requested[row as usize] = true;
    }
    let kept: Vec<Vec<(usize, u32)>> = columns
        .iter()
        .map(|column| {
            let mut entries = with_int_slice!(&column.indices, |row_indices| requested_entries(row_indices, &is_row_requested));
            // Non-canonical matrices may store a column's entries out of order.
            entries.sort_unstable_by_key(|&(_, row)| row);
            entries
        })
        .collect();

    let mut indptr = Vec::with_capacity(columns.len() + 1);
    indptr.push(0u32);
    for column_kept in &kept {
        indptr.push(indptr[indptr.len() - 1] + column_kept.len() as u32);
    }
    let indices: Vec<u32> = kept.iter().flatten().map(|&(_, row)| row).collect();

    let data = match columns.first() {
        None => NumericData::Float32(Arc::new(vec![])),
        Some(first) => with_numeric_slice!(&first.data, |first| gather_entries_as(first, &columns[1..], &kept)),
    };
    CompressedMatrix {
        indptr: NumericData::from(indptr),
        indices: NumericData::from(indices),
        data,
        minor_subset: Some(NumericData::from(rows.to_vec())),
    }
}

/// Identifies one row block of a filtered submatrix, for memoization.
pub struct MatrixBlockKey<'a> {
    pub store_name: &'a str,
    pub matrix_path: &'a str,
    pub cache_mode: MatrixCacheMode,
    pub obs_filtering: Option<&'a AxisCriteria>,
    pub var_filtering: Option<&'a AxisCriteria>,
    pub rows_per_block: usize,
    pub block_index: usize,
}

impl MatrixBlockKey<'_> {
    fn keys(&self, namespace: &str) -> Vec<String> {
        let mut keys = vec![
            namespace.to_string(),
            self.store_name.to_string(),
            self.matrix_path.to_string(),
            format!("{:?}", self.cache_mode),
        ];
        keys.extend(axis_criteria_key(self.obs_filtering));
        keys.extend(axis_criteria_key(self.var_filtering));
        keys.push(self.rows_per_block.to_string());
        keys.push(self.block_index.to_string());
        keys
    }
}

/// One row block of a filtered submatrix: dense, or sparse in the stored
/// compression (CSC when assembled from [`MatrixCacheMode::VarColumns`]).
#[derive(Clone, Debug)]
pub enum MatrixBlock {
    /// Row-major values.
    Dense(Arc<NumericData>),
    Compressed(CompressedAxis, Arc<CompressedMatrix>),
}

/// Loads (and caches) the `rows x cols` block of the matrix identified by
/// `key`, stored as `layout`, in its stored dtype. Sparse matrices stay sparse.
pub async fn load_matrix_block(
    store: Arc<dyn AsyncReadableStorageTraits>,
    key: &MatrixBlockKey<'_>,
    layout: MatrixLayout,
    rows: &[u32],
    cols: &[u32],
    cache_enabled: bool,
) -> Result<MatrixBlock, ArrayError> {
    let keys = key.keys("adata_matrix_block");
    // `join_all` rather than `try_join_all`, so that a column still loading
    // does not cancel the others, which are then cached for the next attempt.
    match (layout, key.cache_mode) {
        (MatrixLayout::Dense, MatrixCacheMode::RowBlocks) => {
            let block = use_memo_numeric_data(async || read_dense_subset_numeric(store, key.matrix_path, rows, cols).await, &keys, cache_enabled).await?;
            Ok(MatrixBlock::Dense(block))
        }
        (MatrixLayout::Dense, MatrixCacheMode::VarColumns) => {
            let block = use_memo_numeric_data(
                async || {
                    let columns = futures::future::join_all(
                        cols.iter().map(|&var_index| load_var_column(store.clone(), key.store_name, key.matrix_path, var_index, cache_enabled)),
                    )
                    .await
                    .into_iter()
                    .collect::<Result<Vec<_>, _>>()?;
                    Ok::<_, ArrayError>(gather_rows_from_columns(&columns, rows))
                },
                &keys,
                cache_enabled,
            )
            .await?;
            Ok(MatrixBlock::Dense(block))
        }
        (MatrixLayout::Compressed(axis), MatrixCacheMode::RowBlocks) => {
            let block = use_memo_compressed_matrix(
                async || read_compressed_subset(store, key.matrix_path, axis, rows, cols).await,
                &keys,
                cache_enabled,
            )
            .await?;
            Ok(MatrixBlock::Compressed(axis, block))
        }
        (MatrixLayout::Compressed(axis), MatrixCacheMode::VarColumns) => {
            let block = use_memo_compressed_matrix(
                async || {
                    let n_obs = load_matrix_shape(store.clone(), key.store_name, key.matrix_path, cache_enabled).await?[0];
                    let columns = futures::future::join_all(cols.iter().map(|&var_index| {
                        load_compressed_var_column(store.clone(), key.store_name, key.matrix_path, axis, var_index, cache_enabled)
                    }))
                    .await
                    .into_iter()
                    .collect::<Result<Vec<_>, _>>()?;
                    Ok::<_, ArrayError>(gather_compressed_columns(&columns, rows, n_obs))
                },
                &keys,
                cache_enabled,
            )
            .await?;
            Ok(MatrixBlock::Compressed(CompressedAxis::Cols, block))
        }
    }
}

/// Computes (and caches) the `(min, max)` of the `num_rows x num_cols` block
/// identified by `key`, counting a sparse block's implicit zeros.
pub async fn load_matrix_block_extent(
    gpu_context: Option<&GpuContext<'_>>,
    key: &MatrixBlockKey<'_>,
    block: &MatrixBlock,
    num_rows: usize,
    num_cols: usize,
    cache_enabled: bool,
) -> (f32, f32) {
    let extent = use_memo_vec_f32(
        async || {
            let (min, max) = match block {
                MatrixBlock::Dense(values) => reduce_extent(gpu_context, values.as_ref().clone(), &[], &[]).await.background,
                MatrixBlock::Compressed(_, matrix) => {
                    let (min, max) = reduce_extent(gpu_context, matrix.data.clone(), &[], &[]).await.background;
                    if matrix.data.len() < num_rows * num_cols { (min.min(0.0), max.max(0.0)) } else { (min, max) }
                }
            };
            Ok::<_, std::convert::Infallible>(vec![min, max])
        },
        &key.keys("adata_matrix_block_extent"),
        cache_enabled,
    )
    .await
    .expect("infallible");
    (extent[0], extent[1])
}

/// Computes (and caches) the `(mins, maxs)` of every row or column of the
/// `num_rows x num_cols` block identified by `key`.
pub async fn load_matrix_block_axis_extents(
    gpu_context: Option<&GpuContext<'_>>,
    key: &MatrixBlockKey<'_>,
    block: &MatrixBlock,
    num_rows: usize,
    num_cols: usize,
    axis: HeatmapAxis,
    cache_enabled: bool,
) -> (Vec<f32>, Vec<f32>) {
    let namespace = format!("adata_matrix_block_{axis:?}_extents");
    let extents = use_memo_vec_f32(
        async || {
            let (mut mins, maxs) = match block {
                MatrixBlock::Dense(values) => reduce_matrix_axis_extents(gpu_context, values, num_rows, num_cols, axis).await,
                MatrixBlock::Compressed(compressed_axis, matrix) => {
                    reduce_compressed_matrix_axis_extents(gpu_context, matrix, *compressed_axis, num_rows, num_cols, axis).await
                }
            };
            mins.extend(maxs);
            Ok::<_, std::convert::Infallible>(mins)
        },
        &key.keys(&namespace),
        cache_enabled,
    )
    .await
    .expect("infallible");
    let (mins, maxs) = extents.split_at(extents.len() / 2);
    (mins.to_vec(), maxs.to_vec())
}

/// The most bins the histogram reductions support, which percentile searches use.
const PERCENTILE_NUM_BINS: u32 = 256;
/// Each pass after the first narrows a percentile's range by up to `PERCENTILE_NUM_BINS` times.
const PERCENTILE_NUM_PASSES: usize = 2;

fn range_keys(ranges: &[(f32, f32)]) -> impl Iterator<Item = String> + '_ {
    ranges.iter().flat_map(|&(lo, hi)| [lo.to_bits().to_string(), hi.to_bits().to_string()])
}

fn counts_of(data: &NumericData) -> &[u32] {
    match data {
        NumericData::Uint32(counts) => counts,
        _ => unreachable!("histograms are memoized as Uint32"),
    }
}

/// One loaded row block of the filtered matrix.
pub struct LoadedMatrixBlock<'a> {
    pub key: MatrixBlockKey<'a>,
    pub block: &'a MatrixBlock,
    pub num_rows: usize,
}

/// Computes (and caches) a `num_bins`-bin histogram over `range` of every
/// value of the `num_rows x num_cols` block identified by `key`, counting a
/// sparse block's implicit zeros.
pub async fn load_matrix_block_histogram(
    gpu_context: Option<&GpuContext<'_>>,
    key: &MatrixBlockKey<'_>,
    block: &MatrixBlock,
    num_rows: usize,
    num_cols: usize,
    range: (f32, f32),
    num_bins: u32,
    cache_enabled: bool,
) -> Arc<NumericData> {
    let mut keys = key.keys("adata_matrix_block_histogram");
    keys.push(num_bins.to_string());
    keys.extend(range_keys(&[range]));
    use_memo_numeric_data(
        async || {
            let (lo, hi) = range;
            let counts = match block {
                MatrixBlock::Dense(values) => reduce_histogram_with_known_extent(gpu_context, values.as_ref().clone(), num_bins, lo, hi, &[], &[]).await.background,
                MatrixBlock::Compressed(_, matrix) => {
                    let mut counts = reduce_histogram_with_known_extent(gpu_context, matrix.data.clone(), num_bins, lo, hi, &[], &[]).await.background;
                    counts[histogram_bin(0.0, lo, hi, num_bins)] += (num_rows * num_cols - matrix.data.len()) as u32;
                    counts
                }
            };
            Ok::<_, std::convert::Infallible>(NumericData::from(counts))
        },
        &keys,
        cache_enabled,
    )
    .await
    .expect("infallible")
}

/// Computes (and caches) a `num_bins`-bin histogram of every row or column
/// of the `num_rows x num_cols` block identified by `key`, each over its own `ranges` entry.
#[allow(clippy::too_many_arguments)]
pub async fn load_matrix_block_axis_histograms(
    gpu_context: Option<&GpuContext<'_>>,
    key: &MatrixBlockKey<'_>,
    block: &MatrixBlock,
    num_rows: usize,
    num_cols: usize,
    axis: HeatmapAxis,
    ranges: &[(f32, f32)],
    num_bins: u32,
    cache_enabled: bool,
) -> Arc<NumericData> {
    let mut keys = key.keys(&format!("adata_matrix_block_{axis:?}_histograms"));
    keys.push(num_bins.to_string());
    keys.extend(range_keys(ranges));
    use_memo_numeric_data(
        async || {
            let counts = match block {
                MatrixBlock::Dense(values) => reduce_matrix_axis_histograms(gpu_context, values, num_rows, num_cols, axis, ranges, num_bins).await,
                MatrixBlock::Compressed(compressed_axis, matrix) => {
                    reduce_compressed_matrix_axis_histograms(gpu_context, matrix, *compressed_axis, num_rows, num_cols, axis, ranges, num_bins).await
                }
            };
            Ok::<_, std::convert::Infallible>(NumericData::from(counts))
        },
        &keys,
        cache_enabled,
    )
    .await
    .expect("infallible")
}

async fn summed_block_histogram(
    gpu_context: Option<&GpuContext<'_>>,
    blocks: &[LoadedMatrixBlock<'_>],
    num_cols: usize,
    range: (f32, f32),
    cache_enabled: bool,
) -> Vec<u32> {
    let mut counts = vec![0u32; PERCENTILE_NUM_BINS as usize];
    for loaded in blocks {
        let block_counts = load_matrix_block_histogram(gpu_context, &loaded.key, loaded.block, loaded.num_rows, num_cols, range, PERCENTILE_NUM_BINS, cache_enabled).await;
        counts.iter_mut().zip(counts_of(&block_counts)).for_each(|(count, block_count)| *count += block_count);
    }
    counts
}

async fn matrix_percentile(
    gpu_context: Option<&GpuContext<'_>>,
    blocks: &[LoadedMatrixBlock<'_>],
    num_cols: usize,
    extent: (f32, f32),
    percentile: f32,
    cache_enabled: bool,
) -> f32 {
    if percentile <= 0.0 {
        return extent.0;
    }
    if percentile >= 100.0 {
        return extent.1;
    }
    let mut search = PercentileSearch::new(extent, percentile);
    for _ in 1..PERCENTILE_NUM_PASSES {
        search.narrow(&summed_block_histogram(gpu_context, blocks, num_cols, search.range(), cache_enabled).await);
    }
    search.estimate(&summed_block_histogram(gpu_context, blocks, num_cols, search.range(), cache_enabled).await)
}

/// The `(lower, upper)` percentiles of every value of `blocks`, whose `(min, max)` is `extent`.
pub async fn load_matrix_percentile_domain(
    gpu_context: Option<&GpuContext<'_>>,
    blocks: &[LoadedMatrixBlock<'_>],
    num_cols: usize,
    extent: (f32, f32),
    (lower, upper): (f32, f32),
    cache_enabled: bool,
) -> (f32, f32) {
    (
        matrix_percentile(gpu_context, blocks, num_cols, extent, lower, cache_enabled).await,
        matrix_percentile(gpu_context, blocks, num_cols, extent, upper, cache_enabled).await,
    )
}

#[allow(clippy::too_many_arguments)]
async fn summed_block_axis_histograms(
    gpu_context: Option<&GpuContext<'_>>,
    blocks: &[LoadedMatrixBlock<'_>],
    num_cols: usize,
    axis: HeatmapAxis,
    searches: &[PercentileSearch],
    num_bins: u32,
    cache_enabled: bool,
) -> Vec<u32> {
    let ranges: Vec<(f32, f32)> = searches.iter().map(PercentileSearch::range).collect();
    let mut counts = vec![0u32; ranges.len() * num_bins as usize];
    for loaded in blocks {
        let block_counts =
            load_matrix_block_axis_histograms(gpu_context, &loaded.key, loaded.block, loaded.num_rows, num_cols, axis, &ranges, num_bins, cache_enabled).await;
        counts.iter_mut().zip(counts_of(&block_counts)).for_each(|(count, block_count)| *count += block_count);
    }
    counts
}

#[allow(clippy::too_many_arguments)]
async fn axis_percentiles(
    gpu_context: Option<&GpuContext<'_>>,
    blocks: &[LoadedMatrixBlock<'_>],
    num_cols: usize,
    axis: HeatmapAxis,
    (mins, maxs): (&[f32], &[f32]),
    percentile: f32,
    num_bins: u32,
    cache_enabled: bool,
) -> Vec<f32> {
    if percentile <= 0.0 {
        return mins.to_vec();
    }
    if percentile >= 100.0 {
        return maxs.to_vec();
    }
    let mut searches: Vec<PercentileSearch> = mins.iter().zip(maxs).map(|(&lo, &hi)| PercentileSearch::new((lo, hi), percentile)).collect();
    for _ in 1..PERCENTILE_NUM_PASSES {
        let counts = summed_block_axis_histograms(gpu_context, blocks, num_cols, axis, &searches, num_bins, cache_enabled).await;
        searches.iter_mut().zip(counts.chunks(num_bins as usize)).for_each(|(search, counts)| search.narrow(counts));
    }
    let counts = summed_block_axis_histograms(gpu_context, blocks, num_cols, axis, &searches, num_bins, cache_enabled).await;
    searches.iter().zip(counts.chunks(num_bins as usize)).map(|(search, counts)| search.estimate(counts)).collect()
}

/// The `(lower, upper)` percentiles of every row or column of `blocks`, whose
/// `(mins, maxs)` are given. Every block's histograms are summed, so for
/// [`HeatmapAxis::Rows`], `blocks` must hold a single block.
#[allow(clippy::too_many_arguments)]
pub async fn load_matrix_axis_percentile_domains(
    gpu_context: Option<&GpuContext<'_>>,
    blocks: &[LoadedMatrixBlock<'_>],
    num_cols: usize,
    axis: HeatmapAxis,
    extents: (&[f32], &[f32]),
    (lower, upper): (f32, f32),
    cache_enabled: bool,
) -> (Vec<f32>, Vec<f32>) {
    let values_per_segment = if axis == HeatmapAxis::Rows { num_cols } else { blocks.iter().map(|loaded| loaded.num_rows).sum() };
    let num_bins = PERCENTILE_NUM_BINS.min(values_per_segment as u32).max(1);
    (
        axis_percentiles(gpu_context, blocks, num_cols, axis, extents, lower, num_bins, cache_enabled).await,
        axis_percentiles(gpu_context, blocks, num_cols, axis, extents, upper, num_bins, cache_enabled).await,
    )
}
