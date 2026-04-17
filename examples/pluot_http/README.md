# pluot_http example

From the root of the repository, run:

```sh
cargo run --manifest-path examples/pluot_http/Cargo.toml
```

## API

`POST /render-svg` and `POST /render-png` accept a JSON body whose fields match
`RenderParams`. Only `layers` is required; everything else falls back to its
default value. The `format` field is ignored — it is set by the endpoint.

## Example request

```sh
curl -X POST http://127.0.0.1:7878/render-svg \
  -H 'Content-Type: application/json' \
  -d '{
    "layers": [
      {
        "layer_type": "PointLayer",
        "layer_params": {
          "layer_id": "my_points",
          "bounds": null,
          "data_unit_mode_x": "Pixels",
          "data_unit_mode_y": "Pixels",
          "point_radius": 12.0,
          "point_radius_unit_mode_x": "Pixels",
          "point_radius_unit_mode_y": "Pixels",
          "point_shape_mode": "Circle",
          "position_x": [80.0, 200.0, 320.0],
          "position_y": [100.0, 250.0, 80.0],
          "labels_vec": [0, 1, 2]
        }
      }
    ],
    "width": 400,
    "height": 300
  }' \
  -o output.svg
```

Replace `/render-svg` with `/render-png` and `-o output.svg` with `-o output.png`
to get a raster image instead.
