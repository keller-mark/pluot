#![cfg(not(target_arch = "wasm32"))]

use std::sync::Arc;

mod test_utils;
use test_utils::render_and_check_both_snapshots;

use pluot::{
    RenderParams, LayerParams,
    AspectRatioMode, UnitsMode, MarginParams,
    CategoricalCustomParams, ColorMode, PolygonLayerParams, NumericData,
    SizeMode, OpacityMode, InstancedSizeParams, InstancedOpacityParams,
};

// For each test suite we check:
// - Square (100x100), wide (200x100), tall (100x200) canvases
// - Contain / Ignore / Cover aspect ratio modes
// - Data and Pixel unit modes
// - View-level and layer-level margins
// - Stroked only, filled only, stroked + filled
// - Multiple polygons in one layer

// ── Test data helpers ──────────────────────────────────────────────────────────

/// A triangle in [0,1] data space.
fn triangle_data() -> PolygonLayerParams {
    PolygonLayerParams {
        layer_id: "my_polygon_layer".to_string(),
        bounds: None,
        data_unit_mode_x: UnitsMode::Data,
        data_unit_mode_y: UnitsMode::Data,
        stroke_width_unit_mode: UnitsMode::Pixels,
        model_matrix: None,
        // Flat interleaved [x0, y0, x1, y1, …]; one polygon spanning vertices 0..3.
        polygons: NumericData::Float32(Arc::new(vec![
            0.1, 0.1,
            0.9, 0.1,
            0.5, 0.9,
        ])),
        polygon_offsets: NumericData::Uint32(Arc::new(vec![0, 3])),
        stroked: true,
        filled: false,
        stroke_color: Some(ColorMode::UniformRgb((255, 0, 0))),
        stroke_width: Some(SizeMode::UniformSize(2.0)),
        stroke_opacity: Some(OpacityMode::UniformOpacity(1.0)),
        fill_color: Some(ColorMode::UniformRgb((0, 0, 255))),
        fill_opacity: Some(OpacityMode::UniformOpacity(1.0)),
    }
}

/// A quadrilateral (pentagon) in [0,1] data space.
fn quad_data() -> PolygonLayerParams {
    PolygonLayerParams {
        // One polygon spanning vertices 0..5.
        polygons: NumericData::Float32(Arc::new(vec![
            0.1, 0.3,
            0.5, 0.1,
            0.9, 0.3,
            0.7, 0.9,
            0.3, 0.9,
        ])),
        polygon_offsets: NumericData::Uint32(Arc::new(vec![0, 5])),
        ..triangle_data()
    }
}

/// Triangle in pixel space (100×100 canvas).
fn triangle_pixels() -> PolygonLayerParams {
    PolygonLayerParams {
        data_unit_mode_x: UnitsMode::Pixels,
        data_unit_mode_y: UnitsMode::Pixels,
        polygons: NumericData::Float32(Arc::new(vec![
            10.0, 10.0,
            90.0, 10.0,
            50.0, 90.0,
        ])),
        polygon_offsets: NumericData::Uint32(Arc::new(vec![0, 3])),
        ..triangle_data()
    }
}

/// Two non-overlapping triangles in data space.
fn two_triangles_data() -> PolygonLayerParams {
    PolygonLayerParams {
        // Two polygons concatenated: vertices 0..3 and 3..6.
        polygons: NumericData::Float32(Arc::new(vec![
            0.05, 0.05, 0.45, 0.05, 0.25, 0.45,
            0.55, 0.55, 0.95, 0.55, 0.75, 0.95,
        ])),
        polygon_offsets: NumericData::Uint32(Arc::new(vec![0, 3, 6])),
        ..triangle_data()
    }
}

fn layer_params(poly_params: PolygonLayerParams) -> Vec<LayerParams> {
    vec![LayerParams::PolygonLayer(poly_params)]
}

// ── Square canvas (100x100) ────────────────────────────────────────────────────

#[tokio::test]
async fn test_polygon_layer_square_contain_data_units_stroked() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(triangle_data()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_polygon_layer_square_contain_data_units_stroked").await;
}

#[tokio::test]
async fn test_polygon_layer_square_ignore_data_units_stroked() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(triangle_data()),
        aspect_ratio_mode: AspectRatioMode::Ignore,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_polygon_layer_square_ignore_data_units_stroked").await;
}

#[tokio::test]
async fn test_polygon_layer_square_cover_data_units_stroked() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(triangle_data()),
        aspect_ratio_mode: AspectRatioMode::Cover,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_polygon_layer_square_cover_data_units_stroked").await;
}

