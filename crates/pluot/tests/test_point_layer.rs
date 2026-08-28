#![cfg(not(target_arch = "wasm32"))]

use std::sync::Arc;

mod test_utils;
use test_utils::render_and_check_both_snapshots;

use pluot::{
    RenderParams, LayerParams,
    AspectRatioMode, UnitsMode, MarginParams,
    PointLayerParams, PointShapeMode,
    CategoricalColormap, CategoricalParams, CategoricalCustomParams, ColorMode,
    QuantitativeParams, QuantitativeColormap,
    SizeMode, OpacityMode, InstancedSizeParams, InstancedOpacityParams,
    NumericData,
    EmphasisCriteria, CategoricalCriteriaParams, QuantitativeCriteriaParams,
};

// For primitive layer tests, we always want to test the following cases (and combinations of them):
// - Square and non-square (wide and tall) aspect ratios
// - Each aspect ratio mode (ignore, contain, cover)
// - Both data and pixel data_unit_modes
// - With and without margins at the view level
// - With and without margins (bounds) at the layer level
// - Multiple camera matrices (identity, zoomed-in, zoomed-out, panned)
// - Raster and vector (which the helper function already handles for us)
// - Layer-specific stuff
//   - For PointLayer, this includes testing different point shapes, sizes, and point radius unit modes

// Helper: 4 points at the corners of [0,1]x[0,1] in data space
fn corner_points_data() -> PointLayerParams {
    PointLayerParams {
        layer_id: "my_point_layer".to_string(),
        bounds: None,
        data_unit_mode_x: UnitsMode::Data,
        data_unit_mode_y: UnitsMode::Data,
        point_radius: Some(SizeMode::UniformSize(10.0)),
        point_radius_unit_mode_x: UnitsMode::Pixels,
        point_radius_unit_mode_y: UnitsMode::Pixels,
        point_shape_mode: PointShapeMode::Square,
        model_matrix: None,
        position_x: NumericData::Float32(Arc::new(vec![0.0, 1.0, 1.0, 0.0])),
        position_y: NumericData::Float32(Arc::new(vec![0.0, 0.0, 1.0, 1.0])),
        fill_color: Some(ColorMode::Categorical(CategoricalParams {
            codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3])),
            colormap: CategoricalColormap::Tableau10,
        })),
        ..Default::default()
    }
}

// Helper: 4 points at the corners of a 100x100 pixel space
fn corner_points_pixels() -> PointLayerParams {
    PointLayerParams {
        layer_id: "my_point_layer".to_string(),
        bounds: None,
        data_unit_mode_x: UnitsMode::Pixels,
        data_unit_mode_y: UnitsMode::Pixels,
        point_radius: Some(SizeMode::UniformSize(10.0)),
        point_radius_unit_mode_x: UnitsMode::Pixels,
        point_radius_unit_mode_y: UnitsMode::Pixels,
        point_shape_mode: PointShapeMode::Square,
        model_matrix: None,
        position_x: NumericData::Float32(Arc::new(vec![0.0, 100.0, 100.0, 0.0])),
        position_y: NumericData::Float32(Arc::new(vec![0.0, 0.0, 100.0, 100.0])),
        fill_color: Some(ColorMode::Categorical(CategoricalParams {
            codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3])),
            colormap: CategoricalColormap::Tableau10,
        })),
        ..Default::default()
    }
}

// Helper: 4 points with x in [0,1] data space, y in 100px pixel space
fn corner_points_data_x_pixel_y() -> PointLayerParams {
    PointLayerParams {
        data_unit_mode_x: UnitsMode::Data,
        data_unit_mode_y: UnitsMode::Pixels,
        position_x: NumericData::Float32(Arc::new(vec![0.0, 0.5, 0.5, 0.0])),
        position_y: NumericData::Float32(Arc::new(vec![0.0, 0.0, 100.0, 100.0])),
        ..corner_points_data()
    }
}

// Helper: 4 points with x in 100px pixel space, y in [0,1] data space
fn corner_points_pixel_x_data_y() -> PointLayerParams {
    PointLayerParams {
        data_unit_mode_x: UnitsMode::Pixels,
        data_unit_mode_y: UnitsMode::Data,
        position_x: NumericData::Float32(Arc::new(vec![0.0, 100.0, 100.0, 0.0])),
        position_y: NumericData::Float32(Arc::new(vec![0.0, 0.0, 0.5, 0.5])),
        ..corner_points_data()
    }
}

