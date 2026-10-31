// Loads gene x obs-category expression summaries from an AnnData-Zarr store.
//
// Every loaded/computed piece is cached independently (var names, the obs groupby columns, each
// gene's expression column, each gene's per-category summaries), so that none of them block or
// invalidate one another: changing one gene does not require re-fetching or recomputing
// anything for the others. Per-category reductions run on the GPU when a GPU context is given.

use std::collections::HashMap;
use std::sync::Arc;

use pluot_core::cache::{use_memo_numeric_data, use_memo_vec_f32, use_memo_vec_string};
use pluot_core::compute::reduce_stratified::{reduce_stratified_count, reduce_stratified_sum_and_count, StratifyBy};
use pluot_core::compute::ForegroundBackground;
use pluot_core::log;
use pluot_core::numeric_data::NumericData;
use pluot_core::render_traits::{EmphasisCriteria, QuantitativeCriteriaParams};
use pluot_core::render_types::GpuContext;
use pluot_core::zarr::is_timed_out_zarrs_error;
use zarrs::array::ArrayError;
use zarrs::storage::AsyncReadableStorageTraits;

use crate::adata_io::{read_matrix_column_f32, read_string_array};
use crate::adata_paths::{node_path, path_name};
use crate::heatmap_data::{axis_criteria_key, index_mask, load_axis_len, AdataAxis, AxisCriteria};
use crate::memo::use_memo_shared;
use crate::zarr_numeric_data::load_arr_as_numeric_data;

/// The store holding an AnnData object, possibly nested within a MuData or SpatialData object.
pub type AdataStore = Arc<dyn AsyncReadableStorageTraits>;

/// One gene's expression summary over one combination of obs categories' cells.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GeneSummary {
    /// The gene's ID, as it appears in the var labels it was resolved from.
    pub var_name: String,
    /// The combination of categories (one per `groupby` column) this summary was computed over.
    pub obs_values: Vec<String>,

    /// Sum of expression values across the category's cells.
    pub sum: f32,
    /// Number of cells in the category.
    pub cell_count: u32,
    /// Number of the category's cells with expression above the cutoff.
    pub num_expressing: u32,
}

impl GeneSummary {
    /// Mean expression and fraction of cells expressing, over the category's cells.
    /// A category with no cells (`cell_count == 0`) summarizes to `(0.0, 0.0)` rather than NaN.
    pub fn mean_and_fraction_expressing(&self) -> (f32, f32) {
        if self.cell_count > 0 {
            (self.sum / self.cell_count as f32, self.num_expressing as f32 / self.cell_count as f32)
        } else {
            (0.0, 0.0)
        }
    }
}

// ---------------------------------------------------------------------------
// Independent single-element loads
// ---------------------------------------------------------------------------

/// Loads (and caches) the category labels and per-observation codes of the
/// categorical column at `column_path`. The two arrays are cached independently of each other.
pub async fn load_categorical(store: AdataStore, store_name: &str, column_path: &str, cache_enabled: bool) -> Result<(Arc<Vec<String>>, Arc<NumericData>), ArrayError> {
    let column_path = node_path(column_path);

    let categories_keys = vec!["adata_categories".to_string(), store_name.to_string(), column_path.clone()];
    let categories_future = use_memo_vec_string(
        async || read_string_array(store.clone(), &format!("{column_path}/categories")).await,
        &categories_keys,
        cache_enabled,
    );

    let codes_keys = vec!["adata_codes".to_string(), store_name.to_string(), column_path.clone()];
    let codes_future = use_memo_numeric_data(
        async || load_arr_as_numeric_data(store.clone(), &format!("{column_path}/codes")).await,
        &codes_keys,
        cache_enabled,
    );

    futures::try_join!(categories_future, codes_future)
}

/// Loads (and caches) the column at `col_index` of the expression matrix at `matrix_path`.
pub async fn load_gene_expression(store: AdataStore, store_name: &str, col_index: u64, matrix_path: &str, cache_enabled: bool) -> Result<Arc<Vec<f32>>, ArrayError> {
    let array_path = node_path(matrix_path);
    let expr_keys = vec!["adata_matrix_column".to_string(), store_name.to_string(), array_path.clone(), col_index.to_string()];
    // Whether the matrix is dense, CSR or CSC is the reader's business, not ours.
    use_memo_vec_f32(async || read_matrix_column_f32(store, &array_path, col_index).await, &expr_keys, cache_enabled).await
}

