mod bindings;
pub mod d3;
mod params;
mod plots;
mod cache;
mod render;
mod layers;

pub(crate) mod timeout;
pub mod two;
mod zarr;

mod maybe;

// When using Vello:
//pub use vello::wgpu;

// Re-export wgpu from vello for convenience.
// Switch to use the following when not using Vello:
pub use wgpu;

// Export things needed for layer-based plotting via Rust.
pub use crate::params::{RenderParams, PlotParams, LayerParams, GraphicsFormat, LayeredPlotRenderParams};
pub use crate::layers::core::{AspectRatioMode, UnitsMode, ViewParams, MarginParams};
pub use crate::layers::scatterplot_layer::{ScatterplotLayerParams, PointShapeMode};

// Unified exports.
#[cfg(target_arch = "wasm32")]
pub use crate::bindings::wasm::{
    log, render_wasm, set_panic_hook, zarr_get, zarr_get_range_from_end,
    zarr_get_range_from_offset, zarr_has,
};

#[cfg(all(not(target_arch = "wasm32"), feature = "python"))]
pub use crate::bindings::python::{
    log_info as log, render_py, zarr_get, zarr_get_range_from_end, zarr_get_range_from_offset,
    zarr_has,
};

#[cfg(all(not(target_arch = "wasm32"), not(feature = "python")))]
pub use crate::bindings::plain_rust::{
    log, render, zarr_get, zarr_get_range_from_end, zarr_get_range_from_offset, zarr_has,
};
