#![cfg(not(target_arch = "wasm32"))]

use std::sync::Arc;

mod test_utils;
use test_utils::render_and_check_both_snapshots;

use pluot::{
    RenderParams, LayerParams,
    AspectRatioMode, UnitsMode, MarginParams,
    LineLayerParams, NumericData,
};

// For primitive layer tests, we always want to test the following cases (and combinations of them):
// - Square and non-square (wide and tall) aspect ratios
// - Each aspect ratio mode (ignore, contain, cover)
// - Both data and pixel data_unit_modes
// - With and without margins at the view level
// - With and without margins (bounds) at the layer level
// - Raster and vector (which the helper function already handles for us)
// - Layer-specific stuff
//   - For LineLayer, this includes testing different line widths and line width unit modes

// Helper: 8 lines forming a toy house with a chimney in a 1x1 data space
fn cross_lines_data() -> LineLayerParams {
    LineLayerParams {
        layer_id: "my_line_layer".to_string(),
        bounds: None,
        data_unit_mode_x: UnitsMode::Data,
        data_unit_mode_y: UnitsMode::Data,
        line_width: 2.0,
        line_width_unit_mode: UnitsMode::Pixels,
        model_matrix: None,
        source_position_x: NumericData::Float32(Arc::new(vec![0.0, 0.0, 1.0, 0.0, 1.0, 0.70, 1.00, 0.70])),
        source_position_y: NumericData::Float32(Arc::new(vec![0.0, 0.0, 0.0, 0.5, 0.5, 0.75, 0.50, 1.00])),
        target_position_x: NumericData::Float32(Arc::new(vec![1.0, 0.0, 1.0, 0.5, 0.5, 0.70, 1.00, 1.00])),
        target_position_y: NumericData::Float32(Arc::new(vec![0.0, 0.5, 0.5, 1.0, 1.0, 1.00, 1.00, 1.00])),
        labels_vec: Arc::new(vec![0, 1, 2, 3, 4, 5, 6, 7]),
    }
}

// Helper: 8 lines forming a toy house with a chimney in a 100x100 pixel space
fn cross_lines_pixels() -> LineLayerParams {
    LineLayerParams {
        layer_id: "my_line_layer".to_string(),
        bounds: None,
        data_unit_mode_x: UnitsMode::Pixels,
        data_unit_mode_y: UnitsMode::Pixels,
        line_width: 2.0,
        line_width_unit_mode: UnitsMode::Pixels,
        model_matrix: None,
        source_position_x: NumericData::Float32(Arc::new(vec![  0.0,  0.0, 100.0,  0.0, 100.0,  70.0, 100.0,  70.0])),
        source_position_y: NumericData::Float32(Arc::new(vec![  0.0,  0.0,   0.0, 50.0,  50.0,  75.0,  50.0, 100.0])),
        target_position_x: NumericData::Float32(Arc::new(vec![100.0,  0.0, 100.0, 50.0,  50.0,  70.0, 100.0, 100.0])),
        target_position_y: NumericData::Float32(Arc::new(vec![  0.0, 50.0,  50.0,100.0, 100.0, 100.0, 100.0, 100.0])),
        labels_vec: Arc::new(vec![0, 1, 2, 3, 4, 5, 6, 7]),
    }
}

// Helper: lines with x in [0,1] data space, y in 100px pixel space
fn cross_lines_data_x_pixel_y() -> LineLayerParams {
    LineLayerParams {
        data_unit_mode_x: UnitsMode::Data,
        data_unit_mode_y: UnitsMode::Pixels,
        source_position_x: NumericData::Float32(Arc::new(vec![0.0, 0.0, 0.5, 0.0, 0.5, 0.35, 0.5, 0.35])),
        source_position_y: NumericData::Float32(Arc::new(vec![0.0, 0.0, 0.0, 50.0, 50.0, 75.0, 50.0, 100.0])),
        target_position_x: NumericData::Float32(Arc::new(vec![0.5, 0.0, 0.5, 0.25, 0.25, 0.35, 0.5, 0.5])),
        target_position_y: NumericData::Float32(Arc::new(vec![0.0, 50.0, 50.0, 100.0, 100.0, 100.0, 100.0, 100.0])),
        ..cross_lines_data()
    }
}

// Helper: lines with x in 100px pixel space, y in [0,1] data space
fn cross_lines_pixel_x_data_y() -> LineLayerParams {
    LineLayerParams {
        data_unit_mode_x: UnitsMode::Pixels,
        data_unit_mode_y: UnitsMode::Data,
        source_position_x: NumericData::Float32(Arc::new(vec![0.0, 0.0, 100.0, 0.0, 100.0, 70.0, 100.0, 70.0])),
        source_position_y: NumericData::Float32(Arc::new(vec![0.0, 0.0, 0.0, 0.25, 0.25, 0.375, 0.25, 0.5])),
        target_position_x: NumericData::Float32(Arc::new(vec![100.0, 0.0, 100.0, 50.0, 50.0, 70.0, 100.0, 100.0])),
        target_position_y: NumericData::Float32(Arc::new(vec![0.0, 0.25, 0.25, 0.5, 0.5, 0.5, 0.5, 0.5])),
        ..cross_lines_data()
    }
}

