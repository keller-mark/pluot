render_to_raster(
  layers = list(
    list(
      layer_type = "PointLayer",
      layer_params = list(
        layer_id = "pts",
        bounds = NULL,
        data_unit_mode_x = "Data",
        data_unit_mode_y = "Data",
        point_radius_unit_mode_x = "Pixels",
        point_radius_unit_mode_y = "Pixels",
        point_shape_mode = "Circle",
        model_matrix = NULL,
        point_radius = list(
          size_mode = "UniformSize",
          size_params = 5.0
        ),
        fill_color = list(
          color_mode = "Categorical",
          color_params = list(
            codes = list(
              dtype = "Uint8",
              values = c(
                0,
                1,
                2,
                3
              )
            ),
            colormap = "Tableau10"
          )
        ),
        fill_opacity = NULL,
        stroke_width_unit_mode = "Pixels",
        stroke_color = NULL,
        stroke_opacity = NULL,
        stroke_width = NULL,
        position_x = list(
          dtype = "Float32",
          values = c(
            0.0,
            1.0,
            1.0,
            0.0
          )
        ),
        position_y = list(
          dtype = "Float32",
          values = c(
            0.0,
            0.0,
            1.0,
            1.0
          )
        )
      )
    ),
    list(
      layer_type = "AxisLinearLayer",
      layer_params = list(
        layer_id = "left_axis",
        position = "Left"
      )
    )
  ),
  width = 640,
  height = 480,
  device_pixel_ratio = 1.0,
  camera_view = c(
    0.15000000596046448,
    0.0,
    0.0,
    0.0,
    0.0,
    0.15000000596046448,
    0.0,
    0.0,
    0.0,
    0.0,
    1.0,
    0.0,
    0.0,
    0.0,
    0.0,
    1.0
  ),
  aspect_ratio_mode = "Contain",
  aspect_ratio_alignment_mode = "Center",
  view_mode = "2d",
  plot_id = "plot_1",
  stores = list(
    my_store = list(
      store_type = "HttpStore",
      store_params = list(
        url = "https://example.com/my_store.zarr",
        options = NULL
      ),
      store_extensions = NULL
    )
  ),
  wait_for_store_gets = TRUE,
  timeout = NULL,
  cache_enabled = TRUE,
  svg_compression_enabled = FALSE,
  svg_include_document = TRUE,
  margin_left = 60.0,
  margin_right = NULL,
  margin_top = NULL,
  margin_bottom = NULL,
  pickable = FALSE,
  render_backend = NULL,
  compute_backend = NULL
)
