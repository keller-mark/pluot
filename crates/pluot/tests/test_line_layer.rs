#![cfg(not(target_arch = "wasm32"))]

use std::sync::Arc;

mod test_utils;
use test_utils::render_and_check_both_snapshots;

use pluot::{
    RenderParams, LayerParams,
    AspectRatioMode, UnitsMode, MarginParams,
    CategoricalColormap, CategoricalParams, CategoricalCustomParams, ColorMode,
    QuantitativeParams, QuantitativeColormap,
    LineLayerParams, NumericData, SizeMode, OpacityMode, InstancedSizeParams, InstancedOpacityParams,
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
        stroke_width: Some(SizeMode::UniformSize(2.0)),
        stroke_width_unit_mode: UnitsMode::Pixels,
        stroke_opacity: None,
        model_matrix: None,
        stroke_color: Some(ColorMode::Categorical(CategoricalParams {
            codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3, 4, 5, 6, 7])),
            colormap: CategoricalColormap::Tableau10,
        })),
        source_position_x: NumericData::Float32(Arc::new(vec![0.0, 0.0, 1.0, 0.0, 1.0, 0.70, 1.00, 0.70])),
        source_position_y: NumericData::Float32(Arc::new(vec![0.0, 0.0, 0.0, 0.5, 0.5, 0.75, 0.50, 1.00])),
        target_position_x: NumericData::Float32(Arc::new(vec![1.0, 0.0, 1.0, 0.5, 0.5, 0.70, 1.00, 1.00])),
        target_position_y: NumericData::Float32(Arc::new(vec![0.0, 0.5, 0.5, 1.0, 1.0, 1.00, 1.00, 1.00])),
    }
}

// Helper: 8 lines forming a toy house with a chimney in a 100x100 pixel space
fn cross_lines_pixels() -> LineLayerParams {
    LineLayerParams {
        layer_id: "my_line_layer".to_string(),
        bounds: None,
        data_unit_mode_x: UnitsMode::Pixels,
        data_unit_mode_y: UnitsMode::Pixels,
        stroke_width: Some(SizeMode::UniformSize(2.0)),
        stroke_width_unit_mode: UnitsMode::Pixels,
        stroke_opacity: None,
        model_matrix: None,
        stroke_color: Some(ColorMode::Categorical(CategoricalParams {
            codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3, 4, 5, 6, 7])),
            colormap: CategoricalColormap::Tableau10,
        })),
        source_position_x: NumericData::Float32(Arc::new(vec![  0.0,  0.0, 100.0,  0.0, 100.0,  70.0, 100.0,  70.0])),
        source_position_y: NumericData::Float32(Arc::new(vec![  0.0,  0.0,   0.0, 50.0,  50.0,  75.0,  50.0, 100.0])),
        target_position_x: NumericData::Float32(Arc::new(vec![100.0,  0.0, 100.0, 50.0,  50.0,  70.0, 100.0, 100.0])),
        target_position_y: NumericData::Float32(Arc::new(vec![  0.0, 50.0,  50.0,100.0, 100.0, 100.0, 100.0, 100.0])),
    }
}

// Helper: 8 lines forming a toy house with a chimney in a [0,1]x[0,1] normalized
// space. Uses the same fractions as cross_lines_pixels()'s pixel coordinates
// divided by 100, so on a 100x100 canvas this renders identically to
// cross_lines_pixels() while remaining agnostic to the layer's actual pixel
// dimensions (unlike Pixels mode, the same params render the same
// *proportions* on any canvas size).
fn cross_lines_normalized() -> LineLayerParams {
    LineLayerParams {
        layer_id: "my_line_layer".to_string(),
        bounds: None,
        data_unit_mode_x: UnitsMode::Normalized,
        data_unit_mode_y: UnitsMode::Normalized,
        stroke_width: Some(SizeMode::UniformSize(2.0)),
        stroke_width_unit_mode: UnitsMode::Pixels,
        stroke_opacity: None,
        model_matrix: None,
        stroke_color: Some(ColorMode::Categorical(CategoricalParams {
            codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3, 4, 5, 6, 7])),
            colormap: CategoricalColormap::Tableau10,
        })),
        source_position_x: NumericData::Float32(Arc::new(vec![0.0, 0.0, 1.0, 0.0, 1.0, 0.70, 1.00, 0.70])),
        source_position_y: NumericData::Float32(Arc::new(vec![0.0, 0.0, 0.0, 0.5, 0.5, 0.75, 0.50, 1.00])),
        target_position_x: NumericData::Float32(Arc::new(vec![1.0, 0.0, 1.0, 0.5, 0.5, 0.70, 1.00, 1.00])),
        target_position_y: NumericData::Float32(Arc::new(vec![0.0, 0.5, 0.5, 1.0, 1.0, 1.00, 1.00, 1.00])),
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

// Helper: lines with x in [0,1] data space, y in [0,1] normalized space
fn cross_lines_data_x_normalized_y() -> LineLayerParams {
    LineLayerParams {
        data_unit_mode_x: UnitsMode::Data,
        data_unit_mode_y: UnitsMode::Normalized,
        source_position_x: NumericData::Float32(Arc::new(vec![0.0, 0.0, 0.5, 0.0, 0.5, 0.35, 0.5, 0.35])),
        source_position_y: NumericData::Float32(Arc::new(vec![0.0, 0.0, 0.0, 0.5, 0.5, 0.75, 0.5, 1.0])),
        target_position_x: NumericData::Float32(Arc::new(vec![0.5, 0.0, 0.5, 0.25, 0.25, 0.35, 0.5, 0.5])),
        target_position_y: NumericData::Float32(Arc::new(vec![0.0, 0.5, 0.5, 1.0, 1.0, 1.0, 1.0, 1.0])),
        ..cross_lines_data()
    }
}

