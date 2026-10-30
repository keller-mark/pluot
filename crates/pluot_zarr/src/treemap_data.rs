// Counts observations per combination of categories across one or more `obs` categorical columns.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use pluot_core::compute::code_combinations::{count_code_combinations, num_code_combinations};
use pluot_core::numeric_data::NumericData;
use pluot_core::render_types::GpuContext;
use zarrs::array::ArrayError;

use crate::dotplot_data::{load_obs_categorical, AdataStore};

/// The number of observations sharing one combination of categories.
#[derive(Clone, Debug, PartialEq)]
pub struct ObsGroupCount {
    /// One category code per `groupby` column, in the order of the columns.
    pub codes: Vec<u32>,
    pub count: u32,
}

thread_local! {
    static USE_MEMO_CACHE_OBS_GROUP_COUNTS: RefCell<Option<HashMap<Vec<String>, Arc<Vec<ObsGroupCount>>>>> = const { RefCell::new(None) };
}

async fn use_memo_obs_group_counts(initializer: impl AsyncFnOnce() -> Vec<ObsGroupCount>, keys: &[String], cache_enabled: bool) -> Arc<Vec<ObsGroupCount>> {
    if !cache_enabled {
        return Arc::new(initializer().await);
    }

    let cached = USE_MEMO_CACHE_OBS_GROUP_COUNTS.with(|cache| cache.borrow().as_ref().and_then(|map| map.get(keys).cloned()));
    if let Some(counts) = cached {
        return counts;
    }

    let counts = Arc::new(initializer().await);
    USE_MEMO_CACHE_OBS_GROUP_COUNTS.with(|cache| {
        cache.borrow_mut().get_or_insert_with(HashMap::new).insert(keys.to_vec(), counts.clone());
    });
    counts
}

/// Counts the rows sharing each combination of codes across `codes_per_column`, sorted by codes.
/// Rows with a missing value (a negative code, per the AnnData spec) in any column are not counted.
pub fn count_obs_groups(codes_per_column: &[&NumericData]) -> Vec<ObsGroupCount> {
    let columns: Vec<_> = codes_per_column.iter().map(|codes| codes.as_f32()).collect();
    let num_rows = columns.iter().map(|codes| codes.len()).min().unwrap_or(0);

    let mut counts: HashMap<Vec<u32>, u32> = HashMap::new();
    let mut row_codes: Vec<u32> = Vec::with_capacity(columns.len());
    'rows: for row in 0..num_rows {
        row_codes.clear();
        for codes in &columns {
            let code = codes[row];
            if code < 0.0 {
                continue 'rows;
            }
            row_codes.push(code as u32);
        }
        match counts.get_mut(row_codes.as_slice()) {
            Some(count) => *count += 1,
            None => {
                counts.insert(row_codes.clone(), 1);
            }
        }
    }

    let mut groups: Vec<ObsGroupCount> = counts.into_iter().map(|(codes, count)| ObsGroupCount { codes, count }).collect();
    groups.sort_by(|a, b| a.codes.cmp(&b.codes));
    groups
}

/// [`count_obs_groups`], counting on the GPU. `num_categories` holds each column's number of
/// categories, whose combinations must fit in one GPU buffer (see [`num_code_combinations`]).
pub async fn count_obs_groups_on_gpu(gpu_context: &GpuContext<'_>, codes_per_column: &[&NumericData], num_categories: &[usize]) -> Vec<ObsGroupCount> {
    if codes_per_column.is_empty() {
        return Vec::new();
    }
    let counts = count_code_combinations(gpu_context, codes_per_column, num_categories).await;
    counts
        .into_iter()
        .enumerate()
        .filter(|&(_, count)| count > 0)
        .map(|(mut combination, count)| {
            let mut codes = vec![0u32; num_categories.len()];
            for (code, &n) in codes.iter_mut().zip(num_categories).rev() {
                *code = (combination % n) as u32;
                combination /= n;
            }
            ObsGroupCount { codes, count }
        })
        .collect()
}

/// Loads (and caches) each `groupby` column's category labels, and the number of observations per
/// combination of their categories. The counts are cached independently of the columns they were computed from.
pub async fn load_obs_group_counts(
    gpu_context: Option<&GpuContext<'_>>,
    store: AdataStore,
    store_name: &str,
    groupby: &[String],
    cache_enabled: bool,
) -> Result<(Vec<Arc<Vec<String>>>, Arc<Vec<ObsGroupCount>>), ArrayError> {
    let columns = futures::future::try_join_all(
        groupby.iter().map(|column| load_obs_categorical(store.clone(), store_name, column, cache_enabled)),
    ).await?;

    let mut counts_keys = vec!["treemap_obs_group_counts".to_string(), store_name.to_string()];
    counts_keys.extend(groupby.iter().cloned());
    let counts = use_memo_obs_group_counts(
        async || {
            let codes_per_column: Vec<&NumericData> = columns.iter().map(|(_, codes)| codes.as_ref()).collect();
            let num_categories: Vec<usize> = columns.iter().map(|(categories, _)| categories.len()).collect();
            match gpu_context {
                Some(gpu_context) if num_code_combinations(gpu_context, &num_categories).is_some() => {
                    count_obs_groups_on_gpu(gpu_context, &codes_per_column, &num_categories).await
                }
                _ => count_obs_groups(&codes_per_column),
            }
        },
        &counts_keys,
        cache_enabled,
    ).await;

    let categories_per_column = columns.into_iter().map(|(categories, _)| categories).collect();
    Ok((categories_per_column, counts))
}
