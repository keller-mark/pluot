use nalgebra_glm::{Vec2, Vec4, Mat4};

use pluot_core::positioning::{get_point_position, get_point_size};
use pluot_core::{AspectRatioMode, AspectRatioAlignmentMode, UnitsMode};


// The "base" / easiest case: square aspect ratio, ignore mode, identity camera, zero margins.
#[test]
fn test_square_aspect_ratio_with_ignore_mode_and_identity_camera_aram_center() {
    // Consider data points at the corners of a unit square.
    let points = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
    ];

    let camera_view = Mat4::identity();

    let layer_width_px = 100.0;
    let layer_height_px = 100.0;

    let aspect_ratio_mode = AspectRatioMode::Ignore;
    let aspect_ratio_alignment_mode = AspectRatioAlignmentMode::Center;
    let data_unit_mode = UnitsMode::Data;

    // These are in pixel space relative to the layer dimensions.
    let expected_points_ndc = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 100.0),
        Vec2::new(100.0, 0.0),
        Vec2::new(100.0, 100.0),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        let (out_x, out_y) = get_point_position(
            point_pos_orig.x,
            point_pos_orig.y,
            // "uniforms"
            layer_width_px,
            layer_height_px,
            camera_view.as_slice(), // column-major order
            data_unit_mode,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            None,
        );
        return Vec2::new(out_x, out_y);
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);

    let (size_out_x, size_out_y) = get_point_size(
        1.0,
        1.0,
        layer_width_px,
        layer_height_px,
        camera_view.as_slice(),
        data_unit_mode,
        aspect_ratio_mode,
        aspect_ratio_alignment_mode,
        None,
    );
    assert_eq!((size_out_x, size_out_y), (100.0_f32, 100.0_f32));
}

// ======== TESTING HANDLING OF DIFFERENT ASPECT RATIO MODES ========
#[test]
fn test_wide_aspect_ratio_with_ignore_mode_and_identity_camera_aram_center() {
    // Consider data points at the corners of a unit square.
    let points = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
    ];

    let camera_view = Mat4::identity();

    let layer_width_px = 200.0;
    let layer_height_px = 100.0;

    // When using a wide aspect ratio with "ignore",
    // we expect streching in the X direction.
    let aspect_ratio_mode = AspectRatioMode::Ignore;
    let aspect_ratio_alignment_mode = AspectRatioAlignmentMode::Center;
    let data_unit_mode = UnitsMode::Data;

    // These are in pixel space relative to the layer dimensions.
    let expected_points_ndc = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 100.0),
        Vec2::new(200.0, 0.0),
        Vec2::new(200.0, 100.0),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        let (out_x, out_y) = get_point_position(
            point_pos_orig.x,
            point_pos_orig.y,
            // "uniforms"
            layer_width_px,
            layer_height_px,
            camera_view.as_slice(), // column-major order
            data_unit_mode,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            None,
        );
        return Vec2::new(out_x, out_y);
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);

    let (size_out_x, size_out_y) = get_point_size(
        1.0,
        1.0,
        layer_width_px,
        layer_height_px,
        camera_view.as_slice(),
        data_unit_mode,
        aspect_ratio_mode,
        aspect_ratio_alignment_mode,
        None,
    );
    assert_eq!((size_out_x, size_out_y), (200.0_f32, 100.0_f32));
}


// Testing "contain" mode.
#[test]
fn test_wide_aspect_ratio_with_contain_mode_and_identity_camera_aram_center() {
    // Consider data points at the corners of a unit square.
    let points = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
    ];

    let camera_view = Mat4::identity();

    let layer_width_px = 200.0;
    let layer_height_px = 100.0;

    // When using a wide aspect ratio with "contain",
    // we expect to be viewing more data in the X direction.
    let aspect_ratio_mode = AspectRatioMode::Contain;
    let aspect_ratio_alignment_mode = AspectRatioAlignmentMode::Center;
    let data_unit_mode = UnitsMode::Data;

    // These are in pixel space relative to the layer dimensions.
    let expected_points_ndc = vec![
        // Due to the "contain" aspect_ratio_mode,
        // the X coordinates of the unit square will be compressed.
        Vec2::new(50.0, 0.0),
        Vec2::new(50.0, 100.0),
        Vec2::new(150.0, 0.0),
        Vec2::new(150.0, 100.0),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        let (out_x, out_y) = get_point_position(
            point_pos_orig.x,
            point_pos_orig.y,
            // "uniforms"
            layer_width_px,
            layer_height_px,
            camera_view.as_slice(), // column-major order
            data_unit_mode,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            None,
        );
        return Vec2::new(out_x, out_y);
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);

    let (size_out_x, size_out_y) = get_point_size(
        1.0,
        1.0,
        layer_width_px,
        layer_height_px,
        camera_view.as_slice(),
        data_unit_mode,
        aspect_ratio_mode,
        aspect_ratio_alignment_mode,
        None,
    );
    assert_eq!((size_out_x, size_out_y), (100.0_f32, 100.0_f32));
}

#[test]
fn test_tall_aspect_ratio_with_contain_mode_and_identity_camera_aram_center() {
    // Consider data points at the corners of a unit square.
    let points = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
    ];

    let camera_view = Mat4::identity();

    let layer_width_px = 100.0;
    let layer_height_px = 200.0;

    // When using a tall aspect ratio with "contain",
    // we expect to be viewing more data in the Y direction.
    let aspect_ratio_mode = AspectRatioMode::Contain;
    let aspect_ratio_alignment_mode = AspectRatioAlignmentMode::Center;
    let data_unit_mode = UnitsMode::Data;

    // These are in pixel space relative to the layer dimensions.
    let expected_points_ndc = vec![
        // Due to the "contain" aspect_ratio_mode,
        // the Y coordinates of the unit square will be compressed.
        Vec2::new(0.0, 50.0),
        Vec2::new(0.0, 150.0),
        Vec2::new(100.0, 50.0),
        Vec2::new(100.0, 150.0),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        let (out_x, out_y) = get_point_position(
            point_pos_orig.x,
            point_pos_orig.y,
            // "uniforms"
            layer_width_px,
            layer_height_px,
            camera_view.as_slice(), // column-major order
            data_unit_mode,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            None,
        );
        return Vec2::new(out_x, out_y);
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);

    let (size_out_x, size_out_y) = get_point_size(
        1.0,
        1.0,
        layer_width_px,
        layer_height_px,
        camera_view.as_slice(),
        data_unit_mode,
        aspect_ratio_mode,
        aspect_ratio_alignment_mode,
        None,
    );
    assert_eq!((size_out_x, size_out_y), (100.0_f32, 100.0_f32));
}

