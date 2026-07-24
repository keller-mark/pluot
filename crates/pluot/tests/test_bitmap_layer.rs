#![cfg(not(target_arch = "wasm32"))]

use std::sync::Arc;

mod test_utils;
use test_utils::render_and_check_both_snapshots;

use pluot::{
    RenderParams, LayerParams,
    AspectRatioMode, UnitsMode, MarginParams,
    BitmapLayerParams, ChannelSettings, DimensionOrder, NumericData,
};

// For bitmap layer tests, we always want to test the following cases (and combinations of them):
// - Square and non-square (wide and tall) aspect ratios
// - Each aspect ratio mode (ignore, contain, cover)
// - Both data and pixel data_unit_modes
// - With and without margins at the view level
// - With and without margins (bounds) at the layer level
// - Raster and vector (which the helper function already handles for us)
// - Layer-specific stuff
//   - For BitmapLayer, this includes testing different dimension orders,
//     different channel settings (colors, windows), opacity, and pixel_offset

// Helper: a 4x4 two-channel image in CYX order (matches the JS demo)
// Channel 0 (red): low values, Channel 1 (blue): high values
fn bitmap_cyx_data() -> BitmapLayerParams {
    BitmapLayerParams {
        layer_id: "my_bitmap_layer".to_string(),
        bounds: None,
        data_unit_mode_x: UnitsMode::Data,
        data_unit_mode_y: UnitsMode::Data,
        pixel_offset: None,
        model_matrix: None,
        dimension_order: DimensionOrder::CYX,
        shape: vec![2, 4, 4],
        channel_settings: vec![
            ChannelSettings { window: (0.0, 500.0), color: (1.0, 0.0, 0.0) },
            ChannelSettings { window: (0.0, 500.0), color: (0.0, 0.0, 1.0) },
        ],
        opacity: 1.0,
        data: NumericData::Uint16(Arc::new(vec![
            //  Channel 0 (red), row-major YX
              0, 110, 210, 310,
             20, 120, 220, 320,
             30, 130, 230, 330,
             40, 140, 240, 340,
            // Channel 1 (blue), row-major YX
            300, 110, 210, 310,
             20, 120, 220, 320,
             30, 130, 230, 330,
             40, 140, 240,   0,
        ])),
    }
}

// Helper: same image in Pixels unit mode (4x4 pixel image positioned in pixel space)
fn bitmap_cyx_pixels() -> BitmapLayerParams {
    BitmapLayerParams {
        data_unit_mode_x: UnitsMode::Pixels,
        data_unit_mode_y: UnitsMode::Pixels,
        ..bitmap_cyx_data()
    }
}

fn bitmap_cyx_data_x_pixel_y() -> BitmapLayerParams {
    BitmapLayerParams {
        data_unit_mode_x: UnitsMode::Data,
        data_unit_mode_y: UnitsMode::Pixels,
        ..bitmap_cyx_data()
    }
}

fn bitmap_cyx_pixel_x_data_y() -> BitmapLayerParams {
    BitmapLayerParams {
        data_unit_mode_x: UnitsMode::Pixels,
        data_unit_mode_y: UnitsMode::Data,
        ..bitmap_cyx_data()
    }
}

// Helper: same image in Normalized unit mode. Unlike RectLayer's explicit
// position fields (which can just be re-expressed as 0-1 fractions), the
// bitmap layer's position/size come from `pixel_offset` and the image's
// `shape` (always in native image-pixel units), and bitmap_layer.wgsl does
// NOT divide these by the layer size in Normalized mode (it only skips that
// division, unlike Pixels mode) -- so a raw img_size of 4x4 would be
// interpreted as 4x the layer's normalized (0,1) extent, way off-canvas.
// A model_matrix scale is the mechanism to bring it into (0,1) space.
// Scaling by 0.01 shrinks the 4x4 image to a 0.04x0.04 normalized extent,
// which matches bitmap_cyx_pixels()'s 4px / 100px layer size exactly on a
// 100x100 canvas, so this renders identically to bitmap_cyx_pixels() there.
// Unlike Pixels mode (whose apparent size is a fraction of the *canvas'*
// absolute pixel dimensions), this stays at exactly 4% of the layer's width
// and height on any canvas size, since Normalized mode is not divided by
// layer size at all.
fn bitmap_cyx_normalized() -> BitmapLayerParams {
    BitmapLayerParams {
        data_unit_mode_x: UnitsMode::Normalized,
        data_unit_mode_y: UnitsMode::Normalized,
        model_matrix: Some([
            0.01, 0.0, 0.0, 0.0,
            0.0, 0.01, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ]),
        ..bitmap_cyx_data()
    }
}

fn bitmap_cyx_data_x_normalized_y() -> BitmapLayerParams {
    BitmapLayerParams {
        data_unit_mode_x: UnitsMode::Data,
        data_unit_mode_y: UnitsMode::Normalized,
        ..bitmap_cyx_data()
    }
}

