// Viewport utilities intended for use by layers, to implement features like picking.
// We may also want to expose functions like project, unproject, and get_bounds via the public API and bindings.

use crate::render_traits::{MarginParams, ViewParams, AspectRatioMode};

pub struct ScreenCoord {
    pub x: f32,
    pub y: f32,
}

pub enum DataCoord {
    TwoD { x: f32, y: f32 },
    ThreeD { x: f32, y: f32, z: f32 },
}

pub struct DataBounds {
    pub x_min: f32,
    pub x_max: f32,
    pub y_min: f32,
    pub y_max: f32,
    // TODO: add things for 3D?
}


// Projects data-space coordinates to pixel coordinates on screen.
// Reference: https://deck.gl/docs/api-reference/core/viewport
pub async fn project(view_params: &ViewParams, layer_margin: Option<MarginParams>, coord: DataCoord) -> ScreenCoord {
    // TODO: implement
}


// Unproject pixel coordinates on screen into data-space coordinates.
// Reference: https://deck.gl/docs/api-reference/core/viewport
pub async fn unproject(view_params: &ViewParams, layer_margin: Option<MarginParams>, coord: ScreenCoord) -> DataCoord {
    // TODO: implement
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

/// Calculate the visible data range based on camera view and other view parameters.
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
