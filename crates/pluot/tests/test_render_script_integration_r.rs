// Integration test for the R `GraphicsFormat::ScriptR` target: rather than
// just snapshotting the generated *source text* (see test_render_script.rs),
// this actually executes the generated R code in its own language runtime
// and checks that the resulting SVG matches the shared reference snapshot
// produced by `test_render_script_integration_canonical_svg`
// (test_render_script_integration.rs).
//
// Only compiled/run when the `rlang` feature is enabled, since it shells out
// to `Rscript` and doesn't need the `extendr` bindings themselves to be
// exercised here.
#![cfg(all(not(feature = "lacks_rlang_env"), not(target_arch = "wasm32")))]

use std::process::Command;

mod test_utils;
use test_utils::{
    check_svg_snapshot, fresh_scratch_dir, panic_on_failure, plot_params, CANONICAL_SVG_SNAPSHOT,
};

use pluot::{render_to_script, GraphicsFormat};

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
