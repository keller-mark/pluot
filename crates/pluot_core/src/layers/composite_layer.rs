// Inspired by the DeckGL CompositeLayer
// Reference: https://deck.gl/docs/api-reference/layers/scatterplot-layer

use encase::{ShaderType, UniformBuffer};
use glam::{Mat4, Vec2, Vec4};
use serde::{Deserialize, Serialize};

use crate::layer_traits::{AspectRatioMode, DrawToCanvas, DrawToSvg, MarginParams, PreparedLayer, UnitsMode, ViewParams, PreparedAndDraw};
use crate::wgpu;
use crate::cache::{use_memo_vec_f32, use_memo_vec_i32};
use svg::node::element::Group;
use crate::two::shapes::{TwoCircle, TwoElement, TwoGroup, TwoLine, TwoPath, TwoRectangle, TwoText};
use crate::two::svg::update_svg;
use crate::layers::position_utils::get_point_position;
use crate::params::{LayerParams, PrepareResult, RenderResult};
use crate::layered_plot::get_layer;

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


pub async fn base_prepare_composite_layer(sub_layer_instances: &mut [Box<dyn PreparedAndDraw>]) -> PrepareResult {
    // TODO: use futures::join, the same as in the core::render functions.
    let mut bailed_early = false;
    for sub_layer in sub_layer_instances.iter_mut() {
        let sub_layer_result = sub_layer.prepare().await;
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
    async fn prepare(&mut self) -> PrepareResult {
        return base_prepare_composite_layer(&mut self.sub_layer_instances).await;
    }
}

// Reusable function that can be used by other composite layers: raster variant.
pub async fn base_draw_composite_layer(
    sub_layer_instances: &[Box<dyn PreparedAndDraw>],
    device: wgpu::Device,
    queue: wgpu::Queue,
    pass: &mut wgpu::RenderPass<'_>,
) {
    for sub_layer in sub_layer_instances.iter() {
        DrawToCanvas::draw(sub_layer.as_ref(), device.clone(), queue.clone(), pass).await;
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToCanvas for CompositeLayer {
    async fn draw(&self, device: wgpu::Device, queue: wgpu::Queue, pass: &mut wgpu::RenderPass) {
        // TODO: ensure this works with pyO3. If needed, change trait to take &mut self,
        // and then use the same pattern as in the core::render_ functions.
        base_draw_composite_layer(&self.sub_layer_instances, device, queue, pass).await;
    }
}


// Reusable function that can be used by other composite layers: SVG variant.
pub async fn base_draw_composite_layer_svg(
    sub_layer_instances: &[Box<dyn PreparedAndDraw>],
    group: &Group,
) -> Group {
    let mut updated_group = group.clone();
    for sub_layer in sub_layer_instances.iter() {
        updated_group = DrawToSvg::draw(sub_layer.as_ref(), &updated_group).await;
    }
    updated_group
}


#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DrawToSvg for CompositeLayer {
    async fn draw(&self, group: &Group) -> Group {
        base_draw_composite_layer_svg(&self.sub_layer_instances, group).await
    }
}
