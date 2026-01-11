#![cfg(test)]

// In this file, we will check our logic for coordinate systems used in plotting,
// and our matrix operations used in our shaders.
// We need to test different combinations of:
// - View aspect ratios (square, wide, tall)
// - Margins (none, equal on all sides, only on some sides) resulting in different "layer" aspect ratios
// - Aspect ratio modes (ignore/squeeze, fit/contain, fill/cover)
// - Aspect ratio alignment modes (center, start, end)
// - Camera matrices (identity, zoomed in, panned, both zoomed and panned)
// - User-supplied model matrices (arbitrary affine transformations of the data points)
// - Data unit modes (pixel units, data units)
//
// See slides in https://docs.google.com/presentation/d/1Dnp93BjfdIPbHS_B1J1AHVq8jsc9U1xvJMG8yageLEI/edit?slide=id.p#slide=id.p

// Known things that need work/fixing:
// - Implementing aspect ratio alignment modes
// - Something is off about the camera handling when using non-square aspect ratios
//   (see comment at end of get_aspect_ratio_mat function)
// - Implementing user-supplied model matrices
// - Implementing pixel unit mode

use nalgebra_glm::{Vec2, Vec4, Mat4};


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

fn get_aspect_ratio_mat(layer_aspect_ratio: f32, aspect_ratio_mode: u32) -> Mat4 {
    // Determine the x and y extents to use,
    // based on the aspect ratio mode and layer aspect ratio.
    // We only need to handle the aspect ratio mode when the layer_aspect_ratio is not 1.
    let mut x_scale_for_aspect_ratio_mode = 1.0;
    let mut y_scale_for_aspect_ratio_mode = 1.0;
    if (aspect_ratio_mode == 1) {
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
    } else if (aspect_ratio_mode == 2) {
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
        // TODO: do we only need to scale in X, and always keep Y scale at 1.0? would this fix the camera issues?
        y_scale_for_aspect_ratio_mode,
        1.0
    );
}

// Here, we "simulate" the vertex shader logic in Rust,
// enabling us to check the logic that we are using for handling margins, aspect ratios, and camera transforms.
// It will require us to manually keep things in sync with the actual shader code, but that is ok.
// The rust syntax is luckily very similar to WGSL.
fn simulate_vertex_shader(
    point_pos_orig: Vec2,
    // "uniforms" below
    camera_view: Mat4,
    view_width_px: f32,
    view_height_px: f32,
    margin_left_px: f32, // TODO: remove margin handling here and in the tests since we are now using set_viewport/set_scissor_rect.
    margin_top_px: f32,
    margin_right_px: f32,
    margin_bottom_px: f32,
    aspect_ratio_mode: u32, // 0: ignore/squeeze, 1: fit/contain, 2: fill/cover.
    aspect_ratio_alignment_mode: u32, // 0: center, 1: start, 2: end.
    data_unit_mode: u32, // 0: pixel units, 1: data units.
) -> Vec2 {
    // Simulate the vertex shader logic here.
    // Ideally, use the same variable names, and where possible, the same syntax.

    let layer_width_px = view_width_px - (margin_left_px + margin_right_px);
    let layer_height_px = view_height_px - (margin_top_px + margin_bottom_px);
    let layer_aspect_ratio = layer_width_px / layer_height_px;

    // Get the scale() matrix to handle the aspect ratio mode.
    let ASPECT_RATIO_MAT = get_aspect_ratio_mat(
        layer_aspect_ratio,
        aspect_ratio_mode
    );

    // NOTE: these same calculations will need to be done on the CPU as well,
    // to determine the extents to use for the axes.

    // Convert margins from pixel to (0 to 1) normalized units.
    // TODO: simplify and use more matrix operations once things are working.
    let margin_top_norm = margin_top_px / view_height_px;
    let margin_right_norm = margin_right_px / view_width_px;
    let margin_bottom_norm = margin_bottom_px / view_height_px;
    let margin_left_norm = margin_left_px / view_width_px;

    // Transformation matrix so that points are drawn within the plot area.
    // I.e., transform from normalized "layer" space to normalized "view" space.
    // Aka MARGIN_MAT (handles the view margins).
    let LAYER_NORM_TO_VIEW_NORM_MAT = translate(
        margin_left_norm, // left
        margin_bottom_norm, // bottom
        0.0
    ) * scale(
        1.0 - (margin_left_norm + margin_right_norm),
        1.0 - (margin_top_norm + margin_bottom_norm),
        1.0
    ); // Scale down by (1 - total_margin), THEN translate the scaled stuff by left/bottom margins.

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

    let point_pos_norm = LAYER_NORM_TO_VIEW_NORM_MAT * (
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
        * Vec4::new(point_pos_orig.x, point_pos_orig.y, 0.0, 1.0)
    );
    let point_pos_ndc = NORM_TO_NDC_MAT * Vec4::new(point_pos_norm.x, point_pos_norm.y, 0.0, 1.0);

    point_pos_ndc.xy()
}


