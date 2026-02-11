use std::ops::{Add, Sub};
use std::time::{Duration, SystemTime};
use std::sync::Arc;

use xilem::masonry::dpi::LogicalSize;
use xilem::masonry::properties::types::{CrossAxisAlignment, MainAxisAlignment};
use xilem::masonry::layout::AsUnit;
use xilem::{EventLoop, EventLoopBuilder};
use xilem::winit::error::EventLoopError;
use xilem::core::fork;
use xilem::core::one_of::Either;
use xilem::view::{FlexSequence, FlexSpacer, flex_col, flex_row, label, task, text_button, canvas};
use xilem::{WidgetView, WindowOptions, Xilem, ImageBrush, ImageFormat,};
use xilem::core::Edit;
use xilem::vello::Scene;
use xilem::vello::kurbo::{Affine, Size};
use xilem::vello::peniko::{ImageAlphaType, ImageData};

use pluot::{
    render, RenderParams, PlotParams, LayeredPlotRenderParams, GraphicsFormat,
    AspectRatioMode, LayerParams, UnitsMode, ViewParams,
    MarginParams, ScatterplotLayerParams, PointShapeMode,
};


async fn generate_image(width: u32, height: u32) -> Vec<u8> {
    let num_pixels = (width * height) as usize;
    let mut pixels = vec![0u8; (num_pixels * 4)];
    for y in 0..height as i32 {
        for x in 0..width as i32 {
            let index = ((y * width as i32 + x) * 4) as usize;
            pixels[index] = 255 as u8;
            pixels[index + 1] = 0 as u8;
            pixels[index + 2] = 0 as u8;
            pixels[index + 3] = 255;
        }
    }
    return pixels;
}

async fn render_unit_square_raster(width: u32, height: u32) -> Vec<u8> {
    let params = RenderParams {
        width: width,
        height: height,
        format: GraphicsFormat::Raster,
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: vec![
                LayerParams {
                    layer_type: "ScatterplotLayer".to_string(),
                    layer_params: serde_json::to_value(ScatterplotLayerParams {
                        layer_id: "my_scatterplot_layer".to_string(),
                        bounds: Some(MarginParams {
                            margin_left: Some(0.0),
                            margin_right: Some(0.0),
                            margin_top: Some(0.0),
                            margin_bottom: Some(0.0),
                        }),
                        data_unit_mode: UnitsMode::Data,
                        point_radius: 10.0,
                        point_radius_unit_mode: UnitsMode::Pixels,
                        point_shape_mode: PointShapeMode::Square,
                        x_vec: Arc::new(vec![0.0, 1.0, 1.0, 0.0]),
                        y_vec: Arc::new(vec![0.0, 0.0, 1.0, 1.0]),
                        labels_vec: Arc::new(vec![0, 1, 2, 3]),
                    }).unwrap(),
                },
            ],
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    let result_vec = render(params).await;

    let NUM_EXTRA_BYTES = 1;

    // Return result_vec minus the extra bytes.

    return result_vec[..(result_vec.len() - NUM_EXTRA_BYTES)].to_vec();
}

/// The state of the entire application.
///
/// This is owned by Xilem, used to construct the view tree, and updated by event handlers.
struct Stopwatch {
    active: bool,

    img_size: Size,
    img_pixels: Vec<u8>,
}

fn app_logic(data: &mut Stopwatch) -> impl WidgetView<Edit<Stopwatch>> + use<> {
    fork(
        flex_col((
            FlexSpacer::Fixed(1.px()),
            start_stop_button(data),
            canvas(|state: &mut Stopwatch, _ctx, scene: &mut Scene, size: Size| {
                let pixels = state.img_pixels.clone();
                let image_data = ImageData {
                    width: size.width as u32,
                    height: size.height as u32,
                    format: ImageFormat::Rgba8,
                    data: pixels.into(),
                    alpha_type: ImageAlphaType::Alpha,
                };
                let image_brush: ImageBrush = ImageBrush::new(image_data);
                scene.draw_image(&image_brush, Affine::IDENTITY);
              state.img_size = size;
            }),
            FlexSpacer::Fixed(1_i32.px()),
        )),
        data.active.then(|| {
            // Only update while active.
            task(
                |proxy, data: &mut Stopwatch| {
                    let width = data.img_size.width as u32;
                    let height = data.img_size.height as u32;
                    async move {
                        let pixels = render_unit_square_raster(width, height).await;
                        proxy.message(pixels.clone());
                    }
                },
                |data: &mut Stopwatch, pixels: Vec<u8>| {
                    data.img_pixels = pixels;
                },
            )
        }),
    )
}


fn start_stop_button(data: &mut Stopwatch) -> impl WidgetView<Edit<Stopwatch>> + use<> {
    if data.active {
        Either::A(text_button("Stop", |data: &mut Stopwatch| {
            data.active = false;
        }))
    } else {
        Either::B(text_button("Start", |data: &mut Stopwatch| {
            data.active = true;
        }))
    }
}


pub(crate) fn run(event_loop: EventLoopBuilder) -> Result<(), EventLoopError> {
    let data = Stopwatch {
        active: false,
        img_pixels: vec![255; 450 * 300 * 4],
        img_size: Size::new(450.0, 300.0),
    };

    let window_options = WindowOptions::new("Stopwatch")
        .with_min_inner_size(LogicalSize::new(300., 200.))
        .with_initial_inner_size(LogicalSize::new(450., 300.));
    let app = Xilem::new_simple(data, app_logic, window_options);
    app.run_in(event_loop)?;
    Ok(())
}

// Boilerplate code: Identical across all applications which support Android

fn main() -> Result<(), EventLoopError> {
    run(EventLoop::with_user_event())
}
