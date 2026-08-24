//! Unit tests for `pluot_zarr::layers::ome_zarr_utils`: the OME-NGFF
//! dimension order type, upgrading legacy (v0.4/v0.5) multiscale metadata into
//! the v0.6 shape, and building the pixel-to-world model matrix for a
//! resolution level.

use pluot_zarr::layers::ome_zarr_utils::{
    axis_unit_to_meters, model_matrix_pixel_size, target_coordinate_system_model_matrix,
    upgrade_ome_multiscales, OmeDim, OmeDimensionOrder, INTRINSIC_COORDINATE_SYSTEM,
};
use pluot_zarr::ome_zarr_transformations::affine::AffineMatrix;
use pluot_zarr::ome_zarr_transformations::dag::{CoordinateSystemId, TransformationGraph};
use pluot_zarr::ome_zarr_transformations::metadata::CoordinateSystemAxis;

#[test]
fn test_ome_dim_order_new() {
    let order = OmeDimensionOrder::new(vec![OmeDim::T, OmeDim::Z, OmeDim::C, OmeDim::Y, OmeDim::X]);
    assert_eq!(order.num_dims(), 5);
    assert_eq!(order.index_of(OmeDim::X), Some(4));
    assert_eq!(order.index_of(OmeDim::T), Some(0));
    assert!(order.has_dim(OmeDim::C));
    assert_eq!(order.to_string(), "TZCYX");
}

#[test]
fn test_ome_dim_order_from_str() {
    let order = OmeDimensionOrder::try_from("CZYX").unwrap();
    assert_eq!(order.num_dims(), 4);
    assert_eq!(order.index_of(OmeDim::C), Some(0));
    assert_eq!(order.index_of(OmeDim::Z), Some(1));
    assert_eq!(order.index_of(OmeDim::Y), Some(2));
    assert_eq!(order.index_of(OmeDim::X), Some(3));
    assert!(!order.has_dim(OmeDim::T));
    assert_eq!(order.to_string(), "CZYX");
}

#[test]
fn test_ome_dim_order_lowercase() {
    // Lowercase input is accepted; order is preserved, output is uppercase.
    let order = OmeDimensionOrder::try_from("tczyx").unwrap();
    assert_eq!(order.to_string(), "TCZYX");
}

#[test]
fn test_ome_dim_order_into_string() {
    let order = OmeDimensionOrder::new(vec![OmeDim::C, OmeDim::Y, OmeDim::X]);
    let s: String = order.into();
    assert_eq!(s, "CYX");
}

#[test]
fn test_ome_dim_order_err_no_x() {
    assert!(OmeDimensionOrder::try_from("CY").is_err());
}

#[test]
fn test_ome_dim_order_err_no_y() {
    assert!(OmeDimensionOrder::try_from("CX").is_err());
}

#[test]
fn test_ome_dim_order_err_duplicate() {
    assert!(OmeDimensionOrder::try_from("XYXY").is_err());
}

#[test]
fn test_ome_dim_order_err_invalid_char() {
    assert!(OmeDimensionOrder::try_from("AXY").is_err());
}

#[test]
#[should_panic]
fn test_ome_dim_order_new_panics_on_duplicate() {
    OmeDimensionOrder::new(vec![OmeDim::X, OmeDim::Y, OmeDim::X]);
}

/// Axes for a target coordinate system, all in micrometers.
fn micrometer_axes(names: &[&str]) -> Vec<CoordinateSystemAxis> {
    names
        .iter()
        .map(|name| {
            serde_json::from_value(serde_json::json!({
                "name": name,
                "type": "space",
                "unit": "micrometer",
            }))
            .unwrap()
        })
        .collect()
}

/// Apply a column-major 4x4 model matrix to a Y-up pixel coordinate.
fn apply_model_matrix(matrix: &[f32; 16], px: f64, py: f64) -> (f64, f64) {
    (
        matrix[0] as f64 * px + matrix[4] as f64 * py + matrix[12] as f64,
        matrix[1] as f64 * px + matrix[5] as f64 * py + matrix[13] as f64,
    )
}