// The "base" / easiest case: square aspect ratio, ignore mode, identity camera, zero margins.
#[test]
fn test_square_aspect_ratio_with_ignore_mode_and_identity_camera_and_zero_margins() {
    // Consider data points at the corners of a unit square.
    let points = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
    ];

    let camera_view = Mat4::identity();

    let view_width_px = 100.0;
    let view_height_px = 100.0;

    let margin_left_px = 0.0;
    let margin_top_px = 0.0;
    let margin_right_px = 0.0;
    let margin_bottom_px = 0.0;

    let aspect_ratio_mode = 0; // Ignore
    let aspect_ratio_alignment_mode = 0; // Center
    let data_unit_mode = 1; // Data units

    // After applying the "vertex shader" logic, we should obtain these
    // coordinates in NDC space.
    let expected_points_ndc = vec![
        Vec2::new(-1.0, -1.0),
        Vec2::new(-1.0, 1.0),
        Vec2::new(1.0, -1.0),
        Vec2::new(1.0, 1.0),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        simulate_vertex_shader(
            *point_pos_orig,
            // "uniforms"
            camera_view,
            view_width_px,
            view_height_px,
            margin_left_px,
            margin_top_px,
            margin_right_px,
            margin_bottom_px,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            data_unit_mode,
        )
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);
}

// ======== TESTING HANDLING OF DIFFERENT ASPECT RATIO MODES ========
#[test]
fn test_wide_aspect_ratio_with_ignore_mode_and_identity_camera_and_zero_margins() {
    // Consider data points at the corners of a unit square.
    let points = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
    ];

    let camera_view = Mat4::identity();

    let view_width_px = 200.0;
    let view_height_px = 100.0;

    let margin_left_px = 0.0;
    let margin_top_px = 0.0;
    let margin_right_px = 0.0;
    let margin_bottom_px = 0.0;

    // When using a wide aspect ratio with "ignore",
    // we expect streching in the X direction.
    let aspect_ratio_mode = 0; // Ignore
    let aspect_ratio_alignment_mode = 0; // Center
    let data_unit_mode = 1; // Data units

    // After applying the "vertex shader" logic, we should obtain these
    // coordinates in NDC space.
    let expected_points_ndc = vec![
        // Due to the "ignore" aspect_ratio_mode,
        // these NDC coordinates will look the same as in the square case.
        Vec2::new(-1.0, -1.0),
        Vec2::new(-1.0, 1.0),
        Vec2::new(1.0, -1.0),
        Vec2::new(1.0, 1.0),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        simulate_vertex_shader(
            *point_pos_orig,
            // "uniforms"
            camera_view,
            view_width_px,
            view_height_px,
            margin_left_px,
            margin_top_px,
            margin_right_px,
            margin_bottom_px,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            data_unit_mode,
        )
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);
}

// Testing "contain" mode.
#[test]
fn test_wide_aspect_ratio_with_contain_mode_and_identity_camera_and_zero_margins() {
    // Consider data points at the corners of a unit square.
    let points = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
    ];

    let camera_view = Mat4::identity();

    let view_width_px = 200.0;
    let view_height_px = 100.0;

    let margin_left_px = 0.0;
    let margin_top_px = 0.0;
    let margin_right_px = 0.0;
    let margin_bottom_px = 0.0;

    // When using a wide aspect ratio with "contain",
    // we expect to be viewing more data in the X direction.
    let aspect_ratio_mode = 1; // Contain (fit)
    let aspect_ratio_alignment_mode = 0; // Center
    let data_unit_mode = 1; // Data units

    // After applying the "vertex shader" logic, we should obtain these
    // coordinates in NDC space.
    let expected_points_ndc = vec![
        // Due to the "contain" aspect_ratio_mode,
        // the X coordinates of the unit square will be compressed.
        Vec2::new(-0.5, -1.0),
        Vec2::new(-0.5, 1.0),
        Vec2::new(0.5, -1.0),
        Vec2::new(0.5, 1.0),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        simulate_vertex_shader(
            *point_pos_orig,
            // "uniforms"
            camera_view,
            view_width_px,
            view_height_px,
            margin_left_px,
            margin_top_px,
            margin_right_px,
            margin_bottom_px,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            data_unit_mode,
        )
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);
}

