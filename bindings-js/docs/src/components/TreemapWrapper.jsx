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
    ...Object.fromEntries(treemapInfo.groupby.map((column, i) => [column, treemapInfo.obs_values[i]])),
    'Cells': countFormatter.format(treemapInfo.count),
  };
}

const TREEMAP_LAYER_TYPE = 'AdataZarrTreemapLayer';

function withTreemapControls(plotParams, { reverseLevels, colorBy, tile }) {
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
        },
      };
    }),
  };
}

export function TreemapWrapper(props) {
  const { plotParams } = props;
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
  }), [groupby.join(','), defaultColorBy, defaultTile]);

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
