use std::collections::HashMap;
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use pluot_core::{maybe_timeout, FutureExt, Duration};

use pluot_core::emphasis_mode::DEFAULT_BACKGROUND_COLOR;
use pluot_core::log;
use pluot_core::wgpu;
use pluot_core::zarr::is_timed_out_zarrs_error;
use zarrs::storage::AsyncReadableStorageTraits;
use pluot_core::two::svg::SvgContext;
use pluot_core::render_traits::{BooleanCriteriaParams, EmphasisCriteria, ExtentableLayer, ColorMode, DrawToRasterCpu, DrawToRasterGpu, DrawToSvg, InstancedSizeParams, MarginParams, PickableLayer, PreparedAndDraw, PreparedLayer, QuantitativeColormap, QuantitativeParams, SizeMode, UnitsMode, ViewParams, resolve_store_name};
use pluot_core::render_types::{CpuContext, CpuRenderPass, PrepareResult};
use pluot_core::gpu_send::GpuRenderPass;
use pluot_core::render_types::GpuContext;
use pluot_core::composite_layer::{base_draw_composite_layer, base_draw_composite_layer_svg};
use pluot_core::composite_layers::axis_band_layer::{AxisBandLayer, AxisBandLayerParams};
use pluot_core::composite_layers::axis_linear_layer::AxisPosition;
use pluot_core::composite_layers::legend_colormap_quantitative_layer::{LegendColormapQuantitativeLayer, LegendColormapQuantitativeLayerParams, LegendOrientation};
use pluot_core::composite_layers::legend_point_size_quantitative_layer::{LegendPointSizeQuantitativeLayer, LegendPointSizeQuantitativeLayerParams};
use pluot_core::d3::scale::{ScaleBand, ScaleLinear, Scaleable};
use pluot_core::layers::point_layer::{PointLayer, PointLayerParams, PointPickingInfo};
use pluot_core::numeric_data::{NumericData, NumericValue};
use pluot_core::LayerPickingResult;
use pluot_core::viewport::DataCoord;
use pluot_core::viewport::ScreenCoord;

use crate::dotplot_data::{
    background_and_foreground_rows, bucket_rows_by_category, expr_array_path, filter_genes, genes_among, load_gene_summaries_for_gene, load_obs_categorical,
    load_var_names, obs_rows_key, resolve_gene_columns, GeneSummary,
};
use crate::heatmap_data::{indices_of, load_criteria_indices, AxisCriteria, MatrixAxis};

/// Layer params struct for [`AdataZarrDotPlotLayer`].
#[derive(Serialize, Deserialize, Debug, Clone, TS)]
#[serde(default)]
#[ts(optional_fields = nullable)]
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

    // TODO: support multiple groupby (a list of columns from coarse to fine).
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

    /// Which obs are summarized. `None` includes every obs. When set, categories
    /// without any included obs are omitted.
    pub obs_filtering: Option<AxisCriteria>,
    /// Which of the included obs to emphasize: each dot then summarizes only the
    /// selected obs, in front of a grey dot summarizing every included obs.
    /// `None` emphasizes every obs.
    pub obs_selection: Option<AxisCriteria>,
    /// Which of `var_names` to include. `None` includes every one.
    pub var_filtering: Option<AxisCriteria>,
    /// Which of the included `var_names` to emphasize. The dots of the others
    /// are greyed out. `None` emphasizes every one.
    pub var_selection: Option<AxisCriteria>,

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
            obs_filtering: None,
            obs_selection: None,
            var_filtering: None,
            var_selection: None,
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
    /// The first `num_point_layers` of `sub_layer_instances` draw the dots, in back-to-front order.
    num_point_layers: usize,
    // One entry per dot in the same order as each point sublayer's instances (see `prepare`),
    // so a picked instance index looks up its gene/category/summary directly.
    gene_summaries: Vec<GeneSummary>,
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
            num_point_layers: 0,
            gene_summaries: Vec::new(),
        }
    }
}

