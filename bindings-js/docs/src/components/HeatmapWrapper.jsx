import React, { useCallback } from 'react';
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

function withHeatmapColormap(plotParams, colormap) {
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
          colormap: {
            colormap_mode: 'Quantitative',
            colormap_params: { ...prevQuantitativeParams, colormap },
          },
        },
      };
    }),
  };
}

export function HeatmapWrapper(props) {
  const { plotParams } = props;
  const defaultColormap = plotParams?.layers
    ?.find(layer => layer.layer_type === HEATMAP_LAYER_TYPE)
    ?.layer_params?.colormap?.colormap_params?.colormap;
  const colormapOption = useColormapOption(defaultColormap);

  const getPlotParams = useCallback(
    controlValues => withHeatmapColormap(resolvePlotParams(plotParams, controlValues), controlValues.colormap),
    [plotParams],
  );

  return (
    <PluotWrapper
      {...props}
      plotParams={getPlotParams}
      plotSpecificOptions={colormapOption}
      onHover={onHoverHeatmap}
      enableTooltip
    />
  );
}

// TODO: use anndata.js to load the cell type categories and gene list on the JS side, and display multi-select inputs, which the plot params can depend on.
