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


pub use pluot_core::{LayerParams as RawLayerParams, RenderParams as RawRenderParams};

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

#[derive(Serialize, Deserialize, Debug)]
pub struct LayeredPlotRenderParams {
    pub layers: Vec<LayerParams>,
}

fn to_raw_layer_params(layers: &[LayerParams]) -> Vec<RawLayerParams> {
    vec![
        // TODO: convert each element of layers to a RawLayerParams object.
        RawLayerParams {
            layer_type: "LineLayer".to_string(),
            layer_params: serde_json::to_value(typed_layer_params).unwrap(),
        }
    ]
}

// TODO: raw_render accepts layer_params (within plot_params) as serde_json Value type.
// Here, we want to define a render() function that accepts layer_params as a strongly-typed value instead.
// We need to define a strongly-typed RenderParams and convert from LayerParams to RawLayerParams.
pub async fn render(render_params: RenderParams) {


    let raw_params = RawRenderParams {
        // TODO
        plot_params // TODO
    };


    // Render the plot.
    let result = raw_render(raw_params).await;

    return result;
}
