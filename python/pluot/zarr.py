
# Global mapping from store_name to Zarr store objects.
GLOBAL_STORES = {}

def zarr_has(store_name: str, key: str) -> bool:
    """Check if a key exists in the Zarr store."""
    pass


def zarr_get(store_name: str, key: str) -> bytes:
    """Get the value for a key from the Zarr store."""
    pass

def zarr_get_range_from_offset(store_name: str, key: str, offset: int, length: int) -> bytes:
    """Get a byte range from a value in the Zarr store, specified by offset and length."""
    pass

def zarr_get_range_from_end(store_name: str, key: str, suffix_length: int) -> bytes:
    """Get a byte range from the end of a value in the Zarr store, specified by length."""
    pass