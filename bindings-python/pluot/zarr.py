import zarr
from zarr.storage import MemoryStore, LocalStore
from zarr.abc.store import RangeByteRequest, SuffixByteRequest
from zarr.core.buffer.core import default_buffer_prototype
from os.path import join, dirname
from enum import IntEnum

# Global mapping from store_name to Zarr store objects.

GLOBAL_STORES = {
    "my_store": LocalStore(join(dirname(__file__), "..", "..", "data", "out", "gaussian_quantiles.zarr")),
    "ome_ngff": LocalStore(join(dirname(__file__), "..", "..", "data", "out", "6001240_labels.ome.zarr")),
}

class ZarrPeekResult(IntEnum):
    Pending = 0
    Fulfilled = 1
    Rejected = 2

# Cache for tracking completed async results.
# Maps a string cache key to either the result value or an Exception.
_RESULT_CACHE: dict = {}

def _has_cache_key(store_name: str, key: str) -> str:
    return f"has:{store_name}:{key}"

def _get_cache_key(store_name: str, key: str) -> str:
    return f"get:{store_name}:{key}"

def _get_range_offset_cache_key(store_name: str, key: str, offset: int, length: int) -> str:
    return f"get_range_offset:{store_name}:{key}:{offset}:{length}"

def _get_range_end_cache_key(store_name: str, key: str, suffix_length: int) -> str:
    return f"get_range_end:{store_name}:{key}:{suffix_length}"

def _peek_status(cache_key: str) -> ZarrPeekResult:
    if cache_key not in _RESULT_CACHE:
        return ZarrPeekResult.Pending
    if isinstance(_RESULT_CACHE[cache_key], Exception):
        return ZarrPeekResult.Rejected
    return ZarrPeekResult.Fulfilled

def zarr_has_status(store_name: str, key: str) -> ZarrPeekResult:
    """Synchronously check the status of a zarr_has call without awaiting."""
    return _peek_status(_has_cache_key(store_name, key))

def zarr_get_status(store_name: str, key: str) -> ZarrPeekResult:
    """Synchronously check the status of a zarr_get call without awaiting."""
    return _peek_status(_get_cache_key(store_name, key))

def zarr_get_range_from_offset_status(store_name: str, key: str, offset: int, length: int) -> ZarrPeekResult:
    """Synchronously check the status of a zarr_get_range_from_offset call without awaiting."""
    return _peek_status(_get_range_offset_cache_key(store_name, key, offset, length))

def zarr_get_range_from_end_status(store_name: str, key: str, suffix_length: int) -> ZarrPeekResult:
    """Synchronously check the status of a zarr_get_range_from_end call without awaiting."""
    return _peek_status(_get_range_end_cache_key(store_name, key, suffix_length))

async def zarr_has(store_name: str, key: str) -> bool:
    """Check if a key exists in the Zarr store."""
    cache_key = _has_cache_key(store_name, key)
    if cache_key in _RESULT_CACHE:
        val = _RESULT_CACHE[cache_key]
        if isinstance(val, Exception):
            raise val
        return val
    store = GLOBAL_STORES[store_name]
    try:
        result = await store.exists(key)
        _RESULT_CACHE[cache_key] = result
        return result
    except Exception as e:
        _RESULT_CACHE[cache_key] = e
        raise

async def zarr_get(store_name: str, key: str) -> bytes:
    """Get the value for a key from the Zarr store."""
    cache_key = _get_cache_key(store_name, key)
    if cache_key in _RESULT_CACHE:
        val = _RESULT_CACHE[cache_key]
        if isinstance(val, Exception):
            raise val
        return val
    store = GLOBAL_STORES[store_name]
    try:
        result = (await store.get(
            key,
            prototype=default_buffer_prototype(),
        )).to_bytes()
        _RESULT_CACHE[cache_key] = result
        return result
    except Exception as e:
        _RESULT_CACHE[cache_key] = e
        raise

async def zarr_get_range_from_offset(store_name: str, key: str, offset: int, length: int) -> bytes:
    """Get a byte range from a value in the Zarr store, specified by offset and length."""
    cache_key = _get_range_offset_cache_key(store_name, key, offset, length)
    if cache_key in _RESULT_CACHE:
        val = _RESULT_CACHE[cache_key]
        if isinstance(val, Exception):
            raise val
        return val
    store = GLOBAL_STORES[store_name]
    try:
        result = (await store.get(
            key,
            byte_range=RangeByteRequest(start=offset, end=offset+length),
            prototype=default_buffer_prototype(),
        )).to_bytes()
        _RESULT_CACHE[cache_key] = result
        return result
    except Exception as e:
        _RESULT_CACHE[cache_key] = e
        raise

async def zarr_get_range_from_end(store_name: str, key: str, suffix_length: int) -> bytes:
    """Get a byte range from the end of a value in the Zarr store, specified by length."""
    cache_key = _get_range_end_cache_key(store_name, key, suffix_length)
    if cache_key in _RESULT_CACHE:
        val = _RESULT_CACHE[cache_key]
        if isinstance(val, Exception):
            raise val
        return val
    store = GLOBAL_STORES[store_name]
    try:
        result = (await store.get(
            key,
            byte_range=SuffixByteRequest(suffix=suffix_length),
            prototype=default_buffer_prototype(),
        )).to_bytes()
        _RESULT_CACHE[cache_key] = result
        return result
    except Exception as e:
        _RESULT_CACHE[cache_key] = e
        raise