// Helper: 4 points at the corners of a [0,1]x[0,1] normalized space. Uses the
// same fractions as corner_points_pixels() (divided by 100), so on a 100x100
// canvas this renders identically to corner_points_pixels() while remaining
// agnostic to the layer's actual pixel dimensions (unlike Pixels mode, the
// same params render the same *proportions* on any canvas size).
fn corner_points_normalized() -> PointLayerParams {
    PointLayerParams {
        layer_id: "my_point_layer".to_string(),
        bounds: None,
        data_unit_mode_x: UnitsMode::Normalized,
        data_unit_mode_y: UnitsMode::Normalized,
        point_radius: Some(SizeMode::UniformSize(10.0)),
        point_radius_unit_mode_x: UnitsMode::Pixels,
        point_radius_unit_mode_y: UnitsMode::Pixels,
        point_shape_mode: PointShapeMode::Square,
        model_matrix: None,
        position_x: NumericData::Float32(Arc::new(vec![0.0, 1.0, 1.0, 0.0])),
        position_y: NumericData::Float32(Arc::new(vec![0.0, 0.0, 1.0, 1.0])),
        fill_color: Some(ColorMode::Categorical(CategoricalParams {
            codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3])),
            colormap: CategoricalColormap::Tableau10,
        })),
        ..Default::default()
    }
}

// Helper: 4 points with x in [0,1] data space, y in [0,1] normalized space
fn corner_points_data_x_normalized_y() -> PointLayerParams {
    PointLayerParams {
        data_unit_mode_x: UnitsMode::Data,
        data_unit_mode_y: UnitsMode::Normalized,
        position_x: NumericData::Float32(Arc::new(vec![0.0, 0.5, 0.5, 0.0])),
        position_y: NumericData::Float32(Arc::new(vec![0.0, 0.0, 1.0, 1.0])),
        ..corner_points_data()
    }
}

// Helper: 4 points with x in [0,1] normalized space, y in [0,1] data space
fn corner_points_normalized_x_data_y() -> PointLayerParams {
    PointLayerParams {
        data_unit_mode_x: UnitsMode::Normalized,
        data_unit_mode_y: UnitsMode::Data,
        position_x: NumericData::Float32(Arc::new(vec![0.0, 1.0, 1.0, 0.0])),
        position_y: NumericData::Float32(Arc::new(vec![0.0, 0.0, 0.5, 0.5])),
        ..corner_points_data()
    }
}

fn layer_params(point_params: PointLayerParams) -> Vec<LayerParams> {
    vec![LayerParams::PointLayer(point_params)]
}

// ── Square canvas (100x100) ───────────────────────────────────────────────────