// Testing "cover" mode.
#[test]
fn test_wide_aspect_ratio_with_cover_mode_and_identity_camera_aram_center() {
    // Consider data points at the corners of a unit square.
    let points = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
    ];

    let camera_view = Mat4::identity();

    let layer_width_px = 200.0;
    let layer_height_px = 100.0;

    // When using a wide aspect ratio with "contain",
    // we expect to be viewing more data in the X direction.
    let aspect_ratio_mode = AspectRatioMode::Cover;
    let aspect_ratio_alignment_mode = AspectRatioAlignmentMode::Center;
    let data_unit_mode = UnitsMode::Data;

    // These are in pixel space relative to the layer dimensions.
    let expected_points_ndc = vec![
        // Due to the "cover" aspect_ratio_mode,
        // the Y coordinates of the unit square will be outside of NDC.
        Vec2::new(0.0, -50.0),
        Vec2::new(0.0, 150.0),
        Vec2::new(200.0, -50.0),
        Vec2::new(200.0, 150.0),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        let (out_x, out_y) = get_point_position(
            point_pos_orig.x,
            point_pos_orig.y,
            // "uniforms"
            layer_width_px,
            layer_height_px,
            camera_view.as_slice(), // column-major order
            data_unit_mode,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            None,
        );
        return Vec2::new(out_x, out_y);
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);

    let (size_out_x, size_out_y) = get_point_size(
        1.0,
        1.0,
        layer_width_px,
        layer_height_px,
        camera_view.as_slice(),
        data_unit_mode,
        aspect_ratio_mode,
        aspect_ratio_alignment_mode,
        None,
    );
    assert_eq!((size_out_x, size_out_y), (200.0_f32, 200.0_f32));
}

#[test]
fn test_tall_aspect_ratio_with_cover_mode_and_identity_camera_aram_center() {
    // Consider data points at the corners of a unit square.
    let points = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
    ];

    let camera_view = Mat4::identity();

    let layer_width_px = 100.0;
    let layer_height_px = 200.0;

    // When using a tall aspect ratio with "cover",
    // we expect to be viewing less data in the X direction.
    let aspect_ratio_mode = AspectRatioMode::Cover;
    let aspect_ratio_alignment_mode = AspectRatioAlignmentMode::Center;
    let data_unit_mode = UnitsMode::Data;

    // These are in pixel space relative to the layer dimensions.
    let expected_points_ndc = vec![
        // Due to the "cover" aspect_ratio_mode,
        // the Y coordinates of the unit square will be outside of NDC.
        Vec2::new(-50.0, 0.0),
        Vec2::new(-50.0, 200.0),
        Vec2::new(150.0, 0.0),
        Vec2::new(150.0, 200.0),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        let (out_x, out_y) = get_point_position(
            point_pos_orig.x,
            point_pos_orig.y,
            // "uniforms"
            layer_width_px,
            layer_height_px,
            camera_view.as_slice(), // column-major order
            data_unit_mode,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            None,
        );
        return Vec2::new(out_x, out_y);
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);

    let (size_out_x, size_out_y) = get_point_size(
        1.0,
        1.0,
        layer_width_px,
        layer_height_px,
        camera_view.as_slice(),
        data_unit_mode,
        aspect_ratio_mode,
        aspect_ratio_alignment_mode,
        None,
    );
    assert_eq!((size_out_x, size_out_y), (200.0_f32, 200.0_f32));
}


// ======== TESTING CAMERA ZOOM TRANSFORMS ========
// The "base" / easiest case: square aspect ratio, ignore mode, zero margins.
#[test]
fn test_square_aspect_ratio_with_ignore_mode_and_zoomed_in_2x_camera_aram_center() {
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

    let layer_width_px = 100.0;
    let layer_height_px = 100.0;

    let aspect_ratio_mode = AspectRatioMode::Ignore;
    let aspect_ratio_alignment_mode = AspectRatioAlignmentMode::Center;
    let data_unit_mode = UnitsMode::Data;

    // These are in pixel space relative to the layer dimensions.
    let expected_points_ndc = vec![
        Vec2::new(-50.0, -50.0),
        Vec2::new(-50.0, 150.0),
        Vec2::new(150.0, -50.0),
        Vec2::new(150.0, 150.0),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        let (out_x, out_y) = get_point_position(
            point_pos_orig.x,
            point_pos_orig.y,
            // "uniforms"
            layer_width_px,
            layer_height_px,
            camera_view.as_slice(), // column-major order
            data_unit_mode,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            None,
        );
        return Vec2::new(out_x, out_y);
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);

    let (size_out_x, size_out_y) = get_point_size(
        1.0,
        1.0,
        layer_width_px,
        layer_height_px,
        camera_view.as_slice(),
        data_unit_mode,
        aspect_ratio_mode,
        aspect_ratio_alignment_mode,
        None,
    );
    assert_eq!((size_out_x, size_out_y), (200.0_f32, 200.0_f32));
}

