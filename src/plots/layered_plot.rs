use crate::layers::scatterplot_layer::{PointShapeMode, ScatterplotLayer};
use crate::layers::zarr_scatterplot_layer::ZarrScatterplotLayer;
use crate::layers::core::{AspectRatioMode, MarginParams, PreparedAndDrawToCanvas, UnitsMode, ViewParams, render_canvas};
use crate::wgpu;
use crate::log;
use crate::params::{PlotParams, RenderContext, RenderResult};
use crate::d3::axis::{Axis, AxisOrientation};
use crate::d3::scale::{LinearRangeable, ScaleLinear, Tickable};
use crate::two::shapes::{
    TwoCircle, TwoElement, TwoGroup, TwoLine, TwoPath, TwoRectangle, TwoText,
};

pub async fn render_layered_plot(
    context: &mut RenderContext<'_>,
    encoder: &mut wgpu::CommandEncoder,
) -> RenderResult {
    // Get x and y data from the Zarr store.
    let store = context.store;
    let height = context.params.height as f64;
    let width = context.params.width as f64;

    let margin_top = context.params.margin_top.unwrap_or(0.0) as f32;
    let margin_right = context.params.margin_right.unwrap_or(0.0) as f32;
    let margin_bottom = context.params.margin_bottom.unwrap_or(0.0) as f32;
    let margin_left = context.params.margin_left.unwrap_or(0.0) as f32;

    let PlotParams::LayeredPlot(plot_params) = &context.params.plot_params else {
        panic!("Expected scatterplot params");
    };

    let view_params = ViewParams {
        view_id: context.params.plot_id.to_string(),
        width: context.params.width,
        height: context.params.height,
        margins: None,
        device_pixel_ratio: context.params.device_pixel_ratio,
        camera_view: context.params.camera_view,
        timeout: context.params.timeout,
        cache_enabled: context.params.cache_enabled,
        aspect_ratio_mode: AspectRatioMode::Contain,
    };

    let layers: Vec<Box<dyn PreparedAndDrawToCanvas>> = vec![
        Box::new(ZarrScatterplotLayer::new(
            view_params.clone(),
            Some(MarginParams {
                margin_top: Some(margin_top),
                margin_right: Some(margin_right),
                margin_bottom: Some(margin_bottom),
                margin_left: Some(margin_left),
            }),
            store.clone(),
            context.params.store_name.clone(),
            "my_layer".to_string(),
            plot_params.x_key.clone(),
            plot_params.y_key.clone(),
            plot_params.color_key.clone(),
            plot_params.point_radius.unwrap_or(5.0),
            UnitsMode::Pixels,
            PointShapeMode::Square,
        )),
    ];

    // TODO: render to canvas or svg depending on `format` param.
    let render_result = render_canvas(view_params, layers, context, encoder).await;

    return render_result;
}
