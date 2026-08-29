#![cfg(not(target_arch = "wasm32"))]

use std::sync::Arc;

mod test_utils;
use test_utils::render_and_check_both_snapshots;

use pluot::{
    RenderParams, LayerParams,
    AspectRatioMode, UnitsMode, MarginParams,
    TextLayerParams, TextAlignMode, TextBaselineMode,
    FontWeight, FontStyle, NumericData, ColorMode,
    CategoricalColormap, CategoricalParams, QuantitativeParams, QuantitativeColormap,
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
//   - For TextLayer, this includes testing different text sizes, alignment modes,
//     baseline modes, and optional rotation

// Absolute path to a vendored TTF used by the custom-font filesystem test.
const NIMBUS_ROMAN_TTF: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../vendor/urw-core35-fonts/NimbusRoman-Regular.ttf",
);

// Helper: 4 text labels at the corners of [0,1]x[0,1] in data space
fn corner_text_data() -> TextLayerParams {
    TextLayerParams {
        layer_id: "my_text_layer".to_string(),
        bounds: None,
        data_unit_mode_x: UnitsMode::Data,
        data_unit_mode_y: UnitsMode::Data,
        text_size: 12.0,
        text_size_unit_mode: UnitsMode::Pixels,
        text_align_mode: TextAlignMode::Middle,
        text_baseline_mode: TextBaselineMode::Middle,
        model_matrix: None,
        text_rotation: None,
        font_family: None,
        font_weight: FontWeight::Normal,
        font_style: FontStyle::Normal,
        fill_color: None,
        position_x: NumericData::Float32(Arc::new(vec![0.0, 1.0, 1.0, 0.0, 0.5])),
        position_y: NumericData::Float32(Arc::new(vec![0.0, 0.0, 1.0, 1.0, 0.5])),
        text_vec: Arc::new(vec![
            "A".to_string(),
            "B".to_string(),
            "C".to_string(),
            "D".to_string(),
            "Hello world".to_string(),
        ]),
        ..Default::default()
    }
}

// Helper: 4 text labels in a 100x100 pixel space
fn corner_text_pixels() -> TextLayerParams {
    TextLayerParams {
        layer_id: "my_text_layer".to_string(),
        bounds: None,
        data_unit_mode_x: UnitsMode::Pixels,
        data_unit_mode_y: UnitsMode::Pixels,
        text_size: 12.0,
        text_size_unit_mode: UnitsMode::Pixels,
        text_align_mode: TextAlignMode::Middle,
        text_baseline_mode: TextBaselineMode::Middle,
        model_matrix: None,
        text_rotation: None,
        font_family: None,
        font_weight: FontWeight::Normal,
        font_style: FontStyle::Normal,
        fill_color: None,
        position_x: NumericData::Float32(Arc::new(vec![0.0, 100.0, 100.0, 0.0])),
        position_y: NumericData::Float32(Arc::new(vec![0.0, 0.0, 100.0, 100.0])),
        text_vec: Arc::new(vec![
            "A".to_string(),
            "B".to_string(),
            "C".to_string(),
            "D".to_string(),
        ]),
        ..Default::default()
    }
}

// Helper: text labels with x in [0,1] data space, y in 100px pixel space
fn corner_text_data_x_pixel_y() -> TextLayerParams {
    TextLayerParams {
        data_unit_mode_x: UnitsMode::Data,
        data_unit_mode_y: UnitsMode::Pixels,
        position_x: NumericData::Float32(Arc::new(vec![0.0, 1.0, 1.0, 0.0, 0.5])),
        position_y: NumericData::Float32(Arc::new(vec![0.0, 0.0, 100.0, 100.0, 50.0])),
        ..corner_text_data()
    }
}

// Helper: text labels with x in 100px pixel space, y in [0,1] data space
fn corner_text_pixel_x_data_y() -> TextLayerParams {
    TextLayerParams {
        data_unit_mode_x: UnitsMode::Pixels,
        data_unit_mode_y: UnitsMode::Data,
        position_x: NumericData::Float32(Arc::new(vec![0.0, 100.0, 100.0, 0.0, 50.0])),
        position_y: NumericData::Float32(Arc::new(vec![0.0, 0.0, 1.0, 1.0, 0.5])),
        ..corner_text_data()
    }
}

