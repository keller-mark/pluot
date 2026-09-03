// CurveLayer renders SVG-like vector paths as stroked and/or filled curves,
// delegating to StrokedCurveLayer and FilledCurveLayer sublayers.

use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::picking::LayerPickingResult;
use crate::render_traits::{BrushableLayer, ColorMode, DrawToRasterGpu, DrawToRasterCpu, DrawToSvg, EmphasisCriteria, OpacityMode, PickableLayer, PreparedLayer, SizeMode, ViewParams, UnitsMode, MarginParams};
use crate::render_types::{CpuContext, CpuRenderPass, PrepareResult};
use crate::render_types::GpuContext;
use crate::two::svg::SvgContext;
use crate::viewport::{DataCoord, ScreenCoord};
use crate::wgpu;

use crate::layers::stroked_curve_layer::{StrokedCurveLayer, StrokedCurveLayerParams};
use crate::layers::filled_curve_layer::{FilledCurveLayer, FilledCurveLayerParams};

pub use crate::curve_and_polygon_utils::PathCommand;

/// Layer params struct for [`CurveLayer`].
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct CurveLayerParams {
    pub layer_id: String,
    pub bounds: Option<MarginParams>,
    pub data_unit_mode_x: UnitsMode,
    pub data_unit_mode_y: UnitsMode,
    /// Whether `stroke_width` is measured in pixels or in data-coordinate units.
    pub stroke_width_unit_mode: UnitsMode,
    pub model_matrix: Option<[f32; 16]>,
    pub commands: Arc<Vec<PathCommand>>,
    pub subdivisions: u32,
    pub stroked: bool,
    pub filled: bool,
    /// How to color the stroke. See [`ColorMode`]. `CurveLayer` renders a single
    /// shape, so modes carrying `NumericData` are expected to supply a single
    /// (length-1) value.
    pub stroke_color: Option<ColorMode>,
    /// Stroke width. See [`SizeMode`]: `UniformSize` and `InstancedSize` (a
    /// single, length-1 value for this single-shape layer) are both accepted.
    /// Interpreted in the units given by `stroke_width_unit_mode`. Defaults to 1.
    pub stroke_width: Option<SizeMode>,
    /// How to color the fill. See [`ColorMode`]. Same single-shape caveat as
    /// `stroke_color`.
    pub fill_color: Option<ColorMode>,
    /// Opacity multiplier for the stroke. See [`OpacityMode`]. Defaults to 1.
    pub stroke_opacity: Option<OpacityMode>,
    /// Opacity multiplier for the fill. See [`OpacityMode`]. Defaults to 1.
    pub fill_opacity: Option<OpacityMode>,

    /// Criteria AND-ed together to determine whether the single shape is
    /// selected ("foreground") / filtered-in ("background"). `CurveLayer`
    /// renders a single shape, so modes carrying `NumericData` are expected to
    /// supply a single (length-1) value. An empty list means the shape is
    /// included. Forwarded to both the stroke and fill sub-layers.
    pub selection_criteria: Vec<EmphasisCriteria>,
    pub filtering_criteria: Vec<EmphasisCriteria>,

    /// Stroke/fill colors used when the shape is filter-included, but
    /// selection-excluded ("background"), in place of `stroke_color` /
    /// `fill_color`.
    pub background_stroke_color: Option<(u8, u8, u8)>,
    pub background_fill_color: Option<(u8, u8, u8)>,

    /// Stroke/fill opacity and stroke width used when the shape is
    /// filter-included, but selection-excluded ("background"), in place of
    /// `stroke_opacity`/`fill_opacity`/`stroke_width`. Only applied when the
    /// corresponding `enable_background_*` flag is set AND a value is
    /// provided here; otherwise the shape's normal value is used unchanged
    /// (there is no universal "de-emphasized" default for these, unlike
    /// `background_stroke_color`/`background_fill_color`, which fall back to
    /// `DEFAULT_BACKGROUND_COLOR`).
    pub background_stroke_opacity: Option<f32>,
    pub background_fill_opacity: Option<f32>,
    pub background_stroke_width: Option<f32>,

    /// When true, the shape has the stroke/fill color specified via
    /// `background_stroke_color`/`background_fill_color` when
    /// selection-excluded.
    pub enable_background_stroke_color: bool,
    pub enable_background_fill_color: bool,
    /// When true, the shape has the stroke/fill opacity specified via
    /// `background_stroke_opacity`/`background_fill_opacity` when
    /// selection-excluded.
    pub enable_background_stroke_opacity: bool,
    pub enable_background_fill_opacity: bool,
    /// When true, the shape has the stroke width specified via
    /// `background_stroke_width` when selection-excluded. Only affects the
    /// stroke's width, not whether it is drawn at all (that is `stroked`).
    pub enable_background_stroke_width: bool,
}