/// The model matrix is built in f32, so compare with a tolerance that is
/// loose enough for f32 rounding but tight enough to catch a wrong
/// coefficient or sign. The absolute floor covers the cases where an
/// expected value of zero comes out of cancelling terms of the order of the
/// image extent; 0.01 nanometers is far below any meaningful precision here.
fn assert_close(actual: (f64, f64), expected: (f64, f64)) {
    let close = |a: f64, e: f64| (a - e).abs() <= 1e-6 * e.abs() + 1e-11;
    assert!(
        close(actual.0, expected.0) && close(actual.1, expected.1),
        "{actual:?} vs {expected:?}",
    );
}

/// Build the transformation from a dataset's array coordinate system to the
/// intrinsic one, the way the multiscale layer does after upgrading.
fn intrinsic_transformation(ome: serde_json::Value, path: &str, ndim: usize) -> AffineMatrix {
    let mut ome = ome;
    upgrade_ome_multiscales(&mut ome);
    let graph = TransformationGraph::from_ome_attributes(&ome).expect("build graph");
    let intrinsic = graph.resolve_name(INTRINSIC_COORDINATE_SYSTEM).expect("intrinsic").clone();
    graph
        .transformation_between_with_ndim(&CoordinateSystemId::array(path), &intrinsic, ndim)
        .expect("transformation to intrinsic")
}

/// A v0.4/v0.5 `multiscales` entry with the given dataset and multiscale
/// level transformations.
fn legacy_ome(
    dataset_transformations: serde_json::Value,
    shared_transformations: serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "version": "0.5",
        "multiscales": [{
            "axes": [
                { "name": "y", "type": "space", "unit": "micrometer" },
                { "name": "x", "type": "space", "unit": "micrometer" },
            ],
            "datasets": [{ "path": "0", "coordinateTransformations": dataset_transformations }],
            "coordinateTransformations": shared_transformations,
        }],
    })
}

#[test]
fn test_upgrade_declares_the_intrinsic_coordinate_system() {
    let mut ome = legacy_ome(
        serde_json::json!([{ "type": "scale", "scale": [0.5, 0.25] }]),
        serde_json::json!([]),
    );
    upgrade_ome_multiscales(&mut ome);

    // The v0.4/v0.5 `axes` list becomes a named coordinate system.
    let systems = &ome["multiscales"][0]["coordinateSystems"];
    assert_eq!(systems.as_array().unwrap().len(), 1);
    assert_eq!(systems[0]["name"], INTRINSIC_COORDINATE_SYSTEM);
    assert_eq!(systems[0]["axes"][0]["name"], "y");
    assert_eq!(systems[0]["axes"][1]["unit"], "micrometer");

    // The dataset transformation gains the endpoints it was missing.
    let transformation = &ome["multiscales"][0]["datasets"][0]["coordinateTransformations"][0];
    assert_eq!(transformation["type"], "scale");
    assert_eq!(transformation["input"]["path"], "0");
    assert_eq!(transformation["output"]["name"], INTRINSIC_COORDINATE_SYSTEM);
}

#[test]
fn test_upgrade_preserves_transformation_order() {
    // v0.4 applies a dataset's transformations in order, then the
    // multiscale-wide ones, so the composed result must scale before
    // translating and translate before the shared scale.
    let ome = legacy_ome(
        serde_json::json!([
            { "type": "scale", "scale": [2.0, 2.0] },
            { "type": "translation", "translation": [1.0, 1.0] },
        ]),
        serde_json::json!([{ "type": "scale", "scale": [10.0, 10.0] }]),
    );
    let transformation = intrinsic_transformation(ome, "0", 2);
    // (3, 4) -> (6, 8) -> (7, 9) -> (70, 90)
    assert_eq!(transformation.apply(&[3.0, 4.0]).unwrap(), vec![70.0, 90.0]);
}

#[test]
fn test_upgrade_gives_datasets_without_transformations_an_identity() {
    let ome = legacy_ome(serde_json::json!([]), serde_json::json!([]));
    let transformation = intrinsic_transformation(ome, "0", 2);
    assert_eq!(transformation.apply(&[3.0, 4.0]).unwrap(), vec![3.0, 4.0]);
}

