// FilledPolygonLayer accepts polygon vertices as input, internally converts these to triangles,
// and ultimately renders a TriangulatedLayer as a sub-layer.
// This layer is intended to be used as a sub-layer of PolygonLayer.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use crate::picking::LayerPickingResult;
use crate::positioning::get_point_position;
use crate::numeric_data::NumericData;
use crate::curve_and_polygon_utils::{
    polygon_rings_from_flat, resolve_margins, triangulate_polygon_rings,
};
use crate::picking_geometry::{point_in_polygon, unapply_model_matrix};
use crate::render_traits::{
    ColorMode, DrawToRasterCpu, DrawToRasterGpu, DrawToSvg,
    BrushableLayer, EmphasisCriteria, MarginParams, OpacityMode, PickableLayer, PreparedLayer, UnitsMode, ViewParams,
};
use crate::render_types::{CpuContext, CpuRenderPass, GpuContext, PrepareResult, RenderResult};
use crate::viewport::{DataCoord, ScreenCoord};
use crate::color_mode::{cpu_fill_color, quantitative_domain};
use crate::scalar_mode::cpu_fill_opacity;
use crate::emphasis_mode::{cpu_is_included, resolve_background_scalar, DEFAULT_BACKGROUND_COLOR};
use crate::two::shapes::{TwoColor, TwoElement, TwoGroup, TwoPath};
use crate::two::svg::{update_svg, SvgContext};
use crate::wgpu;

use super::triangulated_layer::{TriangulatedLayer, TriangulatedLayerParams};

/// Layer params struct for [`FilledPolygonLayer`].
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct FilledPolygonLayerParams {
    pub layer_id: String,
    pub bounds: Option<MarginParams>,
    pub data_unit_mode_x: UnitsMode,
    pub data_unit_mode_y: UnitsMode,
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

    /// How to color each polygon. See [`ColorMode`]: modes carrying `NumericData`
    /// (instanced/categorical/quantitative) supply one value per polygon.
    pub fill_color: Option<ColorMode>,
    /// Opacity multiplier for the fill. See [`OpacityMode`]: `UniformOpacity`
    /// shares one value across all polygons, `InstancedOpacity` supplies one per
    /// polygon. Defaults to 1.
    pub fill_opacity: Option<OpacityMode>,

    /// Criteria AND-ed together to determine the selected ("foreground") /
    /// filtered-in ("background") set of polygons. An empty list means every
    /// polygon is included.
    pub selection_criteria: Vec<EmphasisCriteria>,
    pub filtering_criteria: Vec<EmphasisCriteria>,

    /// Fill color used for filter-included, but selection-excluded
    /// ("background") polygons, in place of `fill_color`.
    pub background_fill_color: Option<(u8, u8, u8)>,

    /// Fill opacity used for filter-included, but selection-excluded
    /// ("background") polygons, in place of `fill_opacity`. Only applied when
    /// `enable_background_fill_opacity` is set AND a value is provided here;
    /// otherwise the polygon's normal fill opacity is used unchanged (there is
    /// no universal "de-emphasized" default for this, unlike
    /// `background_fill_color`, which falls back to `DEFAULT_BACKGROUND_COLOR`).
    pub background_fill_opacity: Option<f32>,

    /// When true, "background" polygons have the fill color specified via
    /// `background_fill_color`.
    pub enable_background_fill_color: bool,
    /// When true, "background" polygons have the fill opacity specified via
    /// `background_fill_opacity`.
    pub enable_background_fill_opacity: bool,
}

