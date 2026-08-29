#![cfg(not(target_arch = "wasm32"))]

use std::sync::Arc;

mod test_utils;
use test_utils::render_and_check_both_snapshots;

use pluot::{
    AspectRatioMode, CategoricalColormap, CategoricalParams, CategoricalCustomParams, ColorMode,
    InstancedOpacityParams, InstancedSizeParams, LayerParams, MarginParams, OpacityMode,
    QuantitativeParams, QuantitativeColormap,
    RectLayerParams, RenderParams, SizeMode, UnitsMode, NumericData,
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

// ── Filtering and selection criteria ─────────────────────────────────────────
// Filter-excluded rects are not rendered at all; filter-included but
// selection-excluded ("background") rects still render, but re-colored with
// `background_fill_color`/`background_stroke_color` in place of their
// configured fill/stroke color.

// Helper: 4 rects at the corners of [0,1]x[0,1] in data space (bottom-left,
// bottom-right, top-right, top-left), used by the filtering/selection tests
// below, where 4 items (rather than corner_rects_data()'s 2) are useful to
// demonstrate subsets.
fn criteria_rects_data() -> RectLayerParams {
    RectLayerParams {
        layer_id: "my_rect_layer".to_string(),
        bounds: None,
        data_unit_mode_x: UnitsMode::Data,
        data_unit_mode_y: UnitsMode::Data,
        position_x0: NumericData::Float32(Arc::new(vec![0.0, 0.5, 0.5, 0.0])),
        position_y0: NumericData::Float32(Arc::new(vec![0.0, 0.0, 0.5, 0.5])),
        position_x1: NumericData::Float32(Arc::new(vec![0.4, 0.9, 0.9, 0.4])),
        position_y1: NumericData::Float32(Arc::new(vec![0.4, 0.4, 0.9, 0.9])),
        fill_color: Some(ColorMode::Categorical(CategoricalParams {
            codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3])),
            colormap: CategoricalColormap::Tableau10,
        })),
        ..Default::default()
    }
}

// Categorical filtering: only rects whose category code is in
// `included_codes` are rendered at all. Reuses the same codes as
// `fill_color` (0,1,2,3, one per corner), including only codes 0 and 2, so
// only the bottom-left and top-right rects render.
#[tokio::test]
async fn test_rect_layer_square_contain_filtering_categorical_subset() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(RectLayerParams {
            filtering_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3])),
                included_codes: vec![0, 2],
            })],
            ..criteria_rects_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_square_contain_filtering_categorical_subset").await;
}

// An explicit empty `included_codes` list means nothing is included: no
// rects render at all (distinct from an empty `filtering_criteria` list,
// which includes everything).
#[tokio::test]
async fn test_rect_layer_square_contain_filtering_categorical_empty_excludes_all() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(RectLayerParams {
            filtering_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3])),
                included_codes: vec![],
            })],
            ..criteria_rects_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_square_contain_filtering_categorical_empty_excludes_all").await;
}

// Quantitative filtering with both a min and a max bound: a per-rect value
// column of [0, 1, 2, 3] filtered to the inclusive range [1, 2] includes only
// the second and third rects (bottom-right and top-right).
#[tokio::test]
async fn test_rect_layer_square_contain_filtering_quantitative_range() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(RectLayerParams {
            filtering_criteria: vec![EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
                values: NumericData::Float32(Arc::new(vec![0.0, 1.0, 2.0, 3.0])),
                min: Some(1.0),
                max: Some(2.0),
            })],
            ..criteria_rects_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_square_contain_filtering_quantitative_range").await;
}

// Quantitative filtering with only a `min` bound: `max` is omitted, meaning
// +infinity, so every rect with value >= 2 is included (the last two rects).
#[tokio::test]
async fn test_rect_layer_square_contain_filtering_quantitative_min_only() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(RectLayerParams {
            filtering_criteria: vec![EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
                values: NumericData::Float32(Arc::new(vec![0.0, 1.0, 2.0, 3.0])),
                min: Some(2.0),
                max: None,
            })],
            ..criteria_rects_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_square_contain_filtering_quantitative_min_only").await;
}

// Categorical selection: unlike filtering, selection-excluded rects still
// render (all 4 corners are visible), but rects whose code is not in
// `included_codes` (1 and 3) are re-colored with `background_fill_color`
// instead of their categorical `fill_color`.
#[tokio::test]
async fn test_rect_layer_square_contain_selection_categorical_subset() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(RectLayerParams {
            selection_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3])),
                included_codes: vec![0, 2],
            })],
            ..criteria_rects_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_square_contain_selection_categorical_subset").await;
}

