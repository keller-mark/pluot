use std::path::{Path, PathBuf};

use kompari::color::Rgba8;
use kompari::{compare_images, load_image, ImageDifference, MinImage};

use pluot::{render, RenderParams, GraphicsFormat};

fn snapshots_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("snaps-blessed")
}

fn current_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("snaps-dirty")
}

/// The render function appends one trailing byte (`bailed_early` flag) to raster output.
const NUM_EXTRA_BYTES: usize = 1;

/// Convert raw RGBA bytes to a kompari `MinImage`.
fn rgba_bytes_to_image(data: &[u8], width: u32, height: u32) -> MinImage {
    let pixels: Vec<Rgba8> = data
        .chunks_exact(4)
        .map(|c| Rgba8::from_u8_array([c[0], c[1], c[2], c[3]]))
        .collect();
    MinImage {
        width,
        height,
        data: pixels,
    }
}

/// Render with the given params and compare the raster output against a PNG snapshot.
///
/// Handles the extra trailing byte, size assertion, not-all-black check,
/// and RGBA-to-MinImage conversion before delegating to `check_raster_snapshot`.
pub async fn render_and_check_raster_snapshot(params: RenderParams, name: &str) {
    let width = params.width;
    let height = params.height;
    let result_vec = render(params).await;

    assert_eq!(
        result_vec.len(),
        (width as usize) * (height as usize) * 4 + NUM_EXTRA_BYTES,
        "Unexpected raster output length",
    );

    let pixel_data = &result_vec[..result_vec.len() - NUM_EXTRA_BYTES];

    // The rendered image may be entirely zeros if all data points fall outside the viewport.

    let image = rgba_bytes_to_image(pixel_data, width, height);
    check_raster_snapshot(&image, name);
}

/// Render with both Raster and Vector formats and check snapshots for both.
///
/// `base_params` should have `format` left at its default; this function overrides
/// it for each format. Snapshot names are derived from `base_name` by appending
/// `.png` (raster) and `.svg` (vector).
pub async fn render_and_check_both_snapshots(base_params: RenderParams, base_name: &str) {
    // Only run on non-WASM targets and when the GPU is available.
    #[cfg(all(not(target_arch = "wasm32"), not(feature = "lacks_gpu")))]
    {
        let raster_params = RenderParams {
            format: GraphicsFormat::Raster,
            ..base_params.clone()
        };
        render_and_check_raster_snapshot(raster_params, &format!("{base_name}.png")).await;

        // TODO: add checks to compare the SVG and PNG output (by rasterizing the SVG using resvg).
    }
    // Always run the vector tests.
    let vector_params = RenderParams {
        format: GraphicsFormat::Vector,
        svg_compression_enabled: false,
        ..base_params
    };
    render_and_check_svg_snapshot(vector_params, &format!("{base_name}.svg")).await;
}

/// Render with the given params (which must use a `Script*` [`GraphicsFormat`])
/// and compare the generated code / JSON against a text snapshot named `name`.
///
/// Unlike the raster/vector snapshots, script rendering needs no GPU, so this
/// runs on every target/feature combination.
pub async fn render_and_check_script_snapshot(params: RenderParams, name: &str) {
    let result_vec = render(params).await;
    let script = String::from_utf8(result_vec).expect("Invalid UTF-8 in script output");
    check_text_snapshot(&script, name);
}

/// Compare a text string (generated source code / JSON) against a reference
/// snapshot named `name`.
///
/// Writes the current output to `tests/snaps-dirty/<name>`, compares against
/// `tests/snaps-blessed/<name>`, and panics with blessing instructions on
/// mismatch. Comparison trims trailing whitespace per line and ignores the
/// trailing newline, but preserves indentation and blank lines (which are
/// meaningful in generated code).
fn check_text_snapshot(text: &str, name: &str) {
    let snapshot_path = snapshots_dir().join(name);
    let current_path = current_dir().join(name);

    // Ensure snaps-dirty/ directory exists.
    std::fs::create_dir_all(current_dir()).unwrap();

    // Always write the current output so it can be inspected / blessed.
    std::fs::write(&current_path, text).unwrap();

    if !snapshot_path.exists() {
        panic!(
            "No script snapshot found at {path}.\n\
             A new file has been written to {current}.\n\
             Inspect it and bless with:\n  cp {current} {path}",
            path = snapshot_path.display(),
            current = current_path.display(),
        );
    }
    let reference = std::fs::read_to_string(&snapshot_path).unwrap();

    let normalize = |s: &str| -> String {
        s.lines().map(|l| l.trim_end()).collect::<Vec<_>>().join("\n")
    };

    if normalize(text) != normalize(&reference) {
        panic!(
            "Script snapshot mismatch for '{name}'.\n\
             Current output: {current}\n\
             Reference snapshot: {snap}\n\
             To accept the new output:\n  cp {current} {snap}",
            name = name,
            current = current_path.display(),
            snap = snapshot_path.display(),
        );
    }
}

