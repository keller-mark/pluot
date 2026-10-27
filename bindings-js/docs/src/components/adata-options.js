import * as zarr from 'zarrita';
import { readZarr, get } from 'anndata.js';
import { useQuery, QueryClient } from '@tanstack/react-query';

export const adataQueryClient = new QueryClient();

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

export function useAdataOptions(storeUrl, obsColumn) {
  return useQuery({
    queryKey: ['adata-options', storeUrl, obsColumn],
    queryFn: loadAdataOptions,
  });
}

export function getExtentKey({ obsCategories, genes }) {
  return JSON.stringify({ obsCategories, genes });
}
