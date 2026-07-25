// Utility functions inspired by the DeckGL CompositeLayer
// Reference: https://deck.gl/docs/api-reference/layers/scatterplot-layer

use crate::render_traits::{DrawToRasterGpu, DrawToRasterCpu, DrawToSvg, PreparedLayer, PreparedAndDraw};
use crate::wgpu;
use crate::two::svg::SvgContext;
use crate::render_types::{CpuContext, CpuRenderPass, PrepareResult, RenderResult};
use crate::render_types::GpuContext;


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

// Reusable function that can be used by other composite layers: SVG variant.
pub async fn base_draw_composite_layer_svg(
    sub_layer_instances: &[Box<dyn PreparedAndDraw>],
    ctx: &mut SvgContext,
) {
    for sub_layer in sub_layer_instances.iter() {
        DrawToSvg::draw(sub_layer.as_ref(), ctx).await;
    }
}
