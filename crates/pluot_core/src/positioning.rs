// Simulated vertex shader logic for SVG point positioning.
use nalgebra_glm::{Vec2, Vec4, Mat4};

use crate::render_traits::{AspectRatioMode, AspectRatioAlignmentMode, UnitsMode};

// Pixels and Normalized units are both independent of the camera view and
// aspect ratio mode (unlike Data units). They differ only in whether the
// value must first be divided by the layer size to land in the (0, 1)
// normalized space that the camera/aspect-ratio pipeline (and the model
// matrix) operates in: Pixels values are in screen pixels, so they are
// divided by the layer size; Normalized values are already in (0, 1) space.
fn is_camera_independent_mode(mode: UnitsMode) -> bool {
    matches!(mode, UnitsMode::Pixels | UnitsMode::Normalized)
}

pub fn get_scale_mat(x: f32, y: f32, z: f32) -> Mat4 {
  return Mat4::from_columns(&[
    Vec4::new(x, 0.0, 0.0, 0.0),
    Vec4::new(0.0, y, 0.0, 0.0),
    Vec4::new(0.0, 0.0, z, 0.0),
    Vec4::new(0.0, 0.0, 0.0, 1.0)
  ]);
}

pub fn get_translate_mat(x: f32, y: f32, z: f32) -> Mat4 {
  return Mat4::from_columns(&[
    Vec4::new(1.0, 0.0, 0.0, 0.0),
    Vec4::new(0.0, 1.0, 0.0, 0.0),
    Vec4::new(0.0, 0.0, 1.0, 0.0),
    Vec4::new(x, y, z, 1.0),
  ]);
}

pub fn get_aspect_ratio_mat(layer_aspect_ratio: f32, aspect_ratio_mode: AspectRatioMode, aspect_ratio_alignment_mode: AspectRatioAlignmentMode) -> Mat4 {
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

    // To handle aspect_ratio_alignment_mode, we compute the required translation.
    let mut x_translation_for_aspect_ratio_alignment_mode = 0.0;
    let mut y_translation_for_aspect_ratio_alignment_mode = 0.0;
    if (aspect_ratio_alignment_mode == AspectRatioAlignmentMode::Start) {
        // start
        x_translation_for_aspect_ratio_alignment_mode = x_scale_for_aspect_ratio_mode - 1.0;
        y_translation_for_aspect_ratio_alignment_mode = y_scale_for_aspect_ratio_mode - 1.0;
    } else if (aspect_ratio_alignment_mode == AspectRatioAlignmentMode::End) {
        // end
        x_translation_for_aspect_ratio_alignment_mode = 1.0 - x_scale_for_aspect_ratio_mode;
        y_translation_for_aspect_ratio_alignment_mode = 1.0 - y_scale_for_aspect_ratio_mode;
    }

    // Only scaling will result in the (0, 1) region being centered.
    // If we want to align 0 to the left or bottom, we need to add a translation step as well.
    return get_translate_mat(
        x_translation_for_aspect_ratio_alignment_mode,
        y_translation_for_aspect_ratio_alignment_mode,
        0.0
    ) * get_scale_mat(
        x_scale_for_aspect_ratio_mode,
        y_scale_for_aspect_ratio_mode,
        1.0
    );
}

// TODO: get_margin_mat for handling of margins?
// (despite not needing to handle them in get_point_position)

