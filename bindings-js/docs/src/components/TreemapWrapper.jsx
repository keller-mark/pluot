import React, { useCallback, useMemo } from 'react';
import { PluotWrapper } from './PluotWrapper.jsx';
import { resolvePlotParams } from './PlotControls.jsx';

// The purpose of this wrapper component is to define a treemap-specific
// onHover function to transform the picking result into a tooltip dict.

const countFormatter = new Intl.NumberFormat('en-US');

function onHoverTreemap(info) {
  const treemapInfo = info?.layer_results?.[0]?.info;
  if (!treemapInfo) {
    // The treemap layer may not have rendered/prepared yet.
    return undefined;
  }
  return {
    ...Object.fromEntries(treemapInfo.groupby.map((columnPath, i) => [columnPath.split('/').pop(), treemapInfo.obs_values[i]])),
    'count': countFormatter.format(treemapInfo.count),
  };
}

const TREEMAP_LAYER_TYPE = 'AdataZarrTreemapLayer';

const EXPRESSION_MODES = ['None', 'Filter', 'Select'];

function isExpressionModeActive(get) {
  return get('Plot-Specific Options.expressionMode') !== 'None';
}

function expressedAbove(varName, threshold) {
  return {
    axis_criteria_mode: 'Expression',
    axis_criteria_params: { var_name: varName, min: threshold, min_exclusive: true },
  };
}

function withTreemapControls(plotParams, { reverseLevels, colorBy, tile, expressionMode, varName, expressionThreshold }) {
  const expressionCriteria = expressionMode !== 'None' && varName ? expressedAbove(varName, expressionThreshold) : null;
  return {
    ...plotParams,
    layers: plotParams.layers.map((layer) => {
      if (layer.layer_type !== TREEMAP_LAYER_TYPE) {
        return layer;
      }
      const groupby = layer.layer_params.groupby ?? [];
      return {
        ...layer,
        layer_params: {
          ...layer.layer_params,
          groupby: reverseLevels ? [...groupby].reverse() : groupby,
          color_by: colorBy,
          tile,
          ...(expressionCriteria ? {
            [expressionMode === 'Filter' ? 'obs_filtering' : 'obs_selection']: expressionCriteria,
          } : {}),
        },
      };
    }),
  };
}

export function TreemapWrapper(props) {
  const { plotParams, varNameOptions = [] } = props;
  const defaultLayerParams = plotParams?.layers
    ?.find(layer => layer.layer_type === TREEMAP_LAYER_TYPE)
    ?.layer_params;
  const groupby = defaultLayerParams?.groupby ?? [];
  const defaultColorBy = defaultLayerParams?.color_by ?? groupby[0];
  const defaultTile = defaultLayerParams?.tile ?? 'Binary';

  const plotSpecificOptions = useMemo(() => ({
    reverseLevels: {
      value: false,
      label: 'Reverse Levels',
    },
    colorBy: {
      value: defaultColorBy,
      options: groupby,
      label: 'Color By',
    },
    tile: {
      value: defaultTile,
      options: ['Binary', 'Dice', 'Slice'],
      label: 'Tiling',
    },
    ...(varNameOptions.length > 0 ? {
      expressionMode: {
        value: 'None',
        options: EXPRESSION_MODES,
        label: 'Gene Expression',
        hint: 'Filter: count only the cells expressing the gene above the threshold. Select: fill each rectangle in proportion to its share of such cells.',
      },
      varName: {
        value: varNameOptions[0],
        options: varNameOptions,
        label: 'Gene',
        render: isExpressionModeActive,
      },
      expressionThreshold: {
        value: 0,
        min: -3,
        max: 3,
        step: 0.1,
        label: 'Threshold',
        render: isExpressionModeActive,
      },
    } : {}),
  }), [groupby.join(','), defaultColorBy, defaultTile, varNameOptions.join(',')]);

  const getPlotParams = useCallback(
    controlValues => withTreemapControls(resolvePlotParams(plotParams, controlValues), controlValues),
    [plotParams],
  );

  return (
    <PluotWrapper
      {...props}
      plotParams={getPlotParams}
      plotSpecificOptions={plotSpecificOptions}
      onHover={onHoverTreemap}
      enableTooltip
    />
  );
}
