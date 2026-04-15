// Viewport utilities intended for use by layers, to implement features like picking.
// We may also want to expose functions like project, unproject, and get_bounds via the public API and bindings.
use nalgebra_glm::{Vec2, Vec4, Mat4};
use serde::{Deserialize, Serialize};
use crate::render_traits::{MarginParams, ViewParams, AspectRatioMode, AspectRatioAlignmentMode, UnitsMode};
use crate::positioning::{get_point_position, get_scale_mat, get_translate_mat, get_aspect_ratio_mat};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ScreenCoord {
    pub x: f32,
    // Note: we treat the Y coordinate as increasing upwards, for consistency with the data coordinate system.
    // Conversion to a coordinate system where Y increases downwards (e.g., for HTML canvas) is delegated to the caller.
    pub y: f32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum DataCoord {
    TwoD { x: f32, y: f32 },
    ThreeD { x: f32, y: f32, z: f32 },
}

#[derive(Debug, Clone, Copy)]
pub struct DataBounds {
    pub x_min: f32,
    pub x_max: f32,
    pub y_min: f32,
    pub y_max: f32,
    // TODO: add things for 3D?
}


// Projects data-space coordinates to pixel coordinates on screen.
// Reference: https://deck.gl/docs/api-reference/core/viewport
pub fn project(view_params: &ViewParams, layer_bounds: Option<MarginParams>, coord: DataCoord) -> ScreenCoord {
    // TODO: accept a model_matrix parameter for data transformation?
    let (pos_x, pos_y) = match coord {
        DataCoord::TwoD { x, y } => (x, y),
        DataCoord::ThreeD { x, y, .. } => {
            panic!("3D coordinates not supported in project function yet");
        }
    };

    // TODO: reduce code reuse here
    let camera_view = view_params.camera_view.unwrap_or([
        // Column 0
        1.0, 0.0, 0.0, 0.0, // Column 1
        0.0, 1.0, 0.0, 0.0, // Column 2
        0.0, 0.0, 1.0, 0.0, // Column 3
        0.0, 0.0, 0.0, 1.0,
    ]);

    // Use layer-specific bounds if not None, otherwise use the view's margins
    // (which may also be None).
    let bounds = if layer_bounds.is_none() {
        &view_params.margins
    } else {
        &layer_bounds
    };

    let (x_px, y_px) = get_point_position(
        pos_x,
        pos_y,
        view_params.width as f32,
        view_params.height as f32,
        &camera_view,
        UnitsMode::Data,
        UnitsMode::Data,
        view_params.aspect_ratio_mode,
        view_params.aspect_ratio_alignment_mode,
        None,
    );

    // TODO: translate to account for margins/layer_bounds.
    // get_point_position does not currently take margins into account,
    // because we rely on TwoGroup.translate to translate by margin_left, margin_top.

    // TODO: handle clipping.
    // Either return None,
    // or return an "OffscreenCoord" variant of ScreenCoord which specifies
    // the nearest ScreenCoord
    // and potentially also the negative-pixel-unit distance from the edge of the screen to the point
    // (e.g., right: -10px).

    return ScreenCoord {
        x: x_px,
        y: y_px,
    };
}


// Unproject pixel coordinates on screen into data-space coordinates.
// Reference: https://deck.gl/docs/api-reference/core/viewport
pub fn unproject(view_params: &ViewParams, layer_bounds: Option<MarginParams>, coord: ScreenCoord) -> Option<DataCoord> {
    // TODO: accept a model_matrix parameter for data transformation?
    let camera_view_raw = view_params.camera_view.unwrap_or([
        1.0, 0.0, 0.0, 0.0,
        0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        0.0, 0.0, 0.0, 1.0,
    ]);
    let camera_view = Mat4::from_column_slice(&camera_view_raw);

    // Return None if the ScreenCoord is in the margins.
    // TODO: allow None for the individual x/y and instead return a single-dimensional coordinate if the other dimension is in the margins?

    // Subtract margins from the screen coordinates to get the coordinates relative to the layer.
    let bounds = if layer_bounds.is_none() {
        &view_params.margins
    } else {
        &layer_bounds
    };

    let margin_top = bounds.as_ref().and_then(|m| m.margin_top).unwrap_or(0.0);
    let margin_left = bounds.as_ref().and_then(|m| m.margin_left).unwrap_or(0.0);
    let margin_right = bounds.as_ref().and_then(|m| m.margin_right).unwrap_or(0.0);
    let margin_bottom = bounds.as_ref().and_then(|m| m.margin_bottom).unwrap_or(0.0);

    let layer_screen_coord = ScreenCoord {
        x: coord.x - margin_left,
        // Assume the Y coordinate provided is increasing upwards, for consistency with the data coordinate system.
        y: coord.y - margin_bottom,
    };

    let layer_w = view_params.width as f32 - margin_left - margin_right;
    let layer_h = view_params.height as f32 - margin_top - margin_bottom;

    if layer_screen_coord.x < 0.0 || layer_screen_coord.x > layer_w || layer_screen_coord.y < 0.0 || layer_screen_coord.y > layer_h {
        return None;
    }

    // Obtain normalized coordinates.
    let norm_x = layer_screen_coord.x / layer_w;
    let norm_y = layer_screen_coord.y / layer_h;

    let layer_aspect_ratio = layer_w / layer_h;

    // Get the same matrices used in get_point_position for the forward projection, so that we can invert them to unproject.
    let ASPECT_RATIO_MAT = get_aspect_ratio_mat(
        layer_aspect_ratio,
        view_params.aspect_ratio_mode,
        view_params.aspect_ratio_alignment_mode
    );

    let NORM_TO_NDC_MAT = get_translate_mat(-1.0, -1.0, 0.0) * get_scale_mat(2.0, 2.0, 1.0);
    let NDC_TO_NORM_MAT =  get_translate_mat(0.5, 0.5, 0.0) * get_scale_mat(0.5, 0.5, 1.0);

    let model_view_projection = ASPECT_RATIO_MAT * camera_view;

    // TODO: incorporate model_matrix here
    let forward_mat = (NDC_TO_NORM_MAT * model_view_projection * NORM_TO_NDC_MAT);
    let inverse_mat = forward_mat.try_inverse().expect("Forward projection matrix is not invertible");

    let data_pos = inverse_mat * Vec4::new(norm_x, norm_y, 0.0, 1.0);

    return Some(DataCoord::TwoD {
        x: data_pos.x,
        y: data_pos.y
    });
}


/// Extract zoom and translation from the camera_view matrix.
pub fn camera_matrix_to_zoom_and_translation(camera_view: Option<[f32; 16]>) -> (f32, f32, f32) {
    let camera_view = camera_view.unwrap_or([
        1.0, 0.0, 0.0, 0.0,
        0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        0.0, 0.0, 0.0, 1.0,
    ]);
    let zoom = camera_view[0];
    let translate_x = camera_view[12];
    let translate_y = camera_view[13];
    (zoom, translate_x, translate_y)
}

// Calculate the visible data range based on camera view and other view parameters.
pub fn get_bounds(view_params: &ViewParams) -> DataBounds {
    let (zoom, translate_x, translate_y) = camera_matrix_to_zoom_and_translation(view_params.camera_view);

    let aspect_ratio_mode = view_params.aspect_ratio_mode;
    let aspect_ratio_alignment_mode = view_params.aspect_ratio_alignment_mode;

    let bounds = &view_params.margins;

    let margin_top = bounds.as_ref().and_then(|m| m.margin_top).unwrap_or(0.0) as f64;
    let margin_right = bounds.as_ref().and_then(|m| m.margin_right).unwrap_or(0.0) as f64;
    let margin_bottom = bounds.as_ref().and_then(|m| m.margin_bottom).unwrap_or(0.0) as f64;
    let margin_left = bounds.as_ref().and_then(|m| m.margin_left).unwrap_or(0.0) as f64;

    let viewport_w = view_params.width as f32;
    let viewport_h = view_params.height as f32;

    let layer_w = viewport_w - (margin_left + margin_right) as f32;
    let layer_h = viewport_h - (margin_top + margin_bottom) as f32;

    let layer_aspect_ratio = layer_w / layer_h;

    let mut x_scale_for_aspect_ratio_mode = 1.0_f32;
    let mut y_scale_for_aspect_ratio_mode = 1.0_f32;
    match aspect_ratio_mode {
        AspectRatioMode::Ignore => {}
        AspectRatioMode::Contain => {
            if layer_aspect_ratio > 1.0 {
                x_scale_for_aspect_ratio_mode = layer_aspect_ratio;
            } else if layer_aspect_ratio < 1.0 {
                y_scale_for_aspect_ratio_mode = 1.0 / layer_aspect_ratio;
            }
        }
        AspectRatioMode::Cover => {
            if layer_aspect_ratio > 1.0 {
                y_scale_for_aspect_ratio_mode = 1.0 / layer_aspect_ratio;
            } else if layer_aspect_ratio < 1.0 {
                x_scale_for_aspect_ratio_mode = layer_aspect_ratio;
            }
        }
    }

    // Handle aspect ratio alignment mode
    let mut x_translation_for_aspect_ratio_alignment_mode = 0.0_f32;
    let mut y_translation_for_aspect_ratio_alignment_mode = 0.0_f32;
    match aspect_ratio_alignment_mode {
        AspectRatioAlignmentMode::Center => {}
        AspectRatioAlignmentMode::Start => {
            x_translation_for_aspect_ratio_alignment_mode = x_scale_for_aspect_ratio_mode - 1.0;
            y_translation_for_aspect_ratio_alignment_mode = y_scale_for_aspect_ratio_mode - 1.0;
        }
        AspectRatioAlignmentMode::End => {
            x_translation_for_aspect_ratio_alignment_mode = 1.0 - x_scale_for_aspect_ratio_mode;
            y_translation_for_aspect_ratio_alignment_mode = 1.0 - y_scale_for_aspect_ratio_mode;
        }
    }

    let x_adjustment = x_scale_for_aspect_ratio_mode - 1.0;
    let y_adjustment = y_scale_for_aspect_ratio_mode - 1.0;

    let min_x = (((-translate_x - 1.0 - x_adjustment + x_translation_for_aspect_ratio_alignment_mode) / zoom) + 1.0) / 2.0;
    let max_x = (((-translate_x + 1.0 + x_adjustment + x_translation_for_aspect_ratio_alignment_mode) / zoom) + 1.0) / 2.0;
    let min_y = (((-translate_y - 1.0 - y_adjustment + y_translation_for_aspect_ratio_alignment_mode) / zoom) + 1.0) / 2.0;
    let max_y = (((-translate_y + 1.0 + y_adjustment + y_translation_for_aspect_ratio_alignment_mode) / zoom) + 1.0) / 2.0;

    DataBounds {
        x_min: min_x,
        x_max: max_x,
        y_min: min_y,
        y_max: max_y,
    }
}

// Given x_min/x_max and y_min/y_max values, compute the corresponding camera matrix that would show data in this range.
pub fn get_camera_matrix_from_bounds(view_params: &ViewParams, data_bounds: &DataBounds) -> [f32; 16] {
    let aspect_ratio_mode = view_params.aspect_ratio_mode;
    let aspect_ratio_alignment_mode = view_params.aspect_ratio_alignment_mode;

    let bounds = &view_params.margins;

    let margin_top = bounds.as_ref().and_then(|m| m.margin_top).unwrap_or(0.0);
    let margin_right = bounds.as_ref().and_then(|m| m.margin_right).unwrap_or(0.0);
    let margin_bottom = bounds.as_ref().and_then(|m| m.margin_bottom).unwrap_or(0.0);
    let margin_left = bounds.as_ref().and_then(|m| m.margin_left).unwrap_or(0.0);

    let viewport_w = view_params.width as f32;
    let viewport_h = view_params.height as f32;

    let layer_w = viewport_w - margin_left - margin_right;
    let layer_h = viewport_h - margin_top - margin_bottom;

    let layer_aspect_ratio = layer_w / layer_h;

    let mut x_scale_for_aspect_ratio_mode = 1.0_f32;
    let mut y_scale_for_aspect_ratio_mode = 1.0_f32;
    match aspect_ratio_mode {
        AspectRatioMode::Ignore => {}
        AspectRatioMode::Contain => {
            if layer_aspect_ratio > 1.0 {
                x_scale_for_aspect_ratio_mode = layer_aspect_ratio;
            } else if layer_aspect_ratio < 1.0 {
                y_scale_for_aspect_ratio_mode = 1.0 / layer_aspect_ratio;
            }
        }
        AspectRatioMode::Cover => {
            if layer_aspect_ratio > 1.0 {
                y_scale_for_aspect_ratio_mode = 1.0 / layer_aspect_ratio;
            } else if layer_aspect_ratio < 1.0 {
                x_scale_for_aspect_ratio_mode = layer_aspect_ratio;
            }
        }
    }

    let mut x_translation_for_aspect_ratio_alignment_mode = 0.0_f32;
    let mut y_translation_for_aspect_ratio_alignment_mode = 0.0_f32;
    match aspect_ratio_alignment_mode {
        AspectRatioAlignmentMode::Center => {}
        AspectRatioAlignmentMode::Start => {
            x_translation_for_aspect_ratio_alignment_mode = x_scale_for_aspect_ratio_mode - 1.0;
            y_translation_for_aspect_ratio_alignment_mode = y_scale_for_aspect_ratio_mode - 1.0;
        }
        AspectRatioAlignmentMode::End => {
            x_translation_for_aspect_ratio_alignment_mode = 1.0 - x_scale_for_aspect_ratio_mode;
            y_translation_for_aspect_ratio_alignment_mode = 1.0 - y_scale_for_aspect_ratio_mode;
        }
    }

    let x_adjustment = x_scale_for_aspect_ratio_mode - 1.0;
    let y_adjustment = y_scale_for_aspect_ratio_mode - 1.0;

    let x_range = data_bounds.x_max - data_bounds.x_min;
    let y_range = data_bounds.y_max - data_bounds.y_min;

    // Derive zoom from both axes; take the minimum to ensure all requested data fits.
    // For consistent bounds (i.e., produced by get_bounds), zoom_x == zoom_y.
    let zoom_x = (1.0 + x_adjustment) / x_range;
    let zoom_y = (1.0 + y_adjustment) / y_range;
    let zoom = zoom_x.min(zoom_y);

    // Invert the get_bounds translation equations:
    //   min + max = (-translate + align) / zoom + 1.0
    // So: translate = align - zoom * ((min + max) - 1.0)
    let translate_x = x_translation_for_aspect_ratio_alignment_mode - zoom * ((data_bounds.x_min + data_bounds.x_max) - 1.0);
    let translate_y = y_translation_for_aspect_ratio_alignment_mode - zoom * ((data_bounds.y_min + data_bounds.y_max) - 1.0);

    [
        zoom, 0.0,  0.0, 0.0,
        0.0,  zoom, 0.0, 0.0,
        0.0,  0.0,  1.0, 0.0,
        translate_x, translate_y, 0.0, 1.0,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AspectRatioAlignmentMode, render_traits::{AspectRatioMode, MarginParams, ViewParams}};

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
            store_name: None,
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

    // =================== camera_matrix_to_zoom_and_translation ===================

    #[test]
    fn test_camera_matrix_to_zoom_and_translation_none() {
        assert_eq!(camera_matrix_to_zoom_and_translation(None), (1.0, 0.0, 0.0));
    }

    #[test]
    fn test_camera_matrix_to_zoom_and_translation_identity() {
        assert_eq!(camera_matrix_to_zoom_and_translation(identity_camera()), (1.0, 0.0, 0.0));
    }

    #[test]
    fn test_camera_matrix_to_zoom_and_translation_zoomed_in_2x() {
        assert_eq!(camera_matrix_to_zoom_and_translation(zoom_camera(2.0)), (2.0, 0.0, 0.0));
    }

    #[test]
    fn test_camera_matrix_to_zoom_and_translation_with_translation() {
        let camera_view = Some([
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.5, -0.3, 0.0, 1.0,
        ]);
        assert_eq!(camera_matrix_to_zoom_and_translation(camera_view), (1.0, 0.5, -0.3));
    }

    // =================== project ===================

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

    // Wide viewport with ignore mode — data stretches to fill width.
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

    // Wide viewport with contain mode — data is centered horizontally (pixel range [50, 150]).
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

    // Tall viewport with contain mode — data is centered vertically (pixel range [50, 150]).
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

    // =================== unproject ===================

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

    // Inverse of project with wide contain — screen corners of data area map back to data corners.
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

        // x=10 is inside the left margin (< 20px) → None
        assert!(unproject(&view_params, margin_bounds.clone(), ScreenCoord { x: 10.0, y: 50.0 }).is_none());
        // Bottom-left corner of the layer area maps to data (0, 0)
        assert_data_2d(unproject(&view_params, margin_bounds.clone(), ScreenCoord { x: 20.0,  y: 20.0  }), 0.0, 0.0);
        // Top-right corner of the layer area maps to data (1, 1)
        assert_data_2d(unproject(&view_params, margin_bounds.clone(), ScreenCoord { x: 100.0, y: 100.0 }), 1.0, 1.0);
    }

    // Round-trip: project a data coord to screen, then unproject back — should recover the original.
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

    // =================== get_bounds ===================

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

    // With margins: layer dimensions shrink but stay square → same [0, 1] data bounds.
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
            store_name: None,
        };
        let b = get_bounds(&view_params);
        assert_eq!((b.x_min, b.x_max, b.y_min, b.y_max), (0.0, 1.0, 0.0, 1.0));
    }

    // Tests for get_camera_matrix_from_bounds
    #[test]
    fn test_get_bounds_get_camera_matrix_from_bounds_roundtrip_1() {
        // With margins: layer dimensions shrink but stay square → same [0, 1] data bounds.
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
            store_name: None,
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

    // Full [0, 1] range → identity camera (no zoom, no translation).
    #[test]
    fn test_get_camera_matrix_from_bounds_identity() {
        let view_params = make_view_params(100, 100, AspectRatioMode::Ignore, identity_camera());
        let data_bounds = DataBounds { x_min: 0.0, x_max: 1.0, y_min: 0.0, y_max: 1.0 };
        let camera_matrix = get_camera_matrix_from_bounds(&view_params, &data_bounds);
        assert_eq!(camera_matrix, identity_camera().unwrap());
    }

    // [-0.5, 1.5] range in both axes → 0.5× zoom (zoomed out 2×).
    #[test]
    fn test_get_camera_matrix_from_bounds_zoomed_out_2x() {
        let view_params = make_view_params(100, 100, AspectRatioMode::Ignore, identity_camera());
        let data_bounds = DataBounds { x_min: -0.5, x_max: 1.5, y_min: -0.5, y_max: 1.5 };
        let camera_matrix = get_camera_matrix_from_bounds(&view_params, &data_bounds);
        assert_eq!(camera_matrix, zoom_camera(0.5).unwrap());
    }

    // Offset bounds in x only → zoom=1, translate_x=0.5, translate_y=0.
    #[test]
    fn test_get_camera_matrix_from_bounds_x_translation_only() {
        let view_params = make_view_params(100, 100, AspectRatioMode::Ignore, identity_camera());
        // These are the bounds produced by get_bounds for translate_x=0.5, zoom=1.
        let data_bounds = DataBounds { x_min: -0.25, x_max: 0.75, y_min: 0.0, y_max: 1.0 };
        let camera_matrix = get_camera_matrix_from_bounds(&view_params, &data_bounds);
        assert_eq!(camera_matrix, zoom_and_translate_camera(1.0, 0.5, 0.0).unwrap());
    }

    // Bounds from a zoom=2 + translated camera → camera_matrix recovers zoom and both translations.
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

    // Wide contain (2:1 viewport): bounds [-0.5, 1.5] × [0, 1] → identity camera.
    #[test]
    fn test_get_camera_matrix_from_bounds_wide_contain() {
        let view_params = make_view_params(200, 100, AspectRatioMode::Contain, identity_camera());
        // These are the bounds returned by get_bounds for a 2:1 contain viewport with identity camera.
        let data_bounds = DataBounds { x_min: -0.5, x_max: 1.5, y_min: 0.0, y_max: 1.0 };
        let camera_matrix = get_camera_matrix_from_bounds(&view_params, &data_bounds);
        assert_eq!(camera_matrix, identity_camera().unwrap());
    }

    // Tall contain (1:2 viewport): bounds [0, 1] × [-0.5, 1.5] → identity camera.
    #[test]
    fn test_get_camera_matrix_from_bounds_tall_contain() {
        let view_params = make_view_params(100, 200, AspectRatioMode::Contain, identity_camera());
        let data_bounds = DataBounds { x_min: 0.0, x_max: 1.0, y_min: -0.5, y_max: 1.5 };
        let camera_matrix = get_camera_matrix_from_bounds(&view_params, &data_bounds);
        assert_eq!(camera_matrix, identity_camera().unwrap());
    }

    // Wide cover (2:1 viewport): bounds [0, 1] × [0.25, 0.75] → identity camera.
    #[test]
    fn test_get_camera_matrix_from_bounds_wide_cover() {
        let view_params = make_view_params(200, 100, AspectRatioMode::Cover, identity_camera());
        // These are the bounds returned by get_bounds for a 2:1 cover viewport with identity camera.
        let data_bounds = DataBounds { x_min: 0.0, x_max: 1.0, y_min: 0.25, y_max: 0.75 };
        let camera_matrix = get_camera_matrix_from_bounds(&view_params, &data_bounds);
        assert_eq!(camera_matrix, identity_camera().unwrap());
    }

    // When x_range < y_range the minimum zoom is chosen so all data fits; x is not over-zoomed.
    #[test]
    fn test_get_camera_matrix_from_bounds_asymmetric_ranges_takes_min_zoom() {
        let view_params = make_view_params(100, 100, AspectRatioMode::Ignore, identity_camera());
        // x spans [0, 0.5] (range=0.5, zoom_x=2.0) but y spans [0, 1.0] (range=1.0, zoom_y=1.0).
        // min zoom = 1.0 (constrained by y), with translation to center x.
        let data_bounds = DataBounds { x_min: 0.0, x_max: 0.5, y_min: 0.0, y_max: 1.0 };
        let camera_matrix = get_camera_matrix_from_bounds(&view_params, &data_bounds);
        assert_eq!(camera_matrix, zoom_and_translate_camera(1.0, 0.5, 0.0).unwrap());
    }

    // Roundtrip: get_bounds(zoom_camera(2.0)) → get_camera_matrix_from_bounds → zoom_camera(2.0).
    #[test]
    fn test_get_bounds_get_camera_matrix_from_bounds_roundtrip_zoomed_in() {
        let view_params = make_view_params(100, 100, AspectRatioMode::Ignore, zoom_camera(2.0));
        let b = get_bounds(&view_params);
        assert_eq!((b.x_min, b.x_max, b.y_min, b.y_max), (0.25, 0.75, 0.25, 0.75));
        let camera_matrix = get_camera_matrix_from_bounds(&view_params, &b);
        assert_eq!(camera_matrix, zoom_camera(2.0).unwrap());
    }

    // Roundtrip: wide contain viewport, identity camera → get_bounds → get_camera_matrix_from_bounds → identity.
    #[test]
    fn test_get_bounds_get_camera_matrix_from_bounds_roundtrip_wide_contain() {
        let view_params = make_view_params(200, 100, AspectRatioMode::Contain, identity_camera());
        let b = get_bounds(&view_params);
        assert_eq!((b.x_min, b.x_max, b.y_min, b.y_max), (-0.5, 1.5, 0.0, 1.0));
        let camera_matrix = get_camera_matrix_from_bounds(&view_params, &b);
        assert_eq!(camera_matrix, identity_camera().unwrap());
    }

    // Roundtrip: square ignore, zoom=2 + translation → get_bounds → get_camera_matrix_from_bounds → same camera.
    // Uses power-of-2 fractions (tx=0.5, ty=0.25) so f32 arithmetic is exact throughout.
    #[test]
    fn test_get_bounds_get_camera_matrix_from_bounds_roundtrip_zoom_and_translation() {
        let original_camera = zoom_and_translate_camera(2.0, 0.5, 0.25);
        let view_params = make_view_params(100, 100, AspectRatioMode::Ignore, original_camera);
        let b = get_bounds(&view_params);
        let camera_matrix = get_camera_matrix_from_bounds(&view_params, &b);
        assert_eq!(camera_matrix, original_camera.unwrap());
    }
}
