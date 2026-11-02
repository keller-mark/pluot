use std::sync::Arc;

use pluot_core::composite_layers::curve_layer::{CurveLayer, CurveLayerParams, PathCommand};
use pluot_core::layers::filled_curve_layer::FilledCurvePickingInfo;
use pluot_core::layers::stroked_curve_layer::StrokedCurvePickingInfo;
use pluot_core::render_traits::PickableLayer;
use pluot_core::viewport::{DataCoord, ScreenCoord};
use pluot_core::numeric_data::NumericData;
use pluot_core::render_traits::{CategoricalCriteriaParams, EmphasisCriteria};
use pluot_core::{LayerPickingResult, UnitsMode, ViewParams};

/// A single 10x10 closed square path: (0,0) -> (10,0) -> (10,10) -> (0,10) -> close.
fn square_commands() -> Arc<Vec<PathCommand>> {
    Arc::new(vec![
        PathCommand::MoveTo { x: 0.0, y: 0.0 },
        PathCommand::LineTo { x: 10.0, y: 0.0 },
        PathCommand::LineTo { x: 10.0, y: 10.0 },
        PathCommand::LineTo { x: 0.0, y: 10.0 },
        PathCommand::Close,
    ])
}

fn pick_at(layer: &CurveLayer, x: f32, y: f32) -> Option<LayerPickingResult> {
    layer.pick(
        ScreenCoord { x: 0.0, y: 0.0 },
        Some(DataCoord::TwoD { x, y }),
    )
}

#[test]
fn test_pick_filled_containment() {
    let layer = CurveLayer::new(
        ViewParams::default(),
        CurveLayerParams {
            layer_id: "test_curve".to_string(),
            commands: square_commands(),
            stroked: false,
            filled: true,
            ..CurveLayerParams::default()
        },
    );

    // Inside the square: contained by the fill.
    let r = pick_at(&layer, 5.0, 5.0).unwrap();
    assert_eq!(r.layer_id, "test_curve_filled");
    assert_eq!(r.info_as::<FilledCurvePickingInfo>().unwrap().subpath_index, 0);

    // Outside the square: fill-only layer has nothing to fall back to.
    assert!(pick_at(&layer, 50.0, 50.0).is_none());
}

#[test]
fn test_pick_stroked_nearest_segment() {
    let layer = CurveLayer::new(
        ViewParams::default(),
        CurveLayerParams {
            layer_id: "test_curve".to_string(),
            commands: square_commands(),
            stroked: true,
            filled: false,
            ..CurveLayerParams::default()
        },
    );

    // Near the left edge (x=0): nearest-segment picking always finds a match.
    let r = pick_at(&layer, 0.5, 5.0).unwrap();
    assert_eq!(r.layer_id, "test_curve_stroked");
    assert_eq!(r.info_as::<StrokedCurvePickingInfo>().unwrap().subpath_index, 0);

    // Far from the curve: still returns the nearest segment (no threshold).
    assert!(pick_at(&layer, 1000.0, 1000.0).is_some());
}

#[test]
fn test_pick_fill_takes_priority_over_stroke() {
    let layer = CurveLayer::new(
        ViewParams::default(),
        CurveLayerParams {
            layer_id: "test_curve".to_string(),
            commands: square_commands(),
            stroked: true,
            filled: true,
            ..CurveLayerParams::default()
        },
    );

    // Inside the square: the fill sub-layer's containment hit wins over the
    // stroke sub-layer's unconditional nearest-segment hit.
    let r = pick_at(&layer, 5.0, 5.0).unwrap();
    assert_eq!(r.layer_id, "test_curve_filled");
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
    let layer = CurveLayer::new(
        ViewParams::default(),
        CurveLayerParams {
            layer_id: "test_curve".to_string(),
            model_matrix: Some(model_matrix),
            commands: square_commands(),
            stroked: false,
            filled: true,
            ..CurveLayerParams::default()
        },
    );

    let r = pick_at(&layer, 10.0, 10.0).unwrap();
    assert_eq!(r.info_as::<FilledCurvePickingInfo>().unwrap().subpath_index, 0);

    assert!(pick_at(&layer, 30.0, 30.0).is_none());
}

#[test]
fn test_pick_pixels_units_mode_returns_none() {
    let layer = CurveLayer::new(
        ViewParams::default(),
        CurveLayerParams {
            layer_id: "test_curve".to_string(),
            commands: square_commands(),
            stroked: true,
            filled: true,
            data_unit_mode_x: UnitsMode::Pixels,
            data_unit_mode_y: UnitsMode::Pixels,
            ..CurveLayerParams::default()
        },
    );
    assert!(pick_at(&layer, 5.0, 5.0).is_none());
}

/// Two curves: the first is two squares at x in [0, 10] and [20, 30]; the second is a square at x in [40, 50].
fn multi_curve_layer(stroked: bool, filled: bool, filtering_criteria: Vec<EmphasisCriteria>) -> CurveLayer {
    let square_at = |x0: f32| vec![
        PathCommand::MoveTo { x: x0, y: 0.0 },
        PathCommand::LineTo { x: x0 + 10.0, y: 0.0 },
        PathCommand::LineTo { x: x0 + 10.0, y: 10.0 },
        PathCommand::LineTo { x: x0, y: 10.0 },
        PathCommand::Close,
    ];
    let commands: Vec<PathCommand> = [square_at(0.0), square_at(20.0), square_at(40.0)].concat();
    CurveLayer::new(
        ViewParams::default(),
        CurveLayerParams {
            layer_id: "test_curve".to_string(),
            commands: Arc::new(commands),
            curve_offsets: Some(Arc::new(vec![0, 10, 15])),
            stroked,
            filled,
            filtering_criteria,
            ..CurveLayerParams::default()
        },
    )
}

#[test]
fn test_pick_filled_multiple_curves() {
    let layer = multi_curve_layer(false, true, vec![]);
    let info = |x| pick_at(&layer, x, 5.0).unwrap().into_info::<FilledCurvePickingInfo>().unwrap();
    assert_eq!((info(5.0).curve_index, info(5.0).subpath_index), (0, 0));
    assert_eq!((info(25.0).curve_index, info(25.0).subpath_index), (0, 1));
    assert_eq!((info(45.0).curve_index, info(45.0).subpath_index), (1, 2));
    assert!(pick_at(&layer, 15.0, 5.0).is_none());
}

#[test]
fn test_pick_stroked_multiple_curves() {
    let layer = multi_curve_layer(true, false, vec![]);
    let info = pick_at(&layer, 50.5, 5.0).unwrap().into_info::<StrokedCurvePickingInfo>().unwrap();
    assert_eq!((info.curve_index, info.subpath_index), (1, 2));
}

#[test]
fn test_pick_ignores_filtered_out_curves() {
    let filter_out_first_curve = vec![EmphasisCriteria::Categorical(CategoricalCriteriaParams {
        codes: NumericData::Int32(Arc::new(vec![0, 1])),
        included_codes: vec![1],
    })];
    let filled = multi_curve_layer(false, true, filter_out_first_curve.clone());
    assert!(pick_at(&filled, 5.0, 5.0).is_none());
    assert_eq!(pick_at(&filled, 45.0, 5.0).unwrap().info_as::<FilledCurvePickingInfo>().unwrap().curve_index, 1);

    let stroked = multi_curve_layer(true, false, filter_out_first_curve);
    assert_eq!(pick_at(&stroked, 0.5, 5.0).unwrap().info_as::<StrokedCurvePickingInfo>().unwrap().curve_index, 1);
}
