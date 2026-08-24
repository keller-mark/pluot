//! Unit tests for `TransformationGraph`: building a graph from `ome`
//! attributes, resolving coordinate systems by name, and finding a path of
//! composed (and possibly inverted) transformations between two of them.

use pluot_zarr::ome_zarr_transformations::dag::{
    CoordinateSystemId, TransformationError, TransformationGraph,
};
use serde_json::{json, Value};

/// Build a graph from an `ome` attributes object containing only a scene,
/// mirroring the layout of the OME-Zarr transformations conformance cases.
fn scene_graph(scene: Value) -> TransformationGraph {
    TransformationGraph::from_ome_attributes(&json!({ "version": "0.6", "scene": scene }))
        .expect("parse scene")
}

/// Transform points from one named coordinate system to another, the
/// operation the conformance suite exercises.
fn transform(
    graph: &TransformationGraph,
    source: &str,
    target: &str,
    points: &[&[f64]],
) -> Result<Vec<Vec<f64>>, TransformationError> {
    let source = graph
        .resolve_name(source)
        .ok_or_else(|| TransformationError::UnknownCoordinateSystem(source.to_string()))?
        .clone();
    let target = graph
        .resolve_name(target)
        .ok_or_else(|| TransformationError::UnknownCoordinateSystem(target.to_string()))?
        .clone();
    let matrix = graph.transformation_between(&source, &target)?;
    points
        .iter()
        .map(|point| matrix.apply(point).map_err(TransformationError::Invalid))
        .collect()
}

fn assert_close(actual: &[Vec<f64>], expected: &[&[f64]]) {
    assert_eq!(actual.len(), expected.len(), "{actual:?} vs {expected:?}");
    for (a_point, e_point) in actual.iter().zip(expected) {
        assert_eq!(a_point.len(), e_point.len(), "{actual:?} vs {expected:?}");
        for (a, e) in a_point.iter().zip(e_point.iter()) {
            assert!(
                (a - e).abs() <= 1e-6 + 1e-3 * e.abs(),
                "{actual:?} vs {expected:?}",
            );
        }
    }
}

/// Two 2D coordinate systems named "input" and "output".
fn two_systems() -> Value {
    json!([
        { "name": "input", "axes": [
            { "name": "y", "type": "space" },
            { "name": "x", "type": "space" },
        ]},
        { "name": "output", "axes": [
            { "name": "y", "type": "space" },
            { "name": "x", "type": "space" },
        ]},
    ])
}

/// A single input-to-output transformation of the given type.
fn one_transformation(extra: Value) -> Value {
    let mut transformation = json!({
        "name": "inputToOutput",
        "input": { "name": "input" },
        "output": { "name": "output" },
    });
    let object = transformation.as_object_mut().unwrap();
    for (key, value) in extra.as_object().unwrap() {
        object.insert(key.clone(), value.clone());
    }
    json!({
        "coordinateSystems": two_systems(),
        "coordinateTransformations": [transformation],
    })
}

// Cases mirroring `ome_zarr_transformations_conformance/cases_config`.

#[test]
fn test_identity() {
    let graph = scene_graph(one_transformation(json!({ "type": "identity" })));
    assert_close(&transform(&graph, "input", "output", &[&[1.0, 2.0]]).unwrap(), &[&[1.0, 2.0]]);
    assert_close(&transform(&graph, "output", "input", &[&[1.0, 2.0]]).unwrap(), &[&[1.0, 2.0]]);
}

#[test]
fn test_scale() {
    let graph = scene_graph(one_transformation(json!({ "type": "scale", "scale": [10, 20] })));
    assert_close(
        &transform(&graph, "input", "output", &[&[1.0, 2.0]]).unwrap(),
        &[&[10.0, 40.0]],
    );
    assert_close(
        &transform(&graph, "output", "input", &[&[10.0, 40.0]]).unwrap(),
        &[&[1.0, 2.0]],
    );
}

#[test]
fn test_translation() {
    let graph =
        scene_graph(one_transformation(json!({ "type": "translation", "translation": [10, 20] })));
    assert_close(
        &transform(&graph, "input", "output", &[&[1.0, 2.0]]).unwrap(),
        &[&[11.0, 22.0]],
    );
    assert_close(
        &transform(&graph, "output", "input", &[&[11.0, 22.0]]).unwrap(),
        &[&[1.0, 2.0]],
    );
}

#[test]
fn test_map_axis() {
    let graph = scene_graph(one_transformation(json!({ "type": "mapAxis", "mapAxis": [1, 0] })));
    assert_close(&transform(&graph, "input", "output", &[&[1.0, 2.0]]).unwrap(), &[&[2.0, 1.0]]);
    assert_close(&transform(&graph, "output", "input", &[&[2.0, 1.0]]).unwrap(), &[&[1.0, 2.0]]);
}