// Here, we "simulate" the vertex shader logic in Rust,
// enabling us to check the logic that we are using for handling margins, aspect ratios, and camera transforms.
// It will require us to manually keep things in sync with the actual shader code, but that is ok.
// The rust syntax is luckily very similar to WGSL.
// Note: we treat the Y coordinate as increasing upwards, for consistency with the data coordinate system.
// Conversion to a coordinate system where Y increases downwards (e.g., for HTML canvas) is delegated to the caller.
pub fn get_point_position(
    pos_x: f32,
    pos_y: f32,

    // "uniforms" below
    layer_width_px: f32,
    layer_height_px: f32,
    camera_view_raw: &[f32],
    data_unit_mode_x: UnitsMode,
    data_unit_mode_y: UnitsMode,
    aspect_ratio_mode: AspectRatioMode,
    aspect_ratio_alignment_mode: AspectRatioAlignmentMode,
    model_matrix_raw: Option<&[f32]>, // Column-major 4x4 model matrix (identity if None).
) -> (f32, f32) {
    // Simulate the vertex shader logic here.
    // Ideally, use the same variable names, and where possible, the same syntax.
    // However, we want to output to pixel coordinates within the layer area.

    let model_matrix = model_matrix_raw
        .map(|m| Mat4::from_column_slice(m))
        .unwrap_or(Mat4::identity());



    let mut pixel_output: (f32, f32) = (0.0, 0.0);
    if is_camera_independent_mode(data_unit_mode_x) || is_camera_independent_mode(data_unit_mode_y) {
        // Pixel/Normalized units mode: model_matrix is applied in normalized (0,1) space.
        // Matches the shader logic:
        //   point_pos_norm = vertex_pos_px / layer_size (Pixels) or vertex_pos (Normalized, already 0-1)
        //   point_pos_ndc = NORM_TO_NDC_MAT * model_matrix * vec4(point_pos_norm, 0, 1)
        let pos_norm = Vec4::new(
            if data_unit_mode_x == UnitsMode::Normalized { pos_x } else { pos_x / layer_width_px },
            if data_unit_mode_y == UnitsMode::Normalized { pos_y } else { pos_y / layer_height_px },
            0.0, 1.0
        );
        let pos_transformed = model_matrix * pos_norm;
        pixel_output = (pos_transformed.x * layer_width_px, pos_transformed.y * layer_height_px);
        if is_camera_independent_mode(data_unit_mode_x) && is_camera_independent_mode(data_unit_mode_y) {
            return pixel_output;
        }
    }

    let camera_view = Mat4::from_column_slice(camera_view_raw);

    let layer_aspect_ratio = layer_width_px / layer_height_px;

    // Get the scale() matrix to handle the aspect ratio mode.
    let ASPECT_RATIO_MAT = get_aspect_ratio_mat(
        layer_aspect_ratio,
        aspect_ratio_mode,
        aspect_ratio_alignment_mode
    );

    // We operate in (0 to 1) space, since it is more intuitive.
    // We therefore need matrices to transform (0, 1) into clip space ("NDC") (-1 to 1)
    let NORM_TO_NDC_MAT = get_translate_mat(-1.0, -1.0, 0.0) * get_scale_mat(2.0, 2.0, 1.0); // Scale up by 2, THEN translate by -1 (i.e., translating in the scaled-up space)
    // And the inverse, to convert back from NDC (-1 to 1) to normalized (0 to 1) space.
    let NDC_TO_NORM_MAT =  get_translate_mat(0.5, 0.5, 0.0) * get_scale_mat(0.5, 0.5, 1.0); // Scale down by 0.5, THEN translate by 0.5 (i.e., translating in the scaled-down space)

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
        // The model_matrix transforms coordinates in model space before the camera is applied,
        // to allow for applying user-provided affine transformations.
        * model_matrix * Vec4::new(pos_x, pos_y, 0.0, 1.0)
    );

    // Matrix to convert from normalized (0 to 1) space to pixel space.
    // Note: the SVG coordinate system has (0,0) at the top-left,
    // with +X to the right and +Y downwards, so we also need to flip the Y axis.
    let NORM_TO_PX_MAT = get_scale_mat(
        layer_width_px,
        layer_height_px,
        1.0
    );
    let point_pos_px = NORM_TO_PX_MAT * Vec4::new(point_pos_norm.x, point_pos_norm.y, 0.0, 1.0);

    let output_x = if is_camera_independent_mode(data_unit_mode_x) { pixel_output.0 } else { point_pos_px.x };
    let output_y = if is_camera_independent_mode(data_unit_mode_y) { pixel_output.1 } else { point_pos_px.y };

    // Don't flip the Y coordinate here, and instead delegate to the caller if flipping is required.
    return (output_x, output_y);
}

// Compute how a size (width, height) transforms through the same pipeline as positions.
// A size is the difference between two positions, so translations cancel out (w=0).
// This is useful for determining, e.g., how large an image appears after camera/aspect-ratio transforms.
pub fn get_point_size(
    size_x: f32,
    size_y: f32,

    // "uniforms" below
    layer_width_px: f32,
    layer_height_px: f32,
    camera_view_raw: &[f32],
    data_unit_mode_x: UnitsMode,
    data_unit_mode_y: UnitsMode,
    aspect_ratio_mode: AspectRatioMode,
    aspect_ratio_alignment_mode: AspectRatioAlignmentMode,
    model_matrix_raw: Option<&[f32]>,
) -> (f32, f32) {
    let model_matrix = model_matrix_raw
        .map(|m| Mat4::from_column_slice(m))
        .unwrap_or(Mat4::identity());

    let mut pixel_output = (0.0_f32, 0.0_f32);
    if is_camera_independent_mode(data_unit_mode_x) || is_camera_independent_mode(data_unit_mode_y) {
        // Pixel/Normalized mode: model_matrix applied in normalized space (w=0 for size).
        let size_norm = Vec4::new(
            if data_unit_mode_x == UnitsMode::Normalized { size_x } else { size_x / layer_width_px },
            if data_unit_mode_y == UnitsMode::Normalized { size_y } else { size_y / layer_height_px },
            0.0, 0.0
        );
        let size_transformed = model_matrix * size_norm;
        pixel_output = (size_transformed.x * layer_width_px, size_transformed.y * layer_height_px);
        if is_camera_independent_mode(data_unit_mode_x) && is_camera_independent_mode(data_unit_mode_y) {
            return pixel_output;
        }
    }

    let camera_view = Mat4::from_column_slice(camera_view_raw);

    let layer_aspect_ratio = layer_width_px / layer_height_px;

    let ASPECT_RATIO_MAT = get_aspect_ratio_mat(
        layer_aspect_ratio,
        aspect_ratio_mode,
        aspect_ratio_alignment_mode
    );

    let NORM_TO_NDC_MAT = get_translate_mat(-1.0, -1.0, 0.0) * get_scale_mat(2.0, 2.0, 1.0);
    let NDC_TO_NORM_MAT = get_translate_mat(0.5, 0.5, 0.0) * get_scale_mat(0.5, 0.5, 1.0);

    let model_view_projection = ASPECT_RATIO_MAT * camera_view;

    // Use w=0: translations cancel out for sizes (deltas between two positions).
    let size_norm = (NDC_TO_NORM_MAT * model_view_projection * NORM_TO_NDC_MAT)
        * model_matrix * Vec4::new(size_x, size_y, 0.0, 0.0);

    let NORM_TO_PX_MAT = get_scale_mat(
        layer_width_px,
        layer_height_px,
        1.0
    );
    let size_px = NORM_TO_PX_MAT * Vec4::new(size_norm.x, size_norm.y, 0.0, 0.0);

    let output_x = if is_camera_independent_mode(data_unit_mode_x) { pixel_output.0 } else { size_px.x };
    let output_y = if is_camera_independent_mode(data_unit_mode_y) { pixel_output.1 } else { size_px.y };

    return (output_x, output_y);
}
