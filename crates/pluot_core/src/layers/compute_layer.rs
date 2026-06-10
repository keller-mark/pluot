// TODO: remove this once there are other compute shader examples.
use encase::{ShaderType, UniformBuffer};
use glam::{Mat4, Vec2, Vec4};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::layers::text_layer::{TextAlignMode, TextBaselineMode, TextLayer, TextLayerParams};
use crate::render_traits::{AspectRatioMode, DrawToRasterGpu, DrawToRasterCpu, DrawToSvg, MarginParams, PickableLayer, PreparedLayer, UnitsMode, ViewParams, PreparedAndDraw, FontWeight, FontStyle};
use crate::wgpu;
use crate::two::shapes::{TwoCircle, TwoElement, TwoGroup, TwoLine, TwoPath, TwoRectangle, TwoText};
use crate::two::svg::SvgContext;
use crate::positioning::get_point_position;
use crate::params::{LayerParams};
use crate::render_types::{CpuContext, CpuRenderPass, PrepareResult, RenderResult};
use crate::render_types::GpuContext;
use crate::render_traits::get_layer;
use crate::compute::example::compute_example_with_memo;


#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ComputeLayerParams {
    pub layer_id: String,
}

// TODO: defaults for params?


pub struct ComputeLayer {
    view_params: ViewParams,
    layer_params: ComputeLayerParams,

    sub_layer_instances: Vec<Box<dyn PreparedAndDraw>>,
}

impl ComputeLayer {
    pub fn new(
        view_params: ViewParams,
        layer_params: ComputeLayerParams,
    ) -> Self {
        Self {
            view_params,
            layer_params,
            sub_layer_instances: Vec::new(),
        }
    }
}


#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for ComputeLayer {
    async fn prepare(&mut self, gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {

        let Some(gpu_context_inner) = gpu_context else {
            panic!("ComputeLayer requires a GPU context for preparation");
        };

        // For demonstration, we'll just call a compute function that uses memoization.
        // In a real implementation, this is where you'd set up GPU resources, load data, etc.
        let compute_result = compute_example_with_memo(gpu_context_inner).await;

        let text_position_x = vec![10.0];
        let text_position_y = vec![20.0];
        let text_strings = vec!["Compute result: ".to_string() + &compute_result.to_string()];


        let text_params = TextLayerParams {
            layer_id: format!("{}_text", self.layer_params.layer_id),
            bounds: self.view_params.margins.clone(),
            data_unit_mode_x: UnitsMode::Pixels,
            data_unit_mode_y: UnitsMode::Pixels,
            text_size: 24.0_f32,
            text_size_unit_mode: UnitsMode::Pixels,
            text_align_mode: TextAlignMode::Start,
            text_baseline_mode: TextBaselineMode::Middle,
            text_rotation: Some(0.0_f32),
            font_family: None,
            font_weight: FontWeight::Normal,
            font_style: FontStyle::Normal,
            model_matrix: None,

            position_x: Arc::new(text_position_x),
            position_y: Arc::new(text_position_y),
            text_vec: Arc::new(text_strings),
        };
        self.sub_layer_instances.push(Box::new(TextLayer::new(
            self.view_params.clone(),
            text_params,
        )));

        for sub_layer in self.sub_layer_instances.iter_mut() {
            sub_layer.prepare(gpu_context).await;
        }

        return PrepareResult {
            bailed_early: false,
        };
    }
}

// Reusable function that can be used by other composite layers: raster variant.
pub async fn base_draw_compute_layer(
    sub_layer_instances: &[Box<dyn PreparedAndDraw>],
    gpu_context: &GpuContext<'_>,
    pass: &mut wgpu::RenderPass<'_>,
) {
    for sub_layer in sub_layer_instances.iter() {
        DrawToRasterGpu::draw(sub_layer.as_ref(), gpu_context, pass).await;
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterGpu for ComputeLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        base_draw_compute_layer(&self.sub_layer_instances, gpu_context, pass).await;
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for ComputeLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}


// Reusable function that can be used by other composite layers: SVG variant.
pub async fn base_draw_compute_layer_svg(
    sub_layer_instances: &[Box<dyn PreparedAndDraw>],
    ctx: &mut SvgContext,
) {
    for sub_layer in sub_layer_instances.iter() {
        DrawToSvg::draw(sub_layer.as_ref(), ctx).await;
    }
}


#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for ComputeLayer {
    async fn draw(&self, ctx: &mut SvgContext) {
        base_draw_compute_layer_svg(&self.sub_layer_instances, ctx).await
    }
}

inventory::submit! {
    crate::registry::LayerRegistration {
        layer_type_name: "ComputeLayer",
        create_layer: |value, view_params| {
            let params: ComputeLayerParams = serde_json::from_value(value).unwrap();
            Box::new(ComputeLayer::new(view_params.clone(), params))
        },
    }
}

impl PickableLayer for ComputeLayer {}