fn layer_params(line_params: LineLayerParams) -> Vec<LayerParams> {
    vec![LayerParams::LineLayer(line_params)]
}

// ── Square canvas (100x100) ───────────────────────────────────────────────────

#[tokio::test]
async fn test_line_layer_square_contain_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(LineLayerParams {
            bounds: Some(MarginParams {
                margin_left: Some(0.0),
                margin_right: Some(0.0),
                margin_top: Some(0.0),
                margin_bottom: Some(0.0),
            }),
            ..cross_lines_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_square_contain_data_units_no_margins").await;
}

#[tokio::test]
async fn test_line_layer_square_ignore_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(cross_lines_data()),
        aspect_ratio_mode: AspectRatioMode::Ignore,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_square_ignore_data_units_no_margins").await;
}

#[tokio::test]
async fn test_line_layer_square_cover_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(cross_lines_data()),
        aspect_ratio_mode: AspectRatioMode::Cover,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_square_cover_data_units_no_margins").await;
}

#[tokio::test]
async fn test_line_layer_square_contain_pixel_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(cross_lines_pixels()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_square_contain_pixel_units_no_margins").await;
}

#[tokio::test]
async fn test_line_layer_square_contain_data_units_view_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(cross_lines_data()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        margin_left: Some(10.0),
        margin_right: Some(10.0),
        margin_top: Some(10.0),
        margin_bottom: Some(10.0),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_square_contain_data_units_view_margins").await;
}

#[tokio::test]
async fn test_line_layer_square_contain_data_units_layer_bounds() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(LineLayerParams {
            bounds: Some(MarginParams {
                margin_left: Some(10.0),
                margin_right: Some(10.0),
                margin_top: Some(10.0),
                margin_bottom: Some(10.0),
            }),
            ..cross_lines_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_square_contain_data_units_layer_bounds").await;
}

// Layer bounds take precedence over view margins when both are set
#[tokio::test]
async fn test_line_layer_square_contain_data_units_layer_bounds_overrides_view_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(LineLayerParams {
            bounds: Some(MarginParams {
                margin_left: Some(10.0),
                margin_right: Some(10.0),
                margin_top: Some(10.0),
                margin_bottom: Some(10.0),
            }),
            ..cross_lines_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        margin_left: Some(20.0),
        margin_right: Some(20.0),
        margin_top: Some(20.0),
        margin_bottom: Some(20.0),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_square_contain_data_units_layer_bounds_overrides_view_margins").await;
}

// Wide canvas (200x100)

#[tokio::test]
async fn test_line_layer_wide_ignore_data_units_no_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(cross_lines_data()),
        aspect_ratio_mode: AspectRatioMode::Ignore,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_wide_ignore_data_units_no_margins").await;
}

#[tokio::test]
async fn test_line_layer_wide_contain_data_units_no_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(cross_lines_data()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_wide_contain_data_units_no_margins").await;
}

#[tokio::test]
async fn test_line_layer_wide_cover_data_units_no_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(cross_lines_data()),
        aspect_ratio_mode: AspectRatioMode::Cover,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_wide_cover_data_units_no_margins").await;
}

#[tokio::test]
async fn test_line_layer_wide_contain_pixel_units_no_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(LineLayerParams {
            source_position_x: NumericData::Float32(Arc::new(vec![  0.0,  0.0, 200.0,   0.0, 200.0, 140.0, 200.0, 140.0])),
            source_position_y: NumericData::Float32(Arc::new(vec![  0.0,  0.0,   0.0,  50.0,  50.0,  75.0,  75.0, 100.0])),
            target_position_x: NumericData::Float32(Arc::new(vec![200.0,  0.0, 200.0, 100.0, 100.0, 140.0, 200.0, 200.0])),
            target_position_y: NumericData::Float32(Arc::new(vec![  0.0, 50.0,  50.0, 100.0, 100.0, 100.0, 100.0, 100.0])),
            ..cross_lines_pixels()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_wide_contain_pixel_units_no_margins").await;
}

#[tokio::test]
async fn test_line_layer_wide_contain_data_units_view_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(cross_lines_data()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        margin_left: Some(10.0),
        margin_right: Some(10.0),
        margin_top: Some(10.0),
        margin_bottom: Some(10.0),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_wide_contain_data_units_view_margins").await;
}

#[tokio::test]
async fn test_line_layer_wide_contain_data_units_layer_bounds() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(LineLayerParams {
            bounds: Some(MarginParams {
                margin_left: Some(10.0),
                margin_right: Some(10.0),
                margin_top: Some(10.0),
                margin_bottom: Some(10.0),
            }),
            ..cross_lines_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_wide_contain_data_units_layer_bounds").await;
}

// Tall canvas (100x200)

#[tokio::test]
async fn test_line_layer_tall_ignore_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(cross_lines_data()),
        aspect_ratio_mode: AspectRatioMode::Ignore,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_tall_ignore_data_units_no_margins").await;
}

#[tokio::test]
async fn test_line_layer_tall_contain_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(cross_lines_data()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_tall_contain_data_units_no_margins").await;
}

#[tokio::test]
async fn test_line_layer_tall_cover_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(cross_lines_data()),
        aspect_ratio_mode: AspectRatioMode::Cover,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_tall_cover_data_units_no_margins").await;
}