// Helper: 4 text labels within a [0,1]x[0,1] normalized space. Uses the same
// fractions as corner_text_pixels() (0.0/100.0 divided down to 0.0/1.0), so on
// a 100x100 canvas this renders identically to corner_text_pixels() while
// remaining agnostic to the layer's actual pixel dimensions (unlike Pixels
// mode, the same params render the same *proportions* on any canvas size).
fn corner_text_normalized() -> TextLayerParams {
    TextLayerParams {
        layer_id: "my_text_layer".to_string(),
        bounds: None,
        data_unit_mode_x: UnitsMode::Normalized,
        data_unit_mode_y: UnitsMode::Normalized,
        text_size: 12.0,
        text_size_unit_mode: UnitsMode::Pixels,
        text_align_mode: TextAlignMode::Middle,
        text_baseline_mode: TextBaselineMode::Middle,
        model_matrix: None,
        text_rotation: None,
        font_family: None,
        font_weight: FontWeight::Normal,
        font_style: FontStyle::Normal,
        fill_color: None,
        position_x: NumericData::Float32(Arc::new(vec![0.0, 1.0, 1.0, 0.0])),
        position_y: NumericData::Float32(Arc::new(vec![0.0, 0.0, 1.0, 1.0])),
        text_vec: Arc::new(vec![
            "A".to_string(),
            "B".to_string(),
            "C".to_string(),
            "D".to_string(),
        ]),
        ..Default::default()
    }
}

// Helper: text labels with x in [0,1] data space, y in [0,1] normalized space
fn corner_text_data_x_normalized_y() -> TextLayerParams {
    TextLayerParams {
        data_unit_mode_x: UnitsMode::Data,
        data_unit_mode_y: UnitsMode::Normalized,
        ..corner_text_data()
    }
}

// Helper: text labels with x in [0,1] normalized space, y in [0,1] data space
fn corner_text_normalized_x_data_y() -> TextLayerParams {
    TextLayerParams {
        data_unit_mode_x: UnitsMode::Normalized,
        data_unit_mode_y: UnitsMode::Data,
        ..corner_text_data()
    }
}

fn layer_params(text_params: TextLayerParams) -> Vec<LayerParams> {
    vec![LayerParams::TextLayer(text_params)]
}

// ── Square canvas (100x100) ───────────────────────────────────────────────────

#[tokio::test]
async fn test_text_layer_square_contain_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(TextLayerParams {
            bounds: Some(MarginParams {
                margin_left: Some(0.0),
                margin_right: Some(0.0),
                margin_top: Some(0.0),
                margin_bottom: Some(0.0),
            }),
            ..corner_text_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_square_contain_data_units_no_margins").await;
}

#[tokio::test]
async fn test_text_layer_square_ignore_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(corner_text_data()),
        aspect_ratio_mode: AspectRatioMode::Ignore,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_square_ignore_data_units_no_margins").await;
}

#[tokio::test]
async fn test_text_layer_square_cover_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(corner_text_data()),
        aspect_ratio_mode: AspectRatioMode::Cover,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_square_cover_data_units_no_margins").await;
}

#[tokio::test]
async fn test_text_layer_square_contain_pixel_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(corner_text_pixels()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_square_contain_pixel_units_no_margins").await;
}

// Normalized units: on a 100x100 canvas this renders identically to the Pixels
// test above, since corner_text_normalized() uses the same fractions (0.0/1.0)
// that corner_text_pixels() uses as absolute pixel values out of 100.
#[tokio::test]
async fn test_text_layer_square_contain_normalized_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(corner_text_normalized()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_square_contain_normalized_units_no_margins").await;
}

#[tokio::test]
async fn test_text_layer_square_contain_data_units_view_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(corner_text_data()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        margin_left: Some(10.0),
        margin_right: Some(10.0),
        margin_top: Some(10.0),
        margin_bottom: Some(10.0),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_square_contain_data_units_view_margins").await;
}

