// FilledCurveLayer accepts path commands as input, internally converts the path to triangles,
// and ultimately renders a TriangulatedLayer as a sub-layer.
// This layer is intended to be used as a sub-layer of CurveLayer.

use kurbo::{CubicBez, ParamCurve};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::positioning::get_point_position;
use crate::numeric_data::NumericData;
use crate::render_traits::{
    ColorMode, DrawToRasterCpu, DrawToRasterGpu, DrawToSvg,
    MarginParams, PickableLayer, PreparedLayer, UnitsMode, ViewParams,
};
use crate::render_types::{CpuContext, CpuRenderPass, GpuContext, PrepareResult, RenderResult};
use crate::color_mode::{cpu_fill_color, quantitative_domain};
use crate::two::shapes::{TwoColor, TwoElement, TwoGroup, TwoPath};
use crate::two::svg::{update_svg, SvgContext};
use crate::wgpu;

use super::curve_and_polygon_utils::{
    commands_to_subpaths, compute_fill_vertices, resolve_margins, PathCommand,
};
use super::triangulated_layer::{TriangulatedLayer, TriangulatedLayerParams};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct FilledCurveLayerParams {
    pub layer_id: String,
    pub bounds: Option<MarginParams>,
    pub data_unit_mode_x: UnitsMode,
    pub data_unit_mode_y: UnitsMode,
    pub model_matrix: Option<[f32; 16]>,
    pub commands: Arc<Vec<PathCommand>>,
    pub subdivisions: u32,
    /// How to color the fill. See [`ColorMode`]. `FilledCurveLayer` renders a
    /// single shape, so modes carrying `NumericData` are expected to supply a
    /// single (length-1) value.
    pub fill_color: Option<ColorMode>,
    /// Opacity multiplier for the fill. Defaults to 1.
    pub fill_opacity: f32,
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
            subdivisions: 32,
            fill_color: None,
            fill_opacity: 1.0,
        }
    }
}

pub struct FilledCurveLayer {
    view_params: ViewParams,
    layer_params: FilledCurveLayerParams,
    subpaths: Vec<Vec<CubicBez>>,
    /// Triangulated fill geometry as a flat interleaved [x, y, …] f32 array.
    fill_vertices: NumericData,
    /// Per-vertex color index (all zero: a single shape uses one color),
    /// parallel to `fill_vertices`.
    vertex_color_index: NumericData,
}

impl FilledCurveLayer {
    pub fn new(view_params: ViewParams, layer_params: FilledCurveLayerParams) -> Self {
        // TODO: move the triangulation into the prepare() function?
        // TODO: only do the triangulation in the raster drawing case?
        let subpaths = commands_to_subpaths(&layer_params.commands);
        let subdivisions = layer_params.subdivisions.max(1);
        let fill_vertices_raw = compute_fill_vertices(&subpaths, subdivisions);
        let num_vertices = fill_vertices_raw.len() / 2;
        let fill_vertices = NumericData::Float32(Arc::new(fill_vertices_raw));
        let vertex_color_index = NumericData::Uint32(Arc::new(vec![0u32; num_vertices]));
        Self { view_params, layer_params, subpaths, fill_vertices, vertex_color_index }
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
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        if self.fill_vertices.is_empty() {
            return;
        }
        let triangulated = TriangulatedLayer::new(
            self.view_params.clone(),
            TriangulatedLayerParams {
                layer_id: self.layer_params.layer_id.clone(),
                bounds: self.layer_params.bounds.clone(),
                data_unit_mode_x: self.layer_params.data_unit_mode_x.clone(),
                data_unit_mode_y: self.layer_params.data_unit_mode_y.clone(),
                model_matrix: self.layer_params.model_matrix,
                vertices: self.fill_vertices.clone(),
                vertex_color_index: self.vertex_color_index.clone(),
                fill_color: self.layer_params.fill_color.clone(),
                fill_opacity: self.layer_params.fill_opacity,
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
        let Self { layer_params, view_params, subpaths, .. } = self;

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
                layer_params.data_unit_mode_x.clone(),
                layer_params.data_unit_mode_y.clone(),
                view_params.aspect_ratio_mode.clone(),
                view_params.aspect_ratio_alignment_mode.clone(),
                layer_params.model_matrix.as_ref().map(|m| m.as_slice()),
            );
            (px as f64, (layer_h - py) as f64)
        };

        // A single shape uses one color, resolved from element 0.
        let quant_domain = match layer_params.fill_color.as_ref() {
            Some(ColorMode::Quantitative(params)) => quantitative_domain(params),
            _ => [0.0, 1.0],
        };
        let fill = TwoColor::Rgb(cpu_fill_color(layer_params.fill_color.as_ref(), 0, quant_domain));

        let mut svg_elements: Vec<TwoElement> = Vec::with_capacity(subpaths.len());
        for subpath in subpaths {
            if subpath.is_empty() {
                continue;
            }
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
                fill: Some(fill.clone()),
                linewidth: 0.0,
                opacity: 1.0,
                fill_opacity: layer_params.fill_opacity as f64,
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

impl PickableLayer for FilledCurveLayer {}
