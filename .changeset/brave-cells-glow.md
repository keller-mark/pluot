---
"@pluot/core": patch
---

Add HeatmapLayer and AdataZarrHeatmapLayer, with per-axis filtering and selection, whole-matrix/per-obs/per-var colormap normalization with lower/upper percentile thresholds, and camera filter functions to support zoom-to-square and limit-extent modes. Heatmap data is loaded from dense, CSC- or CSR-sparse arrays, without densifying sparse data (i.e., sparse arrays are passed to the heatmap shader as-is with CSC- and CSR-specific GLSL indexing functions).
