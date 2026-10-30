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

use crate::dotplot_data::{
    categories_with_included_obs, category_strata, expr_array_path, load_gene_expression, load_obs_categorical, load_var_names, obs_rows_key,
    resolve_gene_columns,
};
use crate::heatmap_data::{load_axis_emphasis_criteria, AxisCriteria, MatrixAxis};
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
    /// Must point to the root of an anndata object.
    pub store_name: Option<String>,

    /// Layer in AnnData to use for expression values. `None` (or `"X"`) uses adata.X.
    pub layer: Option<String>,
    /// The categorical `obs` column whose categories each get one violin.
    // TODO: support multiple groupby (a list of columns from coarse to fine).
    pub groupby: String,
    /// The `groupby` categories to plot, in order along the X axis. `None` means every category, in its stored order.
    pub categories: Option<Vec<String>>,
    /// Gene ID from adata.var.index (or from the `gene_symbols` column, when set).
    pub var_name: String,
    /// Key in adata.var to use for gene symbols, if different from adata.var.index.
    pub gene_symbols: Option<String>,

    /// Which obs are summarized. `None` includes every obs. When set, categories
    /// without any included obs are omitted.
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
    /// Colors the violins by the codes of their categories. Defaults to Tableau10.
    pub cmap: Option<CategoricalColormap>,
}

impl Default for AdataZarrViolinLayerParams {
    fn default() -> Self {
        Self {
            layer_id: "".to_string(),
            bounds: None,
            orientation: None,
            store_name: None,
            layer: None,
            groupby: "".to_string(),
            categories: None,
            var_name: "".to_string(),
            gene_symbols: None,
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
            summaries: Vec::new(),
        }
    }

    fn array_layer(&self) -> Option<&str> {
        self.layer_params.layer.as_deref().filter(|layer| *layer != "X")
    }

    /// Loads the gene's expression, or returns `Err` when its data is not loaded yet, and `Ok(None)` when the gene is not found.
    async fn load_expression(&self, var_index_values: &[String]) -> Result<Option<(String, Arc<Vec<f32>>)>, ()> {
        let p = &self.layer_params;
        let Some((gene_name, col_index)) = resolve_gene_columns(std::slice::from_ref(&p.var_name), var_index_values).pop() else {
            return Ok(None);
        };
        let var_colname = p.gene_symbols.as_deref().unwrap_or("index");
        let expression_future = load_gene_expression(self.store.clone(), &self.store_name, var_colname, &gene_name, col_index, self.array_layer(), self.view_params.cache_enabled);
        match maybe_timeout!(expression_future, self.view_params.timeout).await {
            Ok(Ok(expression)) => Ok(Some((gene_name, expression))),
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
        let matrix_path = expr_array_path(self.array_layer());
        let metadata_future = async {
            futures::try_join!(
                load_var_names(self.store.clone(), &self.store_name, p.gene_symbols.as_deref(), cache_enabled),
                load_obs_categorical(self.store.clone(), &self.store_name, &p.groupby, cache_enabled),
                load_axis_emphasis_criteria(gpu_context, self.store.clone(), &self.store_name, &matrix_path, MatrixAxis::Obs, p.obs_filtering.as_ref(), cache_enabled),
                load_axis_emphasis_criteria(gpu_context, self.store.clone(), &self.store_name, &matrix_path, MatrixAxis::Obs, p.obs_selection.as_ref(), cache_enabled),
            )
        };
        let (var_index_values, (obs_categories, obs_codes), obs_filtering, obs_selection) = match maybe_timeout!(metadata_future, self.view_params.timeout).await {
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
        let mut categories = p.categories.clone().unwrap_or_else(|| obs_categories.as_ref().clone());
        if p.obs_filtering.is_some() {
            categories = categories_with_included_obs(
                gpu_context, &self.store_name, &p.groupby, categories, &obs_categories, &obs_codes, &obs_filtering, &criteria_key, cache_enabled,
            )
            .await;
        }
        let Ok(expression) = self.load_expression(&var_index_values).await else {
            return PrepareResult { bailed_early: true };
        };

        let p = &self.layer_params;
        let num_bins = p.num_bins.unwrap_or(DEFAULT_NUM_BINS) as usize;
        let strata = category_strata(&categories, &obs_categories, &obs_codes);
        let (stats, domain, thresholds, bins) = match &expression {
            Some((gene_name, expression)) => {
                let values = NumericData::Float32(expression.clone());
                let input = StratifiedViolinInput {
                    values: &values,
                    stratify_by: &strata,
                    filtering_criteria: &obs_filtering,
                    selection_criteria: &obs_selection,
                    keep_zeros: p.keep_zeros.unwrap_or(true),
                };
                let mut keys = vec![
                    self.store_name.clone(),
                    matrix_path.clone(),
                    p.gene_symbols.clone().unwrap_or_else(|| "index".to_string()),
                    gene_name.clone(),
                    p.groupby.clone(),
                    input.keep_zeros.to_string(),
                ];
                keys.extend(categories.iter().cloned());
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
        let fill_colors = categories.iter().enumerate()
            .map(|(index, category)| {
                let code = obs_categories.iter().position(|c| c == category).unwrap_or(index);
                to_rgb8(colormaps_categorical::sample(cmap, code))
            })
            .collect();

        let mut violin_layer = PrecomputedViolinLayer::new(
            self.view_params.clone(),
            PrecomputedViolinLayerParams {
                layer_id: format!("{}_precomputed_violin_sublayer", p.layer_id),
                bounds: p.bounds.clone(),
                orientation: p.orientation.clone(),
                groups: Arc::new(categories),
                bin_starts: Arc::new(violin_bin_starts(domain, &thresholds)),
                bin_counts: Arc::new(bins.foreground),
                background_bin_counts: has_selection.then(|| Arc::new(bins.background)),
                density_norm: p.density_norm,
                quartiles: Some(Arc::new(quartiles)),
                value_domain: Some(domain),
                fill_colors: Some(Arc::new(fill_colors)),
                group_axis_title: Some(p.groupby.clone()),
                value_axis_title: Some(p.var_name.clone()),
                ..Default::default()
            },
        );
        let prepare_result = violin_layer.prepare(gpu_context).await;
        self.inner = Some(violin_layer);
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
    pub obs_value: String,
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
            obs_value: violin.group,
            quartiles: summary.quartiles,
            num_values: summary.num_values,
        }))
    }
}
