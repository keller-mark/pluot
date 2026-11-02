---
"@pluot/core": patch
"@pluot/react": patch
---

`ZarrCategoricalCriteriaParams.included_codes` is renamed to `included_categories`, which accepts either category codes or category labels (looked up in the new optional `categories_key`, defaulting to the `categories` array alongside `codes_key`). The `categories` param of the `AdataZarrDotPlotLayer`, `AdataZarrViolinLayer`, and `AdataZarrViolinMatrixLayer` is removed: to plot only some categories, use `obs_filtering` with a `Categorical` criteria on a `groupby` column's codes, whose `included_categories` also determine the order of that column's categories.
