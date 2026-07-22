use clap::Parser;
use serde::{Deserialize, Serialize};
use image::{save_buffer_with_format, ColorType, ImageFormat};
use std::collections::HashMap;
use std::sync::Arc;

use pluot::{
    render, render_with_stores, AspectRatioMode, GraphicsFormat, LayerParams, RenderParams, ViewMode,
    ZarrStoreInfo, ZarrStoreParams, HttpStoreParams, LocalStoreParams, MemoryStoreParams, StoreMap,
};
use zarrs_storage::storage_adapter::sync_to_async::{SyncToAsyncSpawnBlocking, SyncToAsyncStorageAdapter};
use zarrs_storage::AsyncReadableStorageTraits;
use resvg::usvg;
use tiny_skia;
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;
use std::process;

/// Runs blocking `zarrs_filesystem`/`zarrs_http` store calls on Tokio's
/// blocking thread pool, so they can back the `async` store trait that
/// `render_with_stores` expects. Mirrors the example in
/// [`SyncToAsyncSpawnBlocking`]'s docs.
struct TokioSpawnBlocking;

impl SyncToAsyncSpawnBlocking for TokioSpawnBlocking {
    fn spawn_blocking<F, R>(&self, f: F) -> impl std::future::Future<Output = R> + Send
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        async move { tokio::task::spawn_blocking(f).await.unwrap() }
    }
}

/// Construct the real Zarr store instances declared in `stores` (an `HttpStore`
/// or `LocalStore` per entry), wrapping each synchronous `zarrs_http`/
/// `zarrs_filesystem` store so it satisfies the async store trait that
/// `render_with_stores` expects.
///
/// `MemoryStore` entries are rejected: unlike the JS/Python bindings, `pluot_cli`
/// has no generic byte payload to construct one from JSON.
fn build_store_map(stores: &HashMap<String, ZarrStoreInfo>) -> StoreMap {
    let mut map: HashMap<String, Arc<dyn AsyncReadableStorageTraits>> =
        HashMap::with_capacity(stores.len());
    for (name, info) in stores {
        let store: Arc<dyn AsyncReadableStorageTraits> = match &info.store_params {
            ZarrStoreParams::HttpStore(HttpStoreParams { url, .. }) => {
                let sync_store = zarrs_http::HTTPStore::new(url).unwrap_or_else(|e| {
                    eprintln!("Error constructing HTTP store '{name}' at '{url}': {e}");
                    process::exit(1);
                });
                Arc::new(SyncToAsyncStorageAdapter::new(
                    Arc::new(sync_store),
                    TokioSpawnBlocking,
                ))
            }
            ZarrStoreParams::LocalStore(LocalStoreParams { path }) => {
                let sync_store = zarrs_filesystem::FilesystemStore::new(path).unwrap_or_else(|e| {
                    eprintln!("Error constructing local store '{name}' at '{path}': {e}");
                    process::exit(1);
                });
                Arc::new(SyncToAsyncStorageAdapter::new(
                    Arc::new(sync_store),
                    TokioSpawnBlocking,
                ))
            }
            ZarrStoreParams::MemoryStore(_) => {
                eprintln!(
                    "Error: store '{name}' is a MemoryStore, which pluot_cli cannot construct \
                     from JSON input (no generic byte payload). Use HttpStore or LocalStore."
                );
                process::exit(1);
            }
        };
        map.insert(name.clone(), store);
    }
    StoreMap(map)
}

/// Pluot CLI. Render plots to SVG or PNG.
///
/// Plot parameters (plot_type + plot_params) are read as JSON from a file
/// (--input) or from stdin. All other rendering parameters are provided
/// via CLI flags.
///
/// The output format (SVG or PNG) is inferred from the --output file extension.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to a JSON file containing PlotParams (plot_type + plot_params).
    /// If omitted, JSON is read from stdin.
    #[arg(short, long)]
    input: Option<PathBuf>,

    /// Output file path. The extension determines the format:
    ///   .svg         -> vector (SVG)
    ///   .png         -> raster (GPU-rendered PNG)
    ///   .via_svg.png -> SVG rendered to PNG via resvg
    #[arg(short, long)]
    output: PathBuf,

    /// Canvas width in pixels.
    #[arg(long, default_value_t = 800)]
    width: u32,

    /// Canvas height in pixels.
    #[arg(long, default_value_t = 600)]
    height: u32,

    /// Device pixel ratio (e.g. 2.0 for retina displays).
    #[arg(long, default_value_t = 1.0)]
    device_pixel_ratio: f32,

    /// Aspect ratio mode: "ignore", "contain", or "cover".
    #[arg(long, default_value = "contain")]
    aspect_ratio_mode: String,

    /// View mode: "2d" or "3d".
    #[arg(long, default_value = "2d")]
    view_mode: String,

    /// Camera view as 16 comma-separated floats (4x4 column-major matrix).
    /// If omitted, no camera view override is applied.
    #[arg(long)]
    camera_view: Option<String>,

    /// Unique plot identifier (used for caching intermediate computations).
    #[arg(long, default_value = "plot-0")]
    plot_id: String,

    /// Name of the backing data store.
    #[arg(long, default_value = "default")]
    store_name: String,

    /// Left margin in pixels.
    #[arg(long)]
    margin_left: Option<f32>,

    /// Right margin in pixels.
    #[arg(long)]
    margin_right: Option<f32>,

    /// Top margin in pixels.
    #[arg(long)]
    margin_top: Option<f32>,

    /// Bottom margin in pixels.
    #[arg(long)]
    margin_bottom: Option<f32>,

    /// Font file(s) to register for SVG-->PNG rendering via resvg.
    /// Can be specified multiple times. Has no effect on GPU raster output.
    #[arg(long = "font_path")]
    font_path: Vec<PathBuf>,
}


