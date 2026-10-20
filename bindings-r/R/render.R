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
#'   matrix). When `NULL`, the camera is computed to show `x_lim`/`y_lim`, with
#'   any unspecified limit filled in from the union of the layer extents.
#'   Mutually exclusive with `x_lim`/`y_lim`.
#' @param x_lim,y_lim Optional `c(min, max)` data ranges to show. Ignored for
#'   3D plots.
#' @param aspect_ratio_mode One of `"Contain"` (default), `"Cover"`, `"Fill"`.
#' @param aspect_ratio_alignment_mode One of `"Center"` (default), `"Start"`,
#'   `"End"`.
#' @param view_mode `"2d"` (default) or `"3d"`.
#' @param plot_id Identifier string used as a cache key (default `""`).
#' @param store Optional single Zarr store, as either a pizzarr store instance
#'   or an already-derived `ZarrStoreInfo` metadata list (default `NULL`). Named
#'   via `store_name`, or `"default"` if `store_name` is not given.
#' @param store_name Name for the single `store` argument, or (when `store` is
#'   not given) the name of a Zarr store previously registered via
#'   [pluot_register_store()]. Its metadata is derived and passed as a
#'   single-entry `stores` map (default `NULL`).
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
#' @useDynLib pluotr wrap__camera_view_from_lims_r
pluot_render <- function(
  layers,
  schema_version = NULL,
  width,
  height,
  format = "Raster",
  device_pixel_ratio = 1.0,
  camera_view = NULL,
  x_lim = NULL,
  y_lim = NULL,
  aspect_ratio_mode = "Contain",
  aspect_ratio_alignment_mode = "Center",
  view_mode = "2d",
  plot_id = "",
  store = NULL,
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
  if (!is.null(camera_view) && (!is.null(x_lim) || !is.null(y_lim))) {
    stop("`camera_view` is mutually exclusive with `x_lim`/`y_lim`.")
  }
  params <- .pluot_render_params(
    layers = layers,
    width = width,
    height = height,
    format = format,
    device_pixel_ratio = device_pixel_ratio,
    camera_view = camera_view,
    aspect_ratio_mode = aspect_ratio_mode,
    aspect_ratio_alignment_mode = aspect_ratio_alignment_mode,
    view_mode = view_mode,
    plot_id = plot_id,
    store = store,
    store_name = store_name,
    stores = stores,
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
  if (is.null(params$camera_view)) {
    params$camera_view <- .pluot_resolve_camera_view(params, x_lim, y_lim)
  }
  .Call("wrap__render_r", .pluot_params_to_json(params))
}

#' Compute the data extent of each layer
#'
#' Accepts the same arguments as [pluot_render()] (other than `x_lim`/`y_lim`).
#'
#' @param ... Arguments passed on to the render parameter construction; see
#'   [pluot_render()].
#' @return A list with element `layer_results`: one list per layer reporting an
#'   extent, with elements `layer_id`, `x`, `y`, and `z`. `x`/`y`/`z` are
#'   `c(min, max)` numeric vectors (`z` is `NULL` for 2D layers).
#' @export
#' @useDynLib pluotr wrap__extent_r
pluot_extent <- function(...) {
  .pluot_extent(.pluot_render_params(...))
}

.pluot_extent <- function(params) {
  json_str <- .Call("wrap__extent_r", .pluot_params_to_json(params))
  result <- jsonlite::fromJSON(json_str, simplifyVector = FALSE)
  result$layer_results <- lapply(result$layer_results, function(r) {
    r$x <- unlist(r$x)
    r$y <- unlist(r$y)
    r$z <- unlist(r$z)
    r
  })
  result
}

# The bounding box of every layer's extent, or NULL when no layer reports one.
.pluot_union_extent <- function(extent_result) {
  layer_results <- extent_result$layer_results
  if (length(layer_results) == 0) {
    return(NULL)
  }
  list(
    x_lim = range(unlist(lapply(layer_results, `[[`, "x"))),
    y_lim = range(unlist(lapply(layer_results, `[[`, "y")))
  )
}

# Returns NULL (the default camera) when the extent query is needed but no layer
# reports an extent, or for 3D plots, which the bounds-to-camera conversion does
# not yet support.
.pluot_resolve_camera_view <- function(params, x_lim, y_lim) {
  if (params$view_mode == "3d") {
    return(NULL)
  }

  if (is.null(x_lim) || is.null(y_lim)) {
    union <- .pluot_union_extent(.pluot_extent(params))
    if (is.null(union)) {
      return(NULL)
    }
    if (is.null(x_lim)) x_lim <- union$x_lim
    if (is.null(y_lim)) y_lim <- union$y_lim
  }

  .Call(
    "wrap__camera_view_from_lims_r",
    .pluot_params_to_json(params),
    as.double(x_lim),
    as.double(y_lim)
  )
}

.pluot_params_to_json <- function(params) {
  as.character(jsonlite::toJSON(params, auto_unbox = TRUE, null = "null", digits = NA))
}

.pluot_render_params <- function(
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
  store = NULL,
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
  stores_meta <- .pluot_build_stores(stores = stores, store = store, store_name = store_name)

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
    svg_bytes <- raw_bytes[-length(raw_bytes)]          # drop status byte
    return(rawToChar(svg_bytes))
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
