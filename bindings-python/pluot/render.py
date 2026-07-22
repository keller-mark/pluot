from PIL import Image
import numpy as np
from zarr.abc.store import Store
from .zarr import GLOBAL_STORES, store_instance_to_metadata
from ._internal import render_py

NUM_EXTRA_BYTES = 1 # This needs to match on the rust side.

def parse_kwargs(kwargs):
    """Parse kwargs for render functions.

    Zarr stores are declared via the top-level ``stores`` map that
    ``RenderParams`` expects (store name -> ``ZarrStoreInfo`` metadata). Callers
    can supply stores in two ways:

      - ``stores={name: store_instance_or_metadata, ...}`` for one or more
        named stores (layers reference them by ``store_name``); or
      - ``store=store_instance`` (optionally with ``store_name=...``) for a
        single store.

    In every case we register the concrete store instance(s) in
    ``GLOBAL_STORES`` (so the ``zarr_``-prefixed bound functions can reach them)
    and derive each store's portable metadata for the ``stores`` field.
    """
    new_kwargs = dict(kwargs)

    stores_arg = new_kwargs.pop("stores", None)
    store_arg = new_kwargs.pop("store", None)
    # Optional explicit name for the single-store `store=` argument.
    single_store_name = new_kwargs.pop("store_name", None)

    stores_meta = {}

    # 1. Explicit multi-store map.
    if stores_arg is not None:
        for name, value in stores_arg.items():
            if isinstance(value, Store):
                GLOBAL_STORES[name] = value
                stores_meta[name] = store_instance_to_metadata(value)
            elif isinstance(value, dict):
                # Already-derived ZarrStoreInfo metadata. The concrete instance
                # (if any) must be registered separately (e.g. in GLOBAL_STORES).
                stores_meta[name] = value
            else:
                raise ValueError(
                    "Each `stores` value must be a zarr Store instance or a ZarrStoreInfo dict."
                )

    # 2. Single-store convenience argument.
    if store_arg is not None:
        if not isinstance(store_arg, Store):
            raise ValueError("Expected `store` value to be an instance of zarr.abc.store.Store")
        # Use a deterministic name so the Rust-side cache key is stable across
        # re-renders (id(store) is stable for a given Python instance).
        name = single_store_name if single_store_name is not None else str(id(store_arg))
        GLOBAL_STORES[name] = store_arg
        stores_meta[name] = store_instance_to_metadata(store_arg)

    if stores_meta:
        new_kwargs["stores"] = stores_meta

    return new_kwargs

_RENDER_DEFAULTS = dict(timeout=None, wait_for_store_gets=True, cache_enabled=True, device_pixel_ratio=1.0, format="Raster", aspect_ratio_mode="Contain", aspect_ratio_alignment_mode="Center", view_mode="2d", pickable=False, svg_compression_enabled=False, svg_include_document=True)

async def render(**kwargs):
    """Render to raw bytes."""
    # We wrap the internal function here to be able to provide types, docstrings, etc.
    new_kwargs = parse_kwargs(kwargs)

    merged_params = {**_RENDER_DEFAULTS, **new_kwargs}

    result = await render_py(**merged_params)
    return result

async def render_raw(**kwargs):
    """Render to raw bytes, bypassing parse_kwargs.

    The caller is responsible for passing a ready ``stores`` metadata map and for
    ensuring each referenced store name is already registered in
    ``GLOBAL_STORES`` before calling this function.
    """
    merged_params = {**_RENDER_DEFAULTS, **kwargs}
    return await render_py(**merged_params)

async def render_to_array(**kwargs):
    """Render to a NumPy array, with shape (height, width, RGBA)."""
    width = kwargs["width"]
    height = kwargs["height"]
    result = await render(**kwargs)
    arr = np.frombuffer(result[:-NUM_EXTRA_BYTES], dtype=np.dtype('uint8')).reshape((height, width, 4))
    return arr

async def render_to_image(**kwargs):
    arr = await render_to_array(**kwargs)
    img = Image.fromarray(arr)
    return img

async def render_to_svg(**kwargs):
    """Render to an SVG string."""
    result = await render(**kwargs, format="Vector")
    # TODO: account for bailed_early extra byte (once appended to SVG outputs on the Rust side)
    return result.decode("utf-8")
