import zarr
from zarr.storage import MemoryStore, LocalStore
from zarr.abc.store import RangeByteRequest, SuffixByteRequest
from os.path import join, dirname

# Global mapping from store_name to Zarr store objects.

GLOBAL_STORES = {
    "my_store": LocalStore(join(dirname(__file__), "..", "..", "data", "out", "gaussian_quantiles.zarr")),
}

async def zarr_has(store_name: str, key: str) -> bool:
    """Check if a key exists in the Zarr store."""
    store = GLOBAL_STORES[store_name]
    #print(f"Checking existence of key '{key}' in store '{store_name}'")
    return await store.exists(key)

async def zarr_get(store_name: str, key: str) -> bytes:
    """Get the value for a key from the Zarr store."""
    store = GLOBAL_STORES[store_name]
    #print(f"Getting key '{key}' from store '{store_name}'")
    return (await store.get(key)).to_bytes()

async def zarr_get_range_from_offset(store_name: str, key: str, offset: int, length: int) -> bytes:
    """Get a byte range from a value in the Zarr store, specified by offset and length."""
    store = GLOBAL_STORES[store_name]
    #print(f"Getting range from offset {offset} with length {length} for key '{key}' from store '{store_name}'")
    return (await store.get(key, byte_range=RangeByteRequest(start=offset, end=offset+length))).to_bytes()

async def zarr_get_range_from_end(store_name: str, key: str, suffix_length: int) -> bytes:
    """Get a byte range from the end of a value in the Zarr store, specified by length."""
    store = GLOBAL_STORES[store_name]
    #print(f"Getting range from end with length {suffix_length} for key '{key}' from store '{store_name}'")
    return (await store.get(key, byte_range=SuffixByteRequest(suffix=suffix_length))).to_bytes()
