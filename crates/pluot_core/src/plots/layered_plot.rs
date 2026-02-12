use crate::layers::core::{MarginParams, PreparedAndDraw, ViewParams};
use crate::registry::get_layer_from_registry;
use crate::wgpu;
use crate::params::{PlotParams, RenderContext, LayerParams};

pub fn get_layer(layer_params: &LayerParams, view_params: &ViewParams) -> Box<dyn PreparedAndDraw> {
    get_layer_from_registry(&layer_params.layer_type, layer_params.layer_params.clone(), view_params)
}

pub fn render_layered_plot(
    context: &mut RenderContext<'_>,
    encoder: &mut wgpu::CommandEncoder,
) -> Vec<Box<dyn PreparedAndDraw>> {
    // Get x and y data from the Zarr store.
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
        store_name: Some(context.params.store_name.clone()),
    };

    let layers: Vec<Box<dyn PreparedAndDraw>> = plot_params.layers.iter().map(|layer_params| {
        get_layer(layer_params, &view_params)
    }).collect();

    return layers;
}
