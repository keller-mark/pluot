from PIL import Image
import numpy as np
from zarr.abc.store import Store
from pluot_core.zarr import store_instance_to_metadata, store_metadata_to_instance, http_store_from_url
from pluot_core.viewport import Bounds, Margins, ViewportParams, get_camera_matrix_from_bounds
from .zarr import GLOBAL_STORES
from ._internal import render_py, render_to_script_py, extent_py

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
            instance = http_store_from_url(store_arg)
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

_RENDER_DEFAULTS = dict(
    schema_version=None,
    timeout=None,
    wait_for_store_gets=True,
    cache_enabled=True,
    device_pixel_ratio=1.0,
    format="Raster",
    aspect_ratio_mode="Contain",
    aspect_ratio_alignment_mode="Center",
    view_mode="2d",
    pickable=False,
    svg_compression_enabled=False,
    svg_include_document=True
)

def _check_lims(merged_params, x_lim, y_lim):
    if merged_params.get("camera_view") is not None and (x_lim is not None or y_lim is not None):
        raise ValueError("`camera_view` is mutually exclusive with `x_lim`/`y_lim`.")


def _union_extent(extent_result):
    """The bounding box of every layer's extent, or None when no layer reports one."""
    layer_results = extent_result["layer_results"]
    if not layer_results:
        return None
    x_lim = (min(r["x"][0] for r in layer_results), max(r["x"][1] for r in layer_results))
    y_lim = (min(r["y"][0] for r in layer_results), max(r["y"][1] for r in layer_results))
    return x_lim, y_lim


def _camera_view_from_lims(merged_params, x_lim, y_lim):
    viewport_params = ViewportParams(
        width=merged_params["width"],
        height=merged_params["height"],
        aspect_ratio_mode=merged_params["aspect_ratio_mode"],
        aspect_ratio_alignment_mode=merged_params["aspect_ratio_alignment_mode"],
        margins=Margins(
            margin_top=merged_params.get("margin_top") or 0.0,
            margin_right=merged_params.get("margin_right") or 0.0,
            margin_bottom=merged_params.get("margin_bottom") or 0.0,
            margin_left=merged_params.get("margin_left") or 0.0,
        ),
    )
    bounds = Bounds(x_min=x_lim[0], x_max=x_lim[1], y_min=y_lim[0], y_max=y_lim[1])
    # The previous camera only fills in missing bounds, and all bounds are given.
    identity = np.eye(4, dtype=np.float32).flatten()
    return get_camera_matrix_from_bounds(bounds, identity, viewport_params).tolist()


async def _resolve_camera_view(merged_params, x_lim, y_lim):
    """Compute the camera matrix showing `x_lim`/`y_lim`, running the extent
    query to fill in whichever of them is unspecified.

    Returns None (the default camera) when the extent query is needed but no
    layer reports an extent, or for 3D plots, which the bounds-to-camera
    conversion does not yet support.
    """
    if merged_params["view_mode"] == "3d":
        return None

    if x_lim is None or y_lim is None:
        extent_result = await extent_py(**{**merged_params, "camera_view": None})
        union = _union_extent(extent_result)
        if union is None:
            return None
        extent_x, extent_y = union
        x_lim = x_lim if x_lim is not None else extent_x
        y_lim = y_lim if y_lim is not None else extent_y

    return _camera_view_from_lims(merged_params, x_lim, y_lim)


async def extent(**kwargs):
    """Compute the data extent of each layer.

    Returns a dict ``{"layer_results": [{"layer_id", "x", "y", "z"}, ...]}``
    where ``x``/``y``/``z`` are ``(min, max)`` pairs (``z`` is None for 2D layers).
    """
    new_kwargs = parse_kwargs(kwargs)

    merged_params = {**_RENDER_DEFAULTS, **new_kwargs}

    return await extent_py(**merged_params)


async def render(x_lim=None, y_lim=None, **kwargs):
    """Render to raw bytes.

    The view is determined by ``camera_view`` (a 16-element camera matrix) or,
    alternatively, by ``x_lim``/``y_lim`` (``(min, max)`` data ranges). When
    ``camera_view`` is omitted, any unspecified limit is filled in from the
    union of the layer extents.
    """
    # We wrap the internal function here to be able to provide types, docstrings, etc.
    new_kwargs = parse_kwargs(kwargs)

    merged_params = {**_RENDER_DEFAULTS, **new_kwargs}

    _check_lims(merged_params, x_lim, y_lim)
    if merged_params.get("camera_view") is None:
        merged_params["camera_view"] = await _resolve_camera_view(merged_params, x_lim, y_lim)

    result = await render_py(**merged_params)
    return result

def render_to_script(x_lim=None, y_lim=None, **kwargs):
    """Render to a code string.

    Unlike ``render``, both ``x_lim`` and ``y_lim`` must be given to replace
    ``camera_view``, since filling in a missing limit requires the async
    extent query.
    """
    # We wrap the internal function here to be able to provide types, docstrings, etc.
    new_kwargs = parse_kwargs(kwargs)

    merged_params = {**_RENDER_DEFAULTS, **new_kwargs}

    _check_lims(merged_params, x_lim, y_lim)
    if (x_lim is None) != (y_lim is None):
        raise ValueError("render_to_script requires both `x_lim` and `y_lim`, or neither.")
    if x_lim is not None and merged_params["view_mode"] != "3d":
        merged_params["camera_view"] = _camera_view_from_lims(merged_params, x_lim, y_lim)

    result = render_to_script_py(**merged_params)
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
    return result[:-NUM_EXTRA_BYTES].decode("utf-8")
