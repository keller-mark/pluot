// Derives the colormap domain of one var's expression, and opacity thresholds at its percentiles, for `AdataZarrHilbertHeatmapLayer`.

use pluot_core::cache::use_memo_vec_f32;
use pluot_core::layers::hilbert_heatmap_layer::OpacityThreshold;
use pluot_core::compute::histogram_percentile::PercentileSearch;
use pluot_core::compute::reduce::{reduce_extent, reduce_histogram_with_known_extent};
use pluot_core::numeric_data::NumericData;
use pluot_core::render_traits::EmphasisCriteria;
use pluot_core::render_types::GpuContext;
use zarrs::array::ArrayError;

use crate::dotplot_data::AdataStore;
use crate::heatmap_data::{axis_criteria_key, load_axis_emphasis_criteria, load_var_expression, AdataAxis, AxisCriteria};
use crate::layers::adata_zarr_hilbert_heatmap_layer::PercentileOpacity;

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

const PERCENTILE_NUM_BINS: u32 = 256;
/// Each pass after the first narrows a percentile's range by up to `PERCENTILE_NUM_BINS` times.
const PERCENTILE_NUM_PASSES: usize = 2;

/// Loads (and caches) the [`OpacityThreshold`] of each of `percentile_opacities`, at its
/// percentile of the expression of the var labeled `var_name` (see [`load_var_expression`])
/// among the obs of `obs_axis` included by `filtering`, or `None` when there is no such var.
#[allow(clippy::too_many_arguments)]
pub async fn load_var_value_opacity_thresholds(
    gpu_context: Option<&GpuContext<'_>>,
    store: AdataStore,
    store_name: &str,
    var_name: &str,
    var_index_path: Option<&str>,
    matrix_path: &str,
    obs_axis: &AdataAxis,
    filtering: Option<&AxisCriteria>,
    percentile_opacities: &[PercentileOpacity],
    cache_enabled: bool,
) -> Result<Option<Vec<OpacityThreshold>>, ArrayError> {
    let Some(values) = load_var_expression(store.clone(), store_name, var_name, var_index_path, matrix_path, cache_enabled).await? else {
        return Ok(None);
    };
    let mut keys = vec![
        "adata_var_value_opacity_thresholds".to_string(),
        store_name.to_string(),
        matrix_path.to_string(),
        format!("{var_index_path:?}"),
        var_name.to_string(),
        obs_axis.dataframe_path.clone(),
        format!("{percentile_opacities:?}"),
    ];
    keys.extend(axis_criteria_key(filtering));
    let flat_thresholds = use_memo_vec_f32(
        async || {
            let filtering_criteria = load_axis_emphasis_criteria(gpu_context, store.clone(), store_name, obs_axis, filtering, cache_enabled).await?;
            let thresholds = var_opacity_thresholds(gpu_context, &NumericData::Float32(values.clone()), &filtering_criteria, percentile_opacities).await;
            Ok::<_, ArrayError>(thresholds.iter().flat_map(|threshold| [threshold.percentile, threshold.value, threshold.opacity]).collect())
        },
        &keys,
        cache_enabled,
    )
    .await?;
    Ok(Some(flat_thresholds.chunks_exact(3).map(|triple| OpacityThreshold { percentile: triple[0], value: triple[1], opacity: triple[2] }).collect()))
}

/// The threshold of each of `percentile_opacities`, ordered by percentile, whose `value` is
/// exceeded by those values with at least `percentile`% of the included values lower than them.
/// The percentiles are estimated from histograms rather than by sorting, so this runs on the GPU when there is one.
pub async fn var_opacity_thresholds(
    gpu_context: Option<&GpuContext<'_>>,
    values: &NumericData,
    filtering_criteria: &[EmphasisCriteria],
    percentile_opacities: &[PercentileOpacity],
) -> Vec<OpacityThreshold> {
    let mut percentile_opacities = percentile_opacities.to_vec();
    percentile_opacities.sort_by(|a, b| a.percentile.total_cmp(&b.percentile));
    let extent = reduce_extent(gpu_context, values.clone(), filtering_criteria, &[]).await.background;
    if !(extent.0.is_finite() && extent.1.is_finite()) {
        return vec![];
    }
    let histogram = async |(lo, hi): (f32, f32)| {
        reduce_histogram_with_known_extent(gpu_context, values.clone(), PERCENTILE_NUM_BINS, lo, hi, filtering_criteria, &[]).await.background
    };
    let mut thresholds = Vec::with_capacity(percentile_opacities.len());
    for PercentileOpacity { percentile, opacity } in percentile_opacities {
        let value = if percentile <= 0.0 {
            f32::MIN
        } else {
            let mut search = PercentileSearch::new(extent, percentile);
            for _ in 1..PERCENTILE_NUM_PASSES {
                search.narrow(&histogram(search.range()).await);
            }
            search.estimate(&histogram(search.range()).await)
        };
        thresholds.push(OpacityThreshold { percentile, value, opacity });
    }
    thresholds
}
