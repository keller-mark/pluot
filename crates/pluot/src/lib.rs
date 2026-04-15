//! `pluot` is a Rust crate that supports static and interactive visualization.

mod plugins;

// Export things needed for layer-based plotting via Rust.
pub use pluot_core::params::{GraphicsFormat, ViewMode};
pub use pluot_core::render_traits::{AspectRatioMode, AspectRatioAlignmentMode, UnitsMode, ViewParams, MarginParams};
pub use pluot_core::{LayerParams as RawLayerParams, RenderParams as RawRenderParams};


// Re-export layer param types for convenience.
pub use pluot_core::layers::point_layer::{PointLayerParams, PointShapeMode};
pub use pluot_core::layers::line_layer::{LineLayerParams};
pub use pluot_core::layers::rect_layer::{RectLayerParams};
pub use pluot_core::layers::text_layer::{TextLayerParams, TextAlignMode, TextBaselineMode};
pub use pluot_core::layers::bitmap_layer::{BitmapLayerParams, ChannelSettings};
pub use pluot_core::layers::axis_linear_layer::{AxisLinearLayerParams, AxisPosition};
pub use pluot_core::layers::axis_band_layer::{AxisBandLayerParams};
pub use pluot_core::layers::point_3d_layer::Point3dLayerParams;
pub use pluot_core::plot_layers::bar_plot_layer::{BarPlotLayerParams, BarOrientation};

// Zarr layers
pub use pluot_zarr::layers::zarr_point_layer::ZarrPointLayerParams;
pub use pluot_zarr::layers::zarr_point_3d_layer::ZarrPoint3dLayerParams;
pub use pluot_zarr::layers::ome_zarr_bitmap_layer::OmeZarrBitmapLayerParams;
pub use pluot_zarr::layers::ome_zarr_multiscale_layer::OmeZarrMultiscaleLayerParams;
pub use pluot_zarr::layers::zarr_bar_plot_layer::ZarrBarPlotLayerParams;

mod render_params;

pub use crate::render_params::{RenderParams, LayerParams};

// Unified exports.
mod render;
pub use crate::render::{render};

// Exports for WASM bindings.
#[cfg(target_arch = "wasm32")]
pub use pluot_core::bindings::wasm::{render_wasm, set_panic_hook};

// Exports for Python bindings.
#[cfg(all(not(target_arch = "wasm32"), feature = "python"))]
pub use pluot_core::bindings::python::{render_py};
