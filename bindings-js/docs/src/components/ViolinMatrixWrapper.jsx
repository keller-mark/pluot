import React, { useCallback, useMemo } from 'react';
import { QueryClientProvider } from '@tanstack/react-query';
import { PluotWrapper } from './PluotWrapper.jsx';
import { useColormapOption, resolvePlotParams, multiSelect } from './PlotControls.jsx';
import {
  adataQueryClient,
  useAdataOptions,
  getExtentKey,
  obsColumnName,
  findCategoricalCriterion,
  includedCategoryLabels,
  withIncludedCategories,
} from './adata-options.js';

// The purpose of this wrapper component is to define a violin-matrix-specific
// onHover function to transform the picking result into a tooltip dict.

const valueFormatter = new Intl.NumberFormat('en-US', { maximumFractionDigits: 3 });

function onHoverViolinMatrix(info) {
  const cellInfo = info?.layer_results?.[0]?.info;
  if (!cellInfo) {
    // The violin matrix layer may not have rendered/prepared yet.
    return undefined;
  }
  const [q1, median, q3] = cellInfo.quartiles ?? [];
  return {
    'Cell Type': cellInfo.obs_values.join(' / '),
    'Gene': cellInfo.var_name,
    'Cells': valueFormatter.format(cellInfo.num_values),
    ...(cellInfo.quartiles ? {
      'Median': valueFormatter.format(median),
      'IQR': `${valueFormatter.format(q1)} – ${valueFormatter.format(q3)}`,
    } : {}),
  };
}

const VIOLIN_MATRIX_LAYER_TYPE = 'AdataZarrViolinMatrixLayer';

function withViolinMatrixControls(plotParams, { colormap, densityNorm, obsCategories, genes }, groupbyColumnPath) {
  return {
    ...plotParams,
    layers: plotParams.layers.map(layer => (layer.layer_type === VIOLIN_MATRIX_LAYER_TYPE ? {
      ...layer,
      layer_params: {
        ...layer.layer_params,
        cmap: colormap,
        density_norm: densityNorm,
        ...(groupbyColumnPath ? {
          obs_filtering: withIncludedCategories(layer.layer_params.obs_filtering, groupbyColumnPath, obsCategories),
        } : {}),
        var_names: genes,
      },
    } : layer)),
  };
}

function ViolinMatrixWrapperInner(props) {
  const { plotParams, storeUrl } = props;
  const defaultLayerParams = plotParams?.layers
    ?.find(layer => layer.layer_type === VIOLIN_MATRIX_LAYER_TYPE)
    ?.layer_params;
  const colormapOption = useColormapOption(defaultLayerParams?.cmap ?? 'Blues');

  // Only the categories of the first (coarsest) groupby column are selectable.
  const groupbyColumnPath = defaultLayerParams?.groupby?.[0];
  const adataOptions = useAdataOptions(storeUrl, groupbyColumnPath);
  const categories = adataOptions.data?.categories;
  const varNames = adataOptions.data?.varNames;

  const defaultCategories = useMemo(
    () => includedCategoryLabels(findCategoricalCriterion(defaultLayerParams?.obs_filtering, groupbyColumnPath), categories),
    [defaultLayerParams, groupbyColumnPath, categories],
  );
  const defaultGenes = defaultLayerParams?.var_names;
  const defaultDensityNorm = defaultLayerParams?.density_norm ?? 'Width';
  const plotSpecificOptions = useMemo(() => ({
    ...(groupbyColumnPath ? {
      obsCategories: multiSelect({
        selected: defaultCategories ?? categories ?? [],
        options: categories ?? [],
        label: obsColumnName(groupbyColumnPath),
      }),
    } : {}),
    genes: multiSelect({
      selected: defaultGenes ?? [],
      options: varNames ?? [],
      size: 10,
      label: 'Genes',
    }),
    ...colormapOption,
    densityNorm: {
      value: defaultDensityNorm,
      options: {
        'Width': 'Width',
        'Area': 'Area',
        'Count': 'Count',
      },
      label: 'Density Norm',
    },
  }), [groupbyColumnPath, categories, varNames, defaultCategories, defaultGenes, colormapOption, defaultDensityNorm]);

  const getPlotParams = useCallback(
    controlValues => withViolinMatrixControls(resolvePlotParams(plotParams, controlValues), controlValues, groupbyColumnPath),
    [plotParams, groupbyColumnPath],
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
      onHover={onHoverViolinMatrix}
      enableTooltip
    />
  );
}

export function ViolinMatrixWrapper(props) {
  return (
    <QueryClientProvider client={adataQueryClient}>
      <ViolinMatrixWrapperInner {...props} />
    </QueryClientProvider>
  );
}
