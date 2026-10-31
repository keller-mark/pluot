// Counts observations per combination of categories across one or more `obs` categorical columns.

use std::collections::HashMap;
use std::sync::Arc;

use pluot_core::compute::code_combinations::{count_code_combinations, num_code_combinations};
use pluot_core::emphasis_mode::cpu_is_included;
use pluot_core::numeric_data::NumericData;
use pluot_core::render_traits::EmphasisCriteria;
use pluot_core::render_types::GpuContext;
use zarrs::array::ArrayError;

use crate::dotplot_data::{expr_array_path, load_obs_categorical, obs_rows_key, AdataStore};
use crate::heatmap_data::{load_axis_emphasis_criteria, AxisCriteria, MatrixAxis};
use crate::memo::use_memo_shared;

/// The number of observations sharing one combination of categories.
#[derive(Clone, Debug, PartialEq)]
pub struct ObsGroupCount {
    /// One category code per `groupby` column, in the order of the columns.
    pub codes: Vec<u32>,
    /// The number of filter-included observations.
    pub count: u32,
    /// The number of those observations that are also selected.
    pub selected_count: u32,
}

/// Counts the rows sharing each combination of codes across `codes_per_column`, sorted by codes:
/// those meeting `filtering_criteria`, and among them those meeting `selection_criteria`.
/// Rows with a missing value (a negative code, per the AnnData spec) in any column are not counted,
/// nor are combinations without any filter-included row.
pub fn count_obs_groups(
    codes_per_column: &[&NumericData],
    filtering_criteria: &[EmphasisCriteria],
    selection_criteria: &[EmphasisCriteria],
) -> Vec<ObsGroupCount> {
    let columns: Vec<_> = codes_per_column.iter().map(|codes| codes.as_f32()).collect();
    let num_rows = columns.iter().map(|codes| codes.len()).min().unwrap_or(0);

    let mut counts: HashMap<Vec<u32>, (u32, u32)> = HashMap::new();
    let mut row_codes: Vec<u32> = Vec::with_capacity(columns.len());
    'rows: for row in 0..num_rows {
        if !cpu_is_included(filtering_criteria, row) {
            continue;
        }
        row_codes.clear();
        for codes in &columns {
            let code = codes[row];
            if code < 0.0 {
                continue 'rows;
            }
            row_codes.push(code as u32);
        }
        let is_selected = cpu_is_included(selection_criteria, row) as u32;
        match counts.get_mut(row_codes.as_slice()) {
            Some((count, selected_count)) => {
                *count += 1;
                *selected_count += is_selected;
            }
            None => {
                counts.insert(row_codes.clone(), (1, is_selected));
            }
        }
    }

    let mut groups: Vec<ObsGroupCount> =
        counts.into_iter().map(|(codes, (count, selected_count))| ObsGroupCount { codes, count, selected_count }).collect();
    groups.sort_by(|a, b| a.codes.cmp(&b.codes));
    groups
}

/// [`count_obs_groups`], counting on the GPU. `num_categories` holds each column's number of
/// categories, whose combinations must fit in one GPU buffer (see [`num_code_combinations`]).
pub async fn count_obs_groups_on_gpu(
    gpu_context: &GpuContext<'_>,
    codes_per_column: &[&NumericData],
    num_categories: &[usize],
    filtering_criteria: &[EmphasisCriteria],
    selection_criteria: &[EmphasisCriteria],
) -> Vec<ObsGroupCount> {
    if codes_per_column.is_empty() {
        return Vec::new();
    }
    let counts = count_code_combinations(gpu_context, codes_per_column, num_categories, filtering_criteria, selection_criteria).await;
    counts
        .background
        .into_iter()
        .zip(counts.foreground)
        .enumerate()
        .filter(|&(_, (count, _))| count > 0)
        .map(|(mut combination, (count, selected_count))| {
            let mut codes = vec![0u32; num_categories.len()];
            for (code, &n) in codes.iter_mut().zip(num_categories).rev() {
                *code = (combination % n) as u32;
                combination /= n;
            }
            ObsGroupCount { codes, count, selected_count }
        })
        .collect()
}

/// Loads (and caches) each `groupby` column's category labels, and the number of observations per
/// combination of their categories: those included by `obs_filtering`, and among them those included
/// by `obs_selection`. The counts are cached independently of the columns they were computed from.
pub async fn load_obs_group_counts(
    gpu_context: Option<&GpuContext<'_>>,
    store: AdataStore,
    store_name: &str,
    groupby: &[String],
    obs_filtering: Option<&AxisCriteria>,
    obs_selection: Option<&AxisCriteria>,
    cache_enabled: bool,
) -> Result<(Vec<Arc<Vec<String>>>, Arc<Vec<ObsGroupCount>>), ArrayError> {
    // The obs criteria only need the matrix for the number of obs, which `X` always has.
    let matrix_path = expr_array_path(None);
    let (columns, filtering_criteria, selection_criteria) = futures::try_join!(
        futures::future::try_join_all(groupby.iter().map(|column| load_obs_categorical(store.clone(), store_name, column, cache_enabled))),
        load_axis_emphasis_criteria(gpu_context, store.clone(), store_name, &matrix_path, MatrixAxis::Obs, obs_filtering, cache_enabled),
        load_axis_emphasis_criteria(gpu_context, store.clone(), store_name, &matrix_path, MatrixAxis::Obs, obs_selection, cache_enabled),
    )?;

    let mut counts_keys = vec!["treemap_obs_group_counts".to_string(), store_name.to_string(), groupby.len().to_string()];
    counts_keys.extend(groupby.iter().cloned());
    counts_keys.extend(obs_rows_key(obs_filtering, obs_selection));
    let counts = use_memo_shared(
        async || {
            let codes_per_column: Vec<&NumericData> = columns.iter().map(|(_, codes)| codes.as_ref()).collect();
            let num_categories: Vec<usize> = columns.iter().map(|(categories, _)| categories.len()).collect();
            match gpu_context {
                Some(gpu_context) if num_code_combinations(gpu_context, &num_categories).is_some() => {
                    count_obs_groups_on_gpu(gpu_context, &codes_per_column, &num_categories, &filtering_criteria, &selection_criteria).await
                }
                _ => count_obs_groups(&codes_per_column, &filtering_criteria, &selection_criteria),
            }
        },
        &counts_keys,
        cache_enabled,
    ).await;

    let categories_per_column = columns.into_iter().map(|(categories, _)| categories).collect();
    Ok((categories_per_column, counts))
}
