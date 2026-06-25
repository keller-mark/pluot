#![cfg(not(target_arch = "wasm32"))]

use std::sync::Arc;

mod test_utils;
use test_utils::render_and_check_both_snapshots;

use pluot::{
    RenderParams, LayerParams,
    AspectRatioMode, UnitsMode, MarginParams,
    CurveLayerParams, PathCommand,
};

// For primitive layer tests, we always want to test the following cases (and combinations of them):
// - Square and non-square (wide and tall) aspect ratios
// - Each aspect ratio mode (ignore, contain, cover)
// - Both data and pixel data_unit_modes
// - With and without margins at the view level
// - With and without margins (bounds) at the layer level
// - Raster and vector (which the helper function already handles for us)
// - Layer-specific stuff
//   - For CurveLayer, this includes testing different line widths, subdivisions,
//     and the various path command types (line, cubic/quadratic Bezier, arc, close).

// Helper: an open S-shaped wave built from two cubic Bezier segments, in 1x1 data space.
fn wave_curve_data() -> CurveLayerParams {
    CurveLayerParams {
        layer_id: "my_curve_layer".to_string(),
        bounds: None,
        data_unit_mode_x: UnitsMode::Data,
        data_unit_mode_y: UnitsMode::Data,
        stroke_width: 2.0,
        stroke_width_unit_mode: UnitsMode::Pixels,
        model_matrix: None,
        commands: Arc::new(vec![
            PathCommand::MoveTo { x: 0.1, y: 0.5 },
            PathCommand::CubicTo { x1: 0.3, y1: 0.9, x2: 0.4, y2: 0.9, x: 0.5, y: 0.5 },
            PathCommand::CubicTo { x1: 0.6, y1: 0.1, x2: 0.7, y2: 0.1, x: 0.9, y: 0.5 },
        ]),
        subdivisions: 32,
        stroked: true,
        filled: false,
        stroke_color: [1.0, 0.0, 0.0],
        fill_color: [0.0, 0.0, 1.0],
        stroke_opacity: 1.0,
        fill_opacity: 1.0,
    }
}

// Helper: the same wave in a 100x100 pixel space.
fn wave_curve_pixels() -> CurveLayerParams {
    CurveLayerParams {
        data_unit_mode_x: UnitsMode::Pixels,
        data_unit_mode_y: UnitsMode::Pixels,
        commands: Arc::new(vec![
            PathCommand::MoveTo { x: 10.0, y: 50.0 },
            PathCommand::CubicTo { x1: 30.0, y1: 90.0, x2: 40.0, y2: 90.0, x: 50.0, y: 50.0 },
            PathCommand::CubicTo { x1: 60.0, y1: 10.0, x2: 70.0, y2: 10.0, x: 90.0, y: 50.0 },
        ]),
        ..wave_curve_data()
    }
}

// Helper: wave with x in [0,1] data space, y in 100px pixel space.
fn wave_curve_data_x_pixel_y() -> CurveLayerParams {
    CurveLayerParams {
        data_unit_mode_x: UnitsMode::Data,
        data_unit_mode_y: UnitsMode::Pixels,
        commands: Arc::new(vec![
            PathCommand::MoveTo { x: 0.1, y: 50.0 },
            PathCommand::CubicTo { x1: 0.3, y1: 90.0, x2: 0.4, y2: 90.0, x: 0.5, y: 50.0 },
            PathCommand::CubicTo { x1: 0.6, y1: 10.0, x2: 0.7, y2: 10.0, x: 0.9, y: 50.0 },
        ]),
        ..wave_curve_data()
    }
}

// Helper: wave with x in 100px pixel space, y in [0,1] data space.
fn wave_curve_pixel_x_data_y() -> CurveLayerParams {
    CurveLayerParams {
        data_unit_mode_x: UnitsMode::Pixels,
        data_unit_mode_y: UnitsMode::Data,
        commands: Arc::new(vec![
            PathCommand::MoveTo { x: 10.0, y: 0.5 },
            PathCommand::CubicTo { x1: 30.0, y1: 0.9, x2: 40.0, y2: 0.9, x: 50.0, y: 0.5 },
            PathCommand::CubicTo { x1: 60.0, y1: 0.1, x2: 70.0, y2: 0.1, x: 90.0, y: 0.5 },
        ]),
        ..wave_curve_data()
    }
}

