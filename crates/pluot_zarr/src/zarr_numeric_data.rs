use std::sync::Arc;
use pluot_core::cache::use_memo_numeric_data;
use pluot_core::log;
use pluot_core::numeric_data::NumericData;
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
pub(crate) fn arr_cache_key(store_name: &str, array_path: &str) -> Vec<String> {
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
) -> Result<Arc<NumericData>, zarrs::array::ArrayError> {
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

/// Load a zarr array from the zarr store into a [`NumericData`] in the
/// array's native dtype.
///
/// The whole array is retrieved directly as its native element type and wrapped
/// in the matching `NumericData` variant — there is no per-element cast to a
/// single dtype. `PointLayer` then uploads each dtype to the GPU at its native
/// width.
/// TODO: extend to support loading partial slices and ND arrs as well?
pub async fn load_arr_as_numeric_data(
    store: Arc<dyn AsyncReadableStorageTraits>,
    array_path: &str,
) -> Result<NumericData, zarrs::array::ArrayError> {
    let array = zarrs::array::Array::async_open(store, array_path)
        .await
        .unwrap();
    let subset = array.subset_all();

    use zarrs::plugin::ZarrVersion;
    let dtype_name = array
        .data_type()
        .name(ZarrVersion::V3)
        .expect("Array data type must have a V3 name")
        .to_string();

    // Retrieve the whole array in its native dtype and wrap it in the matching
    // `NumericData` variant — no per-element conversion.
    macro_rules! load {
        ($rust_ty:ty, $variant:ident) => {{
            let data = array
                .async_retrieve_array_subset::<Vec<$rust_ty>>(&subset)
                .await?;
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
        _ => panic!("Unsupported zarr data type for point coordinates: {}", dtype_name),
    })
}
