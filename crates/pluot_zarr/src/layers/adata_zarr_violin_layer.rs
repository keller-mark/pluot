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
use pluot_core::numeric_data::NumericData;
use pluot_core::LayerExtentResult;
use pluot_core::LayerPickingResult;
use pluot_core::viewport::DataCoord;
use pluot_core::viewport::ScreenCoord;

use crate::dotplot_data::{bucket_rows_by_category, load_gene_expression, load_obs_categorical, load_var_names, resolve_gene_columns};
use crate::violin_data::{bin_violins, load_violin_summary, ViolinSummary};

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
    pub groupby: String,
    /// The `groupby` categories to plot, in order along the X axis. `None` means every category, in its stored order.
    pub categories: Option<Vec<String>>,
    /// Gene ID from adata.var.index (or from the `gene_symbols` column, when set).
    pub var_name: String,
    /// Key in adata.var to use for gene symbols, if different from adata.var.index.
    pub gene_symbols: Option<String>,

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
    summaries: Vec<Arc<ViolinSummary>>,
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

    /// Summarizes the gene's expression per category, or returns `None` when its data is not loaded yet.
    /// A gene that is not found yields empty summaries.
    async fn load_summaries(&self, categories: &[String], obs_categories: &[String], obs_codes: &NumericData, var_index_values: &[String]) -> Option<Vec<Arc<ViolinSummary>>> {
        let p = &self.layer_params;
        let cache_enabled = self.view_params.cache_enabled;
        let Some((gene_name, col_index)) = resolve_gene_columns(std::slice::from_ref(&p.var_name), var_index_values).pop() else {
            return Some(vec![Arc::new(ViolinSummary::default()); categories.len()]);
        };
        let var_colname = p.gene_symbols.as_deref().unwrap_or("index");

        let expression_future = load_gene_expression(self.store.clone(), &self.store_name, var_colname, &gene_name, col_index, self.array_layer(), cache_enabled);
        let expression = match maybe_timeout!(expression_future, self.view_params.timeout).await {
            Ok(Ok(expression)) => expression,
            Ok(Err(e)) => {
                if is_timed_out_zarrs_error(&e) {
                    return None;
                }
                panic!("Zarrs error loading expression during AdataZarrViolinLayer prepare: {:?}", e);
            }
            Err(_) => return None, // Wall-clock timeout from maybe_timeout!
        };

        let keep_zeros = p.keep_zeros.unwrap_or(true);
        let rows_by_category = bucket_rows_by_category(categories, obs_categories, obs_codes);
        let summaries = categories.iter()
            .map(|category| {
                let keys = vec![
                    "violin_summary".to_string(),
                    self.store_name.clone(),
                    self.array_layer().unwrap_or("X").to_string(),
                    var_colname.to_string(),
                    gene_name.clone(),
                    p.groupby.clone(),
                    category.clone(),
                    keep_zeros.to_string(),
                ];
                load_violin_summary(&expression, &rows_by_category[category], keep_zeros, &keys, cache_enabled)
            })
            .collect();
        Some(summaries)
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for AdataZarrViolinLayer {
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
                panic!("Zarrs error during AdataZarrViolinLayer prepare: {:?}", e);
            }
            Err(_) => return PrepareResult { bailed_early: true }, // Wall-clock timeout from maybe_timeout!
        };

        let categories = p.categories.clone().unwrap_or_else(|| obs_categories.as_ref().clone());
        let Some(summaries) = self.load_summaries(&categories, &obs_categories, &obs_codes, &var_index_values).await else {
            return PrepareResult { bailed_early: true };
        };

        let summary_refs: Vec<&ViolinSummary> = summaries.iter().map(Arc::as_ref).collect();
        let bins = bin_violins(&summary_refs, p.num_bins.unwrap_or(DEFAULT_NUM_BINS) as usize, p.value_min);
        let quartiles = summaries.iter().map(|summary| summary.quartiles).collect();

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
                bin_starts: Arc::new(bins.bin_starts),
                bin_counts: Arc::new(bins.bin_counts),
                density_norm: p.density_norm,
                quartiles: Some(Arc::new(quartiles)),
                value_domain: Some(bins.value_domain),
                fill_colors: Some(Arc::new(fill_colors)),
                group_axis_title: Some(p.groupby.clone()),
                value_axis_title: Some(p.var_name.clone()),
                ..Default::default()
            },
        );
        let prepare_result = violin_layer.prepare(gpu_context).await;
        self.inner = Some(violin_layer);
        self.summaries = summaries;

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
