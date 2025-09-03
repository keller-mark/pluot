mod bindings;
pub mod d3;
mod plots;
mod render;
pub mod two;
mod utils;
mod zarr;

// When using Vello:
pub use vello::wgpu; // Re-export wgpu from vello for convenience.
                     // Switch to use the following when not using Vello:
                     //pub use wgpu;

pub use crate::utils::{PlotParams, RenderParams};

// Unified exports.
#[cfg(target_arch = "wasm32")]
pub use crate::bindings::wasm::{
    log, render_wasm, set_panic_hook, zarr_get, zarr_get_range_from_end,
    zarr_get_range_from_offset, zarr_has,
};

#[cfg(all(not(target_arch = "wasm32"), feature = "python"))]
pub use crate::bindings::python::{
    log, render_py, zarr_get, zarr_get_range_from_end, zarr_get_range_from_offset, zarr_has,
};

#[cfg(all(not(target_arch = "wasm32"), not(feature = "python")))]
pub use crate::bindings::plain_rust::{
    log, render, zarr_get, zarr_get_range_from_end, zarr_get_range_from_offset, zarr_has,
};
