//! Coordinate transformation tests driven by the OME-Zarr RFC-5 example suite.
//!
//! Every `ome` attributes object below is inlined verbatim from
//! <https://github.com/jo-mueller/ngff-rfc5-coordinate-transformation-examples>
//! (commit `f5c30d0`), which the repository carries as the git submodule
//! `vendor/ngff-rfc5-coordinate-transformation-examples`. Each test names the
//! `.zarr` it came from, and array shapes quoted in the tests are the ones in
//! that example's array `zarr.json`.
//!
//! The metadata is inlined rather than read from the submodule so that the
//! tests describe the cases they cover, keep passing without the submodule
//! checked out, and stay pinned to the metadata they were written against.
//!
//! Reference: <https://ngff.openmicroscopy.org/rfc/5/index.html>

#![cfg(all(test, not(target_arch = "wasm32")))]

use pluot_zarr::layers::ome_zarr_utils::{
    model_matrix_pixel_size, target_coordinate_system_model_matrix, OmeDimensionOrder,
};
use pluot_zarr::ome_zarr_transformations::dag::{
    CoordinateSystemId, TransformationError, TransformationGraph,
};
use pluot_zarr::ome_zarr_transformations::metadata::CoordinateTransformation;
use serde_json::{json, Value};

// Helpers.

fn build_graph(ome: Value) -> TransformationGraph {
    TransformationGraph::from_ome_attributes(&ome).expect("build the transformation graph")
}

/// Map `point`, given in the array coordinate system of the dataset at
/// `array_path`, into the coordinate system named `target`.
///
/// The dimensionality is taken from the point rather than from the metadata,
/// the way the multiscale layer takes it from the Zarr array shape: an array
/// coordinate system does not declare its axes, so a dimension-agnostic
/// transformation such as `identity` leaves the graph nothing to infer from.
fn array_to_named(
    graph: &TransformationGraph,
    array_path: &str,
    target: &str,
    point: &[f64],
) -> Result<Vec<f64>, TransformationError> {
    let target = graph
        .resolve_name(target)
        .ok_or_else(|| TransformationError::UnknownCoordinateSystem(target.to_string()))?
        .clone();
    graph
        .transformation_between_with_ndim(
            &CoordinateSystemId::array(array_path),
            &target,
            point.len(),
        )?
        .apply(point)
        .map_err(TransformationError::Invalid)
}

/// Map `point` from the array coordinate system of one dataset into another's,
/// which is how resolution levels of a multiscale image are related to each
/// other: via the coordinate system they both map into.
fn array_to_array(
    graph: &TransformationGraph,
    from_path: &str,
    to_path: &str,
    point: &[f64],
) -> Vec<f64> {
    graph
        .transformation_between_with_ndim(
            &CoordinateSystemId::array(from_path),
            &CoordinateSystemId::array(to_path),
            point.len(),
        )
        .expect("a path between the two array coordinate systems")
        .apply(point)
        .expect("apply the transformation")
}

/// Map `point` from the coordinate system named `source` to the one named
/// `target`, both declared in the metadata.
fn named_to_named(
    graph: &TransformationGraph,
    source: &str,
    target: &str,
    point: &[f64],
) -> Result<Vec<f64>, TransformationError> {
    let source = graph
        .resolve_name(source)
        .ok_or_else(|| TransformationError::UnknownCoordinateSystem(source.to_string()))?
        .clone();
    let target = graph
        .resolve_name(target)
        .ok_or_else(|| TransformationError::UnknownCoordinateSystem(target.to_string()))?
        .clone();
    graph
        .transformation_between(&source, &target)?
        .apply(point)
        .map_err(TransformationError::Invalid)
}

/// The tolerance of the OME-Zarr transformations conformance suite.
fn assert_close(actual: &[f64], expected: &[f64]) {
    assert_eq!(actual.len(), expected.len(), "{actual:?} vs {expected:?}");
    for (a, e) in actual.iter().zip(expected) {
        assert!((a - e).abs() <= 1e-6 + 1e-3 * e.abs(), "{actual:?} vs {expected:?}");
    }
}

/// The axes of the coordinate system named `name`, for feeding to
/// [`target_coordinate_system_model_matrix`].
fn target_axes(
    graph: &TransformationGraph,
    name: &str,
) -> Vec<pluot_zarr::ome_zarr_transformations::metadata::CoordinateSystemAxis> {
    let id = graph.resolve_name(name).expect("the target coordinate system").clone();
    graph.coordinate_system(&id).expect("declared axes").axes.clone()
}

/// The model matrix the multiscale layer would build for one resolution level.
fn level_model_matrix(
    graph: &TransformationGraph,
    array_path: &str,
    target: &str,
    dimension_order: &str,
    level_height: u64,
    target_z: u64,
) -> [f32; 16] {
    let dimension_order = OmeDimensionOrder::try_from(dimension_order).unwrap();
    let axes = target_axes(graph, target);
    let target_id = graph.resolve_name(target).unwrap().clone();
    let transformation = graph
        .transformation_between_with_ndim(
            &CoordinateSystemId::array(array_path),
            &target_id,
            dimension_order.num_dims(),
        )
        .expect("transformation into the target coordinate system");
    target_coordinate_system_model_matrix(
        &transformation,
        &dimension_order,
        &axes,
        level_height,
        target_z,
        0,
    )
    .expect("model matrix")
}

/// Apply a column-major 4x4 model matrix to a Y-up pixel coordinate, as the
/// renderer does.
fn apply_model_matrix(matrix: &[f32; 16], px: f64, py: f64) -> Vec<f64> {
    vec![
        matrix[0] as f64 * px + matrix[4] as f64 * py + matrix[12] as f64,
        matrix[1] as f64 * px + matrix[5] as f64 * py + matrix[13] as f64,
    ]
}

/// Two 2D `space` axes in micrometers, the shape every 2D example uses.
fn yx_micrometer_axes() -> Value {
    json!([
        { "type": "space", "name": "y", "unit": "micrometer", "discrete": false },
        { "type": "space", "name": "x", "unit": "micrometer", "discrete": false },
    ])
}

/// Three 3D `space` axes in micrometers.
fn zyx_micrometer_axes() -> Value {
    json!([
        { "type": "space", "name": "z", "unit": "micrometer", "discrete": false },
        { "type": "space", "name": "y", "unit": "micrometer", "discrete": false },
        { "type": "space", "name": "x", "unit": "micrometer", "discrete": false },
    ])
}

// 2d/basic — cases every implementation is required to support.

