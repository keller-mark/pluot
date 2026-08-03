// Loads gene x obs-category expression summaries from an AnnData-Zarr store.
//
// Every loaded/computed piece is cached independently (var names, the obs groupby column, each
// gene's expression column, each gene x category summary), so that none of them block or
// invalidate one another: changing one gene or one category does not require re-fetching or
// recomputing anything for the others.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use pluot_core::cache::{use_memo_numeric_data, use_memo_vec_string};
use pluot_core::log;
use pluot_core::numeric_data::NumericData;
use pluot_core::zarr::is_timed_out_zarrs_error;
use zarrs::array::ArrayError;
use zarrs::storage::AsyncReadableStorageTraits;

use crate::adata_io::{read_dataframe_index, read_dense_column_numeric, read_encoding, read_string_array};
use crate::adata_metadata::AnnDataEncoding;
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
// Memoization (plain thread-local cache; no recursive re-entrancy here, so no
// need for the nested-borrow-safe macro pattern used elsewhere in the crate)
// ---------------------------------------------------------------------------

thread_local! {
    static USE_MEMO_CACHE_GENE_SUMMARY: RefCell<Option<HashMap<Vec<String>, Arc<GeneSummary>>>> = const { RefCell::new(None) };
}

async fn use_memo_gene_summary(initializer: impl AsyncFnOnce() -> GeneSummary, keys: &[String], cache_enabled: bool) -> Arc<GeneSummary> {
    if !cache_enabled {
        return Arc::new(initializer().await);
    }

    let cached = USE_MEMO_CACHE_GENE_SUMMARY.with(|cache| cache.borrow().as_ref().and_then(|map| map.get(keys).cloned()));
    if let Some(summary) = cached {
        return summary;
    }

    let summary = Arc::new(initializer().await);
    USE_MEMO_CACHE_GENE_SUMMARY.with(|cache| {
        cache.borrow_mut().get_or_insert_with(HashMap::new).insert(keys.to_vec(), summary.clone());
    });
    summary
}

// ---------------------------------------------------------------------------
// Independent single-element loads
// ---------------------------------------------------------------------------

