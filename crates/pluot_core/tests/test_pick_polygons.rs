use std::sync::Arc;

use pluot_core::composite_layers::polygon_layer::{PolygonLayer, PolygonLayerParams};
use pluot_core::render_traits::PickableLayer;
use pluot_core::viewport::{DataCoord, ScreenCoord};
use pluot_core::{LayerPickingResult, UnitsMode, ViewParams, NumericData};

/// A single 10x10 square ring: (0,0), (10,0), (10,10), (0,10).
fn square_polygon_data() -> (NumericData, NumericData) {
    (
        NumericData::Float32(Arc::new(vec![
            0.0, 0.0, 10.0, 0.0, 10.0, 10.0, 0.0, 10.0,
        ])),
        NumericData::Uint32(Arc::new(vec![0, 4])),
    )
}

fn pick_at(layer: &PolygonLayer, x: f32, y: f32) -> Option<LayerPickingResult> {
    layer.pick(
        ScreenCoord { x: 0.0, y: 0.0 },
        Some(DataCoord::TwoD { x, y }),
    )
}

#[test]
fn test_pick_filled_containment() {
    let (polygons, polygon_offsets) = square_polygon_data();
    let layer = PolygonLayer::new(
        ViewParams::default(),
        PolygonLayerParams {
            layer_id: "test_polygon".to_string(),
            polygons,
            polygon_offsets,
            stroked: false,
            filled: true,
            ..PolygonLayerParams::default()
        },
    );

    // Inside the square: contained by the fill.
    let r = pick_at(&layer, 5.0, 5.0).unwrap();
    assert_eq!(r.layer_id, "test_polygon_filled");
    assert_eq!(r.info.get("index").unwrap(), "0");

    // Outside the square: fill-only layer has nothing to fall back to.
    assert!(pick_at(&layer, 50.0, 50.0).is_none());
}

#[test]
fn test_pick_stroked_nearest_edge() {
    let (polygons, polygon_offsets) = square_polygon_data();
    let layer = PolygonLayer::new(
        ViewParams::default(),
        PolygonLayerParams {
            layer_id: "test_polygon".to_string(),
            polygons,
            polygon_offsets,
            stroked: true,
            filled: false,
            ..PolygonLayerParams::default()
        },
    );

    // Near the left edge (x=0): nearest-edge picking always finds a match.
    let r = pick_at(&layer, 0.5, 5.0).unwrap();
    assert_eq!(r.layer_id, "test_polygon_stroked");
    assert_eq!(r.info.get("index").unwrap(), "0");

    // Far from the polygon: still returns the nearest edge (no threshold).
    let r = pick_at(&layer, 1000.0, 1000.0).unwrap();
    assert_eq!(r.info.get("index").unwrap(), "0");
}

#[test]
fn test_pick_fill_takes_priority_over_stroke() {
    let (polygons, polygon_offsets) = square_polygon_data();
    let layer = PolygonLayer::new(
        ViewParams::default(),
        PolygonLayerParams {
            layer_id: "test_polygon".to_string(),
            polygons,
            polygon_offsets,
            stroked: true,
            filled: true,
            ..PolygonLayerParams::default()
        },
    );

    // Inside the square: the fill sub-layer's containment hit wins over the
    // stroke sub-layer's unconditional nearest-edge hit.
    let r = pick_at(&layer, 5.0, 5.0).unwrap();
    assert_eq!(r.layer_id, "test_polygon_filled");
}

#[test]
fn test_pick_with_scale_matrix() {
    // world = 2 * model: the square now spans world [0,20]^2.
    let model_matrix = [
        2.0, 0.0, 0.0, 0.0,
        0.0, 2.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        0.0, 0.0, 0.0, 1.0,
    ];
    let (polygons, polygon_offsets) = square_polygon_data();
    let layer = PolygonLayer::new(
        ViewParams::default(),
        PolygonLayerParams {
            layer_id: "test_polygon".to_string(),
            model_matrix: Some(model_matrix),
            polygons,
            polygon_offsets,
            stroked: false,
            filled: true,
            ..PolygonLayerParams::default()
        },
    );

    let r = pick_at(&layer, 10.0, 10.0).unwrap();
    assert_eq!(r.info.get("index").unwrap(), "0");

    assert!(pick_at(&layer, 30.0, 30.0).is_none());
}

#[test]
fn test_pick_pixels_units_mode_returns_none() {
    let (polygons, polygon_offsets) = square_polygon_data();
    let layer = PolygonLayer::new(
        ViewParams::default(),
        PolygonLayerParams {
            layer_id: "test_polygon".to_string(),
            polygons,
            polygon_offsets,
            stroked: true,
            filled: true,
            data_unit_mode_x: UnitsMode::Pixels,
            data_unit_mode_y: UnitsMode::Pixels,
            ..PolygonLayerParams::default()
        },
    );
    assert!(pick_at(&layer, 5.0, 5.0).is_none());
}
