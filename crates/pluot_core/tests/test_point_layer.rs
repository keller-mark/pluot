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

// Helper: 4 points at the corners of [0,1]x[0,1] in data space
fn corner_points_data() -> PointLayerParams {
    PointLayerParams {
        layer_id: "my_point_layer".to_string(),
        bounds: None,
        data_unit_mode: UnitsMode::Data,
        point_radius: 10.0,
        point_radius_unit_mode: UnitsMode::Pixels,
        point_shape_mode: PointShapeMode::Square,
        position_x: Arc::new(vec![0.0, 1.0, 1.0, 0.0]),
        position_y: Arc::new(vec![0.0, 0.0, 1.0, 1.0]),
        labels_vec: Arc::new(vec![0, 1, 2, 3]),
    }
}

// Helper: 4 points at the corners of a 100x100 pixel space
fn corner_points_pixels() -> PointLayerParams {
    PointLayerParams {
        layer_id: "my_point_layer".to_string(),
        bounds: None,
        data_unit_mode: UnitsMode::Pixels,
        point_radius: 10.0,
        point_radius_unit_mode: UnitsMode::Pixels,
        point_shape_mode: PointShapeMode::Square,
        position_x: Arc::new(vec![0.0, 100.0, 100.0, 0.0]),
        position_y: Arc::new(vec![0.0, 0.0, 100.0, 100.0]),
        labels_vec: Arc::new(vec![0, 1, 2, 3]),
    }
}

fn layer_params(point_params: PointLayerParams) -> Vec<LayerParams> {
    vec![LayerParams {
        layer_type: "PointLayer".to_string(),
        layer_params: serde_json::to_value(point_params).unwrap(),
    }]
}

// ── Square canvas (100×100) ───────────────────────────────────────────────────

#[tokio::test]
async fn test_point_layer_square_contain_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(PointLayerParams {
                bounds: Some(MarginParams {
                    margin_left: Some(0.0),
                    margin_right: Some(0.0),
                    margin_top: Some(0.0),
                    margin_bottom: Some(0.0),
                }),
                ..corner_points_data()
            }),
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_data_units_no_margins").await;
}

#[tokio::test]
async fn test_point_layer_square_ignore_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(corner_points_data()),
        }),
        aspect_ratio_mode: AspectRatioMode::Ignore,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_ignore_data_units_no_margins").await;
}

#[tokio::test]
async fn test_point_layer_square_cover_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(corner_points_data()),
        }),
        aspect_ratio_mode: AspectRatioMode::Cover,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_cover_data_units_no_margins").await;
}

#[tokio::test]
async fn test_point_layer_square_contain_pixel_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(corner_points_pixels()),
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_pixel_units_no_margins").await;
}

#[tokio::test]
async fn test_point_layer_square_contain_data_units_view_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(corner_points_data()),
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        margin_left: Some(10.0),
        margin_right: Some(10.0),
        margin_top: Some(10.0),
        margin_bottom: Some(10.0),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_data_units_view_margins").await;
}

#[tokio::test]
async fn test_point_layer_square_contain_data_units_layer_bounds() {
    let params = RenderParams {
        width: 100,
        height: 100,
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(PointLayerParams {
                bounds: Some(MarginParams {
                    margin_left: Some(10.0),
                    margin_right: Some(10.0),
                    margin_top: Some(10.0),
                    margin_bottom: Some(10.0),
                }),
                ..corner_points_data()
            }),
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_data_units_layer_bounds").await;
}

// Layer bounds take precedence over view margins when both are set
#[tokio::test]
async fn test_point_layer_square_contain_data_units_layer_bounds_overrides_view_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(PointLayerParams {
                bounds: Some(MarginParams {
                    margin_left: Some(10.0),
                    margin_right: Some(10.0),
                    margin_top: Some(10.0),
                    margin_bottom: Some(10.0),
                }),
                ..corner_points_data()
            }),
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        margin_left: Some(20.0),
        margin_right: Some(20.0),
        margin_top: Some(20.0),
        margin_bottom: Some(20.0),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_data_units_layer_bounds_overrides_view_margins").await;
}

