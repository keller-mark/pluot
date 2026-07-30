use crate::{render_traits::AspectRatioAlignmentMode, wgpu};
use crate::zarr::AsyncZarritaStore;
use crate::render_traits::AspectRatioMode;
use crate::version::CRATE_VERSION;
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
}


/// The code format for render-to-script outputs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum CodeFormat {
    ExpressionRust,
    ScriptRust,
    ExpressionPython,
    ScriptPython,
    ExpressionR,
    ScriptR,
    ExpressionJs,
    ScriptJs,
    ExpressionJsx,
    ScriptReact,
    ScriptHtml,
    ScriptHtmlReact,
    Json,

    // Uses the pluot_cli from examples/pluot_cli
    ScriptBash,

    // TODO: jupyter nb?
    // TODO: marimo nb?
    // TODO: rmarkdown?
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
/// Layers register themselves via `inventory::submit!` with
/// a factory function that knows how to deserialize their specific params.
///
/// Serializes to `{"layer_type": "PointLayer", "layer_params": {...}}`
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LayerParams {
    pub layer_type: String,
    pub layer_params: serde_json::Value,
}

/// Specify [`LayerParams`] for rendering of one or more layers.
///
/// Serializes to `{"layers": [...]}`
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LayeredPlotRenderParams {
    pub layers: Vec<LayerParams>,
}

/// Specify how to render a plot.
///
/// Currently, a sole layer-wise configuration mechanism is supported,
/// but this could be expanded in the future.
///
/// Serializes to `{"plot_type": "LayeredPlot", "plot_params": {...}}`
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "plot_type", content = "plot_params")]
pub enum PlotParams {
    // Using adjacently tagged enum representation.
    // { "plot_type": "Scatterplot" }
    // Reference: https://serde.rs/enum-representations.html

    LayeredPlot(LayeredPlotRenderParams),
}

/// Represents a local, filesystem-backed Zarr directory store.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LocalStoreParams {
    /// Path to the local Zarr store directory on disk.
    pub path: String
}

/// Represents an in-memory, ephemeral Zarr store.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MemoryStoreParams {
    /// Display a custom message indicating the origin/source of the store's data,
    /// and potentially how to re-construct it from scratch.
    // For memory stores, they are not really portable in the same way as the other store types,
    // but perhaps we can show a custom message related to how the data originates.
    pub message: String
}

/// Specify additional options to pass when making HTTP requests.
///
/// Corresponds to the second parameter of the JavaScript `fetch` API.
// Reference: https://developer.mozilla.org/en-US/docs/Web/API/Fetch_API
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

/// Represents a remote Zarr store located on a standard HTTP static file server.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HttpStoreParams {
    /// The absolute URL to the root of the Zarr store directory.
    pub url: String,
    /// Optional parameters, such as authentication headers, to use when making HTTP requests to load data from this Zarr store.
    pub options: Option<RequestInit>,
}

/// A serializable, cross-platform representation of a Zarr store.
///
/// Serializes to `{"store_type": "HttpStore", "store_params": {...}}`
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "store_type", content = "store_params")]
pub enum ZarrStoreParams {
    HttpStore(HttpStoreParams),
    LocalStore(LocalStoreParams), // TODO: rename to FileSystemStore?
    MemoryStore(MemoryStoreParams),
    // TODO: ObjectStore(ObjectStoreParams),
    // TODO: WebFileSystemStore(WebFileSystemStoreParams),
}

/// A serializable, cross-platform representation of a Zarr store extension.
///
/// These allow specifying wrapper store functionality
/// to "virtualize" non-zarr data as zarr data.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ZarrStoreExtension {
    TiffAsVirtualZarr,
    OmeTiffAsVirtualZarr,
    Hdf5AsVirtualZarr,
    ParquetAsVirtualZarr,
    ZipAsVirtualZarr,
}


/// A serializable, cross-platform representation of a Zarr store and any associated extensions.
///
/// Serializes to `{"store_type": "HttpStore", "store_params": {...}, "store_extensions": [...]}`
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

    /// To enable forward compatibility, specify the schema version that was used to generate the plot.
    /// For now, we will just specify the crate version, and will throw a warning if there is a mismatch.
    /// We will not fully error, nor will we initially implement any auto-upgrade functionality to convert a prior version to a later version,
    /// but we could implement these features in the future.
    /// TODO: In the future, we should also decouple the schema_version from the crate/package version,
    /// as the latter could advance more quickly than the former.
    pub schema_version: Option<String>,

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
            schema_version: Some(CRATE_VERSION.to_string()),
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
