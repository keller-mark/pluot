// FilledCurveLayer accepts path commands for one or more curves as input, internally converts them to triangles,
// and ultimately renders a TriangulatedLayer as a sub-layer.
// This layer is intended to be used as a sub-layer of CurveLayer.

use kurbo::{CubicBez, ParamCurve};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use std::sync::Arc;

use crate::picking::LayerPickingResult;
use crate::positioning::get_point_position;
use crate::numeric_data::NumericData;
use crate::render_traits::{
    ColorMode, DrawToRasterCpu, DrawToRasterGpu, DrawToSvg,
    ExtentableLayer, EmphasisCriteria, MarginParams, OpacityMode, PickableLayer, PreparedLayer, UnitsMode, ViewParams,
};
use crate::render_types::{CpuContext, CpuRenderPass, GpuContext, PrepareResult, RenderResult};
use crate::gpu_send::GpuRenderPass;
use crate::color_mode::{cpu_fill_color, quantitative_domain};
use crate::scalar_mode::cpu_fill_opacity;
use crate::emphasis_mode::{cpu_is_included, resolve_background_scalar, DEFAULT_BACKGROUND_COLOR};
use crate::two::shapes::{TwoColor, TwoElement, TwoGroup, TwoPath};
use crate::two::svg::{update_svg, SvgContext};
use crate::viewport::{DataCoord, ScreenCoord};
use crate::wgpu;

use crate::curve_and_polygon_utils::{
    compute_fill_vertices, curves_to_subpaths, flatten_subpath, resolve_margins, split_curves, PathCommand,
};
use crate::picking_geometry::{point_in_polygon, unapply_model_matrix};
use super::triangulated_layer::{TriangulatedLayer, TriangulatedLayerParams};

/// Layer params struct for [`FilledCurveLayer`].
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct FilledCurveLayerParams {
    pub layer_id: String,
    pub bounds: Option<MarginParams>,
    pub data_unit_mode_x: UnitsMode,
    pub data_unit_mode_y: UnitsMode,
    pub model_matrix: Option<[f32; 16]>,
    pub commands: Arc<Vec<PathCommand>>,
    /// See `CurveLayerParams::curve_offsets`. `None` means a single curve.
    pub curve_offsets: Option<Arc<Vec<u32>>>,
    pub subdivisions: u32,
    /// How to color the fill. See [`ColorMode`]. Modes carrying `NumericData`
    /// supply one value per curve.
    pub fill_color: Option<ColorMode>,
    /// Opacity multiplier for the fill. See [`OpacityMode`]. Instanced modes
    /// supply one value per curve. Defaults to 1.
    pub fill_opacity: Option<OpacityMode>,

    /// Criteria AND-ed together to determine whether each curve is selected
    /// ("foreground") / filtered-in ("background"). Modes carrying
    /// `NumericData` supply one value per curve. An empty list means every
    /// curve is included.
    pub selection_criteria: Vec<EmphasisCriteria>,
    pub filtering_criteria: Vec<EmphasisCriteria>,

    /// Fill color used when the shape is filter-included, but
    /// selection-excluded ("background"), in place of `fill_color`.
    pub background_fill_color: Option<(u8, u8, u8)>,

    /// Fill opacity used when the shape is filter-included, but
    /// selection-excluded ("background"), in place of `fill_opacity`. Only
    /// applied when `enable_background_fill_opacity` is set AND a value is
    /// provided here; otherwise the shape's normal fill opacity is used
    /// unchanged (there is no universal "de-emphasized" default for this,
    /// unlike `background_fill_color`, which falls back to
    /// `DEFAULT_BACKGROUND_COLOR`).
    pub background_fill_opacity: Option<f32>,

    /// When true, the shape has the fill color specified via
    /// `background_fill_color` when selection-excluded.
    pub enable_background_fill_color: bool,
    /// When true, the shape has the fill opacity specified via
    /// `background_fill_opacity` when selection-excluded.
    pub enable_background_fill_opacity: bool,
}

impl Default for FilledCurveLayerParams {
    fn default() -> Self {
        Self {
            layer_id: "".to_string(),
            bounds: None,
            data_unit_mode_x: UnitsMode::Data,
            data_unit_mode_y: UnitsMode::Data,
            model_matrix: None,
            commands: Arc::new(vec![]),
            curve_offsets: None,
            subdivisions: 32,
            fill_color: None,
            fill_opacity: Some(OpacityMode::UniformOpacity(1.0)),
            selection_criteria: vec![],
            filtering_criteria: vec![],
            background_fill_color: None,
            background_fill_opacity: None,
            enable_background_fill_color: true,
            enable_background_fill_opacity: false,
        }
    }
}

