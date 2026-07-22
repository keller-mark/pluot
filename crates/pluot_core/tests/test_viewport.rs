use pluot_core::viewport::{
    DataCoord, ScreenCoord, DataBounds,
    camera_matrix_to_zoom_and_translation,
    project, unproject,
    get_bounds, get_camera_matrix_from_bounds,
};
use pluot_core::{AspectRatioAlignmentMode};
use pluot_core::render_traits::{AspectRatioMode, MarginParams, ViewParams};

fn make_view_params(
    width: u32,
    height: u32,
    aspect_ratio_mode: AspectRatioMode,
    camera_view: Option<[f32; 16]>,
) -> ViewParams {
    ViewParams {
        view_id: "test".to_string(),
        width,
        height,
        aspect_ratio_mode,
        aspect_ratio_alignment_mode: AspectRatioAlignmentMode::Center,
        device_pixel_ratio: 1.0,
        camera_view,
        timeout: None,
        wait_for_store_gets: true,
        cache_enabled: false,
        margins: None,
        stores: None,
        store_objects: None,
    }
}

fn identity_camera() -> Option<[f32; 16]> {
    Some([
        1.0, 0.0, 0.0, 0.0,
        0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        0.0, 0.0, 0.0, 1.0,
    ])
}

fn zoom_camera(zoom: f32) -> Option<[f32; 16]> {
    Some([
        zoom, 0.0, 0.0, 0.0,
        0.0,  zoom, 0.0, 0.0,
        0.0,  0.0,  1.0, 0.0,
        0.0,  0.0,  0.0, 1.0,
    ])
}

fn zoom_and_translate_camera(zoom: f32, tx: f32, ty: f32) -> Option<[f32; 16]> {
    Some([
        zoom, 0.0,  0.0, 0.0,
        0.0,  zoom, 0.0, 0.0,
        0.0,  0.0,  1.0, 0.0,
        tx,   ty,   0.0, 1.0,
    ])
}

fn assert_data_2d(actual: Option<DataCoord>, expected_x: f32, expected_y: f32) {
    let coord = actual.expect("expected Some(DataCoord), got None");
    match coord {
        DataCoord::TwoD { x, y } => {
            assert_eq!(x, expected_x);
            assert_eq!(y, expected_y);
        }
        _ => panic!("expected TwoD variant"),
    }
}

// camera_matrix_to_zoom_and_translation

#[test]
fn test_camera_matrix_to_zoom_and_translation_none() {
    assert_eq!(camera_matrix_to_zoom_and_translation(None), (1.0, 1.0, 0.0, 0.0));
}

#[test]
fn test_camera_matrix_to_zoom_and_translation_identity() {
    assert_eq!(camera_matrix_to_zoom_and_translation(identity_camera()), (1.0, 1.0, 0.0, 0.0));
}

#[test]
fn test_camera_matrix_to_zoom_and_translation_zoomed_in_2x() {
    assert_eq!(camera_matrix_to_zoom_and_translation(zoom_camera(2.0)), (2.0, 2.0, 0.0, 0.0));
}

#[test]
fn test_camera_matrix_to_zoom_and_translation_with_translation() {
    let camera_view = Some([
        1.0, 0.0, 0.0, 0.0,
        0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        0.5, -0.3, 0.0, 1.0,
    ]);
    assert_eq!(camera_matrix_to_zoom_and_translation(camera_view), (1.0, 1.0, 0.5, -0.3));
}

// project

// The "base" / easiest case: square aspect ratio, ignore mode, identity camera, zero margins.
#[test]
fn test_project_square_ignore_identity_camera() {
    let view_params = make_view_params(100, 100, AspectRatioMode::Ignore, identity_camera());

    let s0 = project(&view_params, None, DataCoord::TwoD { x: 0.0, y: 0.0 });
    assert_eq!((s0.x, s0.y), (0.0, 0.0));

    let s1 = project(&view_params, None, DataCoord::TwoD { x: 0.5, y: 0.5 });
    assert_eq!((s1.x, s1.y), (50.0, 50.0));

    let s2 = project(&view_params, None, DataCoord::TwoD { x: 1.0, y: 1.0 });
    assert_eq!((s2.x, s2.y), (100.0, 100.0));
}

