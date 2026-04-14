use std::sync::Arc;

mod test_utils;
use test_utils::render_and_check_both_snapshots;

use pluot_core::params::{RenderParams, PlotParams, LayerParams, LayeredPlotRenderParams};
use pluot_core::render_traits::{AspectRatioMode, UnitsMode, MarginParams};
use pluot_core::layers::rect_layer::RectLayerParams;

// For primitive layer tests, we always want to test the following cases (and combinations of them):
// - Square and non-square (wide and tall) aspect ratios
// - Each aspect ratio mode (ignore, contain, cover)
// - Both data and pixel data_unit_modes
// - With and without margins at the view level
// - With and without margins (bounds) at the layer level
// - Raster and vector (which the helper function already handles for us)
// - Layer-specific stuff
//   - For RectLayer, this includes testing different stroke widths and stroke width unit modes

// Helper: 2 rects within [0,1]x[0,1] in data space
fn corner_rects_data() -> RectLayerParams {
    RectLayerParams {
        layer_id: "my_rect_layer".to_string(),
        bounds: None,
        data_unit_mode_x: UnitsMode::Data,
        data_unit_mode_y: UnitsMode::Data,
        stroke_width: 2.0,
        stroke_width_unit_mode: UnitsMode::Pixels,
        position_x0: Arc::new(vec![0.0, 0.5]),
        position_y0: Arc::new(vec![0.0, 0.5]),
        position_x1: Arc::new(vec![0.4, 1.0]),
        position_y1: Arc::new(vec![0.4, 1.0]),
        labels_vec: Arc::new(vec![0, 1]),
    }
}

// Helper: 2 rects within a 100x100 pixel space
fn corner_rects_pixels() -> RectLayerParams {
    RectLayerParams {
        layer_id: "my_rect_layer".to_string(),
        bounds: None,
        data_unit_mode_x: UnitsMode::Pixels,
        data_unit_mode_y: UnitsMode::Pixels,
        stroke_width: 2.0,
        stroke_width_unit_mode: UnitsMode::Pixels,
        position_x0: Arc::new(vec![0.0, 50.0]),
        position_y0: Arc::new(vec![0.0, 50.0]),
        position_x1: Arc::new(vec![40.0, 100.0]),
        position_y1: Arc::new(vec![40.0, 100.0]),
        labels_vec: Arc::new(vec![0, 1]),
    }
}

// Helper: 2 rects — x in [0,1] data space, y in 100px pixel space
fn corner_rects_data_x_pixel_y() -> RectLayerParams {
    RectLayerParams {
        layer_id: "my_rect_layer".to_string(),
        bounds: None,
        data_unit_mode_x: UnitsMode::Data,
        data_unit_mode_y: UnitsMode::Pixels,
        stroke_width: 2.0,
        stroke_width_unit_mode: UnitsMode::Pixels,
        position_x0: Arc::new(vec![0.0, 0.5]),
        position_y0: Arc::new(vec![0.0, 50.0]),
        position_x1: Arc::new(vec![0.4, 1.0]),
        position_y1: Arc::new(vec![40.0, 100.0]),
        labels_vec: Arc::new(vec![0, 1]),
    }
}

// Helper: 2 rects — x in 100px pixel space, y in [0,1] data space
fn corner_rects_pixel_x_data_y() -> RectLayerParams {
    RectLayerParams {
        layer_id: "my_rect_layer".to_string(),
        bounds: None,
        data_unit_mode_x: UnitsMode::Pixels,
        data_unit_mode_y: UnitsMode::Data,
        stroke_width: 2.0,
        stroke_width_unit_mode: UnitsMode::Pixels,
        position_x0: Arc::new(vec![0.0, 50.0]),
        position_y0: Arc::new(vec![0.0, 0.5]),
        position_x1: Arc::new(vec![40.0, 100.0]),
        position_y1: Arc::new(vec![0.4, 1.0]),
        labels_vec: Arc::new(vec![0, 1]),
    }
}

fn layer_params(rect_params: RectLayerParams) -> Vec<LayerParams> {
    vec![LayerParams {
        layer_type: "RectLayer".to_string(),
        layer_params: serde_json::to_value(rect_params).unwrap(),
    }]
}

// ── Square canvas (100×100) ───────────────────────────────────────────────────

#[tokio::test]
async fn test_rect_layer_square_contain_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(RectLayerParams {
                bounds: Some(MarginParams {
                    margin_left: Some(0.0),
                    margin_right: Some(0.0),
                    margin_top: Some(0.0),
                    margin_bottom: Some(0.0),
                }),
                ..corner_rects_data()
            }),
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_square_contain_data_units_no_margins").await;
}

#[tokio::test]
async fn test_rect_layer_square_ignore_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(corner_rects_data()),
        }),
        aspect_ratio_mode: AspectRatioMode::Ignore,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_square_ignore_data_units_no_margins").await;
}

#[tokio::test]
async fn test_rect_layer_square_cover_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(corner_rects_data()),
        }),
        aspect_ratio_mode: AspectRatioMode::Cover,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_square_cover_data_units_no_margins").await;
}

#[tokio::test]
async fn test_rect_layer_square_contain_pixel_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(corner_rects_pixels()),
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_square_contain_pixel_units_no_margins").await;
}

#[tokio::test]
async fn test_rect_layer_square_contain_data_units_view_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(corner_rects_data()),
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        margin_left: Some(10.0),
        margin_right: Some(10.0),
        margin_top: Some(10.0),
        margin_bottom: Some(10.0),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_square_contain_data_units_view_margins").await;
}

