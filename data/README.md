# Scripts to generate sample datasets for Pluot

```sh
cd data
# Generate MNIST dataset
uv run mnist.py
```

OME-NGFF v0.5 files from https://idr.github.io/ome-ngff-samples/

```sh
aws s3 sync --endpoint-url https://uk1s3.embassy.ebi.ac.uk --no-sign-request s3://idr/zarr/v0.5/idr0062A/6001240_labels.zarr/ 6001240_labels.ome.zarr
```

