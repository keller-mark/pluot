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
        store = http_store_from_url(params["url"])
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


def http_store_from_url(url: str):
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
