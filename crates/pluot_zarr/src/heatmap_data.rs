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

use pluot_core::cache::{use_memo_numeric_data, use_memo_vec_f32, use_memo_vec_string};
use pluot_core::compute::included_indices::compute_included_indices;
use pluot_core::compute::reduce::reduce_extent;
use pluot_core::log;
use pluot_core::numeric_data::NumericData;
use pluot_core::render_types::GpuContext;

use crate::adata_io::{read_dataframe_column_strings, read_matrix_shape, read_matrix_subset_numeric};
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

/// Identifies one row block of a filtered submatrix, for memoization.
pub struct MatrixBlockKey<'a> {
    pub store_name: &'a str,
    pub matrix_path: &'a str,
    pub obs_filtering: Option<&'a AxisCriteria>,
    pub var_filtering: Option<&'a AxisCriteria>,
    pub rows_per_block: usize,
    pub block_index: usize,
}

impl MatrixBlockKey<'_> {
    fn keys(&self, namespace: &str) -> Vec<String> {
        let mut keys = vec![namespace.to_string(), self.store_name.to_string(), self.matrix_path.to_string()];
        keys.extend(axis_criteria_key(self.obs_filtering));
        keys.extend(axis_criteria_key(self.var_filtering));
        keys.push(self.rows_per_block.to_string());
        keys.push(self.block_index.to_string());
        keys
    }
}

/// Loads (and caches) the row-major `rows x cols` block of the matrix
/// identified by `key`, in its stored dtype.
pub async fn load_matrix_block(
    store: Arc<dyn AsyncReadableStorageTraits>,
    key: &MatrixBlockKey<'_>,
    rows: &[u32],
    cols: &[u32],
    cache_enabled: bool,
) -> Result<Arc<NumericData>, ArrayError> {
    use_memo_numeric_data(
        async || read_matrix_subset_numeric(store, key.matrix_path, rows, cols).await,
        &key.keys("adata_matrix_block"),
        cache_enabled,
    )
    .await
}

/// Computes (and caches) the `(min, max)` of the block identified by `key`.
pub async fn load_matrix_block_extent(
    gpu_context: Option<&GpuContext<'_>>,
    key: &MatrixBlockKey<'_>,
    block: &NumericData,
    cache_enabled: bool,
) -> (f32, f32) {
    let extent = use_memo_vec_f32(
        async || {
            let (min, max) = reduce_extent(gpu_context, block.clone(), &[], &[]).await.background;
            Ok::<_, std::convert::Infallible>(vec![min, max])
        },
        &key.keys("adata_matrix_block_extent"),
        cache_enabled,
    )
    .await
    .expect("infallible");
    (extent[0], extent[1])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zarr_emphasis_criteria::ZarrQuantitativeCriteriaParams;

    #[test]
    fn identifiers_resolve_in_order_skipping_unknown_and_repeats() {
        let labels: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        let requested: Vec<String> = ["c", "x", "a", "c"].iter().map(|s| s.to_string()).collect();
        assert_eq!(resolve_identifier_indices(&requested, &labels), vec![2, 0]);
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
}
