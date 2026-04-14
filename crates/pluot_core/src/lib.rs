pub mod bindings;
pub mod d3;
pub mod params;
pub mod render_types;
pub mod cache;
pub mod render_traits;
mod render;
pub mod positioning;
pub mod viewport;
mod picking;
pub mod layers;
pub mod registry;
pub mod compute;

pub(crate) mod timeout;
pub mod two;
pub mod zarr_types;
pub mod zarr;

pub mod maybe;

// When using Vello:
//pub use vello::wgpu;

// Re-export wgpu from vello for convenience.
// Switch to use the following when not using Vello:
pub use wgpu;

// Export things needed for layer-based plotting via Rust.
pub use crate::params::{RenderParams, PlotParams, LayerParams, GraphicsFormat, LayeredPlotRenderParams, ViewMode};
pub use crate::render_traits::{AspectRatioMode, AspectRatioAlignmentMode, UnitsMode, ViewParams, MarginParams};
pub use crate::registry::{LayerRegistration, get_layer_from_registry};
pub use crate::render::{render};
pub use crate::picking::{pick, PickingResult, LayerPickingResult};
pub use crate::viewport::{project, unproject, get_bounds, camera_matrix_to_zoom_and_translation};

// Export things needed by workspace packages that define other layers.
pub use crate::cache::{get_or_init_store, use_memo_vec_f32, use_memo_vec_i32, use_memo_vec_string, use_memo_numeric_data};

// Unified exports.
#[cfg(target_arch = "wasm32")]
pub use crate::bindings::wasm::{
    log, zarr_get, zarr_get_range_from_end, zarr_get_range_from_offset, zarr_has,
    zarr_get_status, zarr_get_range_from_end_status, zarr_get_range_from_offset_status, zarr_has_status,
};

#[cfg(all(not(target_arch = "wasm32"), feature = "python"))]
pub use crate::bindings::python::{
    log_info as log, zarr_get, zarr_get_range_from_end, zarr_get_range_from_offset, zarr_has,
    zarr_get_status, zarr_get_range_from_end_status, zarr_get_range_from_offset_status, zarr_has_status,
};

#[cfg(all(not(target_arch = "wasm32"), not(feature = "python")))]
pub use crate::bindings::plain_rust::{
    log, zarr_get, zarr_get_range_from_end, zarr_get_range_from_offset, zarr_has,
    zarr_get_status, zarr_get_range_from_end_status, zarr_get_range_from_offset_status, zarr_has_status,
};
