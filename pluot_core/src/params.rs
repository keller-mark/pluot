use crate::wgpu;
use crate::zarr::AsyncZarritaStore;
use crate::layers::core::AspectRatioMode;
use crate::layers::scatterplot_layer::ScatterplotLayerParams;
use crate::layers::zarr_scatterplot_layer::ZarrScatterplotLayerParams;
use crate::layers::line_layer::LineLayerParams;
use crate::layers::rect_layer::RectLayerParams;
use crate::layers::text_layer::TextLayerParams;
use crate::layers::bitmap_layer::BitmapLayerParams;
use crate::layers::axis_layer::AxisLayerParams;
use crate::layers::tile_layer::TileLayerParams;
use serde::{Deserialize, Serialize};
use svg::node::element::Group;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum GraphicsFormat {
    // 0: pixels
    Raster,
    // 1: SVG.
    Vector,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ViewMode {
    // 2D ~= OrthographicView in DeckGL terms
    // Reference: https://deck.gl/docs/developer-guide/views#types-of-views
    #[serde(rename = "2d")]
    TwoD,
    // 3D ~= OrbitView in DeckGL terms
    #[serde(rename = "3d")]
    ThreeD,
    // Note that 3D may have multiple camera modes
    // (e.g., orbit, turntable, matrix), but perhaps only the
    // interactive adapter needs to care about that.
    // Reference: https://github.com/mikolalysenko/3d-view
}


// TODO: use more Observable Plot-like parameter names?
// Reference: https://observablehq.com/plot/marks/bar

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "layer_type", content = "layer_params")]
pub enum LayerParams {
    // Using adjacently tagged enum representation.
    // { "layer_type": "ScatterplotLayer" }
    // Reference: https://serde.rs/enum-representations.html

    ScatterplotLayer(ScatterplotLayerParams),
    ZarrScatterplotLayer(ZarrScatterplotLayerParams),

    LineLayer(LineLayerParams),
    RectLayer(RectLayerParams),
    TextLayer(TextLayerParams),
    BitmapLayer(BitmapLayerParams),

    AxisLayer(AxisLayerParams),
    TileLayer(TileLayerParams)
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LayeredPlotRenderParams {
    pub layers: Vec<LayerParams>,
}


#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "plot_type", content = "plot_params")]
pub enum PlotParams {
    // Using adjacently tagged enum representation.
    // { "plot_type": "Scatterplot" }
    // Reference: https://serde.rs/enum-representations.html

    LayeredPlot(LayeredPlotRenderParams),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RenderParams {
    pub width: u32,
    pub height: u32,
    pub format: GraphicsFormat,

    // Device pixel ratio to support retina displays.
    // Default to 1.0 for standard displays.
    // Retina screens will have a value of 2.0 or higher.
    pub device_pixel_ratio: f32,

    // TODO: interactive adapters may support specifying zoom/target rather than camera_view,
    // but should internally convert to camera_view matrix if so.
    // Alternatively, use an enum type here to allow either, and put the logic on the rust side.
    //pub zoom: Option<f32>,
    //pub target_x: Option<f32>,
    //pub target_y: Option<f32>,
    pub camera_view: Option<[f32; 16]>,

    pub aspect_ratio_mode: AspectRatioMode,

    pub view_mode: ViewMode,

    // TODO: remove plot_params? instead, directly specify `layers`` here
    // without needing the extra nesting

    #[serde(flatten)]
    pub plot_params: PlotParams,

    // We need a plot ID for cacheing of certain intermediate expensive computations per plot.
    // Note that solely data-dependent computations should be cached via the (store_name, key) tuple.
    pub plot_id: String,
    pub store_name: String,

    // Timeout in ms before bailing out of awaiting a data request.
    pub timeout: Option<u32>,

    // Allow disabling memoization/cacheing. Useful for testing/debugging.
    pub cache_enabled: bool,

    // Whether to compress the SVG string using LZ-string if the output format is Vector.
    pub svg_compression_enabled: bool,

    // Margins for plots that need them (e.g. scatterplot axes).
    // TODO: make non-optional
    pub margin_left: Option<f32>,
    pub margin_right: Option<f32>,
    pub margin_top: Option<f32>,
    pub margin_bottom: Option<f32>,

    // Pickable determines whether an extra render target is created/used
    // to facilitate picking, but will only be true in certain situations
    // (e.g., interactive plots).
    pub pickable: bool,
}
pub struct RenderContext<'a> {
    pub store: &'a Arc<AsyncZarritaStore>,
    pub device: &'a wgpu::Device,
    pub texture_desc: &'a wgpu::TextureDescriptor<'a>,
    pub out_tex: &'a wgpu::Texture,
    pub queue: &'a wgpu::Queue,
    pub params: &'a RenderParams,

    pub vello_tex: &'a wgpu::Texture,
    //pub vello_scene: &'a mut vello::Scene,

    pub out_group: &'a mut Group,
}

pub struct RenderResult {
    pub bailed_early: bool,
}

impl Default for RenderParams {
    fn default() -> Self {
        Self {
            width: 100,
            height: 100,
            format: GraphicsFormat::Raster,

            device_pixel_ratio: 1.0,
            aspect_ratio_mode: AspectRatioMode::Contain,
            view_mode: ViewMode::TwoD,
            //zoom: None,
            //target_x: None,
            //target_y: None,
            camera_view: None,
            plot_id: "default_plot".to_string(),
            store_name: "default_store".to_string(),
            plot_params: PlotParams::LayeredPlot(LayeredPlotRenderParams {
                layers: vec![],
            }),
            timeout: None,
            cache_enabled: true,
            svg_compression_enabled: false,
            margin_left: None,
            margin_right: None,
            margin_top: None,
            margin_bottom: None,
            pickable: false,
        }
    }
}