/// Loads (and caches) the gene IDs: `var.index` when `var_column` is `None`, else the named
/// `var` column (in its entirety).
pub async fn load_var_names(store: AdataStore, store_name: &str, var_column: Option<&str>, cache_enabled: bool) -> Result<Arc<Vec<String>>, ArrayError> {
    let keys = vec!["dotplot_var_names".to_string(), store_name.to_string(), var_column.unwrap_or("index").to_string()];
    use_memo_vec_string(
        async || match var_column {
            None => read_dataframe_index(store.clone(), "/var").await,
            Some(column) => {
                let column_path = format!("/var/{column}");
                let values_path = match read_encoding(store.clone(), &column_path).await {
                    AnnDataEncoding::NullableStringArray { .. } => format!("{column_path}/values"),
                    AnnDataEncoding::StringArray { .. } => column_path,
                    other => panic!("Unsupported var column encoding at \"{column_path}\": {other:?}"),
                };
                read_string_array(store, &values_path).await
            }
        },
        &keys,
        cache_enabled,
    )
    .await
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
fn expr_array_path(layer: Option<&str>) -> String {
    match layer {
        None => "/X".to_string(),
        Some(layer) => format!("/layers/{layer}"),
    }
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

/// Buckets observation row indices by requested obs category, given the `obs` groupby column's
/// stored categories and per-observation codes (`-1` denotes a missing value, per the AnnData
/// spec). Requested categories not present in `obs_categories` are logged and yield no rows.
pub fn bucket_rows_by_category(requested_categories: &[String], obs_categories: &[String], obs_codes: &NumericData) -> HashMap<String, Vec<u32>> {
    let category_position: HashMap<&str, usize> = obs_categories.iter().enumerate().map(|(i, category)| (category.as_str(), i)).collect();
    let mut code_to_requested: HashMap<usize, &str> = HashMap::new();
    for category in requested_categories {
        match category_position.get(category.as_str()) {
            Some(&position) => {
                code_to_requested.insert(position, category.as_str());
            }
            None => log(&format!("dotplot_data: obs category \"{category}\" not found; skipping")),
        }
    }

    let mut rows_by_category: HashMap<String, Vec<u32>> = requested_categories.iter().map(|category| (category.clone(), Vec::new())).collect();
    for (row, code) in obs_codes.as_f32().iter().enumerate() {
        if *code < 0.0 {
            continue;
        }
        if let Some(&category) = code_to_requested.get(&(*code as usize)) {
            rows_by_category.get_mut(category).unwrap().push(row as u32);
        }
    }
    rows_by_category
}

/// Summarizes one gene's expression (a whole column of `X`) over one category's observation
/// rows: the sum and count of those cells, plus how many of them exceed `expression_cutoff`.
fn summarize_gene_category(
    values: &[f32],
    rows: &[u32],
    expression_cutoff: f32,
    var_colname: &str,
    var_name: &str,
    obs_colname: &str,
    obs_value: &str,
) -> GeneSummary {
    let mut sum = 0.0f32;
    let mut num_expressing = 0u32;
    for &row in rows {
        let value = values[row as usize];
        sum += value;
        if value > expression_cutoff {
            num_expressing += 1;
        }
    }
    GeneSummary {
        var_colname: var_colname.to_string(),
        var_name: var_name.to_string(),
        obs_colname: obs_colname.to_string(),
        obs_value: obs_value.to_string(),
        sum,
        cell_count: rows.len() as u32,
        num_expressing,
    }
}

// ---------------------------------------------------------------------------
// Entry points
// ---------------------------------------------------------------------------

/// Loads one gene's `GeneSummary` per requested obs category, given the obs groupby column's
/// already-loaded (or already-bucketed) row indices per category.
///
/// This is the independent, per-gene unit of work: it loads (and caches) just this gene's array
/// column and then computes (and caches) its per-category summaries. Returns `None` when the
/// gene's data isn't loaded *yet* rather than propagating an error: under `wait_for_store_gets:
/// false`, a chunk not already in memory returns a "timed out" zarrs error immediately while the
/// store fetches it in the background, which is an expected, transient condition (see
/// `is_timed_out_zarrs_error`), not a failure — the same distinction other layers in this crate
/// make (e.g. `ZarrHistogramLayer::prepare`). Any other zarrs error is a genuine, unexpected
/// failure and panics, matching that same convention. Because a pending gene never propagates an
/// error, many of these can run concurrently (e.g. via `join_all`) without one slow or pending
/// gene cancelling or blocking the others. Exposed directly (rather than only via
/// [`load_gene_summaries`]) so a caller doing progressive/incremental rendering can render
/// whichever genes' futures resolved to `Some` so far, while treating any `None` as "not ready
/// yet, try again" (see `AdataZarrDotPlotLayer::prepare` for an example).
pub async fn load_gene_summaries_for_gene(
    store: AdataStore,
    store_name: &str,
    var_colname: &str,
    gene_name: &str,
    col_index: u64,
    layer: Option<&str>,
    groupby: &str,
    categories: &[String],
    rows_by_category: &HashMap<String, Vec<u32>>,
    expression_cutoff: f32,
    cache_enabled: bool,
) -> Option<Vec<GeneSummary>> {
    let array_path = expr_array_path(layer);
    let expr_keys = vec![
        "dotplot_expr".to_string(),
        store_name.to_string(),
        array_path.clone(),
        var_colname.to_string(),
        gene_name.to_string(),
    ];
    let expr = match use_memo_numeric_data(async || read_dense_column_numeric(store.clone(), &array_path, col_index).await, &expr_keys, cache_enabled).await {
        Ok(expr) => expr,
        Err(error) => {
            if is_timed_out_zarrs_error(&error) {
                return None;
            }
            panic!("Zarrs error loading AnnData expression column for gene \"{gene_name}\" at \"{array_path}\": {error:?}");
        }
    };
    let expr_values = expr.as_f32();

    let empty_rows: Vec<u32> = Vec::new();
    let mut summaries = Vec::with_capacity(categories.len());
    for category in categories {
        let rows = rows_by_category.get(category).unwrap_or(&empty_rows);
        let summary_keys = vec![
            "dotplot_gene_summary".to_string(),
            store_name.to_string(),
            var_colname.to_string(),
            gene_name.to_string(),
            groupby.to_string(),
            category.clone(),
            expression_cutoff.to_string(),
        ];
        let summary = use_memo_gene_summary(
            async || summarize_gene_category(&expr_values, rows, expression_cutoff, var_colname, gene_name, groupby, category),
            &summary_keys,
            cache_enabled,
        )
        .await;
        summaries.push((*summary).clone());
    }
    Some(summaries)
}

/// Loads one [`GeneSummary`] per (resolved gene, resolved obs category) pair, all at once.
///
/// The var names and the obs groupby column are loaded in parallel (`try_join!`), since neither
/// depends on the other. Once both are ready, each resolved gene is handled by its own
/// [`load_gene_summaries_for_gene`] future; these run concurrently via `join_all` rather than
/// `try_join_all`, so one gene's failed or slow read can never cancel another gene's in-flight
/// read. This waits for every gene before returning; a caller that wants to render whatever has
/// loaded so far without waiting for the rest should drive [`load_gene_summaries_for_gene`]
/// directly instead (see `AdataZarrDotPlotLayer::prepare` for an example).
pub async fn load_gene_summaries(
    store: AdataStore,
    store_name: &str,
    var_names: &[String],
    var_column: Option<&str>,
    layer: Option<&str>,
    groupby: &str,
    categories: &[String],
    expression_cutoff: f32,
    cache_enabled: bool,
) -> Result<Vec<GeneSummary>, ArrayError> {
    let (var_index_values, (obs_categories, obs_codes)) = futures::try_join!(
        load_var_names(store.clone(), store_name, var_column, cache_enabled),
        load_obs_categorical(store.clone(), store_name, groupby, cache_enabled),
    )?;

    let resolved_genes = resolve_gene_columns(var_names, &var_index_values);
    let rows_by_category = bucket_rows_by_category(categories, &obs_categories, &obs_codes);
    let var_colname = var_column.unwrap_or("index").to_string();

    let gene_futures = resolved_genes.iter().map(|(gene_name, col_index)| {
        load_gene_summaries_for_gene(
            store.clone(),
            store_name,
            &var_colname,
            gene_name,
            *col_index,
            layer,
            groupby,
            categories,
            &rows_by_category,
            expression_cutoff,
            cache_enabled,
        )
    });

    // A `None` here means that gene's data wasn't loaded yet (see `load_gene_summaries_for_gene`);
    // since this function makes only one pass and has no bail/retry mechanism of its own, such a
    // gene is simply omitted rather than waited on.
    let results = futures::future::join_all(gene_futures).await;
    Ok(results.into_iter().flatten().flatten().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_gene_columns_skips_genes_not_found() {
        let var_index = vec!["GeneA".to_string(), "GeneB".to_string(), "GeneC".to_string()];
        let requested = vec!["GeneC".to_string(), "GeneX".to_string(), "GeneA".to_string()];
        let resolved = resolve_gene_columns(&requested, &var_index);
        assert_eq!(resolved, vec![("GeneC".to_string(), 2), ("GeneA".to_string(), 0)]);
    }

    #[test]
    fn bucket_rows_by_category_groups_by_requested_categories_only() {
        let obs_categories = vec!["B".to_string(), "T".to_string(), "NK".to_string()];
        // Cell 5 has a missing category (-1); cell 2 belongs to "NK", which is not requested.
        let obs_codes = NumericData::Int32(Arc::new(vec![0, 0, 2, 1, 1, -1]));
        let requested = vec!["T".to_string(), "B".to_string()];

        let buckets = bucket_rows_by_category(&requested, &obs_categories, &obs_codes);
        assert_eq!(buckets.get("B"), Some(&vec![0, 1]));
        assert_eq!(buckets.get("T"), Some(&vec![3, 4]));
        assert_eq!(buckets.len(), 2);
    }

    #[test]
    fn bucket_rows_by_category_with_no_requested_categories_is_empty() {
        let obs_categories = vec!["B".to_string(), "T".to_string()];
        let obs_codes = NumericData::Int32(Arc::new(vec![0, 1]));
        let buckets = bucket_rows_by_category(&[], &obs_categories, &obs_codes);
        assert!(buckets.is_empty());
    }

    #[test]
    fn bucket_rows_by_category_logs_and_skips_unknown_category() {
        let obs_categories = vec!["B".to_string()];
        let obs_codes = NumericData::Int32(Arc::new(vec![0, 0]));
        let requested = vec!["B".to_string(), "Unknown".to_string()];
        let buckets = bucket_rows_by_category(&requested, &obs_categories, &obs_codes);
        assert_eq!(buckets.get("B"), Some(&vec![0, 1]));
        assert_eq!(buckets.get("Unknown"), Some(&vec![]));
    }

    #[test]
    fn summarize_gene_category_computes_sum_count_and_num_expressing() {
        let values = [0.0, 1.0, 2.0, 9.0];

        let summary = summarize_gene_category(&values, &[0, 1, 2], 0.0, "index", "GeneA", "celltype", "T");
        assert_eq!(summary.cell_count, 3);
        assert_eq!(summary.sum, 3.0);
        // The cutoff is exclusive, so the 0.0 value does not count as expressing.
        assert_eq!(summary.num_expressing, 2);
        assert_eq!(summary.var_name, "GeneA");
        assert_eq!(summary.obs_value, "T");

        // Only the given rows contribute, in any order.
        let summary = summarize_gene_category(&values, &[3, 1], 1.5, "index", "GeneA", "celltype", "T");
        assert_eq!(summary.sum, 10.0);
        assert_eq!(summary.num_expressing, 1);

        // An empty group is all zeros rather than NaN.
        let summary = summarize_gene_category(&values, &[], 0.0, "index", "GeneA", "celltype", "T");
        assert_eq!(summary.sum, 0.0);
        assert_eq!(summary.cell_count, 0);
        assert_eq!(summary.num_expressing, 0);
    }
}