#[test]
fn test_square_aspect_ratio_with_ignore_mode_and_zoomed_in_4x_camera_aram_center() {
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

    let layer_width_px = 100.0;
    let layer_height_px = 100.0;

    let aspect_ratio_mode = AspectRatioMode::Ignore;
    let aspect_ratio_alignment_mode = AspectRatioAlignmentMode::Center;
    let data_unit_mode = UnitsMode::Data;

    // These are in pixel space relative to the layer dimensions.
    let expected_points_ndc = vec![
        Vec2::new(-150.0, -150.0),
        Vec2::new(-150.0, 250.0),
        Vec2::new(250.0, -150.0),
        Vec2::new(250.0, 250.0),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        let (out_x, out_y) = get_point_position(
            point_pos_orig.x,
            point_pos_orig.y,
            // "uniforms"
            layer_width_px,
            layer_height_px,
            camera_view.as_slice(), // column-major order
            data_unit_mode,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            None,
        );
        return Vec2::new(out_x, out_y);
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);

    let (size_out_x, size_out_y) = get_point_size(
        1.0,
        1.0,
        layer_width_px,
        layer_height_px,
        camera_view.as_slice(),
        data_unit_mode,
        aspect_ratio_mode,
        aspect_ratio_alignment_mode,
        None,
    );
    assert_eq!((size_out_x, size_out_y), (400.0_f32, 400.0_f32));
}

#[test]
fn test_square_aspect_ratio_with_ignore_mode_and_zoomed_out_2x_camera_aram_center() {
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

    let layer_width_px = 100.0;
    let layer_height_px = 100.0;

    let aspect_ratio_mode = AspectRatioMode::Ignore;
    let aspect_ratio_alignment_mode = AspectRatioAlignmentMode::Center;
    let data_unit_mode = UnitsMode::Data;

    // These are in pixel space relative to the layer dimensions.
    let expected_points_ndc = vec![
        Vec2::new(25.0, 25.0),
        Vec2::new(25.0, 75.0),
        Vec2::new(75.0, 25.0),
        Vec2::new(75.0, 75.0),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        let (out_x, out_y) = get_point_position(
            point_pos_orig.x,
            point_pos_orig.y,
            // "uniforms"
            layer_width_px,
            layer_height_px,
            camera_view.as_slice(), // column-major order
            data_unit_mode,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            None,
        );
        return Vec2::new(out_x, out_y);
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);

    let (size_out_x, size_out_y) = get_point_size(
        1.0,
        1.0,
        layer_width_px,
        layer_height_px,
        camera_view.as_slice(),
        data_unit_mode,
        aspect_ratio_mode,
        aspect_ratio_alignment_mode,
        None,
    );
    assert_eq!((size_out_x, size_out_y), (50.0_f32, 50.0_f32));
}

#[test]
fn test_square_aspect_ratio_with_ignore_mode_and_zoomed_out_4x_camera_aram_center() {
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

    let layer_width_px = 100.0;
    let layer_height_px = 100.0;

    let aspect_ratio_mode = AspectRatioMode::Ignore;
    let aspect_ratio_alignment_mode = AspectRatioAlignmentMode::Center;
    let data_unit_mode = UnitsMode::Data;

    // These are in pixel space relative to the layer dimensions.
    let expected_points_ndc = vec![
        Vec2::new(37.5, 37.5),
        Vec2::new(37.5, 62.5),
        Vec2::new(62.5, 37.5),
        Vec2::new(62.5, 62.5),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        let (out_x, out_y) = get_point_position(
            point_pos_orig.x,
            point_pos_orig.y,
            // "uniforms"
            layer_width_px,
            layer_height_px,
            camera_view.as_slice(), // column-major order
            data_unit_mode,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            None,
        );
        return Vec2::new(out_x, out_y);
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);

    let (size_out_x, size_out_y) = get_point_size(
        1.0,
        1.0,
        layer_width_px,
        layer_height_px,
        camera_view.as_slice(),
        data_unit_mode,
        aspect_ratio_mode,
        aspect_ratio_alignment_mode,
        None,
    );
    assert_eq!((size_out_x, size_out_y), (25.0_f32, 25.0_f32));
}

// ======== _aram_start variants ========

// The "base" / easiest case: square aspect ratio, ignore mode, identity camera, zero margins.
#[test]
fn test_square_aspect_ratio_with_ignore_mode_and_identity_camera_aram_start() {
    // Consider data points at the corners of a unit square.
    let points = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
    ];

    let camera_view = Mat4::identity();

    let layer_width_px = 100.0;
    let layer_height_px = 100.0;

    let aspect_ratio_mode = AspectRatioMode::Ignore;
    let aspect_ratio_alignment_mode = AspectRatioAlignmentMode::Start;
    let data_unit_mode = UnitsMode::Data;

    // These are in pixel space relative to the layer dimensions.
    let expected_points_ndc = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 100.0),
        Vec2::new(100.0, 0.0),
        Vec2::new(100.0, 100.0),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        let (out_x, out_y) = get_point_position(
            point_pos_orig.x,
            point_pos_orig.y,
            // "uniforms"
            layer_width_px,
            layer_height_px,
            camera_view.as_slice(), // column-major order
            data_unit_mode,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            None,
        );
        return Vec2::new(out_x, out_y);
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);

    let (size_out_x, size_out_y) = get_point_size(
        1.0,
        1.0,
        layer_width_px,
        layer_height_px,
        camera_view.as_slice(),
        data_unit_mode,
        aspect_ratio_mode,
        aspect_ratio_alignment_mode,
        None,
    );
    assert_eq!((size_out_x, size_out_y), (100.0_f32, 100.0_f32));
}

