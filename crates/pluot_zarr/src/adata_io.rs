// Async I/O helpers for reading AnnData-Zarr elements, built on the
// `AnnDataEncoding` types in `adata_metadata.rs`.

use std::sync::Arc;

use pluot_core::numeric_data::NumericData;
use zarrs::storage::AsyncReadableStorageTraits;

use crate::adata_metadata::AnnDataEncoding;
use crate::zarr_numeric_data::load_arr_as_numeric_data;


/// Reads the `attributes` of the zarr group or array at `path` and
/// deserializes them into an [`AnnDataEncoding`], regardless of whether the
/// element is a group (e.g. `dataframe`, `categorical`, `dict`) or an array
/// (e.g. `array`, `string-array`).
pub async fn read_encoding(store: Arc<dyn AsyncReadableStorageTraits>, path: &str) -> AnnDataEncoding {
    let attributes = if let Ok(group) = zarrs::group::Group::async_open(store.clone(), path).await {
        group.attributes().clone()
    } else {
        zarrs::array::Array::async_open(store, path)
            .await
            .unwrap_or_else(|e| panic!("Failed to open AnnData element at \"{path}\": {e}"))
            .attributes()
            .clone()
    };
    serde_json::from_value(serde_json::Value::Object(attributes))
        .unwrap_or_else(|e| panic!("Invalid AnnData encoding attributes at \"{path}\": {e}"))
}

/// Reads a whole `string-array`-encoded zarr array.
pub async fn read_string_array(store: Arc<dyn AsyncReadableStorageTraits>, path: &str) -> Result<Vec<String>, zarrs::array::ArrayError> {
    let array = zarrs::array::Array::async_open(store, path).await.unwrap();
    let subset = array.subset_all();
    array.async_retrieve_array_subset::<Vec<String>>(&subset).await
}

/// Reads the row labels of an AnnData dataframe (e.g. `obs` or `var`), i.e.
/// the column named by its `_index` attribute. Handles both `string-array`-
/// and `nullable-string-array`-encoded index columns; the `mask` sibling of
/// the latter is ignored, since a dataframe index is not expected to have
/// missing values.
/// TODO: also support integer and range-type indices
pub async fn read_dataframe_index(store: Arc<dyn AsyncReadableStorageTraits>, dataframe_path: &str) -> Result<Vec<String>, zarrs::array::ArrayError> {
    let index_column = match read_encoding(store.clone(), dataframe_path).await {
        AnnDataEncoding::DataFrame { index, .. } => index,
        other => panic!("Expected a dataframe encoding at \"{dataframe_path}\", got {other:?}"),
    };
    let index_path = format!("{dataframe_path}/{index_column}");
    let values_path = match read_encoding(store.clone(), &index_path).await {
        AnnDataEncoding::NullableStringArray { .. } => format!("{index_path}/values"),
        AnnDataEncoding::StringArray { .. } => index_path,
        other => panic!("Unsupported dataframe index encoding at \"{index_path}\": {other:?}"),
    };
    read_string_array(store, &values_path).await
}

/// Reads an AnnData `categorical` column (e.g. an `obs` groupby column) as
/// its `categories` labels and per-observation `codes` (the category index of
/// each row; `-1` denotes a missing value, per the AnnData spec).
/// TODO: also support non-string categories
pub async fn read_categorical_column(store: Arc<dyn AsyncReadableStorageTraits>, column_path: &str) -> Result<(Vec<String>, NumericData), zarrs::array::ArrayError> {
    let categories = read_string_array(store.clone(), &format!("{column_path}/categories")).await?;
    let codes = load_arr_as_numeric_data(store, &format!("{column_path}/codes")).await?;
    Ok((categories, codes))
}

/// Reads a numeric column of an AnnData dataframe (e.g., a column of `obs` or `var`).
pub async fn read_numeric_column(store: Arc<dyn AsyncReadableStorageTraits>, column_path: &str) -> Result<NumericData, zarrs::array::ArrayError> {
    let values = load_arr_as_numeric_data(store, column_path).await?;
    Ok(values)
}

/// Reads one column (all rows) of a dense 2D numeric zarr array (e.g. AnnData
/// `X` or a `layers` entry) in its native dtype, given the zero-based column
/// index. See [`load_arr_as_numeric_data`] for the whole-array equivalent.
pub async fn read_dense_column_numeric(store: Arc<dyn AsyncReadableStorageTraits>, array_path: &str, col_index: u64) -> Result<NumericData, zarrs::array::ArrayError> {
    let array = zarrs::array::Array::async_open(store, array_path).await.unwrap();
    let n_obs = array.shape()[0];
    let subset = zarrs::array::ArraySubset::new_with_ranges(&[0..n_obs, col_index..col_index + 1]);

    use zarrs::plugin::ZarrVersion;
    let dtype_name = array.data_type().name(ZarrVersion::V3).expect("Array data type must have a V3 name").to_string();

    macro_rules! load {
        ($rust_ty:ty, $variant:ident) => {{
            let data = array.async_retrieve_array_subset::<Vec<$rust_ty>>(&subset).await?;
            NumericData::$variant(Arc::new(data))
        }};
    }

    Ok(match dtype_name.as_str() {
        "uint8" => load!(u8, Uint8),
        "uint16" => load!(u16, Uint16),
        "uint32" => load!(u32, Uint32),
        "uint64" => load!(u64, Uint64),
        "int8" => load!(i8, Int8),
        "int16" => load!(i16, Int16),
        "int32" => load!(i32, Int32),
        "int64" => load!(i64, Int64),
        "float32" => load!(f32, Float32),
        "float64" => load!(f64, Float64),
        other => panic!("Unsupported dtype \"{other}\" for AnnData expression array \"{array_path}\""),
    })
}

/// Reads one column (all rows) of a dense 2D numeric zarr array (e.g. AnnData
/// `X` or a `layers` entry) as `f32`, given the zero-based column index.
pub async fn read_dense_column_f32(store: Arc<dyn AsyncReadableStorageTraits>, array_path: &str, col_index: u64) -> Result<Vec<f32>, zarrs::array::ArrayError> {
    let array = zarrs::array::Array::async_open(store, array_path).await.unwrap();
    let n_obs = array.shape()[0];
    let subset = zarrs::array::ArraySubset::new_with_ranges(&[0..n_obs, col_index..col_index + 1]);

    use zarrs::plugin::ZarrVersion;
    let dtype_name = array.data_type().name(ZarrVersion::V3).expect("Array data type must have a V3 name").to_string();

    Ok(match dtype_name.as_str() {
        "float32" => array.async_retrieve_array_subset::<Vec<f32>>(&subset).await?,
        "float64" => {
            let values = array.async_retrieve_array_subset::<Vec<f64>>(&subset).await?;
            values.iter().map(|&x| x as f32).collect()
        }
        other => panic!("Unsupported dtype \"{other}\" for AnnData expression array \"{array_path}\" (expected float32 or float64)"),
    })
}
