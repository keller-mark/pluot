//! Build script to build the UI.

fn main() {
    slint_build::compile("ui/main_app.slint").expect("Slint build failed");
}
