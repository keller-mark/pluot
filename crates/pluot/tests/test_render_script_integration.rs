// Integration tests for the code-based GraphicsFormats: rather than just
// snapshotting the generated *source text* (see test_render_script.rs), these
// tests actually execute the generated code in its own language runtime and
// check that the resulting SVG matches a shared reference snapshot.
//
// Python and R get their own files (test_render_script_integration_python.rs,
// test_render_script_integration_r.rs), each gated behind the corresponding
// `python` / `rlang` crate feature, since they require that language's
// runtime to be installed. Bash and Rust have no such runtime dependency
// beyond what's already required to build/test this crate, so they stay
// here, ungated. JS/HTML are intentionally not covered here: the compiled
// wasm module touches WebGPU even for `Vector` (SVG) output, which has no
// headless equivalent in this test environment (would require a real browser).
//
// Calling `render_to_script` directly (rather than going through `render()`)
// lets `RenderParams.format` carry the *real* desired output (`Vector`, i.e.
// SVG) while the second argument independently selects the code target
// (`ScriptBash`, `ScriptRust`, ...). See `resolved_format` in
// `pluot_core::render_script` for how the generators pick this up.
#![cfg(not(target_arch = "wasm32"))]

use std::path::Path;
use std::process::Command;

mod test_utils;
use test_utils::{
    check_svg_snapshot, fresh_scratch_dir, panic_on_failure, plot_params,
    render_and_check_svg_snapshot, repo_root, CANONICAL_SVG_SNAPSHOT,
};

use pluot::{render_to_script, GraphicsFormat};

// ── Bash / pluot_cli ────────────────────────────────────────────────────────

#[tokio::test]
async fn test_render_script_integration_bash() {
    let script = render_to_script(plot_params(GraphicsFormat::Vector), &GraphicsFormat::ScriptBash);

    // The generated script locates `examples/pluot_cli` via `$(dirname "$0")`,
    // i.e. it assumes it's being run from a copy that lives at the repo root.
    let script_path = repo_root().join(format!(".pluot_integration_test_{}.sh", std::process::id()));
    std::fs::write(&script_path, &script).expect("write bash script");

    // `--output plot.svg` is a relative path, resolved against the working
    // directory the script is *run* from (independent of `$0`'s location).
    let scratch = fresh_scratch_dir("bash");

    let output = Command::new("bash")
        .arg(&script_path)
        .current_dir(&scratch)
        .output();

    // Always clean up the temp script at the repo root, even on failure.
    let _ = std::fs::remove_file(&script_path);

    let output = output.expect("failed to spawn bash");
    panic_on_failure("bash", &output);

    let svg = std::fs::read_to_string(scratch.join("plot.svg")).expect("read CLI SVG output");
    check_svg_snapshot(&svg, CANONICAL_SVG_SNAPSHOT);

    let _ = std::fs::remove_dir_all(&scratch);
}

// ── Rust ──────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_render_script_integration_rust() {
    let snippet = render_to_script(plot_params(GraphicsFormat::Vector), &GraphicsFormat::ScriptRust);

    let scratch = fresh_scratch_dir("rust");
    std::fs::create_dir_all(scratch.join("src")).expect("create src dir");

    let pluot_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .expect("pluot crate should exist");

    let cargo_toml = format!(
        "[workspace]\n\
         \n\
         [package]\n\
         name = \"pluot_render_script_check\"\n\
         version = \"0.0.0\"\n\
         edition = \"2021\"\n\
         publish = false\n\
         \n\
         [[bin]]\n\
         name = \"pluot_render_script_check\"\n\
         path = \"src/main.rs\"\n\
         \n\
         [dependencies]\n\
         pluot = {{ path = {pluot_path:?} }}\n\
         # Pinned to the exact versions in the workspace's `Cargo.lock`: a looser\n\
         # requirement can resolve a newer `serde_json` pulling in a different\n\
         # `serde_core`, which breaks the `Deserialize` trait bound on `pluot`'s\n\
         # `RenderParams` (a duplicate-crate-version error, not a real bug).\n\
         serde_core = \"=1.0.228\"\n\
         serde_json = \"=1.0.143\"\n\
         tokio = {{ version = \"=1.49.0\", features = [\"full\"] }}\n",
    );
    std::fs::write(scratch.join("Cargo.toml"), cargo_toml).expect("write Cargo.toml");

    // The generated snippet declares `let pixels = render(params).await;`
    // (and nothing else); append a small driver that writes it out, without
    // altering the snippet itself.
    let main_rs = format!(
        "#[tokio::main]\n\
         async fn main() {{\n\
         {snippet}\n\
         \x20   let out_path = std::env::var(\"PLUOT_TEST_OUTPUT\").expect(\"PLUOT_TEST_OUTPUT env var\");\n\
         \x20   std::fs::write(&out_path, &pixels).expect(\"failed to write output\");\n\
         }}\n",
    );
    std::fs::write(scratch.join("src").join("main.rs"), main_rs).expect("write main.rs");

    let output_path = scratch.join("out.svg");
    // Share the workspace's target dir so already-built dependencies
    // (`pluot_core` itself included) are reused instead of recompiled.
    let shared_target_dir = repo_root().join("target");

    let output = Command::new("cargo")
        .arg("run")
        .arg("--quiet")
        .current_dir(&scratch)
        .env("CARGO_TARGET_DIR", &shared_target_dir)
        .env("PLUOT_TEST_OUTPUT", &output_path)
        .output()
        .expect("failed to spawn cargo");
    panic_on_failure("cargo run", &output);

    let svg = std::fs::read_to_string(&output_path).expect("read rust SVG output");
    check_svg_snapshot(&svg, CANONICAL_SVG_SNAPSHOT);

    let _ = std::fs::remove_dir_all(&scratch);
}

// ── Canonical reference ───────────────────────────────────────────────────────

/// Renders the same plot directly (not via code generation) with
/// `GraphicsFormat::Vector`, producing the shared SVG snapshot that the
/// executed Python/R/Bash/Rust outputs above are checked against.
#[tokio::test]
async fn test_render_script_integration_canonical_svg() {
    render_and_check_svg_snapshot(plot_params(GraphicsFormat::Vector), CANONICAL_SVG_SNAPSHOT).await;
}
