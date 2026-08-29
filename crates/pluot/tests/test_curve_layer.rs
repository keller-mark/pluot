#![cfg(not(target_arch = "wasm32"))]

use std::sync::Arc;

mod test_utils;
use test_utils::render_and_check_both_snapshots;

use pluot::{
    RenderParams, LayerParams,
    AspectRatioMode, UnitsMode, MarginParams,
    ColorMode, CurveLayerParams, NumericData, PathCommand, QuantitativeColormap, QuantitativeParams,
    SizeMode, OpacityMode, InstancedSizeParams, InstancedOpacityParams,
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
//   - For CurveLayer, this includes testing different line widths, subdivisions,
//     and the various path command types (line, cubic/quadratic Bezier, arc, close).

// Helper: an open S-shaped wave built from two cubic Bezier segments, in 1x1 data space.
fn wave_curve_data() -> CurveLayerParams {
    CurveLayerParams {
        layer_id: "my_curve_layer".to_string(),
        bounds: None,
        data_unit_mode_x: UnitsMode::Data,
        data_unit_mode_y: UnitsMode::Data,
        stroke_width: Some(SizeMode::UniformSize(2.0)),
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
        stroke_color: Some(ColorMode::UniformRgb((255, 0, 0))),
        fill_color: Some(ColorMode::UniformRgb((0, 0, 255))),
        stroke_opacity: Some(OpacityMode::UniformOpacity(1.0)),
        fill_opacity: Some(OpacityMode::UniformOpacity(1.0)),
        ..Default::default()
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

// Helper: the same wave in a [0,1]x[0,1] normalized space. Uses the same
// fractions as wave_curve_pixels() (divided by 100), so on a 100x100 canvas
// this renders identically to wave_curve_pixels() while remaining agnostic to
// the layer's actual pixel dimensions (unlike Pixels mode, the same params
// render the same *proportions* on any canvas size).
fn wave_curve_normalized() -> CurveLayerParams {
    CurveLayerParams {
        data_unit_mode_x: UnitsMode::Normalized,
        data_unit_mode_y: UnitsMode::Normalized,
        commands: Arc::new(vec![
            PathCommand::MoveTo { x: 0.1, y: 0.5 },
            PathCommand::CubicTo { x1: 0.3, y1: 0.9, x2: 0.4, y2: 0.9, x: 0.5, y: 0.5 },
            PathCommand::CubicTo { x1: 0.6, y1: 0.1, x2: 0.7, y2: 0.1, x: 0.9, y: 0.5 },
        ]),
        ..wave_curve_data()
    }
}

// Helper: wave with x in [0,1] data space, y in [0,1] normalized space.
fn wave_curve_data_x_normalized_y() -> CurveLayerParams {
    CurveLayerParams {
        data_unit_mode_x: UnitsMode::Data,
        data_unit_mode_y: UnitsMode::Normalized,
        commands: Arc::new(vec![
            PathCommand::MoveTo { x: 0.1, y: 0.5 },
            PathCommand::CubicTo { x1: 0.3, y1: 0.9, x2: 0.4, y2: 0.9, x: 0.5, y: 0.5 },
            PathCommand::CubicTo { x1: 0.6, y1: 0.1, x2: 0.7, y2: 0.1, x: 0.9, y: 0.5 },
        ]),
        ..wave_curve_data()
    }
}

// Helper: wave with x in [0,1] normalized space, y in [0,1] data space.
fn wave_curve_normalized_x_data_y() -> CurveLayerParams {
    CurveLayerParams {
        data_unit_mode_x: UnitsMode::Normalized,
        data_unit_mode_y: UnitsMode::Data,
        commands: Arc::new(vec![
            PathCommand::MoveTo { x: 0.1, y: 0.5 },
            PathCommand::CubicTo { x1: 0.3, y1: 0.9, x2: 0.4, y2: 0.9, x: 0.5, y: 0.5 },
            PathCommand::CubicTo { x1: 0.6, y1: 0.1, x2: 0.7, y2: 0.1, x: 0.9, y: 0.5 },
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
async fn test_curve_layer_square_contain_normalized_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(wave_curve_normalized()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_square_contain_normalized_units_no_margins").await;
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

#[tokio::test]
async fn test_curve_layer_square_contain_data_x_normalized_y_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(wave_curve_data_x_normalized_y()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_square_contain_data_x_normalized_y_no_margins").await;
}

#[tokio::test]
async fn test_curve_layer_square_contain_normalized_x_data_y_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(wave_curve_normalized_x_data_y()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_square_contain_normalized_x_data_y_no_margins").await;
}

// ── Line width ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_curve_layer_wide_contain_data_units_thick_line_width() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(CurveLayerParams {
            stroke_width: Some(SizeMode::UniformSize(10.0)),
            ..wave_curve_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_wide_contain_data_units_thick_line_width").await;
}

// Stroke width measured in data-coordinate units (rather than pixels): the
// stroke scales with the view/aspect-ratio transform.
#[tokio::test]
async fn test_curve_layer_square_contain_data_units_stroke_width() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(CurveLayerParams {
            stroke_width: Some(SizeMode::UniformSize(0.05)),
            stroke_width_unit_mode: UnitsMode::Data,
            ..wave_curve_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_square_contain_data_units_stroke_width").await;
}

#[tokio::test]
async fn test_curve_layer_wide_contain_data_units_stroke_width() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(CurveLayerParams {
            stroke_width: Some(SizeMode::UniformSize(0.05)),
            stroke_width_unit_mode: UnitsMode::Data,
            ..wave_curve_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_wide_contain_data_units_stroke_width").await;
}

// Stroke width measured as a fraction (0 to 1) of the layer height, independent
// of the camera. Unlike the "wide" canvas used for the Data-units pair above
// (which keeps the same 100px height as the square canvas and so wouldn't show
// any scaling), this pair uses a "tall" (100x200) canvas so the height actually
// changes between the two tests, demonstrating height-relative scaling.
#[tokio::test]
async fn test_curve_layer_square_contain_normalized_units_stroke_width() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(CurveLayerParams {
            stroke_width: Some(SizeMode::UniformSize(0.03)),
            stroke_width_unit_mode: UnitsMode::Normalized,
            ..wave_curve_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_square_contain_normalized_units_stroke_width").await;
}

// Same normalized stroke width (0.03) on a taller (100x200) canvas: since it is
// height-relative, the stroke renders at 0.03 * 200px == 6px, twice as thick as
// the square-canvas test above.
#[tokio::test]
async fn test_curve_layer_tall_contain_normalized_units_stroke_width() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(CurveLayerParams {
            stroke_width: Some(SizeMode::UniformSize(0.03)),
            stroke_width_unit_mode: UnitsMode::Normalized,
            ..wave_curve_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_tall_contain_normalized_units_stroke_width").await;
}

// ── Instanced stroke width / opacity, fill opacity ──────────────────────────────
// `CurveLayer` renders a single shape, so the instanced modes supply a single
// (length-1) value — but still exercise the GPU value-texture code path rather
// than the uniform path.

#[tokio::test]
async fn test_curve_layer_square_contain_instanced_stroke_width() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(CurveLayerParams {
            stroke_width: Some(SizeMode::InstancedSize(InstancedSizeParams {
                values: NumericData::Float32(Arc::new(vec![6.0])),
            })),
            ..wave_curve_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_square_contain_instanced_stroke_width").await;
}

#[tokio::test]
async fn test_curve_layer_square_contain_instanced_stroke_opacity() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(CurveLayerParams {
            stroke_width: Some(SizeMode::UniformSize(6.0)),
            stroke_opacity: Some(OpacityMode::InstancedOpacity(InstancedOpacityParams {
                values: NumericData::Float32(Arc::new(vec![0.4])),
            })),
            ..wave_curve_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_square_contain_instanced_stroke_opacity").await;
}

#[tokio::test]
async fn test_curve_layer_square_contain_instanced_fill_opacity() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(CurveLayerParams {
            stroked: false,
            filled: true,
            fill_color: Some(ColorMode::UniformRgb((0, 0, 255))),
            fill_opacity: Some(OpacityMode::InstancedOpacity(InstancedOpacityParams {
                values: NumericData::Float32(Arc::new(vec![0.4])),
            })),
            ..closed_curve_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_square_contain_instanced_fill_opacity").await;
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
            fill_color: Some(ColorMode::UniformRgb((0, 0, 255))),
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
            stroke_width: Some(SizeMode::UniformSize(4.0)),
            stroke_color: Some(ColorMode::UniformRgb((255, 0, 0))),
            fill_color: Some(ColorMode::UniformRgb((0, 0, 255))),
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
            stroke_width: Some(SizeMode::UniformSize(4.0)),
            stroke_color: Some(ColorMode::UniformRgb((255, 0, 0))),
            fill_color: Some(ColorMode::UniformRgb((0, 0, 255))),
            stroke_opacity: Some(OpacityMode::UniformOpacity(1.0)),
            fill_opacity: Some(OpacityMode::UniformOpacity(0.5)),
            ..closed_curve_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_square_contain_closed_curve_fill_opacity").await;
}

// `CurveLayer` renders a single shape, so a `ColorMode::Quantitative` fill
// resolves against a length-1 value array (always element 0).
#[tokio::test]
async fn test_curve_layer_square_contain_closed_curve_quantitative_fill() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(CurveLayerParams {
            stroked: false,
            filled: true,
            fill_color: Some(ColorMode::Quantitative(QuantitativeParams {
                values: NumericData::Float32(Arc::new(vec![0.75])),
                colormap: QuantitativeColormap::Viridis,
                reverse: false,
                domain: Some((0.0, 1.0)),
            })),
            ..closed_curve_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_square_contain_closed_curve_quantitative_fill").await;
}

// ── Filtering and selection criteria ─────────────────────────────────────────
// `CurveLayer` renders a single shape, so `filtering_criteria`/
// `selection_criteria` (each carrying a single, length-1 value per criteria)
// act as an all-or-nothing toggle for the whole shape rather than selecting a
// subset of many items. Filter-excluded means the shape (both stroke and
// fill sub-layers) is not rendered at all; filter-included but
// selection-excluded means it still renders, but re-colored with
// `background_stroke_color`/`background_fill_color` in place of its
// configured stroke/fill color.

// Categorical filtering matching the shape's own code: the shape renders
// normally.
#[tokio::test]
async fn test_curve_layer_square_contain_filtering_categorical_included() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(CurveLayerParams {
            filtering_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0])),
                included_codes: vec![0],
            })],
            ..wave_curve_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_square_contain_filtering_categorical_included").await;
}

// Categorical filtering excluding the shape's code (an explicit empty
// `included_codes` list, distinct from an empty `filtering_criteria` list):
// neither the stroke nor the fill sub-layer renders anything.
#[tokio::test]
async fn test_curve_layer_square_contain_filtering_categorical_excludes_shape() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(CurveLayerParams {
            filtering_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0])),
                included_codes: vec![],
            })],
            ..wave_curve_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_square_contain_filtering_categorical_excludes_shape").await;
}

// Quantitative filtering with both a min and a max bound including the
// shape's value: the shape renders normally.
#[tokio::test]
async fn test_curve_layer_square_contain_filtering_quantitative_range_included() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(CurveLayerParams {
            filtering_criteria: vec![EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
                values: NumericData::Float32(Arc::new(vec![5.0])),
                min: Some(1.0),
                max: Some(10.0),
            })],
            ..wave_curve_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_square_contain_filtering_quantitative_range_included").await;
}

// Quantitative filtering whose range excludes the shape's value: not
// rendered.
#[tokio::test]
async fn test_curve_layer_square_contain_filtering_quantitative_range_excludes_shape() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(CurveLayerParams {
            filtering_criteria: vec![EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
                values: NumericData::Float32(Arc::new(vec![5.0])),
                min: Some(10.0),
                max: None,
            })],
            ..wave_curve_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_square_contain_filtering_quantitative_range_excludes_shape").await;
}

// Categorical selection matching the shape's own code: the shape renders
// with its normal stroke/fill colors.
#[tokio::test]
async fn test_curve_layer_square_contain_selection_categorical_included() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(CurveLayerParams {
            selection_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0])),
                included_codes: vec![0],
            })],
            ..wave_curve_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_square_contain_selection_categorical_included").await;
}

