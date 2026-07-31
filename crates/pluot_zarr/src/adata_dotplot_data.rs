// A reusable "hook" (in the `pluot_core::cache::use_memo_*` sense: a cached
// async data loader) for the data behind an AnnData dot plot: mean expression
// and fraction of cells expressing, per (groupby category, gene) pair. Lives
// in its own module so any future AnnData-Zarr layer needing the same shape
// of data (e.g. a violin or heatmap layer) can reuse it.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use pluot_core::cache::{use_memo_numeric_data, use_memo_vec_f32, use_memo_vec_string};
use pluot_core::log;
use zarrs::storage::AsyncReadableStorageTraits;

use crate::adata_io::{read_dataframe_index, read_dense_column_f32, read_string_array};
use crate::zarr_numeric_data::load_arr_as_numeric_data;

/// Mean expression and fraction of cells expressing, per (groupby category,
/// gene) pair, laid out row-major as `[group][gene]`.
pub struct DotPlotData {
    /// The subset of the requested gene IDs that were found in `var.index`,
    /// in the same relative order as requested. May be shorter than
    /// requested if some gene IDs were not found.
    pub var_names: Vec<String>,
    pub categories: Arc<Vec<String>>,
    pub mean_expression: Vec<f32>,
    pub fraction_expressing: Vec<f32>,
}

thread_local! {
    static USE_MEMO_CACHE_DOTPLOT_DATA: RefCell<Option<HashMap<Vec<String>, Arc<DotPlotData>>>> = const { RefCell::new(None) };
}

/// Loads (and caches) the [`DotPlotData`] for one dot plot configuration.
///
/// Nests two levels of caching: the individual zarr reads (var index,
/// groupby categories/codes, per-gene expression columns) are each cached
/// independently via `pluot_core::cache`'s `use_memo_*` (so e.g. a `groupby`
/// change doesn't re-fetch gene expression columns that are still valid),
/// while the derived mean/fraction values are cached as a single unit keyed
/// on every parameter that affects them.
pub async fn use_dotplot_data(
    store: Arc<dyn AsyncReadableStorageTraits>,
    store_name: &str,
    groupby: &str,
    expr_array_path: &str,
    requested_var_names: &[String],
    expression_cutoff: f32,
    cache_enabled: bool,
) -> Result<Arc<DotPlotData>, zarrs::array::ArrayError> {
    // TODO: cache the data independently for each gene ID and groupby colname

    let keys = vec![
        "adata_dotplot_data".to_string(),
        store_name.to_string(),
        groupby.to_string(),
        expr_array_path.to_string(),
        requested_var_names.join(","),
        expression_cutoff.to_string(),
    ];

    if cache_enabled {
        let cached = USE_MEMO_CACHE_DOTPLOT_DATA.with(|map| map.borrow().as_ref().and_then(|m| m.get(&keys).cloned()));
        if let Some(data) = cached {
            return Ok(data);
        }
    }

    let data = Arc::new(
        load_dotplot_data(store, store_name, groupby, expr_array_path, requested_var_names, expression_cutoff, cache_enabled).await?,
    );

    if cache_enabled {
        USE_MEMO_CACHE_DOTPLOT_DATA.with(|map| {
            map.borrow_mut().get_or_insert_with(HashMap::new).insert(keys, data.clone());
        });
    }

    Ok(data)
}

