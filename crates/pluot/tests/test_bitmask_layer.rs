#![cfg(not(target_arch = "wasm32"))]

use std::sync::Arc;

mod test_utils;
use test_utils::render_and_check_both_snapshots;

use pluot::{
    RenderParams, LayerParams,
    AspectRatioMode, UnitsMode, MarginParams,
    BitmaskChannelSettings, BitmaskLayerParams, CategoricalColormap, CategoricalCustomParams,
    CategoricalParams, ColorMode, DimensionOrder, InstancedRgbInterleavedParams, InstancedRgbParams,
    NumericData, OpacityMode, QuantitativeColormap, QuantitativeParams, SizeMode, InstancedSizeParams,
    InstancedOpacityParams,
};

// Helpers: the two single-purpose channel shapes most of these fixtures use --
// filled only (no outline) and stroked only (no fill) -- each colored by one
// `ColorMode`. Channels exercising both at once are written out inline.
fn filled_channel(fill_color: ColorMode) -> BitmaskChannelSettings {
    BitmaskChannelSettings {
        stroked: false,
        filled: true,
        fill_color: Some(fill_color),
        ..BitmaskChannelSettings::default()
    }
}

fn stroked_channel(stroke_color: ColorMode, stroke_width: f32) -> BitmaskChannelSettings {
    BitmaskChannelSettings {
        stroked: true,
        filled: false,
        stroke_color: Some(stroke_color),
        stroke_width: Some(SizeMode::UniformSize(stroke_width)),
        ..BitmaskChannelSettings::default()
    }
}

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

// Helper: the 16x16, 4-object mask shared by every channel below, matching
// the `MASK` array in bindings-js/docs/src/content/docs/examples/bitmask.mdx
// (a diamond (1), a triangle (2), a plus (3), and a ring (4); 0 is
// background). Object 4's interior hole exercises the outline of a
// non-simply-connected object.
#[rustfmt::skip]
const MASK: [u32; 256] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 2, 2, 0, 0, 0,
    0, 0, 1, 1, 1, 1, 0, 0, 0, 0, 2, 2, 2, 2, 0, 0,
    0, 1, 1, 1, 1, 1, 1, 0, 0, 0, 2, 2, 2, 2, 0, 0,
    0, 1, 1, 1, 1, 1, 1, 0, 0, 2, 2, 2, 2, 2, 2, 0,
    0, 0, 1, 1, 1, 1, 0, 0, 0, 2, 2, 2, 2, 2, 2, 0,
    0, 0, 0, 1, 1, 0, 0, 0, 0, 2, 2, 2, 2, 2, 2, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 3, 3, 0, 0, 0, 0, 4, 4, 4, 4, 4, 4, 0,
    0, 0, 0, 3, 3, 0, 0, 0, 0, 4, 4, 4, 4, 4, 4, 0,
    0, 3, 3, 3, 3, 3, 3, 0, 0, 4, 4, 0, 0, 4, 4, 0,
    0, 3, 3, 3, 3, 3, 3, 0, 0, 4, 4, 0, 0, 4, 4, 0,
    0, 0, 0, 3, 3, 0, 0, 0, 0, 4, 4, 4, 4, 4, 4, 0,
    0, 0, 0, 3, 3, 0, 0, 0, 0, 4, 4, 4, 4, 4, 4, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

// Helper: `MASK`'s first 8 rows (8x16) -- the diamond (1) and triangle (2),
// with the plus and ring cropped out. Used where a non-square, non-4x4 mask
// is wanted (e.g. the "thick mask" stroke-width family below).
#[rustfmt::skip]
const MASK_TOP_8_ROWS: [u32; 128] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 2, 2, 0, 0, 0,
    0, 0, 1, 1, 1, 1, 0, 0, 0, 0, 2, 2, 2, 2, 0, 0,
    0, 1, 1, 1, 1, 1, 1, 0, 0, 0, 2, 2, 2, 2, 0, 0,
    0, 1, 1, 1, 1, 1, 1, 0, 0, 2, 2, 2, 2, 2, 2, 0,
    0, 0, 1, 1, 1, 1, 0, 0, 0, 2, 2, 2, 2, 2, 2, 0,
    0, 0, 0, 1, 1, 0, 0, 0, 0, 2, 2, 2, 2, 2, 2, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

