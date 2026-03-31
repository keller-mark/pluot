//! `pluot` is a Rust crate that supports static and interactive visualization.

mod plugins;

// Export things needed for layer-based plotting via Rust.
pub use pluot_core::params::{RenderParams, PlotParams, LayerParams, GraphicsFormat, LayeredPlotRenderParams, ViewMode};
pub use pluot_core::render_traits::{AspectRatioMode, UnitsMode, ViewParams, MarginParams};


// Re-export layer param types for convenience.
pub use pluot_core::layers::point_layer::{PointLayerParams, PointShapeMode};
pub use pluot_core::layers::line_layer::{LineLayerParams};
pub use pluot_core::layers::rect_layer::{RectLayerParams};
pub use pluot_core::layers::text_layer::{TextLayerParams, TextAlignMode, TextBaselineMode};
pub use pluot_core::layers::bitmap_layer::{BitmapLayerParams, ChannelSettings};
pub use pluot_core::layers::axis_layer::{AxisLayerParams, AxisPosition};
pub use pluot_core::layers::point_3d_layer::Point3dLayerParams;
pub use pluot_zarr::layers::zarr_point_layer::ZarrPointLayerParams;
pub use pluot_zarr::layers::zarr_point_3d_layer::ZarrPoint3dLayerParams;
pub use pluot_zarr::layers::ome_zarr_bitmap_layer::OmeZarrBitmapLayerParams;
pub use pluot_zarr::layers::ome_zarr_multiscale_layer::OmeZarrMultiscaleLayerParams;

// Unified exports.
#[cfg(target_arch = "wasm32")]
pub use pluot_core::bindings::wasm::{render_wasm, set_panic_hook};

#[cfg(all(not(target_arch = "wasm32"), feature = "python"))]
pub use pluot_core::bindings::python::{render_py};

#[cfg(all(not(target_arch = "wasm32"), not(feature = "python")))]
pub use pluot_core::bindings::plain_rust::{render};
