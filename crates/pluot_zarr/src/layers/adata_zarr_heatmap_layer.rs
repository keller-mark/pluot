use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use zarrs::array::ArrayError;
use zarrs::storage::AsyncReadableStorageTraits;

use pluot_core::composite_layer::{base_draw_composite_layer, base_draw_composite_layer_svg};
use pluot_core::composite_layers::axis_band_layer::{AxisBandLayer, AxisBandLayerParams};
use pluot_core::composite_layers::axis_linear_layer::AxisPosition;
use pluot_core::composite_layers::legend_colormap_quantitative_layer::{
    LegendColormapQuantitativeLayer, LegendColormapQuantitativeLayerParams, LegendOrientation,
};
use pluot_core::d3::scale::ScaleLinear;
use pluot_core::compute::matrix_axis_extents::MatrixAxis as HeatmapAxis;
use pluot_core::layers::heatmap_layer::{HeatmapAxisDomains, HeatmapColormap, HeatmapLayer, HeatmapLayerParams};
use pluot_core::numeric_data::NumericData;
use pluot_core::render_traits::{
    resolve_store_name, BrushableLayer, DrawToRasterCpu, DrawToRasterGpu, DrawToSvg, ExtentableLayer, MarginParams,
    PickableLayer, PreparedAndDraw, PreparedLayer, UnitsMode, ViewParams,
};
use pluot_core::render_types::{CpuContext, CpuRenderPass, GpuContext, PrepareResult};
use pluot_core::two::svg::SvgContext;
use pluot_core::viewport::{DataCoord, ScreenCoord};
use pluot_core::wgpu;
use pluot_core::zarr::is_timed_out_zarrs_error;
use pluot_core::{maybe_timeout, Duration, FutureExt, LayerExtentResult, LayerPickingResult};

use crate::heatmap_data::{
    indices_of, load_axis_indices, load_axis_labels, load_axis_selection_mask, load_matrix_block,
    load_matrix_block_axis_extents, load_matrix_block_extent, AxisCriteria, HeatmapNormalization, MatrixAxis,
    MatrixBlockKey, MatrixCacheMode,
};

/// Upper bound on the number of matrix elements loaded (and cached) per row
/// block. Each block is its own sublayer, so blocks appear as they load.
const ELEMENTS_PER_BLOCK: usize = 1 << 22;

/// Layer params struct for [`AdataZarrHeatmapLayer`].
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct AdataZarrHeatmapLayerParams {
    pub layer_id: String,
    pub bounds: Option<MarginParams>,
    /// Must point to the root of an anndata object.
    pub store_name: Option<String>,
    /// Layer in AnnData to use for matrix values: "X" for `adata.X`, else a `layers` key.
    pub layer: String,

    /// Which obs (rows) to include. `None` includes every obs.
    pub obs_filtering: Option<AxisCriteria>,
    /// Which of the included obs to emphasize. `None` emphasizes every obs.
    pub obs_selection: Option<AxisCriteria>,
    /// Which var (columns) to include. `None` includes every var.
    pub var_filtering: Option<AxisCriteria>,
    /// Which of the included var to emphasize. `None` emphasizes every var.
    pub var_selection: Option<AxisCriteria>,

    /// Key in adata.var to use for gene symbols in axis labels, if different from adata.var.index.
    pub gene_symbols: Option<String>,

    /// When false, obs run top-to-bottom along the y axis and var left-to-right along the x axis.
    pub swap_axes: bool,
    /// Defaults to viridis, with the domain spanning the loaded values.
    pub colormap: Option<HeatmapColormap>,
    /// Opacity of cells whose obs or var is not selected.
    pub background_opacity: Option<f32>,

    /// Whether to render the obs axis (a band axis with one tick and label
    /// per filtered obs). Defaults to true.
    pub show_obs_axis: Option<bool>,
    /// Whether to render the var axis (a band axis with one tick and label
    /// per filtered var). Defaults to true.
    pub show_var_axis: Option<bool>,
    /// Title of the quantitative colormap legend. Defaults to "Expression".
    pub legend_title: Option<String>,

    // TODO: axis label name for each axis.

    // TODO: for each axis, a boolean flag param which, when true, enables automatically hiding axis tick labels when the current row/col size in pixels drops below the font size, to prevent the tick text from overlapping.
    // Instead, when the ticks are hidden, we will only show an axis label text element.
    // When the flag is false, we will always render the ticks.

    /// How loaded matrix values are cached. Defaults to [`MatrixCacheMode::RowBlocks`].
    pub cache_mode: Option<MatrixCacheMode>,
    /// How a quantitative colormap without an explicit `domain` or
    /// `axis_domains` is normalized. Defaults to [`HeatmapNormalization::Matrix`].
    pub normalization: Option<HeatmapNormalization>,
}