#[tokio::test]
async fn test_text_layer_square_contain_data_units_layer_bounds() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(TextLayerParams {
            bounds: Some(MarginParams {
                margin_left: Some(10.0),
                margin_right: Some(10.0),
                margin_top: Some(10.0),
                margin_bottom: Some(10.0),
            }),
            ..corner_text_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_square_contain_data_units_layer_bounds").await;
}

// Layer bounds take precedence over view margins when both are set
#[tokio::test]
async fn test_text_layer_square_contain_data_units_layer_bounds_overrides_view_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(TextLayerParams {
            bounds: Some(MarginParams {
                margin_left: Some(10.0),
                margin_right: Some(10.0),
                margin_top: Some(10.0),
                margin_bottom: Some(10.0),
            }),
            ..corner_text_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        margin_left: Some(20.0),
        margin_right: Some(20.0),
        margin_top: Some(20.0),
        margin_bottom: Some(20.0),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_square_contain_data_units_layer_bounds_overrides_view_margins").await;
}

// Test text-specific: rotated text
#[tokio::test]
async fn test_text_layer_square_contain_data_units_rotated() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(TextLayerParams {
            text_rotation: Some(45.0),
            ..corner_text_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_square_contain_data_units_rotated").await;
}

// Test text-specific: start alignment, top baseline
#[tokio::test]
async fn test_text_layer_square_contain_data_units_align_start_baseline_top() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(TextLayerParams {
            text_align_mode: TextAlignMode::Start,
            text_baseline_mode: TextBaselineMode::Top,
            ..corner_text_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_square_contain_data_units_align_start_baseline_top").await;
}

// Test text-specific: end alignment, bottom baseline
#[tokio::test]
async fn test_text_layer_square_contain_data_units_align_end_baseline_bottom() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(TextLayerParams {
            text_align_mode: TextAlignMode::End,
            text_baseline_mode: TextBaselineMode::Bottom,
            ..corner_text_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_square_contain_data_units_align_end_baseline_bottom").await;
}

// Wide canvas (200x100)

#[tokio::test]
async fn test_text_layer_wide_ignore_data_units_no_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(corner_text_data()),
        aspect_ratio_mode: AspectRatioMode::Ignore,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_wide_ignore_data_units_no_margins").await;
}

#[tokio::test]
async fn test_text_layer_wide_contain_data_units_no_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(corner_text_data()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_wide_contain_data_units_no_margins").await;
}

#[tokio::test]
async fn test_text_layer_wide_cover_data_units_no_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(corner_text_data()),
        aspect_ratio_mode: AspectRatioMode::Cover,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_wide_cover_data_units_no_margins").await;
}

#[tokio::test]
async fn test_text_layer_wide_contain_pixel_units_no_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(TextLayerParams {
            position_x: NumericData::Float32(Arc::new(vec![0.0, 200.0, 200.0, 0.0])),
            position_y: NumericData::Float32(Arc::new(vec![0.0, 0.0, 100.0, 100.0])),
            ..corner_text_pixels()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_wide_contain_pixel_units_no_margins").await;
}

// Normalized units on a wide canvas: unlike the Pixels test above (which needs
// its own position overrides rescaled to the 200px width), corner_text_normalized()
// is reused completely unchanged from the square-canvas test, since its 0-1
// fractions are agnostic to the layer's actual pixel dimensions.
#[tokio::test]
async fn test_text_layer_wide_contain_normalized_units_no_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(corner_text_normalized()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_wide_contain_normalized_units_no_margins").await;
}

#[tokio::test]
async fn test_text_layer_wide_contain_data_units_view_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(corner_text_data()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        margin_left: Some(10.0),
        margin_right: Some(10.0),
        margin_top: Some(10.0),
        margin_bottom: Some(10.0),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_wide_contain_data_units_view_margins").await;
}

#[tokio::test]
async fn test_text_layer_wide_contain_data_units_layer_bounds() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(TextLayerParams {
            bounds: Some(MarginParams {
                margin_left: Some(10.0),
                margin_right: Some(10.0),
                margin_top: Some(10.0),
                margin_bottom: Some(10.0),
            }),
            ..corner_text_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_wide_contain_data_units_layer_bounds").await;
}

// Tall canvas (100x200)

#[tokio::test]
async fn test_text_layer_tall_ignore_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(corner_text_data()),
        aspect_ratio_mode: AspectRatioMode::Ignore,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_tall_ignore_data_units_no_margins").await;
}

#[tokio::test]
async fn test_text_layer_tall_contain_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(corner_text_data()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_tall_contain_data_units_no_margins").await;
}

#[tokio::test]
async fn test_text_layer_tall_cover_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(corner_text_data()),
        aspect_ratio_mode: AspectRatioMode::Cover,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_tall_cover_data_units_no_margins").await;
}

#[tokio::test]
async fn test_text_layer_tall_contain_pixel_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(TextLayerParams {
            position_x: NumericData::Float32(Arc::new(vec![0.0, 100.0, 100.0, 0.0])),
            position_y: NumericData::Float32(Arc::new(vec![0.0, 0.0, 200.0, 200.0])),
            ..corner_text_pixels()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_tall_contain_pixel_units_no_margins").await;
}

// Normalized units on a tall canvas: again reusing corner_text_normalized()
// unchanged, demonstrating pixel-dimension independence.
#[tokio::test]
async fn test_text_layer_tall_contain_normalized_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(corner_text_normalized()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_tall_contain_normalized_units_no_margins").await;
}

#[tokio::test]
async fn test_text_layer_tall_contain_data_units_view_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(corner_text_data()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        margin_left: Some(10.0),
        margin_right: Some(10.0),
        margin_top: Some(10.0),
        margin_bottom: Some(10.0),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_tall_contain_data_units_view_margins").await;
}

#[tokio::test]
async fn test_text_layer_tall_contain_data_units_layer_bounds() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(TextLayerParams {
            bounds: Some(MarginParams {
                margin_left: Some(10.0),
                margin_right: Some(10.0),
                margin_top: Some(10.0),
                margin_bottom: Some(10.0),
            }),
            ..corner_text_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_tall_contain_data_units_layer_bounds").await;
}

// ── Wide canvas rotation tests ────────────────────────────────────────────────

#[tokio::test]
async fn test_text_layer_wide_contain_data_units_rotated_45() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(TextLayerParams {
            text_rotation: Some(45.0),
            ..corner_text_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_wide_contain_data_units_rotated_45").await;
}

#[tokio::test]
async fn test_text_layer_wide_contain_data_units_rotated_90() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(TextLayerParams {
            text_rotation: Some(90.0),
            ..corner_text_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_wide_contain_data_units_rotated_90").await;
}

// ── Mixed unit modes (data_unit_mode_x ≠ data_unit_mode_y) ───────────────────

#[tokio::test]
async fn test_text_layer_square_contain_data_x_pixel_y_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(corner_text_data_x_pixel_y()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_square_contain_data_x_pixel_y_no_margins").await;
}

#[tokio::test]
async fn test_text_layer_square_contain_pixel_x_data_y_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(corner_text_pixel_x_data_y()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_square_contain_pixel_x_data_y_no_margins").await;
}

#[tokio::test]
async fn test_text_layer_square_contain_data_x_normalized_y_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(corner_text_data_x_normalized_y()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_square_contain_data_x_normalized_y_no_margins").await;
}

#[tokio::test]
async fn test_text_layer_square_contain_normalized_x_data_y_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(corner_text_normalized_x_data_y()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_square_contain_normalized_x_data_y_no_margins").await;
}

// Font loading

// PDF Base-14 font name resolved via the embedded URW font map.
// Requires the `embed_fonts` feature so that the plain-Rust binding can resolve
// "Helvetica" to the embedded NimbusSans-Regular bytes without a filesystem hit.
#[cfg(feature = "embed_fonts")]
#[tokio::test]
async fn test_text_layer_pdf_base14_font_helvetica() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(TextLayerParams {
            font_family: Some("Helvetica".to_string()),
            font_weight: FontWeight::Normal,
            font_style: FontStyle::Normal,
            ..corner_text_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_pdf_base14_font_helvetica").await;
}

// model_matrix

// Scale 0.5 in data mode: text labels shrink to lower-left quadrant of the unit square.
#[tokio::test]
async fn test_text_layer_square_contain_data_units_model_matrix_scale() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(TextLayerParams {
            model_matrix: Some([
                0.5, 0.0, 0.0, 0.0,
                0.0, 0.5, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                0.0, 0.0, 0.0, 1.0,
            ]),
            ..corner_text_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_square_contain_data_units_model_matrix_scale").await;
}

// Translate +0.25 in data mode: text labels shift toward upper-right.
#[tokio::test]
async fn test_text_layer_square_contain_data_units_model_matrix_translate() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(TextLayerParams {
            model_matrix: Some([
                1.0,  0.0,  0.0, 0.0,
                0.0,  1.0,  0.0, 0.0,
                0.0,  0.0,  1.0, 0.0,
                0.25, 0.25, 0.0, 1.0,
            ]),
            ..corner_text_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_square_contain_data_units_model_matrix_translate").await;
}

// Scale 0.5 in pixel mode: model_matrix operates in normalized [0,1] space.
#[tokio::test]
async fn test_text_layer_square_contain_pixel_units_model_matrix_scale() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(TextLayerParams {
            model_matrix: Some([
                0.5, 0.0, 0.0, 0.0,
                0.0, 0.5, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                0.0, 0.0, 0.0, 1.0,
            ]),
            ..corner_text_pixels()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_square_contain_pixel_units_model_matrix_scale").await;
}

// Scale 0.5 in normalized mode: like pixel mode, model_matrix operates in
// normalized [0,1] space, so this should render identically to the pixel-mode
// model-matrix-scale test above (on a 100x100 canvas, where they coincide).
#[tokio::test]
async fn test_text_layer_square_contain_normalized_units_model_matrix_scale() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(TextLayerParams {
            model_matrix: Some([
                0.5, 0.0, 0.0, 0.0,
                0.0, 0.5, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                0.0, 0.0, 0.0, 1.0,
            ]),
            ..corner_text_normalized()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_square_contain_normalized_units_model_matrix_scale").await;
}

// ── Fill color modes ──────────────────────────────────────────────────────────

#[tokio::test]
async fn test_text_layer_square_contain_data_units_categorical_color() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(TextLayerParams {
            fill_color: Some(ColorMode::Categorical(CategoricalParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3, 4])),
                colormap: CategoricalColormap::Tableau10,
            })),
            ..corner_text_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_square_contain_data_units_categorical_color").await;
}

#[tokio::test]
async fn test_text_layer_square_contain_data_units_quantitative_color() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(TextLayerParams {
            fill_color: Some(ColorMode::Quantitative(QuantitativeParams {
                values: NumericData::Float32(Arc::new(vec![0.0, 0.25, 0.5, 0.75, 1.0])),
                colormap: QuantitativeColormap::Viridis,
                reverse: false,
                domain: None,
            })),
            ..corner_text_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_square_contain_data_units_quantitative_color").await;
}

// ── Filtering and selection criteria ─────────────────────────────────────────
// Filter-excluded text elements are not rendered at all; filter-included but
// selection-excluded ("background") text elements still render, but
// re-colored with `background_fill_color` in place of their configured fill
// color.

// Helper: `corner_text_data()` with a categorical `fill_color` so that each
// of the 5 text elements (A, B, C, D, "Hello world") has a distinct color,
// making filtering/selection subsets easy to distinguish.
fn criteria_text_data() -> TextLayerParams {
    TextLayerParams {
        fill_color: Some(ColorMode::Categorical(CategoricalParams {
            codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3, 4])),
            colormap: CategoricalColormap::Tableau10,
        })),
        ..corner_text_data()
    }
}

// Categorical filtering: only text elements whose category code is in
// `included_codes` are rendered at all. Reuses the same codes as
// `fill_color` (0-4, one per element), including only codes 0 and 2, so
// only "A" (bottom-left) and "C" (top-right) render.
#[tokio::test]
async fn test_text_layer_square_contain_filtering_categorical_subset() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(TextLayerParams {
            filtering_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3, 4])),
                included_codes: vec![0, 2],
            })],
            ..criteria_text_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_square_contain_filtering_categorical_subset").await;
}

// An explicit empty `included_codes` list means nothing is included: no text
// elements render at all (distinct from an empty `filtering_criteria` list,
// which includes everything).
#[tokio::test]
async fn test_text_layer_square_contain_filtering_categorical_empty_excludes_all() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(TextLayerParams {
            filtering_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3, 4])),
                included_codes: vec![],
            })],
            ..criteria_text_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_square_contain_filtering_categorical_empty_excludes_all").await;
}