/// The background summaries (over every filter-included obs) of one gene, and its
/// foreground summaries (over the selected ones) when there is an obs selection.
type GeneSummaries = (Vec<GeneSummary>, Option<Vec<GeneSummary>>);

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for AdataZarrDotPlotLayer {
    async fn prepare(&mut self, gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        let store = self.store.clone();
        let store_name = self.store_name.clone();
        let cache_enabled = self.view_params.cache_enabled;
        let timeout = self.view_params.timeout;

        let params = self.layer_params.clone();
        let var_column = params.gene_symbols.clone();
        // The `layer` param names `adata.X` as "X", which is not a `layers` entry.
        let array_layer = if params.layer == "X" { None } else { Some(params.layer.clone()) };
        let matrix_path = expr_array_path(array_layer.as_deref());
        let groupby = params.groupby.clone();
        let requested_categories = params.categories.clone();
        let expression_cutoff = params.expression_cutoff;

        // Stage 1: the obs groupby column, the var names/column used to resolve gene
        // columns, and the indices included by each filtering and selection. None depends on
        // another (`try_join`), and all are prerequisites for the axes and every gene's points below.
        let metadata_future = async {
            futures::try_join!(
                load_var_names(store.clone(), &store_name, var_column.as_deref(), cache_enabled),
                load_obs_categorical(store.clone(), &store_name, &groupby, cache_enabled),
                load_criteria_indices(gpu_context, store.clone(), &store_name, &matrix_path, MatrixAxis::Obs, params.obs_filtering.as_ref(), cache_enabled),
                load_criteria_indices(gpu_context, store.clone(), &store_name, &matrix_path, MatrixAxis::Obs, params.obs_selection.as_ref(), cache_enabled),
                load_criteria_indices(gpu_context, store.clone(), &store_name, &matrix_path, MatrixAxis::Var, params.var_filtering.as_ref(), cache_enabled),
                load_criteria_indices(gpu_context, store.clone(), &store_name, &matrix_path, MatrixAxis::Var, params.var_selection.as_ref(), cache_enabled),
            )
        };
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
        let (var_names, var_selected, group_labels, background_summaries, foreground_summaries, genes_bailed) = match &metadata {
            Some((var_index_values, (obs_categories, obs_codes), obs_filtered, obs_selected, var_filtered, var_selected)) => {
                // A `None` `categories` param means "every category of `groupby`", in its
                // stored order -- matching `sc.pl.dotplot`'s own default and the behavior this
                // layer had before `categories` became an explicit list. `Some(vec![])` means
                // no categories at all, and is passed through as-is.
                let mut requested_categories = requested_categories.unwrap_or_else(|| obs_categories.as_ref().clone());
                let rows_by_category = bucket_rows_by_category(&requested_categories, obs_categories, obs_codes);
                let (background_rows, foreground_rows) = background_and_foreground_rows(
                    rows_by_category,
                    obs_filtered.as_deref().map(indices_of),
                    obs_selected.as_deref().map(indices_of),
                    obs_codes.len(),
                );
                if obs_filtered.is_some() {
                    requested_categories.retain(|category| !background_rows[category].is_empty());
                }

                let var_names = filter_genes(&params.var_names, var_index_values, var_filtered.as_deref().map(indices_of));
                let var_selected = var_selected.as_deref().map(|selected| genes_among(&var_names, var_index_values, indices_of(selected)));
                let resolved_genes = resolve_gene_columns(&var_names, var_index_values);
                let var_colname = var_column.unwrap_or_else(|| "index".to_string());
                let background_rows_key = obs_rows_key(params.obs_filtering.as_ref(), None);
                let foreground_rows_key = obs_rows_key(params.obs_filtering.as_ref(), params.obs_selection.as_ref());

                let mut gene_results: Vec<Option<GeneSummaries>> = vec![None; resolved_genes.len()];
                let gene_futures = resolved_genes.iter().zip(gene_results.iter_mut()).map(|((gene_name, col_index), slot)| {
                    let summaries_over = |rows_by_category, rows_key| {
                        load_gene_summaries_for_gene(
                            store.clone(),
                            &store_name,
                            &var_colname,
                            gene_name,
                            *col_index,
                            array_layer.as_deref(),
                            &groupby,
                            &requested_categories,
                            rows_by_category,
                            rows_key,
                            expression_cutoff,
                            cache_enabled,
                        )
                    };
                    let (background_rows, foreground_rows) = (&background_rows, &foreground_rows);
                    let (background_rows_key, foreground_rows_key) = (&background_rows_key, &foreground_rows_key);
                    async move {
                        let Some(background) = summaries_over(background_rows, background_rows_key).await else {
                            return;
                        };
                        let foreground = match foreground_rows {
                            Some(foreground_rows) => match summaries_over(foreground_rows, foreground_rows_key).await {
                                Some(foreground) => Some(foreground),
                                None => return,
                            },
                            None => None,
                        };
                        *slot = Some((background, foreground));
                    }
                });

                // Discard the wall-clock result itself: whether it fires or not, `gene_results`
                // already holds `Some` for every gene that finished and `None` for every gene
                // that didn't, which is exactly what's needed below.
                let _ = maybe_timeout!(futures::future::join_all(gene_futures), timeout).await;
                let genes_bailed = gene_results.iter().any(Option::is_none);
                let (background, foreground): (Vec<_>, Vec<_>) = gene_results.into_iter().flatten().unzip();
                let background: Vec<GeneSummary> = background.into_iter().flatten().collect();
                let foreground: Option<Vec<GeneSummary>> = foreground_rows.as_ref().map(|_| foreground.into_iter().flatten().flatten().collect());
                (var_names, var_selected, requested_categories, background, foreground, genes_bailed)
            }
            None => {
                let var_names = if params.var_filtering.is_none() { params.var_names.clone() } else { Vec::new() };
                (var_names, None, Vec::new(), Vec::new(), None, true)
            }
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

        // A ScaleLinear mapping fraction-expressing values (always in [0, 1], by
        // definition) to pixel radii (0 to `max_dot_radius`). Reused below to size the
        // point-size legend, so both draw from the exact same domain/range.
        let mut size_scale = ScaleLinear::new();
        size_scale.set_domain((0.0, 1.0));
        size_scale.set_range((0.0, max_dot_radius as f64));

        // We define a second legend with an adjusted minimum value,
        // so that the smallest legend value is not zero.
        let min_size_domain_for_legend: f64 = 0.2;
        let min_size_range_for_legend = size_scale.scale(&min_size_domain_for_legend);

        let mut size_scale_for_legend = ScaleLinear::new();
        size_scale_for_legend.set_domain((min_size_domain_for_legend, 1.0));
        size_scale_for_legend.set_range((min_size_range_for_legend, max_dot_radius as f64));

        // With an obs selection, the colored dots summarize the selected obs, in front of
        // grey dots summarizing every filter-included obs.
        let (gene_summaries, greyed_summaries) = match foreground_summaries {
            Some(foreground_summaries) => (foreground_summaries, Some(background_summaries)),
            None => (background_summaries, None),
        };

        // One instance per dot: each summary names its own gene and category, so its
        // position is a scale lookup on its two axis labels. The fraction expressing
        // drives the radius and the mean expression the color. A category with no cells
        // (e.g. one that doesn't exist in `groupby`) summarizes to a zero-radius dot
        // rather than NaN.
        let dot_layer = |name: &str, summaries: &[GeneSummary], fill_color: ColorMode, selection_criteria: Vec<EmphasisCriteria>| {
            let mut position_x: Vec<f32> = Vec::with_capacity(summaries.len());
            let mut position_y: Vec<f32> = Vec::with_capacity(summaries.len());
            let mut point_radius_values: Vec<f32> = Vec::with_capacity(summaries.len());
            for summary in summaries {
                let group_pos = groups_scale.scale(&summary.obs_value) as f32 + groups_bandwidth / 2.0;
                let gene_pos = genes_scale.scale(&summary.var_name) as f32 + genes_bandwidth / 2.0;
                let (x, y) = if swap_axes { (group_pos, gene_pos) } else { (gene_pos, group_pos) };
                position_x.push(x);
                position_y.push(y);
                let (_, fraction_expressing) = summary.mean_and_fraction_expressing();
                point_radius_values.push(size_scale.scale(&(fraction_expressing as f64)) as f32);
            }
            PointLayer::new(
                self.view_params.clone(),
                PointLayerParams {
                    layer_id: format!("{}_{name}_sublayer", self.layer_params.layer_id),
                    bounds: self.layer_params.bounds.clone(),
                    data_unit_mode_x: UnitsMode::Pixels,
                    data_unit_mode_y: UnitsMode::Pixels,
                    point_radius_unit_mode_x: UnitsMode::Pixels,
                    point_radius_unit_mode_y: UnitsMode::Pixels,
                    point_radius: Some(SizeMode::InstancedSize(InstancedSizeParams {
                        values: NumericData::Float32(Arc::new(point_radius_values)),
                    })),
                    fill_color: Some(fill_color),
                    position_x: NumericData::Float32(Arc::new(position_x)),
                    position_y: NumericData::Float32(Arc::new(position_y)),
                    selection_criteria,
                    ..Default::default()
                },
            )
        };

        let mean_expression_values: Vec<f32> = gene_summaries.iter().map(|summary| summary.mean_and_fraction_expressing().0).collect();
        let (expression_min, expression_max) = mean_expression_values.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), &mean| (lo.min(mean), hi.max(mean)));
        // No dots (or an all-NaN expression matrix) leaves the accumulators at their
        // infinite seeds; fall back to the unit domain rather than emitting one the
        // shader cannot normalize against.
        let expression_domain = if expression_min.is_finite() && expression_max.is_finite() {
            (expression_min, expression_max)
        } else {
            (0.0, 1.0)
        };

        // The dots' fill color and `color_scale` -- and the legend below, which is built
        // from it -- are given the same explicit `expression_domain`, so the legend
        // reflects exactly the range the dots are normalized against in the shader.
        let fill_color_params = QuantitativeParams {
            values: NumericData::Float32(Arc::new(mean_expression_values)),
            colormap: self.layer_params.cmap,
            reverse: false,
            domain: Some(expression_domain),
        };
        let mut color_scale = ScaleLinear::new();
        color_scale.set_domain((expression_domain.0 as f64, expression_domain.1 as f64));

        let var_selection_criteria = match &var_selected {
            Some(var_selected) => {
                let is_var_selected: HashMap<&str, bool> = var_names.iter().map(String::as_str).zip(var_selected.iter().copied()).collect();
                let mask: Vec<u8> = gene_summaries.iter().map(|summary| is_var_selected[summary.var_name.as_str()] as u8).collect();
                vec![EmphasisCriteria::Boolean(BooleanCriteriaParams { mask: NumericData::from(mask) })]
            }
            None => vec![],
        };
        let mut point_layers: Vec<PointLayer> = Vec::new();
        if let Some(greyed_summaries) = &greyed_summaries {
            point_layers.push(dot_layer("background_point", greyed_summaries, ColorMode::UniformRgb(DEFAULT_BACKGROUND_COLOR), vec![]));
        }
        point_layers.push(dot_layer("point", &gene_summaries, ColorMode::Quantitative(fill_color_params), var_selection_criteria));
        for point_layer in &mut point_layers {
            point_layer.prepare(gpu_context).await;
        }

        // Kept for `pick`: a picked point's instance index is this same index into
        // `gene_summaries`, since every point layer was built in the order of `gene_summaries`.
        self.gene_summaries = gene_summaries;
        self.num_point_layers = point_layers.len();

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
                data_range: None,
                ..Default::default()
            },
        );
        x_axis_layer.prepare(gpu_context).await;

        let mut y_axis_layer = AxisBandLayer::new(
            self.view_params.clone(),
            AxisBandLayerParams {
                layer_id: format!("{}_y_axis_sublayer", self.layer_params.layer_id),
                position: AxisPosition::Left,
                domain: y_domain,
                data_range: None,
                ..Default::default()
            },
        );
        y_axis_layer.prepare(gpu_context).await;

        // TODO: remove this padding once AxisLinearLayerParams.fit_outer_ticks is implemented.
        const LEGEND_PADDING_HORIZONTAL: f32 = 5.0;
        // Approximate vertical space (in pixels) consumed by the color legend above --
        // title + gradient bar + axis line + tick labels -- used to stack the point-size
        // legend beneath it within the same right-margin column.
        const COLOR_LEGEND_RESERVED_HEIGHT_PX: f32 = 80.0;

        // Legend for the dot color (mean expression -> `cmap`), rendered within the plot's
        // right margin, i.e. the band from `viewport_w - margin_right` to `viewport_w`.
        let mut legend_layer = LegendColormapQuantitativeLayer::new(
            self.view_params.clone(),
            LegendColormapQuantitativeLayerParams {
                layer_id: format!("{}_legend_sublayer", self.layer_params.layer_id),
                bounds: Some(MarginParams {
                    margin_left: Some(self.view_params.width as f32 - margin_right + LEGEND_PADDING_HORIZONTAL),
                    margin_right: Some(LEGEND_PADDING_HORIZONTAL),
                    margin_top: Some(margin_top),
                    margin_bottom: Some(0.0),
                }),
                title: "Mean expression".to_string(),
                colormap: self.layer_params.cmap,
                reverse: false,
                scale: Some(color_scale),
                orientation: LegendOrientation::Horizontal,
            },
        );
        legend_layer.prepare(gpu_context).await;

        // Legend for the dot size (fraction expressing -> radius), stacked beneath the
        // color legend in the same right-margin column. Uses the exact same `size_scale`
        // that mapped the dots' own fraction-expressing values to pixel radii above.
        let mut size_legend_layer = LegendPointSizeQuantitativeLayer::new(
            self.view_params.clone(),
            LegendPointSizeQuantitativeLayerParams {
                layer_id: format!("{}_size_legend_sublayer", self.layer_params.layer_id),
                bounds: Some(MarginParams {
                    margin_left: Some(self.view_params.width as f32 - margin_right + LEGEND_PADDING_HORIZONTAL),
                    margin_right: Some(LEGEND_PADDING_HORIZONTAL),
                    margin_top: Some(margin_top + COLOR_LEGEND_RESERVED_HEIGHT_PX),
                    margin_bottom: Some(0.0),
                }),
                title: "Fraction expressing".to_string(),
                scale: size_scale_for_legend,
                orientation: LegendOrientation::Vertical,
                ..Default::default()
            },
        );
        size_legend_layer.prepare(gpu_context).await;

        let mut sub_layer_instances: Vec<Box<dyn PreparedAndDraw>> = point_layers.into_iter().map(|layer| Box::new(layer) as Box<dyn PreparedAndDraw>).collect();
        sub_layer_instances.extend([
            Box::new(x_axis_layer) as Box<dyn PreparedAndDraw>,
            Box::new(y_axis_layer),
            Box::new(legend_layer),
            Box::new(size_legend_layer),
        ]);
        self.sub_layer_instances = sub_layer_instances;

        PrepareResult { bailed_early }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for AdataZarrDotPlotLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut GpuRenderPass<'_>) {
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


