// Lays out the filter-included obs along a generalized Hilbert ("gilbert")
// curve filling the layer bounds, sorted by one or more categorical obs
// columns so that each group forms a contiguous blob, and colors them by either
// one var's expression or one categorical obs column.
//
// References:
// - https://github.com/keller-mark/scplx/blob/main/src/scplx/gilbert.py
// - https://github.com/keller-mark/scplx/blob/main/notebooks/hilbert_heatmap.ipynb
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use zarrs::array::ArrayError;
use zarrs::storage::AsyncReadableStorageTraits;

use pluot_core::composite_layer::{base_draw_composite_layer, base_draw_composite_layer_svg};
use pluot_core::composite_layers::legend_colormap_quantitative_layer::{
    LegendColormapQuantitativeLayer, LegendColormapQuantitativeLayerParams, LegendOrientation,
};
use pluot_core::d3::scale::ScaleLinear;
use pluot_core::gpu_send::GpuRenderPass;
use pluot_core::layers::heatmap_layer::{HeatmapColormap, HeatmapQuantitativeColormapParams};
use pluot_core::layers::hilbert_heatmap_layer::{HilbertHeatmapLayer, HilbertHeatmapLayerParams, HilbertHeatmapPickingInfo};
use pluot_core::log;
use pluot_core::numeric_data::NumericData;
use pluot_core::render_traits::{
    resolve_store_name, CategoricalColormap, DrawToRasterCpu, DrawToRasterGpu, DrawToSvg, ExtentableLayer, MarginParams, PickableLayer,
    PreparedAndDraw, PreparedLayer, QuantitativeColormap, ViewParams,
};
use pluot_core::render_types::{CpuContext, CpuRenderPass, GpuContext, PrepareResult};
use pluot_core::two::svg::SvgContext;
use pluot_core::viewport::{DataCoord, ScreenCoord};
use pluot_core::wgpu;
use pluot_core::zarr::is_timed_out_zarrs_error;
use pluot_core::{maybe_timeout, Duration, FutureExt, LayerPickingResult};

use crate::dotplot_data::{expr_array_path, load_obs_categorical};
use crate::heatmap_data::{load_axis_emphasis_criteria, load_axis_labels, load_var_expression, AxisCriteria, MatrixAxis};
use crate::hilbert_heatmap_data::{load_nested_grouped_obs_indices, load_var_expression_extent};

/// Layer params struct for [`AdataZarrHilbertHeatmapLayer`].
#[derive(Serialize, Deserialize, Debug, Clone, TS)]
#[serde(default)]
#[ts(optional_fields = nullable)]
pub struct AdataZarrHilbertHeatmapLayerParams {
    pub layer_id: String,
    pub bounds: Option<MarginParams>,
    /// Must point to the root of an anndata object.
    pub store_name: Option<String>,

    /// Categorical `obs` columns by which to order the obs along the curve, from
    /// coarsest to finest (e.g., `["cell_type", "sample"]`). Empty keeps the filtered order.
    pub groupby: Vec<String>,

    /// When set, colors each obs by its expression of this var, using `quantitative_colormap`.
    /// Otherwise, colors each obs by its category in `color_by`, using `categorical_colormap`.
    pub var_name: Option<String>,
    /// The `var` column holding `var_name`. `None` means its index.
    pub var_column: Option<String>,
    /// The `layers` entry holding the expression values. `None` means `X`.
    pub layer: Option<String>,
    /// Categorical `obs` column to color by when there is no `var_name`. Defaults to the first `groupby` column.
    pub color_by: Option<String>,

    /// Defaults to viridis.
    pub quantitative_colormap: Option<QuantitativeColormap>,
    /// The (min, max) expression mapped to the ends of `quantitative_colormap`.
    /// Defaults to the extent over the filter-included obs.
    pub domain: Option<(f32, f32)>,
    /// Defaults to Tableau10.
    pub categorical_colormap: Option<CategoricalColormap>,

    /// Which obs are laid out along the curve. `None` includes every obs.
    pub obs_filtering: Option<AxisCriteria>,
    /// Which of the included obs to emphasize. `None` emphasizes every obs.
    pub obs_selection: Option<AxisCriteria>,
    /// Opacity of cells whose obs is not selected.
    pub background_opacity: Option<f32>,

