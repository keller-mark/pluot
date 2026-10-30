use std::collections::HashMap;
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
use pluot_core::numeric_data::NumericData;
use pluot_core::LayerPickingResult;
use pluot_core::viewport::DataCoord;
use pluot_core::viewport::ScreenCoord;

use crate::dotplot_data::{
    background_and_foreground_rows, bucket_rows_by_category, expr_array_path, filter_genes, genes_among, load_gene_expression, load_obs_categorical,
    load_var_names, obs_rows_key, resolve_gene_columns, AdataStore,
};
use crate::heatmap_data::{indices_of, load_criteria_indices, AxisCriteria, MatrixAxis};
use crate::violin_data::{bin_violins, load_violin_summary, ViolinSummary};

const DEFAULT_NUM_BINS: u32 = 16;

/// The obs rows of each category, along with their [`obs_rows_key`].
type ObsRows<'a> = (&'a HashMap<String, Vec<u32>>, Vec<String>);

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
    summaries: Vec<Arc<ViolinSummary>>,
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

    /// Summarizes one gene's expression per category, over each of `rows` (the rows of each
    /// category, along with their [`obs_rows_key`]), or returns `None` when its data is not loaded yet.
    async fn load_gene_summaries(&self, gene_name: &str, col_index: u64, categories: &[String], rows: &[ObsRows<'_>]) -> Option<Vec<Vec<Arc<ViolinSummary>>>> {
        let p = &self.layer_params;
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

        let keep_zeros = p.keep_zeros.unwrap_or(true);
        let summaries_over = |(rows_by_category, rows_key): &ObsRows<'_>| {
            categories.iter()
                .map(|category| {
                    let mut keys = vec![
                        "violin_summary".to_string(),
                        self.store_name.clone(),
                        self.array_layer().unwrap_or("X").to_string(),
                        self.var_colname().to_string(),
                        gene_name.to_string(),
                        p.groupby.clone(),
                        category.clone(),
                        keep_zeros.to_string(),
                    ];
                    keys.extend(rows_key.iter().cloned());
                    load_violin_summary(&expression, &rows_by_category[category], keep_zeros, &keys, cache_enabled)
                })
                .collect()
        };
        Some(rows.iter().map(summaries_over).collect())
    }

    /// Summarizes every gene's expression per category, over each of `rows`, as one summary per
    /// cell (see [`pluot_core::composite_layers::precomputed_violin_matrix_layer::violin_matrix_cell_index`]).
    /// Genes that are not found, or whose data is not loaded yet, yield empty summaries.
    /// Also returns whether any gene's data is not loaded yet.
    async fn load_cell_summaries(&self, categories: &[String], var_names: &[String], rows: &[ObsRows<'_>], var_index_values: &[String]) -> (Vec<Vec<Arc<ViolinSummary>>>, bool) {
        let resolved_genes = resolve_gene_columns(var_names, var_index_values);

        let mut gene_results: Vec<Option<Vec<Vec<Arc<ViolinSummary>>>>> = vec![None; resolved_genes.len()];
        let gene_futures = resolved_genes.iter().zip(gene_results.iter_mut()).map(|((gene_name, col_index), slot)| async move {
            *slot = self.load_gene_summaries(gene_name, *col_index, categories, rows).await;
        });
        // Whether or not the wall clock fires, each slot holds `Some` exactly when its gene finished.
        let _ = maybe_timeout!(futures::future::join_all(gene_futures), self.view_params.timeout).await;
        let genes_bailed = gene_results.iter().any(Option::is_none);

        let empty = Arc::new(ViolinSummary::default());
        let summaries_by_var: Vec<Option<&Vec<Vec<Arc<ViolinSummary>>>>> = var_names.iter()
            .map(|var_name| {
                let resolved_index = resolved_genes.iter().position(|(gene_name, _)| gene_name == var_name)?;
                gene_results[resolved_index].as_ref()
            })
            .collect();
        let cell_summaries = (0..rows.len())
            .map(|rows_index| {
                (0..categories.len())
                    .flat_map(|category_index| summaries_by_var.iter().map(move |summaries| (category_index, summaries)))
                    .map(|(category_index, summaries)| summaries.map_or_else(|| empty.clone(), |summaries| summaries[rows_index][category_index].clone()))
                    .collect()
            })
            .collect();
        (cell_summaries, genes_bailed)
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for AdataZarrViolinMatrixLayer {
    async fn prepare(&mut self, gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        let p = &self.layer_params;
        let cache_enabled = self.view_params.cache_enabled;
        let matrix_path = expr_array_path(self.array_layer());
        let criteria_indices = |axis, criteria| {
            load_criteria_indices(gpu_context, self.store.clone(), &self.store_name, &matrix_path, axis, criteria, cache_enabled)
        };
        let metadata_future = async {
            futures::try_join!(
                load_var_names(self.store.clone(), &self.store_name, p.gene_symbols.as_deref(), cache_enabled),
                load_obs_categorical(self.store.clone(), &self.store_name, &p.groupby, cache_enabled),
                criteria_indices(MatrixAxis::Obs, p.obs_filtering.as_ref()),
                criteria_indices(MatrixAxis::Obs, p.obs_selection.as_ref()),
                criteria_indices(MatrixAxis::Var, p.var_filtering.as_ref()),
                criteria_indices(MatrixAxis::Var, p.var_selection.as_ref()),
            )
        };
        let (var_index_values, (obs_categories, obs_codes), obs_filtered, obs_selected, var_filtered, var_selected) = match maybe_timeout!(metadata_future, self.view_params.timeout).await {
            Ok(Ok(loaded)) => loaded,
            Ok(Err(e)) => {
                if is_timed_out_zarrs_error(&e) {
                    return PrepareResult { bailed_early: true };
                }
                panic!("Zarrs error during AdataZarrViolinMatrixLayer prepare: {:?}", e);
            }
            Err(_) => return PrepareResult { bailed_early: true }, // Wall-clock timeout from maybe_timeout!
        };

        let mut categories = p.categories.clone().unwrap_or_else(|| obs_categories.as_ref().clone());
        let (background_rows, foreground_rows) = background_and_foreground_rows(
            bucket_rows_by_category(&categories, &obs_categories, &obs_codes),
            obs_filtered.as_deref().map(indices_of),
            obs_selected.as_deref().map(indices_of),
            obs_codes.len(),
        );
        if obs_filtered.is_some() {
            categories.retain(|category| !background_rows[category].is_empty());
        }
        let mut rows = vec![(&background_rows, obs_rows_key(p.obs_filtering.as_ref(), None))];
        if let Some(foreground_rows) = &foreground_rows {
            rows.push((foreground_rows, obs_rows_key(p.obs_filtering.as_ref(), p.obs_selection.as_ref())));
        }
        let var_names = filter_genes(&p.var_names, &var_index_values, var_filtered.as_deref().map(indices_of));
        let var_selection = var_selected.as_deref().map(|selected| Arc::new(genes_among(&var_names, &var_index_values, indices_of(selected))));
        let (mut summaries_per_rows, genes_bailed) = self.load_cell_summaries(&categories, &var_names, &rows, &var_index_values).await;

        let p = &self.layer_params;
        // Binned together, so that the background and foreground violins share their bins.
        let all_summaries: Vec<&ViolinSummary> = summaries_per_rows.iter().flatten().map(Arc::as_ref).collect();
        let mut bins = bin_violins(&all_summaries, p.num_bins.unwrap_or(DEFAULT_NUM_BINS) as usize, p.value_min);
        let summaries = summaries_per_rows.pop().unwrap_or_default();
        let bin_counts = bins.bin_counts.split_off(bins.bin_counts.len() - summaries.len());
        let background_bin_counts = foreground_rows.is_some().then(|| Arc::new(bins.bin_counts));
        let medians = summaries.iter()
            .map(|summary| summary.quartiles.map(|(_, median, _)| median))
            .collect();

        let mut violin_matrix_layer = PrecomputedViolinMatrixLayer::new(
            self.view_params.clone(),
            PrecomputedViolinMatrixLayerParams {
                layer_id: format!("{}_precomputed_violin_matrix_sublayer", p.layer_id),
                bounds: p.bounds.clone(),
                swap_axes: p.swap_axes,
                var_names: Arc::new(var_names),
                groups: Arc::new(categories),
                bin_starts: Arc::new(bins.bin_starts),
                bin_counts: Arc::new(bin_counts),
                background_bin_counts,
                density_norm: p.density_norm,
                var_selection,
                value_domain: Some(bins.value_domain),
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
        self.summaries = summaries;

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