// Wide viewport with ignore mode. Data stretches to fill width.
#[test]
fn test_project_wide_ignore_identity_camera() {
    let view_params = make_view_params(200, 100, AspectRatioMode::Ignore, identity_camera());

    let s0 = project(&view_params, None, DataCoord::TwoD { x: 0.0, y: 0.0 });
    assert_eq!((s0.x, s0.y), (0.0, 0.0));

    let s1 = project(&view_params, None, DataCoord::TwoD { x: 0.5, y: 0.5 });
    assert_eq!((s1.x, s1.y), (100.0, 50.0));

    let s2 = project(&view_params, None, DataCoord::TwoD { x: 1.0, y: 1.0 });
    assert_eq!((s2.x, s2.y), (200.0, 100.0));
}

// Wide viewport with contain mode. Data is centered horizontally (pixel range [50, 150]).
#[test]
fn test_project_wide_contain_identity_camera() {
    let view_params = make_view_params(200, 100, AspectRatioMode::Contain, identity_camera());

    let s0 = project(&view_params, None, DataCoord::TwoD { x: 0.0, y: 0.0 });
    assert_eq!((s0.x, s0.y), (50.0, 0.0));

    let s1 = project(&view_params, None, DataCoord::TwoD { x: 0.5, y: 0.5 });
    assert_eq!((s1.x, s1.y), (100.0, 50.0));

    let s2 = project(&view_params, None, DataCoord::TwoD { x: 1.0, y: 1.0 });
    assert_eq!((s2.x, s2.y), (150.0, 100.0));
}

// Tall viewport with contain mode. Data is centered vertically (pixel range [50, 150]).
#[test]
fn test_project_tall_contain_identity_camera() {
    let view_params = make_view_params(100, 200, AspectRatioMode::Contain, identity_camera());

    let s0 = project(&view_params, None, DataCoord::TwoD { x: 0.0, y: 0.0 });
    assert_eq!((s0.x, s0.y), (0.0, 50.0));

    let s1 = project(&view_params, None, DataCoord::TwoD { x: 0.5, y: 0.5 });
    assert_eq!((s1.x, s1.y), (50.0, 100.0));

    let s2 = project(&view_params, None, DataCoord::TwoD { x: 1.0, y: 1.0 });
    assert_eq!((s2.x, s2.y), (100.0, 150.0));
}

// unproject

#[test]
fn test_unproject_square_ignore_identity_camera() {
    let view_params = make_view_params(100, 100, AspectRatioMode::Ignore, identity_camera());

    assert_data_2d(unproject(&view_params, None, ScreenCoord { x: 0.0, y: 0.0 }), 0.0, 0.0);
    assert_data_2d(unproject(&view_params, None, ScreenCoord { x: 50.0, y: 50.0 }), 0.5, 0.5);
    assert_data_2d(unproject(&view_params, None, ScreenCoord { x: 100.0, y: 100.0 }), 1.0, 1.0);
}

#[test]
fn test_unproject_returns_none_for_out_of_bounds() {
    let view_params = make_view_params(100, 100, AspectRatioMode::Ignore, identity_camera());

    assert!(unproject(&view_params, None, ScreenCoord { x: -1.0,  y: 50.0  }).is_none());
    assert!(unproject(&view_params, None, ScreenCoord { x: 101.0, y: 50.0  }).is_none());
    assert!(unproject(&view_params, None, ScreenCoord { x: 50.0,  y: -1.0  }).is_none());
    assert!(unproject(&view_params, None, ScreenCoord { x: 50.0,  y: 101.0 }).is_none());
}

