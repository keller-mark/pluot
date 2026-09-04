import React, { useState } from 'react';
import { Pluot } from '@pluot/react';

const INITIAL_CAMERA = [
  0.000027623773348750547, 0, 0, 0,
  0, 0.000027623773348750547, 0, 0,
  0, 0, 0.004999999888241291, 0,
  -54.476478576660156, -11.740765571594238, 0, 1
];

// Affine matrix which approximates the transformation from EPSG:4326 to ESRI:102718
const MODEL_MATRIX = [
  277280.782739, 158.270457, 0, 0,
  -207.957329, 364329.680242, 0, 0,
  0, 0, 1, 0,
  21511491.787490, -14622204.589828, 0, 1
];

export function TaxiExample(props) {
  const [cameraMatrix, setCameraMatrix] = useState(INITIAL_CAMERA);
  return (
    <div style={{ display: 'flex', flexDirection: 'row'}}>
      <style>{`canvas { margin-top: 0; }`}</style>
      <div style={{ marginTop: 0, border: '1px solid silver' }}>
        <Pluot
          width={300}
          height={600}
          marginLeft={0}
          marginRight={0}
          marginTop={0}
          marginBottom={0}
          plotId={"taxi-example"}
          plotType={"LayeredPlot"}
          store={"https://pub-adb3658c8ed642caa534fdc612cd1c0c.r2.dev/nyc-rides-2010.adata.zarr"}
          plotParams={{
            layers: [
              {
                layer_type: "ZarrPointLayer",
                layer_params: {
                  layer_id: "layer_1",
                  data_unit_mode_x: "Data",
                  data_unit_mode_y: "Data",
                  point_radius_unit_mode_x: "Pixels",
                  point_radius_unit_mode_y: "Pixels",
                  point_shape_mode: "Circle",
                  point_radius: null,
                  bounds: null,
                  point_opacity: null,
                  model_matrix: MODEL_MATRIX,

                  x_key: "/obs/pickup_longitude",
                  y_key: "/obs/pickup_latitude",
                  color_key: "/obs/passenger_count",
                }
              },
              {
                layer_type: "TextLayer",
                layer_params: {
                  layer_id: "layer_text",
                  data_unit_mode_x: "Normalized",
                  data_unit_mode_y: "Normalized",
                  text_size_unit_mode: "Pixels",
                  text_size: 20.0,
                  text_align_mode: "Start",
                  text_baseline_mode: "Bottom",
                  font_family: "Courier",
                  font_weight: "Normal",
                  font_style: "Normal",
                  bounds: null,

                  position_x: { dtype: "Float32", values: [0.03] },
                  position_y: { dtype: "Float32", values: [0.95] },
                  text_vec: ["Pickups"],
                }
              }
            ]
          }}
          viewMode={"2d"}
          cameraMatrix={cameraMatrix}
          setCameraMatrix={setCameraMatrix}
        />
      </div>
      <div style={{ marginTop: 0, border: '1px solid silver' }}>
        <Pluot
          width={300}
          height={600}
          marginLeft={0}
          marginRight={0}
          marginTop={0}
          marginBottom={0}
          plotId={"taxi-example-2"}
          plotType={"LayeredPlot"}
          store={"https://pub-adb3658c8ed642caa534fdc612cd1c0c.r2.dev/nyc-rides-2010.adata.zarr"}
          plotParams={{
            layers: [
              {
                layer_type: "ZarrPointLayer",
                layer_params: {
                  layer_id: "layer_1",
                  data_unit_mode_x: "Data",
                  data_unit_mode_y: "Data",
                  point_radius_unit_mode_x: "Pixels",
                  point_radius_unit_mode_y: "Pixels",
                  point_shape_mode: "Circle",
                  point_radius: null,
                  bounds: null,
                  point_opacity: null,
                  model_matrix: MODEL_MATRIX,

                  x_key: "/obs/dropoff_longitude",
                  y_key: "/obs/dropoff_latitude",
                  color_key: "/obs/passenger_count",
                }
              },
              {
                layer_type: "TextLayer",
                layer_params: {
                  layer_id: "layer_text",
                  data_unit_mode_x: "Normalized",
                  data_unit_mode_y: "Normalized",
                  text_size_unit_mode: "Pixels",
                  text_size: 20.0,
                  text_align_mode: "Start",
                  text_baseline_mode: "Bottom",
                  font_family: "Courier",
                  font_weight: "Normal",
                  font_style: "Normal",
                  bounds: null,

                  position_x: { dtype: "Float32", values: [0.03] },
                  position_y: { dtype: "Float32", values: [0.95] },
                  text_vec: ["Dropoffs"],
                }
              }
            ]
          }}
          viewMode={"2d"}
          cameraMatrix={cameraMatrix}
          setCameraMatrix={setCameraMatrix}
        />
      </div>
    </div>
  );
}