#[test]
fn test_rotation() {
    let c = std::f64::consts::FRAC_1_SQRT_2;
    let graph = scene_graph(one_transformation(
        json!({ "type": "rotation", "rotation": [[c, -c], [c, c]] }),
    ));
    assert_close(
        &transform(&graph, "input", "output", &[&[-1.0, -1.0], &[0.0, 0.0], &[2.0, 2.0]]).unwrap(),
        &[&[0.0, -1.414_213_56], &[0.0, 0.0], &[0.0, 2.828_427_12]],
    );
    assert_close(
        &transform(&graph, "output", "input", &[&[0.0, -1.414_213_56]]).unwrap(),
        &[&[-1.0, -1.0]],
    );
}

#[test]
fn test_sequence() {
    let graph = scene_graph(one_transformation(json!({
        "type": "sequence",
        "transformations": [
            { "type": "scale", "scale": [10, 20] },
            { "type": "scale", "scale": [0.1, 0.05] },
        ],
    })));
    assert_close(&transform(&graph, "input", "output", &[&[1.0, 2.0]]).unwrap(), &[&[1.0, 2.0]]);
    assert_close(&transform(&graph, "output", "input", &[&[1.0, 2.0]]).unwrap(), &[&[1.0, 2.0]]);
}

#[test]
fn test_sequence_of_identity_and_translation() {
    // The dimensionality of a nested `identity` comes from the coordinate
    // system the sequence starts in.
    let graph = scene_graph(one_transformation(json!({
        "type": "sequence",
        "transformations": [
            { "type": "identity" },
            { "type": "translation", "translation": [1, 2] },
        ],
    })));
    assert_close(&transform(&graph, "input", "output", &[&[1.0, 2.0]]).unwrap(), &[&[2.0, 4.0]]);
}

#[test]
fn test_affine() {
    let graph = scene_graph(json!({
        "coordinateSystems": [
            { "name": "input", "axes": [
                { "name": "z", "type": "space" },
                { "name": "y", "type": "space" },
                { "name": "x", "type": "space" },
            ]},
            { "name": "output", "axes": [
                { "name": "z", "type": "space" },
                { "name": "y", "type": "space" },
                { "name": "x", "type": "space" },
            ]},
        ],
        "coordinateTransformations": [{
            "name": "inputToOutput",
            "input": { "name": "input" },
            "output": { "name": "output" },
            "type": "affine",
            "affine": [[2, 0, 0, 20], [0, 3, 0, 60], [0, 0, 4, 120]],
        }],
    }));
    let points: &[&[f64]] = &[&[-1.0, -1.0, -1.0], &[0.0, 0.0, 0.0], &[2.0, 2.0, 2.0]];
    let expected: &[&[f64]] = &[&[18.0, 57.0, 116.0], &[20.0, 60.0, 120.0], &[24.0, 66.0, 128.0]];
    assert_close(&transform(&graph, "input", "output", points).unwrap(), expected);
    assert_close(&transform(&graph, "output", "input", expected).unwrap(), points);
}

#[test]
fn test_affine_up_projection() {
    let graph = scene_graph(json!({
        "coordinateSystems": [
            { "name": "input", "axes": [
                { "name": "y", "type": "space" },
                { "name": "x", "type": "space" },
            ]},
            { "name": "output", "axes": [
                { "name": "z", "type": "space" },
                { "name": "y", "type": "space" },
                { "name": "x", "type": "space" },
            ]},
        ],
        "coordinateTransformations": [{
            "name": "input->output",
            "input": { "name": "input" },
            "output": { "name": "output" },
            "type": "affine",
            "affine": [[2, 0, 0], [0, 3, 0], [1, 1, 0]],
        }],
    }));
    assert_close(
        &transform(&graph, "input", "output", &[&[1.0, 2.0]]).unwrap(),
        &[&[2.0, 6.0, 3.0]],
    );
    // A projection cannot be inverted, so the reverse direction fails.
    assert!(matches!(
        transform(&graph, "output", "input", &[&[2.0, 6.0, 3.0]]),
        Err(TransformationError::NotInvertible { .. }),
    ));
}

