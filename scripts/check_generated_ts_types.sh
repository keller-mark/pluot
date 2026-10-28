#!/usr/bin/env bash
# Regenerates the ts-rs TypeScript types in bindings-js/core/src/generated
# and fails if they differ from what is committed.
# Used by the plain_rust_unit_tests CI job.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

GENERATED_DIR=bindings-js/core/src/generated

cargo test -p pluot --lib --features lacks_gpu export_layer_params_typescript

# The generator deletes the directory before exporting, so removed types show up as deletions.
if [ -n "$(git status --porcelain -- "$GENERATED_DIR")" ]; then
  git status --porcelain -- "$GENERATED_DIR"
  git --no-pager diff -- "$GENERATED_DIR"
  echo "Error: $GENERATED_DIR is stale. Run 'pnpm run generate-types' and commit the result." >&2
  exit 1
fi
echo "$GENERATED_DIR is up to date."