#[tokio::test]
async fn test_rect_layer_square_contain_data_units_layer_bounds() {
    let params = RenderParams {
        width: 100,
        height: 100,
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(RectLayerParams {
                bounds: Some(MarginParams {
                    margin_left: Some(10.0),
                    margin_right: Some(10.0),
                    margin_top: Some(10.0),
                    margin_bottom: Some(10.0),
                }),
                ..corner_rects_data()
            }),
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_square_contain_data_units_layer_bounds").await;
}

// Layer bounds take precedence over view margins when both are set
#[tokio::test]
async fn test_rect_layer_square_contain_data_units_layer_bounds_overrides_view_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(RectLayerParams {
                bounds: Some(MarginParams {
                    margin_left: Some(10.0),
                    margin_right: Some(10.0),
                    margin_top: Some(10.0),
                    margin_bottom: Some(10.0),
                }),
                ..corner_rects_data()
            }),
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        margin_left: Some(20.0),
        margin_right: Some(20.0),
        margin_top: Some(20.0),
        margin_bottom: Some(20.0),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_square_contain_data_units_layer_bounds_overrides_view_margins").await;
}

// ── Wide canvas (200×100) ─────────────────────────────────────────────────────

#[tokio::test]
async fn test_rect_layer_wide_ignore_data_units_no_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(corner_rects_data()),
        }),
        aspect_ratio_mode: AspectRatioMode::Ignore,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_wide_ignore_data_units_no_margins").await;
}

#[tokio::test]
async fn test_rect_layer_wide_contain_data_units_no_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(corner_rects_data()),
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_wide_contain_data_units_no_margins").await;
}

#[tokio::test]
async fn test_rect_layer_wide_cover_data_units_no_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(corner_rects_data()),
        }),
        aspect_ratio_mode: AspectRatioMode::Cover,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_wide_cover_data_units_no_margins").await;
}

#[tokio::test]
async fn test_rect_layer_wide_contain_pixel_units_no_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(RectLayerParams {
                position_x0: Arc::new(vec![0.0, 100.0]),
                position_y0: Arc::new(vec![0.0, 50.0]),
                position_x1: Arc::new(vec![80.0, 200.0]),
                position_y1: Arc::new(vec![40.0, 100.0]),
                ..corner_rects_pixels()
            }),
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_wide_contain_pixel_units_no_margins").await;
}

#[tokio::test]
async fn test_rect_layer_wide_contain_data_units_view_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(corner_rects_data()),
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        margin_left: Some(10.0),
        margin_right: Some(10.0),
        margin_top: Some(10.0),
        margin_bottom: Some(10.0),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_wide_contain_data_units_view_margins").await;
}

#[tokio::test]
async fn test_rect_layer_wide_contain_data_units_layer_bounds() {
    let params = RenderParams {
        width: 200,
        height: 100,
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(RectLayerParams {
                bounds: Some(MarginParams {
                    margin_left: Some(10.0),
                    margin_right: Some(10.0),
                    margin_top: Some(10.0),
                    margin_bottom: Some(10.0),
                }),
                ..corner_rects_data()
            }),
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_wide_contain_data_units_layer_bounds").await;
}

// ── Tall canvas (100×200) ─────────────────────────────────────────────────────

#[tokio::test]
async fn test_rect_layer_tall_ignore_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(corner_rects_data()),
        }),
        aspect_ratio_mode: AspectRatioMode::Ignore,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_tall_ignore_data_units_no_margins").await;
}

#[tokio::test]
async fn test_rect_layer_tall_contain_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(corner_rects_data()),
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_tall_contain_data_units_no_margins").await;
}

#[tokio::test]
async fn test_rect_layer_tall_cover_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(corner_rects_data()),
        }),
        aspect_ratio_mode: AspectRatioMode::Cover,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_tall_cover_data_units_no_margins").await;
}

#[tokio::test]
async fn test_rect_layer_tall_contain_pixel_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(RectLayerParams {
                position_x0: Arc::new(vec![0.0, 50.0]),
                position_y0: Arc::new(vec![0.0, 100.0]),
                position_x1: Arc::new(vec![40.0, 100.0]),
                position_y1: Arc::new(vec![80.0, 200.0]),
                ..corner_rects_pixels()
            }),
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_tall_contain_pixel_units_no_margins").await;
}

#[tokio::test]
async fn test_rect_layer_tall_contain_data_units_view_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(corner_rects_data()),
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        margin_left: Some(10.0),
        margin_right: Some(10.0),
        margin_top: Some(10.0),
        margin_bottom: Some(10.0),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_tall_contain_data_units_view_margins").await;
}

#[tokio::test]
async fn test_rect_layer_tall_contain_data_units_layer_bounds() {
    let params = RenderParams {
        width: 100,
        height: 200,
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(RectLayerParams {
                bounds: Some(MarginParams {
                    margin_left: Some(10.0),
                    margin_right: Some(10.0),
                    margin_top: Some(10.0),
                    margin_bottom: Some(10.0),
                }),
                ..corner_rects_data()
            }),
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_tall_contain_data_units_layer_bounds").await;
}

// ── Mixed unit modes (data_unit_mode_x ≠ data_unit_mode_y) ───────────────────

#[tokio::test]
async fn test_rect_layer_square_contain_data_x_pixel_y_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(corner_rects_data_x_pixel_y()),
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_square_contain_data_x_pixel_y_no_margins").await;
}

#[tokio::test]
async fn test_rect_layer_square_contain_pixel_x_data_y_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(corner_rects_pixel_x_data_y()),
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_square_contain_pixel_x_data_y_no_margins").await;
}
