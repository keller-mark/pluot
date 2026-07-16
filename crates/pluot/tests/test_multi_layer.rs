#![cfg(not(target_arch = "wasm32"))]

use std::sync::Arc;

mod test_utils;
use test_utils::render_and_check_both_snapshots;

use pluot::{
    RenderParams, LayerParams,
    AspectRatioMode, UnitsMode, MarginParams,
    PointLayerParams, PointShapeMode,
    NumericData,
    LineLayerParams,
    TextLayerParams, TextAlignMode, TextBaselineMode,
    FontWeight, FontStyle,
};

// Multi-layer tests exercise rendering of multiple layers stacked together.
// We test combinations of:
// - Layer ordering (points on top vs. lines on top)
// - Different aspect ratio modes
// - Square, wide, and tall canvases
// - With and without view-level margins
// - Mixed layer types: point+line, point+text, line+text, point+line+text

// Shared layer helpers

fn corner_points() -> PointLayerParams {
    PointLayerParams {
        layer_id: "points".to_string(),
        bounds: None,
        data_unit_mode_x: UnitsMode::Data,
        data_unit_mode_y: UnitsMode::Data,
        point_radius: 8.0,
        point_radius_unit_mode_x: UnitsMode::Pixels,
        point_radius_unit_mode_y: UnitsMode::Pixels,
        point_shape_mode: PointShapeMode::Square,
        model_matrix: None,
        position_x: NumericData::Float32(Arc::new(vec![0.0, 1.0, 1.0, 0.0])),
        position_y: NumericData::Float32(Arc::new(vec![0.0, 0.0, 1.0, 1.0])),
        labels_vec: Arc::new(vec![0, 1, 2, 3]),
        ..Default::default()
    }
}

fn cross_lines() -> LineLayerParams {
    LineLayerParams {
        layer_id: "lines".to_string(),
        bounds: None,
        data_unit_mode_x: UnitsMode::Data,
        data_unit_mode_y: UnitsMode::Data,
        line_width: 2.0,
        line_width_unit_mode: UnitsMode::Pixels,
        model_matrix: None,
        source_position_x: NumericData::Float32(Arc::new(vec![0.0, 1.0])),
        source_position_y: NumericData::Float32(Arc::new(vec![0.0, 0.0])),
        target_position_x: NumericData::Float32(Arc::new(vec![1.0, 0.0])),
        target_position_y: NumericData::Float32(Arc::new(vec![1.0, 1.0])),
        labels_vec: Arc::new(vec![0, 1]),
    }
}

fn corner_labels() -> TextLayerParams {
    TextLayerParams {
        layer_id: "labels".to_string(),
        bounds: None,
        data_unit_mode_x: UnitsMode::Data,
        data_unit_mode_y: UnitsMode::Data,
        text_size: 10.0,
        text_size_unit_mode: UnitsMode::Pixels,
        text_align_mode: TextAlignMode::Middle,
        text_baseline_mode: TextBaselineMode::Middle,
        model_matrix: None,
        text_rotation: None,
        font_family: None,
        font_weight: FontWeight::Normal,
        font_style: FontStyle::Normal,
        position_x: NumericData::Float32(Arc::new(vec![0.0, 1.0, 1.0, 0.0])),
        position_y: NumericData::Float32(Arc::new(vec![0.0, 0.0, 1.0, 1.0])),
        text_vec: Arc::new(vec![
            "A".to_string(),
            "B".to_string(),
            "C".to_string(),
            "D".to_string(),
        ]),
    }
}

fn point_layer_params(p: PointLayerParams) -> LayerParams {
    LayerParams::PointLayer(p)
}

fn line_layer_params(l: LineLayerParams) -> LayerParams {
    LayerParams::LineLayer(l)
}

fn text_layer_params(t: TextLayerParams) -> LayerParams {
    LayerParams::TextLayer(t)
}

// Point + Line

// Points rendered first (below lines)
#[tokio::test]
async fn test_multi_layer_square_contain_points_then_lines() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: vec![
            point_layer_params(corner_points()),
            line_layer_params(cross_lines()),
        ],
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_multi_layer_square_contain_points_then_lines").await;
}

// Lines rendered first (below points)
#[tokio::test]
async fn test_multi_layer_square_contain_lines_then_points() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: vec![
            line_layer_params(cross_lines()),
            point_layer_params(corner_points()),
        ],
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_multi_layer_square_contain_lines_then_points").await;
}

// Point + Text

#[tokio::test]
async fn test_multi_layer_square_contain_points_then_text() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: vec![
            point_layer_params(corner_points()),
            text_layer_params(corner_labels()),
        ],
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_multi_layer_square_contain_points_then_text").await;
}

#[tokio::test]
async fn test_multi_layer_square_contain_text_then_points() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: vec![
            text_layer_params(corner_labels()),
            point_layer_params(corner_points()),
        ],
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_multi_layer_square_contain_text_then_points").await;
}

// Line + Text

#[tokio::test]
async fn test_multi_layer_square_contain_lines_then_text() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: vec![
            line_layer_params(cross_lines()),
            text_layer_params(corner_labels()),
        ],
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_multi_layer_square_contain_lines_then_text").await;
}

