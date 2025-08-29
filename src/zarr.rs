
use crate::{
    zarr_has, zarr_get, zarr_get_range_from_offset, zarr_get_range_from_end,
};

use zarrs::storage::{
    byte_range::{ByteRange}, AsyncBytes, StoreKey,
    AsyncReadableStorageTraits, MaybeAsyncBytes, StorageError,
};

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock, Arc};

// We need to use quick_cache.
// Using mini_moka did not work, as its sync Cache was not compatible with WASM,
// and its unsync Cache was not cooperating with OnceLock/Mutex/RefCell/OnceCell/etc.
use quick_cache::sync::Cache;


static ZARR_STORE_CACHES: OnceLock<Mutex<HashMap<String, Arc<Cache<String, AsyncBytes>>>>> = OnceLock::new();

fn get_or_init_store_cache(name: &str) -> Arc<Cache<String, AsyncBytes>> {
    let map_mutex = ZARR_STORE_CACHES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut map = map_mutex.lock().unwrap();
    
    if let Some(cache) = map.get(name) {
        cache.clone()
    } else {
        // TODO: is 100 a good cache size?
        let new_cache = Arc::new(Cache::new(100)); // Cache up to 100 items
        map.insert(name.to_string(), new_cache.clone());
        new_cache
    }
}

// References:
// - https://github.com/zarrs/zarrs/blob/3f7eb5a466e1ef613ecc620125b0df70b72f42f2/zarrs_storage/src/storage_async.rs
// - https://github.com/zarrs/zarrs/blob/3f7eb5a466e1ef613ecc620125b0df70b72f42f2/zarrs_storage/src/store/memory_store.rs
// - https://github.com/zarrs/zarrs/blob/3f7eb5a466e1ef613ecc620125b0df70b72f42f2/zarrs_storage/src/store_test.rs#L238
// - https://github.com/zarrs/zarrs/blob/3f7eb5a466e1ef613ecc620125b0df70b72f42f2/zarrs_object_store/src/lib.rs

/// An asynchronous store backed by an [`object_store::ObjectStore`].
pub struct AsyncZarritaStore {
    store_name: String,
}

impl AsyncZarritaStore {
    /// Create a new [`AsyncZarritaStore`].
    #[must_use]
    pub fn new(store_name: String) -> Self {
        Self { store_name }
    }

    pub async fn has(&self, key: &StoreKey) -> Result<bool, StorageError> {
        let store_name = self.store_name.clone();

        let has = zarr_has(&store_name, key.as_str()).await;
        Ok(has)
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl AsyncReadableStorageTraits for AsyncZarritaStore {
    async fn get(&self, key: &StoreKey) -> Result<MaybeAsyncBytes, StorageError> {

        // TODO: normalize the key similar to lru_cache.ts
        // considering potential Range info.
        let key_str = key.as_str();

        // Check the cache first
        let cache = get_or_init_store_cache(&self.store_name);
        if let Some(cached) = cache.get(&key_str.to_string()) {
            return Ok(Some(cached.clone()));
        }

        if !self.has(key).await.expect("store.has failed") {
            return Ok(None);
        }
        // Use the zarr_get_js function to fetch the data
        let bytes = zarr_get(&self.store_name, key.as_str()).await;

        // Store in cache
        cache.insert(key.to_string(), bytes.clone());
        
        Ok(Some(bytes))
    }

    async fn get_partial_values_key(
        &self,
        key: &StoreKey,
        byte_ranges: &mut dyn zarrs::storage::byte_range::ByteRangeIterator,
    ) -> Result<Option<Vec<AsyncBytes>>, StorageError> {
        let mut results = Vec::new();

        // TODO: use the cache here.
    
        for byte_range in byte_ranges {
            let bytes = match byte_range {
                ByteRange::FromStart(start, Some(end)) => {
                    zarr_get_range_from_offset(&self.store_name, key.as_str(), start as u32, (end - start) as u32).await
                },
                ByteRange::Suffix(suffix_length) => {
                    zarr_get_range_from_end(&self.store_name, key.as_str(), suffix_length as u32).await
                },
                _ => panic!("Unsupported ByteRange variant"),
            };
            results.push(bytes);
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
