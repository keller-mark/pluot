import React, { useCallback } from 'react';
import { PluotWrapper } from './PluotWrapper.jsx';
import { useColormapOption, resolvePlotParams } from './PlotControls.jsx';

// The purpose of this wrapper component is to define a violin-matrix-specific
// onHover function to transform the picking result into a tooltip dict.

const valueFormatter = new Intl.NumberFormat('en-US', { maximumFractionDigits: 3 });

function onHoverViolinMatrix(info) {
  const cellInfo = info?.layer_results?.[0]?.info;
  if (!cellInfo) {
    // The violin matrix layer may not have rendered/prepared yet.
    return undefined;
  }
  const [q1, median, q3] = cellInfo.quartiles ?? [];
  return {
    'Cell Type': cellInfo.obs_value,
    'Gene': cellInfo.var_name,
    'Cells': valueFormatter.format(cellInfo.num_values),
    ...(cellInfo.quartiles ? {
      'Median': valueFormatter.format(median),
      'IQR': `${valueFormatter.format(q1)} – ${valueFormatter.format(q3)}`,
    } : {}),
  };
}

const VIOLIN_MATRIX_LAYER_TYPE = 'AdataZarrViolinMatrixLayer';

function withViolinMatrixColormap(plotParams, cmap) {
  return {
    ...plotParams,
    layers: plotParams.layers.map(layer => (layer.layer_type === VIOLIN_MATRIX_LAYER_TYPE ? {
      ...layer,
      layer_params: { ...layer.layer_params, cmap },
    } : layer)),
  };
}

export function ViolinMatrixWrapper(props) {
  const { plotParams } = props;
  const defaultColormap = plotParams?.layers
    ?.find(layer => layer.layer_type === VIOLIN_MATRIX_LAYER_TYPE)
    ?.layer_params?.cmap;
  const colormapOption = useColormapOption(defaultColormap);

  const getPlotParams = useCallback(
    controlValues => withViolinMatrixColormap(resolvePlotParams(plotParams, controlValues), controlValues.colormap),
    [plotParams],
  );

  return (
    <PluotWrapper
      {...props}
      plotParams={getPlotParams}
      plotSpecificOptions={colormapOption}
      onHover={onHoverViolinMatrix}
      enableTooltip
    />
  );
}
