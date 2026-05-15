# Note: no longer needed.
import zarr
from os.path import join


store = zarr.storage.LocalStore(join(".", "out", "6001240_labels.ome.zarr"))
z = zarr.open(store)

# Disable compression until Zarrs-via-WASM supports Blosc and Zstd.
# Reference: https://github.com/zarr-developers/zarr-python/issues/3389
no_compression = dict(filters=None, compressors=None, serializer="auto")

for dataset in z.attrs["ome"]["multiscales"][0]["datasets"]:
    arr = z[f"/{dataset['path']}"]
    z.create_array(name=f"/{dataset['path']}_nc", data=arr[()], **no_compression, overwrite=True)
