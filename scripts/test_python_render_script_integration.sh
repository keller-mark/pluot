#!/usr/bin/env bash
# Generates the render-to-script integration fixtures (see
# gen_render_script_fixtures.sh) and then runs the Python test suite, which
# includes bindings-python/tests/test_render_script_integration.py: it execs
# the generated render_script.py and checks the resulting SVG against the
# canonical reference. Any extra arguments are forwarded to `pytest`.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

export PLUOT_RENDER_SCRIPT_FIXTURES_DIR
PLUOT_RENDER_SCRIPT_FIXTURES_DIR="$("$REPO_ROOT/scripts/gen_render_script_fixtures.sh")"

cd "$REPO_ROOT"
uv run pytest "$@"
