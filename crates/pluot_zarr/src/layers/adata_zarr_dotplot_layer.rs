use std::collections::HashMap;
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use pluot_core::{maybe_timeout, FutureExt, Duration, log};

use pluot_core::wgpu;
use pluot_core::cache::{use_memo_vec_string, use_memo_numeric_data};
use pluot_core::zarr::is_timed_out_zarrs_error;
use zarrs::storage::AsyncReadableStorageTraits;
use pluot_core::two::svg::SvgContext;
use pluot_core::render_traits::{ColorMode, DrawToRasterCpu, DrawToRasterGpu, DrawToSvg, InstancedSizeParams, MarginParams, PickableLayer, PreparedAndDraw, PreparedLayer, QuantitativeColormap, QuantitativeParams, SizeMode, UnitsMode, ViewParams, resolve_store_name};
use pluot_core::render_types::{CpuContext, CpuRenderPass, PrepareResult};
use pluot_core::render_types::GpuContext;
use pluot_core::composite_layer::{base_draw_composite_layer, base_draw_composite_layer_svg};
use pluot_core::d3::scale::{ScaleBand, Scaleable};
use pluot_core::layers::point_layer::{PointLayer, PointLayerParams};
use pluot_core::numeric_data::NumericData;

use crate::zarr_numeric_data::load_arr_as_numeric_data;
use crate::adata_io::{read_dataframe_index, read_dense_column_f32, read_string_array};

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
        let groupby = self.layer_params.groupby.clone();
        let groupby_path = format!("obs/{groupby}");
        let expr_array_path = if self.layer_params.layer == "X" {
            "X".to_string()
        } else {
            format!("layers/{}", self.layer_params.layer)
        };

        // Load the var dataframe's row labels (gene IDs) and the groupby
        // column's categories + per-cell codes, in parallel. (The obs and
        // root anndata metadata are not needed here: a dot plot never
        // displays per-cell/per-obs identifiers.)
        let var_index_deps = vec!["adata_var_index".to_string(), self.store_name.clone()];
        let var_index_future = use_memo_vec_string(async || {
            read_dataframe_index(store.clone(), "var").await
        }, &var_index_deps, self.view_params.cache_enabled);

        let categories_deps = vec!["adata_obs_categories".to_string(), self.store_name.clone(), groupby.clone()];
        let categories_future = use_memo_vec_string(async || {
            read_string_array(store.clone(), &format!("{groupby_path}/categories")).await
        }, &categories_deps, self.view_params.cache_enabled);

        let codes_deps = vec!["adata_obs_codes".to_string(), self.store_name.clone(), groupby.clone()];
        let codes_future = use_memo_numeric_data(async || {
            load_arr_as_numeric_data(store.clone(), &format!("{groupby_path}/codes")).await
        }, &codes_deps, self.view_params.cache_enabled);

        let futures_try_join_result = futures::try_join!(
            maybe_timeout!(var_index_future, self.view_params.timeout),
            maybe_timeout!(categories_future, self.view_params.timeout),
            maybe_timeout!(codes_future, self.view_params.timeout),
        );

        let (var_index, categories, codes) = match futures_try_join_result {
            Ok((var_index_result, categories_result, codes_result)) => {
                match (var_index_result, categories_result, codes_result) {
                    (Ok(v), Ok(c), Ok(d)) => (v, c, d),
                    (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => {
                        if is_timed_out_zarrs_error(&e) {
                            return PrepareResult { bailed_early: true };
                        } else {
                            panic!("Zarrs error during AdataZarrDotPlotLayer prepare: {:?}", e);
                        }
                    }
                }
            }
            Err(_) => {
                // Wall-clock timeout from maybe_timeout!
                return PrepareResult { bailed_early: true };
            }
        };

        // Resolve the requested gene IDs to their column index in `var.index`,
        // logging (and skipping) any that aren't found.
        let var_index_lookup: HashMap<&str, usize> = var_index.iter().enumerate().map(|(i, name)| (name.as_str(), i)).collect();
        let mut found_var_names: Vec<String> = Vec::new();
        let mut gene_col_indices: Vec<u64> = Vec::new();
        for var_name in &self.layer_params.var_names {
            match var_index_lookup.get(var_name.as_str()) {
                Some(&i) => {
                    found_var_names.push(var_name.clone());
                    gene_col_indices.push(i as u64);
                }
                None => log(&format!("AdataZarrDotPlotLayer: gene \"{var_name}\" not found in var.index; skipping")),
            }
        }

        if found_var_names.is_empty() || categories.is_empty() {
            self.sub_layer_instances = vec![];
            return PrepareResult { bailed_early: false };
        }

        // Load the expression values for each requested gene (one column of
        // `X`/`layers[layer]` per gene; for now, assumed to be a dense array).
        let column_futures = gene_col_indices.iter().map(|&col_index| {
            let store = store.clone();
            let array_path = expr_array_path.clone();
            async move { read_dense_column_f32(store, &array_path, col_index).await }
        });
        let columns_result = maybe_timeout!(futures::future::join_all(column_futures), self.view_params.timeout).await;
        let columns: Vec<Vec<f32>> = match columns_result {
            Ok(results) => {
                let mut out = Vec::with_capacity(results.len());
                for r in results {
                    match r {
                        Ok(col) => out.push(col),
                        Err(e) => {
                            if is_timed_out_zarrs_error(&e) {
                                return PrepareResult { bailed_early: true };
                            } else {
                                panic!("Zarrs error during AdataZarrDotPlotLayer prepare: {:?}", e);
                            }
                        }
                    }
                }
                out
            }
            Err(_) => return PrepareResult { bailed_early: true },
        };

        // Group cell (obs) indices by their groupby category code. A code of
        // -1 (or out of range) denotes a missing value, per the AnnData spec;
        // such cells are excluded from every group's mean/fraction.
        let n_groups = categories.len();
        let mut cells_by_group: Vec<Vec<usize>> = vec![Vec::new(); n_groups];
        for cell_i in 0..codes.len() {
            let code = codes.get_f32(cell_i) as i64;
            if code >= 0 && (code as usize) < n_groups {
                cells_by_group[code as usize].push(cell_i);
            }
        }

        // For each (groupby category, gene) pair, compute the mean expression
        // and the fraction of cells expressing (value > expression_cutoff),
        // matching scanpy's `sc.pl.dotplot` semantics.
        let cutoff = self.layer_params.expression_cutoff;
        let n_genes = found_var_names.len();
        let mut mean_expr: Vec<f32> = Vec::with_capacity(n_groups * n_genes);
        let mut frac_expr: Vec<f32> = Vec::with_capacity(n_groups * n_genes);
        for cell_indices in &cells_by_group {
            for col in &columns {
                if cell_indices.is_empty() {
                    mean_expr.push(0.0);
                    frac_expr.push(0.0);
                    continue;
                }
                let mut sum = 0.0f32;
                let mut n_expressing = 0usize;
                for &cell_i in cell_indices {
                    let v = col[cell_i];
                    sum += v;
                    if v > cutoff {
                        n_expressing += 1;
                    }
                }
                mean_expr.push(sum / cell_indices.len() as f32);
                frac_expr.push(n_expressing as f32 / cell_indices.len() as f32);
            }
        }

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
        genes_scale.set_domain(found_var_names.clone());
        let mut groups_scale = ScaleBand::new();
        groups_scale.set_domain(categories.as_ref().clone());
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
            let group_pos = groups_scale.scale(&categories[g]) as f32 + groups_bandwidth / 2.0;
            for j in 0..n_genes {
                let gene_pos = genes_scale.scale(&found_var_names[j]) as f32 + genes_bandwidth / 2.0;
                let (x, y) = if swap_axes { (group_pos, gene_pos) } else { (gene_pos, group_pos) };
                position_x.push(x);
                position_y.push(y);
                point_radius_values.push(frac_expr[g * n_genes + j] * max_dot_radius);
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
                    values: NumericData::Float32(Arc::new(mean_expr)),
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

        self.sub_layer_instances = vec![Box::new(point_layer)];

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