// Inverse of project with wide contain. Screen corners of data area map back to data corners.
#[test]
fn test_unproject_wide_contain_identity_camera() {
    let view_params = make_view_params(200, 100, AspectRatioMode::Contain, identity_camera());

    assert_data_2d(unproject(&view_params, None, ScreenCoord { x: 50.0,  y: 0.0   }), 0.0, 0.0);
    assert_data_2d(unproject(&view_params, None, ScreenCoord { x: 100.0, y: 50.0  }), 0.5, 0.5);
    assert_data_2d(unproject(&view_params, None, ScreenCoord { x: 150.0, y: 100.0 }), 1.0, 1.0);
}

// With margins: coords in the margin area return None; coords in the layer area unproject correctly.
#[test]
fn test_unproject_with_margins() {
    let margin_bounds = Some(MarginParams {
        margin_left: Some(20.0),
        margin_right: None,
        margin_top: None,
        margin_bottom: Some(20.0),
    });
    let view_params = make_view_params(100, 100, AspectRatioMode::Ignore, identity_camera());

    // x=10 is inside the left margin (< 20px) --> None
    assert!(unproject(&view_params, margin_bounds.clone(), ScreenCoord { x: 10.0, y: 50.0 }).is_none());
    // Bottom-left corner of the layer area maps to data (0, 0)
    assert_data_2d(unproject(&view_params, margin_bounds.clone(), ScreenCoord { x: 20.0,  y: 20.0  }), 0.0, 0.0);
    // Top-right corner of the layer area maps to data (1, 1)
    assert_data_2d(unproject(&view_params, margin_bounds.clone(), ScreenCoord { x: 100.0, y: 100.0 }), 1.0, 1.0);
}

// Round-trip: project a data coord to screen, then unproject back. Should recover the original.
#[test]
fn test_project_unproject_roundtrip_square_ignore() {
    let view_params = make_view_params(100, 100, AspectRatioMode::Ignore, identity_camera());

    for original in [
        DataCoord::TwoD { x: 0.0,  y: 0.0  },
        DataCoord::TwoD { x: 0.5,  y: 0.5  },
        DataCoord::TwoD { x: 1.0,  y: 1.0  },
        DataCoord::TwoD { x: 0.25, y: 0.75 },
    ] {
        let screen = project(&view_params, None, original);
        let DataCoord::TwoD { x: ox, y: oy } = original else { unreachable!() };
        assert_data_2d(unproject(&view_params, None, screen), ox, oy);
    }
}

// get_bounds

// Identity camera, square viewport, ignore mode: full [0, 1] range visible in both axes.
#[test]
fn test_get_bounds_identity_camera_square_ignore() {
    let view_params = make_view_params(100, 100, AspectRatioMode::Ignore, identity_camera());
    let b = get_bounds(&view_params);
    assert_eq!((b.x_min, b.x_max, b.y_min, b.y_max), (0.0, 1.0, 0.0, 1.0));
}

// 2x zoom centers on [0.25, 0.75] in both axes.
#[test]
fn test_get_bounds_zoomed_in_2x_square_ignore() {
    let view_params = make_view_params(100, 100, AspectRatioMode::Ignore, zoom_camera(2.0));
    let b = get_bounds(&view_params);
    assert_eq!((b.x_min, b.x_max, b.y_min, b.y_max), (0.25, 0.75, 0.25, 0.75));
}

// 0.5x zoom (zoomed out 2x) shows a wider range: [-0.5, 1.5] in both axes.
#[test]
fn test_get_bounds_zoomed_out_2x_square_ignore() {
    let view_params = make_view_params(100, 100, AspectRatioMode::Ignore, zoom_camera(0.5));
    let b = get_bounds(&view_params);
    assert_eq!((b.x_min, b.x_max, b.y_min, b.y_max), (-0.5, 1.5, -0.5, 1.5));
}

