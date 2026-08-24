# /// script
# requires-python = ">=3.12"
# dependencies = [
#   "zarr>=3",
#   "numpy>=2.0.0",
#   "pandas",
#   "spatialdata==0.8.0"
# ]
# ///

import spatialdata as sd
from os.path import join

sdata_blobs = sd.datasets.blobs()

sdata_blobs_path = join("out", "blobs.sdata.zarr")
sdata_blobs.write(sdata_blobs_path)