#[test]
fn test_tall_aspect_ratio_with_contain_mode_and_identity_camera_and_zero_margins() {
    // Consider data points at the corners of a unit square.
    let points = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
    ];

    let camera_view = Mat4::identity();

    let view_width_px = 100.0;
    let view_height_px = 200.0;

    let margin_left_px = 0.0;
    let margin_top_px = 0.0;
    let margin_right_px = 0.0;
    let margin_bottom_px = 0.0;

    // When using a wide aspect ratio with "contain",
    // we expect to be viewing more data in the X direction.
    let aspect_ratio_mode = 1; // Contain (fit)
    let aspect_ratio_alignment_mode = 0; // Center
    let data_unit_mode = 1; // Data units

    // After applying the "vertex shader" logic, we should obtain these
    // coordinates in NDC space.
    let expected_points_ndc = vec![
        // Due to the "contain" aspect_ratio_mode,
        // the Y coordinates of the unit square will be compressed.
        Vec2::new(-1.0, -0.5),
        Vec2::new(-1.0, 0.5),
        Vec2::new(1.0, -0.5),
        Vec2::new(1.0, 0.5),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        simulate_vertex_shader(
            *point_pos_orig,
            // "uniforms"
            camera_view,
            view_width_px,
            view_height_px,
            margin_left_px,
            margin_top_px,
            margin_right_px,
            margin_bottom_px,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            data_unit_mode,
        )
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);
}

// Testing "cover" mode.
#[test]
fn test_wide_aspect_ratio_with_cover_mode_and_identity_camera_and_zero_margins() {
    // Consider data points at the corners of a unit square.
    let points = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
    ];

    let camera_view = Mat4::identity();

    let view_width_px = 200.0;
    let view_height_px = 100.0;

    let margin_left_px = 0.0;
    let margin_top_px = 0.0;
    let margin_right_px = 0.0;
    let margin_bottom_px = 0.0;

    // When using a wide aspect ratio with "cover",
    // we expect to be viewing less data in the Y direction.
    let aspect_ratio_mode = 2; // Cover (fill)
    let aspect_ratio_alignment_mode = 0; // Center
    let data_unit_mode = 1; // Data units

    // After applying the "vertex shader" logic, we should obtain these
    // coordinates in NDC space.
    let expected_points_ndc = vec![
        // Due to the "cover" aspect_ratio_mode,
        // the Y coordinates of the unit square will be outside of NDC.
        Vec2::new(-1.0, -2.0),
        Vec2::new(-1.0, 2.0),
        Vec2::new(1.0, -2.0),
        Vec2::new(1.0, 2.0),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        simulate_vertex_shader(
            *point_pos_orig,
            // "uniforms"
            camera_view,
            view_width_px,
            view_height_px,
            margin_left_px,
            margin_top_px,
            margin_right_px,
            margin_bottom_px,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            data_unit_mode,
        )
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);
}

#[test]
fn test_tall_aspect_ratio_with_cover_mode_and_identity_camera_and_zero_margins() {
    // Consider data points at the corners of a unit square.
    let points = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
    ];

    let camera_view = Mat4::identity();

    let view_width_px = 100.0;
    let view_height_px = 200.0;

    let margin_left_px = 0.0;
    let margin_top_px = 0.0;
    let margin_right_px = 0.0;
    let margin_bottom_px = 0.0;

    // When using a tall aspect ratio with "cover",
    // we expect to be viewing less data in the X direction.
    let aspect_ratio_mode = 2; // Cover (fill)
    let aspect_ratio_alignment_mode = 0; // Center
    let data_unit_mode = 1; // Data units

    // After applying the "vertex shader" logic, we should obtain these
    // coordinates in NDC space.
    let expected_points_ndc = vec![
        // Due to the "contain" aspect_ratio_mode,
        // the Y coordinates of the unit square will be compressed.
        Vec2::new(-2.0, -1.0),
        Vec2::new(-2.0, 1.0),
        Vec2::new(2.0, -1.0),
        Vec2::new(2.0, 1.0),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        simulate_vertex_shader(
            *point_pos_orig,
            // "uniforms"
            camera_view,
            view_width_px,
            view_height_px,
            margin_left_px,
            margin_top_px,
            margin_right_px,
            margin_bottom_px,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            data_unit_mode,
        )
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);
}