// ======== TESTING HANDLING OF DIFFERENT ASPECT RATIO MODES ========
#[test]
fn test_wide_aspect_ratio_with_ignore_mode_and_identity_camera_aram_start() {
    // Consider data points at the corners of a unit square.
    let points = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
    ];

    let camera_view = Mat4::identity();

    let layer_width_px = 200.0;
    let layer_height_px = 100.0;

    // When using a wide aspect ratio with "ignore",
    // we expect streching in the X direction.
    let aspect_ratio_mode = AspectRatioMode::Ignore;
    let aspect_ratio_alignment_mode = AspectRatioAlignmentMode::Start;
    let data_unit_mode = UnitsMode::Data;

    // These are in pixel space relative to the layer dimensions.
    let expected_points_ndc = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 100.0),
        Vec2::new(200.0, 0.0),
        Vec2::new(200.0, 100.0),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        let (out_x, out_y) = get_point_position(
            point_pos_orig.x,
            point_pos_orig.y,
            // "uniforms"
            layer_width_px,
            layer_height_px,
            camera_view.as_slice(), // column-major order
            data_unit_mode,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            None,
        );
        return Vec2::new(out_x, out_y);
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);

    let (size_out_x, size_out_y) = get_point_size(
        1.0,
        1.0,
        layer_width_px,
        layer_height_px,
        camera_view.as_slice(),
        data_unit_mode,
        aspect_ratio_mode,
        aspect_ratio_alignment_mode,
        None,
    );
    assert_eq!((size_out_x, size_out_y), (200.0_f32, 100.0_f32));
}


// Testing "contain" mode.
#[test]
fn test_wide_aspect_ratio_with_contain_mode_and_identity_camera_aram_start() {
    // Consider data points at the corners of a unit square.
    let points = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
    ];

    let camera_view = Mat4::identity();

    let layer_width_px = 200.0;
    let layer_height_px = 100.0;

    // When using a wide aspect ratio with "contain",
    // we expect to be viewing more data in the X direction.
    let aspect_ratio_mode = AspectRatioMode::Contain;
    let aspect_ratio_alignment_mode = AspectRatioAlignmentMode::Start;
    let data_unit_mode = UnitsMode::Data;

    // These are in pixel space relative to the layer dimensions.
    let expected_points_ndc = vec![
        // Due to the "contain" aspect_ratio_mode,
        // the X coordinates of the unit square will be compressed.
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 100.0),
        Vec2::new(100.0, 0.0),
        Vec2::new(100.0, 100.0),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        let (out_x, out_y) = get_point_position(
            point_pos_orig.x,
            point_pos_orig.y,
            // "uniforms"
            layer_width_px,
            layer_height_px,
            camera_view.as_slice(), // column-major order
            data_unit_mode,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            None,
        );
        return Vec2::new(out_x, out_y);
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);

    let (size_out_x, size_out_y) = get_point_size(
        1.0,
        1.0,
        layer_width_px,
        layer_height_px,
        camera_view.as_slice(),
        data_unit_mode,
        aspect_ratio_mode,
        aspect_ratio_alignment_mode,
        None,
    );
    assert_eq!((size_out_x, size_out_y), (100.0_f32, 100.0_f32));
}

#[test]
fn test_tall_aspect_ratio_with_contain_mode_and_identity_camera_aram_start() {
    // Consider data points at the corners of a unit square.
    let points = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
    ];

    let camera_view = Mat4::identity();

    let layer_width_px = 100.0;
    let layer_height_px = 200.0;

    // When using a tall aspect ratio with "contain",
    // we expect to be viewing more data in the Y direction.
    let aspect_ratio_mode = AspectRatioMode::Contain;
    let aspect_ratio_alignment_mode = AspectRatioAlignmentMode::Start;
    let data_unit_mode = UnitsMode::Data;

    // These are in pixel space relative to the layer dimensions.
    let expected_points_ndc = vec![
        // Due to the "contain" aspect_ratio_mode,
        // the Y coordinates of the unit square will be compressed.
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 100.0),
        Vec2::new(100.0, 0.0),
        Vec2::new(100.0, 100.0),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        let (out_x, out_y) = get_point_position(
            point_pos_orig.x,
            point_pos_orig.y,
            // "uniforms"
            layer_width_px,
            layer_height_px,
            camera_view.as_slice(), // column-major order
            data_unit_mode,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            None,
        );
        return Vec2::new(out_x, out_y);
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);

    let (size_out_x, size_out_y) = get_point_size(
        1.0,
        1.0,
        layer_width_px,
        layer_height_px,
        camera_view.as_slice(),
        data_unit_mode,
        aspect_ratio_mode,
        aspect_ratio_alignment_mode,
        None,
    );
    assert_eq!((size_out_x, size_out_y), (100.0_f32, 100.0_f32));
}

// Testing "cover" mode.
#[test]
fn test_wide_aspect_ratio_with_cover_mode_and_identity_camera_aram_start() {
    // Consider data points at the corners of a unit square.
    let points = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
    ];

    let camera_view = Mat4::identity();

    let layer_width_px = 200.0;
    let layer_height_px = 100.0;

    // When using a wide aspect ratio with "contain",
    // we expect to be viewing more data in the X direction.
    let aspect_ratio_mode = AspectRatioMode::Cover;
    let aspect_ratio_alignment_mode = AspectRatioAlignmentMode::Start;
    let data_unit_mode = UnitsMode::Data;

    // These are in pixel space relative to the layer dimensions.
    let expected_points_ndc = vec![
        // Due to the "cover" aspect_ratio_mode,
        // the Y coordinates of the unit square will be outside of NDC.
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 200.0),
        Vec2::new(200.0, 0.0),
        Vec2::new(200.0, 200.0),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        let (out_x, out_y) = get_point_position(
            point_pos_orig.x,
            point_pos_orig.y,
            // "uniforms"
            layer_width_px,
            layer_height_px,
            camera_view.as_slice(), // column-major order
            data_unit_mode,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            None,
        );
        return Vec2::new(out_x, out_y);
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);

    let (size_out_x, size_out_y) = get_point_size(
        1.0,
        1.0,
        layer_width_px,
        layer_height_px,
        camera_view.as_slice(),
        data_unit_mode,
        aspect_ratio_mode,
        aspect_ratio_alignment_mode,
        None,
    );
    assert_eq!((size_out_x, size_out_y), (200.0_f32, 200.0_f32));
}

