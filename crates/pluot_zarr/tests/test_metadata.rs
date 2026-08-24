//! Unit tests for deserializing OME-Zarr RFC-5 `coordinateSystems` and
//! `coordinateTransformations` metadata.

use pluot_zarr::ome_zarr_transformations::metadata::{CoordinateTransformation, Scene, Transformation};
use serde_json::json;

#[test]
fn test_parse_scene() {
    // The conformance suite's `scale` case.
    let scene: Scene = serde_json::from_value(json!({
        "coordinateSystems": [
            { "name": "input", "axes": [
                { "name": "y", "type": "space" },
                { "name": "x", "type": "space" },
            ]},
            { "name": "output", "axes": [
                { "name": "y", "type": "space" },
                { "name": "x", "type": "space" },
            ]},
        ],
        "coordinateTransformations": [
            {
                "name": "inputToOutput",
                "input": { "name": "input" },
                "output": { "name": "output" },
                "type": "scale",
                "scale": [10, 20],
            },
        ],
    }))
    .unwrap();

    assert_eq!(scene.coordinate_systems.len(), 2);
    assert_eq!(scene.coordinate_systems[0].axes[1].name, "x");
    assert_eq!(
        scene.coordinate_systems[0].axes[1].axis_type.as_deref(),
        Some("space"),
    );

    let transform = &scene.coordinate_transformations[0];
    assert_eq!(transform.name.as_deref(), Some("inputToOutput"));
    assert_eq!(transform.input.as_ref().unwrap().name.as_deref(), Some("input"));
    match &transform.transformation {
        Transformation::Scale { scale } => assert_eq!(scale, &[10.0, 20.0]),
        other => panic!("expected a scale, got {other:?}"),
    }
}

#[test]
fn test_parse_nested_sequence() {
    let transform: CoordinateTransformation = serde_json::from_value(json!({
        "input": { "name": "input" },
        "output": { "name": "output" },
        "type": "sequence",
        "transformations": [
            { "type": "scale", "scale": [10, 20] },
            { "type": "translation", "translation": [1, 2] },
        ],
    }))
    .unwrap();

    match &transform.transformation {
        Transformation::Sequence { transformations } => {
            assert_eq!(transformations.len(), 2);
            assert!(transformations[0].input.is_none());
        }
        other => panic!("expected a sequence, got {other:?}"),
    }
}

#[test]
fn test_reference_accepts_object_and_string_forms() {
    let transform: CoordinateTransformation = serde_json::from_value(json!({
        "input": "input",
        "output": { "path": "0" },
        "type": "identity",
    }))
    .unwrap();

    let input = transform.input.unwrap();
    assert_eq!(input.name.as_deref(), Some("input"));
    assert_eq!(input.path, None);

    let output = transform.output.unwrap();
    assert_eq!(output.name, None);
    assert_eq!(output.path.as_deref(), Some("0"));
}

#[test]
fn test_unknown_type_is_unsupported_rather_than_an_error() {
    let transform: CoordinateTransformation = serde_json::from_value(json!({
        "name": "inputToOutput",
        "input": { "name": "input" },
        "output": { "name": "output" },
        "type": "displacements",
        "path": "coordinateTransformations/inputToOutput",
        "interpolation": "linear",
    }))
    .unwrap();

    assert_eq!(transform.type_name, "displacements");
    assert!(matches!(transform.transformation, Transformation::Unsupported));
}

#[test]
fn test_array_backed_parameters_are_unsupported_rather_than_an_error() {
    // A known type whose parameters live in a Zarr array rather than inline.
    let transform: CoordinateTransformation = serde_json::from_value(json!({
        "input": { "name": "input" },
        "output": { "name": "output" },
        "type": "affine",
        "path": "coordinateTransformations/inputToOutput",
    }))
    .unwrap();

    assert_eq!(transform.type_name, "affine");
    assert!(matches!(transform.transformation, Transformation::Unsupported));
}