/// The combinations of the categories of one or more categorical `obs` columns, from coarsest to finest.
pub struct NestedCategories {
    pub categories_per_column: Vec<Arc<Vec<String>>>,
    /// One code per obs: the position of its combination among [`NestedCategories::combinations`],
    /// or -1 when it is missing a category in any column.
    pub codes: Arc<NumericData>,
}

impl NestedCategories {
    fn num_combinations(&self) -> usize {
        self.categories_per_column.iter().map(|categories| categories.len()).product()
    }

    /// Every combination of categories, ordered by the first column's categories, then the second's, etc.
    pub fn combinations(&self) -> Vec<Vec<String>> {
        (0..self.num_combinations()).map(|code| self.combination(code)).collect()
    }

    fn combination(&self, mut code: usize) -> Vec<String> {
        let mut combination = vec![String::new(); self.categories_per_column.len()];
        for (category, categories) in combination.iter_mut().zip(&self.categories_per_column).rev() {
            *category = categories[code % categories.len()].clone();
            code /= categories.len();
        }
        combination
    }

    /// The code of `combination`, or `None` when any of its categories is unknown.
    pub fn code_of(&self, combination: &[String]) -> Option<i64> {
        if combination.len() != self.categories_per_column.len() {
            return None;
        }
        combination.iter().zip(&self.categories_per_column).try_fold(0i64, |code, (category, categories)| {
            let column_code = categories.iter().position(|c| c == category)?;
            Some(code * categories.len() as i64 + column_code as i64)
        })
    }

    /// The code of the first column's category in `combination`, if any.
    pub fn first_column_code(&self, combination: &[String]) -> Option<usize> {
        let (first_categories, first_category) = self.categories_per_column.first().zip(combination.first())?;
        first_categories.iter().position(|c| c == first_category)
    }
}

/// The text labeling a combination of categories, e.g. on an axis.
pub fn combination_label(combination: &[String]) -> String {
    combination.join(" / ")
}

/// The text titling an axis of the combinations of categories of the `groupby` columns.
pub fn groupby_title(groupby: &[String]) -> String {
    groupby.iter().map(|column_path| path_name(column_path)).collect::<Vec<_>>().join(" / ")
}

/// The combination code of each row of `codes_per_column` (see [`NestedCategories::codes`]).
pub fn combine_codes(codes_per_column: &[&NumericData], num_categories: &[usize]) -> Vec<i32> {
    let columns: Vec<_> = codes_per_column.iter().map(|codes| codes.as_f32()).collect();
    let num_rows = columns.iter().map(|codes| codes.len()).min().unwrap_or(0);
    (0..num_rows)
        .map(|row| {
            columns.iter().zip(num_categories).try_fold(0i32, |code, (codes, &n)| {
                let column_code = codes[row];
                (column_code >= 0.0).then(|| code * n as i32 + column_code as i32)
            })
            .unwrap_or(-1)
        })
        .collect()
}

/// Loads (and caches) the [`NestedCategories`] of the `groupby` columns (each a path to a categorical
/// column of the `obs_axis` dataframe). Without any column, every obs has the one empty combination.
pub async fn load_nested_categories(
    store: AdataStore,
    store_name: &str,
    obs_axis: &AdataAxis,
    groupby: &[String],
    cache_enabled: bool,
) -> Result<NestedCategories, ArrayError> {
    let columns = futures::future::try_join_all(groupby.iter().map(|column_path| load_categorical(store.clone(), store_name, column_path, cache_enabled))).await?;
    let (categories_per_column, codes_per_column): (Vec<_>, Vec<_>) = columns.into_iter().unzip();
    if let [codes] = codes_per_column.as_slice() {
        return Ok(NestedCategories { categories_per_column, codes: codes.clone() });
    }
    let mut keys = vec!["adata_nested_codes".to_string(), store_name.to_string(), obs_axis.dataframe_path.clone(), groupby.len().to_string()];
    keys.extend(groupby.iter().cloned());
    let codes = use_memo_numeric_data(
        async || {
            if groupby.is_empty() {
                let num_obs = load_axis_len(store.clone(), store_name, obs_axis, cache_enabled).await?;
                return Ok::<_, ArrayError>(NumericData::from(vec![0i32; num_obs]));
            }
            let codes: Vec<&NumericData> = codes_per_column.iter().map(Arc::as_ref).collect();
            let num_categories: Vec<usize> = categories_per_column.iter().map(|categories| categories.len()).collect();
            Ok(NumericData::from(combine_codes(&codes, &num_categories)))
        },
        &keys,
        cache_enabled,
    )
    .await?;
    Ok(NestedCategories { categories_per_column, codes })
}

// ---------------------------------------------------------------------------
// Pure helpers (no I/O), factored out for direct unit testing
// ---------------------------------------------------------------------------

