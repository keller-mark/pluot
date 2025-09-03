mod utils;
mod zarr;
mod plots;
mod render;
mod bindings;
pub mod d3;

// When using Vello:
pub use vello::wgpu; // Re-export wgpu from vello for convenience.
// Switch to use the following when not using Vello:
//pub use wgpu;

pub use crate::utils::{RenderParams, PlotParams};

// Unified exports.
#[cfg(target_arch = "wasm32")]
pub use crate::bindings::wasm::{render_wasm, log, zarr_has, zarr_get, zarr_get_range_from_offset, zarr_get_range_from_end, set_panic_hook};

#[cfg(all(not(target_arch = "wasm32"), feature = "python"))]
pub use crate::bindings::python::{render_py, log, zarr_has, zarr_get, zarr_get_range_from_offset, zarr_get_range_from_end};

#[cfg(all(not(target_arch = "wasm32"), not(feature = "python")))]
pub use crate::bindings::plain_rust::{render, log, zarr_has, zarr_get, zarr_get_range_from_offset, zarr_get_range_from_end};