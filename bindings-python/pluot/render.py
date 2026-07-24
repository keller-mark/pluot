from PIL import Image
import numpy as np
from zarr.abc.store import Store
from .zarr import GLOBAL_STORES, store_instance_to_metadata, store_metadata_to_instance, _http_store_from_url
from ._internal import render_py

NUM_EXTRA_BYTES = 1 # This needs to match on the rust side.

def parse_kwargs(kwargs):
    """Parse kwargs for render functions.

    Zarr stores are declared via the top-level ``stores`` map that
    ``RenderParams`` expects (store name -> ``ZarrStoreInfo`` metadata). Mirrors
    the `stores` useMemo in the JS/React binding (Pluot.jsx): callers supply
    stores in two mutually exclusive ways:

      - ``store=store_url_or_instance_or_metadata`` (optionally with
        ``store_name=...``) for a single store; or
      - ``stores={name: store_instance_or_metadata, ...}`` for one or more
        named stores (layers reference them by ``store_name``).

    ``store`` may be a URL string, a live ``zarr.abc.store.Store`` instance, or
    an already-derived ``ZarrStoreInfo`` dict; each value of ``stores`` may be
    a live instance or a dict. Live instances (including ones constructed from
    a URL string or reconstructed from a ``store=`` dict) are registered in
    ``GLOBAL_STORES`` (so the ``zarr_``-prefixed bound functions can reach
    them) and their portable metadata is derived for the ``stores`` field;
    dicts passed via ``stores=`` pass through as-is with no instance
    registered (the concrete instance, if any, must be registered separately).
    """
    new_kwargs = dict(kwargs)

    stores_arg = new_kwargs.pop("stores", None)
    store_arg = new_kwargs.pop("store", None)
    # Optional explicit name for the single-store `store=` argument.
    single_store_name = new_kwargs.pop("store_name", None)

    if (store_arg is not None or single_store_name is not None) and stores_arg is not None:
        raise ValueError("`store`/`store_name` (singular) are mutually exclusive with `stores` (plural).")

    stores_meta = {}

    # 1. Single-store convenience argument.
    if store_arg is not None:
        if isinstance(store_arg, str):
            # Assume `store_arg` is a URL; construct a remote store for it.
            name = single_store_name if single_store_name is not None else store_arg
            instance = _http_store_from_url(store_arg)
            GLOBAL_STORES[name] = instance
            stores_meta[name] = store_instance_to_metadata(instance)
        elif isinstance(store_arg, dict):
            # Already-derived ZarrStoreInfo metadata; reconstruct and register
            # a usable instance, but pass the given metadata through as-is.
            name = single_store_name if single_store_name is not None else "default"
            instance = store_metadata_to_instance(store_arg)
            GLOBAL_STORES[name] = instance
            stores_meta[name] = store_arg
        elif isinstance(store_arg, Store):
            # Use a deterministic name so the Rust-side cache key is stable across
            # re-renders (id(store) is stable for a given Python instance).
            name = single_store_name if single_store_name is not None else str(id(store_arg))
            GLOBAL_STORES[name] = store_arg
            stores_meta[name] = store_instance_to_metadata(store_arg)
        else:
            raise ValueError(
                "Expected `store` value to be a URL string, an instance of zarr.abc.store.Store, or a ZarrStoreInfo dict."
            )

    # 2. Explicit multi-store map.
    if stores_arg is not None:
        for name, value in stores_arg.items():
            if isinstance(value, Store):
                GLOBAL_STORES[name] = value
                stores_meta[name] = store_instance_to_metadata(value)
            elif isinstance(value, dict):
                # Already-derived ZarrStoreInfo metadata.
                instance = store_metadata_to_instance(value)
                GLOBAL_STORES[name] = instance
                stores_meta[name] = value
            else:
                raise ValueError(
                    "Each `stores` value must be a zarr Store instance or a ZarrStoreInfo dict."
                )

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
