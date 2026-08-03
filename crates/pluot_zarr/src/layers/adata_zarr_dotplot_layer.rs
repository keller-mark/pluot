use std::sync::Arc;
use serde::{Deserialize, Serialize};
use pluot_core::{maybe_timeout, FutureExt, Duration};

use pluot_core::log;
use pluot_core::wgpu;
use pluot_core::zarr::is_timed_out_zarrs_error;
use zarrs::storage::AsyncReadableStorageTraits;
use pluot_core::two::svg::SvgContext;
use pluot_core::render_traits::{ColorMode, DrawToRasterCpu, DrawToRasterGpu, DrawToSvg, InstancedSizeParams, MarginParams, PickableLayer, PreparedAndDraw, PreparedLayer, QuantitativeColormap, QuantitativeParams, SizeMode, UnitsMode, ViewParams, resolve_store_name};
use pluot_core::render_types::{CpuContext, CpuRenderPass, PrepareResult};
use pluot_core::render_types::GpuContext;
use pluot_core::composite_layer::{base_draw_composite_layer, base_draw_composite_layer_svg};
use pluot_core::composite_layers::axis_band_layer::{AxisBandLayer, AxisBandLayerParams};
use pluot_core::composite_layers::axis_linear_layer::AxisPosition;
use pluot_core::d3::scale::{ScaleBand, Scaleable};
use pluot_core::layers::point_layer::{PointLayer, PointLayerParams};
use pluot_core::numeric_data::NumericData;
use pluot_core::LayerPickingResult;
use pluot_core::viewport::DataCoord;
use pluot_core::viewport::ScreenCoord;

