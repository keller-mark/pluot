import React, { useState, useMemo, useCallback } from 'react';
import { Pluot } from '@pluot/react';

const INITIAL_CAMERA = [
  0.000036549841752275825, 0, 0, 0,
  0, 0.000036549841752275825, 0, 0,
  0, 0, 0.004999999888241291, 0,
  -72.32178497314453, -14.90757942199707, 0, 1
];

// Affine matrix which approximates the transformation from EPSG:4326 to ESRI:102718
const MODEL_MATRIX = [
  277280.782739, 158.270457, 0, 0,
  -207.957329, 364329.680242, 0, 0,
  0, 0, 1, 0,
  21511491.787490, -14622204.589828, 0, 1
];

// Brush coordinates arrive in post-modelMatrix (world) units,
// but filtering criteria operate on the raw lon/lat columns.
function invertAffine2d(m) {
  const [a, b, , , c, d, , , , , , , tx, ty] = m;
  const det = a * d - b * c;
  return (x, y) => {
    const dx = x - tx;
    const dy = y - ty;
    return [(d * dx - c * dy) / det, (a * dy - b * dx) / det];
  };
}

const worldToLonLat = invertAffine2d(MODEL_MATRIX);

const NOOP = () => { };

const STORES = {
  "nyc-rides-2010": "https://pub-adb3658c8ed642caa534fdc612cd1c0c.r2.dev/nyc-rides-2010.adata.zarr",
};

