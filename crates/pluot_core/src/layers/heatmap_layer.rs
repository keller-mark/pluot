// The heatmap layer should support either a quantitative or a categorical colormap.
// The functionality should be similar to the bitmap and bitmask layers, with the main difference being that we will support aggregation along X and Y
// in the same way as the vitessce heatmapBitmapLayer.
// References:
// - references/vitessce/packages/gl/src/HeatmapBitmapLayer.js
// - references/vitessce/packages/gl/src/HeatmapCompositeTextLayer.js
// - references/vitessce/packages/gl/src/PaddedExpressionHeatmapBitmapLayer.js
// - references/vitessce/packages/gl/src/heatmap-bitmap-layer-shaders.js
// - references/vitessce/packages/gl/src/padded-expression-heatmap-bitmap-layer-shaders.js
// - references/vitessce/packages/workers/src/heatmap.js
// - references/vitessce/packages/workers/src/heatmap.worker.js

// There are multiple cases for how the X/Y axis categories may be selected and/or filtered:
// - None: consider all elements along this axis
// - Explicit identifiers: consider these specified entries along this axis
//   - Technically, the identifier columns are axis-aligned row/columns with the same number of elements (as below). We need to load this identifier column in order to identify the row/column indices within the matrix corresponding to the specified identifiers.
// - Axis-aligned row/column with the same number of elements (e.g., obs column of anndata object to filter the list of cells i.e. rows of the X matrix)
//   - Boolean mask: consider the True entries along this axis
//   - Categorical with list of categories
//   - Quantitative with min/max range
//
// Given an axis-aligned row/column and filtering/selection criteria, we can perform a mapping from the criteria to the list of included matrix row or column indices on either the GPU (via compute shader) or CPU. This will be similar to the reducer operations, except rather than the output being a summary or distribution, it will be a list of integer indices.
// Then, we will use this list of indices in order to load the matrix data (via loading subsets of the zarr array corresponding to the specified row/column indices - or all data along a given axis if no filtering criteria was provided).

// We will upload the heatmap matrix as a texture to the shader.
// Since the (filtered) matrix may be very long or wide, a single row or column of the matrix may need to wrap onto multiple rows of a texture. We will need a function to look up the texture index corresponding to the data item of interest.
// We will simply consider the 2D texture with shape texture_shape to be a 1D array with flat_texture_length, and we will also consider the 2D matrix with shape matrix_shape to be a 1D array with flat_matrix_length, and we will just wrap the flat matrix into the texture as needed. We will render multiple textures corresponding to multiple heatmap layers as needed when the matrix is larger than a single texture.

// Which operations should be performed in the base layer versus the zarr layer?
// - The base layer will be given the filtered matrix returned from the zarr array subset loading operation. The base layer will transform the matrix into the texture for rendering.
// - The base layer will NOT perform filtering or accept filtering criteria; the parent zarr layer will provide the pre-filtered matrix.
// - The base layer will be given an optional boolean mask of selection criteria for the rows/columns, with the row/column count of mask entries matching the filtered matrix row/column count. The base layer will accept a background_opacity parameter to use when coloring the selection-excluded items.
// - The zarr layer will load the axis-aligned rows/columns, use these to compute the filter-included matrix row/column indices, load the filter-included matrix subset array, and compute the filter-included selection boolean mask. The zarr layer will pass these things to the base layer to render.
