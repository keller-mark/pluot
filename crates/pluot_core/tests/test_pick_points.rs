use std::sync::Arc;

use pluot_core::layers::point_layer::{PointLayer, PointLayerParams};
use pluot_core::render_traits::PickableLayer;
use pluot_core::viewport::{DataCoord, ScreenCoord};
use pluot_core::{LayerPickingResult, ViewParams, NumericData};

fn make_layer(model_matrix: Option<[f32; 16]>) -> PointLayer {
    PointLayer::new(
        ViewParams::default(),
        PointLayerParams {
            layer_id: "test_point".to_string(),
            model_matrix,
            position_x: NumericData::Float32(Arc::new(vec![0.0, 10.0])),
            position_y: NumericData::Float32(Arc::new(vec![0.0, 10.0])),
            ..PointLayerParams::default()
        },
    )
}

fn pick_at(layer: &PointLayer, x: f32, y: f32) -> Option<LayerPickingResult> {
    layer.pick(
        ScreenCoord { x: 0.0, y: 0.0 },
        Some(DataCoord::TwoD { x, y }),
    )
}

#[test]
fn test_pick_identity_matrix() {
    let layer = make_layer(None);

    let r = pick_at(&layer, 0.5, 0.5).unwrap();
    assert_eq!(r.layer_id, "test_point");
    assert_eq!(r.info.get("index").unwrap(), "0");

    let r = pick_at(&layer, 9.0, 9.0).unwrap();
    assert_eq!(r.info.get("index").unwrap(), "1");
}

#[test]
fn test_pick_with_scale_matrix() {
    // world = 2 * model: the point at model (10, 10) appears at world (20, 20).
    let model_matrix = [
        2.0, 0.0, 0.0, 0.0,
        0.0, 2.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        0.0, 0.0, 0.0, 1.0,
    ];
    let layer = make_layer(Some(model_matrix));

    // world (19, 19) --> model (9.5, 9.5) --> closest to point 1.
    let r = pick_at(&layer, 19.0, 19.0).unwrap();
    assert_eq!(r.info.get("index").unwrap(), "1");

    // world (1, 1) --> model (0.5, 0.5) --> closest to point 0.
    let r = pick_at(&layer, 1.0, 1.0).unwrap();
    assert_eq!(r.info.get("index").unwrap(), "0");
}

#[test]
fn test_pick_with_translation_matrix() {
    // world = model + (100, 0): shifts both points 100 units in x.
    let model_matrix = [
        1.0, 0.0, 0.0, 0.0,
        0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        100.0, 0.0, 0.0, 1.0,
    ];
    let layer = make_layer(Some(model_matrix));

    let r = pick_at(&layer, 100.5, 0.5).unwrap();
    assert_eq!(r.info.get("index").unwrap(), "0");

    let r = pick_at(&layer, 109.0, 9.0).unwrap();
    assert_eq!(r.info.get("index").unwrap(), "1");
}
