use clap::Parser;
use serde::{Deserialize, Serialize};
use image::{save_buffer_with_format, ColorType, ImageFormat};
use std::collections::HashMap;
use std::sync::Arc;

use pluot::{
    render, render_to_script, render_with_stores, AspectRatioMode, GraphicsFormat, LayerParams,
    RenderParams, ViewMode, ZarrStoreInfo, ZarrStoreParams, HttpStoreParams, LocalStoreParams,
    MemoryStoreParams, StoreMap,
};
use zarrs_storage::storage_adapter::async_to_sync::{
    AsyncToSyncBlockOn, AsyncToSyncStorageAdapter,
};
use zarrs_storage::storage_adapter::sync_to_async::{
    SyncToAsyncSpawnBlocking, SyncToAsyncStorageAdapter
};
use zarrs_storage::AsyncReadableStorageTraits;
use resvg::usvg;
use tiny_skia;
use std::fs;
use std::io::{self, Cursor, Read};
use std::path::PathBuf;
use std::process;
use image::ImageReader;
use stega::{decode as stega_decode, encode as stega_encode, Carrier, Payload};

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

struct TokioBlockOn(tokio::runtime::Runtime);

impl AsyncToSyncBlockOn for TokioBlockOn {
    fn block_on<F: core::future::Future>(&self, future: F) -> F::Output {
        self.0.block_on(future)
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

    /// Output file path. Required unless --decode is used. The extension
    /// determines the format:
    ///   .svg         -> vector (SVG)
    ///   .png         -> raster (GPU-rendered PNG)
    ///   .via_svg.png -> SVG rendered to PNG via resvg
    #[arg(short, long, required_unless_present = "decode")]
    output: Option<PathBuf>,

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

    /// Embed the input RenderParams JSON into the rendered SVG or PNG output,
    /// analogous to vendor/stega-lite: a hidden `<desc>` element for SVG, or
    /// LSB steganography (via the `stega` crate) for PNG. Has no effect on
    /// .py/.R script output. Transparency is preserved for PNG output.
    #[arg(long)]
    embed_params: bool,

    /// Given the path to an SVG or PNG file produced with `--embed-params`
    /// (as the value of this --decode option), decode the embedded
    /// RenderParams JSON representation, print it to stdout, and exit.
    /// When a path to a file to decode is provided,
    /// all other rendering flags (--input, --output, etc.) are ignored.
    #[arg(long)]
    decode: Option<PathBuf>,
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

/// If `--output` names a `.py` or `.R` file (case-insensitive), return the
/// matching code-generation target instead of a real render format. Used to
/// let `pluot_cli` double as a code-gen step for the Python/R
/// render-to-script integration tests (see `scripts/gen_render_script_fixtures.sh`).
fn script_format_for_output(path: &PathBuf) -> Option<GraphicsFormat> {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("py") => Some(GraphicsFormat::ScriptPython),
        Some("r") => Some(GraphicsFormat::ScriptR),
        _ => None,
    }
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

// --- RenderParams JSON embedding/decoding (see vendor/stega-lite, which does
// the analogous thing for Vega-Lite specs) ---

/// Infer whether `--decode` names an SVG or PNG file. Unlike `infer_format`,
/// this only distinguishes the two container formats a `--decode` input can
/// actually be, not the pluot graphics-format enum.
fn decode_format_for_path(path: &PathBuf) -> Result<&'static str, String> {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("svg") => Ok("svg"),
        Some("png") => Ok("png"),
        Some(other) => Err(format!(
            "Unsupported file extension for --decode: '.{other}'. Use .svg or .png."
        )),
        None => Err(format!(
            "Cannot infer format for --decode: '{}' has no file extension.",
            path.display()
        )),
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn html_unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
}

/// Embed the RenderParams JSON into the SVG as a hidden `<desc>` element,
/// inserted right after the opening `<svg ...>` tag.
fn embed_json_in_svg(svg: &str, params_json: &str) -> String {
    if let Some(pos) = svg.find('>') {
        let (before, after) = svg.split_at(pos + 1);
        format!(
            "{before}<desc class=\"pluot-params\">{}</desc>{after}",
            html_escape(params_json)
        )
    } else {
        svg.to_string()
    }
}

/// Extract the RenderParams JSON previously embedded by `embed_json_in_svg`.
fn extract_json_from_svg(svg: &str) -> Option<String> {
    let start_tag = "<desc class=\"pluot-params\">";
    let end_tag = "</desc>";
    let start = svg.find(start_tag)? + start_tag.len();
    let end = svg[start..].find(end_tag)? + start;
    Some(html_unescape(&svg[start..end]))
}

/// Encode raw RGBA8 pixel data (as produced by the raster render path) into
/// an in-memory PNG file, without writing to disk.
fn png_bytes_from_rgba(pixel_data: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    let mut out_bytes: Vec<u8> = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut out_bytes);
    image::ImageEncoder::write_image(
        encoder,
        pixel_data,
        width,
        height,
        image::ExtendedColorType::Rgba8,
    )
    .map_err(|e| format!("Failed to encode PNG: {e}"))?;
    Ok(out_bytes)
}