#[tokio::test]
async fn test_point_layer_square_contain_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PointLayerParams {
            bounds: Some(MarginParams {
                margin_left: Some(0.0),
                margin_right: Some(0.0),
                margin_top: Some(0.0),
                margin_bottom: Some(0.0),
            }),
            ..corner_points_data()
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
        layers: layer_params(corner_points_data()),
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
        layers: layer_params(corner_points_data()),
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
        layers: layer_params(corner_points_pixels()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_pixel_units_no_margins").await;
}

// Normalized units: on a 100x100 canvas this renders identically to the Pixels
// test above, since corner_points_normalized() uses the same fractions (0.0/1.0)
// that corner_points_pixels() uses as absolute pixel values out of 100.
#[tokio::test]
async fn test_point_layer_square_contain_normalized_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(corner_points_normalized()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_normalized_units_no_margins").await;
}

#[tokio::test]
async fn test_point_layer_square_contain_data_units_view_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(corner_points_data()),
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
        layers: layer_params(PointLayerParams {
            bounds: Some(MarginParams {
                margin_left: Some(10.0),
                margin_right: Some(10.0),
                margin_top: Some(10.0),
                margin_bottom: Some(10.0),
            }),
            ..corner_points_data()
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
        layers: layer_params(PointLayerParams {
            bounds: Some(MarginParams {
                margin_left: Some(10.0),
                margin_right: Some(10.0),
                margin_top: Some(10.0),
                margin_bottom: Some(10.0),
            }),
            ..corner_points_data()
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

// Wide canvas (200x100)

#[tokio::test]
async fn test_point_layer_wide_ignore_data_units_no_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(corner_points_data()),
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
        layers: layer_params(corner_points_data()),
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
        layers: layer_params(corner_points_data()),
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
        layers: layer_params(PointLayerParams {
            position_x: NumericData::Float32(Arc::new(vec![0.0, 200.0, 200.0, 0.0])),
            position_y: NumericData::Float32(Arc::new(vec![0.0, 0.0, 100.0, 100.0])),
            ..corner_points_pixels()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_wide_contain_pixel_units_no_margins").await;
}

// Normalized units on a wide canvas: unlike the Pixels test above (which needs
// its own position overrides rescaled to the 200px width), corner_points_normalized()
// is reused completely unchanged from the square-canvas test, since its 0-1
// fractions are agnostic to the layer's actual pixel dimensions.
#[tokio::test]
async fn test_point_layer_wide_contain_normalized_units_no_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(corner_points_normalized()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_wide_contain_normalized_units_no_margins").await;
}

#[tokio::test]
async fn test_point_layer_wide_contain_data_units_view_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(corner_points_data()),
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
        layers: layer_params(PointLayerParams {
            bounds: Some(MarginParams {
                margin_left: Some(10.0),
                margin_right: Some(10.0),
                margin_top: Some(10.0),
                margin_bottom: Some(10.0),
            }),
            ..corner_points_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_wide_contain_data_units_layer_bounds").await;
}

// Tall canvas (100x200)

#[tokio::test]
async fn test_point_layer_tall_ignore_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(corner_points_data()),
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
        layers: layer_params(corner_points_data()),
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
        layers: layer_params(corner_points_data()),
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
        layers: layer_params(PointLayerParams {
            position_x: NumericData::Float32(Arc::new(vec![0.0, 100.0, 100.0, 0.0])),
            position_y: NumericData::Float32(Arc::new(vec![0.0, 0.0, 200.0, 200.0])),
            ..corner_points_pixels()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_tall_contain_pixel_units_no_margins").await;
}

// Normalized units on a tall canvas: again reusing corner_points_normalized()
// unchanged, demonstrating pixel-dimension independence.
#[tokio::test]
async fn test_point_layer_tall_contain_normalized_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(corner_points_normalized()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_tall_contain_normalized_units_no_margins").await;
}

#[tokio::test]
async fn test_point_layer_tall_contain_data_units_view_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(corner_points_data()),
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
        layers: layer_params(PointLayerParams {
            bounds: Some(MarginParams {
                margin_left: Some(10.0),
                margin_right: Some(10.0),
                margin_top: Some(10.0),
                margin_bottom: Some(10.0),
            }),
            ..corner_points_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_tall_contain_data_units_layer_bounds").await;
}

// ── Mixed unit modes (data_unit_mode_x ≠ data_unit_mode_y) ───────────────────

#[tokio::test]
async fn test_point_layer_square_contain_data_x_pixel_y_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(corner_points_data_x_pixel_y()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_data_x_pixel_y_no_margins").await;
}

#[tokio::test]
async fn test_point_layer_square_contain_pixel_x_data_y_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(corner_points_pixel_x_data_y()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_pixel_x_data_y_no_margins").await;
}

#[tokio::test]
async fn test_point_layer_square_contain_data_x_normalized_y_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(corner_points_data_x_normalized_y()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_data_x_normalized_y_no_margins").await;
}

#[tokio::test]
async fn test_point_layer_square_contain_normalized_x_data_y_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(corner_points_normalized_x_data_y()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_normalized_x_data_y_no_margins").await;
}

// Circle shape

fn corner_points_circle() -> PointLayerParams {
    PointLayerParams {
        point_shape_mode: PointShapeMode::Circle,
        ..corner_points_data()
    }
}

#[tokio::test]
async fn test_point_layer_square_contain_circle_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PointLayerParams {
            bounds: Some(MarginParams {
                margin_left: Some(0.0),
                margin_right: Some(0.0),
                margin_top: Some(0.0),
                margin_bottom: Some(0.0),
            }),
            ..corner_points_circle()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_circle_no_margins").await;
}

#[tokio::test]
async fn test_point_layer_square_ignore_circle_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(corner_points_circle()),
        aspect_ratio_mode: AspectRatioMode::Ignore,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_ignore_circle_no_margins").await;
}

#[tokio::test]
async fn test_point_layer_square_cover_circle_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(corner_points_circle()),
        aspect_ratio_mode: AspectRatioMode::Cover,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_cover_circle_no_margins").await;
}

#[tokio::test]
async fn test_point_layer_square_contain_circle_view_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(corner_points_circle()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        margin_left: Some(10.0),
        margin_right: Some(10.0),
        margin_top: Some(10.0),
        margin_bottom: Some(10.0),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_circle_view_margins").await;
}

#[tokio::test]
async fn test_point_layer_square_contain_circle_layer_bounds() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PointLayerParams {
            bounds: Some(MarginParams {
                margin_left: Some(10.0),
                margin_right: Some(10.0),
                margin_top: Some(10.0),
                margin_bottom: Some(10.0),
            }),
            ..corner_points_circle()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_circle_layer_bounds").await;
}

#[tokio::test]
async fn test_point_layer_wide_contain_circle_no_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(corner_points_circle()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_wide_contain_circle_no_margins").await;
}

#[tokio::test]
async fn test_point_layer_wide_ignore_circle_no_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(corner_points_circle()),
        aspect_ratio_mode: AspectRatioMode::Ignore,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_wide_ignore_circle_no_margins").await;
}

#[tokio::test]
async fn test_point_layer_tall_contain_circle_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(corner_points_circle()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_tall_contain_circle_no_margins").await;
}

#[tokio::test]
async fn test_point_layer_tall_ignore_circle_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(corner_points_circle()),
        aspect_ratio_mode: AspectRatioMode::Ignore,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_tall_ignore_circle_no_margins").await;
}

// ── Data-units point radius ──────────────────────────────────────────────────
// point_radius_unit_mode_x/y == UnitsMode::Data: the radius is expressed in the
// same data units as the positions, so it scales with the camera/aspect-ratio
// transform (and the model matrix). Both X and Y radius unit modes must match.

// Helper: corner points in data space with the radius also expressed in data
// units (0.1 data units == 10% of the [0,1] data extent in both axes).
fn corner_points_data_radius() -> PointLayerParams {
    PointLayerParams {
        point_radius: Some(SizeMode::UniformSize(0.25)),
        point_radius_unit_mode_x: UnitsMode::Data,
        point_radius_unit_mode_y: UnitsMode::Data,
        fill_opacity: Some(OpacityMode::UniformOpacity(0.5)),
        ..corner_points_data()
    }
}

#[tokio::test]
async fn test_point_layer_square_contain_data_radius_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PointLayerParams {
            bounds: Some(MarginParams {
                margin_left: Some(0.0),
                margin_right: Some(0.0),
                margin_top: Some(0.0),
                margin_bottom: Some(0.0),
            }),
            ..corner_points_data_radius()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_data_radius_no_margins").await;
}

#[tokio::test]
async fn test_point_layer_square_ignore_data_radius_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(corner_points_data_radius()),
        aspect_ratio_mode: AspectRatioMode::Ignore,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_ignore_data_radius_no_margins").await;
}

#[tokio::test]
async fn test_point_layer_square_cover_data_radius_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(corner_points_data_radius()),
        aspect_ratio_mode: AspectRatioMode::Cover,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_cover_data_radius_no_margins").await;
}

#[tokio::test]
async fn test_point_layer_square_contain_data_radius_view_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(corner_points_data_radius()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        margin_left: Some(10.0),
        margin_right: Some(10.0),
        margin_top: Some(10.0),
        margin_bottom: Some(10.0),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_data_radius_view_margins").await;
}

#[tokio::test]
async fn test_point_layer_square_contain_data_radius_layer_bounds() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PointLayerParams {
            bounds: Some(MarginParams {
                margin_left: Some(10.0),
                margin_right: Some(10.0),
                margin_top: Some(10.0),
                margin_bottom: Some(10.0),
            }),
            ..corner_points_data_radius()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_data_radius_layer_bounds").await;
}

// Wide/tall canvases: a data-units radius is anisotropic in screen space under
// Ignore, but Contain keeps the data axes uniformly scaled.
#[tokio::test]
async fn test_point_layer_wide_contain_data_radius_no_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(corner_points_data_radius()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_wide_contain_data_radius_no_margins").await;
}

#[tokio::test]
async fn test_point_layer_tall_contain_data_radius_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(corner_points_data_radius()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_tall_contain_data_radius_no_margins").await;
}

// Circle shape with a data-units radius.
#[tokio::test]
async fn test_point_layer_square_contain_circle_data_radius_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PointLayerParams {
            point_shape_mode: PointShapeMode::Circle,
            ..corner_points_data_radius()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_circle_data_radius_no_margins").await;
}

// A data-units radius scales with the model matrix (unlike a pixel radius).
#[tokio::test]
async fn test_point_layer_square_contain_data_radius_model_matrix_scale() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PointLayerParams {
            model_matrix: Some([
                0.5, 0.0, 0.0, 0.0,
                0.0, 0.5, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                0.0, 0.0, 0.0, 1.0,
            ]),
            ..corner_points_data_radius()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_data_radius_model_matrix_scale").await;
}

// ── Normalized-units point radius ────────────────────────────────────────────
// point_radius_unit_mode_x/y == UnitsMode::Normalized: the radius is a fraction
// (0 to 1) of the layer height, independent of the camera (like Pixels, but
// scaling with the layer's actual pixel dimensions rather than a fixed pixel
// count). Both X and Y radius unit modes must match.

// Helper: corner points in data space with the radius expressed as 0.05 (5%
// of the layer height) in normalized units.
fn corner_points_normalized_radius() -> PointLayerParams {
    PointLayerParams {
        point_radius: Some(SizeMode::UniformSize(0.05)),
        point_radius_unit_mode_x: UnitsMode::Normalized,
        point_radius_unit_mode_y: UnitsMode::Normalized,
        ..corner_points_data()
    }
}

#[tokio::test]
async fn test_point_layer_square_contain_normalized_radius_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(corner_points_normalized_radius()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_normalized_radius_no_margins").await;
}

// Same normalized radius (0.05) on a taller (100x200) canvas: since it is
// height-relative, the radius renders at 0.05 * 200px == 10px, twice the
// 0.05 * 100px == 5px radius on the square-canvas test above, demonstrating
// the height-relative scaling.
#[tokio::test]
async fn test_point_layer_tall_contain_normalized_radius_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(corner_points_normalized_radius()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_tall_contain_normalized_radius_no_margins").await;
}

// model_matrix

// Scale 0.5 in data mode: corner points at (0,1) become (0,0.5), lower-left quadrant.
#[tokio::test]
async fn test_point_layer_square_contain_data_units_model_matrix_scale() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PointLayerParams {
            model_matrix: Some([
                0.5, 0.0, 0.0, 0.0,
                0.0, 0.5, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                0.0, 0.0, 0.0, 1.0,
            ]),
            ..corner_points_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_data_units_model_matrix_scale").await;
}

// Translate +0.25 in data mode: corner points shift toward upper-right.
#[tokio::test]
async fn test_point_layer_square_contain_data_units_model_matrix_translate() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PointLayerParams {
            model_matrix: Some([
                1.0,  0.0,  0.0, 0.0,
                0.0,  1.0,  0.0, 0.0,
                0.0,  0.0,  1.0, 0.0,
                0.25, 0.25, 0.0, 1.0,
            ]),
            ..corner_points_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_data_units_model_matrix_translate").await;
}

// Scale 0.5 in pixel mode: model_matrix operates in normalized [0,1] space.
// Points at pixel corners --> normalized (0,1) --> scaled to (0,0.5), lower-left quadrant.
#[tokio::test]
async fn test_point_layer_square_contain_pixel_units_model_matrix_scale() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PointLayerParams {
            model_matrix: Some([
                0.5, 0.0, 0.0, 0.0,
                0.0, 0.5, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                0.0, 0.0, 0.0, 1.0,
            ]),
            ..corner_points_pixels()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_pixel_units_model_matrix_scale").await;
}

// Scale 0.5 in normalized mode: like pixel mode, model_matrix operates in
// normalized [0,1] space, so this should render identically to the pixel-mode
// model-matrix-scale test above (on a 100x100 canvas, where they coincide).
#[tokio::test]
async fn test_point_layer_square_contain_normalized_units_model_matrix_scale() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PointLayerParams {
            model_matrix: Some([
                0.5, 0.0, 0.0, 0.0,
                0.0, 0.5, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                0.0, 0.0, 0.0, 1.0,
            ]),
            ..corner_points_normalized()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_normalized_units_model_matrix_scale").await;
}

// ── Fill color modes ──────────────────────────────────────────────────────────

#[tokio::test]
async fn test_point_layer_square_contain_data_units_quantitative_color() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PointLayerParams {
            fill_color: Some(ColorMode::Quantitative(QuantitativeParams {
                values: NumericData::Float32(Arc::new(vec![0.0, 0.33, 0.67, 1.0])),
                colormap: QuantitativeColormap::Viridis,
                reverse: false,
                domain: None,
            })),
            ..corner_points_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_data_units_quantitative_color").await;
}

#[tokio::test]
async fn test_point_layer_square_contain_data_units_categorical_custom_color() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PointLayerParams {
            fill_color: Some(ColorMode::CategoricalCustom(CategoricalCustomParams {
                values: NumericData::Int32(Arc::new(vec![0, 1, 2, 3])),
                colormap: vec![
                    (255, 0, 0),
                    (0, 200, 0),
                    (0, 0, 255),
                    (200, 200, 0),
                ],
            })),
            ..corner_points_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_data_units_categorical_custom_color").await;
}

// ── Instanced point radius (SizeMode) ─────────────────────────────────────────
// SizeMode::InstancedSize supplies one radius per point (uploaded to the GPU as
// a value texture), rather than a single UniformSize shared by all points.

#[tokio::test]
async fn test_point_layer_square_contain_pixel_units_instanced_radius() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PointLayerParams {
            // One distinct radius (in pixels) per corner point.
            point_radius: Some(SizeMode::InstancedSize(InstancedSizeParams {
                values: NumericData::Float32(Arc::new(vec![5.0, 10.0, 15.0, 20.0])),
            })),
            ..corner_points_pixels()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_pixel_units_instanced_radius").await;
}

// ── Instanced point opacity (OpacityMode) ─────────────────────────────────────
// OpacityMode::InstancedOpacity supplies one opacity per point (uploaded to the
// GPU as a value texture), rather than a single UniformOpacity shared by all.

#[tokio::test]
async fn test_point_layer_square_contain_pixel_units_instanced_opacity() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PointLayerParams {
            // One distinct opacity per corner point.
            fill_opacity: Some(OpacityMode::InstancedOpacity(InstancedOpacityParams {
                values: NumericData::Float32(Arc::new(vec![0.25, 0.5, 0.75, 1.0])),
            })),
            ..corner_points_pixels()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_pixel_units_instanced_opacity").await;
}

// ── stroke_width_unit_mode: Normalized ────────────────────────────────────────
//
// Normalized stroke width is a fraction (0 to 1) of the layer height,
// independent of the camera. 0.02 * 100px == 2px stroke around each point.
#[tokio::test]
async fn test_point_layer_square_contain_normalized_units_stroke_width_normalized_mode() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PointLayerParams {
            stroke_width: Some(SizeMode::UniformSize(0.02)),
            stroke_width_unit_mode: UnitsMode::Normalized,
            stroke_color: Some(ColorMode::UniformRgb((0, 0, 0))),
            ..corner_points_normalized()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_normalized_units_stroke_width_normalized_mode").await;
}

// Same normalized stroke width (0.02) on a taller (100x200) canvas: since it is
// height-relative, the border renders at 0.02 * 200px == 4px, twice as thick as
// the square-canvas test above, demonstrating the height-relative scaling.
#[tokio::test]
async fn test_point_layer_tall_contain_normalized_units_stroke_width_normalized_mode() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(PointLayerParams {
            stroke_width: Some(SizeMode::UniformSize(0.02)),
            stroke_width_unit_mode: UnitsMode::Normalized,
            stroke_color: Some(ColorMode::UniformRgb((0, 0, 0))),
            ..corner_points_normalized()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_tall_contain_normalized_units_stroke_width_normalized_mode").await;
}

// ── Filtering and selection criteria ─────────────────────────────────────────
// Filter-excluded points are not
// rendered at all; filter-included but selection-excluded ("background")
// points still render, but re-colored with `background_fill_color`/
// `background_stroke_color` in place of their configured fill/stroke color.

// Categorical filtering: only points whose category code is in
// `included_codes` are rendered at all. Reuses the same codes as
// `fill_color` (0,1,2,3, one per corner), including only codes 0 and 2, so
// only the bottom-left and top-right corner points render.
#[tokio::test]
async fn test_point_layer_square_contain_filtering_categorical_subset() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PointLayerParams {
            filtering_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3])),
                included_codes: vec![0, 2],
            })],
            ..corner_points_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_filtering_categorical_subset").await;
}

