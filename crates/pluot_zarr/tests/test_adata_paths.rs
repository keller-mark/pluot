use pluot_zarr::adata_paths::{adata_path_of_matrix, node_path, path_name};
use pluot_zarr::heatmap_data::{AdataAxis, MatrixAxis};

#[test]
fn matrices_belong_to_the_adata_object_holding_them() {
    assert_eq!(adata_path_of_matrix("X"), "");
    assert_eq!(adata_path_of_matrix("/layers/log1p"), "");
    assert_eq!(adata_path_of_matrix("mod/rna/X"), "/mod/rna");
    assert_eq!(adata_path_of_matrix("tables/table/layers/log1p"), "/tables/table");
}

#[test]
fn axes_of_a_matrix_are_the_dataframes_of_its_adata_object() {
    assert_eq!(AdataAxis::of_matrix("X", MatrixAxis::Var).dataframe_path, "/var");
    assert_eq!(AdataAxis::of_matrix("mod/rna/layers/counts", MatrixAxis::Obs).dataframe_path, "/mod/rna/obs");
    assert_eq!(AdataAxis::of_matrix("tables/table/X", MatrixAxis::Obs).default_matrix_path(), "/tables/table/X");
}

#[test]
fn obs_axis_of_columns_is_the_dataframe_of_the_first_column() {
    assert_eq!(AdataAxis::obs_of_columns(["tables/table/obs/region", "obs/other"]).dataframe_path, "/tables/table/obs");
    assert_eq!(AdataAxis::obs_of_columns([]).dataframe_path, "/obs");
    assert_eq!(AdataAxis::obs_of_columns(["obs/cell_type"]).default_matrix_path(), "/X");
}

#[test]
fn paths_are_normalized_to_node_paths() {
    assert_eq!(node_path("obs/cell_type/"), "/obs/cell_type");
    assert_eq!(node_path("/obs/cell_type"), "/obs/cell_type");
    assert_eq!(path_name("mod/rna/obs/cell_type"), "cell_type");
}
