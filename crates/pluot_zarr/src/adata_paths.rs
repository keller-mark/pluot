// Paths of elements of an AnnData object, which may be the root of its store
// or nested within a MuData (`mod/<modality>`) or SpatialData (`tables/<table>`) object.
// Layer params take these paths relative to the store root, e.g. `obs/cell_type`
// or `tables/table/layers/log1p`.

/// The zarr node path of `path`, given relative to the store root.
pub fn node_path(path: &str) -> String {
    format!("/{}", path.trim_matches('/'))
}

/// The node path of the group holding the element at `path`.
pub fn parent_path(path: &str) -> String {
    let path = node_path(path);
    let (parent, _) = path.rsplit_once('/').expect("node paths start with '/'");
    parent.to_string()
}

/// The last component of `path`, e.g. the name of a dataframe column.
pub fn path_name(path: &str) -> &str {
    let path = path.trim_matches('/');
    path.rsplit_once('/').map_or(path, |(_, name)| name)
}

/// The node path of the AnnData object holding the matrix at `matrix_path`
/// (its `X` or a `layers` entry), e.g. `/tables/table` for `tables/table/layers/log1p`.
/// Empty for the store root.
pub fn adata_path_of_matrix(matrix_path: &str) -> String {
    let parent = parent_path(matrix_path);
    parent.strip_suffix("/layers").map_or(parent.clone(), str::to_string)
}

/// The node path of the AnnData object holding the dataframe at `dataframe_path` (its `obs` or `var`).
pub fn adata_path_of_dataframe(dataframe_path: &str) -> String {
    parent_path(dataframe_path)
}

/// Joins `segments` onto the node path `base`.
pub fn join_path(base: &str, segments: &str) -> String {
    node_path(&format!("{base}/{segments}"))
}