    /// Side length, in pixels, of each cell of the curve's grid. Defaults to 1.
    pub cell_size: Option<f32>,
    /// Title of the quantitative colormap legend. Defaults to `var_name`.
    pub legend_title: Option<String>,
}

impl Default for AdataZarrHilbertHeatmapLayerParams {
    fn default() -> Self {
        Self {
            layer_id: "".to_string(),
            bounds: None,
            store_name: None,
            groupby: vec![],
            var_name: None,
            var_column: None,
            layer: None,
            color_by: None,
            quantitative_colormap: None,
            domain: None,
            categorical_colormap: None,
            obs_filtering: None,
            obs_selection: None,
            background_opacity: None,
            cell_size: None,
            legend_title: None,
        }
    }
}

enum ObsColors {
    Expression { values: Arc<Vec<f32>>, domain: (f32, f32) },
    Categorical { column: String, categories: Arc<Vec<String>>, codes: Arc<NumericData> },
}

/// What `pick` needs to describe an obs, kept from `prepare`.
struct PreparedObs {
    obs_labels: Arc<Vec<String>>,
    groupby_columns: Vec<(Arc<Vec<String>>, Arc<NumericData>)>,
    colors: ObsColors,
}

pub struct AdataZarrHilbertHeatmapLayer {
    view_params: ViewParams,
    layer_params: AdataZarrHilbertHeatmapLayerParams,
    store: Arc<dyn AsyncReadableStorageTraits>,
    store_name: String,

    /// The hilbert heatmap sublayer first, then the legend, if any.
    sub_layer_instances: Vec<Box<dyn PreparedAndDraw>>,
    obs: Option<PreparedObs>,
}

impl AdataZarrHilbertHeatmapLayer {
    pub fn new(view_params: ViewParams, layer_params: AdataZarrHilbertHeatmapLayerParams) -> Self {
        let store_name = resolve_store_name(&layer_params.store_name, &view_params);
        let store = view_params.get_store(&store_name);
        Self {
            view_params,
            layer_params,
            store,
            store_name,
            sub_layer_instances: Vec::new(),
            obs: None,
        }
    }

    fn color_by(&self) -> Option<&str> {
        self.layer_params.color_by.as_deref().or(self.layer_params.groupby.first().map(String::as_str))
    }

    /// `None` when there is nothing to color the obs by.
    async fn load_colors(&self, gpu_context: Option<&GpuContext<'_>>) -> Result<Option<ObsColors>, ArrayError> {
        let p = &self.layer_params;
        let (store, store_name, cache_enabled) = (self.store.clone(), self.store_name.as_str(), self.view_params.cache_enabled);
        if let Some(var_name) = &p.var_name {
            let values_future = load_var_expression(store.clone(), store_name, var_name, p.var_column.as_deref(), p.layer.as_deref(), cache_enabled);
            let domain_future = async {
                match p.domain {
                    Some(domain) => Ok(Some(domain)),
                    None => {
                        load_var_expression_extent(
                            gpu_context, store.clone(), store_name, var_name, p.var_column.as_deref(), p.layer.as_deref(), p.obs_filtering.as_ref(), cache_enabled,
                        )
                        .await
                    }
                }
            };
            let (values, domain) = futures::try_join!(values_future, domain_future)?;
            let (Some(values), Some((lo, hi))) = (values, domain) else {
                log(&format!("AdataZarrHilbertHeatmapLayer: var \"{var_name}\" not found; drawing nothing"));
                return Ok(None);
            };
            let domain = if lo.is_finite() && hi.is_finite() { (lo, hi) } else { (0.0, 1.0) };
            return Ok(Some(ObsColors::Expression { values, domain }));
        }
        let Some(column) = self.color_by() else {
            log("AdataZarrHilbertHeatmapLayer: neither var_name, color_by nor groupby is set; drawing nothing");
            return Ok(None);
        };
        let (categories, codes) = load_obs_categorical(store, store_name, column, cache_enabled).await?;
        Ok(Some(ObsColors::Categorical { column: column.to_string(), categories, codes }))
    }

