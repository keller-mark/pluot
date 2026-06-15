library(pluotr)

# ─── Zarr callback function tests ──────────────────────────────────────────────

test_that("pluot_register_store returns invisibly", {
  g <- pizzarr::zarr_create_group(zarr_format = 3L)
  expect_invisible(pluot_register_store("cb_store_reg", g$get_store()))
})

test_that("pluot_zarr_has returns TRUE for an existing key", {
  g <- pizzarr::zarr_create_group(zarr_format = 3L)
  g$create_dataset("arr", data = array(1.0), shape = 1L, dtype = "<f8")
  pluot_register_store("cb_has_true", g$get_store())

  expect_true(pluot_zarr_has("cb_has_true", "arr/zarr.json"))
})

test_that("pluot_zarr_has returns FALSE for a missing key", {
  g <- pizzarr::zarr_create_group(zarr_format = 3L)
  pluot_register_store("cb_has_false", g$get_store())

  expect_false(pluot_zarr_has("cb_has_false", "missing"))
})

test_that("pluot_zarr_get_status returns 1 (Fulfilled) for an existing key", {
  g <- pizzarr::zarr_create_group(zarr_format = 3L)
  g$create_dataset("arr", data = array(1.0), shape = 1L, dtype = "<f8")
  pluot_register_store("cb_get_status", g$get_store())

  expect_equal(pluot_zarr_get_status("cb_get_status", "arr/zarr.json"), 1L)
})

test_that("pluot_zarr_has_status returns 1 (Fulfilled) for an existing key", {
  g <- pizzarr::zarr_create_group(zarr_format = 3L)
  g$create_dataset("arr", data = array(1.0), shape = 1L, dtype = "<f8")
  pluot_register_store("cb_has_status", g$get_store())

  expect_equal(pluot_zarr_has_status("cb_has_status", "arr/zarr.json"), 1L)
})

# ─── Full ZarrPointLayer render test ──────────────────────────────────────────
# Creates a zarr-v3 store in memory via pizzarr, then renders via ZarrPointLayer
# and verifies the output.

test_that("ZarrPointLayer renders correctly from a MemoryStore", {
  g <- pizzarr::zarr_create_group(zarr_format = 3L)

  # Four points at the corners of the [0,1]x[0,1] data space.
  x_vals <- c(0.0, 1.0, 0.0, 1.0)
  y_vals <- c(0.0, 0.0, 1.0, 1.0)
  l_vals <- c(0L, 1L, 2L, 3L)

  g$create_dataset("x",      data = array(x_vals), shape = length(x_vals), dtype = "<f8")
  g$create_dataset("y",      data = array(y_vals), shape = length(y_vals), dtype = "<f8")
  g$create_dataset("labels", data = array(l_vals), shape = length(l_vals), dtype = "<i8")

  pluot_register_store("zarr_render_store", g$get_store())

  svg_str <- render_to_svg(
    layers = list(
      list(
        layer_type = "ZarrPointLayer",
        layer_params = list(
          layer_id                 = "scatter",
          data_unit_mode_x         = "Data",
          data_unit_mode_y         = "Data",
          point_radius_unit_mode_x = "Pixels",
          point_radius_unit_mode_y = "Pixels",
          point_shape_mode         = "Square",
          point_radius             = 25.0,
          bounds                   = NULL,
          store_name               = NULL,
          x_key                    = "/x",
          y_key                    = "/y",
          color_key                = "/labels"
        )
      )
    ),
    width               = 100L,
    height              = 100L,
    camera_view         = c(1,0,0,0, 0,1,0,0, 0,0,1,0, 0,0,0,1),
    plot_id             = "zarr_test",
    store_name          = "zarr_render_store",
    wait_for_store_gets = TRUE,
    cache_enabled       = FALSE
  )

  expect_true(startsWith(svg_str, "<"))
  expect_equal(nchar(svg_str), 623)
})