impl Default for CurveLayerParams {
    fn default() -> Self {
        Self {
            layer_id: "".to_string(),
            bounds: None,
            data_unit_mode_x: UnitsMode::Data,
            data_unit_mode_y: UnitsMode::Data,
            stroke_width_unit_mode: UnitsMode::Pixels,
            model_matrix: None,
            commands: Arc::new(vec![]),
            subdivisions: 32,
            stroked: true,
            filled: false,
            stroke_color: None,
            stroke_width: Some(SizeMode::UniformSize(1.0)),
            fill_color: None,
            stroke_opacity: Some(OpacityMode::UniformOpacity(1.0)),
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

pub struct CurveLayer {
    layer_params: CurveLayerParams,
    stroke_sublayer: Option<StrokedCurveLayer>,
    fill_sublayer: Option<FilledCurveLayer>,
}

impl CurveLayer {
    pub fn new(view_params: ViewParams, layer_params: CurveLayerParams) -> Self {
        if layer_params.stroke_width_unit_mode == UnitsMode::Data
            && (layer_params.data_unit_mode_x != UnitsMode::Data
                || layer_params.data_unit_mode_y != UnitsMode::Data)
        {
            panic!("stroke_width_unit_mode cannot be 'data' when data_unit_mode is 'pixels' or 'normalized'");
        }

        let stroke_sublayer = if layer_params.stroked {
            Some(StrokedCurveLayer::new(view_params.clone(), StrokedCurveLayerParams {
                layer_id: format!("{}_stroked", layer_params.layer_id),
                bounds: layer_params.bounds.clone(),
                data_unit_mode_x: layer_params.data_unit_mode_x.clone(),
                data_unit_mode_y: layer_params.data_unit_mode_y.clone(),
                stroke_width: layer_params.stroke_width.clone(),
                stroke_width_unit_mode: layer_params.stroke_width_unit_mode.clone(),
                model_matrix: layer_params.model_matrix,
                commands: Arc::clone(&layer_params.commands),
                subdivisions: layer_params.subdivisions,
                stroke_color: layer_params.stroke_color.clone(),
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
            Some(FilledCurveLayer::new(view_params.clone(), FilledCurveLayerParams {
                layer_id: format!("{}_filled", layer_params.layer_id),
                bounds: layer_params.bounds.clone(),
                data_unit_mode_x: layer_params.data_unit_mode_x.clone(),
                data_unit_mode_y: layer_params.data_unit_mode_y.clone(),
                model_matrix: layer_params.model_matrix,
                commands: Arc::clone(&layer_params.commands),
                subdivisions: layer_params.subdivisions,
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

        Self { layer_params, stroke_sublayer, fill_sublayer }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for CurveLayer {
    async fn prepare(&mut self, _gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        // TODO: run the sub-layers' prepare() functions here
        PrepareResult { bailed_early: false }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for CurveLayer {
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
impl DrawToRasterCpu for CurveLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for CurveLayer {
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
        layer_type_name: "CurveLayer",
        create_layer: |value, view_params| {
            let params: CurveLayerParams = serde_json::from_value(value).unwrap();
            Box::new(CurveLayer::new(view_params.clone(), params))
        },
    }
}

impl BrushableLayer for CurveLayer {}

impl PickableLayer for CurveLayer {
    // Delegate to the sub-layers, which own the actual curve geometry. The
    // fill (an area) takes priority over the stroke (a thin outline band,
    // always "hit" by the sub-layer's nearest-segment search), mirroring
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
