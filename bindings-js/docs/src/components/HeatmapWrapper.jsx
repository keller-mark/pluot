import React, { useCallback, useMemo } from 'react';
import { QueryClientProvider } from '@tanstack/react-query';
import { PluotWrapper } from './PluotWrapper.jsx';
import { useColormapOption, resolvePlotParams, multiSelect } from './PlotControls.jsx';
import { adataQueryClient, useAdataOptions, getExtentKey } from './adata-options.js';

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

const CATEGORICAL_CODES_KEY_REGEX = /^\/obs\/(.+)\/codes$/;

function findCategoricalObsFilter(layerParams) {
  const criteria = layerParams?.obs_filtering?.axis_criteria_params;
  if (!Array.isArray(criteria)) {
    return undefined;
  }
  return criteria.find(criterion => criterion.criteria_mode === 'Categorical')?.criteria_params;
}

function withSelectedObsCodes(obsFiltering, includedCodes) {
  return {
    ...obsFiltering,
    axis_criteria_params: obsFiltering.axis_criteria_params.map(criterion => (
      criterion.criteria_mode === 'Categorical' ? {
        ...criterion,
        criteria_params: { ...criterion.criteria_params, included_codes: includedCodes },
      } : criterion
    )),
  };
}

function withHeatmapControls(plotParams, controlValues, categories) {
  const { colormap, reverseColormap, normalization, lowerPercentile, upperPercentile, obsCategories, genes } = controlValues;
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
          ...(obsCategories ? {
            obs_filtering: withSelectedObsCodes(
              layer.layer_params.obs_filtering,
              obsCategories.map(category => categories.indexOf(category)),
            ),
          } : {}),
          var_filtering: {
            axis_criteria_mode: 'Identifiers',
            axis_criteria_params: { identifiers: genes },
          },
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

function HeatmapWrapperInner(props) {
  const { plotParams, storeUrl } = props;
  const defaultLayerParams = plotParams?.layers
    ?.find(layer => layer.layer_type === HEATMAP_LAYER_TYPE)
    ?.layer_params;
  const colormapOption = useColormapOption(defaultLayerParams?.colormap?.colormap_params?.colormap);

  const categoricalObsFilter = findCategoricalObsFilter(defaultLayerParams);
  const obsColumn = categoricalObsFilter?.codes_key?.match(CATEGORICAL_CODES_KEY_REGEX)?.[1];

  const adataOptions = useAdataOptions(storeUrl, obsColumn);
  const categories = adataOptions.data?.categories;
  const varNames = adataOptions.data?.varNames;

  const defaultReverseColormap = defaultLayerParams?.colormap?.colormap_params?.reverse ?? false;
  const defaultNormalization = defaultLayerParams?.normalization ?? 'Matrix';
  const defaultLowerPercentile = defaultLayerParams?.normalization_lower_percentile ?? 0;
  const defaultUpperPercentile = defaultLayerParams?.normalization_upper_percentile ?? 100;
  const defaultIncludedCodes = categoricalObsFilter?.included_codes;
  const defaultGenes = defaultLayerParams?.var_filtering?.axis_criteria_params?.identifiers;
  const plotSpecificOptions = useMemo(() => ({
    ...(obsColumn ? {
      obsCategories: multiSelect({
        selected: (defaultIncludedCodes ?? []).map(code => categories?.[code]),
        options: categories ?? [],
        label: obsColumn,
      }),
    } : {}),
    genes: multiSelect({
      selected: defaultGenes ?? [],
      options: varNames ?? [],
      size: 10,
      label: 'Genes',
    }),
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
  }), [obsColumn, categories, varNames, defaultIncludedCodes, defaultGenes, colormapOption, defaultReverseColormap, defaultNormalization, defaultLowerPercentile, defaultUpperPercentile]);

  const getPlotParams = useCallback(
    controlValues => withHeatmapControls(resolvePlotParams(plotParams, controlValues), controlValues, categories),
    [plotParams, categories],
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
      onHover={onHoverHeatmap}
      enableTooltip
    />
  );
}

export function HeatmapWrapper(props) {
  return (
    <QueryClientProvider client={adataQueryClient}>
      <HeatmapWrapperInner {...props} />
    </QueryClientProvider>
  );
}