/// Embed the RenderParams JSON into a PNG file's pixels using LSB
/// steganography via the `stega` crate. The carrier only supports RGB
/// images, so the alpha channel is split off before encoding and recombined
/// with the (possibly LSB-perturbed) RGB channels afterwards, rather than
/// dropped: many renders are transparent outside the plotted area, and
/// those pixels' underlying RGB is typically (0, 0, 0), so discarding alpha
/// would turn them solid black instead of transparent.
fn embed_json_in_png(png_bytes: &[u8], params_json: &str) -> Result<Vec<u8>, String> {
    let img = ImageReader::new(Cursor::new(png_bytes))
        .with_guessed_format()
        .map_err(|e| format!("Failed to guess PNG format: {e}"))?
        .decode()
        .map_err(|e| format!("Failed to decode PNG: {e}"))?;
    let rgba_image = img.to_rgba8();
    let (width, height) = (rgba_image.width(), rgba_image.height());

    let mut alpha = Vec::with_capacity((width * height) as usize);
    let mut rgb_raw = Vec::with_capacity((width * height * 3) as usize);
    for pixel in rgba_image.into_raw().chunks_exact(4) {
        rgb_raw.extend_from_slice(&pixel[0..3]);
        alpha.push(pixel[3]);
    }
    let rgb_image = image::RgbImage::from_raw(width, height, rgb_raw)
        .ok_or_else(|| "Failed to build RGB carrier image".to_string())?;

    let mut carrier = Carrier::new(rgb_image)
        .map_err(|e| format!("Failed to create steganography carrier: {e:?}"))?;

    let payload = Payload::new(params_json);
    stega_encode(&payload, &mut carrier)
        .map_err(|e| format!("RenderParams JSON too large for image capacity: {e:?}"))?;

    let result_image = carrier.unwrap();
    let mut rgba_out = Vec::with_capacity((width * height * 4) as usize);
    for (rgb, a) in result_image.into_raw().chunks_exact(3).zip(alpha.iter()) {
        rgba_out.extend_from_slice(rgb);
        rgba_out.push(*a);
    }

    let mut out_bytes: Vec<u8> = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut out_bytes);
    image::ImageEncoder::write_image(
        encoder,
        &rgba_out,
        width,
        height,
        image::ExtendedColorType::Rgba8,
    )
    .map_err(|e| format!("Failed to encode PNG: {e}"))?;

    Ok(out_bytes)
}

/// Extract the RenderParams JSON previously embedded by `embed_json_in_png`.
fn extract_json_from_png(png_bytes: &[u8]) -> Result<String, String> {
    let img = ImageReader::new(Cursor::new(png_bytes))
        .with_guessed_format()
        .map_err(|e| format!("Failed to guess PNG format: {e}"))?
        .decode()
        .map_err(|e| format!("Failed to decode PNG: {e}"))?;
    let rgb_image = img.to_rgb8();

    let carrier = Carrier::new(rgb_image)
        .map_err(|e| format!("Failed to create steganography carrier: {e:?}"))?;

    stega_decode(&carrier).map_err(|e| format!("Failed to decode hidden data from PNG: {e:?}"))
}



