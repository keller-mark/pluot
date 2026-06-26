//! This crate provides functionality for rendering visualizations.

mod plugins;

// Export things needed for layer-based plotting via Rust.
pub use pluot_core::params::{GraphicsFormat, ViewMode};
pub use pluot_core::render_traits::{AspectRatioMode, AspectRatioAlignmentMode, UnitsMode, ViewParams, MarginParams, ColorMode};
pub use pluot_core::{RenderParams as RawRenderParams, LayerParams as RawLayerParams, LayeredPlotRenderParams as RawLayeredPlotRenderParams, PlotParams as RawPlotParams};
pub use pluot_core::{project, unproject, get_bounds};

// Re-export layer param types for convenience.
pub use pluot_core::layers::point_layer::{PointLayerParams, PointShapeMode};
pub use pluot_core::layers::line_layer::{LineLayerParams};
pub use pluot_core::layers::curve_layer::{CurveLayerParams, PathCommand};
pub use pluot_core::layers::polygon_layer::PolygonLayerParams;
pub use pluot_core::layers::rect_layer::{RectLayerParams};
pub use pluot_core::layers::text_layer::{TextLayerParams, TextAlignMode, TextBaselineMode};
pub use pluot_core::render_traits::{FontWeight, FontStyle};
pub use pluot_core::layers::bitmap_layer::{BitmapLayerParams, ChannelSettings, DimensionOrder, NumericData};
pub use pluot_core::layers::axis_linear_layer::{AxisLinearLayerParams, AxisPosition};
pub use pluot_core::layers::axis_band_layer::{AxisBandLayerParams};
pub use pluot_core::layers::point_3d_layer::Point3dLayerParams;
pub use pluot_core::plot_layers::bar_plot_layer::{BarPlotLayerParams, BarOrientation};
pub use pluot_core::plot_layers::histogram_layer::HistogramLayerParams;

// Zarr layers
pub use pluot_zarr::layers::zarr_point_layer::ZarrPointLayerParams;
pub use pluot_zarr::layers::zarr_point_3d_layer::ZarrPoint3dLayerParams;
pub use pluot_zarr::layers::ome_zarr_bitmap_layer::OmeZarrBitmapLayerParams;
pub use pluot_zarr::layers::ome_zarr_multiscale_layer::OmeZarrMultiscaleLayerParams;
pub use pluot_zarr::layers::zarr_bar_plot_layer::ZarrBarPlotLayerParams;
pub use pluot_zarr::layers::zarr_histogram_layer::ZarrHistogramLayerParams;

mod render_params;

pub use crate::render_params::{RenderParams, LayerParams};


// TODO: picking exports.

// Unified exports.
mod render;
pub use crate::render::{render};

// Exports for WASM bindings.
#[cfg(target_arch = "wasm32")]
pub use pluot_core::bindings::wasm::{render_wasm, set_panic_hook};

// Exports for Python bindings.
#[cfg(all(not(target_arch = "wasm32"), feature = "python"))]
pub use pluot_core::bindings::python::{render_py};
