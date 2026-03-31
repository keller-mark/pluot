// I am having trouble deciding whether to centrally define the layer enum type,
// versus using `inventory` to register them in a more distributed way.
// Reference: https://github.com/keller-mark/pluot/issues/178


use serde::{Deserialize, Serialize};
use pluot_core::layer_traits::{PreparedAndDraw, ViewParams};

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

#[derive(Serialize, Deserialize, Debug)]
pub struct LayeredPlotRenderParams {
    pub layers: Vec<LayerParams>,
}


pub fn get_layer_from_registry(layer_params: &LayerParams, view_params: &ViewParams) -> Box<dyn PreparedAndDraw> {
    match layer_params {
        LayerParams::ZarrPointLayer(params) => {
            Box::new(ZarrPointLayer::new(
                view_params.clone(),
                params.clone(),
            )) as Box<dyn PreparedAndDraw>
        },
        LayerParams::PointLayer(params) => {
            Box::new(PointLayer::new(
                view_params.clone(),
                params.clone(),
            )) as Box<dyn PreparedAndDraw>
        },
        LayerParams::LineLayer(params) => {
            Box::new(LineLayer::new(
                view_params.clone(),
                params.clone(),
            )) as Box<dyn PreparedAndDraw>
        },
        LayerParams::TextLayer(params) => {
            Box::new(TextLayer::new(
                view_params.clone(),
                params.clone(),
            )) as Box<dyn PreparedAndDraw>
        },
        LayerParams::BitmapLayer(params) => {
            Box::new(BitmapLayer::new(
                view_params.clone(),
                params.clone(),
            )) as Box<dyn PreparedAndDraw>
        },
        LayerParams::AxisLayer(params) => {
            Box::new(AxisLayer::new(
                view_params.clone(),
                params.clone(),
            )) as Box<dyn PreparedAndDraw>
        },
        LayerParams::RectLayer(params) => {
            Box::new(RectLayer::new(
                view_params.clone(),
                params.clone(),
            )) as Box<dyn PreparedAndDraw>
        },
        LayerParams::TileLayer(params) => {
            Box::new(TileLayer::new(
                view_params.clone(),
                params.clone(),
            )) as Box<dyn PreparedAndDraw>
        },
        LayerParams::MultiscaleLayer(params) => {
            Box::new(MultiscaleLayer::new(
                view_params.clone(),
                params.clone(),
            )) as Box<dyn PreparedAndDraw>
        },
        // We do not want a catch-all here, so that we get a compile error
        // when implementing new layer types.
    }
}