// An explicit empty `included_codes` list means nothing is included: no
// points render at all (distinct from an empty `filtering_criteria` list,
// which includes everything).
#[tokio::test]
async fn test_point_layer_square_contain_filtering_categorical_empty_excludes_all() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PointLayerParams {
            filtering_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3])),
                included_codes: vec![],
            })],
            ..corner_points_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_filtering_categorical_empty_excludes_all").await;
}

// Quantitative filtering with both a min and a max bound: a per-point value
// column of [0, 1, 2, 3] filtered to the inclusive range [1, 2] includes only
// the second and third corner points.
#[tokio::test]
async fn test_point_layer_square_contain_filtering_quantitative_range() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PointLayerParams {
            filtering_criteria: vec![EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
                values: NumericData::Float32(Arc::new(vec![0.0, 1.0, 2.0, 3.0])),
                min: Some(1.0),
                max: Some(2.0),
            })],
            ..corner_points_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_filtering_quantitative_range").await;
}

// Quantitative filtering with only a `min` bound: `max` is omitted, meaning
// +infinity, so every point with value >= 2 is included (the last two
// corners).
#[tokio::test]
async fn test_point_layer_square_contain_filtering_quantitative_min_only() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PointLayerParams {
            filtering_criteria: vec![EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
                values: NumericData::Float32(Arc::new(vec![0.0, 1.0, 2.0, 3.0])),
                min: Some(2.0),
                max: None,
            })],
            ..corner_points_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_filtering_quantitative_min_only").await;
}