fn bitmap_cyx_normalized_x_data_y() -> BitmapLayerParams {
    BitmapLayerParams {
        data_unit_mode_x: UnitsMode::Normalized,
        data_unit_mode_y: UnitsMode::Data,
        ..bitmap_cyx_data()
    }
}

fn layer_params(bitmap_params: BitmapLayerParams) -> Vec<LayerParams> {
    vec![LayerParams::BitmapLayer(bitmap_params)]
}

// Column-major 4x4 scale matrix: zoom of 1/8 (zoomed out 8x), centered at origin.
// Format matches position_utils.rs: [scale, 0, 0, 0, 0, scale, 0, 0, 0, 0, 0, 0, tx, ty, 0, 1]
const CAMERA_ZOOM_OUT_8X: [f32; 16] = [
    0.125, 0.0,   0.0, 0.0,
    0.0,   0.125, 0.0, 0.0,
    0.0,   0.0,   0.0, 0.0,
    0.0,   0.0,   0.0, 1.0,
];

// ── Square canvas (100x100) ───────────────────────────────────────────────────

#[tokio::test]
async fn test_bitmap_layer_square_contain_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(BitmapLayerParams {
            bounds: Some(MarginParams {
                margin_left: Some(0.0),
                margin_right: Some(0.0),
                margin_top: Some(0.0),
                margin_bottom: Some(0.0),
            }),
            ..bitmap_cyx_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmap_layer_square_contain_data_units_no_margins").await;
}

#[tokio::test]
async fn test_bitmap_layer_square_ignore_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(bitmap_cyx_data()),
        aspect_ratio_mode: AspectRatioMode::Ignore,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmap_layer_square_ignore_data_units_no_margins").await;
}

#[tokio::test]
async fn test_bitmap_layer_square_cover_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(bitmap_cyx_data()),
        aspect_ratio_mode: AspectRatioMode::Cover,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmap_layer_square_cover_data_units_no_margins").await;
}

#[tokio::test]
async fn test_bitmap_layer_square_contain_pixel_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(bitmap_cyx_pixels()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmap_layer_square_contain_pixel_units_no_margins").await;
}

#[tokio::test]
async fn test_bitmap_layer_square_contain_data_units_view_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(bitmap_cyx_data()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        margin_left: Some(10.0),
        margin_right: Some(10.0),
        margin_top: Some(10.0),
        margin_bottom: Some(10.0),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmap_layer_square_contain_data_units_view_margins").await;
}

#[tokio::test]
async fn test_bitmap_layer_square_contain_data_units_layer_bounds() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(BitmapLayerParams {
            bounds: Some(MarginParams {
                margin_left: Some(10.0),
                margin_right: Some(10.0),
                margin_top: Some(10.0),
                margin_bottom: Some(10.0),
            }),
            ..bitmap_cyx_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmap_layer_square_contain_data_units_layer_bounds").await;
}

// Layer bounds take precedence over view margins when both are set
#[tokio::test]
async fn test_bitmap_layer_square_contain_data_units_layer_bounds_overrides_view_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(BitmapLayerParams {
            bounds: Some(MarginParams {
                margin_left: Some(10.0),
                margin_right: Some(10.0),
                margin_top: Some(10.0),
                margin_bottom: Some(10.0),
            }),
            ..bitmap_cyx_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        margin_left: Some(20.0),
        margin_right: Some(20.0),
        margin_top: Some(20.0),
        margin_bottom: Some(20.0),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmap_layer_square_contain_data_units_layer_bounds_overrides_view_margins").await;
}

// Wide canvas (200x100)

#[tokio::test]
async fn test_bitmap_layer_wide_ignore_data_units_no_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(bitmap_cyx_data()),
        aspect_ratio_mode: AspectRatioMode::Ignore,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmap_layer_wide_ignore_data_units_no_margins").await;
}

#[tokio::test]
async fn test_bitmap_layer_wide_contain_data_units_no_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(bitmap_cyx_data()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmap_layer_wide_contain_data_units_no_margins").await;
}

#[tokio::test]
async fn test_bitmap_layer_wide_cover_data_units_no_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(bitmap_cyx_data()),
        aspect_ratio_mode: AspectRatioMode::Cover,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmap_layer_wide_cover_data_units_no_margins").await;
}

#[tokio::test]
async fn test_bitmap_layer_wide_contain_pixel_units_no_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(bitmap_cyx_pixels()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmap_layer_wide_contain_pixel_units_no_margins").await;
}

#[tokio::test]
async fn test_bitmap_layer_wide_contain_data_units_view_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(bitmap_cyx_data()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        margin_left: Some(10.0),
        margin_right: Some(10.0),
        margin_top: Some(10.0),
        margin_bottom: Some(10.0),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmap_layer_wide_contain_data_units_view_margins").await;
}

