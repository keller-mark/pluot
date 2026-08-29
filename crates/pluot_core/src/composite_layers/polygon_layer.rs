// PolygonLayer renders a collection of polygons as stroked outlines, filled
// interiors, or both, by delegating to StrokedPolygonLayer and FilledPolygonLayer.

use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::picking::LayerPickingResult;
use crate::render_traits::{
    ColorMode, DrawToRasterCpu, DrawToRasterGpu, DrawToSvg,
    EmphasisCriteria, MarginParams, OpacityMode, PickableLayer, PreparedLayer, SizeMode, UnitsMode, ViewParams,
};
use crate::render_types::{CpuContext, CpuRenderPass, GpuContext, PrepareResult};
use crate::numeric_data::NumericData;
use crate::two::svg::SvgContext;
use crate::viewport::{DataCoord, ScreenCoord};
use crate::wgpu;

use crate::layers::stroked_polygon_layer::{StrokedPolygonLayer, StrokedPolygonLayerParams};
use crate::layers::filled_polygon_layer::{FilledPolygonLayer, FilledPolygonLayerParams};


/// Layer params struct for [`PolygonLayer`].
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct PolygonLayerParams {
    pub layer_id: String,
    /// If `None`, the view-level margins are used.
    pub bounds: Option<MarginParams>,
    pub data_unit_mode_x: UnitsMode,
    pub data_unit_mode_y: UnitsMode,
    /// Whether `stroke_width` is measured in pixels or in data-coordinate units.
    /// Defaults to pixels.
    pub stroke_width_unit_mode: UnitsMode,
    pub model_matrix: Option<[f32; 16]>,

    /// All polygon vertices as a flat, interleaved 1D array of model-space
    /// coordinates `[x0, y0, x1, y1, …]`, with each polygon's ring concatenated
    /// after the previous one. Any supported numeric dtype is accepted.
    pub polygons: NumericData,
    /// Arrow-style vertex offsets with `num_polygons + 1` entries: polygon `p`
    /// occupies vertex indices `polygon_offsets[p]..polygon_offsets[p + 1]`.
    /// Any supported numeric dtype is accepted. Rings with fewer than 3 vertices
    /// are silently skipped.
    pub polygon_offsets: NumericData,

    /// Whether to stroke the polygon outlines. Defaults to `true`.
    pub stroked: bool,
    /// Whether to fill the polygon interiors. Defaults to `false`.
    pub filled: bool,

    /// How to color each polygon's outline. See [`ColorMode`]: modes carrying
    /// `NumericData` (instanced/categorical/quantitative) supply one value per
    /// polygon.
    pub stroke_color: Option<ColorMode>,
    /// Stroke width. See [`SizeMode`]: `UniformSize` shares one width across all
    /// polygons, `InstancedSize` supplies one per polygon. Interpreted in the
    /// units given by `stroke_width_unit_mode`. Defaults to 1.
    pub stroke_width: Option<SizeMode>,
    /// Opacity multiplier for the stroke. See [`OpacityMode`]: `UniformOpacity`
    /// shares one value across all polygons, `InstancedOpacity` supplies one per
    /// polygon. Defaults to 1.
    pub stroke_opacity: Option<OpacityMode>,

    /// How to color each polygon's interior. See [`ColorMode`]: modes carrying
    /// `NumericData` (instanced/categorical/quantitative) supply one value per
    /// polygon.
    pub fill_color: Option<ColorMode>,
    /// Opacity multiplier for the fill. See [`OpacityMode`]: `UniformOpacity`
    /// shares one value across all polygons, `InstancedOpacity` supplies one per
    /// polygon. Defaults to 1.
    pub fill_opacity: Option<OpacityMode>,

    /// Criteria AND-ed together to determine the selected ("foreground") /
    /// filtered-in ("background") set of polygons. An empty list means every
    /// polygon is included. Forwarded to both the stroke and fill sub-layers.
    pub selection_criteria: Vec<EmphasisCriteria>,
    pub filtering_criteria: Vec<EmphasisCriteria>,

    /// Stroke/fill colors used for filter-included, but selection-excluded
    /// ("background") polygons, in place of `stroke_color` / `fill_color`.
    pub background_stroke_color: Option<(u8, u8, u8)>,
    pub background_fill_color: Option<(u8, u8, u8)>,

    /// Stroke/fill opacity and stroke width used for filter-included, but
    /// selection-excluded ("background") polygons, in place of
    /// `stroke_opacity`/`fill_opacity`/`stroke_width`. Only applied when the
    /// corresponding `enable_background_*` flag is set AND a value is
    /// provided here; otherwise the polygon's normal value is used unchanged
    /// (there is no universal "de-emphasized" default for these, unlike
    /// `background_stroke_color`/`background_fill_color`, which fall back to
    /// `DEFAULT_BACKGROUND_COLOR`).
    pub background_stroke_opacity: Option<f32>,
    pub background_fill_opacity: Option<f32>,
    pub background_stroke_width: Option<f32>,

    /// When true, "background" polygons have the stroke/fill color specified
    /// via `background_stroke_color`/`background_fill_color`.
    pub enable_background_stroke_color: bool,
    pub enable_background_fill_color: bool,
    /// When true, "background" polygons have the stroke/fill opacity
    /// specified via `background_stroke_opacity`/`background_fill_opacity`.
    pub enable_background_stroke_opacity: bool,
    pub enable_background_fill_opacity: bool,
    /// When true, "background" polygons have the stroke width specified via
    /// `background_stroke_width`. Only affects the stroke's width, not
    /// whether it is drawn at all (that is `stroked`).
    pub enable_background_stroke_width: bool,
}

