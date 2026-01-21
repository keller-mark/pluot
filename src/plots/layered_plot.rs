use crate::layers::line_layer::LineLayer;
use crate::layers::scatterplot_layer::{PointShapeMode, ScatterplotLayer};
use crate::layers::zarr_scatterplot_layer::ZarrScatterplotLayer;
use crate::layers::core::{AspectRatioMode, MarginParams, PreparedAndDraw, UnitsMode, ViewParams, render_canvas};
use crate::wgpu;
use crate::log;
use crate::params::{LayeredPlotRenderParams, PlotParams, RenderContext, RenderResult, LayerParams};
use crate::d3::axis::{Axis, AxisOrientation};
use crate::d3::scale::{LinearRangeable, ScaleLinear, Tickable};
use crate::two::shapes::{
    TwoCircle, TwoElement, TwoGroup, TwoLine, TwoPath, TwoRectangle, TwoText,
};

pub fn render_layered_plot(
    context: &mut RenderContext<'_>,
    encoder: &mut wgpu::CommandEncoder,
) -> Vec<Box<dyn PreparedAndDraw>> {
    // Get x and y data from the Zarr store.
    let store = context.store;
    let height = context.params.height as f64;
    let width = context.params.width as f64;

    let margin_top = context.params.margin_top.unwrap_or(0.0) as f32;
    let margin_right = context.params.margin_right.unwrap_or(0.0) as f32;
    let margin_bottom = context.params.margin_bottom.unwrap_or(0.0) as f32;
    let margin_left = context.params.margin_left.unwrap_or(0.0) as f32;

    let PlotParams::LayeredPlot(plot_params) = &context.params.plot_params else {
        panic!("Expected layered plot params");
    };

    let view_params = ViewParams {
        view_id: context.params.plot_id.to_string(),
        width: context.params.width,
        height: context.params.height,
        margins: Some(MarginParams {
            margin_top: Some(margin_top),
            margin_right: Some(margin_right),
            margin_bottom: Some(margin_bottom),
            margin_left: Some(margin_left),
        }),
        device_pixel_ratio: context.params.device_pixel_ratio,
        camera_view: context.params.camera_view,
        timeout: context.params.timeout,
        cache_enabled: context.params.cache_enabled,
        aspect_ratio_mode: context.params.aspect_ratio_mode,
    };

    let layers: Vec<Box<dyn PreparedAndDraw>> = plot_params.layers.iter().map(|layer_params| {
        match layer_params {
            LayerParams::ZarrScatterplotLayer(layer_params) => {
                Box::new(ZarrScatterplotLayer::new(
                    view_params.clone(),
                    layer_params.clone(),
                )) as Box<dyn PreparedAndDraw>
            },
            LayerParams::ScatterplotLayer(layer_params) => {
                Box::new(ScatterplotLayer::new(
                    view_params.clone(),
                    layer_params.clone(),
                )) as Box<dyn PreparedAndDraw>
            },
            LayerParams::LineLayer(layer_params) => {
                Box::new(LineLayer::new(
                    view_params.clone(),
                    layer_params.clone(),
                )) as Box<dyn PreparedAndDraw>
            },
            LayerParams::TextLayer(layer_params) => {
                Box::new(crate::layers::text_layer::TextLayer::new(
                    view_params.clone(),
                    layer_params.clone(),
                )) as Box<dyn PreparedAndDraw>
            },
            // We do not want a catch-all here, so that we get a compile error
            // when implementing new layer types.
        }
    }).collect();
    
    return layers;
}
