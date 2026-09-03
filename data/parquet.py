# /// script
# requires-python = ">=3.12"
# dependencies = [
#   "zarr>=3",
#   "numpy>=2.0.0",
#   "pandas",
#   "anndata>=0.13.2",
#   "pyarrow"
# ]
# ///

from os.path import join

import pandas as pd
from anndata import AnnData

# Flights data
# Reference: https://idl.uw.edu/mosaic/examples/flights-10m.html
flights_path = join("out", "flights-10m.parquet")

flights_df = pd.read_parquet(flights_path)
flights_adata = AnnData(obs=flights_df)

flights_adata_path = join("out", "flights-10m.adata.zarr")
flights_adata.write_zarr(flights_adata_path)

# NYC taxi
# Reference: https://idl.uw.edu/mosaic/examples/nyc-taxi-rides.html
taxi_path = join("out", "nyc-rides-2010.parquet")
taxi_df = pd.read_parquet(taxi_path)

taxi_adata = AnnData(obs=taxi_df)

taxi_adata_path = join("out", "nyc-rides-2010.adata.zarr")
taxi_adata.write_zarr(taxi_adata_path)
