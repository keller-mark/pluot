#' Render a layered plot to raw bytes
#'
#' Calls the Rust pluot rendering engine and returns the result as a raw vector.
#' For raster output (default), the bytes are RGBA pixels (width * height * 4
#' bytes, plus one trailing status byte). For vector output, the bytes are a
#' UTF-8 encoded SVG string.
#'
#' Each layer in `layers` must be a named list with at minimum:
#' \describe{
#'   \item{layer_type}{A string matching a registered layer name, e.g.
#'     `"PointLayer"`, `"BarPlotLayer"`, `"OmeZarrBitmapLayer"`, etc.}
#'   \item{layer_params}{A named list of layer-specific parameters.}
#' }
#'
#' @param layers A list of layer definitions.
#' @param width  Output width in pixels (integer).
#' @param height Output height in pixels (integer).
#' @param format Graphics format: `"Raster"` (default) or `"Vector"` (SVG).
#' @param device_pixel_ratio Device pixel ratio (default 1.0).
#' @param camera_view Optional 16-element numeric vector (column-major 4x4
#'   matrix). `NULL` uses the default camera.
#' @param aspect_ratio_mode One of `"Contain"` (default), `"Cover"`, `"Fill"`.
#' @param aspect_ratio_alignment_mode One of `"Center"` (default), `"Start"`,
#'   `"End"`.
#' @param view_mode `"2d"` (default) or `"3d"`.
#' @param plot_id Identifier string used as a cache key (default `""`).
#' @param store_name Name of a registered Zarr store (default `""`).
#' @param wait_for_store_gets Wait for in-flight store requests (default `TRUE`).
#' @param timeout Optional render timeout in milliseconds. `NULL` means no
#'   timeout.
#' @param cache_enabled Enable render cache (default `TRUE`).
#' @param svg_compression_enabled Compress SVG output (default `FALSE`).
#' @param svg_include_document Wrap SVG in an XML document header (default
#'   `TRUE`).
#' @param margin_left,margin_right,margin_top,margin_bottom Optional margins in
#'   pixels. `NULL` means no margin.
#' @param pickable Enable picking (default `FALSE`).
#' @param render_backend Optional render backend string. `NULL` uses the
#'   default.
#' @param compute_backend Optional compute backend string. `NULL` uses the
#'   default.
#'
#' @return A raw vector of bytes.
#' @export
#' @useDynLib pluotr render_wrapper
pluot_render <- function(
  layers,
  width,
  height,
  format = "Raster",
  device_pixel_ratio = 1.0,
  camera_view = NULL,
  aspect_ratio_mode = "Contain",
  aspect_ratio_alignment_mode = "Center",
  view_mode = "2d",
  plot_id = "",
  store_name = "",
  wait_for_store_gets = TRUE,
  timeout = NULL,
  cache_enabled = TRUE,
  svg_compression_enabled = FALSE,
  svg_include_document = TRUE,
  margin_left = NULL,
  margin_right = NULL,
  margin_top = NULL,
  margin_bottom = NULL,
  pickable = FALSE,
  render_backend = NULL,
  compute_backend = NULL
) {
  params <- list(
    layers = layers,
    width = as.integer(width),
    height = as.integer(height),
    format = format,
    device_pixel_ratio = as.double(device_pixel_ratio),
    camera_view = camera_view,
    aspect_ratio_mode = aspect_ratio_mode,
    aspect_ratio_alignment_mode = aspect_ratio_alignment_mode,
    view_mode = view_mode,
    plot_id = plot_id,
    store_name = store_name,
    wait_for_store_gets = wait_for_store_gets,
    timeout = timeout,
    cache_enabled = cache_enabled,
    svg_compression_enabled = svg_compression_enabled,
    svg_include_document = svg_include_document,
    margin_left = margin_left,
    margin_right = margin_right,
    margin_top = margin_top,
    margin_bottom = margin_bottom,
    pickable = pickable,
    render_backend = render_backend,
    compute_backend = compute_backend
  )
  json_str <- jsonlite::toJSON(params, auto_unbox = TRUE, null = "null")
  .Call(render_wrapper, as.character(json_str))
}