#[test]
fn test_tall_aspect_ratio_with_cover_mode_and_identity_camera_aram_start() {
    // Consider data points at the corners of a unit square.
    let points = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
    ];

    let camera_view = Mat4::identity();

    let layer_width_px = 100.0;
    let layer_height_px = 200.0;

    // When using a tall aspect ratio with "cover",
    // we expect to be viewing less data in the X direction.
    let aspect_ratio_mode = AspectRatioMode::Cover;
    let aspect_ratio_alignment_mode = AspectRatioAlignmentMode::Start;
    let data_unit_mode = UnitsMode::Data;

    // These are in pixel space relative to the layer dimensions.
    let expected_points_ndc = vec![
        // Due to the "cover" aspect_ratio_mode,
        // the Y coordinates of the unit square will be outside of NDC.
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 200.0),
        Vec2::new(200.0, 0.0),
        Vec2::new(200.0, 200.0),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        let (out_x, out_y) = get_point_position(
            point_pos_orig.x,
            point_pos_orig.y,
            // "uniforms"
            layer_width_px,
            layer_height_px,
            camera_view.as_slice(), // column-major order
            data_unit_mode,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            None,
        );
        return Vec2::new(out_x, out_y);
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);

    let (size_out_x, size_out_y) = get_point_size(
        1.0,
        1.0,
        layer_width_px,
        layer_height_px,
        camera_view.as_slice(),
        data_unit_mode,
        aspect_ratio_mode,
        aspect_ratio_alignment_mode,
        None,
    );
    assert_eq!((size_out_x, size_out_y), (200.0_f32, 200.0_f32));
}


// ======== TESTING CAMERA ZOOM TRANSFORMS ========
// The "base" / easiest case: square aspect ratio, ignore mode, zero margins.
#[test]
fn test_square_aspect_ratio_with_ignore_mode_and_zoomed_in_2x_camera_aram_start() {
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

    let layer_width_px = 100.0;
    let layer_height_px = 100.0;

    let aspect_ratio_mode = AspectRatioMode::Ignore;
    let aspect_ratio_alignment_mode = AspectRatioAlignmentMode::Start;
    let data_unit_mode = UnitsMode::Data;

    // These are in pixel space relative to the layer dimensions.
    let expected_points_ndc = vec![
        Vec2::new(-50.0, -50.0),
        Vec2::new(-50.0, 150.0),
        Vec2::new(150.0, -50.0),
        Vec2::new(150.0, 150.0),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        let (out_x, out_y) = get_point_position(
            point_pos_orig.x,
            point_pos_orig.y,
            // "uniforms"
            layer_width_px,
            layer_height_px,
            camera_view.as_slice(), // column-major order
            data_unit_mode,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            None,
        );
        return Vec2::new(out_x, out_y);
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);

    let (size_out_x, size_out_y) = get_point_size(
        1.0,
        1.0,
        layer_width_px,
        layer_height_px,
        camera_view.as_slice(),
        data_unit_mode,
        aspect_ratio_mode,
        aspect_ratio_alignment_mode,
        None,
    );
    assert_eq!((size_out_x, size_out_y), (200.0_f32, 200.0_f32));
}

#[test]
fn test_square_aspect_ratio_with_ignore_mode_and_zoomed_in_4x_camera_aram_start() {
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

    let layer_width_px = 100.0;
    let layer_height_px = 100.0;

    let aspect_ratio_mode = AspectRatioMode::Ignore;
    let aspect_ratio_alignment_mode = AspectRatioAlignmentMode::Start;
    let data_unit_mode = UnitsMode::Data;

    // These are in pixel space relative to the layer dimensions.
    let expected_points_ndc = vec![
        Vec2::new(-150.0, -150.0),
        Vec2::new(-150.0, 250.0),
        Vec2::new(250.0, -150.0),
        Vec2::new(250.0, 250.0),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        let (out_x, out_y) = get_point_position(
            point_pos_orig.x,
            point_pos_orig.y,
            // "uniforms"
            layer_width_px,
            layer_height_px,
            camera_view.as_slice(), // column-major order
            data_unit_mode,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            None,
        );
        return Vec2::new(out_x, out_y);
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);

    let (size_out_x, size_out_y) = get_point_size(
        1.0,
        1.0,
        layer_width_px,
        layer_height_px,
        camera_view.as_slice(),
        data_unit_mode,
        aspect_ratio_mode,
        aspect_ratio_alignment_mode,
        None,
    );
    assert_eq!((size_out_x, size_out_y), (400.0_f32, 400.0_f32));
}

#[test]
fn test_square_aspect_ratio_with_ignore_mode_and_zoomed_out_2x_camera_aram_start() {
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

    let layer_width_px = 100.0;
    let layer_height_px = 100.0;

    let aspect_ratio_mode = AspectRatioMode::Ignore;
    let aspect_ratio_alignment_mode = AspectRatioAlignmentMode::Start;
    let data_unit_mode = UnitsMode::Data;

    // These are in pixel space relative to the layer dimensions.
    let expected_points_ndc = vec![
        Vec2::new(25.0, 25.0),
        Vec2::new(25.0, 75.0),
        Vec2::new(75.0, 25.0),
        Vec2::new(75.0, 75.0),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        let (out_x, out_y) = get_point_position(
            point_pos_orig.x,
            point_pos_orig.y,
            // "uniforms"
            layer_width_px,
            layer_height_px,
            camera_view.as_slice(), // column-major order
            data_unit_mode,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            None,
        );
        return Vec2::new(out_x, out_y);
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);

    let (size_out_x, size_out_y) = get_point_size(
        1.0,
        1.0,
        layer_width_px,
        layer_height_px,
        camera_view.as_slice(),
        data_unit_mode,
        aspect_ratio_mode,
        aspect_ratio_alignment_mode,
        None,
    );
    assert_eq!((size_out_x, size_out_y), (50.0_f32, 50.0_f32));
}

