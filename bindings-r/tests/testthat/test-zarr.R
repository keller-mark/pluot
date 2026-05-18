library(pluotr)

# ─── Helpers to populate a pizzarr MemoryStore with zarr-v3 arrays ─────────────

.zarr_v3_meta <- function(n, data_type, fill_value = 0) {
  jsonlite::toJSON(list(
    zarr_format = 3L,
    node_type = "array",
    shape = list(n),
    data_type = data_type,
    chunk_grid = list(
      name = "regular",
      configuration = list(chunk_shape = list(n))
    ),
    chunk_key_encoding = list(
      name = "default",
      configuration = list(separator = "/")
    ),
    fill_value = fill_value,
    codecs = list(
      list(name = "bytes", configuration = list(endian = "little"))
    ),
    attributes = setNames(list(), character(0)),
    storage_transformers = list()
  ), auto_unbox = TRUE, null = "null")
}

.float64_bytes <- function(values) {
  con <- rawConnection(raw(0), "wb")
  writeBin(as.double(values), con, size = 8, endian = "little")
  result <- rawConnectionValue(con)
  close(con)
  result
}

.int64_bytes <- function(values) {
  # R integers are 32-bit; write low 32 bits then 4 zero bytes for each value.
  # Correct for non-negative integers < 2^31.
  con <- rawConnection(raw(0), "wb")
  for (v in as.integer(values)) {
    writeBin(v, con, size = 4, endian = "little")
    writeBin(0L, con, size = 4, endian = "little")
  }
  result <- rawConnectionValue(con)
  close(con)
  result
}

.add_zarr_array <- function(store, path, n, data_type, chunk_bytes) {
  arr_path <- sub("^/+", "", path)
  store$set_item(paste0(arr_path, "/zarr.json"), charToRaw(.zarr_v3_meta(n, data_type)))
  store$set_item(paste0(arr_path, "/c/0"),       chunk_bytes)
}

# ─── Zarr callback function tests ──────────────────────────────────────────────

test_that("pluot_register_store returns invisibly", {
  store <- pizzarr::MemoryStore$new()
  expect_invisible(pluot_register_store("cb_store_reg", store))
})

test_that("pluot_zarr_has returns TRUE for an existing key", {
  store <- pizzarr::MemoryStore$new()
  store$set_item("my_key", charToRaw("hello"))
  pluot_register_store("cb_has_true", store)

  expect_true(pluot_zarr_has("cb_has_true", "my_key"))
})

test_that("pluot_zarr_has returns FALSE for a missing key", {
  store <- pizzarr::MemoryStore$new()
  pluot_register_store("cb_has_false", store)

  expect_false(pluot_zarr_has("cb_has_false", "missing"))
})

test_that("pluot_zarr_get returns the correct bytes", {
  store <- pizzarr::MemoryStore$new()
  expected <- charToRaw("hello world")
  store$set_item("greet", expected)
  pluot_register_store("cb_get_bytes", store)

  result <- pluot_zarr_get("cb_get_bytes", "greet")
  expect_equal(result, expected)
})

test_that("pluot_zarr_get_status returns 1 (Fulfilled) for an existing key", {
  store <- pizzarr::MemoryStore$new()
  store$set_item("k", charToRaw("v"))
  pluot_register_store("cb_get_status", store)

  expect_equal(pluot_zarr_get_status("cb_get_status", "k"), 1L)
})

test_that("pluot_zarr_has_status returns 1 (Fulfilled) for an existing key", {
  store <- pizzarr::MemoryStore$new()
  store$set_item("k", charToRaw("v"))
  pluot_register_store("cb_has_status", store)

  expect_equal(pluot_zarr_has_status("cb_has_status", "k"), 1L)
})

test_that("pluot_zarr_get_range_from_offset returns the correct slice", {
  store <- pizzarr::MemoryStore$new()
  store$set_item("data", as.raw(0:9))   # bytes 0x00 .. 0x09
  pluot_register_store("cb_range_off", store)

  # offset=2, length=3 → bytes at 0-based positions 2,3,4 → 0x02 0x03 0x04
  result <- pluot_zarr_get_range_from_offset("cb_range_off", "data", 2L, 3L)
  expect_equal(result, as.raw(c(0x02, 0x03, 0x04)))
})

test_that("pluot_zarr_get_range_from_end returns the correct suffix", {
  store <- pizzarr::MemoryStore$new()
  store$set_item("data", as.raw(0:9))   # bytes 0x00 .. 0x09
  pluot_register_store("cb_range_end", store)

  # suffix_length=4 → last 4 bytes → 0x06 0x07 0x08 0x09
  result <- pluot_zarr_get_range_from_end("cb_range_end", "data", 4L)
  expect_equal(result, as.raw(c(0x06, 0x07, 0x08, 0x09)))
})

# ─── Full ZarrPointLayer render test ──────────────────────────────────────────
# Creates a zarr-v3 store in memory mirroring the layout used by the Gaussian
# Quantiles dataset, then renders via ZarrPointLayer and verifies the result.

test_that("ZarrPointLayer renders correctly from a MemoryStore", {
  store <- pizzarr::MemoryStore$new()

  # Four points at the corners of the [0,1]x[0,1] data space.
  x_vals <- c(0.0, 1.0, 0.0, 1.0)
  y_vals <- c(0.0, 0.0, 1.0, 1.0)
  l_vals <- c(0L, 1L, 2L, 3L)

  .add_zarr_array(store, "/x",      4L, "float64", .float64_bytes(x_vals))
  .add_zarr_array(store, "/y",      4L, "float64", .float64_bytes(y_vals))
  .add_zarr_array(store, "/labels", 4L, "int64",   .int64_bytes(l_vals))

  pluot_register_store("zarr_render_store", store)

  result <- pluot_render(
    layers = list(
      list(
        layer_type = "ZarrPointLayer",
        layer_params = list(
          layer_id              = "scatter",
          data_unit_mode_x      = "Data",
          data_unit_mode_y      = "Data",
          point_radius_unit_mode_x = "Pixels",
          point_radius_unit_mode_y = "Pixels",
          point_shape_mode      = "Square",
          point_radius          = 25.0,
          bounds                = NULL,
          store_name            = NULL,
          x_key                 = "/x",
          y_key                 = "/y",
          color_key             = "/labels"
        )
      )
    ),
    width             = 100L,
    height            = 100L,
    camera_view       = c(1,0,0,0, 0,1,0,0, 0,0,1,0, 0,0,0,1),
    plot_id           = "zarr_test",
    store_name        = "zarr_render_store",
    wait_for_store_gets = TRUE,
    cache_enabled     = FALSE
  )

  expect_type(result, "raw")
  # RGBA × pixels + trailing status byte
  expect_length(result, 100L * 100L * 4L + 1L)
  # Verify the image is non-empty (points were actually rendered)
  pixel_bytes <- result[-length(result)]
  expect_true(sum(as.integer(pixel_bytes)) > 0L)
})