#[tokio::test]
async fn test_polygon_layer_square_contain_pixel_units_stroked() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(triangle_pixels()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_polygon_layer_square_contain_pixel_units_stroked").await;
}

#[tokio::test]
async fn test_polygon_layer_square_contain_data_units_view_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(triangle_data()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        margin_left: Some(10.0),
        margin_right: Some(10.0),
        margin_top: Some(10.0),
        margin_bottom: Some(10.0),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_polygon_layer_square_contain_data_units_view_margins").await;
}

#[tokio::test]
async fn test_polygon_layer_square_contain_data_units_layer_bounds() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PolygonLayerParams {
            bounds: Some(MarginParams {
                margin_left: Some(10.0),
                margin_right: Some(10.0),
                margin_top: Some(10.0),
                margin_bottom: Some(10.0),
            }),
            ..triangle_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_polygon_layer_square_contain_data_units_layer_bounds").await;
}

// ── Wide canvas (200x100) ──────────────────────────────────────────────────────

#[tokio::test]
async fn test_polygon_layer_wide_contain_data_units_stroked() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(triangle_data()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_polygon_layer_wide_contain_data_units_stroked").await;
}

#[tokio::test]
async fn test_polygon_layer_wide_ignore_data_units_stroked() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(triangle_data()),
        aspect_ratio_mode: AspectRatioMode::Ignore,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_polygon_layer_wide_ignore_data_units_stroked").await;
}

#[tokio::test]
async fn test_polygon_layer_wide_cover_data_units_stroked() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(triangle_data()),
        aspect_ratio_mode: AspectRatioMode::Cover,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_polygon_layer_wide_cover_data_units_stroked").await;
}

#[tokio::test]
async fn test_polygon_layer_wide_contain_data_units_view_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(triangle_data()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        margin_left: Some(10.0),
        margin_right: Some(10.0),
        margin_top: Some(10.0),
        margin_bottom: Some(10.0),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_polygon_layer_wide_contain_data_units_view_margins").await;
}

// ── Tall canvas (100x200) ──────────────────────────────────────────────────────

#[tokio::test]
async fn test_polygon_layer_tall_contain_data_units_stroked() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(triangle_data()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_polygon_layer_tall_contain_data_units_stroked").await;
}

#[tokio::test]
async fn test_polygon_layer_tall_ignore_data_units_stroked() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(triangle_data()),
        aspect_ratio_mode: AspectRatioMode::Ignore,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_polygon_layer_tall_ignore_data_units_stroked").await;
}

#[tokio::test]
async fn test_polygon_layer_tall_cover_data_units_stroked() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(triangle_data()),
        aspect_ratio_mode: AspectRatioMode::Cover,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_polygon_layer_tall_cover_data_units_stroked").await;
}

// ── Fill modes ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_polygon_layer_square_contain_filled_only() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PolygonLayerParams {
            stroked: false,
            filled: true,
            ..triangle_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_polygon_layer_square_contain_filled_only").await;
}

#[tokio::test]
async fn test_polygon_layer_square_contain_stroke_and_fill() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PolygonLayerParams {
            stroked: true,
            filled: true,
            stroke_width: Some(SizeMode::UniformSize(4.0)),
            stroke_color: Some(ColorMode::UniformRgb((255, 0, 0))),
            fill_color: Some(ColorMode::UniformRgb((0, 0, 255))),
            ..triangle_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_polygon_layer_square_contain_stroke_and_fill").await;
}

#[tokio::test]
async fn test_polygon_layer_square_contain_fill_opacity() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PolygonLayerParams {
            stroked: true,
            filled: true,
            stroke_width: Some(SizeMode::UniformSize(4.0)),
            stroke_color: Some(ColorMode::UniformRgb((255, 0, 0))),
            fill_color: Some(ColorMode::UniformRgb((0, 0, 255))),
            stroke_opacity: Some(OpacityMode::UniformOpacity(1.0)),
            fill_opacity: Some(OpacityMode::UniformOpacity(0.5)),
            ..triangle_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_polygon_layer_square_contain_fill_opacity").await;
}

// ── Stroke width ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_polygon_layer_wide_contain_thick_stroke() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(PolygonLayerParams {
            stroke_width: Some(SizeMode::UniformSize(10.0)),
            ..triangle_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_polygon_layer_wide_contain_thick_stroke").await;
}

