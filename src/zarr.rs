
use crate::{zarr_has_js, zarr_get_js, zarr_get_range_from_offset_js, zarr_get_range_from_end_js};


use zarrs::storage::{
    async_store_set_partial_values, byte_range::ByteRange, Bytes, AsyncBytes, AsyncListableStorageTraits,
    AsyncReadableStorageTraits, AsyncWritableStorageTraits, MaybeAsyncBytes, StorageError,
    StoreKey, StoreKeyOffsetValue, StoreKeys, StoreKeysPrefixes, StorePrefix,
};


pub fn convert_to_bytes(u8arr: js_sys::Uint8Array) -> AsyncBytes {
    // Copy data from Uint8Array into a Rust Vec<u8>
    let mut vec = vec![0u8; u8arr.length() as usize];

    // TODO: can this be done without copying?
    // The issue is that the original Uint8Array is created via JS fetch() within zarrita fetchStore.

    u8arr.copy_to(&mut vec);

    // Convert Vec<u8> into Bytes
    Bytes::from(vec)
}


// References:
// - https://github.com/zarrs/zarrs/blob/3f7eb5a466e1ef613ecc620125b0df70b72f42f2/zarrs_storage/src/storage_async.rs
// - https://github.com/zarrs/zarrs/blob/3f7eb5a466e1ef613ecc620125b0df70b72f42f2/zarrs_storage/src/store/memory_store.rs
// - https://github.com/zarrs/zarrs/blob/3f7eb5a466e1ef613ecc620125b0df70b72f42f2/zarrs_storage/src/store_test.rs#L238
// - https://github.com/zarrs/zarrs/blob/3f7eb5a466e1ef613ecc620125b0df70b72f42f2/zarrs_object_store/src/lib.rs

/// An asynchronous store backed by an [`object_store::ObjectStore`].
pub struct AsyncZarritaStore {
    store_name: String,

    // locks: AsyncStoreLocks,
}

impl AsyncZarritaStore {
    /// Create a new [`AsyncZarritaStore`].
    #[must_use]
    pub fn new(store_name: String) -> Self {
        Self { store_name }
    }

    pub async fn has(&self, key: &StoreKey) -> Result<bool, StorageError> {
        let store_name = self.store_name.clone();

        let has = zarr_has_js(&store_name, key.as_str()).await;
        Ok(has.is_truthy())
    }
}

#[async_trait::async_trait(?Send)]
impl AsyncReadableStorageTraits for AsyncZarritaStore {
    async fn get(&self, key: &StoreKey) -> Result<MaybeAsyncBytes, StorageError> {

        if !self.has(key).await.expect("store.has failed") {
            return Ok(None);
        }
        // Use the zarr_get_js function to fetch the data
        let js_bytes = zarr_get_js(&self.store_name, key.as_str()).await;
        
        // TODO: Convert the js_sys::Uint8Array to AsyncBytes
        Ok(Some(convert_to_bytes(js_bytes)))
    }

    async fn get_partial_values_key(
        &self,
        key: &StoreKey,
        byte_ranges: &[ByteRange],
    ) -> Result<Option<Vec<AsyncBytes>>, StorageError> {
        let mut results = Vec::new();
    
        for byte_range in byte_ranges {
            let js_bytes = match byte_range {
                ByteRange::FromStart(start, Some(end)) => {
                    zarr_get_range_from_offset_js(&self.store_name, key.as_str(), *start as u32, (*end - *start) as u32).await
                },
                ByteRange::Suffix(suffix_length) => {
                    zarr_get_range_from_end_js(&self.store_name, key.as_str(), *suffix_length as u32).await
                },
                _ => panic!("Unsupported ByteRange variant"),
            };
            results.push(convert_to_bytes(js_bytes));
        }
        Ok(Some(results))
    }

    async fn size_key(&self, key: &StoreKey) -> Result<Option<u64>, StorageError> {
        /*
        Ok(
            handle_result_notfound(self.object_store.head(&key_to_path(key)).await)?
                .map(|meta| meta.size),
        )
        */
        Ok(None) // TODO: implement. can zarrita return a size?
    }
}
