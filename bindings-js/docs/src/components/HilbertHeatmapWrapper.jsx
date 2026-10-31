import React, { useCallback, useMemo } from 'react';
import { PluotWrapper } from './PluotWrapper.jsx';
import { resolvePlotParams } from './PlotControls.jsx';

const HILBERT_HEATMAP_LAYER_TYPE = 'AdataZarrHilbertHeatmapLayer';
const COLOR_BY_EXPRESSION = 'Expression';
const FADE_LEVELS = [
  { opacity: 0.2, defaultPercentile: 0, control: 'percentileForOpacity20' },
  { opacity: 0.6, defaultPercentile: 50, control: 'percentileForOpacity60' },
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
  const { colorBy, fadeByExpression } = controlValues;
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
          var_name: colorByExpression || fade ? layer.layer_params.var_name : null,
          color_by: colorByExpression ? null : colorBy,
          expression_percentile_opacities: fade
            ? FADE_LEVELS.map(({ opacity, control }) => ({ percentile: controlValues[control], opacity }))
            : null,
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
  const varName = defaultLayerParams?.var_name;

  const plotSpecificOptions = useMemo(() => {
    const isColoredByCategory = get => get('Plot-Specific Options.colorBy') !== COLOR_BY_EXPRESSION;
    return {
      colorBy: {
        value: COLOR_BY_EXPRESSION,
        options: [COLOR_BY_EXPRESSION, ...groupby],
        label: 'Color By',
      },
      ...(varName ? {
        fadeByExpression: {
          value: false,
          label: `Fade By ${varName}`,
          hint: `Set each cell's opacity by the percentile of its ${varName} expression among the plotted cells.`,
          render: isColoredByCategory,
        },
        ...Object.fromEntries(FADE_LEVELS.map(({ opacity, defaultPercentile, control }) => [control, {
          value: defaultPercentile,
          min: 0,
          max: 100,
          step: 1,
          label: `Opacity ${opacity.toFixed(1)} From`,
          hint: `The ${varName} expression percentile from which cells are drawn at opacity ${opacity.toFixed(1)}, unless they reach a higher threshold. Cells below every threshold are hidden.`,
          render: get => isColoredByCategory(get) && get('Plot-Specific Options.fadeByExpression'),
        }])),
      } : {}),
    };
  }, [groupby.join(','), varName]);

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
