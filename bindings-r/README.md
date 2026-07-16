# R bindings

## Naming

- `pluot`: The Rust rendering library (`crates/pluot` in this repo)
- `pluotr`: The R package in `bindings-r`
- `pluotr_rs`: The Rust staticlib crate embedded inside the R package; depends on `pluot` and `extendr-api`, and registers the package entry points with R via extendr's `extendr_module!` macro

## Development

Usage in RStudio:

```r
devtools::install()
library(pluotr)
```

## Testing

The zarr tests require **pizzarr >= 0.2.0** for `zarr_format = 3L` support. Install the pure-R build from r-universe if you have issues with the CRAN build:

```r
# Download the pure-R tarball from r-universe (no Rust compilation needed)
tmp <- tempfile(fileext = ".tar.gz")
download.file(
  "https://zarr-developers.r-universe.dev/src/contrib/pizzarr_0.2.0.tar.gz",
  tmp
)
install.packages(tmp, repos = NULL, type = "source")
```

The r-universe package has no `src/` directory; it is a pure-R implementation that installs without a Rust toolchain.

## Importing `pluot` from `pluotr_rs`

`pluotr_rs` is a standalone Cargo project. It declares `[workspace]` in its own `Cargo.toml` so that Cargo does not traverse up into the pluot workspace. This keeps `cargo vendor` scoped to only `pluotr_rs`'s transitive dependencies (not the entire workspace).

The `pluot` crate is made available via a symlink, referenced by path in `pluotr_rs/Cargo.toml`.
Because `pluot`, `pluot_core`, and `pluot_zarr` use `*.workspace = true` for many of their fields and dependencies, the Cargo.toml workspace root is symlinked from the repo root and is provided alongside the symlinked crates directory. When Cargo walks up from `src/crates/pluot/` to resolve workspace-inherited values, it finds this file. `pluotr_rs` still declares its own `[workspace]` and is not a member of the `src/` workspace.

## Package Structure

[src/Makevars](src/Makevars) compiles `pluotr_rs` as a static library and links it into the R shared object alongside the thin `wrapper.c` entrypoint.

```
bindings-r/
├─ configure                # checks if 'cargo' is installed on PATH
├─ cleanup                  # stub; re-enable for CRAN (runs vendor-update.sh)
├─ src/
│  ├─ Cargo.toml -> ../../Cargo.toml  # symlink; tricks into seeing this as workspace root
│  ├─ crates -> ../../crates          # symlink; R CMD build follows it to include crate source
│  ├─ pluotr_rs/            # standalone staticlib crate: extendr-based wrappers around pluot
│  │  ├─ Cargo.toml         # own [workspace] root; deps: pluot (path to symlink), extendr-api
│  │  ├─ src/
│  │  │  ├─ lib.rs          # #[extendr] fn render_r + extendr_module! { mod pluotr; }
│  │  │  └─ render.rs       # do_render(): calls pluot::render via block_on
│  │  ├─ vendor-update.sh   # creates vendor.tar.xz for CRAN
│  │  └─ vendor-authors.R   # generates inst/AUTHORS from cargo metadata
│  ├─ Makevars              # builds pluotr_rs, links libpluotr_rs.a
│  ├─ Makevars.win          # Windows variant (cross-compile targets)
│  └─ wrapper.c             # 2-line entrypoint: R_init_pluotr --> R_init_pluotr_extendr
├─ R/
│  ├─ render.R              # pluot_render()
│  └─ zarr.R                # pluot_register_store(), pluot_zarr_*() callbacks
├─ tests/
│  ├─ testthat.R
│  └─ testthat/
│     ├─ test-render.R
│     └─ test-zarr.R
├─ DESCRIPTION
└─ NAMESPACE
```

## Vendoring

> **Vendoring is currently disabled.** The [cleanup](cleanup) script is a stub. Re-enable it for CRAN submission by uncommenting the body of that file.

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