// Helper: `MASK`'s first 8 columns (16x8) -- the diamond (1) and plus (3),
// with the triangle and ring cropped out. A second "other array shape"
// variant, tall rather than wide.
#[rustfmt::skip]
const MASK_LEFT_8_COLS: [u32; 128] = [
    0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 1, 1, 0, 0, 0,
    0, 0, 1, 1, 1, 1, 0, 0,
    0, 1, 1, 1, 1, 1, 1, 0,
    0, 1, 1, 1, 1, 1, 1, 0,
    0, 0, 1, 1, 1, 1, 0, 0,
    0, 0, 0, 1, 1, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 3, 3, 0, 0, 0,
    0, 0, 0, 3, 3, 0, 0, 0,
    0, 3, 3, 3, 3, 3, 3, 0,
    0, 3, 3, 3, 3, 3, 3, 0,
    0, 0, 0, 3, 3, 0, 0, 0,
    0, 0, 0, 3, 3, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0,
];

// Helper: colors each object by its id via a custom categorical palette,
// mirroring `OBJECT_COLORS` in bitmask.mdx -- object k looks up
// `values[k - 1]` (ids are 1-based, since 0 means background), which indexes
// into `colormap`.
fn object_colors() -> ColorMode {
    ColorMode::CategoricalCustom(CategoricalCustomParams {
        values: NumericData::Uint8(Arc::new(vec![0, 1, 2, 3])),
        colormap: vec![(31, 119, 180), (255, 127, 14), (44, 160, 44), (214, 39, 40)],
    })
}

fn repeated_mask_data(num_channels: usize) -> NumericData {
    let mut v = Vec::with_capacity(MASK.len() * num_channels);
    for _ in 0..num_channels {
        v.extend_from_slice(&MASK);
    }
    NumericData::Uint32(Arc::new(v))
}

// Helper: builds a mask the same size as `MASK`, keeping only the given
// object ids and zeroing everything else. Used to give different channels
// genuinely different content (rather than identical repeated data), so
// multi-channel blending is exercised on more than just per-channel color.
fn mask_filtered(allowed_ids: &[u32]) -> Vec<u32> {
    MASK.iter().map(|&v| if allowed_ids.contains(&v) { v } else { 0 }).collect()
}

// Helper: a 16x16 two-channel mask in CYX order (matches the bitmap layer test's shape).
// Channel 0: filled, each object colored via `object_colors()` (ColorMode::CategoricalCustom).
// Channel 1: outline-only, blue (ColorMode::UniformRgb).
//
// The outline is one mask texel thick, which these fixtures pin down by giving
// the stroke width in whichever unit mode makes "one texel" exact for how they
// position the mask -- `Data` here (the identity model_matrix makes one texel
// one data unit) and `Pixels` in the variants below whose Y axis is not in data
// space. Doing so keeps thickness independent of the canvas size and camera,
// which vary across the tests that use these.
fn bitmask_cyx_data() -> BitmaskLayerParams {
    BitmaskLayerParams {
        layer_id: "my_bitmask_layer".to_string(),
        bounds: None,
        data_unit_mode_x: UnitsMode::Data,
        data_unit_mode_y: UnitsMode::Data,
        stroke_width_unit_mode: UnitsMode::Data,
        pixel_offset: None,
        model_matrix: None,
        dimension_order: DimensionOrder::CYX,
        shape: vec![2, 16, 16],
        channel_settings: vec![
            filled_channel(object_colors()),
            stroked_channel(ColorMode::UniformRgb((0, 0, 255)), 1.0),
        ],
        opacity: 1.0,
        data: repeated_mask_data(2),
    }
}

// Helper: override the outline-only channel's (channel 1) stroke width, for the
// fixtures below whose unit mode makes "one texel" a value other than 1.0.
fn with_stroke_width(mut params: BitmaskLayerParams, stroke_width: f32) -> BitmaskLayerParams {
    params.channel_settings[1].stroke_width = Some(SizeMode::UniformSize(stroke_width));
    params
}

