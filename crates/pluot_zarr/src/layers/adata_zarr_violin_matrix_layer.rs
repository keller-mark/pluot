use std::sync::Arc;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use pluot_core::{maybe_timeout, FutureExt, Duration};

use pluot_core::wgpu;
use pluot_core::zarr::is_timed_out_zarrs_error;
use zarrs::storage::AsyncReadableStorageTraits;
use pluot_core::two::svg::SvgContext;
use pluot_core::render_traits::{DrawToRasterCpu, DrawToRasterGpu, DrawToSvg, ExtentableLayer, MarginParams, PickableLayer, PreparedLayer, QuantitativeColormap, ViewParams, resolve_store_name};
use pluot_core::render_types::{CpuContext, CpuRenderPass, PrepareResult};
use pluot_core::gpu_send::GpuRenderPass;
use pluot_core::render_types::GpuContext;
use pluot_core::composite_layers::precomputed_violin_layer::ViolinDensityNorm;
use pluot_core::composite_layers::precomputed_violin_matrix_layer::{PrecomputedViolinMatrixLayer, PrecomputedViolinMatrixLayerParams, PrecomputedViolinMatrixPickingInfo};
use pluot_core::compute::reduce_stratified::StratifyBy;
use pluot_core::compute::stratified_violin::{StratifiedViolinInput, ViolinStratumStats};
use pluot_core::numeric_data::NumericData;
use pluot_core::render_traits::EmphasisCriteria;
use pluot_core::LayerPickingResult;
use pluot_core::viewport::DataCoord;
use pluot_core::viewport::ScreenCoord;

use crate::dotplot_data::{
    categories_with_included_obs, category_strata, expr_array_path, filter_genes, genes_among, load_gene_expression, load_obs_categorical, load_var_names,
    obs_rows_key, resolve_gene_columns, AdataStore,
};
use crate::heatmap_data::{indices_of, load_axis_emphasis_criteria, load_criteria_indices, AxisCriteria, MatrixAxis};
use crate::violin_data::{empty_violin_stats, load_violin_bins, load_violin_stats, violin_bin_edges, violin_bin_starts, ViolinBinCounts, ViolinStats};

const DEFAULT_NUM_BINS: u32 = 16;

/// The obs that every gene's violins summarize, per category.
struct CategoryObs<'a> {
    strata: &'a StratifyBy,
    filtering_criteria: &'a [EmphasisCriteria],
    selection_criteria: &'a [EmphasisCriteria],
    /// Identifies the categories and both criteria, for caching.
    key: Vec<String>,
}

/// One gene's expression values, and the stats of their violins.
struct GeneViolins {
    values: NumericData,
    stats: Arc<ViolinStats>,
    keys: Vec<String>,
}

/// Layer params struct for [`AdataZarrViolinMatrixLayer`].
#[derive(Serialize, Deserialize, Debug, Clone, TS)]
#[serde(default)]
#[ts(optional_fields = nullable)]
pub struct AdataZarrViolinMatrixLayerParams {
    pub layer_id: String,
    pub bounds: Option<MarginParams>,

    /// By default, the X axis contains `var_names` and the Y axis the `groupby` categories.
    /// When true, the X axis contains the `groupby` categories and the Y axis `var_names`.
    /// Defaults to false.
    pub swap_axes: Option<bool>,

    // Data keys
    /// Must point to the root of an anndata object.
    pub store_name: Option<String>,

    /// Layer in AnnData to use for expression values. `None` (or `"X"`) uses adata.X.
    pub layer: Option<String>,
    /// The categorical `obs` column whose categories each get one row (or column, when `swap_axes`).
    // TODO: support multiple groupby (a list of columns from coarse to fine).
    pub groupby: String,
    /// The `groupby` categories to plot, in order along the group axis. `None` means every category, in its stored order.
    pub categories: Option<Vec<String>>,
    /// Gene IDs from adata.var.index (or from the `gene_symbols` column, when set), in order along the var axis.
    pub var_names: Vec<String>,
    /// Key in adata.var to use for gene symbols, if different from adata.var.index.
    pub gene_symbols: Option<String>,

