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
#' @param store_name Name of a Zarr store previously registered via
#'   [pluot_register_store()]. Its metadata is derived and passed as a
#'   single-entry `stores` map (default `NULL`). Ignored when `stores` is given.
#' @param stores Optional named list mapping store names to either a pizzarr
#'   store instance or an already-derived `ZarrStoreInfo` metadata list. Store
#'   instances are registered automatically and their metadata derived. Layers
#'   reference a store by `store_name` (or fall back to it when it is the only
#'   store). (default `NULL`).
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
#' @useDynLib pluotr wrap__render_r
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
  store_name = NULL,
  stores = NULL,
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
  # Build the top-level `stores` metadata map (store name -> ZarrStoreInfo),
  # registering any store instances so the bound functions can reach them.
  stores_meta <- .pluot_build_stores(stores = stores, store_name = store_name)

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
    stores = stores_meta,
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
  .Call("wrap__render_r", as.character(json_str))
}

#' @export
render_to_raster <- function(
  layers,
  width,
  height,
  ...
) {
    raw_bytes <- pluot_render(layers=layers, width=width, height=height, format = "Raster", ...)
    pixel_bytes <- raw_bytes[-length(raw_bytes)]          # drop status byte
    vals <- as.integer(pixel_bytes)
    arr  <- array(vals, dim = c(4L, width, height))       # [channel, x, y]
    img  <- as.raster(aperm(arr, c(3L, 2L, 1L)), max = 255L)  # --> [y, x, channel]
    return(img)
}

#' @export
render_to_svg <- function(
    layers,
    width,
    height,
    ...
) {
    raw_bytes <- pluot_render(layers=layers, width=width, height=height, format = "Vector", ...)
    return(rawToChar(raw_bytes))
}

#' @export
display_raster <- function(
    layers,
    width,
    height,
    ...
) {
    raster_obj <- render_to_raster(layers=layers, width=width, height=height, ...)
    plot.new()
    plot.window(xlim = c(0, width), ylim = c(0, height), asp = 1)
    rasterImage(raster_obj, 0, 0, width, height)
}

#' @export
display_svg <- function(
    layers,
    width,
    height,
    ...
) {
    svg_str <- render_to_svg(layers=layers, width=width, height=height, ...)
    htmltools::browsable(htmltools::HTML(svg_str))
}
