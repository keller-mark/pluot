import React from 'react';
import { PluotWrapper } from './PluotWrapper.jsx';

const SELECTION_CRITERIA = [
  {
    criteria_mode: "Quantitative",
    criteria_params: {
      values_key: "/obs/DISTANCE",
      min: 1000,
      max: 3000,
    },
  },
];

export function FlightsExample(props) {
  return (
    <>
      <p>Arrival delay (minutes):</p>
      <PluotWrapper
        plotId={"flights-example-arr-delay"}
        plotType={"LayeredPlot"}
        storeUrl={"https://pub-adb3658c8ed642caa534fdc612cd1c0c.r2.dev/flights-10m.adata.zarr"}
        plotParams={{
          layers: [
            {
              layer_type: "ZarrHistogramLayer",
              layer_params: {
                layer_id: "histogram_layer",
                bounds: null,
                orientation: "Vertical",
                data_key: "/obs/ARR_DELAY",
                num_bins: 30,
                cache_data: false,
                fill_color: null,
                selection_criteria: SELECTION_CRITERIA,
              }
            }
          ]
        }}
        viewMode={"2d"}
        marginLeft={60}
        marginBottom={100}
        marginTop={10}
        marginRight={10}
        width={700}
        height={250}
        cameraMatrix={[
          1, 0, 0, 0,
          0, 3.059022901652497e-7, 0, 0,
          0.0, 0.0, 1.0, 0.0,
          0.0, -1.0, 0.0, 1.0,
        ]}
        showControls={false}
      />
      <p>Departure time (hours):</p>
      <PluotWrapper
        plotId={"flights-example-dep-time"}
        plotType={"LayeredPlot"}
        storeUrl={"https://pub-adb3658c8ed642caa534fdc612cd1c0c.r2.dev/flights-10m.adata.zarr"}
        plotParams={{
          layers: [
            {
              layer_type: "ZarrHistogramLayer",
              layer_params: {
                layer_id: "histogram_layer",
                bounds: null,
                orientation: "Vertical",
                data_key: "/obs/DEP_TIME",
                num_bins: 30,
                cache_data: false,
                fill_color: null,
                selection_criteria: SELECTION_CRITERIA,
              }
            }
          ]
        }}
        viewMode={"2d"}
        marginLeft={60}
        marginBottom={100}
        marginTop={10}
        marginRight={10}
        width={700}
        height={250}
        cameraMatrix={[
          1, 0, 0, 0,
          0, 0.0000016985858337648096, 0, 0,
          0.0, 0.0, 1.0, 0.0,
          0.0, -1.0, 0.0, 1.0,
        ]}
        showControls={false}
      />
      <p>Flight Distance (miles):</p>
      <PluotWrapper
        plotId={"flights-example-dist"}
        plotType={"LayeredPlot"}
        storeUrl={"https://pub-adb3658c8ed642caa534fdc612cd1c0c.r2.dev/flights-10m.adata.zarr"}
        plotParams={{
          layers: [
            {
              layer_type: "ZarrHistogramLayer",
              layer_params: {
                layer_id: "histogram_layer",
                bounds: null,
                orientation: "Vertical",
                data_key: "/obs/DISTANCE",
                num_bins: 30,
                cache_data: false,
                fill_color: null,
                selection_criteria: SELECTION_CRITERIA,
              }
            }
          ]
        }}
        viewMode={"2d"}
        marginLeft={60}
        marginBottom={100}
        marginTop={10}
        marginRight={10}
        width={700}
        height={250}
        cameraMatrix={[
          1, 0, 0, 0,
          0, 3.059022901652497e-7, 0, 0,
          0.0, 0.0, 1.0, 0.0,
          0.0, -1.0, 0.0, 1.0,
        ]}
        showControls={false}
      />
    </>
  );
}
