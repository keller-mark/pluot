// We only run this test on non-WASM targets AND when the "lacks_gpu" feature is not enabled (e.g., CI).
#![cfg(all(not(target_arch = "wasm32"), not(feature="lacks_gpu")))]

use std::sync::Arc;

mod test_utils;
use test_utils::{render_and_check_raster_snapshot, render_and_check_svg_snapshot};

use pluot_core::params::{RenderParams, PlotParams, LayerParams, GraphicsFormat, LayeredPlotRenderParams, ViewMode};
use pluot_core::layer_traits::{AspectRatioMode, UnitsMode, ViewParams, MarginParams};
use pluot_core::layers::point_layer::{PointLayerParams, PointShapeMode};


#[tokio::test]
async fn test_render_unit_square_raster() {
    let params = RenderParams {
        width: 100,
        height: 100,
        format: GraphicsFormat::Raster,
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: vec![
                LayerParams {
                    layer_type: "PointLayer".to_string(),
                    layer_params: serde_json::to_value(PointLayerParams {
                        layer_id: "my_point_layer".to_string(),
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
                        position_x: Arc::new(vec![0.0, 1.0, 1.0, 0.0]),
                        position_y: Arc::new(vec![0.0, 0.0, 1.0, 1.0]),
                        labels_vec: Arc::new(vec![0, 1, 2, 3]),
                    }).unwrap(),
                },
            ],
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_raster_snapshot(params, "test_render_unit_square_raster.png").await;
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
                    layer_type: "PointLayer".to_string(),
                    layer_params: serde_json::to_value(PointLayerParams {
                        layer_id: "my_point_layer".to_string(),
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
                        position_x: Arc::new(vec![0.0, 1.0, 1.0, 0.0]),
                        position_y: Arc::new(vec![0.0, 0.0, 1.0, 1.0]),
                        labels_vec: Arc::new(vec![0, 1, 2, 3]),
                    }).unwrap(),
                },
            ],
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_svg_snapshot(params, "test_render_unit_square_vector.svg").await;
}

// TODO: performance tests with many elements, both raster and svg formats

// To compare svg to raster, render svg using resvg
// Reference: https://github.com/linebender/resvg/blob/9876cd45dd461ac3083f584cc83e66473a3061ef/crates/resvg/examples/minimal.rs#L27
