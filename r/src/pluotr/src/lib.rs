// Import dependencies
extern crate libc;

// Modules are other .rs source files
mod hello;

// Export functions called by R
pub use hello::rust_roundtrip;

