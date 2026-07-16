# Snapshot Tests

Rendering tests use snapshot comparison: pixel-exact for raster (via [kompari](https://github.com/linebender/kompari)) and whitespace-normalized text for SVG.

## Directory Layout

```
tests/
  snaps-blessed/   # Reference snapshots (committed to git)
  snaps-dirty/     # Test-generated outputs (gitignored)
  snapshot_utils.rs  # Helpers functions
  test_rect_layer.rs  # Raster + vector rendering tests
```

## Taking Initial Snapshots

When you add a new test:

1. Call the appropriate helper at the end of the test:
   - Raster: `check_snapshot(&image, "my_test.png")`
   - SVG: `check_svg_snapshot(&svg_string, "my_test.svg")`
2. Run the test. It will **fail** because no reference snapshot exists yet:
   ```
   cargo test -p pluot test_name
   ```
3. The test writes the output to `tests/snaps-dirty/`.
4. Inspect the output to verify it looks correct.
5. Bless it by copying to the `snaps-blessed` directory:
   ```
   cp crates/pluot_core/tests/snaps-dirty/<name> crates/pluot_core/tests/snaps-blessed/<name>
   ```
6. Commit the new snapshot file.

## Updating Snapshots After Intentional Changes

When rendering output changes intentionally (e.g., shader fix, new layer behavior):

1. Run the tests. The affected snapshot test(s) will fail with a mismatch message.
2. The test writes the new output to `tests/snaps-dirty/`.
3. Inspect the current output to verify the change is correct.
4. Bless the updated snapshot:
   ```
   cp crates/pluot/tests/snaps-dirty/<name> crates/pluot/tests/snaps-blessed/<name>
   # Or, if you need to copy multiple files with the same prefix:
   cp crates/pluot/tests/snaps-dirty/<prefix>* crates/pluot/tests/snaps-blessed/
   ```
5. Commit the updated snapshot file.

## How It Works

- **Raster tests** render an image and call `check_snapshot(image, name)`. The comparison uses `kompari::compare_images` for pixel-exact diffing.
- **SVG tests** render an SVG string and call `check_svg_snapshot(svg, name)`. The comparison normalizes whitespace (trims lines, drops blanks) before diffing.
- Both helpers always write the current output to `tests/snaps-dirty/` and panic with `cp` instructions on mismatch or missing snapshot.

## Running Tests

These tests require GPU access and are skipped in environments where the `lacks_gpu` feature is enabled:

```
cargo test -p pluot
```
