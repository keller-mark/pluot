use std::sync::Arc;

mod test_utils;
use test_utils::render_and_check_both_snapshots;

use pluot_core::params::{RenderParams, PlotParams, LayerParams, LayeredPlotRenderParams};
use pluot_core::layer_traits::{AspectRatioMode, UnitsMode, MarginParams};
use pluot_core::layers::point_layer::{PointLayerParams, PointShapeMode};

// For primitive layer tests, we always want to test the following cases (and combinations of them):
// - Square and non-square (wide and tall) aspect ratios
// - Each aspect ratio mode (ignore, contain, cover)
// - Both data and pixel data_unit_modes
// - With and without margins at the view level
// - With and without margins (bounds) at the layer level
// - Raster and vector (which the helper function already handles for us)

#[tokio::test]
async fn test_point_layer_square_contain_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
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
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_data_units_no_margins").await;
}

// TODO: performance tests with many elements, both raster and svg formats

// To compare svg to raster, render svg using resvg
// Reference: https://github.com/linebender/resvg/blob/9876cd45dd461ac3083f584cc83e66473a3061ef/crates/resvg/examples/minimal.rs#L27
