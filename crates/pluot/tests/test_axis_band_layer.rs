#![cfg(not(target_arch = "wasm32"))]

use std::sync::Arc;

mod test_utils;
use test_utils::render_and_check_both_snapshots;

use pluot::{
    get_camera_matrix_from_bounds, AspectRatioMode, AxisBandLayerParams, AxisPosition, DataBounds, HeatmapLayerParams,
    LayerParams, MarginParams, NumericData, RenderParams, ViewParams,
};

const WIDTH: u32 = 200;
const HEIGHT: u32 = 200;
const MARGIN_LEFT: f32 = 40.0;
const MARGIN_BOTTOM: f32 = 40.0;

fn labels(prefix: &str, n: usize) -> Arc<Vec<String>> {
    Arc::new((0..n).map(|i| format!("{prefix}{i}")).collect())
}

/// A 4-column, 3-row heatmap drawn in cell units, so that it spans data
/// x in [0, 4] and y in [0, 3], with row 0 at the top.
fn heatmap() -> LayerParams {
    LayerParams::HeatmapLayer(HeatmapLayerParams {
        layer_id: "heatmap".to_string(),
        num_rows: 3,
        num_cols: 4,
        data: NumericData::Float32(Arc::new((0..12).map(|v| v as f32 / 11.0).collect())),
        ..Default::default()
    })
}

fn band_axes(data_ranges: Option<((f64, f64), (f64, f64))>) -> Vec<LayerParams> {
    let mut row_labels = labels("row", 3).to_vec();
    // Band axes run bottom-to-top along y, whereas row 0 is drawn at the top.
    row_labels.reverse();
    vec![
        LayerParams::AxisBandLayer(AxisBandLayerParams {
            layer_id: "x_axis".to_string(),
            position: AxisPosition::Bottom,
            domain: labels("col", 4),
            data_range: data_ranges.map(|(x, _)| x),
        }),
        LayerParams::AxisBandLayer(AxisBandLayerParams {
            layer_id: "y_axis".to_string(),
            position: AxisPosition::Left,
            domain: Arc::new(row_labels),
            data_range: data_ranges.map(|(_, y)| y),
        }),
    ]
}

/// The camera showing exactly `bounds`, as a client would fit it to a layer's extent.
fn camera_for(bounds: DataBounds) -> [f32; 16] {
    let view_params = ViewParams {
        width: WIDTH,
        height: HEIGHT,
        aspect_ratio_mode: AspectRatioMode::Ignore,
        margins: Some(MarginParams {
            margin_left: Some(MARGIN_LEFT),
            margin_bottom: Some(MARGIN_BOTTOM),
            margin_top: Some(0.0),
            margin_right: Some(0.0),
        }),
        ..Default::default()
    };
    get_camera_matrix_from_bounds(&view_params, &bounds)
}

fn params(layers: Vec<LayerParams>, camera_view: Option<[f32; 16]>) -> RenderParams {
    RenderParams {
        width: WIDTH,
        height: HEIGHT,
        margin_left: Some(MARGIN_LEFT),
        margin_bottom: Some(MARGIN_BOTTOM),
        aspect_ratio_mode: AspectRatioMode::Ignore,
        camera_view,
        layers,
        ..Default::default()
    }
}

#[tokio::test]
async fn test_axis_band_layer_pixels_ignore_camera() {
    let camera = camera_for(DataBounds { x_min: 0.0, x_max: 2.0, y_min: 0.0, y_max: 2.0 });
    render_and_check_both_snapshots(params(band_axes(None), Some(camera)), "test_axis_band_layer_pixels_ignore_camera").await;
}

#[tokio::test]
async fn test_axis_band_layer_data_range_fit_to_heatmap_extent() {
    let camera = camera_for(DataBounds { x_min: 0.0, x_max: 4.0, y_min: 0.0, y_max: 3.0 });
    let mut layers = vec![heatmap()];
    layers.extend(band_axes(Some(((0.0, 4.0), (0.0, 3.0)))));
    render_and_check_both_snapshots(params(layers, Some(camera)), "test_axis_band_layer_data_range_fit_to_heatmap_extent").await;
}

// Zoomed in on the bottom-left of the heatmap: only the bands whose centers
// are visible (col0, col1, row2, row1) are labeled, each under its cells.
#[tokio::test]
async fn test_axis_band_layer_data_range_zoomed_in() {
    let camera = camera_for(DataBounds { x_min: 0.0, x_max: 2.0, y_min: 0.0, y_max: 2.0 });
    let mut layers = vec![heatmap()];
    layers.extend(band_axes(Some(((0.0, 4.0), (0.0, 3.0)))));
    render_and_check_both_snapshots(params(layers, Some(camera)), "test_axis_band_layer_data_range_zoomed_in").await;
}

// Zoomed out, the bands occupy only part of each axis.
#[tokio::test]
async fn test_axis_band_layer_data_range_zoomed_out() {
    let camera = camera_for(DataBounds { x_min: -2.0, x_max: 6.0, y_min: -1.0, y_max: 5.0 });
    let mut layers = vec![heatmap()];
    layers.extend(band_axes(Some(((0.0, 4.0), (0.0, 3.0)))));
    render_and_check_both_snapshots(params(layers, Some(camera)), "test_axis_band_layer_data_range_zoomed_out").await;
}
