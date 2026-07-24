# scripts/

Bash helpers that use the `pluot_cli` example binary as a code-generation
step ahead of running each language binding's own test suite.

## Render-to-script integration tests

`crates/pluot_core/src/render_script.rs` can turn a plot definition into
equivalent Python/R source code (`GraphicsFormat::ScriptPython`/`ScriptR`).
Rather than re-implementing execution of that generated code in Rust (which
can't exercise it the way real users - or CI - actually do), these scripts
generate the code via `pluot_cli` and hand off to each language's own test
runner:

- `gen_render_script_fixtures.sh [out_dir]` — builds `pluot_cli` and runs it
  against `fixtures/render_script_integration_layers.json` to produce
  `render_script.py`, `render_script.R`, and `canonical.svg` (a copy of the
  already-blessed
  `crates/pluot/tests/snaps-blessed/test_render_script_integration.svg`
  snapshot) in `out_dir` (default: `target/render_script_fixtures`). Prints
  the resolved `out_dir` on success. Pure code generation, no test execution.

- `test_python_render_script_integration.sh [pytest args...]` — runs the
  generator, points `bindings-python/tests/test_render_script_integration.py`
  at the result via the `PLUOT_RENDER_SCRIPT_FIXTURES_DIR` environment
  variable, then runs `uv run pytest` (the whole suite, same as CI).

- `test_r_render_script_integration.sh` — runs the generator, then performs
  the same command-line `R CMD build` + `R CMD check` documented in the root
  README's "Build for R" section (and used by the `rlang_compile` CI job),
  which runs `bindings-r/tests/testthat/test-render-script-integration.R`.

Both new test files skip themselves with a pointer back to these scripts if
`PLUOT_RENDER_SCRIPT_FIXTURES_DIR` isn't set, so running `uv run pytest` or
`R CMD check` directly (without the generation step) still works for
everything else in each suite.

The environment variable (rather than a fixed path relative to the repo) is
used because `R CMD check` runs tests against an installed copy of the
package in an isolated temp directory, not the source tree - a relative path
wouldn't resolve there, but an inherited environment variable does.