/// Resolves each requested gene ID to its column index in `var_index_values`, logging (and
/// skipping) any that aren't found.
pub fn resolve_gene_columns(requested: &[String], var_index_values: &[String]) -> Vec<(String, u64)> {
    let lookup: HashMap<&str, usize> = var_index_values.iter().enumerate().map(|(i, name)| (name.as_str(), i)).collect();
    let mut resolved = Vec::new();
    for gene in requested {
        match lookup.get(gene.as_str()) {
            Some(&index) => resolved.push((gene.clone(), index as u64)),
            None => log(&format!("dotplot_data: gene \"{gene}\" not found in var names; skipping")),
        }
    }
    resolved
}

/// Whether each of `genes`, looked up in `var_index_values`, is among the var `indices`. Unknown genes are not.
pub fn genes_among(genes: &[String], var_index_values: &[String], indices: &[u32]) -> Vec<bool> {
    let mask = index_mask(indices, var_index_values.len());
    let lookup: HashMap<&str, usize> = var_index_values.iter().enumerate().map(|(i, name)| (name.as_str(), i)).collect();
    genes.iter().map(|gene| lookup.get(gene.as_str()).is_some_and(|&index| mask[index])).collect()
}

/// The `genes` among the var `included` indices (see [`genes_among`]), in order, or every one when `included` is `None`.
pub fn filter_genes(genes: &[String], var_index_values: &[String], included: Option<&[u32]>) -> Vec<String> {
    let Some(included) = included else {
        return genes.to_vec();
    };
    genes.iter().zip(genes_among(genes, var_index_values, included)).filter(|(_, kept)| *kept).map(|(gene, _)| gene.clone()).collect()
}

/// The code of each of `requested_categories` in `nested`, as strata to reduce over.
/// Unknown combinations are logged, and given distinct codes below -1 that no obs has.
pub fn category_strata(requested_categories: &[Vec<String>], nested: &NestedCategories) -> StratifyBy {
    let mut num_unknown = 0;
    let included_codes = requested_categories
        .iter()
        .map(|combination| {
            nested.code_of(combination).unwrap_or_else(|| {
                log(&format!("dotplot_data: obs categories {combination:?} not found; skipping"));
                num_unknown += 1;
                -1 - num_unknown
            })
        })
        .collect();
    StratifyBy { codes: nested.codes.as_ref().clone(), included_codes }
}

/// Cache key parts identifying the obs rows that are summarized: those included by
/// `filtering`, and among them, those included by `selection`.
pub fn obs_rows_key(filtering: Option<&AxisCriteria>, selection: Option<&AxisCriteria>) -> Vec<String> {
    let mut keys = axis_criteria_key(filtering);
    keys.extend(axis_criteria_key(selection));
    keys
}

/// Cache key parts identifying `groupby` and the combinations of its `categories`.
pub fn categories_key(groupby: &[String], categories: &[Vec<String>]) -> Vec<String> {
    let mut keys = vec![groupby.len().to_string()];
    keys.extend(groupby.iter().cloned());
    keys.push(categories.len().to_string());
    keys.extend(categories.iter().flatten().cloned());
    keys
}

// ---------------------------------------------------------------------------
// Entry points
// ---------------------------------------------------------------------------

/// The obs to summarize per combination of categories: those meeting `filtering_criteria`
/// (the background), and among them those meeting `selection_criteria` (the foreground).
pub struct CategoryObs<'a> {
    pub categories: &'a [Vec<String>],
    /// One stratum per combination of categories (see [`category_strata`]).
    pub strata: &'a StratifyBy,
    pub filtering_criteria: &'a [EmphasisCriteria],
    pub selection_criteria: &'a [EmphasisCriteria],
    /// Cache key parts identifying the categories and both criteria (see [`categories_key`] and [`obs_rows_key`]).
    pub key: &'a [String],
}

