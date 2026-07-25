#![cfg(not(target_arch = "wasm32"))]

use std::sync::Arc;

mod test_utils;
use test_utils::render_and_check_both_snapshots;

use pluot::{
    AspectRatioMode, CategoricalColormap, CategoricalParams, CategoricalCustomParams, ColorMode,
    InstancedOpacityParams, InstancedSizeParams, LayerParams, MarginParams, OpacityMode,
    QuantitativeParams, QuantitativeColormap,
    RectLayerParams, RenderParams, SizeMode, UnitsMode, NumericData
};

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
        stroke_width: Some(SizeMode::UniformSize(2.0)),
        stroke_width_unit_mode: UnitsMode::Pixels,
        model_matrix: None,
        position_x0: NumericData::Float32(Arc::new(vec![0.0, 0.5])),
        position_y0: NumericData::Float32(Arc::new(vec![0.0, 0.5])),
        position_x1: NumericData::Float32(Arc::new(vec![0.4, 1.0])),
        position_y1: NumericData::Float32(Arc::new(vec![0.4, 1.0])),
        fill_color: Some(ColorMode::Categorical(CategoricalParams {
            codes: NumericData::Int32(Arc::new(vec![0, 1])),
            colormap: CategoricalColormap::Tableau10,
        })),
        ..Default::default()
    }
}

// Helper: 2 rects within a 100x100 pixel space
fn corner_rects_pixels() -> RectLayerParams {
    RectLayerParams {
        layer_id: "my_rect_layer".to_string(),
        bounds: None,
        data_unit_mode_x: UnitsMode::Pixels,
        data_unit_mode_y: UnitsMode::Pixels,
        stroke_width: Some(SizeMode::UniformSize(2.0)),
        stroke_width_unit_mode: UnitsMode::Pixels,
        model_matrix: None,
        position_x0: NumericData::Float32(Arc::new(vec![0.0, 50.0])),
        position_y0: NumericData::Float32(Arc::new(vec![0.0, 50.0])),
        position_x1: NumericData::Float32(Arc::new(vec![40.0, 100.0])),
        position_y1: NumericData::Float32(Arc::new(vec![40.0, 100.0])),
        fill_color: Some(ColorMode::Categorical(CategoricalParams {
            codes: NumericData::Int32(Arc::new(vec![0, 1])),
            colormap: CategoricalColormap::Tableau10,
        })),
        ..Default::default()
    }
}

// Helper: 2 rects. x in [0,1] data space, y in 100px pixel space
fn corner_rects_data_x_pixel_y() -> RectLayerParams {
    RectLayerParams {
        layer_id: "my_rect_layer".to_string(),
        bounds: None,
        data_unit_mode_x: UnitsMode::Data,
        data_unit_mode_y: UnitsMode::Pixels,
        stroke_width: Some(SizeMode::UniformSize(2.0)),
        stroke_width_unit_mode: UnitsMode::Pixels,
        model_matrix: None,
        position_x0: NumericData::Float32(Arc::new(vec![0.0, 0.5])),
        position_y0: NumericData::Float32(Arc::new(vec![0.0, 50.0])),
        position_x1: NumericData::Float32(Arc::new(vec![0.4, 1.0])),
        position_y1: NumericData::Float32(Arc::new(vec![40.0, 100.0])),
        fill_color: Some(ColorMode::Categorical(CategoricalParams {
            codes: NumericData::Int32(Arc::new(vec![0, 1])),
            colormap: CategoricalColormap::Tableau10,
        })),
        ..Default::default()
    }
}

// Helper: 2 rects within a [0,1]x[0,1] normalized space. Uses the same
// fractions as corner_rects_pixels() (0.0/0.4 and 0.5/1.0), so on a 100x100
// canvas this renders identically to corner_rects_pixels() while remaining
// agnostic to the layer's actual pixel dimensions (unlike Pixels mode, the
// same params render the same *proportions* on any canvas size).
fn corner_rects_normalized() -> RectLayerParams {
    RectLayerParams {
        layer_id: "my_rect_layer".to_string(),
        bounds: None,
        data_unit_mode_x: UnitsMode::Normalized,
        data_unit_mode_y: UnitsMode::Normalized,
        stroke_width: Some(SizeMode::UniformSize(2.0)),
        stroke_width_unit_mode: UnitsMode::Pixels,
        model_matrix: None,
        position_x0: NumericData::Float32(Arc::new(vec![0.0, 0.5])),
        position_y0: NumericData::Float32(Arc::new(vec![0.0, 0.5])),
        position_x1: NumericData::Float32(Arc::new(vec![0.4, 1.0])),
        position_y1: NumericData::Float32(Arc::new(vec![0.4, 1.0])),
        fill_color: Some(ColorMode::Categorical(CategoricalParams {
            codes: NumericData::Int32(Arc::new(vec![0, 1])),
            colormap: CategoricalColormap::Tableau10,
        })),
        ..Default::default()
    }
}

