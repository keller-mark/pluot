// Simulated vertex shader logic for SVG point positioning.
use nalgebra_glm::{Vec2, Vec4, Mat4};

use crate::layers::core::{AspectRatioMode, UnitsMode};


fn scale(x: f32, y: f32, z: f32) -> Mat4 {
  return Mat4::from_columns(&[
    Vec4::new(x, 0.0, 0.0, 0.0),
    Vec4::new(0.0, y, 0.0, 0.0),
    Vec4::new(0.0, 0.0, z, 0.0),
    Vec4::new(0.0, 0.0, 0.0, 1.0)
  ]);
}

fn translate(x: f32, y: f32, z: f32) -> Mat4 {
  return Mat4::from_columns(&[
    Vec4::new(1.0, 0.0, 0.0, 0.0),
    Vec4::new(0.0, 1.0, 0.0, 0.0),
    Vec4::new(0.0, 0.0, 1.0, 0.0),
    Vec4::new(x, y, z, 1.0),
  ]);
}

fn get_aspect_ratio_mat(layer_aspect_ratio: f32, aspect_ratio_mode: AspectRatioMode) -> Mat4 {
    // Determine the x and y extents to use,
    // based on the aspect ratio mode and layer aspect ratio.
    // We only need to handle the aspect ratio mode when the layer_aspect_ratio is not 1.
    let mut x_scale_for_aspect_ratio_mode = 1.0;
    let mut y_scale_for_aspect_ratio_mode = 1.0;
    if (aspect_ratio_mode == AspectRatioMode::Contain) {
        // fit/contain
        if (layer_aspect_ratio > 1.0) {
            // Wide rectangle
            // Show more than (0, 1) in x direction. Show exactly (0, 1) in y direction.
            x_scale_for_aspect_ratio_mode = 1.0 / layer_aspect_ratio;
        } else if(layer_aspect_ratio < 1.0) {
            // Tall layer
            // Show exactly (0, 1) in x direction. Show more than (0, 1) in y direction.
            y_scale_for_aspect_ratio_mode = layer_aspect_ratio;
        } else {
            // Square layer; no change needed.
            // Show exactly (0, 1) in both directions.
        }
    } else if (aspect_ratio_mode == AspectRatioMode::Cover) {
        // fill/cover
        if(layer_aspect_ratio > 1.0) {
            // Wide rectangle
            // Show exactly (0, 1) in x direction. Show less than (0, 1) in y direction.
            y_scale_for_aspect_ratio_mode = layer_aspect_ratio;
        } else if(layer_aspect_ratio < 1.0) {
            // Tall layer
            // Show less than (0, 1) in x direction. Show exactly (0, 1) in y direction.
            x_scale_for_aspect_ratio_mode = 1.0 / layer_aspect_ratio;
        } else {
            // Square layer; no change needed.
            // Show exactly (0, 1) in both directions.
        }
    }

    // Only scaling will result in the (0, 1) region being centered.
    // If we want to align 0 to the left or bottom, we need to add a translation step as well.
    return scale(
        x_scale_for_aspect_ratio_mode,
        y_scale_for_aspect_ratio_mode,
        1.0
    );
}

