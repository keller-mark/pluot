pub mod bindings;
pub mod d3;
pub mod params;
// mod plots;
pub mod cache;
pub mod layer_traits;
pub mod layered_plot;
mod render;
pub mod layers;
pub mod zarr_layers;
pub mod registry;

pub(crate) mod timeout;
pub mod two;
pub mod zarr;

pub mod maybe;

// When using Vello:
//pub use vello::wgpu;

// Re-export wgpu from vello for convenience.
// Switch to use the following when not using Vello:
pub use wgpu;

// Export things needed for layer-based plotting via Rust.
pub use crate::params::{RenderParams, PlotParams, LayerParams, GraphicsFormat, LayeredPlotRenderParams, ViewMode};
pub use crate::layer_traits::{AspectRatioMode, UnitsMode, ViewParams, MarginParams};
pub use crate::registry::{get_layer_from_registry};

// Export things needed by workspace packages that define other layers.
pub use crate::cache::{get_or_init_store, use_memo_vec_f32, use_memo_vec_i32};

// Unified exports.
#[cfg(target_arch = "wasm32")]
pub use crate::bindings::wasm::{
    log, zarr_get, zarr_get_range_from_end, zarr_get_range_from_offset, zarr_has,
};

#[cfg(all(not(target_arch = "wasm32"), feature = "python"))]
pub use crate::bindings::python::{
    log_info as log, zarr_get, zarr_get_range_from_end, zarr_get_range_from_offset, zarr_has,
};

#[cfg(all(not(target_arch = "wasm32"), not(feature = "python")))]
pub use crate::bindings::plain_rust::{
    log, zarr_get, zarr_get_range_from_end, zarr_get_range_from_offset, zarr_has,
};
