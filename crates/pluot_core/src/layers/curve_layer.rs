// CurveLayer renders SVG-like vector paths as stroked and/or filled curves,
// delegating to StrokedCurveLayer and FilledCurveLayer sublayers.

use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::render_traits::{ColorMode, DrawToRasterGpu, DrawToRasterCpu, DrawToSvg, PickableLayer, PreparedLayer, ViewParams, UnitsMode, MarginParams};
use crate::render_types::{CpuContext, CpuRenderPass, PrepareResult};
use crate::render_types::GpuContext;
use crate::two::svg::SvgContext;
use crate::wgpu;

use super::stroked_curve_layer::{StrokedCurveLayer, StrokedCurveLayerParams};
use super::filled_curve_layer::{FilledCurveLayer, FilledCurveLayerParams};

pub use super::curve_and_polygon_utils::PathCommand;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct CurveLayerParams {
    pub layer_id: String,
    pub bounds: Option<MarginParams>,
    pub data_unit_mode_x: UnitsMode,
    pub data_unit_mode_y: UnitsMode,
    pub stroke_width: f32,
    pub stroke_width_unit_mode: UnitsMode,
    pub model_matrix: Option<[f32; 16]>,
    pub commands: Arc<Vec<PathCommand>>,
    pub subdivisions: u32,
    pub stroked: bool,
    pub filled: bool,
    /// How to color the stroke. See [`ColorMode`]. `CurveLayer` renders a single
    /// shape, so modes carrying `NumericData` are expected to supply a single
    /// (length-1) value.
    pub stroke_color: ColorMode,
    /// How to color the fill. See [`ColorMode`]. Same single-shape caveat as
    /// `stroke_color`.
    pub fill_color: ColorMode,
    pub stroke_opacity: f32,
    pub fill_opacity: f32,
}

impl Default for CurveLayerParams {
    fn default() -> Self {
        Self {
            layer_id: "".to_string(),
            bounds: None,
            data_unit_mode_x: UnitsMode::Data,
            data_unit_mode_y: UnitsMode::Data,
            stroke_width: 1.0,
            stroke_width_unit_mode: UnitsMode::Pixels,
            model_matrix: None,
            commands: Arc::new(vec![]),
            subdivisions: 32,
            stroked: true,
            filled: false,
            stroke_color: ColorMode::UniformRgb(None),
            fill_color: ColorMode::UniformRgb(None),
            stroke_opacity: 1.0,
            fill_opacity: 1.0,
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
            && (layer_params.data_unit_mode_x == UnitsMode::Pixels
                || layer_params.data_unit_mode_y == UnitsMode::Pixels)
        {
            panic!("stroke_width_unit_mode cannot be 'data' when data_unit_mode is 'pixels'");
        }

        let stroke_sublayer = if layer_params.stroked {
            Some(StrokedCurveLayer::new(view_params.clone(), StrokedCurveLayerParams {
                layer_id: format!("{}_stroked", layer_params.layer_id),
                bounds: layer_params.bounds.clone(),
                data_unit_mode_x: layer_params.data_unit_mode_x.clone(),
                data_unit_mode_y: layer_params.data_unit_mode_y.clone(),
                stroke_width: layer_params.stroke_width,
                model_matrix: layer_params.model_matrix,
                commands: Arc::clone(&layer_params.commands),
                subdivisions: layer_params.subdivisions,
                stroke_color: layer_params.stroke_color.clone(),
                stroke_opacity: layer_params.stroke_opacity,
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
                fill_opacity: layer_params.fill_opacity,
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

impl PickableLayer for CurveLayer {}