impl Default for AdataZarrHeatmapLayerParams {
    fn default() -> Self {
        Self {
            layer_id: "".to_string(),
            bounds: None,
            store_name: None,
            layer: "X".to_string(),
            obs_filtering: None,
            obs_selection: None,
            var_filtering: None,
            var_selection: None,
            gene_symbols: None,
            swap_axes: false,
            colormap: None,
            background_opacity: None,
            show_obs_axis: None,
            show_var_axis: None,
            legend_title: None,
            cache_mode: None,
            normalization: None,
        }
    }
}

/// Everything about the filtered axes that `pick` and `extent` need, kept from `prepare`.
struct PreparedAxes {
    obs_indices: Arc<Vec<u32>>,
    var_indices: Arc<Vec<u32>>,
    obs_labels: Arc<Vec<String>>,
    var_labels: Arc<Vec<String>>,
}

/// Each cell is drawn one data unit wide and tall, so fitting the camera to
/// [`ExtentableLayer::extent`] frames the whole (filtered) heatmap.
pub struct AdataZarrHeatmapLayer {
    view_params: ViewParams,
    layer_params: AdataZarrHeatmapLayerParams,
    store: Arc<dyn AsyncReadableStorageTraits>,
    store_name: String,

    sub_layer_instances: Vec<Box<dyn PreparedAndDraw>>,
    /// The first filtered row of each heatmap sublayer, which are the first
    /// `block_first_rows.len()` entries of `sub_layer_instances`.
    block_first_rows: Vec<usize>,
    /// `None` until a `prepare` has resolved the filtered axes.
    axes: Option<PreparedAxes>,
}

impl AdataZarrHeatmapLayer {
    pub fn new(view_params: ViewParams, layer_params: AdataZarrHeatmapLayerParams) -> Self {
        let store_name = resolve_store_name(&layer_params.store_name, &view_params);
        let store = view_params.get_store(&store_name);
        Self {
            view_params,
            layer_params,
            store,
            store_name,
            sub_layer_instances: Vec::new(),
            block_first_rows: Vec::new(),
            axes: None,
        }
    }

    fn matrix_path(&self) -> String {
        if self.layer_params.layer == "X" { "/X".to_string() } else { format!("/layers/{}", self.layer_params.layer) }
    }

    fn margins(&self) -> (f32, f32, f32, f32) {
        let bounds = self.layer_params.bounds.as_ref().or(self.view_params.margins.as_ref());
        let margin = |f: fn(&MarginParams) -> Option<f32>| bounds.and_then(f).unwrap_or(0.0);
        (margin(|m| m.margin_top), margin(|m| m.margin_right), margin(|m| m.margin_bottom), margin(|m| m.margin_left))
    }

}

type AxesData = (Arc<NumericData>, Arc<NumericData>, Option<Arc<NumericData>>, Option<Arc<NumericData>>, Arc<Vec<String>>, Arc<Vec<String>>);