// Quantitative filtering with both a min and a max bound: a per-element value
// column of [0, 1, 2, 3, 4] filtered to the inclusive range [1, 3] includes
// only "B", "C", and "D".
#[tokio::test]
async fn test_text_layer_square_contain_filtering_quantitative_range() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(TextLayerParams {
            filtering_criteria: vec![EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
                values: NumericData::Float32(Arc::new(vec![0.0, 1.0, 2.0, 3.0, 4.0])),
                min: Some(1.0),
                max: Some(3.0),
            })],
            ..criteria_text_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_square_contain_filtering_quantitative_range").await;
}

// Quantitative filtering with only a `min` bound: `max` is omitted, meaning
// +infinity, so every element with value >= 3 is included ("D" and "Hello world").
#[tokio::test]
async fn test_text_layer_square_contain_filtering_quantitative_min_only() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(TextLayerParams {
            filtering_criteria: vec![EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
                values: NumericData::Float32(Arc::new(vec![0.0, 1.0, 2.0, 3.0, 4.0])),
                min: Some(3.0),
                max: None,
            })],
            ..criteria_text_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_square_contain_filtering_quantitative_min_only").await;
}

// Categorical selection: unlike filtering, selection-excluded text elements
// still render (all 5 are visible), but elements whose code is not in
// `included_codes` (1, 3, 4) are re-colored with `background_fill_color`
// instead of their categorical `fill_color`.
#[tokio::test]
async fn test_text_layer_square_contain_selection_categorical_subset() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(TextLayerParams {
            selection_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3, 4])),
                included_codes: vec![0, 2],
            })],
            ..criteria_text_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_square_contain_selection_categorical_subset").await;
}