async fn load_dotplot_data(
    store: Arc<dyn AsyncReadableStorageTraits>,
    store_name: &str,
    groupby: &str,
    expr_array_path: &str,
    requested_var_names: &[String],
    expression_cutoff: f32,
    cache_enabled: bool,
) -> Result<DotPlotData, zarrs::array::ArrayError> {
    let groupby_path = format!("/obs/{groupby}");

    let var_index_deps = vec!["adata_var_index".to_string(), store_name.to_string()];
    let var_index_future = use_memo_vec_string(async || {
        read_dataframe_index(store.clone(), "/var").await
    }, &var_index_deps, cache_enabled);

    let categories_deps = vec!["adata_obs_categories".to_string(), store_name.to_string(), groupby.to_string()];
    let categories_future = use_memo_vec_string(async || {
        read_string_array(store.clone(), &format!("{groupby_path}/categories")).await
    }, &categories_deps, cache_enabled);

    let codes_deps = vec!["adata_obs_codes".to_string(), store_name.to_string(), groupby.to_string()];
    let codes_future = use_memo_numeric_data(async || {
        load_arr_as_numeric_data(store.clone(), &format!("{groupby_path}/codes")).await
    }, &codes_deps, cache_enabled);

    let (var_index, categories, codes) = futures::try_join!(var_index_future, categories_future, codes_future)?;

    // Resolve the requested gene IDs to their column index in `var.index`,
    // logging (and skipping) any that aren't found.
    let var_index_lookup: HashMap<&str, usize> = var_index.iter().enumerate().map(|(i, name)| (name.as_str(), i)).collect();
    let mut found_var_names: Vec<String> = Vec::new();
    let mut gene_col_indices: Vec<u64> = Vec::new();
    for var_name in requested_var_names {
        match var_index_lookup.get(var_name.as_str()) {
            Some(&i) => {
                found_var_names.push(var_name.clone());
                gene_col_indices.push(i as u64);
            }
            None => log(&format!("AdataZarrDotPlotLayer: gene \"{var_name}\" not found in var.index; skipping")),
        }
    }

    if found_var_names.is_empty() || categories.is_empty() {
        return Ok(DotPlotData {
            var_names: found_var_names,
            categories,
            mean_expression: vec![],
            fraction_expressing: vec![],
        });
    }

    // Load the expression values for each requested gene: one column of
    // `X`/`layers[layer]` per gene, each cached independently of `groupby`
    // and `expression_cutoff` (for now, assumed to be a dense array).
    let column_futures = gene_col_indices.iter().map(|&col_index| {
        let store = store.clone();
        let array_path = expr_array_path.to_string();
        let deps = vec!["adata_expr_column".to_string(), store_name.to_string(), expr_array_path.to_string(), col_index.to_string()];
        async move {
            use_memo_vec_f32(async || read_dense_column_f32(store, &array_path, col_index).await, &deps, cache_enabled).await
        }
    });
    let columns: Vec<Arc<Vec<f32>>> = futures::future::try_join_all(column_futures).await?;

    // Group cell (obs) indices by their groupby category code. A code of
    // -1 (or out of range) denotes a missing value, per the AnnData spec;
    // such cells are excluded from every group's mean/fraction.
    let n_groups = categories.len();
    let mut cells_by_group: Vec<Vec<usize>> = vec![Vec::new(); n_groups];
    for cell_i in 0..codes.len() {
        let code = codes.get_f32(cell_i) as i64;
        if code >= 0 && (code as usize) < n_groups {
            cells_by_group[code as usize].push(cell_i);
        }
    }

    // For each (groupby category, gene) pair, compute the mean expression
    // and the fraction of cells expressing (value > expression_cutoff),
    // matching scanpy's `sc.pl.dotplot` semantics.
    let n_genes = found_var_names.len();
    let mut mean_expression: Vec<f32> = Vec::with_capacity(n_groups * n_genes);
    let mut fraction_expressing: Vec<f32> = Vec::with_capacity(n_groups * n_genes);
    for cell_indices in &cells_by_group {
        for col in &columns {
            if cell_indices.is_empty() {
                mean_expression.push(0.0);
                fraction_expressing.push(0.0);
                continue;
            }
            let mut sum = 0.0f32;
            let mut n_expressing = 0usize;
            for &cell_i in cell_indices {
                let v = col[cell_i];
                sum += v;
                if v > expression_cutoff {
                    n_expressing += 1;
                }
            }
            mean_expression.push(sum / cell_indices.len() as f32);
            fraction_expressing.push(n_expressing as f32 / cell_indices.len() as f32);
        }
    }

    Ok(DotPlotData {
        var_names: found_var_names,
        categories,
        mean_expression,
        fraction_expressing,
    })
}
