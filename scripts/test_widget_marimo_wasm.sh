#!/usr/bin/env bash
# Runs bindings-python/tests/test_widget_marimo_wasm.py, which renders
# PluotWasmWidget in a marimo-wasm notebook via Playwright.
# Requires pnpm dependencies to be installed (`pnpm install`).
# Runs without the root project, so the Rust extension is not compiled.
# Any extra arguments are forwarded to `pytest`.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

# The wheel build skips the JS build when static/widget.js exists, so rebuild it to avoid testing a stale bundle.
pnpm -C bindings-python/pluot_widget run build

UV_RUN=(uv run --no-project --python 3.12
  --with pytest
  --with playwright
  --with "marimo>=0.25.1"
  --with "zarr>=3"
  --with numpy
  --with pillow
)
"${UV_RUN[@]}" python -m playwright install chromium
"${UV_RUN[@]}" pytest bindings-python/tests/test_widget_marimo_wasm.py "$@"