// Categorical selection excluding the shape's code: unlike filtering, the
// shape still renders (both stroke and fill), but re-colored with
// `background_stroke_color`/`background_fill_color`.
#[tokio::test]
async fn test_curve_layer_square_contain_selection_categorical_deemphasizes_shape() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(CurveLayerParams {
            selection_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0])),
                included_codes: vec![],
            })],
            ..wave_curve_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_square_contain_selection_categorical_deemphasizes_shape").await;
}

// Quantitative selection whose range excludes the shape's value: the shape
// still renders, de-emphasized with the background stroke/fill colors.
#[tokio::test]
async fn test_curve_layer_square_contain_selection_quantitative_range() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(CurveLayerParams {
            selection_criteria: vec![EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
                values: NumericData::Float32(Arc::new(vec![5.0])),
                min: Some(10.0),
                max: None,
            })],
            ..wave_curve_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_square_contain_selection_quantitative_range").await;
}

// Selection criteria may be entirely orthogonal to filtering criteria: here
// filtering uses a categorical column that includes the shape (so it still
// renders), while selection uses an unrelated quantitative column whose
// range excludes the shape's value (so it renders de-emphasized).
#[tokio::test]
async fn test_curve_layer_square_contain_selection_orthogonal_to_filtering() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(CurveLayerParams {
            filtering_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0])),
                included_codes: vec![0],
            })],
            selection_criteria: vec![EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
                values: NumericData::Float32(Arc::new(vec![5.0])),
                min: Some(10.0),
                max: None,
            })],
            ..wave_curve_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_square_contain_selection_orthogonal_to_filtering").await;
}

