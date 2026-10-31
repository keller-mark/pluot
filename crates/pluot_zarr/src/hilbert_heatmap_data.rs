// Orders the filter-included obs by one or more categorical columns, and
// derives the colormap domain of one var's expression, for `AdataZarrHilbertHeatmapLayer`.

use std::sync::Arc;

use pluot_core::cache::{use_memo_numeric_data, use_memo_vec_f32};
use pluot_core::compute::reduce::reduce_extent;
use pluot_core::compute::stratified_indices::{compute_stratified_indices, CompactionMethod, RowSource};
use pluot_core::numeric_data::NumericData;
use pluot_core::render_types::GpuContext;
use zarrs::array::ArrayError;

use crate::dotplot_data::{expr_array_path, load_obs_categorical, AdataStore};
use crate::heatmap_data::{
    axis_criteria_key, categories_then_missing, indices_of, load_axis_emphasis_criteria, load_axis_indices, load_var_expression, AxisCriteria,
    MatrixAxis,
};

/// Loads (and caches) the obs included by `filtering`, sorted by the categories
/// of each `groupby` column, from the coarsest (first) to the finest (last).
/// Within each column, obs follow its category order, with missing values
/// last. Obs that tie on every column keep their filtered order (stored order,
/// or listed order for identifiers).
pub async fn load_nested_grouped_obs_indices(
    gpu_context: Option<&GpuContext<'_>>,
    store: AdataStore,
    store_name: &str,
    filtering: Option<&AxisCriteria>,
    groupby: &[String],
    cache_enabled: bool,
) -> Result<Arc<NumericData>, ArrayError> {
    // The obs criteria only need the matrix for the number of obs, which `X` always has.
    let matrix_path = expr_array_path(None);
    let mut keys = vec!["adata_nested_grouped_obs_indices".to_string(), store_name.to_string(), groupby.len().to_string()];
    keys.extend(groupby.iter().cloned());
    keys.extend(axis_criteria_key(filtering));

    use_memo_numeric_data(
        async || {
            let (included, columns) = futures::try_join!(
                load_axis_indices(gpu_context, store.clone(), store_name, &matrix_path, MatrixAxis::Obs, filtering, cache_enabled),
                futures::future::try_join_all(groupby.iter().map(|column| load_obs_categorical(store.clone(), store_name, column, cache_enabled))),
            )?;
            // A stable sort by each column, from the finest to the coarsest, is a sort by all of them.
            let mut indices = indices_of(&included).to_vec();
            for (categories, codes) in columns.iter().rev() {
                let stratify_by = categories_then_missing(codes.as_ref().clone(), categories.len());
                let sorted = compute_stratified_indices(gpu_context, RowSource::Rows(&indices), Some(&stratify_by), &[], CompactionMethod::default()).await;
                indices = sorted.indices;
            }
            Ok::<_, ArrayError>(NumericData::from(indices))
        },
        &keys,
        cache_enabled,
    )
    .await
}

/// Loads (and caches) the `(min, max)` expression of the var whose `var_column`
/// (its index when `None`) is `var_name` over the obs included by `filtering`,
/// or `None` when there is no such var.
#[allow(clippy::too_many_arguments)]
pub async fn load_var_expression_extent(
    gpu_context: Option<&GpuContext<'_>>,
    store: AdataStore,
    store_name: &str,
    var_name: &str,
    var_column: Option<&str>,
    layer: Option<&str>,
    filtering: Option<&AxisCriteria>,
    cache_enabled: bool,
) -> Result<Option<(f32, f32)>, ArrayError> {
    let Some(values) = load_var_expression(store.clone(), store_name, var_name, var_column, layer, cache_enabled).await? else {
        return Ok(None);
    };
    let mut keys = vec![
        "adata_var_expression_extent".to_string(),
        store_name.to_string(),
        expr_array_path(layer),
        var_column.unwrap_or("index").to_string(),
        var_name.to_string(),
    ];
    keys.extend(axis_criteria_key(filtering));
    let extent = use_memo_vec_f32(
        async || {
            let matrix_path = expr_array_path(None);
            let filtering_criteria = load_axis_emphasis_criteria(gpu_context, store.clone(), store_name, &matrix_path, MatrixAxis::Obs, filtering, cache_enabled).await?;
            let (min, max) = reduce_extent(gpu_context, NumericData::Float32(values), &filtering_criteria, &[]).await.background;
            Ok::<_, ArrayError>(vec![min, max])
        },
        &keys,
        cache_enabled,
    )
    .await?;
    Ok(Some((extent[0], extent[1])))
}
