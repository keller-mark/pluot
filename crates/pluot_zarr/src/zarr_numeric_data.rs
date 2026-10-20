use std::sync::Arc;
use pluot_core::cache::use_memo_numeric_data;
use pluot_core::log;
use pluot_core::numeric_data::NumericData;
use zarrs::array::ArrayError;
use zarrs::storage::AsyncReadableStorageTraits;


/// The memoization key under which one zarr array's [`NumericData`] is cached.
///
/// Deliberately only `(store_name, array_path)`: what
/// [`load_arr_as_numeric_data`] returns is fully determined by those two, so
/// nothing else belongs in the key. In particular the key excludes
/// - the role the array plays for the caller — a plotted data column, a
///   filtering `codes_key`, a selection `values_key` — so an array used in
///   several roles is fetched once;
/// - the calling layer and, for criteria, the criterion's position in its list
///   and its thresholds (`included_codes`/`min`/`max`), so re-tuning a filter
///   or dragging a brush re-uses the already-loaded array.
pub fn arr_cache_key(store_name: &str, array_path: &str) -> Vec<String> {
    vec![
        "zarr_numeric_data_arr".to_string(),
        store_name.to_string(),
        array_path.to_string(),
    ]
}

/// [`load_arr_as_numeric_data`], memoized under [`arr_cache_key`].
///
/// Every zarr `NumericData` array a layer loads should go through this, so that
/// all of them share one cache entry per `(store, path)` regardless of which
/// layer or which role asked for it.
pub async fn load_arr_as_numeric_data_memoized(
    store: Arc<dyn AsyncReadableStorageTraits>,
    store_name: &str,
    array_path: &str,
    cache_enabled: bool,
) -> Result<Arc<NumericData>, ArrayError> {
    let keys = arr_cache_key(store_name, array_path);
    use_memo_numeric_data(
        async || {
            load_arr_as_numeric_data(store, array_path).await
        },
        &keys,
        cache_enabled,
    )
    .await
}

/// Retrieves `subset` of `array` as a [`NumericData`] in the array's native dtype, with no
/// per-element conversion. `bool` arrays are read as `Uint8`.
pub async fn retrieve_numeric_subset(
    array: &zarrs::array::Array<dyn AsyncReadableStorageTraits>,
    subset: &zarrs::array::ArraySubset,
) -> Result<NumericData, ArrayError> {
    use zarrs::plugin::ZarrVersion;
    let dtype_name = array.data_type().name(ZarrVersion::V3).expect("Array data type must have a V3 name").to_string();

    macro_rules! retrieve {
        ($rust_ty:ty) => {
            NumericData::from(array.async_retrieve_array_subset::<Vec<$rust_ty>>(subset).await?)
        };
    }

    Ok(match dtype_name.as_str() {
        "uint8" => retrieve!(u8),
        "uint16" => retrieve!(u16),
        "uint32" => retrieve!(u32),
        "uint64" => retrieve!(u64),
        "int8" => retrieve!(i8),
        "int16" => retrieve!(i16),
        "int32" => retrieve!(i32),
        "int64" => retrieve!(i64),
        "float32" => retrieve!(f32),
        "float64" => retrieve!(f64),
        "bool" => {
            let data = array.async_retrieve_array_subset::<Vec<bool>>(subset).await?;
            NumericData::from(data.into_iter().map(u8::from).collect::<Vec<u8>>())
        }
        other => panic!("Unsupported zarr data type \"{other}\" for array \"{}\"", array.path()),
    })
}

/// Load a zarr array from the zarr store into a [`NumericData`] in the
/// array's native dtype. See [`retrieve_numeric_subset`].
/// TODO: extend to support loading partial slices and ND arrs as well?
pub async fn load_arr_as_numeric_data(
    store: Arc<dyn AsyncReadableStorageTraits>,
    array_path: &str,
) -> Result<NumericData, ArrayError> {
    let array = zarrs::array::Array::async_open(store, array_path)
        .await
        .unwrap();
    retrieve_numeric_subset(&array, &array.subset_all()).await
}