#[tokio::test]
async fn test_line_layer_tall_contain_pixel_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(LineLayerParams {
            source_position_x: NumericData::Float32(Arc::new(vec![  0.0,  0.0, 100.0,  0.0, 100.0,  70.0, 100.0,  70.0])),
            source_position_y: NumericData::Float32(Arc::new(vec![  0.0,  0.0,   0.0,100.0, 100.0, 150.0, 150.0, 200.0])),
            target_position_x: NumericData::Float32(Arc::new(vec![100.0,  0.0, 100.0, 50.0,  50.0,  70.0, 100.0, 100.0])),
            target_position_y: NumericData::Float32(Arc::new(vec![  0.0,100.0, 100.0,200.0, 200.0, 200.0, 200.0, 200.0])),
            ..cross_lines_pixels()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_tall_contain_pixel_units_no_margins").await;
}

#[tokio::test]
async fn test_line_layer_tall_contain_data_units_view_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(cross_lines_data()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        margin_left: Some(10.0),
        margin_right: Some(10.0),
        margin_top: Some(10.0),
        margin_bottom: Some(10.0),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_tall_contain_data_units_view_margins").await;
}

#[tokio::test]
async fn test_line_layer_tall_contain_data_units_layer_bounds() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(LineLayerParams {
            bounds: Some(MarginParams {
                margin_left: Some(10.0),
                margin_right: Some(10.0),
                margin_top: Some(10.0),
                margin_bottom: Some(10.0),
            }),
            ..cross_lines_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_tall_contain_data_units_layer_bounds").await;
}

// Line width tests

#[tokio::test]
async fn test_line_layer_wide_contain_data_units_thick_line_width() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(LineLayerParams {
            line_width: 10.0,
            ..cross_lines_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_wide_contain_data_units_thick_line_width").await;
}

// ── Mixed unit modes (data_unit_mode_x ≠ data_unit_mode_y) ───────────────────

#[tokio::test]
async fn test_line_layer_square_contain_data_x_pixel_y_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(cross_lines_data_x_pixel_y()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_square_contain_data_x_pixel_y_no_margins").await;
}

#[tokio::test]
async fn test_line_layer_square_contain_pixel_x_data_y_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(cross_lines_pixel_x_data_y()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_square_contain_pixel_x_data_y_no_margins").await;
}

// model_matrix

// Scale 0.5 in data mode: lines shrink to lower-left quadrant of the unit square.
#[tokio::test]
async fn test_line_layer_square_contain_data_units_model_matrix_scale() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(LineLayerParams {
            model_matrix: Some([
                0.5, 0.0, 0.0, 0.0,
                0.0, 0.5, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                0.0, 0.0, 0.0, 1.0,
            ]),
            ..cross_lines_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_square_contain_data_units_model_matrix_scale").await;
}

// Translate +0.25 in data mode: lines shift toward upper-right.
#[tokio::test]
async fn test_line_layer_square_contain_data_units_model_matrix_translate() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(LineLayerParams {
            model_matrix: Some([
                1.0,  0.0,  0.0, 0.0,
                0.0,  1.0,  0.0, 0.0,
                0.0,  0.0,  1.0, 0.0,
                0.25, 0.25, 0.0, 1.0,
            ]),
            ..cross_lines_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_square_contain_data_units_model_matrix_translate").await;
}

// Scale 0.5 in pixel mode: model_matrix operates in normalized [0,1] space.
#[tokio::test]
async fn test_line_layer_square_contain_pixel_units_model_matrix_scale() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(LineLayerParams {
            model_matrix: Some([
                0.5, 0.0, 0.0, 0.0,
                0.0, 0.5, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                0.0, 0.0, 0.0, 1.0,
            ]),
            ..cross_lines_pixels()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_square_contain_pixel_units_model_matrix_scale").await;
}
