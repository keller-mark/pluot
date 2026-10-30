// Loads gene x obs-category expression summaries from an AnnData-Zarr store.
//
// Every loaded/computed piece is cached independently (var names, the obs groupby column, each
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

use crate::adata_io::{read_dataframe_column_strings, read_matrix_column_f32, read_string_array};
use crate::heatmap_data::{axis_criteria_key, index_mask, AxisCriteria};
use crate::memo::use_memo_shared;
use crate::zarr_numeric_data::load_arr_as_numeric_data;

/// The store of a single AnnData-Zarr object (i.e. rooted at the `anndata`-encoded group).
pub type AdataStore = Arc<dyn AsyncReadableStorageTraits>;

/// One gene's expression summary over one obs category's cells.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GeneSummary {
    /// `var` column the gene was resolved from: `"index"` for `var.index`, else the column name.
    pub var_colname: String,
    /// The gene's ID, as it appears in `var_colname`.
    pub var_name: String,
    /// The `obs` categorical column the cells were grouped by.
    pub obs_colname: String,
    /// The category label this summary was computed over.
    pub obs_value: String,

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

/// Loads (and caches) the gene IDs: `var.index` when `var_column` is `None`, else the named
/// `var` column (in its entirety).
pub async fn load_var_names(store: AdataStore, store_name: &str, var_column: Option<&str>, cache_enabled: bool) -> Result<Arc<Vec<String>>, ArrayError> {
    let keys = vec!["dotplot_var_names".to_string(), store_name.to_string(), var_column.unwrap_or("index").to_string()];
    use_memo_vec_string(async || read_dataframe_column_strings(store, "/var", var_column).await, &keys, cache_enabled).await
}

/// Loads (and caches) the `obs` groupby column's category labels and per-observation codes.
/// The two arrays are cached independently of each other.
pub async fn load_obs_categorical(store: AdataStore, store_name: &str, groupby: &str, cache_enabled: bool) -> Result<(Arc<Vec<String>>, Arc<NumericData>), ArrayError> {
    let column_path = format!("/obs/{groupby}");

    let categories_keys = vec!["dotplot_obs_categories".to_string(), store_name.to_string(), groupby.to_string()];
    let categories_future = use_memo_vec_string(
        async || read_string_array(store.clone(), &format!("{column_path}/categories")).await,
        &categories_keys,
        cache_enabled,
    );

    let codes_keys = vec!["dotplot_obs_codes".to_string(), store_name.to_string(), groupby.to_string()];
    let codes_future = use_memo_numeric_data(
        async || load_arr_as_numeric_data(store.clone(), &format!("{column_path}/codes")).await,
        &codes_keys,
        cache_enabled,
    );

    futures::try_join!(categories_future, codes_future)
}

/// Path of the expression matrix for `layer`: `/X` for `None`, else `/layers/<layer>`.
pub fn expr_array_path(layer: Option<&str>) -> String {
    match layer {
        None => "/X".to_string(),
        Some(layer) => format!("/layers/{layer}"),
    }
}