use crate::dotplot_data::{bucket_rows_by_category, load_gene_summaries_for_gene, load_obs_categorical, load_var_names, resolve_gene_columns, GeneSummary};

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
    /// The `groupby` categories to plot, in the order they should appear along the group axis.
    /// `None` (the default) means every category of `groupby`, in its stored order. `Some(vec![])`
    /// means no categories at all. Each requested category is looked up independently, so an
    /// unknown one yields an empty (zero-cell) column rather than an error.
    pub categories: Option<Vec<String>>,
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
            categories: None,
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
        let store_name = self.store_name.clone();
        let cache_enabled = self.view_params.cache_enabled;
        let timeout = self.view_params.timeout;

        let var_names = self.layer_params.var_names.clone();
        let var_column = self.layer_params.gene_symbols.clone();
        // The `layer` param names `adata.X` as "X", which is not a `layers` entry.
        let array_layer = if self.layer_params.layer == "X" { None } else { Some(self.layer_params.layer.clone()) };
        let groupby = self.layer_params.groupby.clone();
        let requested_categories = self.layer_params.categories.clone();
        let expression_cutoff = self.layer_params.expression_cutoff;

        // Stage 1: the obs groupby column and the var names/column used to resolve gene
        // columns. Neither depends on the other (`try_join`), and both are prerequisites for
        // the group axis and every gene's points below -- but NOT for the gene axis, which is
        // rendered further down directly from `var_names` (the requested gene list from the
        // layer params) with no I/O at all.
        let metadata_future = futures::future::try_join(
            load_var_names(store.clone(), &store_name, var_column.as_deref(), cache_enabled),
            load_obs_categorical(store.clone(), &store_name, &groupby, cache_enabled),
        );
        let metadata = match maybe_timeout!(metadata_future, timeout).await {
            Ok(Ok(metadata)) => Some(metadata),
            Ok(Err(e)) => {
                if is_timed_out_zarrs_error(&e) {
                    None
                } else {
                    panic!("Zarrs error during AdataZarrDotPlotLayer prepare: {:?}", e);
                }
            }
            Err(_) => None, // Wall-clock timeout from maybe_timeout!
        };

        // Stage 2: once the metadata above is ready, resolve genes to column indices and
        // bucket obs rows by category (both pure/synchronous), then load each resolved gene's
        // own array column independently and concurrently. Each gene's slot is mutated in
        // place as its own future resolves to `Some(..)`; a slot left at `None` means that
        // gene's data isn't loaded yet (either it hit zarrs's "not cached yet" condition, or
        // the wall-clock timeout below cut the rest short before its turn) -- either way, it's
        // simply omitted from this round's dots rather than blocking the genes that did load.
        let (group_labels, gene_summaries, genes_bailed) = match &metadata {
            Some((var_index_values, (obs_categories, obs_codes))) => {
                // A `None` `categories` param means "every category of `groupby`", in its
                // stored order -- matching `sc.pl.dotplot`'s own default and the behavior this
                // layer had before `categories` became an explicit list. `Some(vec![])` means
                // no categories at all, and is passed through as-is.
                let requested_categories = requested_categories.unwrap_or_else(|| obs_categories.as_ref().clone());

                let resolved_genes = resolve_gene_columns(&var_names, var_index_values);
                let rows_by_category = bucket_rows_by_category(&requested_categories, obs_categories, obs_codes);
                let var_colname = var_column.unwrap_or_else(|| "index".to_string());

                let mut gene_results: Vec<Option<Vec<GeneSummary>>> = vec![None; resolved_genes.len()];
                let gene_futures = resolved_genes.iter().zip(gene_results.iter_mut()).map(|((gene_name, col_index), slot)| {
                    let store = store.clone();
                    let store_name = &store_name;
                    let var_colname = &var_colname;
                    let array_layer = array_layer.as_deref();
                    let groupby = &groupby;
                    let requested_categories = &requested_categories;
                    let rows_by_category = &rows_by_category;
                    async move {
                        *slot = load_gene_summaries_for_gene(
                            store,
                            store_name,
                            var_colname,
                            gene_name,
                            *col_index,
                            array_layer,
                            groupby,
                            requested_categories,
                            rows_by_category,
                            expression_cutoff,
                            cache_enabled,
                        )
                        .await;
                    }
                });

                // Discard the wall-clock result itself: whether it fires or not, `gene_results`
                // already holds `Some` for every gene that finished and `None` for every gene
                // that didn't, which is exactly what's needed below.
                let _ = maybe_timeout!(futures::future::join_all(gene_futures), timeout).await;
                let genes_bailed = gene_results.iter().any(Option::is_none);
                let all_summaries: Vec<GeneSummary> = gene_results.into_iter().flatten().flatten().collect();
                (requested_categories, all_summaries, genes_bailed)
            }
            None => (Vec::new(), Vec::new(), true),
        };
        // Per the dot plot's contract, bailed_early stays true until every requested gene's
        // data and summaries have loaded -- not merely until this call's wall clock runs out.
        let bailed_early = metadata.is_none() || genes_bailed;

        // --- Everything from here is pure/synchronous layout, using whatever loaded above:
        // `var_names` (the gene axis) is always the full requested list; `group_labels` and
        // `gene_summaries` (the group axis and the dots) reflect only what stages 1/2 above
        // managed to load within their timeouts. ---

        // Lay genes and groupby categories out on a pixel-space grid, one
        // band scale per axis (swapped when `swap_axes` is set).
        let bounds = if self.layer_params.bounds.is_none() { &self.view_params.margins } else { &self.layer_params.bounds };
        let (margin_top, margin_right, margin_bottom, margin_left) = match bounds {
            Some(m) => (
                m.margin_top.unwrap_or(0.0),
                m.margin_right.unwrap_or(0.0),
                m.margin_bottom.unwrap_or(0.0),
                m.margin_left.unwrap_or(0.0),
            ),
            None => (0.0, 0.0, 0.0, 0.0),
        };
        let layer_w = (self.view_params.width as f32 - (margin_left + margin_right)).max(1.0);
        let layer_h = (self.view_params.height as f32 - (margin_top + margin_bottom)).max(1.0);

        let swap_axes = self.layer_params.swap_axes;

        // One tick per gene, and one per requested groupby category.
        let mut genes_scale = ScaleBand::new();
        genes_scale.set_domain(var_names.clone());
        let mut groups_scale = ScaleBand::new();
        groups_scale.set_domain(group_labels.clone());
        if swap_axes {
            genes_scale.set_range((0.0, layer_h as f64));
            groups_scale.set_range((0.0, layer_w as f64));
        } else {
            genes_scale.set_range((0.0, layer_w as f64));
            groups_scale.set_range((0.0, layer_h as f64));
        }
        let genes_bandwidth = genes_scale.bandwidth() as f32;
        let groups_bandwidth = groups_scale.bandwidth() as f32;

        // The largest dot (fraction expressing == 1.0) is sized to comfortably
        // fit within its grid cell on the tighter of the two axes.
        let max_dot_radius = (genes_bandwidth.min(groups_bandwidth) / 2.0 * 0.9).max(1.0);

        // One instance per dot: each summary names its own gene and category, so its
        // position is a scale lookup on its two axis labels. The fraction expressing
        // drives the radius and the mean expression the color. A category with no cells
        // (e.g. one that doesn't exist in `groupby`) summarizes to a zero-radius dot
        // rather than NaN.
        let n_dots = gene_summaries.len();

        log(&format!("n_dots: {}", n_dots));

        let mut position_x: Vec<f32> = Vec::with_capacity(n_dots);
        let mut position_y: Vec<f32> = Vec::with_capacity(n_dots);
        let mut point_radius_values: Vec<f32> = Vec::with_capacity(n_dots);
        let mut mean_expression_values: Vec<f32> = Vec::with_capacity(n_dots);
        for summary in &gene_summaries {
            let group_pos = groups_scale.scale(&summary.obs_value) as f32 + groups_bandwidth / 2.0;
            let gene_pos = genes_scale.scale(&summary.var_name) as f32 + genes_bandwidth / 2.0;
            let (x, y) = if swap_axes { (group_pos, gene_pos) } else { (gene_pos, group_pos) };
            position_x.push(x);
            position_y.push(y);
            let (mean, fraction_expressing) = if summary.cell_count > 0 {
                (summary.sum / summary.cell_count as f32, summary.num_expressing as f32 / summary.cell_count as f32)
            } else {
                (0.0, 0.0)
            };
            point_radius_values.push(fraction_expressing * max_dot_radius);
            mean_expression_values.push(mean);

            log(&format!("var_name: {}, obs_value: {}, mean: {}, fraction: {}", summary.var_name, summary.obs_value, mean, fraction_expressing));
        }

        let mut point_layer = PointLayer::new(
            self.view_params.clone(),
            PointLayerParams {
                layer_id: format!("{}_point_sublayer", self.layer_params.layer_id),
                bounds: self.layer_params.bounds.clone(),
                data_unit_mode_x: UnitsMode::Pixels,
                data_unit_mode_y: UnitsMode::Pixels,
                point_radius_unit_mode_x: UnitsMode::Pixels,
                point_radius_unit_mode_y: UnitsMode::Pixels,
                point_radius: Some(SizeMode::InstancedSize(InstancedSizeParams {
                    values: NumericData::Float32(Arc::new(point_radius_values)),
                })),
                fill_color: Some(ColorMode::Quantitative(QuantitativeParams {
                    values: NumericData::Float32(Arc::new(mean_expression_values)),
                    colormap: self.layer_params.cmap.clone(),
                    reverse: false,
                    domain: None,
                })),
                position_x: NumericData::Float32(Arc::new(position_x)),
                position_y: NumericData::Float32(Arc::new(position_y)),
                ..Default::default()
            },
        );
        point_layer.prepare(gpu_context).await;

        // Band-scale axes for the two categorical dimensions (genes and
        // groupby categories), swapped along with the dot grid itself.
        let (x_domain, y_domain) = if swap_axes {
            (Arc::new(group_labels), Arc::new(var_names))
        } else {
            (Arc::new(var_names), Arc::new(group_labels))
        };

        let mut x_axis_layer = AxisBandLayer::new(
            self.view_params.clone(),
            AxisBandLayerParams {
                layer_id: format!("{}_x_axis_sublayer", self.layer_params.layer_id),
                position: AxisPosition::Bottom,
                domain: x_domain,
            },
        );
        x_axis_layer.prepare(gpu_context).await;

        let mut y_axis_layer = AxisBandLayer::new(
            self.view_params.clone(),
            AxisBandLayerParams {
                layer_id: format!("{}_y_axis_sublayer", self.layer_params.layer_id),
                position: AxisPosition::Left,
                domain: y_domain,
            },
        );
        y_axis_layer.prepare(gpu_context).await;

        self.sub_layer_instances = vec![Box::new(point_layer), Box::new(x_axis_layer), Box::new(y_axis_layer)];

        PrepareResult { bailed_early }
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