// An explicit empty `included_codes` list for selection means nothing is
// selected: all 5 text elements still render (filtering_criteria is empty),
// but every one is de-emphasized with `background_fill_color`.
#[tokio::test]
async fn test_text_layer_square_contain_selection_categorical_empty_deemphasizes_all() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(TextLayerParams {
            selection_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3, 4])),
                included_codes: vec![],
            })],
            ..criteria_text_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_square_contain_selection_categorical_empty_deemphasizes_all").await;
}

// Quantitative selection: a value column of [0, 10, 20, 30, 40] selected to
// the range [10, 30] renders "B", "C", "D" with their normal fill color and
// de-emphasizes "A" and "Hello world" with `background_fill_color`.
#[tokio::test]
async fn test_text_layer_square_contain_selection_quantitative_range() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(TextLayerParams {
            selection_criteria: vec![EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
                values: NumericData::Float32(Arc::new(vec![0.0, 10.0, 20.0, 30.0, 40.0])),
                min: Some(10.0),
                max: Some(30.0),
            })],
            ..criteria_text_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_square_contain_selection_quantitative_range").await;
}

// Selection criteria may be entirely orthogonal to filtering criteria: here
// filtering uses the same categorical codes as `fill_color` (excluding code
// 4, so "Hello world" is not rendered at all), while selection uses an
// unrelated quantitative column. Of the 4 filter-included elements, the ones
// with value >= 15 (indices 1 and 2, "B" and "C") are selected (normal
// color); index 0 ("A") is filter-included but selection-excluded
// (background color); index 4 is filter-excluded and not rendered
// regardless of its selection value.
#[tokio::test]
async fn test_text_layer_square_contain_selection_orthogonal_to_filtering() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(TextLayerParams {
            filtering_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3, 4])),
                included_codes: vec![0, 1, 2, 3],
            })],
            selection_criteria: vec![EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
                values: NumericData::Float32(Arc::new(vec![5.0, 25.0, 20.0, 8.0, 50.0])),
                min: Some(15.0),
                max: None,
            })],
            ..criteria_text_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_square_contain_selection_orthogonal_to_filtering").await;
}

