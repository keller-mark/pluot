use std::sync::Arc;

use pluot_core::layers::rect_layer::{RectLayer, RectLayerParams};
use pluot_core::render_traits::PickableLayer;
use pluot_core::viewport::{DataCoord, ScreenCoord};
use pluot_core::{LayerPickingResult, UnitsMode, ViewParams, NumericData};

/// Two rects: [0,0]-[10,10] and [20,20]-[30,30].
fn make_layer(model_matrix: Option<[f32; 16]>) -> RectLayer {
    RectLayer::new(
        ViewParams::default(),
        RectLayerParams {
            layer_id: "test_rect".to_string(),
            model_matrix,
            position_x0: NumericData::Float32(Arc::new(vec![0.0, 20.0])),
            position_y0: NumericData::Float32(Arc::new(vec![0.0, 20.0])),
            position_x1: NumericData::Float32(Arc::new(vec![10.0, 30.0])),
            position_y1: NumericData::Float32(Arc::new(vec![10.0, 30.0])),
            ..RectLayerParams::default()
        },
    )
}

fn pick_at(layer: &RectLayer, x: f32, y: f32) -> Option<LayerPickingResult> {
    layer.pick(
        ScreenCoord { x: 0.0, y: 0.0 },
        Some(DataCoord::TwoD { x, y }),
    )
}

#[test]
fn test_pick_identity_matrix() {
    let layer = make_layer(None);

    let r = pick_at(&layer, 5.0, 5.0).unwrap();
    assert_eq!(r.layer_id, "test_rect");
    assert_eq!(r.info.get("index").unwrap(), "0");

    let r = pick_at(&layer, 25.0, 25.0).unwrap();
    assert_eq!(r.info.get("index").unwrap(), "1");
}

#[test]
fn test_pick_outside_returns_none() {
    let layer = make_layer(None);
    assert!(pick_at(&layer, 15.0, 15.0).is_none());
    assert!(pick_at(&layer, -5.0, 5.0).is_none());
}

#[test]
fn test_pick_with_scale_matrix() {
    // world = 2 * model: rect 0 spans world [0,20]^2, rect 1 spans [40,60]^2.
    let model_matrix = [
        2.0, 0.0, 0.0, 0.0,
        0.0, 2.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        0.0, 0.0, 0.0, 1.0,
    ];
    let layer = make_layer(Some(model_matrix));

    let r = pick_at(&layer, 10.0, 10.0).unwrap();
    assert_eq!(r.info.get("index").unwrap(), "0");

    let r = pick_at(&layer, 50.0, 50.0).unwrap();
    assert_eq!(r.info.get("index").unwrap(), "1");

    assert!(pick_at(&layer, 30.0, 30.0).is_none());
}

#[test]
fn test_pick_pixels_units_mode_returns_none() {
    let layer = RectLayer::new(
        ViewParams::default(),
        RectLayerParams {
            layer_id: "test_rect".to_string(),
            position_x0: NumericData::Float32(Arc::new(vec![0.0])),
            position_y0: NumericData::Float32(Arc::new(vec![0.0])),
            position_x1: NumericData::Float32(Arc::new(vec![10.0])),
            position_y1: NumericData::Float32(Arc::new(vec![10.0])),
            data_unit_mode_x: UnitsMode::Pixels,
            data_unit_mode_y: UnitsMode::Pixels,
            ..RectLayerParams::default()
        },
    );
    assert!(pick_at(&layer, 5.0, 5.0).is_none());
}
