// Export things needed for layer-based plotting via Rust.
pub use pluot_core::params::{RenderParams, PlotParams, LayerParams, GraphicsFormat, LayeredPlotRenderParams};
pub use pluot_core::layers::core::{AspectRatioMode, UnitsMode, ViewParams, MarginParams};
pub use pluot_core::layers::scatterplot_layer::{ScatterplotLayerParams, PointShapeMode};

// Unified exports.
#[cfg(target_arch = "wasm32")]
pub use pluot_core::bindings::wasm::{
    log, render_wasm, set_panic_hook, zarr_get, zarr_get_range_from_end,
    zarr_get_range_from_offset, zarr_has,
};

#[cfg(all(not(target_arch = "wasm32"), feature = "python"))]
pub use pluot_core::bindings::python::{
    log_info as log, render_py, zarr_get, zarr_get_range_from_end, zarr_get_range_from_offset,
    zarr_has,
};

#[cfg(all(not(target_arch = "wasm32"), not(feature = "python")))]
pub use pluot_core::bindings::plain_rust::{
    log, render, zarr_get, zarr_get_range_from_end, zarr_get_range_from_offset, zarr_has,
};
