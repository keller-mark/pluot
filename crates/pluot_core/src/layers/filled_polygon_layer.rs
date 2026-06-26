// FilledPolygonLayer accepts polygon vertices as input, internally converts these to triangles,
// and ultimately renders a TriangulatedLayer as a sub-layer.
// This layer is intended to be used as a sub-layer of PolygonLayer.

use earcut::Earcut;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::positioning::get_point_position;
use super::curve_and_polygon_utils::resolve_margins;
use crate::render_traits::{
    DrawToRasterCpu, DrawToRasterGpu, DrawToSvg,
    MarginParams, PickableLayer, PreparedLayer, UnitsMode, ViewParams,
};
use crate::render_types::{CpuContext, CpuRenderPass, GpuContext, PrepareResult, RenderResult};
use crate::two::shapes::{TwoColor, TwoElement, TwoGroup, TwoPath};
use crate::two::svg::{update_svg, SvgContext};
use crate::wgpu;

use super::triangulated_layer::{TriangulatedLayer, TriangulatedLayerParams};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct FilledPolygonLayerParams {
    pub layer_id: String,
    pub bounds: Option<MarginParams>,
    pub data_unit_mode_x: UnitsMode,
    pub data_unit_mode_y: UnitsMode,
    pub model_matrix: Option<[f32; 16]>,

    /// One polygon per element; each is a ring of (x, y) model-space vertices.
    /// Rings with fewer than 3 points are silently skipped.
    pub polygons: Arc<Vec<Vec<(f32, f32)>>>,

    /// RGB fill color as `[r, g, b]` bytes in `[0, 255]`. Defaults to opaque black.
    pub fill_color: [u8; 3],
    /// Opacity multiplier for the fill. Defaults to 1.
    pub fill_opacity: f32,
}

impl Default for FilledPolygonLayerParams {
    fn default() -> Self {
        Self {
            layer_id: "".to_string(),
            bounds: None,
            data_unit_mode_x: UnitsMode::Data,
            data_unit_mode_y: UnitsMode::Data,
            model_matrix: None,
            polygons: Arc::new(vec![]),
            fill_color: [0, 0, 0],
            fill_opacity: 1.0,
        }
    }
}

pub struct FilledPolygonLayer {
    view_params: ViewParams,
    layer_params: FilledPolygonLayerParams,
    fill_vertices: Arc<Vec<(f32, f32)>>,
}

impl FilledPolygonLayer {
    pub fn new(view_params: ViewParams, layer_params: FilledPolygonLayerParams) -> Self {
        // TODO: move the triangulation into the prepare() function?
        // TODO: only do the triangulation in the raster drawing case?
        let mut ec = Earcut::new();
        let mut indices = Vec::new();
        let mut verts: Vec<(f32, f32)> = Vec::new();
        for ring in layer_params.polygons.iter() {
            if ring.len() < 3 {
                continue;
            }
            ec.earcut(ring.iter().map(|&(x, y)| [x, y]), &[] as &[u32], &mut indices);
            for &i in indices.iter() {
                verts.push(ring[i as usize]);
            }
        }
        Self { view_params, layer_params, fill_vertices: Arc::new(verts) }
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
                data_unit_mode_x: self.layer_params.data_unit_mode_x.clone(),
                data_unit_mode_y: self.layer_params.data_unit_mode_y.clone(),
                model_matrix: self.layer_params.model_matrix,
                vertices: Arc::clone(&self.fill_vertices),
                fill_color: self.layer_params.fill_color,
                fill_opacity: self.layer_params.fill_opacity,
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
                layer_params.data_unit_mode_x.clone(),
                layer_params.data_unit_mode_y.clone(),
                view_params.aspect_ratio_mode.clone(),
                view_params.aspect_ratio_alignment_mode.clone(),
                layer_params.model_matrix.as_ref().map(|m| m.as_slice()),
            );
            (px as f64, (layer_h - py) as f64)
        };

        let [r, g, b] = layer_params.fill_color;
        let fill = TwoColor::Rgb((r, g, b));

        let mut svg_elements: Vec<TwoElement> = Vec::with_capacity(layer_params.polygons.len());
        for ring in layer_params.polygons.iter() {
            if ring.len() < 3 {
                continue;
            }
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

impl PickableLayer for FilledPolygonLayer {}