#[test]
fn test_path_via_intermediate_system() {
    let graph = scene_graph(json!({
        "coordinateSystems": [
            { "name": "input", "axes": [{ "name": "y" }, { "name": "x" }] },
            { "name": "intermediate", "axes": [{ "name": "y" }, { "name": "x" }] },
            { "name": "output", "axes": [{ "name": "y" }, { "name": "x" }] },
        ],
        "coordinateTransformations": [
            {
                "name": "input->intermediate",
                "input": { "name": "input" },
                "output": { "name": "intermediate" },
                "type": "scale",
                "scale": [2, 4],
            },
            {
                "name": "intermediate->output",
                "input": { "name": "intermediate" },
                "output": { "name": "output" },
                "type": "translation",
                "translation": [1, 1],
            },
        ],
    }));
    assert_close(&transform(&graph, "input", "output", &[&[1.0, 2.0]]).unwrap(), &[&[3.0, 9.0]]);
    assert_close(&transform(&graph, "output", "input", &[&[3.0, 9.0]]).unwrap(), &[&[1.0, 2.0]]);
    // A partial path is also usable in either direction.
    assert_close(
        &transform(&graph, "output", "intermediate", &[&[3.0, 9.0]]).unwrap(),
        &[&[2.0, 8.0]],
    );
}

#[test]
fn test_path_mixing_forward_and_reverse_edges() {
    // Both transformations point at "output", so reaching "output" from
    // "other" requires traversing the first edge backwards.
    let graph = scene_graph(json!({
        "coordinateSystems": [
            { "name": "input", "axes": [{ "name": "y" }, { "name": "x" }] },
            { "name": "other", "axes": [{ "name": "y" }, { "name": "x" }] },
            { "name": "output", "axes": [{ "name": "y" }, { "name": "x" }] },
        ],
        "coordinateTransformations": [
            {
                "name": "input->other",
                "input": { "name": "input" },
                "output": { "name": "other" },
                "type": "scale",
                "scale": [2, 2],
            },
            {
                "name": "input->output",
                "input": { "name": "input" },
                "output": { "name": "output" },
                "type": "translation",
                "translation": [5, 5],
            },
        ],
    }));
    // other -> input halves, input -> output adds 5.
    assert_close(&transform(&graph, "other", "output", &[&[4.0, 6.0]]).unwrap(), &[&[7.0, 8.0]]);
}

#[test]
fn test_unknown_source_and_target_are_errors() {
    let graph = scene_graph(one_transformation(json!({ "type": "identity" })));
    assert!(matches!(
        transform(&graph, "not_input", "output", &[&[1.0, 2.0]]),
        Err(TransformationError::UnknownCoordinateSystem(_)),
    ));
    assert!(matches!(
        transform(&graph, "input", "not_output", &[&[1.0, 2.0]]),
        Err(TransformationError::UnknownCoordinateSystem(_)),
    ));
}

#[test]
fn test_disconnected_systems_have_no_path() {
    let graph = scene_graph(json!({
        "coordinateSystems": [
            { "name": "input", "axes": [{ "name": "y" }, { "name": "x" }] },
            { "name": "output", "axes": [{ "name": "y" }, { "name": "x" }] },
            { "name": "island", "axes": [{ "name": "y" }, { "name": "x" }] },
        ],
        "coordinateTransformations": [{
            "name": "inputToOutput",
            "input": { "name": "input" },
            "output": { "name": "output" },
            "type": "identity",
        }],
    }));
    assert!(matches!(
        transform(&graph, "input", "island", &[&[1.0, 2.0]]),
        Err(TransformationError::NoPath { .. }),
    ));
}

#[test]
fn test_unsupported_type_on_the_path_is_an_error() {
    let graph = scene_graph(one_transformation(json!({
        "type": "displacements",
        "path": "coordinateTransformations/inputToOutput",
        "interpolation": "linear",
    })));
    let error = transform(&graph, "input", "output", &[&[1.0, 2.0]]).unwrap_err();
    assert!(matches!(error, TransformationError::UnsupportedType { .. }));
    assert!(error.to_string().contains("displacements"), "{error}");
}

#[test]
fn test_unsupported_type_off_the_path_is_ignored() {
    // The unsupported edge leads somewhere we do not need to go, so it must
    // not prevent the supported edge from being used.
    let graph = scene_graph(json!({
        "coordinateSystems": [
            { "name": "input", "axes": [{ "name": "y" }, { "name": "x" }] },
            { "name": "output", "axes": [{ "name": "y" }, { "name": "x" }] },
            { "name": "warped", "axes": [{ "name": "y" }, { "name": "x" }] },
        ],
        "coordinateTransformations": [
            {
                "name": "input->warped",
                "input": { "name": "input" },
                "output": { "name": "warped" },
                "type": "displacements",
                "path": "coordinateTransformations/warp",
            },
            {
                "name": "input->output",
                "input": { "name": "input" },
                "output": { "name": "output" },
                "type": "scale",
                "scale": [10, 20],
            },
        ],
    }));
    assert_close(
        &transform(&graph, "input", "output", &[&[1.0, 2.0]]).unwrap(),
        &[&[10.0, 40.0]],
    );
}