impl Default for PolygonLayerParams {
    fn default() -> Self {
        Self {
            layer_id: "".to_string(),
            bounds: None,
            data_unit_mode_x: UnitsMode::Data,
            data_unit_mode_y: UnitsMode::Data,
            stroke_width_unit_mode: UnitsMode::Pixels,
            model_matrix: None,
            polygons: NumericData::Float32(Arc::new(vec![])),
            polygon_offsets: NumericData::Uint32(Arc::new(vec![])),
            stroked: true,
            filled: false,
            stroke_color: None,
            stroke_width: Some(SizeMode::UniformSize(1.0)),
            stroke_opacity: Some(OpacityMode::UniformOpacity(1.0)),
            fill_color: None,
            fill_opacity: Some(OpacityMode::UniformOpacity(1.0)),
            selection_criteria: vec![],
            filtering_criteria: vec![],
            background_stroke_color: None,
            background_fill_color: None,
            background_stroke_opacity: None,
            background_fill_opacity: None,
            background_stroke_width: None,
            enable_background_stroke_color: true,
            enable_background_fill_color: true,
            enable_background_stroke_opacity: false,
            enable_background_fill_opacity: false,
            enable_background_stroke_width: false,
        }
    }
}

pub struct PolygonLayer {
    stroke_sublayer: Option<StrokedPolygonLayer>,
    fill_sublayer: Option<FilledPolygonLayer>,
}