// ======== TESTING HANDLING OF MARGINS ========
#[test]
fn test_square_aspect_ratio_with_ignore_mode_and_identity_camera_and_margins_all_sides_equal() {
    // Consider data points at the corners of a unit square.
    let points = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
    ];

    let camera_view = Mat4::identity();

    let view_width_px = 1000.0;
    let view_height_px = 1000.0;

    // Large margins of 250 pixels on all sides, for a 1000x1000 view.
    let margin_left_px = 250.0;
    let margin_top_px = 250.0;
    let margin_right_px = 250.0;
    let margin_bottom_px = 250.0;

    let aspect_ratio_mode = 0; // Ignore
    let aspect_ratio_alignment_mode = 0; // Center
    let data_unit_mode = 1; // Data units

    // After applying the "vertex shader" logic, we should obtain these
    // coordinates in NDC space.
    let expected_points_ndc = vec![
        Vec2::new(-0.5, -0.5),
        Vec2::new(-0.5, 0.5),
        Vec2::new(0.5, -0.5),
        Vec2::new(0.5, 0.5),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        simulate_vertex_shader(
            *point_pos_orig,
            // "uniforms"
            camera_view,
            view_width_px,
            view_height_px,
            margin_left_px,
            margin_top_px,
            margin_right_px,
            margin_bottom_px,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            data_unit_mode,
        )
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);
}

#[test]
fn test_square_aspect_ratio_with_ignore_mode_and_identity_camera_and_margins_bottom_and_left() {
    // Consider data points at the corners of a unit square.
    let points = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
    ];

    let camera_view = Mat4::identity();

    let view_width_px = 1000.0;
    let view_height_px = 1000.0;

    // Large margins of 250 pixels on bottom and left sides, for a 1000x1000 view.
    // The plot will be therefore shifted up and to the right, still with a square aspect ratio.
    let margin_left_px = 250.0;
    let margin_top_px = 0.0;
    let margin_right_px = 0.0;
    let margin_bottom_px = 250.0;

    let aspect_ratio_mode = 0; // Ignore
    let aspect_ratio_alignment_mode = 0; // Center
    let data_unit_mode = 1; // Data units

    // After applying the "vertex shader" logic, we should obtain these
    // coordinates in NDC space.
    let expected_points_ndc = vec![
        Vec2::new(-0.5, -0.5),
        Vec2::new(-0.5, 1.0),
        Vec2::new(1.0, -0.5),
        Vec2::new(1.0, 1.0),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        simulate_vertex_shader(
            *point_pos_orig,
            // "uniforms"
            camera_view,
            view_width_px,
            view_height_px,
            margin_left_px,
            margin_top_px,
            margin_right_px,
            margin_bottom_px,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            data_unit_mode,
        )
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);
}

#[test]
fn test_square_aspect_ratio_with_ignore_mode_and_identity_camera_and_margins_top_and_right() {
    // Consider data points at the corners of a unit square.
    let points = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
    ];

    let camera_view = Mat4::identity();

    let view_width_px = 1000.0;
    let view_height_px = 1000.0;

    // Large margins of 250 pixels on top and right sides, for a 1000x1000 view.
    // The plot will be therefore rendered in the bottom left, still with a square aspect ratio.
    let margin_left_px = 0.0;
    let margin_top_px = 250.0;
    let margin_right_px = 250.0;
    let margin_bottom_px = 0.0;

    let aspect_ratio_mode = 0; // Ignore
    let aspect_ratio_alignment_mode = 0; // Center
    let data_unit_mode = 1; // Data units

    // After applying the "vertex shader" logic, we should obtain these
    // coordinates in NDC space.
    let expected_points_ndc = vec![
        Vec2::new(-1.0, -1.0),
        Vec2::new(-1.0, 0.5),
        Vec2::new(0.5, -1.0),
        Vec2::new(0.5, 0.5),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        simulate_vertex_shader(
            *point_pos_orig,
            // "uniforms"
            camera_view,
            view_width_px,
            view_height_px,
            margin_left_px,
            margin_top_px,
            margin_right_px,
            margin_bottom_px,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            data_unit_mode,
        )
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);
}

// In the margin tests above, we have always kept the aspect ratio of the inner plotting region a square.
// In the margin tests below, we will create a non-square plotting regions via margins.

#[test]
fn test_square_view_wide_layer_aspect_ratio_with_ignore_mode_and_identity_camera_and_margin_bottom_only() {
    // Consider data points at the corners of a unit square.
    let points = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
    ];

    let camera_view = Mat4::identity();

    let view_width_px = 1000.0;
    let view_height_px = 1000.0;

    // Large margin of 500 pixels on bottom side of a 1000x1000 view.
    // The plot will therefore have a wide aspect ratio, but keep in mind we are using "ignore" mode here.
    let margin_left_px = 0.0;
    let margin_top_px = 0.0;
    let margin_right_px = 0.0;
    let margin_bottom_px = 500.0;

    let aspect_ratio_mode = 0; // Ignore
    let aspect_ratio_alignment_mode = 0; // Center
    let data_unit_mode = 1; // Data units

    // After applying the "vertex shader" logic, we should obtain these
    // coordinates in NDC space.
    let expected_points_ndc = vec![
        Vec2::new(-1.0, 0.0),
        Vec2::new(-1.0, 1.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        simulate_vertex_shader(
            *point_pos_orig,
            // "uniforms"
            camera_view,
            view_width_px,
            view_height_px,
            margin_left_px,
            margin_top_px,
            margin_right_px,
            margin_bottom_px,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            data_unit_mode,
        )
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);
}

