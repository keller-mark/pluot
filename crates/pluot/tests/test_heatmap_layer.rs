#![cfg(not(target_arch = "wasm32"))]

use std::sync::Arc;

mod test_utils;
use test_utils::render_and_check_both_snapshots;

use pluot::{
    AspectRatioMode, CategoricalColormap, HeatmapColormap, HeatmapLayerParams, HeatmapQuantitativeColormapParams,
    LayerParams, NumericData, QuantitativeColormap, RenderParams, UnitsMode,
};

// Scales the 4x3 cell grid into data space (0, 1) so it fills the layer at the identity camera.
const FIT_4_COLS_3_ROWS: [f32; 16] = [
    0.25, 0.0, 0.0, 0.0,
    0.0, 1.0 / 3.0, 0.0, 0.0,
    0.0, 0.0, 1.0, 0.0,
    0.0, 0.0, 0.0, 1.0,
];

fn quantitative_heatmap() -> HeatmapLayerParams {
    HeatmapLayerParams {
        layer_id: "my_heatmap_layer".to_string(),
        num_rows: 3,
        num_cols: 4,
        data: NumericData::Float32(Arc::new(vec![
            0.0, 1.0, 2.0, 3.0,
            4.0, 5.0, 6.0, 7.0,
            8.0, 9.0, 10.0, 11.0,
        ])),
        model_matrix: Some(FIT_4_COLS_3_ROWS),
        colormap: Some(HeatmapColormap::Quantitative(HeatmapQuantitativeColormapParams {
            colormap: QuantitativeColormap::Viridis,
            reverse: false,
            domain: Some((0.0, 10.0)),
        })),
        ..Default::default()
    }
}

fn params(heatmap: HeatmapLayerParams) -> RenderParams {
    RenderParams {
        width: 100,
        height: 100,
        aspect_ratio_mode: AspectRatioMode::Ignore,
        layers: vec![LayerParams::HeatmapLayer(heatmap)],
        ..Default::default()
    }
}

#[tokio::test]
async fn test_heatmap_layer_quantitative() {
    render_and_check_both_snapshots(params(quantitative_heatmap()), "test_heatmap_layer_quantitative").await;
}

#[tokio::test]
async fn test_heatmap_layer_quantitative_reversed_integer_data() {
    let heatmap = HeatmapLayerParams {
        data: NumericData::Uint16(Arc::new((0..12).collect())),
        colormap: Some(HeatmapColormap::Quantitative(HeatmapQuantitativeColormapParams {
            colormap: QuantitativeColormap::Plasma,
            reverse: true,
            domain: Some((0.0, 11.0)),
        })),
        ..quantitative_heatmap()
    };
    render_and_check_both_snapshots(params(heatmap), "test_heatmap_layer_quantitative_reversed_integer_data").await;
}

#[tokio::test]
async fn test_heatmap_layer_categorical() {
    let heatmap = HeatmapLayerParams {
        data: NumericData::Int32(Arc::new(vec![0, 1, 2, 3, 1, 2, 3, 0, 2, 3, 0, -1])),
        colormap: Some(HeatmapColormap::Categorical(CategoricalColormap::Tableau10)),
        ..quantitative_heatmap()
    };
    render_and_check_both_snapshots(params(heatmap), "test_heatmap_layer_categorical").await;
}

#[tokio::test]
async fn test_heatmap_layer_categorical_custom() {
    let heatmap = HeatmapLayerParams {
        data: NumericData::Uint8(Arc::new(vec![0, 1, 0, 1, 1, 0, 1, 0, 0, 1, 0, 1])),
        colormap: Some(HeatmapColormap::CategoricalCustom(vec![(255, 0, 0), (0, 0, 255)])),
        ..quantitative_heatmap()
    };
    render_and_check_both_snapshots(params(heatmap), "test_heatmap_layer_categorical_custom").await;
}

#[tokio::test]
async fn test_heatmap_layer_swap_axes() {
    let heatmap = HeatmapLayerParams {
        swap_axes: true,
        model_matrix: Some([
            1.0 / 3.0, 0.0, 0.0, 0.0,
            0.0, 0.25, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ]),
        ..quantitative_heatmap()
    };
    render_and_check_both_snapshots(params(heatmap), "test_heatmap_layer_swap_axes").await;
}

#[tokio::test]
async fn test_heatmap_layer_row_and_col_selection() {
    let heatmap = HeatmapLayerParams {
        row_selection: Some(NumericData::Uint8(Arc::new(vec![1, 0, 1]))),
        col_selection: Some(NumericData::Uint8(Arc::new(vec![0, 1, 1, 1]))),
        background_opacity: Some(0.3),
        ..quantitative_heatmap()
    };
    render_and_check_both_snapshots(params(heatmap), "test_heatmap_layer_row_and_col_selection").await;
}

#[tokio::test]
async fn test_heatmap_layer_pixel_units_with_offset_and_margins() {
    let heatmap = HeatmapLayerParams {
        data_unit_mode_x: UnitsMode::Pixels,
        data_unit_mode_y: UnitsMode::Pixels,
        cell_offset: Some((1.0, 1.0)),
        model_matrix: Some([
            10.0, 0.0, 0.0, 0.0,
            0.0, 10.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ]),
        ..quantitative_heatmap()
    };
    let render_params = RenderParams {
        margin_left: Some(20.0),
        margin_bottom: Some(10.0),
        ..params(heatmap)
    };
    render_and_check_both_snapshots(render_params, "test_heatmap_layer_pixel_units_with_offset_and_margins").await;
}

