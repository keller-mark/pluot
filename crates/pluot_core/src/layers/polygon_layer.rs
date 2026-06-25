// PolygonLayer renders a collection of polygons as stroked outlines, filled
// interiors, or both, by delegating to StrokedPolygonLayer and FilledPolygonLayer.

use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::render_traits::{
    DrawToRasterCpu, DrawToRasterGpu, DrawToSvg,
    MarginParams, PickableLayer, PreparedLayer, UnitsMode, ViewParams,
};
use crate::render_types::{CpuContext, CpuRenderPass, GpuContext, PrepareResult};
use crate::two::svg::SvgContext;
use crate::wgpu;

use super::stroked_polygon_layer::{StrokedPolygonLayer, StrokedPolygonLayerParams};
use super::filled_polygon_layer::{FilledPolygonLayer, FilledPolygonLayerParams};

// ── Params ─────────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct PolygonLayerParams {
    pub layer_id: String,
    /// If `None`, the view-level margins are used.
    pub bounds: Option<MarginParams>,
    pub data_unit_mode_x: UnitsMode,
    pub data_unit_mode_y: UnitsMode,
    pub model_matrix: Option<[f32; 16]>,

    /// One polygon per element; each polygon is a ring of (x, y) model-space vertices.
    /// Rings with fewer than 3 points are silently skipped.
    pub polygons: Arc<Vec<Vec<(f32, f32)>>>,

    /// Whether to stroke the polygon outlines. Defaults to `true`.
    pub stroked: bool,
    /// Whether to fill the polygon interiors. Defaults to `false`.
    pub filled: bool,

    /// RGBA stroke color in [0, 1]. Defaults to opaque black.
    pub stroke_color: [f32; 4],
    /// Stroke width in pixels. Defaults to 1.
    pub stroke_width: f32,
    /// Additional opacity multiplier for the stroke. Defaults to 1.
    pub stroke_opacity: f32,

    /// RGBA fill color in [0, 1]. Defaults to opaque black.
    pub fill_color: [f32; 4],
    /// Additional opacity multiplier for the fill. Defaults to 1.
    pub fill_opacity: f32,
}

impl Default for PolygonLayerParams {
    fn default() -> Self {
        Self {
            layer_id: "".to_string(),
            bounds: None,
            data_unit_mode_x: UnitsMode::Data,
            data_unit_mode_y: UnitsMode::Data,
            model_matrix: None,
            polygons: Arc::new(vec![]),
            stroked: true,
            filled: false,
            stroke_color: [0.0, 0.0, 0.0, 1.0],
            stroke_width: 1.0,
            stroke_opacity: 1.0,
            fill_color: [0.0, 0.0, 0.0, 1.0],
            fill_opacity: 1.0,
        }
    }
}

// ── Layer ──────────────────────────────────────────────────────────────────────

pub struct PolygonLayer {
    stroke_sublayer: Option<StrokedPolygonLayer>,
    fill_sublayer: Option<FilledPolygonLayer>,
}

impl PolygonLayer {
    pub fn new(view_params: ViewParams, layer_params: PolygonLayerParams) -> Self {
        let stroke_sublayer = if layer_params.stroked {
            Some(StrokedPolygonLayer::new(view_params.clone(), StrokedPolygonLayerParams {
                layer_id: format!("{}_stroked", layer_params.layer_id),
                bounds: layer_params.bounds.clone(),
                data_unit_mode_x: layer_params.data_unit_mode_x.clone(),
                data_unit_mode_y: layer_params.data_unit_mode_y.clone(),
                model_matrix: layer_params.model_matrix,
                polygons: Arc::clone(&layer_params.polygons),
                stroke_color: layer_params.stroke_color,
                stroke_width: layer_params.stroke_width,
                stroke_opacity: layer_params.stroke_opacity,
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
                polygons: Arc::clone(&layer_params.polygons),
                fill_color: layer_params.fill_color,
                fill_opacity: layer_params.fill_opacity,
            }))
        } else {
            None
        };

        Self { stroke_sublayer, fill_sublayer }
    }
}

// ── Trait impls ────────────────────────────────────────────────────────────────

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for PolygonLayer {
    async fn prepare(&mut self, _gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
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

impl PickableLayer for PolygonLayer {}