// Wide contain: x bounds extend to [-0.5, 1.5] to show more data; y stays [0, 1].
#[test]
fn test_get_bounds_wide_contain_identity_camera() {
    let view_params = make_view_params(200, 100, AspectRatioMode::Contain, identity_camera());
    let b = get_bounds(&view_params);
    assert_eq!((b.x_min, b.x_max, b.y_min, b.y_max), (-0.5, 1.5, 0.0, 1.0));
}

// Tall contain: x stays [0, 1]; y shows [0.25, 0.75] (current behavior).
#[test]
fn test_get_bounds_tall_contain_identity_camera() {
    let view_params = make_view_params(100, 200, AspectRatioMode::Contain, identity_camera());
    let b = get_bounds(&view_params);
    assert_eq!((b.x_min, b.x_max, b.y_min, b.y_max), (0.0, 1.0, -0.5, 1.5));
}

// Wide cover: y bounds shrink to [0.25, 0.75] (less data visible); x stays [0, 1].
#[test]
fn test_get_bounds_wide_cover_identity_camera() {
    let view_params = make_view_params(200, 100, AspectRatioMode::Cover, identity_camera());
    let b = get_bounds(&view_params);
    assert_eq!((b.x_min, b.x_max, b.y_min, b.y_max), (0.0, 1.0, 0.25, 0.75));
}

// With margins: layer dimensions shrink but stay square --> same [0, 1] data bounds.
#[test]
fn test_get_bounds_with_margins() {
    let view_params = ViewParams {
        view_id: "test".to_string(),
        width: 100,
        height: 100,
        aspect_ratio_mode: AspectRatioMode::Ignore,
        aspect_ratio_alignment_mode: AspectRatioAlignmentMode::Center,
        device_pixel_ratio: 1.0,
        camera_view: identity_camera(),
        timeout: None,
        wait_for_store_gets: true,
        cache_enabled: false,
        margins: Some(MarginParams {
            margin_left: Some(20.0),
            margin_right: None,
            margin_top: None,
            margin_bottom: Some(20.0),
        }),
        stores: None,
        store_objects: None,
    };
    let b = get_bounds(&view_params);
    assert_eq!((b.x_min, b.x_max, b.y_min, b.y_max), (0.0, 1.0, 0.0, 1.0));
}

// Tests for get_camera_matrix_from_bounds
#[test]
fn test_get_bounds_get_camera_matrix_from_bounds_roundtrip_1() {
    // With margins: layer dimensions shrink but stay square --> same [0, 1] data bounds.
    let view_params = ViewParams {
        view_id: "test".to_string(),
        width: 100,
        height: 100,
        aspect_ratio_mode: AspectRatioMode::Ignore,
        aspect_ratio_alignment_mode: AspectRatioAlignmentMode::Center,
        device_pixel_ratio: 1.0,
        camera_view: identity_camera(),
        timeout: None,
        wait_for_store_gets: true,
        cache_enabled: false,
        margins: Some(MarginParams {
            margin_left: Some(20.0),
            margin_right: None,
            margin_top: None,
            margin_bottom: Some(20.0),
        }),
        stores: None,
        store_objects: None,
    };
    let b = get_bounds(&view_params);
    assert_eq!((b.x_min, b.x_max, b.y_min, b.y_max), (0.0, 1.0, 0.0, 1.0));

    let camera_matrix = get_camera_matrix_from_bounds(&view_params, &b);

    assert_eq!(camera_matrix, identity_camera().unwrap());
}

#[test]
fn test_get_camera_matrix_from_bounds_1() {
    let view_params = make_view_params(
        100, 100, AspectRatioMode::Ignore,
        // Here, we can pass any camera matrix value when constructing ViewParams - it should not matter.
        identity_camera()
    );
    let data_bounds = DataBounds {
        x_min: 0.25,
        x_max: 0.75,
        y_min: 0.25,
        y_max: 0.75
    };
    let camera_matrix = get_camera_matrix_from_bounds(&view_params, &data_bounds);
    assert_eq!(camera_matrix, zoom_camera(2.0).unwrap());
}