// An explicit empty `included_codes` list for selection means nothing is
// selected: all 4 rects still render (filtering_criteria is empty), but
// every one is de-emphasized with `background_fill_color`.
#[tokio::test]
async fn test_rect_layer_square_contain_selection_categorical_empty_deemphasizes_all() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(RectLayerParams {
            selection_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3])),
                included_codes: vec![],
            })],
            ..criteria_rects_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_square_contain_selection_categorical_empty_deemphasizes_all").await;
}

// Quantitative selection: a value column of [0, 10, 20, 30] selected to the
// range [10, 20] renders the middle two rects with their normal fill color
// and de-emphasizes the first/last rects with `background_fill_color`.
#[tokio::test]
async fn test_rect_layer_square_contain_selection_quantitative_range() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(RectLayerParams {
            selection_criteria: vec![EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
                values: NumericData::Float32(Arc::new(vec![0.0, 10.0, 20.0, 30.0])),
                min: Some(10.0),
                max: Some(20.0),
            })],
            ..criteria_rects_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_square_contain_selection_quantitative_range").await;
}

// Selection criteria may be entirely orthogonal to filtering criteria: here
// filtering uses the same categorical codes as `fill_color` (excluding code 3,
// so the top-left rect is not rendered at all), while selection uses an
// unrelated quantitative column. Of the 3 filter-included rects, the ones
// with value >= 15 (indices 1 and 2) are selected (normal color); index 0
// is filter-included but selection-excluded (background color); index 3 is
// filter-excluded and not rendered regardless of its selection value.
#[tokio::test]
async fn test_rect_layer_square_contain_selection_orthogonal_to_filtering() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(RectLayerParams {
            filtering_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3])),
                included_codes: vec![0, 1, 2],
            })],
            selection_criteria: vec![EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
                values: NumericData::Float32(Arc::new(vec![5.0, 25.0, 15.0, 8.0])),
                min: Some(15.0),
                max: None,
            })],
            ..criteria_rects_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_square_contain_selection_orthogonal_to_filtering").await;
}

// `filtering_criteria` is a list of criteria AND-ed together: a rect must
// satisfy every one to be included. Here a categorical criteria (codes
// 0,1,2,3, including 0/1/2 — excludes index 3) is combined with a
// quantitative criteria (values 0,5,15,25, min 10 — excludes indices 0/1).
// Only index 2 satisfies both, so only the top-right rect renders.
#[tokio::test]
async fn test_rect_layer_square_contain_filtering_multiple_criteria_and() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(RectLayerParams {
            filtering_criteria: vec![
                EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                    codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3])),
                    included_codes: vec![0, 1, 2],
                }),
                EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
                    values: NumericData::Float32(Arc::new(vec![0.0, 5.0, 15.0, 25.0])),
                    min: Some(10.0),
                    max: None,
                }),
            ],
            ..criteria_rects_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_square_contain_filtering_multiple_criteria_and").await;
}

// `selection_criteria` AND-ing mirrors `filtering_criteria`: a categorical
// criteria (included_codes 0/2) combined with a quantitative criteria (min
// 10, excluding indices 0/1) leaves only index 2 selected (normal color);
// every other rect still renders (no filtering), but de-emphasized with
// `background_fill_color`.
#[tokio::test]
async fn test_rect_layer_square_contain_selection_multiple_criteria_and() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(RectLayerParams {
            selection_criteria: vec![
                EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                    codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3])),
                    included_codes: vec![0, 2],
                }),
                EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
                    values: NumericData::Float32(Arc::new(vec![0.0, 5.0, 15.0, 25.0])),
                    min: Some(10.0),
                    max: None,
                }),
            ],
            ..criteria_rects_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_square_contain_selection_multiple_criteria_and").await;
}

// Custom background fill/stroke colors, combined with a stroke, so that both
// the de-emphasized fill and the de-emphasized stroke are visible. Rects 1
// and 3 are selected (normal categorical fill + black stroke); rects 0 and 2
// are selection-excluded and rendered with a red background fill and a green
// background stroke instead.
#[tokio::test]
async fn test_rect_layer_square_contain_selection_custom_background_colors() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(RectLayerParams {
            stroke_width: Some(SizeMode::UniformSize(3.0)),
            stroke_color: Some(ColorMode::UniformRgb((0, 0, 0))),
            background_fill_color: Some((255, 0, 0)),
            background_stroke_color: Some((0, 255, 0)),
            selection_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3])),
                included_codes: vec![1, 3],
            })],
            ..criteria_rects_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_square_contain_selection_custom_background_colors").await;
}

