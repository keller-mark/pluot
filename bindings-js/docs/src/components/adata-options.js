import * as zarr from 'zarrita';
import { readZarr, get } from 'anndata.js';
import { useQuery, QueryClient } from '@tanstack/react-query';

export const adataQueryClient = new QueryClient();

const OBS_COLUMN_PATH_REGEX = /^\/?obs\/(.+?)(\/codes)?\/?$/;

// Accepts a path to an obs column (e.g., `obs/cell_type`) or to its codes (e.g., `/obs/cell_type/codes`).
// Columns of AnnData objects nested within the store (e.g., `mod/rna/obs/cell_type`) are not supported.
export function obsColumnName(path) {
  return path?.match(OBS_COLUMN_PATH_REGEX)?.[1];
}

function obsCodesPath(path) {
  return `/obs/${obsColumnName(path)}/codes`;
}

async function loadAdataOptions({ queryKey: [, storeUrl, obsColumn] }) {
  const store = await zarr.withMaybeConsolidatedMetadata(new zarr.FetchStore(storeUrl));
  const adata = await readZarr(store);
  const [categories, varNames] = await Promise.all([
    obsColumn ? adata.obs.get(obsColumn).then(column => get(column.categories, [null])) : null,
    adata.varNames().then(names => get(names, [null])),
  ]);
  return {
    categories: categories ? Array.from(categories.data) : [],
    varNames: Array.from(varNames.data),
  };
}

export function useAdataOptions(storeUrl, obsColumnPath) {
  return useQuery({
    queryKey: ['adata-options', storeUrl, obsColumnName(obsColumnPath)],
    queryFn: loadAdataOptions,
  });
}

function isCategoricalCriterionOn(criterion, obsColumnPath) {
  return criterion.criteria_mode === 'Categorical'
    && obsColumnName(criterion.criteria_params.codes_key) === obsColumnName(obsColumnPath);
}

export function findCategoricalCriterion(axisCriteria, obsColumnPath) {
  if (axisCriteria?.axis_criteria_mode !== 'Criteria') {
    return undefined;
  }
  return axisCriteria.axis_criteria_params
    .find(criterion => obsColumnPath ? isCategoricalCriterionOn(criterion, obsColumnPath) : criterion.criteria_mode === 'Categorical')
    ?.criteria_params;
}

// `included_categories` may be given as codes or as labels.
export function includedCategoryLabels(criterionParams, categories) {
  return criterionParams?.included_categories
    ?.map(category => (typeof category === 'number' ? categories?.[category] : category))
    .filter(label => label !== undefined);
}

export function withIncludedCategories(axisCriteria, obsColumnPath, labels) {
  const prevParams = findCategoricalCriterion(axisCriteria, obsColumnPath);
  const criterion = {
    criteria_mode: 'Categorical',
    criteria_params: { codes_key: obsCodesPath(obsColumnPath), ...prevParams, included_categories: labels },
  };
  if (axisCriteria?.axis_criteria_mode !== 'Criteria') {
    return { axis_criteria_mode: 'Criteria', axis_criteria_params: [criterion] };
  }
  const criteria = axisCriteria.axis_criteria_params;
  return {
    ...axisCriteria,
    axis_criteria_params: criteria.some(c => isCategoricalCriterionOn(c, obsColumnPath))
      ? criteria.map(c => (isCategoricalCriterionOn(c, obsColumnPath) ? criterion : c))
      : [...criteria, criterion],
  };
}

export function getExtentKey({ obsCategories, genes }) {
  return JSON.stringify({ obsCategories, genes });
}