impl ExtentableLayer for AdataZarrDotPlotLayer {}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
pub struct AdataZarrDotPlotPickingInfo {
    pub var_name: String,
    pub obs_value: String,
    pub mean_expression: NumericValue,
    pub fraction_expressing: NumericValue,
}

impl PickableLayer for AdataZarrDotPlotLayer {
    fn pick(&self, screen_coord: ScreenCoord, data_coord: Option<DataCoord>) -> Option<LayerPickingResult> {
        let point_result = self.sub_layer_instances[..self.num_point_layers]
            .iter()
            .rev()
            .find_map(|point_layer| point_layer.pick(screen_coord, data_coord))?;
        let layer_id = point_result.layer_id.clone();
        // The picked point's index is into `gene_summaries`,
        // since every point layer was built in its order in `prepare`.
        let point: PointPickingInfo = point_result.into_info()?;
        let summary = self.gene_summaries.get(point.index)?;
        let (mean, fraction_expressing) = summary.mean_and_fraction_expressing();

        Some(LayerPickingResult::new(layer_id, AdataZarrDotPlotPickingInfo {
            var_name: summary.var_name.clone(),
            obs_value: summary.obs_value.clone(),
            mean_expression: NumericValue::Float32(mean),
            fraction_expressing: NumericValue::Float32(fraction_expressing),
        }))
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
