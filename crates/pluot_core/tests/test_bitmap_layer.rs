use std::sync::Arc;

mod test_utils;
use test_utils::render_and_check_both_snapshots;

use pluot_core::params::{RenderParams, PlotParams, LayerParams, LayeredPlotRenderParams};
use pluot_core::render_traits::{AspectRatioMode, UnitsMode, MarginParams};
use pluot_core::layers::bitmap_layer::{BitmapLayerParams, ChannelSettings, DimensionOrder, NumericData};

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

// Helper: a 4×4 two-channel image in CYX order (matches the JS demo)
// Channel 0 (red): low values, Channel 1 (blue): high values
fn bitmap_cyx_data() -> BitmapLayerParams {
    BitmapLayerParams {
        layer_id: "my_bitmap_layer".to_string(),
        bounds: None,
        data_unit_mode: UnitsMode::Data,
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

// Helper: same image in Pixels unit mode (4×4 pixel image positioned in pixel space)
fn bitmap_cyx_pixels() -> BitmapLayerParams {
    BitmapLayerParams {
        data_unit_mode: UnitsMode::Pixels,
        ..bitmap_cyx_data()
    }
}

fn layer_params(bitmap_params: BitmapLayerParams) -> Vec<LayerParams> {
    vec![LayerParams {
        layer_type: "BitmapLayer".to_string(),
        layer_params: serde_json::to_value(bitmap_params).unwrap(),
    }]
}

// Column-major 4×4 scale matrix: zoom of 1/8 (zoomed out 8×), centered at origin.
// Format matches position_utils.rs: [scale, 0, 0, 0, 0, scale, 0, 0, 0, 0, 0, 0, tx, ty, 0, 1]
const CAMERA_ZOOM_OUT_8X: [f32; 16] = [
    0.125, 0.0,   0.0, 0.0,
    0.0,   0.125, 0.0, 0.0,
    0.0,   0.0,   0.0, 0.0,
    0.0,   0.0,   0.0, 1.0,
];

// ── Square canvas (100×100) ───────────────────────────────────────────────────

#[tokio::test]
async fn test_bitmap_layer_square_contain_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(BitmapLayerParams {
                bounds: Some(MarginParams {
                    margin_left: Some(0.0),
                    margin_right: Some(0.0),
                    margin_top: Some(0.0),
                    margin_bottom: Some(0.0),
                }),
                ..bitmap_cyx_data()
            }),
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
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(bitmap_cyx_data()),
        }),
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
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(bitmap_cyx_data()),
        }),
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
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(bitmap_cyx_pixels()),
        }),
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
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(bitmap_cyx_data()),
        }),
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
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(BitmapLayerParams {
                bounds: Some(MarginParams {
                    margin_left: Some(10.0),
                    margin_right: Some(10.0),
                    margin_top: Some(10.0),
                    margin_bottom: Some(10.0),
                }),
                ..bitmap_cyx_data()
            }),
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
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(BitmapLayerParams {
                bounds: Some(MarginParams {
                    margin_left: Some(10.0),
                    margin_right: Some(10.0),
                    margin_top: Some(10.0),
                    margin_bottom: Some(10.0),
                }),
                ..bitmap_cyx_data()
            }),
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

// ── Wide canvas (200×100) ─────────────────────────────────────────────────────

#[tokio::test]
async fn test_bitmap_layer_wide_ignore_data_units_no_margins() {
    let params = RenderParams {
        width: 200,
        height: 100,
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(bitmap_cyx_data()),
        }),
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
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(bitmap_cyx_data()),
        }),
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
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(bitmap_cyx_data()),
        }),
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
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(bitmap_cyx_pixels()),
        }),
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
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(bitmap_cyx_data()),
        }),
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
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(BitmapLayerParams {
                bounds: Some(MarginParams {
                    margin_left: Some(10.0),
                    margin_right: Some(10.0),
                    margin_top: Some(10.0),
                    margin_bottom: Some(10.0),
                }),
                ..bitmap_cyx_data()
            }),
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmap_layer_wide_contain_data_units_layer_bounds").await;
}

// ── Tall canvas (100×200) ─────────────────────────────────────────────────────

#[tokio::test]
async fn test_bitmap_layer_tall_ignore_data_units_no_margins() {
    let params = RenderParams {
        width: 100,
        height: 200,
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(bitmap_cyx_data()),
        }),
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
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(bitmap_cyx_data()),
        }),
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
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(bitmap_cyx_data()),
        }),
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
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(bitmap_cyx_pixels()),
        }),
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
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(bitmap_cyx_data()),
        }),
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
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(BitmapLayerParams {
                bounds: Some(MarginParams {
                    margin_left: Some(10.0),
                    margin_right: Some(10.0),
                    margin_top: Some(10.0),
                    margin_bottom: Some(10.0),
                }),
                ..bitmap_cyx_data()
            }),
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
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(BitmapLayerParams {
                opacity: 0.5,
                ..bitmap_cyx_data()
            }),
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
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(BitmapLayerParams {
                pixel_offset: Some((1, 1)),
                ..bitmap_cyx_data()
            }),
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
    // Same 4×4 two-channel image, but stored in XYC order
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
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(BitmapLayerParams {
                dimension_order: DimensionOrder::XYC,
                shape: vec![4, 4, 2],
                data: NumericData::Uint16(Arc::new(data_xyc)),
                ..bitmap_cyx_data()
            }),
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
        plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
            layers: layer_params(BitmapLayerParams {
                channel_settings: vec![
                    ChannelSettings { window: (100.0, 200.0), color: (1.0, 0.0, 0.0) },
                    ChannelSettings { window: (100.0, 200.0), color: (0.0, 0.0, 1.0) },
                ],
                ..bitmap_cyx_data()
            }),
        }),
        aspect_ratio_mode: AspectRatioMode::Contain,
        camera_view: Some(CAMERA_ZOOM_OUT_8X),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_bitmap_layer_square_contain_data_units_narrow_window").await;
}