impl Default for FilledPolygonLayerParams {
    fn default() -> Self {
        Self {
            layer_id: "".to_string(),
            bounds: None,
            data_unit_mode_x: UnitsMode::Data,
            data_unit_mode_y: UnitsMode::Data,
            model_matrix: None,
            polygons: NumericData::Float32(Arc::new(vec![])),
            polygon_offsets: NumericData::Uint32(Arc::new(vec![])),
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

pub struct FilledPolygonLayer {
    view_params: ViewParams,
    layer_params: FilledPolygonLayerParams,
    /// Triangulated fill geometry as a flat interleaved [x, y, …] f32 array.
    fill_vertices: NumericData,
    /// Per-vertex polygon index (into `fill_color`'s per-element arrays),
    /// parallel to `fill_vertices`.
    vertex_color_index: NumericData,
}

impl FilledPolygonLayer {
    pub fn new(view_params: ViewParams, layer_params: FilledPolygonLayerParams) -> Self {
        // TODO: move the triangulation into the prepare() function?
        // TODO: only do the triangulation in the raster drawing case?
        let rings = polygon_rings_from_flat(&layer_params.polygons, &layer_params.polygon_offsets);
        let (verts, vertex_ring_indices) = triangulate_polygon_rings(&rings);
        Self {
            view_params,
            layer_params,
            fill_vertices: NumericData::Float32(Arc::new(verts)),
            vertex_color_index: NumericData::Uint32(Arc::new(vertex_ring_indices)),
        }
    }
}


#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for FilledPolygonLayer {
    async fn prepare(&mut self, _gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        // TODO: run the TriangulatedLayer sub-layer's prepare function here?
        PrepareResult { bailed_early: false }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for FilledPolygonLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
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
impl DrawToRasterCpu for FilledPolygonLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for FilledPolygonLayer {
    async fn draw(&self, ctx: &mut SvgContext) {
        let Self { layer_params, view_params, .. } = self;

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

        // Quantitative normalization domain, computed once for the whole layer.
        let quant_domain = match layer_params.fill_color.as_ref() {
            Some(ColorMode::Quantitative(params)) => quantitative_domain(params),
            _ => [0.0, 1.0],
        };

        let rings = polygon_rings_from_flat(&layer_params.polygons, &layer_params.polygon_offsets);
        let mut svg_elements: Vec<TwoElement> = Vec::with_capacity(rings.len());
        for (poly_index, ring) in rings.iter().enumerate() {
            if ring.len() < 3 {
                continue;
            }
            // Filter-excluded polygons are not rendered at all.
            if !cpu_is_included(&layer_params.filtering_criteria, poly_index) {
                continue;
            }
            // Filter-included but selection-excluded ("background") polygons
            // still render, but de-emphasized with `background_fill_color` in
            // place of their configured fill color.
            let is_selected = cpu_is_included(&layer_params.selection_criteria, poly_index);

            let mut d = String::new();
            for (i, &(x, y)) in ring.iter().enumerate() {
                let (px, py) = to_px(x, y);
                if i == 0 {
                    d.push_str(&format!("M {} {}", px, py));
                } else {
                    d.push_str(&format!(" L {} {}", px, py));
                }
            }
            d.push_str(" Z");
            let fill = TwoColor::Rgb(if is_selected || !layer_params.enable_background_fill_color {
                cpu_fill_color(layer_params.fill_color.as_ref(), poly_index, quant_domain)
            } else {
                layer_params.background_fill_color.unwrap_or(DEFAULT_BACKGROUND_COLOR)
            });
            let fill_opacity = resolve_background_scalar(
                is_selected,
                layer_params.enable_background_fill_opacity,
                layer_params.background_fill_opacity,
                cpu_fill_opacity(layer_params.fill_opacity.as_ref(), poly_index),
            ) as f64;
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

impl BrushableLayer for FilledPolygonLayer {}

impl PickableLayer for FilledPolygonLayer {
    fn pick(&self, _screen_coord: ScreenCoord, data_coord: Option<DataCoord>) -> Option<LayerPickingResult> {
        let DataCoord::TwoD { x: wx, y: wy } = data_coord? else {
            return None;
        };

        // Pixel/normalized-units positioning places polygons relative to the
        // layer bounds rather than in data space, so a data-space
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

        let rings = polygon_rings_from_flat(&self.layer_params.polygons, &self.layer_params.polygon_offsets);

        // Naive containment test: iterate over every polygon ring, keeping
        // the last (topmost, since later polygons draw on top) match.
        let mut hit_idx: Option<usize> = None;
        for (i, ring) in rings.iter().enumerate() {
            // Filter-excluded polygons are ignored in picking.
            if !cpu_is_included(&self.layer_params.filtering_criteria, i) {
                continue;
            }
            if point_in_polygon(cx, cy, ring) {
                hit_idx = Some(i);
            }
        }

        let idx = hit_idx?;
        let mut info = HashMap::new();
        info.insert("index".to_string(), idx.to_string());

        Some(LayerPickingResult {
            layer_id: self.layer_params.layer_id.clone(),
            info,
        })
    }
}
