use std::sync::Arc;
use serde::{Deserialize, Serialize};
use pluot_core::{maybe_timeout, FutureExt, Duration};

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

use crate::adata_dotplot_data::use_dotplot_data;

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
        let expr_array_path = if self.layer_params.layer == "X" {
            "/X".to_string()
        } else {
            format!("/layers/{}", self.layer_params.layer)
        };

        let data_future = use_dotplot_data(
            store,
            &self.store_name,
            &self.layer_params.groupby,
            &expr_array_path,
            &self.layer_params.var_names,
            self.layer_params.expression_cutoff,
            self.view_params.cache_enabled,
        );

        let data = match maybe_timeout!(data_future, self.view_params.timeout).await {
            Ok(Ok(data)) => data,
            Ok(Err(e)) => {
                if is_timed_out_zarrs_error(&e) {
                    return PrepareResult { bailed_early: true };
                } else {
                    panic!("Zarrs error during AdataZarrDotPlotLayer prepare: {:?}", e);
                }
            }
            Err(_) => {
                // Wall-clock timeout from maybe_timeout!
                return PrepareResult { bailed_early: true };
            }
        };

        if data.var_names.is_empty() || data.categories.is_empty() {
            self.sub_layer_instances = vec![];
            return PrepareResult { bailed_early: false };
        }

        let n_groups = data.categories.len();
        let n_genes = data.var_names.len();

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

        let mut genes_scale = ScaleBand::new();
        genes_scale.set_domain(data.var_names.clone());
        let mut groups_scale = ScaleBand::new();
        groups_scale.set_domain(data.categories.as_ref().clone());
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

        let mut position_x: Vec<f32> = Vec::with_capacity(n_groups * n_genes);
        let mut position_y: Vec<f32> = Vec::with_capacity(n_groups * n_genes);
        let mut point_radius_values: Vec<f32> = Vec::with_capacity(n_groups * n_genes);
        for g in 0..n_groups {
            let group_pos = groups_scale.scale(&data.categories[g]) as f32 + groups_bandwidth / 2.0;
            for j in 0..n_genes {
                let gene_pos = genes_scale.scale(&data.var_names[j]) as f32 + genes_bandwidth / 2.0;
                let (x, y) = if swap_axes { (group_pos, gene_pos) } else { (gene_pos, group_pos) };
                position_x.push(x);
                position_y.push(y);
                point_radius_values.push(data.fraction_expressing[g * n_genes + j] * max_dot_radius);
            }
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
                    values: NumericData::Float32(Arc::new(data.mean_expression.clone())),
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
            (data.categories.clone(), Arc::new(data.var_names.clone()))
        } else {
            (Arc::new(data.var_names.clone()), data.categories.clone())
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

impl PickableLayer for AdataZarrDotPlotLayer {
    fn pick(&self, screen_coord: ScreenCoord, data_coord: Option<DataCoord>) -> Option<LayerPickingResult> {
        // The PointLayer rendering the dots is always sub_layer_instances[0]
        // (see `prepare`); the axis sublayers are not pickable.
        self.sub_layer_instances.first()?.pick(screen_coord, data_coord)

        // TODO: convert the point layer picking result into something more semantically meaningful for the dotplot
        // (the var name, category name, the mean expression value, the fraction expressing value for the given point).
    }
}
