// We only run this test on non-WASM targets AND when the "lacks_gpu" feature is not enabled (e.g., CI).
#![cfg(all(not(target_arch = "wasm32"), not(feature="lacks_gpu")))]

use std::sync::Arc;
use dify::diff;
use image::{ImageReader, RgbaImage, save_buffer_with_format, ColorType, ImageFormat};
use pluot::{
    render, RenderParams, PlotParams, LayeredPlotRenderParams, GraphicsFormat,
    AspectRatioMode, LayerParams, UnitsMode, ViewParams,
    MarginParams, ScatterplotLayerParams, PointShapeMode,
};


// Reference: https://github.com/jihchi/dify/blob/0e5f1fa546d7cd134cbb12cb019f337d36a3a053/benches/benchmark.rs#L5
fn get_image(path: &str) -> RgbaImage {
    ImageReader::open(path)
        .unwrap()
        .decode()
        .unwrap()
        .into_rgba8()
}

fn put_image(buffer: &[u8], width: u32, height: u32, path: &str) -> Result<(), image::ImageError> {
    save_buffer_with_format(
        path,
        buffer,
        width,
        height,
        ColorType::Rgba8,
        ImageFormat::Png,
    )
}



#[tokio::test]
async fn test_render_unit_square_raster() {
    let params = RenderParams {
        width: 100,
        height: 100,
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

    assert_eq!(
        result_vec.len(),
        ((100 * 100 * 4) + NUM_EXTRA_BYTES) as usize
    );

    let is_not_all_zero = result_vec.iter().any(|&x| x != 0);
    assert!(
        is_not_all_zero,
        "The rendered image should not be all black."
    );

    // TODO: compare to an expected image here.
    put_image(
        &result_vec[..result_vec.len() - NUM_EXTRA_BYTES],
        100,
        100,
        "tests/fixtures/test_render_unit_square.png",
    ).expect("Failed to write image");


}

/// Helper function to compare two strings, ignoring newlines and leading/trailing whitespace on each line.
/// TODO: move to a common test utilities file.
fn assert_strings_equal_ignore_whitespace(actual: &str, expected: &str) {
    let actual_processed: String = actual
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect();
    let expected_processed: String = expected
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect();
    assert_eq!(actual_processed, expected_processed);
}

#[tokio::test]
async fn test_render_unit_square_vector() { // TODO: move to different file and run when lacks_gpu feature is enabled.
    let params = RenderParams {
        width: 100,
        height: 100,
        format: GraphicsFormat::Vector,
        svg_compression_enabled: false,
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

    let svg_string = String::from_utf8(result_vec).expect("Invalid UTF-8 in SVG output");

    println!("{}", svg_string);

    let expected_svg_str = r#"
        <g height="100" width="100">
            <clipPath id="my_scatterplot_layer_clip_path">
                <rect height="100" width="100" x="0" y="0"/>
            </clipPath>
            <g clip-path="url(#my_scatterplot_layer_clip_path)" transform="translate(0,0)">
                <rect fill="rgb(0, 0, 0)" height="20" opacity="1" width="20" x="-10" y="90"/>
                <rect fill="rgb(0, 0, 0)" height="20" opacity="1" width="20" x="90" y="90"/>
                <rect fill="rgb(0, 0, 0)" height="20" opacity="1" width="20" x="90" y="-10"/>
                <rect fill="rgb(0, 0, 0)" height="20" opacity="1" width="20" x="-10" y="-10"/>
            </g>
        </g>
    "#;
    assert_strings_equal_ignore_whitespace(&svg_string, expected_svg_str);
}

// TODO: performance tests with many elements, both raster and svg formats

// To compare svg to raster, render svg using resvg
// Reference: https://github.com/linebender/resvg/blob/9876cd45dd461ac3083f584cc83e66473a3061ef/crates/resvg/examples/minimal.rs#L27
// TODO: use dify for pixel-based diffing
// Reference: https://github.com/emilk/egui/blob/fa78d25564a5dbcb546ff6db0a9e14cb603ba03b/crates/egui_kittest/src/snapshot.rs#L484

// TODO: use kompari for pixel diffing instead?
// Reference: https://github.com/linebender/kompari
