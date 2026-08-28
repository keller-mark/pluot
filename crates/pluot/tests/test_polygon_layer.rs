#![cfg(not(target_arch = "wasm32"))]

use std::sync::Arc;

mod test_utils;
use test_utils::render_and_check_both_snapshots;

use pluot::{
    RenderParams, LayerParams,
    AspectRatioMode, UnitsMode, MarginParams,
    CategoricalColormap, CategoricalParams, CategoricalCustomParams, ColorMode, PolygonLayerParams, NumericData,
    SizeMode, OpacityMode, InstancedSizeParams, InstancedOpacityParams,
    EmphasisCriteria, CategoricalCriteriaParams, QuantitativeCriteriaParams,
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
        ..Default::default()
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

/// Triangle in normalized [0,1] space (bottom-left=0, top-right=1). Uses the
/// same fractions as triangle_pixels()'s pixel coordinates divided by the
/// 100x100 canvas triangle_pixels() assumes, so on a 100x100 canvas this
/// renders identically to triangle_pixels() while remaining agnostic to the
/// layer's actual pixel dimensions (unlike Pixels mode, the same params
/// render the same *proportions* on any canvas size).
fn triangle_normalized() -> PolygonLayerParams {
    PolygonLayerParams {
        data_unit_mode_x: UnitsMode::Normalized,
        data_unit_mode_y: UnitsMode::Normalized,
        polygons: NumericData::Float32(Arc::new(vec![
            0.1, 0.1,
            0.9, 0.1,
            0.5, 0.9,
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

// Normalized units: on a 100x100 canvas this renders identically to the Pixels
// test above, since triangle_normalized() uses the same fractions (0.1/0.9/0.5)
// that triangle_pixels() uses as absolute pixel values out of 100.
#[tokio::test]
async fn test_polygon_layer_square_contain_normalized_units_stroked() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(triangle_normalized()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_polygon_layer_square_contain_normalized_units_stroked").await;
}

// Normalized units on a wide (200x100) canvas: unlike Pixels mode (which would
// need its coordinates rescaled to the new canvas width), triangle_normalized()
// is reused completely unchanged from the square-canvas test above, since its
// 0-1 fractions are agnostic to the layer's actual pixel dimensions.
#[tokio::test]
async fn test_polygon_layer_wide_contain_normalized_units_stroked() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(triangle_normalized()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_polygon_layer_wide_contain_normalized_units_stroked").await;
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

// Stroke width as a fraction (0 to 1) of the layer height (normalized units,
// camera-independent). 0.02 * 100px == 2px, matching the 2px border used by
// triangle_normalized()'s default (Pixels) stroke width above.
#[tokio::test]
async fn test_polygon_layer_square_contain_normalized_units_stroke_width() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PolygonLayerParams {
            stroke_width: Some(SizeMode::UniformSize(0.02)),
            stroke_width_unit_mode: UnitsMode::Normalized,
            ..triangle_normalized()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_polygon_layer_square_contain_normalized_units_stroke_width").await;
}

// Same normalized stroke width (0.02) on a taller (100x200) canvas: since it is
// height-relative, the border renders at 0.02 * 200px == 4px, twice as thick as
// the square-canvas test above, demonstrating the height-relative scaling.
#[tokio::test]
async fn test_polygon_layer_tall_contain_normalized_units_stroke_width() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(PolygonLayerParams {
            stroke_width: Some(SizeMode::UniformSize(0.02)),
            stroke_width_unit_mode: UnitsMode::Normalized,
            ..triangle_normalized()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_polygon_layer_tall_contain_normalized_units_stroke_width").await;
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

// Scale 0.5 in normalized mode: like pixel mode, model_matrix operates in
// normalized [0,1] space, so this shrinks the triangle into the lower-left
// quadrant, analogous to the data-units model-matrix-scale test above.
#[tokio::test]
async fn test_polygon_layer_square_contain_normalized_units_model_matrix_scale() {
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
            ..triangle_normalized()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_polygon_layer_square_contain_normalized_units_model_matrix_scale").await;
}

// ── Filtering and selection criteria ─────────────────────────────────────────
// Filter-excluded polygons are not rendered at all; filter-included but
// selection-excluded ("background") polygons still render, but re-colored
// with `background_fill_color`/`background_stroke_color` in place of their
// configured fill/stroke color.

// Helper: `two_triangles_data()`'s two triangles, stroked and filled, with a
// categorical fill color (one per polygon) so filtering/selection subsets
// are easy to distinguish.
fn criteria_polygons_data() -> PolygonLayerParams {
    PolygonLayerParams {
        stroked: true,
        filled: true,
        stroke_color: Some(ColorMode::UniformRgb((0, 0, 0))),
        fill_color: Some(ColorMode::Categorical(CategoricalParams {
            codes: NumericData::Int32(Arc::new(vec![0, 1])),
            colormap: CategoricalColormap::Tableau10,
        })),
        ..two_triangles_data()
    }
}

// Categorical filtering: only polygons whose category code is in
// `included_codes` are rendered at all. Reuses the same codes as
// `fill_color` (0 and 1, one per triangle), including only code 0, so only
// the first triangle renders.
#[tokio::test]
async fn test_polygon_layer_square_contain_filtering_categorical_subset() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PolygonLayerParams {
            filtering_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1])),
                included_codes: vec![0],
            })],
            ..criteria_polygons_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_polygon_layer_square_contain_filtering_categorical_subset").await;
}

// An explicit empty `included_codes` list means nothing is included: no
// polygons render at all (distinct from an empty `filtering_criteria` list,
// which includes everything).
#[tokio::test]
async fn test_polygon_layer_square_contain_filtering_categorical_empty_excludes_all() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PolygonLayerParams {
            filtering_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1])),
                included_codes: vec![],
            })],
            ..criteria_polygons_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_polygon_layer_square_contain_filtering_categorical_empty_excludes_all").await;
}