impl PickableLayer for AdataZarrDotPlotLayer {
    fn pick(&self, screen_coord: ScreenCoord, data_coord: Option<DataCoord>) -> Option<LayerPickingResult> {
        // The PointLayer rendering the dots is always sub_layer_instances[0]
        // (see `prepare`); the axis sublayers are not pickable.
        self.sub_layer_instances.first()?.pick(screen_coord, data_coord)

        // TODO: convert the point layer picking result into something more semantically meaningful for the dotplot
        // (the var name, category name, the mean expression value, the fraction expressing value for the given point).
        // The picked instance index is the index of the corresponding `GeneSummary`, which names its own gene and
        // category, so this only needs `prepare` to hold on to the loaded `Vec<GeneSummary>`.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `prepare()`'s per-gene stage mutates disjoint `iter_mut()` slots in place and wraps the
    /// whole batch in one `maybe_timeout!`. This reproduces that exact pattern with a fast
    /// future alongside one that never resolves (standing in for a gene whose data isn't
    /// loaded yet), to confirm directly -- rather than just by inspection -- that the fast
    /// one's result is still visible after the wrapping timeout fires and drops the rest, so
    /// partial gene data is never discarded along with whatever didn't finish in time.
    #[tokio::test]
    async fn partial_completion_survives_the_wrapping_timeout() {
        let mut slots: Vec<Option<u32>> = vec![None, None];
        let futures = slots.iter_mut().enumerate().map(|(i, slot)| async move {
            if i == 0 {
                *slot = Some(42);
            } else {
                futures::future::pending::<()>().await;
            }
        });

        let timeout_ms: Option<u32> = Some(20);
        let result = maybe_timeout!(futures::future::join_all(futures), timeout_ms).await;

        assert!(result.is_err(), "the never-resolving slot should have made the whole batch time out");
        assert_eq!(slots[0], Some(42), "the fast slot's result must survive even though the batch as a whole timed out");
        assert_eq!(slots[1], None, "the never-resolving slot is left exactly as it started");
    }
}
