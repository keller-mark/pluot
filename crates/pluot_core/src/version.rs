// Get the version of the crate at compile-time,
// to enable specifying the version in the generated scripts.
// Note that the Rust, Python, JS, and R packages are
// versioned together, so this version applies to them as well.
pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");