#[test]
fn test_square_aspect_ratio_with_ignore_mode_and_zoomed_out_4x_camera_aram_start() {
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

    let layer_width_px = 100.0;
    let layer_height_px = 100.0;

    let aspect_ratio_mode = AspectRatioMode::Ignore;
    let aspect_ratio_alignment_mode = AspectRatioAlignmentMode::Start;
    let data_unit_mode = UnitsMode::Data;

    // These are in pixel space relative to the layer dimensions.
    let expected_points_ndc = vec![
        Vec2::new(37.5, 37.5),
        Vec2::new(37.5, 62.5),
        Vec2::new(62.5, 37.5),
        Vec2::new(62.5, 62.5),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        let (out_x, out_y) = get_point_position(
            point_pos_orig.x,
            point_pos_orig.y,
            // "uniforms"
            layer_width_px,
            layer_height_px,
            camera_view.as_slice(), // column-major order
            data_unit_mode,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            None,
        );
        return Vec2::new(out_x, out_y);
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);

    let (size_out_x, size_out_y) = get_point_size(
        1.0,
        1.0,
        layer_width_px,
        layer_height_px,
        camera_view.as_slice(),
        data_unit_mode,
        aspect_ratio_mode,
        aspect_ratio_alignment_mode,
        None,
    );
    assert_eq!((size_out_x, size_out_y), (25.0_f32, 25.0_f32));
}

// ======== _aram_end variants ========

// The "base" / easiest case: square aspect ratio, ignore mode, identity camera, zero margins.
#[test]
fn test_square_aspect_ratio_with_ignore_mode_and_identity_camera_aram_end() {
    // Consider data points at the corners of a unit square.
    let points = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
    ];

    let camera_view = Mat4::identity();

    let layer_width_px = 100.0;
    let layer_height_px = 100.0;

    let aspect_ratio_mode = AspectRatioMode::Ignore;
    let aspect_ratio_alignment_mode = AspectRatioAlignmentMode::End;
    let data_unit_mode = UnitsMode::Data;

    // These are in pixel space relative to the layer dimensions.
    let expected_points_ndc = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 100.0),
        Vec2::new(100.0, 0.0),
        Vec2::new(100.0, 100.0),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        let (out_x, out_y) = get_point_position(
            point_pos_orig.x,
            point_pos_orig.y,
            // "uniforms"
            layer_width_px,
            layer_height_px,
            camera_view.as_slice(), // column-major order
            data_unit_mode,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            None,
        );
        return Vec2::new(out_x, out_y);
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);

    let (size_out_x, size_out_y) = get_point_size(
        1.0,
        1.0,
        layer_width_px,
        layer_height_px,
        camera_view.as_slice(),
        data_unit_mode,
        aspect_ratio_mode,
        aspect_ratio_alignment_mode,
        None,
    );
    assert_eq!((size_out_x, size_out_y), (100.0_f32, 100.0_f32));
}

// ======== TESTING HANDLING OF DIFFERENT ASPECT RATIO MODES ========
#[test]
fn test_wide_aspect_ratio_with_ignore_mode_and_identity_camera_aram_end() {
    // Consider data points at the corners of a unit square.
    let points = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
    ];

    let camera_view = Mat4::identity();

    let layer_width_px = 200.0;
    let layer_height_px = 100.0;

    // When using a wide aspect ratio with "ignore",
    // we expect streching in the X direction.
    let aspect_ratio_mode = AspectRatioMode::Ignore;
    let aspect_ratio_alignment_mode = AspectRatioAlignmentMode::End;
    let data_unit_mode = UnitsMode::Data;

    // These are in pixel space relative to the layer dimensions.
    let expected_points_ndc = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 100.0),
        Vec2::new(200.0, 0.0),
        Vec2::new(200.0, 100.0),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        let (out_x, out_y) = get_point_position(
            point_pos_orig.x,
            point_pos_orig.y,
            // "uniforms"
            layer_width_px,
            layer_height_px,
            camera_view.as_slice(), // column-major order
            data_unit_mode,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            None,
        );
        return Vec2::new(out_x, out_y);
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);

    let (size_out_x, size_out_y) = get_point_size(
        1.0,
        1.0,
        layer_width_px,
        layer_height_px,
        camera_view.as_slice(),
        data_unit_mode,
        aspect_ratio_mode,
        aspect_ratio_alignment_mode,
        None,
    );
    assert_eq!((size_out_x, size_out_y), (200.0_f32, 100.0_f32));
}


// Testing "contain" mode.
#[test]
fn test_wide_aspect_ratio_with_contain_mode_and_identity_camera_aram_end() {
    // Consider data points at the corners of a unit square.
    let points = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
    ];

    let camera_view = Mat4::identity();

    let layer_width_px = 200.0;
    let layer_height_px = 100.0;

    // When using a wide aspect ratio with "contain",
    // we expect to be viewing more data in the X direction.
    let aspect_ratio_mode = AspectRatioMode::Contain;
    let aspect_ratio_alignment_mode = AspectRatioAlignmentMode::End;
    let data_unit_mode = UnitsMode::Data;

    // These are in pixel space relative to the layer dimensions.
    let expected_points_ndc = vec![
        // Due to the "contain" aspect_ratio_mode,
        // the X coordinates of the unit square will be compressed.
        Vec2::new(100.0, 0.0),
        Vec2::new(100.0, 100.0),
        Vec2::new(200.0, 0.0),
        Vec2::new(200.0, 100.0),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        let (out_x, out_y) = get_point_position(
            point_pos_orig.x,
            point_pos_orig.y,
            // "uniforms"
            layer_width_px,
            layer_height_px,
            camera_view.as_slice(), // column-major order
            data_unit_mode,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            None,
        );
        return Vec2::new(out_x, out_y);
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);

    let (size_out_x, size_out_y) = get_point_size(
        1.0,
        1.0,
        layer_width_px,
        layer_height_px,
        camera_view.as_slice(),
        data_unit_mode,
        aspect_ratio_mode,
        aspect_ratio_alignment_mode,
        None,
    );
    assert_eq!((size_out_x, size_out_y), (100.0_f32, 100.0_f32));
}