export function TaxiExample(props) {
  const [cameraMatrix, setCameraMatrix] = useState(INITIAL_CAMERA);

  const [hourMin, setHourMin] = useState();
  const [hourMax, setHourMax] = useState();

  // To support rectangular brushing in the map region,
  // we keep track of x/y min/max values for pickup and dropoff.
  const [xMinPickup, setXMinPickup] = useState();
  const [xMaxPickup, setXMaxPickup] = useState();
  const [yMinPickup, setYMinPickup] = useState();
  const [yMaxPickup, setYMaxPickup] = useState();

  const [xMinDropoff, setXMinDropoff] = useState();
  const [xMaxDropoff, setXMaxDropoff] = useState();
  const [yMinDropoff, setYMinDropoff] = useState();
  const [yMaxDropoff, setYMaxDropoff] = useState();

  const hourCriteria = useMemo(() => {
    return [
      ...(hourMin || hourMax ? [{
        criteria_mode: "Quantitative",
        criteria_params: {
          values_key: "/obs/pickup_hour",
          min: hourMin,
          max: hourMax,
        },
      }] : []),
    ];
  }, [hourMin, hourMax]);

  const pickupCriteria = useMemo(() => {
    return [
      ...(xMinPickup && xMaxPickup && yMinPickup && yMaxPickup ? [
        {
          criteria_mode: "Quantitative",
          criteria_params: {
            values_key: "/obs/pickup_longitude",
            min: xMinPickup,
            max: xMaxPickup,
          },
        },
        {
          criteria_mode: "Quantitative",
          criteria_params: {
            values_key: "/obs/pickup_latitude",
            min: yMinPickup,
            max: yMaxPickup,
          },
        }
      ] : []),
    ];
  }, [xMinPickup, xMaxPickup, yMinPickup, yMaxPickup]);

  const dropoffCriteria = useMemo(() => {
    return [
      ...(xMinDropoff && xMaxDropoff && yMinDropoff && yMaxDropoff ? [
        {
          criteria_mode: "Quantitative",
          criteria_params: {
            values_key: "/obs/dropoff_longitude",
            min: xMinDropoff,
            max: xMaxDropoff,
          },
        },
        {
          criteria_mode: "Quantitative",
          criteria_params: {
            values_key: "/obs/dropoff_latitude",
            min: yMinDropoff,
            max: yMaxDropoff,
          },
        }
      ] : []),
    ];
  }, [xMinDropoff, xMaxDropoff, yMinDropoff, yMaxDropoff]);

  const onBrushHour = useCallback((brush, brushResult) => {
    if (brush && brush.vertices.length > 2) {
      const xVals = brush.vertices.map(obj => obj.x_data);
      const xMin = Math.min(...xVals);
      const xMax = Math.max(...xVals);
      setHourMin(xMin);
      setHourMax(xMax);
    }
  });

  const onBrushPickup = useCallback((brush, brushResult) => {
    if (brush && brush.vertices.length > 2) {
      const lonLatVertices = brush.vertices.map(obj => worldToLonLat(obj.x_data, obj.y_data));
      const xVals = lonLatVertices.map(([lon, _lat]) => lon);
      const yVals = lonLatVertices.map(([_lon, lat]) => lat);
      const xMin = Math.min(...xVals);
      const xMax = Math.max(...xVals);
      const yMin = Math.min(...yVals);
      const yMax = Math.max(...yVals);
      setXMinPickup(xMin);
      setXMaxPickup(xMax);
      setYMinPickup(yMin);
      setYMaxPickup(yMax);
    }
  });

  const onBrushDropoff = useCallback((brush, brushResult) => {
    if (brush && brush.vertices.length > 2) {
      const lonLatVertices = brush.vertices.map(obj => worldToLonLat(obj.x_data, obj.y_data));
      const xVals = lonLatVertices.map(([lon, _lat]) => lon);
      const yVals = lonLatVertices.map(([_lon, lat]) => lat);
      const xMin = Math.min(...xVals);
      const xMax = Math.max(...xVals);
      const yMin = Math.min(...yVals);
      const yMax = Math.max(...yVals);
      setXMinDropoff(xMin);
      setXMaxDropoff(xMax);
      setYMinDropoff(yMin);
      setYMaxDropoff(yMax);
    }
  });


  return (
    <div>
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
            stores={STORES}
            shouldClearCache={false}
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
                    filtering_criteria: [
                      ...hourCriteria,
                      ...dropoffCriteria,
                    ],
                    selection_criteria: [
                      ...pickupCriteria,
                    ],
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

            enableBrushCreate
            enableBrushEdit
            enableBrushClear
            brushMode="Rect"
            brushUnitsModeX="Data"
            brushUnitsModeY="Data"
            persistBrush
            //onBrush={onBrushPickup}
            onBrushEnd={onBrushPickup}
            onBrushClear={() => {
              setXMinPickup(null);
              setXMaxPickup(null);
              setYMinPickup(null);
              setYMaxPickup(null);
            }}
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
            stores={STORES}
            shouldClearCache={false}
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
                    filtering_criteria: [
                      ...hourCriteria,
                      ...pickupCriteria,
                    ],
                    selection_criteria: [
                      ...dropoffCriteria,
                    ],
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

            enableBrushCreate
            enableBrushEdit
            enableBrushClear
            brushMode="Rect"
            brushUnitsModeX="Data"
            brushUnitsModeY="Data"
            persistBrush
            //onBrush={onBrushDropoff}
            onBrushEnd={onBrushDropoff}
            onBrushClear={() => {
              setXMinDropoff(null);
              setXMaxDropoff(null);
              setYMinDropoff(null);
              setYMaxDropoff(null);
            }}
          />
        </div>
      </div>
      <div>
        <Pluot
          plotId={"taxi-example-pickup-hist"}
          format={"Vector"}
          plotType={"LayeredPlot"}
          stores={STORES}
          shouldClearCache={false}
          plotParams={{
            layers: [
              {
                layer_type: "ZarrHistogramLayer",
                layer_params: {
                  layer_id: "histogram_layer",
                  bounds: null,
                  orientation: "Vertical",
                  data_key: "/obs/pickup_hour",
                  num_bins: 30,
                  cache_data: true,
                  fill_color: null,
                  selection_criteria: [
                    ...hourCriteria,
                    ...pickupCriteria,
                    ...dropoffCriteria,
                  ],
                }
              },
              {
                layer_type: "TextLayer",
                layer_params: {
                  layer_id: "layer_text",
                  data_unit_mode_x: "Normalized",
                  data_unit_mode_y: "Normalized",
                  text_size_unit_mode: "Pixels",
                  text_size: 12.0,
                  text_align_mode: "End",
                  text_baseline_mode: "Bottom",
                  font_weight: "Normal",
                  font_style: "Normal",
                  bounds: {
                    margin_bottom: 0,
                    margin_right: 0,
                  },

                  position_x: { dtype: "Float32", values: [0.98] },
                  position_y: { dtype: "Float32", values: [0.05] },
                  text_vec: ["Pickup Hour"],
                }
              }
            ]
          }}
          viewMode={"2d"}
          marginLeft={50}
          marginBottom={50}
          marginTop={10}
          marginRight={10}
          width={600}
          height={200}
          aspectRatioMode="Ignore"
          //setCameraMatrix={NOOP}
          cameraFilter="fixXAndFixXAxisAtYZero"

          brushDelay={0}
          maybeBrushDelay={0}
          enableBrushCreate
          enableBrushEdit
          enableBrushClear
          brushMode="RangeX"
          brushUnitsModeX="Data"
          persistBrush
          //onBrush={onBrushHour}
          onBrushEnd={onBrushHour}
          onBrushClear={() => {
            setHourMin(null);
            setHourMax(null);
          }}


        />
      </div>
    </div>
  );
}
