use std::sync::Arc;

use pluot_core::layers::text_layer::{TextLayer, TextLayerParams};
use pluot_core::render_traits::PickableLayer;
use pluot_core::viewport::{DataCoord, ScreenCoord};
use pluot_core::{LayerPickingResult, UnitsMode, ViewParams, NumericData};

fn make_layer(model_matrix: Option<[f32; 16]>) -> TextLayer {
    TextLayer::new(
        ViewParams::default(),
        TextLayerParams {
            layer_id: "test_text".to_string(),
            model_matrix,
            position_x: NumericData::Float32(Arc::new(vec![0.0, 10.0])),
            position_y: NumericData::Float32(Arc::new(vec![0.0, 10.0])),
            text_vec: Arc::new(vec!["a".to_string(), "b".to_string()]),
            ..TextLayerParams::default()
        },
    )
}

fn pick_at(layer: &TextLayer, x: f32, y: f32) -> Option<LayerPickingResult> {
    layer.pick(
        ScreenCoord { x: 0.0, y: 0.0 },
        Some(DataCoord::TwoD { x, y }),
    )
}

#[test]
fn test_pick_identity_matrix() {
    let layer = make_layer(None);

    let r = pick_at(&layer, 0.5, 0.5).unwrap();
    assert_eq!(r.layer_id, "test_text");
    assert_eq!(r.info.get("index").unwrap(), "0");
    assert_eq!(r.info.get("text").unwrap(), "a");

    let r = pick_at(&layer, 9.0, 9.0).unwrap();
    assert_eq!(r.info.get("index").unwrap(), "1");
    assert_eq!(r.info.get("text").unwrap(), "b");
}

#[test]
fn test_pick_with_scale_matrix() {
    // world = 2 * model: the label at model (10, 10) appears at world (20, 20).
    let model_matrix = [
        2.0, 0.0, 0.0, 0.0,
        0.0, 2.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        0.0, 0.0, 0.0, 1.0,
    ];
    let layer = make_layer(Some(model_matrix));

    // world (19, 19) --> model (9.5, 9.5) --> closest to label 1.
    let r = pick_at(&layer, 19.0, 19.0).unwrap();
    assert_eq!(r.info.get("index").unwrap(), "1");

    // world (1, 1) --> model (0.5, 0.5) --> closest to label 0.
    let r = pick_at(&layer, 1.0, 1.0).unwrap();
    assert_eq!(r.info.get("index").unwrap(), "0");
}

#[test]
fn test_pick_pixels_units_mode_returns_none() {
    let layer = TextLayer::new(
        ViewParams::default(),
        TextLayerParams {
            layer_id: "test_text".to_string(),
            position_x: NumericData::Float32(Arc::new(vec![0.0])),
            position_y: NumericData::Float32(Arc::new(vec![0.0])),
            text_vec: Arc::new(vec!["a".to_string()]),
            data_unit_mode_x: UnitsMode::Pixels,
            data_unit_mode_y: UnitsMode::Pixels,
            ..TextLayerParams::default()
        },
    );
    assert!(pick_at(&layer, 0.0, 0.0).is_none());
}