/// Loads (and caches) one gene's whole column of the expression matrix for `layer`.
pub async fn load_gene_expression(
    store: AdataStore,
    store_name: &str,
    var_colname: &str,
    gene_name: &str,
    col_index: u64,
    layer: Option<&str>,
    cache_enabled: bool,
) -> Result<Arc<Vec<f32>>, ArrayError> {
    let array_path = expr_array_path(layer);
    let expr_keys = vec![
        "dotplot_expr".to_string(),
        store_name.to_string(),
        array_path.clone(),
        var_colname.to_string(),
        gene_name.to_string(),
    ];
    // Whether the matrix is dense, CSR or CSC is the reader's business, not ours.
    use_memo_vec_f32(async || read_matrix_column_f32(store, &array_path, col_index).await, &expr_keys, cache_enabled).await
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

/// The code of each of `requested_categories` in `obs_categories`, as strata to reduce over.
/// Unknown categories are logged, and given distinct codes below -1 that no obs has.
pub fn category_strata(requested_categories: &[String], obs_categories: &[String], obs_codes: &NumericData) -> StratifyBy {
    let code_of: HashMap<&str, i64> = obs_categories.iter().enumerate().map(|(code, category)| (category.as_str(), code as i64)).collect();
    let mut num_unknown = 0;
    let included_codes = requested_categories
        .iter()
        .map(|category| {
            code_of.get(category.as_str()).copied().unwrap_or_else(|| {
                log(&format!("dotplot_data: obs category \"{category}\" not found; skipping"));
                num_unknown += 1;
                -1 - num_unknown
            })
        })
        .collect();
    StratifyBy { codes: obs_codes.clone(), included_codes }
}

/// Cache key parts identifying the obs rows that are summarized: those included by
/// `filtering`, and among them, those included by `selection`.
pub fn obs_rows_key(filtering: Option<&AxisCriteria>, selection: Option<&AxisCriteria>) -> Vec<String> {
    let mut keys = axis_criteria_key(filtering);
    keys.extend(axis_criteria_key(selection));
    keys
}

// ---------------------------------------------------------------------------
// Entry points
// ---------------------------------------------------------------------------

/// The obs to summarize per category: those meeting `filtering_criteria` (the background),
/// and among them those meeting `selection_criteria` (the foreground).
pub struct CategoryObs<'a> {
    pub groupby: &'a str,
    pub categories: &'a [String],
    /// One stratum per category (see [`category_strata`]).
    pub strata: &'a StratifyBy,
    pub filtering_criteria: &'a [EmphasisCriteria],
    pub selection_criteria: &'a [EmphasisCriteria],
    /// Cache key parts identifying both criteria (see [`obs_rows_key`]).
    pub criteria_key: &'a [String],
}

/// The `categories` with any obs included by `filtering_criteria` (which `criteria_key`
/// identifies), counting (and caching) each category's included obs.
#[allow(clippy::too_many_arguments)]
pub async fn categories_with_included_obs(
    gpu_context: Option<&GpuContext<'_>>,
    store_name: &str,
    groupby: &str,
    categories: Vec<String>,
    obs_categories: &[String],
    obs_codes: &NumericData,
    filtering_criteria: &[EmphasisCriteria],
    criteria_key: &[String],
    cache_enabled: bool,
) -> Vec<String> {
    let mut keys = vec!["dotplot_category_counts".to_string(), store_name.to_string(), groupby.to_string()];
    keys.extend(categories.iter().cloned());
    keys.extend(criteria_key.iter().cloned());
    let counts = use_memo_vec_f32(
        async || {
            let strata = category_strata(&categories, obs_categories, obs_codes);
            let counts = reduce_stratified_count(gpu_context, obs_codes.clone(), &strata, filtering_criteria, &[]).await;
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
    var_colname: &str,
    gene_name: &str,
    col_index: u64,
    layer: Option<&str>,
    obs: &CategoryObs<'_>,
    expression_cutoff: f32,
    cache_enabled: bool,
) -> Option<Arc<GeneSummaries>> {
    let expr_values = match load_gene_expression(store, store_name, var_colname, gene_name, col_index, layer, cache_enabled).await {
        Ok(expr) => expr,
        Err(error) => {
            if is_timed_out_zarrs_error(&error) {
                return None;
            }
            panic!("Zarrs error loading AnnData expression column for gene \"{gene_name}\" at \"{}\": {error:?}", expr_array_path(layer));
        }
    };

    let mut keys = vec![
        "dotplot_gene_summaries".to_string(),
        store_name.to_string(),
        expr_array_path(layer),
        var_colname.to_string(),
        gene_name.to_string(),
        obs.groupby.to_string(),
        expression_cutoff.to_string(),
    ];
    keys.extend(obs.categories.iter().cloned());
    keys.extend(obs.criteria_key.iter().cloned());
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
                    .map(|(category, (&(sum, cell_count), &num_expressing))| GeneSummary {
                        var_colname: var_colname.to_string(),
                        var_name: gene_name.to_string(),
                        obs_colname: obs.groupby.to_string(),
                        obs_value: category.clone(),
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
