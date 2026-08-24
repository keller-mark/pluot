#![cfg(not(target_arch = "wasm32"))]

use std::sync::Arc;

mod test_utils;
use test_utils::render_and_check_both_snapshots;

use pluot::{
    RenderParams, LayerParams,
    AspectRatioMode, UnitsMode, MarginParams,
    BitmaskChannelSettings, BitmaskLayerParams, CategoricalColormap, CategoricalCustomParams,
    CategoricalParams, ColorMode, DimensionOrder, InstancedRgbInterleavedParams, InstancedRgbParams,
    NumericData, QuantitativeColormap, QuantitativeParams,
};

// For bitmask layer tests, we always want to test the following cases (and combinations of them):
// - Square and non-square (wide and tall) aspect ratios
// - Each aspect ratio mode (ignore, contain, cover)
// - Both data and pixel data_unit_modes
// - With and without margins at the view level
// - With and without margins (bounds) at the layer level
// - Raster and vector (which the helper function already handles for us)
// - Layer-specific stuff
//   - For BitmaskLayer, this includes testing every `ColorMode` variant,
//     filled vs. outline-only channels, multi-channel blending, colormap
//     deduplication, dimension order, opacity, and pixel_offset.

// Helper: a 4x4, 2-object mask, shared by every channel below:
//   row 0: [1, 1, 2, 2]
//   row 1: [1, 1, 2, 2]
//   row 2: [0, 0, 2, 2]
//   row 3: [0, 0, 0, 0]
const CHANNEL_MASK: [u32; 16] = [
    1, 1, 2, 2,
    1, 1, 2, 2,
    0, 0, 2, 2,
    0, 0, 0, 0,
];

fn repeated_mask_data(num_channels: usize) -> NumericData {
    let mut v = Vec::with_capacity(CHANNEL_MASK.len() * num_channels);
    for _ in 0..num_channels {
        v.extend_from_slice(&CHANNEL_MASK);
    }
    NumericData::Uint32(Arc::new(v))
}

// Helper: a 4x4 two-channel mask in CYX order (matches the bitmap layer test's shape).
// Channel 0: filled, object 1 red / object 2 green (ColorMode::CategoricalCustom).
// Channel 1: outline-only, blue (ColorMode::UniformRgb).
fn bitmask_cyx_data() -> BitmaskLayerParams {
    BitmaskLayerParams {
        layer_id: "my_bitmask_layer".to_string(),
        bounds: None,
        data_unit_mode_x: UnitsMode::Data,
        data_unit_mode_y: UnitsMode::Data,
        pixel_offset: None,
        model_matrix: None,
        dimension_order: DimensionOrder::CYX,
        shape: vec![2, 4, 4],
        channel_settings: vec![
            BitmaskChannelSettings {
                color: Some(ColorMode::CategoricalCustom(CategoricalCustomParams {
                    values: NumericData::Uint8(Arc::new(vec![0, 1])),
                    colormap: vec![(255, 0, 0), (0, 255, 0)],
                })),
                ..BitmaskChannelSettings::default()
            },
            BitmaskChannelSettings {
                color: Some(ColorMode::UniformRgb((0, 0, 255))),
                filled: false,
                stroke_width: 1.0,
                ..BitmaskChannelSettings::default()
            },
        ],
        opacity: 1.0,
        data: repeated_mask_data(2),
    }
}

// Helper: same mask in Pixels unit mode (4x4 pixel mask positioned in pixel space)
fn bitmask_cyx_pixels() -> BitmaskLayerParams {
    BitmaskLayerParams {
        data_unit_mode_x: UnitsMode::Pixels,
        data_unit_mode_y: UnitsMode::Pixels,
        ..bitmask_cyx_data()
    }
}

fn bitmask_cyx_data_x_pixel_y() -> BitmaskLayerParams {
    BitmaskLayerParams {
        data_unit_mode_x: UnitsMode::Data,
        data_unit_mode_y: UnitsMode::Pixels,
        ..bitmask_cyx_data()
    }
}

fn bitmask_cyx_pixel_x_data_y() -> BitmaskLayerParams {
    BitmaskLayerParams {
        data_unit_mode_x: UnitsMode::Pixels,
        data_unit_mode_y: UnitsMode::Data,
        ..bitmask_cyx_data()
    }
}