// `filtering_criteria` is a list of criteria AND-ed together: a text element
// must satisfy every one to be included. Here a categorical criteria
// (excluding index 4) is combined with a quantitative criteria (min 2,
// excluding indices 0/1). Only indices 2 and 3 ("C" and "D") satisfy both.
#[tokio::test]
async fn test_text_layer_square_contain_filtering_multiple_criteria_and() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(TextLayerParams {
            filtering_criteria: vec![
                EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                    codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3, 4])),
                    included_codes: vec![0, 1, 2, 3],
                }),
                EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
                    values: NumericData::Float32(Arc::new(vec![0.0, 1.0, 2.0, 3.0, 4.0])),
                    min: Some(2.0),
                    max: None,
                }),
            ],
            ..criteria_text_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_square_contain_filtering_multiple_criteria_and").await;
}

// `selection_criteria` AND-ing mirrors `filtering_criteria`: a categorical
// criteria (included_codes 0-3) combined with a quantitative criteria (min
// 2, excluding indices 0/1) leaves only indices 2/3 selected (normal color);
// every other element still renders (no filtering), but de-emphasized with
// `background_fill_color`.
#[tokio::test]
async fn test_text_layer_square_contain_selection_multiple_criteria_and() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(TextLayerParams {
            selection_criteria: vec![
                EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                    codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3, 4])),
                    included_codes: vec![0, 1, 2, 3],
                }),
                EmphasisCriteria::Quantitative(QuantitativeCriteriaParams {
                    values: NumericData::Float32(Arc::new(vec![0.0, 1.0, 2.0, 3.0, 4.0])),
                    min: Some(2.0),
                    max: None,
                }),
            ],
            ..criteria_text_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_square_contain_selection_multiple_criteria_and").await;
}