// `filtering_criteria` is a list of criteria AND-ed together: the shape must
// satisfy every one to be included. Here a categorical criteria that
// includes the shape is combined with a quantitative criteria that excludes
// it, so the shape is not rendered.
#[tokio::test]
async fn test_curve_layer_square_contain_filtering_multiple_criteria_and() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(CurveLayerParams {
            filtering_criteria: vec![
                EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                    codes: NumericData::Int32(Arc::new(vec![0])),
                    included_codes: vec![0],
                }),
                EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
                    values: NumericData::Float32(Arc::new(vec![5.0])),
                    min: Some(10.0),
                    max: None,
                }),
            ],
            ..wave_curve_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_square_contain_filtering_multiple_criteria_and").await;
}

// `selection_criteria` AND-ing mirrors `filtering_criteria`: both criteria
// here match the shape, so it is selected and renders with its normal
// stroke/fill colors.
#[tokio::test]
async fn test_curve_layer_square_contain_selection_multiple_criteria_and() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(CurveLayerParams {
            selection_criteria: vec![
                EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                    codes: NumericData::Int32(Arc::new(vec![0])),
                    included_codes: vec![0],
                }),
                EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
                    values: NumericData::Float32(Arc::new(vec![5.0])),
                    min: Some(1.0),
                    max: Some(10.0),
                }),
            ],
            ..wave_curve_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_square_contain_selection_multiple_criteria_and").await;
}

