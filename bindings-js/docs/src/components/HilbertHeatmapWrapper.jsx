import React, { useCallback, useMemo } from 'react';
import { PluotWrapper } from './PluotWrapper.jsx';
import { resolvePlotParams } from './PlotControls.jsx';

const HILBERT_HEATMAP_LAYER_TYPE = 'AdataZarrHilbertHeatmapLayer';
const COLOR_BY_EXPRESSION = 'Expression';

function onHoverHilbertHeatmap(info) {
  const cellInfo = info?.layer_results?.[0]?.info;
  if (!cellInfo) {
    return undefined;
  }
  return {
    'cell': cellInfo.obs_name,
    ...Object.fromEntries(cellInfo.groupby.map((column, i) => [column, cellInfo.obs_values[i] ?? 'NA'])),
    ...(cellInfo.var_name ? { [cellInfo.var_name]: cellInfo.expression?.toFixed(2) } : {}),
  };
}

function withHilbertHeatmapControls(plotParams, { colorBy }) {
  return {
    ...plotParams,
    layers: plotParams.layers.map((layer) => {
      if (layer.layer_type !== HILBERT_HEATMAP_LAYER_TYPE) {
        return layer;
      }
      const colorByExpression = colorBy === COLOR_BY_EXPRESSION;
      return {
        ...layer,
        layer_params: {
          ...layer.layer_params,
          var_name: colorByExpression ? layer.layer_params.var_name : null,
          color_by: colorByExpression ? null : colorBy,
        },
      };
    }),
  };
}

export function HilbertHeatmapWrapper(props) {
  const { plotParams } = props;
  const defaultLayerParams = plotParams?.layers
    ?.find(layer => layer.layer_type === HILBERT_HEATMAP_LAYER_TYPE)
    ?.layer_params;
  const groupby = defaultLayerParams?.groupby ?? [];

  const plotSpecificOptions = useMemo(() => ({
    colorBy: {
      value: COLOR_BY_EXPRESSION,
      options: [COLOR_BY_EXPRESSION, ...groupby],
      label: 'Color By',
    },
  }), [groupby.join(',')]);

  const getPlotParams = useCallback(
    controlValues => withHilbertHeatmapControls(resolvePlotParams(plotParams, controlValues), controlValues),
    [plotParams],
  );

  return (
    <PluotWrapper
      {...props}
      plotParams={getPlotParams}
      plotSpecificOptions={plotSpecificOptions}
      onHover={onHoverHilbertHeatmap}
      enableTooltip
    />
  );
}
