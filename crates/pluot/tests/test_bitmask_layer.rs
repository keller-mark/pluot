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
        stroke_width_unit_mode: UnitsMode::Pixels,
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

// Helper: `bitmask_cyx_data()` with the outline-only channel widened, to
// visually distinguish the edge-detection thickness. The width is given in
// `unit_mode` units.
fn bitmask_cyx_data_wide_stroke(unit_mode: UnitsMode, stroke_width: f32) -> BitmaskLayerParams {
    BitmaskLayerParams {
        stroke_width_unit_mode: unit_mode,
        channel_settings: vec![
            bitmask_cyx_data().channel_settings[0].clone(),
            BitmaskChannelSettings {
                color: Some(ColorMode::UniformRgb((0, 0, 255))),
                filled: false,
                stroke_width,
                ..BitmaskChannelSettings::default()
            },
        ],
        ..bitmask_cyx_data()
    }
}

// The three stroke-width unit modes below are all set to resolve to the same
// 2-mask-texel outline on this 100x100 canvas, so they must render
// identically (i.e. share one snapshot family). With the identity
// model_matrix, one mask texel is one data unit and -- at the 1/8 camera zoom
// -- 12.5 screen pixels, i.e. 1/8 of the layer height.

// Data-unit stroke width: 2 data units == 2 texels.
#[tokio::test]
async fn test_bitmask_layer_square_contain_data_units_wide_stroke() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(bitmask_cyx_data_wide_stroke(UnitsMode::Data, 2.0)),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_square_contain_data_units_wide_stroke").await;
}

// Pixel stroke width: 25 screen px / 12.5 px per texel == 2 texels.
#[tokio::test]
async fn test_bitmask_layer_square_contain_data_units_wide_stroke_pixel_units() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(bitmask_cyx_data_wide_stroke(UnitsMode::Pixels, 25.0)),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_square_contain_data_units_wide_stroke").await;
}

// Normalized stroke width: 0.25 * 100px layer height == 25 px == 2 texels.
#[tokio::test]
async fn test_bitmask_layer_square_contain_data_units_wide_stroke_normalized_units() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(bitmask_cyx_data_wide_stroke(UnitsMode::Normalized, 0.25)),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_square_contain_data_units_wide_stroke").await;
}

// ── Stroke width unit modes ───────────────────────────────────────────────────
//
// `CHANNEL_MASK`'s objects are only 2 texels wide, so every object texel is a
// boundary even at a 1-texel stroke and outline *thickness* is invisible in
// its snapshots (the tests above only establish that the three unit modes
// agree). The tests below use a larger mask, where a 1-texel and a 2-texel
// outline are plainly different, to check thickness itself.

// Helper: an 8x8 single-object, single-channel mask -- a 6x6 block of object 1
// inset by one texel:
//   row 0:     [0, 0, 0, 0, 0, 0, 0, 0]
//   rows 1-6:  [0, 1, 1, 1, 1, 1, 1, 0]
//   row 7:     [0, 0, 0, 0, 0, 0, 0, 0]
// A 1-texel outline leaves a 4x4 hole; a 2-texel outline leaves a 2x2 hole.
const THICK_MASK: [u32; 64] = [
    0, 0, 0, 0, 0, 0, 0, 0,
    0, 1, 1, 1, 1, 1, 1, 0,
    0, 1, 1, 1, 1, 1, 1, 0,
    0, 1, 1, 1, 1, 1, 1, 0,
    0, 1, 1, 1, 1, 1, 1, 0,
    0, 1, 1, 1, 1, 1, 1, 0,
    0, 1, 1, 1, 1, 1, 1, 0,
    0, 0, 0, 0, 0, 0, 0, 0,
];

