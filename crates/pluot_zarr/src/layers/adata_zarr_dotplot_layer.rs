use std::sync::Arc;
use serde::{Deserialize, Serialize};
use pluot_core::{maybe_timeout, FutureExt, Duration, log};

use pluot_core::wgpu;
use pluot_core::cache::use_memo_vec_f32;
use pluot_core::zarr::is_timed_out_zarrs_error;
use zarrs::storage::AsyncReadableStorageTraits;
use pluot_core::two::svg::SvgContext;
use pluot_core::render_traits::{ColorMode, DrawToRasterCpu, DrawToRasterGpu, DrawToSvg, MarginParams, PickableLayer, PreparedAndDraw, PreparedLayer, QuantitativeColormap, UnitsMode, ViewParams, resolve_store_name};
use pluot_core::render_types::{CpuContext, CpuRenderPass, PrepareResult};
use pluot_core::render_types::GpuContext;
use pluot_core::composite_layer::{base_draw_composite_layer, base_draw_composite_layer_svg};
use pluot_core::compute::reduce::{reduce_extent, reduce_histogram_with_known_extent};
use pluot_core::composite_layers::bar_plot_layer::{BarOrientation, BarPlotLayer, BarPlotLayerParams};

/// Layer params struct for [`AdataZarrDotPlotLayer`].
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct AdataZarrDotPlotLayerParams {
    pub layer_id: String,
    pub bounds: Option<MarginParams>,


    // For now, we reuse the param names from sc.pl.dotplot.
    /// If True, transpose the dotplot. Defaults to False.
    /// By default, the x axis contains var_names (e.g. genes) and the y axis the groupby categories.
    /// By setting swap_axes then x are the groupby categories and y the var_names.
    pub swap_axes: bool,
    /// Color map (viridis, plasma, jet). Defaults to viridis.
    pub cmap: QuantitativeColormap,
    /// Title of the plot.
    pub title: Option<String>,
    /// Expression cutoff that is used for binarizing the gene expression
    /// and determining the fraction of cells expressing given genes.
    /// A gene is expressed only if the expression value is greater than this threshold.
    pub expression_cutoff: f32,

    // Data keys
    /// Must point to the root of an anndata object.
    pub store_name: Option<String>,

    /// Layer in AnnData to use for expression values. Defaults to None, which uses adata.X.
    pub layer: String,
    pub groupby: String,
    /// List of gene IDs from adata.var.index.
    pub var_names: Vec<String>,
    /// Key in adata.var to use for gene symbols, if different from adata.var.index.
    pub gene_symbols: Option<String>,

    // Whether to cache the full data array once loaded.
    // If false, will only cache the histogram result, throwing away the full array.
    // (E.g., will need to re-load the full array if num_bins changes).
    pub cache_data: bool,
}

impl Default for AdataZarrDotPlotLayerParams {
    fn default() -> Self {
        Self {
            layer_id: "".to_string(),
            bounds: None,
            swap_axes: false,
            cmap: QuantitativeColormap::Viridis,
            title: None,
            expression_cutoff: 0.0,
            store_name: None,
            layer: "X".to_string(),
            groupby: "bulk_labels".to_string(),
            var_names: vec![],
            gene_symbols: None,
            cache_data: true,
        }
    }
}

pub struct AdataZarrDotPlotLayer {
    view_params: ViewParams,
    layer_params: AdataZarrDotPlotLayerParams,
    store: Arc<dyn AsyncReadableStorageTraits>,
    store_name: String,

    sub_layer_instances: Vec<Box<dyn PreparedAndDraw>>,
}

impl AdataZarrDotPlotLayer {
    pub fn new(view_params: ViewParams, layer_params: AdataZarrDotPlotLayerParams) -> Self {
        let store_name = resolve_store_name(&layer_params.store_name, &view_params);

        let store = view_params.get_store(&store_name);
        Self {
            view_params,
            layer_params,
            store,
            store_name,
            sub_layer_instances: Vec::new(),
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for AdataZarrDotPlotLayer {
    async fn prepare(&mut self, gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        let store = self.store.clone();

        // Load the metadata for the anndata object (top-level), var dataframe, obs dataframe, specified obs column,
        // var index column, obs index column, and specified `layer` array.
        // Deserialize using the utilities in `adata_metadata.rs`.
        //
        // Then, load arrays:
        // Load var index
        // Load obs index
        // Load obs column corresponding to groupby (categories and codes arrays)
        // For now, assume the `layer` array is a dense array.
        // Determine which subset of the `layer` array to load (all cells/rows; the genes/cols specified by the var_names and their indices in var.index)
        // Load the subset of the `layer` array.
        // Once the array data has been loaded: for each gene and obs category pair, compute the dot color and size.

        // Load using nested use_memos - see `prepare` for ZarrHistogramLayer for an example

        // Then, construct a pointLayer.
        let point_layer = PointLayer::new(
            self.view_params.clone(),
            PointLayerParams {
                // TODO
            },
        );

        self.sub_layer_instances = vec![Box::new(point_layer)];

        for sub_layer in self.sub_layer_instances.iter_mut() {
            sub_layer.prepare(gpu_context).await;
        }

        PrepareResult { bailed_early: false }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for AdataZarrDotPlotLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        base_draw_composite_layer(&self.sub_layer_instances, gpu_context, pass).await;
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for AdataZarrDotPlotLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for AdataZarrDotPlotLayer {
    async fn draw(&self, ctx: &mut SvgContext) {
        base_draw_composite_layer_svg(&self.sub_layer_instances, ctx).await
    }
}

impl PickableLayer for AdataZarrDotPlotLayer {}
