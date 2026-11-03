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

use crate::adata_paths::node_path;
use crate::dotplot_data::{
    categories_key, combination_label, genes_selected, groupby_title, included_genes, load_gene_expression, load_nested_categories,
    obs_rows_key, plotted_categories, AdataStore,
};
use crate::heatmap_data::{indices_of, load_axis_emphasis_criteria, load_axis_indices, load_axis_labels, load_criteria_indices, AdataAxis, AxisCriteria, MatrixAxis};
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

    /// By default, the X axis contains the genes and the Y axis the `groupby` categories.
    /// When true, the X axis contains the `groupby` categories and the Y axis the genes.
    /// Defaults to false.
    pub swap_axes: Option<bool>,

    // Data keys
    pub store_name: Option<String>,

    /// Path to the matrix of expression values, relative to the store root
    /// (e.g., `X`, `layers/log1p`, or `mod/rna/X`). Defaults to `X`.
    pub layer_path: String,
    /// Paths to categorical `obs` columns, from coarsest to finest (e.g., `["obs/cell_type", "obs/sample"]`).
    /// Each combination of their categories gets one row (or column, when `swap_axes`), ordered by
    /// the first column's categories, then the second's, etc. (see `obs_filtering` to reorder
    /// them). With more than one `groupby` column, combinations without any obs are omitted.
    pub groupby: Vec<String>,
    /// Path to the `var` column labeling the genes (e.g., `var/gene_symbols`). `None` means the `var` index.
    pub var_index_path: Option<String>,

    /// Which obs are summarized. `None` includes every obs. When set, categories
    /// without any included obs are omitted, so e.g. a `Categorical` criteria on a
    /// `groupby` column's codes plots only its `included_categories`, in their order.
    pub obs_filtering: Option<AxisCriteria>,
    /// Which of the included obs to emphasize: each violin then summarizes only
    /// the selected obs, in front of a grey violin summarizing every included obs.
    /// `None` emphasizes every obs.
    pub obs_selection: Option<AxisCriteria>,
    /// Which var (genes) to include, in order along the var axis. `None` includes every var.
    pub var_filtering: Option<AxisCriteria>,
    /// Which of the included var to emphasize. The violins of the others
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

    /// Title of the axis listing the genes. `None` draws no title.
    pub var_axis_title: Option<String>,
    /// Title of the axis listing the `groupby` categories. Defaults to the names of the `groupby` columns.
    pub group_axis_title: Option<String>,
}

impl Default for AdataZarrViolinMatrixLayerParams {
    fn default() -> Self {
        Self {
            layer_id: "".to_string(),
            bounds: None,
            swap_axes: None,
            store_name: None,
            layer_path: "X".to_string(),
            groupby: vec![],
            var_index_path: None,
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
    // One per row (or column, when `swap_axes`), in the order of the inner layer's groups.
    categories: Vec<Vec<String>>,
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
            categories: Vec::new(),
            summaries: Vec::new(),
        }
    }

