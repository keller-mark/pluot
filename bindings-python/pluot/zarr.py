import zarr
from zarr.storage import MemoryStore, LocalStore
from zarr.abc.store import RangeByteRequest, SuffixByteRequest
from zarr.core.buffer.core import default_buffer_prototype
from os.path import join, dirname
from enum import IntEnum
from pluot.font import FontStore

# Global mapping from store_name to Zarr store objects.

GLOBAL_STORES: dict = {
    # "my_store": LocalStore(join(dirname(__file__), "..", "..", "data", "out", "gaussian_quantiles.zarr")),
    # "ome_ngff": LocalStore(join(dirname(__file__), "..", "..", "data", "out", "6001240_labels.ome.zarr")),
    "__fonts__": FontStore(),
}

def store_instance_to_metadata(store) -> dict:
    """Derive portable ``ZarrStoreInfo`` metadata from a zarr-python store instance.

    The result mirrors the Rust ``ZarrStoreInfo`` JSON (see
    ``crates/pluot_core/src/params.rs``): an adjacently-tagged ``store_type`` /
    ``store_params`` pair plus an optional ``store_extensions`` list.

    Resolution order:
      1. A wrapper store may declare its own metadata via a ``store_metadata``
         attribute (see :func:`store_with_metadata`), which already passes the
         inner store's metadata through and layers on any extension.
      2. A ``LocalStore`` exposes ``.root`` -> ``LocalStore``.
      3. An fsspec/remote-backed store yields a URL -> ``HttpStore``.
      4. Otherwise fall back to a ``MemoryStore`` descriptor. The instance is
         still usable at render time (it is registered by name in
         ``GLOBAL_STORES``), but its data is not reconstructable from metadata.
    """
    declared = getattr(store, "store_metadata", None)
    if isinstance(declared, dict) and "store_type" in declared:
        return declared

    # LocalStore exposes `.root` (a path).
    root = getattr(store, "root", None)
    if root is not None:
        return {
            "store_type": "LocalStore",
            "store_params": {"path": str(root)},
            "store_extensions": None,
        }

    # Remote / fsspec-backed stores: derive a URL where possible.
    url = _derive_store_url(store)
    if url is not None:
        return {
            "store_type": "HttpStore",
            "store_params": {"url": url},
            "store_extensions": None,
        }

    return {
        "store_type": "MemoryStore",
        "store_params": {
            "message": f"In-memory or custom store ({type(store).__name__})"
        },
        "store_extensions": None,
    }


# Registry of store-extension appliers used by store_metadata_to_instance to
# reconstruct virtual-zarr wrapper stores (the inverse of the store_extensions
# recorded by store_instance_to_metadata). Appliers are opt-in so this package
# need not depend on every virtual-zarr implementation.
_STORE_EXTENSION_APPLIERS: dict = {}


def register_store_extension(extension: str, applier) -> None:
    """Register the applier used to reconstruct a ``ZarrStoreExtension`` wrapper.

    ``applier`` takes a base store and returns a wrapped store (e.g. one that
    virtualizes OME-TIFF data as zarr).
    """
    _STORE_EXTENSION_APPLIERS[extension] = applier


def store_metadata_to_instance(info: dict):
    """Construct a concrete zarr-python store instance from ``ZarrStoreInfo`` metadata.

    The inverse of :func:`store_instance_to_metadata`:

      - ``HttpStore`` -> a remote fsspec-backed store for the URL;
      - ``LocalStore`` -> a ``zarr.storage.LocalStore`` for the path;
      - ``MemoryStore`` -> raises (an in-memory store has no portable
        representation and must be provided directly).

    Any ``store_extensions`` are then applied outermost-last using appliers
    registered via :func:`register_store_extension`.
    """
    store_type = info["store_type"]
    params = info.get("store_params") or {}

    if store_type == "HttpStore":
        store = _http_store_from_url(params["url"])
    elif store_type == "LocalStore":
        from zarr.storage import LocalStore
        store = LocalStore(params["path"])
    elif store_type == "MemoryStore":
        raise ValueError(
            "Cannot reconstruct an in-memory store from metadata "
            f"({params.get('message')!r}); provide the store instance directly."
        )
    else:
        raise ValueError(f"Unknown store_type: {store_type!r}")

    for ext in info.get("store_extensions") or []:
        applier = _STORE_EXTENSION_APPLIERS.get(ext)
        if applier is None:
            raise ValueError(
                f"No applier registered for store extension {ext!r}. "
                "Register one via register_store_extension()."
            )
        store = applier(store)
    return store


def _http_store_from_url(url: str):
    """Construct a remote (fsspec-backed) zarr store from a URL."""
    from obstore.store import HTTPStore
    from zarr.storage import ObjectStore

    obs_store = HTTPStore.from_url(url)
    return ObjectStore(obs_store, read_only=True)


def _derive_store_url(store):
    """Best-effort extraction of a URL from a remote obstore-backed zarr store."""
    # zarr.storage.ObjectStore's .store should contain an obstore HTTPStore.
    obs_store = getattr(store, "store", None)
    url = getattr(obs_store, "url", None)
    if isinstance(url, str) and "://" in url:
        return url
    return None


class ZarrPeekResult(IntEnum):
    Pending = 0
    Fulfilled = 1
    Rejected = 2

# Cache for tracking completed async results.
# Maps a string cache key to either the result value or an Exception.
# TODO: replace with zarr-python's CacheStore
# Reference: https://github.com/zarr-developers/zarr-python/blob/82170464470197bcd816993aa059ee00dafee214/src/zarr/experimental/cache_store.py#L37
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
    cache_key = _get_cache_key(store_name, key)
    if cache_key not in _RESULT_CACHE:
        store = GLOBAL_STORES.get(store_name)
        if store is not None and hasattr(store, 'get_sync'):
            # Store supports synchronous resolution; populate the cache eagerly so the
            # status can be returned immediately without an async round-trip.
            try:
                _RESULT_CACHE[cache_key] = store.get_sync(key)
            except Exception as e:
                _RESULT_CACHE[cache_key] = e
    return _peek_status(cache_key)

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
