<Pluot
  width={640}
  height={480}
  format="Raster"
  cameraMatrix={[
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
  ]}
  aspectRatioMode="Contain"
  aspectRatioAlignmentMode="Center"
  viewMode="2d"
  plotType="LayeredPlot"
  plotParams={{
    layers: [
      {
        layer_type: "PointLayer",
        layer_params: {
          layer_id: "pts",
          bounds: null,
          data_unit_mode_x: "Data",
          data_unit_mode_y: "Data",
          point_radius_unit_mode_x: "Pixels",
          point_radius_unit_mode_y: "Pixels",
          point_shape_mode: "Circle",
          model_matrix: null,
          point_radius: {
            size_mode: "UniformSize",
            size_params: 5.0
          },
          fill_color: {
            color_mode: "Categorical",
            color_params: {
              codes: {
                dtype: "Uint8",
                values: [
                  0,
                  1,
                  2,
                  3
                ]
              },
              colormap: "Tableau10"
            }
          },
          fill_opacity: null,
          stroke_width_unit_mode: "Pixels",
          stroke_color: null,
          stroke_opacity: null,
          stroke_width: null,
          position_x: {
            dtype: "Float32",
            values: [
              0.0,
              1.0,
              1.0,
              0.0
            ]
          },
          position_y: {
            dtype: "Float32",
            values: [
              0.0,
              0.0,
              1.0,
              1.0
            ]
          }
        }
      },
      {
        layer_type: "AxisLinearLayer",
        layer_params: {
          layer_id: "left_axis",
          position: "Left"
        }
      }
    ]
  }}
  plotId="plot_1"
  stores={{
    my_store: {
      store_type: "HttpStore",
      store_params: {
        url: "https://example.com/my_store.zarr",
        options: null
      },
      store_extensions: null
    }
  }}
  marginLeft={60.0}
  enablePicking={false}
/>