// For the JSON representation, we want to pass an object like
// { plot_type: "LayeredPlot", plot_params: { layers: [] } }
// Which would allow alternative plot_type values in the future.

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct JsonLayeredPlotRenderParams {
    pub layers: Vec<LayerParams>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "plot_type", content = "plot_params")]
pub enum JsonPlotParams {
    // Using adjacently tagged enum representation.
    // { "plot_type": "Scatterplot" }
    // Reference: https://serde.rs/enum-representations.html

    LayeredPlot(JsonLayeredPlotRenderParams),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct JsonRenderParams {
    #[serde(flatten)]
    pub plot_params: JsonPlotParams,

    /// Zarr stores, keyed by store name, that layers can refer to via their
    /// `store_name` field. `HttpStore` and `LocalStore` entries are backed by
    /// real `zarrs_http`/`zarrs_filesystem` store instances (see
    /// `build_store_map`); `MemoryStore` is not supported here.
    pub stores: Option<HashMap<String, ZarrStoreInfo>>,
}

/// Return true when the output path ends with `.via_svg.png`.
fn is_via_svg_png(path: &PathBuf) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.ends_with(".via_svg.png"))
        .unwrap_or(false)
}

/// Infer the graphics format from the output file extension.
///
/// `.via_svg.png` uses the vector renderer; post-processing converts it to PNG.
fn infer_format(path: &PathBuf) -> GraphicsFormat {
    if is_via_svg_png(path) {
        return GraphicsFormat::Vector;
    }
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| match ext.to_ascii_lowercase().as_str() {
            "svg" => GraphicsFormat::Vector,
            _ => GraphicsFormat::Raster,
        })
        .unwrap_or(GraphicsFormat::Raster)
}

/// Parse an aspect ratio mode string.
fn parse_aspect_ratio_mode(s: &str) -> Result<AspectRatioMode, String> {
    match s.to_ascii_lowercase().as_str() {
        "ignore" | "squeeze" => Ok(AspectRatioMode::Ignore),
        "contain" | "fit" => Ok(AspectRatioMode::Contain),
        "cover" | "fill" => Ok(AspectRatioMode::Cover),
        _ => Err(format!(
            "Unknown aspect_ratio_mode '{}'. Expected: ignore, contain, or cover.",
            s
        )),
    }
}

/// Parse a view mode string.
fn parse_view_mode(s: &str) -> Result<ViewMode, String> {
    match s.to_ascii_lowercase().as_str() {
        "2d" => Ok(ViewMode::TwoD),
        "3d" => Ok(ViewMode::ThreeD),
        _ => Err(format!(
            "Unknown view_mode '{}'. Expected: 2d or 3d.",
            s
        )),
    }
}

/// Parse a comma-separated string of 16 floats into a [f32; 16] camera view matrix.
fn parse_camera_view(s: &str) -> Result<[f32; 16], String> {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() != 16 {
        return Err(format!(
            "camera_view requires exactly 16 comma-separated floats, got {}.",
            parts.len()
        ));
    }
    let mut matrix = [0.0f32; 16];
    for (i, part) in parts.iter().enumerate() {
        matrix[i] = part
            .trim()
            .parse::<f32>()
            .map_err(|e| format!("Failed to parse camera_view element {}: {}", i, e))?;
    }
    Ok(matrix)
}

/// Read the JSON string from a file or stdin.
fn read_json(input: &Option<PathBuf>) -> Result<String, io::Error> {
    match input {
        Some(path) => fs::read_to_string(path),
        None => {
            let mut buf = String::new();
            io::stdin().read_to_string(&mut buf)?;
            Ok(buf)
        }
    }
}