/// `2d/basic/identity.zarr`, whose array is 576x720.
#[test]
fn test_2d_basic_identity() {
    let graph = build_graph(json!({
        "version": "0.6",
        "multiscales": [{
            "name": "multiscales",
            "coordinateSystems": [{ "name": "physical", "axes": yx_micrometer_axes() }],
            "datasets": [{
                "path": "array",
                "coordinateTransformations": [{
                    "type": "identity",
                    "output": { "name": "physical" },
                    "input": { "path": "array" },
                    "name": "transform-name",
                }],
            }],
        }],
    }));

    assert_close(&array_to_named(&graph, "array", "physical", &[10.0, 20.0]).unwrap(), &[10.0, 20.0]);

    // An `identity` carries no dimensionality, and an array coordinate system
    // does not declare its axes, so the graph cannot infer one here: the
    // dimensionality has to come from the array shape, which is why the
    // multiscale layer uses `transformation_between_with_ndim`.
    let array = CoordinateSystemId::array("array");
    assert_eq!(graph.ndim(&array), None);
    let physical = graph.resolve_name("physical").unwrap().clone();
    assert!(matches!(
        graph.transformation_between(&array, &physical),
        Err(TransformationError::UnknownDimensionality(_)),
    ));

    // The image occupies 576x720 micrometers, top-left corner at world (0, 0),
    // with world Y increasing upwards.
    let matrix = level_model_matrix(&graph, "array", "physical", "YX", 576, 0);
    assert_close(&apply_model_matrix(&matrix, 0.0, 576.0), &[0.0, 0.0]);
    assert_close(&apply_model_matrix(&matrix, 720.0, 0.0), &[720e-6, -576e-6]);
    assert_close(&model_matrix_pixel_size(&matrix), &[1e-6, 1e-6]);
}

/// `2d/basic/scale.zarr`, whose array is 576x720.
#[test]
fn test_2d_basic_scale() {
    let graph = build_graph(json!({
        "version": "0.6",
        "multiscales": [{
            "name": "multiscales",
            "coordinateSystems": [{ "name": "physical", "axes": yx_micrometer_axes() }],
            "datasets": [{
                "path": "array",
                "coordinateTransformations": [{
                    "type": "scale",
                    "output": { "name": "physical" },
                    "input": { "path": "array" },
                    "name": "transform-name",
                    "scale": [3.0, 2.0],
                }],
            }],
        }],
    }));

    assert_close(&array_to_named(&graph, "array", "physical", &[10.0, 20.0]).unwrap(), &[30.0, 40.0]);
    // Here the `scale` parameters do give the graph the dimensionality.
    assert_eq!(graph.ndim(&CoordinateSystemId::array("array")), Some(2));

    let matrix = level_model_matrix(&graph, "array", "physical", "YX", 576, 0);
    // The scale is in micrometers, so one pixel is 3x2 micrometers.
    assert_close(&model_matrix_pixel_size(&matrix), &[3e-6, 2e-6]);
    assert_close(&apply_model_matrix(&matrix, 0.0, 576.0), &[0.0, 0.0]);
    assert_close(&apply_model_matrix(&matrix, 720.0, 0.0), &[1440e-6, -1728e-6]);
}

/// `2d/basic/scale_multiscale.zarr`, whose levels are 576x720, 288x360 and
/// 144x180.
#[test]
fn test_2d_basic_scale_multiscale() {
    let level = |path: &str, scale: [f64; 2]| {
        json!({
            "path": path,
            "coordinateTransformations": [{
                "type": "scale",
                "output": { "name": "physical" },
                "input": { "path": path },
                "name": "transform-name",
                "scale": scale,
            }],
        })
    };
    let graph = build_graph(json!({
        "version": "0.6",
        "multiscales": [{
            "name": "multiscales",
            "coordinateSystems": [{ "name": "physical", "axes": yx_micrometer_axes() }],
            "datasets": [
                level("s0", [6.0, 4.0]),
                level("s1", [12.0, 8.0]),
                level("s2", [24.0, 16.0]),
            ],
        }],
    }));

    // Every level covers the same physical extent, which is the property the
    // renderer relies on when it swaps one level for another.
    let shapes = [("s0", [576.0, 720.0]), ("s1", [288.0, 360.0]), ("s2", [144.0, 180.0])];
    for (path, shape) in shapes {
        assert_close(&array_to_named(&graph, path, "physical", &[0.0, 0.0]).unwrap(), &[0.0, 0.0]);
        assert_close(
            &array_to_named(&graph, path, "physical", &shape).unwrap(),
            &[3456.0, 2880.0],
        );
    }

    // Levels are also relatable directly, by way of `physical`.
    assert_close(&array_to_array(&graph, "s2", "s0", &[10.0, 10.0]), &[40.0, 40.0]);
    assert_close(&array_to_array(&graph, "s0", "s2", &[40.0, 40.0]), &[10.0, 10.0]);

    // Each level's pixel size doubles, and the levels share a world origin.
    for (path, shape) in shapes {
        let matrix = level_model_matrix(&graph, path, "physical", "YX", shape[0] as u64, 0);
        let factor = 576.0 / shape[0];
        assert_close(&model_matrix_pixel_size(&matrix), &[6e-6 * factor, 4e-6 * factor]);
        assert_close(&apply_model_matrix(&matrix, 0.0, shape[0]), &[0.0, 0.0]);
        assert_close(&apply_model_matrix(&matrix, shape[1], 0.0), &[2880e-6, -3456e-6]);
    }
}

/// `2d/basic/sequenceScaleTranslation.zarr`.
#[test]
fn test_2d_basic_sequence_scale_translation() {
    let graph = build_graph(json!({
        "version": "0.6",
        "multiscales": [{
            "name": "multiscales",
            "coordinateSystems": [{ "name": "physical", "axes": yx_micrometer_axes() }],
            "datasets": [{
                "path": "array",
                "coordinateTransformations": [{
                    "type": "sequence",
                    "output": { "name": "physical" },
                    "input": { "path": "array" },
                    "transformations": [
                        { "type": "scale", "scale": [3.0, 2.0] },
                        { "type": "translation", "translation": [30.0, 20.0], "name": "" },
                    ],
                    "name": "transform-name",
                }],
            }],
        }],
    }));

    // The sequence scales before translating.
    assert_close(&array_to_named(&graph, "array", "physical", &[10.0, 20.0]).unwrap(), &[60.0, 60.0]);
    assert_close(&array_to_named(&graph, "array", "physical", &[0.0, 0.0]).unwrap(), &[30.0, 20.0]);
    // And is invertible, so `physical` can be mapped back to array indices.
    let physical = graph.resolve_name("physical").unwrap().clone();
    let inverse = graph
        .transformation_between_with_ndim(&physical, &CoordinateSystemId::array("array"), 2)
        .unwrap();
    assert_close(&inverse.apply(&[60.0, 60.0]).unwrap(), &[10.0, 20.0]);

    // The translation offsets the image in world space: the top-left corner of
    // a 576-row level sits at target (y=30, x=20), i.e. world (20, -30) µm.
    let matrix = level_model_matrix(&graph, "array", "physical", "YX", 576, 0);
    assert_close(&apply_model_matrix(&matrix, 0.0, 576.0), &[20e-6, -30e-6]);
    assert_close(&model_matrix_pixel_size(&matrix), &[3e-6, 2e-6]);
}

