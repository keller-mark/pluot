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
    EmphasisCriteria, CategoricalCriteriaParams, QuantitativeCriteriaParams,
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
        ..Default::default()
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
        ..Default::default()
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
        ..Default::default()
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

// Normalized units on a tall canvas: again reusing cross_lines_normalized()
// unchanged, demonstrating pixel-dimension independence.
#[tokio::test]
async fn test_line_layer_tall_contain_normalized_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(cross_lines_normalized()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_tall_contain_normalized_units_no_margins").await;
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

#[tokio::test]
async fn test_line_layer_square_contain_data_x_normalized_y_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(cross_lines_data_x_normalized_y()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_square_contain_data_x_normalized_y_no_margins").await;
}

#[tokio::test]
async fn test_line_layer_square_contain_normalized_x_data_y_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(cross_lines_normalized_x_data_y()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_square_contain_normalized_x_data_y_no_margins").await;
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

// Scale 0.5 in normalized mode: like pixel mode, model_matrix operates in
// normalized [0,1] space, so this should render identically to the pixel-mode
// model-matrix-scale test above (on a 100x100 canvas, where they coincide).
#[tokio::test]
async fn test_line_layer_square_contain_normalized_units_model_matrix_scale() {
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
            ..cross_lines_normalized()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_square_contain_normalized_units_model_matrix_scale").await;
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

// ── stroke_width_unit_mode: Normalized ────────────────────────────────────────
//
// Normalized stroke width is a fraction (0 to 1) of the layer height,
// independent of the camera. 0.02 * 100px == 2px, matching the 2px line width
// used by cross_lines_normalized()'s default (Pixels) stroke width above.
#[tokio::test]
async fn test_line_layer_square_contain_normalized_units_line_width_normalized_mode() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(LineLayerParams {
            stroke_width: Some(SizeMode::UniformSize(0.02)),
            stroke_width_unit_mode: UnitsMode::Normalized,
            ..cross_lines_normalized()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_square_contain_normalized_units_line_width_normalized_mode").await;
}

// Same normalized stroke width (0.02) on a taller (100x200) canvas: since it is
// height-relative, the line renders at 0.02 * 200px == 4px, twice as thick as
// the square-canvas test above, demonstrating the height-relative scaling.
#[tokio::test]
async fn test_line_layer_tall_contain_normalized_units_line_width_normalized_mode() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(LineLayerParams {
            stroke_width: Some(SizeMode::UniformSize(0.02)),
            stroke_width_unit_mode: UnitsMode::Normalized,
            ..cross_lines_normalized()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_tall_contain_normalized_units_line_width_normalized_mode").await;
}

// ── Filtering and selection criteria ─────────────────────────────────────────
// Filter-excluded lines are not rendered at all; filter-included but
// selection-excluded ("background") lines still render, but re-colored with
// `background_stroke_color` in place of their configured stroke color.

// Categorical filtering: only lines whose category code is in
// `included_codes` are rendered at all. Reuses the same codes as
// `stroke_color` (0-7, one per line of the house shape), including only the
// even-numbered lines, so only half the house renders.
#[tokio::test]
async fn test_line_layer_square_contain_filtering_categorical_subset() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(LineLayerParams {
            filtering_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3, 4, 5, 6, 7])),
                included_codes: vec![0, 2, 4, 6],
            })],
            ..cross_lines_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_square_contain_filtering_categorical_subset").await;
}

// An explicit empty `included_codes` list means nothing is included: no
// lines render at all (distinct from an empty `filtering_criteria` list,
// which includes everything).
#[tokio::test]
async fn test_line_layer_square_contain_filtering_categorical_empty_excludes_all() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(LineLayerParams {
            filtering_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3, 4, 5, 6, 7])),
                included_codes: vec![],
            })],
            ..cross_lines_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_square_contain_filtering_categorical_empty_excludes_all").await;
}

// Quantitative filtering with both a min and a max bound: a per-line value
// column of [0..7] filtered to the inclusive range [2, 5] includes only
// lines 2 through 5.
#[tokio::test]
async fn test_line_layer_square_contain_filtering_quantitative_range() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(LineLayerParams {
            filtering_criteria: vec![EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
                values: NumericData::Float32(Arc::new(vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0])),
                min: Some(2.0),
                max: Some(5.0),
                min_exclusive: None,
                max_exclusive: None,
            })],
            ..cross_lines_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_square_contain_filtering_quantitative_range").await;
}

