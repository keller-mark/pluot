import uuid
from PIL import Image
import numpy as np
import zarr
from zarr.storage import MemoryStore
from .zarr import GLOBAL_STORES
from ._internal import render_py

# Disable compression until Zarrs-via-WASM supports Blosc and Zstd.
# Reference: https://github.com/zarr-developers/zarr-python/issues/3389
no_compression = dict(filters=None, compressors=None, serializer="auto")

def replace_arr_with_key(d, store):
    """Replace _arr keys with _key keys in a dict, inserting NumPy array data into a Zarr store."""

    if isinstance(d, list):
        return [
            replace_arr_with_key(item, store)
            for item in d
        ]
    elif not isinstance(d, dict):
        return d  # Base case: not a dict, return as is.

    # D is a dict.
    new_d = {}
    for key, val in d.items():
        if key.endswith("_arr") and isinstance(val, np.ndarray):
            new_key = key.replace("_arr", "_key")
            new_val = f"/{new_key}_arr"
            zarr.create_array(
                store=store,
                data=val,
                name=new_val,
                **no_compression
            )
            new_d[new_key] = new_val
        else:
            # Recursively handle nested dicts
            new_d[key] = replace_arr_with_key(val, store)
    return new_d

# Helper function to convert _arr params to _key params,
# inserting NumPy array data into an in-memory Zarr store.
def parse_kwargs(kwargs):
    """Parse kwargs for render functions."""
    kwargs_has_store = "store_name" in kwargs
    kwargs_has_plot_params = "plot_params" in kwargs
    new_kwargs = kwargs
    if (not kwargs_has_store) and kwargs_has_plot_params:
        store_name = str(uuid.uuid4())
        new_kwargs = {
            "store_name": store_name,
            **kwargs,
            "plot_params": {},
        }
        GLOBAL_STORES[store_name] = MemoryStore()
        # recursively traverse to find _keys
        new_kwargs["plot_params"] = replace_arr_with_key(kwargs["plot_params"], GLOBAL_STORES[store_name])
    return new_kwargs

async def render(**kwargs):
    """Render to raw bytes."""
    # We wrap the internal function here to be able to provide types, docstrings, etc.
    new_kwargs = parse_kwargs(kwargs)
    result = await render_py(timeout=None, wait_for_store_gets=True, cache_enabled=True, device_pixel_ratio=1.0, aspect_ratio_mode="Contain", aspect_ratio_alignment_mode="Center", format="Raster", view_mode="2d", pickable=False, svg_compression_enabled=False, svg_include_document=True, **new_kwargs)
    return result

async def render_to_array(**kwargs):
    """Render to a NumPy array, with shape (height, width, RGBA)."""
    width = kwargs["width"]
    height = kwargs["height"]
    result = await render(**kwargs)
    NUM_EXTRA_BYTES = 1 # This needs to match on the rust side.
    arr = np.frombuffer(result[:-NUM_EXTRA_BYTES], dtype=np.dtype('uint8')).reshape((height, width, 4))
    return arr

async def render_to_image(**kwargs):
    arr = await render_to_array(**kwargs)
    img = Image.fromarray(arr)
    return img
