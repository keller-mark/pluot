import React, { useCallback } from 'react';
import { PluotWrapper } from './PluotWrapper.jsx';
import { useColormapOption, resolvePlotParams } from './PlotControls.jsx';

// The purpose of this wrapper component is to define a dotplot-specific
// onHover function to transform the picking result into a tooltip dict.

const numFormatter = new Intl.NumberFormat('en-US', {
  minimumFractionDigits: 2,
  maximumFractionDigits: 2
});

// TODO: Move this logic to Rust, so that the behavior can be used
// across platforms, which would remove the need for this DotPlot-specific Wrapper component.
function onHoverDotPlot(info) {
  const dotPlotInfo = info?.layer_results?.[0]?.info;
  if (!dotPlotInfo) {
    // It is possible that the dot plot layer has not yet rendered/prepared,
    // so there is not yet any returned info.
    return undefined;
  }
  // Note: Sort the dict based on the keys, since rust returns hashmap in any order
  return {
    'Cell Type': dotPlotInfo.obs_value,
    'Gene': dotPlotInfo.var_name,
    'Fraction expressing': numFormatter.format(dotPlotInfo.fraction_expressing),
    'Mean expression': numFormatter.format(dotPlotInfo.mean_expression),
  }
}

const DOTPLOT_LAYER_TYPE = 'AdataZarrDotPlotLayer';

function withDotPlotColormap(plotParams, cmap) {
  return {
    ...plotParams,
    layers: plotParams.layers.map(layer => (layer.layer_type === DOTPLOT_LAYER_TYPE ? {
      ...layer,
      layer_params: { ...layer.layer_params, cmap },
    } : layer)),
  };
}

export function DotPlotWrapper(props) {
  const { plotParams } = props;
  const defaultColormap = plotParams?.layers
    ?.find(layer => layer.layer_type === DOTPLOT_LAYER_TYPE)
    ?.layer_params?.cmap;
  const colormapOption = useColormapOption(defaultColormap);

  const getPlotParams = useCallback(
    controlValues => withDotPlotColormap(resolvePlotParams(plotParams, controlValues), controlValues.colormap),
    [plotParams],
  );

  return (
    <PluotWrapper
      {...props}
      plotParams={getPlotParams}
      plotSpecificOptions={colormapOption}
      onHover={onHoverDotPlot}
      enableTooltip
    />
  );
}

// TODO: use anndata.js to load the cell type categories and gene list on the JS side, and display multi-select inputs, which the plot params can depend on.