// Helper: `THICK_MASK` as a single outline-only blue channel, with the stroke
// width given in `unit_mode` units.
fn bitmask_thick(unit_mode: UnitsMode, stroke_width: f32) -> BitmaskLayerParams {
    BitmaskLayerParams {
        layer_id: "my_bitmask_layer".to_string(),
        data_unit_mode_x: UnitsMode::Data,
        data_unit_mode_y: UnitsMode::Data,
        stroke_width_unit_mode: unit_mode,
        dimension_order: DimensionOrder::CYX,
        shape: vec![1, 8, 8],
        channel_settings: vec![BitmaskChannelSettings {
            color: Some(ColorMode::UniformRgb((0, 0, 255))),
            filled: false,
            stroke_width,
            ..BitmaskChannelSettings::default()
        }],
        data: NumericData::Uint32(Arc::new(THICK_MASK.to_vec())),
        ..BitmaskLayerParams::default()
    }
}

// Zoom of 1/16, which on the 200x200 canvas the tests below use makes one
// texel of the 8x8 `THICK_MASK` exactly 12.5 screen px (so the whole mask
// spans 100px, half the canvas).
const CAMERA_ZOOM_OUT_16X: [f32; 16] = [
    0.0625, 0.0,    0.0, 0.0,
    0.0,    0.0625, 0.0, 0.0,
    0.0,    0.0,    0.0, 0.0,
    0.0,    0.0,    0.0, 1.0,
];

// Zoom of 1/32, i.e. 2x further out: one texel is 6.25 screen px.
const CAMERA_ZOOM_OUT_32X: [f32; 16] = [
    0.03125, 0.0,     0.0, 0.0,
    0.0,     0.03125, 0.0, 0.0,
    0.0,     0.0,     0.0, 0.0,
    0.0,     0.0,     0.0, 1.0,
];

fn thick_params(bitmask_params: BitmaskLayerParams, camera_view: [f32; 16]) -> RenderParams {
    RenderParams {
        width: 200,
        height: 200,
        layers: layer_params(bitmask_params),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(camera_view),
        ..Default::default()
    }
}

// One texel is one data unit (identity model_matrix), 12.5 screen px, and
// 12.5/200 = 0.0625 of the layer height, so these three all resolve to the
// same 1-texel outline and must render identically.
#[tokio::test]
async fn test_bitmask_layer_thick_mask_thin_stroke_data_units() {
    let params = thick_params(bitmask_thick(UnitsMode::Data, 1.0), CAMERA_ZOOM_OUT_16X);
    render_and_check_both_snapshots(params, "test_bitmask_layer_thick_mask_thin_stroke").await;
}

#[tokio::test]
async fn test_bitmask_layer_thick_mask_thin_stroke_pixel_units() {
    let params = thick_params(bitmask_thick(UnitsMode::Pixels, 12.5), CAMERA_ZOOM_OUT_16X);
    render_and_check_both_snapshots(params, "test_bitmask_layer_thick_mask_thin_stroke").await;
}

#[tokio::test]
async fn test_bitmask_layer_thick_mask_thin_stroke_normalized_units() {
    let params = thick_params(bitmask_thick(UnitsMode::Normalized, 0.0625), CAMERA_ZOOM_OUT_16X);
    render_and_check_both_snapshots(params, "test_bitmask_layer_thick_mask_thin_stroke").await;
}

// Doubling the width doubles the outline: a visibly thicker ring than the
// snapshot above, leaving a 2x2 rather than 4x4 hole.
#[tokio::test]
async fn test_bitmask_layer_thick_mask_wide_stroke_data_units() {
    let params = thick_params(bitmask_thick(UnitsMode::Data, 2.0), CAMERA_ZOOM_OUT_16X);
    render_and_check_both_snapshots(params, "test_bitmask_layer_thick_mask_wide_stroke").await;
}

#[tokio::test]
async fn test_bitmask_layer_thick_mask_wide_stroke_pixel_units() {
    let params = thick_params(bitmask_thick(UnitsMode::Pixels, 25.0), CAMERA_ZOOM_OUT_16X);
    render_and_check_both_snapshots(params, "test_bitmask_layer_thick_mask_wide_stroke").await;
}

