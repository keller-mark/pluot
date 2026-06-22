use crate::{render_traits::AspectRatioAlignmentMode, wgpu};
use crate::zarr::AsyncZarritaStore;
use crate::render_traits::AspectRatioMode;
use serde::{Deserialize, Serialize};
use svg::node::element::Group;
use std::sync::Arc;


/// Select whether to use GPU or CPU for graphics rendering.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum RenderBackend {
    /// GPU via WebGPU render pipelines.
    Gpu,
    /// CPU
    Cpu,
}

/// Select whether to use GPU or CPU for compute operations.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ComputeBackend {
    /// GPU via WebGPU compute pipelines.
    Gpu,
    /// CPU
    Cpu,
}

/// The graphics format for outputs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum GraphicsFormat {
    /// Raster / Bitmap / Canvas / Pixels
    Raster,
    /// Vector / SVG
    Vector,

    // TODO: add AccessKit as a GraphicsFormat?
}

/// Whether displaying 2D versus 3D graphics.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ViewMode {
    // 2D ~= OrthographicView in DeckGL terms
    // Reference: https://deck.gl/docs/developer-guide/views#types-of-views
    /// 2D
    #[serde(rename = "2d")]
    TwoD,
    // 3D ~= OrbitView in DeckGL terms
    /// 3D
    #[serde(rename = "3d")]
    ThreeD,
    // Note that 3D may have multiple camera modes
    // (e.g., orbit, turntable, matrix), but perhaps only the
    // interactive adapter needs to care about that.
    // Reference: https://github.com/mikolalysenko/3d-view
}

/// Layer parameters in their raw serde Value form.
///
/// Each layer type is identified by its `layer_type` string,
/// and the `layer_params` field holds the layer-specific parameters as
/// an opaque JSON value.
/// Layers register themselves via `inventory::submit!` with
/// a factory function that knows how to deserialize their specific params.
///
/// JSON wire format: `{"layer_type": "PointLayer", "layer_params": {...}}`
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LayerParams {
    pub layer_type: String,
    // TODO: figure out how to enable this value to be type-checked
    // when used within Rust code.
    pub layer_params: serde_json::Value,
}


#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LayeredPlotRenderParams {
    pub layers: Vec<LayerParams>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "plot_type", content = "plot_params")]
pub enum PlotParams {
    // Using adjacently tagged enum representation.
    // { "plot_type": "Scatterplot" }
    // Reference: https://serde.rs/enum-representations.html

    LayeredPlot(LayeredPlotRenderParams),
}

/// The params that are passed to the [`crate::render::render`] function.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct RenderParams {
    /// The width of the plot, in pixels.
    pub width: u32,
    /// The height of the plot, in pixels.
    pub height: u32,
    /// Format to use for outputs.
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
    /// The 4x4 camera matrix.
    pub camera_view: Option<[f32; 16]>,

    pub aspect_ratio_mode: AspectRatioMode,
    pub aspect_ratio_alignment_mode: AspectRatioAlignmentMode,

    pub view_mode: ViewMode,

    // TODO: remove plot_params? instead, directly specify `layers`` here
    // without needing the extra nesting

    #[serde(flatten)]
    pub plot_params: PlotParams,

    // We need a plot ID for cacheing of certain intermediate expensive computations per plot.
    // Note that solely data-dependent computations should be cached via the (store_name, key) tuple.
    /// The plot ID is used for cacheing of intermediate values per plot,
    /// and should therefore be unique among plots in the same application.
    pub plot_id: String,

    /// The name of the Zarr store, used when calling the `zarr_`-prefixed bound functions.
    pub store_name: String,

    /// Whether to wait for store.get and store.getRange async calls to resolve.
    /// If true, we will try to wait for .get/.getRange async calls to resolve (BUT we will still bail early if `timeout` elapses first).
    /// If false, proceed to rendering something partially, without waiting for all .get/.getRange async calls to successfully resolve.
    pub wait_for_store_gets: bool,

    // TODO: combine wait_for_store_gets and timeout into a single enum, since the timeout value is irrelevant when wait_for_store_gets is false

    /// Timeout in ms before bailing out of awaiting a data request.
    pub timeout: Option<u32>,

    /// Allow disabling memoization/cacheing. Useful for testing/debugging.
    pub cache_enabled: bool,

    /// Whether to compress the SVG string using LZ-string if the output format is Vector.
    pub svg_compression_enabled: bool,

    /// Whether to include the parent `<svg>` document tag,
    /// versus only the inner `<g>` group/contents.
    pub svg_include_document: bool,

    // TODO: make non-optional
    /// Margins for plots that need them (e.g. scatterplot axes).
    pub margin_left: Option<f32>,
    pub margin_right: Option<f32>,
    pub margin_top: Option<f32>,
    pub margin_bottom: Option<f32>,

    /// Pickable determines whether an extra render target is created/used
    /// to facilitate picking, but will only be true in certain situations
    /// (e.g., interactive plots).
    pub pickable: bool,

    /// Whether to use GPU or CPU for rendering.
    /// If None, try GPU, then fallback to CPU.
    pub render_backend: Option<RenderBackend>,

    /// Whether to use GPU or CPU for compute operations.
    /// If None, try GPU, then fallback to CPU.
    pub compute_backend: Option<ComputeBackend>,
}

impl Default for RenderParams {
    fn default() -> Self {
        Self {
            width: 100,
            height: 100,
            format: GraphicsFormat::Raster,

            device_pixel_ratio: 1.0,
            aspect_ratio_mode: AspectRatioMode::Contain,
            aspect_ratio_alignment_mode: AspectRatioAlignmentMode::Center,
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
            wait_for_store_gets: true,
            timeout: None,
            cache_enabled: true,
            svg_compression_enabled: false,
            svg_include_document: true,
            margin_left: None,
            margin_right: None,
            margin_top: None,
            margin_bottom: None,
            pickable: false,
            render_backend: None,
            compute_backend: None,
        }
    }
}