// Helper: lines with x in [0,1] normalized space, y in [0,1] data space
fn cross_lines_normalized_x_data_y() -> LineLayerParams {
    LineLayerParams {
        data_unit_mode_x: UnitsMode::Normalized,
        data_unit_mode_y: UnitsMode::Data,
        source_position_x: NumericData::Float32(Arc::new(vec![0.0, 0.0, 1.0, 0.0, 1.0, 0.7, 1.0, 0.7])),
        source_position_y: NumericData::Float32(Arc::new(vec![0.0, 0.0, 0.0, 0.25, 0.25, 0.375, 0.25, 0.5])),
        target_position_x: NumericData::Float32(Arc::new(vec![1.0, 0.0, 1.0, 0.5, 0.5, 0.7, 1.0, 1.0])),
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

// Normalized units: on a 100x100 canvas this renders identically to the Pixels
// test above, since cross_lines_normalized() uses the same fractions (0.0/1.0,
// 0.70, etc.) that cross_lines_pixels() uses as absolute pixel values out of 100.
#[tokio::test]
async fn test_line_layer_square_contain_normalized_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(cross_lines_normalized()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_square_contain_normalized_units_no_margins").await;
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

// Normalized units on a wide canvas: unlike the Pixels test above (which needs
// its own position overrides rescaled to the 200px width), cross_lines_normalized()
// is reused completely unchanged from the square-canvas test, since its 0-1
// fractions are agnostic to the layer's actual pixel dimensions.
#[tokio::test]
async fn test_line_layer_wide_contain_normalized_units_no_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(cross_lines_normalized()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_wide_contain_normalized_units_no_margins").await;
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
            stroke_width: Some(SizeMode::UniformSize(10.0)),
            ..cross_lines_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_wide_contain_data_units_thick_line_width").await;
}

// Line width expressed in data-coordinate units: the width scales with the
// camera / aspect-ratio transform, unlike the pixel-unit default.
#[tokio::test]
async fn test_line_layer_square_contain_data_units_data_line_width() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(LineLayerParams {
            stroke_width: Some(SizeMode::UniformSize(0.05)),
            stroke_width_unit_mode: UnitsMode::Data,
            ..cross_lines_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_square_contain_data_units_data_line_width").await;
}

// Same data-unit line width on a wide canvas: with Contain the data axes scale
// uniformly, so the line width remains visually consistent.
#[tokio::test]
async fn test_line_layer_wide_contain_data_units_data_line_width() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(LineLayerParams {
            stroke_width: Some(SizeMode::UniformSize(0.05)),
            stroke_width_unit_mode: UnitsMode::Data,
            ..cross_lines_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_wide_contain_data_units_data_line_width").await;
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

// ── Stroke color modes ────────────────────────────────────────────────────────

#[tokio::test]
async fn test_line_layer_square_contain_data_units_quantitative_color() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(LineLayerParams {
            stroke_color: Some(ColorMode::Quantitative(QuantitativeParams {
                values: NumericData::Float32(Arc::new(vec![0.0, 0.14, 0.28, 0.43, 0.57, 0.71, 0.85, 1.0])),
                colormap: QuantitativeColormap::Viridis,
                reverse: false,
                domain: None,
            })),
            ..cross_lines_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_square_contain_data_units_quantitative_color").await;
}

#[tokio::test]
async fn test_line_layer_square_contain_data_units_categorical_custom_color() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(LineLayerParams {
            stroke_color: Some(ColorMode::CategoricalCustom(CategoricalCustomParams {
                values: NumericData::Int32(Arc::new(vec![0, 1, 2, 3, 0, 1, 2, 3])),
                colormap: vec![
                    (255, 0, 0),
                    (0, 200, 0),
                    (0, 0, 255),
                    (200, 200, 0),
                ],
            })),
            ..cross_lines_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_square_contain_data_units_categorical_custom_color").await;
}

// ── Instanced line width (SizeMode) ───────────────────────────────────────────
// SizeMode::InstancedSize supplies one width per line (uploaded to the GPU as
// a value texture), rather than a single UniformSize shared by all lines.

#[tokio::test]
async fn test_line_layer_square_contain_pixel_units_instanced_width() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(LineLayerParams {
            // One distinct width (in pixels) per line.
            stroke_width: Some(SizeMode::InstancedSize(InstancedSizeParams {
                values: NumericData::Float32(Arc::new(vec![1.0, 2.0, 4.0, 6.0, 8.0, 10.0, 12.0, 14.0])),
            })),
            ..cross_lines_pixels()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_square_contain_pixel_units_instanced_width").await;
}

// ── Instanced line opacity (OpacityMode) ──────────────────────────────────────
// OpacityMode::InstancedOpacity supplies one opacity per line (uploaded to the
// GPU as a value texture), rather than a single UniformOpacity shared by all.

#[tokio::test]
async fn test_line_layer_square_contain_pixel_units_instanced_opacity() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(LineLayerParams {
            // One distinct opacity per line.
            stroke_opacity: Some(OpacityMode::InstancedOpacity(InstancedOpacityParams {
                values: NumericData::Float32(Arc::new(vec![0.1, 0.25, 0.4, 0.55, 0.7, 0.85, 0.9, 1.0])),
            })),
            ..cross_lines_pixels()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_square_contain_pixel_units_instanced_opacity").await;
}
