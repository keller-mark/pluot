# pluot CLI example

From the root of the repository, run:

```sh
# From workspace root
cargo build -p pluot_cli
cargo run -p pluot_cli -- --help

# From this directory
cargo build
cargo run -- --help

# The `pluot` binary will be located at `target/debug/pluot` (or `target/release/pluot` if you build with `--release`).
```

## Example usage:
```sh
# From a JSON file --> SVG
cargo run -- \
    --input examples/layers.in.json \
    --output examples/layers.out.svg \
    --width 500 \
    --height 500
    
# From a JSON file --> PNG
cargo run -- \
    --input examples/layers.in.json \
    --output examples/layers.out.png \
    --width 400 \
    --height 400

# From stdin
cat examples/layers.in.json | cargo run -- -o examples/layers.out.svg --width 600 --height 600
```

## SVG vs GPU comparison scripts

These scripts render the same scene via two backends. SVG-->PNG (resvg) and GPU raster (wgpu), then report a `kompari` pixel-diff score. A score of 0 means identical pixels. They are useful for iterating on layer shaders to bring GPU output in line with the reference SVG render.

### `compare_circles_via_svg.sh` for PointLayer (circles)

Renders 15 `PointLayer` instances (circles) at varying positions and radii (10–64 px) across an 800x600 canvas, including a cluster of overlapping circles near the centre.

```sh
./compare_circles_via_svg.sh <output-dir>
# e.g.
./compare_circles_via_svg.sh circle_diffs
```

Outputs `<output-dir>/out.via_svg.png`, `<output-dir>/out.png`, and `<output-dir>/params.json`.

### `compare_text_via_svg.sh` for TextLayer

Renders 15 `TextLayer` instances across an 800x600 canvas covering font sizes 12–48 px, all three alignment modes (`Start`/`Middle`/`End`), and all four baseline modes (`Top`/`Middle`/`Bottom`/`Alphabetic`). Both backends use `vendor/urw-core35-fonts/NimbusSans-Regular.ttf` (registered via `--font_path` for the SVG path; bundled as `FONT_BYTES` for the GPU path) so the diff measures shader differences rather than font differences.

```sh
./compare_text_via_svg.sh <output-dir>
# e.g.
./compare_text_via_svg.sh text_diffs
```

Outputs `<output-dir>/out.via_svg.png`, `<output-dir>/out.png`, and `<output-dir>/params.json`.
