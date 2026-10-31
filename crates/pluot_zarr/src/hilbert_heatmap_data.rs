// Derives the colormap domain of one var's expression, for `AdataZarrHilbertHeatmapLayer`.

use pluot_core::cache::use_memo_vec_f32;
use pluot_core::compute::reduce::reduce_extent;
use pluot_core::numeric_data::NumericData;
use pluot_core::render_types::GpuContext;
use zarrs::array::ArrayError;

use crate::dotplot_data::AdataStore;
use crate::heatmap_data::{axis_criteria_key, load_axis_emphasis_criteria, load_var_expression, AdataAxis, AxisCriteria};

/// Loads (and caches) the `(min, max)` expression, in the matrix at `matrix_path`, of the var
/// labeled `var_name` (see [`load_var_expression`]) over the obs of `obs_axis` included by
/// `filtering`, or `None` when there is no such var.
#[allow(clippy::too_many_arguments)]
pub async fn load_var_expression_extent(
    gpu_context: Option<&GpuContext<'_>>,
    store: AdataStore,
    store_name: &str,
    var_name: &str,
    var_index_path: Option<&str>,
    matrix_path: &str,
    obs_axis: &AdataAxis,
    filtering: Option<&AxisCriteria>,
    cache_enabled: bool,
) -> Result<Option<(f32, f32)>, ArrayError> {
    let Some(values) = load_var_expression(store.clone(), store_name, var_name, var_index_path, matrix_path, cache_enabled).await? else {
        return Ok(None);
    };
    let mut keys = vec![
        "adata_var_expression_extent".to_string(),
        store_name.to_string(),
        matrix_path.to_string(),
        format!("{var_index_path:?}"),
        var_name.to_string(),
        obs_axis.dataframe_path.clone(),
    ];
    keys.extend(axis_criteria_key(filtering));
    let extent = use_memo_vec_f32(
        async || {
            let filtering_criteria = load_axis_emphasis_criteria(gpu_context, store.clone(), store_name, obs_axis, filtering, cache_enabled).await?;
            let (min, max) = reduce_extent(gpu_context, NumericData::Float32(values), &filtering_criteria, &[]).await.background;
            Ok::<_, ArrayError>(vec![min, max])
        },
        &keys,
        cache_enabled,
    )
    .await?;
    Ok(Some((extent[0], extent[1])))
}