#[test]
fn test_square_view_wide_layer_aspect_ratio_with_contain_mode_and_identity_camera_and_margin_bottom_only() {
    // Consider data points at the corners of a unit square.
    let points = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
    ];

    let camera_view = Mat4::identity();

    let view_width_px = 1000.0;
    let view_height_px = 1000.0;

    // Large margin of 500 pixels on bottom side of a 1000x1000 view.
    // The plot will therefore have a wide aspect ratio.
    let margin_left_px = 0.0;
    let margin_top_px = 0.0;
    let margin_right_px = 0.0;
    let margin_bottom_px = 500.0;

    let aspect_ratio_mode = 1; // Contain
    let aspect_ratio_alignment_mode = 0; // Center
    let data_unit_mode = 1; // Data units

    // After applying the "vertex shader" logic, we should obtain these
    // coordinates in NDC space.
    let expected_points_ndc = vec![
        Vec2::new(-0.5, 0.0),
        Vec2::new(-0.5, 1.0),
        Vec2::new(0.5, 0.0),
        Vec2::new(0.5, 1.0),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        simulate_vertex_shader(
            *point_pos_orig,
            // "uniforms"
            camera_view,
            view_width_px,
            view_height_px,
            margin_left_px,
            margin_top_px,
            margin_right_px,
            margin_bottom_px,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            data_unit_mode,
        )
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);
}


#[test]
fn test_square_view_wide_layer_aspect_ratio_with_cover_mode_and_identity_camera_and_margin_bottom_only() {
    // Consider data points at the corners of a unit square.
    let points = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
    ];

    let camera_view = Mat4::identity();

    let view_width_px = 1000.0;
    let view_height_px = 1000.0;

    // Large margin of 500 pixels on bottom side of a 1000x1000 view.
    // The plot will therefore have a wide aspect ratio.
    let margin_left_px = 0.0;
    let margin_top_px = 0.0;
    let margin_right_px = 0.0;
    let margin_bottom_px = 500.0;

    let aspect_ratio_mode = 2; // Cover
    let aspect_ratio_alignment_mode = 0; // Center
    let data_unit_mode = 1; // Data units

    // After applying the "vertex shader" logic, we should obtain these
    // coordinates in NDC space.
    let expected_points_ndc = vec![
        Vec2::new(-1.0, -0.5),
        Vec2::new(-1.0, 1.5),
        Vec2::new(1.0, -0.5),
        Vec2::new(1.0, 1.5),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        simulate_vertex_shader(
            *point_pos_orig,
            // "uniforms"
            camera_view,
            view_width_px,
            view_height_px,
            margin_left_px,
            margin_top_px,
            margin_right_px,
            margin_bottom_px,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            data_unit_mode,
        )
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);
}

#[test]
fn test_wide_view_square_layer_aspect_ratio_with_ignore_mode_and_identity_camera_and_margin_left_only() {
    // Consider data points at the corners of a unit square.
    let points = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
    ];

    let camera_view = Mat4::identity();

    let view_width_px = 1000.0;
    let view_height_px = 500.0; // Wide view.

    // Large margin of 500 pixels on left side of a 1000x500 view.
    // The plot will therefore have a square aspect ratio.
    let margin_left_px = 500.0;
    let margin_top_px = 0.0;
    let margin_right_px = 0.0;
    let margin_bottom_px = 0.0;

    let aspect_ratio_mode = 0; // Ignore
    let aspect_ratio_alignment_mode = 0; // Center
    let data_unit_mode = 1; // Data units

    // After applying the "vertex shader" logic, we should obtain these
    // coordinates in NDC space.
    let expected_points_ndc = vec![
        Vec2::new(0.0, -1.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, -1.0),
        Vec2::new(1.0, 1.0),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        simulate_vertex_shader(
            *point_pos_orig,
            // "uniforms"
            camera_view,
            view_width_px,
            view_height_px,
            margin_left_px,
            margin_top_px,
            margin_right_px,
            margin_bottom_px,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            data_unit_mode,
        )
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);
}

