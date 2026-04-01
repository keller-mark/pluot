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

use pluot_core::{LayerParams as RawLayerParams, RenderParams as RawRenderParams};
use pluot_core::params::{PlotParams, LayeredPlotRenderParams as RawLayeredPlotRenderParams};
pub use pluot_core::params::{GraphicsFormat, ViewMode, RenderBackend, ComputeBackend};
pub use pluot_core::render_traits::AspectRatioMode;

pub use pluot_core::bindings::plain_rust::{render as raw_render};

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
#[derive(Debug, Clone)]
pub struct RenderParams {
    pub layers: Vec<LayerParams>,
    pub width: u32,
    pub height: u32,
    pub format: GraphicsFormat,
    pub device_pixel_ratio: f32,
    pub camera_view: Option<[f32; 16]>,
    pub aspect_ratio_mode: AspectRatioMode,
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

fn to_raw_layer_params(layers: &[LayerParams]) -> Vec<RawLayerParams> {
    layers.iter().map(|layer| {
        // LayerParams is tagged as { "layer_type": "...", "layer_params": {...} }
        // which matches the fields of RawLayerParams exactly.
        let value = serde_json::to_value(layer).expect("LayerParams serialization failed");
        let obj = value.as_object().expect("LayerParams must serialize to an object");
        RawLayerParams {
            layer_type: obj["layer_type"].as_str().expect("layer_type must be a string").to_string(),
            layer_params: obj["layer_params"].clone(),
        }
    }).collect()
}

pub async fn render(render_params: RenderParams) -> Vec<u8> {
    let raw_layers = to_raw_layer_params(&render_params.layers);
    let raw_params = RawRenderParams {
        width: render_params.width,
        height: render_params.height,
        format: render_params.format,
        device_pixel_ratio: render_params.device_pixel_ratio,
        camera_view: render_params.camera_view,
        aspect_ratio_mode: render_params.aspect_ratio_mode,
        view_mode: render_params.view_mode,
        plot_id: render_params.plot_id,
        store_name: render_params.store_name,
        wait_for_store_gets: render_params.wait_for_store_gets,
        timeout: render_params.timeout,
        cache_enabled: render_params.cache_enabled,
        svg_compression_enabled: render_params.svg_compression_enabled,
        svg_include_document: render_params.svg_include_document,
        margin_left: render_params.margin_left,
        margin_right: render_params.margin_right,
        margin_top: render_params.margin_top,
        margin_bottom: render_params.margin_bottom,
        pickable: render_params.pickable,
        render_backend: render_params.render_backend,
        compute_backend: render_params.compute_backend,
        plot_params: PlotParams::LayeredPlot(RawLayeredPlotRenderParams {
            layers: raw_layers,
        }),
    };
    raw_render(raw_params).await
}
