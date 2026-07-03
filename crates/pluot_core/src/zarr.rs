// TODO: can/should this be moved into pluot_zarr?

use std::io;
use crate::{
    zarr_get, zarr_get_range_from_end, zarr_get_range_from_offset, zarr_has,
    zarr_get_status, zarr_get_range_from_end_status, zarr_get_range_from_offset_status, zarr_has_status,
};
use crate::zarr_types::ZarrPeekResult;

use futures::{stream, StreamExt, TryStreamExt};
use zarrs::storage::{
    byte_range::{ByteRange, ByteRangeIterator},
    AsyncMaybeBytesIterator, AsyncReadableStorageTraits, Bytes, MaybeBytes, StorageError, StoreKey,
};

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};


// "Pushing" model for Promise statuses.
// Rather than doing a full roundtrip (e.g., Rust->JS->Rust) on every re-render while a Promise
// is pending, we keep a HashMap of Promise statuses on the Rust side.
// On the first status check for a particular key, we still do the roundtrip so that the host
// (e.g., JS) side "kicks off" the Promise loading for this key, and we record the returned status.
// On subsequent re-renders, we only check the Rust-side HashMap.
// When a Promise settles on the host side, the host calls a bound Rust function
// (exposed via bindings.rs) to "push" the updated status for this key into the HashMap,
// so the next re-render "sees" it without a roundtrip.
// Hosts that do not push status updates (e.g., Python, R) must pass
// `wait_for_store_pushes: false` in the render params, in which case every status check
// falls back to the roundtrip ("pulling" model).
// Maps store name -> (host cache key -> last known Promise status).
static ZARR_PROMISE_STATUSES: OnceLock<Mutex<HashMap<String, HashMap<String, ZarrPeekResult>>>> =
    OnceLock::new();

fn get_promise_statuses() -> &'static Mutex<HashMap<String, HashMap<String, ZarrPeekResult>>> {
    ZARR_PROMISE_STATUSES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Called by the host to push the settled (or re-pending) status of the Promise
/// for `cache_key` (the host-side cache key, e.g., "/path/to/chunk" or "/path/to/chunk:0:99").
pub fn push_promise_status(store_name: &str, cache_key: &str, status: ZarrPeekResult) {
    let mut map = get_promise_statuses().lock().unwrap();
    map.entry(store_name.to_string())
        .or_default()
        .insert(cache_key.to_string(), status);
}

/// Called by the host when it forgets the Promise for `cache_key` (e.g., LRU eviction),
/// so the next status check does a fresh roundtrip.
pub fn remove_promise_status(store_name: &str, cache_key: &str) {
    if let Some(store_map) = get_promise_statuses().lock().unwrap().get_mut(store_name) {
        store_map.remove(cache_key);
    }
}

/// Called by the host when it clears all Promises for a store (e.g., cache clear or store replacement).
pub fn clear_promise_statuses(store_name: &str) {
    get_promise_statuses().lock().unwrap().remove(store_name);
}

fn get_pushed_promise_status(store_name: &str, cache_key: &str) -> Option<ZarrPeekResult> {
    get_promise_statuses()
        .lock()
        .unwrap()
        .get(store_name)
        .and_then(|store_map| store_map.get(cache_key))
        .copied()
}

/// The host-side cache key for this key/byte-range.
/// The host (see lru-store.ts) uses zarrita AbsolutePath keys, which have a leading slash.
fn host_cache_key(key: &str, byte_range: Option<ByteRange>) -> String {
    format!("/{}", normalize_key(key, byte_range))
}

// We need to use quick_cache.
// Using mini_moka did not work, as its sync Cache was not compatible with WASM,
// and its unsync Cache was not cooperating with OnceLock/Mutex/RefCell/OnceCell/etc.
use quick_cache::sync::Cache;

static ZARR_STORE_CACHES: OnceLock<Mutex<HashMap<String, Arc<Cache<String, Bytes>>>>> =
    OnceLock::new();

// We no longer need caching at the store level,
// since we now have the use_memo_ functions in cache.rs.
// We disable cacheing here to prevent double caching,
// minimizing memory usage.
// TODO: remove the caching stuff from here entirely
const ZARR_CACHE_ENABLED: bool = false;

fn get_or_init_store_cache(name: &str) -> Arc<Cache<String, Bytes>> {
    let map_mutex = ZARR_STORE_CACHES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut map = map_mutex.lock().unwrap();

    if let Some(cache) = map.get(name) {
        cache.clone()
    } else {
        // TODO: is 100 a good cache size?
        let new_cache = Arc::new(Cache::new(10000)); // Cache up to 100 items
        map.insert(name.to_string(), new_cache.clone());
        new_cache
    }
}

