use crate::{render_traits::AspectRatioAlignmentMode, wgpu};
use crate::zarr::AsyncZarritaStore;
use crate::render_traits::AspectRatioMode;
use serde::{Deserialize, Serialize};
use svg::node::element::Group;
use std::sync::Arc;
use std::collections::HashMap;


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

    // When "rendering to a script", specify the output format.
    ExpressionRust,
    ScriptRust,
    ExpressionPython,
    // The python script should include PEP 723 inline script metadata for dependencies
    ScriptPython,
    ExpressionR,
    ScriptR,
    ExpressionJs,
    ScriptJs,
    ExpressionJsx,
    // TODO: when rendering to React, do not inline dict values (e.g., stores, plotParams). Construct useMemos
    // which memoize any objects, to prevent construction of new variable references on every rerender.
    ScriptReact,
    // TODO: use dynamic-importmap in the generated HTML?
    ScriptHtml,
    Json,

    // Use the pluot_cli from examples/pluot_cli
    ScriptBash,

    // TODO: support ScriptHtmlReact which uses the react component in a standalone HTML file?
    // TODO: jupyter nb?
    // TODO: marimo nb?
    // TODO: rmarkdown?
}

impl GraphicsFormat {
    /// Whether this format is a "code" output: rather than rendering pixels or an
    /// SVG, [`crate::render::render`] serializes the [`RenderParams`] into a
    /// string of source code (or JSON) that reproduces the plot using one of the
    /// language bindings (`bindings-js`, `bindings-r`, `bindings-python`) or the
    /// Rust API. See [`crate::render_script`].
    ///
    /// The `Expression*` variants emit a single expression (e.g. a function call
    /// or JSX element), whereas the `Script*` variants emit a self-contained
    /// script including imports, variable definitions and library initialization.
    pub fn is_code(&self) -> bool {
        matches!(
            self,
            GraphicsFormat::ExpressionRust
                | GraphicsFormat::ScriptRust
                | GraphicsFormat::ExpressionPython
                | GraphicsFormat::ScriptPython
                | GraphicsFormat::ExpressionR
                | GraphicsFormat::ScriptR
                | GraphicsFormat::ExpressionJs
                | GraphicsFormat::ScriptJs
                | GraphicsFormat::ExpressionJsx
                | GraphicsFormat::ScriptReact
                | GraphicsFormat::ScriptHtml
                | GraphicsFormat::Json
                | GraphicsFormat::ScriptBash
        )
    }
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

/// Path to the local Zarr store directory on disk.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LocalStoreParams {
    pub path: String
}


#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MemoryStoreParams {
    // For memory stores, they are not really portable in the same way as the other store types,
    // but perhaps we can show a custom message related to how the data originates.
    pub message: String
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RequestInit {
     pub method: Option<String>,
     pub headers: Option<HashMap<String, String>>,
     pub body: Option<String>,
     pub mode: Option<String>,
     pub credentials: Option<String>,
     pub cache: Option<String>,
     pub redirect: Option<String>,
     pub referrer: Option<String>,
     pub integrity: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HttpStoreParams {
    // Absolute URL to the root of the zarr store directory.
    pub url: String,
    pub options: Option<RequestInit>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "store_type", content = "store_params")]
pub enum ZarrStoreParams {
    HttpStore(HttpStoreParams),
    LocalStore(LocalStoreParams), // TODO: rename to FileSystemStore?
    MemoryStore(MemoryStoreParams),
    // TODO: ObjectStore(ObjectStoreParams),
    // TODO: WebFileSystemStore(WebFileSystemStoreParams),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ZarrStoreExtension {
    TiffAsVirtualZarr,
    OmeTiffAsVirtualZarr,
    Hdf5AsVirtualZarr,
    ParquetAsVirtualZarr,
    ZipAsVirtualZarr,
}


// We want to define sufficient info.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ZarrStoreInfo {

    #[serde(flatten)]
    pub store_params: ZarrStoreParams,

    // Used when one or more WrapperStores is needed "in front of" the store specified via store_params.
    // E.g., store_params may point to a non-zarr file or directory, requiring an extension mechanism
    // to interpret this file/directory as a zarr store.
    // See https://zarrita.dev/store-extensions.html for more information.
    // A given "primitive" store that points to a file/folder/dictionary may
    // require one or more wrapper store layers, to "virtualize" data for zarr compatibility.
    // For example, we can use a store extension to interpret OME-TIFF data as OME-Zarr,
    // or HDF5 as Zarr, agnostic to whether the original HDF5 file lives on HTTP or a local directory.
    // See https://github.com/keller-mark/hdf5-as-virtual-zarr.js
    // or https://github.com/keller-mark/tiff-as-virtual-zarr.js
    // or https://github.com/keller-mark/parquet-as-virtual-zarr.js
    pub store_extensions: Option<Vec<ZarrStoreExtension>>,

    // TODO: Should we define options like supports_writes, supports_deletes, supports_listing, etc.?
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

    /// Zarr stores, keyed by store name, defined once at the top level so that
    /// multiple layers can refer to the same store (and its metadata, such as
    /// which URL/path it points at and any store extensions it requires).
    ///
    /// Every Zarr-based layer identifies the store it reads from via a
    /// `store_name` field, which must be present in the keys of this map. As an
    /// ergonomic shortcut, a layer may omit `store_name` when exactly one store
    /// is defined here, in which case that single store is used. See
    /// [`crate::render_traits::resolve_store_name`].
    ///
    /// The language bindings (`bindings-js`, `bindings-python`, `bindings-r`)
    /// use each [`ZarrStoreInfo`] to construct the concrete store object and
    /// register it under its name before rendering, so that Rust's
    /// `zarr_`-prefixed bound functions can resolve `(store_name, key)` lookups.
    pub stores: Option<HashMap<String, ZarrStoreInfo>>,

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
            stores: None,
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
