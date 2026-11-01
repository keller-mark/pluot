use std::sync::Arc;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use pluot_core::{maybe_timeout, FutureExt, Duration};

use pluot_core::colormaps_categorical;
use pluot_core::wgpu;
use pluot_core::zarr::is_timed_out_zarrs_error;
use zarrs::storage::AsyncReadableStorageTraits;
use pluot_core::two::svg::SvgContext;
use pluot_core::render_traits::{CategoricalColormap, DrawToRasterCpu, DrawToRasterGpu, DrawToSvg, ExtentableLayer, MarginParams, PickableLayer, PreparedLayer, ViewParams, resolve_store_name};
use pluot_core::render_types::{CpuContext, CpuRenderPass, PrepareResult};
use pluot_core::gpu_send::GpuRenderPass;
use pluot_core::render_types::GpuContext;
use pluot_core::composite_layers::bar_plot_layer::BarOrientation;
use pluot_core::composite_layers::precomputed_violin_layer::{PrecomputedViolinLayer, PrecomputedViolinLayerParams, PrecomputedViolinPickingInfo, ViolinDensityNorm};
use pluot_core::compute::stratified_violin::{StratifiedViolinInput, ViolinStratumStats};
use pluot_core::compute::ForegroundBackground;
use pluot_core::numeric_data::NumericData;
use pluot_core::LayerExtentResult;
use pluot_core::LayerPickingResult;
use pluot_core::viewport::DataCoord;
use pluot_core::viewport::ScreenCoord;

use crate::adata_paths::node_path;
use crate::dotplot_data::{
    categories_key, combination_label, groupby_title, load_gene_expression, load_nested_categories, obs_rows_key, plotted_categories,
    resolve_gene_columns,
};
use crate::heatmap_data::{load_axis_emphasis_criteria, load_axis_labels, AdataAxis, AxisCriteria, MatrixAxis};
use crate::violin_data::{empty_violin_stats, load_violin_bins, load_violin_stats, violin_bin_edges, violin_bin_starts};

const DEFAULT_NUM_BINS: u32 = 16;

/// Layer params struct for [`AdataZarrViolinLayer`].
#[derive(Serialize, Deserialize, Debug, Clone, TS)]
#[serde(default)]
#[ts(optional_fields = nullable)]
pub struct AdataZarrViolinLayerParams {
    pub layer_id: String,
    pub bounds: Option<MarginParams>,

    /// `Vertical` (the default) lays the categories out along the X axis, with the expression values along the Y axis.
    /// `Horizontal` lays the categories out along the Y axis, from bottom to top, with the expression values along the X axis.
    pub orientation: Option<BarOrientation>,

    // Data keys
    pub store_name: Option<String>,

    /// Path to the matrix of expression values, relative to the store root
    /// (e.g., `X`, `layers/log1p`, or `mod/rna/X`). Defaults to `X`.
    pub layer_path: String,
    /// Paths to categorical `obs` columns, from coarsest to finest (e.g., `["obs/cell_type", "obs/sample"]`).
    /// Each combination of their categories gets one violin, ordered by the first column's
    /// categories, then the second's, etc. (see `obs_filtering` to reorder them). With more than
    /// one `groupby` column, combinations without any obs are omitted.
    pub groupby: Vec<String>,
    /// Gene ID, as it appears in `var_index_path`.
    pub var_name: String,
    /// Path to the `var` column holding `var_name` (e.g., `var/gene_symbols`). `None` means the `var` index.
    pub var_index_path: Option<String>,

    /// Which obs are summarized. `None` includes every obs. When set, categories
    /// without any included obs are omitted, so e.g. a `Categorical` criteria on a
    /// `groupby` column's codes plots only its `included_categories`, in their order.
    pub obs_filtering: Option<AxisCriteria>,
    /// Which of the included obs to emphasize: each violin then summarizes only
    /// the selected obs, in front of a grey violin summarizing every included obs.
    /// `None` emphasizes every obs.
    pub obs_selection: Option<AxisCriteria>,

    /// Whether zero expression values are included in each violin. Defaults to true.
    pub keep_zeros: Option<bool>,
    /// The approximate number of bins along the value axis, which are placed at nice values. Defaults to 16.
    pub num_bins: Option<u32>,
    /// How violin widths are scaled relative to one another. Defaults to `Width`, matching scanpy's `sc.pl.violin`.
    pub density_norm: Option<ViolinDensityNorm>,
    /// The minimum of the value axis. Defaults to the lesser of zero and the smallest expression value.
    pub value_min: Option<f32>,
    /// Colors the violins by the codes of their categories in the first `groupby` column. Defaults to Tableau10.
    pub cmap: Option<CategoricalColormap>,
}

impl Default for AdataZarrViolinLayerParams {
    fn default() -> Self {
        Self {
            layer_id: "".to_string(),
            bounds: None,
            orientation: None,
            store_name: None,
            layer_path: "X".to_string(),
            groupby: vec![],
            var_name: "".to_string(),
            var_index_path: None,
            obs_filtering: None,
            obs_selection: None,
            keep_zeros: None,
            num_bins: None,
            density_norm: None,
            value_min: None,
            cmap: None,
        }
    }
}

