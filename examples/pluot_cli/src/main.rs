use clap::Parser;
use image::{save_buffer_with_format, ColorType, ImageFormat};
use pluot::{render, AspectRatioMode, GraphicsFormat, PlotParams, RenderParams, ViewMode};
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;
use std::process;

/// Pluot CLI — render plots to SVG or PNG.
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
    ///   .svg  -> vector (SVG)
    ///   .png  -> raster (PNG)
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
}

/// Infer the graphics format from the output file extension.
fn infer_format(path: &PathBuf) -> GraphicsFormat {
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

    // --- Read and parse JSON plot params ---

    let json_str = match read_json(&args.input) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading input: {}", e);
            process::exit(1);
        }
    };

    let plot_params: PlotParams = match serde_json::from_str(&json_str) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error parsing JSON into PlotParams: {}", e);
            process::exit(1);
        }
    };

    // --- Build RenderParams ---

    let params = RenderParams {
        width: args.width,
        height: args.height,
        format,
        device_pixel_ratio: args.device_pixel_ratio,
        camera_view,
        aspect_ratio_mode,
        view_mode,
        plot_params,
        plot_id: args.plot_id,
        store_name: args.store_name,
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
    let is_vector = params.format == GraphicsFormat::Vector;

    // Render the plot.
    let result = render(params).await;

    // Write the output.
    if is_vector {
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