#[tokio::main]
async fn main() {
    let args = Args::parse();

    // --- Parse CLI parameters ---

    let format = infer_format(&args.output);

    let aspect_ratio_mode = match parse_aspect_ratio_mode(&args.aspect_ratio_mode) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Error: {}", e);
            process::exit(1);
        }
    };

    let view_mode = match parse_view_mode(&args.view_mode) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Error: {}", e);
            process::exit(1);
        }
    };

    let camera_view = match &args.camera_view {
        Some(s) => match parse_camera_view(s) {
            Ok(m) => Some(m),
            Err(e) => {
                eprintln!("Error: {}", e);
                process::exit(1);
            }
        },
        None => None,
    };

    // --- Read and parse JSON layers ---

    let json_str = match read_json(&args.input) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading input: {}", e);
            process::exit(1);
        }
    };

    let render_params: JsonRenderParams = match serde_json::from_str(&json_str) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error parsing JSON into layers: {}", e);
            process::exit(1);
        }
    };

    let stores_input = render_params.stores.clone();

    let layers = match render_params.plot_params {
        JsonPlotParams::LayeredPlot(layer_params) => layer_params.layers,
    };

    // --- Build RenderParams ---

    let params = RenderParams {
        layers,
        width: args.width,
        height: args.height,
        format,
        device_pixel_ratio: args.device_pixel_ratio,
        camera_view,
        aspect_ratio_mode,
        view_mode,
        plot_id: args.plot_id,
        stores: Some(stores_input.clone().unwrap_or_else(|| {
            // No `stores` declared in the input JSON: declare a single backing
            // store under the provided name. Zarr data loading has no generic
            // source in this fallback, so a MemoryStore descriptor is used as
            // a placeholder; layers reference it by name (or fall back to it
            // as the only store).
            HashMap::from([(
                args.store_name.clone(),
                ZarrStoreInfo {
                    store_params: ZarrStoreParams::MemoryStore(MemoryStoreParams {
                        message: "pluot_cli store (zarr loading unimplemented in plain-Rust mode)"
                            .to_string(),
                    }),
                    store_extensions: None,
                },
            )])
        })),
        margin_left: args.margin_left,
        margin_right: args.margin_right,
        margin_top: args.margin_top,
        margin_bottom: args.margin_bottom,
        // The following parameters are only relevant for interactive plotting.
        timeout: None,
        cache_enabled: false,
        svg_compression_enabled: false,
        svg_include_document: true,
        pickable: false,
        ..Default::default()
    };

    let width = params.width;
    let height = params.height;
    let via_svg_png = is_via_svg_png(&args.output);
    let is_vector = params.format == GraphicsFormat::Vector;

    // Render the plot. When the input JSON declares `stores`, construct real
    // Zarr store instances for them and render via `render_with_stores`;
    // otherwise fall back to plain `render` (the placeholder MemoryStore
    // above is only used for `store_name` bookkeeping, never actually read).
    let result = match &stores_input {
        Some(stores) => render_with_stores(params, Some(build_store_map(stores))).await,
        None => render(params).await,
    };

    // Write the output.
    if via_svg_png {
        // SVG --> PNG via resvg: render with the vector backend, then rasterize.
        let svg_string = match String::from_utf8(result) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Error: SVG output is not valid UTF-8: {}", e);
                process::exit(1);
            }
        };
        let mut opt = usvg::Options::default();
        for path in &args.font_path {
            if let Err(e) = opt.fontdb_mut().load_font_file(path) {
                eprintln!("Warning: failed to load font {:?}: {}", path, e);
            }
        }
        let tree = match usvg::Tree::from_str(&svg_string, &opt) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("Error parsing SVG: {}", e);
                process::exit(1);
            }
        };
        let size = tree.size().to_int_size();
        let mut pixmap = match tiny_skia::Pixmap::new(size.width(), size.height()) {
            Some(p) => p,
            None => {
                eprintln!(
                    "Error: failed to allocate pixmap ({}x{})",
                    size.width(),
                    size.height()
                );
                process::exit(1);
            }
        };
        resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());
        match pixmap.save_png(&args.output) {
            Ok(_) => {
                eprintln!(
                    "Wrote PNG output via SVG ({}x{}) to {}",
                    size.width(),
                    size.height(),
                    args.output.display()
                );
            }
            Err(e) => {
                eprintln!("Error writing PNG output: {}", e);
                process::exit(1);
            }
        }
    } else if is_vector {
        // Vector: the render function returns a complete SVG document as UTF-8 bytes.
        match fs::write(&args.output, &result) {
            Ok(_) => {
                eprintln!(
                    "Wrote SVG output ({} bytes) to {}",
                    result.len(),
                    args.output.display()
                );
            }
            Err(e) => {
                eprintln!("Error writing SVG output: {}", e);
                process::exit(1);
            }
        }
    } else {
        // Raster: the render function returns raw RGBA pixels followed by
        // 1 extra byte (the bailed_early flag). Strip the trailing byte
        // before encoding to PNG.
        let num_extra_bytes: usize = 1;
        let pixel_data = &result[..result.len() - num_extra_bytes];

        match save_buffer_with_format(
            &args.output,
            pixel_data,
            width,
            height,
            ColorType::Rgba8,
            ImageFormat::Png,
        ) {
            Ok(_) => {
                eprintln!(
                    "Wrote PNG output ({}x{}) to {}",
                    width,
                    height,
                    args.output.display()
                );
            }
            Err(e) => {
                eprintln!("Error writing PNG output: {}", e);
                process::exit(1);
            }
        }
    }
}