#[test]
fn test_tall_aspect_ratio_with_contain_mode_and_identity_camera_aram_end() {
    // Consider data points at the corners of a unit square.
    let points = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
    ];

    let camera_view = Mat4::identity();

    let layer_width_px = 100.0;
    let layer_height_px = 200.0;

    // When using a tall aspect ratio with "contain",
    // we expect to be viewing more data in the Y direction.
    let aspect_ratio_mode = AspectRatioMode::Contain;
    let aspect_ratio_alignment_mode = AspectRatioAlignmentMode::End;
    let data_unit_mode = UnitsMode::Data;

    // These are in pixel space relative to the layer dimensions.
    let expected_points_ndc = vec![
        // Due to the "contain" aspect_ratio_mode,
        // the Y coordinates of the unit square will be compressed.
        Vec2::new(0.0, 100.0),
        Vec2::new(0.0, 200.0),
        Vec2::new(100.0, 100.0),
        Vec2::new(100.0, 200.0),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        let (out_x, out_y) = get_point_position(
            point_pos_orig.x,
            point_pos_orig.y,
            // "uniforms"
            layer_width_px,
            layer_height_px,
            camera_view.as_slice(), // column-major order
            data_unit_mode,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            None,
        );
        return Vec2::new(out_x, out_y);
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);

    let (size_out_x, size_out_y) = get_point_size(
        1.0,
        1.0,
        layer_width_px,
        layer_height_px,
        camera_view.as_slice(),
        data_unit_mode,
        aspect_ratio_mode,
        aspect_ratio_alignment_mode,
        None,
    );
    assert_eq!((size_out_x, size_out_y), (100.0_f32, 100.0_f32));
}

// Testing "cover" mode.
#[test]
fn test_wide_aspect_ratio_with_cover_mode_and_identity_camera_aram_end() {
    // Consider data points at the corners of a unit square.
    let points = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
    ];

    let camera_view = Mat4::identity();

    let layer_width_px = 200.0;
    let layer_height_px = 100.0;

    // When using a wide aspect ratio with "contain",
    // we expect to be viewing more data in the X direction.
    let aspect_ratio_mode = AspectRatioMode::Cover;
    let aspect_ratio_alignment_mode = AspectRatioAlignmentMode::End;
    let data_unit_mode = UnitsMode::Data;

    // These are in pixel space relative to the layer dimensions.
    let expected_points_ndc = vec![
        // Due to the "cover" aspect_ratio_mode,
        // the Y coordinates of the unit square will be outside of NDC.
        Vec2::new(0.0, -100.0),
        Vec2::new(0.0, 100.0),
        Vec2::new(200.0, -100.0),
        Vec2::new(200.0, 100.0),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        let (out_x, out_y) = get_point_position(
            point_pos_orig.x,
            point_pos_orig.y,
            // "uniforms"
            layer_width_px,
            layer_height_px,
            camera_view.as_slice(), // column-major order
            data_unit_mode,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            None,
        );
        return Vec2::new(out_x, out_y);
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);

    let (size_out_x, size_out_y) = get_point_size(
        1.0,
        1.0,
        layer_width_px,
        layer_height_px,
        camera_view.as_slice(),
        data_unit_mode,
        aspect_ratio_mode,
        aspect_ratio_alignment_mode,
        None,
    );
    assert_eq!((size_out_x, size_out_y), (200.0_f32, 200.0_f32));
}

#[test]
fn test_tall_aspect_ratio_with_cover_mode_and_identity_camera_aram_end() {
    // Consider data points at the corners of a unit square.
    let points = vec![
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
    ];

    let camera_view = Mat4::identity();

    let layer_width_px = 100.0;
    let layer_height_px = 200.0;

    // When using a tall aspect ratio with "cover",
    // we expect to be viewing less data in the X direction.
    let aspect_ratio_mode = AspectRatioMode::Cover;
    let aspect_ratio_alignment_mode = AspectRatioAlignmentMode::End;
    let data_unit_mode = UnitsMode::Data;

    // These are in pixel space relative to the layer dimensions.
    let expected_points_ndc = vec![
        // Due to the "cover" aspect_ratio_mode,
        // the Y coordinates of the unit square will be outside of NDC.
        Vec2::new(-100.0, 0.0),
        Vec2::new(-100.0, 200.0),
        Vec2::new(100.0, 0.0),
        Vec2::new(100.0, 200.0),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        let (out_x, out_y) = get_point_position(
            point_pos_orig.x,
            point_pos_orig.y,
            // "uniforms"
            layer_width_px,
            layer_height_px,
            camera_view.as_slice(), // column-major order
            data_unit_mode,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            None,
        );
        return Vec2::new(out_x, out_y);
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);

    let (size_out_x, size_out_y) = get_point_size(
        1.0,
        1.0,
        layer_width_px,
        layer_height_px,
        camera_view.as_slice(),
        data_unit_mode,
        aspect_ratio_mode,
        aspect_ratio_alignment_mode,
        None,
    );
    assert_eq!((size_out_x, size_out_y), (200.0_f32, 200.0_f32));
}