// Quantitative filtering with only a `min` bound: `max` is omitted, meaning
// +infinity, so every line with value >= 4 is included (lines 4-7).
#[tokio::test]
async fn test_line_layer_square_contain_filtering_quantitative_min_only() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(LineLayerParams {
            filtering_criteria: vec![EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
                values: NumericData::Float32(Arc::new(vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0])),
                min: Some(4.0),
                max: None,
                min_exclusive: None,
                max_exclusive: None,
            })],
            ..cross_lines_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_square_contain_filtering_quantitative_min_only").await;
}

// Categorical selection: unlike filtering, selection-excluded lines still
// render (the full house shape is visible), but lines whose code is not in
// `included_codes` are re-colored with `background_stroke_color` instead of
// their categorical `stroke_color`.
#[tokio::test]
async fn test_line_layer_square_contain_selection_categorical_subset() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(LineLayerParams {
            selection_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3, 4, 5, 6, 7])),
                included_codes: vec![0, 2, 4, 6],
            })],
            ..cross_lines_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_square_contain_selection_categorical_subset").await;
}

// An explicit empty `included_codes` list for selection means nothing is
// selected: all 8 lines still render (filtering_criteria is empty), but
// every one is de-emphasized with `background_stroke_color`.
#[tokio::test]
async fn test_line_layer_square_contain_selection_categorical_empty_deemphasizes_all() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(LineLayerParams {
            selection_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3, 4, 5, 6, 7])),
                included_codes: vec![],
            })],
            ..cross_lines_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_square_contain_selection_categorical_empty_deemphasizes_all").await;
}

// Quantitative selection: a value column of [0,10,...,70] selected to the
// range [20, 50] renders lines 2-5 with their normal stroke color and
// de-emphasizes the rest with `background_stroke_color`.
#[tokio::test]
async fn test_line_layer_square_contain_selection_quantitative_range() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(LineLayerParams {
            selection_criteria: vec![EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
                values: NumericData::Float32(Arc::new(vec![0.0, 10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0])),
                min: Some(20.0),
                max: Some(50.0),
                min_exclusive: None,
                max_exclusive: None,
            })],
            ..cross_lines_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_square_contain_selection_quantitative_range").await;
}

// Selection criteria may be entirely orthogonal to filtering criteria: here
// filtering uses the same categorical codes as `stroke_color` (excluding line
// 7, so the last line is not rendered at all), while selection uses an
// unrelated quantitative column. Of the 7 filter-included lines, only the
// ones with value >= 15 are selected (normal color); the rest are
// filter-included but selection-excluded (background color).
#[tokio::test]
async fn test_line_layer_square_contain_selection_orthogonal_to_filtering() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(LineLayerParams {
            filtering_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3, 4, 5, 6, 7])),
                included_codes: vec![0, 1, 2, 3, 4, 5, 6],
            })],
            selection_criteria: vec![EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
                values: NumericData::Float32(Arc::new(vec![5.0, 8.0, 25.0, 3.0, 18.0, 2.0, 30.0, 1.0])),
                min: Some(15.0),
                max: None,
                min_exclusive: None,
                max_exclusive: None,
            })],
            ..cross_lines_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_square_contain_selection_orthogonal_to_filtering").await;
}

// `filtering_criteria` is a list of criteria AND-ed together: a line must
// satisfy every one to be included. Here a categorical criteria (excluding
// line 7) is combined with a quantitative criteria (min 4, excluding lines
// 0-3). Only lines 4-6 satisfy both.
#[tokio::test]
async fn test_line_layer_square_contain_filtering_multiple_criteria_and() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(LineLayerParams {
            filtering_criteria: vec![
                EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                    codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3, 4, 5, 6, 7])),
                    included_codes: vec![0, 1, 2, 3, 4, 5, 6],
                }),
                EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
                    values: NumericData::Float32(Arc::new(vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0])),
                    min: Some(4.0),
                    max: None,
                    min_exclusive: None,
                    max_exclusive: None,
                }),
            ],
            ..cross_lines_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_square_contain_filtering_multiple_criteria_and").await;
}