/// `2d/basic/sequenceScaleTranslation_multiscale.zarr`, whose per-level
/// translations implement the pixel-centre convention for downsampling.
#[test]
fn test_2d_basic_sequence_scale_translation_multiscale() {
    let level = |path: &str, scale: [f64; 2], translation: [f64; 2]| {
        json!({
            "path": path,
            "coordinateTransformations": [{
                "type": "sequence",
                "output": { "name": "physical" },
                "input": { "path": path },
                "transformations": [
                    { "type": "scale", "scale": scale },
                    { "type": "translation", "translation": translation, "name": "" },
                ],
                "name": "transform-name",
            }],
        })
    };
    let graph = build_graph(json!({
        "version": "0.6",
        "multiscales": [{
            "name": "multiscales",
            "coordinateSystems": [{ "name": "physical", "axes": yx_micrometer_axes() }],
            "datasets": [
                level("s0", [3.0, 2.0], [0.0, 0.0]),
                level("s1", [6.0, 4.0], [1.5, 1.0]),
                level("s2", [12.0, 8.0], [4.5, 3.0]),
            ],
        }],
    }));

    // One s1 pixel covers two s0 pixels, and its centre falls between them:
    // s1 index i maps to s0 index 2i + 0.5. Likewise s2 index i maps to
    // 4i + 1.5. Getting the translations wrong would show up as levels that
    // drift against each other as the renderer switches between them.
    assert_close(&array_to_array(&graph, "s1", "s0", &[0.0, 0.0]), &[0.5, 0.5]);
    assert_close(&array_to_array(&graph, "s1", "s0", &[3.0, 7.0]), &[6.5, 14.5]);
    assert_close(&array_to_array(&graph, "s2", "s0", &[0.0, 0.0]), &[1.5, 1.5]);
    assert_close(&array_to_array(&graph, "s2", "s1", &[2.0, 2.0]), &[4.5, 4.5]);

    assert_close(&array_to_named(&graph, "s0", "physical", &[10.0, 20.0]).unwrap(), &[30.0, 40.0]);
    assert_close(&array_to_named(&graph, "s1", "physical", &[10.0, 20.0]).unwrap(), &[61.5, 81.0]);
}

// 2d/simple — affines and subsets of affines.

/// `2d/simple/affine.zarr`, whose array is 576x720. The dataset maps into
/// `physical` and a shared `affine` shears `physical` into `sheared`, so
/// reaching `sheared` from the array needs both edges composed.
#[test]
fn test_2d_simple_affine() {
    let graph = build_graph(json!({
        "version": "0.6",
        "multiscales": [{
            "name": "multiscales",
            "coordinateSystems": [
                { "name": "sheared", "axes": yx_micrometer_axes() },
                { "name": "physical", "axes": yx_micrometer_axes() },
            ],
            "datasets": [{
                "path": "array",
                "coordinateTransformations": [{
                    "type": "scale",
                    "output": { "name": "physical" },
                    "input": { "path": "array" },
                    "name": "array2physical",
                    "scale": [1, 1],
                }],
            }],
            "coordinateTransformations": [{
                "type": "affine",
                "output": { "name": "sheared" },
                "input": { "name": "physical" },
                "name": "shear-transformation",
                "affine": [[3, 0.4, 30], [0.3, 2, 20]],
            }],
        }],
    }));

    assert_close(&array_to_named(&graph, "array", "physical", &[10.0, 20.0]).unwrap(), &[10.0, 20.0]);
    assert_close(&array_to_named(&graph, "array", "sheared", &[10.0, 20.0]).unwrap(), &[68.0, 63.0]);
    // The shear is invertible, so `sheared` maps back to array indices.
    assert_close(&named_to_named(&graph, "sheared", "physical", &[68.0, 63.0]).unwrap(), &[10.0, 20.0]);

    // Rendering into `sheared` rotates and shears the image: stepping one pixel
    // along the array's X axis moves along both world axes, so the pixel size
    // is the length of the model matrix column rather than a single coefficient.
    let matrix = level_model_matrix(&graph, "array", "sheared", "YX", 576, 0);
    assert_close(&model_matrix_pixel_size(&matrix), &[3.014_962_686e-6, 2.039_607_805e-6]);
    // Array (row 0, column 0) maps to sheared (y=30, x=20), i.e. world (20, -30).
    assert_close(&apply_model_matrix(&matrix, 0.0, 576.0), &[20e-6, -30e-6]);
    // Stepping one column adds (0.3, 2) in `sheared`, i.e. (2, -0.3) in world.
    assert_close(&apply_model_matrix(&matrix, 1.0, 576.0), &[22e-6, -30.3e-6]);
    // Stepping one array row adds (3, 0.4), i.e. (0.4, -3) in world.
    assert_close(&apply_model_matrix(&matrix, 0.0, 575.0), &[20.4e-6, -33e-6]);
}

/// `2d/simple/multiscale.zarr` and `2d/simple/affine_multiscale.zarr`. The two
/// declare the same datasets; the latter adds a shared `affine` from `physical`
/// to `sheared`, and its `sheared` transformation is the one in
/// `2d/simple/affine.zarr`.
// The per-level translations are quoted from the examples, where they happen to
// be truncations of 1/sqrt(2) and 3/sqrt(2).
#[allow(clippy::approx_constant)]
#[test]
fn test_2d_simple_affine_multiscale_extends_multiscale() {
    let datasets = json!([
        {
            "path": "s0",
            "coordinateTransformations": [{
                "type": "sequence",
                "output": { "name": "physical" },
                "input": { "path": "s0" },
                "name": "scale0_to_physical",
                "transformations": [
                    { "type": "scale", "scale": [1, 1] },
                    { "type": "translation", "translation": [0, 0] },
                ],
            }],
        },
        {
            "path": "s1",
            "coordinateTransformations": [{
                "type": "sequence",
                "output": { "name": "physical" },
                "input": { "path": "s1" },
                "name": "scale1_to_physical",
                "transformations": [
                    { "type": "scale", "scale": [2, 2] },
                    { "type": "translation", "translation": [0.7071, 0.7071] },
                ],
            }],
        },
        {
            "path": "s2",
            "coordinateTransformations": [{
                "type": "sequence",
                "output": { "name": "physical" },
                "input": { "path": "s2" },
                "name": "scale2_to_physical",
                "transformations": [
                    { "type": "scale", "scale": [4, 4] },
                    { "type": "translation", "translation": [2.1213, 2.1213] },
                ],
            }],
        },
    ]);

    let multiscale = build_graph(json!({
        "version": "0.6",
        "multiscales": [{
            "name": "multiscales",
            "coordinateSystems": [{ "name": "physical", "axes": yx_micrometer_axes() }],
            "datasets": datasets,
        }],
    }));
    let affine_multiscale = build_graph(json!({
        "version": "0.6",
        "multiscales": [{
            "name": "multiscales",
            "coordinateSystems": [
                { "name": "sheared", "axes": yx_micrometer_axes() },
                { "name": "physical", "axes": yx_micrometer_axes() },
            ],
            "datasets": datasets,
            "coordinateTransformations": [{
                "type": "affine",
                "name": "physical-to-sheared",
                "input": { "name": "physical" },
                "output": { "name": "sheared" },
                "affine": [[3, 0.4, 30], [0.3, 2, 20]],
            }],
        }],
    }));

    // The shared transformation does not disturb the levels' own mapping.
    for path in ["s0", "s1", "s2"] {
        let point = [10.0, 20.0];
        assert_close(
            &array_to_named(&affine_multiscale, path, "physical", &point).unwrap(),
            &array_to_named(&multiscale, path, "physical", &point).unwrap(),
        );
    }
    // s0 is the identity into `physical`, so reaching `sheared` from it applies
    // exactly the shear of `2d/simple/affine.zarr`.
    assert_close(
        &array_to_named(&affine_multiscale, "s0", "sheared", &[10.0, 20.0]).unwrap(),
        &[68.0, 63.0],
    );
    // `sheared` is unreachable in `multiscale.zarr`, where it is not declared.
    assert!(matches!(
        array_to_named(&multiscale, "s0", "sheared", &[10.0, 20.0]),
        Err(TransformationError::UnknownCoordinateSystem(_)),
    ));
}