// Here, we "simulate" the vertex shader logic in Rust,
// enabling us to check the logic that we are using for handling margins, aspect ratios, and camera transforms.
// It will require us to manually keep things in sync with the actual shader code, but that is ok.
// The rust syntax is luckily very similar to WGSL.
pub fn get_point_position(
    pos_x: f32,
    pos_y: f32,

    // "uniforms" below
    layer_width_px: f32,
    layer_height_px: f32,
    camera_view_raw: &[f32; 16],
    data_unit_mode: UnitsMode, // 0: pixel units, 1: data units. // TODO: keep the enums here?
    aspect_ratio_mode: AspectRatioMode, // 0: ignore/squeeze, 1: fit/contain, 2: fill/cover.
    aspect_ratio_alignment_mode: u32, // 0: center, 1: start, 2: end.
) -> (f32, f32) {
    // Simulate the vertex shader logic here.
    // Ideally, use the same variable names, and where possible, the same syntax.
    // However, we want to output to pixel coordinates within the layer area.

    if (data_unit_mode == UnitsMode::Pixels) {
        // Pixel units mode: positions are already in pixel units.
        return (pos_x, layer_height_px - pos_y);
    }

    let camera_view = Mat4::from_column_slice(camera_view_raw);

    let layer_aspect_ratio = layer_width_px / layer_height_px;

    // Get the scale() matrix to handle the aspect ratio mode.
    let ASPECT_RATIO_MAT = get_aspect_ratio_mat(
        layer_aspect_ratio,
        aspect_ratio_mode
    );

    // We operate in (0 to 1) space, since it is more intuitive.
    // We therefore need matrices to transform (0, 1) into clip space ("NDC") (-1 to 1)
    let NORM_TO_NDC_MAT = translate(-1.0, -1.0, 0.0) * scale(2.0, 2.0, 1.0); // Scale up by 2, THEN translate by -1 (i.e., translating in the scaled-up space)
    // And the inverse, to convert back from NDC (-1 to 1) to normalized (0 to 1) space.
    let NDC_TO_NORM_MAT =  translate(0.5, 0.5, 0.0) * scale(0.5, 0.5, 1.0); // Scale down by 0.5, THEN translate by 0.5 (i.e., translating in the scaled-down space)

    // Model-view-projection matrix
    // References:
    // - https://github.com/flekschas/regl-scatterplot/blob/17a650c352fad313d1574472b2fdc5f58b9e1eca/src/index.js#L1582
    // - https://nalgebra.rs/docs/user_guide/cg_recipes#build-a-mvp-matrix
    let model_view_projection = ASPECT_RATIO_MAT * camera_view;

    // TYPICALLY: position = projectionMatrix * viewMatrix * modelMatrix * inputModelSpacePosition
    // Where:
    // - inputPosition - the 4D vertex position (homogeneous coordinate) in model space.
    // - modelMatrix - the 4x4 matrix that transforms input vertices from model space to world space.
    // - viewMatrix - the 4x4 view matrix, which takes as input a point in world space and the result is a point in camera space.
    // - projectionMatrix - the 4x4 projection matrix, which takes as input a point in camera space and the result is a projected point in clip space.

    let point_pos_norm = /*LAYER_NORM_TO_VIEW_NORM_MAT * */ (
        // The camera from dom-2d-camera operates in NDC space.
        // The `dom-2d-camera` library is designed to work in **NDC space (-1 to 1)**, not normalized space (0 to 1).
        // When you zoom in, the scale increases, and when you pan, the translation values are in NDC space.
        // However, after this transformation, we want to be working in (0 to 1) normalized space.

        // The camera operates in NDC space, but your data is in normalized space. We need to:
        // 1. Convert data from (0,1) to NDC (-1,1)
        // 2. Apply camera
        // 3. Convert back to (0,1)
        // 4. Apply aspect ratio and margins
        // 5. Convert final result to NDC for rendering
        // We apply camera AFTER converting to NDC, and DON'T convert back until
        // after all NDC-space operations are done. This keeps translations in the correct space.

        (NDC_TO_NORM_MAT * model_view_projection * NORM_TO_NDC_MAT)
        // TODO: support applying a model matrix (arbitrarily passed by the user)
        // before applying the camera (i.e., transforming the data coordinates).
        * Vec4::new(pos_x, pos_y, 0.0, 1.0)
    );

    // Matrix to convert from normalized (0 to 1) space to pixel space.
    // Note: the SVG coordinate system has (0,0) at the top-left,
    // with +X to the right and +Y downwards, so we also need to flip the Y axis.
    let NORM_TO_PX_MAT = scale(
        layer_width_px,
        layer_height_px,
        1.0
    );
    let point_pos_px  = NORM_TO_PX_MAT * Vec4::new(point_pos_norm.x, point_pos_norm.y, 0.0, 1.0);

    return (point_pos_px.x, layer_height_px - point_pos_px.y);
}

// TODO: add inline unit tests here