/// Render with the given params and compare the SVG output against a text snapshot.
pub async fn render_and_check_svg_snapshot(params: RenderParams, name: &str) {
    let result_vec = render(params).await;

    // TODO: add a helper function or option to render the parent <svg/> element, so that the outputs are valid and render in other apps.

    let svg_string = String::from_utf8(result_vec).expect("Invalid UTF-8 in SVG output");
    check_svg_snapshot(&svg_string, name);
}

/// Compare an SVG string against a reference snapshot named `name`.
///
/// Writes current output to `tests/current/<name>`, compares against
/// `tests/snapshots/<name>`, panics with instructions on mismatch.
/// Comparison ignores leading/trailing whitespace per line and blank lines.
///
/// Public so callers can compare SVG text obtained some other way than
/// [`render_and_check_svg_snapshot`] (e.g. by executing a generated
/// `Script*`/`Expression*` snippet in its own language runtime; see
/// `test_render_script_integration.rs`).
pub fn check_svg_snapshot(svg: &str, name: &str) {
    let snapshot_path = snapshots_dir().join(name);
    let current_path = current_dir().join(name);

    // Ensure current/ directory exists.
    std::fs::create_dir_all(current_dir()).unwrap();

    // Always write the current output.
    std::fs::write(&current_path, svg).unwrap();

    // Load reference snapshot.
    if !snapshot_path.exists() {
        panic!(
            "No SVG snapshot found at {path}.\n\
             A new SVG has been written to {current}.\n\
             Inspect it and bless with:\n  cp {current} {path}",
            path = snapshot_path.display(),
            current = current_path.display(),
        );
    }
    let reference = std::fs::read_to_string(&snapshot_path).unwrap();

    // Normalize: trim each line, drop empty lines, then compare.
    let normalize = |s: &str| -> String {
        s.lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    };
    let current_norm = normalize(svg);
    let reference_norm = normalize(&reference);

    if current_norm != reference_norm {
        panic!(
            "SVG snapshot mismatch for '{name}'.\n\
             Current output: {current}\n\
             Reference snapshot: {snap}\n\
             To accept the new output:\n  cp {current} {snap}",
            name = name,
            current = current_path.display(),
            snap = snapshot_path.display(),
        );
    }
}

/// Compare `image` against the reference PNG snapshot named `name`.
///
/// On every call the current image is saved to `tests/current/<name>`.
/// - If no snapshot exists yet in `tests/snapshots/<name>`, the test panics
///   with instructions to inspect the output and bless it.
/// - If the snapshot exists but differs, the test panics with a diff message.
fn check_raster_snapshot(image: &MinImage, name: &str) {
    let snapshot_path = snapshots_dir().join(name);
    let current_path = current_dir().join(name);

    // Ensure current/ directory exists.
    std::fs::create_dir_all(current_dir()).unwrap();

    // Always write the current output so it can be inspected / blessed.
    let mut buf = Vec::new();
    image.encode_to_png(&mut buf).unwrap();
    std::fs::write(&current_path, &buf).unwrap();

    // Load reference snapshot.
    if !snapshot_path.exists() {
        panic!(
            "No snapshot found at {path}.\n\
             A new image has been written to {current}.\n\
             Inspect it and bless with:\n  cp {current} {path}",
            path = snapshot_path.display(),
            current = current_path.display(),
        );
    }
    let snapshot = load_image(&snapshot_path).unwrap();

    // Pixel-level comparison.
    let diff = compare_images(&snapshot, image);
    match diff {
        ImageDifference::None => {} // pass
        _ => {
            panic!(
                "Snapshot mismatch for '{name}'.\n\
                 Current output: {current}\n\
                 Reference snapshot: {snap}\n\
                 To accept the new output:\n  cp {current} {snap}",
                name = name,
                current = current_path.display(),
                snap = snapshot_path.display(),
            );
        }
    }
}
