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
        point_opacity: Some(OpacityMode::UniformOpacity(0.5)),
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
            point_opacity: Some(OpacityMode::InstancedOpacity(InstancedOpacityParams {
                values: NumericData::Float32(Arc::new(vec![0.25, 0.5, 0.75, 1.0])),
            })),
            ..corner_points_pixels()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_point_layer_square_contain_pixel_units_instanced_opacity").await;
}

// TODO: performance tests with many elements, both raster and svg formats

// To compare svg to raster, render svg using resvg
// Reference: https://github.com/linebender/resvg/blob/9876cd45dd461ac3083f584cc83e66473a3061ef/crates/resvg/examples/minimal.rs#L27