fn normalize_key(key: &str, byte_range: Option<ByteRange>) -> String {
    // Reference: https://github.com/hms-dbmi/vizarr/blob/862745c1c7c095748bbe97475da61807d5b49189/src/lru-store.ts#L14
    match byte_range {
        Some(ByteRange::FromStart(start, Some(len))) => {
            format!("{}:{}:{}", key, start, start + len - 1)
        }
        Some(ByteRange::Suffix(suffix_length)) => {
            format!("{}:-{}", key, suffix_length)
        }
        None => key.to_string(),
        _ => panic!("Unsupported ByteRange variant"),
    }
}

fn make_storage_error() -> StorageError {
    return StorageError::IOError(Arc::new(io::Error::new(io::ErrorKind::TimedOut, "too slow")));
}

fn is_storage_error_timed_out(err: &StorageError) -> bool {
    match err {
        StorageError::IOError(io_err) => io_err.kind() == io::ErrorKind::TimedOut,
        _ => false,
    }
}

fn is_codec_error_timed_out(err: &zarrs::array::CodecError) -> bool {
    match err {
        zarrs::array::CodecError::StorageError(se) => is_storage_error_timed_out(se),
        zarrs::array::CodecError::IOError(io_err) => io_err.kind() == io::ErrorKind::TimedOut,
        _ => false,
    }
}

/// Check whether a zarrs `ArrayError` wraps a `TimedOut` IO error,
/// possibly nested inside `StorageError` or `CodecError(StorageError)`.
pub fn is_timed_out_zarrs_error(err: &zarrs::array::ArrayError) -> bool {
    match err {
        zarrs::array::ArrayError::StorageError(se) => is_storage_error_timed_out(se),
        zarrs::array::ArrayError::CodecError(ce) => is_codec_error_timed_out(ce),
        _ => false,
    }
}

// References:
// - https://github.com/zarrs/zarrs/blob/3f7eb5a466e1ef613ecc620125b0df70b72f42f2/zarrs_storage/src/storage_async.rs
// - https://github.com/zarrs/zarrs/blob/3f7eb5a466e1ef613ecc620125b0df70b72f42f2/zarrs_storage/src/store/memory_store.rs
// - https://github.com/zarrs/zarrs/blob/3f7eb5a466e1ef613ecc620125b0df70b72f42f2/zarrs_storage/src/store_test.rs#L238
// - https://github.com/zarrs/zarrs/blob/3f7eb5a466e1ef613ecc620125b0df70b72f42f2/zarrs_object_store/src/lib.rs

/// An asynchronous store that calls bound functions.
pub struct AsyncZarritaStore {
    store_name: String,
    wait_for_store_gets: bool,
    wait_for_store_pushes: bool,
}

impl AsyncZarritaStore {
    /// Create a new [`AsyncZarritaStore`].
    #[must_use]
    pub fn new(store_name: String, wait_for_store_gets: bool, wait_for_store_pushes: bool) -> Self {
        Self {
            store_name,
            wait_for_store_gets,
            wait_for_store_pushes,
        }
    }

    /// Check the Promise status for `cache_key`, preferring the Rust-side HashMap
    /// when the host pushes status updates (`wait_for_store_pushes`).
    /// The `fetch_status` roundtrip only happens on the first check for a key
    /// (kicking off the Promise on the host side), or on every check when the
    /// host does not push.
    fn check_promise_status(
        &self,
        cache_key: &str,
        fetch_status: impl FnOnce() -> ZarrPeekResult,
    ) -> ZarrPeekResult {
        if !self.wait_for_store_pushes {
            return fetch_status();
        }
        if let Some(status) = get_pushed_promise_status(&self.store_name, cache_key) {
            return status;
        }
        let status = fetch_status();
        // or_insert: never overwrite a status the host pushed while the roundtrip was in flight.
        let mut map = get_promise_statuses().lock().unwrap();
        *map.entry(self.store_name.to_string())
            .or_default()
            .entry(cache_key.to_string())
            .or_insert(status)
    }

