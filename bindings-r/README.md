# R bindings

## Naming

- `pluot`: The Rust rendering library (`crates/pluot` in this repo)
- `pluotr`: The R package
- `pluotr_rs`: The Rust staticlib crate embedded inside the R package; depends on `pluot` and contains the C FFI entry points used by R

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

Reference: https://github.com/r-rust/pluotr

## Testing

```r
# Run all tests
devtools::test(pkg = "/path/to/pluot/bindings-r")

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

## Importing `pluot` from `pluotr_rs`

`pluotr_rs` is a standalone Cargo project — it declares `[workspace]` in its own `Cargo.toml` so that Cargo does not traverse up into the pluot workspace. This keeps `cargo vendor` scoped to only `pluotr_rs`'s transitive dependencies (not the entire workspace).

The `pluot` crate is made available via a symlink:

```
src/crates -> ../../crates   (symlink)
```

and referenced by path in `pluotr_rs/Cargo.toml`:

```toml
pluot = { path = "../crates/pluot" }
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

[src/Makevars](src/Makevars) compiles `pluotr_rs` as a static library and links it into the R shared object.

```
bindings-r/
├─ configure                ← checks if 'cargo' is installed on PATH
├─ cleanup                  ← stub; re-enable for CRAN (runs vendor-update.sh)
├─ src/
│  ├─ Cargo.toml            ← workspace root for pluot/pluot_core/pluot_zarr;
│  │                           provides [workspace.package] and [workspace.dependencies]
│  │                           so their *.workspace = true fields resolve correctly
│  ├─ crates -> ../../crates  ← symlink; R CMD build follows it to include crate source
│  ├─ pluotr_rs/            ← standalone staticlib crate: C FFI wrappers over pluot
│  │  ├─ Cargo.toml         ← own [workspace] root; pluot dep: path = "../crates/pluot"
│  │  ├─ src/
│  │  │  ├─ lib.rs
│  │  │  └─ render.rs       ← rust_render / free_bytes_from_rust
│  │  ├─ api.h              ← C declarations for all exported Rust symbols
│  │  ├─ vendor-update.sh   ← creates vendor.tar.xz for CRAN
│  │  └─ vendor-authors.R   ← generates inst/AUTHORS from cargo metadata
│  ├─ Makevars              ← builds pluotr_rs, links libpluotr_rs.a
│  ├─ Makevars.win          ← Windows variant (cross-compile targets)
│  └─ wrapper.c             ← C glue: R ↔ Rust (roundtrip_wrapper, render_wrapper)
├─ R/
│  └─ render.R              ← pluot_render()
├─ tests/
│  ├─ testthat.R
│  └─ testthat/
│     ├─ test-render.R
│     └─ test-fps.R
├─ DESCRIPTION
└─ NAMESPACE
```

## Vendoring

> **Vendoring is currently disabled** — the [cleanup](cleanup) script is a stub. Re-enable it for CRAN submission by uncommenting the body of that file.

Per the [2023 CRAN guidelines](https://cran.r-project.org/web/packages/using_rust.html), cargo crates should be vendored in the source package to support offline installation. The two-step process when re-enabled:

 1. (by package author) The [vendor-update.sh](src/pluotr_rs/vendor-update.sh) script creates the `vendor.tar.xz` bundle. The [vendor-authors.R](src/pluotr_rs/vendor-authors.R) script generates `inst/AUTHORS`. Both are called from [cleanup](cleanup) and run automatically during `R CMD build`.
 2. (by the user) At install time, [Makevars](src/Makevars) extracts `vendor.tar.xz` (when present) and writes a `.cargo/config.toml` pointing at the vendored sources.

Without a `vendor.tar.xz`, `cargo build` downloads crates from crates.io as normal.

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
 - [extendr](https://github.com/extendr): a more advanced R extension interface using Rust
 - Duncan's proof of concept: [RCallRust](https://github.com/duncantl/RCallRust)
