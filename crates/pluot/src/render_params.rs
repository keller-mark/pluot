use serde::{Deserialize, Serialize};

use pluot_core::layers::point_layer::{PointLayerParams, PointShapeMode};
use pluot_core::layers::line_layer::{LineLayerParams};
use pluot_core::layers::rect_layer::{RectLayerParams};
use pluot_core::layers::text_layer::{TextLayerParams, TextAlignMode, TextBaselineMode};
use pluot_core::layers::bitmap_layer::{BitmapLayerParams, ChannelSettings};
use pluot_core::layers::axis_linear_layer::{AxisLinearLayerParams, AxisPosition};
use pluot_core::layers::axis_band_layer::{AxisBandLayerParams};
use pluot_core::layers::point_3d_layer::Point3dLayerParams;
use pluot_core::layers::compute_layer::ComputeLayerParams;
use pluot_core::layers::tile_layer::TileLayerParams;
use pluot_core::layers::multiscale_layer::MultiscaleLayerParams;
use pluot_core::plot_layers::bar_plot_layer::BarPlotLayerParams;
use pluot_core::plot_layers::histogram_layer::HistogramLayerParams;

use pluot_zarr::layers::zarr_point_layer::ZarrPointLayerParams;
use pluot_zarr::layers::zarr_point_3d_layer::ZarrPoint3dLayerParams;
use pluot_zarr::layers::ome_zarr_bitmap_layer::OmeZarrBitmapLayerParams;
use pluot_zarr::layers::ome_zarr_multiscale_layer::OmeZarrMultiscaleLayerParams;
use pluot_zarr::layers::zarr_bar_plot_layer::ZarrBarPlotLayerParams;
use pluot_zarr::layers::zarr_histogram_layer::ZarrHistogramLayerParams;

use pluot_core::{AspectRatioAlignmentMode, LayerParams as RawLayerParams, RenderParams as RawRenderParams};
use pluot_core::params::{PlotParams, LayeredPlotRenderParams as RawLayeredPlotRenderParams};
use pluot_core::params::{GraphicsFormat, ViewMode, RenderBackend, ComputeBackend};
use pluot_core::render_traits::AspectRatioMode;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[cfg_attr(test, derive(strum::VariantNames))]
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

    AxisLinearLayer(AxisLinearLayerParams),
    AxisBandLayer(AxisBandLayerParams),

    // Plot-layers
    BarPlotLayer(BarPlotLayerParams),
    HistogramLayer(HistogramLayerParams),

    // Temporary/for development purposes only
    ComputeLayer(ComputeLayerParams),
    TileLayer(TileLayerParams),
    MultiscaleLayer(MultiscaleLayerParams),

    // Zarr
    ZarrPointLayer(ZarrPointLayerParams),
    ZarrBarPlotLayer(ZarrBarPlotLayerParams),
    ZarrHistogramLayer(ZarrHistogramLayerParams),
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

#[cfg(test)]
mod tests {
    use super::LayerParams;
    use pluot_core::registry::LayerRegistration;
    use std::collections::BTreeSet;
    use strum::VariantNames;

    /// Verifies that every layer registered via `inventory::submit!`
    /// has a matching variant in [`LayerParams`], and vice versa.
    #[test]
    fn layer_params_matches_inventory() {
        let in_enum: BTreeSet<&str> = LayerParams::VARIANTS.iter().copied().collect();
        let registered: BTreeSet<&str> = inventory::iter::<LayerRegistration>
            .into_iter()
            .map(|r| r.layer_type_name)
            .collect();

        let missing_from_enum: Vec<&&str> = registered.difference(&in_enum).collect();
        let extra_in_enum: Vec<&&str> = in_enum.difference(&registered).collect();

        assert!(
            missing_from_enum.is_empty() && extra_in_enum.is_empty(),
            "LayerParams enum and inventory registrations are out of sync.\n  \
             Registered via inventory but missing from LayerParams: {missing_from_enum:?}\n  \
             Present in LayerParams but not registered via inventory: {extra_in_enum:?}",
        );
    }
}
