use std::sync::Arc;

use pluot_core::layers::bitmap_layer::DimensionOrder;
use pluot_core::layers::bitmask_layer::{BitmaskChannelSettings, BitmaskLayer, BitmaskLayerParams};
use pluot_core::render_traits::PickableLayer;
use pluot_core::viewport::{DataCoord, ScreenCoord};
use pluot_core::{LayerPickingResult, NumericData, UnitsMode, ViewParams};

/// A 2x2 single-channel mask (shape [1, 2, 2] in CYX order) with object ids:
///   array row 0 (top):    [1, 2]
///   array row 1 (bottom): [0, 3]
fn make_layer(
    pixel_offset: Option<(u32, u32)>,
    model_matrix: Option<[f32; 16]>,
) -> BitmaskLayer {
    BitmaskLayer::new(
        ViewParams::default(),
        BitmaskLayerParams {
            layer_id: "test_bitmask".to_string(),
            pixel_offset,
            model_matrix,
            dimension_order: DimensionOrder::CYX,
            shape: vec![1, 2, 2],
            channel_settings: vec![BitmaskChannelSettings::default()],
            data: NumericData::Uint32(Arc::new(vec![1, 2, 0, 3])),
            ..BitmaskLayerParams::default()
        },
    )
}

fn pick_at(layer: &BitmaskLayer, x: f32, y: f32) -> Option<LayerPickingResult> {
    layer.pick(
        ScreenCoord { x: 0.0, y: 0.0 },
        Some(DataCoord::TwoD { x, y }),
    )
}

#[test]
fn test_pick_identity_matrix() {
    // With no model_matrix, world == pixel: the mask spans [0,2]^2 with
    // the quad's bottom edge sampling the last array row.
    let layer = make_layer(None, None);

    // Bottom-left quadrant --> array row 1, col 0 --> object id 0 (background).
    let r = pick_at(&layer, 0.5, 0.5).unwrap();
    assert_eq!(r.layer_id, "test_bitmask");
    assert_eq!(r.info.get("x").unwrap(), "0");
    assert_eq!(r.info.get("y").unwrap(), "1");
    assert_eq!(r.info.get("channel_0").unwrap(), "0");

    // Top-right quadrant --> array row 0, col 1 --> object id 2.
    let r = pick_at(&layer, 1.5, 1.5).unwrap();
    assert_eq!(r.info.get("x").unwrap(), "1");
    assert_eq!(r.info.get("y").unwrap(), "0");
    assert_eq!(r.info.get("channel_0").unwrap(), "2");

    // Bottom-right quadrant --> array row 1, col 1 --> object id 3.
    let r = pick_at(&layer, 1.5, 0.5).unwrap();
    assert_eq!(r.info.get("channel_0").unwrap(), "3");
}

#[test]
fn test_pick_outside_returns_none() {
    let layer = make_layer(None, None);
    assert!(pick_at(&layer, -0.5, 0.5).is_none());
    assert!(pick_at(&layer, 2.5, 0.5).is_none());
    assert!(pick_at(&layer, 0.5, 2.5).is_none());
    assert!(pick_at(&layer, 0.5, -0.5).is_none());
}

#[test]
fn test_pick_with_scale_matrix() {
    // world = 2 * pixel: the mask spans [0,4]^2.
    let model_matrix = [
        2.0, 0.0, 0.0, 0.0,
        0.0, 2.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        0.0, 0.0, 0.0, 1.0,
    ];
    let layer = make_layer(None, Some(model_matrix));

    // world (3, 1) --> pixel (1.5, 0.5) --> array row 1, col 1 --> object id 3.
    let r = pick_at(&layer, 3.0, 1.0).unwrap();
    assert_eq!(r.info.get("x").unwrap(), "1");
    assert_eq!(r.info.get("y").unwrap(), "1");
    assert_eq!(r.info.get("channel_0").unwrap(), "3");

    assert!(pick_at(&layer, 4.5, 1.0).is_none());
}