// `selection_criteria` AND-ing mirrors `filtering_criteria`: a categorical
// criteria combined with a quantitative criteria narrows the selected set to
// lines 4-6; every other line still renders (no filtering), but
// de-emphasized with `background_stroke_color`.
#[tokio::test]
async fn test_line_layer_square_contain_selection_multiple_criteria_and() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(LineLayerParams {
            selection_criteria: vec![
                EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                    codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3, 4, 5, 6, 7])),
                    included_codes: vec![0, 1, 2, 3, 4, 5, 6],
                }),
                EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
                    values: NumericData::Float32(Arc::new(vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0])),
                    min: Some(4.0),
                    max: None,
                    min_exclusive: None,
                    max_exclusive: None,
                }),
            ],
            ..cross_lines_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_square_contain_selection_multiple_criteria_and").await;
}

// Custom background stroke color: lines 0, 2, 4, 6 are selected (normal
// categorical stroke color); the rest are selection-excluded and rendered
// with a magenta background stroke instead.
#[tokio::test]
async fn test_line_layer_square_contain_selection_custom_background_color() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(LineLayerParams {
            background_stroke_color: Some((255, 0, 255)),
            selection_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3, 4, 5, 6, 7])),
                included_codes: vec![0, 2, 4, 6],
            })],
            ..cross_lines_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_square_contain_selection_custom_background_color").await;
}

// ── Background stroke opacity and stroke width overrides ────────────────────
// `enable_background_*` flags gate whether a filter-included, selection-
// excluded ("background") line uses the corresponding `background_*`
// override in place of its normal stroke color, opacity, or width. Unlike
// `background_stroke_color` (which falls back to a default gray when unset),
// the opacity/width overrides are a no-op when left `None`, even if their
// `enable_background_*` flag is set.

// `enable_background_stroke_color: false` disables the (otherwise default-on)
// stroke-color de-emphasis: every line keeps its normal categorical stroke
// color even though lines 1, 3, 5, 7 are selection-excluded.
#[tokio::test]
async fn test_line_layer_square_contain_selection_disable_background_stroke_color() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(LineLayerParams {
            enable_background_stroke_color: false,
            selection_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3, 4, 5, 6, 7])),
                included_codes: vec![0, 2, 4, 6],
            })],
            ..cross_lines_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_square_contain_selection_disable_background_stroke_color").await;
}

// `background_stroke_opacity` + `enable_background_stroke_opacity`: the
// selection-excluded lines render at 0.15 stroke opacity instead of the
// default 1.0, while the selected lines stay fully opaque.
#[tokio::test]
async fn test_line_layer_square_contain_selection_background_stroke_opacity() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(LineLayerParams {
            enable_background_stroke_color: false,
            background_stroke_opacity: Some(0.15),
            enable_background_stroke_opacity: true,
            selection_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3, 4, 5, 6, 7])),
                included_codes: vec![0, 2, 4, 6],
            })],
            ..cross_lines_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_square_contain_selection_background_stroke_opacity").await;
}

// `background_stroke_width` + `enable_background_stroke_width`: the
// selection-excluded lines render 6px thick instead of the layer's 2px
// `stroke_width`, while the selected lines stay at 2px.
#[tokio::test]
async fn test_line_layer_square_contain_selection_background_stroke_width() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(LineLayerParams {
            enable_background_stroke_color: false,
            background_stroke_width: Some(6.0),
            enable_background_stroke_width: true,
            selection_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3, 4, 5, 6, 7])),
                included_codes: vec![0, 2, 4, 6],
            })],
            ..cross_lines_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_square_contain_selection_background_stroke_width").await;
}

// Enabling a background override with its value left `None` is a no-op
// (falls back to the normal foreground value), unlike
// `background_stroke_color`, which falls back to a default gray. This should
// render identically to eight normal, undifferentiated lines despite
// selection excluding half of them and every scalar override flag being on.
#[tokio::test]
async fn test_line_layer_square_contain_selection_background_overrides_none_value_is_noop() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(LineLayerParams {
            enable_background_stroke_color: false,
            enable_background_stroke_opacity: true,
            enable_background_stroke_width: true,
            selection_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3, 4, 5, 6, 7])),
                included_codes: vec![0, 2, 4, 6],
            })],
            ..cross_lines_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_line_layer_square_contain_selection_background_overrides_none_value_is_noop").await;
}