// Custom background fill color: "B" and "D" (indices 1, 3) are selected
// (normal categorical fill color); the rest are selection-excluded and
// rendered with a magenta background fill instead.
#[tokio::test]
async fn test_text_layer_square_contain_selection_custom_background_color() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(TextLayerParams {
            background_fill_color: Some((255, 0, 255)),
            selection_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3, 4])),
                included_codes: vec![1, 3],
            })],
            ..criteria_text_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_square_contain_selection_custom_background_color").await;
}

// `enable_background_fill_color: false` disables the (otherwise default-on)
// fill-color de-emphasis: every text element keeps its normal categorical
// fill color even though "A" and "C" (indices 0, 2) are selection-excluded.
// `TextLayerParams` has no opacity/stroke/size modes to gate, so this is the
// only background override this layer supports.
#[tokio::test]
async fn test_text_layer_square_contain_selection_disable_background_fill_color() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(TextLayerParams {
            enable_background_fill_color: false,
            selection_criteria: vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
                codes: NumericData::Int32(Arc::new(vec![0, 1, 2, 3, 4])),
                included_codes: vec![1, 3],
            })],
            ..criteria_text_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_square_contain_selection_disable_background_fill_color").await;
}

/*
// TODO: re-enable after #207 is complete
// Custom TTF supplied as a filesystem path.
#[tokio::test]
async fn test_text_layer_custom_ttf_font_file() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(TextLayerParams {
            font_family: Some(NIMBUS_ROMAN_TTF.to_string()),
            font_weight: FontWeight::Normal,
            font_style: FontStyle::Normal,
            ..corner_text_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_custom_ttf_font_file").await;
}
*/