// All three layers

#[tokio::test]
async fn test_multi_layer_square_contain_lines_points_text() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: vec![
            line_layer_params(cross_lines()),
            point_layer_params(corner_points()),
            text_layer_params(corner_labels()),
        ],
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_multi_layer_square_contain_lines_points_text").await;
}

#[tokio::test]
async fn test_multi_layer_square_ignore_lines_points_text() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: vec![
            line_layer_params(cross_lines()),
            point_layer_params(corner_points()),
            text_layer_params(corner_labels()),
        ],
        aspect_ratio_mode: AspectRatioMode::Ignore,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_multi_layer_square_ignore_lines_points_text").await;
}

#[tokio::test]
async fn test_multi_layer_square_cover_lines_points_text() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: vec![
            line_layer_params(cross_lines()),
            point_layer_params(corner_points()),
            text_layer_params(corner_labels()),
        ],
        aspect_ratio_mode: AspectRatioMode::Cover,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_multi_layer_square_cover_lines_points_text").await;
}

// With view margins

#[tokio::test]
async fn test_multi_layer_square_contain_lines_points_text_view_margins() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: vec![
            line_layer_params(cross_lines()),
            point_layer_params(corner_points()),
            text_layer_params(corner_labels()),
        ],
        aspect_ratio_mode: AspectRatioMode::Contain,
        margin_left: Some(10.0),
        margin_right: Some(10.0),
        margin_top: Some(10.0),
        margin_bottom: Some(10.0),
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_multi_layer_square_contain_lines_points_text_view_margins").await;
}

// With per-layer bounds

// Two layers with different individual bounds (each occupies a different sub-region)
#[tokio::test]
async fn test_multi_layer_square_contain_split_bounds() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: vec![
            // Points confined to left half
            point_layer_params(PointLayerParams {
                bounds: Some(MarginParams {
                    margin_left: Some(0.0),
                    margin_right: Some(50.0),
                    margin_top: Some(0.0),
                    margin_bottom: Some(0.0),
                }),
                ..corner_points()
            }),
            // Lines confined to right half
            line_layer_params(LineLayerParams {
                bounds: Some(MarginParams {
                    margin_left: Some(50.0),
                    margin_right: Some(0.0),
                    margin_top: Some(0.0),
                    margin_bottom: Some(0.0),
                }),
                ..cross_lines()
            }),
        ],
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_multi_layer_square_contain_split_bounds").await;
}

// Wide and tall canvases

#[tokio::test]
async fn test_multi_layer_wide_contain_lines_points_text() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: vec![
            line_layer_params(cross_lines()),
            point_layer_params(corner_points()),
            text_layer_params(corner_labels()),
        ],
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_multi_layer_wide_contain_lines_points_text").await;
}

#[tokio::test]
async fn test_multi_layer_wide_ignore_lines_points_text() {
    let params = RenderParams {
        width: 200,
        height: 100,
        layers: vec![
            line_layer_params(cross_lines()),
            point_layer_params(corner_points()),
            text_layer_params(corner_labels()),
        ],
        aspect_ratio_mode: AspectRatioMode::Ignore,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_multi_layer_wide_ignore_lines_points_text").await;
}

#[tokio::test]
async fn test_multi_layer_tall_contain_lines_points_text() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: vec![
            line_layer_params(cross_lines()),
            point_layer_params(corner_points()),
            text_layer_params(corner_labels()),
        ],
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_multi_layer_tall_contain_lines_points_text").await;
}

#[tokio::test]
async fn test_multi_layer_tall_ignore_lines_points_text() {
    let params = RenderParams {
        width: 100,
        height: 200,
        layers: vec![
            line_layer_params(cross_lines()),
            point_layer_params(corner_points()),
            text_layer_params(corner_labels()),
        ],
        aspect_ratio_mode: AspectRatioMode::Ignore,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_multi_layer_tall_ignore_lines_points_text").await;
}

// ── Same layer type stacked twice ─────────────────────────────────────────────

// Two point layers with different positions, confirming they both render
#[tokio::test]
async fn test_multi_layer_square_contain_two_point_layers() {
    let params = RenderParams {
        width: 100,
        height: 100,
        layers: vec![
            point_layer_params(PointLayerParams {
                layer_id: "points_a".to_string(),
                position_x: NumericData::Float32(Arc::new(vec![0.0, 1.0])),
                position_y: NumericData::Float32(Arc::new(vec![0.0, 0.0])),
                labels_vec: Arc::new(vec![0, 1]),
                ..corner_points()
            }),
            point_layer_params(PointLayerParams {
                layer_id: "points_b".to_string(),
                position_x: NumericData::Float32(Arc::new(vec![0.0, 1.0])),
                position_y: NumericData::Float32(Arc::new(vec![1.0, 1.0])),
                labels_vec: Arc::new(vec![2, 3]),
                ..corner_points()
            }),
        ],
        aspect_ratio_mode: AspectRatioMode::Contain,
        ..Default::default()
    };
    render_and_check_both_snapshots(params, "test_multi_layer_square_contain_two_point_layers").await;
}