// ── Wide canvas (200×100) ─────────────────────────────────────────────────────

#[tokio::test]
async fn test_point_layer_wide_ignore_data_units_no_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(corner_points_data()),
        }),
        aspect_ratio_mode: AspectRatioMode::Ignore,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_wide_ignore_data_units_no_margins").await;
}

#[tokio::test]
async fn test_point_layer_wide_contain_data_units_no_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(corner_points_data()),
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_wide_contain_data_units_no_margins").await;
}

#[tokio::test]
async fn test_point_layer_wide_cover_data_units_no_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(corner_points_data()),
        }),
        aspect_ratio_mode: AspectRatioMode::Cover,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_wide_cover_data_units_no_margins").await;
}

#[tokio::test]
async fn test_point_layer_wide_contain_pixel_units_no_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(PointLayerParams {
                position_x: Arc::new(vec![0.0, 200.0, 200.0, 0.0]),
                position_y: Arc::new(vec![0.0, 0.0, 100.0, 100.0]),
                ..corner_points_pixels()
            }),
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_wide_contain_pixel_units_no_margins").await;
}

#[tokio::test]
async fn test_point_layer_wide_contain_data_units_view_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(corner_points_data()),
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        margin_left: Some(10.0),
        margin_right: Some(10.0),
        margin_top: Some(10.0),
        margin_bottom: Some(10.0),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_wide_contain_data_units_view_margins").await;
}

#[tokio::test]
async fn test_point_layer_wide_contain_data_units_layer_bounds() {
    let params = RenderParams {
        width: 200,
        height: 100,
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(PointLayerParams {
                bounds: Some(MarginParams {
                    margin_left: Some(10.0),
                    margin_right: Some(10.0),
                    margin_top: Some(10.0),
                    margin_bottom: Some(10.0),
                }),
                ..corner_points_data()
            }),
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_wide_contain_data_units_layer_bounds").await;
}

// ── Tall canvas (100×200) ─────────────────────────────────────────────────────

#[tokio::test]
async fn test_point_layer_tall_ignore_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(corner_points_data()),
        }),
        aspect_ratio_mode: AspectRatioMode::Ignore,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_tall_ignore_data_units_no_margins").await;
}

#[tokio::test]
async fn test_point_layer_tall_contain_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(corner_points_data()),
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_tall_contain_data_units_no_margins").await;
}

#[tokio::test]
async fn test_point_layer_tall_cover_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(corner_points_data()),
        }),
        aspect_ratio_mode: AspectRatioMode::Cover,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_tall_cover_data_units_no_margins").await;
}

#[tokio::test]
async fn test_point_layer_tall_contain_pixel_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(PointLayerParams {
                position_x: Arc::new(vec![0.0, 100.0, 100.0, 0.0]),
                position_y: Arc::new(vec![0.0, 0.0, 200.0, 200.0]),
                ..corner_points_pixels()
            }),
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_tall_contain_pixel_units_no_margins").await;
}

#[tokio::test]
async fn test_point_layer_tall_contain_data_units_view_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(corner_points_data()),
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        margin_left: Some(10.0),
        margin_right: Some(10.0),
        margin_top: Some(10.0),
        margin_bottom: Some(10.0),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_tall_contain_data_units_view_margins").await;
}

#[tokio::test]
async fn test_point_layer_tall_contain_data_units_layer_bounds() {
    let params = RenderParams {
        width: 100,
        height: 200,
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(PointLayerParams {
                bounds: Some(MarginParams {
                    margin_left: Some(10.0),
                    margin_right: Some(10.0),
                    margin_top: Some(10.0),
                    margin_bottom: Some(10.0),
                }),
                ..corner_points_data()
            }),
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_tall_contain_data_units_layer_bounds").await;
}

// TODO: performance tests with many elements, both raster and svg formats

// To compare svg to raster, render svg using resvg
// Reference: https://github.com/linebender/resvg/blob/9876cd45dd461ac3083f584cc83e66473a3061ef/crates/resvg/examples/minimal.rs#L27