#[test]
fn test_source_equals_target() {
    let graph = scene_graph(one_transformation(json!({ "type": "scale", "scale": [10, 20] })));
    assert_close(&transform(&graph, "input", "input", &[&[1.0, 2.0]]).unwrap(), &[&[1.0, 2.0]]);
}

// Cases covering `multiscales` metadata rather than a scene.

#[test]
fn test_multiscales_dataset_to_intrinsic() {
    let graph = TransformationGraph::from_ome_attributes(&json!({
        "version": "0.6",
        "multiscales": [{
            "coordinateSystems": [{ "name": "intrinsic", "axes": [
                { "name": "y", "type": "space", "unit": "micrometer" },
                { "name": "x", "type": "space", "unit": "micrometer" },
            ]}],
            "datasets": [
                {
                    "path": "0",
                    "coordinateTransformations": [{
                        "type": "scale",
                        "scale": [0.5, 0.5],
                        "input": { "path": "0" },
                        "output": { "name": "intrinsic" },
                    }],
                },
                {
                    "path": "1",
                    "coordinateTransformations": [{
                        "type": "sequence",
                        "input": { "path": "1" },
                        "output": { "name": "intrinsic" },
                        "transformations": [
                            { "type": "scale", "scale": [1.0, 1.0] },
                            { "type": "translation", "translation": [0.25, 0.25] },
                        ],
                    }],
                },
            ],
        }],
    }))
    .unwrap();

    let intrinsic = graph.resolve_name("intrinsic").unwrap().clone();
    assert_eq!(graph.coordinate_system(&intrinsic).unwrap().axes.len(), 2);

    // The array coordinate system of a dataset is named after its path.
    let level_0 = CoordinateSystemId::array("0");
    assert_eq!(graph.ndim(&level_0), Some(2));
    let matrix = graph.transformation_between(&level_0, &intrinsic).unwrap();
    assert_close(&[matrix.apply(&[10.0, 20.0]).unwrap()], &[&[5.0, 10.0]]);

    let level_1 = CoordinateSystemId::array("1");
    let matrix = graph.transformation_between(&level_1, &intrinsic).unwrap();
    assert_close(&[matrix.apply(&[10.0, 20.0]).unwrap()], &[&[10.25, 20.25]]);

    // Levels can also be related to each other, via the intrinsic system.
    let matrix = graph.transformation_between(&level_0, &level_1).unwrap();
    assert_close(&[matrix.apply(&[10.0, 20.0]).unwrap()], &[&[4.75, 9.75]]);
}

#[test]
fn test_multiscales_dataset_to_scene_system() {
    // A dataset reaches a scene-level coordinate system by way of the
    // intrinsic system declared on the multiscales entry.
    let graph = TransformationGraph::from_ome_attributes(&json!({
        "version": "0.6",
        "scene": {
            "coordinateSystems": [{ "name": "physical", "axes": [
                { "name": "y", "type": "space", "unit": "micrometer" },
                { "name": "x", "type": "space", "unit": "micrometer" },
            ]}],
            "coordinateTransformations": [{
                "name": "intrinsic->physical",
                "input": { "name": "intrinsic" },
                "output": { "name": "physical" },
                "type": "translation",
                "translation": [100, 200],
            }],
        },
        "multiscales": [{
            "coordinateSystems": [{ "name": "intrinsic", "axes": [
                { "name": "y", "type": "space", "unit": "micrometer" },
                { "name": "x", "type": "space", "unit": "micrometer" },
            ]}],
            "datasets": [{
                "path": "0",
                "coordinateTransformations": [{
                    "type": "scale",
                    "scale": [0.5, 0.5],
                    "input": { "path": "0" },
                    "output": { "name": "intrinsic" },
                }],
            }],
        }],
    }))
    .unwrap();

    let physical = graph.resolve_name("physical").unwrap().clone();
    let matrix = graph
        .transformation_between(&CoordinateSystemId::array("0"), &physical)
        .unwrap();
    assert_close(&[matrix.apply(&[10.0, 20.0]).unwrap()], &[&[105.0, 210.0]]);
}

#[test]
fn test_legacy_transformations_without_endpoints_are_not_edges() {
    // OME-Zarr v0.4/v0.5 dataset transformations have no input or output.
    let graph = TransformationGraph::from_ome_attributes(&json!({
        "version": "0.5",
        "multiscales": [{
            "axes": [{ "name": "y", "type": "space" }, { "name": "x", "type": "space" }],
            "datasets": [{
                "path": "0",
                "coordinateTransformations": [{ "type": "scale", "scale": [0.5, 0.5] }],
            }],
        }],
    }))
    .unwrap();
    assert!(graph.node_ids().is_empty());
    assert_eq!(graph.resolve_name("intrinsic"), None);
}