// Custom background stroke/fill colors: the shape is selection-excluded, so
// it renders with a red background fill and a green background stroke
// instead of its configured blue fill / red stroke.
#[tokio::test]
async fn test_curve_layer_square_contain_selection_custom_background_colors() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(CurveLayerParams {
            background_fill_color: Some((255, 0, 0)),
            background_stroke_color: Some((0, 255, 0)),
            stroked: true,
            filled: true,
            selection_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0])),
                included_codes: vec![],
            })],
            ..wave_curve_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_square_contain_selection_custom_background_colors").await;
}

// ── Background fill/stroke opacity and stroke width overrides ───────────────
// `enable_background_*` flags gate whether a selection-excluded ("background")
// shape uses the corresponding `background_*` override in place of its
// normal fill/stroke color, opacity, or stroke width. Unlike
// `background_fill_color`/`background_stroke_color` (which fall back to a
// default gray when unset), the opacity/width overrides are a no-op when
// left `None`, even if their `enable_background_*` flag is set. All tests
// below use an empty `included_codes` selection criteria, so the single
// shape is always selection-excluded.

// `enable_background_fill_color: false` disables the (otherwise default-on)
// fill-color de-emphasis: the shape keeps its normal fill color even though
// it is selection-excluded.
#[tokio::test]
async fn test_curve_layer_square_contain_selection_disable_background_fill_color() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(CurveLayerParams {
            stroked: true,
            filled: true,
            enable_background_fill_color: false,
            selection_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0])),
                included_codes: vec![],
            })],
            ..wave_curve_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_square_contain_selection_disable_background_fill_color").await;
}

