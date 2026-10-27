import React, { useCallback, useMemo } from 'react';
import { QueryClientProvider } from '@tanstack/react-query';
import { PluotWrapper } from './PluotWrapper.jsx';
import { useColormapOption, resolvePlotParams, multiSelect } from './PlotControls.jsx';
import { adataQueryClient, useAdataOptions, getExtentKey } from './adata-options.js';

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

function withDotPlotControls(plotParams, { colormap, obsCategories, genes }) {
  return {
    ...plotParams,
    layers: plotParams.layers.map(layer => (layer.layer_type === DOTPLOT_LAYER_TYPE ? {
      ...layer,
      layer_params: {
        ...layer.layer_params,
        cmap: colormap,
        categories: obsCategories,
        var_names: genes,
      },
    } : layer)),
  };
}

function DotPlotWrapperInner(props) {
  const { plotParams, storeUrl } = props;
  const defaultLayerParams = plotParams?.layers
    ?.find(layer => layer.layer_type === DOTPLOT_LAYER_TYPE)
    ?.layer_params;
  const colormapOption = useColormapOption(defaultLayerParams?.cmap);

  const groupby = defaultLayerParams?.groupby;
  const adataOptions = useAdataOptions(storeUrl, groupby);
  const categories = adataOptions.data?.categories;
  const varNames = adataOptions.data?.varNames;

  const defaultCategories = defaultLayerParams?.categories;
  const defaultGenes = defaultLayerParams?.var_names;
  const plotSpecificOptions = useMemo(() => ({
    obsCategories: multiSelect({
      selected: defaultCategories ?? categories ?? [],
      options: categories ?? [],
      label: groupby,
    }),
    genes: multiSelect({
      selected: defaultGenes ?? [],
      options: varNames ?? [],
      size: 10,
      label: 'Genes',
    }),
    ...colormapOption,
  }), [groupby, categories, varNames, defaultCategories, defaultGenes, colormapOption]);

  const getPlotParams = useCallback(
    controlValues => withDotPlotControls(resolvePlotParams(plotParams, controlValues), controlValues),
    [plotParams],
  );

  if (adataOptions.isPending) {
    return (<p>Loading...</p>);
  }
  if (adataOptions.isError) {
    return (<p>Error loading AnnData: {adataOptions.error.message}</p>);
  }

  return (
    <PluotWrapper
      {...props}
      plotParams={getPlotParams}
      extentKey={getExtentKey}
      plotSpecificOptions={plotSpecificOptions}
      onHover={onHoverDotPlot}
      enableTooltip
    />
  );
}

export function DotPlotWrapper(props) {
  return (
    <QueryClientProvider client={adataQueryClient}>
      <DotPlotWrapperInner {...props} />
    </QueryClientProvider>
  );
}