// Helper: same mask in Normalized unit mode. As with `BitmapLayer`, position/size come
// from `pixel_offset` and the mask's `shape` (always in native pixel units), and
// bitmask_layer.wgsl does NOT divide these by the layer size in Normalized mode (it
// only skips that division, unlike Pixels mode) -- so a raw img_size of 4x4 would be
// interpreted as 4x the layer's normalized (0,1) extent, way off-canvas. A
// model_matrix scale is the mechanism to bring it into (0,1) space. Scaling by 0.01
// shrinks the 4x4 mask to a 0.04x0.04 normalized extent, which matches
// bitmask_cyx_pixels()'s 4px / 100px layer size exactly on a 100x100 canvas, so this
// renders identically to bitmask_cyx_pixels() there.
fn bitmask_cyx_normalized() -> BitmaskLayerParams {
    BitmaskLayerParams {
        data_unit_mode_x: UnitsMode::Normalized,
        data_unit_mode_y: UnitsMode::Normalized,
        model_matrix: Some([
            0.01, 0.0, 0.0, 0.0,
            0.0, 0.01, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ]),
        ..bitmask_cyx_data()
    }
}

// Helper: x in data space (unscaled, matching bitmask_cyx_data_x_pixel_y()'s
// treatment of the data axis), y in normalized space (scaled via model_matrix).
fn bitmask_cyx_data_x_normalized_y() -> BitmaskLayerParams {
    BitmaskLayerParams {
        data_unit_mode_x: UnitsMode::Data,
        data_unit_mode_y: UnitsMode::Normalized,
        model_matrix: Some([
            1.0, 0.0, 0.0, 0.0,
            0.0, 0.01, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ]),
        ..bitmask_cyx_data()
    }
}

// Helper: x in normalized space (scaled via model_matrix), y in data space (unscaled,
// matching bitmask_cyx_pixel_x_data_y()'s treatment of the data axis).
fn bitmask_cyx_normalized_x_data_y() -> BitmaskLayerParams {
    BitmaskLayerParams {
        data_unit_mode_x: UnitsMode::Normalized,
        data_unit_mode_y: UnitsMode::Data,
        model_matrix: Some([
            0.01, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ]),
        ..bitmask_cyx_data()
    }
}

fn layer_params(bitmask_params: BitmaskLayerParams) -> Vec<LayerParams> {
    vec![LayerParams::BitmaskLayer(bitmask_params)]
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
async fn test_bitmask_layer_square_contain_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(BitmaskLayerParams {
            bounds: Some(MarginParams {
                margin_left: Some(0.0),
                margin_right: Some(0.0),
                margin_top: Some(0.0),
                margin_bottom: Some(0.0),
            }),
            ..bitmask_cyx_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_square_contain_data_units_no_margins").await;
}

#[tokio::test]
async fn test_bitmask_layer_square_ignore_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(bitmask_cyx_data()),
        aspect_ratio_mode: AspectRatioMode::Ignore,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_square_ignore_data_units_no_margins").await;
}

#[tokio::test]
async fn test_bitmask_layer_square_cover_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(bitmask_cyx_data()),
        aspect_ratio_mode: AspectRatioMode::Cover,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_square_cover_data_units_no_margins").await;
}

#[tokio::test]
async fn test_bitmask_layer_square_contain_pixel_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(bitmask_cyx_pixels()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_square_contain_pixel_units_no_margins").await;
}

// Normalized units: on a 100x100 canvas this renders identically to the Pixels
// test above, since bitmask_cyx_normalized()'s model_matrix scale (0.01) applied
// to the 4x4 img_size yields the same 0.04 normalized extent as 4px / 100px.
#[tokio::test]
async fn test_bitmask_layer_square_contain_normalized_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(bitmask_cyx_normalized()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_square_contain_normalized_units_no_margins").await;
}

#[tokio::test]
async fn test_bitmask_layer_square_contain_data_units_view_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(bitmask_cyx_data()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        margin_left: Some(10.0),
        margin_right: Some(10.0),
        margin_top: Some(10.0),
        margin_bottom: Some(10.0),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_square_contain_data_units_view_margins").await;
}