impl PolygonLayer {
    pub fn new(view_params: ViewParams, layer_params: PolygonLayerParams) -> Self {
        // The flat interleaved coordinate array + vertex offsets are passed
        // straight through to the sub-layers, sharing the underlying buffers
        // (cloning a `NumericData` only bumps its inner `Arc`).
        let stroke_sublayer = if layer_params.stroked {
            Some(StrokedPolygonLayer::new(view_params.clone(), StrokedPolygonLayerParams {
                layer_id: format!("{}_stroked", layer_params.layer_id),
                bounds: layer_params.bounds.clone(),
                data_unit_mode_x: layer_params.data_unit_mode_x.clone(),
                data_unit_mode_y: layer_params.data_unit_mode_y.clone(),
                stroke_width_unit_mode: layer_params.stroke_width_unit_mode.clone(),
                model_matrix: layer_params.model_matrix,
                polygons: layer_params.polygons.clone(),
                polygon_offsets: layer_params.polygon_offsets.clone(),
                stroke_color: layer_params.stroke_color.clone(),
                stroke_width: layer_params.stroke_width.clone(),
                stroke_opacity: layer_params.stroke_opacity.clone(),
                selection_criteria: layer_params.selection_criteria.clone(),
                filtering_criteria: layer_params.filtering_criteria.clone(),
                background_stroke_color: layer_params.background_stroke_color,
                background_stroke_opacity: layer_params.background_stroke_opacity,
                background_stroke_width: layer_params.background_stroke_width,
                enable_background_stroke_color: layer_params.enable_background_stroke_color,
                enable_background_stroke_opacity: layer_params.enable_background_stroke_opacity,
                enable_background_stroke_width: layer_params.enable_background_stroke_width,
            }))
        } else {
            None
        };

        let fill_sublayer = if layer_params.filled {
            Some(FilledPolygonLayer::new(view_params.clone(), FilledPolygonLayerParams {
                layer_id: format!("{}_filled", layer_params.layer_id),
                bounds: layer_params.bounds.clone(),
                data_unit_mode_x: layer_params.data_unit_mode_x.clone(),
                data_unit_mode_y: layer_params.data_unit_mode_y.clone(),
                model_matrix: layer_params.model_matrix,
                polygons: layer_params.polygons.clone(),
                polygon_offsets: layer_params.polygon_offsets.clone(),
                fill_color: layer_params.fill_color.clone(),
                fill_opacity: layer_params.fill_opacity.clone(),
                selection_criteria: layer_params.selection_criteria.clone(),
                filtering_criteria: layer_params.filtering_criteria.clone(),
                background_fill_color: layer_params.background_fill_color,
                background_fill_opacity: layer_params.background_fill_opacity,
                enable_background_fill_color: layer_params.enable_background_fill_color,
                enable_background_fill_opacity: layer_params.enable_background_fill_opacity,
            }))
        } else {
            None
        };

        Self { stroke_sublayer, fill_sublayer }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for PolygonLayer {
    async fn prepare(&mut self, _gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        // TODO: run the sub-layers' prepare() functions here
        PrepareResult { bailed_early: false }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for PolygonLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        // Fill first so stroke renders on top.
        if let Some(fill) = &self.fill_sublayer {
            DrawToRasterGpu::draw(fill, gpu_context, pass).await;
        }
        if let Some(stroke) = &self.stroke_sublayer {
            DrawToRasterGpu::draw(stroke, gpu_context, pass).await;
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for PolygonLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for PolygonLayer {
    async fn draw(&self, ctx: &mut SvgContext) {
        // Fill first so stroke renders on top.
        if let Some(fill) = &self.fill_sublayer {
            DrawToSvg::draw(fill, ctx).await;
        }
        if let Some(stroke) = &self.stroke_sublayer {
            DrawToSvg::draw(stroke, ctx).await;
        }
    }
}

inventory::submit! {
    crate::registry::LayerRegistration {
        layer_type_name: "PolygonLayer",
        create_layer: |value, view_params| {
            let params: PolygonLayerParams = serde_json::from_value(value).unwrap();
            Box::new(PolygonLayer::new(view_params.clone(), params))
        },
    }
}

impl PickableLayer for PolygonLayer {
    // Delegate to the sub-layers, which own the actual polygon geometry.
    // The fill (an area) takes priority over the stroke (a thin outline
    // band, always "hit" by the sub-layer's nearest-edge search), mirroring
    // fill-then-stroke draw order.
    fn pick(&self, screen_coord: ScreenCoord, data_coord: Option<DataCoord>) -> Option<LayerPickingResult> {
        if let Some(fill) = &self.fill_sublayer {
            if let Some(result) = fill.pick(screen_coord, data_coord) {
                return Some(result);
            }
        }
        if let Some(stroke) = &self.stroke_sublayer {
            if let Some(result) = stroke.pick(screen_coord, data_coord) {
                return Some(result);
            }
        }
        None
    }
}