#[test]
fn test_pick_with_pixel_offset() {
    // pixel_offset shifts the mask to world [2,4]^2 (identity matrix).
    let layer = make_layer(Some((2, 2)), None);

    assert!(pick_at(&layer, 0.5, 0.5).is_none());

    // world (2.5, 2.5) --> local pixel (0.5, 0.5) --> array row 1, col 0 --> object id 0.
    let r = pick_at(&layer, 2.5, 2.5).unwrap();
    assert_eq!(r.info.get("x").unwrap(), "0");
    assert_eq!(r.info.get("y").unwrap(), "1");
    assert_eq!(r.info.get("channel_0").unwrap(), "0");
}

#[test]
fn test_pick_pixels_units_mode_returns_none() {
    let layer = BitmaskLayer::new(
        ViewParams::default(),
        BitmaskLayerParams {
            layer_id: "test_bitmask".to_string(),
            dimension_order: DimensionOrder::CYX,
            shape: vec![1, 2, 2],
            channel_settings: vec![BitmaskChannelSettings::default()],
            data: NumericData::Uint32(Arc::new(vec![1, 2, 0, 3])),
            data_unit_mode_x: UnitsMode::Pixels,
            ..BitmaskLayerParams::default()
        },
    );
    assert!(pick_at(&layer, 0.5, 0.5).is_none());
}

#[test]
#[should_panic(expected = "data length")]
fn test_constructor_validates_data_length() {
    BitmaskLayer::new(
        ViewParams::default(),
        BitmaskLayerParams {
            dimension_order: DimensionOrder::CYX,
            shape: vec![1, 2, 2],
            channel_settings: vec![BitmaskChannelSettings::default()],
            data: NumericData::Uint32(Arc::new(vec![1, 2, 3])), // wrong length
            ..BitmaskLayerParams::default()
        },
    );
}

#[test]
#[should_panic(expected = "channel_settings length")]
fn test_constructor_validates_channel_settings_count() {
    BitmaskLayer::new(
        ViewParams::default(),
        BitmaskLayerParams {
            dimension_order: DimensionOrder::CYX,
            shape: vec![2, 2, 2],
            channel_settings: vec![BitmaskChannelSettings::default()], // should be 2
            data: NumericData::Uint32(Arc::new(vec![1, 2, 0, 3, 1, 2, 0, 3])),
            ..BitmaskLayerParams::default()
        },
    );
}

/// A 2x2, 2-channel mask (shape [2, 2, 2] in CYX order):
///   channel 0: [1, 2 / 0, 3]  (same as the single-channel tests above)
///   channel 1: [0, 1 / 2, 0]
#[test]
fn test_pick_multi_channel() {
    let layer = BitmaskLayer::new(
        ViewParams::default(),
        BitmaskLayerParams {
            layer_id: "test_bitmask_multi".to_string(),
            dimension_order: DimensionOrder::CYX,
            shape: vec![2, 2, 2],
            channel_settings: vec![BitmaskChannelSettings::default(), BitmaskChannelSettings::default()],
            data: NumericData::Uint32(Arc::new(vec![
                1, 2, 0, 3, // channel 0
                0, 1, 2, 0, // channel 1
            ])),
            ..BitmaskLayerParams::default()
        },
    );

    // Bottom-left quadrant --> array row 1, col 0.
    let r = pick_at(&layer, 0.5, 0.5).unwrap();
    assert_eq!(r.info.get("channel_0").unwrap(), "0");
    assert_eq!(r.info.get("channel_1").unwrap(), "2");

    // Top-right quadrant --> array row 0, col 1.
    let r = pick_at(&layer, 1.5, 1.5).unwrap();
    assert_eq!(r.info.get("channel_0").unwrap(), "2");
    assert_eq!(r.info.get("channel_1").unwrap(), "1");
}