// Categorical selection: unlike filtering, selection-excluded points still
// render (all 4 corners are visible), but points whose code is not in
// `included_codes` (1 and 3) are re-colored with `background_fill_color`
// instead of their categorical `fill_color`.
#[tokio::test]
async fn test_point_layer_square_contain_selection_categorical_subset() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PointLayerParams {
            selection_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3])),
                included_codes: vec![0, 2],
            })],
            ..corner_points_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_selection_categorical_subset").await;
}

// An explicit empty `included_codes` list for selection means nothing is
// selected: all 4 points still render (filtering_criteria is empty), but
// every one is de-emphasized with `background_fill_color`.
#[tokio::test]
async fn test_point_layer_square_contain_selection_categorical_empty_deemphasizes_all() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PointLayerParams {
            selection_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3])),
                included_codes: vec![],
            })],
            ..corner_points_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_selection_categorical_empty_deemphasizes_all").await;
}

// Quantitative selection: a value column of [0, 10, 20, 30] selected to the
// range [10, 20] renders the middle two corners with their normal fill color
// and de-emphasizes the first/last corners with `background_fill_color`.
#[tokio::test]
async fn test_point_layer_square_contain_selection_quantitative_range() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PointLayerParams {
            selection_criteria: vec![EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
                values: NumericData::Float32(Arc::new(vec![0.0, 10.0, 20.0, 30.0])),
                min: Some(10.0),
                max: Some(20.0),
            })],
            ..corner_points_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_selection_quantitative_range").await;
}