// `enable_background_stroke_color: false` disables stroke-color
// de-emphasis: the stroke stays its normal color even though
// `background_stroke_color` is set to green and the shape is
// selection-excluded.
#[tokio::test]
async fn test_curve_layer_square_contain_selection_disable_background_stroke_color() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(CurveLayerParams {
            stroked: true,
            filled: true,
            background_stroke_color: Some((0, 255, 0)),
            enable_background_stroke_color: false,
            selection_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0])),
                included_codes: vec![],
            })],
            ..wave_curve_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_square_contain_selection_disable_background_stroke_color").await;
}

// `background_fill_opacity` + `enable_background_fill_opacity`: the
// selection-excluded shape renders at 0.2 fill opacity instead of the
// default 1.0.
#[tokio::test]
async fn test_curve_layer_square_contain_selection_background_fill_opacity() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(CurveLayerParams {
            stroked: true,
            filled: true,
            enable_background_fill_color: false,
            background_fill_opacity: Some(0.2),
            enable_background_fill_opacity: true,
            selection_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0])),
                included_codes: vec![],
            })],
            ..wave_curve_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_square_contain_selection_background_fill_opacity").await;
}

// `background_stroke_opacity` + `enable_background_stroke_opacity`: mirrors
// the fill-opacity test above, but for the stroke.
#[tokio::test]
async fn test_curve_layer_square_contain_selection_background_stroke_opacity() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(CurveLayerParams {
            stroked: true,
            filled: true,
            enable_background_fill_color: false,
            enable_background_stroke_color: false,
            background_stroke_opacity: Some(0.15),
            enable_background_stroke_opacity: true,
            selection_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0])),
                included_codes: vec![],
            })],
            ..wave_curve_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_square_contain_selection_background_stroke_opacity").await;
}

// `background_stroke_width` + `enable_background_stroke_width`: the
// selection-excluded shape renders with a much thicker stroke than the
// layer's default 1px `stroke_width`.
#[tokio::test]
async fn test_curve_layer_square_contain_selection_background_stroke_width() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(CurveLayerParams {
            stroked: true,
            filled: false,
            enable_background_fill_color: false,
            background_stroke_width: Some(6.0),
            enable_background_stroke_width: true,
            selection_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0])),
                included_codes: vec![],
            })],
            ..wave_curve_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_square_contain_selection_background_stroke_width").await;
}

// Enabling a background override with its value left `None` is a no-op
// (falls back to the normal foreground value), unlike
// `background_fill_color`/`background_stroke_color`, which fall back to a
// default gray. This should render identically to the normal,
// undifferentiated shape despite selection excluding it and every scalar
// override flag being on.
#[tokio::test]
async fn test_curve_layer_square_contain_selection_background_overrides_none_value_is_noop() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(CurveLayerParams {
            stroked: true,
            filled: true,
            enable_background_fill_color: false,
            enable_background_fill_opacity: true,
            enable_background_stroke_width: true,
            selection_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0])),
                included_codes: vec![],
            })],
            ..wave_curve_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_curve_layer_square_contain_selection_background_overrides_none_value_is_noop").await;
}