/// `2d/simple/rotation.zarr`. The rotation is about the origin, as the example
/// suite's README notes.
#[test]
fn test_2d_simple_rotation() {
    let graph = build_graph(json!({
        "version": "0.6",
        "multiscales": [{
            "name": "multiscales",
            "coordinateSystems": [
                { "name": "rotated", "axes": yx_micrometer_axes() },
                { "name": "physical", "axes": yx_micrometer_axes() },
            ],
            "datasets": [{
                "path": "array",
                "coordinateTransformations": [{
                    "type": "scale",
                    "output": { "name": "physical" },
                    "input": { "path": "array" },
                    "scale": [1, 1],
                    "name": "array to physical",
                }],
            }],
            "coordinateTransformations": [{
                "name": "rotation",
                "input": { "name": "physical" },
                "output": { "name": "rotated" },
                "type": "rotation",
                "rotation": [[0, 1], [-1, 0]],
            }],
        }],
    }));

    // A quarter turn about the origin: (y, x) becomes (x, -y).
    assert_close(&array_to_named(&graph, "array", "rotated", &[1.0, 0.0]).unwrap(), &[0.0, -1.0]);
    assert_close(&array_to_named(&graph, "array", "rotated", &[0.0, 1.0]).unwrap(), &[1.0, 0.0]);
    assert_close(&array_to_named(&graph, "array", "rotated", &[10.0, 20.0]).unwrap(), &[20.0, -10.0]);
    // A rotation is orthonormal, so traversing the edge backwards works.
    assert_close(&named_to_named(&graph, "rotated", "physical", &[20.0, -10.0]).unwrap(), &[10.0, 20.0]);

    // A rotation preserves pixel size.
    let matrix = level_model_matrix(&graph, "array", "rotated", "YX", 576, 0);
    assert_close(&model_matrix_pixel_size(&matrix), &[1e-6, 1e-6]);
    // Array (row 0, column 0) is at the target origin. Stepping one array
    // column moves along the target's positive Y — the quarter turn sends the
    // array's X axis to the target's Y — and world Y is the negation of that.
    assert_close(&apply_model_matrix(&matrix, 0.0, 576.0), &[0.0, 0.0]);
    assert_close(&apply_model_matrix(&matrix, 1.0, 576.0), &[0.0, -1e-6]);
    // Stepping one array row moves along the target's negative X.
    assert_close(&apply_model_matrix(&matrix, 0.0, 575.0), &[-1e-6, 0.0]);
}

/// `2d/simple/affineParams.zarr` and `2d/simple/rotationParams.zarr`, which
/// keep their transformation parameters in a Zarr array referenced by `path`
/// instead of inline. Pluot does not read those arrays, so the edge is
/// unsupported — but only for callers that need to cross it.
#[test]
fn test_2d_simple_transformation_parameters_from_an_array_are_unsupported() {
    let graph = build_graph(json!({
        "version": "0.6",
        "multiscales": [{
            "name": "multiscales",
            "coordinateSystems": [
                { "name": "sheared", "axes": yx_micrometer_axes() },
                { "name": "physical", "axes": yx_micrometer_axes() },
            ],
            "datasets": [{
                "path": "array",
                "coordinateTransformations": [{
                    "type": "scale",
                    "output": { "name": "physical" },
                    "input": { "path": "array" },
                    "name": "array-to-physical",
                    "scale": [0.5, 0.5],
                }],
            }],
            "coordinateTransformations": [{
                "type": "affine",
                "input": { "name": "physical" },
                "output": { "name": "sheared" },
                "path": "affineParams",
                "name": "shearing-transform",
            }],
        }],
    }));

    // `physical` is still reachable, so the example renders in that coordinate
    // system even though the shear cannot be applied.
    assert_close(&array_to_named(&graph, "array", "physical", &[10.0, 20.0]).unwrap(), &[5.0, 10.0]);

    let error = array_to_named(&graph, "array", "sheared", &[10.0, 20.0]).unwrap_err();
    assert!(
        matches!(&error, TransformationError::UnsupportedType { type_name, .. } if type_name == "affine"),
        "{error:?}",
    );
    // The error names the transformation, so the message points at the metadata.
    assert!(error.to_string().contains("shearing-transform"), "{error}");

    // The `rotation` equivalent, whose target coordinate system also happens to
    // declare its axes in (x, y) rather than (y, x) order.
    let graph = build_graph(json!({
        "version": "0.6",
        "multiscales": [{
            "name": "multiscales",
            "coordinateSystems": [
                { "name": "rotated", "axes": [
                    { "type": "space", "name": "x", "unit": "micrometer", "discrete": false },
                    { "type": "space", "name": "y", "unit": "micrometer", "discrete": false },
                ]},
                { "name": "physical", "axes": yx_micrometer_axes() },
            ],
            "datasets": [{
                "path": "array",
                "coordinateTransformations": [{
                    "type": "scale",
                    "output": { "name": "physical" },
                    "input": { "path": "array" },
                    "scale": [1.4, 1.4],
                }],
            }],
            "coordinateTransformations": [{
                "type": "rotation",
                "output": { "name": "rotated" },
                "path": "rotationParams",
                "input": { "name": "physical" },
                "name": "image-rotation",
            }],
        }],
    }));
    assert_close(&array_to_named(&graph, "array", "physical", &[10.0, 20.0]).unwrap(), &[14.0, 28.0]);
    assert!(matches!(
        array_to_named(&graph, "array", "rotated", &[10.0, 20.0]),
        Err(TransformationError::UnsupportedType { .. }),
    ));
}

// 2d/axis_dependent — transformations that depend on the input axes.