// Quantitative filtering with both a min and a max bound: a per-polygon value
// column of [0, 1] filtered to the inclusive range [0, 0] includes only the
// first triangle.
#[tokio::test]
async fn test_polygon_layer_square_contain_filtering_quantitative_range() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PolygonLayerParams {
            filtering_criteria: vec![EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
                values: NumericData::Float32(Arc::new(vec![0.0, 1.0])),
                min: Some(0.0),
                max: Some(0.0),
            })],
            ..criteria_polygons_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_polygon_layer_square_contain_filtering_quantitative_range").await;
}

// Quantitative filtering with only a `min` bound: `max` is omitted, meaning
// +infinity, so only the second triangle (value 1) is included.
#[tokio::test]
async fn test_polygon_layer_square_contain_filtering_quantitative_min_only() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PolygonLayerParams {
            filtering_criteria: vec![EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
                values: NumericData::Float32(Arc::new(vec![0.0, 1.0])),
                min: Some(1.0),
                max: None,
            })],
            ..criteria_polygons_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_polygon_layer_square_contain_filtering_quantitative_min_only").await;
}

// Categorical selection: unlike filtering, selection-excluded polygons still
// render (both triangles are visible), but the polygon whose code is not in
// `included_codes` (1) is re-colored with `background_fill_color`/
// `background_stroke_color` instead of its configured fill/stroke color.
#[tokio::test]
async fn test_polygon_layer_square_contain_selection_categorical_subset() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PolygonLayerParams {
            selection_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1])),
                included_codes: vec![0],
            })],
            ..criteria_polygons_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_polygon_layer_square_contain_selection_categorical_subset").await;
}

// An explicit empty `included_codes` list for selection means nothing is
// selected: both polygons still render (filtering_criteria is empty), but
// every one is de-emphasized with `background_fill_color`/
// `background_stroke_color`.
#[tokio::test]
async fn test_polygon_layer_square_contain_selection_categorical_empty_deemphasizes_all() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PolygonLayerParams {
            selection_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1])),
                included_codes: vec![],
            })],
            ..criteria_polygons_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_polygon_layer_square_contain_selection_categorical_empty_deemphasizes_all").await;
}