    fn legend_layer(&self, colors: &ObsColors) -> Option<LegendColormapQuantitativeLayer> {
        let ObsColors::Expression { domain: (lo, hi), .. } = colors else {
            return None;
        };
        const LEGEND_PADDING_HORIZONTAL: f32 = 5.0;
        let p = &self.layer_params;
        let bounds = p.bounds.as_ref().or(self.view_params.margins.as_ref());
        let margin = |get: fn(&MarginParams) -> Option<f32>| bounds.and_then(get).unwrap_or(0.0);
        let mut color_scale = ScaleLinear::new();
        color_scale.set_domain((*lo as f64, *hi as f64));
        Some(LegendColormapQuantitativeLayer::new(
            self.view_params.clone(),
            LegendColormapQuantitativeLayerParams {
                layer_id: format!("{}_legend_sublayer", p.layer_id),
                bounds: Some(MarginParams {
                    margin_left: Some(self.view_params.width as f32 - margin(|m| m.margin_right) + LEGEND_PADDING_HORIZONTAL),
                    margin_right: Some(LEGEND_PADDING_HORIZONTAL),
                    margin_top: Some(margin(|m| m.margin_top)),
                    margin_bottom: Some(0.0),
                }),
                title: p.legend_title.clone().or(p.var_name.clone()).unwrap_or_default(),
                colormap: p.quantitative_colormap.unwrap_or(QuantitativeColormap::Viridis),
                reverse: false,
                scale: Some(color_scale),
                orientation: LegendOrientation::Horizontal,
            },
        ))
    }
}

