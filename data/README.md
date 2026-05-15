# Scripts to generate sample datasets for Pluot

```sh
cd data
# Generate MNIST dataset
uv run mnist.py
```

OME-NGFF v0.5 files from https://idr.github.io/ome-ngff-samples/

```sh
uv run --with awscli aws s3 sync --endpoint-url https://livingobjects.ebi.ac.uk --no-sign-request s3://idr/zarr/v0.5/idr0062A/6001240_labels.zarr/ 6001240_labels.ome.zarr
mv 6001240_labels.ome.zarr out/
# No longer needed
# uv run remove_ngff_compression.py

uv run --with awscli aws s3 sync --endpoint-url https://livingobjects.ebi.ac.uk --no-sign-request "s3://idr/zarr/v0.5/idr0157/Asterella gracilis SWE/IMG_1033-1112 Asterella gracilis (Mannia gracilis) stature.ome.zarr" "IMG_1033-1112 Asterella gracilis (Mannia gracilis) stature.ome.zarr"
```


Copying to R2 S3:

- Configure rclone: https://developers.cloudflare.com/r2/examples/rclone/
- Install rclone: https://developers.cloudflare.com/r2/examples/rclone/
- Setup rclone for r2: https://rclone.org/s3/#cloudflare-r2

OR:

Set AWS env vars, including `AWS_ENDPOINT_URL` and `AWS_DEFAULT_REGION="auto"`. Then, use the `aws s3 cp` command.