// ── Background fill/stroke opacity and stroke width overrides ───────────────
// `enable_background_*` flags gate whether a filter-included, selection-
// excluded ("background") rect uses the corresponding `background_*`
// override in place of its normal fill/stroke color, opacity, or stroke
// width. Unlike `background_fill_color`/`background_stroke_color` (which
// fall back to a default gray when unset), the opacity/width overrides are a
// no-op when left `None`, even if their `enable_background_*` flag is set.

// `enable_background_fill_color: false` disables the (otherwise default-on)
// fill-color de-emphasis: all 4 rects keep their normal categorical fill
// color even though rects 0 and 2 are selection-excluded.
#[tokio::test]
async fn test_rect_layer_square_contain_selection_disable_background_fill_color() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(RectLayerParams {
            enable_background_fill_color: false,
            selection_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3])),
                included_codes: vec![0, 2],
            })],
            ..criteria_rects_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_square_contain_selection_disable_background_fill_color").await;
}

// `enable_background_stroke_color: false` disables stroke-color de-emphasis:
// every rect's stroke stays black even though `background_stroke_color` is
// set to green and rects 0/2 are selection-excluded.
#[tokio::test]
async fn test_rect_layer_square_contain_selection_disable_background_stroke_color() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(RectLayerParams {
            stroke_width: Some(SizeMode::UniformSize(3.0)),
            stroke_color: Some(ColorMode::UniformRgb((0, 0, 0))),
            background_stroke_color: Some((0, 255, 0)),
            enable_background_stroke_color: false,
            selection_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3])),
                included_codes: vec![0, 2],
            })],
            ..criteria_rects_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_square_contain_selection_disable_background_stroke_color").await;
}

// `background_fill_opacity` + `enable_background_fill_opacity`: rects 1 and 3
// (selection-excluded) render at 0.2 fill opacity instead of the default 1.0,
// while rects 0 and 2 (selected) stay fully opaque.
#[tokio::test]
async fn test_rect_layer_square_contain_selection_background_fill_opacity() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(RectLayerParams {
            enable_background_fill_color: false,
            background_fill_opacity: Some(0.2),
            enable_background_fill_opacity: true,
            selection_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3])),
                included_codes: vec![0, 2],
            })],
            ..criteria_rects_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_square_contain_selection_background_fill_opacity").await;
}

// `background_stroke_opacity` + `enable_background_stroke_opacity`: mirrors
// the fill-opacity test above, but for the stroke band.
#[tokio::test]
async fn test_rect_layer_square_contain_selection_background_stroke_opacity() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(RectLayerParams {
            stroke_width: Some(SizeMode::UniformSize(3.0)),
            stroke_color: Some(ColorMode::UniformRgb((0, 0, 0))),
            enable_background_fill_color: false,
            enable_background_stroke_color: false,
            background_stroke_opacity: Some(0.15),
            enable_background_stroke_opacity: true,
            selection_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3])),
                included_codes: vec![0, 2],
            })],
            ..criteria_rects_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_square_contain_selection_background_stroke_opacity").await;
}

// `background_stroke_width` can draw a border for background rects even when
// the layer-level `stroke_width` is `None` (so selected rects 0/2 have no
// border at all, but selection-excluded rects 1/3 get a 3px black border).
#[tokio::test]
async fn test_rect_layer_square_contain_selection_background_stroke_width_only() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(RectLayerParams {
            // No layer-level stroke_width: selected (foreground) rects have no border.
            stroke_color: Some(ColorMode::UniformRgb((0, 0, 0))),
            enable_background_fill_color: false,
            background_stroke_width: Some(3.0),
            enable_background_stroke_width: true,
            selection_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3])),
                included_codes: vec![0, 2],
            })],
            ..criteria_rects_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_square_contain_selection_background_stroke_width_only").await;
}

// Enabling a background override with its value left `None` is a no-op
// (falls back to the normal foreground value), unlike
// `background_fill_color`/`background_stroke_color`, which fall back to a
// default gray. This should render identically to four normal,
// undifferentiated rects despite selection excluding rects 1 and 3 and every
// scalar override flag being on.
#[tokio::test]
async fn test_rect_layer_square_contain_selection_background_overrides_none_value_is_noop() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(RectLayerParams {
            enable_background_fill_color: false,
            enable_background_fill_opacity: true,
            enable_background_stroke_width: true,
            selection_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3])),
                included_codes: vec![0, 2],
            })],
            ..criteria_rects_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_rect_layer_square_contain_selection_background_overrides_none_value_is_noop").await;
}
