use std::sync::Arc;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use pluot_core::{maybe_timeout, FutureExt, Duration};

use pluot_core::log;
use pluot_core::wgpu;
use pluot_core::zarr::is_timed_out_zarrs_error;
use zarrs::storage::AsyncReadableStorageTraits;
use pluot_core::two::svg::SvgContext;
use pluot_core::render_traits::{CategoricalColormap, CategoricalParams, ColorMode, DrawToRasterCpu, DrawToRasterGpu, DrawToSvg, ExtentableLayer, MarginParams, OpacityMode, PickableLayer, PreparedLayer, ViewParams, resolve_store_name};
use pluot_core::render_types::{CpuContext, CpuRenderPass, PrepareResult};
use pluot_core::gpu_send::GpuRenderPass;
use pluot_core::render_types::GpuContext;
use pluot_core::d3::hierarchy::TreemapTile;
use pluot_core::composite_layers::precomputed_treemap_layer::{PrecomputedTreemapLayer, PrecomputedTreemapLayerParams, PrecomputedTreemapPickingInfo};
use pluot_core::numeric_data::NumericData;
use pluot_core::LayerPickingResult;
use pluot_core::viewport::DataCoord;
use pluot_core::viewport::ScreenCoord;

use crate::treemap_data::load_obs_group_counts;

const FILL_OPACITY: f32 = 0.8;

/// Layer params struct for [`AdataZarrTreemapLayer`].
#[derive(Serialize, Deserialize, Debug, Clone, TS)]
#[serde(default)]
#[ts(optional_fields = nullable)]
pub struct AdataZarrTreemapLayerParams {
    pub layer_id: String,
    pub bounds: Option<MarginParams>,

    // Data keys
    /// Must point to the root of an anndata object.
    pub store_name: Option<String>,

    /// Categorical `obs` columns defining the levels of the hierarchy, from outermost to innermost
    /// (e.g., `["cell_type", "sample"]`). Each leaf is one combination of their categories, sized by its number of observations.
    pub groupby: Vec<String>,
    /// The `groupby` column whose categories determine the fill color. Defaults to the outermost column.
    pub color_by: Option<String>,
    /// Defaults to Tableau10.
    pub cmap: Option<CategoricalColormap>,

    /// Defaults to `Binary`.
    pub tile: Option<TreemapTile>,
    /// The gap, in pixels, between siblings and between a parent and its children. Defaults to 1.
    pub padding: Option<f32>,
}

impl Default for AdataZarrTreemapLayerParams {
    fn default() -> Self {
        Self {
            layer_id: "".to_string(),
            bounds: None,
            store_name: None,
            groupby: vec![],
            color_by: None,
            cmap: None,
            tile: None,
            padding: None,
        }
    }
}

pub struct AdataZarrTreemapLayer {
    view_params: ViewParams,
    layer_params: AdataZarrTreemapLayerParams,
    store: Arc<dyn AsyncReadableStorageTraits>,
    store_name: String,

    inner: Option<PrecomputedTreemapLayer>,
}

impl AdataZarrTreemapLayer {
    pub fn new(view_params: ViewParams, layer_params: AdataZarrTreemapLayerParams) -> Self {
        let store_name = resolve_store_name(&layer_params.store_name, &view_params);

        let store = view_params.get_store(&store_name);
        Self {
            view_params,
            layer_params,
            store,
            store_name,
            inner: None,
        }
    }

    fn color_by_level(&self) -> usize {
        let Some(color_by) = &self.layer_params.color_by else {
            return 0;
        };
        self.layer_params.groupby.iter().position(|column| column == color_by).unwrap_or_else(|| {
            log(&format!("AdataZarrTreemapLayer: color_by \"{color_by}\" is not one of the groupby columns; coloring by the outermost column"));
            0
        })
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for AdataZarrTreemapLayer {
    async fn prepare(&mut self, gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        let counts_future = load_obs_group_counts(
            gpu_context,
            self.store.clone(),
            &self.store_name,
            &self.layer_params.groupby,
            self.view_params.cache_enabled,
        );
        let (categories_per_column, group_counts) = match maybe_timeout!(counts_future, self.view_params.timeout).await {
            Ok(Ok(loaded)) => loaded,
            Ok(Err(e)) => {
                if is_timed_out_zarrs_error(&e) {
                    return PrepareResult { bailed_early: true };
                }
                panic!("Zarrs error during AdataZarrTreemapLayer prepare: {:?}", e);
            }
            Err(_) => return PrepareResult { bailed_early: true }, // Wall-clock timeout from maybe_timeout!
        };

        let paths: Vec<Vec<String>> = group_counts
            .iter()
            .map(|group| {
                group.codes.iter().zip(&categories_per_column)
                    .map(|(&code, categories)| categories.get(code as usize).cloned().unwrap_or_default())
                    .collect()
            })
            .collect();
        let values: Vec<f32> = group_counts.iter().map(|group| group.count as f32).collect();
        let color_by_level = self.color_by_level();
        let color_codes: Vec<u32> = group_counts.iter().map(|group| group.codes.get(color_by_level).copied().unwrap_or(0)).collect();

        let mut treemap_layer = PrecomputedTreemapLayer::new(
            self.view_params.clone(),
            PrecomputedTreemapLayerParams {
                layer_id: format!("{}_precomputed_treemap_sublayer", self.layer_params.layer_id),
                bounds: self.layer_params.bounds.clone(),
                paths: Arc::new(paths),
                values: Arc::new(values),
                tile: self.layer_params.tile,
                padding: self.layer_params.padding,
                fill_color: Some(ColorMode::Categorical(CategoricalParams {
                    codes: NumericData::Uint32(Arc::new(color_codes)),
                    colormap: self.layer_params.cmap.unwrap_or(CategoricalColormap::Tableau10),
                })),
                fill_opacity: Some(OpacityMode::UniformOpacity(FILL_OPACITY)),
                ..Default::default()
            },
        );
        let prepare_result = treemap_layer.prepare(gpu_context).await;
        self.inner = Some(treemap_layer);

        prepare_result
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for AdataZarrTreemapLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut GpuRenderPass<'_>) {
        if let Some(inner) = &self.inner {
            DrawToRasterGpu::draw(inner, gpu_context, pass).await;
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for AdataZarrTreemapLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for AdataZarrTreemapLayer {
    async fn draw(&self, ctx: &mut SvgContext) {
        if let Some(inner) = &self.inner {
            DrawToSvg::draw(inner, ctx).await
        }
    }
}

impl ExtentableLayer for AdataZarrTreemapLayer {}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
pub struct AdataZarrTreemapPickingInfo {
    /// The `obs` columns of the hierarchy, from outermost to innermost.
    pub groupby: Vec<String>,
    /// The picked leaf's category in each `groupby` column.
    pub obs_values: Vec<String>,
    pub count: u32,
}

impl PickableLayer for AdataZarrTreemapLayer {
    fn pick(&self, screen_coord: ScreenCoord, data_coord: Option<DataCoord>) -> Option<LayerPickingResult> {
        let inner_result = self.inner.as_ref()?.pick(screen_coord, data_coord)?;
        let leaf: PrecomputedTreemapPickingInfo = inner_result.into_info()?;
        Some(LayerPickingResult::new(&self.layer_params.layer_id, AdataZarrTreemapPickingInfo {
            groupby: self.layer_params.groupby.clone(),
            obs_values: leaf.path,
            count: leaf.value as u32,
        }))
    }
}