// Quantitative selection: a value column of [0, 1] selected to the range
// [1, 1] renders the second triangle with its normal fill/stroke color and
// de-emphasizes the first with `background_fill_color`/
// `background_stroke_color`.
#[tokio::test]
async fn test_polygon_layer_square_contain_selection_quantitative_range() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PolygonLayerParams {
            selection_criteria: vec![EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
                values: NumericData::Float32(Arc::new(vec![0.0, 1.0])),
                min: Some(1.0),
                max: Some(1.0),
            })],
            ..criteria_polygons_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_polygon_layer_square_contain_selection_quantitative_range").await;
}

// Selection criteria may be entirely orthogonal to filtering criteria: here
// filtering uses the same categorical codes as `fill_color`, including both
// polygons, while selection uses an unrelated quantitative column. The first
// triangle (value 5) is filter-included but selection-excluded (background
// color); the second (value 25) is both filter- and selection-included
// (normal color).
#[tokio::test]
async fn test_polygon_layer_square_contain_selection_orthogonal_to_filtering() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PolygonLayerParams {
            filtering_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1])),
                included_codes: vec![0, 1],
            })],
            selection_criteria: vec![EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
                values: NumericData::Float32(Arc::new(vec![5.0, 25.0])),
                min: Some(10.0),
                max: None,
            })],
            ..criteria_polygons_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_polygon_layer_square_contain_selection_orthogonal_to_filtering").await;
}

// `filtering_criteria` is a list of criteria AND-ed together: a polygon must
// satisfy every one to be included. Here a categorical criteria (codes 0,1,
// including both) is combined with a quantitative criteria (values 0,25,
// min 10 — excludes the first triangle). Only the second triangle satisfies
// both, so only it renders.
#[tokio::test]
async fn test_polygon_layer_square_contain_filtering_multiple_criteria_and() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PolygonLayerParams {
            filtering_criteria: vec![
                EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                    codes: NumericData::Int32(Arc::new(vec![0, 1])),
                    included_codes: vec![0, 1],
                }),
                EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
                    values: NumericData::Float32(Arc::new(vec![0.0, 25.0])),
                    min: Some(10.0),
                    max: None,
                }),
            ],
            ..criteria_polygons_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_polygon_layer_square_contain_filtering_multiple_criteria_and").await;
}

// `selection_criteria` AND-ing mirrors `filtering_criteria`: a categorical
// criteria (included_codes 0,1) combined with a quantitative criteria (min
// 10, excluding the first triangle) leaves only the second triangle
// selected (normal color); the first still renders (no filtering), but
// de-emphasized with `background_fill_color`/`background_stroke_color`.
#[tokio::test]
async fn test_polygon_layer_square_contain_selection_multiple_criteria_and() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PolygonLayerParams {
            selection_criteria: vec![
                EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                    codes: NumericData::Int32(Arc::new(vec![0, 1])),
                    included_codes: vec![0, 1],
                }),
                EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
                    values: NumericData::Float32(Arc::new(vec![0.0, 25.0])),
                    min: Some(10.0),
                    max: None,
                }),
            ],
            ..criteria_polygons_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_polygon_layer_square_contain_selection_multiple_criteria_and").await;
}

// Custom background fill/stroke colors: the first triangle is selected
// (normal categorical fill + black stroke); the second is selection-excluded
// and rendered with a red background fill and a green background stroke
// instead.
#[tokio::test]
async fn test_polygon_layer_square_contain_selection_custom_background_colors() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(PolygonLayerParams {
            background_fill_color: Some((255, 0, 0)),
            background_stroke_color: Some((0, 255, 0)),
            selection_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1])),
                included_codes: vec![0],
            })],
            ..criteria_polygons_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_polygon_layer_square_contain_selection_custom_background_colors").await;
}
