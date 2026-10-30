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

use crate::dotplot_data::{bucket_rows_by_category, load_gene_expression, load_obs_categorical, load_var_names, resolve_gene_columns, AdataStore};
use crate::violin_data::{bin_violins, load_violin_summary, ViolinSummary};

const DEFAULT_NUM_BINS: u32 = 16;

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

    /// Summarizes one gene's expression per category, or returns `None` when its data is not loaded yet.
    async fn load_gene_summaries(&self, gene_name: &str, col_index: u64, categories: &[String], rows_by_category: &HashMap<String, Vec<u32>>) -> Option<Vec<Arc<ViolinSummary>>> {
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
        let summaries = categories.iter()
            .map(|category| {
                let keys = vec![
                    "violin_summary".to_string(),
                    self.store_name.clone(),
                    self.array_layer().unwrap_or("X").to_string(),
                    self.var_colname().to_string(),
                    gene_name.to_string(),
                    p.groupby.clone(),
                    category.clone(),
                    keep_zeros.to_string(),
                ];
                load_violin_summary(&expression, &rows_by_category[category], keep_zeros, &keys, cache_enabled)
            })
            .collect();
        Some(summaries)
    }

    /// Summarizes every gene's expression per category, as one summary per cell (see
    /// [`pluot_core::composite_layers::precomputed_violin_matrix_layer::violin_matrix_cell_index`]).
    /// Genes that are not found, or whose data is not loaded yet, yield empty summaries.
    /// Also returns whether any gene's data is not loaded yet.
    async fn load_cell_summaries(&self, categories: &[String], obs_categories: &[String], obs_codes: &NumericData, var_index_values: &[String]) -> (Vec<Arc<ViolinSummary>>, bool) {
        let var_names = &self.layer_params.var_names;
        let rows_by_category = bucket_rows_by_category(categories, obs_categories, obs_codes);
        let resolved_genes = resolve_gene_columns(var_names, var_index_values);

        let mut gene_results: Vec<Option<Vec<Arc<ViolinSummary>>>> = vec![None; resolved_genes.len()];
        let gene_futures = resolved_genes.iter().zip(gene_results.iter_mut()).map(|((gene_name, col_index), slot)| {
            let rows_by_category = &rows_by_category;
            async move {
                *slot = self.load_gene_summaries(gene_name, *col_index, categories, rows_by_category).await;
            }
        });
        // Whether or not the wall clock fires, each slot holds `Some` exactly when its gene finished.
        let _ = maybe_timeout!(futures::future::join_all(gene_futures), self.view_params.timeout).await;
        let genes_bailed = gene_results.iter().any(Option::is_none);

        let empty = Arc::new(ViolinSummary::default());
        let summaries_by_var: Vec<Option<&Vec<Arc<ViolinSummary>>>> = var_names.iter()
            .map(|var_name| {
                let resolved_index = resolved_genes.iter().position(|(gene_name, _)| gene_name == var_name)?;
                gene_results[resolved_index].as_ref()
            })
            .collect();
        let cell_summaries = (0..categories.len())
            .flat_map(|category_index| summaries_by_var.iter().map(move |summaries| (category_index, summaries)))
            .map(|(category_index, summaries)| summaries.map_or_else(|| empty.clone(), |summaries| summaries[category_index].clone()))
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
        let metadata_future = futures::future::try_join(
            load_var_names(self.store.clone(), &self.store_name, p.gene_symbols.as_deref(), cache_enabled),
            load_obs_categorical(self.store.clone(), &self.store_name, &p.groupby, cache_enabled),
        );
        let (var_index_values, (obs_categories, obs_codes)) = match maybe_timeout!(metadata_future, self.view_params.timeout).await {
            Ok(Ok(loaded)) => loaded,
            Ok(Err(e)) => {
                if is_timed_out_zarrs_error(&e) {
                    return PrepareResult { bailed_early: true };
                }
                panic!("Zarrs error during AdataZarrViolinMatrixLayer prepare: {:?}", e);
            }
            Err(_) => return PrepareResult { bailed_early: true }, // Wall-clock timeout from maybe_timeout!
        };

        let categories = p.categories.clone().unwrap_or_else(|| obs_categories.as_ref().clone());
        let (summaries, genes_bailed) = self.load_cell_summaries(&categories, &obs_categories, &obs_codes, &var_index_values).await;

        let p = &self.layer_params;
        let summary_refs: Vec<&ViolinSummary> = summaries.iter().map(Arc::as_ref).collect();
        let bins = bin_violins(&summary_refs, p.num_bins.unwrap_or(DEFAULT_NUM_BINS) as usize, p.value_min);
        let medians = summaries.iter()
            .map(|summary| summary.quartiles.map(|(_, median, _)| median))
            .collect();

        let mut violin_matrix_layer = PrecomputedViolinMatrixLayer::new(
            self.view_params.clone(),
            PrecomputedViolinMatrixLayerParams {
                layer_id: format!("{}_precomputed_violin_matrix_sublayer", p.layer_id),
                bounds: p.bounds.clone(),
                swap_axes: p.swap_axes,
                var_names: Arc::new(p.var_names.clone()),
                groups: Arc::new(categories),
                bin_starts: Arc::new(bins.bin_starts),
                bin_counts: Arc::new(bins.bin_counts),
                density_norm: p.density_norm,
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
