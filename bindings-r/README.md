# R bindings

## Naming

- `pluot`: The Rust rendering library (`crates/pluot` in this repo)
- `pluotr`: The R package
- `pluotr_rs`: The Rust staticlib crate embedded inside the R package; depends on `pluot` and `extendr-api`, and registers the package entry points with R via extendr's `extendr_module!` macro

## Architecture

`pluotr_rs` uses [extendr](https://extendr.github.io/) (v0.9) instead of hand-written C glue:

| Concern | Before | After |
|---|---|---|
| Exposing `render_r` to R | `render_wrapper` in `wrapper.c` (manual SEXP alloc, `R_CallMethodDef`, `R_init_pluotr`) | `#[extendr] fn render_r(...)` + `extendr_module!` in Rust |
| Calling R zarr callbacks from Rust | `RZarrCallbacks` function-pointer struct + `OnceLock` + C intermediates | `R!("pluotr:::fn({{arg}})")` via extendr's `R!` macro |
| C source in `src/` | `wrapper.c` (231 lines) + `api.h` | `wrapper.c` (2 lines: forwards `R_init_pluotr` → `R_init_pluotr_extendr`) |

### How the call chain works

**Rendering:**
```
R: pluot_render(...)
  → .Call("wrap__render_r", json_str)          # extendr-registered symbol
  → Rust: render_r(json_params: &str) -> Raw   # #[extendr] fn in lib.rs
  → render::do_render(json_str)                # calls pluot::render via futures::block_on
```

**Zarr store callbacks (e.g. when the layer fetches a chunk):**
```
Rust: zarr_get(store_name, key)                # in pluot_core::bindings::r
  → R!("pluotr:::pluot_zarr_get({{store_name}}, {{key}})")
  → R: pluot_zarr_get(store_name, key)         # in zarr.R — reads from pizzarr store cache
```

The `R!` macro is evaluated synchronously (R is single-threaded); extendr handles the lock internally.

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
pluot = { path = "../crates/pluot", features = ["r"] }
```

`R CMD build` follows the symlink and copies the real crate source into the build tarball, so the package builds correctly when installed from a temporary directory (as `devtools::install()` does).

Because `pluot`, `pluot_core`, and `pluot_zarr` use `*.workspace = true` for many of their fields and dependencies, a dedicated [src/Cargo.toml](src/Cargo.toml) workspace root is provided alongside the symlink. When Cargo walks up from `src/crates/pluot/` to resolve workspace-inherited values, it finds this file. `pluotr_rs` still declares its own `[workspace]` and is not a member of the `src/` workspace.

## macOS build note

R's build system exports `MACOSX_DEPLOYMENT_TARGET` based on its SDK (e.g. `26.1`), which cc-rs passes as `-mmacosx-version-min=26.1` when compiling C++ dependencies (snappy/blosc via zarrs). This breaks the C++ stdlib header search on current Xcode toolchains.

The [Makevars](src/Makevars) works around both issues:

```makefile
LLVM_PREFIX = $(shell test -d /opt/homebrew/opt/llvm/bin && echo /opt/homebrew/opt/llvm/bin:)

env -u MACOSX_DEPLOYMENT_TARGET -u CXXFLAGS \
    PATH="$(LLVM_PREFIX)${PATH}:${HOME}/.cargo/bin" \
    cargo build ...
```

`MACOSX_DEPLOYMENT_TARGET` is unset so cc-rs does not pass a bad `-mmacosx-version-min` flag. If the Homebrew LLVM toolchain is present at `/opt/homebrew/opt/llvm/bin`, it is prepended to `PATH` so that cc-rs picks up `clang`/`clang++` from LLVM rather than Apple's toolchain (Apple clang fails to locate C++ stdlib headers at macOS SDK 26.1). LLVM can be installed via `brew install llvm`.

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
│  └─ wrapper.c             ← 2-line entrypoint: R_init_pluotr → R_init_pluotr_extendr
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

### `'pluot_zarr_get' is not an exported object from 'namespace:pluotr'`

The Rust zarr callbacks call internal R functions using `pluotr:::fn_name` (triple-colon). If you see this error it means the call is using `::` (double-colon) which only resolves exported symbols. Check `crates/pluot_core/src/bindings.rs` and ensure all `R!(...)` calls use `pluotr:::`.

### `'string' file not found` during C++ compilation (snappy/blosc)

The cc-rs crate used to build C++ compression libraries picked up Apple clang (`c++`) instead of LLVM's `clang++`. Install LLVM via Homebrew and ensure it is found:

```sh
brew install llvm
```

Then rebuild. The Makevars automatically detects LLVM at `/opt/homebrew/opt/llvm/bin` and sets `CXX` accordingly.

### `Undefined symbols for architecture arm64: _pluot_zarr_*`

This would occur if the `bindings.rs` `r` module were reverted to the old `extern "C"` declaration style. macOS's staticlib linker checks for unresolved symbols at archive-link time. The current design avoids this by calling back into R exclusively through extendr's `R!` macro — there are no external C symbol references from the staticlib.

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

Alternatively, to download and install from within R itself:

```r
# install.packages("remotes")
remotes::install_github("r-rust/pluotr")
```

## What is Cargo

The standard rust toolchain includes a great package manager `cargo` with a corresponding registry [crates.io](https://crates.io/). Cargo makes it very easy to build a rust package including all dependencies into a static library that can easily be linked into an R package.

This is perfect for R because we can compile and link all rust code at build-time without any system dependencies. Rust itself has no substantial runtime so the resulting R package is entirely self contained. Indeed, rust has been designed specifically to serve well as an embedded language.

## Installing Rust on Linux / MacOS

Note that `cargo` is only needed at __build-time__. Rust has __no runtime dependencies__. The easiest way to install the latest version of Rust (including cargo) is from: https://www.rust-lang.org/tools/install

Alternatively, you may install cargo from your OS package manager:

 - Debian/Ubuntu: `sudo apt-get install cargo`
 - Fedora/CentOS*: `sudo yum install cargo`
 - MacOS: `brew install rustc`

*Note that on CentOS you first need to enable EPEL via `sudo yum install epel-release`.

## Installing Rust for R on Windows

In order for rust to work with R you need to install the toolchain using `rustup` and then add the `x86_64-pc-windows-gnu` target. First download [rustup-init.exe](https://win.rustup.rs/) and then install the default toolchain:

```
rustup-init.exe -y --default-host x86_64-pc-windows-gnu
```

Or if rust is already installed (for example on GitHub actions), you can simply add the target:

```
rustup target add x86_64-pc-windows-gnu
```

To compile 32bit packages also add the `i686-pc-windows-gnu` target, but 32-bit is no longer supported as of R 4.2.

## GitHub Actions

__Update 2023:__ This step is no longer needed because GitHub action runners now have the required Rust targets preinstalled by default.

To use GitHub actions, you can use the [standard r workflow](https://github.com/r-lib/actions/blob/HEAD/.github/workflows/check-standard.yaml) script in combination with this extra step:

```
- name: Add Rtools targets to Rust
  if: runner.os == 'Windows'
  run: |
    rustup target add i686-pc-windows-gnu
    rustup target add x86_64-pc-windows-gnu
```

## In the real world

The [gifski](https://cran.r-project.org/web/packages/gifski/index.html) package has been on CRAN since 2018, and uses this same structure.

## More Resources
 - [r-rust FAQ](https://github.com/r-rust/faq)
 - Erum2018 [slides](https://jeroen.github.io/erum2018/) about this project presented by Jeroen
 - [Rust Inside Other Languages](https://doc.rust-lang.org/1.6.0/book/rust-inside-other-languages.html) chapter from official rust documentation
 - [extendr](https://github.com/extendr): the R extension interface used by this package
 - Duncan's proof of concept: [RCallRust](https://github.com/duncantl/RCallRust)