// Helper: same mask in Pixels unit mode (16x16 pixel mask positioned in pixel space).
// With the identity model_matrix one texel is one screen pixel, so a 1px stroke
// is the same one-texel outline as the data-positioned fixture above.
fn bitmask_cyx_pixels() -> BitmaskLayerParams {
    BitmaskLayerParams {
        data_unit_mode_x: UnitsMode::Pixels,
        data_unit_mode_y: UnitsMode::Pixels,
        stroke_width_unit_mode: UnitsMode::Pixels,
        ..bitmask_cyx_data()
    }
}

fn bitmask_cyx_data_x_pixel_y() -> BitmaskLayerParams {
    BitmaskLayerParams {
        data_unit_mode_x: UnitsMode::Data,
        data_unit_mode_y: UnitsMode::Pixels,
        stroke_width_unit_mode: UnitsMode::Pixels,
        ..bitmask_cyx_data()
    }
}

// Y is in data space here, so the inherited `Data` stroke width still resolves
// to exactly one texel.
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
// only skips that division, unlike Pixels mode) -- so a raw img_size of 16x16 would be
// interpreted as 16x the layer's normalized (0,1) extent, way off-canvas. A
// model_matrix scale is the mechanism to bring it into (0,1) space. Scaling by 0.01
// shrinks the 16x16 mask to a 0.16x0.16 normalized extent, which matches
// bitmask_cyx_pixels()'s 16px / 100px layer size exactly on a 100x100 canvas, so this
// renders identically to bitmask_cyx_pixels() there.
//
// The stroke width is given in normalized units too, matching the model_matrix
// scale so that it is exactly one texel: a normalized width is a fraction of
// the layer height, and so is a texel here, so the two stay in step whatever
// the canvas size (unlike a pixel width, which would be one texel only on a
// 100px-tall layer).
fn bitmask_cyx_normalized() -> BitmaskLayerParams {
    with_stroke_width(
        BitmaskLayerParams {
            data_unit_mode_x: UnitsMode::Normalized,
            data_unit_mode_y: UnitsMode::Normalized,
            stroke_width_unit_mode: UnitsMode::Normalized,
            model_matrix: Some([
                0.01, 0.0, 0.0, 0.0,
                0.0, 0.01, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                0.0, 0.0, 0.0, 1.0,
            ]),
            ..bitmask_cyx_data()
        },
        0.01,
    )
}

// Helper: x in data space (unscaled, matching bitmask_cyx_data_x_pixel_y()'s
// treatment of the data axis), y in normalized space (scaled via model_matrix).
fn bitmask_cyx_data_x_normalized_y() -> BitmaskLayerParams {
    with_stroke_width(
        BitmaskLayerParams {
            data_unit_mode_x: UnitsMode::Data,
            data_unit_mode_y: UnitsMode::Normalized,
            stroke_width_unit_mode: UnitsMode::Normalized,
            model_matrix: Some([
                1.0, 0.0, 0.0, 0.0,
                0.0, 0.01, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                0.0, 0.0, 0.0, 1.0,
            ]),
            ..bitmask_cyx_data()
        },
        0.01,
    )
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
        camera_view: Some(CAMERA_ZOOM_OUT_32X),
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
        camera_view: Some(CAMERA_ZOOM_OUT_32X),
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
        camera_view: Some(CAMERA_ZOOM_OUT_32X),
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
        camera_view: Some(CAMERA_ZOOM_OUT_32X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_square_contain_pixel_units_no_margins").await;
}

// Normalized units: on a 100x100 canvas this renders identically to the Pixels
// test above, since bitmask_cyx_normalized()'s model_matrix scale (0.01) applied
// to the 16x16 img_size yields the same 0.16 normalized extent as 16px / 100px.
#[tokio::test]
async fn test_bitmask_layer_square_contain_normalized_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(bitmask_cyx_normalized()),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_32X),
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
        camera_view: Some(CAMERA_ZOOM_OUT_32X),
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
        camera_view: Some(CAMERA_ZOOM_OUT_32X),
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
        camera_view: Some(CAMERA_ZOOM_OUT_32X),
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
        camera_view: Some(CAMERA_ZOOM_OUT_32X),
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
        camera_view: Some(CAMERA_ZOOM_OUT_32X),
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
        camera_view: Some(CAMERA_ZOOM_OUT_32X),
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
        camera_view: Some(CAMERA_ZOOM_OUT_32X),
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
        camera_view: Some(CAMERA_ZOOM_OUT_32X),
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
        camera_view: Some(CAMERA_ZOOM_OUT_32X),
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
        camera_view: Some(CAMERA_ZOOM_OUT_32X),
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
        camera_view: Some(CAMERA_ZOOM_OUT_32X),
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
        camera_view: Some(CAMERA_ZOOM_OUT_32X),
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
        camera_view: Some(CAMERA_ZOOM_OUT_32X),
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
        camera_view: Some(CAMERA_ZOOM_OUT_32X),
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
        camera_view: Some(CAMERA_ZOOM_OUT_32X),
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
        camera_view: Some(CAMERA_ZOOM_OUT_32X),
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
        camera_view: Some(CAMERA_ZOOM_OUT_32X),
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
        camera_view: Some(CAMERA_ZOOM_OUT_32X),
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
        camera_view: Some(CAMERA_ZOOM_OUT_32X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_square_contain_data_units_pixel_offset").await;
}