#[test]
fn test_wide_view_square_layer_aspect_ratio_with_contain_mode_and_identity_camera_and_margin_left_only() {
    // Consider data points at the corners of a unit square.
    let points = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
    ];

    let camera_view = Mat4::identity();

    let view_width_px = 1000.0;
    let view_height_px = 500.0; // Wide view.

    // Large margin of 500 pixels on left side of a 1000x500 view.
    // The plot will therefore have a square aspect ratio.
    let margin_left_px = 500.0;
    let margin_top_px = 0.0;
    let margin_right_px = 0.0;
    let margin_bottom_px = 0.0;

    let aspect_ratio_mode = 1; // Contain
    let aspect_ratio_alignment_mode = 0; // Center
    let data_unit_mode = 1; // Data units

    // After applying the "vertex shader" logic, we should obtain these
    // coordinates in NDC space.
    let expected_points_ndc = vec![
        Vec2::new(0.0, -1.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, -1.0),
        Vec2::new(1.0, 1.0),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        simulate_vertex_shader(
            *point_pos_orig,
            // "uniforms"
            camera_view,
            view_width_px,
            view_height_px,
            margin_left_px,
            margin_top_px,
            margin_right_px,
            margin_bottom_px,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            data_unit_mode,
        )
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);
}

#[test]
fn test_wide_view_square_layer_aspect_ratio_with_cover_mode_and_identity_camera_and_margin_left_only() {
    // Consider data points at the corners of a unit square.
    let points = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
    ];

    let camera_view = Mat4::identity();

    let view_width_px = 1000.0;
    let view_height_px = 500.0; // Wide view.

    // Large margin of 500 pixels on left side of a 1000x500 view.
    // The plot will therefore have a square aspect ratio.
    let margin_left_px = 500.0;
    let margin_top_px = 0.0;
    let margin_right_px = 0.0;
    let margin_bottom_px = 0.0;

    let aspect_ratio_mode = 1; // Contain
    let aspect_ratio_alignment_mode = 0; // Center
    let data_unit_mode = 1; // Data units

    // After applying the "vertex shader" logic, we should obtain these
    // coordinates in NDC space.
    let expected_points_ndc = vec![
        Vec2::new(0.0, -1.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, -1.0),
        Vec2::new(1.0, 1.0),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        simulate_vertex_shader(
            *point_pos_orig,
            // "uniforms"
            camera_view,
            view_width_px,
            view_height_px,
            margin_left_px,
            margin_top_px,
            margin_right_px,
            margin_bottom_px,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            data_unit_mode,
        )
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);
}

#[test]
fn test_wide_view_square_layer_aspect_ratio_with_ignore_mode_and_identity_camera_and_margin_right_only() {
    // Consider data points at the corners of a unit square.
    let points = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
    ];

    let camera_view = Mat4::identity();

    let view_width_px = 1000.0;
    let view_height_px = 500.0; // Wide view.

    // Large margin of 500 pixels on right side of a 1000x500 view.
    // The plot will therefore have a square aspect ratio.
    let margin_left_px = 0.0;
    let margin_top_px = 0.0;
    let margin_right_px = 500.0;
    let margin_bottom_px = 0.0;

    let aspect_ratio_mode = 0; // Ignore
    let aspect_ratio_alignment_mode = 0; // Center
    let data_unit_mode = 1; // Data units

    // After applying the "vertex shader" logic, we should obtain these
    // coordinates in NDC space.
    let expected_points_ndc = vec![
        Vec2::new(-1.0, -1.0),
        Vec2::new(-1.0, 1.0),
        Vec2::new(0.0, -1.0),
        Vec2::new(0.0, 1.0),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        simulate_vertex_shader(
            *point_pos_orig,
            // "uniforms"
            camera_view,
            view_width_px,
            view_height_px,
            margin_left_px,
            margin_top_px,
            margin_right_px,
            margin_bottom_px,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            data_unit_mode,
        )
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);
}

#[test]
fn test_wide_view_square_layer_aspect_ratio_with_contain_mode_and_identity_camera_and_margin_right_only() {
    // Consider data points at the corners of a unit square.
    let points = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
    ];

    let camera_view = Mat4::identity();

    let view_width_px = 1000.0;
    let view_height_px = 500.0; // Wide view.

    // Large margin of 500 pixels on right side of a 1000x500 view.
    // The plot will therefore have a square aspect ratio.
    let margin_left_px = 0.0;
    let margin_top_px = 0.0;
    let margin_right_px = 500.0;
    let margin_bottom_px = 0.0;

    let aspect_ratio_mode = 1; // Contain
    let aspect_ratio_alignment_mode = 0; // Center
    let data_unit_mode = 1; // Data units

    // After applying the "vertex shader" logic, we should obtain these
    // coordinates in NDC space.
    let expected_points_ndc = vec![
        Vec2::new(-1.0, -1.0),
        Vec2::new(-1.0, 1.0),
        Vec2::new(0.0, -1.0),
        Vec2::new(0.0, 1.0),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        simulate_vertex_shader(
            *point_pos_orig,
            // "uniforms"
            camera_view,
            view_width_px,
            view_height_px,
            margin_left_px,
            margin_top_px,
            margin_right_px,
            margin_bottom_px,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            data_unit_mode,
        )
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);
}