#[tokio::test]
async fn test_bitmap_layer_wide_contain_data_units_layer_bounds() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(BitmapLayerParams {
            bounds: Some(MarginParams {
                margin_left: Some(10.0),
                margin_right: Some(10.0),
                margin_top: Some(10.0),
                margin_bottom: Some(10.0),
            }),
            ..bitmap_cyx_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmap_layer_wide_contain_data_units_layer_bounds").await;
}

// Tall canvas (100x200)

#[tokio::test]
async fn test_bitmap_layer_tall_ignore_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(bitmap_cyx_data()),
        aspect_ratio_mode: AspectRatioMode::Ignore,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmap_layer_tall_ignore_data_units_no_margins").await;
}

#[tokio::test]
async fn test_bitmap_layer_tall_contain_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(bitmap_cyx_data()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmap_layer_tall_contain_data_units_no_margins").await;
}

#[tokio::test]
async fn test_bitmap_layer_tall_cover_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(bitmap_cyx_data()),
        aspect_ratio_mode: AspectRatioMode::Cover,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmap_layer_tall_cover_data_units_no_margins").await;
}

#[tokio::test]
async fn test_bitmap_layer_tall_contain_pixel_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(bitmap_cyx_pixels()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmap_layer_tall_contain_pixel_units_no_margins").await;
}

#[tokio::test]
async fn test_bitmap_layer_tall_contain_data_units_view_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(bitmap_cyx_data()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        margin_left: Some(10.0),
        margin_right: Some(10.0),
        margin_top: Some(10.0),
        margin_bottom: Some(10.0),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmap_layer_tall_contain_data_units_view_margins").await;
}

#[tokio::test]
async fn test_bitmap_layer_tall_contain_data_units_layer_bounds() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(BitmapLayerParams {
            bounds: Some(MarginParams {
                margin_left: Some(10.0),
                margin_right: Some(10.0),
                margin_top: Some(10.0),
                margin_bottom: Some(10.0),
            }),
            ..bitmap_cyx_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmap_layer_tall_contain_data_units_layer_bounds").await;
}

// ── BitmapLayer-specific tests ────────────────────────────────────────────────

// Test with reduced opacity
#[tokio::test]
async fn test_bitmap_layer_square_contain_data_units_half_opacity() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(BitmapLayerParams {
            opacity: 0.5,
            ..bitmap_cyx_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmap_layer_square_contain_data_units_half_opacity").await;
}

// Test with pixel_offset applied
#[tokio::test]
async fn test_bitmap_layer_square_contain_data_units_pixel_offset() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(BitmapLayerParams {
            pixel_offset: Some((1, 1)),
            ..bitmap_cyx_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmap_layer_square_contain_data_units_pixel_offset").await;
}

// Test with a different dimension order (XYC)
#[tokio::test]
async fn test_bitmap_layer_square_contain_data_units_xyc_order() {
    // Same 4x4 two-channel image, but stored in XYC order
    // XYC: [x=0..3, y=0..3, c=0..1]
    // For each (x, y) pair: [channel_0_value, channel_1_value]
    let data_xyc: Vec<u16> = {
        // Original CYX data: ch0[y][x] and ch1[y][x]
        let ch0: [[u16; 4]; 4] = [
            [  0, 110, 210, 310],
            [ 20, 120, 220, 320],
            [ 30, 130, 230, 330],
            [ 40, 140, 240, 340],
        ];
        let ch1: [[u16; 4]; 4] = [
            [300, 110, 210, 310],
            [ 20, 120, 220, 320],
            [ 30, 130, 230, 330],
            [ 40, 140, 240,   0],
        ];
        let mut v = Vec::with_capacity(4 * 4 * 2);
        for x in 0..4 {
            for y in 0..4 {
                v.push(ch0[y][x]);
                v.push(ch1[y][x]);
            }
        }
        v
    };
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(BitmapLayerParams {
            dimension_order: DimensionOrder::XYC,
            shape: vec![4, 4, 2],
            data: NumericData::Uint16(Arc::new(data_xyc)),
            ..bitmap_cyx_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmap_layer_square_contain_data_units_xyc_order").await;
}

// Test with a narrow channel window (higher contrast)
#[tokio::test]
async fn test_bitmap_layer_square_contain_data_units_narrow_window() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(BitmapLayerParams {
            channel_settings: vec![
                ChannelSettings { window: (100.0, 200.0), color: (1.0, 0.0, 0.0) },
                ChannelSettings { window: (100.0, 200.0), color: (0.0, 0.0, 1.0) },
            ],
            ..bitmap_cyx_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmap_layer_square_contain_data_units_narrow_window").await;
}

// ── Mixed unit modes (data_unit_mode_x ≠ data_unit_mode_y) ───────────────────

#[tokio::test]
async fn test_bitmap_layer_square_contain_data_x_pixel_y_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(bitmap_cyx_data_x_pixel_y()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmap_layer_square_contain_data_x_pixel_y_no_margins").await;
}

#[tokio::test]
async fn test_bitmap_layer_square_contain_pixel_x_data_y_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(bitmap_cyx_pixel_x_data_y()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmap_layer_square_contain_pixel_x_data_y_no_margins").await;
}