// Helper: 2 rects. x in [0,1] data space, y in [0,1] normalized space
fn corner_rects_data_x_normalized_y() -> RectLayerParams {
    RectLayerParams {
        layer_id: "my_rect_layer".to_string(),
        bounds: None,
        data_unit_mode_x: UnitsMode::Data,
        data_unit_mode_y: UnitsMode::Normalized,
        stroke_width: Some(SizeMode::UniformSize(2.0)),
        stroke_width_unit_mode: UnitsMode::Pixels,
        model_matrix: None,
        position_x0: NumericData::Float32(Arc::new(vec![0.0, 0.5])),
        position_y0: NumericData::Float32(Arc::new(vec![0.0, 0.5])),
        position_x1: NumericData::Float32(Arc::new(vec![0.4, 1.0])),
        position_y1: NumericData::Float32(Arc::new(vec![0.4, 1.0])),
        fill_color: Some(ColorMode::Categorical(CategoricalParams {
            codes: NumericData::Int32(Arc::new(vec![0, 1])),
            colormap: CategoricalColormap::Tableau10,
        })),
        ..Default::default()
    }
}

// Helper: 2 rects. x in [0,1] normalized space, y in [0,1] data space
fn corner_rects_normalized_x_data_y() -> RectLayerParams {
    RectLayerParams {
        layer_id: "my_rect_layer".to_string(),
        bounds: None,
        data_unit_mode_x: UnitsMode::Normalized,
        data_unit_mode_y: UnitsMode::Data,
        stroke_width: Some(SizeMode::UniformSize(2.0)),
        stroke_width_unit_mode: UnitsMode::Pixels,
        model_matrix: None,
        position_x0: NumericData::Float32(Arc::new(vec![0.0, 0.5])),
        position_y0: NumericData::Float32(Arc::new(vec![0.0, 0.5])),
        position_x1: NumericData::Float32(Arc::new(vec![0.4, 1.0])),
        position_y1: NumericData::Float32(Arc::new(vec![0.4, 1.0])),
        fill_color: Some(ColorMode::Categorical(CategoricalParams {
            codes: NumericData::Int32(Arc::new(vec![0, 1])),
            colormap: CategoricalColormap::Tableau10,
        })),
        ..Default::default()
    }
}

// Helper: 2 rects. x in 100px pixel space, y in [0,1] data space
fn corner_rects_pixel_x_data_y() -> RectLayerParams {
    RectLayerParams {
        layer_id: "my_rect_layer".to_string(),
        bounds: None,
        data_unit_mode_x: UnitsMode::Pixels,
        data_unit_mode_y: UnitsMode::Data,
        stroke_width: Some(SizeMode::UniformSize(2.0)),
        stroke_width_unit_mode: UnitsMode::Pixels,
        model_matrix: None,
        position_x0: NumericData::Float32(Arc::new(vec![0.0, 50.0])),
        position_y0: NumericData::Float32(Arc::new(vec![0.0, 0.5])),
        position_x1: NumericData::Float32(Arc::new(vec![40.0, 100.0])),
        position_y1: NumericData::Float32(Arc::new(vec![0.4, 1.0])),
        fill_color: Some(ColorMode::Categorical(CategoricalParams {
            codes: NumericData::Int32(Arc::new(vec![0, 1])),
            colormap: CategoricalColormap::Tableau10,
        })),
        ..Default::default()
    }
}

fn layer_params(rect_params: RectLayerParams) -> Vec<LayerParams> {
    vec![LayerParams::RectLayer(rect_params)]
}

// ── Square canvas (100x100) ───────────────────────────────────────────────────

