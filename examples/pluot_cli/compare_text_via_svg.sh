#!/usr/bin/env bash
# compare_text_via_svg.sh
#
# Renders TextLayers (varying position, size, alignment, and baseline) to both
# a .via_svg.png (SVG-->PNG via resvg) and a .png (GPU raster), then reports
# the kompari pixel-diff score between them. A score of 0 means identical pixels.
#
# The same TTF font (NimbusSans-Regular from vendor/) is registered in both
# rendering paths so the comparison measures shader/rasterisation differences,
# not font differences.
#
# Requirements: a GPU must be available for the raster (.png) render.
# Usage: compare_text_via_svg.sh <output-dir>

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FONT_PATH="$SCRIPT_DIR/../../vendor/urw-core35-fonts/NimbusSans-Regular.ttf"

if [[ $# -ne 1 ]]; then
  echo "Usage: $0 <output-dir>" >&2
  exit 1
fi

WORK_DIR="$1"
mkdir -p "$WORK_DIR"

# Build
echo "Building binaries..." >&2
cd "$SCRIPT_DIR"
cargo build --bin pluot_cli --bin img_diff 2>&1 | grep -E '(Compiling|Finished|error)' >&2

PLUOT="$SCRIPT_DIR/target/debug/pluot_cli"
DIFF="$SCRIPT_DIR/target/debug/img_diff"

# Plot JSON: TextLayers on an 800x600 canvas (pixel coordinates).
#
# Layout:
#   Row 1 (y=100): 5 labels at 12 px, middle-aligned, alphabetic baseline
#   Row 2 (y=220): 4 labels at 24 px, mixed alignment (start/middle/end)
#   Row 3 (y=360): 3 labels at 36 px, middle-aligned, middle baseline
#   Row 4 (y=480): 3 labels at 48 px, middle-aligned, top/middle/bottom baseline
#
# All layers use Pixels data_unit_mode and no explicit font_name so that
# both code paths use the bundled NimbusSans-Regular.ttf.
cat > "$WORK_DIR/params.json" << 'EOF'
{
  "plot_type": "LayeredPlot",
  "plot_params": {
    "layers": [
      {
        "layer_type": "TextLayer",
        "layer_params": {
          "layer_id": "t0",
          "bounds": null,
          "data_unit_mode_x": "Pixels",
          "data_unit_mode_y": "Pixels",
          "text_size": 12.0,
          "text_size_unit_mode": "Pixels",
          "text_align_mode": "Middle",
          "text_baseline_mode": "Alphabetic",
          "text_rotation": null,
          "font_family": null,
          "font_weight": "Normal",
          "font_style": "Normal",
          "position_x": [80.0],
          "position_y": [100.0],
          "text_vec": ["Hello"]
        }
      },
      {
        "layer_type": "TextLayer",
        "layer_params": {
          "layer_id": "t1",
          "bounds": null,
          "data_unit_mode_x": "Pixels",
          "data_unit_mode_y": "Pixels",
          "text_size": 12.0,
          "text_size_unit_mode": "Pixels",
          "text_align_mode": "Middle",
          "text_baseline_mode": "Alphabetic",
          "text_rotation": null,
          "font_family": null,
          "font_weight": "Normal",
          "font_style": "Normal",
          "position_x": [230.0],
          "position_y": [100.0],
          "text_vec": ["World"]
        }
      },
      {
        "layer_type": "TextLayer",
        "layer_params": {
          "layer_id": "t2",
          "bounds": null,
          "data_unit_mode_x": "Pixels",
          "data_unit_mode_y": "Pixels",
          "text_size": 12.0,
          "text_size_unit_mode": "Pixels",
          "text_align_mode": "Middle",
          "text_baseline_mode": "Alphabetic",
          "text_rotation": null,
          "font_family": null,
          "font_weight": "Normal",
          "font_style": "Normal",
          "position_x": [400.0],
          "position_y": [100.0],
          "text_vec": ["Cluster 1"]
        }
      },
      {
        "layer_type": "TextLayer",
        "layer_params": {
          "layer_id": "t3",
          "bounds": null,
          "data_unit_mode_x": "Pixels",
          "data_unit_mode_y": "Pixels",
          "text_size": 12.0,
          "text_size_unit_mode": "Pixels",
          "text_align_mode": "Middle",
          "text_baseline_mode": "Alphabetic",
          "text_rotation": null,
          "font_family": null,
          "font_weight": "Normal",
          "font_style": "Normal",
          "position_x": [570.0],
          "position_y": [100.0],
          "text_vec": ["Cluster 2"]
        }
      },
      {
        "layer_type": "TextLayer",
        "layer_params": {
          "layer_id": "t4",
          "bounds": null,
          "data_unit_mode_x": "Pixels",
          "data_unit_mode_y": "Pixels",
          "text_size": 12.0,
          "text_size_unit_mode": "Pixels",
          "text_align_mode": "Middle",
          "text_baseline_mode": "Alphabetic",
          "text_rotation": null,
          "font_family": null,
          "font_weight": "Normal",
          "font_style": "Normal",
          "position_x": [720.0],
          "position_y": [100.0],
          "text_vec": ["Label"]
        }
      },
      {
        "layer_type": "TextLayer",
        "layer_params": {
          "layer_id": "t5",
          "bounds": null,
          "data_unit_mode_x": "Pixels",
          "data_unit_mode_y": "Pixels",
          "text_size": 24.0,
          "text_size_unit_mode": "Pixels",
          "text_align_mode": "Start",
          "text_baseline_mode": "Alphabetic",
          "text_rotation": null,
          "font_family": null,
          "font_weight": "Normal",
          "font_style": "Normal",
          "position_x": [60.0],
          "position_y": [220.0],
          "text_vec": ["Start"]
        }
      },
      {
        "layer_type": "TextLayer",
        "layer_params": {
          "layer_id": "t6",
          "bounds": null,
          "data_unit_mode_x": "Pixels",
          "data_unit_mode_y": "Pixels",
          "text_size": 24.0,
          "text_size_unit_mode": "Pixels",
          "text_align_mode": "Middle",
          "text_baseline_mode": "Alphabetic",
          "text_rotation": null,
          "font_family": null,
          "font_weight": "Normal",
          "font_style": "Normal",
          "position_x": [280.0],
          "position_y": [220.0],
          "text_vec": ["Middle"]
        }
      },
      {
        "layer_type": "TextLayer",
        "layer_params": {
          "layer_id": "t7",
          "bounds": null,
          "data_unit_mode_x": "Pixels",
          "data_unit_mode_y": "Pixels",
          "text_size": 24.0,
          "text_size_unit_mode": "Pixels",
          "text_align_mode": "End",
          "text_baseline_mode": "Alphabetic",
          "text_rotation": null,
          "font_family": null,
          "font_weight": "Normal",
          "font_style": "Normal",
          "position_x": [530.0],
          "position_y": [220.0],
          "text_vec": ["End"]
        }
      },
      {
        "layer_type": "TextLayer",
        "layer_params": {
          "layer_id": "t8",
          "bounds": null,
          "data_unit_mode_x": "Pixels",
          "data_unit_mode_y": "Pixels",
          "text_size": 24.0,
          "text_size_unit_mode": "Pixels",
          "text_align_mode": "Middle",
          "text_baseline_mode": "Middle",
          "text_rotation": null,
          "font_family": null,
          "font_weight": "Normal",
          "font_style": "Normal",
          "position_x": [720.0],
          "position_y": [220.0],
          "text_vec": ["Mid-B"]
        }
      },
      {
        "layer_type": "TextLayer",
        "layer_params": {
          "layer_id": "t9",
          "bounds": null,
          "data_unit_mode_x": "Pixels",
          "data_unit_mode_y": "Pixels",
          "text_size": 36.0,
          "text_size_unit_mode": "Pixels",
          "text_align_mode": "Middle",
          "text_baseline_mode": "Middle",
          "text_rotation": null,
          "font_family": null,
          "font_weight": "Normal",
          "font_style": "Normal",
          "position_x": [160.0],
          "position_y": [360.0],
          "text_vec": ["Abc"]
        }
      },
      {
        "layer_type": "TextLayer",
        "layer_params": {
          "layer_id": "t10",
          "bounds": null,
          "data_unit_mode_x": "Pixels",
          "data_unit_mode_y": "Pixels",
          "text_size": 36.0,
          "text_size_unit_mode": "Pixels",
          "text_align_mode": "Middle",
          "text_baseline_mode": "Middle",
          "text_rotation": null,
          "font_family": null,
          "font_weight": "Normal",
          "font_style": "Normal",
          "position_x": [400.0],
          "position_y": [360.0],
          "text_vec": ["XyZ 123"]
        }
      },
      {
        "layer_type": "TextLayer",
        "layer_params": {
          "layer_id": "t11",
          "bounds": null,
          "data_unit_mode_x": "Pixels",
          "data_unit_mode_y": "Pixels",
          "text_size": 36.0,
          "text_size_unit_mode": "Pixels",
          "text_align_mode": "Middle",
          "text_baseline_mode": "Middle",
          "text_rotation": null,
          "font_family": null,
          "font_weight": "Normal",
          "font_style": "Normal",
          "position_x": [660.0],
          "position_y": [360.0],
          "text_vec": ["glyph"]
        }
      },
      {
        "layer_type": "TextLayer",
        "layer_params": {
          "layer_id": "t12",
          "bounds": null,
          "data_unit_mode_x": "Pixels",
          "data_unit_mode_y": "Pixels",
          "text_size": 48.0,
          "text_size_unit_mode": "Pixels",
          "text_align_mode": "Middle",
          "text_baseline_mode": "Top",
          "text_rotation": null,
          "font_family": null,
          "font_weight": "Normal",
          "font_style": "Normal",
          "position_x": [160.0],
          "position_y": [480.0],
          "text_vec": ["Top"]
        }
      },
      {
        "layer_type": "TextLayer",
        "layer_params": {
          "layer_id": "t13",
          "bounds": null,
          "data_unit_mode_x": "Pixels",
          "data_unit_mode_y": "Pixels",
          "text_size": 48.0,
          "text_size_unit_mode": "Pixels",
          "text_align_mode": "Middle",
          "text_baseline_mode": "Middle",
          "text_rotation": null,
          "font_family": null,
          "font_weight": "Normal",
          "font_style": "Normal",
          "position_x": [400.0],
          "position_y": [480.0],
          "text_vec": ["Mid"]
        }
      },
      {
        "layer_type": "TextLayer",
        "layer_params": {
          "layer_id": "t14",
          "bounds": null,
          "data_unit_mode_x": "Pixels",
          "data_unit_mode_y": "Pixels",
          "text_size": 48.0,
          "text_size_unit_mode": "Pixels",
          "text_align_mode": "Middle",
          "text_baseline_mode": "Bottom",
          "text_rotation": null,
          "font_family": null,
          "font_weight": "Normal",
          "font_style": "Normal",
          "position_x": [640.0],
          "position_y": [480.0],
          "text_vec": ["Bot"]
        }
      },
      {
        "layer_type": "TextLayer",
        "layer_params": {
          "layer_id": "t15",
          "bounds": null,
          "data_unit_mode_x": "Pixels",
          "data_unit_mode_y": "Pixels",
          "text_size": 8.0,
          "text_size_unit_mode": "Pixels",
          "text_align_mode": "Middle",
          "text_baseline_mode": "Alphabetic",
          "text_rotation": null,
          "font_family": null,
          "font_weight": "Normal",
          "font_style": "Normal",
          "position_x": [50.0],
          "position_y": [15.0],
          "text_vec": ["tiny a"]
        }
      },
      {
        "layer_type": "TextLayer",
        "layer_params": {
          "layer_id": "t16",
          "bounds": null,
          "data_unit_mode_x": "Pixels",
          "data_unit_mode_y": "Pixels",
          "text_size": 8.0,
          "text_size_unit_mode": "Pixels",
          "text_align_mode": "Middle",
          "text_baseline_mode": "Alphabetic",
          "text_rotation": null,
          "font_family": null,
          "font_weight": "Normal",
          "font_style": "Normal",
          "position_x": [150.0],
          "position_y": [15.0],
          "text_vec": ["tiny b"]
        }
      },
      {
        "layer_type": "TextLayer",
        "layer_params": {
          "layer_id": "t17",
          "bounds": null,
          "data_unit_mode_x": "Pixels",
          "data_unit_mode_y": "Pixels",
          "text_size": 8.0,
          "text_size_unit_mode": "Pixels",
          "text_align_mode": "Middle",
          "text_baseline_mode": "Alphabetic",
          "text_rotation": null,
          "font_family": null,
          "font_weight": "Normal",
          "font_style": "Normal",
          "position_x": [250.0],
          "position_y": [15.0],
          "text_vec": ["tiny c"]
        }
      },
      {
        "layer_type": "TextLayer",
        "layer_params": {
          "layer_id": "t18",
          "bounds": null,
          "data_unit_mode_x": "Pixels",
          "data_unit_mode_y": "Pixels",
          "text_size": 8.0,
          "text_size_unit_mode": "Pixels",
          "text_align_mode": "Middle",
          "text_baseline_mode": "Alphabetic",
          "text_rotation": null,
          "font_family": null,
          "font_weight": "Normal",
          "font_style": "Normal",
          "position_x": [350.0],
          "position_y": [15.0],
          "text_vec": ["tiny d"]
        }
      },
      {
        "layer_type": "TextLayer",
        "layer_params": {
          "layer_id": "t19",
          "bounds": null,
          "data_unit_mode_x": "Pixels",
          "data_unit_mode_y": "Pixels",
          "text_size": 8.0,
          "text_size_unit_mode": "Pixels",
          "text_align_mode": "Middle",
          "text_baseline_mode": "Alphabetic",
          "text_rotation": null,
          "font_family": null,
          "font_weight": "Normal",
          "font_style": "Normal",
          "position_x": [450.0],
          "position_y": [15.0],
          "text_vec": ["tiny e"]
        }
      },
      {
        "layer_type": "TextLayer",
        "layer_params": {
          "layer_id": "t20",
          "bounds": null,
          "data_unit_mode_x": "Pixels",
          "data_unit_mode_y": "Pixels",
          "text_size": 8.0,
          "text_size_unit_mode": "Pixels",
          "text_align_mode": "Middle",
          "text_baseline_mode": "Alphabetic",
          "text_rotation": null,
          "font_family": null,
          "font_weight": "Normal",
          "font_style": "Normal",
          "position_x": [550.0],
          "position_y": [15.0],
          "text_vec": ["tiny f"]
        }
      },
      {
        "layer_type": "TextLayer",
        "layer_params": {
          "layer_id": "t21",
          "bounds": null,
          "data_unit_mode_x": "Pixels",
          "data_unit_mode_y": "Pixels",
          "text_size": 8.0,
          "text_size_unit_mode": "Pixels",
          "text_align_mode": "Middle",
          "text_baseline_mode": "Alphabetic",
          "text_rotation": null,
          "font_family": null,
          "font_weight": "Normal",
          "font_style": "Normal",
          "position_x": [650.0],
          "position_y": [15.0],
          "text_vec": ["tiny g"]
        }
      },
      {
        "layer_type": "TextLayer",
        "layer_params": {
          "layer_id": "t22",
          "bounds": null,
          "data_unit_mode_x": "Pixels",
          "data_unit_mode_y": "Pixels",
          "text_size": 8.0,
          "text_size_unit_mode": "Pixels",
          "text_align_mode": "Middle",
          "text_baseline_mode": "Alphabetic",
          "text_rotation": null,
          "font_family": null,
          "font_weight": "Normal",
          "font_style": "Normal",
          "position_x": [750.0],
          "position_y": [15.0],
          "text_vec": ["tiny h"]
        }
      },
      {
        "layer_type": "TextLayer",
        "layer_params": {
          "layer_id": "t23",
          "bounds": null,
          "data_unit_mode_x": "Pixels",
          "data_unit_mode_y": "Pixels",
          "text_size": 10.0,
          "text_size_unit_mode": "Pixels",
          "text_align_mode": "Middle",
          "text_baseline_mode": "Alphabetic",
          "text_rotation": null,
          "font_family": null,
          "font_weight": "Normal",
          "font_style": "Normal",
          "position_x": [50.0],
          "position_y": [36.0],
          "text_vec": ["sm a"]
        }
      },
      {
        "layer_type": "TextLayer",
        "layer_params": {
          "layer_id": "t24",
          "bounds": null,
          "data_unit_mode_x": "Pixels",
          "data_unit_mode_y": "Pixels",
          "text_size": 10.0,
          "text_size_unit_mode": "Pixels",
          "text_align_mode": "Middle",
          "text_baseline_mode": "Alphabetic",
          "text_rotation": null,
          "font_family": null,
          "font_weight": "Normal",
          "font_style": "Normal",
          "position_x": [150.0],
          "position_y": [36.0],
          "text_vec": ["sm b"]
        }
      },
      {
        "layer_type": "TextLayer",
        "layer_params": {
          "layer_id": "t25",
          "bounds": null,
          "data_unit_mode_x": "Pixels",
          "data_unit_mode_y": "Pixels",
          "text_size": 10.0,
          "text_size_unit_mode": "Pixels",
          "text_align_mode": "Middle",
          "text_baseline_mode": "Alphabetic",
          "text_rotation": null,
          "font_family": null,
          "font_weight": "Normal",
          "font_style": "Normal",
          "position_x": [250.0],
          "position_y": [36.0],
          "text_vec": ["sm c"]
        }
      },
      {
        "layer_type": "TextLayer",
        "layer_params": {
          "layer_id": "t26",
          "bounds": null,
          "data_unit_mode_x": "Pixels",
          "data_unit_mode_y": "Pixels",
          "text_size": 10.0,
          "text_size_unit_mode": "Pixels",
          "text_align_mode": "Middle",
          "text_baseline_mode": "Alphabetic",
          "text_rotation": null,
          "font_family": null,
          "font_weight": "Normal",
          "font_style": "Normal",
          "position_x": [350.0],
          "position_y": [36.0],
          "text_vec": ["sm d"]
        }
      },
      {
        "layer_type": "TextLayer",
        "layer_params": {
          "layer_id": "t27",
          "bounds": null,
          "data_unit_mode_x": "Pixels",
          "data_unit_mode_y": "Pixels",
          "text_size": 10.0,
          "text_size_unit_mode": "Pixels",
          "text_align_mode": "Middle",
          "text_baseline_mode": "Alphabetic",
          "text_rotation": null,
          "font_family": null,
          "font_weight": "Normal",
          "font_style": "Normal",
          "position_x": [450.0],
          "position_y": [36.0],
          "text_vec": ["sm e"]
        }
      },
      {
        "layer_type": "TextLayer",
        "layer_params": {
          "layer_id": "t28",
          "bounds": null,
          "data_unit_mode_x": "Pixels",
          "data_unit_mode_y": "Pixels",
          "text_size": 10.0,
          "text_size_unit_mode": "Pixels",
          "text_align_mode": "Middle",
          "text_baseline_mode": "Alphabetic",
          "text_rotation": null,
          "font_family": null,
          "font_weight": "Normal",
          "font_style": "Normal",
          "position_x": [550.0],
          "position_y": [36.0],
          "text_vec": ["sm f"]
        }
      },
      {
        "layer_type": "TextLayer",
        "layer_params": {
          "layer_id": "t29",
          "bounds": null,
          "data_unit_mode_x": "Pixels",
          "data_unit_mode_y": "Pixels",
          "text_size": 10.0,
          "text_size_unit_mode": "Pixels",
          "text_align_mode": "Middle",
          "text_baseline_mode": "Alphabetic",
          "text_rotation": null,
          "font_family": null,
          "font_weight": "Normal",
          "font_style": "Normal",
          "position_x": [650.0],
          "position_y": [36.0],
          "text_vec": ["sm g"]
        }
      },
      {
        "layer_type": "TextLayer",
        "layer_params": {
          "layer_id": "t30",
          "bounds": null,
          "data_unit_mode_x": "Pixels",
          "data_unit_mode_y": "Pixels",
          "text_size": 10.0,
          "text_size_unit_mode": "Pixels",
          "text_align_mode": "Middle",
          "text_baseline_mode": "Alphabetic",
          "text_rotation": null,
          "font_family": null,
          "font_weight": "Normal",
          "font_style": "Normal",
          "position_x": [750.0],
          "position_y": [36.0],
          "text_vec": ["sm h"]
        }
      },
      {
        "layer_type": "TextLayer",
        "layer_params": {
          "layer_id": "t31",
          "bounds": null,
          "data_unit_mode_x": "Pixels",
          "data_unit_mode_y": "Pixels",
          "text_size": 64.0,
          "text_size_unit_mode": "Pixels",
          "text_align_mode": "Middle",
          "text_baseline_mode": "Alphabetic",
          "text_rotation": null,
          "font_family": null,
          "font_weight": "Normal",
          "font_style": "Normal",
          "position_x": [160.0],
          "position_y": [585.0],
          "text_vec": ["Large A"]
        }
      },
      {
        "layer_type": "TextLayer",
        "layer_params": {
          "layer_id": "t32",
          "bounds": null,
          "data_unit_mode_x": "Pixels",
          "data_unit_mode_y": "Pixels",
          "text_size": 64.0,
          "text_size_unit_mode": "Pixels",
          "text_align_mode": "Middle",
          "text_baseline_mode": "Alphabetic",
          "text_rotation": null,
          "font_family": null,
          "font_weight": "Normal",
          "font_style": "Normal",
          "position_x": [400.0],
          "position_y": [585.0],
          "text_vec": ["Large B"]
        }
      },
      {
        "layer_type": "TextLayer",
        "layer_params": {
          "layer_id": "t33",
          "bounds": null,
          "data_unit_mode_x": "Pixels",
          "data_unit_mode_y": "Pixels",
          "text_size": 64.0,
          "text_size_unit_mode": "Pixels",
          "text_align_mode": "Middle",
          "text_baseline_mode": "Alphabetic",
          "text_rotation": null,
          "font_family": null,
          "font_weight": "Normal",
          "font_style": "Normal",
          "position_x": [640.0],
          "position_y": [585.0],
          "text_vec": ["Large C"]
        }
      },
      {
        "layer_type": "TextLayer",
        "layer_params": {
          "layer_id": "t34",
          "bounds": null,
          "data_unit_mode_x": "Pixels",
          "data_unit_mode_y": "Pixels",
          "text_size": 80.0,
          "text_size_unit_mode": "Pixels",
          "text_align_mode": "Middle",
          "text_baseline_mode": "Alphabetic",
          "text_rotation": null,
          "font_family": null,
          "font_weight": "Normal",
          "font_style": "Normal",
          "position_x": [250.0],
          "position_y": [720.0],
          "text_vec": ["HUGE"]
        }
      },
      {
        "layer_type": "TextLayer",
        "layer_params": {
          "layer_id": "t35",
          "bounds": null,
          "data_unit_mode_x": "Pixels",
          "data_unit_mode_y": "Pixels",
          "text_size": 80.0,
          "text_size_unit_mode": "Pixels",
          "text_align_mode": "Middle",
          "text_baseline_mode": "Alphabetic",
          "text_rotation": null,
          "font_family": null,
          "font_weight": "Normal",
          "font_style": "Normal",
          "position_x": [570.0],
          "position_y": [720.0],
          "text_vec": ["FONT"]
        }
      }
    ]
  }
}
EOF

# Render
echo "Rendering via_svg.png (SVG --> PNG via resvg, font: NimbusSans-Regular)..." >&2
"$PLUOT" \
  --input "$WORK_DIR/params.json" \
  --output "$WORK_DIR/out.via_svg.png" \
  --width 800 --height 800 \
  --font_path "$FONT_PATH" >&2

echo "Rendering out.png (GPU raster)..." >&2
"$PLUOT" \
  --input "$WORK_DIR/params.json" \
  --output "$WORK_DIR/out.png" \
  --width 800 --height 800 >&2

# Pixel diff
echo "Comparing images..." >&2
SCORE="$("$DIFF" "$WORK_DIR/out.via_svg.png" "$WORK_DIR/out.png")"
echo "Pixel diff score (distance_sum; 0 = identical): $SCORE"
