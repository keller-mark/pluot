import React, { useCallback, useMemo } from 'react';
import { PluotWrapper } from './PluotWrapper.jsx';
import { resolvePlotParams } from './PlotControls.jsx';

// The purpose of this wrapper component is to define a violin-plot-specific
// onHover function to transform the picking result into a tooltip dict.

const valueFormatter = new Intl.NumberFormat('en-US', { maximumFractionDigits: 3 });

function onHoverViolin(info) {
  const violinInfo = info?.layer_results?.[0]?.info;
  if (!violinInfo) {
    // The violin layer may not have rendered/prepared yet.
    return undefined;
  }
  const [q1, median, q3] = violinInfo.quartiles ?? [];
  return {
    'Gene': violinInfo.var_name,
    'Group': violinInfo.obs_value,
    'Cells': valueFormatter.format(violinInfo.num_values),
    ...(violinInfo.quartiles ? {
      'Median': valueFormatter.format(median),
      'IQR': `${valueFormatter.format(q1)} – ${valueFormatter.format(q3)}`,
    } : {}),
  };
}

const VIOLIN_LAYER_TYPE = 'AdataZarrViolinLayer';

// The value axis domain depends on the gene's expression values (and on whether zeros are kept).
function getViolinExtentKey({ varName, keepZeros }) {
  return JSON.stringify([varName, keepZeros]);
}

function withViolinControls(plotParams, { varName, keepZeros, densityNorm }) {
  return {
    ...plotParams,
    layers: plotParams.layers.map((layer) => {
      if (layer.layer_type !== VIOLIN_LAYER_TYPE) {
        return layer;
      }
      return {
        ...layer,
        layer_params: {
          ...layer.layer_params,
          var_name: varName,
          keep_zeros: keepZeros,
          density_norm: densityNorm,
        },
      };
    }),
  };
}

export function ViolinWrapper(props) {
  const { plotParams, varNameOptions } = props;
  const defaultLayerParams = plotParams?.layers
    ?.find(layer => layer.layer_type === VIOLIN_LAYER_TYPE)
    ?.layer_params;
  const defaultVarName = defaultLayerParams?.var_name;
  const defaultKeepZeros = defaultLayerParams?.keep_zeros ?? true;
  const defaultDensityNorm = defaultLayerParams?.density_norm ?? 'Width';
  const varNames = varNameOptions ?? [defaultVarName];

  const plotSpecificOptions = useMemo(() => ({
    varName: {
      value: defaultVarName,
      options: varNames,
      label: 'Gene',
    },
    keepZeros: {
      value: defaultKeepZeros,
      label: 'Keep Zeros',
    },
    densityNorm: {
      value: defaultDensityNorm,
      options: ['Width', 'Area', 'Count'],
      label: 'Density Norm',
    },
  }), [varNames.join(','), defaultVarName, defaultKeepZeros, defaultDensityNorm]);

  const getPlotParams = useCallback(
    controlValues => withViolinControls(resolvePlotParams(plotParams, controlValues), controlValues),
    [plotParams],
  );

  return (
    <PluotWrapper
      {...props}
      plotParams={getPlotParams}
      plotSpecificOptions={plotSpecificOptions}
      onHover={onHoverViolin}
      extentKey={getViolinExtentKey}
      enableTooltip
    />
  );
}