// Column-major camera matrices: [zoom, 0, 0, 0, 0, zoom, 0, 0, 0, 0, 1, 0, tx, ty, 0, 1].
const CAMERA_ZOOM_OUT_2X: [f32; 16] = [
    0.5, 0.0, 0.0, 0.0,
    0.0, 0.5, 0.0, 0.0,
    0.0, 0.0, 1.0, 0.0,
    0.0, 0.0, 0.0, 1.0,
];

// Zoomed in 2x and panned so that the bottom-left quadrant of the heatmap fills the view.
const CAMERA_ZOOM_IN_2X_PAN_BOTTOM_LEFT: [f32; 16] = [
    2.0, 0.0, 0.0, 0.0,
    0.0, 2.0, 0.0, 0.0,
    0.0, 0.0, 1.0, 0.0,
    1.0, 1.0, 0.0, 1.0,
];

// Panned right and up by a quarter of the view, without zooming.
const CAMERA_PAN_RIGHT_UP: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0,
    0.0, 1.0, 0.0, 0.0,
    0.0, 0.0, 1.0, 0.0,
    0.5, 0.5, 0.0, 1.0,
];

#[tokio::test]
async fn test_heatmap_layer_camera_zoom_out() {
    let render_params = RenderParams { camera_view: Some(CAMERA_ZOOM_OUT_2X), ..params(quantitative_heatmap()) };
    render_and_check_both_snapshots(render_params, "test_heatmap_layer_camera_zoom_out").await;
}

#[tokio::test]
async fn test_heatmap_layer_camera_zoom_in_pan() {
    let render_params = RenderParams { camera_view: Some(CAMERA_ZOOM_IN_2X_PAN_BOTTOM_LEFT), ..params(quantitative_heatmap()) };
    render_and_check_both_snapshots(render_params, "test_heatmap_layer_camera_zoom_in_pan").await;
}

#[tokio::test]
async fn test_heatmap_layer_camera_pan() {
    let render_params = RenderParams { camera_view: Some(CAMERA_PAN_RIGHT_UP), ..params(quantitative_heatmap()) };
    render_and_check_both_snapshots(render_params, "test_heatmap_layer_camera_pan").await;
}

#[tokio::test]
async fn test_heatmap_layer_camera_zoom_in_pan_swap_axes_with_selection() {
    let heatmap = HeatmapLayerParams {
        swap_axes: true,
        row_selection: Some(NumericData::Uint8(Arc::new(vec![0, 1, 1]))),
        ..quantitative_heatmap()
    };
    let render_params = RenderParams { camera_view: Some(CAMERA_ZOOM_IN_2X_PAN_BOTTOM_LEFT), ..params(heatmap) };
    render_and_check_both_snapshots(render_params, "test_heatmap_layer_camera_zoom_in_pan_swap_axes_with_selection").await;
}

#[tokio::test]
async fn test_heatmap_layer_camera_zoom_out_with_margins() {
    let render_params = RenderParams {
        camera_view: Some(CAMERA_ZOOM_OUT_2X),
        margin_left: Some(20.0),
        margin_top: Some(10.0),
        ..params(quantitative_heatmap())
    };
    render_and_check_both_snapshots(render_params, "test_heatmap_layer_camera_zoom_out_with_margins").await;
}

#[tokio::test]
async fn test_heatmap_layer_camera_pixel_units_ignore_camera() {
    let heatmap = HeatmapLayerParams {
        data_unit_mode_x: UnitsMode::Pixels,
        data_unit_mode_y: UnitsMode::Pixels,
        model_matrix: Some([
            10.0, 0.0, 0.0, 0.0,
            0.0, 10.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ]),
        ..quantitative_heatmap()
    };
    let render_params = RenderParams { camera_view: Some(CAMERA_ZOOM_IN_2X_PAN_BOTTOM_LEFT), ..params(heatmap) };
    render_and_check_both_snapshots(render_params, "test_heatmap_layer_camera_pixel_units_ignore_camera").await;
}

// Non-square canvases, where the camera and the aspect ratio mode interact.
async fn check_aspect_ratio(width: u32, height: u32, aspect_ratio_mode: AspectRatioMode, camera_view: [f32; 16], name: &str) {
    let render_params = RenderParams {
        width,
        height,
        aspect_ratio_mode,
        camera_view: Some(camera_view),
        ..params(quantitative_heatmap())
    };
    render_and_check_both_snapshots(render_params, name).await;
}

#[tokio::test]
async fn test_heatmap_layer_wide_contain_zoom_out() {
    check_aspect_ratio(200, 100, AspectRatioMode::Contain, CAMERA_ZOOM_OUT_2X, "test_heatmap_layer_wide_contain_zoom_out").await;
}

#[tokio::test]
async fn test_heatmap_layer_wide_cover_zoom_out() {
    check_aspect_ratio(200, 100, AspectRatioMode::Cover, CAMERA_ZOOM_OUT_2X, "test_heatmap_layer_wide_cover_zoom_out").await;
}

#[tokio::test]
async fn test_heatmap_layer_tall_contain_zoom_in_pan() {
    check_aspect_ratio(100, 200, AspectRatioMode::Contain, CAMERA_ZOOM_IN_2X_PAN_BOTTOM_LEFT, "test_heatmap_layer_tall_contain_zoom_in_pan").await;
}

#[tokio::test]
async fn test_heatmap_layer_tall_ignore_pan() {
    check_aspect_ratio(100, 200, AspectRatioMode::Ignore, CAMERA_PAN_RIGHT_UP, "test_heatmap_layer_tall_ignore_pan").await;
}