    /// Which obs are summarized. `None` includes every obs. When set, categories
    /// without any included obs are omitted.
    pub obs_filtering: Option<AxisCriteria>,
    /// Which of the included obs to emphasize: each violin then summarizes only
    /// the selected obs, in front of a grey violin summarizing every included obs.
    /// `None` emphasizes every obs.
    pub obs_selection: Option<AxisCriteria>,
    /// Which of `var_names` to include. `None` includes every one.
    pub var_filtering: Option<AxisCriteria>,
    /// Which of the included `var_names` to emphasize. The violins of the others
    /// are greyed out. `None` emphasizes every one.
    pub var_selection: Option<AxisCriteria>,

    /// Whether zero expression values are included in each violin. Defaults to true.
    pub keep_zeros: Option<bool>,
    /// The approximate number of bins along the value axis, which are placed at nice values. Defaults to 16.
    pub num_bins: Option<u32>,
    /// How violin widths are scaled relative to one another. Defaults to `Width`, matching scanpy's `sc.pl.stacked_violin`.
    pub density_norm: Option<ViolinDensityNorm>,
    /// The minimum of the value axis, shared by every violin. Defaults to the lesser of zero and the smallest expression value.
    pub value_min: Option<f32>,
    /// Colors each violin by its median expression. Defaults to Blues, matching scanpy's `sc.pl.stacked_violin`.
    pub cmap: Option<QuantitativeColormap>,

    /// Title of the axis listing `var_names`. `None` draws no title.
    pub var_axis_title: Option<String>,
    /// Title of the axis listing the `groupby` categories. Defaults to `groupby`.
    pub group_axis_title: Option<String>,
}

impl Default for AdataZarrViolinMatrixLayerParams {
    fn default() -> Self {
        Self {
            layer_id: "".to_string(),
            bounds: None,
            swap_axes: None,
            store_name: None,
            layer: None,
            groupby: "".to_string(),
            categories: None,
            var_names: vec![],
            gene_symbols: None,
            obs_filtering: None,
            obs_selection: None,
            var_filtering: None,
            var_selection: None,
            keep_zeros: None,
            num_bins: None,
            density_norm: None,
            value_min: None,
            cmap: None,
            var_axis_title: None,
            group_axis_title: None,
        }
    }
}

pub struct AdataZarrViolinMatrixLayer {
    view_params: ViewParams,
    layer_params: AdataZarrViolinMatrixLayerParams,
    store: AdataStore,
    store_name: String,

    inner: Option<PrecomputedViolinMatrixLayer>,
    // One per cell, in the order of the inner layer's `bin_counts`.
    summaries: Vec<ViolinStratumStats>,
}