// Test with a different dimension order (YXC): channels interleaved per pixel,
// rather than contiguous per-channel blocks, verifying the stride-based
// indexing (shared with `BitmapLayer`) is wired correctly.
#[tokio::test]
async fn test_bitmask_layer_square_contain_data_units_yxc_order() {
    let ch0 = MASK;
    let ch1 = MASK;
    let mut data_yxc = Vec::with_capacity(512);
    for i in 0..256 {
        data_yxc.push(ch0[i]);
        data_yxc.push(ch1[i]);
    }
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(BitmaskLayerParams {
            dimension_order: DimensionOrder::YXC,
            shape: vec![16, 16, 2],
            data: NumericData::Uint32(Arc::new(data_yxc)),
            ..bitmask_cyx_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_32X),
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
            stroked_channel(ColorMode::UniformRgb((0, 0, 255)), stroke_width),
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
        camera_view: Some(CAMERA_ZOOM_OUT_32X),
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
        camera_view: Some(CAMERA_ZOOM_OUT_32X),
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
        camera_view: Some(CAMERA_ZOOM_OUT_32X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_square_contain_data_units_wide_stroke").await;
}

// ── Stroke width unit modes ───────────────────────────────────────────────────
//
// `MASK`'s smallest objects are only 2 texels wide, so every object texel is a
// boundary even at a 1-texel stroke and outline *thickness* is invisible in
// its snapshots (the tests above only establish that the three unit modes
// agree). The tests below use `MASK_TOP_8_ROWS` (the diamond and triangle,
// each up to 6 texels wide), where a 1-texel and a 2-texel outline are
// plainly different, to check thickness itself.

// Helper: `MASK_TOP_8_ROWS` as a single outline-only blue channel, with the
// stroke width given in `unit_mode` units.
fn bitmask_thick(unit_mode: UnitsMode, stroke_width: f32) -> BitmaskLayerParams {
    BitmaskLayerParams {
        layer_id: "my_bitmask_layer".to_string(),
        data_unit_mode_x: UnitsMode::Data,
        data_unit_mode_y: UnitsMode::Data,
        stroke_width_unit_mode: unit_mode,
        dimension_order: DimensionOrder::CYX,
        shape: vec![1, 8, 16],
        channel_settings: vec![stroked_channel(ColorMode::UniformRgb((0, 0, 255)), stroke_width)],
        data: NumericData::Uint32(Arc::new(MASK_TOP_8_ROWS.to_vec())),
        ..BitmaskLayerParams::default()
    }
}

// Zoom of 1/16, which on the 200x200 canvas the tests below use makes one
// texel of `MASK_TOP_8_ROWS` exactly 12.5 screen px (so the whole mask
// spans 200px wide by 100px tall).
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
// snapshot above, eating further into each object's interior.
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

// A stroke thinner than one texel: 3 of the 12.5 screen px one texel spans.
// The GPU decides per screen pixel, so it draws a 3px ring inside each
// object's boundary; the SVG path cannot express that at the mask's own
// resolution (whole texels would round it to a 12.5px ring, four times too
// thick), so it up-samples the mask to one grid cell per screen pixel first.
// Widths given in all three unit modes resolve to the same 3px and must
// render identically.
#[tokio::test]
async fn test_bitmask_layer_thick_mask_sub_texel_stroke_data_units() {
    let params = thick_params(bitmask_thick(UnitsMode::Data, 0.24), CAMERA_ZOOM_OUT_16X);
    render_and_check_both_snapshots(params, "test_bitmask_layer_thick_mask_sub_texel_stroke").await;
}

#[tokio::test]
async fn test_bitmask_layer_thick_mask_sub_texel_stroke_pixel_units() {
    let params = thick_params(bitmask_thick(UnitsMode::Pixels, 3.0), CAMERA_ZOOM_OUT_16X);
    render_and_check_both_snapshots(params, "test_bitmask_layer_thick_mask_sub_texel_stroke").await;
}

#[tokio::test]
async fn test_bitmask_layer_thick_mask_sub_texel_stroke_normalized_units() {
    let params = thick_params(bitmask_thick(UnitsMode::Normalized, 0.015), CAMERA_ZOOM_OUT_16X);
    render_and_check_both_snapshots(params, "test_bitmask_layer_thick_mask_sub_texel_stroke").await;
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

// A data-unit stroke width is meaningless when the mask's Y axis is positioned
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
            channel_settings: vec![filled_channel(ColorMode::UniformRgb((255, 128, 0)))],
            shape: vec![1, 16, 16],
            data: repeated_mask_data(1),
            ..bitmask_cyx_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_32X),
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
            channel_settings: vec![filled_channel(ColorMode::InstancedRgb(InstancedRgbParams {
                r_values: NumericData::Uint8(Arc::new(vec![31, 255, 44, 214])),
                g_values: NumericData::Uint8(Arc::new(vec![119, 127, 160, 39])),
                b_values: NumericData::Uint8(Arc::new(vec![180, 14, 44, 40])),
            }))],
            shape: vec![1, 16, 16],
            data: repeated_mask_data(1),
            ..bitmask_cyx_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_32X),
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
            channel_settings: vec![filled_channel(ColorMode::InstancedRgbInterleaved(
                InstancedRgbInterleavedParams {
                    rgb_values: NumericData::Uint8(Arc::new(vec![
                        31, 119, 180, 255, 127, 14, 44, 160, 44, 214, 39, 40,
                    ])),
                },
            ))],
            shape: vec![1, 16, 16],
            data: repeated_mask_data(1),
            ..bitmask_cyx_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_32X),
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
            channel_settings: vec![filled_channel(ColorMode::Categorical(CategoricalParams {
                codes: NumericData::Uint8(Arc::new(vec![0, 1, 2, 3])),
                colormap: CategoricalColormap::Tableau10,
            }))],
            shape: vec![1, 16, 16],
            data: repeated_mask_data(1),
            ..bitmask_cyx_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_32X),
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
            channel_settings: vec![filled_channel(ColorMode::Quantitative(QuantitativeParams {
                values: NumericData::Float32(Arc::new(vec![0.1, 0.4, 0.6, 0.9])),
                colormap: QuantitativeColormap::Viridis,
                reverse: false,
                domain: None,
            }))],
            shape: vec![1, 16, 16],
            data: repeated_mask_data(1),
            ..bitmask_cyx_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_32X),
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
// color-mode textures per color (on top of the one shared mask-data texture),
// and WebGPU's default `max_sampled_textures_per_shader_stage` limit is
// commonly 16 -- a real constraint on how many color-textured channels a single
// `BitmaskLayer` draw call can use at once (`None`/`UniformRgb` share the
// exact same generated WGSL, so only one is exercised here, and each channel
// below textures only one of its two colors).
#[tokio::test]
async fn test_bitmask_layer_square_contain_data_units_all_color_modes() {
    let channel_settings = vec![
        // UniformRgb: outline-only.
        BitmaskChannelSettings {
            stroke_opacity: Some(OpacityMode::UniformOpacity(0.8)),
            ..stroked_channel(ColorMode::UniformRgb((255, 0, 0)), 1.0)
        },
        // InstancedRgb: filled, per-object explicit RGB.
        BitmaskChannelSettings {
            fill_opacity: Some(OpacityMode::UniformOpacity(0.5)),
            ..filled_channel(ColorMode::InstancedRgb(InstancedRgbParams {
                r_values: NumericData::Uint8(Arc::new(vec![31, 255, 44, 214])),
                g_values: NumericData::Uint8(Arc::new(vec![119, 127, 160, 39])),
                b_values: NumericData::Uint8(Arc::new(vec![180, 14, 44, 40])),
            }))
        },
        // InstancedRgbInterleaved: filled.
        BitmaskChannelSettings {
            fill_opacity: Some(OpacityMode::UniformOpacity(0.5)),
            ..filled_channel(ColorMode::InstancedRgbInterleaved(
                InstancedRgbInterleavedParams {
                    rgb_values: NumericData::Uint8(Arc::new(vec![
                        31, 119, 180, 255, 127, 14, 44, 160, 44, 214, 39, 40,
                    ])),
                },
            ))
        },
        // Categorical ("set colors" via a named palette): filled.
        BitmaskChannelSettings {
            fill_opacity: Some(OpacityMode::UniformOpacity(0.5)),
            ..filled_channel(ColorMode::Categorical(CategoricalParams {
                codes: NumericData::Uint8(Arc::new(vec![0, 1, 2, 3])),
                colormap: CategoricalColormap::Tableau10,
            }))
        },
        // CategoricalCustom ("set colors" via explicit RGB list): outline-only.
        BitmaskChannelSettings {
            stroke_opacity: Some(OpacityMode::UniformOpacity(0.5)),
            ..stroked_channel(
                ColorMode::CategoricalCustom(CategoricalCustomParams {
                    values: NumericData::Uint8(Arc::new(vec![0, 1, 2, 3])),
                    colormap: vec![(10, 20, 30), (200, 100, 50), (50, 200, 100), (100, 50, 200)],
                }),
                1.0,
            )
        },
        // Quantitative: filled, exercises the injected colormap function.
        BitmaskChannelSettings {
            fill_opacity: Some(OpacityMode::UniformOpacity(0.5)),
            ..filled_channel(ColorMode::Quantitative(QuantitativeParams {
                values: NumericData::Float32(Arc::new(vec![0.1, 0.4, 0.6, 0.9])),
                colormap: QuantitativeColormap::Viridis,
                reverse: false,
                domain: None,
            }))
        },
    ];
    let num_channels = channel_settings.len();

    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(BitmaskLayerParams {
            shape: vec![num_channels as u32, 16, 16],
            data: repeated_mask_data(num_channels),
            channel_settings,
            opacity: 0.9,
            ..bitmask_cyx_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_32X),
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
            shape: vec![2, 16, 16],
            data: repeated_mask_data(2),
            channel_settings: vec![
                filled_channel(ColorMode::Quantitative(QuantitativeParams {
                    values: NumericData::Float32(Arc::new(vec![0.1, 0.4, 0.6, 0.9])),
                    colormap: QuantitativeColormap::Viridis,
                    reverse: false,
                    domain: None,
                })),
                BitmaskChannelSettings {
                    fill_opacity: Some(OpacityMode::UniformOpacity(0.5)),
                    ..filled_channel(ColorMode::Quantitative(QuantitativeParams {
                        values: NumericData::Float32(Arc::new(vec![0.9, 0.6, 0.4, 0.1])),
                        colormap: QuantitativeColormap::Viridis,
                        reverse: true,
                        domain: Some((0.0, 1.0)),
                    }))
                },
            ],
            ..bitmask_cyx_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_32X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_square_contain_data_units_quantitative_colormap_dedup").await;
}

// ── Stroked and filled together ───────────────────────────────────────────────
//
// The outline band is the outermost part of an object's interior, so a channel
// that is both stroked and filled draws its stroke over the boundary and its
// fill over what is left, each with its own color and opacity (never a blend of
// the two at one pixel). `MASK_TOP_8_ROWS`'s objects are wide enough for a
// 1-texel outline to leave a visible fill inside them.

// Helper: `MASK_TOP_8_ROWS` as one channel drawn both stroked (blue) and filled
// (semi-transparent orange), with the stroke width in `unit_mode` units.
fn bitmask_thick_stroked_and_filled(unit_mode: UnitsMode, stroke_width: f32) -> BitmaskLayerParams {
    BitmaskLayerParams {
        channel_settings: vec![BitmaskChannelSettings {
            stroked: true,
            filled: true,
            stroke_color: Some(ColorMode::UniformRgb((0, 0, 255))),
            stroke_width: Some(SizeMode::UniformSize(stroke_width)),
            stroke_opacity: Some(OpacityMode::UniformOpacity(1.0)),
            fill_color: Some(ColorMode::UniformRgb((255, 128, 0))),
            fill_opacity: Some(OpacityMode::UniformOpacity(0.4)),
            ..Default::default()
        }],
        ..bitmask_thick(unit_mode, stroke_width)
    }
}

// A 1-texel blue outline around a 40%-opaque orange interior.
#[tokio::test]
async fn test_bitmask_layer_thick_mask_stroked_and_filled() {
    let params = thick_params(
        bitmask_thick_stroked_and_filled(UnitsMode::Data, 1.0),
        CAMERA_ZOOM_OUT_16X,
    );
    render_and_check_both_snapshots(params, "test_bitmask_layer_thick_mask_stroked_and_filled").await;
}

// The same channel with a sub-texel (3 of the 12.5 px a texel spans) stroke, so
// the fill has to be drawn on the up-sampled SVG raster grid the thin outline
// forces -- checking that up-sampling reproduces the fill unchanged rather than
// resampling it.
#[tokio::test]
async fn test_bitmask_layer_thick_mask_stroked_and_filled_sub_texel_stroke() {
    let params = thick_params(
        bitmask_thick_stroked_and_filled(UnitsMode::Pixels, 3.0),
        CAMERA_ZOOM_OUT_16X,
    );
    render_and_check_both_snapshots(
        params,
        "test_bitmask_layer_thick_mask_stroked_and_filled_sub_texel_stroke",
    ).await;
}

// Per-object stroke width, opacity and color: `MASK`'s diamond (1) and plus
// (3) get a 1-texel opaque red/blue outline, the triangle (2) and ring (4) a
// 2-texel half-transparent green/yellow one, over a shared filled channel.
// Exercises the instanced `SizeMode`/`OpacityMode` paths (a value texture per
// property, indexed by object id).
#[tokio::test]
async fn test_bitmask_layer_square_contain_data_units_instanced_stroke() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(BitmaskLayerParams {
            shape: vec![1, 16, 16],
            data: repeated_mask_data(1),
            channel_settings: vec![BitmaskChannelSettings {
                stroked: true,
                filled: true,
                stroke_color: Some(ColorMode::InstancedRgb(InstancedRgbParams {
                    r_values: NumericData::Uint8(Arc::new(vec![255, 0, 0, 255])),
                    g_values: NumericData::Uint8(Arc::new(vec![0, 255, 0, 255])),
                    b_values: NumericData::Uint8(Arc::new(vec![0, 0, 255, 0])),
                })),
                stroke_width: Some(SizeMode::InstancedSize(InstancedSizeParams {
                    values: NumericData::Float32(Arc::new(vec![1.0, 2.0, 1.0, 2.0])),
                })),
                stroke_opacity: Some(OpacityMode::InstancedOpacity(InstancedOpacityParams {
                    values: NumericData::Float32(Arc::new(vec![1.0, 0.5, 1.0, 0.5])),
                })),
                fill_color: Some(ColorMode::UniformRgb((0, 0, 255))),
                fill_opacity: Some(OpacityMode::UniformOpacity(0.3)),
                ..Default::default()
            }],
            ..bitmask_cyx_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_32X),
        ..Default::default()
    };
    render_and_check_both_snapshots(
        params,
        "test_bitmask_layer_square_contain_data_units_instanced_stroke",
    ).await;
}