pub struct FilledCurveLayer {
    view_params: ViewParams,
    layer_params: FilledCurveLayerParams,
    subpaths: Vec<Vec<CubicBez>>,
    /// The index of the curve each of `subpaths` belongs to.
    subpath_curve_indices: Vec<u32>,
    /// Triangulated fill geometry as a flat interleaved [x, y, …] f32 array.
    fill_vertices: NumericData,
    /// Per-vertex curve index, parallel to `fill_vertices`.
    vertex_color_index: NumericData,
}

impl FilledCurveLayer {
    pub fn new(view_params: ViewParams, layer_params: FilledCurveLayerParams) -> Self {
        // TODO: move the triangulation into the prepare() function?
        // TODO: only do the triangulation in the raster drawing case?
        let curve_offsets = layer_params.curve_offsets.as_deref().map(Vec::as_slice);
        let num_curves = split_curves(&layer_params.commands, curve_offsets).len();
        if let Some(fill_color) = &layer_params.fill_color {
            fill_color.validate_len(num_curves);
        }
        if let Some(fill_opacity) = &layer_params.fill_opacity {
            fill_opacity.validate_len(num_curves);
        }
        for criteria in layer_params.selection_criteria.iter().chain(&layer_params.filtering_criteria) {
            criteria.validate_len(num_curves);
        }

        let (subpaths, subpath_curve_indices) = curves_to_subpaths(&layer_params.commands, curve_offsets);
        let subdivisions = layer_params.subdivisions.max(1);
        let (fill_vertices_raw, vertex_curve_indices) = compute_fill_vertices(&subpaths, &subpath_curve_indices, subdivisions);
        let fill_vertices = NumericData::Float32(Arc::new(fill_vertices_raw));
        let vertex_color_index = NumericData::Uint32(Arc::new(vertex_curve_indices));
        Self { view_params, layer_params, subpaths, subpath_curve_indices, fill_vertices, vertex_color_index }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for FilledCurveLayer {
    async fn prepare(&mut self, _gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        // TODO: run the TriangulatedLayer sub-layer's prepare function here?
        PrepareResult { bailed_early: false }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for FilledCurveLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut GpuRenderPass<'_>) {
        if self.fill_vertices.is_empty() {
            return;
        }
        let triangulated = TriangulatedLayer::new(
            self.view_params.clone(),
            TriangulatedLayerParams {
                layer_id: self.layer_params.layer_id.clone(),
                bounds: self.layer_params.bounds.clone(),
                data_unit_mode_x: self.layer_params.data_unit_mode_x,
                data_unit_mode_y: self.layer_params.data_unit_mode_y,
                model_matrix: self.layer_params.model_matrix,
                vertices: self.fill_vertices.clone(),
                vertex_color_index: self.vertex_color_index.clone(),
                fill_color: self.layer_params.fill_color.clone(),
                fill_opacity: self.layer_params.fill_opacity.clone(),
                selection_criteria: self.layer_params.selection_criteria.clone(),
                filtering_criteria: self.layer_params.filtering_criteria.clone(),
                background_fill_color: self.layer_params.background_fill_color,
                background_fill_opacity: self.layer_params.background_fill_opacity,
                enable_background_fill_color: self.layer_params.enable_background_fill_color,
                enable_background_fill_opacity: self.layer_params.enable_background_fill_opacity,
            },
        );
        DrawToRasterGpu::draw(&triangulated, gpu_context, pass).await;
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for FilledCurveLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for FilledCurveLayer {
    async fn draw(&self, ctx: &mut SvgContext) {
        let Self { layer_params, view_params, subpaths, subpath_curve_indices, .. } = self;

        let camera_view = view_params.camera_view.unwrap_or([
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ]);

        let (margin_left, margin_top, margin_right, margin_bottom) =
            resolve_margins(&layer_params.bounds, &view_params.margins);

        let viewport_w = view_params.width as f32;
        let viewport_h = view_params.height as f32;
        let layer_w = viewport_w - (margin_left + margin_right) as f32;
        let layer_h = viewport_h - (margin_top + margin_bottom) as f32;

        let subdivisions = layer_params.subdivisions.max(1) as f64;

        let to_px = |x: f32, y: f32| -> (f64, f64) {
            let (px, py) = get_point_position(
                x, y,
                layer_w, layer_h,
                &camera_view,
                layer_params.data_unit_mode_x,
                layer_params.data_unit_mode_y,
                view_params.aspect_ratio_mode,
                view_params.aspect_ratio_alignment_mode,
                layer_params.model_matrix.as_ref().map(|m| m.as_slice()),
            );
            (px as f64, (layer_h - py) as f64)
        };

        let quant_domain = match layer_params.fill_color.as_ref() {
            Some(ColorMode::Quantitative(params)) => quantitative_domain(params),
            _ => [0.0, 1.0],
        };

        let mut svg_elements: Vec<TwoElement> = Vec::with_capacity(subpaths.len());
        for (subpath, &curve_index) in subpaths.iter().zip(subpath_curve_indices) {
            let curve_index = curve_index as usize;
            // Filter-excluded curves are not rendered at all.
            if !cpu_is_included(&layer_params.filtering_criteria, curve_index) || subpath.is_empty() {
                continue;
            }
            // Filter-included but selection-excluded ("background") curves still
            // render, but de-emphasized with `background_fill_color` in place of
            // the configured fill color.
            let is_selected = cpu_is_included(&layer_params.selection_criteria, curve_index);
            let fill = TwoColor::Rgb(if is_selected || !layer_params.enable_background_fill_color {
                cpu_fill_color(layer_params.fill_color.as_ref(), curve_index, quant_domain)
            } else {
                layer_params.background_fill_color.unwrap_or(DEFAULT_BACKGROUND_COLOR)
            });
            let fill_opacity = resolve_background_scalar(
                is_selected,
                layer_params.enable_background_fill_opacity,
                layer_params.background_fill_opacity,
                cpu_fill_opacity(layer_params.fill_opacity.as_ref(), curve_index),
            ) as f64;

            let mut d = String::new();
            let first = subpath[0].p0;
            let (fx, fy) = to_px(first.x as f32, first.y as f32);
            d.push_str(&format!("M {} {}", fx, fy));
            for seg in subpath {
                for step in 1..=(subdivisions as u32) {
                    let t = step as f64 / subdivisions;
                    let p = seg.eval(t);
                    let (px, py) = to_px(p.x as f32, p.y as f32);
                    d.push_str(&format!(" L {} {}", px, py));
                }
            }
            svg_elements.push(TwoElement::Path(TwoPath {
                d,
                stroke: None,
                fill: Some(fill),
                linewidth: 0.0,
                opacity: 1.0,
                fill_opacity,
                stroke_opacity: 1.0,
                stroke_linejoin: None,
                stroke_linecap: None,
            }));
        }

        let svg_elements = vec![TwoElement::Group(TwoGroup {
            elements: svg_elements,
            translate: Some((margin_left, margin_top)),
            layer_id: Some(layer_params.layer_id.clone()),
            clip_rect: Some((0.0, 0.0, layer_w as f64, layer_h as f64)),
            ..Default::default()
        })];

        update_svg(ctx, &svg_elements);
    }
}


impl ExtentableLayer for FilledCurveLayer {}

#[derive(Serialize, Debug, Clone, PartialEq, TS)]
pub struct FilledCurvePickingInfo {
    /// Index into `curve_offsets` (always 0 when it is `None`).
    pub curve_index: usize,
    /// Index of the sub-path among those of every curve.
    pub subpath_index: usize,
}

impl PickableLayer for FilledCurveLayer {
    fn pick(&self, _screen_coord: ScreenCoord, data_coord: Option<DataCoord>) -> Option<LayerPickingResult> {
        let DataCoord::TwoD { x: wx, y: wy } = data_coord? else {
            return None;
        };

        // Pixel/normalized-units positioning places the curve relative to
        // the layer bounds rather than in data space, so a data-space
        // containment test does not apply.
        if self.layer_params.data_unit_mode_x != UnitsMode::Data
            || self.layer_params.data_unit_mode_y != UnitsMode::Data
        {
            return None;
        }

        // Map the world coordinate into model space by inverting the
        // model_matrix; the vertex shader computes
        // world = model_matrix * vec4(position, 0, 1).
        let (cx, cy) = unapply_model_matrix(self.layer_params.model_matrix, wx, wy)?;

        // Naive containment test: iterate over every sub-path ring, keeping
        // the last (topmost, since later sub-paths draw on top) match.
        let subdivisions = self.layer_params.subdivisions.max(1);
        let mut hit_idx: Option<usize> = None;
        for (i, (subpath, &curve_index)) in self.subpaths.iter().zip(&self.subpath_curve_indices).enumerate() {
            // Filter-excluded curves are ignored in picking.
            if !cpu_is_included(&self.layer_params.filtering_criteria, curve_index as usize) {
                continue;
            }
            let ring = flatten_subpath(subpath, subdivisions);
            if point_in_polygon(cx, cy, &ring) {
                hit_idx = Some(i);
            }
        }

        let idx = hit_idx?;
        Some(LayerPickingResult::new(&self.layer_params.layer_id, FilledCurvePickingInfo {
            curve_index: self.subpath_curve_indices[idx] as usize,
            subpath_index: idx,
        }))
    }
}
