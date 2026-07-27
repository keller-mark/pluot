// Syncs the subset of URW Core 35 fonts used by the `embed_fonts` feature
// (see src/layers/text_layer.rs) from the `vendor/urw-core35-fonts` git
// submodule at the workspace root into `src/vendored-fonts/` inside this
// crate.
//
// `cargo publish` can only package files that live inside the crate
// directory, so `include_bytes!` cannot reach the workspace-level vendor/
// submodule directly. The copies under `src/vendored-fonts/` are
// committed to git and are what actually gets embedded and published;
// this script just keeps them in sync with the submodule automatically
// whenever it's checked out (normal workspace dev builds). When the
// submodule isn't available (e.g. the isolated build `cargo publish`
// verifies against, or a build from the published crates.io tarball),
// this script is a no-op and the already-committed copies are used as-is.

use std::fs;
use std::path::PathBuf;

const FONT_FILES: &[&str] = &[
    "NimbusSans-Regular.ttf",
    "NimbusSans-Bold.ttf",
    "NimbusSans-Oblique.ttf",
    "NimbusSans-BoldOblique.ttf",
    "NimbusMonoPS-Regular.ttf",
    "NimbusMonoPS-Bold.ttf",
    "NimbusMonoPS-Italic.ttf",
    "NimbusMonoPS-BoldItalic.ttf",
    "NimbusRoman-Regular.ttf",
    "NimbusRoman-Bold.ttf",
    "NimbusRoman-Italic.ttf",
    "NimbusRoman-BoldItalic.ttf",
    "StandardSymbolsPS.ttf",
    "D050000L.ttf",
];

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source_dir = manifest_dir.join("../../vendor/urw-core35-fonts");
    let dest_dir = manifest_dir.join("src/vendored-fonts");

    for name in FONT_FILES {
        println!("cargo:rerun-if-changed={}", source_dir.join(name).display());
    }

    if !source_dir.is_dir() {
        return;
    }

    fs::create_dir_all(&dest_dir).expect("failed to create src/vendored-fonts");

    for name in FONT_FILES {
        let src = source_dir.join(name);
        let dst = dest_dir.join(name);
        let src_bytes = fs::read(&src).unwrap_or_else(|e| panic!("failed to read {}: {e}", src.display()));
        if fs::read(&dst).ok().as_deref() != Some(src_bytes.as_slice()) {
            fs::write(&dst, &src_bytes).unwrap_or_else(|e| panic!("failed to write {}: {e}", dst.display()));
        }
    }
}
