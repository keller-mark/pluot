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
use pluot_core::layers::heatmap_layer::{HeatmapColormap, HeatmapLayer, HeatmapLayerParams};
use pluot_core::numeric_data::NumericData;
use pluot_core::positioning::get_point_position;
use pluot_core::render_traits::{
    resolve_store_name, BrushableLayer, DrawToRasterCpu, DrawToRasterGpu, DrawToSvg, ExtentableLayer, MarginParams,
    PickableLayer, PreparedAndDraw, PreparedLayer, UnitsMode, ViewParams,
};
use pluot_core::render_types::{CpuContext, CpuRenderPass, GpuContext, PrepareResult};
use pluot_core::two::svg::SvgContext;
use pluot_core::viewport::{DataCoord, ScreenCoord};
use pluot_core::wgpu;
use pluot_core::zarr::is_timed_out_zarrs_error;
use pluot_core::{maybe_timeout, Duration, FutureExt, LayerPickingResult};

use crate::heatmap_data::{
    indices_of, load_axis_indices, load_axis_labels, load_axis_selection_mask, load_matrix_block,
    load_matrix_block_extent, AxisCriteria, MatrixAxis, MatrixBlockKey,
};

/// Upper bound on the number of matrix elements loaded (and cached) per row
/// block. Each block is its own sublayer, so blocks appear as they load.
const ELEMENTS_PER_BLOCK: usize = 1 << 22;

const IDENTITY_MATRIX: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0,
    0.0, 1.0, 0.0, 0.0,
    0.0, 0.0, 1.0, 0.0,
    0.0, 0.0, 0.0, 1.0,
];

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

    /// Whether to label each obs along its axis. Defaults to true.
    pub show_obs_labels: Option<bool>,
    /// Whether to label each var along its axis. Defaults to true.
    pub show_var_labels: Option<bool>,
    /// Title of the quantitative colormap legend. Defaults to "Expression".
    pub legend_title: Option<String>,

    // TODO: add a flag which changes the behavior of how the matrix data is cached: cached as a single independent NumericData per gene, versus cached as the full expression matrix as a per-block NumericData.

    // TODO: support whole-matrix, row-wise, or column-wise normalization.
    // Compute the min/max value(s) on the GPU (via compute shader) or CPU.
    // Modify the base heatmap_layer to allow providing a quantitative colormap domain globally or per-row or per-column.
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
            show_obs_labels: None,
            show_var_labels: None,
            legend_title: None,
        }
    }
}

/// Everything about the filtered axes that `pick` needs, kept from `prepare`.
#[derive(Default)]
struct PreparedAxes {
    obs_indices: Arc<Vec<u32>>,
    var_indices: Arc<Vec<u32>>,
    obs_labels: Arc<Vec<String>>,
    var_labels: Arc<Vec<String>>,
}

pub struct AdataZarrHeatmapLayer {
    view_params: ViewParams,
    layer_params: AdataZarrHeatmapLayerParams,
    store: Arc<dyn AsyncReadableStorageTraits>,
    store_name: String,

    sub_layer_instances: Vec<Box<dyn PreparedAndDraw>>,
    /// The first filtered row of each heatmap sublayer, which are the first
    /// `block_first_rows.len()` entries of `sub_layer_instances`.
    block_first_rows: Vec<usize>,
    axes: PreparedAxes,
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
            axes: PreparedAxes::default(),
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

    /// The model matrix that stretches a `display_w x display_h` grid of cells
    /// over the whole layer when the camera is at identity, whatever the view's
    /// aspect ratio mode.
    fn fit_model_matrix(&self, display_w: usize, display_h: usize) -> [f32; 16] {
        let (margin_top, margin_right, margin_bottom, margin_left) = self.margins();
        let layer_w = self.view_params.width as f32 - (margin_left + margin_right);
        let layer_h = self.view_params.height as f32 - (margin_top + margin_bottom);
        let position = |x: f32, y: f32| {
            get_point_position(
                x, y, layer_w, layer_h, &IDENTITY_MATRIX, UnitsMode::Data, UnitsMode::Data,
                self.view_params.aspect_ratio_mode, self.view_params.aspect_ratio_alignment_mode, None,
            )
        };
        let (origin, unit) = (position(0.0, 0.0), position(1.0, 1.0));
        let data_at = |px: f32, origin_px: f32, unit_px: f32| (px - origin_px) / (unit_px - origin_px);
        let (x0, x1) = (data_at(0.0, origin.0, unit.0), data_at(layer_w, origin.0, unit.0));
        let (y0, y1) = (data_at(0.0, origin.1, unit.1), data_at(layer_h, origin.1, unit.1));
        [
            (x1 - x0) / display_w.max(1) as f32, 0.0, 0.0, 0.0,
            0.0, (y1 - y0) / display_h.max(1) as f32, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            x0, y0, 0.0, 1.0,
        ]
    }
}