impl AdataZarrViolinMatrixLayer {
    pub fn new(view_params: ViewParams, layer_params: AdataZarrViolinMatrixLayerParams) -> Self {
        let store_name = resolve_store_name(&layer_params.store_name, &view_params);

        let store: Arc<dyn AsyncReadableStorageTraits> = view_params.get_store(&store_name);
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

    fn var_colname(&self) -> &str {
        self.layer_params.gene_symbols.as_deref().unwrap_or("index")
    }

    fn violin_input<'a>(&self, values: &'a NumericData, obs: &'a CategoryObs<'a>) -> StratifiedViolinInput<'a> {
        StratifiedViolinInput {
            values,
            stratify_by: obs.strata,
            filtering_criteria: obs.filtering_criteria,
            selection_criteria: obs.selection_criteria,
            keep_zeros: self.layer_params.keep_zeros.unwrap_or(true),
        }
    }

    /// Loads one gene's expression and the stats of its violins, or returns `None` when its data is not loaded yet.
    async fn load_gene_violins(&self, gpu_context: Option<&GpuContext<'_>>, gene_name: &str, col_index: u64, obs: &CategoryObs<'_>) -> Option<GeneViolins> {
        let cache_enabled = self.view_params.cache_enabled;
        let expression = match load_gene_expression(self.store.clone(), &self.store_name, self.var_colname(), gene_name, col_index, self.array_layer(), cache_enabled).await {
            Ok(expression) => expression,
            Err(e) => {
                if is_timed_out_zarrs_error(&e) {
                    return None;
                }
                panic!("Zarrs error loading expression during AdataZarrViolinMatrixLayer prepare: {:?}", e);
            }
        };
        let values = NumericData::Float32(expression);
        let input = self.violin_input(&values, obs);
        let mut keys = vec![
            self.store_name.clone(),
            expr_array_path(self.array_layer()),
            self.var_colname().to_string(),
            gene_name.to_string(),
            input.keep_zeros.to_string(),
        ];
        keys.extend(obs.key.iter().cloned());
        let stats = load_violin_stats(gpu_context, &input, &keys, cache_enabled).await;
        Some(GeneViolins { values, stats, keys })
    }

    /// The violins of every gene of `var_names` per category, as one stats and one bin counts per
    /// cell (see [`pluot_core::composite_layers::precomputed_violin_matrix_layer::violin_matrix_cell_index`]),
    /// as background and foreground, over bins shared by every cell. Genes that are not found,
    /// or whose data is not loaded yet, yield empty violins. Also returns whether any gene's data
    /// is not loaded yet.
    async fn load_cell_violins(
        &self,
        gpu_context: Option<&GpuContext<'_>>,
        var_names: &[String],
        num_categories: usize,
        obs: &CategoryObs<'_>,
        var_index_values: &[String],
    ) -> (ViolinStats, ViolinBinCounts, (f32, f32), Vec<f64>, bool) {
        let p = &self.layer_params;
        let resolved_genes = resolve_gene_columns(var_names, var_index_values);
        let mut gene_results: Vec<Option<GeneViolins>> = (0..resolved_genes.len()).map(|_| None).collect();
        let gene_futures = resolved_genes.iter().zip(gene_results.iter_mut()).map(|((gene_name, col_index), slot)| async move {
            *slot = self.load_gene_violins(gpu_context, gene_name, *col_index, obs).await;
        });
        // Whether or not the wall clock fires, each slot holds `Some` exactly when its gene finished.
        let _ = maybe_timeout!(futures::future::join_all(gene_futures), self.view_params.timeout).await;
        let genes_bailed = gene_results.iter().any(Option::is_none);

        let extents = gene_results.iter().flatten().flat_map(|gene| gene.stats.background.iter().chain(&gene.stats.foreground)).filter_map(|stats| stats.non_outlier_extent);
        let (domain, thresholds) = violin_bin_edges(extents.collect::<Vec<_>>(), p.num_bins.unwrap_or(DEFAULT_NUM_BINS) as usize, p.value_min);
        let mut gene_bins: Vec<Option<Arc<ViolinBinCounts>>> = Vec::with_capacity(gene_results.len());
        for gene in &gene_results {
            gene_bins.push(match gene {
                Some(gene) => {
                    let input = self.violin_input(&gene.values, obs);
                    Some(load_violin_bins(gpu_context, &input, &gene.stats, domain, &thresholds, &gene.keys, self.view_params.cache_enabled).await)
                }
                None => None,
            });
        }

        let empty_stats = empty_violin_stats(num_categories);
        let empty_bins = vec![vec![0; thresholds.len() + 1]; num_categories];
        let gene_index_of_var: Vec<Option<usize>> =
            var_names.iter().map(|var_name| resolved_genes.iter().position(|(gene_name, _)| gene_name == var_name).filter(|&index| gene_results[index].is_some())).collect();
        let cells = |side: fn(&ViolinStats) -> &Vec<ViolinStratumStats>, bin_side: fn(&ViolinBinCounts) -> &Vec<Vec<u32>>| {
            let mut stats = Vec::with_capacity(num_categories * var_names.len());
            let mut bins = Vec::with_capacity(num_categories * var_names.len());
            for category_index in 0..num_categories {
                for gene_index in &gene_index_of_var {
                    let (gene_stats, gene_bins) = match gene_index {
                        Some(index) => (side(&gene_results[*index].as_ref().unwrap().stats), bin_side(gene_bins[*index].as_ref().unwrap())),
                        None => (side(&empty_stats), &empty_bins),
                    };
                    stats.push(gene_stats[category_index]);
                    bins.push(gene_bins[category_index].clone());
                }
            }
            (stats, bins)
        };
        let (background_stats, background_bins) = cells(|stats| &stats.background, |bins| &bins.background);
        let (foreground_stats, foreground_bins) = cells(|stats| &stats.foreground, |bins| &bins.foreground);
        (
            ViolinStats { background: background_stats, foreground: foreground_stats },
            ViolinBinCounts { background: background_bins, foreground: foreground_bins },
            domain,
            thresholds,
            genes_bailed,
        )
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for AdataZarrViolinMatrixLayer {
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
                load_criteria_indices(gpu_context, self.store.clone(), &self.store_name, &matrix_path, MatrixAxis::Var, p.var_filtering.as_ref(), cache_enabled),
                load_criteria_indices(gpu_context, self.store.clone(), &self.store_name, &matrix_path, MatrixAxis::Var, p.var_selection.as_ref(), cache_enabled),
            )
        };
        let (var_index_values, (obs_categories, obs_codes), obs_filtering, obs_selection, var_filtered, var_selected) =
            match maybe_timeout!(metadata_future, self.view_params.timeout).await {
                Ok(Ok(loaded)) => loaded,
                Ok(Err(e)) => {
                    if is_timed_out_zarrs_error(&e) {
                        return PrepareResult { bailed_early: true };
                    }
                    panic!("Zarrs error during AdataZarrViolinMatrixLayer prepare: {:?}", e);
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
        let strata = category_strata(&categories, &obs_categories, &obs_codes);
        let mut obs_key = vec![p.groupby.clone()];
        obs_key.extend(categories.iter().cloned());
        obs_key.extend(criteria_key);
        let obs = CategoryObs { strata: &strata, filtering_criteria: &obs_filtering, selection_criteria: &obs_selection, key: obs_key };

        let var_names = filter_genes(&p.var_names, &var_index_values, var_filtered.as_deref().map(indices_of));
        let var_selection = var_selected.as_deref().map(|selected| Arc::new(genes_among(&var_names, &var_index_values, indices_of(selected))));
        let (stats, bins, domain, thresholds, genes_bailed) = self.load_cell_violins(gpu_context, &var_names, categories.len(), &obs, &var_index_values).await;

        let p = &self.layer_params;
        let has_selection = p.obs_selection.is_some();
        let medians = stats.foreground.iter()
            .map(|stats| stats.quartiles.map(|(_, median, _)| median))
            .collect();

        let mut violin_matrix_layer = PrecomputedViolinMatrixLayer::new(
            self.view_params.clone(),
            PrecomputedViolinMatrixLayerParams {
                layer_id: format!("{}_precomputed_violin_matrix_sublayer", p.layer_id),
                bounds: p.bounds.clone(),
                swap_axes: p.swap_axes,
                var_names: Arc::new(var_names),
                groups: Arc::new(categories),
                bin_starts: Arc::new(violin_bin_starts(domain, &thresholds)),
                bin_counts: Arc::new(bins.foreground),
                background_bin_counts: has_selection.then(|| Arc::new(bins.background)),
                density_norm: p.density_norm,
                var_selection,
                value_domain: Some(domain),
                fill_values: Some(Arc::new(medians)),
                cmap: p.cmap,
                fill_value_title: Some("Median expression".to_string()),
                var_axis_title: p.var_axis_title.clone(),
                group_axis_title: Some(p.group_axis_title.clone().unwrap_or_else(|| p.groupby.clone())),
                ..Default::default()
            },
        );
        let prepare_result = violin_matrix_layer.prepare(gpu_context).await;
        self.inner = Some(violin_matrix_layer);
        self.summaries = stats.foreground;

        PrepareResult { bailed_early: genes_bailed || prepare_result.bailed_early }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for AdataZarrViolinMatrixLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut GpuRenderPass<'_>) {
        if let Some(inner) = &self.inner {
            DrawToRasterGpu::draw(inner, gpu_context, pass).await;
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for AdataZarrViolinMatrixLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for AdataZarrViolinMatrixLayer {
    async fn draw(&self, ctx: &mut SvgContext) {
        if let Some(inner) = &self.inner {
            DrawToSvg::draw(inner, ctx).await
        }
    }
}

impl ExtentableLayer for AdataZarrViolinMatrixLayer {}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
pub struct AdataZarrViolinMatrixPickingInfo {
    pub var_name: String,
    pub obs_value: String,
    /// (first quartile, median, third quartile), or `None` when the cell has no values.
    pub quartiles: Option<(f32, f32, f32)>,
    /// The number of observations in the cell, including outliers (and excluding zeros, unless `keep_zeros`).
    pub num_values: u32,
}

impl PickableLayer for AdataZarrViolinMatrixLayer {
    fn pick(&self, screen_coord: ScreenCoord, data_coord: Option<DataCoord>) -> Option<LayerPickingResult> {
        let inner_result = self.inner.as_ref()?.pick(screen_coord, data_coord)?;
        let cell: PrecomputedViolinMatrixPickingInfo = inner_result.into_info()?;
        let summary = self.summaries.get(cell.index)?;
        Some(LayerPickingResult::new(&self.layer_params.layer_id, AdataZarrViolinMatrixPickingInfo {
            var_name: cell.var_name,
            obs_value: cell.group,
            quartiles: summary.quartiles,
            num_values: summary.num_values,
        }))
    }
}
