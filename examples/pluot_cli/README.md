# pluot_cli example

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
# From a JSON file → SVG
cargo run -- \
    --input examples/layers.in.json \
    --output examples/layers.out.svg \
    --width 500 \
    --height 500
    
# From a JSON file → PNG
cargo run -- \
    --input examples/layers.in.json \
    --output examples/layers.out.png \
    --width 400 \
    --height 400

# From stdin
cat examples/layers.in.json | cargo run -- -o examples/layers.out.svg --width 600 --height 600
```
