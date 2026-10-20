use pluot_core::compute::matrix_axis_extents::MatrixAxis as HeatmapAxis;
use pluot_zarr::layers::adata_zarr_heatmap_layer::axis_domains;

#[test]
fn axis_domains_replace_empty_extents_with_the_unit_domain() {
    let domains = axis_domains(HeatmapAxis::Cols, vec![1.0, f32::INFINITY], vec![5.0, f32::NEG_INFINITY]);
    assert_eq!(domains.min.as_f32().as_ref(), &[1.0, 0.0]);
    assert_eq!(domains.max.as_f32().as_ref(), &[5.0, 1.0]);
}
