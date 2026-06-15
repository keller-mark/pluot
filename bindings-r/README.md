# R bindings

## Naming

- `pluot`: The Rust rendering library (`crates/pluot` in this repo)
- `pluotr`: The R package in `bindings-r`
- `pluotr_rs`: The Rust staticlib crate embedded inside the R package; depends on `pluot` and `extendr-api`, and registers the package entry points with R via extendr's `extendr_module!` macro

## Architecture

`pluotr_rs` uses [extendr](https://extendr.github.io/) (v0.9) instead of hand-written C glue:

### How the call chain works

**Rendering:**
```
R: pluot_render(...)
  --> .Call("wrap__render_r", json_str)          # extendr-registered symbol
  --> Rust: render_r(json_params: &str) -> Raw   # #[extendr] fn in lib.rs
  --> render::do_render(json_str)                # calls pluot::render via futures::block_on
```

**Zarr store callbacks (e.g. when the layer fetches a chunk):**
```
Rust: zarr_get(store_name, key)                # in pluot_core::bindings::r
  --> R!("pluotr:::pluot_zarr_get({{store_name}}, {{key}})")
  --> R: pluot_zarr_get(store_name, key)         # in zarr.R, reads from pizzarr store cache
```


## Development

Usage in RStudio:

```r
devtools::install()
library(pluotr)

# Render a plot — returns a raw vector of RGBA bytes (width × height × 4)
raw_bytes <- pluotr::pluot_render(
  layers = list(
    list(
      layer_type = "PointLayer",
      layer_params = list(
        x = list(1, 2, 3),
        y = list(4, 5, 6)
      )
    )
  ),
  width  = 800L,
  height = 600L
)

# Reconstruct an image with e.g. the 'png' or 'magick' package
# (drop the trailing status byte emitted by pluot)
arr <- array(as.integer(raw_bytes[-length(raw_bytes)]),
             dim = c(4L, 800L, 600L))
```

## Testing

The zarr tests require **pizzarr >= 0.2.0** for `zarr_format = 3L` support. The CRAN source package fails to compile on macOS (same C++ stdlib issue as pluot itself). Install the pure-R build from r-universe instead:

```r
# Download the pure-R tarball from r-universe (no Rust compilation needed)
tmp <- tempfile(fileext = ".tar.gz")
download.file(
  "https://zarr-developers.r-universe.dev/src/contrib/pizzarr_0.2.0.tar.gz",
  tmp
)
install.packages(tmp, repos = NULL, type = "source")
```

The r-universe package has no `src/` directory — it is a pure-R implementation that installs without a Rust toolchain.

```r
# Run all tests
devtools::test(pkg = "/path/to/pluot/bindings-r")

# Run only the zarr integration tests
devtools::test(pkg = "/path/to/pluot/bindings-r", filter = "zarr")

# Run only the render tests
devtools::test(pkg = "/path/to/pluot/bindings-r", filter = "render")

# Run only the FPS benchmark tests
devtools::test(pkg = "/path/to/pluot/bindings-r", filter = "fps")
```

Tests live in `tests/testthat/` and use the [testthat](https://testthat.r-lib.org/) framework:

| File | What it tests |
|---|---|
| `test-render.R` | Byte count, pixel sum, and SVG output for a 4-point PointLayer at 100×100 |
| `test-fps.R` | PointLayer renders complete at positive FPS across a range of point counts and resolutions |
| `test-zarr.R` | Zarr store callbacks (register, has, get, range), and a full ZarrPointLayer render from a pizzarr MemoryStore |

## Importing `pluot` from `pluotr_rs`

`pluotr_rs` is a standalone Cargo project — it declares `[workspace]` in its own `Cargo.toml` so that Cargo does not traverse up into the pluot workspace. This keeps `cargo vendor` scoped to only `pluotr_rs`'s transitive dependencies (not the entire workspace).

The `pluot` crate is made available via a symlink:

```
src/crates -> ../../crates   (symlink)
```

and referenced by path in `pluotr_rs/Cargo.toml`:

```toml
pluot = { path = "../crates/pluot", features = ["rlang", "embed_fonts"] }
```

`R CMD build` follows the symlink and copies the real crate source into the build tarball, so the package builds correctly when installed from a temporary directory (as `devtools::install()` does).

Because `pluot`, `pluot_core`, and `pluot_zarr` use `*.workspace = true` for many of their fields and dependencies, a dedicated [src/Cargo.toml](src/Cargo.toml) workspace root is provided alongside the symlink. When Cargo walks up from `src/crates/pluot/` to resolve workspace-inherited values, it finds this file. `pluotr_rs` still declares its own `[workspace]` and is not a member of the `src/` workspace.

## Package Structure

[src/Makevars](src/Makevars) compiles `pluotr_rs` as a static library and links it into the R shared object alongside the thin `wrapper.c` entrypoint.

```
bindings-r/
├─ configure                ← checks if 'cargo' is installed on PATH
├─ cleanup                  ← stub; re-enable for CRAN (runs vendor-update.sh)
├─ src/
│  ├─ Cargo.toml            ← workspace root for pluot/pluot_core/pluot_zarr;
│  │                           provides [workspace.package] and [workspace.dependencies]
│  │                           so their *.workspace = true fields resolve correctly
│  ├─ crates -> ../../crates  ← symlink; R CMD build follows it to include crate source
│  ├─ pluotr_rs/            ← standalone staticlib crate: extendr-based wrappers over pluot
│  │  ├─ Cargo.toml         ← own [workspace] root; deps: pluot (path), extendr-api
│  │  ├─ src/
│  │  │  ├─ lib.rs          ← #[extendr] fn render_r + extendr_module! { mod pluotr; }
│  │  │  └─ render.rs       ← do_render(): calls pluot::render via block_on
│  │  ├─ vendor-update.sh   ← creates vendor.tar.xz for CRAN
│  │  └─ vendor-authors.R   ← generates inst/AUTHORS from cargo metadata
│  ├─ Makevars              ← builds pluotr_rs, links libpluotr_rs.a
│  ├─ Makevars.win          ← Windows variant (cross-compile targets)
│  └─ wrapper.c             ← 2-line entrypoint: R_init_pluotr --> R_init_pluotr_extendr
├─ R/
│  ├─ render.R              ← pluot_render()
│  └─ zarr.R                ← pluot_register_store(), pluot_zarr_*() callbacks
├─ tests/
│  ├─ testthat.R
│  └─ testthat/
│     ├─ test-render.R
│     ├─ test-fps.R
│     └─ test-zarr.R
├─ DESCRIPTION
└─ NAMESPACE
```

## Vendoring

> **Vendoring is currently disabled** — the [cleanup](cleanup) script is a stub. Re-enable it for CRAN submission by uncommenting the body of that file.

Per the [2023 CRAN guidelines](https://cran.r-project.org/web/packages/using_rust.html), cargo crates should be vendored in the source package to support offline installation. The two-step process when re-enabled:

 1. (by package author) The [vendor-update.sh](src/pluotr_rs/vendor-update.sh) script creates the `vendor.tar.xz` bundle. The [vendor-authors.R](src/pluotr_rs/vendor-authors.R) script generates `inst/AUTHORS`. Both are called from [cleanup](cleanup) and run automatically during `R CMD build`.
 2. (by the user) At install time, [Makevars](src/Makevars) extracts `vendor.tar.xz` (when present) and writes a `.cargo/config.toml` pointing at the vendored sources.

Without a `vendor.tar.xz`, `cargo build` downloads crates from crates.io as normal.

**After adding or updating a dependency** (including the move to extendr-api 0.9), regenerate the vendor archive before any CRAN submission:

```sh
cd src/pluotr_rs
bash vendor-update.sh
```

## Troubleshooting

### `error: no matching package named 'extendr-api' found`

Cargo is trying to fetch from crates.io but is restricted to a stale vendor directory. This happens when a previous build left a `src/.cargo/config.toml` that points at a vendor directory which no longer exists or is out of date.

**Fix:** delete the stale config and the outdated archive, then rebuild:

```sh
rm -f src/.cargo/config.toml src/pluotr_rs/vendor.tar.xz
R CMD INSTALL .
```

If you need offline/CRAN builds, regenerate `vendor.tar.xz` afterwards (see [Vendoring](#vendoring) above).

### Tests panic with `EOF while parsing a value at line 1 column 0`

The zarr layer received empty bytes for a metadata key. This means the R zarr callback returned nothing — likely because the `pluot_zarr_get_status` call failed silently. Check that:

1. The store was registered with `pluot_register_store(name, store)` before calling `pluot_render`.
2. The key paths in `layer_params` match the keys used in `pluot_register_store`.
3. `wait_for_store_gets = TRUE` is set so Rust waits for synchronous R callbacks.

## Installing this package

If Rust is available, clone this repository and run the regular `R CMD INSTALL` command:

```
R CMD INSTALL pluotr
```
