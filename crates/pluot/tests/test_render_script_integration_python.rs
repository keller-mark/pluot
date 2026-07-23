// Integration test for the Python `GraphicsFormat::ScriptPython` target:
// rather than just snapshotting the generated *source text* (see
// test_render_script.rs), this actually executes the generated Python code
// in its own language runtime and checks that the resulting SVG matches the
// shared reference snapshot produced by
// `test_render_script_integration_canonical_svg` (test_render_script_integration.rs).
//
// Only compiled/run when the `python` feature is enabled, since it shells
// out to the project virtualenv's `python3` and doesn't need the `pyo3`
// bindings themselves to be exercised here.
#![cfg(all(not(feature = "lacks_python_env"), not(target_arch = "wasm32")))]

use std::process::Command;

mod test_utils;
use test_utils::{
    check_svg_snapshot, fresh_scratch_dir, panic_on_failure, plot_params, repo_root,
    CANONICAL_SVG_SNAPSHOT,
};

use pluot::{render_to_script, GraphicsFormat};

#[tokio::test]
async fn test_render_script_integration_python() {
    let script = render_to_script(plot_params(GraphicsFormat::Vector), &GraphicsFormat::ScriptPython);

    let scratch = fresh_scratch_dir("python");
    let output_path = scratch.join("out.svg");

    // The generated script uses top-level `await`, which is only valid inside
    // an async context (e.g. a notebook cell, which is its intended target;
    // see the module docs in `render_script.rs`). Wrap it in an `async def` so
    // it can run under plain CPython, and write out the resulting `img`
    // variable (an SVG string) so this harness can read it back.
    let indented: String = script
        .lines()
        .map(|line| format!("    {line}\n"))
        .collect();
    let driver = format!(
        "import asyncio, os\n\
         \n\
         async def _pluot_integration_main():\n\
         {indented}\
         \n\
         \x20   with open(os.environ[\"PLUOT_TEST_OUTPUT\"], \"w\", encoding=\"utf-8\") as f:\n\
         \x20       f.write(img)\n\
         \n\
         asyncio.run(_pluot_integration_main())\n",
    );
    let driver_path = scratch.join("render.py");
    std::fs::write(&driver_path, &driver).expect("write python driver");

    let python_bin = repo_root().join(".venv").join("bin").join("python3");
    assert!(
        python_bin.exists(),
        "expected a project virtualenv at {python_bin:?}; run `uv sync` (or equivalent) first",
    );

    let output = Command::new(&python_bin)
        .arg(&driver_path)
        .env("PLUOT_TEST_OUTPUT", &output_path)
        .current_dir(&scratch)
        .output()
        .expect("failed to spawn python3");
    panic_on_failure("python3", &output);

    let svg = std::fs::read_to_string(&output_path).expect("read python SVG output");
    check_svg_snapshot(&svg, CANONICAL_SVG_SNAPSHOT);

    let _ = std::fs::remove_dir_all(&scratch);
}