    /// Fetch a single byte range, checking status first if `wait_for_store_gets` is false.
    async fn fetch_byte_range(
        &self,
        key: &str,
        byte_range: ByteRange,
    ) -> Result<Bytes, StorageError> {
        match byte_range {
            ByteRange::FromStart(start, Some(len)) => {
                // This is the getRange({ offset, length }) case.
                if !self.wait_for_store_gets {
                    let promise_status = self.check_promise_status(
                        &host_cache_key(key, Some(byte_range)),
                        || {
                            zarr_get_range_from_offset_status(
                                &self.store_name,
                                key,
                                start as u32,
                                len as u32,
                            )
                        },
                    );
                    if promise_status == ZarrPeekResult::Pending {
                        // We cannot await and the promise is still pending.
                        return Err(make_storage_error());
                    }
                    // We cannot await but the promise is either fulfilled or rejected.
                }
                // We can await (or the promise is already settled).
                Ok(zarr_get_range_from_offset(
                    &self.store_name,
                    key,
                    start as u32,
                    len as u32,
                )
                .await)
            }
            ByteRange::Suffix(suffix_length) => {
                // This is the getRange({ suffixLength }) case.
                if !self.wait_for_store_gets {
                    let promise_status = self.check_promise_status(
                        &host_cache_key(key, Some(byte_range)),
                        || {
                            zarr_get_range_from_end_status(
                                &self.store_name,
                                key,
                                suffix_length as u32,
                            )
                        },
                    );
                    if promise_status == ZarrPeekResult::Pending {
                        // We cannot await and the promise is still pending.
                        return Err(make_storage_error());
                    }
                    // We cannot await but the promise is either fulfilled or rejected.
                }
                // We can await (or the promise is already settled).
                Ok(zarr_get_range_from_end(
                    &self.store_name,
                    key,
                    suffix_length as u32,
                )
                .await)
            }
            _ => panic!("Unsupported ByteRange variant"),
        }
    }

    pub async fn has(&self, key: &StoreKey) -> Result<bool, StorageError> {
        if !self.wait_for_store_gets {
            let promise_status = self.check_promise_status(
                &host_cache_key(key.as_str(), None),
                || zarr_has_status(&self.store_name, key.as_str()),
            );
            if promise_status == ZarrPeekResult::Pending {
                return Err(make_storage_error());
            }
        }

        let has = zarr_has(&self.store_name, key.as_str()).await;
        Ok(has)
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl AsyncReadableStorageTraits for AsyncZarritaStore {
    // Note: this has a default implementation in zarrs,
    // so this may not be necessary.
    // Reference: https://github.com/zarrs/zarrs/blob/cd1ee50c7a7c4af3002ffe4a8314c568c9b11b38/zarrs_storage/src/storage_async.rs#L32
    async fn get(&self, key: &StoreKey) -> Result<MaybeBytes, StorageError> {
        // Normalize the key similar to lru_cache.ts
        // considering potential Range info.
        let key_str = normalize_key(key.as_str(), None);

        if !ZARR_CACHE_ENABLED {
            // Use the zarr_get_js function to fetch the data
            let bytes = zarr_get(&self.store_name, key.as_str()).await;
            return Ok(Some(bytes));
        }

        // Check the cache first
        let cache = get_or_init_store_cache(&self.store_name);
        if let Some(cached) = cache.get(&key_str.to_string()) {
            return Ok(Some(cached.clone()));
        }

        if !self.has(key).await.expect("store.has failed") {
            return Ok(None);
        }

        if !self.wait_for_store_gets {
            let promise_status = self.check_promise_status(
                &host_cache_key(key.as_str(), None),
                || zarr_get_status(&self.store_name, key.as_str()),
            );
            if promise_status == ZarrPeekResult::Pending {
                return Err(make_storage_error());
            }
        }

        // Use the zarr_get_js function to fetch the data
        let bytes = zarr_get(&self.store_name, key.as_str()).await;

        // Store in cache
        cache.insert(key_str.to_string(), bytes.clone());

        Ok(Some(bytes))
    }

    async fn get_partial_many<'a>(
        &'a self,
        key: &StoreKey,
        byte_ranges: ByteRangeIterator<'a>,
    ) -> Result<AsyncMaybeBytesIterator<'a>, StorageError> {
        let mut results = Vec::new();
        let cache = ZARR_CACHE_ENABLED.then(|| get_or_init_store_cache(&self.store_name));

        // Iterate over the requested byte ranges (potentially multiple).
        for byte_range in byte_ranges {
            // Normalize the key similar to lru_cache.ts
            // considering potential Range info.
            let key_str = normalize_key(key.as_str(), Some(byte_range));

            // Check the cache first (if enabled).
            if let Some(cached) = cache.as_ref().and_then(|c| c.get(&key_str)) {
                results.push(Ok(cached.clone()));
                continue;
            }

            // Use the zarr_get_js function to fetch the data
            let bytes_result = self.fetch_byte_range(key.as_str(), byte_range).await;

            // Store in cache on success (if enabled).
            if let (Some(cache), Ok(bytes)) = (&cache, &bytes_result) {
                cache.insert(key_str, bytes.clone());
            }

            // Append to results
            results.push(bytes_result);
        }
        Ok(Some(Box::pin(stream::iter(results))))
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

    fn supports_get_partial(&self) -> bool {
        true
    }
}
