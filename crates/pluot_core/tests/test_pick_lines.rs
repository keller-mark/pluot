use std::sync::Arc;

use pluot_core::layers::line_layer::{LineLayer, LineLayerParams};
use pluot_core::render_traits::PickableLayer;
use pluot_core::viewport::{DataCoord, ScreenCoord};
use pluot_core::{LayerPickingResult, UnitsMode, ViewParams, NumericData};

/// Two horizontal segments: (0,0)-(10,0) and (0,20)-(10,20).
fn make_layer(model_matrix: Option<[f32; 16]>) -> LineLayer {
    LineLayer::new(
        ViewParams::default(),
        LineLayerParams {
            layer_id: "test_line".to_string(),
            model_matrix,
            source_position_x: NumericData::Float32(Arc::new(vec![0.0, 0.0])),
            source_position_y: NumericData::Float32(Arc::new(vec![0.0, 20.0])),
            target_position_x: NumericData::Float32(Arc::new(vec![10.0, 10.0])),
            target_position_y: NumericData::Float32(Arc::new(vec![0.0, 20.0])),
            ..LineLayerParams::default()
        },
    )
}

fn pick_at(layer: &LineLayer, x: f32, y: f32) -> Option<LayerPickingResult> {
    layer.pick(
        ScreenCoord { x: 0.0, y: 0.0 },
        Some(DataCoord::TwoD { x, y }),
    )
}

#[test]
fn test_pick_identity_matrix() {
    let layer = make_layer(None);

    // Closest to segment 0 (y=0).
    let r = pick_at(&layer, 5.0, 1.0).unwrap();
    assert_eq!(r.layer_id, "test_line");
    assert_eq!(r.info.get("index").unwrap(), "0");

    // Closest to segment 1 (y=20).
    let r = pick_at(&layer, 5.0, 19.0).unwrap();
    assert_eq!(r.info.get("index").unwrap(), "1");
}

#[test]
fn test_pick_with_scale_matrix() {
    // world = 2 * model: segment 1 now appears at world y=40.
    let model_matrix = [
        2.0, 0.0, 0.0, 0.0,
        0.0, 2.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        0.0, 0.0, 0.0, 1.0,
    ];
    let layer = make_layer(Some(model_matrix));

    let r = pick_at(&layer, 10.0, 1.0).unwrap();
    assert_eq!(r.info.get("index").unwrap(), "0");

    let r = pick_at(&layer, 10.0, 39.0).unwrap();
    assert_eq!(r.info.get("index").unwrap(), "1");
}

#[test]
fn test_pick_pixels_units_mode_returns_none() {
    let layer = LineLayer::new(
        ViewParams::default(),
        LineLayerParams {
            layer_id: "test_line".to_string(),
            source_position_x: NumericData::Float32(Arc::new(vec![0.0])),
            source_position_y: NumericData::Float32(Arc::new(vec![0.0])),
            target_position_x: NumericData::Float32(Arc::new(vec![10.0])),
            target_position_y: NumericData::Float32(Arc::new(vec![0.0])),
            data_unit_mode_x: UnitsMode::Pixels,
            data_unit_mode_y: UnitsMode::Pixels,
            ..LineLayerParams::default()
        },
    );
    assert!(pick_at(&layer, 5.0, 0.0).is_none());
}
