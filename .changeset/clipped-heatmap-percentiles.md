---
"@pluot/core": patch
---

Add `normalization_lower_percentile` and `normalization_upper_percentile` params to AdataZarrHeatmapLayer, which clip the matrix-wise, per-obs, or per-var colormap normalization to percentiles of the values (on the CPU or GPU) so that outliers do not compress the colors of the other values.