    fn matrix_path(&self) -> String {
        node_path(&self.layer_params.layer_path)
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
    async fn load_gene_violins(&self, gpu_context: Option<&GpuContext<'_>>, col_index: u64, obs: &CategoryObs<'_>) -> Option<GeneViolins> {
        let cache_enabled = self.view_params.cache_enabled;
        let matrix_path = self.matrix_path();
        let expression = match load_gene_expression(self.store.clone(), &self.store_name, col_index, &matrix_path, cache_enabled).await {
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
        let mut keys = vec![self.store_name.clone(), matrix_path, col_index.to_string(), input.keep_zeros.to_string()];
        keys.extend(obs.key.iter().cloned());
        let stats = load_violin_stats(gpu_context, &input, &keys, cache_enabled).await;
        Some(GeneViolins { values, stats, keys })
    }

    /// The violins of every gene column of `gene_cols` per category, as one stats and one bin counts per
    /// cell (see [`pluot_core::composite_layers::precomputed_violin_matrix_layer::violin_matrix_cell_index`]),
    /// as background and foreground, over bins shared by every cell. Genes whose data is not
    /// loaded yet yield empty violins. Also returns whether any gene's data is not loaded yet.
    async fn load_cell_violins(
        &self,
        gpu_context: Option<&GpuContext<'_>>,
        gene_cols: &[u64],
        num_categories: usize,
        obs: &CategoryObs<'_>,
    ) -> (ViolinStats, ViolinBinCounts, (f32, f32), Vec<f64>, bool) {
        let p = &self.layer_params;
        let mut gene_results: Vec<Option<GeneViolins>> = (0..gene_cols.len()).map(|_| None).collect();
        let gene_futures = gene_cols.iter().zip(gene_results.iter_mut()).map(|(col_index, slot)| async move {
            *slot = self.load_gene_violins(gpu_context, *col_index, obs).await;
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
        let cells = |side: fn(&ViolinStats) -> &Vec<ViolinStratumStats>, bin_side: fn(&ViolinBinCounts) -> &Vec<Vec<u32>>| {
            let mut stats = Vec::with_capacity(num_categories * gene_cols.len());
            let mut bins = Vec::with_capacity(num_categories * gene_cols.len());
            for category_index in 0..num_categories {
                for (gene, bins_of_gene) in gene_results.iter().zip(&gene_bins) {
                    let (gene_stats, gene_bins) = match (gene, bins_of_gene) {
                        (Some(gene), Some(bins_of_gene)) => (side(&gene.stats), bin_side(bins_of_gene)),
                        _ => (side(&empty_stats), &empty_bins),
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
        let matrix_path = self.matrix_path();
        let obs_axis = AdataAxis::of_matrix(&matrix_path, MatrixAxis::Obs);
        let var_axis = AdataAxis::of_matrix(&matrix_path, MatrixAxis::Var);
        let metadata_future = async {
            futures::try_join!(
                load_axis_labels(self.store.clone(), &self.store_name, &var_axis, p.var_index_path.as_deref(), cache_enabled),
                load_nested_categories(self.store.clone(), &self.store_name, &obs_axis, &p.groupby, cache_enabled),
                load_axis_emphasis_criteria(gpu_context, self.store.clone(), &self.store_name, &obs_axis, p.obs_filtering.as_ref(), cache_enabled),
                load_axis_emphasis_criteria(gpu_context, self.store.clone(), &self.store_name, &obs_axis, p.obs_selection.as_ref(), cache_enabled),
                load_axis_indices(gpu_context, self.store.clone(), &self.store_name, &var_axis, p.var_filtering.as_ref(), cache_enabled),
                load_criteria_indices(gpu_context, self.store.clone(), &self.store_name, &var_axis, p.var_selection.as_ref(), cache_enabled),
            )
        };
        let (var_index_values, nested, obs_filtering, obs_selection, var_filtered, var_selected) =
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
        let categories: Vec<Vec<String>> = category_codes.iter().map(|&code| nested.combination(code)).collect();
        let strata = nested.strata(&category_codes);
        let mut obs_key = categories_key(&p.groupby, &category_codes);
        obs_key.extend(criteria_key);
        let obs = CategoryObs { strata: &strata, filtering_criteria: &obs_filtering, selection_criteria: &obs_selection, key: obs_key };

        let var_included = indices_of(&var_filtered);
        let (var_names, gene_cols): (Vec<String>, Vec<u64>) = included_genes(&var_index_values, var_included).into_iter().unzip();
        let var_selection = var_selected.as_deref().map(|selected| Arc::new(genes_selected(var_included, indices_of(selected), var_index_values.len())));
        let (stats, bins, domain, thresholds, genes_bailed) = self.load_cell_violins(gpu_context, &gene_cols, categories.len(), &obs).await;

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
                groups: Arc::new(categories.iter().map(|combination| combination_label(combination)).collect()),
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
                group_axis_title: Some(p.group_axis_title.clone().unwrap_or_else(|| groupby_title(&p.groupby))),
                ..Default::default()
            },
        );
        let prepare_result = violin_matrix_layer.prepare(gpu_context).await;
        self.inner = Some(violin_matrix_layer);
        self.categories = categories;
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
    /// The picked cell's category in each `groupby` column.
    pub obs_values: Vec<String>,
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
            obs_values: self.categories.iter().find(|combination| combination_label(combination) == cell.group)?.clone(),
            quartiles: summary.quartiles,
            num_values: summary.num_values,
        }))
    }
}