#[tokio::test]
async fn test_rect_layer_square_contain_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(RectLayerParams {
            bounds: Some(MarginParams {
                margin_left: Some(0.0),
                margin_right: Some(0.0),
                margin_top: Some(0.0),
                margin_bottom: Some(0.0),
            }),
            ..corner_rects_data()
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
        layers: layer_params(corner_rects_data()),
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
        layers: layer_params(corner_rects_data()),
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
        layers: layer_params(corner_rects_pixels()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_square_contain_pixel_units_no_margins").await;
}

// Normalized units: on a 100x100 canvas this renders identically to the Pixels
// test above, since corner_rects_normalized() uses the same fractions (0.0/0.4,
// 0.5/1.0) that corner_rects_pixels() uses as absolute pixel values out of 100.
#[tokio::test]
async fn test_rect_layer_square_contain_normalized_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(corner_rects_normalized()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_square_contain_normalized_units_no_margins").await;
}

#[tokio::test]
async fn test_rect_layer_square_contain_data_units_view_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(corner_rects_data()),
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
        layers: layer_params(RectLayerParams {
            bounds: Some(MarginParams {
                margin_left: Some(10.0),
                margin_right: Some(10.0),
                margin_top: Some(10.0),
                margin_bottom: Some(10.0),
            }),
            ..corner_rects_data()
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
        layers: layer_params(RectLayerParams {
            bounds: Some(MarginParams {
                margin_left: Some(10.0),
                margin_right: Some(10.0),
                margin_top: Some(10.0),
                margin_bottom: Some(10.0),
            }),
            ..corner_rects_data()
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

// Wide canvas (200x100)

#[tokio::test]
async fn test_rect_layer_wide_ignore_data_units_no_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(corner_rects_data()),
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
        layers: layer_params(corner_rects_data()),
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
        layers: layer_params(corner_rects_data()),
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
        layers: layer_params(RectLayerParams {
            position_x0: NumericData::Float32(Arc::new(vec![0.0, 100.0])),
            position_y0: NumericData::Float32(Arc::new(vec![0.0, 50.0])),
            position_x1: NumericData::Float32(Arc::new(vec![80.0, 200.0])),
            position_y1: NumericData::Float32(Arc::new(vec![40.0, 100.0])),
            ..corner_rects_pixels()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_wide_contain_pixel_units_no_margins").await;
}

// Normalized units on a wide canvas: unlike the Pixels test above (which needs
// its own position overrides rescaled to the 200px width), corner_rects_normalized()
// is reused completely unchanged from the square-canvas test, since its 0-1
// fractions are agnostic to the layer's actual pixel dimensions.
#[tokio::test]
async fn test_rect_layer_wide_contain_normalized_units_no_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(corner_rects_normalized()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_wide_contain_normalized_units_no_margins").await;
}

#[tokio::test]
async fn test_rect_layer_wide_contain_data_units_view_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(corner_rects_data()),
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
        layers: layer_params(RectLayerParams {
            bounds: Some(MarginParams {
                margin_left: Some(10.0),
                margin_right: Some(10.0),
                margin_top: Some(10.0),
                margin_bottom: Some(10.0),
            }),
            ..corner_rects_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_wide_contain_data_units_layer_bounds").await;
}

// Tall canvas (100x200)

#[tokio::test]
async fn test_rect_layer_tall_ignore_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(corner_rects_data()),
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
        layers: layer_params(corner_rects_data()),
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
        layers: layer_params(corner_rects_data()),
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
        layers: layer_params(RectLayerParams {
            position_x0: NumericData::Float32(Arc::new(vec![0.0, 50.0])),
            position_y0: NumericData::Float32(Arc::new(vec![0.0, 100.0])),
            position_x1: NumericData::Float32(Arc::new(vec![40.0, 100.0])),
            position_y1: NumericData::Float32(Arc::new(vec![80.0, 200.0])),
            ..corner_rects_pixels()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_tall_contain_pixel_units_no_margins").await;
}

// Normalized units on a tall canvas: again reusing corner_rects_normalized()
// unchanged, demonstrating pixel-dimension independence.
#[tokio::test]
async fn test_rect_layer_tall_contain_normalized_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(corner_rects_normalized()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_tall_contain_normalized_units_no_margins").await;
}

#[tokio::test]
async fn test_rect_layer_tall_contain_data_units_view_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(corner_rects_data()),
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
        layers: layer_params(RectLayerParams {
            bounds: Some(MarginParams {
                margin_left: Some(10.0),
                margin_right: Some(10.0),
                margin_top: Some(10.0),
                margin_bottom: Some(10.0),
            }),
            ..corner_rects_data()
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
        layers: layer_params(corner_rects_data_x_pixel_y()),
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
        layers: layer_params(corner_rects_pixel_x_data_y()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_square_contain_pixel_x_data_y_no_margins").await;
}

#[tokio::test]
async fn test_rect_layer_square_contain_data_x_normalized_y_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(corner_rects_data_x_normalized_y()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_square_contain_data_x_normalized_y_no_margins").await;
}

#[tokio::test]
async fn test_rect_layer_square_contain_normalized_x_data_y_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(corner_rects_normalized_x_data_y()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_square_contain_normalized_x_data_y_no_margins").await;
}

// model_matrix

// Scale 0.5 in data mode: rects shrink to lower-left quadrant of the unit square.
#[tokio::test]
async fn test_rect_layer_square_contain_data_units_model_matrix_scale() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(RectLayerParams {
            model_matrix: Some([
                0.5, 0.0, 0.0, 0.0,
                0.0, 0.5, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                0.0, 0.0, 0.0, 1.0,
            ]),
            ..corner_rects_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_square_contain_data_units_model_matrix_scale").await;
}

// Translate +0.25 in data mode: rects shift toward upper-right.
#[tokio::test]
async fn test_rect_layer_square_contain_data_units_model_matrix_translate() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(RectLayerParams {
            model_matrix: Some([
                1.0,  0.0,  0.0, 0.0,
                0.0,  1.0,  0.0, 0.0,
                0.0,  0.0,  1.0, 0.0,
                0.25, 0.25, 0.0, 1.0,
            ]),
            ..corner_rects_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_square_contain_data_units_model_matrix_translate").await;
}

// Scale 0.5 in pixel mode: model_matrix operates in normalized [0,1] space.
#[tokio::test]
async fn test_rect_layer_square_contain_pixel_units_model_matrix_scale() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(RectLayerParams {
            model_matrix: Some([
                0.5, 0.0, 0.0, 0.0,
                0.0, 0.5, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                0.0, 0.0, 0.0, 1.0,
            ]),
            ..corner_rects_pixels()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_square_contain_pixel_units_model_matrix_scale").await;
}

// Scale 0.5 in normalized mode: like pixel mode, model_matrix operates in
// normalized [0,1] space, so this should render identically to the pixel-mode
// model-matrix-scale test above (on a 100x100 canvas, where they coincide).
#[tokio::test]
async fn test_rect_layer_square_contain_normalized_units_model_matrix_scale() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(RectLayerParams {
            model_matrix: Some([
                0.5, 0.0, 0.0, 0.0,
                0.0, 0.5, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                0.0, 0.0, 0.0, 1.0,
            ]),
            ..corner_rects_normalized()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_square_contain_normalized_units_model_matrix_scale").await;
}

// ── Fill color modes ──────────────────────────────────────────────────────────

#[tokio::test]
async fn test_rect_layer_square_contain_data_units_quantitative_color() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(RectLayerParams {
            fill_color: Some(ColorMode::Quantitative(QuantitativeParams {
                values: NumericData::Float32(Arc::new(vec![0.0, 1.0])),
                colormap: QuantitativeColormap::Viridis,
                reverse: false,
                domain: None,
            })),
            ..corner_rects_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_square_contain_data_units_quantitative_color").await;
}

#[tokio::test]
async fn test_rect_layer_square_contain_data_units_categorical_custom_color() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(RectLayerParams {
            fill_color: Some(ColorMode::CategoricalCustom(CategoricalCustomParams {
                values: NumericData::Int32(Arc::new(vec![0, 1])),
                colormap: vec![
                    (255, 0, 0),
                    (0, 0, 255),
                ],
            })),
            ..corner_rects_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_square_contain_data_units_categorical_custom_color").await;
}

// ── Stroke color / width and fill/stroke opacity ──────────────────────────────

// Uniform stroke color: filled rects with a solid red border.
#[tokio::test]
async fn test_rect_layer_square_contain_data_units_stroke_color() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(RectLayerParams {
            stroke_width: Some(SizeMode::UniformSize(4.0)),
            stroke_color: Some(ColorMode::UniformRgb((255, 0, 0))),
            ..corner_rects_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_square_contain_data_units_stroke_color").await;
}

// Instanced stroke color: each rect's border is colored from a categorical
// palette, independent of its fill.
#[tokio::test]
async fn test_rect_layer_square_contain_data_units_stroke_color_categorical() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(RectLayerParams {
            stroke_width: Some(SizeMode::UniformSize(4.0)),
            fill_color: Some(ColorMode::UniformRgb((200, 200, 200))),
            stroke_color: Some(ColorMode::Categorical(CategoricalParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1])),
                colormap: CategoricalColormap::Tableau10,
            })),
            ..corner_rects_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_square_contain_data_units_stroke_color_categorical").await;
}

// Uniform fill opacity: the fill is drawn at 50% while the border stays opaque.
#[tokio::test]
async fn test_rect_layer_square_contain_data_units_fill_opacity() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(RectLayerParams {
            stroke_width: Some(SizeMode::UniformSize(4.0)),
            fill_opacity: Some(OpacityMode::UniformOpacity(0.5)),
            ..corner_rects_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_square_contain_data_units_fill_opacity").await;
}

// Uniform stroke opacity: the border is drawn at 50% while the fill stays opaque.
#[tokio::test]
async fn test_rect_layer_square_contain_data_units_stroke_opacity() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(RectLayerParams {
            stroke_width: Some(SizeMode::UniformSize(6.0)),
            stroke_color: Some(ColorMode::UniformRgb((0, 0, 0))),
            stroke_opacity: Some(OpacityMode::UniformOpacity(0.5)),
            ..corner_rects_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_square_contain_data_units_stroke_opacity").await;
}

// Instanced stroke width: each rect gets its own border thickness.
#[tokio::test]
async fn test_rect_layer_square_contain_data_units_instanced_stroke_width() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(RectLayerParams {
            stroke_width: Some(SizeMode::InstancedSize(InstancedSizeParams {
                values: NumericData::Float32(Arc::new(vec![2.0, 8.0])),
            })),
            stroke_color: Some(ColorMode::UniformRgb((0, 0, 0))),
            ..corner_rects_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_square_contain_data_units_instanced_stroke_width").await;
}

// Instanced fill opacity: each rect's fill uses its own opacity value.
#[tokio::test]
async fn test_rect_layer_square_contain_data_units_instanced_fill_opacity() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(RectLayerParams {
            fill_opacity: Some(OpacityMode::InstancedOpacity(InstancedOpacityParams {
                values: NumericData::Float32(Arc::new(vec![0.25, 1.0])),
            })),
            ..corner_rects_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_square_contain_data_units_instanced_fill_opacity").await;
}

// ── stroke_width_unit_mode: Normalized ────────────────────────────────────────
//
// Normalized stroke width is a fraction (0 to 1) of the layer height,
// independent of the camera. 0.02 * 100px == 2px, matching the 2px border
// used by corner_rects_normalized()'s default (Pixels) stroke width above.
#[tokio::test]
async fn test_rect_layer_square_contain_normalized_units_stroke_width_normalized_mode() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(RectLayerParams {
            stroke_width: Some(SizeMode::UniformSize(0.02)),
            stroke_width_unit_mode: UnitsMode::Normalized,
            stroke_color: Some(ColorMode::UniformRgb((0, 0, 0))),
            ..corner_rects_normalized()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_square_contain_normalized_units_stroke_width_normalized_mode").await;
}

// Same normalized stroke width (0.02) on a taller (100x200) canvas: since it is
// height-relative, the border renders at 0.02 * 200px == 4px, twice as thick as
// the square-canvas test above, demonstrating the height-relative scaling.
#[tokio::test]
async fn test_rect_layer_tall_contain_normalized_units_stroke_width_normalized_mode() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(RectLayerParams {
            stroke_width: Some(SizeMode::UniformSize(0.02)),
            stroke_width_unit_mode: UnitsMode::Normalized,
            stroke_color: Some(ColorMode::UniformRgb((0, 0, 0))),
            ..corner_rects_normalized()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_tall_contain_normalized_units_stroke_width_normalized_mode").await;
}
