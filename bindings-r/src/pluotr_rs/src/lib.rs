// Import dependencies
extern crate libc;

// Modules are other .rs source files
mod hello;
mod render;

// Export functions called by R
pub use hello::rust_roundtrip;
pub use render::{rust_render, free_bytes_from_rust};