type ObsData = (Arc<NumericData>, Vec<pluot_core::render_traits::EmphasisCriteria>, Vec<(Arc<Vec<String>>, Arc<NumericData>)>, Arc<Vec<String>>, Option<ObsColors>);

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for AdataZarrHilbertHeatmapLayer {
    async fn prepare(&mut self, gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        let p = &self.layer_params;
        let (store, store_name, cache_enabled) = (self.store.clone(), self.store_name.as_str(), self.view_params.cache_enabled);
        // The obs criteria only need the matrix for the number of obs, which `X` always has.
        let matrix_path = expr_array_path(None);
        let obs_future = async {
            futures::try_join!(
                load_nested_grouped_obs_indices(gpu_context, store.clone(), store_name, p.obs_filtering.as_ref(), &p.groupby, cache_enabled),
                load_axis_emphasis_criteria(gpu_context, store.clone(), store_name, &matrix_path, MatrixAxis::Obs, p.obs_selection.as_ref(), cache_enabled),
                futures::future::try_join_all(p.groupby.iter().map(|column| load_obs_categorical(store.clone(), store_name, column, cache_enabled))),
                load_axis_labels(store.clone(), store_name, MatrixAxis::Obs, None, cache_enabled),
                self.load_colors(gpu_context),
            )
        };
        let loaded: ObsData = match maybe_timeout!(obs_future, self.view_params.timeout).await {
            Ok(Ok(loaded)) => loaded,
            Ok(Err(error)) if is_timed_out_zarrs_error(&error) => return PrepareResult { bailed_early: true },
            Ok(Err(error)) => panic!("Zarrs error during AdataZarrHilbertHeatmapLayer prepare: {error:?}"),
            Err(_) => return PrepareResult { bailed_early: true },
        };
        let (ordered_indices, selection_criteria, groupby_columns, obs_labels, colors) = loaded;
        let Some(colors) = colors else {
            self.sub_layer_instances = Vec::new();
            self.obs = None;
            return PrepareResult { bailed_early: false };
        };

        let (values, colormap) = match &colors {
            ObsColors::Expression { values, domain } => (
                NumericData::Float32(values.clone()),
                HeatmapColormap::Quantitative(HeatmapQuantitativeColormapParams {
                    colormap: p.quantitative_colormap.unwrap_or(QuantitativeColormap::Viridis),
                    reverse: false,
                    domain: Some(*domain),
                    axis_domains: None,
                }),
            ),
            ObsColors::Categorical { codes, .. } => (
                codes.as_ref().clone(),
                HeatmapColormap::Categorical(p.categorical_colormap.unwrap_or(CategoricalColormap::Tableau10)),
            ),
        };
        let mut sub_layer_instances: Vec<Box<dyn PreparedAndDraw>> = vec![Box::new(HilbertHeatmapLayer::new(
            self.view_params.clone(),
            HilbertHeatmapLayerParams {
                layer_id: format!("{}_hilbert_heatmap_sublayer", p.layer_id),
                bounds: p.bounds.clone(),
                cell_size: p.cell_size,
                ordered_indices: ordered_indices.as_ref().clone(),
                values,
                colormap: Some(colormap),
                selection_criteria,
                background_opacity: p.background_opacity,
                ..Default::default()
            },
        ))];
        if let Some(legend_layer) = self.legend_layer(&colors) {
            sub_layer_instances.push(Box::new(legend_layer));
        }
        for sub_layer in &mut sub_layer_instances {
            sub_layer.prepare(gpu_context).await;
        }

        self.sub_layer_instances = sub_layer_instances;
        self.obs = Some(PreparedObs { obs_labels, groupby_columns, colors });
        PrepareResult { bailed_early: false }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for AdataZarrHilbertHeatmapLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut GpuRenderPass<'_>) {
        base_draw_composite_layer(&self.sub_layer_instances, gpu_context, pass).await;
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for AdataZarrHilbertHeatmapLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for AdataZarrHilbertHeatmapLayer {
    async fn draw(&self, ctx: &mut SvgContext) {
        base_draw_composite_layer_svg(&self.sub_layer_instances, ctx).await
    }
}

impl ExtentableLayer for AdataZarrHilbertHeatmapLayer {}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(optional_fields = nullable)]
pub struct AdataZarrHilbertHeatmapPickingInfo {
    pub obs_name: String,
    pub obs_index: usize,
    /// The `obs` columns the obs are ordered by, from coarsest to finest.
    pub groupby: Vec<String>,
    /// The picked obs's category in each `groupby` column, or `None` where missing.
    pub obs_values: Vec<Option<String>>,
    /// The `obs` column colored by, when coloring by category.
    pub color_by: Option<String>,
    /// The picked obs's category in `color_by`, when coloring by category.
    pub color_by_value: Option<String>,
    /// The var colored by, when coloring by expression.
    pub var_name: Option<String>,
    /// The picked obs's expression of `var_name`, when coloring by expression.
    pub expression: Option<f32>,
    pub is_selected: bool,
}

fn category_of(categories: &[String], codes: &NumericData, obs_index: usize) -> Option<String> {
    let code = codes.get_f32(obs_index);
    if code < 0.0 { None } else { categories.get(code as usize).cloned() }
}

impl PickableLayer for AdataZarrHilbertHeatmapLayer {
    fn pick(&self, screen_coord: ScreenCoord, data_coord: Option<DataCoord>) -> Option<LayerPickingResult> {
        let cell: HilbertHeatmapPickingInfo = self.sub_layer_instances.first()?.pick(screen_coord, data_coord)?.into_info()?;
        let obs = self.obs.as_ref()?;
        let obs_index = cell.index;
        let (color_by, color_by_value, expression) = match &obs.colors {
            ObsColors::Expression { values, .. } => (None, None, values.get(obs_index).copied()),
            ObsColors::Categorical { column, categories, codes } => (Some(column.clone()), category_of(categories, codes, obs_index), None),
        };
        Some(LayerPickingResult::new(&self.layer_params.layer_id, AdataZarrHilbertHeatmapPickingInfo {
            obs_name: obs.obs_labels.get(obs_index)?.clone(),
            obs_index,
            groupby: self.layer_params.groupby.clone(),
            obs_values: obs.groupby_columns.iter().map(|(categories, codes)| category_of(categories, codes, obs_index)).collect(),
            color_by,
            color_by_value,
            var_name: expression.and(self.layer_params.var_name.clone()),
            expression,
            is_selected: cell.is_selected,
        }))
    }
}
