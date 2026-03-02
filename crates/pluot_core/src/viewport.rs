// Viewport utilities intended for use by layers, to implement features like picking.
// We may also want to expose functions like project, unproject, and get_bounds via the public API and bindings.
use nalgebra_glm::{Vec2, Vec4, Mat4};
use crate::render_traits::{MarginParams, ViewParams, AspectRatioMode, UnitsMode};
use crate::positioning::{get_point_position, get_scale_mat, get_translate_mat, get_aspect_ratio_mat};

#[derive(Debug, Clone, Copy)]
pub struct ScreenCoord {
    pub x: f32,
    // Note: we treat the Y coordinate as increasing upwards, for consistency with the data coordinate system.
    // Conversion to a coordinate system where Y increases downwards (e.g., for HTML canvas) is delegated to the caller.
    pub y: f32,
}

#[derive(Debug, Clone, Copy)]
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
        view_params.aspect_ratio_mode,
        0,
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

    let margin_top = bounds.as_ref().and_then(|m| m.margin_top).unwrap_or(0.0) as f32;
    let margin_left = bounds.as_ref().and_then(|m| m.margin_left).unwrap_or(0.0) as f32;
    let margin_right = bounds.as_ref().and_then(|m| m.margin_right).unwrap_or(0.0) as f32;
    let margin_bottom = bounds.as_ref().and_then(|m| m.margin_bottom).unwrap_or(0.0) as f32;

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
        view_params.aspect_ratio_mode
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
// TODO: refactor layers that currently implement a self.get_visible_range to instead use this get_bounds function.
pub fn get_bounds(view_params: &ViewParams) -> DataBounds {
    let (zoom, translate_x, translate_y) = camera_matrix_to_zoom_and_translation(view_params.camera_view);

    let aspect_ratio_mode = view_params.aspect_ratio_mode;

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
                y_scale_for_aspect_ratio_mode = layer_aspect_ratio;
            }
        }
        AspectRatioMode::Cover => {
            if layer_aspect_ratio > 1.0 {
                y_scale_for_aspect_ratio_mode = 1.0 / layer_aspect_ratio;
            } else if layer_aspect_ratio < 1.0 {
                x_scale_for_aspect_ratio_mode = 1.0 / layer_aspect_ratio;
            }
        }
    }

    // TODO: handle aspect ratio alignment mode

    let x_adjustment = x_scale_for_aspect_ratio_mode - 1.0;
    let y_adjustment = y_scale_for_aspect_ratio_mode - 1.0;

    let min_x = (((-translate_x - 1.0 - x_adjustment) / zoom) + 1.0) / 2.0;
    let max_x = (((-translate_x + 1.0 + x_adjustment) / zoom) + 1.0) / 2.0;
    let min_y = (((-translate_y - 1.0 - y_adjustment) / zoom) + 1.0) / 2.0;
    let max_y = (((-translate_y + 1.0 + y_adjustment) / zoom) + 1.0) / 2.0;

    DataBounds {
        x_min: min_x as f32,
        x_max: max_x as f32,
        y_min: min_y as f32,
        y_max: max_y as f32,
    }
}