// Selection criteria may be entirely orthogonal to filtering criteria: here
// filtering uses the same categorical codes as `fill_color` (excluding code 3,
// so the top-left corner is not rendered at all), while selection uses an
// unrelated quantitative column. Of the 3 filter-included points, the ones
// with value >= 15 (indices 1 and 2) are selected (normal color); index 0
// is filter-included but selection-excluded (background color); index 3 is
// filter-excluded and not rendered regardless of its selection value.
#[tokio::test]
async fn test_point_layer_square_contain_selection_orthogonal_to_filtering() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PointLayerParams {
            filtering_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3])),
                included_codes: vec![0, 1, 2],
            })],
            selection_criteria: vec![EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
                values: NumericData::Float32(Arc::new(vec![5.0, 25.0, 15.0, 8.0])),
                min: Some(15.0),
                max: None,
            })],
            ..corner_points_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_selection_orthogonal_to_filtering").await;
}

// `filtering_criteria` is a list of criteria AND-ed together: a point must
// satisfy every one to be included. Here a categorical criteria (codes
// 0,1,2,3, including 0/1/2 — excludes index 3) is combined with a
// quantitative criteria (values 0,5,15,25, min 10 — excludes indices 0/1).
// Only index 2 satisfies both, so only the top-right corner renders.
#[tokio::test]
async fn test_point_layer_square_contain_filtering_multiple_criteria_and() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PointLayerParams {
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
            ..corner_points_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_filtering_multiple_criteria_and").await;
}