// Full [0, 1] range --> identity camera (no zoom, no translation).
#[test]
fn test_get_camera_matrix_from_bounds_identity() {
    let view_params = make_view_params(100, 100, AspectRatioMode::Ignore, identity_camera());
    let data_bounds = DataBounds { x_min: 0.0, x_max: 1.0, y_min: 0.0, y_max: 1.0 };
    let camera_matrix = get_camera_matrix_from_bounds(&view_params, &data_bounds);
    assert_eq!(camera_matrix, identity_camera().unwrap());
}

// [-0.5, 1.5] range in both axes --> 0.5x zoom (zoomed out 2x).
#[test]
fn test_get_camera_matrix_from_bounds_zoomed_out_2x() {
    let view_params = make_view_params(100, 100, AspectRatioMode::Ignore, identity_camera());
    let data_bounds = DataBounds { x_min: -0.5, x_max: 1.5, y_min: -0.5, y_max: 1.5 };
    let camera_matrix = get_camera_matrix_from_bounds(&view_params, &data_bounds);
    assert_eq!(camera_matrix, zoom_camera(0.5).unwrap());
}

// Offset bounds in x only --> zoom=1, translate_x=0.5, translate_y=0.
#[test]
fn test_get_camera_matrix_from_bounds_x_translation_only() {
    let view_params = make_view_params(100, 100, AspectRatioMode::Ignore, identity_camera());
    // These are the bounds produced by get_bounds for translate_x=0.5, zoom=1.
    let data_bounds = DataBounds { x_min: -0.25, x_max: 0.75, y_min: 0.0, y_max: 1.0 };
    let camera_matrix = get_camera_matrix_from_bounds(&view_params, &data_bounds);
    assert_eq!(camera_matrix, zoom_and_translate_camera(1.0, 0.5, 0.0).unwrap());
}

// Bounds from a zoom=2 + translated camera --> camera_matrix recovers zoom and both translations.
// Uses power-of-2 fractions so f32 arithmetic is exact.
#[test]
fn test_get_camera_matrix_from_bounds_zoom_and_translation() {
    let view_params = make_view_params(100, 100, AspectRatioMode::Ignore, identity_camera());
    // Bounds produced by zoom=2.0, tx=0.5, ty=0.25 on a square/ignore viewport.
    let data_bounds = DataBounds {
        x_min: 0.125, x_max: 0.625,
        y_min: 0.1875, y_max: 0.6875,
    };
    let camera_matrix = get_camera_matrix_from_bounds(&view_params, &data_bounds);
    assert_eq!(camera_matrix, zoom_and_translate_camera(2.0, 0.5, 0.25).unwrap());
}

// Wide contain (2:1 viewport): bounds [-0.5, 1.5] x [0, 1] --> identity camera.
#[test]
fn test_get_camera_matrix_from_bounds_wide_contain() {
    let view_params = make_view_params(200, 100, AspectRatioMode::Contain, identity_camera());
    // These are the bounds returned by get_bounds for a 2:1 contain viewport with identity camera.
    let data_bounds = DataBounds { x_min: -0.5, x_max: 1.5, y_min: 0.0, y_max: 1.0 };
    let camera_matrix = get_camera_matrix_from_bounds(&view_params, &data_bounds);
    assert_eq!(camera_matrix, identity_camera().unwrap());
}

// Tall contain (1:2 viewport): bounds [0, 1] x [-0.5, 1.5] --> identity camera.
#[test]
fn test_get_camera_matrix_from_bounds_tall_contain() {
    let view_params = make_view_params(100, 200, AspectRatioMode::Contain, identity_camera());
    let data_bounds = DataBounds { x_min: 0.0, x_max: 1.0, y_min: -0.5, y_max: 1.5 };
    let camera_matrix = get_camera_matrix_from_bounds(&view_params, &data_bounds);
    assert_eq!(camera_matrix, identity_camera().unwrap());
}