// Stroke width measured in data-coordinate units (rather than pixels): the
// stroke scales with the view/aspect-ratio transform, mirroring the LineLayer
// data-unit line width.
#[tokio::test]
async fn test_polygon_layer_square_contain_data_units_stroke_width() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PolygonLayerParams {
            stroke_width: Some(SizeMode::UniformSize(0.05)),
            stroke_width_unit_mode: UnitsMode::Data,
            ..triangle_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_polygon_layer_square_contain_data_units_stroke_width").await;
}

#[tokio::test]
async fn test_polygon_layer_wide_contain_data_units_stroke_width() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(PolygonLayerParams {
            stroke_width: Some(SizeMode::UniformSize(0.05)),
            stroke_width_unit_mode: UnitsMode::Data,
            ..triangle_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_polygon_layer_wide_contain_data_units_stroke_width").await;
}

// ── Instanced stroke width / opacity, fill opacity ──────────────────────────────
// The instanced modes supply one value per polygon (uploaded to the GPU as a
// value texture), rather than a single uniform value shared by all polygons.

#[tokio::test]
async fn test_polygon_layer_square_contain_instanced_stroke_width() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PolygonLayerParams {
            // One distinct stroke width (in pixels) per polygon.
            stroke_width: Some(SizeMode::InstancedSize(InstancedSizeParams {
                values: NumericData::Float32(Arc::new(vec![2.0, 8.0])),
            })),
            ..two_triangles_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_polygon_layer_square_contain_instanced_stroke_width").await;
}

#[tokio::test]
async fn test_polygon_layer_square_contain_instanced_stroke_opacity() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PolygonLayerParams {
            stroke_width: Some(SizeMode::UniformSize(4.0)),
            // One distinct stroke opacity per polygon.
            stroke_opacity: Some(OpacityMode::InstancedOpacity(InstancedOpacityParams {
                values: NumericData::Float32(Arc::new(vec![0.25, 1.0])),
            })),
            ..two_triangles_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_polygon_layer_square_contain_instanced_stroke_opacity").await;
}

#[tokio::test]
async fn test_polygon_layer_square_contain_instanced_fill_opacity() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PolygonLayerParams {
            stroked: true,
            filled: true,
            // One distinct fill opacity per polygon.
            fill_opacity: Some(OpacityMode::InstancedOpacity(InstancedOpacityParams {
                values: NumericData::Float32(Arc::new(vec![0.25, 0.9])),
            })),
            ..two_triangles_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_polygon_layer_square_contain_instanced_fill_opacity").await;
}

// ── Multiple polygons ──────────────────────────────────────────────────────────

#[tokio::test]
async fn test_polygon_layer_square_contain_two_polygons_stroked() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(two_triangles_data()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_polygon_layer_square_contain_two_polygons_stroked").await;
}

#[tokio::test]
async fn test_polygon_layer_square_contain_two_polygons_filled() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PolygonLayerParams {
            stroked: false,
            filled: true,
            ..two_triangles_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_polygon_layer_square_contain_two_polygons_filled").await;
}

// One color per polygon (indexed 0, 1, …), via `ColorMode::CategoricalCustom`.
#[tokio::test]
async fn test_polygon_layer_square_contain_two_polygons_categorical() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PolygonLayerParams {
            stroked: true,
            filled: true,
            stroke_color: Some(ColorMode::CategoricalCustom(CategoricalCustomParams {
                values: NumericData::Int32(Arc::new(vec![0, 1])),
                colormap: vec![(255, 0, 0), (0, 0, 255)],
            })),
            fill_color: Some(ColorMode::CategoricalCustom(CategoricalCustomParams {
                values: NumericData::Int32(Arc::new(vec![0, 1])),
                colormap: vec![(255, 200, 200), (200, 200, 255)],
            })),
            ..two_triangles_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_polygon_layer_square_contain_two_polygons_categorical").await;
}

// ── Pentagon shape ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_polygon_layer_square_contain_pentagon_stroke_and_fill() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PolygonLayerParams {
            stroked: true,
            filled: true,
            stroke_width: Some(SizeMode::UniformSize(3.0)),
            stroke_color: Some(ColorMode::UniformRgb((0, 128, 0))),
            fill_color: Some(ColorMode::UniformRgb((0, 204, 0))),
            fill_opacity: Some(OpacityMode::UniformOpacity(0.7)),
            ..quad_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_polygon_layer_square_contain_pentagon_stroke_and_fill").await;
}

// ── model_matrix ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_polygon_layer_square_contain_model_matrix_scale() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PolygonLayerParams {
            model_matrix: Some([
                0.5, 0.0, 0.0, 0.0,
                0.0, 0.5, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                0.0, 0.0, 0.0, 1.0,
            ]),
            ..triangle_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_polygon_layer_square_contain_model_matrix_scale").await;
}