// Zooming out 2x halves a texel's on-screen size to 6.25px. A data-unit width
// is camera-*dependent* in screen terms: it stays 1 texel, so the ring shrinks
// with the mask...
#[tokio::test]
async fn test_bitmask_layer_thick_mask_thin_stroke_data_units_zoomed_out() {
    let params = thick_params(bitmask_thick(UnitsMode::Data, 1.0), CAMERA_ZOOM_OUT_32X);
    render_and_check_both_snapshots(
        params,
        "test_bitmask_layer_thick_mask_thin_stroke_data_units_zoomed_out",
    ).await;
}

// ...whereas the same 12.5px width that was 1 texel at the previous zoom is
// now 2 texels, keeping the ring 12.5 screen px thick as the mask shrinks
// around it.
#[tokio::test]
async fn test_bitmask_layer_thick_mask_thin_stroke_pixel_units_zoomed_out() {
    let params = thick_params(bitmask_thick(UnitsMode::Pixels, 12.5), CAMERA_ZOOM_OUT_32X);
    render_and_check_both_snapshots(
        params,
        "test_bitmask_layer_thick_mask_thin_stroke_pixel_units_zoomed_out",
    ).await;
}

// Pixel positioning: the camera does not apply, and `model_matrix` alone sizes
// a texel. A 12.5x scale makes one texel 12.5 screen px, so a 25px width is a
// 2-texel outline -- the same ring as `..._thick_mask_wide_stroke`, just
// anchored at the origin rather than placed by the camera.
#[tokio::test]
async fn test_bitmask_layer_thick_mask_wide_stroke_pixel_units_pixel_positioning() {
    let params = thick_params(
        BitmaskLayerParams {
            data_unit_mode_x: UnitsMode::Pixels,
            data_unit_mode_y: UnitsMode::Pixels,
            model_matrix: Some([
                12.5, 0.0,  0.0, 0.0,
                0.0,  12.5, 0.0, 0.0,
                0.0,  0.0,  1.0, 0.0,
                0.0,  0.0,  0.0, 1.0,
            ]),
            ..bitmask_thick(UnitsMode::Pixels, 25.0)
        },
        CAMERA_ZOOM_OUT_16X,
    );
    render_and_check_both_snapshots(
        params,
        "test_bitmask_layer_thick_mask_wide_stroke_pixel_positioning",
    ).await;
}

// Normalized positioning: `model_matrix` maps texels into a (0 to 1) fraction
// of the layer, so a 0.0625 scale again makes one texel 12.5 of the 200 screen
// px, and a 25px width a 2-texel outline. Renders identically to the pixel-
// positioned case above.
#[tokio::test]
async fn test_bitmask_layer_thick_mask_wide_stroke_pixel_units_normalized_positioning() {
    let params = thick_params(
        BitmaskLayerParams {
            data_unit_mode_x: UnitsMode::Normalized,
            data_unit_mode_y: UnitsMode::Normalized,
            model_matrix: Some([
                0.0625, 0.0,    0.0, 0.0,
                0.0,    0.0625, 0.0, 0.0,
                0.0,    0.0,    1.0, 0.0,
                0.0,    0.0,    0.0, 1.0,
            ]),
            ..bitmask_thick(UnitsMode::Pixels, 25.0)
        },
        CAMERA_ZOOM_OUT_16X,
    );
    render_and_check_both_snapshots(
        params,
        "test_bitmask_layer_thick_mask_wide_stroke_pixel_positioning",
    ).await;
}

// A data-unit stroke width is meaningless when the mask is positioned
// relative to the layer bounds rather than in data space.
#[tokio::test]
#[should_panic(expected = "stroke_width_unit_mode cannot be 'data'")]
async fn test_bitmask_layer_data_stroke_units_with_pixel_positioning_panics() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(BitmaskLayerParams {
            data_unit_mode_x: UnitsMode::Pixels,
            data_unit_mode_y: UnitsMode::Pixels,
            ..bitmask_cyx_data_wide_stroke(UnitsMode::Data, 2.0)
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "unused_panics_before_rendering").await;
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