// Helper: a closed shape exercising line, quadratic Bezier, elliptical arc, and
// close commands, in 1x1 data space.
fn closed_curve_data() -> CurveLayerParams {
    CurveLayerParams {
        commands: Arc::new(vec![
            PathCommand::MoveTo { x: 0.2, y: 0.3 },
            PathCommand::LineTo { x: 0.8, y: 0.3 },
            PathCommand::QuadraticTo { x1: 0.95, y1: 0.5, x: 0.8, y: 0.7 },
            PathCommand::ArcTo {
                rx: 0.35,
                ry: 0.25,
                x_axis_rotation: 0.0,
                large_arc: false,
                sweep: true,
                x: 0.2,
                y: 0.7,
            },
            PathCommand::Close,
        ]),
        ..wave_curve_data()
    }
}

fn layer_params(curve_params: CurveLayerParams) -> Vec<LayerParams> {
    vec![LayerParams::CurveLayer(curve_params)]
}

// ── Square canvas (100x100) ───────────────────────────────────────────────────

#[tokio::test]
async fn test_curve_layer_square_contain_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(CurveLayerParams {
            bounds: Some(MarginParams {
                margin_left: Some(0.0),
                margin_right: Some(0.0),
                margin_top: Some(0.0),
                margin_bottom: Some(0.0),
            }),
            ..wave_curve_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_square_contain_data_units_no_margins").await;
}

#[tokio::test]
async fn test_curve_layer_square_ignore_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(wave_curve_data()),
        aspect_ratio_mode: AspectRatioMode::Ignore,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_square_ignore_data_units_no_margins").await;
}

#[tokio::test]
async fn test_curve_layer_square_cover_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(wave_curve_data()),
        aspect_ratio_mode: AspectRatioMode::Cover,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_square_cover_data_units_no_margins").await;
}

#[tokio::test]
async fn test_curve_layer_square_contain_pixel_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(wave_curve_pixels()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_square_contain_pixel_units_no_margins").await;
}

#[tokio::test]
async fn test_curve_layer_square_contain_data_units_view_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(wave_curve_data()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        margin_left: Some(10.0),
        margin_right: Some(10.0),
        margin_top: Some(10.0),
        margin_bottom: Some(10.0),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_square_contain_data_units_view_margins").await;
}

#[tokio::test]
async fn test_curve_layer_square_contain_data_units_layer_bounds() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(CurveLayerParams {
            bounds: Some(MarginParams {
                margin_left: Some(10.0),
                margin_right: Some(10.0),
                margin_top: Some(10.0),
                margin_bottom: Some(10.0),
            }),
            ..wave_curve_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_square_contain_data_units_layer_bounds").await;
}

// Layer bounds take precedence over view margins when both are set.
#[tokio::test]
async fn test_curve_layer_square_contain_data_units_layer_bounds_overrides_view_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(CurveLayerParams {
            bounds: Some(MarginParams {
                margin_left: Some(10.0),
                margin_right: Some(10.0),
                margin_top: Some(10.0),
                margin_bottom: Some(10.0),
            }),
            ..wave_curve_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        margin_left: Some(20.0),
        margin_right: Some(20.0),
        margin_top: Some(20.0),
        margin_bottom: Some(20.0),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_square_contain_data_units_layer_bounds_overrides_view_margins").await;
}

// Wide canvas (200x100)

#[tokio::test]
async fn test_curve_layer_wide_ignore_data_units_no_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(wave_curve_data()),
        aspect_ratio_mode: AspectRatioMode::Ignore,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_wide_ignore_data_units_no_margins").await;
}

#[tokio::test]
async fn test_curve_layer_wide_contain_data_units_no_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(wave_curve_data()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_wide_contain_data_units_no_margins").await;
}

#[tokio::test]
async fn test_curve_layer_wide_cover_data_units_no_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(wave_curve_data()),
        aspect_ratio_mode: AspectRatioMode::Cover,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_wide_cover_data_units_no_margins").await;
}

#[tokio::test]
async fn test_curve_layer_wide_contain_data_units_view_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(wave_curve_data()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        margin_left: Some(10.0),
        margin_right: Some(10.0),
        margin_top: Some(10.0),
        margin_bottom: Some(10.0),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_wide_contain_data_units_view_margins").await;
}

// Tall canvas (100x200)

#[tokio::test]
async fn test_curve_layer_tall_ignore_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(wave_curve_data()),
        aspect_ratio_mode: AspectRatioMode::Ignore,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_tall_ignore_data_units_no_margins").await;
}