/// `2d/axis_dependent/mapAxis.zarr`, which permutes the array's two axes on the
/// way into `physical`.
#[test]
fn test_2d_axis_dependent_map_axis() {
    let ome = json!({
        "version": "0.6",
        "multiscales": [{
            "name": "multiscales",
            "coordinateSystems": [
                { "name": "physical", "axes": yx_micrometer_axes() },
                { "name": "array_coordinates", "axes": [
                    { "type": "space", "name": "dim_0", "discrete": true },
                    { "type": "space", "name": "dim_1", "discrete": true },
                ]},
            ],
            "datasets": [{
                "path": "array",
                "coordinateTransformations": [{
                    "type": "scale",
                    "output": { "name": "array_coordinates" },
                    "input": { "path": "array" },
                    "name": "default-scale",
                    "scale": [1, 1],
                }],
            }],
            "coordinateTransformations": [{
                "type": "mapAxis",
                "output": { "name": "physical" },
                "input": { "name": "array_coordinates" },
                "name": "transform-name",
                "mapAxis": [1, 0],
            }],
        }],
    });
    let graph = build_graph(ome.clone());

    // The array's first axis becomes `physical`'s x and vice versa.
    assert_close(&array_to_named(&graph, "array", "physical", &[10.0, 20.0]).unwrap(), &[20.0, 10.0]);
    // A permutation is invertible, so the edge is traversable both ways.
    assert_close(
        &named_to_named(&graph, "physical", "array_coordinates", &[20.0, 10.0]).unwrap(),
        &[10.0, 20.0],
    );

    // The model matrix picks the target axes out by name, so a transposing
    // transformation shows up as a transposed model matrix: stepping one array
    // column moves along world Y, not world X.
    let matrix = level_model_matrix(&graph, "array", "physical", "YX", 576, 0);
    assert_close(&apply_model_matrix(&matrix, 0.0, 576.0), &[0.0, 0.0]);
    assert_close(&apply_model_matrix(&matrix, 1.0, 576.0), &[0.0, -1e-6]);
    assert_close(&apply_model_matrix(&matrix, 0.0, 575.0), &[1e-6, 0.0]);

    // Known limitation: the multiscale layer derives its dimension order from
    // the first letter of each axis name of the coordinate system the datasets
    // map into. Here that is `array_coordinates`, whose axes are `dim_0` and
    // `dim_1`, so the derivation cannot work and the layer would reject this
    // example even though the graph handles it.
    let dataset_target = &ome["multiscales"][0]["coordinateSystems"][1];
    let derived: String = dataset_target["axes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|axis| axis["name"].as_str().unwrap().chars().next().unwrap())
        .collect();
    assert_eq!(derived, "dd");
    assert!(OmeDimensionOrder::try_from(derived.as_str()).is_err());
}

/// `2d/axis_dependent/byDimension.zarr`. `byDimension` applies a different
/// transformation to each subset of the axes and cannot be reduced to one
/// matrix here, so the edge into `y-scaled` is unsupported.
#[test]
fn test_2d_axis_dependent_by_dimension_is_unsupported() {
    let graph = build_graph(json!({
        "version": "0.6",
        "multiscales": [{
            "name": "multiscales",
            "coordinateSystems": [
                { "name": "physical", "axes": yx_micrometer_axes() },
                { "name": "y-scaled", "axes": [
                    { "type": "space", "name": "y", "unit": "nanometer", "discrete": false },
                    { "type": "space", "name": "x", "unit": "micrometer", "discrete": false },
                ]},
            ],
            "datasets": [{
                "path": "s0",
                "coordinateTransformations": [{
                    "type": "identity",
                    "output": { "name": "physical" },
                    "input": { "path": "s0" },
                    "name": "transform-name",
                }],
            }],
            "coordinateTransformations": [{
                "type": "byDimension",
                "output": { "name": "y-scaled" },
                "input": { "name": "physical" },
                "transformations": [{
                    "transformation": { "type": "scale", "scale": [1000] },
                    "output_axes": [1],
                    "input_axes": [1],
                }],
                "name": "transform-name",
            }],
        }],
    }));

    // `byDimension` nests a `transformations` list, as `sequence` does, so this
    // also checks that it is not mistaken for one.
    let error = array_to_named(&graph, "s0", "y-scaled", &[10.0, 20.0]).unwrap_err();
    assert!(
        matches!(&error, TransformationError::UnsupportedType { type_name, .. } if type_name == "byDimension"),
        "{error:?}",
    );
    // The rest of the graph is unaffected.
    assert_close(&array_to_named(&graph, "s0", "physical", &[10.0, 20.0]).unwrap(), &[10.0, 20.0]);
}

// 3d.

/// `3d/basic/scale.zarr`, whose `physical` axes declare no `discrete` field.
#[test]
fn test_3d_basic_scale() {
    let graph = build_graph(json!({
        "version": "0.6",
        "multiscales": [{
            "name": "multiscales",
            "coordinateSystems": [{ "name": "physical", "axes": [
                { "type": "space", "name": "z", "unit": "micrometer" },
                { "type": "space", "name": "y", "unit": "micrometer" },
                { "type": "space", "name": "x", "unit": "micrometer" },
            ]}],
            "datasets": [{
                "path": "array",
                "coordinateTransformations": [{
                    "type": "scale",
                    "output": { "name": "physical" },
                    "input": { "path": "array" },
                    "name": "transform-name",
                    "scale": [4, 3, 2],
                }],
            }],
        }],
    }));

    assert_close(
        &array_to_named(&graph, "array", "physical", &[1.0, 2.0, 3.0]).unwrap(),
        &[4.0, 6.0, 6.0],
    );
    // The renderer draws one Z slice at a time, so the Z scale does not enter
    // the model matrix, but it does move the slice along the target Z axis.
    let matrix = level_model_matrix(&graph, "array", "physical", "ZYX", 100, 0);
    assert_close(&model_matrix_pixel_size(&matrix), &[3e-6, 2e-6]);
    assert_close(&apply_model_matrix(&matrix, 0.0, 100.0), &[0.0, 0.0]);
}

/// `3d/axis_dependent/mapAxis.zarr`, which reverses the three axes.
#[test]
fn test_3d_axis_dependent_map_axis() {
    let graph = build_graph(json!({
        "version": "0.6",
        "multiscales": [{
            "name": "multiscales",
            "coordinateSystems": [
                { "name": "physical", "axes": zyx_micrometer_axes() },
                { "name": "permuted", "axes": zyx_micrometer_axes() },
            ],
            "datasets": [{
                "path": "0",
                "coordinateTransformations": [{
                    "type": "identity",
                    "output": { "name": "physical" },
                    "input": { "path": "0" },
                }],
            }],
            "coordinateTransformations": [{
                "type": "mapAxis",
                "output": { "name": "permuted" },
                "input": { "name": "physical" },
                "name": "transform-name",
                "mapAxis": [2, 1, 0],
            }],
        }],
    }));

    assert_close(
        &array_to_named(&graph, "0", "permuted", &[1.0, 2.0, 3.0]).unwrap(),
        &[3.0, 2.0, 1.0],
    );
    assert_close(
        &named_to_named(&graph, "permuted", "physical", &[3.0, 2.0, 1.0]).unwrap(),
        &[1.0, 2.0, 3.0],
    );
}

/// `3d/simple/affine.zarr`, whose array is 27x226x186. The shear mixes the
/// array's Z axis into the target X and Y, so the slice being rendered changes
/// where the image sits.
#[test]
fn test_3d_simple_affine() {
    let graph = build_graph(json!({
        "version": "0.6",
        "multiscales": [{
            "name": "multiscales",
            "coordinateSystems": [
                { "name": "sheared", "axes": zyx_micrometer_axes() },
                { "name": "physical", "axes": zyx_micrometer_axes() },
            ],
            "datasets": [{
                "path": "array",
                "coordinateTransformations": [{
                    "type": "scale",
                    "output": { "name": "physical" },
                    "input": { "path": "array" },
                    "name": "array-to-physical",
                    "scale": [1, 1, 1],
                }],
            }],
            "coordinateTransformations": [{
                "type": "affine",
                "output": { "name": "sheared" },
                "input": { "name": "physical" },
                "name": "physical-to-sheared",
                "affine": [
                    [4, 0.8, 0.6, 30],
                    [0.8, 3, 0.4, 20],
                    [0.1, 0.3, 2, 10],
                ],
            }],
        }],
    }));

    assert_close(
        &array_to_named(&graph, "array", "sheared", &[1.0, 2.0, 3.0]).unwrap(),
        &[37.4, 28.0, 16.7],
    );
    // The shear is invertible, so `sheared` maps back to `physical`.
    assert_close(
        &named_to_named(&graph, "sheared", "physical", &[37.4, 28.0, 16.7]).unwrap(),
        &[1.0, 2.0, 3.0],
    );

    // Array (row 0, column 0) of slice z maps to sheared (x = 10 + 0.1 z,
    // y = 20 + 0.8 z), i.e. world (10 + 0.1 z, -(20 + 0.8 z)) micrometers.
    for z in [0u64, 5, 26] {
        let matrix = level_model_matrix(&graph, "array", "sheared", "ZYX", 226, z);
        assert_close(
            &apply_model_matrix(&matrix, 0.0, 226.0),
            &[(10.0 + 0.1 * z as f64) * 1e-6, -(20.0 + 0.8 * z as f64) * 1e-6],
        );
        // The in-plane part of the shear does not depend on the slice.
        assert_close(&model_matrix_pixel_size(&matrix), &[3.014_962_686e-6, 2.039_607_805e-6]);
    }
}

/// `3d/nonlinear/invDisplacements.zarr` and `3d/nonlinear/coordinates.zarr`.
/// Neither transformation type is closed-form, and `invDisplacements` also
/// declares its edge in the direction the renderer needs to traverse backwards.
#[test]
fn test_3d_nonlinear_fields_are_unsupported() {
    let nonlinear = |transformation: Value| {
        build_graph(json!({
            "version": "0.6",
            "multiscales": [{
                "name": "multiscales",
                "coordinateSystems": [
                    { "name": "displaced", "axes": zyx_micrometer_axes() },
                    { "name": "physical", "axes": [
                        { "type": "space", "name": "z", "discrete": false },
                        { "type": "space", "name": "y", "discrete": false },
                        { "type": "space", "name": "x", "discrete": false },
                    ]},
                ],
                "datasets": [{
                    "path": "0",
                    "coordinateTransformations": [{
                        "type": "scale",
                        "output": { "name": "physical" },
                        "input": { "path": "0" },
                        "scale": [0.5, 0.5, 0.5],
                        "name": "scale-transform",
                    }],
                }],
                "coordinateTransformations": [transformation],
            }],
        }))
    };

    // `invDisplacements` maps `displaced` into `physical`, so reaching
    // `displaced` from the array means traversing that edge backwards. The
    // unsupported type is reported before invertibility is even considered.
    let graph = nonlinear(json!({
        "type": "displacements",
        "output": { "name": "physical" },
        "input": { "name": "displaced" },
        "path": "displacementField",
        "interpolation": "linear",
        "name": "inverse-dfield",
    }));
    let error = array_to_named(&graph, "0", "displaced", &[1.0, 2.0, 3.0]).unwrap_err();
    assert!(
        matches!(&error, TransformationError::UnsupportedType { type_name, .. } if type_name == "displacements"),
        "{error:?}",
    );
    // `physical` is reachable, so the image still renders unregistered.
    assert_close(
        &array_to_named(&graph, "0", "physical", &[1.0, 2.0, 3.0]).unwrap(),
        &[0.5, 1.0, 1.5],
    );

    // `coordinates.zarr` expresses the same registration the other way round.
    let graph = nonlinear(json!({
        "type": "coordinates",
        "output": { "name": "displaced" },
        "input": { "name": "physical" },
        "path": "coordinatesField",
        "interpolation": "linear",
        "name": "inverse-cfield",
    }));
    let error = array_to_named(&graph, "0", "displaced", &[1.0, 2.0, 3.0]).unwrap_err();
    assert!(
        matches!(&error, TransformationError::UnsupportedType { type_name, .. } if type_name == "coordinates"),
        "{error:?}",
    );

    // Axes without a unit still resolve, defaulting to micrometers, so the
    // `physical` coordinate system of these examples is usable as a target.
    let matrix = level_model_matrix(&graph, "0", "physical", "ZYX", 100, 0);
    assert_close(&model_matrix_pixel_size(&matrix), &[0.5e-6, 0.5e-6]);
}

// user_stories.

/// Add one Zarr node's `ome` metadata to `graph`, qualifying every coordinate
/// system reference with the node's `path` relative to the group that owns the
/// graph.
///
/// [`TransformationGraph::from_ome_attributes`] reads a single Zarr node, so
/// the scene-level references in the `user_stories` examples — which look like
/// `{"path": "tile_0", "name": "physical"}` — do not resolve on their own: the
/// coordinate system they name is declared in `tile_0/zarr.json`, where it is
/// simply `"physical"`. Stitching several nodes into one graph therefore means
/// prefixing the subgroup's own references, which is what this does. Pluot has
/// no built-in equivalent yet.
fn add_node_under_path(graph: &mut TransformationGraph, path: &str, ome: &Value) {
    let qualify = |reference: &Value| match reference.get("path").and_then(Value::as_str) {
        // A reference to a Zarr array, whose path is also relative to the node.
        Some(nested) => match reference.get("name") {
            Some(name) => json!({ "path": format!("{path}/{nested}"), "name": name }),
            None => json!({ "path": format!("{path}/{nested}") }),
        },
        // A reference to a named coordinate system declared by the node itself.
        None => json!({ "path": path, "name": reference["name"] }),
    };

    let mut transformations = Vec::new();
    for multiscale in ome["multiscales"].as_array().into_iter().flatten() {
        let datasets = multiscale["datasets"].as_array().into_iter().flatten();
        let own = multiscale["coordinateTransformations"].as_array().into_iter().flatten();
        for owner in datasets.chain([multiscale]).chain(own) {
            for transformation in
                owner["coordinateTransformations"].as_array().into_iter().flatten()
            {
                let mut transformation = transformation.clone();
                transformation["input"] = qualify(&transformation["input"]);
                transformation["output"] = qualify(&transformation["output"]);
                transformations.push(transformation);
            }
        }
    }
    let transformations: Vec<CoordinateTransformation> =
        serde_json::from_value(Value::Array(transformations)).expect("parse transformations");
    graph.add_transformations(transformations);
}

/// A `user_stories/stitched_tiles_2d.zarr` tile: a 300x372 array mapping into
/// the tile's own `physical` coordinate system at one micrometer per pixel.
fn stitched_tile_ome(name: &str) -> Value {
    json!({
        "version": "0.6",
        "multiscales": [{
            "name": "multiscales",
            "coordinateSystems": [{ "name": "physical", "axes": yx_micrometer_axes() }],
            "datasets": [{
                "path": "0",
                "coordinateTransformations": [{
                    "type": "scale",
                    "output": { "name": "physical" },
                    "input": { "path": "0" },
                    "name": format!("{name} to physical"),
                    "scale": [1, 1],
                }],
            }],
        }],
    })
}

/// `user_stories/stitched_tiles_2d.zarr`: four 300x372 tiles, each translated
/// into a shared `world` coordinate system.
#[test]
fn test_user_stories_stitched_tiles_2d() {
    let tile_translation = |name: &str, translation: [f64; 2]| {
        json!({
            "type": "translation",
            "output": { "name": "world" },
            "input": { "path": name, "name": "physical" },
            "translation": translation,
            "name": format!("{name}_mm to world"),
        })
    };
    let scene = json!({
        "version": "0.6",
        "scene": {
            "coordinateTransformations": [
                tile_translation("tile_0", [0.0, 0.0]),
                tile_translation("tile_1", [0.0, 348.0]),
                tile_translation("tile_2", [276.0, 0.0]),
                tile_translation("tile_3", [276.0, 348.0]),
            ],
            // Note that the example declares `world` with its axes in (x, y)
            // order, while every tile's `physical` is (y, x). The translations
            // are applied componentwise, so component 0 lands on `world`'s
            // first axis whatever it is called; the assertions below are in
            // component order rather than by axis name.
            "coordinateSystems": [{ "name": "world", "axes": [
                { "type": "space", "name": "x", "unit": "micrometer", "discrete": false },
                { "type": "space", "name": "y", "unit": "micrometer", "discrete": false },
            ]}],
        },
    });

    // The scene alone knows only about the tiles' `physical` coordinate systems
    // as opaque references; it cannot reach any array.
    let scene_only = build_graph(scene.clone());
    assert!(matches!(
        array_to_named(&scene_only, "tile_0/0", "world", &[0.0, 0.0]),
        Err(TransformationError::UnknownCoordinateSystem(_)),
    ));

    // Adding each tile's own metadata under its path joins the two halves.
    let mut graph = build_graph(scene);
    for tile in ["tile_0", "tile_1", "tile_2", "tile_3"] {
        add_node_under_path(&mut graph, tile, &stitched_tile_ome(tile));
    }

    // Each tile's top-left pixel sits at its own offset in `world`, and the
    // 276x348 offsets overlap the 300x372 tiles, which is the point of the
    // example: adjacent tiles share a strip.
    let corners = [
        ("tile_0", [0.0, 0.0]),
        ("tile_1", [0.0, 348.0]),
        ("tile_2", [276.0, 0.0]),
        ("tile_3", [276.0, 348.0]),
    ];
    for (tile, corner) in corners {
        let array = format!("{tile}/0");
        assert_close(&array_to_named(&graph, &array, "world", &[0.0, 0.0]).unwrap(), &corner);
        assert_close(
            &array_to_named(&graph, &array, "world", &[300.0, 372.0]).unwrap(),
            &[corner[0] + 300.0, corner[1] + 372.0],
        );
    }

    // Tiles can also be related to each other, through `world`: the pixel of
    // tile_0 that overlaps tile_1's (0, 0) is 348 columns to the right.
    assert_close(
        &array_to_array(&graph, "tile_1/0", "tile_0/0", &[0.0, 0.0]),
        &[0.0, 348.0],
    );
    assert_close(
        &array_to_array(&graph, "tile_3/0", "tile_0/0", &[10.0, 10.0]),
        &[286.0, 358.0],
    );
}

/// `user_stories/SCAPE.zarr`, whose `stack` subgroup deskews a two-level
/// pyramid with a shear along the Y axis.
#[test]
fn test_user_stories_scape_deskewing() {
    let stack = json!({
        "version": "0.6",
        "multiscales": [{
            "coordinateSystems": [
                { "name": "unskewed", "axes": zyx_micrometer_axes() },
                { "name": "physical", "axes": [
                    { "type": "space", "name": "z", "unit": "micrometer" },
                    { "type": "space", "name": "y", "unit": "micrometer" },
                    { "type": "space", "name": "x", "unit": "micrometer" },
                ]},
            ],
            "datasets": [
                {
                    "path": "scale0",
                    "coordinateTransformations": [{
                        "name": "scale0 to physical",
                        "type": "sequence",
                        "input": { "path": "scale0" },
                        "output": { "name": "physical" },
                        "transformations": [
                            { "type": "scale", "scale": [1.0, 0.3245, 0.3245] },
                            { "type": "translation", "translation": [0.0, 0.16225, 0.16225] },
                        ],
                    }],
                },
                {
                    "path": "scale1",
                    "coordinateTransformations": [{
                        "type": "sequence",
                        "name": "scale1 to physical",
                        "input": { "path": "scale1" },
                        "output": { "name": "physical" },
                        "transformations": [
                            { "type": "scale", "scale": [2.0, 0.649, 0.649] },
                            { "type": "translation", "translation": [0.5, 0.16225, 0.16225] },
                        ],
                    }],
                },
            ],
            "coordinateTransformations": [{
                "name": "deskewing",
                "input": { "name": "physical" },
                "output": { "name": "unskewed" },
                "type": "affine",
                "affine": [
                    [1.0, 0.0, 0.0, 0.0],
                    [0.0, 1.0, 0.838_950_16, 0.0],
                    [0.0, 0.0, 1.0, 0.0],
                ],
            }],
        }],
    });
    let graph = build_graph(stack.clone());

    // scale0 voxel (10, 100, 200) is at physical (10, 32.61225, 65.06225), and
    // the deskew shears X into Y.
    assert_close(
        &array_to_named(&graph, "scale0", "physical", &[10.0, 100.0, 200.0]).unwrap(),
        &[10.0, 32.61225, 65.06225],
    );
    assert_close(
        &array_to_named(&graph, "scale0", "unskewed", &[10.0, 100.0, 200.0]).unwrap(),
        &[10.0, 87.196_235_047, 65.06225],
    );
    // The two levels are relatable through `physical`. Along Z the example uses
    // the pixel-centre convention, so scale1 voxel i sits between scale0 voxels
    // 2i and 2i + 1. Along Y and X it does not: scale1 repeats scale0's
    // 0.16225 offset instead of doubling it to 0.3245, so the levels are
    // corner-aligned in-plane and scale1 lands exactly on an even scale0 index.
    // That looks like an oversight in the example rather than intent, and it
    // would show as a half-voxel in-plane shift when the renderer swaps levels.
    assert_close(&array_to_array(&graph, "scale1", "scale0", &[0.0, 0.0, 0.0]), &[0.5, 0.0, 0.0]);
    assert_close(&array_to_array(&graph, "scale1", "scale0", &[5.0, 50.0, 50.0]), &[10.5, 100.0, 100.0]);

    // The shear does not change the in-plane pixel size along X, but it does
    // stretch the displacement produced by stepping one column.
    let matrix = level_model_matrix(&graph, "scale0", "unskewed", "ZYX", 600, 0);
    let [scale_y, scale_x] = model_matrix_pixel_size(&matrix);
    assert_close(&[scale_y], &[0.3245e-6]);
    assert_close(&[scale_x], &[0.3245 * (1.0 + 0.838_950_16_f64.powi(2)).sqrt() * 1e-6]);

    // The scene positions `stack`'s `unskewed` in a `world` coordinate system,
    // but declares a two-component translation for a three-dimensional input,
    // so composing across that edge is rejected rather than silently
    // mis-positioning the stack. This is a defect in the example metadata.
    let scene = json!({
        "version": "0.6",
        "scene": {
            "coordinateTransformations": [{
                "type": "translation",
                "output": { "name": "world" },
                "input": { "path": "stack", "name": "unskewed" },
                "translation": [5882.2, 44249.4],
                "name": "stack to world",
            }],
            "coordinateSystems": [{ "name": "world", "axes": [
                { "type": "space", "name": "x", "unit": "micrometer", "discrete": false },
                { "type": "space", "name": "y", "unit": "micrometer", "discrete": false },
            ]}],
        },
    });
    let mut combined = build_graph(scene);
    add_node_under_path(&mut combined, "stack", &stack);
    let error = array_to_named(&combined, "stack/scale0", "world", &[0.0, 0.0, 0.0]).unwrap_err();
    assert!(matches!(&error, TransformationError::Invalid(_)), "{error:?}");
    assert!(error.to_string().contains("stack to world"), "{error}");
}

/// `user_stories/image_registration_3d.zarr`. A `bijection` stores the forward
/// and inverse of a non-invertible registration explicitly; Pluot represents
/// neither, so the edge is unsupported in both directions.
#[test]
fn test_user_stories_image_registration_3d_bijection_is_unsupported() {
    let graph = build_graph(json!({
        "version": "0.6",
        "scene": {
            "coordinateTransformations": [{
                "type": "bijection",
                "input": { "path": "JRC2018F", "name": "physical" },
                "output": { "path": "FCWB", "name": "physical" },
                "forward": {
                    "type": "sequence",
                    "name": "JRC2018F to FCWB",
                    "transformations": [
                        {
                            "type": "displacements",
                            "path": "coordinateTransformations/dfield",
                            "interpolation": "linear",
                        },
                        {
                            "type": "affine",
                            "affine": [
                                [0.549687, -0.0138092, 0.000127526, 2.9986],
                                [0.0893289, 1.04339, -0.000121014, -6.39702],
                                [0.00779285, 0.00299018, 0.907875, -3.77146],
                            ],
                        },
                    ],
                },
                "inverse": {
                    "type": "sequence",
                    "name": "FCWB to JRC2018F",
                    "transformations": [
                        {
                            "type": "affine",
                            "affine": [
                                [1.8153162032371448, 0.024026315573955494, -0.00025178851007148946, -5.290659956068192],
                                [-0.1554184181171034, 0.9563570184920926, 0.00014930742384645888, 6.584435749976974],
                                [-0.015070089856986017, -0.003356093187801388, 1.1014748899286995, 4.177888664571422],
                            ],
                        },
                        {
                            "type": "displacements",
                            "path": "coordinateTransformations/invdfield",
                            "interpolation": "linear",
                            "name": "",
                        },
                    ],
                },
            }],
        },
    }));

    // Both endpoints are cross-node references, and the edge between them is a
    // type Pluot cannot reduce to a matrix.
    let source = CoordinateSystemId { path: Some("JRC2018F".into()), name: "physical".into() };
    let target = CoordinateSystemId { path: Some("FCWB".into()), name: "physical".into() };
    assert!(graph.contains(&source) && graph.contains(&target));
    for (from, to) in [(&source, &target), (&target, &source)] {
        let error = graph.transformation_between_with_ndim(from, to, 3).unwrap_err();
        assert!(
            matches!(&error, TransformationError::UnsupportedType { type_name, .. } if type_name == "bijection"),
            "{error:?}",
        );
    }
}

/// `user_stories/lens_correction.zarr`: a `byDimension` wrapping a 2D
/// `displacements` field applied to every slice of a 3D image.
#[test]
fn test_user_stories_lens_correction_is_unsupported() {
    let graph = build_graph(json!({
        "version": "0.6",
        "scene": {
            "coordinateTransformations": [{
                "type": "byDimension",
                "input": { "path": "image", "name": "raw" },
                "output": { "name": "corrected" },
                "transformations": [
                    {
                        "transformation": {
                            "type": "displacements",
                            "path": "coordinateTransformations/lensCorrection",
                        },
                        "input_axes": [1, 2],
                        "output_axes": [1, 2],
                    },
                    {
                        "transformation": { "type": "identity" },
                        "input_axes": [0],
                        "output_axes": [0],
                    },
                ],
                "name": "lens correction 3d",
            }],
            "coordinateSystems": [{ "name": "corrected", "axes": [
                { "type": "space", "name": "z", "unit": "nanometer", "discrete": false },
                { "type": "space", "name": "y", "unit": "nanometer", "discrete": false },
                { "type": "space", "name": "x", "unit": "nanometer", "discrete": false },
            ]}],
        },
    }));

    let raw = CoordinateSystemId { path: Some("image".into()), name: "raw".into() };
    let corrected = graph.resolve_name("corrected").expect("corrected").clone();
    let error = graph.transformation_between_with_ndim(&raw, &corrected, 3).unwrap_err();
    assert!(
        matches!(&error, TransformationError::UnsupportedType { type_name, .. } if type_name == "byDimension"),
        "{error:?}",
    );
    assert!(error.to_string().contains("lens correction 3d"), "{error}");

    // The target coordinate system is in nanometers, so a renderer that did
    // support the correction would scale accordingly.
    let axes = target_axes(&graph, "corrected");
    assert_eq!(axes[2].unit.as_deref(), Some("nanometer"));
}
