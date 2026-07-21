//! Tests for `NumericData` and its use as the dtype-generic coordinate type of
//! `PointLayer`.

use std::sync::Arc;

use pluot_core::layers::point_layer::PointLayerParams;
use pluot_core::numeric_data::NumericData;

// Guards the JSON contract exercised by the layer registry
// (`serde_json::from_value` in `PointLayer`'s `inventory::submit!`): coordinate
// arrays use the tagged `NumericData` form, and X and Y may differ in dtype.
#[test]
fn deserializes_tagged_numeric_positions_with_differing_dtypes() {
    let json = r#"{
        "layer_id": "pts",
        "position_x": {"dtype": "Float32", "values": [0.0, 1.0, 2.0]},
        "position_y": {"dtype": "Uint16", "values": [10, 20, 30]}
    }"#;
    let params: PointLayerParams = serde_json::from_str(json).unwrap();
    assert!(matches!(params.position_x, NumericData::Float32(_)));
    assert!(matches!(params.position_y, NumericData::Uint16(_)));
    assert_eq!(params.position_x.len(), 3);
    assert_eq!(params.position_y.len(), 3);
}

// A bare (untagged) array is rejected: the tagged form is required, matching
// `BitmapLayer.data`. (Bindings pass e.g. `{"Float32": [...]}`.)
#[test]
fn rejects_bare_array_for_position() {
    let json = r#"{ "layer_id": "pts", "position_x": [0.0, 1.0] }"#;
    assert!(serde_json::from_str::<PointLayerParams>(json).is_err());
}

// The adjacently-tagged representation round-trips through
// serialize + deserialize.
#[test]
fn tagged_representation_round_trips() {
    let data = NumericData::Int32(Arc::new(vec![-1, 0, 7]));
    let json = serde_json::to_string(&data).unwrap();
    assert_eq!(json, r#"{"dtype":"Int32","values":[-1,0,7]}"#);
    let back: NumericData = serde_json::from_str(&json).unwrap();
    assert!(matches!(back, NumericData::Int32(_)));
}

#[test]
fn from_impls_select_the_matching_variant() {
    // Vec<T> and Arc<Vec<T>> both convert; each type maps to one variant.
    assert!(matches!(NumericData::from(vec![1.0f32, 2.0]), NumericData::Float32(_)));
    assert!(matches!(NumericData::from(vec![1u8, 2]), NumericData::Uint8(_)));
    assert!(matches!(NumericData::from(vec![1i64, 2]), NumericData::Int64(_)));

    let arc: Arc<Vec<u32>> = Arc::new(vec![1, 2, 3]);
    let nd: NumericData = arc.into();
    assert!(matches!(nd, NumericData::Uint32(_)));
}

#[test]
fn accessors_convert_and_format_across_dtypes() {
    let u = NumericData::Uint16(Arc::new(vec![7, 300]));
    assert_eq!(u.len(), 2);
    assert!(!u.is_empty());
    assert_eq!(u.get_f32(1), 300.0);
    assert_eq!(u.get_f64(0), 7.0);
    // Integers format without a decimal point (used by picking display).
    assert_eq!(u.format_element(1), "300");

    let f = NumericData::Float32(Arc::new(vec![1.5]));
    assert_eq!(f.format_element(0), "1.5");
}