// `selection_criteria` AND-ing mirrors `filtering_criteria`: a categorical
// criteria (included_codes 0/2) combined with a quantitative criteria (min
// 10, excluding indices 0/1) leaves only index 2 selected (normal color);
// every other point still renders (no filtering), but de-emphasized with
// `background_fill_color`.
#[tokio::test]
async fn test_point_layer_square_contain_selection_multiple_criteria_and() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PointLayerParams {
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
            ..corner_points_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_selection_multiple_criteria_and").await;
}

// Custom background fill/stroke colors, combined with a stroke, so that both
// the de-emphasized fill and the de-emphasized stroke are visible. Points 1
// and 3 are selected (normal categorical fill + black stroke); points 0 and 2
// are selection-excluded and rendered with a red background fill and a green
// background stroke instead.
#[tokio::test]
async fn test_point_layer_square_contain_selection_custom_background_colors() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PointLayerParams {
            stroke_width: Some(SizeMode::UniformSize(3.0)),
            stroke_color: Some(ColorMode::UniformRgb((0, 0, 0))),
            background_fill_color: Some((255, 0, 0)),
            background_stroke_color: Some((0, 255, 0)),
            selection_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3])),
                included_codes: vec![1, 3],
            })],
            ..corner_points_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_selection_custom_background_colors").await;
}

// ── Background fill/stroke opacity, radius and stroke width overrides ───────
// `enable_background_*` flags gate whether a filter-included, selection-
// excluded ("background") point uses the corresponding `background_*`
// override in place of its normal fill/stroke color, opacity, radius, or
// stroke width. Unlike `background_fill_color`/`background_stroke_color`
// (which fall back to a default gray when unset), the opacity/radius/width
// overrides are a no-op when left `None`, even if their `enable_background_*`
// flag is set (see `resolve_background_scalar` in `emphasis_mode.rs`).