pub struct AdataZarrViolinLayer {
    view_params: ViewParams,
    layer_params: AdataZarrViolinLayerParams,
    store: Arc<dyn AsyncReadableStorageTraits>,
    store_name: String,

    inner: Option<PrecomputedViolinLayer>,
    // One per violin, in the order of the inner layer's groups.
    categories: Vec<Vec<String>>,
    summaries: Vec<ViolinStratumStats>,
}

fn to_rgb8([r, g, b, _]: [f32; 4]) -> (u8, u8, u8) {
    ((r * 255.0).round() as u8, (g * 255.0).round() as u8, (b * 255.0).round() as u8)
}

impl AdataZarrViolinLayer {
    pub fn new(view_params: ViewParams, layer_params: AdataZarrViolinLayerParams) -> Self {
        let store_name = resolve_store_name(&layer_params.store_name, &view_params);

        let store = view_params.get_store(&store_name);
        Self {
            view_params,
            layer_params,
            store,
            store_name,
            inner: None,
            categories: Vec::new(),
            summaries: Vec::new(),
        }
    }

    fn matrix_path(&self) -> String {
        node_path(&self.layer_params.layer_path)
    }

    /// Loads the gene's expression, or returns `Err` when its data is not loaded yet, and `Ok(None)` when the gene is not found.
    async fn load_expression(&self, var_index_values: &[String]) -> Result<Option<(u64, Arc<Vec<f32>>)>, ()> {
        let p = &self.layer_params;
        let Some((_, col_index)) = resolve_gene_columns(std::slice::from_ref(&p.var_name), var_index_values).pop() else {
            return Ok(None);
        };
        let matrix_path = self.matrix_path();
        let expression_future = load_gene_expression(self.store.clone(), &self.store_name, col_index, &matrix_path, self.view_params.cache_enabled);
        match maybe_timeout!(expression_future, self.view_params.timeout).await {
            Ok(Ok(expression)) => Ok(Some((col_index, expression))),
            Ok(Err(e)) => {
                if is_timed_out_zarrs_error(&e) {
                    return Err(());
                }
                panic!("Zarrs error loading expression during AdataZarrViolinLayer prepare: {:?}", e);
            }
            Err(_) => Err(()), // Wall-clock timeout from maybe_timeout!
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for AdataZarrViolinLayer {
    async fn prepare(&mut self, gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        let p = &self.layer_params;
        let cache_enabled = self.view_params.cache_enabled;
        let matrix_path = self.matrix_path();
        let obs_axis = AdataAxis::of_matrix(&matrix_path, MatrixAxis::Obs);
        let var_axis = AdataAxis::of_matrix(&matrix_path, MatrixAxis::Var);
        let metadata_future = async {
            futures::try_join!(
                load_axis_labels(self.store.clone(), &self.store_name, &var_axis, p.var_index_path.as_deref(), cache_enabled),
                load_nested_categories(self.store.clone(), &self.store_name, &obs_axis, &p.groupby, cache_enabled),
                load_axis_emphasis_criteria(gpu_context, self.store.clone(), &self.store_name, &obs_axis, p.obs_filtering.as_ref(), cache_enabled),
                load_axis_emphasis_criteria(gpu_context, self.store.clone(), &self.store_name, &obs_axis, p.obs_selection.as_ref(), cache_enabled),
            )
        };
        let (var_index_values, nested, obs_filtering, obs_selection) = match maybe_timeout!(metadata_future, self.view_params.timeout).await {
            Ok(Ok(loaded)) => loaded,
            Ok(Err(e)) => {
                if is_timed_out_zarrs_error(&e) {
                    return PrepareResult { bailed_early: true };
                }
                panic!("Zarrs error during AdataZarrViolinLayer prepare: {:?}", e);
            }
            Err(_) => return PrepareResult { bailed_early: true }, // Wall-clock timeout from maybe_timeout!
        };

        let criteria_key = obs_rows_key(p.obs_filtering.as_ref(), p.obs_selection.as_ref());
        let category_codes = plotted_categories(
            gpu_context,
            &self.store_name,
            &p.groupby,
            &nested,
            p.obs_filtering.as_ref(),
            &obs_filtering,
            &criteria_key,
            cache_enabled,
        )
        .await;
        let Ok(expression) = self.load_expression(&var_index_values).await else {
            return PrepareResult { bailed_early: true };
        };

        let p = &self.layer_params;
        let num_bins = p.num_bins.unwrap_or(DEFAULT_NUM_BINS) as usize;
        let categories: Vec<Vec<String>> = category_codes.iter().map(|&code| nested.combination(code)).collect();
        let strata = nested.strata(&category_codes);
        let (stats, domain, thresholds, bins) = match &expression {
            Some((col_index, expression)) => {
                let values = NumericData::Float32(expression.clone());
                let input = StratifiedViolinInput {
                    values: &values,
                    stratify_by: &strata,
                    filtering_criteria: &obs_filtering,
                    selection_criteria: &obs_selection,
                    keep_zeros: p.keep_zeros.unwrap_or(true),
                };
                let mut keys = vec![self.store_name.clone(), matrix_path.clone(), col_index.to_string(), input.keep_zeros.to_string()];
                keys.extend(categories_key(&p.groupby, &category_codes));
                keys.extend(criteria_key.iter().cloned());
                let stats = load_violin_stats(gpu_context, &input, &keys, cache_enabled).await;
                let extents = stats.background.iter().chain(&stats.foreground).filter_map(|stats| stats.non_outlier_extent);
                let (domain, thresholds) = violin_bin_edges(extents, num_bins, p.value_min);
                let bins = load_violin_bins(gpu_context, &input, &stats, domain, &thresholds, &keys, cache_enabled).await;
                (stats.as_ref().clone(), domain, thresholds, bins.as_ref().clone())
            }
            None => {
                let (domain, thresholds) = violin_bin_edges([], num_bins, p.value_min);
                let no_counts = vec![vec![0; thresholds.len() + 1]; categories.len()];
                (empty_violin_stats(categories.len()), domain, thresholds, ForegroundBackground { background: no_counts.clone(), foreground: no_counts })
            }
        };
        let has_selection = p.obs_selection.is_some();
        let quartiles = stats.foreground.iter().map(|stats| stats.quartiles).collect();

        let cmap = p.cmap.unwrap_or(CategoricalColormap::Tableau10);
        let fill_colors = category_codes.iter().enumerate()
            .map(|(index, &code)| to_rgb8(colormaps_categorical::sample(cmap, nested.first_column_code(code).unwrap_or(index))))
            .collect();

        let mut violin_layer = PrecomputedViolinLayer::new(
            self.view_params.clone(),
            PrecomputedViolinLayerParams {
                layer_id: format!("{}_precomputed_violin_sublayer", p.layer_id),
                bounds: p.bounds.clone(),
                orientation: p.orientation.clone(),
                groups: Arc::new(categories.iter().map(|combination| combination_label(combination)).collect()),
                bin_starts: Arc::new(violin_bin_starts(domain, &thresholds)),
                bin_counts: Arc::new(bins.foreground),
                background_bin_counts: has_selection.then(|| Arc::new(bins.background)),
                density_norm: p.density_norm,
                quartiles: Some(Arc::new(quartiles)),
                value_domain: Some(domain),
                fill_colors: Some(Arc::new(fill_colors)),
                group_axis_title: Some(groupby_title(&p.groupby)),
                value_axis_title: Some(p.var_name.clone()),
                ..Default::default()
            },
        );
        let prepare_result = violin_layer.prepare(gpu_context).await;
        self.inner = Some(violin_layer);
        self.categories = categories;
        self.summaries = stats.foreground;

        prepare_result
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for AdataZarrViolinLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut GpuRenderPass<'_>) {
        if let Some(inner) = &self.inner {
            DrawToRasterGpu::draw(inner, gpu_context, pass).await;
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for AdataZarrViolinLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for AdataZarrViolinLayer {
    async fn draw(&self, ctx: &mut SvgContext) {
        if let Some(inner) = &self.inner {
            DrawToSvg::draw(inner, ctx).await
        }
    }
}

impl ExtentableLayer for AdataZarrViolinLayer {
    fn extent(&self) -> Option<LayerExtentResult> {
        let inner_extent = self.inner.as_ref()?.extent()?;
        Some(LayerExtentResult { layer_id: self.layer_params.layer_id.clone(), ..inner_extent })
    }
}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
pub struct AdataZarrViolinPickingInfo {
    pub var_name: String,
    /// The picked violin's category in each `groupby` column.
    pub obs_values: Vec<String>,
    /// (first quartile, median, third quartile), or `None` when the category has no values.
    pub quartiles: Option<(f32, f32, f32)>,
    /// The number of observations in the category, including outliers (and excluding zeros, unless `keep_zeros`).
    pub num_values: u32,
}

impl PickableLayer for AdataZarrViolinLayer {
    fn pick(&self, screen_coord: ScreenCoord, data_coord: Option<DataCoord>) -> Option<LayerPickingResult> {
        let inner_result = self.inner.as_ref()?.pick(screen_coord, data_coord)?;
        let violin: PrecomputedViolinPickingInfo = inner_result.into_info()?;
        let summary = self.summaries.get(violin.index)?;
        Some(LayerPickingResult::new(&self.layer_params.layer_id, AdataZarrViolinPickingInfo {
            var_name: self.layer_params.var_name.clone(),
            obs_values: self.categories.get(violin.index)?.clone(),
            quartiles: summary.quartiles,
            num_values: summary.num_values,
        }))
    }
}