#[test]
fn test_wide_view_square_layer_aspect_ratio_with_cover_mode_and_identity_camera_and_margin_right_only() {
    // Consider data points at the corners of a unit square.
    let points = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
    ];

    let camera_view = Mat4::identity();

    let view_width_px = 1000.0;
    let view_height_px = 500.0; // Wide view.

    // Large margin of 500 pixels on right side of a 1000x500 view.
    // The plot will therefore have a square aspect ratio.
    let margin_left_px = 0.0;
    let margin_top_px = 0.0;
    let margin_right_px = 500.0;
    let margin_bottom_px = 0.0;

    let aspect_ratio_mode = 1; // Contain
    let aspect_ratio_alignment_mode = 0; // Center
    let data_unit_mode = 1; // Data units

    // After applying the "vertex shader" logic, we should obtain these
    // coordinates in NDC space.
    let expected_points_ndc = vec![
        Vec2::new(-1.0, -1.0),
        Vec2::new(-1.0, 1.0),
        Vec2::new(0.0, -1.0),
        Vec2::new(0.0, 1.0),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        simulate_vertex_shader(
            *point_pos_orig,
            // "uniforms"
            camera_view,
            view_width_px,
            view_height_px,
            margin_left_px,
            margin_top_px,
            margin_right_px,
            margin_bottom_px,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            data_unit_mode,
        )
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);
}

// ======== TESTING CAMERA ZOOM TRANSFORMS ========
// The "base" / easiest case: square aspect ratio, ignore mode, zero margins.
#[test]
fn test_square_aspect_ratio_with_ignore_mode_and_zoomed_in_2x_camera_and_zero_margins() {
    // Consider data points at the corners of a unit square.
    let points = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
    ];

    // When camera zoom factor is 2,
    // we expect the points to be scaled by 2x,
    // so that we only see data in the range [0.25, 0.75] in both X and Y,
    // which maps to NDC coordinates [-1, 1].
    let camera_zoom = 2.0;
    let camera_target_x = 0.0;
    let camera_target_y = 0.0;

    let camera_view = Mat4::from_columns(&[
      Vec4::new(camera_zoom, 0.0, 0.0, 0.0),
      Vec4::new(0.0, camera_zoom, 0.0, 0.0),
      Vec4::new(0.0, 0.0, 0.0, 0.0),
      Vec4::new(camera_target_x, camera_target_y, 0.0, 1.0)
    ]);

    let view_width_px = 100.0;
    let view_height_px = 100.0;

    let margin_left_px = 0.0;
    let margin_top_px = 0.0;
    let margin_right_px = 0.0;
    let margin_bottom_px = 0.0;

    let aspect_ratio_mode = 0; // Ignore
    let aspect_ratio_alignment_mode = 0; // Center
    let data_unit_mode = 1; // Data units

    // After applying the "vertex shader" logic, we should obtain these
    // coordinates in NDC space.
    let expected_points_ndc = vec![
        Vec2::new(-2.0, -2.0),
        Vec2::new(-2.0, 2.0),
        Vec2::new(2.0, -2.0),
        Vec2::new(2.0, 2.0),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        simulate_vertex_shader(
            *point_pos_orig,
            // "uniforms"
            camera_view,
            view_width_px,
            view_height_px,
            margin_left_px,
            margin_top_px,
            margin_right_px,
            margin_bottom_px,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            data_unit_mode,
        )
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);
}

#[test]
fn test_square_aspect_ratio_with_ignore_mode_and_zoomed_in_4x_camera_and_zero_margins() {
    // Consider data points at the corners of a unit square.
    let points = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
    ];

    // When camera zoom factor is 2,
    // we expect the points to be scaled by 2x,
    // so that we only see data in the range [0.25, 0.75] in both X and Y,
    // which maps to NDC coordinates [-1, 1].
    let camera_zoom = 4.0;
    let camera_target_x = 0.0;
    let camera_target_y = 0.0;

    let camera_view = Mat4::from_columns(&[
      Vec4::new(camera_zoom, 0.0, 0.0, 0.0),
      Vec4::new(0.0, camera_zoom, 0.0, 0.0),
      Vec4::new(0.0, 0.0, 0.0, 0.0),
      Vec4::new(camera_target_x, camera_target_y, 0.0, 1.0)
    ]);

    let view_width_px = 100.0;
    let view_height_px = 100.0;

    let margin_left_px = 0.0;
    let margin_top_px = 0.0;
    let margin_right_px = 0.0;
    let margin_bottom_px = 0.0;

    let aspect_ratio_mode = 0; // Ignore
    let aspect_ratio_alignment_mode = 0; // Center
    let data_unit_mode = 1; // Data units

    // After applying the "vertex shader" logic, we should obtain these
    // coordinates in NDC space.
    let expected_points_ndc = vec![
        Vec2::new(-4.0, -4.0),
        Vec2::new(-4.0, 4.0),
        Vec2::new(4.0, -4.0),
        Vec2::new(4.0, 4.0),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        simulate_vertex_shader(
            *point_pos_orig,
            // "uniforms"
            camera_view,
            view_width_px,
            view_height_px,
            margin_left_px,
            margin_top_px,
            margin_right_px,
            margin_bottom_px,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            data_unit_mode,
        )
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);
}