// `enable_background_fill_color: false` disables the (otherwise default-on)
// fill-color de-emphasis: all 4 points keep their normal categorical fill
// color even though points 0 and 2 are selection-excluded.
#[tokio::test]
async fn test_point_layer_square_contain_selection_disable_background_fill_color() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PointLayerParams {
            enable_background_fill_color: false,
            selection_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3])),
                included_codes: vec![0, 2],
            })],
            ..corner_points_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_selection_disable_background_fill_color").await;
}

// `enable_background_stroke_color: false` disables stroke-color de-emphasis:
// every point's stroke stays black even though `background_stroke_color` is
// set to green and points 0/2 are selection-excluded.
#[tokio::test]
async fn test_point_layer_square_contain_selection_disable_background_stroke_color() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PointLayerParams {
            stroke_width: Some(SizeMode::UniformSize(3.0)),
            stroke_color: Some(ColorMode::UniformRgb((0, 0, 0))),
            background_stroke_color: Some((0, 255, 0)),
            enable_background_stroke_color: false,
            selection_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3])),
                included_codes: vec![0, 2],
            })],
            ..corner_points_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_selection_disable_background_stroke_color").await;
}

// `background_fill_opacity` + `enable_background_fill_opacity`: points 1 and 3
// (selection-excluded) render at 0.2 fill opacity instead of the default 1.0,
// while points 0 and 2 (selected) stay fully opaque. `enable_background_fill_color`
// is disabled so only the opacity change is exercised.
#[tokio::test]
async fn test_point_layer_square_contain_selection_background_fill_opacity() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PointLayerParams {
            enable_background_fill_color: false,
            background_fill_opacity: Some(0.2),
            enable_background_fill_opacity: true,
            selection_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3])),
                included_codes: vec![0, 2],
            })],
            ..corner_points_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_selection_background_fill_opacity").await;
}

// `background_stroke_opacity` + `enable_background_stroke_opacity`: mirrors
// the fill-opacity test above, but for the stroke band (points 0/2
// selection-excluded, stroke opacity drops to 0.15). Fill/stroke color
// de-emphasis is disabled so only the stroke-opacity change is exercised.
#[tokio::test]
async fn test_point_layer_square_contain_selection_background_stroke_opacity() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PointLayerParams {
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
            ..corner_points_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_selection_background_stroke_opacity").await;
}

// `background_point_radius` + `enable_background_point_radius`: points 1 and 3
// (selection-excluded) shrink to a 3px radius instead of the layer's 10px
// `point_radius`, while points 0 and 2 (selected) stay at 10px.
#[tokio::test]
async fn test_point_layer_square_contain_selection_background_point_radius() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PointLayerParams {
            enable_background_fill_color: false,
            background_point_radius: Some(3.0),
            enable_background_point_radius: true,
            selection_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3])),
                included_codes: vec![0, 2],
            })],
            ..corner_points_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_selection_background_point_radius").await;
}

// `background_stroke_width` can draw a stroke for background points even when
// the layer-level `stroke_width` is `None` (so selected points 0/2 have no
// stroke at all, but selection-excluded points 1/3 get a 3px black stroke).
#[tokio::test]
async fn test_point_layer_square_contain_selection_background_stroke_width_only() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PointLayerParams {
            // No layer-level stroke_width: selected (foreground) points have no stroke.
            stroke_color: Some(ColorMode::UniformRgb((0, 0, 0))),
            enable_background_fill_color: false,
            background_stroke_width: Some(3.0),
            enable_background_stroke_width: true,
            selection_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3])),
                included_codes: vec![0, 2],
            })],
            ..corner_points_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_selection_background_stroke_width_only").await;
}

// Enabling a background override with its value left `None` is a no-op (falls
// back to the normal foreground value), unlike `background_fill_color`/
// `background_stroke_color`, which fall back to a default gray. This should
// render identically to four normal, undifferentiated points despite
// selection excluding points 1 and 3 and every scalar override flag being on.
#[tokio::test]
async fn test_point_layer_square_contain_selection_background_overrides_none_value_is_noop() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PointLayerParams {
            enable_background_fill_color: false,
            enable_background_fill_opacity: true,
            enable_background_point_radius: true,
            enable_background_stroke_width: true,
            selection_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3])),
                included_codes: vec![0, 2],
            })],
            ..corner_points_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_selection_background_overrides_none_value_is_noop").await;
}

// TODO: performance tests with many elements, both raster and svg formats

// To compare svg to raster, render svg using resvg
// Reference: https://github.com/linebender/resvg/blob/9876cd45dd461ac3083f584cc83e66473a3061ef/crates/resvg/examples/minimal.rs#L27