#[tokio::test]
async fn test_bitmask_layer_square_contain_data_units_layer_bounds() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(BitmaskLayerParams {
            bounds: Some(MarginParams {
                margin_left: Some(10.0),
                margin_right: Some(10.0),
                margin_top: Some(10.0),
                margin_bottom: Some(10.0),
            }),
            ..bitmask_cyx_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_square_contain_data_units_layer_bounds").await;
}

// Layer bounds take precedence over view margins when both are set
#[tokio::test]
async fn test_bitmask_layer_square_contain_data_units_layer_bounds_overrides_view_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(BitmaskLayerParams {
            bounds: Some(MarginParams {
                margin_left: Some(10.0),
                margin_right: Some(10.0),
                margin_top: Some(10.0),
                margin_bottom: Some(10.0),
            }),
            ..bitmask_cyx_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        margin_left: Some(20.0),
        margin_right: Some(20.0),
        margin_top: Some(20.0),
        margin_bottom: Some(20.0),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_square_contain_data_units_layer_bounds_overrides_view_margins").await;
}

// Wide canvas (200x100)

#[tokio::test]
async fn test_bitmask_layer_wide_ignore_data_units_no_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(bitmask_cyx_data()),
        aspect_ratio_mode: AspectRatioMode::Ignore,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_wide_ignore_data_units_no_margins").await;
}

#[tokio::test]
async fn test_bitmask_layer_wide_contain_data_units_no_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(bitmask_cyx_data()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_wide_contain_data_units_no_margins").await;
}

#[tokio::test]
async fn test_bitmask_layer_wide_cover_data_units_no_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(bitmask_cyx_data()),
        aspect_ratio_mode: AspectRatioMode::Cover,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_wide_cover_data_units_no_margins").await;
}

#[tokio::test]
async fn test_bitmask_layer_wide_contain_pixel_units_no_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(bitmask_cyx_pixels()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_wide_contain_pixel_units_no_margins").await;
}

#[tokio::test]
async fn test_bitmask_layer_wide_contain_normalized_units_no_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(bitmask_cyx_normalized()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_wide_contain_normalized_units_no_margins").await;
}

#[tokio::test]
async fn test_bitmask_layer_wide_contain_data_units_view_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(bitmask_cyx_data()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        margin_left: Some(10.0),
        margin_right: Some(10.0),
        margin_top: Some(10.0),
        margin_bottom: Some(10.0),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_wide_contain_data_units_view_margins").await;
}

#[tokio::test]
async fn test_bitmask_layer_wide_contain_data_units_layer_bounds() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: layer_params(BitmaskLayerParams {
            bounds: Some(MarginParams {
                margin_left: Some(10.0),
                margin_right: Some(10.0),
                margin_top: Some(10.0),
                margin_bottom: Some(10.0),
            }),
            ..bitmask_cyx_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_wide_contain_data_units_layer_bounds").await;
}

// Tall canvas (100x200)

#[tokio::test]
async fn test_bitmask_layer_tall_ignore_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(bitmask_cyx_data()),
        aspect_ratio_mode: AspectRatioMode::Ignore,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_tall_ignore_data_units_no_margins").await;
}

#[tokio::test]
async fn test_bitmask_layer_tall_contain_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(bitmask_cyx_data()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_tall_contain_data_units_no_margins").await;
}

#[tokio::test]
async fn test_bitmask_layer_tall_cover_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(bitmask_cyx_data()),
        aspect_ratio_mode: AspectRatioMode::Cover,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_tall_cover_data_units_no_margins").await;
}

#[tokio::test]
async fn test_bitmask_layer_tall_contain_pixel_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(bitmask_cyx_pixels()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_tall_contain_pixel_units_no_margins").await;
}

#[tokio::test]
async fn test_bitmask_layer_tall_contain_normalized_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(bitmask_cyx_normalized()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_tall_contain_normalized_units_no_margins").await;
}

#[tokio::test]
async fn test_bitmask_layer_tall_contain_data_units_view_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(bitmask_cyx_data()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        margin_left: Some(10.0),
        margin_right: Some(10.0),
        margin_top: Some(10.0),
        margin_bottom: Some(10.0),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_tall_contain_data_units_view_margins").await;
}

