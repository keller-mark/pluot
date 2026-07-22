#!/usr/bin/env bash
set -euo pipefail

# Renders this plot via the `pluot_cli` example (examples/pluot_cli),
# which reads the plot/layer params (and any `stores`) as JSON (piped
# below via a heredoc on stdin) and every other rendering parameter
# as a CLI flag.
#
# `HttpStore`/`LocalStore` entries in `stores` are backed by real
# `zarrs_http`/`zarrs_filesystem` instances; `MemoryStore` entries are
# rejected, since the CLI has no generic byte payload to construct
# one from.

# Build the CLI once (run from the root of the pluot repository).
cargo build --release -p pluot_cli
PLUOT_CLI="$(dirname "$0")/target/release/pluot_cli"

# `--output`'s extension selects the backend: .svg (vector), .png
# (GPU raster), or .via_svg.png (vector rendered to PNG via resvg).
"$PLUOT_CLI" \
  --output plot.png \
  --width 640 \
  --height 480 \
  --device_pixel_ratio 1.0 \
  --aspect_ratio_mode contain \
  --view_mode 2d \
  --camera_view "0.15000000596046448,0.0,0.0,0.0,0.0,0.15000000596046448,0.0,0.0,0.0,0.0,1.0,0.0,0.0,0.0,0.0,1.0" \
  --plot_id "plot_1" \
  --margin_left 60.0 \
  <<'JSON'
{
  "plot_type": "LayeredPlot",
  "plot_params": {
    "layers": [
      {
        "layer_type": "PointLayer",
        "layer_params": {
          "layer_id": "pts",
          "bounds": null,
          "data_unit_mode_x": "Data",
          "data_unit_mode_y": "Data",
          "point_radius_unit_mode_x": "Pixels",
          "point_radius_unit_mode_y": "Pixels",
          "point_shape_mode": "Circle",
          "model_matrix": null,
          "point_radius": {
            "size_mode": "UniformSize",
            "size_params": 5.0
          },
          "fill_color": {
            "color_mode": "Categorical",
            "color_params": {
              "codes": {
                "dtype": "Uint8",
                "values": [
                  0,
                  1,
                  2,
                  3
                ]
              },
              "colormap": "Tableau10"
            }
          },
          "fill_opacity": null,
          "stroke_width_unit_mode": "Pixels",
          "stroke_color": null,
          "stroke_opacity": null,
          "stroke_width": null,
          "position_x": {
            "dtype": "Float32",
            "values": [
              0.0,
              1.0,
              1.0,
              0.0
            ]
          },
          "position_y": {
            "dtype": "Float32",
            "values": [
              0.0,
              0.0,
              1.0,
              1.0
            ]
          }
        }
      },
      {
        "layer_type": "AxisLinearLayer",
        "layer_params": {
          "layer_id": "left_axis",
          "position": "Left"
        }
      }
    ]
  },
  "stores": {
    "my_store": {
      "store_type": "HttpStore",
      "store_params": {
        "url": "https://example.com/my_store.zarr",
        "options": null
      },
      "store_extensions": null
    }
  }
}
JSON