#[tokio::test]
async fn test_curve_layer_tall_contain_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(wave_curve_data()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_tall_contain_data_units_no_margins").await;
}

#[tokio::test]
async fn test_curve_layer_tall_cover_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(wave_curve_data()),
        aspect_ratio_mode: AspectRatioMode::Cover,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_tall_cover_data_units_no_margins").await;
}

// ── Mixed unit modes (data_unit_mode_x ≠ data_unit_mode_y) ───────────────────

#[tokio::test]
async fn test_curve_layer_square_contain_data_x_pixel_y_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(wave_curve_data_x_pixel_y()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_square_contain_data_x_pixel_y_no_margins").await;
}

#[tokio::test]
async fn test_curve_layer_square_contain_pixel_x_data_y_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(wave_curve_pixel_x_data_y()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_square_contain_pixel_x_data_y_no_margins").await;
}

// ── Line width ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_curve_layer_wide_contain_data_units_thick_line_width() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(CurveLayerParams {
            stroke_width: 10.0,
            ..wave_curve_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_wide_contain_data_units_thick_line_width").await;
}

// ── Subdivisions ─────────────────────────────────────────────────────────────

// Few subdivisions: the curve should look visibly faceted (polyline-like).
#[tokio::test]
async fn test_curve_layer_square_contain_data_units_low_subdivisions() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(CurveLayerParams {
            subdivisions: 3,
            ..wave_curve_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_square_contain_data_units_low_subdivisions").await;
}

// ── model_matrix ─────────────────────────────────────────────────────────────

// Scale 0.5 in data mode: curve shrinks to lower-left quadrant of the unit square.
#[tokio::test]
async fn test_curve_layer_square_contain_data_units_model_matrix_scale() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(CurveLayerParams {
            model_matrix: Some([
                0.5, 0.0, 0.0, 0.0,
                0.0, 0.5, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                0.0, 0.0, 0.0, 1.0,
            ]),
            ..wave_curve_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_square_contain_data_units_model_matrix_scale").await;
}

// Translate +0.25 in data mode: curve shifts toward the upper-right.
#[tokio::test]
async fn test_curve_layer_square_contain_data_units_model_matrix_translate() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(CurveLayerParams {
            model_matrix: Some([
                1.0,  0.0,  0.0, 0.0,
                0.0,  1.0,  0.0, 0.0,
                0.0,  0.0,  1.0, 0.0,
                0.25, 0.25, 0.0, 1.0,
            ]),
            ..wave_curve_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_square_contain_data_units_model_matrix_translate").await;
}

// ── Closed path (line + quadratic + arc + close) ─────────────────────────────

#[tokio::test]
async fn test_curve_layer_square_contain_closed_curve_data_units() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(closed_curve_data()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_square_contain_closed_curve_data_units").await;
}

#[tokio::test]
async fn test_curve_layer_wide_contain_closed_curve_data_units() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(closed_curve_data()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_wide_contain_closed_curve_data_units").await;
}

// ── Fill modes (stroked / filled / both, separate colors and opacity) ────────

// Filled only: opaque blue interior, no stroke outline.
#[tokio::test]
async fn test_curve_layer_square_contain_closed_curve_filled() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(CurveLayerParams {
            stroked: false,
            filled: true,
            fill_color: [0.0, 0.0, 1.0],
            ..closed_curve_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_square_contain_closed_curve_filled").await;
}

// Both stroked and filled: blue fill under a red stroke.
#[tokio::test]
async fn test_curve_layer_square_contain_closed_curve_stroke_and_fill() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(CurveLayerParams {
            stroked: true,
            filled: true,
            stroke_width: 4.0,
            stroke_color: [1.0, 0.0, 0.0],
            fill_color: [0.0, 0.0, 1.0],
            ..closed_curve_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_square_contain_closed_curve_stroke_and_fill").await;
}

// Separate stroke/fill opacity values: semi-transparent fill, opaque stroke.
#[tokio::test]
async fn test_curve_layer_square_contain_closed_curve_fill_opacity() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(CurveLayerParams {
            stroked: true,
            filled: true,
            stroke_width: 4.0,
            stroke_color: [1.0, 0.0, 0.0],
            fill_color: [0.0, 0.0, 1.0],
            stroke_opacity: 1.0,
            fill_opacity: 0.5,
            ..closed_curve_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_square_contain_closed_curve_fill_opacity").await;
}