#[test]
fn test_upgrade_leaves_v0_6_metadata_unchanged() {
    let original = serde_json::json!({
        "version": "0.6",
        "multiscales": [{
            "coordinateSystems": [{ "name": "intrinsic", "axes": [
                { "name": "y", "type": "space", "unit": "micrometer" },
                { "name": "x", "type": "space", "unit": "micrometer" },
            ]}],
            "datasets": [{
                "path": "0",
                "coordinateTransformations": [{
                    "type": "sequence",
                    "input": { "path": "0" },
                    "output": { "name": "intrinsic" },
                    "transformations": [{ "type": "scale", "scale": [0.5, 0.5] }],
                }],
            }],
        }],
    });
    let mut upgraded = original.clone();
    upgrade_ome_multiscales(&mut upgraded);
    assert_eq!(upgraded, original);
}

#[test]
fn test_upgrade_ignores_metadata_without_multiscales() {
    let original = serde_json::json!({ "version": "0.6", "scene": { "coordinateSystems": [] } });
    let mut upgraded = original.clone();
    upgrade_ome_multiscales(&mut upgraded);
    assert_eq!(upgraded, original);
}

#[test]
fn test_upgraded_legacy_metadata_reproduces_the_level_pixel_size() {
    // The pixel size the multiscale layer derives for a level must match the
    // dataset's declared scale, converted from micrometers to meters.
    let ome = legacy_ome(
        serde_json::json!([{ "type": "scale", "scale": [0.5, 0.25] }]),
        serde_json::json!([]),
    );
    let transformation = intrinsic_transformation(ome, "0", 2);
    let matrix = target_coordinate_system_model_matrix(
        &transformation,
        &OmeDimensionOrder::try_from("YX").unwrap(),
        &micrometer_axes(&["y", "x"]),
        100,
        0,
        0,
    )
    .unwrap();
    let [scale_y, scale_x] = model_matrix_pixel_size(&matrix);
    assert_close((scale_x, scale_y), (0.25e-6, 0.5e-6));
}

#[test]
fn test_axis_unit_to_meters() {
    assert_eq!(axis_unit_to_meters(Some("micrometer")).unwrap(), 1e-6);
    assert_eq!(axis_unit_to_meters(Some("millimeter")).unwrap(), 1e-3);
    assert_eq!(axis_unit_to_meters(Some("meter")).unwrap(), 1.0);
    // Axes without a unit are treated as micrometers.
    assert_eq!(axis_unit_to_meters(None).unwrap(), 1e-6);
    assert!(axis_unit_to_meters(Some("furlong")).is_err());
}

#[test]
fn test_target_model_matrix_matches_the_scale_only_case() {
    // A plain per-axis scale, which is what OME-Zarr v0.4/v0.5 datasets use.
    let transformation = AffineMatrix::from_scale(&[1.0, 0.5, 0.25]);
    let order = OmeDimensionOrder::try_from("CYX").unwrap();
    let matrix = target_coordinate_system_model_matrix(
        &transformation,
        &order,
        &micrometer_axes(&["c", "y", "x"]),
        100,
        0,
        0,
    )
    .unwrap();

    // World X is the pixel column times the X scale.
    assert_close(apply_model_matrix(&matrix, 0.0, 0.0), (0.0, -0.5 * 100.0 * 1e-6));
    assert_close(apply_model_matrix(&matrix, 4.0, 0.0), (1e-6, -0.5 * 100.0 * 1e-6));
    // The top of the image (Y-up row 100, i.e. array row 0) is at world Y 0,
    // and the bottom of the image is one image height below it.
    assert_close(apply_model_matrix(&matrix, 0.0, 100.0), (0.0, 0.0));
    let [scale_y, scale_x] = model_matrix_pixel_size(&matrix);
    assert_close((scale_x, scale_y), (0.25e-6, 0.5e-6));
}

#[test]
fn test_target_model_matrix_applies_translation_and_units() {
    let transformation = AffineMatrix::from_affine(&[
        vec![0.5, 0.0, 10.0],
        vec![0.0, 0.25, 20.0],
    ])
    .unwrap();
    let order = OmeDimensionOrder::try_from("YX").unwrap();
    let matrix = target_coordinate_system_model_matrix(
        &transformation,
        &order,
        &micrometer_axes(&["y", "x"]),
        100,
        0,
        0,
    )
    .unwrap();

    // Array row 0, column 0 maps to target (y=10, x=20), i.e. world (20, -10).
    assert_close(apply_model_matrix(&matrix, 0.0, 100.0), (20.0e-6, -10.0e-6));
    // Stepping four columns moves one micrometer along target X.
    assert_close(apply_model_matrix(&matrix, 4.0, 100.0), (21.0e-6, -10.0e-6));
}