/// The combinations of categories to plot: the `requested` ones, or else every one, in nested
/// order. When `is_filtered`, or with more than one `groupby` column, those without any obs
/// included by `filtering_criteria` (which `criteria_key` identifies) are omitted, counting (and
/// caching) each combination's included obs.
#[allow(clippy::too_many_arguments)]
pub async fn plotted_categories(
    gpu_context: Option<&GpuContext<'_>>,
    store_name: &str,
    groupby: &[String],
    requested: Option<&[Vec<String>]>,
    nested: &NestedCategories,
    is_filtered: bool,
    filtering_criteria: &[EmphasisCriteria],
    criteria_key: &[String],
    cache_enabled: bool,
) -> Vec<Vec<String>> {
    let categories = requested.map_or_else(|| nested.combinations(), <[_]>::to_vec);
    if !is_filtered && groupby.len() <= 1 {
        return categories;
    }
    let mut keys = vec!["dotplot_category_counts".to_string(), store_name.to_string()];
    keys.extend(categories_key(groupby, &categories));
    keys.extend(criteria_key.iter().cloned());
    let counts = use_memo_vec_f32(
        async || {
            let strata = category_strata(&categories, nested);
            let counts = reduce_stratified_count(gpu_context, nested.codes.as_ref().clone(), &strata, filtering_criteria, &[]).await;
            Ok::<_, std::convert::Infallible>(counts.background)
        },
        &keys,
        cache_enabled,
    )
    .await
    .expect("infallible");
    categories.into_iter().zip(counts.iter()).filter(|(_, &count)| count > 0.0).map(|(category, _)| category).collect()
}

/// Each category's background and foreground summaries of one gene.
pub type GeneSummaries = ForegroundBackground<Vec<GeneSummary>>;

/// Loads one gene's [`GeneSummaries`], or `None` when its data isn't loaded *yet*, rather than
/// propagating an error.
///
/// This is the independent, per-gene unit of work: it loads (and caches) just this gene's array
/// column and then computes (and caches) its per-category summaries. Under `wait_for_store_gets:
/// false`, a chunk not already in memory returns a "timed out" zarrs error immediately while the
/// store fetches it in the background, which is an expected, transient condition (see
/// `is_timed_out_zarrs_error`), not a failure — the same distinction other layers in this crate
/// make (e.g. `ZarrHistogramLayer::prepare`). Any other zarrs error is a genuine, unexpected
/// failure and panics, matching that same convention. Because a pending gene never propagates an
/// error, many of these can run concurrently (e.g. via `join_all`) without one slow or pending
/// gene cancelling or blocking the others, so a caller can render whichever genes have loaded so
/// far (see `AdataZarrDotPlotLayer::prepare` for an example).
#[allow(clippy::too_many_arguments)]
pub async fn load_gene_summaries_for_gene(
    gpu_context: Option<&GpuContext<'_>>,
    store: AdataStore,
    store_name: &str,
    matrix_path: &str,
    gene_name: &str,
    col_index: u64,
    obs: &CategoryObs<'_>,
    expression_cutoff: f32,
    cache_enabled: bool,
) -> Option<Arc<GeneSummaries>> {
    let expr_values = match load_gene_expression(store, store_name, col_index, matrix_path, cache_enabled).await {
        Ok(expr) => expr,
        Err(error) => {
            if is_timed_out_zarrs_error(&error) {
                return None;
            }
            panic!("Zarrs error loading AnnData expression column for gene \"{gene_name}\" at \"{matrix_path}\": {error:?}");
        }
    };

    let mut keys = vec![
        "dotplot_gene_summaries".to_string(),
        store_name.to_string(),
        node_path(matrix_path),
        col_index.to_string(),
        gene_name.to_string(),
        expression_cutoff.to_string(),
    ];
    keys.extend(obs.key.iter().cloned());
    let summaries = use_memo_shared(
        async || {
            let values = NumericData::Float32(expr_values.clone());
            let is_expressing = EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
                values: values.clone(),
                min: Some(expression_cutoff),
                max: None,
                min_exclusive: Some(true),
                max_exclusive: None,
            });
            let expressing_criteria = [obs.filtering_criteria, &[is_expressing]].concat();
            let sums_and_counts = reduce_stratified_sum_and_count(gpu_context, values.clone(), obs.strata, obs.filtering_criteria, obs.selection_criteria).await;
            let num_expressing = reduce_stratified_count(gpu_context, values, obs.strata, &expressing_criteria, obs.selection_criteria).await;
            let summarize = |sums_and_counts: &[(f32, u32)], num_expressing: &[f32]| {
                obs.categories
                    .iter()
                    .zip(sums_and_counts.iter().zip(num_expressing))
                    .map(|(combination, (&(sum, cell_count), &num_expressing))| GeneSummary {
                        var_name: gene_name.to_string(),
                        obs_values: combination.clone(),
                        sum,
                        cell_count,
                        num_expressing: num_expressing as u32,
                    })
                    .collect()
            };
            ForegroundBackground {
                background: summarize(&sums_and_counts.background, &num_expressing.background),
                foreground: summarize(&sums_and_counts.foreground, &num_expressing.foreground),
            }
        },
        &keys,
        cache_enabled,
    )
    .await;
    Some(summaries)
}
