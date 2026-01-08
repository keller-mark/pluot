use std::borrow::Cow;

use crate::deckish::scatterplot_layer::ScatterplotLayer;
use crate::deckish::layer::{render_canvas, ViewParams, PreparedAndDrawToCanvas};
use crate::wgpu;
use crate::log;
use encase::{ShaderType, UniformBuffer};
use glam::{Mat4, Vec2, Vec4};
/*
use vello::{
    peniko::{Blob, Brush, Color, Fill, Font},
    kurbo::{Affine, Circle, Ellipse, Line, RoundedRect, Stroke},
    AaConfig, AaSupport, Renderer, RendererOptions, RenderParams, Scene,
};
*/
use crate::params::{PlotParams, RenderContext, RenderResult};

use crate::d3::axis::{Axis, AxisOrientation};
use crate::d3::scale::{LinearRangeable, ScaleLinear, Tickable};
use crate::two::shapes::{
    TwoCircle, TwoElement, TwoGroup, TwoLine, TwoPath, TwoRectangle, TwoText,
};

use crate::cache::get_or_init_buffer;

#[derive(ShaderType, Debug)]
struct ScatterplotUniforms {
    viewport_size: Vec2, // (width, height) in pixels
    plot_margin: Vec4,   // (top, right, bottom, left) in pixels
    camera_view: Mat4,   // mat4x4<f32>,
    point_size_px: f32,  // diameter in pixels
    color: Vec4,         // rgba color for points
}

pub async fn render_layered_plot(
    context: &mut RenderContext<'_>,
    encoder: &mut wgpu::CommandEncoder,
) -> RenderResult {
    // Get x and y data from the Zarr store.
    let store = context.store;
    let height = context.params.height as f64;
    let width = context.params.width as f64;

    let margin_top = context.params.margin_top.unwrap_or(0.0) as f64;
    let margin_right = context.params.margin_right.unwrap_or(0.0) as f64;
    let margin_bottom = context.params.margin_bottom.unwrap_or(0.0) as f64;
    let margin_left = context.params.margin_left.unwrap_or(0.0) as f64;

    let PlotParams::Scatterplot(scatterplot_params) = &context.params.plot_params else {
        panic!("Expected scatterplot params");
    };

    let view_params = ViewParams {
        view_id: context.params.plot_id.to_string(),
        width: context.params.width,
        height: context.params.height,
        margin_top: Some(margin_top as f32),
        margin_right: Some(margin_right as f32),
        margin_bottom: Some(margin_bottom as f32),
        margin_left: Some(margin_left as f32),
        device_pixel_ratio: context.params.device_pixel_ratio,
        camera_view: context.params.camera_view,
        timeout: context.params.timeout,
        cache_enabled: context.params.cache_enabled,
    };

    let layers: Vec<Box<dyn PreparedAndDrawToCanvas>> = vec![
        Box::new(ScatterplotLayer::new(
            view_params.clone(),
            store.clone(),
            context.params.store_name.clone(),
            "my_layer".to_string(),
            scatterplot_params.x_key.clone(),
            scatterplot_params.y_key.clone(),
            scatterplot_params.color_key.clone(),
            scatterplot_params.point_radius,
        )),
    ];
    let render_result = render_canvas(view_params, layers, context, encoder).await;

    RenderResult {
        bailed_early: false,
    }
}