#[test]
fn test_square_aspect_ratio_with_ignore_mode_and_zoomed_out_2x_camera_and_zero_margins() {
    // Consider data points at the corners of a unit square.
    let points = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
    ];

    // When camera zoom factor is 2,
    // we expect the points to be scaled by 2x,
    // so that we only see data in the range [0.25, 0.75] in both X and Y,
    // which maps to NDC coordinates [-1, 1].
    let camera_zoom = 0.5;
    let camera_target_x = 0.0;
    let camera_target_y = 0.0;

    let camera_view = Mat4::from_columns(&[
      Vec4::new(camera_zoom, 0.0, 0.0, 0.0),
      Vec4::new(0.0, camera_zoom, 0.0, 0.0),
      Vec4::new(0.0, 0.0, 0.0, 0.0),
      Vec4::new(camera_target_x, camera_target_y, 0.0, 1.0)
    ]);

    let view_width_px = 100.0;
    let view_height_px = 100.0;

    let margin_left_px = 0.0;
    let margin_top_px = 0.0;
    let margin_right_px = 0.0;
    let margin_bottom_px = 0.0;

    let aspect_ratio_mode = 0; // Ignore
    let aspect_ratio_alignment_mode = 0; // Center
    let data_unit_mode = 1; // Data units

    // After applying the "vertex shader" logic, we should obtain these
    // coordinates in NDC space.
    let expected_points_ndc = vec![
        Vec2::new(-0.5, -0.5),
        Vec2::new(-0.5, 0.5),
        Vec2::new(0.5, -0.5),
        Vec2::new(0.5, 0.5),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        simulate_vertex_shader(
            *point_pos_orig,
            // "uniforms"
            camera_view,
            view_width_px,
            view_height_px,
            margin_left_px,
            margin_top_px,
            margin_right_px,
            margin_bottom_px,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            data_unit_mode,
        )
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);
}

#[test]
fn test_square_aspect_ratio_with_ignore_mode_and_zoomed_out_4x_camera_and_zero_margins() {
    // Consider data points at the corners of a unit square.
    let points = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
    ];

    // When camera zoom factor is 2,
    // we expect the points to be scaled by 2x,
    // so that we only see data in the range [0.25, 0.75] in both X and Y,
    // which maps to NDC coordinates [-1, 1].
    let camera_zoom = 0.25;
    let camera_target_x = 0.0;
    let camera_target_y = 0.0;

    let camera_view = Mat4::from_columns(&[
      Vec4::new(camera_zoom, 0.0, 0.0, 0.0),
      Vec4::new(0.0, camera_zoom, 0.0, 0.0),
      Vec4::new(0.0, 0.0, 0.0, 0.0),
      Vec4::new(camera_target_x, camera_target_y, 0.0, 1.0)
    ]);

    let view_width_px = 100.0;
    let view_height_px = 100.0;

    let margin_left_px = 0.0;
    let margin_top_px = 0.0;
    let margin_right_px = 0.0;
    let margin_bottom_px = 0.0;

    let aspect_ratio_mode = 0; // Ignore
    let aspect_ratio_alignment_mode = 0; // Center
    let data_unit_mode = 1; // Data units

    // After applying the "vertex shader" logic, we should obtain these
    // coordinates in NDC space.
    let expected_points_ndc = vec![
        Vec2::new(-0.25, -0.25),
        Vec2::new(-0.25, 0.25),
        Vec2::new(0.25, -0.25),
        Vec2::new(0.25, 0.25),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        simulate_vertex_shader(
            *point_pos_orig,
            // "uniforms"
            camera_view,
            view_width_px,
            view_height_px,
            margin_left_px,
            margin_top_px,
            margin_right_px,
            margin_bottom_px,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            data_unit_mode,
        )
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);
}
