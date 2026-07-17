library(pluotr)

# Identity camera (column-major 4x4).
camera_view <- c(
  1, 0, 0, 0,
  0, 1, 0, 0,
  0, 0, 1, 0,
  0, 0, 0, 1
)

# Four points at the corners of the [0,1]x[0,1] data space.
# Square shape, 25-px radius --> 4 filled rectangles each covering ~1/4 of the
# 100x100 canvas.  Expected pixel sum matches the Python binding test.
basic_layers <- list(
  list(
    layer_type = "PointLayer",
    layer_params = list(
      layer_id              = "scatter_layer",
      data_unit_mode_x      = "Data",
      data_unit_mode_y      = "Data",
      point_radius_unit_mode_x = "Pixels",
      point_radius_unit_mode_y = "Pixels",
      point_shape_mode      = "Square",
      point_radius          = 25.0,
      bounds                = NULL,
      position_x            = list(dtype = "Float32", values = c(0, 1, 0, 1)),
      position_y            = list(dtype = "Float32", values = c(0, 0, 1, 1)),
      labels_vec            = c(0L, 1L, 2L, 3L)
    )
  )
)

test_that("render returns a raw vector of the correct length", {
  skip_if(identical(Sys.getenv("CI"), "true"))

  result <- pluot_render(
    layers      = basic_layers,
    width       = 100L,
    height      = 100L,
    camera_view = camera_view,
    plot_id     = "test",
    store_name  = "my_store"
  )

  expect_type(result, "raw")
  # RGBA for each pixel plus one trailing status byte (matches Python binding)
  expect_length(result, 100L * 100L * 4L + 1L)
})

test_that("render produces expected pixel sum", {
  skip_if(identical(Sys.getenv("CI"), "true"))

  result <- pluot_render(
    layers      = basic_layers,
    width       = 100L,
    height      = 100L,
    camera_view = camera_view,
    plot_id     = "test",
    store_name  = "my_store"
  )

  # Drop the trailing status byte before summing pixels
  pixel_bytes <- result[-length(result)]
  expect_equal(sum(as.integer(pixel_bytes)), 9062500)
})

test_that("SVG render returns valid SVG text", {
  svg_str <- render_to_svg(
    layers      = basic_layers,
    width       = 100L,
    height      = 100L,
    camera_view = camera_view,
    plot_id     = "test",
    store_name  = "my_store",
  )

  expect_true(startsWith(svg_str, "<"))
  expect_equal(nchar(svg_str), 635)
})