#[test]
fn test_target_model_matrix_handles_axis_swapping_transformations() {
    // The transformation sends the array's Y axis to the target's X axis.
    let transformation =
        AffineMatrix::from_affine(&[vec![0.0, 1.0, 0.0], vec![1.0, 0.0, 0.0]]).unwrap();
    let order = OmeDimensionOrder::try_from("YX").unwrap();
    let matrix = target_coordinate_system_model_matrix(
        &transformation,
        &order,
        &micrometer_axes(&["y", "x"]),
        100,
        0,
        0,
    )
    .unwrap();

    // Array row 0, column 5 maps to target (y=5, x=0), i.e. world (0, -5).
    assert_close(apply_model_matrix(&matrix, 5.0, 100.0), (0.0, -5e-6));
    // Array row 3, column 0 maps to target (y=0, x=3), i.e. world (3, 0).
    assert_close(apply_model_matrix(&matrix, 0.0, 97.0), (3e-6, 0.0));
}

#[test]
fn test_target_model_matrix_holds_z_fixed() {
    // An affine that mixes Z into the target Y coordinate.
    let transformation = AffineMatrix::from_affine(&[
        vec![1.0, 0.0, 0.0, 0.0],
        vec![2.0, 1.0, 0.0, 0.0],
        vec![0.0, 0.0, 1.0, 0.0],
    ])
    .unwrap();
    let order = OmeDimensionOrder::try_from("ZYX").unwrap();
    let at_z = |z| {
        target_coordinate_system_model_matrix(
            &transformation,
            &order,
            &micrometer_axes(&["z", "y", "x"]),
            10,
            z,
            0,
        )
        .unwrap()
    };
    // Array row 0 of slice z maps to target y = 2z, i.e. world y = -2z.
    assert_close(apply_model_matrix(&at_z(0), 0.0, 10.0), (0.0, 0.0));
    assert_close(apply_model_matrix(&at_z(3), 0.0, 10.0), (0.0, -6e-6));
}

#[test]
fn test_target_model_matrix_errors() {
    let order = OmeDimensionOrder::try_from("YX").unwrap();
    // The target coordinate system has no X or Y axis.
    assert!(target_coordinate_system_model_matrix(
        &AffineMatrix::identity(2),
        &order,
        &micrometer_axes(&["v", "u"]),
        100,
        0,
        0,
    )
    .is_err());
    // The transformation's input dimensionality does not match the array's.
    assert!(target_coordinate_system_model_matrix(
        &AffineMatrix::identity(3),
        &order,
        &micrometer_axes(&["z", "y", "x"]),
        100,
        0,
        0,
    )
    .is_err());
    // The transformation's output dimensionality does not match the target's.
    assert!(target_coordinate_system_model_matrix(
        &AffineMatrix::identity(2),
        &order,
        &micrometer_axes(&["z", "y", "x"]),
        100,
        0,
        0,
    )
    .is_err());
}

#[test]
fn test_model_matrix_pixel_size_under_rotation() {
    // A 90 degree rotation combined with a uniform scale of 2.
    let matrix: [f32; 16] = [
        0.0, 2.0, 0.0, 0.0,
        -2.0, 0.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        0.0, 0.0, 0.0, 1.0,
    ];
    assert_eq!(model_matrix_pixel_size(&matrix), [2.0, 2.0]);
}

#[test]
fn test_ome_dim_order_serde_roundtrip() {
    let order = OmeDimensionOrder::new(vec![OmeDim::T, OmeDim::C, OmeDim::Z, OmeDim::Y, OmeDim::X]);
    let json = serde_json::to_string(&order).unwrap();
    assert_eq!(json, "\"TCZYX\"");
    let decoded: OmeDimensionOrder = serde_json::from_str(&json).unwrap();
    assert_eq!(order, decoded);
}
