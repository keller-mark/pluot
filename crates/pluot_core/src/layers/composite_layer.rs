// Inspired by the DeckGL CompositeLayer
// Reference: https://deck.gl/docs/api-reference/layers/scatterplot-layer

use encase::{ShaderType, UniformBuffer};
use glam::{Mat4, Vec2, Vec4};
use serde::{Deserialize, Serialize};

use crate::render_traits::{AspectRatioMode, DrawToRasterGpu, DrawToRasterCpu, DrawToSvg, MarginParams, PickableLayer, PreparedLayer, UnitsMode, ViewParams, PreparedAndDraw};
use crate::wgpu;
use crate::two::shapes::{TwoCircle, TwoElement, TwoGroup, TwoLine, TwoPath, TwoRectangle, TwoText};
use crate::two::svg::SvgContext;
use crate::positioning::get_point_position;
use crate::params::{LayerParams};
use crate::render_types::{CpuContext, CpuRenderPass, PrepareResult, RenderResult};
use crate::render_types::GpuContext;
use crate::render_traits::get_layer;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CompositeLayerParams {
    // pub layer_id: String, // TODO: do we need a layer_id here?
    pub sub_layers: Vec<LayerParams>,
}

// TODO: defaults for params?


pub struct CompositeLayer {
    view_params: ViewParams,
    layer_params: CompositeLayerParams,

    sub_layer_instances: Vec<Box<dyn PreparedAndDraw>>,
}

impl CompositeLayer {
    pub fn new(
        view_params: ViewParams,
        layer_params: CompositeLayerParams,
    ) -> Self {

        let sub_layer_instances: Vec<Box<dyn PreparedAndDraw>> = layer_params.sub_layers
            .iter()
            .map(|layer_param| {
                get_layer(layer_param, &view_params)
            })
            .collect();

        Self {
            view_params,
            layer_params,
            sub_layer_instances,
        }
    }
}


pub async fn base_prepare_composite_layer(sub_layer_instances: &mut [Box<dyn PreparedAndDraw>], gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
    // TODO: use futures::join, the same as in the layer_traits::render functions.
    let mut bailed_early = false;
    for sub_layer in sub_layer_instances.iter_mut() {
        let sub_layer_result = sub_layer.prepare(gpu_context).await;
        if sub_layer_result.bailed_early {
            bailed_early = true;
        }
    }
    return PrepareResult {
        bailed_early,
    };
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PreparedLayer for CompositeLayer {
    async fn prepare(&mut self, gpu_context: Option<&GpuContext<'_>>) -> PrepareResult {
        return base_prepare_composite_layer(&mut self.sub_layer_instances, gpu_context).await;
    }
}

// Reusable function that can be used by other composite layers: raster variant.
pub async fn base_draw_composite_layer(
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
impl DrawToRasterGpu for CompositeLayer {
    async fn draw(&self, gpu_context: &GpuContext<'_>, pass: &mut wgpu::RenderPass) {
        base_draw_composite_layer(&self.sub_layer_instances, gpu_context, pass).await;
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToRasterCpu for CompositeLayer {
    async fn draw(&self, _cpu_context: &CpuContext<'_>, _pass: &mut CpuRenderPass) {}
}


// Reusable function that can be used by other composite layers: SVG variant.
pub async fn base_draw_composite_layer_svg(
    sub_layer_instances: &[Box<dyn PreparedAndDraw>],
    ctx: &mut SvgContext,
) {
    for sub_layer in sub_layer_instances.iter() {
        DrawToSvg::draw(sub_layer.as_ref(), ctx).await;
    }
}


#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for CompositeLayer {
    async fn draw(&self, ctx: &mut SvgContext) {
        base_draw_composite_layer_svg(&self.sub_layer_instances, ctx).await
    }
}

impl PickableLayer for CompositeLayer {}