/// `(lo, hi)`, or `(0, 1)` when there were no finite values to take an extent over.
fn finite_or_unit(lo: f32, hi: f32) -> (f32, f32) {
    if lo.is_finite() && hi.is_finite() { (lo, hi) } else { (0.0, 1.0) }
}

fn axis_domains(axis: HeatmapAxis, mins: Vec<f32>, maxs: Vec<f32>) -> HeatmapAxisDomains {
    let (mins, maxs): (Vec<f32>, Vec<f32>) = mins.into_iter().zip(maxs).map(|(lo, hi)| finite_or_unit(lo, hi)).unzip();
    HeatmapAxisDomains { axis, min: NumericData::from(mins), max: NumericData::from(maxs) }
}

/// `None` when the store has not finished loading what was asked for yet.
fn ok_unless_timed_out<T>(result: Result<T, ArrayError>, what: &str) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(error) if is_timed_out_zarrs_error(&error) => None,
        Err(error) => panic!("Zarrs error loading {what} for AdataZarrHeatmapLayer: {error:?}"),
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for AdataZarrHeatmapLayer {
    async fn prepare(&mut self, gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        let store = self.store.clone();
        let store_name = self.store_name.clone();
        let cache_enabled = self.view_params.cache_enabled;
        let timeout = self.view_params.timeout;
        let matrix_path = self.matrix_path();
        let params = self.layer_params.clone();

        let axes_future = async {
            futures::try_join!(
                load_axis_indices(gpu_context, store.clone(), &store_name, &matrix_path, MatrixAxis::Obs, params.obs_filtering.as_ref(), cache_enabled),
                load_axis_indices(gpu_context, store.clone(), &store_name, &matrix_path, MatrixAxis::Var, params.var_filtering.as_ref(), cache_enabled),
                load_axis_selection_mask(gpu_context, store.clone(), &store_name, &matrix_path, MatrixAxis::Obs, params.obs_filtering.as_ref(), params.obs_selection.as_ref(), cache_enabled),
                load_axis_selection_mask(gpu_context, store.clone(), &store_name, &matrix_path, MatrixAxis::Var, params.var_filtering.as_ref(), params.var_selection.as_ref(), cache_enabled),
                load_axis_labels(store.clone(), &store_name, MatrixAxis::Obs, None, cache_enabled),
                load_axis_labels(store.clone(), &store_name, MatrixAxis::Var, params.gene_symbols.as_deref(), cache_enabled),
            )
        };
        let axes: Option<AxesData> = match maybe_timeout!(axes_future, timeout).await {
            Ok(result) => ok_unless_timed_out(result, "axis indices, masks and labels"),
            Err(_) => None,
        };
        let Some((obs_indices, var_indices, obs_mask, var_mask, obs_labels, var_labels)) = axes else {
            self.sub_layer_instances = Vec::new();
            self.block_first_rows = Vec::new();
            self.axes = None;
            return PrepareResult { bailed_early: true };
        };
        let rows = indices_of(&obs_indices);
        let cols = indices_of(&var_indices);
        let (num_rows, num_cols) = (rows.len(), cols.len());

        let rows_per_block = (ELEMENTS_PER_BLOCK / num_cols.max(1)).max(1);
        let block_first_rows: Vec<usize> = (0..num_rows).step_by(rows_per_block).collect();
        let block_key = |block_index: usize| MatrixBlockKey {
            store_name: &store_name,
            matrix_path: &matrix_path,
            cache_mode: params.cache_mode.unwrap_or_default(),
            obs_filtering: params.obs_filtering.as_ref(),
            var_filtering: params.var_filtering.as_ref(),
            rows_per_block,
            block_index,
        };

        let mut blocks: Vec<Option<Arc<NumericData>>> = vec![None; block_first_rows.len()];
        let block_futures = blocks.iter_mut().zip(&block_first_rows).enumerate().map(|(block_index, (slot, &first_row))| {
            let store = store.clone();
            let key = block_key(block_index);
            let block_rows = &rows[first_row..(first_row + rows_per_block).min(num_rows)];
            async move {
                *slot = ok_unless_timed_out(load_matrix_block(store, &key, block_rows, cols, cache_enabled).await, "a matrix block");
            }
        });
        let _ = maybe_timeout!(futures::future::join_all(block_futures), timeout).await;
        let bailed_early = blocks.iter().any(Option::is_none);

        let block_num_rows = |block_index: usize| rows_per_block.min(num_rows - block_first_rows[block_index]);
        let colormap = params.colormap.clone().unwrap_or_default();
        let normalization = match &colormap {
            HeatmapColormap::Quantitative(quantitative) if quantitative.domain.is_none() && quantitative.axis_domains.is_none() => {
                Some(params.normalization.unwrap_or_default())
            }
            _ => None,
        };
        let with_domains = |domain: Option<(f32, f32)>, axis_domains: Option<HeatmapAxisDomains>| match &colormap {
            HeatmapColormap::Quantitative(quantitative) => {
                let mut quantitative = quantitative.clone();
                quantitative.domain = domain;
                quantitative.axis_domains = axis_domains;
                HeatmapColormap::Quantitative(quantitative)
            }
            other => other.clone(),
        };
        let mut block_colormaps: Vec<HeatmapColormap> = vec![colormap.clone(); blocks.len()];
        match normalization {
            None => {}
            Some(HeatmapNormalization::Matrix) => {
                let mut domain = (f32::INFINITY, f32::NEG_INFINITY);
                for (block_index, block) in blocks.iter().enumerate() {
                    if let Some(block) = block {
                        let (min, max) = load_matrix_block_extent(gpu_context, &block_key(block_index), block, cache_enabled).await;
                        domain = (domain.0.min(min), domain.1.max(max));
                    }
                }
                let (lo, hi) = finite_or_unit(domain.0, domain.1);
                block_colormaps.fill(with_domains(Some((lo, hi)), None));
            }
            Some(HeatmapNormalization::PerObs) => {
                for (block_index, block) in blocks.iter().enumerate() {
                    if let Some(block) = block {
                        let (mins, maxs) = load_matrix_block_axis_extents(
                            gpu_context, &block_key(block_index), block, block_num_rows(block_index), num_cols, HeatmapAxis::Rows, cache_enabled,
                        )
                        .await;
                        block_colormaps[block_index] = with_domains(None, Some(axis_domains(HeatmapAxis::Rows, mins, maxs)));
                    }
                }
            }
            Some(HeatmapNormalization::PerVar) => {
                let (mut mins, mut maxs) = (vec![f32::INFINITY; num_cols], vec![f32::NEG_INFINITY; num_cols]);
                for (block_index, block) in blocks.iter().enumerate() {
                    if let Some(block) = block {
                        let (block_mins, block_maxs) = load_matrix_block_axis_extents(
                            gpu_context, &block_key(block_index), block, block_num_rows(block_index), num_cols, HeatmapAxis::Cols, cache_enabled,
                        )
                        .await;
                        for col in 0..num_cols {
                            mins[col] = mins[col].min(block_mins[col]);
                            maxs[col] = maxs[col].max(block_maxs[col]);
                        }
                    }
                }
                block_colormaps.fill(with_domains(None, Some(axis_domains(HeatmapAxis::Cols, mins, maxs))));
            }
        }

        let swap_axes = params.swap_axes;

        let mut sub_layer_instances: Vec<Box<dyn PreparedAndDraw>> = Vec::new();
        let mut loaded_block_first_rows = Vec::new();
        for (block_index, block) in blocks.into_iter().enumerate() {
            let Some(block) = block else { continue };
            let first_row = block_first_rows[block_index];
            let block_num_rows = block_num_rows(block_index);
            let cell_offset = if swap_axes {
                (first_row as f32, 0.0)
            } else {
                (0.0, (num_rows - first_row - block_num_rows) as f32)
            };
            let row_selection = obs_mask.as_ref().map(|mask| match mask.as_ref() {
                NumericData::Uint8(mask) => NumericData::from(mask[first_row..first_row + block_num_rows].to_vec()),
                _ => unreachable!("selection masks are memoized as Uint8"),
            });
            let mut heatmap_layer = HeatmapLayer::new(
                self.view_params.clone(),
                HeatmapLayerParams {
                    layer_id: format!("{}_heatmap_sublayer_{block_index}", params.layer_id),
                    bounds: params.bounds.clone(),
                    cell_offset: Some(cell_offset),
                    num_rows: block_num_rows as u32,
                    num_cols: num_cols as u32,
                    data: block.as_ref().clone(),
                    swap_axes,
                    colormap: Some(block_colormaps[block_index].clone()),
                    row_selection,
                    col_selection: var_mask.as_ref().map(|mask| mask.as_ref().clone()),
                    background_opacity: params.background_opacity,
                    ..Default::default()
                },
            );
            heatmap_layer.prepare(gpu_context).await;
            sub_layer_instances.push(Box::new(heatmap_layer));
            loaded_block_first_rows.push(first_row);
        }

        // The obs axis runs along y (x when swapped) and the var axis along x
        // (y when swapped). Band axes run bottom-to-top along y, whereas the
        // first row/column is drawn at the top, so a y axis's labels are reversed.
        let axes = [
            ("obs", params.show_obs_axis, obs_labels.as_slice(), rows, swap_axes),
            ("var", params.show_var_axis, var_labels.as_slice(), cols, !swap_axes),
        ];
        for (name, shown, all_labels, indices, along_x) in axes {
            if !shown.unwrap_or(true) {
                continue;
            }
            let mut labels: Vec<String> = indices.iter().map(|&index| all_labels[index as usize].clone()).collect();
            let position = if along_x { AxisPosition::Bottom } else { AxisPosition::Left };
            if !along_x {
                labels.reverse();
            }
            let num_cells = labels.len();
            let mut axis_layer = AxisBandLayer::new(
                self.view_params.clone(),
                AxisBandLayerParams {
                    layer_id: format!("{}_{name}_axis_sublayer", params.layer_id),
                    position,
                    domain: Arc::new(labels),
                    data_range: Some((0.0, num_cells as f64)),
                },
            );
            axis_layer.prepare(gpu_context).await;
            sub_layer_instances.push(Box::new(axis_layer));
        }

        if let HeatmapColormap::Quantitative(quantitative) = block_colormaps.first().unwrap_or(&colormap) {
            const LEGEND_PADDING_HORIZONTAL: f32 = 5.0;
            let (margin_top, margin_right, _, _) = self.margins();
            // Per-obs or per-var domains have no single range to label, so the legend spans 0 to 1.
            let color_scale = match (quantitative.domain, &quantitative.axis_domains) {
                (_, Some(_)) => None,
                (domain, None) => {
                    let (lo, hi) = domain.unwrap_or((0.0, 1.0));
                    let mut color_scale = ScaleLinear::new();
                    color_scale.set_domain((lo as f64, hi as f64));
                    Some(color_scale)
                }
            };
            let default_title = match normalization {
                Some(HeatmapNormalization::PerObs) => "Scaled per obs",
                Some(HeatmapNormalization::PerVar) => "Scaled per var",
                _ => "Expression",
            };
            let mut legend_layer = LegendColormapQuantitativeLayer::new(
                self.view_params.clone(),
                LegendColormapQuantitativeLayerParams {
                    layer_id: format!("{}_legend_sublayer", params.layer_id),
                    bounds: Some(MarginParams {
                        margin_left: Some(self.view_params.width as f32 - margin_right + LEGEND_PADDING_HORIZONTAL),
                        margin_right: Some(LEGEND_PADDING_HORIZONTAL),
                        margin_top: Some(margin_top),
                        margin_bottom: Some(0.0),
                    }),
                    title: params.legend_title.clone().unwrap_or_else(|| default_title.to_string()),
                    colormap: quantitative.colormap,
                    reverse: quantitative.reverse,
                    scale: color_scale,
                    orientation: LegendOrientation::Horizontal,
                },
            );
            legend_layer.prepare(gpu_context).await;
            sub_layer_instances.push(Box::new(legend_layer));
        }

        let owned_indices = |data: &NumericData| Arc::new(indices_of(data).to_vec());
        self.axes = Some(PreparedAxes {
            obs_indices: owned_indices(&obs_indices),
            var_indices: owned_indices(&var_indices),
            obs_labels,
            var_labels,
        });
        self.sub_layer_instances = sub_layer_instances;
        self.block_first_rows = loaded_block_first_rows;
        PrepareResult { bailed_early }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for AdataZarrHeatmapLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        base_draw_composite_layer(&self.sub_layer_instances, gpu_context, pass).await;
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for AdataZarrHeatmapLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for AdataZarrHeatmapLayer {
    async fn draw(&self, ctx: &mut SvgContext) {
        base_draw_composite_layer_svg(&self.sub_layer_instances, ctx).await
    }
}

impl BrushableLayer for AdataZarrHeatmapLayer {}

impl ExtentableLayer for AdataZarrHeatmapLayer {
    /// The number of filtered cells along x and y, i.e. var and obs counts
    /// (swapped when `swap_axes` is set), with zero as the minimum.
    fn extent(&self) -> Option<LayerExtentResult> {
        let axes = self.axes.as_ref()?;
        let (num_obs, num_var) = (axes.obs_indices.len() as f32, axes.var_indices.len() as f32);
        let (x_max, y_max) = if self.layer_params.swap_axes { (num_obs, num_var) } else { (num_var, num_obs) };
        Some(LayerExtentResult {
            layer_id: self.layer_params.layer_id.clone(),
            x: (0.0, x_max),
            y: (0.0, y_max),
            z: None,
        })
    }
}

impl PickableLayer for AdataZarrHeatmapLayer {
    /// Returns the picked cell's "obs_name", "var_name", "obs_index", "var_index" and "value".
    fn pick(&self, screen_coord: ScreenCoord, data_coord: Option<DataCoord>) -> Option<LayerPickingResult> {
        let (block_first_row, cell) = self.sub_layer_instances.iter().zip(&self.block_first_rows).find_map(|(sublayer, &first_row)| {
            sublayer.pick(screen_coord, data_coord).map(|cell| (first_row, cell))
        })?;
        let row: usize = block_first_row + cell.info.get("row")?.parse::<usize>().ok()?;
        let col: usize = cell.info.get("col")?.parse().ok()?;
        let axes = self.axes.as_ref()?;
        let obs_index = *axes.obs_indices.get(row)? as usize;
        let var_index = *axes.var_indices.get(col)? as usize;

        let mut info = HashMap::new();
        info.insert("obs_name".to_string(), axes.obs_labels.get(obs_index)?.clone());
        info.insert("var_name".to_string(), axes.var_labels.get(var_index)?.clone());
        info.insert("obs_index".to_string(), obs_index.to_string());
        info.insert("var_index".to_string(), var_index.to_string());
        info.insert("value".to_string(), cell.info.get("value")?.clone());
        Some(LayerPickingResult { layer_id: self.layer_params.layer_id.clone(), info })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axis_domains_replace_empty_extents_with_the_unit_domain() {
        let domains = axis_domains(HeatmapAxis::Cols, vec![1.0, f32::INFINITY], vec![5.0, f32::NEG_INFINITY]);
        assert_eq!(domains.min.as_f32().as_ref(), &[1.0, 0.0]);
        assert_eq!(domains.max.as_f32().as_ref(), &[5.0, 1.0]);
    }
}