// Wide cover (2:1 viewport): bounds [0, 1] x [0.25, 0.75] --> identity camera.
#[test]
fn test_get_camera_matrix_from_bounds_wide_cover() {
    let view_params = make_view_params(200, 100, AspectRatioMode::Cover, identity_camera());
    // These are the bounds returned by get_bounds for a 2:1 cover viewport with identity camera.
    let data_bounds = DataBounds { x_min: 0.0, x_max: 1.0, y_min: 0.25, y_max: 0.75 };
    let camera_matrix = get_camera_matrix_from_bounds(&view_params, &data_bounds);
    assert_eq!(camera_matrix, identity_camera().unwrap());
}

// Under Ignore mode, x and y zoom independently. No min-zoom constraint.
#[test]
fn test_get_camera_matrix_from_bounds_asymmetric_ranges_independent_zoom() {
    let view_params = make_view_params(100, 100, AspectRatioMode::Ignore, identity_camera());
    // x spans [0, 0.5] (zoom_x=2.0), y spans [0, 1.0] (zoom_y=1.0).
    // Each axis is zoomed independently: zoom_x=2, translate_x=1.0; zoom_y=1, translate_y=0.
    let data_bounds = DataBounds { x_min: 0.0, x_max: 0.5, y_min: 0.0, y_max: 1.0 };
    let camera_matrix = get_camera_matrix_from_bounds(&view_params, &data_bounds);
    assert_eq!(camera_matrix, [
        2.0, 0.0, 0.0, 0.0,
        0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        1.0, 0.0, 0.0, 1.0,
    ]);
}

// Roundtrip: get_bounds(zoom_camera(2.0)) --> get_camera_matrix_from_bounds --> zoom_camera(2.0).
#[test]
fn test_get_bounds_get_camera_matrix_from_bounds_roundtrip_zoomed_in() {
    let view_params = make_view_params(100, 100, AspectRatioMode::Ignore, zoom_camera(2.0));
    let b = get_bounds(&view_params);
    assert_eq!((b.x_min, b.x_max, b.y_min, b.y_max), (0.25, 0.75, 0.25, 0.75));
    let camera_matrix = get_camera_matrix_from_bounds(&view_params, &b);
    assert_eq!(camera_matrix, zoom_camera(2.0).unwrap());
}

// Roundtrip: wide contain viewport, identity camera --> get_bounds --> get_camera_matrix_from_bounds --> identity.
#[test]
fn test_get_bounds_get_camera_matrix_from_bounds_roundtrip_wide_contain() {
    let view_params = make_view_params(200, 100, AspectRatioMode::Contain, identity_camera());
    let b = get_bounds(&view_params);
    assert_eq!((b.x_min, b.x_max, b.y_min, b.y_max), (-0.5, 1.5, 0.0, 1.0));
    let camera_matrix = get_camera_matrix_from_bounds(&view_params, &b);
    assert_eq!(camera_matrix, identity_camera().unwrap());
}

// Roundtrip: square ignore, zoom=2 + translation --> get_bounds --> get_camera_matrix_from_bounds --> same camera.
// Uses power-of-2 fractions (tx=0.5, ty=0.25) so f32 arithmetic is exact throughout.
#[test]
fn test_get_bounds_get_camera_matrix_from_bounds_roundtrip_zoom_and_translation() {
    let original_camera = zoom_and_translate_camera(2.0, 0.5, 0.25);
    let view_params = make_view_params(100, 100, AspectRatioMode::Ignore, original_camera);
    let b = get_bounds(&view_params);
    let camera_matrix = get_camera_matrix_from_bounds(&view_params, &b);
    assert_eq!(camera_matrix, original_camera.unwrap());
}
