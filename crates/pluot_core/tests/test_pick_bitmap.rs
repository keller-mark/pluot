use std::sync::Arc;

use pluot_core::layers::bitmap_layer::{
    BitmapLayer, BitmapLayerParams, ChannelSettings, DimensionOrder, NumericData,
};
use pluot_core::render_traits::PickableLayer;
use pluot_core::viewport::{DataCoord, ScreenCoord};
use pluot_core::{LayerPickingResult, UnitsMode, ViewParams};

/// A 2x2 single-channel image with values:
///   array row 0 (top):    [10, 20]
///   array row 1 (bottom): [30, 40]
fn make_layer(
    pixel_offset: Option<(u32, u32)>,
    model_matrix: Option<[f32; 16]>,
) -> BitmapLayer {
    BitmapLayer::new(
        ViewParams::default(),
        BitmapLayerParams {
            layer_id: "test_bitmap".to_string(),
            pixel_offset,
            model_matrix,
            dimension_order: DimensionOrder::CYX,
            shape: vec![1, 2, 2],
            channel_settings: vec![ChannelSettings {
                window: (0.0, 255.0),
                color: (1.0, 1.0, 1.0),
            }],
            data: NumericData::Uint8(Arc::new(vec![10, 20, 30, 40])),
            ..BitmapLayerParams::default()
        },
    )
}

fn pick_at(layer: &BitmapLayer, x: f32, y: f32) -> Option<LayerPickingResult> {
    layer.pick(
        ScreenCoord { x: 0.0, y: 0.0 },
        Some(DataCoord::TwoD { x, y }),
    )
}

#[test]
fn test_pick_identity_matrix() {
    // With no model_matrix, world == pixel: the image spans [0,2]^2 with
    // the quad's bottom edge sampling the last array row.
    let layer = make_layer(None, None);

    // Bottom-left quadrant --> array row 1, col 0 --> 30.
    let r = pick_at(&layer, 0.5, 0.5).unwrap();
    assert_eq!(r.layer_id, "test_bitmap");
    assert_eq!(r.info.get("x").unwrap(), "0");
    assert_eq!(r.info.get("y").unwrap(), "1");
    assert_eq!(r.info.get("channel_0").unwrap(), "30");

    // Top-right quadrant --> array row 0, col 1 --> 20.
    let r = pick_at(&layer, 1.5, 1.5).unwrap();
    assert_eq!(r.info.get("x").unwrap(), "1");
    assert_eq!(r.info.get("y").unwrap(), "0");
    assert_eq!(r.info.get("channel_0").unwrap(), "20");
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
    // world = 2 * pixel: the image spans [0,4]^2.
    let model_matrix = [
        2.0, 0.0, 0.0, 0.0,
        0.0, 2.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        0.0, 0.0, 0.0, 1.0,
    ];
    let layer = make_layer(None, Some(model_matrix));

    // world (3, 1) --> pixel (1.5, 0.5) --> array row 1, col 1 --> 40.
    let r = pick_at(&layer, 3.0, 1.0).unwrap();
    assert_eq!(r.info.get("x").unwrap(), "1");
    assert_eq!(r.info.get("y").unwrap(), "1");
    assert_eq!(r.info.get("channel_0").unwrap(), "40");

    // Inside [0,2]^2 pixel bounds but checked in world space, so still valid;
    // outside the scaled extent is not.
    assert!(pick_at(&layer, 4.5, 1.0).is_none());
}

#[test]
fn test_pick_with_pixel_offset() {
    // pixel_offset shifts the image to world [2,4]^2 (identity matrix).
    let layer = make_layer(Some((2, 2)), None);

    assert!(pick_at(&layer, 0.5, 0.5).is_none());

    // world (2.5, 2.5) --> local pixel (0.5, 0.5) --> array row 1, col 0 --> 30.
    let r = pick_at(&layer, 2.5, 2.5).unwrap();
    assert_eq!(r.info.get("x").unwrap(), "0");
    assert_eq!(r.info.get("y").unwrap(), "1");
    assert_eq!(r.info.get("channel_0").unwrap(), "30");
}

#[test]
fn test_pick_pixels_units_mode_returns_none() {
    let layer = BitmapLayer::new(
        ViewParams::default(),
        BitmapLayerParams {
            layer_id: "test_bitmap".to_string(),
            dimension_order: DimensionOrder::CYX,
            shape: vec![1, 2, 2],
            channel_settings: vec![ChannelSettings {
                window: (0.0, 255.0),
                color: (1.0, 1.0, 1.0),
            }],
            data: NumericData::Uint8(Arc::new(vec![10, 20, 30, 40])),
            data_unit_mode_x: UnitsMode::Pixels,
            ..BitmapLayerParams::default()
        },
    );
    assert!(pick_at(&layer, 0.5, 0.5).is_none());
}