type AxesData = (Arc<NumericData>, Arc<NumericData>, Option<Arc<NumericData>>, Option<Arc<NumericData>>, Arc<Vec<String>>, Arc<Vec<String>>);

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

        let colormap = match params.colormap.clone().unwrap_or_default() {
            HeatmapColormap::Quantitative(mut quantitative) if quantitative.domain.is_none() => {
                let mut domain = (f32::INFINITY, f32::NEG_INFINITY);
                for (block_index, block) in blocks.iter().enumerate() {
                    if let Some(block) = block {
                        let (min, max) = load_matrix_block_extent(gpu_context, &block_key(block_index), block, cache_enabled).await;
                        domain = (domain.0.min(min), domain.1.max(max));
                    }
                }
                quantitative.domain = Some(if domain.0.is_finite() && domain.1.is_finite() { domain } else { (0.0, 1.0) });
                HeatmapColormap::Quantitative(quantitative)
            }
            colormap => colormap,
        };

        let swap_axes = params.swap_axes;
        let (display_w, display_h) = if swap_axes { (num_rows, num_cols) } else { (num_cols, num_rows) };
        let model_matrix = self.fit_model_matrix(display_w, display_h);

        let mut sub_layer_instances: Vec<Box<dyn PreparedAndDraw>> = Vec::new();
        let mut loaded_block_first_rows = Vec::new();
        for (block_index, block) in blocks.into_iter().enumerate() {
            let Some(block) = block else { continue };
            let first_row = block_first_rows[block_index];
            let block_num_rows = rows_per_block.min(num_rows - first_row);
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
                    model_matrix: Some(model_matrix),
                    num_rows: block_num_rows as u32,
                    num_cols: num_cols as u32,
                    data: block.as_ref().clone(),
                    swap_axes,
                    colormap: Some(colormap.clone()),
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

        let filtered_labels = |labels: &[String], indices: &[u32]| -> Vec<String> {
            indices.iter().map(|&index| labels[index as usize].clone()).collect()
        };
        let obs_axis_labels = filtered_labels(&obs_labels, rows);
        let var_axis_labels = filtered_labels(&var_labels, cols);
        // Band axes run bottom-to-top along y, whereas the first row/column is drawn at the top.
        let (x_labels, y_labels, x_shown, y_shown) = if swap_axes {
            (obs_axis_labels, var_axis_labels.into_iter().rev().collect(), params.show_obs_labels, params.show_var_labels)
        } else {
            (var_axis_labels, obs_axis_labels.into_iter().rev().collect(), params.show_var_labels, params.show_obs_labels)
        };
        for (labels, shown, position, name) in [(x_labels, x_shown, AxisPosition::Bottom, "x"), (y_labels, y_shown, AxisPosition::Left, "y")] {
            if !shown.unwrap_or(true) {
                continue;
            }
            let mut axis_layer = AxisBandLayer::new(
                self.view_params.clone(),
                AxisBandLayerParams {
                    layer_id: format!("{}_{name}_axis_sublayer", params.layer_id),
                    position,
                    domain: Arc::new(labels),
                },
            );
            axis_layer.prepare(gpu_context).await;
            sub_layer_instances.push(Box::new(axis_layer));
        }

        if let HeatmapColormap::Quantitative(quantitative) = &colormap {
            const LEGEND_PADDING_HORIZONTAL: f32 = 5.0;
            let (margin_top, margin_right, _, _) = self.margins();
            let (lo, hi) = quantitative.domain.unwrap_or((0.0, 1.0));
            let mut color_scale = ScaleLinear::new();
            color_scale.set_domain((lo as f64, hi as f64));
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
                    title: params.legend_title.clone().unwrap_or_else(|| "Expression".to_string()),
                    colormap: quantitative.colormap,
                    reverse: quantitative.reverse,
                    scale: Some(color_scale),
                    orientation: LegendOrientation::Horizontal,
                },
            );
            legend_layer.prepare(gpu_context).await;
            sub_layer_instances.push(Box::new(legend_layer));
        }

        let owned_indices = |data: &NumericData| Arc::new(indices_of(data).to_vec());
        self.axes = PreparedAxes {
            obs_indices: owned_indices(&obs_indices),
            var_indices: owned_indices(&var_indices),
            obs_labels,
            var_labels,
        };
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

impl ExtentableLayer for AdataZarrHeatmapLayer {}

impl PickableLayer for AdataZarrHeatmapLayer {
    /// Returns the picked cell's "obs_name", "var_name", "obs_index", "var_index" and "value".
    fn pick(&self, screen_coord: ScreenCoord, data_coord: Option<DataCoord>) -> Option<LayerPickingResult> {
        let (block_first_row, cell) = self.sub_layer_instances.iter().zip(&self.block_first_rows).find_map(|(sublayer, &first_row)| {
            sublayer.pick(screen_coord, data_coord).map(|cell| (first_row, cell))
        })?;
        let row: usize = block_first_row + cell.info.get("row")?.parse::<usize>().ok()?;
        let col: usize = cell.info.get("col")?.parse().ok()?;
        let obs_index = *self.axes.obs_indices.get(row)? as usize;
        let var_index = *self.axes.var_indices.get(col)? as usize;

        let mut info = HashMap::new();
        info.insert("obs_name".to_string(), self.axes.obs_labels.get(obs_index)?.clone());
        info.insert("var_name".to_string(), self.axes.var_labels.get(var_index)?.clone());
        info.insert("obs_index".to_string(), obs_index.to_string());
        info.insert("var_index".to_string(), var_index.to_string());
        info.insert("value".to_string(), cell.info.get("value")?.clone());
        Some(LayerPickingResult { layer_id: self.layer_params.layer_id.clone(), info })
    }
}
