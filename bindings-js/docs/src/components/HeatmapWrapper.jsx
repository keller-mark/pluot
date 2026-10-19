import React, { useCallback, useMemo } from 'react';
import { PluotWrapper } from './PluotWrapper.jsx';
import { useColormapOption, resolvePlotParams } from './PlotControls.jsx';

// The purpose of this wrapper component is to define a heatmap-specific
// onHover function to transform the picking result into a tooltip dict.

const numFormatter = new Intl.NumberFormat('en-US', {
  minimumFractionDigits: 2,
  maximumFractionDigits: 2
});

function onHoverHeatmap(info) {
  const heatmapInfo = info?.layer_results?.[0]?.info;
  if (!heatmapInfo) {
    // The heatmap layer may not have rendered/prepared yet.
    return undefined;
  }
  return {
    'Cell': heatmapInfo.obs_name,
    'Gene': heatmapInfo.var_name,
    'Expression': numFormatter.format(Number(heatmapInfo.value)),
  }
}

const HEATMAP_LAYER_TYPE = 'AdataZarrHeatmapLayer';

function withHeatmapControls(plotParams, { colormap, reverseColormap, normalization, lowerPercentile, upperPercentile }) {
  return {
    ...plotParams,
    layers: plotParams.layers.map((layer) => {
      if (layer.layer_type !== HEATMAP_LAYER_TYPE) {
        return layer;
      }
      const prevColormap = layer.layer_params.colormap;
      const prevQuantitativeParams = prevColormap?.colormap_mode === 'Quantitative'
        ? prevColormap.colormap_params
        : {};
      return {
        ...layer,
        layer_params: {
          ...layer.layer_params,
          normalization,
          normalization_lower_percentile: lowerPercentile,
          normalization_upper_percentile: upperPercentile,
          colormap: {
            colormap_mode: 'Quantitative',
            colormap_params: { ...prevQuantitativeParams, colormap, reverse: reverseColormap },
          },
        },
      };
    }),
  };
}

function percentileOption(value, label) {
  return { value, min: 0, max: 100, step: 0.5, label };
}

export function HeatmapWrapper(props) {
  const { plotParams } = props;
  const defaultLayerParams = plotParams?.layers
    ?.find(layer => layer.layer_type === HEATMAP_LAYER_TYPE)
    ?.layer_params;
  const colormapOption = useColormapOption(defaultLayerParams?.colormap?.colormap_params?.colormap);

  const defaultReverseColormap = defaultLayerParams?.colormap?.colormap_params?.reverse ?? false;
  const defaultNormalization = defaultLayerParams?.normalization ?? 'Matrix';
  const defaultLowerPercentile = defaultLayerParams?.normalization_lower_percentile ?? 0;
  const defaultUpperPercentile = defaultLayerParams?.normalization_upper_percentile ?? 100;
  const plotSpecificOptions = useMemo(() => ({
    ...colormapOption,
    reverseColormap: {
      value: defaultReverseColormap,
      label: 'Reverse Colormap',
    },
    normalization: {
      value: defaultNormalization,
      options: {
        'Whole matrix': 'Matrix',
        'Per obs': 'PerObs',
        'Per var': 'PerVar',
      },
      label: 'Normalization',
    },
    lowerPercentile: percentileOption(defaultLowerPercentile, 'Lower Percentile'),
    upperPercentile: percentileOption(defaultUpperPercentile, 'Upper Percentile'),
  }), [colormapOption, defaultReverseColormap, defaultNormalization, defaultLowerPercentile, defaultUpperPercentile]);

  const getPlotParams = useCallback(
    controlValues => withHeatmapControls(resolvePlotParams(plotParams, controlValues), controlValues),
    [plotParams],
  );

  return (
    <PluotWrapper
      {...props}
      plotParams={getPlotParams}
      plotSpecificOptions={plotSpecificOptions}
      onHover={onHoverHeatmap}
      enableTooltip
    />
  );
}

// TODO: use anndata.js to load the cell type categories and gene list on the JS side, and display multi-select inputs, which the plot params can depend on.
