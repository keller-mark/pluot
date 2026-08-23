//! Checks that positioning OME-Zarr resolution levels through the coordinate
//! transformation graph agrees with reading the `scale` transformation directly,
//! using real OME-Zarr metadata on disk.
//!
//! The metadata is read from `data/out/`, which is not checked in, so each test
//! is skipped when its fixture is absent.

#![cfg(all(test, not(target_arch = "wasm32")))]

use pluot_zarr::layers::ome_zarr_utils::{
    axis_unit_to_meters, model_matrix_pixel_size, target_coordinate_system_model_matrix,
    upgrade_ome_multiscales, OmeDim, OmeDimensionOrder, INTRINSIC_COORDINATE_SYSTEM,
};
use pluot_zarr::ome_zarr_transformations::{
    CoordinateSystemId, MultiscaleImage, TransformationGraph,
};

/// Read the `ome` attributes of an OME-Zarr group, or `None` if the fixture is
/// not present.
fn read_ome_attributes(path: &str) -> Option<serde_json::Value> {
    let bytes = std::fs::read(format!("../../{path}/zarr.json")).ok()?;
    let metadata: serde_json::Value = serde_json::from_slice(&bytes).expect("parse zarr.json");
    Some(metadata.pointer("/attributes/ome")?.clone())
}

/// The pixel size each resolution level would be given, computed the way the
/// multiscale layer computes it: upgrade the metadata, build the transformation
/// graph, compose each level's transformation into the intrinsic coordinate
/// system, and read the pixel size off the resulting model matrix.
fn pixel_sizes_via_graph(ome: &serde_json::Value) -> Vec<[f64; 2]> {
    let mut ome = ome.clone();
    upgrade_ome_multiscales(&mut ome);

    let multiscales: Vec<MultiscaleImage> =
        serde_json::from_value(ome["multiscales"].clone()).expect("parse multiscales");
    let multiscale = &multiscales[0];
    let graph = TransformationGraph::from_ome_attributes(&ome).expect("build graph");
    let intrinsic = graph
        .resolve_name(INTRINSIC_COORDINATE_SYSTEM)
        .expect("intrinsic coordinate system")
        .clone();
    let axes = &graph.coordinate_system(&intrinsic).expect("intrinsic axes").axes;

    let dimension_order: String = axes.iter().map(|a| a.name.chars().next().unwrap()).collect();
    let dimension_order = OmeDimensionOrder::try_from(dimension_order.as_str()).unwrap();

    multiscale
        .datasets
        .iter()
        .map(|dataset| {
            let transformation = graph
                .transformation_between_with_ndim(
                    &CoordinateSystemId::array(&dataset.path),
                    &intrinsic,
                    axes.len(),
                )
                .expect("transformation into the intrinsic coordinate system");
            // The level height only shifts the matrix, so any value works here.
            let matrix = target_coordinate_system_model_matrix(
                &transformation,
                &dimension_order,
                axes,
                1024,
                0,
                0,
            )
            .expect("model matrix");
            model_matrix_pixel_size(&matrix)
        })
        .collect()
}

/// The pixel size each resolution level would be given by reading the `scale`
/// transformation of the dataset directly, as the layer did before coordinate
/// transformation graphs were supported.
fn pixel_sizes_from_scale_directly(ome: &serde_json::Value) -> Vec<[f64; 2]> {
    let multiscale = &ome["multiscales"][0];
    let axes = multiscale["axes"].as_array().expect("axes");
    let names: Vec<&str> = axes.iter().map(|a| a["name"].as_str().unwrap()).collect();
    let dimension_order = OmeDimensionOrder::try_from(names.concat().as_str()).unwrap();
    let x = dimension_order.index_of(OmeDim::X).unwrap();
    let y = dimension_order.index_of(OmeDim::Y).unwrap();
    let x_to_meters = axis_unit_to_meters(axes[x]["unit"].as_str()).unwrap();
    let y_to_meters = axis_unit_to_meters(axes[y]["unit"].as_str()).unwrap();

    multiscale["datasets"]
        .as_array()
        .expect("datasets")
        .iter()
        .map(|dataset| {
            let scale = dataset["coordinateTransformations"]
                .as_array()
                .expect("coordinateTransformations")
                .iter()
                .find(|t| t["type"] == "scale")
                .map(|t| t["scale"].as_array().expect("scale").clone())
                .expect("a scale transformation");
            [
                scale[y].as_f64().unwrap() * y_to_meters,
                scale[x].as_f64().unwrap() * x_to_meters,
            ]
        })
        .collect()
}

#[test]
fn test_graph_reproduces_the_declared_scale_for_v0_5_metadata() {
    let Some(ome) = read_ome_attributes("data/out/6001240_labels.ome.zarr") else {
        eprintln!("skipping: data/out/6001240_labels.ome.zarr is not present");
        return;
    };
    assert_eq!(ome["version"], "0.5");

    let via_graph = pixel_sizes_via_graph(&ome);
    let expected = pixel_sizes_from_scale_directly(&ome);
    assert_eq!(via_graph.len(), expected.len());
    assert!(via_graph.len() > 1, "expected a multi-level pyramid");

    for (level, (actual, expected)) in via_graph.iter().zip(&expected).enumerate() {
        // The model matrix is built in f32, so allow for f32 rounding.
        for axis in 0..2 {
            let (a, e) = (actual[axis], expected[axis]);
            assert!(
                (a - e).abs() <= 1e-6 * e.abs(),
                "level {level} axis {axis}: {a} vs {e}",
            );
        }
    }
}