// ======== TESTING CAMERA ZOOM TRANSFORMS ========
// The "base" / easiest case: square aspect ratio, ignore mode, zero margins.
#[test]
fn test_square_aspect_ratio_with_ignore_mode_and_zoomed_in_2x_camera_aram_end() {
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

    let layer_width_px = 100.0;
    let layer_height_px = 100.0;

    let aspect_ratio_mode = AspectRatioMode::Ignore;
    let aspect_ratio_alignment_mode = AspectRatioAlignmentMode::End;
    let data_unit_mode = UnitsMode::Data;

    // These are in pixel space relative to the layer dimensions.
    let expected_points_ndc = vec![
        Vec2::new(-50.0, -50.0),
        Vec2::new(-50.0, 150.0),
        Vec2::new(150.0, -50.0),
        Vec2::new(150.0, 150.0),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        let (out_x, out_y) = get_point_position(
            point_pos_orig.x,
            point_pos_orig.y,
            // "uniforms"
            layer_width_px,
            layer_height_px,
            camera_view.as_slice(), // column-major order
            data_unit_mode,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            None,
        );
        return Vec2::new(out_x, out_y);
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);

    let (size_out_x, size_out_y) = get_point_size(
        1.0,
        1.0,
        layer_width_px,
        layer_height_px,
        camera_view.as_slice(),
        data_unit_mode,
        aspect_ratio_mode,
        aspect_ratio_alignment_mode,
        None,
    );
    assert_eq!((size_out_x, size_out_y), (200.0_f32, 200.0_f32));
}

#[test]
fn test_square_aspect_ratio_with_ignore_mode_and_zoomed_in_4x_camera_aram_end() {
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

    let layer_width_px = 100.0;
    let layer_height_px = 100.0;

    let aspect_ratio_mode = AspectRatioMode::Ignore;
    let aspect_ratio_alignment_mode = AspectRatioAlignmentMode::End;
    let data_unit_mode = UnitsMode::Data;

    // These are in pixel space relative to the layer dimensions.
    let expected_points_ndc = vec![
        Vec2::new(-150.0, -150.0),
        Vec2::new(-150.0, 250.0),
        Vec2::new(250.0, -150.0),
        Vec2::new(250.0, 250.0),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        let (out_x, out_y) = get_point_position(
            point_pos_orig.x,
            point_pos_orig.y,
            // "uniforms"
            layer_width_px,
            layer_height_px,
            camera_view.as_slice(), // column-major order
            data_unit_mode,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            None,
        );
        return Vec2::new(out_x, out_y);
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);

    let (size_out_x, size_out_y) = get_point_size(
        1.0,
        1.0,
        layer_width_px,
        layer_height_px,
        camera_view.as_slice(),
        data_unit_mode,
        aspect_ratio_mode,
        aspect_ratio_alignment_mode,
        None,
    );
    assert_eq!((size_out_x, size_out_y), (400.0_f32, 400.0_f32));
}

#[test]
fn test_square_aspect_ratio_with_ignore_mode_and_zoomed_out_2x_camera_aram_end() {
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

    let layer_width_px = 100.0;
    let layer_height_px = 100.0;

    let aspect_ratio_mode = AspectRatioMode::Ignore;
    let aspect_ratio_alignment_mode = AspectRatioAlignmentMode::End;
    let data_unit_mode = UnitsMode::Data;

    // These are in pixel space relative to the layer dimensions.
    let expected_points_ndc = vec![
        Vec2::new(25.0, 25.0),
        Vec2::new(25.0, 75.0),
        Vec2::new(75.0, 25.0),
        Vec2::new(75.0, 75.0),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        let (out_x, out_y) = get_point_position(
            point_pos_orig.x,
            point_pos_orig.y,
            // "uniforms"
            layer_width_px,
            layer_height_px,
            camera_view.as_slice(), // column-major order
            data_unit_mode,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            None,
        );
        return Vec2::new(out_x, out_y);
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);

    let (size_out_x, size_out_y) = get_point_size(
        1.0,
        1.0,
        layer_width_px,
        layer_height_px,
        camera_view.as_slice(),
        data_unit_mode,
        aspect_ratio_mode,
        aspect_ratio_alignment_mode,
        None,
    );
    assert_eq!((size_out_x, size_out_y), (50.0_f32, 50.0_f32));
}

#[test]
fn test_square_aspect_ratio_with_ignore_mode_and_zoomed_out_4x_camera_aram_end() {
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

    let layer_width_px = 100.0;
    let layer_height_px = 100.0;

    let aspect_ratio_mode = AspectRatioMode::Ignore;
    let aspect_ratio_alignment_mode = AspectRatioAlignmentMode::End;
    let data_unit_mode = UnitsMode::Data;

    // These are in pixel space relative to the layer dimensions.
    let expected_points_ndc = vec![
        Vec2::new(37.5, 37.5),
        Vec2::new(37.5, 62.5),
        Vec2::new(62.5, 37.5),
        Vec2::new(62.5, 62.5),
    ];

    let resulting_points_ndc: Vec<Vec2> = points.iter().map(|point_pos_orig| {
        let (out_x, out_y) = get_point_position(
            point_pos_orig.x,
            point_pos_orig.y,
            // "uniforms"
            layer_width_px,
            layer_height_px,
            camera_view.as_slice(), // column-major order
            data_unit_mode,
            aspect_ratio_mode,
            aspect_ratio_alignment_mode,
            None,
        );
        return Vec2::new(out_x, out_y);
    }).collect();

    assert_eq!(expected_points_ndc, resulting_points_ndc);

    let (size_out_x, size_out_y) = get_point_size(
        1.0,
        1.0,
        layer_width_px,
        layer_height_px,
        camera_view.as_slice(),
        data_unit_mode,
        aspect_ratio_mode,
        aspect_ratio_alignment_mode,
        None,
    );
    assert_eq!((size_out_x, size_out_y), (25.0_f32, 25.0_f32));
}
