use crate::layer_traits::{PreparedAndDraw, ViewParams};
use crate::layers::multiscale_layer::MultiscaleLayer;
use crate::params::LayerParams;
use crate::layers::bitmap_layer::BitmapLayer;
use crate::layers::line_layer::LineLayer;
use crate::layers::rect_layer::RectLayer;
use crate::layers::point_layer::{PointShapeMode, PointLayer};
use crate::layers::text_layer::TextLayer;
use crate::layers::axis_layer::AxisLayer;
use crate::layers::tile_layer::TileLayer;
use crate::zarr_layers::zarr_point_layer::ZarrPointLayer;

/*
pub struct LayerRegistration {
    pub layer_type_name: &'static str,
    pub create_layer: fn(serde_json::Value, &ViewParams) -> Box<dyn PreparedAndDraw>,
}

inventory::collect!(LayerRegistration);

pub fn get_layer_from_registry(
    layer_type: &str,
    layer_params: serde_json::Value,
    view_params: &ViewParams,
) -> Box<dyn PreparedAndDraw> {
    for registration in inventory::iter::<LayerRegistration> {
        if registration.layer_type_name == layer_type {
            return (registration.create_layer)(layer_params, view_params);
        }
    }
    panic!("Unknown layer type: {}", layer_type);
}
*/

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