// A channel with neither `stroked` nor `filled` draws nothing, the replacement
// for the old per-channel `visible` flag.
#[tokio::test]
async fn test_bitmask_layer_square_contain_data_units_neither_stroked_nor_filled() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(BitmaskLayerParams {
            shape: vec![1, 16, 16],
            data: repeated_mask_data(1),
            channel_settings: vec![BitmaskChannelSettings {
                stroked: false,
                filled: false,
                ..filled_channel(ColorMode::UniformRgb((255, 128, 0)))
            }],
            ..bitmask_cyx_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_32X),
        ..Default::default()
    };
    render_and_check_both_snapshots(
        params,
        "test_bitmask_layer_square_contain_data_units_neither_stroked_nor_filled",
    ).await;
}

// A single all-background channel: should render fully transparent.
#[tokio::test]
async fn test_bitmask_layer_square_contain_data_units_empty_mask() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(BitmaskLayerParams {
            shape: vec![1, 16, 16],
            data: NumericData::Uint32(Arc::new(vec![0u32; 256])),
            channel_settings: vec![BitmaskChannelSettings::default()],
            ..bitmask_cyx_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_32X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_square_contain_data_units_empty_mask").await;
}

// Two channels with genuinely different content (rather than the same mask
// repeated per channel, as `repeated_mask_data` gives every other multi-
// channel test): channel 0 keeps only the diamond (1) and triangle (2) via
// `mask_filtered`, channel 1 keeps only the plus (3) and ring (4). Each is
// filled with its own color, exercising channel blending where the channels'
// footprints don't overlap.
#[tokio::test]
async fn test_bitmask_layer_square_contain_data_units_differentiated_channels() {
    let mut data = mask_filtered(&[1, 2]);
    data.extend(mask_filtered(&[3, 4]));
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(BitmaskLayerParams {
            shape: vec![2, 16, 16],
            data: NumericData::Uint32(Arc::new(data)),
            channel_settings: vec![
                filled_channel(ColorMode::UniformRgb((255, 128, 0))),
                filled_channel(ColorMode::UniformRgb((0, 128, 255))),
            ],
            ..bitmask_cyx_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_32X),
        ..Default::default()
    };
    render_and_check_both_snapshots(
        params,
        "test_bitmask_layer_square_contain_data_units_differentiated_channels",
    ).await;
}

// `MASK_LEFT_8_COLS` (16x8, tall): the diamond (1) and plus (3), the other
// "other array shape" variant besides `MASK_TOP_8_ROWS` above -- exercises a
// mask whose height exceeds its width, rather than the other way around.
#[tokio::test]
async fn test_bitmask_layer_square_contain_data_units_left_8_cols_shape() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(BitmaskLayerParams {
            shape: vec![1, 16, 8],
            data: NumericData::Uint32(Arc::new(MASK_LEFT_8_COLS.to_vec())),
            channel_settings: vec![filled_channel(object_colors())],
            ..bitmask_cyx_data()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_32X),
        ..Default::default()
    };
    render_and_check_both_snapshots(
        params,
        "test_bitmask_layer_square_contain_data_units_left_8_cols_shape",
    ).await;
}

// ── Bitmask fully outside layer bounds ────────────────────────────────────────

// `pixel_offset` places the 16x16 mask at (1000, 1000) on a 100x100 canvas, so
// its on-screen rect (1000..1016, 1000..1016) has no overlap whatsoever with
// the layer's visible area (0..100, 0..100) -- the mask is entirely off-screen.
//
// Intended behavior for the SVG path: `BitmaskLayer::draw` (`DrawToSvg` impl)
// must detect that none of the mask overlaps the layer bounds *before* doing
// any CPU rasterization, and skip rasterizing/encoding the mask entirely
// rather than rasterizing an image that would just be clipped away by
// `clip_rect`. So the resulting SVG snapshot must contain no bitmask image
// element (or containing group) at all -- not merely a clipped-to-nothing one.
#[tokio::test]
async fn test_bitmask_layer_square_contain_pixel_units_fully_out_of_bounds() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: layer_params(BitmaskLayerParams {
            pixel_offset: Some((1000, 1000)),
            ..bitmask_cyx_pixels()
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmask_layer_square_contain_pixel_units_fully_out_of_bounds").await;
}
