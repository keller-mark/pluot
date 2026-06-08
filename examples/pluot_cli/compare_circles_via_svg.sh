#!/usr/bin/env bash
# compare_via_svg.sh
#
# Renders 10 PointLayers (one circle each, varying position and size) to both
# a .via_svg.png (SVG→PNG via resvg) and a .png (GPU raster), then reports
# the kompari pixel-diff score between them. A score of 0 means identical pixels.
#
# Requirements: a GPU must be available for the raster (.png) render.
# Usage: compare_via_svg.sh <output-dir>

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if [[ $# -ne 1 ]]; then
  echo "Usage: $0 <output-dir>" >&2
  exit 1
fi

WORK_DIR="$1"
mkdir -p "$WORK_DIR"

# ---------------------------------------------------------------------------
# Build
# ---------------------------------------------------------------------------
echo "Building binaries..." >&2
cd "$SCRIPT_DIR"
cargo build --bin pluot_cli --bin img_diff 2>&1 | grep -E '(Compiling|Finished|error)' >&2

PLUOT="$SCRIPT_DIR/target/debug/pluot_cli"
DIFF="$SCRIPT_DIR/target/debug/img_diff"

# ---------------------------------------------------------------------------
# Plot JSON: 10 PointLayers on an 800×600 canvas (pixel coordinates).
# Each layer has one circle at a different (x, y) with a different radius.
# Labels 0-9 select colours from the Tableau-10 palette.
#
# Layout: two rows of five (non-overlapping), radii 10–64 px.
#   Row 1 (y=150): x=100,250,400,550,700  radii=10,16,22,28,34
#   Row 2 (y=420): x=100,250,400,550,700  radii=40,46,52,58,64
# Plus a cluster of five overlapping circles near the centre (y≈295):
#   p10(340,290,r35) p11(395,270,r30) p12(435,305,r38) p13(370,335,r32) p14(415,325,r28)
# ---------------------------------------------------------------------------
cat > "$WORK_DIR/params.json" << 'EOF'
{
  "plot_type": "LayeredPlot",
  "plot_params": {
    "layers": [
      {
        "layer_type": "PointLayer",
        "layer_params": {
          "layer_id": "p0", "bounds": null,
          "data_unit_mode_x": "Pixels", "data_unit_mode_y": "Pixels",
          "point_radius": 10.0,
          "point_radius_unit_mode_x": "Pixels", "point_radius_unit_mode_y": "Pixels",
          "point_shape_mode": "Circle",
          "position_x": [100.0], "position_y": [150.0], "labels_vec": [0]
        }
      },
      {
        "layer_type": "PointLayer",
        "layer_params": {
          "layer_id": "p1", "bounds": null,
          "data_unit_mode_x": "Pixels", "data_unit_mode_y": "Pixels",
          "point_radius": 16.0,
          "point_radius_unit_mode_x": "Pixels", "point_radius_unit_mode_y": "Pixels",
          "point_shape_mode": "Circle",
          "position_x": [250.0], "position_y": [150.0], "labels_vec": [1]
        }
      },
      {
        "layer_type": "PointLayer",
        "layer_params": {
          "layer_id": "p2", "bounds": null,
          "data_unit_mode_x": "Pixels", "data_unit_mode_y": "Pixels",
          "point_radius": 22.0,
          "point_radius_unit_mode_x": "Pixels", "point_radius_unit_mode_y": "Pixels",
          "point_shape_mode": "Circle",
          "position_x": [400.0], "position_y": [150.0], "labels_vec": [2]
        }
      },
      {
        "layer_type": "PointLayer",
        "layer_params": {
          "layer_id": "p3", "bounds": null,
          "data_unit_mode_x": "Pixels", "data_unit_mode_y": "Pixels",
          "point_radius": 28.0,
          "point_radius_unit_mode_x": "Pixels", "point_radius_unit_mode_y": "Pixels",
          "point_shape_mode": "Circle",
          "position_x": [550.0], "position_y": [150.0], "labels_vec": [3]
        }
      },
      {
        "layer_type": "PointLayer",
        "layer_params": {
          "layer_id": "p4", "bounds": null,
          "data_unit_mode_x": "Pixels", "data_unit_mode_y": "Pixels",
          "point_radius": 34.0,
          "point_radius_unit_mode_x": "Pixels", "point_radius_unit_mode_y": "Pixels",
          "point_shape_mode": "Circle",
          "position_x": [700.0], "position_y": [150.0], "labels_vec": [4]
        }
      },
      {
        "layer_type": "PointLayer",
        "layer_params": {
          "layer_id": "p5", "bounds": null,
          "data_unit_mode_x": "Pixels", "data_unit_mode_y": "Pixels",
          "point_radius": 40.0,
          "point_radius_unit_mode_x": "Pixels", "point_radius_unit_mode_y": "Pixels",
          "point_shape_mode": "Circle",
          "position_x": [100.0], "position_y": [420.0], "labels_vec": [5]
        }
      },
      {
        "layer_type": "PointLayer",
        "layer_params": {
          "layer_id": "p6", "bounds": null,
          "data_unit_mode_x": "Pixels", "data_unit_mode_y": "Pixels",
          "point_radius": 46.0,
          "point_radius_unit_mode_x": "Pixels", "point_radius_unit_mode_y": "Pixels",
          "point_shape_mode": "Circle",
          "position_x": [250.0], "position_y": [420.0], "labels_vec": [6]
        }
      },
      {
        "layer_type": "PointLayer",
        "layer_params": {
          "layer_id": "p7", "bounds": null,
          "data_unit_mode_x": "Pixels", "data_unit_mode_y": "Pixels",
          "point_radius": 52.0,
          "point_radius_unit_mode_x": "Pixels", "point_radius_unit_mode_y": "Pixels",
          "point_shape_mode": "Circle",
          "position_x": [400.0], "position_y": [420.0], "labels_vec": [7]
        }
      },
      {
        "layer_type": "PointLayer",
        "layer_params": {
          "layer_id": "p8", "bounds": null,
          "data_unit_mode_x": "Pixels", "data_unit_mode_y": "Pixels",
          "point_radius": 58.0,
          "point_radius_unit_mode_x": "Pixels", "point_radius_unit_mode_y": "Pixels",
          "point_shape_mode": "Circle",
          "position_x": [550.0], "position_y": [420.0], "labels_vec": [8]
        }
      },
      {
        "layer_type": "PointLayer",
        "layer_params": {
          "layer_id": "p9", "bounds": null,
          "data_unit_mode_x": "Pixels", "data_unit_mode_y": "Pixels",
          "point_radius": 64.0,
          "point_radius_unit_mode_x": "Pixels", "point_radius_unit_mode_y": "Pixels",
          "point_shape_mode": "Circle",
          "position_x": [700.0], "position_y": [420.0], "labels_vec": [9]
        }
      },
      {
        "layer_type": "PointLayer",
        "layer_params": {
          "layer_id": "p10", "bounds": null,
          "data_unit_mode_x": "Pixels", "data_unit_mode_y": "Pixels",
          "point_radius": 35.0,
          "point_radius_unit_mode_x": "Pixels", "point_radius_unit_mode_y": "Pixels",
          "point_shape_mode": "Circle",
          "position_x": [340.0], "position_y": [290.0], "labels_vec": [0]
        }
      },
      {
        "layer_type": "PointLayer",
        "layer_params": {
          "layer_id": "p11", "bounds": null,
          "data_unit_mode_x": "Pixels", "data_unit_mode_y": "Pixels",
          "point_radius": 30.0,
          "point_radius_unit_mode_x": "Pixels", "point_radius_unit_mode_y": "Pixels",
          "point_shape_mode": "Circle",
          "position_x": [395.0], "position_y": [270.0], "labels_vec": [1]
        }
      },
      {
        "layer_type": "PointLayer",
        "layer_params": {
          "layer_id": "p12", "bounds": null,
          "data_unit_mode_x": "Pixels", "data_unit_mode_y": "Pixels",
          "point_radius": 38.0,
          "point_radius_unit_mode_x": "Pixels", "point_radius_unit_mode_y": "Pixels",
          "point_shape_mode": "Circle",
          "position_x": [435.0], "position_y": [305.0], "labels_vec": [2]
        }
      },
      {
        "layer_type": "PointLayer",
        "layer_params": {
          "layer_id": "p13", "bounds": null,
          "data_unit_mode_x": "Pixels", "data_unit_mode_y": "Pixels",
          "point_radius": 32.0,
          "point_radius_unit_mode_x": "Pixels", "point_radius_unit_mode_y": "Pixels",
          "point_shape_mode": "Circle",
          "position_x": [370.0], "position_y": [335.0], "labels_vec": [3]
        }
      },
      {
        "layer_type": "PointLayer",
        "layer_params": {
          "layer_id": "p14", "bounds": null,
          "data_unit_mode_x": "Pixels", "data_unit_mode_y": "Pixels",
          "point_radius": 28.0,
          "point_radius_unit_mode_x": "Pixels", "point_radius_unit_mode_y": "Pixels",
          "point_shape_mode": "Circle",
          "position_x": [415.0], "position_y": [325.0], "labels_vec": [4]
        }
      }
    ]
  }
}
EOF

# ---------------------------------------------------------------------------
# Render
# ---------------------------------------------------------------------------
echo "Rendering via_svg.png (SVG → PNG via resvg)..." >&2
"$PLUOT" --input "$WORK_DIR/params.json" --output "$WORK_DIR/out.via_svg.png" >&2

echo "Rendering out.png (GPU raster)..." >&2
"$PLUOT" --input "$WORK_DIR/params.json" --output "$WORK_DIR/out.png" >&2

# ---------------------------------------------------------------------------
# Pixel diff
# ---------------------------------------------------------------------------
echo "Comparing images..." >&2
SCORE="$("$DIFF" "$WORK_DIR/out.via_svg.png" "$WORK_DIR/out.png")"
echo "Pixel diff score (distance_sum; 0 = identical): $SCORE"