#[tokio::main]
async fn main() {
    let args = Args::parse();

    // --- Decode shortcut ---
    //
    // `--decode <path>` extracts and prints RenderParams JSON previously
    // embedded (via `--embed-params`) in a graphics file, instead of
    // rendering. All other flags are ignored.
    if let Some(decode_path) = &args.decode {
        let decode_format = match decode_format_for_path(decode_path) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Error: {}", e);
                process::exit(1);
            }
        };
        let params_json = if decode_format == "svg" {
            let svg = fs::read_to_string(decode_path).unwrap_or_else(|e| {
                eprintln!("Error reading {}: {}", decode_path.display(), e);
                process::exit(1);
            });
            extract_json_from_svg(&svg).unwrap_or_else(|| {
                eprintln!(
                    "Error: no embedded RenderParams JSON found in {}",
                    decode_path.display()
                );
                process::exit(1);
            })
        } else {
            let png_bytes = fs::read(decode_path).unwrap_or_else(|e| {
                eprintln!("Error reading {}: {}", decode_path.display(), e);
                process::exit(1);
            });
            extract_json_from_png(&png_bytes).unwrap_or_else(|e| {
                eprintln!("Error: {}", e);
                process::exit(1);
            })
        };
        println!("{params_json}");
        return;
    }

    // `--output` is required by clap (`required_unless_present = "decode"`)
    // whenever `--decode` is absent, so this is always present here.
    let output = args.output.clone().unwrap();

    // --- Parse CLI parameters ---

    let format = infer_format(&output);

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

    // --- Code-generation shortcut ---
    //
    // `--output foo.py` / `foo.R` skip real rendering entirely and instead
    // emit the equivalent Python/R source via `render_to_script`, for use as
    // an integration-test fixture (see `scripts/gen_render_script_fixtures.sh`).
    if let Some(script_format) = script_format_for_output(&output) {
        let script_params = RenderParams {
            format: GraphicsFormat::Vector,
            // Passed through verbatim (`None` if the input JSON declared no
            // stores) rather than the synthetic MemoryStore placeholder above,
            // which is a `pluot_cli`-only bookkeeping detail that has no
            // business leaking into generated source.
            stores: stores_input.clone(),
            ..params.clone()
        };
        let script = render_to_script(script_params, &script_format);
        match fs::write(&output, &script) {
            Ok(_) => {
                eprintln!(
                    "Wrote {script_format:?} script ({} bytes) to {}",
                    script.len(),
                    output.display()
                );
            }
            Err(e) => {
                eprintln!("Error writing script output: {}", e);
                process::exit(1);
            }
        }
        return;
    }

    let width = params.width;
    let height = params.height;
    let via_svg_png = is_via_svg_png(&output);
    let is_vector = params.format == GraphicsFormat::Vector;

    // Render the plot. When the input JSON declares `stores`, construct real
    // Zarr store instances for them and render via `render_with_stores`;
    // otherwise fall back to plain `render` (the placeholder MemoryStore
    // above is only used for `store_name` bookkeeping, never actually read).
    let result = match &stores_input {
        Some(stores) => {
            // `build_store_map` constructs a `reqwest::blocking::Client` (via
            // `zarrs_http::HTTPStore::new`), which spins up its own private
            // Tokio runtime internally. Doing that directly on a worker thread
            // of the outer `#[tokio::main]` runtime panics ("Cannot drop a
            // runtime in a context where blocking is not allowed"), since that
            // worker thread disallows blocking. Running it via `spawn_blocking`
            // moves it onto Tokio's blocking thread pool, where blocking is
            // permitted.
            let stores = stores.clone();
            let store_map = tokio::task::spawn_blocking(move || build_store_map(&stores))
                .await
                .unwrap();
            render_with_stores(params, Some(store_map)).await
        }
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
        let png_bytes = match pixmap.encode_png() {
            Ok(b) => b,
            Err(e) => {
                eprintln!("Error encoding PNG: {}", e);
                process::exit(1);
            }
        };
        let final_bytes = if args.embed_params {
            embed_json_in_png(&png_bytes, &json_str).unwrap_or_else(|e| {
                eprintln!("Error embedding RenderParams JSON: {}", e);
                process::exit(1);
            })
        } else {
            png_bytes
        };
        match fs::write(&output, &final_bytes) {
            Ok(_) => {
                eprintln!(
                    "Wrote PNG output via SVG ({}x{}) to {}",
                    size.width(),
                    size.height(),
                    output.display()
                );
            }
            Err(e) => {
                eprintln!("Error writing PNG output: {}", e);
                process::exit(1);
            }
        }
    } else if is_vector {
        // Vector: the render function returns a complete SVG document as UTF-8 bytes.
        let svg_bytes: Vec<u8> = if args.embed_params {
            let svg_string = match std::str::from_utf8(&result) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Error: SVG output is not valid UTF-8: {}", e);
                    process::exit(1);
                }
            };
            embed_json_in_svg(svg_string, &json_str).into_bytes()
        } else {
            result
        };
        match fs::write(&output, &svg_bytes) {
            Ok(_) => {
                eprintln!(
                    "Wrote SVG output ({} bytes) to {}",
                    svg_bytes.len(),
                    output.display()
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

        if args.embed_params {
            let png_bytes = png_bytes_from_rgba(pixel_data, width, height).unwrap_or_else(|e| {
                eprintln!("Error encoding PNG: {}", e);
                process::exit(1);
            });
            let final_bytes = embed_json_in_png(&png_bytes, &json_str).unwrap_or_else(|e| {
                eprintln!("Error embedding RenderParams JSON: {}", e);
                process::exit(1);
            });
            match fs::write(&output, &final_bytes) {
                Ok(_) => {
                    eprintln!(
                        "Wrote PNG output ({}x{}) to {}",
                        width,
                        height,
                        output.display()
                    );
                }
                Err(e) => {
                    eprintln!("Error writing PNG output: {}", e);
                    process::exit(1);
                }
            }
        } else {
            match save_buffer_with_format(
                &output,
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
                        output.display()
                    );
                }
                Err(e) => {
                    eprintln!("Error writing PNG output: {}", e);
                    process::exit(1);
                }
            }
        }
    }
}
