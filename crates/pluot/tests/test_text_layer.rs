#![cfg(not(target_arch = "wasm32"))]

use std::sync::Arc;

mod test_utils;
use test_utils::render_and_check_both_snapshots;

use pluot::{
    RenderParams, LayerParams,
    AspectRatioMode, UnitsMode, MarginParams,
    TextLayerParams, TextAlignMode, TextBaselineMode,
    FontWeight, FontStyle,
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
        position_x: Arc::new(vec![0.0, 1.0, 1.0, 0.0, 0.5]),
        position_y: Arc::new(vec![0.0, 0.0, 1.0, 1.0, 0.5]),
        text_vec: Arc::new(vec![
            "A".to_string(),
            "B".to_string(),
            "C".to_string(),
            "D".to_string(),
            "Hello world".to_string(),
        ]),
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
        position_x: Arc::new(vec![0.0, 100.0, 100.0, 0.0]),
        position_y: Arc::new(vec![0.0, 0.0, 100.0, 100.0]),
        text_vec: Arc::new(vec![
            "A".to_string(),
            "B".to_string(),
            "C".to_string(),
            "D".to_string(),
        ]),
    }
}

// Helper: text labels with x in [0,1] data space, y in 100px pixel space
fn corner_text_data_x_pixel_y() -> TextLayerParams {
    TextLayerParams {
        data_unit_mode_x: UnitsMode::Data,
        data_unit_mode_y: UnitsMode::Pixels,
        position_x: Arc::new(vec![0.0, 1.0, 1.0, 0.0, 0.5]),
        position_y: Arc::new(vec![0.0, 0.0, 100.0, 100.0, 50.0]),
        ..corner_text_data()
    }
}

// Helper: text labels with x in 100px pixel space, y in [0,1] data space
fn corner_text_pixel_x_data_y() -> TextLayerParams {
    TextLayerParams {
        data_unit_mode_x: UnitsMode::Pixels,
        data_unit_mode_y: UnitsMode::Data,
        position_x: Arc::new(vec![0.0, 100.0, 100.0, 0.0, 50.0]),
        position_y: Arc::new(vec![0.0, 0.0, 1.0, 1.0, 0.5]),
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
            position_x: Arc::new(vec![0.0, 200.0, 200.0, 0.0]),
            position_y: Arc::new(vec![0.0, 0.0, 100.0, 100.0]),
            ..corner_text_pixels()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_wide_contain_pixel_units_no_margins").await;
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
            position_x: Arc::new(vec![0.0, 100.0, 100.0, 0.0]),
            position_y: Arc::new(vec![0.0, 0.0, 200.0, 200.0]),
            ..corner_text_pixels()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_text_layer_tall_contain_pixel_units_no_margins").await;
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
