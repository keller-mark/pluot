import React, { useState, useMemo, useCallback } from 'react';
import { Pluot } from '@pluot/react';

const NOOP = () => { };

export function FlightsExample(props) {

  const [distMin, setDistMin] = useState();
  const [distMax, setDistMax] = useState();
  // Arrival delay
  const [delayMin, setDelayMin] = useState();
  const [delayMax, setDelayMax] = useState();

  // Departure time
  const [timeMin, setTimeMin] = useState();
  const [timeMax, setTimeMax] = useState();

  const selectionCriteria = useMemo(() => {
    return [
      ...(distMin || distMax ? [{
        criteria_mode: "Quantitative",
        criteria_params: {
          values_key: "/obs/DISTANCE",
          min: distMin,
          max: distMax,
        },
      }] : []),
      ...(timeMin || timeMax ? [{
        criteria_mode: "Quantitative",
        criteria_params: {
          values_key: "/obs/DEP_TIME",
          min: timeMin,
          max: timeMax,
        },
      }] : []),
      ...(delayMin || delayMax ? [{
        criteria_mode: "Quantitative",
        criteria_params: {
          values_key: "/obs/ARR_DELAY",
          min: delayMin,
          max: delayMax,
        },
      }] : []),
    ];
  }, [distMin, distMax, delayMin, delayMax, timeMin, timeMax]);


  const onBrushDelay = useCallback((brush, brushResult) => {
    const { min, max } = brushResult?.layer_results?.[0]?.info ?? {};
    if (min && max) {
      setDelayMin(parseFloat(min));
      setDelayMax(parseFloat(max));
    }
  });

  const onBrushTime = useCallback((brush, brushResult) => {
    const { min, max } = brushResult?.layer_results?.[0]?.info ?? {};
    if (min && max) {
      setTimeMin(parseFloat(min));
      setTimeMax(parseFloat(max));
    }
  });

  const onBrushDist = useCallback((brush, brushResult) => {
    const { min, max } = brushResult?.layer_results?.[0]?.info ?? {};
    if (min && max) {
      setDistMin(parseFloat(min));
      setDistMax(parseFloat(max));
    }
  });

  return (
    <>
      <p>Arrival delay (minutes):</p>
      <Pluot
        plotId={"flights-example-arr-delay"}
        format={"Vector"}
        plotType={"LayeredPlot"}
        store={"https://pub-adb3658c8ed642caa534fdc612cd1c0c.r2.dev/flights-10m.adata.zarr"}
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
                cache_data: true,
                fill_color: null,
                selection_criteria: selectionCriteria,
                filtering_criteria: [{
                  criteria_mode: "Quantitative",
                  criteria_params: {
                    values_key: "/obs/ARR_DELAY",
                    min: -50,
                    max: 200,
                  },
                }],
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
        setCameraMatrix={NOOP}

        brushDelay={0}
        maybeBrushDelay={0}
        enableBrushCreate
        enableBrushEdit
        enableBrushClear
        brushMode="RangeX"
        brushUnitsModeX="Pixels"
        persistBrush
        //onBrush={onBrushDelay}
        onBrushEnd={onBrushDelay}
        onBrushClear={() => {
          setDelayMin(null);
          setDelayMax(null);
        }}

      />
      <p>Departure time (hours):</p>
      <Pluot
        plotId={"flights-example-dep-time"}
        format={"Vector"}
        plotType={"LayeredPlot"}
        store={"https://pub-adb3658c8ed642caa534fdc612cd1c0c.r2.dev/flights-10m.adata.zarr"}
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
                cache_data: true,
                fill_color: null,
                selection_criteria: selectionCriteria,
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
        setCameraMatrix={NOOP}

        brushDelay={0}
        maybeBrushDelay={0}
        enableBrushCreate
        enableBrushEdit
        enableBrushClear
        brushMode="RangeX"
        brushUnitsModeX="Pixels"
        persistBrush
        //onBrush={onBrushTime}
        onBrushEnd={onBrushTime}
        onBrushClear={() => {
          setTimeMin(null);
          setTimeMax(null);
        }}
      />
      <p>Flight Distance (miles):</p>
      <Pluot
        plotId={"flights-example-dist"}
        format={"Vector"}
        plotType={"LayeredPlot"}
        store={"https://pub-adb3658c8ed642caa534fdc612cd1c0c.r2.dev/flights-10m.adata.zarr"}
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
                cache_data: true,
                fill_color: null,
                selection_criteria: selectionCriteria,
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
        setCameraMatrix={NOOP}

        brushDelay={0}
        maybeBrushDelay={0}
        enableBrushCreate
        enableBrushEdit
        enableBrushClear
        brushMode="RangeX"
        brushUnitsModeX="Pixels"
        persistBrush
        //onBrush={onBrushDist}
        onBrushEnd={onBrushDist}
        onBrushClear={() => {
          setDistMin(null);
          setDistMax(null);
        }}
      />
    </>
  );
}
