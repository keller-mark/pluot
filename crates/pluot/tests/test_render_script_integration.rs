// Integration tests for the code-based GraphicsFormats: rather than just
// snapshotting the generated *source text* (see test_render_script.rs), these
// tests actually execute the generated Python/R/Rust/Bash code in its own
// language runtime and check that the resulting SVG matches a shared
// reference snapshot. JS/HTML are intentionally not covered here: the compiled
// wasm module touches WebGPU even for `Vector` (SVG) output, which has no
// headless equivalent in this test environment (would require a real browser).
//
// Calling `render_to_script` directly (rather than going through `render()`)
// lets `RenderParams.format` carry the *real* desired output (`Vector`, i.e.
// SVG) while the second argument independently selects the code target
// (`ScriptPython`, `ScriptR`, ...). See `resolved_format` in
// `pluot_core::render_script` for how the generators pick this up.
#![cfg(not(target_arch = "wasm32"))]

// TODO: split these tests into separate files and use feature-gating to only run python in python env, and only run R in R env,
// and then run them in CI / GH actions.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

mod test_utils;
use test_utils::{check_svg_snapshot, render_and_check_svg_snapshot};

use pluot::{
    render_to_script, AxisLinearLayerParams, AxisPosition, CategoricalColormap,
    CategoricalParams, ColorMode, GraphicsFormat, LayerParams, NumericData, PointLayerParams,
    PointShapeMode, RenderParams, SizeMode, UnitsMode,
};

/// The shared SVG snapshot every language's executed output is compared
/// against. Produced directly (not via code generation) by
/// `test_render_script_integration_canonical_svg`.
const CANONICAL_SVG_SNAPSHOT: &str = "test_render_script_integration.svg";

/// The same point + axis plot used by `test_render_script.rs`'s
/// `sample_params`, minus the (here, unused) Zarr store, with `format` left
/// for the caller to set.
fn plot_params(format: GraphicsFormat) -> RenderParams {
    RenderParams {
        width: 640,
        height: 480,
        format,
        plot_id: "plot_1".to_string(),
        camera_view: Some([
            0.15, 0.0, 0.0, 0.0, 0.0, 0.15, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ]),
        margin_left: Some(60.0),
        layers: vec![
            LayerParams::PointLayer(PointLayerParams {
                layer_id: "pts".to_string(),
                data_unit_mode_x: UnitsMode::Data,
                data_unit_mode_y: UnitsMode::Data,
                point_shape_mode: PointShapeMode::Circle,
                point_radius: Some(SizeMode::UniformSize(5.0)),
                position_x: NumericData::Float32(Arc::new(vec![0.0, 1.0, 1.0, 0.0])),
                position_y: NumericData::Float32(Arc::new(vec![0.0, 0.0, 1.0, 1.0])),
                fill_color: Some(ColorMode::Categorical(CategoricalParams {
                    codes: NumericData::Uint8(Arc::new(vec![0, 1, 2, 3])),
                    colormap: CategoricalColormap::Tableau10,
                })),
                ..Default::default()
            }),
            LayerParams::AxisLinearLayer(AxisLinearLayerParams {
                layer_id: "left_axis".to_string(),
                position: AxisPosition::Left,
            }),
        ],
        ..Default::default()
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("repo root should exist")
}

/// A fresh, empty scratch directory under the system temp dir, unique to this
/// test's name and the current process (so parallel test runs don't collide).
fn fresh_scratch_dir(test_name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "pluot_integration_{test_name}_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

fn panic_on_failure(label: &str, output: &std::process::Output) {
    if !output.status.success() {
        panic!(
            "{label} exited with {status}\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}",
            status = output.status,
            stdout = String::from_utf8_lossy(&output.stdout),
            stderr = String::from_utf8_lossy(&output.stderr),
        );
    }
}

// ── Python ──────────────────────────────────────────────────────────────────

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

// ── R ─────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_render_script_integration_r() {
    let script = render_to_script(plot_params(GraphicsFormat::Vector), &GraphicsFormat::ScriptR);

    let scratch = fresh_scratch_dir("r");
    let output_path = scratch.join("out.svg");
    let script_path = scratch.join("render.R");
    std::fs::write(&script_path, &script).expect("write R script");

    // `render.R` assigns its result to `img` (an SVG string) but doesn't write
    // it anywhere; `source()` it into the driver's global environment (the
    // default for `local`) and write `img` out ourselves.
    let driver = format!(
        "source({script_path:?})\n\
         writeLines(img, {output_path:?})\n",
    );

    let output = Command::new("Rscript")
        .arg("-e")
        .arg(&driver)
        .current_dir(&scratch)
        .output()
        .expect("failed to spawn Rscript");
    panic_on_failure("Rscript", &output);

    let svg = std::fs::read_to_string(&output_path).expect("read R SVG output");
    check_svg_snapshot(&svg, CANONICAL_SVG_SNAPSHOT);

    let _ = std::fs::remove_dir_all(&scratch);
}

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
