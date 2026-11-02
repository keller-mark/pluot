import React, { useCallback, useMemo } from 'react';
import { PluotWrapper } from './PluotWrapper.jsx';
import { resolvePlotParams } from './PlotControls.jsx';

const HILBERT_HEATMAP_LAYER_TYPE = 'AdataZarrHilbertHeatmapLayer';
const COLOR_BY_EXPRESSION = 'Expression';
const FADE_LEVELS = [
  { opacity: 0.2, defaultPercentile: 0, control: 'percentileForOpacity20' },
  { opacity: 0.6, defaultPercentile: 75, control: 'percentileForOpacity60' },
  { opacity: 1.0, defaultPercentile: 90, control: 'percentileForOpacity100' },
];

function onHoverHilbertHeatmap(info) {
  const cellInfo = info?.layer_results?.[0]?.info;
  if (!cellInfo) {
    return undefined;
  }
  return {
    'cell': cellInfo.obs_name,
    ...Object.fromEntries(cellInfo.groupby.map((columnPath, i) => [columnPath.split('/').pop(), cellInfo.obs_values[i] ?? 'NA'])),
    ...(cellInfo.var_name ? { [cellInfo.var_name]: cellInfo.expression?.toFixed(2) } : {}),
  };
}

function withHilbertHeatmapControls(plotParams, controlValues) {
  const { varName, colorBy, fadeByExpression } = controlValues;
  return {
    ...plotParams,
    layers: plotParams.layers.map((layer) => {
      if (layer.layer_type !== HILBERT_HEATMAP_LAYER_TYPE) {
        return layer;
      }
      const colorByExpression = colorBy === COLOR_BY_EXPRESSION;
      const fade = !colorByExpression && fadeByExpression;
      return {
        ...layer,
        layer_params: {
          ...layer.layer_params,
          var_name: colorByExpression || fade ? varName : null,
          color_by: colorByExpression ? null : colorBy,
          var_percentile_opacities: fade
            ? FADE_LEVELS.map(({ opacity, control }) => ({ percentile: controlValues[control], opacity }))
            : null,
        },
      };
    }),
  };
}

export function HilbertHeatmapWrapper(props) {
  const { plotParams, varNameOptions } = props;
  const defaultLayerParams = plotParams?.layers
    ?.find(layer => layer.layer_type === HILBERT_HEATMAP_LAYER_TYPE)
    ?.layer_params;
  const groupby = defaultLayerParams?.groupby ?? [];
  const defaultVarName = defaultLayerParams?.var_name;
  const varNames = varNameOptions ?? [defaultVarName];

  const plotSpecificOptions = useMemo(() => {
    const isColoredByCategory = get => get('Plot-Specific Options.colorBy') !== COLOR_BY_EXPRESSION;
    return {
      ...(defaultVarName ? {
        varName: {
          value: defaultVarName,
          options: varNames,
          label: 'Gene',
        },
      } : {}),
      colorBy: {
        value: COLOR_BY_EXPRESSION,
        options: [COLOR_BY_EXPRESSION, ...groupby],
        label: 'Color By',
      },
      ...(defaultVarName ? {
        fadeByExpression: {
          value: false,
          label: 'Fade By Gene',
          hint: "Set each cell's opacity by the percentile of its expression of the selected gene among the plotted cells.",
          render: isColoredByCategory,
        },
        ...Object.fromEntries(FADE_LEVELS.map(({ opacity, defaultPercentile, control }) => [control, {
          value: defaultPercentile,
          min: 0,
          max: 100,
          step: 1,
          label: `Opacity ${opacity.toFixed(1)} From`,
          hint: `The selected gene's expression percentile from which cells are drawn at opacity ${opacity.toFixed(1)}, unless they reach a higher threshold. Cells below every threshold are hidden.`,
          render: get => isColoredByCategory(get) && get('Plot-Specific Options.fadeByExpression'),
        }])),
      } : {}),
    };
  }, [groupby.join(','), varNames.join(','), defaultVarName]);

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