#[tokio::test]
async fn test_bitmask_layer_tall_contain_data_units_layer_bounds() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: layer_params(BitmaskLayerParams {
            bounds: Some(MarginParams {
                margin_left: Some(10.0),
                margin_right: Some(10.0),
                margin_top: Some(10.0),
                margin_bottom: Some(10.0),
            }),
            ..bitmask_cyx_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_tall_contain_data_units_layer_bounds").await;
}

// ── Mixed unit modes (data_unit_mode_x ≠ data_unit_mode_y) ───────────────────

#[tokio::test]
async fn test_bitmask_layer_square_contain_data_x_pixel_y_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(bitmask_cyx_data_x_pixel_y()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_square_contain_data_x_pixel_y_no_margins").await;
}

#[tokio::test]
async fn test_bitmask_layer_square_contain_pixel_x_data_y_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(bitmask_cyx_pixel_x_data_y()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_square_contain_pixel_x_data_y_no_margins").await;
}

#[tokio::test]
async fn test_bitmask_layer_square_contain_data_x_normalized_y_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(bitmask_cyx_data_x_normalized_y()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_square_contain_data_x_normalized_y_no_margins").await;
}

#[tokio::test]
async fn test_bitmask_layer_square_contain_normalized_x_data_y_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(bitmask_cyx_normalized_x_data_y()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_square_contain_normalized_x_data_y_no_margins").await;
}

// ── BitmaskLayer-specific tests ────────────────────────────────────────────────

// Test with reduced layer opacity
#[tokio::test]
async fn test_bitmask_layer_square_contain_data_units_half_opacity() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(BitmaskLayerParams {
            opacity: 0.5,
            ..bitmask_cyx_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_square_contain_data_units_half_opacity").await;
}

// Test with pixel_offset applied
#[tokio::test]
async fn test_bitmask_layer_square_contain_data_units_pixel_offset() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(BitmaskLayerParams {
            pixel_offset: Some((1, 1)),
            ..bitmask_cyx_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_square_contain_data_units_pixel_offset").await;
}

// Test with a different dimension order (YXC): channels interleaved per pixel,
// rather than contiguous per-channel blocks, verifying the stride-based
// indexing (shared with `BitmapLayer`) is wired correctly.
#[tokio::test]
async fn test_bitmask_layer_square_contain_data_units_yxc_order() {
    let ch0 = CHANNEL_MASK;
    let ch1 = CHANNEL_MASK;
    let mut data_yxc = Vec::with_capacity(32);
    for i in 0..16 {
        data_yxc.push(ch0[i]);
        data_yxc.push(ch1[i]);
    }
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(BitmaskLayerParams {
            dimension_order: DimensionOrder::YXC,
            shape: vec![4, 4, 2],
            data: NumericData::Uint32(Arc::new(data_yxc)),
            ..bitmask_cyx_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_square_contain_data_units_yxc_order").await;
}

// Test with a wider outline stroke width, to visually distinguish the
// outline-only channel's edge-detection thickness.
#[tokio::test]
async fn test_bitmask_layer_square_contain_data_units_wide_stroke() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(BitmaskLayerParams {
            channel_settings: vec![
                bitmask_cyx_data().channel_settings[0].clone(),
                BitmaskChannelSettings {
                    color: Some(ColorMode::UniformRgb((0, 0, 255))),
                    filled: false,
                    stroke_width: 2.0,
                    ..BitmaskChannelSettings::default()
                },
            ],
            ..bitmask_cyx_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_square_contain_data_units_wide_stroke").await;
}

// Exercises `ColorMode::UniformRgb`, filled, on its own.
#[tokio::test]
async fn test_bitmask_layer_square_contain_data_units_uniform_rgb() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(BitmaskLayerParams {
            channel_settings: vec![BitmaskChannelSettings {
                color: Some(ColorMode::UniformRgb((255, 128, 0))),
                ..BitmaskChannelSettings::default()
            }],
            shape: vec![1, 4, 4],
            data: repeated_mask_data(1),
            ..bitmask_cyx_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_square_contain_data_units_uniform_rgb").await;
}

// Exercises `ColorMode::InstancedRgb`, filled, on its own.
#[tokio::test]
async fn test_bitmask_layer_square_contain_data_units_instanced_rgb() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(BitmaskLayerParams {
            channel_settings: vec![BitmaskChannelSettings {
                color: Some(ColorMode::InstancedRgb(InstancedRgbParams {
                    r_values: NumericData::Uint8(Arc::new(vec![0, 255])),
                    g_values: NumericData::Uint8(Arc::new(vec![255, 0])),
                    b_values: NumericData::Uint8(Arc::new(vec![0, 255])),
                })),
                ..BitmaskChannelSettings::default()
            }],
            shape: vec![1, 4, 4],
            data: repeated_mask_data(1),
            ..bitmask_cyx_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_square_contain_data_units_instanced_rgb").await;
}

// Exercises `ColorMode::InstancedRgbInterleaved`, filled, on its own.
#[tokio::test]
async fn test_bitmask_layer_square_contain_data_units_instanced_rgb_interleaved() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(BitmaskLayerParams {
            channel_settings: vec![BitmaskChannelSettings {
                color: Some(ColorMode::InstancedRgbInterleaved(InstancedRgbInterleavedParams {
                    rgb_values: NumericData::Uint8(Arc::new(vec![0, 255, 0, 255, 0, 255])),
                })),
                ..BitmaskChannelSettings::default()
            }],
            shape: vec![1, 4, 4],
            data: repeated_mask_data(1),
            ..bitmask_cyx_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_square_contain_data_units_instanced_rgb_interleaved").await;
}

// Exercises `ColorMode::Categorical` (a named palette, i.e. "set colors" via
// a shared colormap), filled, on its own.
#[tokio::test]
async fn test_bitmask_layer_square_contain_data_units_categorical() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(BitmaskLayerParams {
            channel_settings: vec![BitmaskChannelSettings {
                color: Some(ColorMode::Categorical(CategoricalParams {
                    codes: NumericData::Uint8(Arc::new(vec![0, 1])),
                    colormap: CategoricalColormap::Tableau10,
                })),
                ..BitmaskChannelSettings::default()
            }],
            shape: vec![1, 4, 4],
            data: repeated_mask_data(1),
            ..bitmask_cyx_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_square_contain_data_units_categorical").await;
}

// Exercises `ColorMode::Quantitative` (a continuous colormap applied to a
// per-object feature value), filled, on its own.
#[tokio::test]
async fn test_bitmask_layer_square_contain_data_units_quantitative() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(BitmaskLayerParams {
            channel_settings: vec![BitmaskChannelSettings {
                color: Some(ColorMode::Quantitative(QuantitativeParams {
                    values: NumericData::Float32(Arc::new(vec![0.1, 0.9])),
                    colormap: QuantitativeColormap::Viridis,
                    reverse: false,
                    domain: None,
                })),
                ..BitmaskChannelSettings::default()
            }],
            shape: vec![1, 4, 4],
            data: repeated_mask_data(1),
            ..bitmask_cyx_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_square_contain_data_units_quantitative").await;
}

// All `ColorMode` variants combined into one multi-channel draw call, to
// exercise shader-assembly bugs (duplicate/undeclared WGSL bindings, name
// collisions between channels, malformed templates) that only surface when
// wgpu actually compiles and runs the generated shader module.
//
// Channel counts here are kept modest deliberately: every channel binds 0-3
// color-mode textures (on top of the one shared mask-data texture), and
// WebGPU's default `max_sampled_textures_per_shader_stage` limit is commonly
// 16 -- a real constraint on how many color-textured channels a single
// `BitmaskLayer` draw call can use at once (`None`/`UniformRgb` share the
// exact same generated WGSL, so only one is exercised here).
#[tokio::test]
async fn test_bitmask_layer_square_contain_data_units_all_color_modes() {
    let channel_settings = vec![
        // UniformRgb: outline-only.
        BitmaskChannelSettings {
            color: Some(ColorMode::UniformRgb((255, 0, 0))),
            filled: false,
            stroke_width: 1.0,
            opacity: 0.8,
            ..BitmaskChannelSettings::default()
        },
        // InstancedRgb: filled, per-object explicit RGB.
        BitmaskChannelSettings {
            color: Some(ColorMode::InstancedRgb(InstancedRgbParams {
                r_values: NumericData::Uint8(Arc::new(vec![0, 255])),
                g_values: NumericData::Uint8(Arc::new(vec![255, 0])),
                b_values: NumericData::Uint8(Arc::new(vec![0, 0])),
            })),
            opacity: 0.5,
            ..BitmaskChannelSettings::default()
        },
        // InstancedRgbInterleaved: filled.
        BitmaskChannelSettings {
            color: Some(ColorMode::InstancedRgbInterleaved(InstancedRgbInterleavedParams {
                rgb_values: NumericData::Uint8(Arc::new(vec![0, 255, 0, 255, 0, 0])),
            })),
            opacity: 0.5,
            ..BitmaskChannelSettings::default()
        },
        // Categorical ("set colors" via a named palette): filled.
        BitmaskChannelSettings {
            color: Some(ColorMode::Categorical(CategoricalParams {
                codes: NumericData::Uint8(Arc::new(vec![0, 1])),
                colormap: CategoricalColormap::Tableau10,
            })),
            opacity: 0.5,
            ..BitmaskChannelSettings::default()
        },
        // CategoricalCustom ("set colors" via explicit RGB list): outline-only.
        BitmaskChannelSettings {
            color: Some(ColorMode::CategoricalCustom(CategoricalCustomParams {
                values: NumericData::Uint8(Arc::new(vec![0, 1])),
                colormap: vec![(10, 20, 30), (200, 100, 50)],
            })),
            filled: false,
            stroke_width: 1.0,
            opacity: 0.5,
            ..BitmaskChannelSettings::default()
        },
        // Quantitative: filled, exercises the injected colormap function.
        BitmaskChannelSettings {
            color: Some(ColorMode::Quantitative(QuantitativeParams {
                values: NumericData::Float32(Arc::new(vec![0.1, 0.9])),
                colormap: QuantitativeColormap::Viridis,
                reverse: false,
                domain: None,
            })),
            opacity: 0.5,
            ..BitmaskChannelSettings::default()
        },
    ];
    let num_channels = channel_settings.len();

    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(BitmaskLayerParams {
            shape: vec![num_channels as u32, 4, 4],
            data: repeated_mask_data(num_channels),
            channel_settings,
            opacity: 0.9,
            ..bitmask_cyx_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_square_contain_data_units_all_color_modes").await;
}

// Two channels sharing the same named colormap: the generated shader must
// only define that colormap's WGSL function once (see `colormap_fns` in
// `BitmaskLayer::draw`), or shader compilation fails with a duplicate
// function definition.
#[tokio::test]
async fn test_bitmask_layer_square_contain_data_units_quantitative_colormap_dedup() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(BitmaskLayerParams {
            shape: vec![2, 4, 4],
            data: repeated_mask_data(2),
            channel_settings: vec![
                BitmaskChannelSettings {
                    color: Some(ColorMode::Quantitative(QuantitativeParams {
                        values: NumericData::Float32(Arc::new(vec![0.1, 0.9])),
                        colormap: QuantitativeColormap::Viridis,
                        reverse: false,
                        domain: None,
                    })),
                    ..BitmaskChannelSettings::default()
                },
                BitmaskChannelSettings {
                    color: Some(ColorMode::Quantitative(QuantitativeParams {
                        values: NumericData::Float32(Arc::new(vec![0.9, 0.1])),
                        colormap: QuantitativeColormap::Viridis,
                        reverse: true,
                        domain: Some((0.0, 1.0)),
                    })),
                    opacity: 0.5,
                    ..BitmaskChannelSettings::default()
                },
            ],
            ..bitmask_cyx_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_square_contain_data_units_quantitative_colormap_dedup").await;
}

// A single all-background channel: should render fully transparent.
#[tokio::test]
async fn test_bitmask_layer_square_contain_data_units_empty_mask() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(BitmaskLayerParams {
            shape: vec![1, 4, 4],
            data: NumericData::Uint32(Arc::new(vec![0u32; 16])),
            channel_settings: vec![BitmaskChannelSettings::default()],
            ..bitmask_cyx_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_square_contain_data_units_empty_mask").await;
}
