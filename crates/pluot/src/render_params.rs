use serde::{Deserialize, Serialize};

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

use pluot_core::{AspectRatioAlignmentMode, LayerParams as RawLayerParams, RenderParams as RawRenderParams};
use pluot_core::params::{PlotParams, LayeredPlotRenderParams as RawLayeredPlotRenderParams};
pub use pluot_core::params::{GraphicsFormat, ViewMode, RenderBackend, ComputeBackend};
pub use pluot_core::render_traits::AspectRatioMode;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "layer_type", content = "layer_params")]
pub enum LayerParams {
    // Using adjacently tagged enum representation.
    // { "layer_type": "ScatterplotLayer" }
    // Reference: https://serde.rs/enum-representations.html

    PointLayer(PointLayerParams),
    LineLayer(LineLayerParams),
    RectLayer(RectLayerParams),
    TextLayer(TextLayerParams),
    BitmapLayer(BitmapLayerParams),

    AxisLayer(AxisLayerParams),

    // Zarr
    ZarrPointLayer(ZarrPointLayerParams),
    OmeZarrBitmapLayer(OmeZarrBitmapLayerParams),
    OmeZarrMultiscaleLayer(OmeZarrMultiscaleLayerParams),

    // 3D
    Point3dLayer(Point3dLayerParams),
    ZarrPoint3dLayer(ZarrPoint3dLayerParams),
}

/// Strongly-typed render params. Mirrors [`RawRenderParams`] but accepts
/// `layers` as a typed [`LayerParams`] enum instead of raw JSON values.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RenderParams {
    pub layers: Vec<LayerParams>,
    pub width: u32,
    pub height: u32,
    pub format: GraphicsFormat,
    pub device_pixel_ratio: f32,
    pub camera_view: Option<[f32; 16]>,
    pub aspect_ratio_mode: AspectRatioMode,
    pub aspect_ratio_alignment_mode: AspectRatioAlignmentMode,
    pub view_mode: ViewMode,
    pub plot_id: String,
    pub store_name: String,
    pub wait_for_store_gets: bool,
    pub timeout: Option<u32>,
    pub cache_enabled: bool,
    pub svg_compression_enabled: bool,
    pub svg_include_document: bool,
    pub margin_left: Option<f32>,
    pub margin_right: Option<f32>,
    pub margin_top: Option<f32>,
    pub margin_bottom: Option<f32>,
    pub pickable: bool,
    pub render_backend: Option<RenderBackend>,
    pub compute_backend: Option<ComputeBackend>,
}

impl Default for RenderParams {
    fn default() -> Self {
        let raw = RawRenderParams::default();
        Self {
            layers: vec![],
            width: raw.width,
            height: raw.height,
            format: raw.format,
            device_pixel_ratio: raw.device_pixel_ratio,
            camera_view: raw.camera_view,
            aspect_ratio_mode: raw.aspect_ratio_mode,
            aspect_ratio_alignment_mode: raw.aspect_ratio_alignment_mode,
            view_mode: raw.view_mode,
            plot_id: raw.plot_id,
            store_name: raw.store_name,
            wait_for_store_gets: raw.wait_for_store_gets,
            timeout: raw.timeout,
            cache_enabled: raw.cache_enabled,
            svg_compression_enabled: raw.svg_compression_enabled,
            svg_include_document: raw.svg_include_document,
            margin_left: raw.margin_left,
            margin_right: raw.margin_right,
            margin_top: raw.margin_top,
            margin_bottom: raw.margin_bottom,
            pickable: raw.pickable,
            render_backend: raw.render_backend,
            compute_backend: raw.compute_backend,
        }
    }
}
