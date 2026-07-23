#!/usr/bin/env bash
# Generates the render-to-script integration fixtures (see
# gen_render_script_fixtures.sh) and then runs the same command-line R check
# documented in the root README.md ("Build for R" > "entirely via the
# command-line") and used by the `rlang_compile` CI job. That check runs
# bindings-r/tests/testthat/test-render-script-integration.R, which sources
# the generated render_script.R and checks the resulting SVG against the
# canonical reference.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

export PLUOT_RENDER_SCRIPT_FIXTURES_DIR
PLUOT_RENDER_SCRIPT_FIXTURES_DIR="$("$REPO_ROOT/scripts/gen_render_script_fixtures.sh")"

PKG_VERSION="$(grep '^Version:' "$REPO_ROOT/bindings-r/DESCRIPTION" | sed 's/Version: *//')"

SCRATCH_DIR="$(mktemp -d)"
trap 'rm -rf "$SCRATCH_DIR"' EXIT

cd "$SCRATCH_DIR"
R CMD build "$REPO_ROOT/bindings-r" --no-build-vignettes
R CMD check "pluotr_${PKG_VERSION}.tar.gz" \
  --no-vignettes --no-build-vignettes --ignore-vignettes --no-manual
